//! Persistence ports of the jukebox. A [`JukeboxTxn`] holds the channel's session row locked from `begin` to
//! `commit`, so two requests that both find a track over (or both vote the last needed vote) are serialised
//! and the track changes once.

use super::model::{Doc, JukeboxError};
use async_trait::async_trait;

/// One locked unit of work on a channel's jukebox.
#[async_trait]
pub trait JukeboxTxn: Send {
    /// The channel's rows.
    async fn load(&mut self) -> Result<Doc, JukeboxError>;
    /// Make the stored rows equal `doc` (the session, the queue, the votes and the listeners).
    async fn save(&mut self, doc: &Doc) -> Result<(), JukeboxError>;
    /// Release the lock and publish the writes.
    async fn commit(self: Box<Self>) -> Result<(), JukeboxError>;
}

/// Where the jukebox state lives.
#[async_trait]
pub trait JukeboxStore: Send + Sync {
    /// A read without a lock; `None` when the channel never used the jukebox.
    async fn peek(&self, channel: i64) -> Result<Option<Doc>, JukeboxError>;
    /// Create the session row when it is missing, lock it and open a unit of work.
    async fn begin(&self, channel: i64, guild: i64) -> Result<Box<dyn JukeboxTxn>, JukeboxError>;
}
