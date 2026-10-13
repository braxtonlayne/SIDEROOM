use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
    routing::{get, post},
};
use http_body_util::BodyExt;
use serde_json::Value;
use sr_api::{
    ApiSettings, AppState, Principal,
    auth::{
        BearerIdentity, BotIdentity, BotOnly, UserOnly, csrf,
        memory::{MemoryBearers, MemoryBots, MemorySessions},
    },
    build_router,
    health::ReadinessCheck,
    internal_router,
    rate_limit::{BucketSpec, RateLimitConfig},
};
use sr_auth::{
    repo::{SessionRecord, SessionRepository},
    session::SessionToken,
};
use sr_core::{Clock, ManualClock};
use std::sync::Arc;
use tower::ServiceExt;

const NOW_MS: i64 = 1_800_000_000_000;

struct Fixture {
    state: AppState,
    sessions: Arc<MemorySessions>,
    clock: Arc<ManualClock>,
}

fn fixture_with(settings: ApiSettings, extra: impl FnOnce(&mut AppStateBuilderExt)) -> Fixture {
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
        },
    );
    let clock = Arc::new(ManualClock::new(NOW_MS));
    let mut ext = AppStateBuilderExt { checks: vec![] };
    extra(&mut ext);
    let mut b = AppState::builder()
        .sessions(sessions.clone())
        .bots(bots)
        .bearers(bearers)
        .clock(clock.clone())
        .settings(settings);
    for c in ext.checks {
        b = b.readiness(c);
    }
    Fixture {
        state: b.build(),
        sessions,
        clock,
    }
}

struct AppStateBuilderExt {
    checks: Vec<Arc<dyn ReadinessCheck>>,
}

fn fixture() -> Fixture {
    fixture_with(ApiSettings::default(), |_| {})
}

fn extra_routes() -> Router<AppState> {
    Router::new()
        .route("/test/write", post(|_: Principal| async { "ok" }))
        .route("/test/bot", get(|_: BotOnly| async { "bot" }))
        .route("/test/user", get(|_: UserOnly| async { "user" }))
}

fn app(f: &Fixture) -> Router {
    build_router(f.state.clone(), extra_routes())
}

async fn send(app: &Router, req: Request<Body>) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let (parts, body) = resp.into_parts();
    let bytes = body.collect().await.unwrap().to_bytes().to_vec();
    (parts.status, parts.headers, bytes)
}

fn get_req(uri: &str) -> Request<Body> {
    Request::get(uri).body(Body::empty()).unwrap()
}

async fn login(f: &Fixture, user_id: i64, created_at: i64, expires_at: i64) -> SessionToken {
    let t = SessionToken::generate().unwrap();
    f.sessions
        .create(SessionRecord {
            id: 99,
            user_id,
            token_hash: t.hash(),
            created_at,
            last_seen_at: created_at,
            expires_at,
            device_name: None,
        })
        .await
        .unwrap();
    t
}

fn json(b: &[u8]) -> Value {
    serde_json::from_slice(b).unwrap()
}

#[tokio::test]
async fn health_and_ready() {
    let f = fixture();
    let (s, _, b) = send(&app(&f), get_req("/healthz")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(json(&b)["status"], "ok");
    let (s, _, _) = send(&app(&f), get_req("/readyz")).await;
    assert_eq!(s, StatusCode::OK);
}

struct Down;
#[async_trait]
impl ReadinessCheck for Down {
    fn name(&self) -> &'static str {
        "db"
    }
    async fn check(&self) -> Result<(), String> {
        Err("connection refused".into())
    }
}

#[tokio::test]
async fn readyz_reports_failures() {
    let f = fixture_with(ApiSettings::default(), |e| e.checks.push(Arc::new(Down)));
    let (s, _, b) = send(&app(&f), get_req("/readyz")).await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(json(&b)["checks"]["db"]["error"], "connection refused");
}

#[tokio::test]
async fn openapi_served_on_v1_and_v10() {
    let f = fixture();
    for p in ["/api/v1/openapi.json", "/api/v10/openapi.json"] {
        let (s, _, b) = send(&app(&f), get_req(p)).await;
        assert_eq!(s, StatusCode::OK, "{p}");
        let doc = json(&b);
        assert_eq!(doc["info"]["title"], "SIDE ROOM API");
        assert!(doc["paths"]["/users/@me"]["get"].is_object());
        assert!(doc["components"]["schemas"]["ErrorBody"].is_object());
        assert!(doc["components"]["securitySchemes"]["session"].is_object());
    }
}

#[tokio::test]
async fn unknown_route_is_discord_style_404() {
    let f = fixture();
    let (s, _, b) = send(&app(&f), get_req("/nope")).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert_eq!(json(&b)["message"], "404: Not Found");
}

#[tokio::test]
async fn request_id_is_generated_and_echoed() {
    let f = fixture();
    let (_, h, _) = send(&app(&f), get_req("/healthz")).await;
    assert!(h.contains_key("x-request-id"));
    let req = Request::get("/healthz")
        .header("x-request-id", "abc-123")
        .body(Body::empty())
        .unwrap();
    let (_, h, _) = send(&app(&f), req).await;
    assert_eq!(h["x-request-id"], "abc-123");
}

#[tokio::test]
async fn cors_preflight_allows_configured_origin_only() {
    let f = fixture();
    let pre = |origin: &str| {
        Request::builder()
            .method(Method::OPTIONS)
            .uri("/api/v1/users/@me")
            .header(header::ORIGIN, origin)
            .header("access-control-request-method", "POST")
            .header("access-control-request-headers", "x-sr-csrf")
            .body(Body::empty())
            .unwrap()
    };
    let (_, h, _) = send(&app(&f), pre("http://localhost:8080")).await;
    assert_eq!(h["access-control-allow-origin"], "http://localhost:8080");
    assert_eq!(h["access-control-allow-credentials"], "true");
    let (_, h, _) = send(&app(&f), pre("http://evil.example")).await;
    assert!(!h.contains_key("access-control-allow-origin"));
}

#[tokio::test]
async fn metrics_record_requests_by_route_pattern() {
    let f = fixture();
    let a = app(&f);
    send(&a, get_req("/api/v1/users/@me")).await; // 401
    metrics::histogram!(
        sr_api::observability::names::GATEWAY_DISPATCH_BYTES,
        "compression" => "none"
    )
    .record(70_000.0);
    let (s, h, b) = send(
        &internal_router(f.state.clone()),
        get_req("/internal/metrics"),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(
        h[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/plain")
    );
    let text = String::from_utf8(b).unwrap();
    assert!(text.contains("http_requests_total"), "{text}");
    assert!(text.contains("http_request_duration_seconds"));
    // F-30: the dispatch size histogram has a 64 KiB bucket bound.
    assert!(
        text.contains("sr_gateway_dispatch_bytes_bucket{compression=\"none\",le=\"65536\"}"),
        "{text}"
    );
    assert!(text.contains("route=\"/api/v1/users/@me\""), "{text}");
    // not exposed publicly
    let (s, _, _) = send(&a, get_req("/internal/metrics")).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn no_credentials_is_401() {
    let f = fixture();
    let (s, _, b) = send(&app(&f), get_req("/api/v1/users/@me")).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert_eq!(json(&b)["code"], 40001);
}

#[tokio::test]
async fn bot_and_bearer_tokens() {
    let f = fixture();
    let req = |auth: &str| {
        Request::get("/api/v10/users/@me")
            .header(header::AUTHORIZATION, auth)
            .body(Body::empty())
            .unwrap()
    };
    let (s, _, b) = send(&app(&f), req("Bot botsecret")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(json(&b)["id"], "7");
    assert_eq!(json(&b)["auth"], "bot");
    assert_eq!(json(&b)["bot"], true);
    assert_eq!(json(&b)["discriminator"], "0");
    assert!(json(&b)["avatar"].is_null());
    assert!(
        json(&b).get("email").is_none(),
        "bots have no account email"
    );
    let (s, _, b) = send(&app(&f), req("Bearer oauthsecret")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(json(&b)["auth"], "bearer");
    for bad in ["Bot nope", "Bearer botsecret", "Basic abc", "Bot"] {
        let (s, _, b) = send(&app(&f), req(bad)).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{bad}");
        assert_eq!(json(&b)["code"], 50014, "{bad}");
    }
}

#[tokio::test]
async fn bot_only_and_user_only() {
    let f = fixture();
    let req = |uri: &str, auth: &str| {
        Request::get(uri)
            .header(header::AUTHORIZATION, auth)
            .body(Body::empty())
            .unwrap()
    };
    let (s, _, b) = send(&app(&f), req("/api/v1/test/bot", "Bearer oauthsecret")).await;
    assert_eq!(
        (s, json(&b)["code"].as_i64()),
        (StatusCode::FORBIDDEN, Some(20002))
    );
    let (s, _, _) = send(&app(&f), req("/api/v1/test/bot", "Bot botsecret")).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _, b) = send(&app(&f), req("/api/v1/test/user", "Bot botsecret")).await;
    assert_eq!(
        (s, json(&b)["code"].as_i64()),
        (StatusCode::FORBIDDEN, Some(20001))
    );
}

#[tokio::test]
async fn session_cookie_and_csrf() {
    let f = fixture();
    let now = NOW_MS / 1000;
    let t = login(&f, 5, now - 10, now + 1000).await;
    let cookie = format!("theme=dark; sr_session={}", t.expose());

    // GET works without CSRF
    let req = Request::get("/api/v1/users/@me")
        .header(header::COOKIE, &cookie)
        .body(Body::empty())
        .unwrap();
    let (s, _, b) = send(&app(&f), req).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(json(&b)["auth"], "session");

    // the CSRF endpoint hands out the token
    let req = Request::get("/api/v1/sr/csrf")
        .header(header::COOKIE, &cookie)
        .body(Body::empty())
        .unwrap();
    let (s, _, b) = send(&app(&f), req).await;
    assert_eq!(s, StatusCode::OK);
    let token = json(&b)["token"].as_str().unwrap().to_owned();
    assert!(csrf::verify(t.expose(), &token));

    // mutation without / with wrong / with right token
    let post = |hdr: Option<&str>| {
        let mut r = Request::post("/api/v1/test/write").header(header::COOKIE, &cookie);
        if let Some(h) = hdr {
            r = r.header("x-sr-csrf", h);
        }
        r.body(Body::empty()).unwrap()
    };
    for hdr in [None, Some("wrong")] {
        let (s, _, b) = send(&app(&f), post(hdr)).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
        assert_eq!(json(&b)["code"], 50001);
    }
    let (s, _, _) = send(&app(&f), post(Some(&token))).await;
    assert_eq!(s, StatusCode::OK);

    // bots do not need CSRF
    let req = Request::post("/api/v1/test/write")
        .header(header::AUTHORIZATION, "Bot botsecret")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app(&f), req).await.0, StatusCode::OK);
}

#[tokio::test]
async fn expired_and_unknown_sessions_are_rejected() {
    let f = fixture();
    let now = NOW_MS / 1000;
    let t = login(&f, 5, now - 10, now + 50).await;
    let req = |tok: &str| {
        Request::get("/api/v1/users/@me")
            .header(header::COOKIE, format!("sr_session={tok}"))
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(send(&app(&f), req(t.expose())).await.0, StatusCode::OK);
    f.clock.advance_ms(60_000);
    assert!(f.clock.now_unix_ms() > NOW_MS);
    assert_eq!(
        send(&app(&f), req(t.expose())).await.0,
        StatusCode::UNAUTHORIZED
    );
    let other = SessionToken::generate().unwrap();
    assert_eq!(
        send(&app(&f), req(other.expose())).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(&app(&f), req("junk")).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn session_expiry_rolls_forward() {
    let f = fixture();
    let now = NOW_MS / 1000;
    let t = login(&f, 5, now - 1000, now + 500).await;
    let req = Request::get("/api/v1/users/@me")
        .header(header::COOKIE, format!("sr_session={}", t.expose()))
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&app(&f), req).await.0, StatusCode::OK);
    let rec = f.sessions.find_by_hash(&t.hash()).await.unwrap().unwrap();
    assert!(rec.expires_at > now + 500);
    assert_eq!(rec.last_seen_at, now);
}

#[tokio::test]
async fn rate_limit_headers_and_429() {
    let settings = ApiSettings {
        rate_limit: RateLimitConfig {
            default_bucket: BucketSpec::new(2, 60),
            ..RateLimitConfig::default()
        },
        ..ApiSettings::default()
    };
    let f = fixture_with(settings, |_| {});
    let a = app(&f);
    let req = || {
        Request::get("/api/v1/users/@me")
            .header(header::AUTHORIZATION, "Bot botsecret")
            .body(Body::empty())
            .unwrap()
    };
    let (s, h, _) = send(&a, req()).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(h["x-ratelimit-limit"], "2");
    assert_eq!(h["x-ratelimit-remaining"], "1");
    assert!(h.contains_key("x-ratelimit-reset"));
    assert!(h.contains_key("x-ratelimit-reset-after"));
    assert!(h.contains_key("x-ratelimit-bucket"));
    assert_eq!(h["x-ratelimit-scope"], "user");
    let bucket = h["x-ratelimit-bucket"].clone();
    let (_, h, _) = send(&a, req()).await;
    assert_eq!(h["x-ratelimit-remaining"], "0");

    let (s, h, b) = send(&a, req()).await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(h["x-ratelimit-bucket"], bucket);
    assert!(h.contains_key("retry-after"));
    let body = json(&b);
    assert_eq!(body["global"], false);
    assert!(body["retry_after"].as_f64().unwrap() > 0.0);
    assert_eq!(body["message"], "You are being rate limited.");

    // v10 alias shares the bucket; a different caller does not
    let v10 = Request::get("/api/v10/users/@me")
        .header(header::AUTHORIZATION, "Bot botsecret")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&a, v10).await.0, StatusCode::TOO_MANY_REQUESTS);
    let other = Request::get("/api/v1/users/@me")
        .header(header::AUTHORIZATION, "Bearer oauthsecret")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&a, other).await.0, StatusCode::OK);
}

#[tokio::test]
async fn global_rate_limit_sets_global_header() {
    let mut settings = ApiSettings::default();
    settings.rate_limit.global_per_second = 1;
    let f = fixture_with(settings, |_| {});
    let a = app(&f);
    let mut saw_global = false;
    for _ in 0..4 {
        let (s, h, b) = send(&a, get_req("/api/v1/openapi.json")).await;
        if s == StatusCode::TOO_MANY_REQUESTS {
            assert_eq!(h["x-ratelimit-global"], "true");
            assert_eq!(json(&b)["global"], true);
            saw_global = true;
        }
    }
    assert!(saw_global);
}
