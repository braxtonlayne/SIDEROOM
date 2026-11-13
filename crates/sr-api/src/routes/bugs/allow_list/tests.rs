use super::*;

const NOW: i64 = 1_800_000_000_000;
const SECRET: &str = "srg_9f8e7d6c5b4a39281706f5e4d3c2b1a0";

/// A JPEG-shaped file: start marker, filler, end marker.
fn jpeg(len: usize) -> Vec<u8> {
    let mut v = vec![0xFF, 0xD8, 0xFF, 0xE0];
    v.resize(len - 2, 0x11);
    v.extend([0xFF, 0xD9]);
    v
}

fn picture(bytes: &[u8]) -> Value {
    json!({ "type": "image/jpeg", "data": BASE64.encode(bytes), "masked": 2 })
}

fn good() -> Value {
    json!({
        "description": "  The send button does nothing.  ",
        "bundle": {
            "commit": "abcdef1234",
            "route": "lounge.:guildId.:channelId",
            "at": NOW,
            "viewport": { "w": 1280, "h": 800, "dpr": 2 },
            "caps": { "engine": "chromium", "webgpu": true, "wasm": false },
            "locale": "en-GB",
            "theme": "lounge-dark",
            "online": true,
            "gateway": { "status": "connected", "reconnects": 3, "heartbeat_ms": 41 },
            "sw": "active",
            "voice": { "connected": false },
            "failed_requests": [
                { "method": "?", "path": "/api/v1/channels/:id/messages", "status": 500, "ms": 312, "ago_s": 14 }
            ],
            "crashes": ["a".repeat(64)]
        }
    })
}

#[test]
fn a_good_report_is_kept_as_the_contract_says() {
    let c = sanitize(&good(), NOW).unwrap();
    assert_eq!(c.description, "The send button does nothing.");
    assert_eq!(c.commit, "abcdef1234");
    assert_eq!(c.route, "lounge.:guildId.:channelId");
    assert_eq!(c.screenshot, None);
    assert_eq!(
        c.bundle,
        json!({
            "commit": "abcdef1234",
            "route": "lounge.:guildId.:channelId",
            "at": NOW,
            "viewport": { "w": 1280, "h": 800, "dpr": 2.0 },
            "caps": { "engine": "chromium", "wasm": false, "webgpu": true },
            "locale": "en-GB",
            "theme": "lounge-dark",
            "online": true,
            "gateway": { "status": "connected", "reconnects": 3, "heartbeat_ms": 41 },
            "sw": "active",
            "voice": { "connected": false },
            "failed_requests": [
                { "method": "?", "path": "/api/v1/channels/:id/messages", "status": 500, "ms": 312, "ago_s": 14 }
            ],
            "crashes": ["a".repeat(64)]
        })
    );
}

#[test]
fn a_bug_report_keeps_only_allow_listed_bundle_fields() {
    let mut r = good();
    // A token-looking string in an unknown field, at the top level and in the bundle.
    r["token"] = json!(SECRET);
    r["bundle"]["session_id"] = json!(SECRET);
    r["bundle"]["gateway"]["resume_url"] = json!(format!("wss://example.test/?t={SECRET}"));
    r["bundle"]["caps"]["note"] = json!(SECRET);
    r["bundle"]["viewport"]["user"] = json!(SECRET);
    // In a path with a query, in a route, in a locale, in a theme, in a crash fingerprint.
    r["bundle"]["failed_requests"] = json!([
        { "method": "GET", "path": format!("/api/v1/invites/x?token={SECRET}"), "status": 404, "ms": 1, "ago_s": 1 },
        { "method": "GET", "path": "/api/v1/users/@me", "status": 401, "ms": 1, "ago_s": 1, "body": SECRET },
    ]);
    r["bundle"]["route"] = json!(format!("lounge.{SECRET}"));
    r["bundle"]["locale"] = json!(SECRET);
    r["bundle"]["theme"] = json!(SECRET);
    r["bundle"]["sw"] = json!(SECRET);
    r["bundle"]["crashes"] = json!([SECRET, "b".repeat(64)]);
    r["bundle"]["commit"] = json!(SECRET);
    let c = sanitize(&r, NOW).unwrap();
    let stored = format!("{}{}{}", c.bundle, c.commit, c.route);
    assert!(!stored.contains(SECRET), "a secret was stored: {stored}");
    assert!(!stored.contains("resume_url") && !stored.contains("session_id"));
    assert_eq!(c.bundle["route"], "unknown");
    assert_eq!(c.bundle["commit"], "0000000");
    assert_eq!(c.bundle["locale"], "und");
    assert_eq!(c.bundle["theme"], "other");
    assert!(c.bundle.get("sw").is_none());
    assert_eq!(c.bundle["crashes"], json!(["b".repeat(64)]));
    // The entry with a query is gone; the other keeps only its named fields.
    assert_eq!(
        c.bundle["failed_requests"],
        json!([{ "method": "GET", "path": "/api/v1/users/@me", "status": 401, "ms": 1, "ago_s": 1 }])
    );
    // Only the listed top-level keys exist.
    let mut keys: Vec<&str> = c
        .bundle
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "at",
            "caps",
            "commit",
            "crashes",
            "failed_requests",
            "gateway",
            "locale",
            "online",
            "route",
            "theme",
            "viewport",
            "voice"
        ]
    );
    // The gateway, caps and viewport hold only their named fields.
    assert_eq!(
        c.bundle["gateway"],
        json!({ "status": "connected", "reconnects": 3, "heartbeat_ms": 41 })
    );
    assert_eq!(
        c.bundle["caps"],
        json!({ "engine": "chromium", "wasm": false, "webgpu": true })
    );
    assert_eq!(
        c.bundle["viewport"],
        json!({ "w": 1280, "h": 800, "dpr": 2.0 })
    );
}

#[test]
fn a_failed_request_path_with_an_id_is_dropped() {
    let entry =
        |path: &str| json!({ "method": "GET", "path": path, "status": 500, "ms": 5, "ago_s": 1 });
    let r = |paths: &[&str]| {
        let mut v = good();
        v["bundle"]["failed_requests"] = Value::Array(paths.iter().map(|p| entry(p)).collect());
        sanitize(&v, NOW).unwrap().bundle["failed_requests"].clone()
    };
    // A run of 6 digits, a query, an upper case id, a host, an escape, an e-mail, no leading slash, too long.
    let long = format!("/{}", "a".repeat(121));
    for bad in [
        "/api/v1/users/123456/profile",
        "/api/v1/users/1234567890123456789",
        "/api/v1/messages?id=1",
        "/api/v1/invites/AbC123",
        "https://example.test/api",
        "/api/v1/a%20b",
        "/api/v1/users/me@example.test",
        "api/v1/users",
        "/",
        "/api/v1/tokens/0123456789abcdef0123",
        long.as_str(),
    ] {
        assert_eq!(r(&[bad]), json!([]), "{bad:?} was kept");
    }
    // The templates a client sends survive.
    for ok in [
        "/api/v1/channels/:id/messages",
        "/api/v1/users/@me",
        "/api/v1/sr/csrf",
        "/api/v1/guilds/:id/members/:id",
    ] {
        assert_eq!(r(&[ok]).as_array().unwrap().len(), 1, "{ok}");
    }
    // Nothing id-like gets through, even below the 6 digits of the contract.
    assert_eq!(r(&["/api/v1/users/12345/x"]), json!([]));
}

#[test]
fn numbers_are_clamped_and_enums_are_exact() {
    let mut v = good();
    v["bundle"]["viewport"] = json!({ "w": 99999, "h": -5, "dpr": 40 });
    v["bundle"]["gateway"] =
        json!({ "status": "connected", "reconnects": 99999999, "heartbeat_ms": -3 });
    v["bundle"]["failed_requests"] = json!([
        { "method": "GET", "path": "/api/v1/a", "status": 500, "ms": 99999999, "ago_s": 99999999 },
        { "method": "get", "path": "/api/v1/b", "status": 500 },
        { "method": "TRACE", "path": "/api/v1/c", "status": 500 },
        { "method": "GET", "path": "/api/v1/d", "status": 99 },
        { "method": "GET", "path": "/api/v1/e", "status": 600 },
        { "method": "POST", "path": "/api/v1/f", "status": 599 },
    ]);
    v["bundle"]["theme"] = json!("Lounge-Dark");
    v["bundle"]["caps"]["engine"] = json!("blink");
    let b = sanitize(&v, NOW).unwrap().bundle;
    assert_eq!(b["viewport"], json!({ "w": 16384, "h": 1, "dpr": 8.0 }));
    assert_eq!(
        b["gateway"],
        json!({ "status": "connected", "reconnects": 10000, "heartbeat_ms": 0 })
    );
    assert_eq!(
        b["failed_requests"],
        json!([
            { "method": "GET", "path": "/api/v1/a", "status": 500, "ms": 600000, "ago_s": 86400 },
            { "method": "POST", "path": "/api/v1/f", "status": 599, "ms": 0, "ago_s": 0 },
        ])
    );
    assert_eq!(b["theme"], "other", "an enum matches exactly");
    assert_eq!(b["caps"]["engine"], "other");
    // A device pixel ratio below the floor is raised to it, and two decimals are kept.
    v["bundle"]["viewport"] = json!({ "w": 800, "h": 600, "dpr": 0.1 });
    assert_eq!(sanitize(&v, NOW).unwrap().bundle["viewport"]["dpr"], 0.5);
    v["bundle"]["viewport"] = json!({ "w": 800, "h": 600, "dpr": 1.23456 });
    assert_eq!(sanitize(&v, NOW).unwrap().bundle["viewport"]["dpr"], 1.23);
}

#[test]
fn at_most_20_failed_requests_and_5_crashes_are_kept() {
    let mut v = good();
    let many: Vec<Value> = (0..30)
        .map(|i| json!({ "method": "GET", "path": format!("/api/v1/r{i}"), "status": 500, "ms": 1, "ago_s": 1 }))
        .collect();
    v["bundle"]["failed_requests"] = Value::Array(many);
    v["bundle"]["crashes"] = Value::Array((0..9).map(|_| json!("c".repeat(64))).collect());
    let b = sanitize(&v, NOW).unwrap().bundle;
    assert_eq!(b["failed_requests"].as_array().unwrap().len(), 20);
    assert_eq!(b["crashes"].as_array().unwrap().len(), 5);
}

#[test]
fn a_description_is_required_and_capped() {
    let with = |d: Value| {
        let mut v = good();
        v["description"] = d;
        sanitize(&v, NOW)
    };
    assert_eq!(with(json!("")).unwrap_err(), Reason::NoDescription);
    assert_eq!(with(json!("  \n\t ")).unwrap_err(), Reason::NoDescription);
    assert_eq!(
        with(json!("\u{0}\u{7}\u{1b}")).unwrap_err(),
        Reason::NoDescription
    );
    assert_eq!(with(json!(7)).unwrap_err(), Reason::NoDescription);
    assert_eq!(with(Value::Null).unwrap_err(), Reason::NoDescription);
    assert_eq!(
        with(json!("x".repeat(2001))).unwrap_err(),
        Reason::DescriptionTooLong
    );
    assert_eq!(
        with(json!("x".repeat(2000))).unwrap().description.len(),
        2000
    );
    // Characters, not bytes, are counted.
    assert!(with(json!("é".repeat(2000))).is_ok());
    assert_eq!(
        with(json!("é".repeat(2001))).unwrap_err(),
        Reason::DescriptionTooLong
    );
    // Missing altogether.
    let mut v = good();
    v.as_object_mut().unwrap().remove("description");
    assert_eq!(sanitize(&v, NOW).unwrap_err(), Reason::NoDescription);
    assert_eq!(sanitize(&json!([1]), NOW).unwrap_err(), Reason::NotAnObject);
}

#[test]
fn a_description_is_stored_as_written_minus_control_characters() {
    let mut v = good();
    v["description"] = json!("line one\r\nline\ttwo\u{7} <b>not html</b> `code` \u{202e}end");
    let d = sanitize(&v, NOW).unwrap().description;
    assert_eq!(d, "line one\nline\ttwo <b>not html</b> `code` \u{202e}end");
}

#[test]
fn a_report_without_a_picture_is_accepted() {
    let mut v = good();
    assert!(sanitize(&v, NOW).unwrap().screenshot.is_none());
    v["screenshot"] = Value::Null;
    let c = sanitize(&v, NOW).unwrap();
    assert!(c.screenshot.is_none());
    assert!(c.bundle.get("masked").is_none());
    // Only a description at all.
    let c = sanitize(&json!({ "description": "it broke" }), NOW).unwrap();
    assert_eq!(c.bundle["route"], "unknown");
    assert_eq!(c.commit, "0000000");
}

#[test]
fn a_picture_that_is_a_jpeg_is_kept_with_its_masked_count() {
    let mut v = good();
    let bytes = jpeg(5000);
    v["screenshot"] = picture(&bytes);
    let c = sanitize(&v, NOW).unwrap();
    assert_eq!(c.screenshot.as_deref(), Some(bytes.as_slice()));
    assert_eq!(c.bundle["masked"], 2);
    v["screenshot"]["masked"] = json!(1000);
    assert_eq!(sanitize(&v, NOW).unwrap().bundle["masked"], 64);
}

#[test]
fn a_picture_that_is_not_a_jpeg_is_refused() {
    let png = [
        0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3, 4, 0xFF, 0xD9,
    ];
    let html = b"<html><script>alert(1)</script></html>".to_vec();
    let mut no_end = jpeg(100);
    no_end.truncate(98);
    let mut no_start = jpeg(100);
    no_start[0] = 0;
    for (what, shot, want) in [
        (
            "a PNG named image/jpeg",
            picture(&png),
            Reason::PictureNotJpeg,
        ),
        (
            "HTML named image/jpeg",
            picture(&html),
            Reason::PictureNotJpeg,
        ),
        ("a JPEG cut short", picture(&no_end), Reason::PictureNotJpeg),
        (
            "a file that starts wrong",
            picture(&no_start),
            Reason::PictureNotJpeg,
        ),
        (
            "too short",
            picture(&[0xFF, 0xD8, 0xFF, 0xD9]),
            Reason::PictureNotJpeg,
        ),
        ("an empty file", picture(&[]), Reason::PictureNotJpeg),
        (
            "a real JPEG named image/png",
            json!({ "type": "image/png", "data": BASE64.encode(&jpeg(100)) }),
            Reason::PictureType,
        ),
        (
            "an SVG",
            json!({ "type": "image/svg+xml", "data": BASE64.encode(b"<svg/>") }),
            Reason::PictureType,
        ),
        (
            "no type",
            json!({ "data": BASE64.encode(&jpeg(100)) }),
            Reason::PictureType,
        ),
        (
            "a string",
            json!("data:image/jpeg;base64,AAAA"),
            Reason::PictureType,
        ),
        (
            "bad base64",
            json!({ "type": "image/jpeg", "data": "not base64 !!" }),
            Reason::PictureEncoding,
        ),
        (
            "no data",
            json!({ "type": "image/jpeg" }),
            Reason::PictureEncoding,
        ),
    ] {
        let mut v = good();
        v["screenshot"] = shot;
        assert_eq!(sanitize(&v, NOW).unwrap_err(), want, "{what}");
    }
}

#[test]
fn a_picture_over_the_size_is_refused() {
    let mut v = good();
    v["screenshot"] = picture(&jpeg(MAX_PICTURE_BYTES));
    assert!(sanitize(&v, NOW).is_ok(), "exactly 2 MiB is the largest");
    v["screenshot"] = picture(&jpeg(MAX_PICTURE_BYTES + 1));
    assert_eq!(sanitize(&v, NOW).unwrap_err(), Reason::PictureTooLarge);
    // A text far over the limit is refused before it is decoded.
    v["screenshot"] = json!({ "type": "image/jpeg", "data": "A".repeat(4 * 1024 * 1024) });
    assert_eq!(sanitize(&v, NOW).unwrap_err(), Reason::PictureTooLarge);
}
