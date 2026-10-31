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

use matrix::harness::{Flip, RULE_GROUPS};
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

/// All the names a flag can have: feature ids, settings-schema field keys, settings catalog keys, and the
/// paths of the config fields.
fn all_flag_names() -> Vec<(String, String)> {
    let mut names: Vec<(String, String)> = Vec::new();
    let registry = sr_server::app::feature_registry().expect("the feature registry builds");
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

#[test]
fn no_flag_name_matches_the_deny_pattern() {
    let mut hits: BTreeSet<String> = BTreeSet::new();
    for (what, name) in all_flag_names() {
        if let Some(d) = denied(&name)
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
    let names: BTreeSet<String> = all_flag_names().into_iter().map(|(_, n)| n).collect();
    for l in LEGACY {
        assert!(
            names.contains(*l),
            "LEGACY entry `{l}` no longer exists: remove it"
        );
    }
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
    for flip in [Flip::KillFeatures, Flip::SettingsMin, Flip::SettingsMax] {
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
