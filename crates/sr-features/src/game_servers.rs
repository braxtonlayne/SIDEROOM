//! `platform.game_servers` (GSV-PAGE-001, D-383): the switch of the "Coming soon" page for game servers.
//!
//! The owner deferred the whole game-server feature. This feature is the one non-visual piece that ships: **on does
//! nothing but show a page** (Guild Settings > Game Servers and a small tile on the voice Activities picker, both
//! cosmetic mock-ups). Off hides the nav entry and the tile, and the route answers with the not-found view. There is
//! no setting, no route, no backend behind it.

use crate::feature::{Feature, FeatureMeta, Scope};
use crate::schema::SettingsSchema;

/// Stable feature id.
pub const GAME_SERVERS_ID: &str = "platform.game_servers";

#[derive(Default)]
pub struct GameServersFeature;

impl Feature for GameServersFeature {
    fn id(&self) -> &'static str {
        GAME_SERVERS_ID
    }
    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: "Game servers page".into(),
            description: "Only shows a page. Game servers are not built yet: while this is on, Server Settings has a Game Servers entry and the voice Activities picker has a tile, both marked \"Coming soon\", and nothing behind them works. Turn it off to hide both; the page address then answers not found.".into(),
            section: "Games".into(),
            phase: 39,
            default_enabled: true,
        }
    }
    /// The web hides the entry, the tile and the route when the platform switch is off.
    fn client_visible(&self) -> bool {
        true
    }
    fn settings_schema(&self) -> SettingsSchema {
        SettingsSchema::default()
    }
    fn scopes(&self) -> &[Scope] {
        &[Scope::Platform]
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{FeatureRegistry, FeatureService, InMemorySettingsStore, ScopeRef};

    #[tokio::test]
    async fn the_page_is_on_by_default_and_the_switch_reaches_clients() {
        let mut registry = FeatureRegistry::new();
        registry.register(GameServersFeature).unwrap();
        let svc = FeatureService::new(
            Arc::new(registry),
            Arc::new(InMemorySettingsStore::new()),
            None,
        );
        let f = GameServersFeature;
        assert!(f.meta().default_enabled);
        assert_eq!(f.meta().section, "Games");
        assert!(f.meta().description.starts_with("Only shows a page"));
        assert!(f.client_visible());
        assert_eq!(f.scopes(), &[Scope::Platform]);
        assert!(f.settings_schema().fields.is_empty());
        assert!(
            svc.is_enabled(GAME_SERVERS_ID, &ScopeRef::Platform)
                .await
                .unwrap()
        );
    }
}
