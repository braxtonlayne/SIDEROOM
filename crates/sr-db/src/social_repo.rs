//! Postgres implementation of the `sr-chat` social repository traits
//! (migrations 0020-0023; read states live in 0013).
//!
//! Queries are runtime-checked. Timestamps cross the boundary as Unix
//! milliseconds, like `chat_repo`. DB-backed tests are in `tests/social_repo.rs`
//! and need `SR_TEST_DATABASE_URL`.

use async_trait::async_trait;
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

use sr_chat::error::RepoError;
use sr_chat::social::dm::{
    DmChannel, DmKind, DmPrivacy, GroupInvite, RequestReason, RequestState, SharedProtection,
};
use sr_chat::social::notifications::{ChannelOverride, Mute, NotificationLevel, ScopeSettings};
use sr_chat::social::presence::{CustomStatus, PresencePrefs, Status};
use sr_chat::social::profile::{Connection, Profile};
use sr_chat::social::read_state::{MAX_MENTION_COUNT, ReadState};
use sr_chat::social::relationships::{
    FriendRequestSources, RelOp, Relationship, RelationshipCounts, RelationshipKind,
};
use sr_chat::social::repo::{
    DmRepository, GuildMembership, NotificationRepository, PresenceRepository, Privacy,
    ProfileRepository, ReadStateRepository, RelationshipRepository, RepoResult,
    UserSettingsRepository,
};
use sr_chat::social::safety::{MediaFilter, SafetyPrefs, SensitiveMedia, SpamFilter};
use sr_chat::social::user_settings::UserSettings;
use sr_core::{ChannelId, GuildId, MessageId, UserId};
use std::collections::BTreeMap;

/// Postgres-backed social repository.
#[derive(Clone)]
pub struct PgSocialRepo {
    pool: PgPool,
}

impl PgSocialRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn db(e: sqlx::Error) -> RepoError {
    RepoError(e.to_string())
}

fn bad(what: &str) -> RepoError {
    RepoError(format!("unexpected value in column {what}"))
}

// Ids are positive Snowflakes (< 2^63), so these casts are lossless.
fn i(id: u64) -> i64 {
    id as i64
}
fn uid(v: i64) -> UserId {
    UserId::new(v as u64)
}

fn media_filter(r: &PgRow, col: &str) -> RepoResult<MediaFilter> {
    let code: i16 = r.try_get(col).map_err(db)?;
    MediaFilter::from_code(code).ok_or_else(|| bad(col))
}

fn overrides_from_json(v: &serde_json::Value) -> BTreeMap<u64, bool> {
    v.as_object()
        .into_iter()
        .flatten()
        .filter_map(|(k, v)| Some((k.parse().ok()?, v.as_bool()?)))
        .collect()
}

fn overrides_to_json(m: &BTreeMap<u64, bool>) -> serde_json::Value {
    serde_json::Value::Object(
        m.iter()
            .map(|(k, v)| (k.to_string(), (*v).into()))
            .collect(),
    )
}

fn guild_key(g: Option<GuildId>) -> i64 {
    g.map_or(0, |g| i(g.get()))
}

fn level_code(l: Option<NotificationLevel>) -> Option<i16> {
    l.map(NotificationLevel::code)
}

fn level_of(row: &PgRow) -> RepoResult<Option<NotificationLevel>> {
    row.try_get::<Option<i16>, _>("level")
        .map_err(db)?
        .map(|c| NotificationLevel::from_code(c).ok_or_else(|| bad("level")))
        .transpose()
}

fn mute_of(row: &PgRow) -> RepoResult<Mute> {
    Ok(Mute {
        muted: row.try_get("muted").map_err(db)?,
        end_ms: row.try_get("mute_end_ms").map_err(db)?,
    })
}

const MS: &str = "(extract(epoch FROM {c}) * 1000)::bigint";

fn ms(col: &str) -> String {
    MS.replace("{c}", col)
}

// ---- relationships ----

#[async_trait]
impl RelationshipRepository for PgSocialRepo {
    async fn user_exists(&self, user: UserId) -> RepoResult<bool> {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM accounts WHERE id = $1 AND state = 'active')",
        )
        .bind(i(user.get()))
        .fetch_one(&self.pool)
        .await
        .map_err(db)
    }

    async fn get_relationship(
        &self,
        user: UserId,
        target: UserId,
    ) -> RepoResult<Option<Relationship>> {
        let sql = format!(
            "SELECT user_id, target_id, type, nickname, {} AS since_ms \
             FROM relationships WHERE user_id = $1 AND target_id = $2",
            ms("created_at")
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(user.get()))
            .bind(i(target.get()))
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        row.as_ref().map(rel_from_row).transpose()
    }

    async fn list_relationships(&self, user: UserId) -> RepoResult<Vec<Relationship>> {
        let sql = format!(
            "SELECT user_id, target_id, type, nickname, {} AS since_ms \
             FROM relationships WHERE user_id = $1 ORDER BY created_at, target_id",
            ms("created_at")
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(user.get()))
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter().map(rel_from_row).collect()
    }

    async fn counts(&self, user: UserId) -> RepoResult<RelationshipCounts> {
        let row = sqlx::query(
            "SELECT count(*) FILTER (WHERE type = 1) AS friends, \
                    count(*) FILTER (WHERE type = 4) AS pending_outgoing, \
                    count(*) FILTER (WHERE type = 2) AS blocked \
             FROM relationships WHERE user_id = $1",
        )
        .bind(i(user.get()))
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        let n = |c: &str| -> RepoResult<usize> {
            Ok(usize::try_from(row.try_get::<i64, _>(c).map_err(db)?).unwrap_or(0))
        };
        Ok(RelationshipCounts {
            friends: n("friends")?,
            pending_outgoing: n("pending_outgoing")?,
            blocked: n("blocked")?,
        })
    }

    async fn apply(&self, ops: &[RelOp]) -> RepoResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        for op in ops {
            match op {
                RelOp::Upsert {
                    user,
                    target,
                    kind,
                    since_ms,
                } => {
                    sqlx::query(
                        "INSERT INTO relationships (user_id, target_id, type, created_at) \
                         VALUES ($1, $2, $3, to_timestamp($4::bigint / 1000.0)) \
                         ON CONFLICT (user_id, target_id) DO UPDATE SET \
                           type = EXCLUDED.type, nickname = NULL, \
                           created_at = EXCLUDED.created_at, updated_at = now()",
                    )
                    .bind(i(user.get()))
                    .bind(i(target.get()))
                    .bind(kind.code())
                    .bind(*since_ms)
                    .execute(&mut *tx)
                    .await
                    .map_err(db)?;
                }
                RelOp::Delete { user, target } => {
                    sqlx::query("DELETE FROM relationships WHERE user_id = $1 AND target_id = $2")
                        .bind(i(user.get()))
                        .bind(i(target.get()))
                        .execute(&mut *tx)
                        .await
                        .map_err(db)?;
                }
                RelOp::SetNickname {
                    user,
                    target,
                    nickname,
                } => {
                    sqlx::query(
                        "UPDATE relationships SET nickname = $3, updated_at = now() \
                         WHERE user_id = $1 AND target_id = $2 AND type = 1",
                    )
                    .bind(i(user.get()))
                    .bind(i(target.get()))
                    .bind(nickname.as_deref())
                    .execute(&mut *tx)
                    .await
                    .map_err(db)?;
                }
            }
        }
        tx.commit().await.map_err(db)
    }

    async fn shares_guild(&self, a: UserId, b: UserId) -> RepoResult<bool> {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM guild_members x JOIN guild_members y \
             ON x.guild_id = y.guild_id WHERE x.user_id = $1 AND y.user_id = $2)",
        )
        .bind(i(a.get()))
        .bind(i(b.get()))
        .fetch_one(&self.pool)
        .await
        .map_err(db)
    }

    async fn has_mutual_friend(&self, a: UserId, b: UserId) -> RepoResult<bool> {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM relationships x JOIN relationships y \
             ON x.target_id = y.target_id \
             WHERE x.user_id = $1 AND y.user_id = $2 AND x.type = 1 AND y.type = 1)",
        )
        .bind(i(a.get()))
        .bind(i(b.get()))
        .fetch_one(&self.pool)
        .await
        .map_err(db)
    }

    async fn mutual_friends(&self, a: UserId, b: UserId) -> RepoResult<Vec<UserId>> {
        let rows: Vec<i64> = sqlx::query_scalar(
            "SELECT x.target_id FROM relationships x JOIN relationships y \
             ON x.target_id = y.target_id \
             WHERE x.user_id = $1 AND y.user_id = $2 AND x.type = 1 AND y.type = 1 \
             ORDER BY x.target_id",
        )
        .bind(i(a.get()))
        .bind(i(b.get()))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows.into_iter().map(uid).collect())
    }

    async fn find_user_by_username(&self, username: &str) -> RepoResult<Option<UserId>> {
        // `username` is CITEXT, so the comparison is case-insensitive.
        let id: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM accounts WHERE username = $1::citext AND state = 'active'",
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        Ok(id.map(uid))
    }

    async fn list_guild_memberships(&self, user: UserId) -> RepoResult<Vec<GuildMembership>> {
        let sql = format!(
            "SELECT guild_id, nick, avatar_key, banner_key, bio, pronouns, theme_primary, theme_accent, profile_effect, avatar_decoration, flags, {} AS joined_ms \
             FROM guild_members WHERE user_id = $1 ORDER BY guild_id",
            ms("joined_at")
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(user.get()))
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter()
            .map(|r| {
                Ok(GuildMembership {
                    guild: GuildId::new(r.try_get::<i64, _>("guild_id").map_err(db)? as u64),
                    nick: r.try_get("nick").map_err(db)?,
                    joined_at_ms: r.try_get("joined_ms").map_err(db)?,
                    avatar_key: r.try_get("avatar_key").map_err(db)?,
                    banner_key: r.try_get("banner_key").map_err(db)?,
                    bio: r.try_get("bio").map_err(db)?,
                    extras: crate::chat_repo::extras_from(r).map_err(db)?,
                })
            })
            .collect()
    }

    async fn guild_peers(&self, user: UserId) -> RepoResult<Vec<UserId>> {
        let rows: Vec<i64> = sqlx::query_scalar(
            "SELECT DISTINCT y.user_id FROM guild_members x JOIN guild_members y \
             ON x.guild_id = y.guild_id \
             WHERE x.user_id = $1 AND y.user_id <> $1 ORDER BY y.user_id",
        )
        .bind(i(user.get()))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows.into_iter().map(uid).collect())
    }

    async fn get_privacy(&self, user: UserId) -> RepoResult<Privacy> {
        let row = sqlx::query(
            "SELECT fr_everyone, fr_friends_of_friends, fr_server_members, \
                    dm_server_members, dm_message_requests, media_friends, media_others, \
                    media_servers, dm_spam_filter, server_dm_overrides \
             FROM user_privacy WHERE user_id = $1",
        )
        .bind(i(user.get()))
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        let Some(r) = row else {
            return Ok(Privacy::default());
        };
        Ok(Privacy {
            friend_requests: FriendRequestSources {
                everyone: r.try_get("fr_everyone").map_err(db)?,
                friends_of_friends: r.try_get("fr_friends_of_friends").map_err(db)?,
                server_members: r.try_get("fr_server_members").map_err(db)?,
            },
            dms: DmPrivacy {
                allow_server_member_dms: r.try_get("dm_server_members").map_err(db)?,
                message_requests: r.try_get("dm_message_requests").map_err(db)?,
            },
            safety: SafetyPrefs {
                sensitive_media: SensitiveMedia {
                    friends: media_filter(&r, "media_friends")?,
                    others: media_filter(&r, "media_others")?,
                    servers: media_filter(&r, "media_servers")?,
                },
                dm_spam_filter: SpamFilter::from_code(r.try_get("dm_spam_filter").map_err(db)?)
                    .ok_or_else(|| bad("dm_spam_filter"))?,
                server_dm_overrides: overrides_from_json(
                    &r.try_get::<serde_json::Value, _>("server_dm_overrides")
                        .map_err(db)?,
                ),
            },
        })
    }

    async fn put_privacy(&self, user: UserId, p: &Privacy) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO user_privacy (user_id, fr_everyone, fr_friends_of_friends, \
               fr_server_members, dm_server_members, dm_message_requests, media_friends, \
               media_others, media_servers, dm_spam_filter, server_dm_overrides) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
             ON CONFLICT (user_id) DO UPDATE SET fr_everyone = $2, fr_friends_of_friends = $3, \
               fr_server_members = $4, dm_server_members = $5, dm_message_requests = $6, \
               media_friends = $7, media_others = $8, media_servers = $9, dm_spam_filter = $10, \
               server_dm_overrides = $11, updated_at = now()",
        )
        .bind(i(user.get()))
        .bind(p.friend_requests.everyone)
        .bind(p.friend_requests.friends_of_friends)
        .bind(p.friend_requests.server_members)
        .bind(p.dms.allow_server_member_dms)
        .bind(p.dms.message_requests)
        .bind(p.safety.sensitive_media.friends.code())
        .bind(p.safety.sensitive_media.others.code())
        .bind(p.safety.sensitive_media.servers.code())
        .bind(p.safety.dm_spam_filter.code())
        .bind(overrides_to_json(&p.safety.server_dm_overrides))
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn is_ignoring(&self, user: UserId, target: UserId) -> RepoResult<bool> {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM user_ignores WHERE user_id = $1 AND target_id = $2)",
        )
        .bind(i(user.get()))
        .bind(i(target.get()))
        .fetch_one(&self.pool)
        .await
        .map_err(db)
    }

    async fn list_ignores(&self, user: UserId) -> RepoResult<Vec<UserId>> {
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT target_id FROM user_ignores WHERE user_id = $1 ORDER BY created_at, target_id",
        )
        .bind(i(user.get()))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(ids.into_iter().map(uid).collect())
    }

    async fn put_ignore(&self, user: UserId, target: UserId) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO user_ignores (user_id, target_id) VALUES ($1, $2) \
             ON CONFLICT (user_id, target_id) DO NOTHING",
        )
        .bind(i(user.get()))
        .bind(i(target.get()))
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn delete_ignore(&self, user: UserId, target: UserId) -> RepoResult<()> {
        sqlx::query("DELETE FROM user_ignores WHERE user_id = $1 AND target_id = $2")
            .bind(i(user.get()))
            .bind(i(target.get()))
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn get_note(&self, user: UserId, target: UserId) -> RepoResult<Option<String>> {
        sqlx::query_scalar("SELECT note FROM user_notes WHERE user_id = $1 AND target_id = $2")
            .bind(i(user.get()))
            .bind(i(target.get()))
            .fetch_optional(&self.pool)
            .await
            .map_err(db)
    }

    async fn set_note(&self, user: UserId, target: UserId, note: Option<&str>) -> RepoResult<()> {
        match note {
            Some(n) => {
                sqlx::query(
                    "INSERT INTO user_notes (user_id, target_id, note) VALUES ($1, $2, $3) \
                     ON CONFLICT (user_id, target_id) DO UPDATE SET note = $3, updated_at = now()",
                )
                .bind(i(user.get()))
                .bind(i(target.get()))
                .bind(n)
                .execute(&self.pool)
                .await
                .map_err(db)?;
            }
            None => {
                sqlx::query("DELETE FROM user_notes WHERE user_id = $1 AND target_id = $2")
                    .bind(i(user.get()))
                    .bind(i(target.get()))
                    .execute(&self.pool)
                    .await
                    .map_err(db)?;
            }
        }
        Ok(())
    }
}

fn rel_from_row(r: &PgRow) -> RepoResult<Relationship> {
    Ok(Relationship {
        user: uid(r.try_get("user_id").map_err(db)?),
        target: uid(r.try_get("target_id").map_err(db)?),
        kind: RelationshipKind::from_code(r.try_get("type").map_err(db)?)
            .ok_or_else(|| bad("type"))?,
        nickname: r.try_get("nickname").map_err(db)?,
        since_ms: r.try_get("since_ms").map_err(db)?,
    })
}

// ---- DMs ----

const DM_SELECT: &str = "SELECT c.id, c.type, c.owner_id, c.name, c.icon_key, c.last_message_id, \
    COALESCE((SELECT array_agg(r.user_id ORDER BY r.seq) FROM channel_recipients r \
              WHERE r.channel_id = c.id), '{}') AS recipients \
    FROM channels c";

fn dm_from_row(r: &PgRow) -> RepoResult<DmChannel> {
    let kind = match r.try_get::<i16, _>("type").map_err(db)? {
        1 => DmKind::Dm,
        3 => DmKind::Group,
        _ => return Err(bad("type")),
    };
    let recipients: Vec<i64> = r.try_get("recipients").map_err(db)?;
    Ok(DmChannel {
        id: ChannelId::new(r.try_get::<i64, _>("id").map_err(db)? as u64),
        kind,
        owner: r
            .try_get::<Option<i64>, _>("owner_id")
            .map_err(db)?
            .map(uid),
        name: r.try_get("name").map_err(db)?,
        icon_key: r.try_get("icon_key").map_err(db)?,
        recipients: recipients.into_iter().map(uid).collect(),
        last_message_id: r
            .try_get::<Option<i64>, _>("last_message_id")
            .map_err(db)?
            .map(|m| MessageId::new(m as u64)),
    })
}

#[async_trait]
impl DmRepository for PgSocialRepo {
    async fn find_dm(&self, a: UserId, b: UserId) -> RepoResult<Option<DmChannel>> {
        let (x, y) = if a < b { (a, b) } else { (b, a) };
        let id: Option<i64> = sqlx::query_scalar(
            "SELECT channel_id FROM dm_channels WHERE user_a = $1 AND user_b = $2",
        )
        .bind(i(x.get()))
        .bind(i(y.get()))
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        match id {
            Some(id) => self.get_dm(ChannelId::new(id as u64)).await,
            None => Ok(None),
        }
    }

    async fn insert_dm(&self, id: ChannelId, a: UserId, b: UserId) -> RepoResult<()> {
        let (x, y) = if a < b { (a, b) } else { (b, a) };
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query("INSERT INTO channels (id, guild_id, type) VALUES ($1, NULL, 1)")
            .bind(i(id.get()))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        for u in [x, y] {
            sqlx::query("INSERT INTO channel_recipients (channel_id, user_id) VALUES ($1, $2)")
                .bind(i(id.get()))
                .bind(i(u.get()))
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        sqlx::query("INSERT INTO dm_channels (channel_id, user_a, user_b) VALUES ($1, $2, $3)")
            .bind(i(id.get()))
            .bind(i(x.get()))
            .bind(i(y.get()))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    async fn insert_group(
        &self,
        id: ChannelId,
        members: &[UserId],
        name: Option<&str>,
    ) -> RepoResult<()> {
        let owner = members
            .first()
            .ok_or_else(|| RepoError("empty group".into()))?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query(
            "INSERT INTO channels (id, guild_id, type, name, owner_id) VALUES ($1, NULL, 3, $2, $3)",
        )
        .bind(i(id.get()))
        .bind(name)
        .bind(i(owner.get()))
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        for u in members {
            sqlx::query("INSERT INTO channel_recipients (channel_id, user_id) VALUES ($1, $2)")
                .bind(i(id.get()))
                .bind(i(u.get()))
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        tx.commit().await.map_err(db)
    }

    async fn get_dm(&self, id: ChannelId) -> RepoResult<Option<DmChannel>> {
        let sql = format!("{DM_SELECT} WHERE c.id = $1 AND c.type IN (1, 3)");
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(id.get()))
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        row.as_ref().map(dm_from_row).transpose()
    }

    async fn add_recipients(&self, id: ChannelId, users: &[UserId]) -> RepoResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        for u in users {
            sqlx::query(
                "INSERT INTO channel_recipients (channel_id, user_id) VALUES ($1, $2) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(i(id.get()))
            .bind(i(u.get()))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        tx.commit().await.map_err(db)
    }

    async fn remove_recipient(&self, id: ChannelId, user: UserId) -> RepoResult<()> {
        sqlx::query("DELETE FROM channel_recipients WHERE channel_id = $1 AND user_id = $2")
            .bind(i(id.get()))
            .bind(i(user.get()))
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn set_owner(&self, id: ChannelId, owner: UserId) -> RepoResult<()> {
        sqlx::query(
            "UPDATE channels SET owner_id = $2, updated_at = now() WHERE id = $1 AND type = 3",
        )
        .bind(i(id.get()))
        .bind(i(owner.get()))
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn update_group(
        &self,
        id: ChannelId,
        name: Option<&str>,
        icon_key: Option<&str>,
    ) -> RepoResult<()> {
        sqlx::query(
            "UPDATE channels SET name = $2, icon_key = $3, updated_at = now() \
             WHERE id = $1 AND type = 3",
        )
        .bind(i(id.get()))
        .bind(name)
        .bind(icon_key)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn delete_dm(&self, id: ChannelId) -> RepoResult<()> {
        sqlx::query("DELETE FROM channels WHERE id = $1 AND type IN (1, 3)")
            .bind(i(id.get()))
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn set_hidden(&self, user: UserId, id: ChannelId, hidden: bool) -> RepoResult<bool> {
        // Only a row whose visibility actually flips counts as changed.
        let done = sqlx::query(
            "UPDATE channel_recipients \
             SET hidden_at = CASE WHEN $3 THEN now() ELSE NULL END \
             WHERE channel_id = $1 AND user_id = $2 \
               AND (hidden_at IS NULL) = $3",
        )
        .bind(i(id.get()))
        .bind(i(user.get()))
        .bind(hidden)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(done.rows_affected() > 0)
    }

    async fn list_dms(&self, user: UserId, include_hidden: bool) -> RepoResult<Vec<DmChannel>> {
        let sql = format!(
            "{DM_SELECT} JOIN channel_recipients me ON me.channel_id = c.id AND me.user_id = $1 \
             WHERE c.type IN (1, 3) AND ($2 OR me.hidden_at IS NULL) \
             ORDER BY c.last_message_id DESC NULLS LAST, c.id DESC"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(user.get()))
            .bind(include_hidden)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter().map(dm_from_row).collect()
    }

    async fn count_groups(&self, user: UserId) -> RepoResult<usize> {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM channels c JOIN channel_recipients r ON r.channel_id = c.id \
             WHERE c.type = 3 AND r.user_id = $1",
        )
        .bind(i(user.get()))
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        Ok(usize::try_from(n).unwrap_or(0))
    }

    async fn get_request_state(
        &self,
        user: UserId,
        id: ChannelId,
    ) -> RepoResult<Option<RequestState>> {
        let code: Option<String> = sqlx::query_scalar(
            "SELECT request_state FROM channel_recipients WHERE channel_id = $1 AND user_id = $2",
        )
        .bind(i(id.get()))
        .bind(i(user.get()))
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        code.map(|c| RequestState::parse(&c).ok_or_else(|| bad("request_state")))
            .transpose()
    }

    async fn set_request_state(
        &self,
        user: UserId,
        id: ChannelId,
        state: RequestState,
    ) -> RepoResult<()> {
        sqlx::query(
            "UPDATE channel_recipients SET request_state = $3 WHERE channel_id = $1 AND user_id = $2",
        )
        .bind(i(id.get()))
        .bind(i(user.get()))
        .bind(state.as_str())
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn get_request_reason(
        &self,
        user: UserId,
        id: ChannelId,
    ) -> RepoResult<Option<RequestReason>> {
        let code: Option<Option<String>> = sqlx::query_scalar(
            "SELECT request_reason FROM channel_recipients WHERE channel_id = $1 AND user_id = $2",
        )
        .bind(i(id.get()))
        .bind(i(user.get()))
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        code.flatten()
            .map(|c| RequestReason::parse(&c).ok_or_else(|| bad("request_reason")))
            .transpose()
    }

    async fn set_request_reason(
        &self,
        user: UserId,
        id: ChannelId,
        reason: Option<RequestReason>,
    ) -> RepoResult<()> {
        sqlx::query(
            "UPDATE channel_recipients SET request_reason = $3 WHERE channel_id = $1 AND user_id = $2",
        )
        .bind(i(id.get()))
        .bind(i(user.get()))
        .bind(reason.map(RequestReason::as_str))
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn shared_protection(&self, a: UserId, b: UserId) -> RepoResult<SharedProtection> {
        let (shared, filtered, explicit, days, paused): (i64, i64, bool, i32, i64) = sqlx::query_as(
            "SELECT count(*), \n                    count(*) FILTER (WHERE COALESCE(c.dm_spam_filter, FALSE)), \n                    COALESCE(bool_or(c.explicit_media_filter_dms), FALSE), \n                    COALESCE(max(c.warn_new_account_days), 0), \n                    count(*) FILTER (WHERE c.dms_disabled_until > now()) \n             FROM guild_members x \n             JOIN guild_members y ON y.guild_id = x.guild_id \n             LEFT JOIN guild_community c ON c.guild_id = x.guild_id \n             WHERE x.user_id = $1 AND y.user_id = $2",
        )
        .bind(i(a.get()))
        .bind(i(b.get()))
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        Ok(SharedProtection {
            shared: usize::try_from(shared).unwrap_or(0),
            spam_filter_all: shared > 0 && filtered == shared,
            explicit_media_any: explicit,
            warn_new_account_days: u32::try_from(days).unwrap_or(0),
            dms_paused_all: shared > 0 && paused == shared,
        })
    }

    async fn insert_group_invite(&self, invite: &GroupInvite) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO group_dm_invites (code, channel_id, inviter_id, created_at, expires_at) \
             VALUES ($1, $2, $3, to_timestamp($4::bigint / 1000.0), to_timestamp($5::bigint / 1000.0))",
        )
        .bind(&invite.code)
        .bind(i(invite.channel_id.get()))
        .bind(i(invite.inviter_id.get()))
        .bind(invite.created_at_ms)
        .bind(invite.expires_at_ms)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn get_group_invite(&self, code: &str) -> RepoResult<Option<GroupInvite>> {
        let row = sqlx::query(GROUP_INVITE_SELECT_ONE)
            .bind(code)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        row.as_ref().map(group_invite_from).transpose()
    }

    async fn revoke_group_invite(&self, code: &str) -> RepoResult<bool> {
        let r = sqlx::query(
            "UPDATE group_dm_invites SET revoked_at = now() WHERE code = $1 AND revoked_at IS NULL",
        )
        .bind(code)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(r.rows_affected() > 0)
    }

    async fn list_group_invites(&self, channel: ChannelId) -> RepoResult<Vec<GroupInvite>> {
        let rows = sqlx::query(GROUP_INVITE_SELECT_CHANNEL)
            .bind(i(channel.get()))
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter().map(group_invite_from).collect()
    }
}

const GROUP_INVITE_SELECT_ONE: &str = "SELECT code, channel_id, inviter_id, \
    (EXTRACT(EPOCH FROM created_at) * 1000)::bigint AS created_ms, \
    (EXTRACT(EPOCH FROM expires_at) * 1000)::bigint AS expires_ms, \
    revoked_at IS NOT NULL AS revoked FROM group_dm_invites WHERE code = $1";

const GROUP_INVITE_SELECT_CHANNEL: &str = "SELECT code, channel_id, inviter_id, \
    (EXTRACT(EPOCH FROM created_at) * 1000)::bigint AS created_ms, \
    (EXTRACT(EPOCH FROM expires_at) * 1000)::bigint AS expires_ms, \
    revoked_at IS NOT NULL AS revoked FROM group_dm_invites WHERE channel_id = $1 \
    ORDER BY created_at DESC";

fn group_invite_from(r: &PgRow) -> RepoResult<GroupInvite> {
    Ok(GroupInvite {
        code: r.try_get("code").map_err(db)?,
        channel_id: ChannelId::new(r.try_get::<i64, _>("channel_id").map_err(db)? as u64),
        inviter_id: uid(r.try_get("inviter_id").map_err(db)?),
        created_at_ms: r.try_get("created_ms").map_err(db)?,
        expires_at_ms: r.try_get("expires_ms").map_err(db)?,
        revoked: r.try_get("revoked").map_err(db)?,
    })
}

// ---- read states ----

fn read_state_from_row(r: &PgRow) -> RepoResult<ReadState> {
    Ok(ReadState {
        user: uid(r.try_get("user_id").map_err(db)?),
        channel: ChannelId::new(r.try_get::<i64, _>("channel_id").map_err(db)? as u64),
        last_read: r
            .try_get::<Option<i64>, _>("last_read_message_id")
            .map_err(db)?
            .map(|m| MessageId::new(m as u64)),
        mention_count: u32::try_from(r.try_get::<i32, _>("mention_count").map_err(db)?)
            .unwrap_or(0),
        last_pin_ms: r.try_get("last_pin_ms").map_err(db)?,
    })
}

fn read_state_select() -> String {
    format!(
        "SELECT user_id, channel_id, last_read_message_id, mention_count, {} AS last_pin_ms \
         FROM read_states",
        ms("last_pin_timestamp")
    )
}

#[async_trait]
impl ReadStateRepository for PgSocialRepo {
    async fn get_read_state(
        &self,
        user: UserId,
        channel: ChannelId,
    ) -> RepoResult<Option<ReadState>> {
        let sql = format!(
            "{} WHERE user_id = $1 AND channel_id = $2",
            read_state_select()
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(user.get()))
            .bind(i(channel.get()))
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        row.as_ref().map(read_state_from_row).transpose()
    }

    async fn put_read_state(&self, s: &ReadState) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO read_states (user_id, channel_id, last_read_message_id, mention_count, \
               last_pin_timestamp) \
             VALUES ($1, $2, $3, $4, to_timestamp($5::bigint / 1000.0)) \
             ON CONFLICT (user_id, channel_id) DO UPDATE SET \
               last_read_message_id = EXCLUDED.last_read_message_id, \
               mention_count = EXCLUDED.mention_count, \
               last_pin_timestamp = EXCLUDED.last_pin_timestamp, updated_at = now()",
        )
        .bind(i(s.user.get()))
        .bind(i(s.channel.get()))
        .bind(s.last_read.map(|m| i(m.get())))
        .bind(i32::try_from(s.mention_count).unwrap_or(i32::MAX))
        .bind(s.last_pin_ms)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn list_read_states(&self, user: UserId) -> RepoResult<Vec<ReadState>> {
        let sql = format!(
            "{} WHERE user_id = $1 ORDER BY channel_id",
            read_state_select()
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(user.get()))
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter().map(read_state_from_row).collect()
    }

    async fn add_mentions(&self, users: &[UserId], channel: ChannelId, n: u32) -> RepoResult<()> {
        let ids: Vec<i64> = users.iter().map(|u| i(u.get())).collect();
        let cap = i32::try_from(MAX_MENTION_COUNT).unwrap_or(i32::MAX);
        sqlx::query(
            "INSERT INTO read_states (user_id, channel_id, mention_count) \
             SELECT u, $2, LEAST($3, $4) FROM unnest($1::bigint[]) AS u \
             ON CONFLICT (user_id, channel_id) DO UPDATE SET \
               mention_count = LEAST(read_states.mention_count + $3, $4), updated_at = now()",
        )
        .bind(&ids)
        .bind(i(channel.get()))
        .bind(i32::try_from(n).unwrap_or(i32::MAX))
        .bind(cap)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }
}

// ---- notification settings ----

#[async_trait]
impl NotificationRepository for PgSocialRepo {
    async fn get_scope(
        &self,
        user: UserId,
        guild: Option<GuildId>,
    ) -> RepoResult<Option<ScopeSettings>> {
        let sql = format!(
            "SELECT level, suppress_everyone, suppress_roles, suppress_highlights, \
             mute_scheduled_events, mobile_push, muted, {} AS mute_end_ms \
             FROM notification_settings WHERE user_id = $1 AND guild_id = $2",
            ms("mute_end_at")
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(user.get()))
            .bind(guild_key(guild))
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        let Some(r) = row else { return Ok(None) };
        Ok(Some(ScopeSettings {
            user,
            guild,
            level: level_of(&r)?,
            suppress_everyone: r.try_get("suppress_everyone").map_err(db)?,
            suppress_roles: r.try_get("suppress_roles").map_err(db)?,
            suppress_highlights: r.try_get("suppress_highlights").map_err(db)?,
            mute_scheduled_events: r.try_get("mute_scheduled_events").map_err(db)?,
            mobile_push: r.try_get("mobile_push").map_err(db)?,
            mute: mute_of(&r)?,
        }))
    }

    async fn put_scope(&self, s: &ScopeSettings) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO notification_settings (user_id, guild_id, level, suppress_everyone, \
               suppress_roles, suppress_highlights, mute_scheduled_events, mobile_push, muted, \
               mute_end_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, to_timestamp($10::bigint / 1000.0)) \
             ON CONFLICT (user_id, guild_id) DO UPDATE SET level = $3, suppress_everyone = $4, \
               suppress_roles = $5, suppress_highlights = $6, mute_scheduled_events = $7, \
               mobile_push = $8, muted = $9, mute_end_at = to_timestamp($10::bigint / 1000.0), \
               updated_at = now()",
        )
        .bind(i(s.user.get()))
        .bind(guild_key(s.guild))
        .bind(level_code(s.level))
        .bind(s.suppress_everyone)
        .bind(s.suppress_roles)
        .bind(s.suppress_highlights)
        .bind(s.mute_scheduled_events)
        .bind(s.mobile_push)
        .bind(s.mute.muted)
        .bind(s.mute.end_ms)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn get_override(
        &self,
        user: UserId,
        channel: ChannelId,
    ) -> RepoResult<Option<ChannelOverride>> {
        let sql = format!(
            "SELECT user_id, channel_id, level, muted, {} AS mute_end_ms \
             FROM channel_notification_overrides WHERE user_id = $1 AND channel_id = $2",
            ms("mute_end_at")
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(user.get()))
            .bind(i(channel.get()))
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        row.as_ref().map(override_from_row).transpose()
    }

    async fn put_override(&self, o: &ChannelOverride) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO channel_notification_overrides (user_id, channel_id, level, muted, \
               mute_end_at) \
             VALUES ($1, $2, $3, $4, to_timestamp($5::bigint / 1000.0)) \
             ON CONFLICT (user_id, channel_id) DO UPDATE SET level = $3, muted = $4, \
               mute_end_at = to_timestamp($5::bigint / 1000.0), updated_at = now()",
        )
        .bind(i(o.user.get()))
        .bind(i(o.channel.get()))
        .bind(level_code(o.level))
        .bind(o.mute.muted)
        .bind(o.mute.end_ms)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn delete_override(&self, user: UserId, channel: ChannelId) -> RepoResult<()> {
        sqlx::query(
            "DELETE FROM channel_notification_overrides WHERE user_id = $1 AND channel_id = $2",
        )
        .bind(i(user.get()))
        .bind(i(channel.get()))
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn list_overrides(&self, user: UserId) -> RepoResult<Vec<ChannelOverride>> {
        let sql = format!(
            "SELECT user_id, channel_id, level, muted, {} AS mute_end_ms \
             FROM channel_notification_overrides WHERE user_id = $1 ORDER BY channel_id",
            ms("mute_end_at")
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(user.get()))
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter().map(override_from_row).collect()
    }

    async fn list_scopes(&self, user: UserId) -> RepoResult<Vec<ScopeSettings>> {
        let sql = format!(
            "SELECT guild_id, level, suppress_everyone, suppress_roles, suppress_highlights, \
             mute_scheduled_events, mobile_push, muted, {} AS mute_end_ms \
             FROM notification_settings WHERE user_id = $1 ORDER BY guild_id",
            ms("mute_end_at")
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(user.get()))
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter()
            .map(|r| {
                let key: i64 = r.try_get("guild_id").map_err(db)?;
                Ok(ScopeSettings {
                    user,
                    guild: (key != 0).then(|| GuildId::new(key as u64)),
                    level: level_of(r)?,
                    suppress_everyone: r.try_get("suppress_everyone").map_err(db)?,
                    suppress_roles: r.try_get("suppress_roles").map_err(db)?,
                    suppress_highlights: r.try_get("suppress_highlights").map_err(db)?,
                    mute_scheduled_events: r.try_get("mute_scheduled_events").map_err(db)?,
                    mobile_push: r.try_get("mobile_push").map_err(db)?,
                    mute: mute_of(r)?,
                })
            })
            .collect()
    }
}

fn override_from_row(r: &PgRow) -> RepoResult<ChannelOverride> {
    Ok(ChannelOverride {
        user: uid(r.try_get("user_id").map_err(db)?),
        channel: ChannelId::new(r.try_get::<i64, _>("channel_id").map_err(db)? as u64),
        level: level_of(r)?,
        mute: mute_of(r)?,
    })
}

// ---- profiles ----

#[async_trait]
impl ProfileRepository for PgSocialRepo {
    async fn get_profile(&self, user: UserId) -> RepoResult<Option<Profile>> {
        let row = sqlx::query(
            "SELECT a.avatar_key, a.banner_key, a.accent_color, a.bio, a.pronouns, a.display_name, \
             a.avatar_decoration, a.profile_effect, a.theme_primary, a.theme_accent, a.is_bot, \
             a.flags, (EXTRACT(EPOCH FROM a.created_at) * 1000)::BIGINT AS created_ms, \
             (EXISTS(SELECT 1 FROM staff_appointments s \
                     WHERE s.account_id = a.id AND s.revoked_at IS NULL) \
              AND COALESCE((SELECT us.settings #>> '{staff,show_badge}' FROM user_settings us \
                            WHERE us.user_id = a.id), 'true') <> 'false') AS is_staff, \
             (EXISTS(SELECT 1 FROM guilds g WHERE g.is_staff_guild AND g.owner_id = a.id) \
              AND COALESCE((SELECT us.settings #>> '{staff,show_badge}' FROM user_settings us \
                            WHERE us.user_id = a.id), 'true') <> 'false') AS is_owner, \
             (EXISTS(SELECT 1 FROM entitlement_grants e \
                     WHERE e.kind = 'patron' AND e.user_id = a.id AND e.revoked_ms IS NULL \
                       AND (e.permanent OR e.ends_ms > (EXTRACT(EPOCH FROM now()) * 1000)::BIGINT \
                            OR e.starts_ms + COALESCE(e.days, 0) * 86400000 \
                               > (EXTRACT(EPOCH FROM now()) * 1000)::BIGINT)) \
              AND NOT COALESCE((SELECT d.hide_badge FROM donor_preferences d \
                                WHERE d.user_id = a.id), FALSE)) AS is_patron \
             FROM accounts a WHERE a.id = $1",
        )
        .bind(i(user.get()))
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        let Some(r) = row else { return Ok(None) };
        let flags: i64 = r.try_get("flags").map_err(db)?;
        // One catalogue (`sr_admin::BADGES`): flag-backed badges read `accounts.flags`, the
        // rest are derived from the queries above.
        let mut badges = Vec::new();
        for b in sr_admin::BADGES {
            let on = match (b.flag, b.id) {
                (Some(bit), _) => flags & bit != 0,
                (None, "owner") => r.try_get::<bool, _>("is_owner").map_err(db)?,
                (None, "staff") => r.try_get::<bool, _>("is_staff").map_err(db)?,
                (None, "patron") => r.try_get::<bool, _>("is_patron").map_err(db)?,
                (None, "bot") => r.try_get::<bool, _>("is_bot").map_err(db)?,
                (None, _) => false,
            };
            if on {
                badges.push(b.id.to_owned());
            }
        }
        let sql = format!(
            "SELECT provider, external_id, display_name, verified, visible, url, show_activity, \
             stats, {} AS created_ms \
             FROM user_connections WHERE user_id = $1 ORDER BY created_at, provider, external_id",
            ms("created_at")
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(user.get()))
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        let connections = rows
            .iter()
            .map(|c| {
                Ok(Connection {
                    user,
                    provider: c.try_get("provider").map_err(db)?,
                    external_id: c.try_get("external_id").map_err(db)?,
                    display_name: c.try_get("display_name").map_err(db)?,
                    verified: c.try_get("verified").map_err(db)?,
                    visible: c.try_get("visible").map_err(db)?,
                    url: c.try_get("url").map_err(db)?,
                    show_activity: c.try_get("show_activity").map_err(db)?,
                    stats: c.try_get("stats").map_err(db)?,
                    created_ms: c.try_get("created_ms").map_err(db)?,
                })
            })
            .collect::<RepoResult<Vec<_>>>()?;
        Ok(Some(Profile {
            user,
            avatar_key: r.try_get("avatar_key").map_err(db)?,
            banner_key: r.try_get("banner_key").map_err(db)?,
            accent_color: r
                .try_get::<Option<i32>, _>("accent_color")
                .map_err(db)?
                .map(|c| c as u32),
            bio: r.try_get("bio").map_err(db)?,
            pronouns: r.try_get("pronouns").map_err(db)?,
            display_name: r.try_get("display_name").map_err(db)?,
            avatar_decoration: r.try_get("avatar_decoration").map_err(db)?,
            profile_effect: r.try_get("profile_effect").map_err(db)?,
            theme_colors: match (
                r.try_get::<Option<i32>, _>("theme_primary").map_err(db)?,
                r.try_get::<Option<i32>, _>("theme_accent").map_err(db)?,
            ) {
                (Some(a), Some(b)) => Some([a as u32, b as u32]),
                _ => None,
            },
            created_ms: r.try_get("created_ms").map_err(db)?,
            badges,
            connections,
        }))
    }

    async fn put_profile(&self, p: &Profile) -> RepoResult<()> {
        sqlx::query(
            "UPDATE accounts SET banner_key = $2, accent_color = $3, bio = $4, pronouns = $5, \
             avatar_key = $6, display_name = $7, avatar_decoration = $8, profile_effect = $9, \
             theme_primary = $10, theme_accent = $11, updated_at = now() WHERE id = $1",
        )
        .bind(i(p.user.get()))
        .bind(p.banner_key.as_deref())
        .bind(p.accent_color.map(|c| c as i32))
        .bind(p.bio.as_deref())
        .bind(p.pronouns.as_deref())
        .bind(p.avatar_key.as_deref())
        .bind(p.display_name.as_deref())
        .bind(p.avatar_decoration.as_deref())
        .bind(p.profile_effect.as_deref())
        .bind(p.theme_colors.map(|c| c[0] as i32))
        .bind(p.theme_colors.map(|c| c[1] as i32))
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn upsert_connection(&self, c: &Connection) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO user_connections (user_id, provider, external_id, display_name, \
               verified, visible, url, show_activity, stats, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $8, $9, $10, to_timestamp($7::bigint / 1000.0)) \
             ON CONFLICT (user_id, provider, external_id) DO UPDATE SET \
               display_name = $4, verified = user_connections.verified OR $5, visible = $6, \
               url = $8, show_activity = $9, stats = $10, updated_at = now()",
        )
        .bind(i(c.user.get()))
        .bind(&c.provider)
        .bind(&c.external_id)
        .bind(&c.display_name)
        .bind(c.verified)
        .bind(c.visible)
        .bind(c.created_ms)
        .bind(c.url.as_deref())
        .bind(c.show_activity)
        .bind(&c.stats)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn delete_connection(
        &self,
        user: UserId,
        provider: &str,
        external_id: &str,
    ) -> RepoResult<()> {
        sqlx::query(
            "DELETE FROM user_connections WHERE user_id = $1 AND provider = $2 AND external_id = $3",
        )
        .bind(i(user.get()))
        .bind(provider)
        .bind(external_id)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn connection_owner(
        &self,
        provider: &str,
        external_id: &str,
    ) -> RepoResult<Option<UserId>> {
        let owner: Option<i64> = sqlx::query_scalar(
            "SELECT user_id FROM user_connections \
             WHERE provider = $1 AND external_id = $2 AND verified LIMIT 1",
        )
        .bind(provider)
        .bind(external_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        Ok(owner.map(|o| UserId::new(o as u64)))
    }

    async fn stale_connections(
        &self,
        before_ms: i64,
        providers: &[&str],
        limit: usize,
    ) -> RepoResult<Vec<Connection>> {
        let providers: Vec<String> = providers.iter().map(|p| (*p).to_owned()).collect();
        let rows = sqlx::query(
            "SELECT user_id, provider, external_id, display_name, visible, url, stats \
             FROM user_connections \
             WHERE verified AND show_activity AND provider = ANY($1) \
               AND (stats_refreshed_at IS NULL \
                    OR stats_refreshed_at < to_timestamp($2::bigint / 1000.0)) \
             ORDER BY stats_refreshed_at NULLS FIRST LIMIT $3",
        )
        .bind(&providers)
        .bind(before_ms)
        .bind(i64::try_from(limit).unwrap_or(100))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(|c| {
                Ok(Connection {
                    user: UserId::new(c.try_get::<i64, _>("user_id").map_err(db)? as u64),
                    provider: c.try_get("provider").map_err(db)?,
                    external_id: c.try_get("external_id").map_err(db)?,
                    display_name: c.try_get("display_name").map_err(db)?,
                    verified: true,
                    visible: c.try_get("visible").map_err(db)?,
                    url: c.try_get("url").map_err(db)?,
                    show_activity: true,
                    stats: c.try_get("stats").map_err(db)?,
                    created_ms: 0,
                })
            })
            .collect()
    }

    async fn set_connection_stats(
        &self,
        user: UserId,
        provider: &str,
        external_id: &str,
        stats: &serde_json::Value,
        now_ms: i64,
    ) -> RepoResult<()> {
        sqlx::query(
            "UPDATE user_connections SET stats = $4, \
               stats_refreshed_at = to_timestamp($5::bigint / 1000.0), updated_at = now() \
             WHERE user_id = $1 AND provider = $2 AND external_id = $3",
        )
        .bind(i(user.get()))
        .bind(provider)
        .bind(external_id)
        .bind(stats)
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn count_connections(&self, user: UserId) -> RepoResult<usize> {
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM user_connections WHERE user_id = $1")
            .bind(i(user.get()))
            .fetch_one(&self.pool)
            .await
            .map_err(db)?;
        Ok(usize::try_from(n).unwrap_or(0))
    }
}

// ---- presence ----

#[async_trait]
impl PresenceRepository for PgSocialRepo {
    async fn get_presence(&self, user: UserId) -> RepoResult<Option<PresencePrefs>> {
        let sql = format!(
            "SELECT status, {} AS status_expires_ms, custom_text, custom_emoji_name, \
             custom_emoji_id, {} AS custom_expires_ms \
             FROM user_presence_prefs WHERE user_id = $1",
            ms("status_expires_at"),
            ms("custom_expires_at")
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(user.get()))
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        let Some(r) = row else { return Ok(None) };
        Ok(Some(PresencePrefs {
            user,
            status: Status::from_code(r.try_get("status").map_err(db)?)
                .ok_or_else(|| bad("status"))?,
            status_expires_ms: r.try_get("status_expires_ms").map_err(db)?,
            custom: CustomStatus {
                text: r.try_get("custom_text").map_err(db)?,
                emoji_name: r.try_get("custom_emoji_name").map_err(db)?,
                emoji_id: r
                    .try_get::<Option<i64>, _>("custom_emoji_id")
                    .map_err(db)?
                    .map(|v| v as u64),
                expires_ms: r.try_get("custom_expires_ms").map_err(db)?,
            },
        }))
    }

    async fn put_presence(&self, p: &PresencePrefs) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO user_presence_prefs (user_id, status, status_expires_at, custom_text, \
               custom_emoji_name, custom_emoji_id, custom_expires_at) \
             VALUES ($1, $2, to_timestamp($3::bigint / 1000.0), $4, $5, $6, \
                     to_timestamp($7::bigint / 1000.0)) \
             ON CONFLICT (user_id) DO UPDATE SET status = $2, \
               status_expires_at = to_timestamp($3::bigint / 1000.0), custom_text = $4, \
               custom_emoji_name = $5, custom_emoji_id = $6, \
               custom_expires_at = to_timestamp($7::bigint / 1000.0), updated_at = now()",
        )
        .bind(i(p.user.get()))
        .bind(p.status.code())
        .bind(p.status_expires_ms)
        .bind(p.custom.text.as_deref())
        .bind(p.custom.emoji_name.as_deref())
        .bind(p.custom.emoji_id.map(i))
        .bind(p.custom.expires_ms)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }
}

#[async_trait]
impl UserSettingsRepository for PgSocialRepo {
    async fn get_user_settings(&self, user: UserId) -> RepoResult<Option<UserSettings>> {
        let row = sqlx::query("SELECT settings, version FROM user_settings WHERE user_id = $1")
            .bind(i(user.get()))
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        row.map(|r| {
            Ok(UserSettings {
                data: r.try_get("settings").map_err(db)?,
                version: r.try_get("version").map_err(db)?,
            })
        })
        .transpose()
    }

    async fn put_user_settings(
        &self,
        user: UserId,
        data: &serde_json::Value,
        expected_version: i64,
    ) -> RepoResult<Option<i64>> {
        sqlx::query_scalar(
            "INSERT INTO user_settings (user_id, settings, version) VALUES ($1, $2, 1) \
             ON CONFLICT (user_id) DO UPDATE SET settings = $2, \
               version = user_settings.version + 1, updated_at = now() \
             WHERE user_settings.version = $3 RETURNING version",
        )
        .bind(i(user.get()))
        .bind(data)
        .bind(expected_version)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)
    }

    async fn get_default_notification_level(
        &self,
        user: UserId,
    ) -> RepoResult<Option<NotificationLevel>> {
        let code: Option<Option<i16>> = sqlx::query_scalar(
            "SELECT default_notification_level FROM user_settings WHERE user_id = $1",
        )
        .bind(i(user.get()))
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        code.flatten()
            .map(|c| {
                NotificationLevel::from_code(c).ok_or_else(|| bad("default_notification_level"))
            })
            .transpose()
    }

    async fn put_default_notification_level(
        &self,
        user: UserId,
        level: Option<NotificationLevel>,
    ) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO user_settings (user_id, default_notification_level) VALUES ($1, $2) \
             ON CONFLICT (user_id) DO UPDATE SET default_notification_level = $2, \
               updated_at = now()",
        )
        .bind(i(user.get()))
        .bind(level_code(level))
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn get_notification_keywords(&self, user: UserId) -> RepoResult<Vec<String>> {
        let v: Option<Vec<String>> = sqlx::query_scalar(
            "SELECT notification_keywords FROM user_settings WHERE user_id = $1",
        )
        .bind(i(user.get()))
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        Ok(v.unwrap_or_default())
    }

    async fn put_notification_keywords(&self, user: UserId, keywords: &[String]) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO user_settings (user_id, notification_keywords) VALUES ($1, $2) \n             ON CONFLICT (user_id) DO UPDATE SET notification_keywords = $2, updated_at = now()",
        )
        .bind(i(user.get()))
        .bind(keywords)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }
}
