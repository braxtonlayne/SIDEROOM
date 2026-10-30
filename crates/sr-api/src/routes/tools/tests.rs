//! Endpoint tests that need no database: authentication on every route, validation that runs before any
//! query, and the OpenAPI fragment. Behaviour against real tables is covered by `sr-userfeat/tests/tools_db.rs`.

use super::{openapi, router};
use crate::routes::userfeat::UserFeatDeps;
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
use sr_userfeat::{Card, Poster, SendFailure, UserFeatures};
use std::sync::Arc;
use tower::ServiceExt;

const MEMBER: i64 = 20;
const NOTE: &str = "123e4567-e89b-12d3-a456-426614174000";

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
    let features = FeatureService::new(
        Arc::new(FeatureRegistry::new()),
        Arc::new(InMemorySettingsStore::new()),
        None,
    );
    let deps = UserFeatDeps {
        uf: UserFeatures::new(pool, features, Arc::new(|| 4242)),
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

type Call = (Method, String, Option<Value>);

fn table() -> Value {
    json!({ "name": "Chest", "rows": [{ "label": "Sword", "weight": 3 }] })
}

fn routes() -> Vec<Call> {
    let note = format!("/users/@me/tools/notes/{NOTE}");
    vec![
        (Method::GET, "/users/@me/tools/notes".into(), None),
        (
            Method::PUT,
            note.clone(),
            Some(json!({ "ciphertext": "AAEC" })),
        ),
        (Method::DELETE, note, None),
        (Method::GET, "/users/@me/tools/loot-tables".into(), None),
        (
            Method::POST,
            "/users/@me/tools/loot-tables".into(),
            Some(table()),
        ),
        (
            Method::PATCH,
            "/users/@me/tools/loot-tables/9".into(),
            Some(table()),
        ),
        (
            Method::DELETE,
            "/users/@me/tools/loot-tables/9".into(),
            None,
        ),
        (Method::GET, "/guilds/9/tools/loot-tables".into(), None),
        (
            Method::POST,
            "/guilds/9/tools/loot-tables".into(),
            Some(table()),
        ),
        (
            Method::PATCH,
            "/guilds/9/tools/loot-tables/9".into(),
            Some(table()),
        ),
        (Method::DELETE, "/guilds/9/tools/loot-tables/9".into(), None),
        (
            Method::PUT,
            "/channels/9/voice-timer".into(),
            Some(json!({ "timer": null })),
        ),
    ]
}

#[tokio::test]
async fn every_route_needs_a_session() {
    let e = env().await;
    for (method, path, body) in routes() {
        let (status, _) = e.call(false, method.clone(), &path, body).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path}");
    }
}

#[tokio::test]
async fn notes_are_validated_before_any_query() {
    let e = env().await;
    let put = |id: String, body: Value| {
        let e = &e;
        async move {
            e.call(
                true,
                Method::PUT,
                &format!("/users/@me/tools/notes/{id}"),
                Some(body),
            )
            .await
        }
    };
    // Not Base64.
    let (status, body) = put(NOTE.into(), json!({ "ciphertext": "not base64!!" })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], 50035);
    // Far over the cap, refused before it is decoded.
    let (status, _) = put(NOTE.into(), json!({ "ciphertext": "A".repeat(100_000) })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    // Empty ciphertext and a bad id never reach the database.
    let (status, _) = put(NOTE.into(), json!({ "ciphertext": "" })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = put("not-a-uuid".into(), json!({ "ciphertext": "AAEC" })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    // A missing field.
    let (status, _) = put(NOTE.into(), json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn loot_tables_are_validated_before_any_query() {
    let e = env().await;
    for body in [
        json!({ "name": "", "rows": [{ "label": "a", "weight": 1 }] }),
        json!({ "name": "n", "rows": [] }),
        json!({ "name": "n", "rows": [{ "label": "a", "weight": 0 }] }),
        json!({ "name": "n", "rows": [{ "label": "a", "weight": 10001 }] }),
        json!({ "name": "n", "rows": [{ "label": "a", "weight": -1 }] }),
        json!({ "name": "n" }),
    ] {
        let (status, b) = e
            .call(
                true,
                Method::POST,
                "/users/@me/tools/loot-tables",
                Some(body.clone()),
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}: {b}");
    }
    // A malformed table id is "not found" before any query.
    let (status, b) = e
        .call(
            true,
            Method::DELETE,
            "/users/@me/tools/loot-tables/abc",
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{b}");
}

#[test]
fn openapi_lists_every_path() {
    let doc = serde_json::to_value(openapi()).unwrap();
    let documented: Vec<&String> = doc["paths"].as_object().unwrap().keys().collect();
    let matches = |doc: &str, path: &str| {
        let (d, p): (Vec<_>, Vec<_>) = (doc.split('/').collect(), path.split('/').collect());
        d.len() == p.len() && d.iter().zip(&p).all(|(d, p)| d.starts_with('{') || d == p)
    };
    for (_, path, _) in routes() {
        assert!(
            documented.iter().any(|d| matches(d, &path)),
            "{path} is not documented"
        );
    }
}
