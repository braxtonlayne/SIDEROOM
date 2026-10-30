//! Status rings over HTTP against Postgres (PRF-RNG-001; run with `SR_TEST_DATABASE_URL`, `--ignored`,
//! `--test-threads=1`): a stranger, a blocked person and a member of a server that was not chosen get exactly the
//! answer a missing ring gets, on every route that names a ring, and the author's viewer list answers nobody else.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use sr_api::{
    ApiSettings, AppState,
    auth::{csrf, memory::MemorySessions},
    build_router,
    routes::userfeat::{RingDeps, UserFeatDeps, router},
};
use sr_auth::{
    repo::{SessionRecord, SessionRepository},
    session::SessionToken,
};
use sr_core::{SnowflakeGenerator, SystemClock};
use sr_db::testing::TestDb;
use sr_features::{FeatureRegistry, FeatureService, InMemorySettingsStore, ScopeRef};
use sr_storage::ObjectStoreStorage;
use sr_storage::mount_storage::MountStorages;
use sr_storage::mounts::MemoryMountRepo;
use sr_userfeat::{
    Card, Poster, SendFailure, UserFeatures,
    features::{RINGS_ID, register_all},
};
use tower::ServiceExt;

/// A 1x1 PNG.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0,
    0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

struct NoPoster;

#[async_trait]
impl Poster for NoPoster {
    async fn post(&self, _: i64, _: Card) -> Result<i64, String> {
        Err("not used".into())
    }
    async fn dm(&self, _: i64, _: Card) -> Result<(i64, i64), String> {
        Err("not used".into())
    }
    async fn send_as(&self, _: i64, _: i64, _: &str) -> Result<i64, SendFailure> {
        Err(SendFailure::Permanent("not used".into()))
    }
    async fn react(&self, _: i64, _: i64, _: &str) {}
    async fn edit(&self, _: i64, _: i64, _: Card) {}
    async fn add_role(&self, _: i64, _: i64, _: i64) -> bool {
        false
    }
    async fn remove_role(&self, _: i64, _: i64, _: i64) -> bool {
        false
    }
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    bytes: Vec<u8>,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.bytes).unwrap_or(Value::Null)
    }
}

struct Http {
    app: Router,
    sessions: Arc<MemorySessions>,
    next: std::sync::atomic::AtomicI64,
}

impl Http {
    async fn login(&self, user: i64) -> String {
        let t = SessionToken::generate().unwrap();
        self.sessions
            .create(SessionRecord {
                id: self.next.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
                user_id: user,
                token_hash: t.hash(),
                created_at: 0,
                last_seen_at: i64::MAX / 4,
                expires_at: i64::MAX / 4,
                device_name: None,
            })
            .await
            .unwrap();
        t.expose().to_owned()
    }

    async fn send(&self, token: &str, method: Method, path: &str, body: Option<Body>) -> Reply {
        let mut req = Request::builder()
            .method(method.clone())
            .uri(format!("/api/v1{path}"))
            .header("origin", "http://localhost:8080")
            .header("cookie", format!("sr_session={token}"));
        if method != Method::GET {
            req = req.header("x-sr-csrf", csrf::token_for(token));
        }
        let resp = self
            .app
            .clone()
            .oneshot(req.body(body.unwrap_or_else(Body::empty)).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = resp
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec();
        Reply {
            status,
            headers,
            bytes,
        }
    }

    async fn call(&self, token: &str, method: Method, path: &str, json: Option<Value>) -> Reply {
        self.send(token, method, path, json.map(|j| Body::from(j.to_string())))
            .await
    }

    /// A JSON POST (the content type matters to the extractor).
    async fn post_json(&self, token: &str, path: &str, j: Value) -> Reply {
        let req = Request::builder()
            .method(Method::POST)
            .uri(format!("/api/v1{path}"))
            .header("origin", "http://localhost:8080")
            .header("cookie", format!("sr_session={token}"))
            .header("x-sr-csrf", csrf::token_for(token))
            .header("content-type", "application/json")
            .body(Body::from(j.to_string()))
            .unwrap();
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = resp
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec();
        Reply {
            status,
            headers,
            bytes,
        }
    }
}

async fn account(pool: &PgPool, id: i64, name: &str) {
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $1, $2, $3, 'x')",
    )
    .bind(id)
    .bind(name)
    .bind(format!("{name}@example.test"))
    .execute(pool)
    .await
    .unwrap();
}

async fn guild(pool: &PgPool, id: i64, owner: i64, members: &[i64]) {
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'Club', $2)")
        .bind(id)
        .bind(owner)
        .execute(pool)
        .await
        .unwrap();
    for m in std::iter::once(&owner).chain(members) {
        sqlx::query(
            "INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(*m)
        .execute(pool)
        .await
        .unwrap();
    }
}

async fn rel(pool: &PgPool, a: i64, b: i64, kind: i16) {
    sqlx::query("INSERT INTO relationships (user_id, target_id, type) VALUES ($1, $2, $3)")
        .bind(a)
        .bind(b)
        .bind(kind)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn an_outsider_gets_the_answer_a_missing_ring_gets_on_every_route() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let (a, friend, member, other, stranger, blocked) = (101, 102, 103, 104, 105, 106);
    for (id, name) in [
        (a, "author"),
        (friend, "friend"),
        (member, "member"),
        (other, "other"),
        (stranger, "stranger"),
        (blocked, "blocked"),
    ] {
        account(&pool, id, name).await;
    }
    sqlx::query(
        "INSERT INTO storage_mounts (id, owner_kind, owner_id, label, provider, endpoint, bucket, access_key_id, secret_key_enc) \
         VALUES (1, 'platform', 0, 'Instance', 's3', 'https://s3.example.test', 'b', 'k', '\\x00')",
    )
    .execute(&pool)
    .await
    .unwrap();
    // friend: mutual friend. blocked: a mutual friend and a member of the chosen server, then blocked by the author.
    for p in [friend, blocked] {
        rel(&pool, a, p, 1).await;
        rel(&pool, p, a, 1).await;
    }
    guild(&pool, 201, a, &[member, blocked]).await; // the chosen server
    guild(&pool, 202, a, &[other]).await; // a server the author did not choose
    sqlx::query("UPDATE relationships SET type = 2 WHERE user_id = $1 AND target_id = $2")
        .bind(a)
        .bind(blocked)
        .execute(&pool)
        .await
        .unwrap();

    let mut registry = FeatureRegistry::new();
    register_all(&mut registry).unwrap();
    let features = FeatureService::new(
        Arc::new(registry),
        Arc::new(InMemorySettingsStore::new()),
        None,
    );
    // Rings start off (the operator turns them on).
    features
        .set_enabled(RINGS_ID, &ScopeRef::Platform, Some(true))
        .await
        .unwrap();
    let ids = Arc::new(SnowflakeGenerator::new(SystemClock, 28, 28).unwrap());
    let uf = UserFeatures::new(
        pool.clone(),
        features,
        Arc::new(move || ids.generate().get() as i64),
    );
    let storage = Arc::new(ObjectStoreStorage::in_memory());
    let sessions = Arc::new(MemorySessions::new());
    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let state = AppState::builder()
        .settings(settings)
        .sessions(sessions.clone())
        .extension(Arc::new(UserFeatDeps {
            uf: uf.clone(),
            poster: Arc::new(NoPoster),
            net_policy: sr_net::Policy::default(),
        }))
        .extension(Arc::new(RingDeps {
            storages: Arc::new(MountStorages::instance_only(storage)),
            mounts: Arc::new(MemoryMountRepo::new()),
        }))
        .build();
    let h = Http {
        app: build_router(state, router()),
        sessions,
        next: std::sync::atomic::AtomicI64::new(1000),
    };
    let t_a = h.login(a).await;

    // A friends ring (text) and a servers ring (a real photo upload) of the author.
    let r = h
        .post_json(
            &t_a,
            "/users/@me/rings",
            json!({"kind": "text", "text": "hello friends", "audience": "friends"}),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json());
    let friends_ring = r.json()["id"].as_str().unwrap().to_owned();
    let r = h
        .send(
            &t_a,
            Method::POST,
            "/users/@me/rings/media?kind=photo&audience=guilds&guild_ids=201&text=pic",
            Some(Body::from(PNG.to_vec())),
        )
        .await;
    assert_eq!(
        r.status,
        StatusCode::CREATED,
        "{:?}",
        String::from_utf8_lossy(&r.bytes)
    );
    let server_ring = r.json()["id"].as_str().unwrap().to_owned();

    // The routes that name a ring, with the method each uses.
    let routes = |id: &str| {
        vec![
            (Method::GET, format!("/rings/{id}")),
            (Method::GET, format!("/rings/{id}/media")),
            (Method::POST, format!("/rings/{id}/view")),
            (Method::GET, format!("/users/@me/rings/{id}/viewers")),
            (Method::DELETE, format!("/users/@me/rings/{id}")),
        ]
    };

    // The answer for a ring that does not exist, per route and per caller (the body must not depend on who asks).
    let missing_id = "900000000000000001";
    let t_stranger = h.login(stranger).await;
    let mut baseline = Vec::new();
    for (m, p) in routes(missing_id) {
        let r = h.call(&t_stranger, m.clone(), &p, None).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND, "{m} {p}");
        baseline.push(r.json());
    }

    // Who may not see which ring, and the answer they must get.
    let outsiders = [
        ("stranger", stranger, vec![&friends_ring, &server_ring]),
        ("blocked", blocked, vec![&friends_ring, &server_ring]),
        (
            "other-server member",
            other,
            vec![&friends_ring, &server_ring],
        ),
        // Friends see the friends ring only; a member of the chosen server sees the servers ring only.
        ("friend", friend, vec![&server_ring]),
        ("chosen-server member", member, vec![&friends_ring]),
    ];
    for (name, user, rings) in outsiders {
        let tok = h.login(user).await;
        for id in rings {
            for (i, (m, p)) in routes(id).into_iter().enumerate() {
                let r = h.call(&tok, m.clone(), &p, None).await;
                assert_eq!(r.status, StatusCode::NOT_FOUND, "{name}: {m} {p}");
                assert_eq!(
                    r.json(),
                    baseline[i],
                    "{name}: {m} {p} must read like a missing ring"
                );
            }
        }
        // Nothing of the author's reaches their feed beyond what they may see.
        let feed = h.call(&tok, Method::GET, "/rings", None).await;
        assert_eq!(feed.status, StatusCode::OK);
        let seen: usize = feed.json().as_array().unwrap().len();
        let expected = usize::from(name == "friend" || name == "chosen-server member");
        assert_eq!(seen, expected, "{name}: {:?}", feed.json());
    }

    // The rightful audience gets the ring, and the viewer list stays the author's.
    let t_friend = h.login(friend).await;
    let r = h
        .call(
            &t_friend,
            Method::GET,
            &format!("/rings/{friends_ring}"),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json()["guild_ids"], json!([]));
    let r = h
        .call(
            &t_friend,
            Method::POST,
            &format!("/rings/{friends_ring}/view"),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let viewers_path = format!("/users/@me/rings/{friends_ring}/viewers");
    let r = h.call(&t_friend, Method::GET, &viewers_path, None).await;
    assert_eq!(
        r.status,
        StatusCode::NOT_FOUND,
        "a viewer does not get the list"
    );
    let r = h.call(&t_a, Method::GET, &viewers_path, None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json().as_array().unwrap().len(), 1);

    let t_member = h.login(member).await;
    let media = h
        .call(
            &t_member,
            Method::GET,
            &format!("/rings/{server_ring}/media"),
            None,
        )
        .await;
    assert_eq!(media.status, StatusCode::OK);
    assert!(
        media.headers["content-type"]
            .to_str()
            .unwrap()
            .starts_with("image/")
    );
    assert_eq!(media.headers["cache-control"], "private, no-store");
    assert_eq!(media.headers["x-content-type-options"], "nosniff");
    assert!(!media.bytes.is_empty());

    // Another member cannot delete the author's ring; the author can, and it is then gone for the audience.
    let r = h
        .call(
            &t_member,
            Method::DELETE,
            &format!("/users/@me/rings/{server_ring}"),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let r = h
        .call(
            &t_a,
            Method::DELETE,
            &format!("/users/@me/rings/{server_ring}"),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let gone = h
        .call(
            &t_member,
            Method::GET,
            &format!("/rings/{server_ring}"),
            None,
        )
        .await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
    assert_eq!(gone.json(), baseline[0]);
    db.cleanup().await.unwrap();
}
