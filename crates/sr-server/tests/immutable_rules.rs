//! OPS-IMM-002: the IMMUTABLE RULE harness (docs/dev/roadmap-p27.md section 2).
//!
//! End-to-end encryption of calls, per-route authorization and the tamper-evident audit and access logs are
//! not features, settings or flags. This file proves it on every wave, in three ways:
//!
//! (a) `refusals_and_logs_hold_in_every_flag_state` (Postgres, `#[ignore]` like the matrix, run by the ignored
//!     step of `rust.yml`): with every registered feature killed at platform scope, then with every setting at
//!     its minimum, then at its maximum, the refusal phase of the authorization matrix (moderation, admin and
//!     the rest group, which holds the storage and voice routes) still refuses, a staff read of a member
//!     archive still writes `staff_access_log`, an admin action still writes `platform_audit_log` and the
//!     access log, and `GET /admin/access-log/verify` still reports an intact chain. The matrix's own seed,
//!     cast and client are reused as a library (`#[path]` include, entry point `authz_matrix::harness`), not
//!     copied. The HTTP rate limits and the abuse limiters stay at their maximum in every state, because at
//!     their minimum every call answers 429 and nothing about authorization would be seen.
//! (b) `no_flag_name_matches_the_deny_pattern`: no feature id, setting key, schema field or config field
//!     contains `e2ee`, `encrypt`, `audit`, `access_log`, `authz`, `authorization` or `permission`, so a later
//!     agent cannot add a switch for the three. The few names that existed before the rule are pinned in
//!     `LEGACY` below and may only shrink.
//! (c) `no_description_claims_to_disable_logging_or_encryption`: no catalog description, label or help text
//!     says it turns off, skips or bypasses logging, auditing, authorization or encryption.
//!
//! Voice: a client or bot that did not complete the end-to-end handshake is refused at the join and never
//! forwarded by the SFU (`sr-gateway` and `sr-voice` tests), and `voice.e2ee` is gone from the config
//! (`e2ee = false` is an error, `sr-core` test); `no_flag_name_matches_the_deny_pattern` also pins that it
//! does not come back. Needs Postgres for (a):
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-server --test immutable_rules -- --include-ignored
//! --test-threads=1 --nocapture`.

// The matrix is included whole: its seed, its cast and its client are what (a) runs on. Its own tests are
// compiled in as well (they are `#[ignore]`d or pure); the harness entry is `matrix::harness`.
#[path = "authz_matrix.rs"]
#[allow(dead_code, unused_imports)]
mod matrix;

use matrix::harness::{Denial, Flip, RULE_GROUPS, Report};
use serde_json::Value;
use std::collections::BTreeSet;

/// The substrings no flag name may contain (lower case).
const DENY: [&str; 7] = [
    "e2ee",
    "encrypt",
    "audit",
    "access_log",
    "authz",
    "authorization",
    "permission",
];

/// Names that matched the pattern before the rule existed. Each is a setting of something else (who may do a
/// thing in a module), not a switch for the three. The list may only shrink; a new entry needs the owner.
const LEGACY: &[&str] = &[
    // How long a server's own audit log (the Discord-parity one members and moderators read) is kept; 0 means
    // forever. It is not the tamper-evident staff audit log or the staff access log, which have no retention
    // setting at all. Kept by P28-OPS-1: the only legacy entry; a rename was rejected (API key, stored row, D-158).
    "moderation.audit_retention_days",
];

fn denied(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    DENY.iter().find(|d| lower.contains(*d)).copied()
}

/// Every `key` string anywhere in a JSON value (the fields of a settings schema, groups included).
fn keys_of(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                if k == "key"
                    && let Some(s) = x.as_str()
                {
                    out.push(s.to_owned());
                }
                keys_of(x, out);
            }
        }
        Value::Array(a) => a.iter().for_each(|x| keys_of(x, out)),
        _ => {}
    }
}

/// Every object key anywhere in a JSON value (the fields of the config).
fn object_keys(v: &Value, path: &str, out: &mut Vec<String>) {
    if let Value::Object(m) = v {
        for (k, x) in m {
            let p = if path.is_empty() {
                k.clone()
            } else {
                format!("{path}.{k}")
            };
            out.push(p.clone());
            object_keys(x, &p, out);
        }
    }
}

/// Every string stored under `label`, `help` or `description` anywhere in a JSON value.
fn texts_of(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                if matches!(k.as_str(), "label" | "help" | "description" | "name")
                    && let Some(s) = x.as_str()
                {
                    out.push(s.to_owned());
                }
                texts_of(x, out);
            }
        }
        Value::Array(a) => a.iter().for_each(|x| texts_of(x, out)),
        _ => {}
    }
}

/// All the names a flag can have: feature ids, settings-schema field keys, settings catalog keys, the paths of
/// the config fields, and (OPS-IMM-003) the names the phase's new crates hand out: the storage kinds and role
/// grants (the emulator's `games` kind and `view_games` grant), the activity bundle capabilities
/// (`sr-activities`) and the bridge's module ids (`sr-bridge` through `sr-features`).
fn all_flag_names() -> Vec<(String, String)> {
    let registry = sr_server::app::feature_registry().expect("the feature registry builds");
    all_flag_names_of(&registry)
}

/// Every storage kind, by an exhaustive match so that a kind a later wave adds stops this file compiling until
/// it is listed (and so checked).
fn storage_kinds() -> Vec<sr_storage::scopes::Kind> {
    use sr_storage::scopes::Kind;
    let all = [Kind::Cinema, Kind::Library, Kind::Music, Kind::Games];
    for k in all {
        match k {
            Kind::Cinema | Kind::Library | Kind::Music | Kind::Games => {}
        }
    }
    all.to_vec()
}

/// [`all_flag_names`] for a given registry (the violation proof registers a test-only feature on top of the
/// real one).
fn all_flag_names_of(registry: &sr_features::FeatureRegistry) -> Vec<(String, String)> {
    let mut names: Vec<(String, String)> = Vec::new();
    for k in storage_kinds() {
        names.push(("storage kind".into(), k.as_str().to_owned()));
    }
    for g in sr_storage::scopes::Grant::ALL {
        names.push(("storage grant".into(), g.as_str().to_owned()));
    }
    for c in sr_activities::bundle::CAPABILITIES {
        names.push(("activity capability".into(), (*c).to_owned()));
    }
    for id in [
        sr_features::BRIDGE_MODULE_ID,
        sr_features::PLATFORM_BRIDGE_ID,
    ] {
        names.push(("bridge module id".into(), id.to_owned()));
    }
    for f in registry.iter() {
        names.push(("feature id".into(), f.id().to_owned()));
        let schema = serde_json::to_value(f.settings_schema()).expect("schema serializes");
        let mut keys = Vec::new();
        keys_of(&schema, &mut keys);
        for k in keys {
            names.push((format!("setting of {}", f.id()), k));
        }
    }
    for d in sr_admin::settings::CATALOG {
        names.push(("platform setting".into(), d.key.to_owned()));
    }
    let cfg = serde_json::to_value(sr_core::config::Config::default()).expect("config serializes");
    let mut paths = Vec::new();
    object_keys(&cfg, "", &mut paths);
    for p in paths {
        names.push(("config field".into(), p));
    }
    names
}

/// The body of the name test, over a given registry: panics (that is the failure the violation proof expects)
/// when a name of the catalog matches the deny pattern outside [`LEGACY`].
fn assert_names_clean(registry: &sr_features::FeatureRegistry) {
    let all = all_flag_names_of(registry);
    let mut hits: BTreeSet<String> = BTreeSet::new();
    for (what, name) in &all {
        if let Some(d) = denied(name)
            && !LEGACY.contains(&name.as_str())
        {
            hits.insert(format!("{what} `{name}` contains `{d}`"));
        }
    }
    assert!(
        hits.is_empty(),
        "names that look like a switch for end-to-end encryption, authorization or the audit and access logs \
         (the IMMUTABLE RULE forbids such flags; if one is a false positive, pin it in LEGACY with the owner):\n{}",
        hits.into_iter().collect::<Vec<_>>().join("\n")
    );
    // The legacy list names things that still exist (it may only shrink).
    let names: BTreeSet<&str> = all.iter().map(|(_, n)| n.as_str()).collect();
    for l in LEGACY {
        assert!(
            names.contains(*l),
            "LEGACY entry `{l}` no longer exists: remove it"
        );
    }
}

#[test]
fn no_flag_name_matches_the_deny_pattern() {
    let registry = sr_server::app::feature_registry().expect("the feature registry builds");
    assert_names_clean(&registry);
}

/// A feature that exists only in this test build, named like a switch for the end-to-end encryption of calls.
struct TestOnlyE2eeSwitch;

impl sr_features::Feature for TestOnlyE2eeSwitch {
    fn id(&self) -> &'static str {
        "voice.e2ee_test"
    }
    fn meta(&self) -> sr_features::feature::FeatureMeta {
        sr_features::feature::FeatureMeta {
            name: "Voice test switch".into(),
            description: "A test-only feature that exists to be caught.".into(),
            section: "Voice".into(),
            phase: 39,
            default_enabled: true,
        }
    }
    fn settings_schema(&self) -> sr_features::SettingsSchema {
        sr_features::SettingsSchema::default()
    }
    fn scopes(&self) -> &[sr_features::feature::Scope] {
        &[sr_features::feature::Scope::Platform]
    }
}

/// OPS-IMM-003, the deliberate violation: `voice.e2ee_test`, registered in this test build only, is caught by
/// the very check the name test runs. `should_panic` makes the expected failure the pass condition; the
/// message must name the feature, so a panic for any other reason does not count.
#[test]
#[should_panic(expected = "feature id `voice.e2ee_test` contains `e2ee`")]
fn a_test_only_voice_e2ee_test_feature_is_caught_by_the_name_test() {
    let mut registry = sr_server::app::feature_registry().expect("the feature registry builds");
    registry
        .register(TestOnlyE2eeSwitch)
        .expect("the test-only feature registers");
    assert_names_clean(&registry);
}

/// The same proof without the panic: the violation is reported, and nothing else is (the real catalog is
/// clean), so the test above fails for the right reason.
#[test]
fn the_deliberate_violation_is_the_only_hit() {
    let mut registry = sr_server::app::feature_registry().expect("the feature registry builds");
    registry.register(TestOnlyE2eeSwitch).unwrap();
    let hits: Vec<String> = all_flag_names_of(&registry)
        .into_iter()
        .filter(|(_, n)| denied(n).is_some() && !LEGACY.contains(&n.as_str()))
        .map(|(w, n)| format!("{w} {n}"))
        .collect();
    assert_eq!(hits, vec!["feature id voice.e2ee_test".to_owned()]);
}

/// The new crates' names are scanned: each source is non-empty (an empty source would make the deny scan pass
/// vacuously) and the emulator's kind and grant are among them.
#[test]
fn the_names_of_the_new_crates_are_scanned() {
    let names = all_flag_names();
    let has = |what: &str, n: &str| names.iter().any(|(w, x)| w == what && x == n);
    assert!(has("storage kind", "games"), "the emulator's storage kind");
    assert!(has("storage grant", "view_games"), "the emulator's grant");
    assert!(has("activity capability", "datachannel"));
    assert!(has("bridge module id", sr_features::BRIDGE_MODULE_ID));
    assert!(has("bridge module id", sr_features::PLATFORM_BRIDGE_ID));
    assert!(names.iter().any(|(w, _)| w == "platform setting"));
    assert!(names.iter().any(|(w, _)| w == "config field"));
}

#[test]
fn the_voice_encryption_switch_is_gone_from_the_config() {
    let cfg = serde_json::to_value(sr_core::config::Config::default()).unwrap();
    assert!(
        cfg["voice"].get("e2ee").is_none(),
        "voice.e2ee must not come back (OPS-IMM-001)"
    );
    assert!(
        !all_flag_names()
            .iter()
            .any(|(_, n)| n.to_ascii_lowercase().contains("e2ee")),
        "no flag may carry e2ee in its name"
    );
}

/// Phrases that turn something off, and what they must not be followed by.
const OFF: [&str; 8] = [
    "disable",
    "turn off",
    "switch off",
    "bypass",
    "skip",
    "stop writing",
    "stop recording",
    "do not log",
];
const PROTECTED: [&str; 9] = [
    "audit",
    "access log",
    "logging",
    "encrypt",
    "e2ee",
    "end-to-end",
    "authoriz",
    "permission check",
    "tamper",
];

/// A text claims to disable the three when an "off" phrase is followed within 48 characters by one of them.
fn claims_to_disable(text: &str) -> Option<String> {
    let t = text.to_ascii_lowercase();
    for off in OFF {
        let mut from = 0;
        while let Some(i) = t[from..].find(off) {
            let at = from + i + off.len();
            let window: String = t[at..].chars().take(48).collect();
            if let Some(p) = PROTECTED.iter().find(|p| window.contains(*p)) {
                return Some(format!("`{off}` ... `{p}`"));
            }
            from = at;
        }
    }
    None
}

#[test]
fn no_description_claims_to_disable_logging_or_encryption() {
    let mut texts: Vec<(String, String)> = Vec::new();
    for d in sr_admin::settings::CATALOG {
        texts.push((d.key.to_owned(), d.description.to_owned()));
    }
    let registry = sr_server::app::feature_registry().expect("the feature registry builds");
    for f in registry.iter() {
        let meta = f.meta();
        texts.push((f.id().to_owned(), meta.name));
        texts.push((f.id().to_owned(), meta.description));
        let schema = serde_json::to_value(f.settings_schema()).unwrap();
        let mut found = Vec::new();
        texts_of(&schema, &mut found);
        texts.extend(found.into_iter().map(|t| (f.id().to_owned(), t)));
    }
    let bad: Vec<String> = texts
        .iter()
        .filter_map(|(who, t)| claims_to_disable(t).map(|c| format!("{who}: {c}: {t}")))
        .collect();
    assert!(
        bad.is_empty(),
        "descriptions that claim to disable logging, authorization or encryption:\n{}",
        bad.join("\n")
    );
}

#[test]
fn the_description_check_catches_what_it_should() {
    assert!(claims_to_disable("Turn off the audit log for staff actions.").is_some());
    assert!(claims_to_disable("Skip end-to-end encryption for old clients").is_some());
    assert!(claims_to_disable("Disable permission checks on uploads").is_some());
    assert!(claims_to_disable("Bypass authorization for bots").is_some());
    assert!(claims_to_disable("Skip the welcome message for new members").is_none());
    assert!(claims_to_disable("Disable slowmode in this channel").is_none());
    assert!(claims_to_disable("Records staff reads in the audit log").is_none());
}

#[test]
fn the_deny_pattern_itself_is_pinned() {
    assert_eq!(
        DENY,
        [
            "e2ee",
            "encrypt",
            "audit",
            "access_log",
            "authz",
            "authorization",
            "permission"
        ],
        "the deny pattern is the one in docs/dev/roadmap-p27.md section 2; change it only with the owner"
    );
    assert_eq!(denied("voice.e2ee"), Some("e2ee"));
    assert_eq!(denied("staff.audit_off"), Some("audit"));
    assert_eq!(denied("limits.rate.ip"), None);
    assert_eq!(
        LEGACY,
        &["moderation.audit_retention_days"],
        "P28-OPS-1: the list may only shrink"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn refusals_and_logs_hold_in_every_flag_state() {
    let mut failures: Vec<String> = Vec::new();
    // OPS-IMM-003: the third state (every guild module off, then every feature killed) joins the first three.
    for flip in [
        Flip::KillFeatures,
        Flip::SettingsMin,
        Flip::SettingsMax,
        Flip::KillAll,
    ] {
        let rep = matrix::harness::run(flip, &RULE_GROUPS).await;
        for l in &rep.lines {
            println!("immutable_rules: {flip:?}: {l}");
        }
        failures.extend(rep.failures.into_iter().map(|f| format!("{flip:?}: {f}")));
    }
    let unique: BTreeSet<&String> = failures.iter().collect();
    assert!(
        unique.is_empty(),
        "{} IMMUTABLE RULE failures:\n{}",
        unique.len(),
        unique
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// BRG-SYN-002 (wave 35b): the whole-guild bridge's settings and the permission bit classes are not switches for
/// the three. The three keys of design section 20 pass the deny pattern whether or not they are registered yet,
/// every bridge name that does exist passes it too, and no bridge setting offers to lift the rule that
/// SIDE ROOM is the master copy of permissions.
#[test]
fn the_whole_guild_bridge_adds_no_switch_for_the_three() {
    for key in [
        "bridge.delete_hold_hours",
        "bridge.reconcile_minutes",
        "bridge.struct_writes_per_sec",
    ] {
        assert!(denied(key).is_none(), "`{key}` matches the deny pattern");
    }
    for (what, name) in all_flag_names() {
        let lower = name.to_ascii_lowercase();
        if lower.contains("bridge") {
            assert!(
                denied(&name).is_none(),
                "{what} `{name}` matches the deny pattern"
            );
            // No bridge setting may let Discord set, import or override SIDE ROOM permissions or roles.
            for bad in [
                "perm",
                "role_sync",
                "discord_master",
                "two_way",
                "import_roles",
            ] {
                assert!(
                    !lower.contains(bad),
                    "{what} `{name}` reads like a switch for who owns permissions"
                );
            }
        }
    }
    // The ceilings only shrink: the bridge's permission classes cover the whole word, exactly once.
    use sr_core::permissions::Permissions;
    assert_eq!(
        Permissions::BRIDGE_SHARED | Permissions::SR_ONLY,
        Permissions::ALL
    );
    assert!((Permissions::BRIDGE_SHARED & Permissions::SR_ONLY).is_empty());
}

// ---------------------------------------------------------------------------------------------
// OPS-IMM-003: the full catalog of this phase.
//
// Everything below walks the REGISTRY (and the guild module list the server answers), never a list of ids, so
// the features the other wave-39 agents register (platform.activities_datachannel, platform.emulator,
// profiles.access, platform.game_servers, the invite settings) are covered the moment they are merged. The
// only hard-coded ids are the ones in `known_at_base` (named in the handoff).
// ---------------------------------------------------------------------------------------------

/// Ids that exist on the base of this wave (waves 27 to 37). Registered ids may be added to the registry freely;
/// one of these disappearing is a finding.
fn known_at_base() -> Vec<&'static str> {
    vec![
        sr_features::BYOS_ID,
        sr_features::PUBLIC_PROFILES_ID,
        sr_features::PLATFORM_BRIDGE_ID,
        sr_features::BRIDGE_MODULE_ID,
        sr_features::COMPLIANCE_ID,
        "lounge.scrapbook",
    ]
}

#[test]
fn the_registry_catalog_is_well_formed_and_every_feature_can_be_killed() {
    use sr_features::feature::Scope;
    let registry = sr_server::app::feature_registry().expect("the feature registry builds");
    let mut problems: Vec<String> = Vec::new();
    for id in known_at_base() {
        if registry.get(id).is_none() {
            problems.push(format!("`{id}` is no longer registered"));
        }
    }
    for f in registry.iter() {
        let id = f.id();
        let meta = f.meta();
        if id.is_empty() // (`unfurl` predates the section.name shape: only the alphabet is checked)
            || id
                .chars()
                .any(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_'))
        {
            problems.push(format!(
                "`{id}`: an id is lower-case letters, digits, dots and underscores"
            ));
        }
        if meta.name.trim().is_empty() || meta.description.trim().is_empty() {
            problems.push(format!(
                "`{id}`: no name or description for Admin > Features"
            ));
        }
        if f.scopes().is_empty() {
            problems.push(format!("`{id}`: declares no scope"));
        }
        // The kill switch is the platform flag: a module with only guild, channel or user scope could not be
        // switched off by an operator, and the flags-killed states would not reach it.
        if !f.scopes().contains(&Scope::Platform) && f.gated_by().is_none() {
            problems.push(format!(
                "`{id}`: no platform scope and no `gated_by` parent, so the operator cannot kill it"
            ));
        }
        if f.owner_only() && !f.scopes().contains(&Scope::Guild) {
            problems.push(format!("`{id}`: owner-only but not a guild module"));
        }
        if let Some(parent) = f.gated_by() {
            match registry.get(parent) {
                None => problems.push(format!("`{id}`: gated_by `{parent}` is not registered")),
                Some(p) if !p.scopes().contains(&Scope::Platform) => problems.push(format!(
                    "`{id}`: gated_by `{parent}` has no platform scope to kill"
                )),
                Some(_) => {}
            }
        }
    }
    assert!(
        problems.is_empty(),
        "the feature catalog is not well formed:\n{}",
        problems.join("\n")
    );
}

/// The number of groups of the authorization matrix (`GROUPS` in `authz_matrix.rs`).
const ALL_GROUPS: usize = 6;

/// The rule groups exist, and the nightly walks every group of the matrix.
#[test]
fn the_rule_groups_exist() {
    assert!(RULE_GROUPS.iter().all(|g| *g < ALL_GROUPS));
    assert_eq!(matrix::harness::group_count(), ALL_GROUPS);
}

/// How two runs of the same matrix, one in the normal state and one with the flags killed, compare on the
/// calls that must be refused. Returns the disagreements (fatal) and the status changes that are still
/// refusals (informational: a killed feature answers 404 where the normal state answered 403).
fn compare_refusals(normal: &[Denial], killed: &[Denial]) -> (Vec<String>, Vec<String>) {
    use std::collections::BTreeMap;
    let n: BTreeMap<&str, &Denial> = normal.iter().map(|d| (d.key.as_str(), d)).collect();
    let k: BTreeMap<&str, &Denial> = killed.iter().map(|d| (d.key.as_str(), d)).collect();
    let (mut bad, mut info) = (Vec::new(), Vec::new());
    let word = |d: &Denial| if d.refused() { "refused" } else { "ADMITTED" };
    for (key, a) in &n {
        match k.get(key) {
            None => bad.push(format!("{key}: not made in the second run")),
            Some(b) => {
                if a.refused() != b.refused() {
                    bad.push(format!(
                        "{key}: first run {} ({}), second run {} ({})",
                        a.status,
                        word(a),
                        b.status,
                        word(b)
                    ));
                } else if a.status != b.status {
                    info.push(format!("{key}: {} -> {}", a.status, b.status));
                }
            }
        }
    }
    for key in k.keys() {
        if !n.contains_key(key) {
            bad.push(format!("{key}: not made in the first run"));
        }
    }
    (bad, info)
}

fn denial(key: &str, status: u16, verdict: &'static str) -> Denial {
    Denial {
        key: key.to_owned(),
        status,
        verdict,
    }
}

#[test]
fn the_refusal_comparison_catches_what_it_should() {
    let normal = vec![
        denial("a|GET /x|member|std", 403, "ok"),
        denial("a|GET /y|member|std", 404, "ok"),
        denial("a|POST /z|outsider|std", 403, "ok"),
    ];
    // Same refusals, one status moved from 403 to 404: fine, reported.
    let same = vec![
        denial("a|GET /x|member|std", 404, "ok"),
        denial("a|GET /y|member|std", 404, "ok"),
        denial("a|POST /z|outsider|std", 403, "ok"),
    ];
    let (bad, info) = compare_refusals(&normal, &same);
    assert!(bad.is_empty(), "{bad:?}");
    assert_eq!(info.len(), 1);
    // A refusal that stops being one when the flags are killed is the failure the job exists to find.
    let mut broken = same.clone();
    broken[2] = denial("a|POST /z|outsider|std", 200, "FAIL");
    let (bad, _) = compare_refusals(&normal, &broken);
    assert_eq!(bad.len(), 1, "{bad:?}");
    assert!(bad[0].contains("ADMITTED"));
    // The other way round is a difference too.
    let (bad, _) = compare_refusals(&broken, &normal);
    assert_eq!(bad.len(), 1, "{bad:?}");
    // A call that was not made in one run is a difference as well.
    let (bad, _) = compare_refusals(&normal, &same[..2]);
    assert_eq!(bad.len(), 1, "{bad:?}");
}

/// Print the run and collect its failures.
fn absorb(label: &str, rep: &Report, failures: &mut Vec<String>) {
    for l in &rep.lines {
        println!("immutable_rules: {label}: {l}");
    }
    failures.extend(rep.failures.iter().map(|f| format!("{label}: {f}")));
}

/// OPS-IMM-003, the nightly (`security-nightly.yml`): the WHOLE authorization matrix, refusals, reads and
/// writes, runs in the normal state and again with the flags killed (every guild module off, then every
/// feature killed at platform scope; `SR_IMMUTABLE_STATES=normal,kill_features,kill_all` adds the plain
/// platform kill). Each run must hold on its own, and every call that must be refused must be refused in both
/// runs. `SR_IMMUTABLE_GROUPS=0,1` restricts the groups (local iteration; the nightly runs all six).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs SR_TEST_DATABASE_URL and SR_SECURITY_NIGHTLY=1; the nightly runs it"]
async fn the_full_matrix_refuses_the_same_with_the_flags_killed() {
    // rust.yml runs every ignored test on every build; this one is the nightly's (and takes a long while).
    if std::env::var_os("SR_SECURITY_NIGHTLY").is_none() {
        println!("immutable_rules: skipped, set SR_SECURITY_NIGHTLY=1 (security-nightly.yml does)");
        return;
    }
    let groups: Vec<usize> = match std::env::var("SR_IMMUTABLE_GROUPS") {
        Ok(v) => v.split(',').filter_map(|x| x.trim().parse().ok()).collect(),
        Err(_) => (0..ALL_GROUPS).collect(),
    };
    let states: Vec<String> = std::env::var("SR_IMMUTABLE_STATES")
        .unwrap_or_else(|_| "normal,kill_all".into())
        .split(',')
        .map(|x| x.trim().to_owned())
        .collect();
    assert!(
        states.first().map(String::as_str) == Some("normal"),
        "the first state is the reference: `normal`"
    );
    let mut failures: Vec<String> = Vec::new();
    let reference = matrix::harness::run_full(Flip::Normal, &groups).await;
    absorb("normal", &reference, &mut failures);
    assert!(
        !reference.denials.is_empty(),
        "the normal run recorded no refusal: the matrix did not run"
    );
    for state in &states[1..] {
        let flip = match state.as_str() {
            "kill_features" => Flip::KillFeatures,
            "kill_all" => Flip::KillAll,
            other => panic!("unknown state `{other}`"),
        };
        let killed = matrix::harness::run_full(flip, &groups).await;
        absorb(state, &killed, &mut failures);
        let (bad, info) = compare_refusals(&reference.denials, &killed.denials);
        println!(
            "immutable_rules: {state}: {} refusals compared, {} disagree, {} changed status but are still refused",
            reference.denials.len(),
            bad.len(),
            info.len()
        );
        for i in info.iter().take(200) {
            println!("immutable_rules: {state}: status change: {i}");
        }
        failures.extend(bad.into_iter().map(|b| format!("{state}: {b}")));
    }
    let unique: BTreeSet<&String> = failures.iter().collect();
    assert!(
        unique.is_empty(),
        "{} IMMUTABLE RULE failures in the full matrix:\n{}",
        unique.len(),
        unique
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
}
