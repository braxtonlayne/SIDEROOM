use super::*;

const FIXTURE: &str = include_str!("../../../../../../e2e/fixtures/crash-hostile.json");
const NOW: i64 = 1_800_000_000_000;

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).unwrap()
}

#[test]
fn a_payload_stuffed_with_secrets_leaves_only_the_allow_listed_card() {
    let f = fixture();
    let s = sanitize(&f["payload"], NOW).unwrap();
    let card = s.card_json();
    let replay = s.replay_json();
    let everything = format!("{card}{replay}{}", s.fingerprint);
    for needle in f["needles"].as_array().unwrap() {
        let n = needle.as_str().unwrap();
        assert!(
            !everything.contains(n),
            "the card still contains {n:?}: {everything}"
        );
    }
    // Exactly the expected card: nothing more got through either.
    let mut want = f["expect"].clone();
    let ticks = want.as_object_mut().unwrap().remove("ticks").unwrap();
    want["at"] = json!(NOW);
    assert_eq!(card, want);
    assert_eq!(replay, ticks);
}

#[test]
fn only_the_listed_top_level_fields_exist_on_the_card() {
    let s = sanitize(&fixture()["payload"], NOW).unwrap();
    let card = s.card_json();
    let mut keys: Vec<&str> = card
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "at", "caps", "commit", "engine", "frames", "kind", "route", "v"
        ]
    );
}

#[test]
fn a_report_without_a_commit_or_kind_is_refused() {
    assert_eq!(sanitize(&json!([1]), NOW), Err(Reject::NotAnObject));
    assert_eq!(
        sanitize(&json!({"kind": "error"}), NOW),
        Err(Reject::BadCommit)
    );
    assert_eq!(
        sanitize(&json!({"commit": "ABCDEF1", "kind": "error"}), NOW),
        Err(Reject::BadCommit)
    );
    assert_eq!(
        sanitize(&json!({"commit": "abcdef1", "kind": "panic!"}), NOW),
        Err(Reject::BadKind)
    );
    assert!(sanitize(&json!({"commit": "abcdef1", "kind": "error"}), NOW).is_ok());
}

#[test]
fn a_route_name_survives_and_a_url_does_not() {
    let r = |route: &str| {
        sanitize(
            &json!({"commit": "abcdef1", "kind": "error", "route": route}),
            NOW,
        )
        .unwrap()
        .card
        .route
    };
    assert_eq!(
        r("lounge.:guildId.:channelId"),
        "lounge.:guildId.:channelId"
    );
    assert_eq!(r("admin.invite-graph"), "admin.invite-graph");
    assert_eq!(r("/lounge/12345678901234567/9?x=1"), "unknown");
    assert_eq!(r("https://a.test/x"), "unknown");
    assert_eq!(r("user.123456789012"), "unknown");
    assert_eq!(r(""), "unknown");
}

#[test]
fn file_names_lose_hosts_queries_and_identifiers() {
    let n = |f: &str| normalize_file(f);
    assert_eq!(
        n("https://h.test/assets/a-1b2c3d4e.js?x=1#y").as_deref(),
        Some("assets/a-1b2c3d4e.js")
    );
    assert_eq!(
        n("https://h.test/a/b/c/d/e/f.js").as_deref(),
        Some("c/d/e/f.js")
    );
    assert_eq!(
        n("https://h.test/x/550e8400-e29b-41d4-a716-446655440000/y.js").as_deref(),
        Some("x/:id/y.js")
    );
    assert_eq!(
        n("/home/bob/src/lib.rs").as_deref(),
        Some("home/:id/src/lib.rs")
    );
    assert_eq!(n("wasm://wasm/1234abcd").as_deref(), Some("wasm"));
    assert_eq!(n("https://h.test").as_deref(), None);
    assert_eq!(n("").as_deref(), None);
    // A piece with a character outside the set is not copied.
    assert_eq!(n("https://h.test/a b/c.js").as_deref(), Some(":id/c.js"));
}

#[test]
fn the_replay_keeps_the_newest_sixty_ticks_and_only_enum_inputs() {
    let ticks: Vec<Value> = (0..100)
        .map(|i| json!({"t": i, "src": "game", "input": if i % 2 == 0 { "left" } else { "free text" }}))
        .collect();
    let s = sanitize(
        &json!({"commit": "abcdef1", "kind": "error", "ticks": ticks}),
        NOW,
    )
    .unwrap();
    assert_eq!(s.ticks.len(), MAX_TICKS);
    assert_eq!(s.ticks[0].t, 40);
    assert_eq!(s.ticks[59].t, 99);
    assert!(s.ticks.iter().all(|t| TICK_INPUTS.contains(&t.input)));
    assert!(s.ticks.iter().any(|t| t.input == "other"));
}

#[test]
fn the_fingerprint_follows_commit_and_top_frames_only() {
    let report = |commit: &str, msg: &str, route: &str| {
        sanitize(
            &json!({
                "commit": commit, "kind": "error", "route": route, "message": msg,
                "frames": [
                    {"file": "https://a.test/assets/x.js?u=1", "line": 4, "col": 2, "fn": "boom"},
                    {"file": "https://b.test/assets/y.js", "line": 9, "col": 1, "fn": "caller"}
                ]
            }),
            NOW,
        )
        .unwrap()
        .fingerprint
    };
    let a = report("abcdef1", "one", "r1");
    // A different message, host, route or query is the same crash.
    assert_eq!(a, report("abcdef1", "two", "r2"));
    // Another build is another crash.
    assert_ne!(a, report("abcdef2", "one", "r1"));
    assert_eq!(a.len(), 64);
    assert!(a.bytes().all(|b| b.is_ascii_hexdigit()));
}

#[test]
fn a_stackless_crash_is_told_apart_by_kind_and_route() {
    let f = |kind: &str, route: &str| {
        sanitize(
            &json!({"commit": "abcdef1", "kind": kind, "route": route}),
            NOW,
        )
        .unwrap()
        .fingerprint
    };
    assert_ne!(f("error", "a"), f("error", "b"));
    assert_ne!(f("error", "a"), f("unhandled_rejection", "a"));
    assert_eq!(f("error", "a"), f("error", "a"));
}

#[test]
fn the_detector_catches_credential_shapes_and_spares_ordinary_names() {
    for bad in [
        "a@b.co",
        "10.1.2.3",
        "AKIAIOSFODNN7EXAMPLE",
        "eyJhbGciOiJIUzI1NiJ9.abc",
        "https://x.test",
        "X-Amz-Signature",
        "0123456789abcdef0123456789abcdef",
    ] {
        assert!(suspicious(bad), "{bad}");
    }
    for ok in [
        "index-Bx3k9Qa1.js",
        "renderChannel",
        "sr_core::mls::decrypt",
        "lib.rs",
        "v1.2.3",
        "useAuthToken",
    ] {
        assert!(!suspicious(ok), "{ok}");
    }
}

#[test]
fn a_far_off_timestamp_is_replaced_by_the_server_clock() {
    let at = |v: i64| {
        sanitize(&json!({"commit": "abcdef1", "kind": "error", "at": v}), NOW)
            .unwrap()
            .card
            .at
    };
    assert_eq!(at(NOW - 5_000), NOW - 5_000);
    assert_eq!(at(NOW + 10 * 24 * 3600 * 1000), NOW);
    assert_eq!(at(-1), NOW);
}
