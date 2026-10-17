//! The non-secret integration policy an administrator saved on the Email & Integrations page
//! (`platform.integrations`, D-254, ADM-PLT-005). The config file holds the bootstrap value and a
//! saved value wins over it. The policy is read once at start, so a change applies from the next start.

use sr_core::config::Config;
use sr_features::{FeatureService, INTEGRATIONS_ID, IntegrationsPolicy, ScopeRef};

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
