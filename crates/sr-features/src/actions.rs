//! What a feature module is allowed to do to the rest of the platform.
//!
//! Modules live in `sr-features`, which sits below the chat and API crates, so they
//! never call those crates directly. They talk to this port; the composition root
//! (`sr-server`) implements it over the real services, and tests use
//! [`crate::testing::FakeActions`]. A failure is logged by the runtime and never
//! stops other modules.

use async_trait::async_trait;
use serde_json::Value;
use sr_events::{ChannelId, GuildId, UserId};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ActionError {
    #[error("not found")]
    NotFound,
    #[error("not allowed: {0}")]
    Forbidden(String),
    #[error("{0}")]
    Backend(String),
}

pub type ActionResult<T> = Result<T, ActionError>;

/// A message a module posts as the platform's system user.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OutMessage {
    pub content: String,
    /// One Discord-shaped embed object, if any.
    pub embed: Option<Value>,
    /// Users the message may ping. Nobody else is ever mentioned.
    pub mention_users: Vec<UserId>,
}

impl OutMessage {
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AttachmentInfo {
    pub url: String,
    pub filename: String,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MessageInfo {
    pub id: u64,
    pub channel: ChannelId,
    pub guild: Option<GuildId>,
    pub author: UserId,
    pub content: String,
    pub embeds: Vec<Value>,
    pub attachments: Vec<AttachmentInfo>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChannelInfo {
    pub id: ChannelId,
    pub guild: Option<GuildId>,
    /// Discord channel type code (0 text, 2 voice, 5 announcement, 10-12 threads).
    pub kind: i16,
    pub name: String,
    pub nsfw: bool,
    pub parent: Option<ChannelId>,
    pub rate_limit_per_user: u32,
    /// Whether every member of the guild (the `@everyone` role) can read the channel.
    pub everyone_can_view: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UserInfo {
    pub id: UserId,
    pub name: String,
    pub bot: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GuildInfo {
    pub id: GuildId,
    pub name: String,
    pub member_count: u64,
}

#[async_trait]
pub trait FeatureActions: Send + Sync {
    /// The user modules post as. Events authored by it are ignored by modules so
    /// they never react to their own output.
    fn self_user(&self) -> UserId;

    /// Public origin of the web app (no trailing slash), for jump links. Empty when
    /// unknown, which yields root-relative links.
    fn base_url(&self) -> String {
        String::new()
    }

    async fn post_message(&self, channel: ChannelId, msg: OutMessage) -> ActionResult<u64>;
    async fn edit_message(
        &self,
        channel: ChannelId,
        message: u64,
        content: String,
    ) -> ActionResult<()>;
    async fn delete_message(&self, channel: ChannelId, message: u64) -> ActionResult<()>;
    async fn send_dm(&self, user: UserId, msg: OutMessage) -> ActionResult<()>;

    async fn get_message(
        &self,
        channel: ChannelId,
        message: u64,
    ) -> ActionResult<Option<MessageInfo>>;
    async fn get_channel(&self, channel: ChannelId) -> ActionResult<Option<ChannelInfo>>;
    async fn get_user(&self, user: UserId) -> ActionResult<Option<UserInfo>>;
    async fn get_guild(&self, guild: GuildId) -> ActionResult<Option<GuildInfo>>;

    /// Distinct users who reacted with `emoji_key` (`name:id` or the unicode string),
    /// not counting `exclude`.
    async fn reaction_count(
        &self,
        channel: ChannelId,
        message: u64,
        emoji_key: &str,
        exclude: Option<UserId>,
    ) -> ActionResult<u32>;
    async fn remove_reaction(
        &self,
        channel: ChannelId,
        message: u64,
        emoji_key: &str,
        user: UserId,
    ) -> ActionResult<()>;

    async fn add_role(&self, guild: GuildId, user: UserId, role: u64) -> ActionResult<()>;
    async fn remove_role(&self, guild: GuildId, user: UserId, role: u64) -> ActionResult<()>;
    async fn has_role(&self, guild: GuildId, user: UserId, role: u64) -> ActionResult<bool>;

    /// Start a public thread on `message` and return its channel id.
    async fn create_thread(
        &self,
        channel: ChannelId,
        message: u64,
        name: &str,
    ) -> ActionResult<ChannelId>;
    async fn set_slowmode(&self, channel: ChannelId, seconds: u32) -> ActionResult<()>;
    async fn set_channel_name(&self, channel: ChannelId, name: &str) -> ActionResult<()>;
}
