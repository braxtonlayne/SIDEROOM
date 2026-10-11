//! Seams that keep the gateway independent of sr-auth / sr-db / sr-api.

use async_trait::async_trait;
use serde_json::{Value, json};
use sr_core::{ChannelId, GuildId, Permissions, UserId};
use thiserror::Error;

use crate::session::SessionHandle;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("invalid token")]
    Invalid,
    #[error("auth backend unavailable: {0}")]
    Unavailable(String),
}

/// Who a token belongs to.
#[derive(Debug, Clone)]
pub struct AuthedUser {
    pub user_id: UserId,
    pub bot: bool,
    /// Privileged intent bits this identity may use (bots only; users get all).
    pub allowed_privileged_intents: u64,
    /// User object placed in READY (`id` should be set).
    pub user: Value,
}

#[async_trait]
pub trait Authenticator: Send + Sync {
    /// Resolve a gateway token (bot token or user session token).
    async fn authenticate(&self, token: &str) -> Result<AuthedUser, AuthError>;

    /// Resolve a browser `sr_session` cookie value. Used only when IDENTIFY or
    /// RESUME carries an empty token, the upgrade request has the cookie, and
    /// its `Origin` is in [`crate::GatewayConfig::allowed_origins`]. Default: refuse.
    async fn authenticate_session_cookie(&self, _cookie: &str) -> Result<AuthedUser, AuthError> {
        Err(AuthError::Invalid)
    }
}

/// A pluggable handler for client opcodes the gateway does not implement itself
/// (voice, member requests, watch parties, E2EE). Registered in
/// [`crate::GatewayConfig::handlers`]. Opcodes no handler claims are ignored.
#[async_trait]
pub trait OpHandler: Send + Sync {
    /// Opcodes this handler wants.
    fn ops(&self) -> &[u8];

    /// Called once per matching client op from an authenticated session.
    async fn handle(&self, session: &SessionHandle, op: u8, d: Value);
}

/// Guild membership and visibility lookups. The permission decision
/// (`VIEW_CHANNEL` after overwrites) lives behind `can_view_channel`; the
/// gateway never computes permissions itself.
#[async_trait]
pub trait GuildDirectory: Send + Sync {
    async fn guilds_for_user(&self, user: UserId) -> Vec<GuildId>;

    async fn can_view_channel(&self, user: UserId, guild: GuildId, channel: ChannelId) -> bool;

    /// Whether `user` holds all of `needed` at guild level (owner and
    /// administrator bypass applied). Gates moderation and configuration
    /// dispatches such as `GUILD_AUDIT_LOG_ENTRY_CREATE`; see
    /// [`crate::hub::required_guild_permission`]. Default: no (fail closed).
    async fn has_guild_permission(
        &self,
        _user: UserId,
        _guild: GuildId,
        _needed: Permissions,
    ) -> bool {
        false
    }

    /// Body of `GUILD_CREATE` for `user`. Default is a bare stub.
    async fn guild_payload(&self, _user: UserId, guild: GuildId) -> Value {
        json!({ "id": guild })
    }

    /// Members of the staff guild; they receive `SR_STAFF_ALERT` dispatches
    /// (open question 142). Default: nobody.
    async fn staff_users(&self) -> Vec<UserId> {
        Vec::new()
    }

    /// Extra READY fields (`private_channels`, `relationships`, ...).
    async fn ready_extras(&self, _user: UserId) -> Value {
        json!({})
    }
}
