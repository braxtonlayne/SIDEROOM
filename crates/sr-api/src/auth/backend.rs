//! Dyn-safe backends the extractors call. The bot and bearer traits and their
//! token formats live in `sr_auth::tokens` (open question 56); this module
//! keeps the session backend, which is blanket-implemented for every
//! [`sr_auth::repo::SessionRepository`].

use async_trait::async_trait;
use sr_auth::{
    repo::{SessionRecord, SessionRepository},
    session::TokenHash,
};

pub use sr_auth::tokens::{BackendError, BearerBackend, BearerIdentity, BotBackend, BotIdentity};

/// Session lookup used by the cookie extractor.
#[async_trait]
pub trait SessionBackend: Send + Sync {
    /// Find a session by token hash.
    async fn find(&self, hash: &TokenHash) -> Result<Option<SessionRecord>, BackendError>;
    /// Roll the expiry forward.
    async fn touch(&self, id: i64, last_seen_at: i64, expires_at: i64) -> Result<(), BackendError>;
}

#[async_trait]
impl<R: SessionRepository> SessionBackend for R {
    async fn find(&self, hash: &TokenHash) -> Result<Option<SessionRecord>, BackendError> {
        self.find_by_hash(hash)
            .await
            .map_err(|e| BackendError(e.to_string()))
    }
    async fn touch(&self, id: i64, last_seen_at: i64, expires_at: i64) -> Result<(), BackendError> {
        SessionRepository::touch(self, id, last_seen_at, expires_at)
            .await
            .map_err(|e| BackendError(e.to_string()))
    }
}
