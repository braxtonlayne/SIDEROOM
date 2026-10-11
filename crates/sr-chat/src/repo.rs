//! Repository traits. The domain logic depends only on these; Postgres
//! implementations live in `sr-db` (`chat_repo`).

use crate::domain::{Channel, Guild, Member, Message, Role};
use crate::error::RepoError;
use crate::guild_rules::GuildBlueprint;
use crate::message_rules::{DeletePlan, EditPlan};
use async_trait::async_trait;
use serde_json::{Value, json};
use sr_core::permissions::Overwrite;
use sr_core::{AttachmentId, ChannelId, GuildId, MessageId, RoleId, UserId};
use std::collections::HashMap;

pub type RepoResult<T> = Result<T, RepoError>;

#[async_trait]
pub trait GuildRepository: Send + Sync {
    /// Insert guild, `@everyone`, owner membership and default channels atomically.
    async fn insert_guild(&self, blueprint: &GuildBlueprint) -> RepoResult<()>;
    async fn get_guild(&self, id: GuildId) -> RepoResult<Option<Guild>>;
    async fn get_member(&self, guild: GuildId, user: UserId) -> RepoResult<Option<Member>>;
    /// Members ordered by user id, strictly after `after`, at most `limit`. Used by
    /// the `@everyone`/role mention fan-out (open question 117).
    async fn page_members(
        &self,
        guild: GuildId,
        after: Option<UserId>,
        limit: usize,
    ) -> RepoResult<Vec<Member>>;
    /// All roles including `@everyone`.
    async fn list_roles(&self, guild: GuildId) -> RepoResult<Vec<Role>>;
    /// Insert a role, first shifting every role with `position >= role.position` up by one.
    async fn insert_role(&self, role: &Role) -> RepoResult<()>;
    async fn update_role(&self, role: &Role) -> RepoResult<()>;
    /// Delete the role and strip it from all members.
    async fn delete_role(&self, guild: GuildId, role: RoleId) -> RepoResult<()>;
}

#[async_trait]
pub trait ChannelRepository: Send + Sync {
    async fn get_channel(&self, id: ChannelId) -> RepoResult<Option<Channel>>;
    async fn list_guild_channels(&self, guild: GuildId) -> RepoResult<Vec<Channel>>;
    async fn get_overwrites(&self, channel: ChannelId) -> RepoResult<Vec<Overwrite>>;
    async fn insert_channel(&self, channel: &Channel, overwrites: &[Overwrite]) -> RepoResult<()>;
    async fn update_channel(&self, channel: &Channel) -> RepoResult<()>;
    /// Delete the channel; children of a category are orphaned, not deleted.
    async fn delete_channel(&self, id: ChannelId) -> RepoResult<()>;
    /// Whether `user` is a recipient of a DM or group DM.
    async fn is_recipient(&self, channel: ChannelId, user: UserId) -> RepoResult<bool>;
}

#[async_trait]
pub trait MessageRepository: Send + Sync {
    async fn get_message(&self, channel: ChannelId, id: MessageId) -> RepoResult<Option<Message>>;
    /// Messages of `channel` among `ids` (any visibility); missing ids are omitted.
    async fn get_messages(&self, channel: ChannelId, ids: &[MessageId])
    -> RepoResult<Vec<Message>>;
    /// Insert and bump `channels.last_message_id` in one transaction.
    async fn insert_message(&self, message: &Message) -> RepoResult<()>;
    /// Persist an edit and append a `message_edits` row when
    /// `plan.history_previous_content` is set.
    async fn apply_edit(&self, plan: &EditPlan) -> RepoResult<()>;
    async fn apply_delete(&self, plan: &DeletePlan) -> RepoResult<()>;
    /// Time the user last posted in the channel, for slowmode. System messages
    /// (types other than default and reply) do not count (open question 154).
    async fn last_sent_ms(&self, channel: ChannelId, user: UserId) -> RepoResult<Option<i64>>;
    async fn count_pins(&self, channel: ChannelId) -> RepoResult<usize>;
    async fn set_pinned(
        &self,
        channel: ChannelId,
        message: MessageId,
        pinned_by: UserId,
        pinned: bool,
        now_ms: i64,
    ) -> RepoResult<()>;
    /// Bulk-hide `author`'s visible messages in `channels` created at or after
    /// `since_ms`, as `user_deleted` (D-121). Returns the hidden ids per channel
    /// (open question 158). `purge_after_ms` is an absolute time; held messages keep
    /// `purge_after` unset (D-122).
    async fn hide_user_messages(
        &self,
        author: UserId,
        channels: &[ChannelId],
        since_ms: i64,
        now_ms: i64,
        purge_after_ms: i64,
    ) -> RepoResult<Vec<(ChannelId, Vec<MessageId>)>>;
}

/// Everything the service needs.
pub trait ChatRepository:
    GuildRepository + ChannelRepository + MessageRepository + UserLookup
{
}
impl<T: GuildRepository + ChannelRepository + MessageRepository + UserLookup> ChatRepository for T {}

/// Summary of a user account as embedded in guild and message objects (open
/// question 157).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserSummary {
    pub id: UserId,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_key: Option<String>,
}

/// Looks up the user fields that Discord embeds in member, ban and message
/// objects. Unknown ids are omitted from the result.
#[async_trait]
pub trait UserLookup: Send + Sync {
    async fn users(&self, ids: &[UserId]) -> RepoResult<Vec<UserSummary>>;
}

/// Failure inside a store implementation.
#[derive(Debug, Clone, thiserror::Error)]
#[error("message store error: {0}")]
pub struct StoreError(pub String);

pub type StoreResult<T> = Result<T, StoreError>;

/// A reaction emoji: unicode (`id == None`) or custom (`name:id`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EmojiKey {
    pub id: Option<u64>,
    pub name: String,
}

impl EmojiKey {
    /// Parse the `{emoji}` path segment (already percent-decoded): `name:id` for custom
    /// emoji (an animated `a:` prefix is tolerated), anything else is unicode.
    pub fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        if raw.is_empty() || raw.len() > 128 || raw.chars().any(char::is_control) {
            return None;
        }
        if let Some((name, id)) = raw.rsplit_once(':') {
            let name = name.strip_prefix("a:").unwrap_or(name);
            let id: u64 = id.parse().ok()?;
            let valid = (2..=32).contains(&name.len())
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            return (valid && id != 0).then(|| Self {
                id: Some(id),
                name: name.to_owned(),
            });
        }
        // A bare ASCII word such as `smile` is not an emoji; emoji and keycap sequences
        // all contain a non-ASCII char.
        let ok = !raw.is_ascii();
        ok.then(|| Self {
            id: None,
            name: raw.to_owned(),
        })
    }

    /// Stable string key (`name:id` or the unicode string).
    pub fn key(&self) -> String {
        match self.id {
            Some(id) => format!("{}:{id}", self.name),
            None => self.name.clone(),
        }
    }

    /// Discord's partial emoji object.
    pub fn json(&self) -> Value {
        json!({ "id": self.id.map(|i| i.to_string()), "name": self.name })
    }
}

/// A history page request; only `Visible` messages are returned.
#[derive(Debug, Clone, Copy)]
pub struct Page {
    /// Exclusive upper bound.
    pub before: Option<MessageId>,
    /// Exclusive lower bound.
    pub after: Option<MessageId>,
    pub limit: usize,
    /// Oldest first when true, newest first otherwise (the limit applies from that end).
    pub ascending: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReactionSummary {
    pub emoji: EmojiKey,
    pub count: u32,
    /// Whether the viewer reacted.
    pub me: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadState {
    /// Slot issued; the object may not exist yet.
    Pending,
    /// Object verified and content type sniffed.
    Ready,
    /// Referenced by a message.
    Attached,
}

/// An attachment row (an upload slot until a message references it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub id: AttachmentId,
    pub channel_id: ChannelId,
    pub uploader: UserId,
    pub filename: String,
    pub size: u64,
    /// Object storage key.
    pub key: String,
    /// What the client claimed; never trusted.
    pub declared_content_type: Option<String>,
    /// Sniffed at completion.
    pub content_type: Option<String>,
    pub description: Option<String>,
    pub spoiler: bool,
    pub state: UploadState,
    pub message_id: Option<MessageId>,
    /// Large files (above the standard limit) expire; `None` lives with the message.
    pub expires_at_ms: Option<i64>,
    pub created_at_ms: i64,
}

#[async_trait]
pub trait MessageStore: Send + Sync {
    /// Visible messages of a channel matching the page request.
    async fn fetch(&self, channel: ChannelId, page: Page) -> StoreResult<Vec<Message>>;
    /// Pinned visible messages with pin time (Unix ms), newest pin first.
    async fn list_pins(&self, channel: ChannelId) -> StoreResult<Vec<(Message, i64)>>;

    /// Message previously created by `author` with `nonce` at or after `since_ms`.
    async fn find_nonce(
        &self,
        channel: ChannelId,
        author: UserId,
        nonce: &str,
        since_ms: i64,
    ) -> StoreResult<Option<MessageId>>;
    async fn put_nonce(
        &self,
        channel: ChannelId,
        author: UserId,
        nonce: &str,
        message: MessageId,
        now_ms: i64,
    ) -> StoreResult<()>;
    /// The nonce a message was created with (echoed in responses to its author's
    /// gateway payload, as Discord does).
    async fn nonce_of(&self, message: MessageId) -> StoreResult<Option<String>>;

    /// Returns true when the reaction is new.
    async fn add_reaction(
        &self,
        channel: ChannelId,
        message: MessageId,
        user: UserId,
        emoji: &EmojiKey,
        now_ms: i64,
    ) -> StoreResult<bool>;
    /// Returns true when a reaction was removed.
    async fn remove_reaction(
        &self,
        message: MessageId,
        user: UserId,
        emoji: &EmojiKey,
    ) -> StoreResult<bool>;
    async fn clear_reactions(&self, message: MessageId) -> StoreResult<()>;
    /// Returns true when the emoji had reactions.
    async fn clear_emoji(&self, message: MessageId, emoji: &EmojiKey) -> StoreResult<bool>;
    async fn reactions_for(
        &self,
        messages: &[MessageId],
        viewer: UserId,
    ) -> StoreResult<HashMap<MessageId, Vec<ReactionSummary>>>;
    /// Users who reacted with `emoji`, ascending by id, after `after`.
    async fn reaction_users(
        &self,
        message: MessageId,
        emoji: &EmojiKey,
        after: Option<UserId>,
        limit: usize,
    ) -> StoreResult<Vec<UserId>>;

    async fn insert_attachment(&self, attachment: &Attachment) -> StoreResult<()>;
    async fn get_attachments(&self, ids: &[AttachmentId]) -> StoreResult<Vec<Attachment>>;
    async fn update_attachment(&self, attachment: &Attachment) -> StoreResult<()>;
    /// Attachments belonging to messages (any state `Attached`).
    async fn attachments_of(
        &self,
        messages: &[MessageId],
    ) -> StoreResult<HashMap<MessageId, Vec<Attachment>>>;
}

impl From<RepoError> for StoreError {
    fn from(e: RepoError) -> Self {
        Self(e.0)
    }
}
