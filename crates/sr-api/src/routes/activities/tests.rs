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
        connect: vec![],
    }
}

fn dev_spec() -> ServeSpec {
    ServeSpec {
        serving: Serving::Dev {
            url: "https://dev.example.com/".to_owned(),
        },
        url_mappings: mappings(),
        connect: vec![],
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
    // ARC-SBX-001: the server enforces the sandbox itself, and a bundle that declared no hosts may connect nowhere.
    assert!(
        csp.starts_with("sandbox allow-scripts allow-pointer-lock;"),
        "{csp}"
    );
    assert!(!csp.contains("allow-same-origin"), "{csp}");
    assert!(csp.contains("script-src 'self'"), "{csp}");
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

fn csp_of(h: &HeaderMap) -> String {
    h[header::CONTENT_SECURITY_POLICY]
        .to_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn the_sandbox_connects_nowhere_unless_the_reviewed_manifest_named_hosts() {
    let (d, _) = rig(true).await;
    let mut spec = bundle_spec();
    spec.url_mappings = vec![];
    let (_, h, _) = text(proxy::serve(&d, APP, &spec, "/app.js").await).await;
    assert!(csp_of(&h).contains("connect-src 'none';"), "{}", csp_of(&h));
    // Module scripts and fonts of an opaque-origin page are CORS fetches.
    assert_eq!(h[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");

    // Declared hosts are named and "self" is honoured. CSP is per host: the whole host is admitted.
    spec.connect = vec!["https://assets.example.com".to_owned(), "self".to_owned()];
    let (_, h, _) = text(proxy::serve(&d, APP, &spec, "/app.js").await).await;
    assert!(
        csp_of(&h).contains("connect-src https://assets.example.com 'self';"),
        "{}",
        csp_of(&h)
    );
    // D-399: reviewed URL mappings are the game's own pipe to its developer's server, reached through 'self'.
    let (_, h, _) = text(proxy::serve(&d, APP, &bundle_spec(), "/app.js").await).await;
    assert!(csp_of(&h).contains("connect-src 'self';"), "{}", csp_of(&h));

    // Error pages never get more than the strictest policy.
    let (s, h, _) = text(proxy::serve(&d, APP, &bundle_spec(), "/nope.js").await).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert!(csp_of(&h).contains("connect-src 'none';"));
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
        connect: vec![],
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
    // The sandboxed game's GET of its own mapping is a CORS fetch without credentials (D-399).
    assert_eq!(h[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");
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
        // A sandboxed frame without allow-same-origin: an opaque origin is never the Activity's own.
        "null",
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

/// D-399: `Origin: null` is what any sandboxed page sends, so without the instance's proxy token (or with
/// something that cannot be one) the write is refused before any lookup, and the refusal is not readable.
#[tokio::test]
async fn an_opaque_origin_without_a_proxy_token_cannot_write() {
    let (d, f) = rig(true).await;
    let spec = bundle_spec();
    let not_hex = "z".repeat(64);
    for token in [None, Some(""), Some("abc"), Some(not_hex.as_str())] {
        let mut h = origin_headers(Some("null"), Some("application/json"));
        if let Some(t) = token {
            h.insert(proxy::TOKEN_HEADER, HeaderValue::from_str(t).unwrap());
        }
        let r =
            proxy::forward_write(&d, APP, &spec, &Method::POST, &h, "/api/x", Body::empty()).await;
        let (s, h, _) = text(r).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{token:?}");
        assert!(h.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
    }
    assert!(f.sent.lock().unwrap().is_empty());
}

/// D-399: the CORS preflight of the sandboxed game's write admits the token header, under a mapping and for
/// `Origin: null` only.
#[tokio::test]
async fn the_preflight_admits_the_token_header_under_a_mapping_only() {
    let (d, _) = rig(true).await;
    let spec = bundle_spec();
    let r = proxy::preflight_answer(&d, &spec, &origin_headers(Some("null"), None), "/api/rooms");
    let (s, h, body) = text(r).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert!(body.is_empty());
    assert_eq!(h[header::ACCESS_CONTROL_ALLOW_ORIGIN], "null");
    assert!(
        h[header::ACCESS_CONTROL_ALLOW_HEADERS]
            .to_str()
            .unwrap()
            .contains(proxy::TOKEN_HEADER)
    );
    assert_hardened(&h);
    for (origin, path) in [
        (Some("null"), "/app.js"),
        (Some("null"), "/"),
        (Some("null"), "/api/../x"),
        (Some("https://evil.example"), "/api/rooms"),
        (Some(OWN_ORIGIN), "/api/rooms"),
        (None, "/api/rooms"),
    ] {
        let r = proxy::preflight_answer(&d, &spec, &origin_headers(origin, None), path);
        let (s, h, _) = text(r).await;
        assert_eq!(s, StatusCode::METHOD_NOT_ALLOWED, "{origin:?} {path}");
        assert!(h.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
    }
}

/// D-399 over real tables: a game with a URL mapping POSTs through it with its instance's proxy token; a page
/// without the token, with a stale one or with one of another application cannot.
/// `SR_TEST_DATABASE_URL=... cargo test -p sr-api --lib routes::activities::tests -- --ignored --test-threads=1`
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_game_with_a_url_mapping_posts_through_it_with_its_proxy_token() {
    let db = sr_db::testing::TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let seq = std::sync::atomic::AtomicI64::new(0);
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|t| t.as_micros() as i64)
        .unwrap_or(1);
    let next = || base * 1000 + seq.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let account = async |name: &str| {
        let id = next();
        let n = format!("{name}{id}");
        assert!(
            sr_db::accounts::create_account(
                &pool,
                &sr_db::accounts::NewAccount {
                    id,
                    person_id: next(),
                    username: &n,
                    display_name: None,
                    email: &format!("{n}@example.test"),
                    password_hash: "x",
                    state: "active",
                    email_verified: true,
                    initial_credits: 0,
                    flags: 0,
                },
                next(),
            )
            .await
            .unwrap()
        );
        id
    };
    let player = account("player").await;
    let stranger = account("stranger").await;
    let owner = account("owner").await;
    let bot = account("bot").await;
    let (app, other_app, instance) = (next(), next(), next());
    for (id, bot_id) in [(app, bot), (other_app, account("bot").await)] {
        sqlx::query(
            "INSERT INTO applications (id, owner_id, bot_user_id, name, client_secret_hash) \
             VALUES ($1, $2, $3, 'Mapped game', decode(repeat('ab', 32), 'hex'))",
        )
        .bind(id)
        .bind(owner)
        .bind(bot_id)
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO activity_instances (id, application_id, guild_id, channel_id, channel_type, started_by) \
         VALUES ($1, $2, 9, 3, 'text', $3)",
    )
    .bind(instance)
    .bind(app)
    .bind(player)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO activity_participants (instance_id, user_id) VALUES ($1, $2)")
        .bind(instance)
        .bind(player)
        .execute(&pool)
        .await
        .unwrap();
    let storage: Arc<dyn ObjectStorage> = Arc::new(ObjectStoreStorage::in_memory());
    let f = Arc::new(FakeFetcher::default());
    let d = Arc::new(ActivitiesDeps::new(
        ActivityService::new(pool.clone(), Arc::new(|| 1)).with_storage(storage),
        true,
        "https://chat.example.org",
        Some(f.clone()),
    ));
    let spec = bundle_spec();
    let token = d.service.mint_proxy_token(player, instance).await.unwrap();
    assert!(
        d.service
            .mint_proxy_token(stranger, instance)
            .await
            .is_err()
    );
    let post = async |app: i64, token: Option<&str>| {
        let mut h = origin_headers(Some("null"), Some("application/json"));
        if let Some(t) = token {
            h.insert(proxy::TOKEN_HEADER, HeaderValue::from_str(t).unwrap());
        }
        text(
            proxy::forward_write(
                &d,
                app,
                &spec,
                &Method::POST,
                &h,
                "/api/scores",
                Body::from("{\"score\":7}"),
            )
            .await,
        )
        .await
    };

    // The game, with its token: forwarded, and the answer is readable by the opaque origin.
    let (s, h, body) = post(app, Some(&token)).await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(body, "{\"ok\":true}");
    assert_eq!(h[header::ACCESS_CONTROL_ALLOW_ORIGIN], "null");
    assert_hardened(&h);
    {
        let sent = f.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].1, "https://api.example.com/v1/scores");
        assert_eq!(sent[0].3, b"{\"score\":7}");
    }

    // A page without the token, with a made-up one, or with this token on another application: refused.
    let made_up = "ab".repeat(32);
    for (a, t) in [
        (app, None),
        (app, Some(made_up.as_str())),
        (other_app, Some(token.as_str())),
    ] {
        let (s, h, _) = post(a, t).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{a} {t:?}");
        assert!(h.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
    }
    // Once the player has left, the token is stale.
    d.service.leave(player, instance).await.unwrap();
    let (s, _, _) = post(app, Some(&token)).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(
        f.sent.lock().unwrap().len(),
        1,
        "refused writes are never forwarded"
    );
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

/// SEC-ENT-003 (docs/dev/entrance-isolation.md section 6): the activity host serves a third party's reviewed
/// document and nothing of ours, so no response from it (document, script, refusal, headers) ever names `/app/`,
/// `/entrance/` or the app shell, whatever path was asked for.
#[tokio::test]
async fn a_response_never_references_the_app_or_the_entrance() {
    let (d, _f) = rig(true).await;
    let bundle = bundle_spec();
    let dev = dev_spec();
    let mut seen = 0;
    for spec in [&bundle, &dev] {
        for path in [
            "/",
            "/app.js",
            "/sub/",
            "/img/logo.png",
            "/app/x.js",
            "/entrance/index.js",
            "/app.html",
            "/api/x",
            "/nope.js",
            "/../x.js",
        ] {
            let (s, h, body) = text(proxy::serve(&d, APP, spec, path).await).await;
            seen += 1;
            let mut all = body;
            for (k, v) in &h {
                all.push_str(&format!("\n{k}: {}", v.to_str().unwrap_or("")));
            }
            for forbidden in ["/app/", "/entrance/", "app.html", "csp.txt"] {
                // The request path itself may echo into a refusal; a body that merely repeats a path the
                // caller asked for is not a reference to our files, so only the answers to ordinary paths count.
                if path.contains(forbidden.trim_matches('/')) {
                    continue;
                }
                assert!(
                    !all.contains(forbidden),
                    "{path} ({s}) mentions {forbidden}: {all}"
                );
            }
        }
    }
    assert!(seen > 0);
}
