//! RDY-SEC-006: the abuse limits are applied by the real router layer on the real route patterns.
//!
//! Dummy handlers stand behind the exact patterns of `ABUSE_ROUTES` (a test in `rate_limit.rs` checks
//! that each of them is a route of the API); the layer under test is the one `build_router` installs.
//! The figures are the live platform settings, set to their floor through the watch channel that
//! `PATCH /admin/settings` feeds.

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
    routing::{MethodRouter, on},
};
use sr_api::{
    ApiSettings, AppState, build_router,
    rate_limit::{ABUSE_ROUTES, Abuse, RateConfig, RateLimitConfig},
};
use tokio::sync::watch;
use tower::ServiceExt;

async fn ok() -> StatusCode {
    StatusCode::NO_CONTENT
}

fn method_router(method: &str) -> MethodRouter<AppState> {
    let filter = match method {
        "POST" => axum::routing::MethodFilter::POST,
        "PUT" => axum::routing::MethodFilter::PUT,
        other => panic!("unexpected method {other}"),
    };
    on(filter, ok)
}

struct Fx {
    app: Router,
    live: watch::Sender<RateConfig>,
}

fn fixture() -> Fx {
    let (live, rx) = watch::channel(RateConfig::default());
    let settings = ApiSettings {
        rate_limit: RateLimitConfig {
            live: Some(rx),
            ..RateLimitConfig::default()
        },
        ..ApiSettings::default()
    };
    let state = AppState::builder().settings(settings).build();
    let mut extra = Router::<AppState>::new();
    for (key, _) in ABUSE_ROUTES {
        let (method, pattern) = key.split_once(' ').unwrap();
        extra = extra.route(pattern, method_router(method));
    }
    // A route with no abuse limit, to see that the others do not spill over.
    extra = extra.route("/sr/unlimited", method_router("POST"));
    Fx {
        app: build_router(state, extra),
        live,
    }
}

/// A concrete URL for a pattern: every `{param}` becomes `1`, but a webhook carries its token.
fn url(pattern: &str, token: &str) -> String {
    if pattern.starts_with("/webhooks/") {
        return format!("/api/v1/webhooks/5/{token}");
    }
    let mut out = String::new();
    let mut in_param = false;
    for c in pattern.chars() {
        match c {
            '{' => {
                in_param = true;
                out.push('1');
            }
            '}' => in_param = false,
            _ if !in_param => out.push(c),
            _ => {}
        }
    }
    format!("/api/v1{out}")
}

async fn send(app: &Router, method: &str, uri: &str, caller: &str) -> (StatusCode, bool) {
    let req = Request::builder()
        .method(Method::from_bytes(method.as_bytes()).unwrap())
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bot {caller}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let retry = resp.headers().contains_key(header::RETRY_AFTER);
    (resp.status(), retry)
}

#[tokio::test]
async fn every_guarded_route_answers_429_after_its_figure() {
    for (key, abuse) in ABUSE_ROUTES {
        let f = fixture();
        let floor = sr_admin::settings::find(abuse.key()).unwrap().min as u32;
        f.live.send_modify(|c| c.abuse[abuse.index()] = floor);
        // The first request of the window is always allowed, whatever the figure's shape.
        let (method, pattern) = key.split_once(' ').unwrap();
        let uri = url(pattern, "tok");
        let (s, _) = send(&f.app, method, &uri, "caller-a").await;
        assert_eq!(s, StatusCode::NO_CONTENT, "{key}: first request");
        // Spend the rest of the burst, then the next is refused with a Retry-After.
        let mut refused = None;
        for i in 1..=200 {
            let (s, retry) = send(&f.app, method, &uri, "caller-a").await;
            if s == StatusCode::TOO_MANY_REQUESTS {
                assert!(retry, "{key}: Retry-After on the 429");
                refused = Some(i);
                break;
            }
            assert_eq!(s, StatusCode::NO_CONTENT, "{key} #{i}");
        }
        assert!(refused.is_some(), "{key} never limited");
        // The unlimited route is untouched (its own default bucket is far away).
        let (s, _) = send(&f.app, "POST", "/api/v1/sr/unlimited", "caller-a").await;
        assert_eq!(s, StatusCode::NO_CONTENT, "{key} spilled over");
    }
}

#[tokio::test]
async fn a_member_limit_is_per_caller_and_a_webhook_limit_is_per_token() {
    let f = fixture();
    f.live.send_modify(|c| {
        c.abuse[Abuse::FriendRequests.index()] = 1;
        c.abuse[Abuse::WebhookExecute.index()] = 12; // a burst of one
    });
    // Friend requests: caller A is limited after one, caller B has their own allowance.
    let fr = "/api/v1/users/@me/relationships";
    assert_eq!(
        send(&f.app, "POST", fr, "a").await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        send(&f.app, "POST", fr, "a").await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        send(&f.app, "POST", fr, "b").await.0,
        StatusCode::NO_CONTENT
    );
    // Webhook: one token is one bucket whoever calls; another token is another bucket.
    assert_eq!(
        send(&f.app, "POST", &url("/webhooks/{a}/{b}", "t1"), "x").await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        send(&f.app, "POST", &url("/webhooks/{a}/{b}", "t1"), "y").await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        send(&f.app, "POST", &url("/webhooks/{a}/{b}", "t2"), "y").await.0,
        StatusCode::NO_CONTENT
    );
}

#[tokio::test]
async fn a_live_change_applies_to_the_next_request() {
    let f = fixture();
    f.live
        .send_modify(|c| c.abuse[Abuse::DmOpens.index()] = 1);
    let uri = "/api/v1/users/@me/channels";
    assert_eq!(
        send(&f.app, "POST", uri, "a").await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        send(&f.app, "POST", uri, "a").await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    f.live
        .send_modify(|c| c.abuse[Abuse::DmOpens.index()] = 500);
    assert_eq!(
        send(&f.app, "POST", uri, "a").await.0,
        StatusCode::NO_CONTENT
    );
}

/// Doc 05 "Abuse limits" lists every limit: a row that names a setting names a catalog entry with the
/// same default, every `abuse.*` setting is listed, and a row's test exists in the file it names.
#[test]
fn doc_05_table_matches_the_catalog_and_its_tests_exist() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let doc = std::fs::read_to_string(root.join("docs/05-api-and-gateway.md")).unwrap();
    let start = doc.find("**Abuse limits (RDY-SEC-006).**").expect("section");
    let rows: Vec<Vec<String>> = doc[start..]
        .lines()
        .skip_while(|l| !l.starts_with("| Limit |"))
        .skip(2)
        .take_while(|l| l.starts_with('|'))
        .map(|l| l.trim_matches('|').split('|').map(|c| c.trim().to_owned()).collect())
        .collect();
    assert!(rows.len() >= 15, "{} rows", rows.len());
    let mut listed = std::collections::HashSet::new();
    for r in &rows {
        assert_eq!(r.len(), 5, "{r:?}");
        if let Some(key) = r[1].strip_prefix('`').and_then(|k| k.split('`').next()) {
            let def = sr_admin::settings::find(key).unwrap_or_else(|| panic!("{key} not in catalog"));
            assert_eq!(r[2], def.default_int.to_string(), "{key} default");
            listed.insert(key.to_owned());
        }
        let (file, name) = r[4]
            .trim_matches('`')
            .split_once("::")
            .unwrap_or_else(|| panic!("test cell: {}", r[4]));
        let src = std::fs::read_to_string(root.join(file)).unwrap_or_else(|_| panic!("{file}"));
        assert!(
            src.contains(&format!("fn {name}(")),
            "{file} has no test {name}"
        );
    }
    for d in sr_admin::settings::CATALOG.iter().filter(|d| d.group == "abuse") {
        assert!(listed.contains(d.key), "{} is not in doc 05", d.key);
    }
}
