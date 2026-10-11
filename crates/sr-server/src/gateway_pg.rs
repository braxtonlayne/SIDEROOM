//! Postgres-backed [`Authenticator`] and [`GuildDirectory`] for the gateway.

use async_trait::async_trait;
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use sr_api::routes::guilds::dto::{ChannelObject, GuildObject, MemberObject, iso8601};
use sr_auth::{repo::SessionRepository, session::SessionPolicy, session::SessionToken};
use sr_chat::repo::UserLookup;
use sr_chat::{
    perms::channel_permissions,
    repo::{ChannelRepository, GuildRepository},
    social::{
        dm::DmKind,
        relationships::RelationshipKind,
        repo::{DmRepository, ReadStateRepository, RelationshipRepository},
    },
};
use sr_core::{ChannelId, Clock, GuildId, Permissions, UserId};
use sr_db::{auth_repo::PgAuthRepo, chat_repo::PgChatRepo, social_repo::PgSocialRepo};
use sr_gateway::{AuthError, AuthedUser, Authenticator, GuildDirectory, LARGE_THRESHOLD_DEFAULT};
use std::sync::Arc;

type BuildResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Resolves a gateway token or `sr_session` cookie to a user. Bot tokens wait for
/// the application tables (see `backends`).
pub struct PgAuthenticator {
    auth: PgAuthRepo,
    pool: PgPool,
    clock: Arc<dyn Clock>,
}

impl PgAuthenticator {
    pub fn new(auth: PgAuthRepo, pool: PgPool, clock: Arc<dyn Clock>) -> Self {
        Self { auth, pool, clock }
    }

    async fn resolve(&self, token: &SessionToken) -> Result<AuthedUser, AuthError> {
        let rec = self
            .auth
            .find_by_hash(&token.hash())
            .await
            .map_err(unavailable)?
            .ok_or(AuthError::Invalid)?;
        let now = self.clock.now_unix_ms().div_euclid(1000);
        if !SessionPolicy::is_valid(rec.expires_at, now) {
            return Err(AuthError::Invalid);
        }
        let row = sqlx::query(
            "SELECT username::text AS username, display_name FROM accounts \
             WHERE id = $1 AND state = 'active'",
        )
        .bind(rec.user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(unavailable)?
        .ok_or(AuthError::Invalid)?;
        let username: String = row.try_get("username").map_err(unavailable)?;
        let display: Option<String> = row.try_get("display_name").map_err(unavailable)?;
        let user_id = UserId::new(rec.user_id as u64);
        Ok(AuthedUser {
            user_id,
            bot: false,
            allowed_privileged_intents: 0,
            user: json!({
                "id": user_id.to_string(),
                "username": username,
                "global_name": display,
                "bot": false,
            }),
        })
    }
}

fn unavailable(e: impl std::fmt::Display) -> AuthError {
    AuthError::Unavailable(e.to_string())
}

#[async_trait]
impl Authenticator for PgAuthenticator {
    async fn authenticate(&self, token: &str) -> Result<AuthedUser, AuthError> {
        if token.starts_with("Bot ") {
            return Err(AuthError::Invalid);
        }
        let token = SessionToken::parse(token).ok_or(AuthError::Invalid)?;
        self.resolve(&token).await
    }

    async fn authenticate_session_cookie(&self, cookie: &str) -> Result<AuthedUser, AuthError> {
        let token = SessionToken::parse(cookie).ok_or(AuthError::Invalid)?;
        self.resolve(&token).await
    }
}

/// Guild membership, channel visibility and READY/GUILD_CREATE bodies from Postgres.
/// Visibility is decided by `sr_chat::perms::channel_permissions` (owner/admin bypass,
/// role and member overwrites, `VIEW_CHANNEL`).
pub struct PgGuildDirectory {
    pool: PgPool,
    chat: PgChatRepo,
    social: PgSocialRepo,
    clock: Arc<dyn Clock>,
}

impl PgGuildDirectory {
    pub fn new(pool: PgPool, clock: Arc<dyn Clock>) -> Self {
        Self {
            chat: PgChatRepo::new(pool.clone()),
            social: PgSocialRepo::new(pool.clone()),
            pool,
            clock,
        }
    }

    /// GUILD_CREATE body: guild object, `channels`, `roles`, `threads`, and `members`
    /// when the guild is at most the default large threshold. The hub adds
    /// `presences` for guilds small enough to carry them.
    async fn build_guild_create(&self, guild: GuildId) -> BuildResult<Value> {
        let Some(g) = self.chat.get_guild(guild).await? else {
            return Ok(json!({ "id": guild, "unavailable": true }));
        };
        let roles = self.chat.list_roles(guild).await?;
        let mut channels = Vec::new();
        for c in self.chat.list_guild_channels(guild).await? {
            let overwrites = self.chat.get_overwrites(c.id).await?;
            channels.push(ChannelObject::new(&c, &overwrites));
        }
        let mut body = serde_json::to_value(GuildObject::new(
            &g,
            Some(roles.as_slice()),
            Some(channels.as_slice()),
        ))?;
        // One row past the threshold tells whether the member list fits.
        let member_ids: Vec<i64> = sqlx::query_scalar(
            "SELECT user_id FROM guild_members WHERE guild_id = $1 ORDER BY user_id LIMIT $2",
        )
        .bind(guild.get() as i64)
        .bind(LARGE_THRESHOLD_DEFAULT as i64 + 1)
        .fetch_all(&self.pool)
        .await?;
        if let Some(obj) = body.as_object_mut() {
            obj.insert("threads".into(), json!([]));
            if member_ids.len() as u64 <= LARGE_THRESHOLD_DEFAULT {
                let mut found = Vec::with_capacity(member_ids.len());
                for id in member_ids {
                    let user = UserId::new(id as u64);
                    if let Some(m) = self.chat.get_member(guild, user).await? {
                        found.push(m);
                    }
                }
                // Name fields for the member objects (open question 157).
                let ids: Vec<UserId> = found.iter().map(|m| m.user_id).collect();
                let users: std::collections::HashMap<UserId, sr_chat::repo::UserSummary> = self
                    .chat
                    .users(&ids)
                    .await?
                    .into_iter()
                    .map(|u| (u.id, u))
                    .collect();
                let members: Vec<MemberObject> = found
                    .iter()
                    .map(|m| MemberObject::new(m, false, &users))
                    .collect();
                obj.insert("members".into(), json!(members));
            }
        }
        Ok(body)
    }

    /// READY extras from the social repositories: DM `private_channels`,
    /// `relationships` (friends, blocks, pending requests) and `read_state`.
    async fn build_ready_extras(&self, user: UserId) -> BuildResult<Value> {
        let private_channels: Vec<Value> = self
            .social
            .list_dms(user, false)
            .await?
            .iter()
            .map(|d| {
                json!({
                    "id": d.id,
                    "type": match d.kind { DmKind::Dm => 1, DmKind::Group => 3 },
                    "name": d.name,
                    "icon": d.icon_key,
                    "owner_id": d.owner,
                    "last_message_id": d.last_message_id,
                    "recipient_ids": d.recipients,
                })
            })
            .collect();
        let relationships: Vec<Value> = self
            .social
            .list_relationships(user)
            .await?
            .iter()
            .map(|r| {
                let kind = match r.kind {
                    RelationshipKind::Friend => 1,
                    RelationshipKind::Blocked => 2,
                    RelationshipKind::PendingIncoming => 3,
                    RelationshipKind::PendingOutgoing => 4,
                };
                json!({
                    "id": r.target,
                    "type": kind,
                    "nickname": r.nickname,
                    "since": iso8601(r.since_ms),
                })
            })
            .collect();
        let entries: Vec<Value> = self
            .social
            .list_read_states(user)
            .await?
            .iter()
            .map(|s| {
                json!({
                    "id": s.channel,
                    "last_message_id": s.last_read,
                    "mention_count": s.mention_count,
                    "last_pin_timestamp": s.last_pin_ms.map(iso8601),
                })
            })
            .collect();
        Ok(json!({
            "private_channels": private_channels,
            "relationships": relationships,
            "read_state": { "entries": entries },
        }))
    }
}

#[async_trait]
impl GuildDirectory for PgGuildDirectory {
    async fn guilds_for_user(&self, user: UserId) -> Vec<GuildId> {
        let rows = sqlx::query_scalar::<_, i64>(
            "SELECT guild_id FROM guild_members WHERE user_id = $1 ORDER BY guild_id",
        )
        .bind(user.get() as i64)
        .fetch_all(&self.pool)
        .await;
        match rows {
            Ok(v) => v.into_iter().map(|g| GuildId::new(g as u64)).collect(),
            Err(e) => {
                tracing::error!(error = %e, "guilds_for_user failed");
                Vec::new()
            }
        }
    }

    async fn can_view_channel(&self, user: UserId, guild: GuildId, channel: ChannelId) -> bool {
        let res: Result<bool, sr_chat::error::RepoError> = async {
            let Some(ch) = self.chat.get_channel(channel).await? else {
                return Ok(false);
            };
            if ch.guild_id != Some(guild) {
                return Ok(false);
            }
            let (Some(g), Some(member)) = (
                self.chat.get_guild(guild).await?,
                self.chat.get_member(guild, user).await?,
            ) else {
                return Ok(false);
            };
            let roles = self.chat.list_roles(guild).await?;
            let overwrites = self.chat.get_overwrites(channel).await?;
            let perms =
                channel_permissions(&g, &member, &roles, &overwrites, self.clock.now_unix_ms());
            Ok(perms.contains(Permissions::VIEW_CHANNEL))
        }
        .await;
        res.unwrap_or_else(|e| {
            tracing::error!(error = %e, "can_view_channel failed");
            false
        })
    }

    async fn staff_users(&self) -> Vec<UserId> {
        let rows = sqlx::query_scalar::<_, i64>(
            "SELECT gm.user_id FROM guild_members gm \
             JOIN guilds g ON g.id = gm.guild_id WHERE g.is_staff_guild ORDER BY gm.user_id",
        )
        .fetch_all(&self.pool)
        .await;
        match rows {
            Ok(v) => v.into_iter().map(|u| UserId::new(u as u64)).collect(),
            Err(e) => {
                tracing::error!(error = %e, "staff_users failed");
                Vec::new()
            }
        }
    }

    async fn guild_payload(&self, _user: UserId, guild: GuildId) -> Value {
        self.build_guild_create(guild).await.unwrap_or_else(|e| {
            tracing::error!(error = %e, "guild_payload failed");
            json!({ "id": guild, "unavailable": true })
        })
    }

    async fn ready_extras(&self, user: UserId) -> Value {
        self.build_ready_extras(user).await.unwrap_or_else(|e| {
            tracing::error!(error = %e, "ready_extras failed");
            json!({})
        })
    }
}
