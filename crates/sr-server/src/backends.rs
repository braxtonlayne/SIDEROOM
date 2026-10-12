//! Bot-token and OAuth-bearer backends.
//!
//! No bot or OAuth token tables exist yet (doc 11 schema is unwritten), so
//! these reject every credential. They are explicit implementations so a
//! guessed `Bot ...` header can never authenticate by accident; replace them
//! when the application tables land (docs/open-questions.md). Session cookies
//! are served by `sr_db::auth_repo::PgAuthRepo`, which `sr-api` accepts
//! directly through its blanket `SessionBackend` impl.

use async_trait::async_trait;
use sr_api::auth::{BackendError, BearerBackend, BearerIdentity, BotBackend, BotIdentity};

/// Rejects every bot token.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoBots;

#[async_trait]
impl BotBackend for NoBots {
    async fn authenticate(&self, _token: &str) -> Result<Option<BotIdentity>, BackendError> {
        Ok(None)
    }
}

/// Rejects every OAuth2 bearer token.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoBearers;

#[async_trait]
impl BearerBackend for NoBearers {
    async fn authenticate(&self, _token: &str) -> Result<Option<BearerIdentity>, BackendError> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_everything() {
        assert!(NoBots.authenticate("x").await.unwrap().is_none());
        assert!(NoBearers.authenticate("x").await.unwrap().is_none());
    }
}
