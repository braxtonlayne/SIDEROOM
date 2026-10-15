//! Storage for the rich message features: link-preview embeds, poll votes and
//! stickers (LNG-MSG-010, LNG-MSG-014, LNG-EXP). Kept apart from
//! [`crate::repo::MessageStore`] so the two evolve independently; the Postgres
//! implementation is `sr_db::rich_repo::PgRichStore`, the in-memory one lives in
//! [`crate::memory`].

use crate::message::Embed;
use crate::repo::StoreResult;
use async_trait::async_trait;
use sr_core::{ChannelId, GuildId, MessageId, UserId};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Vote counts of one poll, and which answers the viewer picked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PollTally {
    pub counts: BTreeMap<u32, u32>,
    pub mine: BTreeSet<u32>,
}

impl PollTally {
    pub fn count(&self, answer: u32) -> u32 {
        self.counts.get(&answer).copied().unwrap_or(0)
    }
}

/// Outcome of casting a vote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoteResult {
    /// Recorded. `replaced` lists the answers dropped because the poll is single choice.
    Added {
        replaced: Vec<u32>,
    },
    AlreadyVoted,
    /// The poll no longer accepts votes (expired or finalised).
    Closed,
    UnknownAnswer,
    /// The message has no poll.
    NoPoll,
}

/// Outcome of withdrawing a vote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetractResult {
    Removed,
    NotVoted,
    Closed,
    NoPoll,
}

/// A sticker row as the message path needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StickerRecord {
    pub id: u64,
    /// `None` for a standard (platform) sticker, usable everywhere.
    pub guild_id: Option<GuildId>,
    pub name: String,
    /// Discord `format_type`: 1 png, 2 apng, 3 lottie, 4 gif.
    pub format_type: i16,
    pub available: bool,
}

/// A sticker of a pack (G-44).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackSticker {
    pub id: u64,
    pub name: String,
    pub description: Option<String>,
    pub tags: String,
    /// Discord `format_type`: 1 png, 2 apng, 3 lottie, 4 gif.
    pub format_type: i16,
    pub available: bool,
    /// Object key under `sticker/` (see `GET /stickers/{id}/url`).
    pub media_key: String,
}

/// A published sticker pack with its stickers in picker order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StickerPack {
    pub id: u64,
    pub name: String,
    pub description: Option<String>,
    pub cover_sticker_id: Option<u64>,
    pub stickers: Vec<PackSticker>,
}

/// How a webhook-posted message is shown (DEV-BOT-006): the webhook id replaces the author id
/// and the name and avatar can be overridden per execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookAuthor {
    pub webhook_id: u64,
    pub username: String,
    pub avatar_url: Option<String>,
}

#[async_trait]
pub trait RichStore: Send + Sync {
    /// Remember who a webhook message is shown as. The default does nothing, so a store
    /// without the table keeps rendering the webhook creator.
    async fn set_webhook_author(
        &self,
        _message: MessageId,
        _author: &WebhookAuthor,
    ) -> StoreResult<()> {
        Ok(())
    }

    /// Display overrides of webhook messages among `messages` (others are omitted).
    async fn webhook_authors(
        &self,
        _messages: &[MessageId],
    ) -> StoreResult<HashMap<MessageId, WebhookAuthor>> {
        Ok(HashMap::new())
    }

    /// Replace the embeds of a message (link previews arriving after the send).
    async fn set_embeds(&self, message: MessageId, embeds: &[Embed]) -> StoreResult<()>;

    /// Vote counts per poll message; `viewer` fills `mine`.
    async fn poll_tallies(
        &self,
        messages: &[MessageId],
        viewer: Option<UserId>,
    ) -> StoreResult<HashMap<MessageId, PollTally>>;

    /// Record a vote atomically with the open/closed check. A single-choice poll
    /// drops the user's earlier votes.
    async fn cast_vote(
        &self,
        message: MessageId,
        answer: u32,
        user: UserId,
        now_ms: i64,
    ) -> StoreResult<VoteResult>;

    async fn retract_vote(
        &self,
        message: MessageId,
        answer: u32,
        user: UserId,
        now_ms: i64,
    ) -> StoreResult<RetractResult>;

    /// Voters of one answer, ascending by user id, strictly after `after`.
    async fn poll_voters(
        &self,
        message: MessageId,
        answer: u32,
        after: Option<UserId>,
        limit: usize,
    ) -> StoreResult<Vec<UserId>>;

    /// Move the expiry of an open poll to `now_ms`. True when it changed.
    async fn expire_poll_now(&self, message: MessageId, now_ms: i64) -> StoreResult<bool>;

    /// Mark a poll whose expiry has passed as finalised. True when this call did it
    /// (so exactly one caller announces the result).
    async fn finalize_poll(&self, message: MessageId, now_ms: i64) -> StoreResult<bool>;

    /// Polls whose expiry has passed but that are not finalised yet.
    async fn due_polls(
        &self,
        now_ms: i64,
        limit: usize,
    ) -> StoreResult<Vec<(ChannelId, MessageId)>>;

    /// Sticker rows among `ids` (unknown ids are omitted).
    async fn stickers(&self, ids: &[u64]) -> StoreResult<Vec<StickerRecord>>;

    /// Published sticker packs, in picker order (G-44). Empty until a pack is installed.
    async fn sticker_packs(&self) -> StoreResult<Vec<StickerPack>>;

    /// The media key of a sticker that belongs to a published pack.
    async fn pack_sticker_media(&self, id: u64) -> StoreResult<Option<String>>;
}
