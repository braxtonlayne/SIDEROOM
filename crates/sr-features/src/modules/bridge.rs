//! BRG-COR-001 The Discord bridge: its two switches (D-381).
//!
//! * `platform.bridge` is the operator's kill switch (platform scope, on by default). Switched off, the module is
//!   hidden everywhere, every bridge route answers 403 `SR_FEATURE_DISABLED`, and every server's bridge is torn
//!   down (`sr-bridge`); the module stays off when the feature returns.
//! * `bridge.enabled` is the server's module (guild scope, off by default, section "Integrations, more"). It is
//!   the one module that only the server **owner** may switch on or off ([`Feature::owner_only`]); the single
//!   feature-toggle handler reads that property, there is no second permission path. Once it is on,
//!   `MANAGE_GUILD` manages the mappings, runs the test and reads the status.
//!
//! The module has no settings of its own: what the owner configures (the bot token, the channel maps) lives in
//! the bridge tables, because a token is not a setting and must be sealed, never returned by the settings API.

use crate::feature::{Feature, FeatureMeta, Scope};
use crate::schema::SettingsSchema;

/// The server's module.
pub const BRIDGE_MODULE_ID: &str = "bridge.enabled";
/// The operator's kill switch.
pub const PLATFORM_BRIDGE_ID: &str = "platform.bridge";

#[derive(Default)]
pub struct BridgeModule;

impl Feature for BridgeModule {
    fn id(&self) -> &'static str {
        BRIDGE_MODULE_ID
    }

    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: "Discord bridge".into(),
            description: "Mirrors the channels you map to a Discord server you choose, with members' names, \
                          avatars and messages, and shows Discord users' messages here. Members can opt out \
                          per channel. Switching it off removes the webhooks from Discord and the mappings \
                          here. Only the server owner can switch it on or off."
                .into(),
            section: "Integrations, more".into(),
            phase: 35,
            default_enabled: false,
        }
    }

    fn owner_only(&self) -> bool {
        true
    }

    fn gated_by(&self) -> Option<&'static str> {
        Some(PLATFORM_BRIDGE_ID)
    }

    fn settings_schema(&self) -> SettingsSchema {
        SettingsSchema::new(Vec::new())
    }

    fn scopes(&self) -> &[Scope] {
        &[Scope::Guild]
    }
}

#[derive(Default)]
pub struct PlatformBridge;

impl Feature for PlatformBridge {
    fn id(&self) -> &'static str {
        PLATFORM_BRIDGE_ID
    }

    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: "Discord bridge".into(),
            description: "The operator's switch for the Discord bridge. Turning it off hides the module in every \
                          server, refuses every bridge route and removes every server's bridge from Discord \
                          (the webhooks it created) and from here; a server must switch it on again, paste its \
                          token and map its channels afresh."
                .into(),
            section: "Integrations".into(),
            phase: 35,
            default_enabled: true,
        }
    }

    /// The web hides the bridge screens when the operator switched it off.
    fn client_visible(&self) -> bool {
        true
    }

    fn settings_schema(&self) -> SettingsSchema {
        SettingsSchema::new(Vec::new())
    }

    fn scopes(&self) -> &[Scope] {
        &[Scope::Platform]
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
        reg.register(BridgeModule).unwrap();
        reg.register(PlatformBridge).unwrap();
        FeatureService::new(Arc::new(reg), Arc::new(InMemorySettingsStore::new()), None)
    }

    fn guild() -> ScopeRef {
        ScopeRef::Guild {
            guild: GuildId::new(7),
        }
    }

    #[tokio::test]
    async fn the_module_is_off_by_default_and_the_platform_switch_is_on() {
        let s = service();
        assert!(!s.is_enabled(BRIDGE_MODULE_ID, &guild()).await.unwrap());
        assert!(
            s.is_enabled(PLATFORM_BRIDGE_ID, &ScopeRef::Platform)
                .await
                .unwrap()
        );
        assert!(s.offered(BRIDGE_MODULE_ID).await.unwrap());
    }

    #[tokio::test]
    async fn the_module_is_owner_only_and_declares_its_parent() {
        assert!(BridgeModule.owner_only());
        assert_eq!(BridgeModule.gated_by(), Some(PLATFORM_BRIDGE_ID));
        assert!(!PlatformBridge.owner_only());
        assert_eq!(BridgeModule.meta().section, "Integrations, more");
        assert!(!BridgeModule.meta().default_enabled);
    }

    #[tokio::test]
    async fn the_kill_switch_hides_and_disables_the_module_and_it_stays_off_after() {
        let s = service();
        s.set_enabled(BRIDGE_MODULE_ID, &guild(), Some(true))
            .await
            .unwrap();
        assert!(s.is_enabled(BRIDGE_MODULE_ID, &guild()).await.unwrap());
        s.set_enabled(PLATFORM_BRIDGE_ID, &ScopeRef::Platform, Some(false))
            .await
            .unwrap();
        assert!(!s.is_enabled(BRIDGE_MODULE_ID, &guild()).await.unwrap());
        assert!(!s.offered(BRIDGE_MODULE_ID).await.unwrap());
        // The platform switch returns: the module is back to what the server chose. The teardown writes
        // an explicit `false` for each server, so for them it stays off.
        s.set_enabled(BRIDGE_MODULE_ID, &guild(), Some(false))
            .await
            .unwrap();
        s.set_enabled(PLATFORM_BRIDGE_ID, &ScopeRef::Platform, Some(true))
            .await
            .unwrap();
        assert!(!s.is_enabled(BRIDGE_MODULE_ID, &guild()).await.unwrap());
        assert!(s.offered(BRIDGE_MODULE_ID).await.unwrap());
    }
}
