//! Platform defaults edited in Admin (D-254): the default theme of each section
//! (`platform.appearance`, ADM-PLT-004) and the non-secret integration policy
//! (`platform.integrations`, ADM-PLT-005). Both are platform-scope features whose settings the
//! registry stores; secrets and endpoints (SMTP, Discord OAuth, API keys) stay operator config.

use std::collections::BTreeMap;

use crate::feature::{Feature, FeatureMeta, Scope};
use crate::schema::{EnumOption, Field, FieldKind, SettingsSchema};
use serde_json::{Map, Value};

/// Stable id of the appearance defaults module.
pub const APPEARANCE_ID: &str = "platform.appearance";
/// Settings key: group of one theme id per section.
pub const DEFAULT_THEME_KEY: &str = "default_theme_by_section";
/// The sections a default can be set for, as the web names them (`tv` is the Live TV guide).
pub const THEME_SECTIONS: &[&str] = &["lounge", "entrance", "cinema", "library", "support", "tv"];
/// The shipped theme ids of doc 10 (`sr-theme` `BUILTIN_THEME_IDS`; approved theme packages join
/// the list when they exist).
pub const SHIPPED_THEMES: &[(&str, &str)] = &[
    ("lounge-dark", "Lounge Dark"),
    ("lounge-light", "Lounge Light"),
    ("lounge-ash", "Lounge Ash"),
    ("lounge-onyx", "Lounge Onyx"),
    ("sideroom", "Side Room"),
    ("sideroom-broadcast", "Side Room Broadcast"),
];

#[derive(Default)]
pub struct AppearanceDefaultsFeature;

impl Feature for AppearanceDefaultsFeature {
    fn id(&self) -> &'static str {
        APPEARANCE_ID
    }
    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: "Default themes".into(),
            description: "The theme each section starts with for a member who has not chosen one."
                .into(),
            section: "Appearance".into(),
            phase: 15,
            default_enabled: true,
        }
    }
    fn settings_schema(&self) -> SettingsSchema {
        let mut options = vec![EnumOption {
            value: String::new(),
            label: "Shipped default".into(),
        }];
        options.extend(SHIPPED_THEMES.iter().map(|(v, l)| EnumOption {
            value: (*v).into(),
            label: (*l).into(),
        }));
        let fields = THEME_SECTIONS
            .iter()
            .map(|s| {
                Field::new(
                    s,
                    &format!("{} theme", capitalised(s)),
                    FieldKind::Enum {
                        options: options.clone(),
                    },
                )
            })
            .collect();
        SettingsSchema::new(vec![
            Field::new(
                DEFAULT_THEME_KEY,
                "Default theme by section",
                FieldKind::Group { fields },
            )
            .help("A member's own choice always wins: own section theme, then own theme for everything, then this default, then the shipped one."),
        ])
    }
    fn scopes(&self) -> &[Scope] {
        &[Scope::Platform]
    }
}

fn capitalised(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// The platform's default theme per section from the effective settings: only sections with a known
/// theme id are returned (an empty choice or a stale id means "the shipped default").
pub fn default_themes_from(values: &Map<String, Value>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let Some(g) = values.get(DEFAULT_THEME_KEY).and_then(Value::as_object) {
        for s in THEME_SECTIONS {
            if let Some(t) = g.get(*s).and_then(Value::as_str)
                && SHIPPED_THEMES.iter().any(|(id, _)| id == &t)
            {
                out.insert((*s).to_owned(), t.to_owned());
            }
        }
    }
    out
}

/// Stable id of the integrations policy module.
pub const INTEGRATIONS_ID: &str = "platform.integrations";

/// The lookup providers an operator can offer (`[lookup]` config enables the keyed ones).
pub const LOOKUP_PROVIDERS: &[&str] = &["anilist", "mangadex", "mangaupdates", "tmdb", "comicvine"];

#[derive(Default)]
pub struct IntegrationsPolicyFeature;

impl Feature for IntegrationsPolicyFeature {
    fn id(&self) -> &'static str {
        INTEGRATIONS_ID
    }
    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: "Integrations policy".into(),
            description: "Link preview hosts and rate, GIF region and locale, and which lookup providers are offered. Secrets and endpoints stay in the operator config.".into(),
            section: "Email & Integrations".into(),
            phase: 15,
            default_enabled: true,
        }
    }
    fn settings_schema(&self) -> SettingsSchema {
        let host = || FieldKind::List {
            item: Box::new(FieldKind::String { max_len: Some(253) }),
            max_items: Some(200),
        };
        SettingsSchema::new(vec![
            Field::new(
                "unfurl",
                "Link previews",
                FieldKind::Group {
                    fields: vec![
                        Field::new("deny_hosts", "Never fetch these hosts", host())
                            .help("A host and its subdomains. The deny list always wins."),
                        Field::new("allow_hosts", "Fetch only these hosts", host())
                            .help("When not empty, previews and the image proxy fetch only these hosts and their subdomains."),
                        Field::new(
                            "per_host_per_minute",
                            "Requests per host per minute",
                            FieldKind::Int {
                                min: Some(1),
                                max: Some(600),
                            },
                        ),
                    ],
                },
            )
            .help("A saved value replaces the config file's [unfurl] value from the next start of the server."),
            Field::new(
                "gif",
                "GIF picker",
                FieldKind::Group {
                    fields: vec![
                        Field::new("country", "Region", FieldKind::String { max_len: Some(2) })
                            .help("ISO 3166-1 alpha-2, for example US."),
                        Field::new("locale", "Locale", FieldKind::String { max_len: Some(16) })
                            .help("For example en_US."),
                    ],
                },
            ),
            Field::new(
                "lookup",
                "Metadata lookup",
                FieldKind::Group {
                    fields: vec![Field::new(
                        "providers_enabled",
                        "Providers offered",
                        FieldKind::List {
                            item: Box::new(FieldKind::Enum {
                                options: LOOKUP_PROVIDERS
                                    .iter()
                                    .map(|p| EnumOption {
                                        value: (*p).into(),
                                        label: (*p).into(),
                                    })
                                    .collect(),
                            }),
                            max_items: Some(8),
                        },
                    )
                    .help("A provider that needs a key is offered only when its key is in the operator config as well.")],
                },
            ),
        ])
    }
    fn scopes(&self) -> &[Scope] {
        &[Scope::Platform]
    }
}

/// The stored policy; every field is `None` until an administrator saved it, and then it wins over the
/// config file's bootstrap value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IntegrationsPolicy {
    pub unfurl_deny_hosts: Option<Vec<String>>,
    pub unfurl_allow_hosts: Option<Vec<String>>,
    pub unfurl_per_host_per_minute: Option<u32>,
    pub gif_country: Option<String>,
    pub gif_locale: Option<String>,
    pub lookup_providers: Option<Vec<String>>,
}

fn strings(v: Option<&Value>) -> Option<Vec<String>> {
    v.and_then(Value::as_array).map(|a| {
        a.iter()
            .filter_map(Value::as_str)
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect()
    })
}

impl IntegrationsPolicy {
    pub fn from_values(values: &Map<String, Value>) -> Self {
        let group = |k: &str| values.get(k).and_then(Value::as_object);
        let unfurl = group("unfurl");
        let gif = group("gif");
        let lookup = group("lookup");
        let text = |g: Option<&Map<String, Value>>, k: &str| {
            g.and_then(|g| g.get(k))
                .and_then(Value::as_str)
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
        };
        Self {
            unfurl_deny_hosts: strings(unfurl.and_then(|g| g.get("deny_hosts"))),
            unfurl_allow_hosts: strings(unfurl.and_then(|g| g.get("allow_hosts"))),
            unfurl_per_host_per_minute: unfurl
                .and_then(|g| g.get("per_host_per_minute"))
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok()),
            gif_country: text(gif, "country"),
            gif_locale: text(gif, "locale"),
            lookup_providers: strings(lookup.and_then(|g| g.get("providers_enabled"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FeatureRegistry, FeatureService, InMemorySettingsStore, ScopeRef};
    use serde_json::json;
    use std::sync::Arc;

    fn service() -> FeatureService {
        let mut r = FeatureRegistry::new();
        r.register(AppearanceDefaultsFeature).unwrap();
        r.register(IntegrationsPolicyFeature).unwrap();
        FeatureService::new(Arc::new(r), Arc::new(InMemorySettingsStore::new()), None)
    }

    #[tokio::test]
    async fn default_themes_are_stored_and_unknown_ids_are_refused() {
        let svc = service();
        let none = svc
            .effective_settings(APPEARANCE_ID, &ScopeRef::Platform)
            .await
            .unwrap();
        assert!(default_themes_from(&none).is_empty());
        let v = json!({ "default_theme_by_section": { "cinema": "lounge-onyx", "library": "" } });
        svc.set_settings(
            APPEARANCE_ID,
            &ScopeRef::Platform,
            v.as_object().unwrap().clone(),
        )
        .await
        .unwrap();
        let got = svc
            .effective_settings(APPEARANCE_ID, &ScopeRef::Platform)
            .await
            .unwrap();
        let themes = default_themes_from(&got);
        assert_eq!(
            themes.get("cinema").map(String::as_str),
            Some("lounge-onyx")
        );
        assert!(!themes.contains_key("library"));
        let bad = json!({ "default_theme_by_section": { "cinema": "neon" } });
        assert!(
            svc.set_settings(
                APPEARANCE_ID,
                &ScopeRef::Platform,
                bad.as_object().unwrap().clone()
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn the_policy_is_absent_until_saved_and_then_wins() {
        let svc = service();
        let none = svc
            .effective_settings(INTEGRATIONS_ID, &ScopeRef::Platform)
            .await
            .unwrap();
        assert_eq!(
            IntegrationsPolicy::from_values(&none),
            IntegrationsPolicy::default()
        );
        let v = json!({
            "unfurl": { "deny_hosts": ["evil.example", " "], "per_host_per_minute": 5 },
            "gif": { "country": "DE" },
            "lookup": { "providers_enabled": ["anilist"] }
        });
        svc.set_settings(
            INTEGRATIONS_ID,
            &ScopeRef::Platform,
            v.as_object().unwrap().clone(),
        )
        .await
        .unwrap();
        let got = svc
            .effective_settings(INTEGRATIONS_ID, &ScopeRef::Platform)
            .await
            .unwrap();
        let p = IntegrationsPolicy::from_values(&got);
        assert_eq!(p.unfurl_deny_hosts, Some(vec!["evil.example".to_owned()]));
        assert_eq!(p.unfurl_allow_hosts, None);
        assert_eq!(p.unfurl_per_host_per_minute, Some(5));
        assert_eq!(p.gif_country.as_deref(), Some("DE"));
        assert_eq!(p.gif_locale, None);
        assert_eq!(p.lookup_providers, Some(vec!["anilist".to_owned()]));
        let bad = json!({ "unfurl": { "per_host_per_minute": 0 } });
        assert!(
            svc.set_settings(
                INTEGRATIONS_ID,
                &ScopeRef::Platform,
                bad.as_object().unwrap().clone()
            )
            .await
            .is_err()
        );
    }
}
