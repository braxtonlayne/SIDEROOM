//! Postgres-backed [`Authenticator`] and [`GuildDirectory`] for the gateway.

use async_trait::async_trait;
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use sr_auth::{repo::SessionRepository, session::SessionPolicy, session::SessionToken};
use sr_chat::{
    perms::channel_permissions,
    repo::{ChannelRepository, GuildRepository},
};
use sr_core::{ChannelId, Clock, GuildId, Permissions, UserId};
use sr_db::{auth_repo::PgAuthRepo, chat_repo::PgChatRepo};
use sr_gateway::{AuthError, AuthedUser, Authenticator, GuildDirectory};
use std::sync::Arc;

/// Resolves a gateway token to a user. Only browser session tokens work for
/// now; bot tokens wait for the application tables (see `backends`).
pub struct PgAuthenticator {
    auth: PgAuthRepo,
    pool: PgPool,
    clock: Arc<dyn Clock>,
}

impl PgAuthenticator {
    pub fn new(auth: PgAuthRepo, pool: PgPool, clock: Arc<dyn Clock>) -> Self {
        Self { auth, pool, clock }
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

/// Guild membership and channel visibility from Postgres. Visibility is
/// decided by `sr_chat::perms::channel_permissions` (owner/admin bypass,
/// role and member overwrites, `VIEW_CHANNEL`).
pub struct PgGuildDirectory {
    pool: PgPool,
    chat: PgChatRepo,
    clock: Arc<dyn Clock>,
}

impl PgGuildDirectory {
    pub fn new(pool: PgPool, clock: Arc<dyn Clock>) -> Self {
        Self {
            chat: PgChatRepo::new(pool.clone()),
            pool,
            clock,
        }
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

    async fn guild_payload(&self, _user: UserId, guild: GuildId) -> Value {
        match self.chat.get_guild(guild).await {
            Ok(Some(g)) => json!({
                "id": guild,
                "name": g.name,
                "description": g.description,
                "icon": g.icon_key,
                "owner_id": g.owner_id,
                "preferred_locale": g.preferred_locale,
            }),
            _ => json!({ "id": guild, "unavailable": true }),
        }
    }
}
