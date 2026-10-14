//! Postgres [`InviteJoinRepository`]: accepting a guild invite as one transaction.
//! Tests: `tests/invite_join.rs` (needs `SR_TEST_DATABASE_URL`).

use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Row, Transaction};

use sr_chat::domain::Member;
use sr_chat::repo::RepoResult;
use sr_chat::settings::SettingsError;
use sr_chat::settings::SettingsResult;
use sr_chat::settings::invites::{GuildInvite, InviteStatus};
use sr_chat::settings::join::{InviteJoin, InviteJoinRepository};
use sr_core::{ChannelId, GuildId, UserId};

use crate::chat_repo::{db, i, u};
use crate::settings_repo::{INVITE_USABLE, invite_cols, invite_from};

#[derive(Clone)]
pub struct PgInviteJoinRepo {
    pool: PgPool,
}

impl PgInviteJoinRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

async fn read_invite(
    tx: &mut Transaction<'_, Postgres>,
    code: &str,
) -> RepoResult<Option<GuildInvite>> {
    let sql = format!(
        "SELECT {} FROM guild_invites WHERE code = $1",
        invite_cols()
    );
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(code)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db)?
        .as_ref()
        .map(invite_from)
        .transpose()
}

/// The synthetic invite behind a server's vanity code (LNG-INV-004), when `code` is one.
/// It points at the system channel, else the first text or announcement channel.
async fn read_vanity(
    tx: &mut Transaction<'_, Postgres>,
    code: &str,
    now_ms: i64,
) -> RepoResult<Option<GuildInvite>> {
    let Some(g) = sqlx::query(
        "SELECT id, owner_id, system_channel_id, vanity_code::text AS vanity_code, vanity_uses \
         FROM guilds WHERE vanity_code = $1::citext",
    )
    .bind(code)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db)?
    else {
        return Ok(None);
    };
    let guild: i64 = g.try_get("id").map_err(db)?;
    let channel: Option<i64> = match g.try_get("system_channel_id").map_err(db)? {
        Some(c) => Some(c),
        None => sqlx::query_scalar(
            "SELECT id FROM channels WHERE guild_id = $1 AND type IN (0, 5) \
             ORDER BY position, id LIMIT 1",
        )
        .bind(guild)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db)?,
    };
    let Some(channel) = channel else {
        return Ok(None);
    };
    let uses: i32 = g.try_get("vanity_uses").map_err(db)?;
    Ok(Some(GuildInvite::vanity(
        g.try_get("vanity_code").map_err(db)?,
        GuildId::new(u(guild)),
        ChannelId::new(u(channel)),
        UserId::new(u(g.try_get("owner_id").map_err(db)?)),
        u32::try_from(uses).unwrap_or(0),
        now_ms,
    )))
}

/// Revoked invites read as unknown, like everywhere else.
fn classify(inv: &GuildInvite, now_ms: i64) -> SettingsResult<()> {
    match inv.status(now_ms) {
        InviteStatus::Active => Ok(()),
        InviteStatus::Expired => Err(SettingsError::InviteExpired),
        InviteStatus::Exhausted => Err(SettingsError::InviteExhausted),
        InviteStatus::Revoked => Err(SettingsError::UnknownInvite),
    }
}

#[async_trait]
impl InviteJoinRepository for PgInviteJoinRepo {
    async fn join_via_invite(
        &self,
        code: &str,
        user: UserId,
        now_ms: i64,
    ) -> SettingsResult<InviteJoin> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        // A real invite wins over a vanity code with the same text.
        let (inv, vanity) = match read_invite(&mut tx, code).await? {
            Some(inv) => (inv, false),
            None => (
                read_vanity(&mut tx, code, now_ms)
                    .await?
                    .ok_or(SettingsError::UnknownInvite)?,
                true,
            ),
        };
        classify(&inv, now_ms)?;
        let (guild, uid) = (i(inv.guild_id.get()), i(user.get()));

        let member: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM guild_members WHERE guild_id = $1 AND user_id = $2)",
        )
        .bind(guild)
        .bind(uid)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        if member {
            return Ok(InviteJoin::AlreadyMember { invite: inv });
        }
        let paused: bool = sqlx::query_scalar(
            "SELECT COALESCE((SELECT invites_paused FROM guild_community WHERE guild_id = $1), FALSE)",
        )
        .bind(guild)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        if paused {
            return Err(SettingsError::InvitesPaused);
        }
        let banned: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM guild_bans WHERE guild_id = $1 AND user_id = $2)",
        )
        .bind(guild)
        .bind(uid)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        if banned {
            return Err(SettingsError::Banned);
        }

        // Claim the membership row first: a concurrent accept by the same user
        // waits on the unique key and then sees the conflict, so it neither
        // double-joins nor consumes a second use. A server with membership
        // screening switched on admits the member as `pending` (LNG-GLD-008);
        // a temporary invite marks the membership (LNG-INV-005).
        let inserted = sqlx::query(
            "INSERT INTO guild_members (guild_id, user_id, joined_at, temporary, pending) \
             VALUES ($1, $2, to_timestamp($3::double precision / 1000.0), $4, \
                     COALESCE((SELECT community_enabled AND screening_enabled \
                               FROM guild_community WHERE guild_id = $1), FALSE)) \
             ON CONFLICT (guild_id, user_id) DO NOTHING RETURNING pending",
        )
        .bind(guild)
        .bind(uid)
        .bind(now_ms)
        .bind(inv.temporary)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        let Some(inserted) = inserted else {
            return Ok(InviteJoin::AlreadyMember { invite: inv });
        };
        let pending: bool = inserted.try_get("pending").map_err(db)?;

        let invite = if vanity {
            // A vanity code has no limits; only the use counter moves.
            let uses: i32 = sqlx::query_scalar(
                "UPDATE guilds SET vanity_uses = vanity_uses + 1 WHERE id = $1 \
                 RETURNING vanity_uses",
            )
            .bind(guild)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
            GuildInvite {
                uses: u32::try_from(uses).unwrap_or(0),
                ..inv.clone()
            }
        } else {
            // The guarded conditional UPDATE keeps max_uses exact: rows locked by a
            // competing accept are re-checked after it commits.
            let sql = format!(
                "UPDATE guild_invites SET uses = uses + 1 WHERE code = $1 AND {INVITE_USABLE} \
                 RETURNING {}",
                invite_cols()
            );
            let consumed = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(code)
                .bind(now_ms)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .as_ref()
                .map(invite_from)
                .transpose()?;
            let Some(invite) = consumed else {
                // Lost the race (or the invite died meanwhile). Dropping the
                // transaction rolls the membership row back.
                let current = read_invite(&mut tx, code)
                    .await?
                    .ok_or(SettingsError::UnknownInvite)?;
                classify(&current, now_ms)?;
                return Err(SettingsError::InviteExhausted);
            };
            invite
        };
        tx.commit().await.map_err(db)?;
        Ok(InviteJoin::Joined {
            invite,
            member: Member {
                guild_id: inv.guild_id,
                user_id: user,
                nick: None,
                role_ids: vec![],
                joined_at_ms: now_ms,
                communication_disabled_until_ms: None,
                pending,
                avatar_key: None,
                banner_key: None,
                bio: None,
            },
        })
    }

    async fn join_listed(
        &self,
        guild: GuildId,
        user: UserId,
        now_ms: i64,
    ) -> SettingsResult<Option<Member>> {
        let (g, uid) = (i(guild.get()), i(user.get()));
        let mut tx = self.pool.begin().await.map_err(db)?;
        let paused: bool = sqlx::query_scalar(
            "SELECT COALESCE((SELECT invites_paused FROM guild_community WHERE guild_id = $1), FALSE)",
        )
        .bind(g)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        let banned: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM guild_bans WHERE guild_id = $1 AND user_id = $2)",
        )
        .bind(g)
        .bind(uid)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        let member: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM guild_members WHERE guild_id = $1 AND user_id = $2)",
        )
        .bind(g)
        .bind(uid)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        if member {
            return Ok(None);
        }
        if paused {
            return Err(SettingsError::InvitesPaused);
        }
        if banned {
            return Err(SettingsError::Banned);
        }
        let inserted = sqlx::query(
            "INSERT INTO guild_members (guild_id, user_id, joined_at, pending) \
             VALUES ($1, $2, to_timestamp($3::double precision / 1000.0), \
                     COALESCE((SELECT community_enabled AND screening_enabled \
                               FROM guild_community WHERE guild_id = $1), FALSE)) \
             ON CONFLICT (guild_id, user_id) DO NOTHING RETURNING pending",
        )
        .bind(g)
        .bind(uid)
        .bind(now_ms)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        let Some(inserted) = inserted else {
            return Ok(None);
        };
        let pending: bool = inserted.try_get("pending").map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(Some(Member {
            guild_id: guild,
            user_id: user,
            nick: None,
            role_ids: vec![],
            joined_at_ms: now_ms,
            communication_disabled_until_ms: None,
            pending,
            avatar_key: None,
            banner_key: None,
            bio: None,
        }))
    }

    async fn take_temporary_memberships(&self, user: UserId) -> SettingsResult<Vec<GuildId>> {
        // Only the memberships that still hold no role; one that was given a role has
        // become a normal member. The membership row's roles cascade with it.
        let rows: Vec<i64> = sqlx::query_scalar(
            "DELETE FROM guild_members gm WHERE gm.user_id = $1 AND gm.temporary \
             AND NOT EXISTS (SELECT 1 FROM member_roles mr \
                             WHERE mr.guild_id = gm.guild_id AND mr.user_id = gm.user_id) \
             RETURNING gm.guild_id",
        )
        .bind(i(user.get()))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows.into_iter().map(|g| GuildId::new(u(g))).collect())
    }
}
