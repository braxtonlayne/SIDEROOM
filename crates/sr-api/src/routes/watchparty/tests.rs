//! Endpoint tests of the Watch Party router that need no database: authentication, id parsing and the body
//! validation that runs before any query. Behaviour against real tables is covered by
//! `sr-watchparty/tests/db.rs`.

use super::{WatchPartyDeps, openapi, router};
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
use sr_core::SystemClock;
use sr_watchparty::PartyService;
use std::sync::Arc;
use tower::ServiceExt;

const MEMBER: i64 = 20;

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
    let service = PartyService::new(pool, Arc::new(SystemClock), Arc::new(|| 4242));
    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let state = AppState::builder()
        .settings(settings)
        .sessions(sessions)
        .extension(Arc::new(WatchPartyDeps::new(service)))
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
        (Method::GET, "/watch-parties/time", None),
        (Method::GET, "/watch-parties/me", None),
        (
            Method::POST,
            "/watch-parties",
            Some(json!({ "title_id": "1" })),
        ),
        (Method::GET, "/watch-parties/9", None),
        (Method::PATCH, "/watch-parties/9", Some(json!({}))),
        (Method::DELETE, "/watch-parties/9", None),
        (Method::GET, "/channels/9/watch-party", None),
        (Method::POST, "/watch-parties/9/join", None),
        (Method::POST, "/watch-parties/9/leave", None),
        (
            Method::POST,
            "/watch-parties/9/host",
            Some(json!({ "user_id": "3" })),
        ),
        (
            Method::PUT,
            "/watch-parties/9/controllers/3",
            Some(json!({ "controller": true })),
        ),
        (Method::DELETE, "/watch-parties/9/members/3", None),
        (
            Method::POST,
            "/watch-parties/9/playback",
            Some(json!({ "action": "play" })),
        ),
        (
            Method::PUT,
            "/watch-parties/9/media",
            Some(json!({ "title_id": "1" })),
        ),
        (
            Method::POST,
            "/watch-parties/9/queue",
            Some(json!({ "title_id": "1" })),
        ),
        (
            Method::PATCH,
            "/watch-parties/9/queue/4",
            Some(json!({ "position": 0 })),
        ),
        (Method::DELETE, "/watch-parties/9/queue/4", None),
        (Method::POST, "/watch-parties/9/next", None),
        (Method::POST, "/watch-parties/9/heartbeat", None),
        (
            Method::POST,
            "/watch-parties/9/reactions",
            Some(json!({ "emoji": "x" })),
        ),
        (
            Method::POST,
            "/watch-parties/9/messages",
            Some(json!({ "content": "x" })),
        ),
        (Method::GET, "/watch-parties/9/audit", None),
    ]
}

#[tokio::test]
async fn every_route_requires_a_session() {
    let env = env().await;
    for (method, path, body) in routes() {
        let (status, _) = env.call(false, method.clone(), path, body).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path}");
    }
}

#[tokio::test]
async fn a_malformed_party_id_names_nothing() {
    let env = env().await;
    for (method, path) in [
        (Method::GET, "/watch-parties/abc"),
        (Method::POST, "/watch-parties/abc/leave"),
        (Method::DELETE, "/watch-parties/0"),
        (Method::GET, "/channels/xyz/watch-party"),
    ] {
        let (status, body) = env.call(true, method.clone(), path, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {path}");
        assert_eq!(body["code"], 900009);
    }
}

#[tokio::test]
async fn the_clock_endpoint_answers_without_a_database() {
    let env = env().await;
    let (status, body) = env
        .call(true, Method::GET, "/watch-parties/time", None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["server_time_ms"].as_i64().unwrap() > 1_600_000_000_000);
}

#[tokio::test]
async fn bad_commands_are_refused_before_any_query() {
    let env = env().await;
    for body in [
        json!({ "action": "seek" }),
        json!({ "action": "rate" }),
        json!({ "action": "rate", "rate": 3.0 }),
        json!({ "action": "rate", "rate": 0.1 }),
    ] {
        let (status, resp) = env
            .call(
                true,
                Method::POST,
                "/watch-parties/9/playback",
                Some(body.clone()),
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(resp["code"], 50035);
    }
    let (status, _) = env
        .call(
            true,
            Method::POST,
            "/watch-parties/9/playback",
            Some(json!({ "action": "explode" })),
        )
        .await;
    assert!(status.is_client_error());
}

#[tokio::test]
async fn reactions_and_messages_are_validated_before_any_query() {
    let env = env().await;
    for (path, body) in [
        ("/watch-parties/9/reactions", json!({ "emoji": "" })),
        ("/watch-parties/9/reactions", json!({ "emoji": "<b>" })),
        ("/watch-parties/9/messages", json!({ "content": "   " })),
        (
            "/watch-parties/9/messages",
            json!({ "content": "a".repeat(501) }),
        ),
    ] {
        let (status, _) = env.call(true, Method::POST, path, Some(body.clone())).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path} {body}");
    }
}

#[test]
fn the_openapi_fragment_lists_every_route() {
    let doc = serde_json::to_value(openapi()).unwrap();
    let paths = doc["paths"].as_object().unwrap();
    for p in [
        "/watch-parties",
        "/watch-parties/time",
        "/watch-parties/me",
        "/watch-parties/{party_id}",
        "/channels/{channel_id}/watch-party",
        "/watch-parties/{party_id}/join",
        "/watch-parties/{party_id}/leave",
        "/watch-parties/{party_id}/host",
        "/watch-parties/{party_id}/controllers/{user_id}",
        "/watch-parties/{party_id}/members/{user_id}",
        "/watch-parties/{party_id}/playback",
        "/watch-parties/{party_id}/media",
        "/watch-parties/{party_id}/queue",
        "/watch-parties/{party_id}/queue/{item_id}",
        "/watch-parties/{party_id}/next",
        "/watch-parties/{party_id}/heartbeat",
        "/watch-parties/{party_id}/reactions",
        "/watch-parties/{party_id}/messages",
        "/watch-parties/{party_id}/audit",
    ] {
        assert!(paths.contains_key(p), "missing {p}");
    }
}
