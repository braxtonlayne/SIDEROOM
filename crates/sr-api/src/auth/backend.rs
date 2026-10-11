//! Dyn-safe backends the extractors call. Real implementations arrive with
//! `sr-db`; [`SessionBackend`] is blanket-implemented for every
//! [`sr_auth::repo::SessionRepository`], so the future Postgres repository
//! plugs in directly.

use async_trait::async_trait;
use sr_auth::{
    repo::{SessionRecord, SessionRepository},
    session::TokenHash,
};

/// Opaque backend failure (maps to a 500).
#[derive(Debug, thiserror::Error)]
#[error("auth backend error: {0}")]
pub struct BackendError(pub String);

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

/// A bot identified by its token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotIdentity {
    /// The bot's user id.
    pub user_id: i64,
    /// Owning application id.
    pub application_id: i64,
}

/// Bot token verification.
#[async_trait]
pub trait BotBackend: Send + Sync {
    /// `None` when the token is unknown or revoked.
    async fn authenticate(&self, token: &str) -> Result<Option<BotIdentity>, BackendError>;
}

/// An OAuth2 access token's owner and scopes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BearerIdentity {
    /// The authorising user.
    pub user_id: i64,
    /// Granted scopes (`identify`, `guilds`, ...).
    pub scopes: Vec<String>,
}

/// OAuth2 bearer token verification.
#[async_trait]
pub trait BearerBackend: Send + Sync {
    /// `None` when the token is unknown, expired or revoked.
    async fn authenticate(&self, token: &str) -> Result<Option<BearerIdentity>, BackendError>;
}
