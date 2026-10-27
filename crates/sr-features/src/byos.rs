//! `platform.byos`: the switch and the server-level choices of bring-your-own-storage (BYOS-FLG-001).
//!
//! The platform flag is the kill switch. Off, every storage screen is hidden and every mount route
//! answers 403 `SR_FEATURE_DISABLED`, except reads of the instance bucket (mount 1), because the
//! beta's media lives there. The module has two server-level choices, both on by default and both
//! settable by the platform (a default for every server) and by a server:
//!
//! * `guild_mounts`: a server may connect buckets of its own.
//! * `member_shelves_in_guild`: members' own shelves show inside this server's Cinema and Library.
//!
//! A server cannot grant itself what the platform withheld: [`ByosSettings::combine`] ANDs the platform
//! value with the server's. Neither choice reaches end-to-end encryption, authorization or the audit
//! logs; those are not settings (the IMMUTABLE RULE).

use serde_json::{Map, Value, json};

use crate::feature::{Feature, FeatureMeta, Scope};
use crate::schema::{Field, FieldKind, SettingsSchema};

/// Stable id of the module.
pub const BYOS_ID: &str = "platform.byos";
/// Settings key: servers may connect buckets.
pub const GUILD_MOUNTS_KEY: &str = "guild_mounts";
/// Settings key: members' shelves show inside this server's sections.
pub const MEMBER_SHELVES_KEY: &str = "member_shelves_in_guild";

#[derive(Default)]
pub struct ByosFeature;

impl Feature for ByosFeature {
    fn id(&self) -> &'static str {
        BYOS_ID
    }
    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: "Bring your own storage".into(),
            description: "Members and servers connect their own S3-compatible buckets. Turning it off hides every storage screen and refuses every mount route; the instance bucket keeps serving the media already on it.".into(),
            section: "Storage".into(),
            phase: 28,
            default_enabled: true,
        }
    }
    /// The web hides the storage screens when the platform switch is off.
    fn client_visible(&self) -> bool {
        true
    }
    fn settings_schema(&self) -> SettingsSchema {
        SettingsSchema::new(vec![
            Field::new(GUILD_MOUNTS_KEY, "Servers may connect buckets", FieldKind::Bool)
                .default_value(json!(true))
                .help("When off, this server cannot connect a bucket of its own. A platform-level off applies to every server."),
            Field::new(
                MEMBER_SHELVES_KEY,
                "Show members' shelves in this server",
                FieldKind::Bool,
            )
            .default_value(json!(true))
            .help("Whether shelves that members share into this server appear in its Cinema and Library sections."),
        ])
    }
    fn scopes(&self) -> &[Scope] {
        &[Scope::Platform, Scope::Guild]
    }
}

/// The two server-level choices as the routes use them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByosSettings {
    pub guild_mounts: bool,
    pub member_shelves_in_guild: bool,
}

impl Default for ByosSettings {
    fn default() -> Self {
        Self {
            guild_mounts: true,
            member_shelves_in_guild: true,
        }
    }
}

impl ByosSettings {
    /// Read effective values; a missing or malformed value is the default (on).
    pub fn from_values(values: &Map<String, Value>) -> Self {
        let flag = |k: &str| values.get(k).and_then(Value::as_bool).unwrap_or(true);
        Self {
            guild_mounts: flag(GUILD_MOUNTS_KEY),
            member_shelves_in_guild: flag(MEMBER_SHELVES_KEY),
        }
    }

    /// The platform's choice and a server's: a server cannot switch on what the platform switched off.
    pub fn combine(platform: Self, guild: Self) -> Self {
        Self {
            guild_mounts: platform.guild_mounts && guild.guild_mounts,
            member_shelves_in_guild: platform.member_shelves_in_guild
                && guild.member_shelves_in_guild,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FeatureRegistry, FeatureService, InMemorySettingsStore, ScopeRef};
    use sr_events::GuildId;
    use std::sync::Arc;

    fn service() -> FeatureService {
        let mut reg = FeatureRegistry::new();
        reg.register(ByosFeature).unwrap();
        FeatureService::new(Arc::new(reg), Arc::new(InMemorySettingsStore::new()), None)
    }

    #[test]
    fn defaults_are_on_and_schema_validates() {
        let f = ByosFeature;
        let s = f.settings_schema();
        s.validate(&s.defaults(Scope::Platform)).unwrap();
        s.validate(&s.defaults(Scope::Guild)).unwrap();
        assert_eq!(
            ByosSettings::from_values(&Map::new()),
            ByosSettings::default()
        );
        assert!(f.meta().default_enabled);
        assert!(f.client_visible());
    }

    #[test]
    fn a_server_cannot_switch_on_what_the_platform_withheld() {
        let off = ByosSettings {
            guild_mounts: false,
            member_shelves_in_guild: true,
        };
        let on = ByosSettings::default();
        assert!(!ByosSettings::combine(off, on).guild_mounts);
        assert!(!ByosSettings::combine(on, off).guild_mounts);
        assert!(ByosSettings::combine(on, on).guild_mounts);
        assert!(ByosSettings::combine(off, on).member_shelves_in_guild);
    }

    #[tokio::test]
    async fn the_platform_switch_kills_every_server() {
        let svc = service();
        let guild = ScopeRef::Guild {
            guild: GuildId::new(7),
        };
        assert!(svc.is_enabled(BYOS_ID, &guild).await.unwrap());
        svc.set_enabled(BYOS_ID, &ScopeRef::Platform, Some(false))
            .await
            .unwrap();
        assert!(!svc.is_enabled(BYOS_ID, &guild).await.unwrap());
        assert!(svc.platform_killed(BYOS_ID).await.unwrap());
    }
}
