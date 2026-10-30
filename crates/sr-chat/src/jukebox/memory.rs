//! An in-memory [`JukeboxStore`] for tests of dependent crates. It does not lock: tests drive it one call at
//! a time (the Postgres store is where concurrency is proven).

use super::model::{Doc, JukeboxError};
use super::store::{JukeboxStore, JukeboxTxn};
use async_trait::async_trait;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

/// The shared map of channel docs.
#[derive(Clone, Default)]
pub struct MemoryJukebox {
    docs: Arc<Mutex<HashMap<i64, Doc>>>,
}

impl MemoryJukebox {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }
}

struct Txn {
    docs: Arc<Mutex<HashMap<i64, Doc>>>,
    channel: i64,
    guild: i64,
}

fn poisoned<T>(_: T) -> JukeboxError {
    JukeboxError::Store("memory store poisoned".into())
}

#[async_trait]
impl JukeboxTxn for Txn {
    async fn load(&mut self) -> Result<Doc, JukeboxError> {
        let g = self.docs.lock().map_err(poisoned)?;
        Ok(g.get(&self.channel)
            .cloned()
            .unwrap_or_else(|| Doc::new(self.channel, self.guild)))
    }

    async fn save(&mut self, doc: &Doc) -> Result<(), JukeboxError> {
        self.docs
            .lock()
            .map_err(poisoned)?
            .insert(self.channel, doc.clone());
        Ok(())
    }

    async fn commit(self: Box<Self>) -> Result<(), JukeboxError> {
        Ok(())
    }
}

#[async_trait]
impl JukeboxStore for MemoryJukebox {
    async fn peek(&self, channel: i64) -> Result<Option<Doc>, JukeboxError> {
        Ok(self.docs.lock().map_err(poisoned)?.get(&channel).cloned())
    }

    async fn begin(&self, channel: i64, guild: i64) -> Result<Box<dyn JukeboxTxn>, JukeboxError> {
        Ok(Box::new(Txn {
            docs: self.docs.clone(),
            channel,
            guild,
        }))
    }
}
