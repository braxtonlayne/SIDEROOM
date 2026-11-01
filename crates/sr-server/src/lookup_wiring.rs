//! Wiring of the metadata lookup (REV-ED-005): which providers the operator turned on in
//! `[lookup]`, and the platform setting `review.lookup.enabled` that gates them at run time.

use std::sync::Arc;

use async_trait::async_trait;
use sqlx::PgPool;
use sr_api::routes::review::{
    AniList, ComicVine, LookupProvider, LookupSwitch, MangaDex, MangaUpdates, Tmdb,
};
use sr_core::config::LookupConfig;
use sr_features::{EXTERNAL_METADATA_ID, FeatureService, ScopeRef};

/// The platform setting (Admin > Limits, group `library`).
pub const LOOKUP_SETTING: &str = "review.lookup.enabled";

/// The providers `[lookup]` enables. Nothing configured, nothing returned.
pub fn providers_from(cfg: &LookupConfig) -> Vec<Arc<dyn LookupProvider>> {
    let mut out: Vec<Arc<dyn LookupProvider>> = Vec::new();
    if cfg.anilist {
        out.push(Arc::new(AniList::default()));
    }
    if cfg.mangadex {
        out.push(Arc::new(MangaDex::default()));
    }
    if cfg.mangaupdates {
        out.push(Arc::new(MangaUpdates::default()));
    }
    if !cfg.tmdb_api_key.expose().is_empty() {
        out.push(Arc::new(Tmdb::new(cfg.tmdb_api_key.expose())));
    }
    if !cfg.comicvine_api_key.expose().is_empty() {
        out.push(Arc::new(ComicVine::new(cfg.comicvine_api_key.expose())));
    }
    out
}

/// Every provider that could be offered: the keyless ones always, a keyed one when its key is in
/// the config. Which of them are offered is the live `platform.integrations` policy (D-300).
pub fn all_providers_from(cfg: &LookupConfig) -> Vec<Arc<dyn LookupProvider>> {
    providers_from(&LookupConfig {
        anilist: true,
        mangadex: true,
        mangaupdates: true,
        ..cfg.clone()
    })
}

/// Reads `review.lookup.enabled` from `platform_settings`; off when absent or unreadable.
pub struct PgLookupSwitch {
    pool: PgPool,
}

impl PgLookupSwitch {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LookupSwitch for PgLookupSwitch {
    async fn enabled(&self) -> bool {
        sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT value FROM platform_settings WHERE key = $1",
        )
        .bind(LOOKUP_SETTING)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    }
}

/// STU-NAV-001: the platform switch `platform.external_metadata` (off by default) in front of the lookup setting.
/// While it is off no provider is offered, so nothing on the server calls TMDB or any other metadata host.
pub struct GatedLookupSwitch {
    inner: Arc<dyn LookupSwitch>,
    features: FeatureService,
}

impl GatedLookupSwitch {
    pub fn new(inner: Arc<dyn LookupSwitch>, features: FeatureService) -> Self {
        Self { inner, features }
    }
}

#[async_trait]
impl LookupSwitch for GatedLookupSwitch {
    async fn enabled(&self) -> bool {
        let on = self
            .features
            .is_enabled(EXTERNAL_METADATA_ID, &ScopeRef::Platform)
            .await
            .unwrap_or(false);
        on && self.inner.enabled().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_features::{FeatureRegistry, InMemorySettingsStore, wave_modules::register_all};

    #[test]
    fn nothing_is_on_by_default() {
        assert!(providers_from(&LookupConfig::default()).is_empty());
    }

    #[test]
    fn keyed_providers_need_their_key_and_keyless_ones_need_the_switch() {
        let mut cfg = LookupConfig {
            anilist: true,
            ..LookupConfig::default()
        };
        let ids = |c: &LookupConfig| providers_from(c).iter().map(|p| p.id()).collect::<Vec<_>>();
        assert_eq!(ids(&cfg), vec!["anilist"]);
        cfg.tmdb_api_key = serde_json::from_str("\"k\"").unwrap();
        assert_eq!(ids(&cfg), vec!["anilist", "tmdb"]);
    }

    struct AlwaysOn;

    #[async_trait]
    impl LookupSwitch for AlwaysOn {
        async fn enabled(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn with_default_settings_no_provider_is_offered() {
        let mut reg = FeatureRegistry::new();
        register_all(&mut reg).unwrap();
        let features =
            FeatureService::new(Arc::new(reg), Arc::new(InMemorySettingsStore::new()), None);
        // Even with the setting on and every provider configured, the platform switch is off by default.
        let gate = GatedLookupSwitch::new(Arc::new(AlwaysOn), features.clone());
        assert!(!gate.enabled().await, "off by default");
        features
            .set_enabled(EXTERNAL_METADATA_ID, &ScopeRef::Platform, Some(true))
            .await
            .unwrap();
        assert!(gate.enabled().await, "an operator can switch it on");
    }
}
