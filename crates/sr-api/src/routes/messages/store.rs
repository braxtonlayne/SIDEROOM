//! Persistence the message endpoints need beyond `sr_chat::repo::ChatRepository`:
//! history paging, nonces, pins listing, reactions and upload slots.
//!
//! `sr-chat`'s repository covers guilds, channels and message CRUD; reactions,
//! nonce lookup and attachment rows have no trait there yet, so they live here
//! (open question in `docs/open-questions.md`). A Postgres implementation is
//! still to be written; [`super::memory::MemoryBackend`] implements everything
//! in memory.

use async_trait::async_trait;
use serde_json::{Value, json};
use sr_chat::Message;
use sr_core::{AttachmentId, ChannelId, MessageId, UserId};
use std::collections::HashMap;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_emoji_paths() {
        let u = EmojiKey::parse("\u{1F44D}").unwrap();
        assert_eq!((u.id, u.key().as_str()), (None, "\u{1F44D}"));
        let c = EmojiKey::parse("party_blob:123456").unwrap();
        assert_eq!(c.id, Some(123_456));
        assert_eq!(c.json()["id"], "123456");
        assert_eq!(EmojiKey::parse("a:wave:9").unwrap().name, "wave");
        assert!(EmojiKey::parse("smile").is_none());
        assert!(EmojiKey::parse("x:12").is_none());
        assert!(EmojiKey::parse("name:abc").is_none());
        assert!(EmojiKey::parse("").is_none());
    }
}
