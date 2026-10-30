//! SEC-ENT-003 (D-385): the gate in front of the built web app, proven line by line against the table of
//! docs/dev/entrance-isolation.md section 3. A temporary web root holds fake shells and assets, `app/csp.txt`, a
//! `surfaces.json` and a legal template; the real `sr_api::build_router` carries the API.
//!
//! Needs no database: the sessions are in memory.

use std::{path::Path, sync::Arc};

use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Method, Request, StatusCode, header},
};
use http_body_util::BodyExt;
use sr_api::{
    AppState,
    auth::{
        BearerIdentity, BotIdentity,
        memory::{MemoryBearers, MemoryBots, MemorySessions},
    },
    build_router,
};
use sr_auth::{
    repo::{SessionRecord, SessionRepository},
    session::SessionToken,
};
use sr_core::{Clock, ManualClock};
use sr_server::web::{NoOperator, OperatorInfo, PUBLIC_CSP, WebError, WebGate};
use tower::ServiceExt;

const NOW_MS: i64 = 1_800_000_000_000;
const APP_CSP: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; frame-src 'self'";
const HASHED_JS: &str = "/app/assets/index-1a2b3c4d.js";

const SURFACES: &str = r#"{
  "version": 1,
  "appShell": "app.html",
  "appPrefix": "/app",
  "surfaces": [
    { "surface": "landing", "kind": "bundle", "shell": "index.html", "paths": ["/", "/join/*"] },
    { "surface": "auth", "kind": "bundle", "shell": "auth.html",
      "paths": ["/login", "/login/*", "/register", "/forgot", "/reset/*", "/verify-email/*", "/email-change/cancel", "/oauth/discord/callback"] },
    { "surface": "invite", "kind": "bundle", "shell": "invite.html", "paths": ["/invite/*", "/template/*", "/no-invitation"] },
    { "surface": "profile", "kind": "bundle", "shell": "profile.html", "paths": ["/~*"], "reserved": true },
    { "surface": "legal", "kind": "html", "paths": ["/house-notes", "/fonts/README.md", "/THIRD_PARTY_NOTICES.md"] },
    { "surface": "error", "kind": "html", "files": ["404.html", "50x.html", "offline.html"], "paths": [] }
  ],
  "publicFiles": ["/entrance/*", "/fonts/*", "/icons/*", "/sw.js", "/manifest.webmanifest", "/offline.html",
    "/offline.css", "/robots.txt", "/THIRD_PARTY_NOTICES.md"]
}"#;

fn write(root: &Path, rel: &str, body: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

fn web_root() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    write(r, "app.html", "<html>APP-SHELL</html>");
    write(r, "app/csp.txt", &format!("{APP_CSP}\n"));
    write(r, "app/assets/index-1a2b3c4d.js", "console.log('app')");
    write(r, "app/manifest.webmanifest", "{\"shortcuts\":[]}");
    write(r, "app/THIRD_PARTY_NOTICES.md", "full notices");
    write(r, "index.html", "<html>LANDING</html>");
    write(r, "auth.html", "<html>AUTH</html>");
    write(r, "invite.html", "<html>INVITE</html>");
    write(r, "entrance/index-9f8e7d6c.js", "console.log('landing')");
    write(r, "entrance/.vite/manifest.json", "{}");
    write(r, "fonts/README.md", "fonts");
    write(r, "sw.js", "// sw");
    write(r, "robots.txt", "User-agent: *\nDisallow: /\n");
    write(r, "404.html", "<html>404</html>");
    write(r, "surfaces.json", SURFACES);
    write(
        r,
        "legal/house-notes.en.html",
        "<p>EN {{operator_country}} | {{operator_contact}}</p>",
    );
    write(
        r,
        "legal/house-notes.de.html",
        "<p>DE {{operator_country}} | {{operator_contact}}</p>",
    );
    d
}

struct Operator(Option<&'static str>, Option<&'static str>);

#[async_trait]
impl OperatorInfo for Operator {
    async fn operator(&self, _api: &AppState) -> (Option<String>, Option<String>) {
        (self.0.map(str::to_owned), self.1.map(str::to_owned))
    }
}

struct Fx {
    app: Router,
    /// A live session cookie header value.
    cookie: String,
    _root: tempfile::TempDir,
}

async fn session(sessions: &MemorySessions, id: i64, expires_at: i64) -> String {
    let t = SessionToken::generate().unwrap();
    sessions
        .create(SessionRecord {
            id,
            user_id: 5,
            token_hash: t.hash(),
            created_at: 1_799_999_000,
            last_seen_at: 1_799_999_000,
            expires_at,
            device_name: None,
        })
        .await
        .unwrap();
    format!("sr_session={}", t.expose())
}

async fn fixture_with(
    serve_public: bool,
    operator: Arc<dyn OperatorInfo>,
) -> (Fx, Arc<MemorySessions>) {
    let root = web_root();
    let sessions = Arc::new(MemorySessions::new());
    let bots = Arc::new(MemoryBots::new());
    bots.insert(
        "botsecret",
        BotIdentity {
            user_id: 7,
            application_id: 70,
        },
    );
    let bearers = Arc::new(MemoryBearers::new());
    bearers.insert(
        "oauthsecret",
        BearerIdentity {
            user_id: 8,
            scopes: vec!["identify".into()],
            application_id: 0,
            expires_at: i64::MAX,
        },
    );
    let clock = Arc::new(ManualClock::new(NOW_MS));
    let now = clock.now_unix_ms() / 1000;
    let state = AppState::builder()
        .sessions(sessions.clone())
        .bots(bots)
        .bearers(bearers)
        .clock(clock)
        .build();
    let cookie = session(&sessions, 100, now + 3600).await;
    let gate = WebGate::open(root.path(), serve_public, false, operator).unwrap();
    let app = gate.install(build_router(state.clone(), Router::new()), state);
    (
        Fx {
            app,
            cookie,
            _root: root,
        },
        sessions,
    )
}

async fn fixture(serve_public: bool) -> Fx {
    fixture_with(serve_public, Arc::new(NoOperator)).await.0
}

struct Got {
    status: StatusCode,
    headers: HeaderMap,
    body: String,
}

impl Got {
    fn h(&self, name: &str) -> &str {
        self.headers
            .get(name)
            .map(|v| v.to_str().unwrap())
            .unwrap_or("")
    }
}

async fn send(f: &Fx, req: Request<Body>) -> Got {
    let resp = f.app.clone().oneshot(req).await.unwrap();
    let (parts, body) = resp.into_parts();
    let bytes = body.collect().await.unwrap().to_bytes();
    Got {
        status: parts.status,
        headers: parts.headers,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

fn req(method: Method, uri: &str) -> axum::http::request::Builder {
    Request::builder().method(method).uri(uri)
}

fn nav(uri: &str) -> axum::http::request::Builder {
    req(Method::GET, uri)
        .header("sec-fetch-mode", "navigate")
        .header(header::ACCEPT, "text/html,application/xhtml+xml")
}

fn empty(b: axum::http::request::Builder) -> Request<Body> {
    b.body(Body::empty()).unwrap()
}

// ---- the table, anonymous --------------------------------------------------------------------------------

#[tokio::test]
async fn anonymous_navigation_is_a_302_to_login_whether_or_not_the_path_exists() {
    for serve_public in [false, true] {
        let f = fixture(serve_public).await;
        for (path, next) in [
            ("/app", "/app"),
            ("/app/", "/app/"),
            ("/channels", "/channels"),
            ("/admin/ingest-sources", "/admin/ingest-sources"),
            ("/help", "/help"),
            ("/nonexistent-xyz", "/nonexistent-xyz"),
            (
                "/oauth2/authorize?client_id=1&scope=identify",
                "/oauth2/authorize%3Fclient_id%3D1%26scope%3Didentify",
            ),
            ("/share", "/share"),
        ] {
            let r = send(&f, empty(nav(path))).await;
            assert_eq!(r.status, StatusCode::FOUND, "{path}");
            assert_eq!(r.h("location"), format!("/login?next={next}"), "{path}");
            assert_eq!(r.h("cache-control"), "no-store", "{path}");
            assert!(r.body.is_empty(), "{path}");
            assert!(!r.headers.contains_key("www-authenticate"));
        }
    }
}

#[tokio::test]
async fn unsafe_next_values_are_dropped() {
    let f = fixture(false).await;
    for path in ["//evil.example/x", "//evil"] {
        let r = send(&f, empty(nav(path))).await;
        assert_eq!(r.status, StatusCode::FOUND, "{path}");
        assert_eq!(r.h("location"), "/login", "{path}");
    }
    let long = format!("/{}", "a".repeat(3000));
    let r = send(&f, empty(nav(&long))).await;
    assert_eq!(r.status, StatusCode::FOUND);
    assert_eq!(r.h("location"), "/login");
    let r = send(&f, empty(nav("/%0D%0Aset-cookie:x"))).await;
    assert_eq!(r.status, StatusCode::FOUND);
    assert!(r.h("location").starts_with("/login"));
    assert!(!r.h("location").contains('\r') && !r.h("location").contains('\n'));
}

#[tokio::test]
async fn anonymous_app_files_are_an_empty_401_present_or_absent() {
    let f = fixture(false).await;
    let present = send(&f, empty(req(Method::GET, HASHED_JS))).await;
    let absent = send(
        &f,
        empty(req(Method::GET, "/app/assets/nothing-00000000.js")),
    )
    .await;
    let manifest = send(&f, empty(req(Method::GET, "/app/manifest.webmanifest"))).await;
    let traversal = send(&f, empty(req(Method::GET, "/app/..%2f..%2fapp.html"))).await;
    for r in [&present, &absent, &manifest, &traversal] {
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
        assert!(r.body.is_empty());
        assert_eq!(r.h("cache-control"), "no-store");
        assert!(!r.headers.contains_key("www-authenticate"));
        assert!(!r.headers.contains_key("content-type"));
    }
    // The answers are the same in every header, not only the status.
    let names = |g: &Got| {
        let mut n: Vec<_> = g.headers.keys().map(|k| k.to_string()).collect();
        n.sort();
        n
    };
    assert_eq!(names(&present), names(&absent));
    assert_eq!(names(&present), names(&manifest));
    // HEAD is the same as GET.
    let head = send(&f, empty(req(Method::HEAD, HASHED_JS))).await;
    assert_eq!(head.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn anonymous_non_navigation_to_an_unknown_path_is_401_and_other_methods_405() {
    let f = fixture(false).await;
    let r = send(&f, empty(req(Method::GET, "/whatever.json"))).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert!(r.body.is_empty());
    for (m, p) in [
        (Method::POST, "/app"),
        (Method::POST, "/app/x.js"),
        (Method::PUT, "/app/assets/index-1a2b3c4d.js"),
        (Method::DELETE, "/channels"),
        (Method::POST, "/unknown"),
    ] {
        let r = send(&f, empty(req(m.clone(), p))).await;
        assert_eq!(r.status, StatusCode::METHOD_NOT_ALLOWED, "{m} {p}");
        assert!(r.body.is_empty());
    }
}

#[tokio::test]
async fn bot_bearer_expired_and_unknown_sessions_get_the_anonymous_answers() {
    let (f, sessions) = fixture_with(false, Arc::new(NoOperator)).await;
    let expired = session(&sessions, 101, 1_000).await;
    let attempts: Vec<(&str, Vec<(&str, String)>)> = vec![
        ("bot", vec![("authorization", "Bot botsecret".into())]),
        (
            "bearer",
            vec![("authorization", "Bearer oauthsecret".into())],
        ),
        ("expired", vec![("cookie", expired)]),
        (
            "unknown",
            vec![("cookie", format!("sr_session={}", "a".repeat(43)))],
        ),
        ("junk", vec![("cookie", "sr_session=junk".into())]),
    ];
    for (name, headers) in attempts {
        let mut b = nav("/app");
        let mut a = req(Method::GET, HASHED_JS);
        for (k, v) in &headers {
            b = b.header(*k, v);
            a = a.header(*k, v);
        }
        let r = send(&f, empty(b)).await;
        assert_eq!(r.status, StatusCode::FOUND, "{name}");
        assert_eq!(r.h("location"), "/login?next=/app", "{name}");
        let r = send(&f, empty(a)).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{name}");
    }
}

#[tokio::test]
async fn a_revoked_session_stops_working() {
    let (f, sessions) = fixture_with(false, Arc::new(NoOperator)).await;
    let ok = send(&f, empty(nav("/app").header(header::COOKIE, &f.cookie))).await;
    assert_eq!(ok.status, StatusCode::OK);
    sessions.revoke(5, 100).await.unwrap();
    let r = send(&f, empty(nav("/app").header(header::COOKIE, &f.cookie))).await;
    assert_eq!(r.status, StatusCode::FOUND);
    let r = send(
        &f,
        empty(req(Method::GET, HASHED_JS).header(header::COOKIE, &f.cookie)),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

// ---- the table, with a session ---------------------------------------------------------------------------

#[tokio::test]
async fn a_session_gets_the_app_shell_and_private_files() {
    let f = fixture(false).await;
    let c = &f.cookie;
    for path in [
        "/app",
        "/app/",
        "/channels",
        "/admin/review",
        "/help/whats-new",
    ] {
        let r = send(&f, empty(nav(path).header(header::COOKIE, c))).await;
        assert_eq!(r.status, StatusCode::OK, "{path}");
        assert_eq!(r.body, "<html>APP-SHELL</html>");
        assert_eq!(r.h("cache-control"), "private, no-store");
        assert_eq!(r.h("vary"), "Cookie");
        assert_eq!(r.h("content-security-policy"), APP_CSP);
        assert_eq!(r.h("x-robots-tag"), "noindex, nofollow");
        assert!(r.h("content-type").starts_with("text/html"));
    }
    // `/app` is the neutral door even for a fetch.
    let r = send(
        &f,
        empty(req(Method::GET, "/app").header(header::COOKIE, c)),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);

    let js = send(
        &f,
        empty(req(Method::GET, HASHED_JS).header(header::COOKIE, c)),
    )
    .await;
    assert_eq!(js.status, StatusCode::OK);
    assert_eq!(js.body, "console.log('app')");
    assert_eq!(
        js.h("cache-control"),
        "private, max-age=31536000, immutable"
    );
    assert_eq!(js.h("vary"), "Cookie");
    assert!(js.h("content-type").starts_with("text/javascript"));
    assert_eq!(js.h("x-content-type-options"), "nosniff");

    let m = send(
        &f,
        empty(req(Method::GET, "/app/manifest.webmanifest").header(header::COOKIE, c)),
    )
    .await;
    assert_eq!(m.status, StatusCode::OK);
    assert_eq!(m.h("cache-control"), "private, no-cache");
    assert_eq!(m.h("content-type"), "application/manifest+json");

    let n = send(
        &f,
        empty(req(Method::GET, "/app/THIRD_PARTY_NOTICES.md").header(header::COOKIE, c)),
    )
    .await;
    assert_eq!(n.body, "full notices");
    assert_eq!(n.h("cache-control"), "private, no-cache");

    // Only a session can see that a file is missing.
    let missing = send(
        &f,
        empty(req(Method::GET, "/app/assets/none-00000000.js").header(header::COOKIE, c)),
    )
    .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);

    let head = send(
        &f,
        empty(req(Method::HEAD, HASHED_JS).header(header::COOKIE, c)),
    )
    .await;
    assert_eq!(head.status, StatusCode::OK);
    assert!(head.body.is_empty());
    assert_eq!(head.h("content-length"), "18");

    let post = send(
        &f,
        empty(req(Method::POST, "/app").header(header::COOKIE, c)),
    )
    .await;
    assert_eq!(post.status, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn paths_cannot_leave_the_app_directory() {
    let f = fixture(false).await;
    let c = &f.cookie;
    for path in [
        "/app/..%2fapp.html",
        "/app/%2e%2e/app.html",
        "/app/assets/..%2f..%2fsurfaces.json",
        "/app/%2Fetc%2Fpasswd",
        "/app/assets%5C..%5C..%5Capp.html",
        "/app/csp.txt%00.js",
        "/app/.hidden",
    ] {
        let r = send(&f, empty(req(Method::GET, path).header(header::COOKIE, c))).await;
        assert!(
            r.status == StatusCode::NOT_FOUND || r.status == StatusCode::BAD_REQUEST,
            "{path} -> {}",
            r.status
        );
        assert!(
            !r.body.contains("APP-SHELL") && !r.body.contains("landing"),
            "{path}"
        );
    }
}

// ---- the API ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_api_has_no_oracle_either() {
    let f = fixture(false).await;
    for p in [
        "/api/v1/nope",
        "/api/v1/openapi.json",
        "/api/v10/x/y",
        "/api/v1/users/@me",
    ] {
        let r = send(&f, empty(req(Method::GET, p))).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{p}");
    }
    let r = send(&f, empty(req(Method::GET, "/developers/docs/api"))).await;
    assert_eq!(r.status, StatusCode::FOUND);
    assert_eq!(r.h("location"), "/login?next=/developers/docs/api");
    // With a principal the unknown path is an honest 404 and the document is served.
    let bot = |p: &str| empty(req(Method::GET, p).header(header::AUTHORIZATION, "Bot botsecret"));
    assert_eq!(
        send(&f, bot("/api/v1/nope")).await.status,
        StatusCode::NOT_FOUND
    );
    let doc = send(&f, bot("/api/v1/openapi.json")).await;
    assert_eq!(doc.status, StatusCode::OK);
    assert!(doc.body.contains("SIDE ROOM API"));
    let docs = send(&f, bot("/developers/docs/api")).await;
    assert_eq!(docs.status, StatusCode::OK);
}

#[tokio::test]
async fn health_stays_public() {
    let f = fixture(false).await;
    let r = send(&f, empty(req(Method::GET, "/healthz"))).await;
    assert_eq!(r.status, StatusCode::OK);
}

// ---- serve_public ----------------------------------------------------------------------------------------

#[tokio::test]
async fn without_serve_public_the_public_paths_are_not_ours() {
    let f = fixture(false).await;
    for p in [
        "/",
        "/login",
        "/login/2fa",
        "/join/abcd",
        "/entrance/index-9f8e7d6c.js",
        "/sw.js",
    ] {
        let r = send(&f, empty(nav(p))).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND, "{p}");
        assert!(r.body.is_empty(), "{p}");
    }
}

#[tokio::test]
async fn with_serve_public_the_surfaces_files_and_error_pages_are_served() {
    let f = fixture(true).await;
    for (p, body) in [
        ("/", "LANDING"),
        ("/join/abcd1234", "LANDING"),
        ("/login", "AUTH"),
        ("/login/2fa", "AUTH"),
        ("/register", "AUTH"),
        ("/reset/tok", "AUTH"),
        ("/oauth/discord/callback", "AUTH"),
        ("/invite/abcd1234", "INVITE"),
        ("/no-invitation", "INVITE"),
    ] {
        let r = send(&f, empty(nav(p))).await;
        assert_eq!(r.status, StatusCode::OK, "{p}");
        assert!(r.body.contains(body), "{p}");
        assert_eq!(r.h("content-security-policy"), PUBLIC_CSP, "{p}");
        assert_eq!(r.h("cache-control"), "no-cache", "{p}");
        assert!(!PUBLIC_CSP.contains("wasm-unsafe-eval") && !PUBLIC_CSP.contains("blob:"));
        // D-384: only the landing page is indexable.
        if p == "/" {
            assert!(!r.headers.contains_key("x-robots-tag"));
        } else {
            assert_eq!(r.h("x-robots-tag"), "noindex, nofollow", "{p}");
        }
    }
    let js = send(&f, empty(req(Method::GET, "/entrance/index-9f8e7d6c.js"))).await;
    assert_eq!(js.status, StatusCode::OK);
    assert_eq!(js.h("cache-control"), "public, max-age=31536000, immutable");
    let sw = send(&f, empty(req(Method::GET, "/sw.js"))).await;
    assert_eq!(sw.h("cache-control"), "no-cache");
    let e = send(&f, empty(req(Method::GET, "/404.html"))).await;
    assert_eq!(e.status, StatusCode::OK);
    assert!(
        e.h("content-security-policy")
            .starts_with("default-src 'none'")
    );
    // A dotted segment is never served, even under a public prefix.
    let dot = send(&f, empty(req(Method::GET, "/entrance/.vite/manifest.json"))).await;
    assert_eq!(dot.status, StatusCode::NOT_FOUND);
    // The app is still gated.
    let app = send(&f, empty(nav("/app"))).await;
    assert_eq!(app.status, StatusCode::FOUND);
    let appfile = send(&f, empty(req(Method::GET, HASHED_JS))).await;
    assert_eq!(appfile.status, StatusCode::UNAUTHORIZED);
    // The reserved profile surface has no shell yet: it is a public path with nothing behind it.
    let prof = send(&f, empty(nav("/~someone"))).await;
    assert_eq!(prof.status, StatusCode::NOT_FOUND);
}

// ---- the legal page --------------------------------------------------------------------------------------

#[tokio::test]
async fn house_notes_fill_the_operator_values_escaped() {
    for serve_public in [false, true] {
        let (f, _) = fixture_with(
            serve_public,
            Arc::new(Operator(Some("NL"), Some("<b>ops</b>&\"x\"@example.org"))),
        )
        .await;
        let r = send(&f, empty(nav("/house-notes"))).await;
        assert_eq!(r.status, StatusCode::OK);
        assert_eq!(
            r.body,
            "<p>EN NL | &lt;b&gt;ops&lt;/b&gt;&amp;&quot;x&quot;@example.org</p>"
        );
        assert!(r.h("content-type").starts_with("text/html"));
        assert_eq!(r.h("cache-control"), "no-cache");
        let csp = r.h("content-security-policy");
        assert!(csp.starts_with("default-src 'none'"));
        assert!(!csp.contains("script-src") && !csp.contains("connect-src"));
        assert_eq!(r.h("x-robots-tag"), "noindex, nofollow");
    }
}

#[tokio::test]
async fn house_notes_choose_the_locale_and_fall_back_to_english() {
    let (f, _) = fixture_with(false, Arc::new(Operator(None, None))).await;
    let get = |uri: &str, al: Option<&str>| {
        let mut b = nav(uri);
        if let Some(a) = al {
            b = b.header(header::ACCEPT_LANGUAGE, a);
        }
        empty(b)
    };
    assert_eq!(
        send(&f, get("/house-notes", None)).await.body,
        "<p>EN  | </p>"
    );
    assert!(
        send(&f, get("/house-notes?lang=de", None))
            .await
            .body
            .starts_with("<p>DE")
    );
    assert!(
        send(&f, get("/house-notes", Some("de-AT,de;q=0.9,en;q=0.5")))
            .await
            .body
            .starts_with("<p>DE")
    );
    // ?lang= wins over Accept-Language.
    assert!(
        send(&f, get("/house-notes?lang=en", Some("de")))
            .await
            .body
            .starts_with("<p>EN")
    );
    // No template for the locale: English.
    assert!(
        send(&f, get("/house-notes?lang=ja", Some("ja,fr")))
            .await
            .body
            .starts_with("<p>EN")
    );
    // A tag that is a path is ignored.
    assert!(
        send(&f, get("/house-notes?lang=..%2Fapp", None))
            .await
            .body
            .starts_with("<p>EN")
    );
}

// ---- start-up --------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_gate_refuses_to_start_without_its_policy() {
    let root = web_root();
    std::fs::remove_file(root.path().join("app/csp.txt")).unwrap();
    let err = WebGate::open(root.path(), false, false, Arc::new(NoOperator))
        .err()
        .expect("must refuse");
    assert!(matches!(err, WebError::MissingCsp(_)), "{err}");
    let missing = Path::new("definitely/not/a/dir");
    assert!(matches!(
        WebGate::open(missing, false, false, Arc::new(NoOperator))
            .err()
            .unwrap(),
        WebError::NoRoot(_)
    ));
}

#[tokio::test]
async fn serve_public_needs_the_surface_table() {
    let root = web_root();
    std::fs::remove_file(root.path().join("surfaces.json")).unwrap();
    assert!(WebGate::open(root.path(), true, false, Arc::new(NoOperator)).is_err());
    assert!(WebGate::open(root.path(), false, false, Arc::new(NoOperator)).is_ok());
}

#[test]
fn an_unset_root_serves_nothing() {
    let cfg = sr_core::config::Config::default();
    assert!(cfg.web.root_path().is_none());
    assert!(
        WebGate::from_config(&cfg, Arc::new(NoOperator))
            .unwrap()
            .is_none()
    );
    let mut cfg = cfg;
    cfg.web.root = Some("  ".into());
    assert!(cfg.web.root_path().is_none());
}

// ---- the 401 bucket --------------------------------------------------------------------------------------

#[tokio::test]
async fn a_noisy_address_is_slowed_down_not_told_anything_new() {
    let f = fixture(false).await;
    let mut last = StatusCode::OK;
    for _ in 0..260 {
        last = send(&f, empty(req(Method::GET, "/app/x.js"))).await.status;
    }
    assert_eq!(last, StatusCode::TOO_MANY_REQUESTS);
    // The session is still served: only the 401s are counted.
    let r = send(&f, empty(nav("/app").header(header::COOKIE, &f.cookie))).await;
    assert_eq!(r.status, StatusCode::OK);
}
