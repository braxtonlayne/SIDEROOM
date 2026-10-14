//! Endpoint tests of the Cinema social router that need no database: authentication, id parsing, the
//! catalog and age checks (which run before any query), and body validation. Behaviour against real
//! tables is covered by `sr-cinema-social/tests/db.rs`.

use super::{CinemaSocialDeps, router};
use crate::{
    ApiSettings, AppState,
    auth::{csrf, memory::MemorySessions},
    build_router,
    routes::search::AgeGate,
};
use async_trait::async_trait;
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
use sr_cinema_social::{CinemaService, MemoryCatalog, TitleKind};
use sr_core::UserId;
use std::sync::Arc;
use tower::ServiceExt;

const MEMBER: i64 = 20;
const ADULT: i64 = 21;

struct Confirmed;

#[async_trait]
impl AgeGate for Confirmed {
    async fn age_confirmed(&self, user: UserId) -> bool {
        user.get() == ADULT as u64
    }
}

struct Env {
    app: Router,
    tokens: std::collections::HashMap<i64, String>,
}

async fn env() -> Env {
    let sessions = Arc::new(MemorySessions::new());
    let mut tokens = std::collections::HashMap::new();
    for (n, uid) in [MEMBER, ADULT].into_iter().enumerate() {
        let t = SessionToken::generate().unwrap();
        sessions
            .create(SessionRecord {
                id: 100 + n as i64,
                user_id: uid,
                token_hash: t.hash(),
                created_at: 0,
                last_seen_at: i64::MAX / 4,
                expires_at: i64::MAX / 4,
                device_name: None,
            })
            .await
            .unwrap();
        tokens.insert(uid, t.expose().to_owned());
    }
    let catalog = MemoryCatalog::new();
    catalog.add_title(MemoryCatalog::title_with(1, TitleKind::Movie, "Akira", &[]));
    catalog.add_title(MemoryCatalog::title_with(2, TitleKind::Series, "Dark", &[]));
    let mut secret = MemoryCatalog::title_with(3, TitleKind::Movie, "Secret", &[]);
    secret.age_restricted = true;
    catalog.add_title(secret);
    // A pool that never connects: every request below is answered before the first query.
    let pool = sqlx::PgPool::connect_lazy("postgres://nobody@127.0.0.1:1/none").unwrap();
    let service = CinemaService::new(pool, Arc::new(catalog), Arc::new(|| 4242));
    let deps = CinemaSocialDeps::new(service).with_age_gate(Arc::new(Confirmed));
    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let state = AppState::builder()
        .settings(settings)
        .sessions(sessions)
        .extension(Arc::new(deps))
        .build();
    Env {
        app: build_router(state, router()),
        tokens,
    }
}

impl Env {
    async fn call(
        &self,
        who: Option<i64>,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method.clone())
            .uri(format!("/api/v1{path}"))
            .header("origin", "http://localhost:8080");
        if let Some(uid) = who {
            let raw = &self.tokens[&uid];
            req = req.header("cookie", format!("sr_session={raw}"));
            if method != Method::GET {
                req = req.header("x-sr-csrf", csrf::token_for(raw));
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

fn beat() -> Value {
    json!({ "position_secs": 10, "duration_secs": 100 })
}

type Call = (Method, &'static str, Option<Value>);

fn routes() -> Vec<Call> {
    vec![
        (Method::GET, "/cinema/titles/1/progress", None),
        (Method::PUT, "/cinema/titles/1/progress", Some(beat())),
        (Method::DELETE, "/cinema/titles/1/progress", None),
        (Method::GET, "/cinema/titles/2/episodes/5/progress", None),
        (
            Method::PUT,
            "/cinema/titles/2/episodes/5/progress",
            Some(beat()),
        ),
        (Method::GET, "/cinema/titles/2/watch-state", None),
        (
            Method::POST,
            "/cinema/titles/2/watched",
            Some(json!({ "watched": true })),
        ),
        (Method::GET, "/cinema/me/history", None),
        (Method::DELETE, "/cinema/me/history", None),
        (Method::GET, "/cinema/me/history/settings", None),
        (
            Method::PUT,
            "/cinema/me/history/settings",
            Some(json!({ "paused": true })),
        ),
        (Method::GET, "/cinema/me/continue-watching", None),
        (Method::DELETE, "/cinema/me/watching", None),
        (Method::GET, "/cinema/me/friends-watching", None),
        (Method::GET, "/cinema/me/list", None),
        (Method::PUT, "/cinema/me/list/1", None),
        (Method::GET, "/cinema/lists", None),
        (Method::POST, "/cinema/lists", Some(json!({ "name": "x" }))),
        (Method::GET, "/cinema/lists/9", None),
        (Method::PATCH, "/cinema/lists/9", Some(json!({}))),
        (Method::DELETE, "/cinema/lists/9", None),
        (Method::PUT, "/cinema/lists/9/items/1", None),
        (Method::DELETE, "/cinema/lists/9/items/1", None),
        (
            Method::PUT,
            "/cinema/lists/9/order",
            Some(json!({ "title_ids": [] })),
        ),
        (Method::GET, "/cinema/users/9/lists", None),
        (Method::GET, "/cinema/titles/1/ratings", None),
        (
            Method::PUT,
            "/cinema/titles/1/rating",
            Some(json!({ "score": 5 })),
        ),
        (Method::DELETE, "/cinema/titles/1/rating", None),
        (Method::GET, "/cinema/titles/1/reviews", None),
        (
            Method::PUT,
            "/cinema/titles/1/review",
            Some(json!({ "body": "x" })),
        ),
        (Method::PUT, "/cinema/reviews/9/helpful", None),
        (
            Method::POST,
            "/cinema/reviews/9/report",
            Some(json!({ "reason": "x" })),
        ),
        (Method::GET, "/cinema/titles/1/comments", None),
        (
            Method::POST,
            "/cinema/titles/1/comments",
            Some(json!({ "body": "x" })),
        ),
        (Method::GET, "/cinema/comments/9/replies", None),
        (Method::PATCH, "/cinema/comments/9", Some(json!({}))),
        (Method::PUT, "/cinema/comments/9/like", None),
        (
            Method::POST,
            "/cinema/comments/9/report",
            Some(json!({ "reason": "x" })),
        ),
        (Method::GET, "/cinema/trending", None),
        (Method::GET, "/cinema/me/recommendations", None),
    ]
}

#[tokio::test]
async fn every_route_needs_a_signed_in_user() {
    let e = env().await;
    for (m, p, b) in routes() {
        let (s, _) = e.call(None, m.clone(), p, b.clone()).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{m} {p}");
    }
}

#[tokio::test]
async fn unknown_titles_episodes_and_bad_ids_are_404() {
    let e = env().await;
    let cases: Vec<Call> = vec![
        (Method::GET, "/cinema/titles/99/progress", None),
        (Method::PUT, "/cinema/titles/99/progress", Some(beat())),
        (Method::GET, "/cinema/titles/abc/progress", None),
        (Method::GET, "/cinema/titles/0/progress", None),
        // A movie has no episodes; an unknown episode of a series.
        (
            Method::PUT,
            "/cinema/titles/1/episodes/5/progress",
            Some(beat()),
        ),
        (
            Method::PUT,
            "/cinema/titles/2/episodes/5/progress",
            Some(beat()),
        ),
        (Method::GET, "/cinema/titles/99/ratings", None),
        (
            Method::PUT,
            "/cinema/titles/99/rating",
            Some(json!({ "score": 5 })),
        ),
        (Method::GET, "/cinema/titles/99/comments", None),
        (Method::GET, "/cinema/titles/2/episodes/5/comments", None),
        (Method::GET, "/cinema/lists/abc", None),
    ];
    for (m, p, b) in cases {
        let (s, body) = e.call(Some(MEMBER), m.clone(), p, b).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{m} {p}: {body}");
    }
}

#[tokio::test]
async fn restricted_titles_need_age_confirmation() {
    let e = env().await;
    let (s, body) = e
        .call(
            Some(MEMBER),
            Method::PUT,
            "/cinema/titles/3/progress",
            Some(beat()),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], 900011, "{body}");
    let (s, _) = e
        .call(Some(MEMBER), Method::GET, "/cinema/titles/3/reviews", None)
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn bad_bodies_are_client_errors() {
    let e = env().await;
    for body in [
        json!({ "position_secs": -1, "duration_secs": 100 }),
        json!({ "position_secs": 1, "duration_secs": 0 }),
        json!({ "position_secs": 500, "duration_secs": 100 }),
    ] {
        let (s, resp) = e
            .call(
                Some(MEMBER),
                Method::PUT,
                "/cinema/titles/1/progress",
                Some(body),
            )
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{resp}");
    }
    // A series is played by episode.
    let (s, _) = e
        .call(
            Some(MEMBER),
            Method::PUT,
            "/cinema/titles/2/progress",
            Some(beat()),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = e
        .call(
            Some(MEMBER),
            Method::PUT,
            "/cinema/titles/1/rating",
            Some(json!({ "score": 11 })),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = e
        .call(
            Some(MEMBER),
            Method::PUT,
            "/cinema/titles/1/progress",
            Some(json!({ "nonsense": 1 })),
        )
        .await;
    assert!(s.is_client_error());
}
