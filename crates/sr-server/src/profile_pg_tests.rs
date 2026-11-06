//! Profiles, wave 50 (PRF-PRO-001; D-502 to D-505) over HTTP against Postgres (run with `SR_TEST_DATABASE_URL`,
//! `--ignored`, `--test-threads=1`):
//!
//! * the showcase mint is the owner's grant for exactly the objects the page names, to the audience `media_tier`
//!   admits, on the owner's own mount; every other caller and every other object get the page's 404, and a
//!   delisted object stops minting at once;
//! * the ring flag of the page is true only when the profile's `ring_tier` AND the ring's own audience admit the
//!   viewer, and the public page carries no such field.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use sr_api::{
    ApiSettings, AppState,
    auth::{csrf, memory::MemorySessions},
    build_router,
    routes::{packages::PackagesDeps, storage::ScopeDeps, userfeat::UserFeatDeps},
};
use sr_auth::{
    repo::{SessionRecord, SessionRepository},
    session::SessionToken,
};
use sr_core::{SnowflakeGenerator, SystemClock};
use sr_db::testing::TestDb;
use sr_features::{FeatureRegistry, FeatureService, InMemorySettingsStore};
use sr_storage::mounts::{MountInput, MountOwner, MountRepo as _, NewMount, Provider, Secret};
use sr_userfeat::{Card, Poster, SendFailure, UserFeatures, features::register_all};
use tower::ServiceExt;

const KEY_HEX: &str = "0707070707070707070707070707070707070707070707070707070707070707";

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

    /// `(status, cache-control, json)`; `token` `None` sends no session.
    async fn get(&self, token: Option<&str>, path: &str) -> (StatusCode, String, Value) {
        let mut req = Request::builder()
            .method(Method::GET)
            .uri(format!("/api/v1{path}"))
            .header("origin", "http://localhost:8080");
        if let Some(t) = token {
            req = req.header("cookie", format!("sr_session={t}"));
        }
        let resp = self
            .app
            .clone()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let cache = resp
            .headers()
            .get("cache-control")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            cache,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn put(&self, token: &str, path: &str, body: Value) -> StatusCode {
        let req = Request::builder()
            .method(Method::PUT)
            .uri(format!("/api/v1{path}"))
            .header("origin", "http://localhost:8080")
            .header("cookie", format!("sr_session={token}"))
            .header("x-sr-csrf", csrf::token_for(token))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        self.app.clone().oneshot(req).await.unwrap().status()
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

async fn rel(pool: &PgPool, a: i64, b: i64, kind: i16) {
    sqlx::query("INSERT INTO relationships (user_id, target_id, type) VALUES ($1, $2, $3)")
        .bind(a)
        .bind(b)
        .bind(kind)
        .execute(pool)
        .await
        .unwrap();
}

async fn friends(pool: &PgPool, a: i64, b: i64) {
    rel(pool, a, b, 1).await;
    rel(pool, b, a, 1).await;
}

/// A member's mount with CORS ready and one indexed object at `key` of `class`: `(mount, object hash)`.
async fn mount_with(pool: &PgPool, id: i64, owner: i64, key: &str, class: &str) -> (i64, String) {
    let secret = sr_db::auth_repo::parse_secret_key(KEY_HEX).unwrap();
    let new = NewMount::seal(
        &sr_db::sealed::SecretBox::new(secret),
        id,
        MountInput {
            owner: MountOwner::User(owner),
            label: format!("Shelf {id}"),
            provider: Provider::Minio,
            endpoint: "http://127.0.0.1:9".into(),
            region: "us-east-1".into(),
            bucket: "shelf".into(),
            prefix: "lib".into(),
            path_style: true,
            access_key_id: "AKIAPROFILETEST".into(),
        },
        &Secret::new("profile-test-secret"),
    )
    .unwrap();
    sr_db::mounts_repo::PgMountRepo::new(pool.clone())
        .create(new, None)
        .await
        .unwrap();
    sqlx::query("UPDATE storage_mounts SET cors_ok_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO storage_objects (id, mount_id, key, kind, class) VALUES ($1, $2, $3, 'music', $4)",
    )
    .bind(id + 1)
    .bind(id)
    .bind(key)
    .bind(class)
    .execute(pool)
    .await
    .unwrap();
    (id, sr_storage::shelf::path_hash(key))
}

async fn rig(pool: &PgPool) -> Http {
    let mut registry = FeatureRegistry::new();
    register_all(&mut registry).unwrap();
    let features = FeatureService::new(
        Arc::new(registry),
        Arc::new(InMemorySettingsStore::new()),
        None,
    );
    let ids = Arc::new(SnowflakeGenerator::new(SystemClock, 29, 29).unwrap());
    let id_src: Arc<dyn Fn() -> i64 + Send + Sync> = Arc::new(move || ids.generate().get() as i64);
    let uf = UserFeatures::new(pool.clone(), features, id_src.clone());
    let secret = sr_db::auth_repo::parse_secret_key(KEY_HEX).unwrap();
    let mounts: Arc<dyn sr_storage::mounts::MountRepo> =
        Arc::new(sr_db::mounts_repo::PgMountRepo::new(pool.clone()));
    let sessions = Arc::new(MemorySessions::new());
    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let state = AppState::builder()
        .settings(settings)
        .sessions(sessions.clone())
        .extension(Arc::new(PackagesDeps {
            service: sr_packages::service::PackageService::new(pool.clone(), id_src),
        }))
        .extension(Arc::new(ScopeDeps {
            mounts,
            store: Arc::new(sr_db::scopes_repo::PgScopeFacts::new(pool.clone())),
            sealer: Arc::new(sr_db::sealed::SecretBox::new(secret)),
            guard: None,
        }))
        .extension(Arc::new(UserFeatDeps {
            uf,
            poster: Arc::new(NoPoster),
            net_policy: sr_net::Policy::default(),
        }))
        .build();
    Http {
        app: build_router(state, sr_api::routes::packages::router()),
        sessions,
        next: std::sync::atomic::AtomicI64::new(1000),
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_showcase_mint_is_the_owners_grant_and_dies_with_the_object() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let (owner, friend, stranger, blocked) = (301, 302, 303, 304);
    for (id, name) in [
        (owner, "showowner"),
        (friend, "showfriend"),
        (stranger, "showstranger"),
        (blocked, "showblocked"),
    ] {
        account(&pool, id, name).await;
    }
    friends(&pool, owner, friend).await;
    friends(&pool, owner, blocked).await;
    rel(&pool, owner, blocked, 2).await; // blocked by the owner after being a friend
    let (mount, song) = mount_with(&pool, 9001, owner, "Music/song.flac", "track").await;
    let (_, picture) = mount_with(&pool, 9101, owner, "Art/cover.png", "artwork").await;
    // The stranger's own mount: an object the owner may never put on their page.
    let (their_mount, theirs) =
        mount_with(&pool, 9201, stranger, "Music/theirs.mp3", "track").await;

    let h = rig(&pool).await;
    let t_owner = h.login(owner).await;
    let t_friend = h.login(friend).await;
    let t_stranger = h.login(stranger).await;
    let t_blocked = h.login(blocked).await;

    // Saving: an object on someone else's shelf, or a picture, is refused; the owner's own track is taken.
    let doc = |m: i64, hash: &str| {
        json!({"version": 1, "blocks": [{"id": "show", "type": "media_showcase", "x": 0, "y": 0, "w": 12, "h": 3,
            "config": {"items": [{"shelf_id": m.to_string(), "object_hash": hash, "caption": "live"}]}}]})
    };
    assert_eq!(
        h.put(
            &t_owner,
            "/users/@me/profile-doc",
            doc(their_mount, &theirs)
        )
        .await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        h.put(&t_owner, "/users/@me/profile-doc", doc(9101, &picture))
            .await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        h.put(&t_owner, "/users/@me/profile-doc", doc(mount, &song))
            .await,
        StatusCode::OK
    );
    assert_eq!(
        h.put(
            &t_owner,
            "/users/@me/profile-privacy",
            json!({"preset": "friends"})
        )
        .await,
        StatusCode::OK
    );

    let mint = |hash: &str| format!("/profile-pages/showowner/showcase/{hash}/url");
    // The owner and a friend the tier admits get a ten-minute address on the owner's mount, never cached.
    for t in [&t_owner, &t_friend] {
        let (status, cache, body) = h.get(Some(t), &mint(&song)).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(cache, "no-store");
        let url = body["url"].as_str().unwrap();
        assert!(
            url.contains("song.flac") && url.contains("X-Amz-Signature"),
            "{url}"
        );
    }
    // The friend's page lists the item, with its title and caption.
    let (status, _, page) = h.get(Some(&t_friend), "/profile-pages/showowner").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["showcase"][0]["object_hash"], song.as_str());
    assert_eq!(page["showcase"][0]["media"], "audio");
    assert_eq!(page["showcase"][0]["caption"], "live");

    // Everyone else, and every other object, gets the page's one 404.
    let (_, _, missing) = h.get(Some(&t_friend), &mint(&"0".repeat(64))).await;
    for (who, path) in [
        (Some(t_stranger.as_str()), mint(&song)),
        (Some(t_blocked.as_str()), mint(&song)),
        (None, mint(&song)),
        (Some(t_friend.as_str()), mint(&theirs)),
        (Some(t_friend.as_str()), mint(&picture)),
        (
            Some(t_friend.as_str()),
            "/profile-pages/nobody/showcase/x/url".to_owned(),
        ),
    ] {
        let (status, _, body) = h.get(who, &path).await;
        assert!(
            status == StatusCode::NOT_FOUND
                || (who.is_none() && status == StatusCode::UNAUTHORIZED),
            "{path}: {status}"
        );
        if who.is_some() {
            assert_eq!(body, missing, "{path}: the same 404 body");
        }
    }

    // A media tier stricter than friends hides the block, page and mint alike.
    assert_eq!(
        h.put(
            &t_owner,
            "/users/@me/profile-privacy",
            json!({"media_tier": "only_me"})
        )
        .await,
        StatusCode::OK
    );
    assert_eq!(
        h.get(Some(&t_friend), &mint(&song)).await.0,
        StatusCode::NOT_FOUND
    );
    let (_, _, page) = h.get(Some(&t_friend), "/profile-pages/showowner").await;
    assert!(page.get("showcase").is_none(), "{page}");
    assert_eq!(h.get(Some(&t_owner), &mint(&song)).await.0, StatusCode::OK);
    assert_eq!(
        h.put(
            &t_owner,
            "/users/@me/profile-privacy",
            json!({"media_tier": "friends"})
        )
        .await,
        StatusCode::OK
    );

    // A takedown cuts it at once: the item leaves the page and the mint is the 404.
    sqlx::query(
        "INSERT INTO shelf_delist_strikes (id, shelf_id, object_hash, path, subject_account, reason, delisted_by) \
         VALUES (1, $1, $2, 'Music/song.flac', $3, 'test', $3)",
    )
    .bind(mount)
    .bind(&song)
    .bind(owner)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE shelf_objects SET delisted_at = now(), \
           delist_strike_id = (SELECT id FROM shelf_delist_strikes LIMIT 1) \
         WHERE shelf_id = $1 AND object_hash = $2",
    )
    .bind(mount)
    .bind(&song)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        h.get(Some(&t_friend), &mint(&song)).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        h.get(Some(&t_owner), &mint(&song)).await.0,
        StatusCode::NOT_FOUND
    );
    let (_, _, page) = h.get(Some(&t_friend), "/profile-pages/showowner").await;
    assert!(page.get("showcase").is_none(), "{page}");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_page_ring_needs_the_profile_tier_and_the_rings_own_audience() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let (owner, friend, mutual) = (401, 402, 403);
    for (id, name) in [
        (owner, "ringpage"),
        (friend, "ringfriend"),
        (mutual, "ringmutual"),
    ] {
        account(&pool, id, name).await;
    }
    friends(&pool, owner, friend).await;
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES (501, 'Club', $1)")
        .bind(owner)
        .execute(&pool)
        .await
        .unwrap();
    for m in [owner, mutual] {
        sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES (501, $1)")
            .bind(m)
            .execute(&pool)
            .await
            .unwrap();
    }
    // A live friends-only text ring.
    sqlx::query(
        "INSERT INTO status_rings (id, author_id, kind, body, audience, expires_at) \
         VALUES (601, $1, 'text', 'hello', 'friends', now() + interval '23 hours')",
    )
    .bind(owner)
    .execute(&pool)
    .await
    .unwrap();
    let h = rig(&pool).await;
    let t_owner = h.login(owner).await;
    let t_friend = h.login(friend).await;
    let t_mutual = h.login(mutual).await;
    assert_eq!(
        h.put(
            &t_owner,
            "/users/@me/profile-doc",
            json!({"version": 1, "blocks": [{"id": "h", "type": "header", "x": 0, "y": 0, "w": 12, "h": 3, "config": {}}]}),
        )
        .await,
        StatusCode::OK
    );
    let set = |tier: &'static str| json!({"page_tier": "mutual_servers", "ring_tier": tier, "bio_tier": "mutual_servers"});
    let ring_of = |v: &Value| {
        v.get("ring_visible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };

    // ring_tier friends: the friend sees it; the mutual-server member does not (the ring is friends-only anyway).
    assert_eq!(
        h.put(&t_owner, "/users/@me/profile-privacy", set("friends"))
            .await,
        StatusCode::OK
    );
    let (_, _, p) = h.get(Some(&t_friend), "/profile-pages/ringpage").await;
    assert!(ring_of(&p), "friend, friends tier: {p}");
    let (_, _, p) = h.get(Some(&t_mutual), "/users/401/profile-page").await;
    assert!(!ring_of(&p), "mutual, friends tier: {p}");
    let (_, _, p) = h.get(Some(&t_owner), "/profile-pages/ringpage").await;
    assert!(ring_of(&p), "the owner always: {p}");

    // ring_tier mutual_servers: the profile tier admits the mutual member, the ring's own audience does not.
    assert_eq!(
        h.put(
            &t_owner,
            "/users/@me/profile-privacy",
            set("mutual_servers")
        )
        .await,
        StatusCode::OK
    );
    let (_, _, p) = h.get(Some(&t_mutual), "/profile-pages/ringpage").await;
    assert!(
        !ring_of(&p),
        "a friends-only ring stays hidden from a mutual-server member: {p}"
    );

    // ring_tier only_me: the same friend no longer sees it on the page.
    assert_eq!(
        h.put(&t_owner, "/users/@me/profile-privacy", set("only_me"))
            .await,
        StatusCode::OK
    );
    let (_, _, p) = h.get(Some(&t_friend), "/profile-pages/ringpage").await;
    assert!(!ring_of(&p), "friend, only_me tier: {p}");
    let (_, _, p) = h.get(Some(&t_owner), "/profile-pages/ringpage").await;
    assert!(ring_of(&p), "the owner still: {p}");

    // `public` is accepted for compatibility and never effective: the stored tier reads back clamped.
    assert_eq!(
        h.put(
            &t_owner,
            "/users/@me/profile-privacy",
            json!({"ring_tier": "public"})
        )
        .await,
        StatusCode::OK
    );
    let (_, _, v) = h.get(Some(&t_owner), "/users/@me/profile-privacy").await;
    assert_ne!(v["effective"]["ring_tier"], "public", "{v}");
}
