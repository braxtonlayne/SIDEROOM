//! Endpoint tests that need no database: authentication on every route, id and body validation that runs before
//! any query, the error mapping, and the OpenAPI fragment. Behaviour against real tables is covered by
//! `sr-userfeat/tests/db.rs` and the Postgres-backed wiring tests in `sr-server`.

use super::{UserFeatDeps, body_time, openapi, router, uerr};
use crate::{
    ApiSettings, AppState,
    auth::{csrf, memory::MemorySessions},
    build_router,
};
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sr_auth::{
    repo::{SessionRecord, SessionRepository},
    session::SessionToken,
};
use sr_features::{FeatureRegistry, FeatureService, InMemorySettingsStore};
use sr_userfeat::{Card, Poster, SendFailure, UfError, UserFeatures, register_all};
use std::sync::Arc;
use tower::ServiceExt;

const MEMBER: i64 = 20;

struct NoPoster;

#[async_trait::async_trait]
impl Poster for NoPoster {
    async fn post(&self, _c: i64, _card: Card) -> Result<i64, String> {
        Err("no chat".into())
    }
    async fn dm(&self, _u: i64, _card: Card) -> Result<(i64, i64), String> {
        Err("no chat".into())
    }
    async fn send_as(&self, _a: i64, _c: i64, _t: &str) -> Result<i64, SendFailure> {
        Err(SendFailure::Permanent("no chat".into()))
    }
    async fn react(&self, _c: i64, _m: i64, _e: &str) {}
    async fn edit(&self, _c: i64, _m: i64, _card: Card) {}
    async fn add_role(&self, _g: i64, _u: i64, _r: i64) -> bool {
        false
    }
    async fn remove_role(&self, _g: i64, _u: i64, _r: i64) -> bool {
        false
    }
}

struct Env {
    app: Router,
    token: String,
}

async fn env() -> Env {
    let sessions = Arc::new(MemorySessions::new());
    let t = SessionToken::generate().unwrap();
    sessions
        .create(SessionRecord {
            id: 100,
            user_id: MEMBER,
            token_hash: t.hash(),
            created_at: 0,
            last_seen_at: i64::MAX / 4,
            expires_at: i64::MAX / 4,
            device_name: None,
        })
        .await
        .unwrap();
    // A pool that never connects: every request below is answered before the first query.
    let pool = sqlx::postgres::PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_millis(100))
        .connect_lazy("postgres://nobody@127.0.0.1:1/none")
        .unwrap();
    let mut registry = FeatureRegistry::new();
    register_all(&mut registry).unwrap();
    let features = FeatureService::new(
        Arc::new(registry),
        Arc::new(InMemorySettingsStore::new()),
        None,
    );
    let uf = UserFeatures::new(pool, features, Arc::new(|| 4242));
    let deps = UserFeatDeps {
        uf,
        poster: Arc::new(NoPoster),
        net_policy: sr_net::Policy::default(),
    };
    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let state = AppState::builder()
        .settings(settings)
        .sessions(sessions)
        .extension(Arc::new(deps))
        .build();
    Env {
        app: build_router(state, router()),
        token: t.expose().to_owned(),
    }
}

impl Env {
    async fn call(
        &self,
        signed_in: bool,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method.clone())
            .uri(format!("/api/v1{path}"))
            .header("origin", "http://localhost:8080");
        if signed_in {
            req = req.header("cookie", format!("sr_session={}", self.token));
            if method != Method::GET {
                req = req.header("x-sr-csrf", csrf::token_for(&self.token));
            }
        }
        let body = match body {
            Some(b) => {
                req = req.header("content-type", "application/json");
                Body::from(b.to_string())
            }
            None => Body::empty(),
        };
        let resp = self
            .app
            .clone()
            .oneshot(req.body(body).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
}

type Call = (Method, &'static str, Option<Value>);

fn routes() -> Vec<Call> {
    vec![
        (
            Method::POST,
            "/channels/9/scheduled-messages",
            Some(json!({ "content": "x", "send_at": "2030-01-01T00:00:00Z" })),
        ),
        (Method::GET, "/users/@me/scheduled-messages", None),
        (
            Method::PATCH,
            "/users/@me/scheduled-messages/9",
            Some(json!({ "content": "y" })),
        ),
        (Method::DELETE, "/users/@me/scheduled-messages/9", None),
        (Method::GET, "/users/@me/reminders", None),
        (
            Method::POST,
            "/users/@me/reminders",
            Some(json!({ "in_seconds": 60 })),
        ),
        (Method::DELETE, "/users/@me/reminders/9", None),
        (
            Method::PUT,
            "/channels/9/messages/8/bookmark",
            Some(json!({ "note": "n" })),
        ),
        (Method::GET, "/users/@me/bookmarks", None),
        (
            Method::PATCH,
            "/users/@me/bookmarks/8",
            Some(json!({ "note": "n" })),
        ),
        (Method::DELETE, "/users/@me/bookmarks/8", None),
        (Method::GET, "/users/@me/birthday", None),
        (
            Method::PUT,
            "/users/@me/birthday",
            Some(json!({ "month": 5, "day": 5 })),
        ),
        (Method::DELETE, "/users/@me/birthday", None),
        (Method::GET, "/guilds/9/birthdays", None),
        (Method::GET, "/guilds/9/leveling/rank/@me", None),
        (Method::GET, "/guilds/9/leveling/leaderboard", None),
        (
            Method::PUT,
            "/users/@me/guilds/9/leveling",
            Some(json!({ "enabled": false })),
        ),
        (Method::GET, "/guilds/9/rss-feeds", None),
        (
            Method::POST,
            "/guilds/9/rss-feeds",
            Some(json!({ "url": "https://example.test/f", "channel_id": "3" })),
        ),
        (
            Method::PATCH,
            "/guilds/9/rss-feeds/4",
            Some(json!({ "enabled": false })),
        ),
        (Method::DELETE, "/guilds/9/rss-feeds/4", None),
        (Method::GET, "/guilds/9/poll-templates", None),
        (
            Method::POST,
            "/guilds/9/poll-templates",
            Some(json!({ "name": "n", "question": "q", "answers": ["a", "b"] })),
        ),
        (
            Method::PATCH,
            "/guilds/9/poll-templates/4",
            Some(json!({ "name": "n", "question": "q", "answers": ["a", "b"] })),
        ),
        (Method::DELETE, "/guilds/9/poll-templates/4", None),
        (Method::GET, "/guilds/9/suggestions", None),
        (
            Method::POST,
            "/guilds/9/suggestions",
            Some(json!({ "content": "c" })),
        ),
        (
            Method::PATCH,
            "/guilds/9/suggestions/4",
            Some(json!({ "status": "approved" })),
        ),
        (Method::DELETE, "/guilds/9/suggestions/4", None),
        (Method::GET, "/guilds/9/tickets", None),
        (
            Method::POST,
            "/guilds/9/tickets",
            Some(json!({ "subject": "help" })),
        ),
        (Method::POST, "/guilds/9/tickets/4/close", None),
    ]
}

#[tokio::test]
async fn every_route_needs_a_session() {
    let e = env().await;
    for (method, path, body) in routes() {
        let (status, _) = e.call(false, method.clone(), path, body).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path}");
    }
}

#[tokio::test]
async fn malformed_ids_are_not_found_before_any_query() {
    let e = env().await;
    for (method, path) in [
        (Method::PATCH, "/users/@me/scheduled-messages/abc"),
        (Method::DELETE, "/users/@me/scheduled-messages/abc"),
        (Method::DELETE, "/users/@me/reminders/abc"),
        (Method::PATCH, "/users/@me/bookmarks/abc"),
        (Method::DELETE, "/users/@me/bookmarks/0"),
    ] {
        let body = (method == Method::PATCH).then(|| json!({ "note": "n" }));
        let (status, body) = e.call(true, method.clone(), path, body).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {path}: {body}");
        assert_eq!(body["code"], 900009, "{method} {path}");
    }
}

#[tokio::test]
async fn bodies_are_validated_before_any_query() {
    let e = env().await;
    // A birthday that does not exist on the calendar.
    let (status, body) = e
        .call(
            true,
            Method::PUT,
            "/users/@me/birthday",
            Some(json!({ "month": 2, "day": 30 })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], 50035);
    let (status, _) = e
        .call(
            true,
            Method::PUT,
            "/users/@me/birthday",
            Some(json!({ "month": "may" })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    // An unparseable time on an edit.
    let (status, body) = e
        .call(
            true,
            Method::PATCH,
            "/users/@me/scheduled-messages/9",
            Some(json!({ "send_at": "tomorrow" })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[test]
fn errors_map_to_their_codes() {
    use crate::routes::messages::error::MsgError;
    let code = |e: UfError| match uerr(e) {
        MsgError::Api(a) => a.status(),
        MsgError::RetryAfter { .. } => 429,
    };
    assert_eq!(code(UfError::NotFound("x")), 404);
    assert_eq!(code(UfError::Forbidden), 403);
    assert_eq!(code(UfError::Invalid("x".into())), 400);
    assert_eq!(code(UfError::Conflict("x".into())), 409);
    assert_eq!(code(UfError::Disabled("x")), 403);
    assert_eq!(code(UfError::Storage("x".into())), 500);
}

#[test]
fn iso_times_parse_to_milliseconds() {
    assert_eq!(
        body_time("send_at", "2030-01-01T00:00:00Z").unwrap(),
        1_893_456_000_000
    );
    assert!(body_time("send_at", "2030-13-01T00:00:00Z").is_err());
    assert!(body_time("send_at", "soon").is_err());
}

#[test]
fn openapi_lists_every_path() {
    let doc = serde_json::to_value(openapi()).unwrap();
    let documented: Vec<&String> = doc["paths"].as_object().unwrap().keys().collect();
    // A documented path matches a concrete one when every segment is equal or a `{parameter}`.
    let matches = |doc: &str, path: &str| {
        let (d, p): (Vec<_>, Vec<_>) = (doc.split('/').collect(), path.split('/').collect());
        d.len() == p.len() && d.iter().zip(&p).all(|(d, p)| d.starts_with('{') || d == p)
    };
    for (_, path, _) in routes() {
        assert!(
            documented.iter().any(|d| matches(d, path)),
            "{path} is not documented"
        );
    }
}
