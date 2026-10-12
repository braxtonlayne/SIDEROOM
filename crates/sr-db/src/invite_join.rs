//! Postgres [`InviteJoinRepository`]: accepting a guild invite as one transaction.
//! Tests: `tests/invite_join.rs` (needs `SR_TEST_DATABASE_URL`).

use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Transaction};

use sr_chat::domain::Member;
use sr_chat::repo::RepoResult;
use sr_chat::settings::SettingsError;
use sr_chat::settings::SettingsResult;
use sr_chat::settings::invites::{GuildInvite, InviteStatus};
use sr_chat::settings::join::{InviteJoin, InviteJoinRepository};
use sr_core::UserId;

use crate::chat_repo::{db, i};
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
        let inv = read_invite(&mut tx, code)
            .await?
            .ok_or(SettingsError::UnknownInvite)?;
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
        // double-joins nor consumes a second use.
        let inserted = sqlx::query(
            "INSERT INTO guild_members (guild_id, user_id, joined_at) \
             VALUES ($1, $2, to_timestamp($3::double precision / 1000.0)) \
             ON CONFLICT (guild_id, user_id) DO NOTHING",
        )
        .bind(guild)
        .bind(uid)
        .bind(now_ms)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();
        if inserted == 0 {
            return Ok(InviteJoin::AlreadyMember { invite: inv });
        }

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
                pending: false,
            },
        })
    }
}
