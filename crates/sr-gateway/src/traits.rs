//! Seams that keep the gateway independent of sr-auth / sr-db / sr-api.

use async_trait::async_trait;
use serde_json::{Value, json};
use sr_core::{ChannelId, GuildId, UserId};
use thiserror::Error;

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
}

/// Guild membership and visibility lookups. The permission decision
/// (`VIEW_CHANNEL` after overwrites) lives behind `can_view_channel`; the
/// gateway never computes permissions itself.
#[async_trait]
pub trait GuildDirectory: Send + Sync {
    async fn guilds_for_user(&self, user: UserId) -> Vec<GuildId>;

    async fn can_view_channel(&self, user: UserId, guild: GuildId, channel: ChannelId) -> bool;

    /// Body of `GUILD_CREATE` for `user`. Default is a bare stub.
    async fn guild_payload(&self, _user: UserId, guild: GuildId) -> Value {
        json!({ "id": guild })
    }

    /// Extra READY fields (`private_channels`, `relationships`, ...).
    async fn ready_extras(&self, _user: UserId) -> Value {
        json!({})
    }
}
