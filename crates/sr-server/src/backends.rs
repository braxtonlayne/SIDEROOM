//! Bot-token and OAuth2 bearer backends.
//!
//! `NoBots` and `NoBearers` rejected every credential until the application tables existed
//! (open questions 56 and 102). They are gone: [`AppsAuth`] authenticates `Authorization: Bot`
//! against `bot_tokens` and `Authorization: Bearer` against `oauth2_tokens`, both by digest
//! (docs/specs/developer-platform.md). Session cookies are served by
//! `sr_db::auth_repo::PgAuthRepo`, which `sr-api` accepts directly through its blanket
//! `SessionBackend` impl.

pub use sr_apps::auth::AppsAuth;

#[cfg(test)]
mod tests {
    use super::*;
    use sr_api::auth::{BearerBackend, BotBackend};
    use sr_apps::{AppsService, memory::MemoryAppsRepo};
    use std::sync::Arc;

    fn auth() -> AppsAuth {
        AppsAuth::new(Arc::new(AppsService::new(
            Arc::new(MemoryAppsRepo::new()),
            Arc::new(sr_core::SystemClock),
            Arc::new(|| 1),
        )))
    }

    /// With empty stores nothing authenticates, and a guessed header never does.
    #[tokio::test]
    async fn rejects_unknown_credentials() {
        let a = auth();
        assert!(BotBackend::authenticate(&a, "x").await.unwrap().is_none());
        assert!(
            BotBackend::authenticate(&a, "a.b.c")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            BearerBackend::authenticate(&a, "x")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            BearerBackend::authenticate(&a, &"A".repeat(43))
                .await
                .unwrap()
                .is_none()
        );
    }
}
