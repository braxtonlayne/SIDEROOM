//! In-memory stub backends for tests and local bring-up before `sr-db`
//! implements the real repositories.

use super::backend::{BackendError, BearerBackend, BearerIdentity, BotBackend, BotIdentity};
use async_trait::async_trait;
use sr_auth::{
    repo::{SessionRecord, SessionRepository},
    session::TokenHash,
};
use std::{collections::HashMap, convert::Infallible, sync::Mutex};

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// In-memory [`SessionRepository`].
#[derive(Debug, Default)]
pub struct MemorySessions(Mutex<HashMap<String, SessionRecord>>);

impl MemorySessions {
    /// Empty store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl SessionRepository for MemorySessions {
    type Error = Infallible;

    async fn create(&self, record: SessionRecord) -> Result<(), Infallible> {
        lock(&self.0).insert(record.token_hash.to_hex(), record);
        Ok(())
    }
    async fn find_by_hash(&self, hash: &TokenHash) -> Result<Option<SessionRecord>, Infallible> {
        Ok(lock(&self.0).get(&hash.to_hex()).cloned())
    }
    async fn touch(&self, id: i64, last_seen_at: i64, expires_at: i64) -> Result<(), Infallible> {
        if let Some(r) = lock(&self.0).values_mut().find(|r| r.id == id) {
            r.last_seen_at = last_seen_at;
            r.expires_at = expires_at;
        }
        Ok(())
    }
    async fn list_for_user(
        &self,
        user_id: i64,
        now: i64,
    ) -> Result<Vec<SessionRecord>, Infallible> {
        Ok(lock(&self.0)
            .values()
            .filter(|r| r.user_id == user_id && r.expires_at > now)
            .cloned()
            .collect())
    }
    async fn revoke(&self, user_id: i64, session_id: i64) -> Result<bool, Infallible> {
        let mut g = lock(&self.0);
        let before = g.len();
        g.retain(|_, r| !(r.user_id == user_id && r.id == session_id));
        Ok(g.len() != before)
    }
    async fn revoke_all_except(&self, user_id: i64, keep: Option<i64>) -> Result<u64, Infallible> {
        let mut g = lock(&self.0);
        let before = g.len();
        g.retain(|_, r| r.user_id != user_id || Some(r.id) == keep);
        Ok((before - g.len()) as u64)
    }
}

/// In-memory bot tokens (raw token to identity).
#[derive(Debug, Default)]
pub struct MemoryBots(Mutex<HashMap<String, BotIdentity>>);

impl MemoryBots {
    /// Empty store.
    pub fn new() -> Self {
        Self::default()
    }
    /// Register a token.
    pub fn insert(&self, token: impl Into<String>, id: BotIdentity) {
        lock(&self.0).insert(token.into(), id);
    }
}

#[async_trait]
impl BotBackend for MemoryBots {
    async fn authenticate(&self, token: &str) -> Result<Option<BotIdentity>, BackendError> {
        Ok(lock(&self.0).get(token).cloned())
    }
}

/// In-memory OAuth2 bearer tokens.
#[derive(Debug, Default)]
pub struct MemoryBearers(Mutex<HashMap<String, BearerIdentity>>);

impl MemoryBearers {
    /// Empty store.
    pub fn new() -> Self {
        Self::default()
    }
    /// Register a token.
    pub fn insert(&self, token: impl Into<String>, id: BearerIdentity) {
        lock(&self.0).insert(token.into(), id);
    }
}

#[async_trait]
impl BearerBackend for MemoryBearers {
    async fn authenticate(&self, token: &str) -> Result<Option<BearerIdentity>, BackendError> {
        Ok(lock(&self.0).get(token).cloned())
    }
}
