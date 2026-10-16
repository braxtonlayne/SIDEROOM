//! Tests of the Activities HTTP layer that need no database: the proxy's response rules with a fake
//! fetcher, the host layer's refusals (which are answered before any query), and authentication. Behaviour
//! over real tables is covered by `sr-activities/tests/db.rs`.

use super::{ActivitiesDeps, ActivityFetcher, Fetched, proxy};
use crate::{ApiSettings, AppState, auth::memory::MemorySessions, build_router};
use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
    routing::get,
};
use data_encoding::HEXLOWER;
use http_body_util::BodyExt;
use sha2::{Digest, Sha256};
use sr_activities::{
    ActivityService,
    model::{ConfigView, ReviewState, UrlMapping},
};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

const ENTRY_HTML: &[u8] = b"<!doctype html><script src=\"/main.js\"></script>";

#[derive(Default)]
struct FakeFetcher {
    asked: Mutex<Vec<String>>,
}

#[async_trait]
impl ActivityFetcher for FakeFetcher {
    async fn get(&self, url: &str, _max: usize) -> Result<Fetched, String> {
        self.asked.lock().unwrap().push(url.to_owned());
        if url.contains("down.example") {
            return Err("the connection failed".to_owned());
        }
        let (ct, body): (&str, &[u8]) = if url == "https://app.example.com/index.html" {
            ("text/html", ENTRY_HTML)
        } else {
            ("text/javascript", b"console.log(1)")
        };
        Ok(Fetched {
            content_type: Some(ct.to_owned()),
            body: body.to_vec(),
        })
    }
}

fn lazy_service() -> ActivityService {
    // A pool that never connects: every request below is answered before the first query.
    let pool = sqlx::postgres::PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_millis(100))
        .connect_lazy("postgres://nobody@127.0.0.1:1/none")
        .unwrap();
    ActivityService::new(pool, Arc::new(|| 1))
}

fn deps(enabled: bool, fetcher: Arc<FakeFetcher>) -> Arc<ActivitiesDeps> {
    Arc::new(ActivitiesDeps::new(
        lazy_service(),
        enabled,
        "https://chat.example.org",
        Some(fetcher),
    ))
}

fn cfg(sha: &str) -> ConfigView {
    ConfigView {
        application_id: "77".into(),
        entry_url: Some("https://app.example.com/index.html".into()),
        url_mappings: vec![UrlMapping {
            prefix: "/api".into(),
            target: "https://api.example.com/v1".into(),
        }],
        review_state: ReviewState::Approved,
        pinned_sha256: Some(sha.into()),
        reviewed_sha256: Some(sha.into()),
        review_note: None,
        updated_at_ms: 0,
    }
}

fn sha(b: &[u8]) -> String {
    HEXLOWER.encode(Sha256::digest(b).as_slice())
}

async fn text(r: axum::response::Response) -> (StatusCode, axum::http::HeaderMap, String) {
    let (p, b) = r.into_parts();
    let bytes = b.collect().await.unwrap().to_bytes();
    (
        p.status,
        p.headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

#[tokio::test]
async fn the_entry_document_is_served_only_when_it_matches_the_reviewed_hash() {
    let f = Arc::new(FakeFetcher::default());
    let d = deps(true, f.clone());
    let ok = proxy::serve(&d, &cfg(&sha(ENTRY_HTML)), "/").await;
    let (status, h, body) = text(ok).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_bytes(), ENTRY_HTML);
    assert_eq!(h[header::CONTENT_TYPE], "text/html");
    let csp = h[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
    assert!(
        csp.contains("frame-ancestors https://chat.example.org"),
        "{csp}"
    );
    assert!(csp.contains("script-src 'self'") && csp.contains("connect-src 'self'"));
    assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert_eq!(h[header::REFERRER_POLICY], "no-referrer");
    assert!(
        h["permissions-policy"]
            .to_str()
            .unwrap()
            .contains("camera=()")
    );
    assert!(h.get(header::SET_COOKIE).is_none());

    // The document changed after review: paused, not served.
    let changed = proxy::serve(&d, &cfg(&sha(b"something else")), "/").await;
    let (status, _, body) = text(changed).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(body.contains("reviewed again") && !body.contains("<script"));
}

#[tokio::test]
async fn paths_follow_the_url_mappings_and_never_escape() {
    let f = Arc::new(FakeFetcher::default());
    let d = deps(true, f.clone());
    let c = cfg(&sha(ENTRY_HTML));
    let (s, h, _) = text(proxy::serve(&d, &c, "/main.js").await).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(h[header::CONTENT_TYPE], "text/javascript");
    let (s, _, _) = text(proxy::serve(&d, &c, "/api/rooms?x=1").await).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        *f.asked.lock().unwrap(),
        vec![
            "https://app.example.com/main.js",
            "https://api.example.com/v1/rooms?x=1"
        ]
    );
    for bad in ["/api/../secret", "/a/%2e%2e/b", "/x%2Fy"] {
        let (s, _, _) = text(proxy::serve(&d, &c, bad).await).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad}");
    }
    assert_eq!(
        f.asked.lock().unwrap().len(),
        2,
        "refused paths are never fetched"
    );

    // An unreachable application server is a 502, not a leak of the reason.
    let mut down = cfg(&sha(ENTRY_HTML));
    down.url_mappings[0].target = "https://down.example".into();
    let (s, _, body) = text(proxy::serve(&d, &down, "/api/x").await).await;
    assert_eq!(s, StatusCode::BAD_GATEWAY);
    assert!(!body.contains("down.example"));
}

fn app_with_layer(d: Arc<ActivitiesDeps>) -> Router {
    let inner = Router::new().route("/shell", get(|| async { "shell" }));
    proxy::wrap_activity_host(inner, d)
}

async fn status_of(app: &Router, host: &str, method: Method, uri: &str) -> StatusCode {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, host)
        .body(Body::empty())
        .unwrap();
    app.clone().oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn the_host_layer_answers_only_activity_hosts() {
    // Off: the activity hosts are 404, the app shell host is untouched.
    let off = app_with_layer(deps(false, Arc::default()));
    assert_eq!(
        status_of(&off, "77.activities.chat.example.org", Method::GET, "/").await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        status_of(&off, "chat.example.org", Method::GET, "/shell").await,
        StatusCode::OK
    );

    let on = app_with_layer(deps(true, Arc::default()));
    assert_eq!(
        status_of(&on, "chat.example.org", Method::GET, "/shell").await,
        StatusCode::OK
    );
    // A name that is not an application id, and a write method, are refused before any lookup.
    assert_eq!(
        status_of(&on, "nope.activities.chat.example.org", Method::GET, "/").await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        status_of(&on, "77.activities.chat.example.org", Method::POST, "/").await,
        StatusCode::METHOD_NOT_ALLOWED
    );
}

#[tokio::test]
async fn member_routes_need_a_session() {
    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let state = AppState::builder()
        .settings(settings)
        .sessions(Arc::new(MemorySessions::new()))
        .extension(deps(true, Arc::default()))
        .build();
    let app = build_router(state, super::router());
    for (m, uri) in [
        (Method::GET, "/activities"),
        (Method::GET, "/channels/1/activity"),
        (Method::POST, "/channels/1/activity"),
        (Method::POST, "/activity-instances/1/join"),
        (Method::GET, "/applications/1/activity"),
        (Method::GET, "/admin/developers/activities"),
    ] {
        let mut req = Request::builder()
            .method(m.clone())
            .uri(format!("/api/v1{uri}"))
            .header("origin", "http://localhost:8080");
        if m == Method::POST {
            req = req.header(header::CONTENT_TYPE, "application/json");
        }
        let r = app
            .clone()
            .oneshot(req.body(Body::from("{}")).unwrap())
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED, "{uri}");
    }
}

#[tokio::test]
async fn hosts_and_origins_come_from_the_public_url() {
    let d = ActivitiesDeps::new(lazy_service(), true, "http://localhost:8080", None);
    assert_eq!(d.base_host, "localhost:8080");
    assert_eq!(d.app_origin, "http://localhost:8080");
    assert_eq!(d.activity_origin(5), "http://5.activities.localhost:8080");
}
