//! OPS-FLG-001: the audit of the feature registry. Every module and feature of waves 27 to 39 that has a switch is
//! registered, with the scopes it makes sense at and the default its wave gave it; the two screens that render the
//! registry (Admin > Features and Guild Settings > Modules) are fed from the same list, which this test also
//! writes as a fixture for their Vitest (`apps/web/src/admin/features/__fixtures__/registry.json`).
//!
//! The roadmap's ids and the shipped ids are the same except one: `lounge.status_rings` is `lounge.rings`
//! (a shipped id is never renamed; `MAPPED` below and the handoff say so).
//!
//! `platform.game_servers` is registered by GSV-PAGE-001 (`sr_features::GAME_SERVERS_ID`);
//! `game_servers_switch_is_registered` pins its scope, default and client visibility.
//!
//! Refresh the fixture after adding a feature: `SR_UPDATE_FEATURE_CATALOG=1 cargo test -p sr-server --test
//! feature_registry`.

use serde_json::{Value, json};
use sr_features::{Feature, FeatureRegistry, Scope, TOOL_FLAGS};

use sr_features::GAME_SERVERS_ID;

/// The roadmap's name and the id that shipped.
const MAPPED: &[(&str, &str)] = &[("lounge.status_rings", "lounge.rings")];

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../apps/web/src/admin/features/__fixtures__/registry.json"
);

fn registry() -> FeatureRegistry {
    sr_server::app::feature_registry().expect("the registry builds")
}

fn scopes_of(f: &dyn Feature) -> Vec<Scope> {
    let mut s = f.scopes().to_vec();
    s.sort();
    s
}

const P: &[Scope] = &[Scope::Platform];
const PG: &[Scope] = &[Scope::Platform, Scope::Guild];
const G: &[Scope] = &[Scope::Guild];

#[test]
fn every_feature_of_waves_27_to_39_is_registered_with_its_scopes_and_default() {
    let reg = registry();
    // (id, scopes, default on, client visible)
    let expected: &[(&str, &[Scope], bool, bool)] = &[
        ("platform.byos", PG, true, true),
        ("lounge.scrapbook", PG, false, true),
        ("platform.public_profiles", P, false, false),
        ("platform.bridge", P, true, true),
        ("bridge.enabled", G, false, false),
        ("lounge.jukebox", PG, true, true),
        ("lounge.listening_rooms", PG, true, true),
        ("platform.tools", P, true, true),
        ("platform.activities_datachannel", P, true, true),
        ("platform.emulator", P, true, true),
        // The roadmap's `lounge.status_rings`.
        ("lounge.rings", P, false, false),
        ("platform.watch_together_embeds", P, true, true),
        // GSV-PAGE-001: shows or hides the cosmetic "Coming soon" page only.
        ("platform.game_servers", P, true, true),
    ];
    for (id, scopes, default_on, client) in expected {
        let f = reg
            .get(id)
            .unwrap_or_else(|| panic!("{id} is not registered"));
        let mut want = scopes.to_vec();
        want.sort();
        assert_eq!(scopes_of(f.as_ref()), want, "{id}: scopes");
        assert_eq!(f.meta().default_enabled, *default_on, "{id}: default");
        // Only the switches that hide or show a screen are client visible; the others are read server side.
        if *client {
            assert!(f.client_visible(), "{id}: client visible");
        }
        assert!(!f.meta().name.is_empty() && !f.meta().description.is_empty());
    }
    // The mapping is the only difference between the roadmap and the registry.
    for (roadmap, shipped) in MAPPED {
        assert!(
            reg.get(roadmap).is_none(),
            "{roadmap} must stay unregistered"
        );
        assert!(reg.get(shipped).is_some(), "{shipped}");
    }
}

#[test]
fn the_bridge_module_is_owner_only_off_by_default_and_dies_with_the_platform_switch() {
    let reg = registry();
    let m = reg.get("bridge.enabled").unwrap();
    assert!(m.owner_only());
    assert!(!m.meta().default_enabled);
    assert_eq!(m.gated_by(), Some("platform.bridge"));
    let s = reg.get("lounge.scrapbook").unwrap();
    assert!(
        s.operator_gated(),
        "the scrapbook stays hidden until the operator opts in"
    );
}

#[test]
fn every_tool_has_a_platform_sub_flag_under_platform_tools() {
    let reg = registry();
    assert_eq!(TOOL_FLAGS.len(), 31, "the drawer's tools but the guide");
    for t in TOOL_FLAGS {
        let f = reg
            .get(t.id)
            .unwrap_or_else(|| panic!("{} is not registered", t.id));
        assert_eq!(scopes_of(f.as_ref()), vec![Scope::Platform], "{}", t.id);
        assert!(f.meta().default_enabled, "{}", t.id);
        assert!(f.client_visible(), "{}", t.id);
        assert_eq!(f.gated_by(), Some("platform.tools"), "{}", t.id);
    }
    assert_eq!(
        reg.get("platform.tools").unwrap().gated_by(),
        None,
        "platform.tools is the parent"
    );
}

#[test]
fn the_listening_rooms_die_with_the_jukebox() {
    let reg = registry();
    assert_eq!(
        reg.get("lounge.listening_rooms").unwrap().gated_by(),
        Some("lounge.jukebox")
    );
}

#[test]
fn no_wave_switch_is_named_like_encryption_authorization_or_a_log() {
    // The IMMUTABLE RULE: nothing to switch off. (immutable_rules.rs pins the same against the whole catalog.)
    for f in registry().iter() {
        let id = f.id();
        for bad in [
            "e2ee",
            "encrypt",
            "audit",
            "access_log",
            "authz",
            "permission",
        ] {
            assert!(!id.contains(bad), "{id}");
        }
    }
}

#[test]
fn game_servers_switch_is_registered() {
    assert_eq!(GAME_SERVERS_ID, "platform.game_servers");
    let reg = registry();
    let f = reg
        .get(GAME_SERVERS_ID)
        .expect("platform.game_servers is registered by GSV-PAGE-001");
    assert_eq!(scopes_of(f.as_ref()), vec![Scope::Platform]);
    assert!(f.meta().default_enabled);
    assert!(f.client_visible());
}

/// The catalog as the two screens receive it, one object per registered feature.
fn catalog() -> Value {
    let reg = registry();
    let list: Vec<Value> = reg
        .iter()
        .map(|f| {
            let m = f.meta();
            let scopes: Vec<&str> = scopes_of(f.as_ref())
                .iter()
                .map(|s| match s {
                    Scope::Platform => "platform",
                    Scope::Guild => "guild",
                    Scope::Channel => "channel",
                    Scope::User => "user",
                })
                .collect();
            json!({
                "id": f.id(),
                "name": m.name,
                "description": m.description,
                "section": m.section,
                "phase": m.phase,
                "default_enabled": m.default_enabled,
                "scopes": scopes,
                "client_visible": f.client_visible(),
                "owner_only": f.owner_only(),
                "operator_gated": f.operator_gated(),
                "gated_by": f.gated_by(),
            })
        })
        .collect();
    Value::Array(list)
}

#[test]
fn the_web_fixture_is_the_registry() {
    let want = format!("{}\n", serde_json::to_string_pretty(&catalog()).unwrap());
    if std::env::var_os("SR_UPDATE_FEATURE_CATALOG").is_some() {
        std::fs::create_dir_all(std::path::Path::new(FIXTURE).parent().unwrap()).unwrap();
        std::fs::write(FIXTURE, &want).unwrap();
        return;
    }
    let have = std::fs::read_to_string(FIXTURE).unwrap_or_default();
    assert_eq!(
        have.replace("\r\n", "\n"),
        want,
        "apps/web/src/admin/features/__fixtures__/registry.json is stale: SR_UPDATE_FEATURE_CATALOG=1 cargo test -p sr-server --test feature_registry"
    );
}
