//! Tests of the Activities HTTP layer that need no database: the proxy's response rules with a fake
//! fetcher and an in-memory bundle store, the host layer's refusals (which are answered before any query), and
//! authentication. Behaviour over real tables is covered by `sr-activities/tests/db.rs`.

use super::{ActivitiesDeps, ActivityFetcher, Fetched, Forwarded, proxy};
use crate::{ApiSettings, AppState, auth::memory::MemorySessions, build_router};
use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    http::{HeaderMap, HeaderValue, Method, Request, StatusCode, header},
    routing::get,
};
use bytes::Bytes;
use http_body_util::BodyExt;
use sr_activities::{ActivityService, ServeSpec, Serving, model::UrlMapping, service::object_key};
use sr_storage::{ObjectStorage, ObjectStoreStorage};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const APP: i64 = 77;

type Sent = (String, String, Option<String>, Vec<u8>);

#[derive(Default)]
struct FakeFetcher {
    asked: Mutex<Vec<String>>,
    /// `(method, url, content type, body)` of every forwarded write.
    sent: Mutex<Vec<Sent>>,
}

#[async_trait]
impl ActivityFetcher for FakeFetcher {
    async fn get(&self, url: &str, _max: usize) -> Result<Fetched, String> {
        self.asked.lock().unwrap().push(url.to_owned());
        if url.contains("down.example") {
            return Err("the connection failed".to_owned());
        }
        let (ct, body): (&str, &[u8]) = if url == "https://dev.example.com/" {
            ("text/html", b"<h1>live</h1>")
        } else {
            ("text/javascript", b"console.log(1)")
        };
        Ok(Fetched {
            content_type: Some(ct.to_owned()),
            body: body.to_vec(),
        })
    }

    async fn forward(
        &self,
        method: &Method,
        url: &str,
        content_type: Option<String>,
        body: Vec<u8>,
        _max: usize,
    ) -> Result<Forwarded, String> {
        self.sent
            .lock()
            .unwrap()
            .push((method.to_string(), url.to_owned(), content_type, body));
        let status = if url.contains("/boom") {
            500
        } else if url.contains("/missing") {
            404
        } else if url.contains("/moved") {
            302
        } else {
            201
        };
        Ok(Forwarded {
            status,
            content_type: Some("application/json".to_owned()),
            body: b"{\"ok\":true}".to_vec(),
        })
    }
}

fn lazy_service(storage: Arc<dyn ObjectStorage>) -> ActivityService {
    // A pool that never connects: every request below is answered before the first query.
    let pool = sqlx::postgres::PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_millis(100))
        .connect_lazy("postgres://nobody@127.0.0.1:1/none")
        .unwrap();
    ActivityService::new(pool, Arc::new(|| 1)).with_storage(storage)
}

async fn rig(enabled: bool) -> (Arc<ActivitiesDeps>, Arc<FakeFetcher>) {
    let storage: Arc<dyn ObjectStorage> = Arc::new(ObjectStoreStorage::in_memory());
    for (path, data) in [
        (
            "index.html",
            "<!doctype html><script src=\"/app.js\"></script>",
        ),
        ("app.js", "console.log('bundle')"),
        ("sub/index.html", "<p>sub</p>"),
        ("img/logo.png", "png"),
        ("data.exe", "MZ"),
    ] {
        storage
            .put(&object_key(APP, SHA, path), Bytes::from(data))
            .await
            .unwrap();
    }
    let f = Arc::new(FakeFetcher::default());
    let d = Arc::new(ActivitiesDeps::new(
        lazy_service(storage),
        enabled,
        "https://chat.example.org",
        Some(f.clone()),
    ));
    (d, f)
}

fn mappings() -> Vec<UrlMapping> {
    vec![UrlMapping {
        prefix: "/api".into(),
        target: "https://api.example.com/v1".into(),
    }]
}

fn bundle_spec() -> ServeSpec {
    ServeSpec {
        serving: Serving::Bundle {
            sha256: SHA.to_owned(),
        },
        url_mappings: mappings(),
    }
}

fn dev_spec() -> ServeSpec {
    ServeSpec {
        serving: Serving::Dev {
            url: "https://dev.example.com/".to_owned(),
        },
        url_mappings: mappings(),
    }
}

async fn text(r: axum::response::Response) -> (StatusCode, HeaderMap, String) {
    let (p, b) = r.into_parts();
    let bytes = b.collect().await.unwrap().to_bytes();
    (
        p.status,
        p.headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

fn assert_hardened(h: &HeaderMap) {
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
}

#[tokio::test]
async fn the_reviewed_bundle_is_served_from_storage_with_the_hardened_headers() {
    let (d, f) = rig(true).await;
    let spec = bundle_spec();
    let (s, h, body) = text(proxy::serve(&d, APP, &spec, "/").await).await;
    assert_eq!(s, StatusCode::OK);
    assert!(body.contains("/app.js"));
    assert_eq!(h[header::CONTENT_TYPE], "text/html; charset=utf-8");
    assert_eq!(h[header::CACHE_CONTROL], "no-cache");
    assert_hardened(&h);
    assert!(h.get(proxy::MODE_HEADER).is_none());

    let (s, h, body) = text(proxy::serve(&d, APP, &spec, "/app.js?v=3").await).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body, "console.log('bundle')");
    assert_eq!(h[header::CONTENT_TYPE], "text/javascript; charset=utf-8");
    assert_eq!(h[header::CACHE_CONTROL], "public, max-age=3600");
    assert_hardened(&h);

    let (s, _, body) = text(proxy::serve(&d, APP, &spec, "/sub/").await).await;
    assert_eq!((s, body.as_str()), (StatusCode::OK, "<p>sub</p>"));
    let (_, h, _) = text(proxy::serve(&d, APP, &spec, "/sub/").await).await;
    assert_eq!(h[header::CACHE_CONTROL], "no-cache");
    let (_, h, _) = text(proxy::serve(&d, APP, &spec, "/img/logo.png").await).await;
    assert_eq!(h[header::CONTENT_TYPE], "image/png");

    // Nothing is fetched from anywhere for a bundle path.
    assert!(f.asked.lock().unwrap().is_empty());
}

#[tokio::test]
async fn bundle_paths_never_leave_the_bundle_and_unknown_files_are_404() {
    let (d, f) = rig(true).await;
    let spec = bundle_spec();
    for bad in [
        "/../x.js",
        "/a/../app.js",
        "/%2e%2e/app.js",
        "/a%2Fb.js",
        "//app.js",
        "/.git/config.txt",
        "/a\\b.js",
        "/sp ace.js",
        "/app.js%00.png",
    ] {
        let (s, _, _) = text(proxy::serve(&d, APP, &spec, bad).await).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad}");
    }
    for missing in ["/nope.js", "/img/", "/data.exe", "/noext", "/sub/app.js"] {
        let (s, _, _) = text(proxy::serve(&d, APP, &spec, missing).await).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{missing}");
    }
    // A file that is in storage but not in an allowed type is still not served.
    // A hash that is not in storage (a draft, a superseded bundle) serves nothing.
    let other = ServeSpec {
        serving: Serving::Bundle {
            sha256: "f".repeat(64),
        },
        url_mappings: vec![],
    };
    let (s, _, _) = text(proxy::serve(&d, APP, &other, "/").await).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // Another application's bundle is not reachable through this one's hash.
    let (s, _, _) = text(proxy::serve(&d, APP + 1, &spec, "/").await).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert!(f.asked.lock().unwrap().is_empty());
}

#[tokio::test]
async fn mapped_paths_go_to_the_target_and_an_unreachable_server_is_a_502() {
    let (d, f) = rig(true).await;
    let spec = bundle_spec();
    let (s, h, _) = text(proxy::serve(&d, APP, &spec, "/api/rooms?x=1").await).await;
    assert_eq!(s, StatusCode::OK);
    assert_hardened(&h);
    assert_eq!(
        *f.asked.lock().unwrap(),
        vec!["https://api.example.com/v1/rooms?x=1"]
    );
    for bad in ["/api/../secret", "/api/%2e%2e/b", "/api%2Fx"] {
        let (s, _, _) = text(proxy::serve(&d, APP, &spec, bad).await).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad}");
    }
    assert_eq!(
        f.asked.lock().unwrap().len(),
        1,
        "refused paths are never fetched"
    );
    let mut down = bundle_spec();
    down.url_mappings[0].target = "https://down.example".into();
    let (s, _, body) = text(proxy::serve(&d, APP, &down, "/api/x").await).await;
    assert_eq!(s, StatusCode::BAD_GATEWAY);
    assert!(!body.contains("down.example"));
}

#[tokio::test]
async fn the_live_development_url_is_fetched_and_marked() {
    let (d, f) = rig(true).await;
    let spec = dev_spec();
    let (s, h, body) = text(proxy::serve(&d, APP, &spec, "/").await).await;
    assert_eq!((s, body.as_str()), (StatusCode::OK, "<h1>live</h1>"));
    assert_eq!(h[proxy::MODE_HEADER], "development");
    assert_hardened(&h);
    let (_, h, _) = text(proxy::serve(&d, APP, &spec, "/main.js").await).await;
    assert_eq!(h[proxy::MODE_HEADER], "development");
    let (_, h, _) = text(proxy::serve(&d, APP, &spec, "/api/x").await).await;
    assert_eq!(h[proxy::MODE_HEADER], "development");
    assert_eq!(
        *f.asked.lock().unwrap(),
        vec![
            "https://dev.example.com/",
            "https://dev.example.com/main.js",
            "https://api.example.com/v1/x"
        ]
    );
    let (s, _, _) = text(proxy::serve(&d, APP, &spec, "/a/../x").await).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

fn origin_headers(origin: Option<&str>, ct: Option<&str>) -> HeaderMap {
    let mut h = HeaderMap::new();
    if let Some(o) = origin {
        h.insert(header::ORIGIN, HeaderValue::from_str(o).unwrap());
    }
    if let Some(c) = ct {
        h.insert(header::CONTENT_TYPE, HeaderValue::from_str(c).unwrap());
    }
    // Things that must never reach the application.
    h.insert(header::COOKIE, HeaderValue::from_static("session=secret"));
    h.insert(
        header::AUTHORIZATION,
        HeaderValue::from_static("Bearer secret"),
    );
    h
}

const OWN_ORIGIN: &str = "https://77.activities.chat.example.org";

#[tokio::test]
async fn writes_under_a_mapping_are_forwarded_with_only_the_body_and_content_type() {
    let (d, f) = rig(true).await;
    let spec = bundle_spec();
    for m in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
        let r = proxy::forward_write(
            &d,
            APP,
            &spec,
            &m,
            &origin_headers(Some(OWN_ORIGIN), Some("application/json")),
            "/api/rooms/1?x=2",
            Body::from("{\"a\":1}"),
        )
        .await;
        let (s, h, body) = text(r).await;
        assert_eq!(s, StatusCode::CREATED, "{m}");
        assert_eq!(body, "{\"ok\":true}");
        assert_eq!(h[header::CONTENT_TYPE], "application/json");
        assert_hardened(&h);
    }
    let sent = f.sent.lock().unwrap();
    assert_eq!(sent.len(), 4);
    assert_eq!(sent[0].0, "POST");
    assert_eq!(sent[3].0, "DELETE");
    assert_eq!(sent[0].1, "https://api.example.com/v1/rooms/1?x=2");
    assert_eq!(sent[0].2.as_deref(), Some("application/json"));
    assert_eq!(sent[0].3, b"{\"a\":1}");
    // The fetcher's signature has no place for a cookie or an authorization header: nothing else is sent.
}

#[tokio::test]
async fn writes_outside_the_mappings_are_405_and_never_fetched() {
    let (d, f) = rig(true).await;
    for (m, path) in [
        (Method::POST, "/"),
        (Method::POST, "/app.js"),
        (Method::PUT, "/index.html"),
        (Method::DELETE, "/apix"),
        (Method::PATCH, "/sub/"),
    ] {
        let r = proxy::forward_write(
            &d,
            APP,
            &bundle_spec(),
            &m,
            &origin_headers(Some(OWN_ORIGIN), None),
            path,
            Body::empty(),
        )
        .await;
        let (s, h, _) = text(r).await;
        assert_eq!(s, StatusCode::METHOD_NOT_ALLOWED, "{m} {path}");
        assert_eq!(h[header::ALLOW], "GET, HEAD");
    }
    // The development URL is not writable either: only mapping targets are.
    let r = proxy::forward_write(
        &d,
        APP,
        &dev_spec(),
        &Method::POST,
        &origin_headers(None, None),
        "/form",
        Body::empty(),
    )
    .await;
    assert_eq!(text(r).await.0, StatusCode::METHOD_NOT_ALLOWED);
    let r = proxy::forward_write(
        &d,
        APP,
        &bundle_spec(),
        &Method::POST,
        &origin_headers(None, None),
        "/api/../x",
        Body::empty(),
    )
    .await;
    assert_eq!(text(r).await.0, StatusCode::BAD_REQUEST);
    assert!(f.sent.lock().unwrap().is_empty());
}

#[tokio::test]
async fn writes_are_limited_to_one_mebibyte_and_to_the_activitys_own_origin() {
    let (d, f) = rig(true).await;
    let spec = bundle_spec();
    // Exactly 1 MiB passes; one byte more is a 413, with or without a Content-Length.
    let ok = vec![b'x'; proxy::MAX_WRITE_BODY];
    let r = proxy::forward_write(
        &d,
        APP,
        &spec,
        &Method::POST,
        &origin_headers(None, Some("text/plain")),
        "/api/up",
        Body::from(ok),
    )
    .await;
    assert_eq!(text(r).await.0, StatusCode::CREATED);
    let big = vec![b'x'; proxy::MAX_WRITE_BODY + 1];
    let r = proxy::forward_write(
        &d,
        APP,
        &spec,
        &Method::POST,
        &origin_headers(None, None),
        "/api/up",
        Body::from(big.clone()),
    )
    .await;
    assert_eq!(text(r).await.0, StatusCode::PAYLOAD_TOO_LARGE);
    let mut h = origin_headers(None, None);
    h.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&big.len().to_string()).unwrap(),
    );
    let r = proxy::forward_write(&d, APP, &spec, &Method::POST, &h, "/api/up", Body::empty()).await;
    assert_eq!(text(r).await.0, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(f.sent.lock().unwrap().len(), 1);

    // A page of another origin cannot write through this Activity's host.
    for origin in [
        "https://evil.example",
        "https://chat.example.org",
        "https://78.activities.chat.example.org",
    ] {
        let r = proxy::forward_write(
            &d,
            APP,
            &spec,
            &Method::POST,
            &origin_headers(Some(origin), None),
            "/api/x",
            Body::empty(),
        )
        .await;
        assert_eq!(text(r).await.0, StatusCode::FORBIDDEN, "{origin}");
    }
    assert_eq!(f.sent.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn upstream_answers_to_writes_are_relayed_only_when_they_are_success_or_client_errors() {
    let (d, _) = rig(true).await;
    let spec = bundle_spec();
    for (path, want) in [
        ("/api/ok", StatusCode::CREATED),
        ("/api/missing", StatusCode::NOT_FOUND),
        ("/api/boom", StatusCode::BAD_GATEWAY),
        ("/api/moved", StatusCode::BAD_GATEWAY),
    ] {
        let r = proxy::forward_write(
            &d,
            APP,
            &spec,
            &Method::POST,
            &origin_headers(None, None),
            path,
            Body::empty(),
        )
        .await;
        let (s, h, body) = text(r).await;
        assert_eq!(s, want, "{path}");
        assert!(h.get(header::LOCATION).is_none());
        if want == StatusCode::BAD_GATEWAY {
            assert!(!body.contains("ok"), "{path}");
        }
    }
    // The development URL's mapped writes carry the marker.
    let r = proxy::forward_write(
        &d,
        APP,
        &dev_spec(),
        &Method::POST,
        &origin_headers(None, None),
        "/api/ok",
        Body::empty(),
    )
    .await;
    assert_eq!(text(r).await.2, "{\"ok\":true}");
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
    // Off: the activity hosts are 404 for every method, the app shell host is untouched.
    let off = app_with_layer(rig(false).await.0);
    for m in [Method::GET, Method::POST, Method::DELETE] {
        assert_eq!(
            status_of(&off, "77.activities.chat.example.org", m, "/").await,
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(
        status_of(&off, "chat.example.org", Method::GET, "/shell").await,
        StatusCode::OK
    );

    let on = app_with_layer(rig(true).await.0);
    assert_eq!(
        status_of(&on, "chat.example.org", Method::GET, "/shell").await,
        StatusCode::OK
    );
    // A name that is not an application id is refused before any lookup, whatever the method.
    for m in [Method::GET, Method::POST] {
        assert_eq!(
            status_of(&on, "nope.activities.chat.example.org", m, "/").await,
            StatusCode::NOT_FOUND
        );
    }
    // Methods that are never forwarded are refused before any lookup.
    for m in [Method::OPTIONS, Method::TRACE, Method::CONNECT] {
        assert_eq!(
            status_of(&on, "77.activities.chat.example.org", m, "/").await,
            StatusCode::METHOD_NOT_ALLOWED
        );
    }
}

#[tokio::test]
async fn member_routes_need_a_session() {
    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let state = AppState::builder()
        .settings(settings)
        .sessions(Arc::new(MemorySessions::new()))
        .extension(rig(true).await.0)
        .build();
    let app = build_router(state, super::router());
    for (m, uri) in [
        (Method::GET, "/activities"),
        (Method::GET, "/channels/1/activity"),
        (Method::POST, "/channels/1/activity"),
        (Method::POST, "/activity-instances/1/join"),
        (Method::GET, "/applications/1/activity"),
        (Method::POST, "/applications/1/activity/bundles"),
        (Method::GET, "/applications/1/activity/bundles/2"),
        (Method::POST, "/applications/1/activity/submit"),
        (Method::GET, "/admin/developers/activities"),
        (
            Method::GET,
            "/admin/developers/activities/1/bundles/2/download",
        ),
        (Method::POST, "/admin/developers/activities/1/decision"),
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
    let storage: Arc<dyn ObjectStorage> = Arc::new(ObjectStoreStorage::in_memory());
    let d = ActivitiesDeps::new(lazy_service(storage), true, "http://localhost:8080", None);
    assert_eq!(d.base_host, "localhost:8080");
    assert_eq!(d.app_origin, "http://localhost:8080");
    assert_eq!(d.activity_origin(5), "http://5.activities.localhost:8080");
}
