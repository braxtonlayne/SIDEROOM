//! Runtime state of feature modules (sticky message ids, starboard links,
//! temporary-role expiries). Settings hold what an admin configured; this holds
//! what the module itself learned. One keyed JSON row per (feature, guild, key)
//! with an optional due time that the module's tick queries.

use std::collections::BTreeMap;
use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::Value;
use sr_events::GuildId;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("feature state error: {0}")]
pub struct StateError(pub String);

pub type StateResult<T> = Result<T, StateError>;

#[derive(Debug, Clone, PartialEq)]
pub struct StateEntry {
    pub feature: String,
    pub guild: GuildId,
    pub key: String,
    pub value: Value,
    /// Unix ms at which the module wants to look at this row again.
    pub due_ms: Option<i64>,
}

impl StateEntry {
    pub fn new(feature: &str, guild: GuildId, key: impl Into<String>, value: Value) -> Self {
        Self {
            feature: feature.to_owned(),
            guild,
            key: key.into(),
            value,
            due_ms: None,
        }
    }

    pub fn due(mut self, due_ms: i64) -> Self {
        self.due_ms = Some(due_ms);
        self
    }
}

#[async_trait]
pub trait FeatureState: Send + Sync {
    async fn get(
        &self,
        feature: &str,
        guild: GuildId,
        key: &str,
    ) -> StateResult<Option<StateEntry>>;
    async fn put(&self, entry: StateEntry) -> StateResult<()>;
    async fn delete(&self, feature: &str, guild: GuildId, key: &str) -> StateResult<()>;
    /// Rows of `feature` whose due time has passed, oldest first.
    async fn due(&self, feature: &str, now_ms: i64, limit: usize) -> StateResult<Vec<StateEntry>>;
    /// Rows of one guild whose key starts with `prefix`.
    async fn list(
        &self,
        feature: &str,
        guild: GuildId,
        prefix: &str,
    ) -> StateResult<Vec<StateEntry>>;
}

#[derive(Debug, Default)]
pub struct InMemoryState {
    map: Mutex<BTreeMap<(String, u64, String), StateEntry>>,
}

impl InMemoryState {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl FeatureState for InMemoryState {
    async fn get(
        &self,
        feature: &str,
        guild: GuildId,
        key: &str,
    ) -> StateResult<Option<StateEntry>> {
        let m = self.map.lock().unwrap_or_else(|p| p.into_inner());
        Ok(m.get(&(feature.to_owned(), guild.get(), key.to_owned()))
            .cloned())
    }

    async fn put(&self, entry: StateEntry) -> StateResult<()> {
        let mut m = self.map.lock().unwrap_or_else(|p| p.into_inner());
        m.insert(
            (entry.feature.clone(), entry.guild.get(), entry.key.clone()),
            entry,
        );
        Ok(())
    }

    async fn delete(&self, feature: &str, guild: GuildId, key: &str) -> StateResult<()> {
        let mut m = self.map.lock().unwrap_or_else(|p| p.into_inner());
        m.remove(&(feature.to_owned(), guild.get(), key.to_owned()));
        Ok(())
    }

    async fn due(&self, feature: &str, now_ms: i64, limit: usize) -> StateResult<Vec<StateEntry>> {
        let m = self.map.lock().unwrap_or_else(|p| p.into_inner());
        let mut out: Vec<StateEntry> = m
            .values()
            .filter(|e| e.feature == feature && e.due_ms.is_some_and(|d| d <= now_ms))
            .cloned()
            .collect();
        out.sort_by_key(|e| e.due_ms);
        out.truncate(limit);
        Ok(out)
    }

    async fn list(
        &self,
        feature: &str,
        guild: GuildId,
        prefix: &str,
    ) -> StateResult<Vec<StateEntry>> {
        let m = self.map.lock().unwrap_or_else(|p| p.into_inner());
        Ok(m.values()
            .filter(|e| e.feature == feature && e.guild == guild && e.key.starts_with(prefix))
            .cloned()
            .collect())
    }
}
