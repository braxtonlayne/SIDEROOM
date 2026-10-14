//! Bot-token and OAuth2 bearer backends for the REST extractors.

use crate::service::AppsService;
use async_trait::async_trait;
use sr_auth::tokens::{BackendError, BearerBackend, BearerIdentity, BotBackend, BotIdentity};
use std::sync::Arc;

/// Authenticates `Authorization: Bot` and `Authorization: Bearer` against the stores.
#[derive(Clone)]
pub struct AppsAuth {
    svc: Arc<AppsService>,
}

impl AppsAuth {
    /// Wrap the service.
    pub fn new(svc: Arc<AppsService>) -> Self {
        Self { svc }
    }
}

fn backend(e: crate::AppsError) -> BackendError {
    BackendError(e.to_string())
}

#[async_trait]
impl BotBackend for AppsAuth {
    async fn authenticate(&self, token: &str) -> Result<Option<BotIdentity>, BackendError> {
        Ok(self
            .svc
            .authenticate_bot(token)
            .await
            .map_err(backend)?
            .map(|r| BotIdentity {
                user_id: r.user_id,
                application_id: r.application.id,
            }))
    }
}

#[async_trait]
impl BearerBackend for AppsAuth {
    async fn authenticate(&self, token: &str) -> Result<Option<BearerIdentity>, BackendError> {
        Ok(self
            .svc
            .bearer(token)
            .await
            .map_err(backend)?
            .map(|(t, _)| BearerIdentity {
                user_id: t.user_id,
                scopes: t.scopes,
                application_id: t.application_id,
                expires_at: t.access_expires_at_ms.div_euclid(1000),
            }))
    }
}
