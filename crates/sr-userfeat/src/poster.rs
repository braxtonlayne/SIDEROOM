//! The port to the chat stack: the system user speaking in channels and DMs.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A message the system user posts: text, an optional embed and optional pings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Card {
    /// Plain content (may carry mentions such as `<@id>` or `<@&role>`).
    pub content: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub url: Option<String>,
    pub color: u32,
    pub thumbnail: Option<String>,
    pub fields: Vec<CardField>,
    pub footer: Option<String>,
    /// Role ids that may be pinged from `content`; empty pings no role.
    pub ping_roles: Vec<i64>,
    /// User ids that may be pinged from `content`; empty pings nobody.
    pub ping_users: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardField {
    pub name: String,
    pub value: String,
    pub inline: bool,
}

/// Posts as the system user. Failures are reported to the caller, which decides whether to retry.
#[async_trait]
pub trait Poster: Send + Sync {
    /// Post `card` into `channel`. Returns the message id.
    async fn post(&self, channel: i64, card: Card) -> Result<i64, String>;
    /// DM `user`. Returns `(dm channel id, message id)`.
    async fn dm(&self, user: i64, card: Card) -> Result<(i64, i64), String>;
    /// Send `content` into `channel` as `author`, with that user's permissions and the same checks as
    /// `POST /channels/{id}/messages` (scheduled messages). Returns the message id.
    async fn send_as(&self, author: i64, channel: i64, content: &str) -> Result<i64, SendFailure>;
    /// Add an emoji reaction from the system user (seeds suggestion votes). Best effort.
    async fn react(&self, channel: i64, message: i64, emoji: &str);
    /// Replace the card of a message the system user posted (suggestion status changes). Best effort.
    async fn edit(&self, channel: i64, message: i64, card: Card);
    /// Give `user` the role in the server. Best effort; `false` when it could not be done.
    async fn add_role(&self, guild: i64, user: i64, role: i64) -> bool;
    /// Take the role from `user`.
    async fn remove_role(&self, guild: i64, user: i64, role: i64) -> bool;
}

/// Why [`Poster::send_as`] did not deliver. `Permanent` stops the row; `Retry` leaves it for the next tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendFailure {
    Permanent(String),
    Retry(String),
}
