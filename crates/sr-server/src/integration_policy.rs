//! The non-secret integration policy an administrator saved on the Email & Integrations page
//! (`platform.integrations`, D-254, ADM-PLT-005). The config file holds the bootstrap value and a
//! saved value wins over it. The policy applies live (D-300, M-07): it is the value inside a
//! [`Live`] handle that link previews, the image proxy, the GIF picker and the metadata lookup
//! read on every request, and a subscriber to the settings event rebuilds it when the saved row
//! changes (in-process, one node). Clients, keys and endpoints stay startup-only (D-254).

use std::sync::Arc;

use futures::StreamExt;
use sr_api::routes::gifs::GifRegion;
use sr_core::config::Config;
use sr_events::{DomainEvent, EventBus, Topic};
use sr_features::{FeatureService, INTEGRATIONS_ID, IntegrationsPolicy, ScopeRef};
use sr_net::{HostLists, Live};
use sr_unfurl::Unfurler;

/// The effective policy: the config file overlaid with the saved row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationPolicy {
    pub deny_hosts: Vec<String>,
    pub allow_hosts: Vec<String>,
    pub per_host_per_minute: u32,
    pub gif_country: String,
    pub gif_locale: String,
    /// Lookup provider ids that are offered (a keyed one also needs its key at startup).
    pub lookup_enabled: Vec<String>,
}

impl IntegrationPolicy {
    /// Read the effective values out of an already-overlaid config.
    pub fn from_config(c: &Config) -> Self {
        let mut lookup_enabled = Vec::new();
        for (id, on) in [
            ("anilist", c.lookup.anilist),
            ("mangadex", c.lookup.mangadex),
            ("mangaupdates", c.lookup.mangaupdates),
            ("tmdb", !c.lookup.tmdb_api_key.expose().is_empty()),
            ("comicvine", !c.lookup.comicvine_api_key.expose().is_empty()),
        ] {
            if on {
                lookup_enabled.push(id.to_owned());
            }
        }
        Self {
            deny_hosts: c.unfurl.deny_hosts.clone(),
            allow_hosts: c.unfurl.allow_hosts.clone(),
            per_host_per_minute: c.unfurl.per_host_per_minute.max(1),
            gif_country: c.gif.country.clone(),
            gif_locale: c.gif.locale.clone(),
            lookup_enabled,
        }
    }

    /// The effective policy for `base` plus a saved row.
    pub fn resolve(base: &Config, saved: &IntegrationsPolicy) -> Self {
        let mut c = base.clone();
        apply(&mut c, saved);
        Self::from_config(&c)
    }
}

/// The live handles the consumers hold. `publish` pushes a policy into all of them.
#[derive(Clone)]
pub struct LivePolicy {
    pub policy: Live<IntegrationPolicy>,
    pub hosts: Live<HostLists>,
    pub gif: Live<GifRegion>,
    pub lookup: Live<Vec<String>>,
    unfurler: Arc<std::sync::OnceLock<Arc<Unfurler>>>,
}

impl LivePolicy {
    pub fn new(p: IntegrationPolicy) -> Self {
        Self {
            hosts: Live::new(HostLists {
                deny_hosts: p.deny_hosts.clone(),
                allow_hosts: p.allow_hosts.clone(),
            }),
            gif: Live::new(GifRegion {
                country: p.gif_country.clone(),
                locale: p.gif_locale.clone(),
            }),
            lookup: Live::new(p.lookup_enabled.clone()),
            policy: Live::new(p),
            unfurler: Arc::default(),
        }
    }

    /// Register the unfurler whose per-host budget follows the policy.
    pub fn attach_unfurler(&self, u: Arc<Unfurler>) {
        let _ = self.unfurler.set(u);
    }

    /// Make `p` the policy everywhere.
    pub fn publish(&self, p: IntegrationPolicy) {
        self.hosts.store(HostLists {
            deny_hosts: p.deny_hosts.clone(),
            allow_hosts: p.allow_hosts.clone(),
        });
        self.gif.store(GifRegion {
            country: p.gif_country.clone(),
            locale: p.gif_locale.clone(),
        });
        self.lookup.store(p.lookup_enabled.clone());
        if let Some(u) = self.unfurler.get() {
            u.set_per_host_per_minute(p.per_host_per_minute);
        }
        self.policy.store(p);
    }
}

/// Rebuild the policy from `base` and the saved row, and publish it.
pub async fn refresh(live: &LivePolicy, base: &Config, features: &FeatureService) {
    match features
        .effective_settings(INTEGRATIONS_ID, &ScopeRef::Platform)
        .await
    {
        Ok(values) => live.publish(IntegrationPolicy::resolve(
            base,
            &IntegrationsPolicy::from_values(&values),
        )),
        Err(e) => {
            tracing::warn!(error = %e, "could not read the integrations policy; keeping the current one")
        }
    }
}

/// Subscribe to the settings event: when `platform.integrations` changes, rebuild and publish.
pub fn spawn_subscriber(
    live: LivePolicy,
    base: Config,
    features: FeatureService,
    bus: Arc<dyn EventBus>,
) -> tokio::task::JoinHandle<()> {
    let mut stream = bus.subscribe(Topic::Platform);
    tokio::spawn(async move {
        while let Some(ev) = stream.next().await {
            if matches!(&ev, DomainEvent::FeatureSettingsChanged { feature_id, .. } if feature_id == INTEGRATIONS_ID)
            {
                refresh(&live, &base, &features).await;
            }
        }
    })
}

/// Apply a saved policy to `cfg`. A policy that cannot be read leaves the config untouched.
pub fn apply(cfg: &mut Config, p: &IntegrationsPolicy) {
    if let Some(v) = &p.unfurl_deny_hosts {
        cfg.unfurl.deny_hosts = v.clone();
    }
    if let Some(v) = &p.unfurl_allow_hosts {
        cfg.unfurl.allow_hosts = v.clone();
    }
    if let Some(v) = p.unfurl_per_host_per_minute {
        cfg.unfurl.per_host_per_minute = v;
    }
    if let Some(v) = &p.gif_country {
        cfg.gif.country = v.to_uppercase();
    }
    if let Some(v) = &p.gif_locale {
        cfg.gif.locale = v.clone();
    }
    if let Some(list) = &p.lookup_providers {
        let on = |id: &str| list.iter().any(|p| p == id);
        cfg.lookup.anilist = on("anilist");
        cfg.lookup.mangadex = on("mangadex");
        cfg.lookup.mangaupdates = on("mangaupdates");
        // A keyed provider the administrator did not list is not offered, whatever its key says.
        if !on("tmdb") {
            cfg.lookup.tmdb_api_key = Default::default();
        }
        if !on("comicvine") {
            cfg.lookup.comicvine_api_key = Default::default();
        }
    }
}

/// Read the saved policy and apply it.
pub async fn overlay(cfg: &mut Config, features: &FeatureService) {
    match features
        .effective_settings(INTEGRATIONS_ID, &ScopeRef::Platform)
        .await
    {
        Ok(values) => apply(cfg, &IntegrationsPolicy::from_values(&values)),
        Err(e) => {
            tracing::warn!(error = %e, "could not read the integrations policy; using the config file")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn saving_the_policy_reaches_the_live_handles_through_the_settings_event() {
        use serde_json::json;
        use sr_events::InProcessBus;
        use sr_features::{FeatureRegistry, InMemorySettingsStore};
        let mut registry = FeatureRegistry::new();
        registry
            .register(sr_features::IntegrationsPolicyFeature)
            .unwrap();
        let bus: Arc<dyn EventBus> = Arc::new(InProcessBus::new());
        let features = FeatureService::new(
            Arc::new(registry),
            Arc::new(InMemorySettingsStore::new()),
            Some(bus.clone()),
        );
        let base = Config::default();
        let live = LivePolicy::new(IntegrationPolicy::resolve(
            &base,
            &IntegrationsPolicy::default(),
        ));
        let task = spawn_subscriber(live.clone(), base, features.clone(), bus);
        tokio::task::yield_now().await;
        assert!(live.hosts.load().deny_hosts.is_empty());
        let saved = json!({"unfurl": {"deny_hosts": ["evil.example"]}, "gif": {"country": "fr"}});
        features
            .set_settings(
                INTEGRATIONS_ID,
                &ScopeRef::Platform,
                saved.as_object().unwrap().clone(),
            )
            .await
            .unwrap();
        for _ in 0..100 {
            if live.hosts.load().deny_hosts == ["evil.example"] {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(live.hosts.load().deny_hosts, ["evil.example"]);
        assert_eq!(live.gif.load().country, "FR");
        task.abort();
    }

    #[test]
    fn a_published_policy_is_visible_to_every_reader() {
        let base = Config::default();
        let live = LivePolicy::new(IntegrationPolicy::resolve(
            &base,
            &IntegrationsPolicy::default(),
        ));
        let (hosts, gif, lookup) = (live.hosts.clone(), live.gif.clone(), live.lookup.clone());
        assert!(hosts.load().deny_hosts.is_empty());
        live.publish(IntegrationPolicy::resolve(
            &base,
            &IntegrationsPolicy {
                unfurl_deny_hosts: Some(vec!["evil.example".into()]),
                gif_country: Some("de".into()),
                lookup_providers: Some(vec!["anilist".into()]),
                ..IntegrationsPolicy::default()
            },
        ));
        assert_eq!(hosts.load().deny_hosts, ["evil.example"]);
        assert_eq!(gif.load().country, "DE");
        assert_eq!(*lookup.load(), ["anilist"]);
        assert_eq!(live.policy.load().deny_hosts, ["evil.example"]);
    }

    #[test]
    fn a_saved_value_wins_and_the_rest_keeps_the_config() {
        let mut cfg = Config::default();
        cfg.unfurl.deny_hosts = vec!["from-config.example".into()];
        cfg.unfurl.per_host_per_minute = 20;
        cfg.lookup.anilist = true;
        cfg.lookup.mangadex = true;
        apply(&mut cfg, &IntegrationsPolicy::default());
        assert_eq!(cfg.unfurl.deny_hosts, ["from-config.example"]);
        assert!(cfg.lookup.anilist && cfg.lookup.mangadex);
        apply(
            &mut cfg,
            &IntegrationsPolicy {
                unfurl_deny_hosts: Some(vec!["db.example".into()]),
                unfurl_per_host_per_minute: Some(7),
                gif_country: Some("de".into()),
                lookup_providers: Some(vec!["mangadex".into()]),
                ..IntegrationsPolicy::default()
            },
        );
        assert_eq!(cfg.unfurl.deny_hosts, ["db.example"]);
        assert_eq!(cfg.unfurl.per_host_per_minute, 7);
        assert_eq!(cfg.gif.country, "DE");
        assert_eq!(cfg.gif.locale, "en_US");
        assert!(!cfg.lookup.anilist && cfg.lookup.mangadex);
    }
}
