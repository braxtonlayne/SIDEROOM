//! Endpoint tests for the mount routes over the in-memory repositories: the whole CRUD, the caps, the
//! cross-tenant 404, the guild 403s, session-only access, input validation, and that no body has the secret.

use super::mounts::{MountsDeps, router};
use crate::{
    ApiSettings, AppState,
    auth::{
        BotIdentity, csrf,
        memory::{MemoryBots, MemorySessions},
    },
    build_router,
    routes::settings::SettingsDeps,
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
use sr_chat::guild_rules::plan_new_guild;
use sr_chat::memory::MemoryBackend;
use sr_chat::repo::GuildRepository;
use sr_chat::settings::memory::MemorySettingsRepo;
use sr_chat::{Member, Role};
use sr_core::permissions::Permissions;
use sr_core::{Clock, ManualClock, RoleId, SnowflakeGenerator, UserId};
use sr_storage::ObjectStoreStorage;
use sr_storage::mounts::{MemoryMountRepo, MountError, Sealer};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tower::ServiceExt;

const T0: i64 = 1_800_000_000_000;
const SECRET: &str = "wJalrXUtnFEMI/K7MDENG/SeedSecretValue";
const OWNER: u64 = 1; // owns the server
const MEMBER: u64 = 2; // member, no roles
const OUTSIDER: u64 = 3; // not in the server
const MANAGER: u64 = 4; // member with MANAGE_GUILD

struct XorSealer;
impl Sealer for XorSealer {
    fn seal(&self, aad: &[u8], plain: &[u8]) -> Result<Vec<u8>, MountError> {
        let mut v = aad.to_vec();
        v.push(b'|');
        v.extend(plain.iter().map(|b| b ^ 0x5a));
        Ok(v)
    }
    fn open(&self, aad: &[u8], blob: &[u8]) -> Result<Vec<u8>, MountError> {
        let rest = blob.strip_prefix(aad).and_then(|r| r.strip_prefix(b"|"));
        rest.map(|r| r.iter().map(|b| b ^ 0x5a).collect())
            .ok_or(MountError::Unseal)
    }
}

struct Env {
    app: Router,
    tokens: Vec<(u64, String)>,
    guild: String,
}

async fn env() -> Env {
    let clock = Arc::new(ManualClock::new(T0));
    let dyn_clock: Arc<dyn Clock> = clock.clone();
    let backend = MemoryBackend::new();
    let ids = SnowflakeGenerator::new(dyn_clock.clone(), 9, 9).unwrap();
    let bp = plan_new_guild(UserId::new(OWNER), "Club", T0, || ids.generate()).unwrap();
    let guild = bp.guild.id;
    backend.insert_guild(&bp).await.unwrap();
    for user in [MEMBER, MANAGER] {
        backend.data().members.push(Member {
            guild_id: guild,
            user_id: UserId::new(user),
            nick: None,
            role_ids: vec![],
            joined_at_ms: T0,
            communication_disabled_until_ms: None,
            pending: false,
            avatar_key: None,
            banner_key: None,
            bio: None,
            extras: Default::default(),
        });
    }
    let mgr = RoleId::new(ids.generate().get());
    {
        let mut d = backend.data();
        d.roles.push(Role {
            id: mgr,
            guild_id: guild,
            name: "mods".into(),
            color: 0,
            secondary_color: None,
            tertiary_color: None,
            hoist: false,
            position: 1,
            permissions: Permissions::MANAGE_GUILD,
            mentionable: false,
            managed: false,
            unicode_emoji: None,
            icon_key: None,
        });
        for m in d.members.iter_mut().filter(|m| m.user_id.get() == MANAGER) {
            m.role_ids.push(mgr);
        }
    }

    let counter = Arc::new(AtomicU64::new(0x9e37_79b9_7f4a_7c15));
    let entropy: crate::routes::settings::Entropy = {
        let c = counter.clone();
        Arc::new(move || c.fetch_add(0x1234_5678_9abc_def1, Ordering::SeqCst))
    };
    let settings_deps = SettingsDeps::new(
        Arc::new(MemorySettingsRepo::default()),
        Arc::new(backend.clone()),
        Arc::new(ObjectStoreStorage::in_memory()),
        dyn_clock.clone(),
        entropy,
        1,
        5,
    )
    .unwrap();

    let sessions = Arc::new(MemorySessions::new());
    let mut tokens = Vec::new();
    for (n, uid) in [OWNER, MEMBER, OUTSIDER, MANAGER].into_iter().enumerate() {
        let t = SessionToken::generate().unwrap();
        sessions
            .create(SessionRecord {
                id: 100 + n as i64,
                user_id: i64::try_from(uid).unwrap(),
                token_hash: t.hash(),
                created_at: 0,
                last_seen_at: i64::MAX / 4,
                expires_at: i64::MAX / 4,
                device_name: None,
            })
            .await
            .unwrap();
        tokens.push((uid, t.expose().to_owned()));
    }
    let bots = Arc::new(MemoryBots::new());
    bots.insert(
        "botty",
        BotIdentity {
            user_id: 77,
            application_id: 1,
        },
    );

    let next = Arc::new(AtomicU64::new(7000));
    let policy = sr_net::Policy {
        allow_private_for_tests: true,
        ..Default::default()
    };
    let mut mounts = MountsDeps::new(
        MemoryMountRepo::shared(),
        Arc::new(XorSealer),
        Arc::new(move || next.fetch_add(1, Ordering::SeqCst)),
        policy,
    );
    mounts.max_per_user = 2;
    mounts.max_per_guild = 1;

    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let state = AppState::builder()
        .settings(settings)
        .sessions(sessions)
        .bots(bots)
        .extension(Arc::new(settings_deps))
        .extension(Arc::new(mounts))
        .build();
    Env {
        app: build_router(state, router()),
        tokens,
        guild: guild.get().to_string(),
    }
}

impl Env {
    fn token(&self, uid: u64) -> &str {
        &self.tokens.iter().find(|(u, _)| *u == uid).unwrap().1
    }

    async fn call(
        &self,
        who: Option<u64>,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value, String) {
        let mut req = Request::builder()
            .method(method.clone())
            .uri(format!("/api/v1{path}"))
            .header("origin", "http://localhost:8080");
        if let Some(uid) = who {
            let t = self.token(uid);
            req = req.header("cookie", format!("sr_session={t}"));
            if method != Method::GET {
                req = req.header("x-sr-csrf", csrf::token_for(t));
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
        let text = String::from_utf8_lossy(&bytes).into_owned();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            text,
        )
    }
}

fn create_body(label: &str) -> Value {
    json!({
        "label": label, "provider": "garage", "endpoint": "http://127.0.0.1:3900",
        "region": "garage", "bucket": "shelf", "prefix": "/media/", "path_style": true,
        "access_key_id": "GK0123456789abcd", "secret_access_key": SECRET,
    })
}

fn err_code(v: &Value) -> &str {
    v["errors"]["mount"]["_errors"][0]["code"]
        .as_str()
        .unwrap_or("")
}

#[tokio::test]
async fn user_mount_crud_caps_and_no_secret_in_any_body() {
    let e = env().await;
    let base = "/users/@me/storage/mounts";
    let mut bodies = Vec::new();

    let (st, m, t) = e
        .call(Some(MEMBER), Method::POST, base, Some(create_body("Home")))
        .await;
    bodies.push(t);
    assert_eq!(st, StatusCode::CREATED, "{m}");
    assert_eq!(m["owner_kind"], "user");
    assert_eq!(m["owner_id"], MEMBER.to_string());
    assert_eq!(m["key_hint"], "abcd");
    assert_eq!(m["prefix"], "media/");
    assert_eq!(m["status"], "active");
    assert!(m["cors_ok_at"].is_null() && m["last_test"].is_null());
    assert!(m.get("secret_access_key").is_none() && m.get("secret_key_enc").is_none());
    let id = m["id"].as_str().unwrap().to_owned();

    // Label taken (case-insensitive) is a stable 409 code.
    let (st, err, t) = e
        .call(Some(MEMBER), Method::POST, base, Some(create_body("home")))
        .await;
    bodies.push(t);
    assert_eq!(st, StatusCode::CONFLICT);
    assert_eq!(err_code(&err), "MOUNT_LABEL_TAKEN");

    let (st, _, t) = e
        .call(Some(MEMBER), Method::POST, base, Some(create_body("Work")))
        .await;
    bodies.push(t);
    assert_eq!(st, StatusCode::CREATED);
    let (st, err, t) = e
        .call(Some(MEMBER), Method::POST, base, Some(create_body("Third")))
        .await;
    bodies.push(t);
    assert_eq!(st, StatusCode::CONFLICT);
    assert_eq!(err_code(&err), "MOUNT_CAP_REACHED");

    let (st, list, t) = e.call(Some(MEMBER), Method::GET, base, None).await;
    bodies.push(t);
    assert_eq!(st, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 2);

    // Patch: rename, disable, re-key.
    let item = format!("{base}/{id}");
    let (st, p, t) = e
        .call(
            Some(MEMBER),
            Method::PATCH,
            &item,
            Some(json!({"label": "Attic", "status": "disabled"})),
        )
        .await;
    bodies.push(t);
    assert_eq!(st, StatusCode::OK, "{p}");
    assert_eq!(p["label"], "Attic");
    assert_eq!(p["status"], "disabled");
    let (st, _, _) = e
        .call(
            Some(MEMBER),
            Method::PATCH,
            &item,
            Some(json!({"status": "failing"})),
        )
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "status failing is server-set");
    let (st, _, _) = e
        .call(
            Some(MEMBER),
            Method::PATCH,
            &item,
            Some(json!({"access_key_id": "NEWKEYID9999"})),
        )
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "a new key id needs its secret");
    let rekey =
        json!({"access_key_id": "NEWKEYID9999", "secret_access_key": "another-secret-value"});
    let (st, p, t) = e
        .call(Some(MEMBER), Method::PATCH, &item, Some(rekey))
        .await;
    bodies.push(t);
    assert_eq!(st, StatusCode::OK, "{p}");
    assert_eq!(p["key_hint"], "9999");

    // Another member's id is 404 on every verb; malformed ids too.
    let (st, _, _) = e
        .call(
            Some(OUTSIDER),
            Method::PATCH,
            &item,
            Some(json!({"label": "x"})),
        )
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _, _) = e.call(Some(OUTSIDER), Method::DELETE, &item, None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _, _) = e
        .call(Some(MEMBER), Method::DELETE, &format!("{base}/abc"), None)
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (_, other, _) = e.call(Some(OUTSIDER), Method::GET, base, None).await;
    assert_eq!(other.as_array().unwrap().len(), 0);

    // Delete frees the slot.
    let (st, _, _) = e.call(Some(MEMBER), Method::DELETE, &item, None).await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    let (st, _, t) = e
        .call(Some(MEMBER), Method::POST, base, Some(create_body("Third")))
        .await;
    bodies.push(t);
    assert_eq!(st, StatusCode::CREATED);

    for b in &bodies {
        assert!(!b.contains(SECRET), "a body leaked the secret: {b}");
        assert!(!b.contains("another-secret-value"));
        assert!(!b.contains("secret_key_enc"));
    }
}

#[tokio::test]
async fn guild_mounts_need_membership_and_manage() {
    let e = env().await;
    let base = format!("/guilds/{}/storage/mounts", e.guild);

    // Outsider 403, plain member 403, unauthenticated 401, unknown server 404.
    for (who, want) in [
        (Some(OUTSIDER), StatusCode::FORBIDDEN),
        (Some(MEMBER), StatusCode::FORBIDDEN),
        (None, StatusCode::UNAUTHORIZED),
    ] {
        let (st, _, _) = e.call(who, Method::GET, &base, None).await;
        assert_eq!(st, want, "GET as {who:?}");
        let (st, _, _) = e
            .call(who, Method::POST, &base, Some(create_body("S")))
            .await;
        assert_eq!(st, want, "POST as {who:?}");
    }
    let (st, _, _) = e
        .call(
            Some(OWNER),
            Method::GET,
            "/guilds/12345/storage/mounts",
            None,
        )
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    // The owner and a MANAGE_GUILD holder may; the cap for a server is 1 here.
    let (st, m, t) = e
        .call(Some(OWNER), Method::POST, &base, Some(create_body("Shelf")))
        .await;
    assert_eq!(st, StatusCode::CREATED, "{m}");
    assert_eq!(m["owner_kind"], "guild");
    assert_eq!(m["owner_id"], e.guild);
    assert!(!t.contains(SECRET));
    let id = m["id"].as_str().unwrap().to_owned();
    let (st, err, _) = e
        .call(
            Some(MANAGER),
            Method::POST,
            &base,
            Some(create_body("More")),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert_eq!(err_code(&err), "MOUNT_CAP_REACHED");
    let (st, list, _) = e.call(Some(MANAGER), Method::GET, &base, None).await;
    assert_eq!((st, list.as_array().unwrap().len()), (StatusCode::OK, 1));

    // A plain member cannot patch or delete; the manager can.
    let item = format!("{base}/{id}");
    let (st, _, _) = e
        .call(
            Some(MEMBER),
            Method::PATCH,
            &item,
            Some(json!({"label": "x"})),
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, _, _) = e.call(Some(MEMBER), Method::DELETE, &item, None).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, _, _) = e
        .call(
            Some(MANAGER),
            Method::PATCH,
            &item,
            Some(json!({"label": "Renamed"})),
        )
        .await;
    assert_eq!(st, StatusCode::OK);

    // A user mount id is not reachable through the guild route (404), nor the guild's through the user route.
    let (_, mine, _) = e
        .call(
            Some(MANAGER),
            Method::POST,
            "/users/@me/storage/mounts",
            Some(create_body("Mine")),
        )
        .await;
    let mine = mine["id"].as_str().unwrap().to_owned();
    let (st, _, _) = e
        .call(
            Some(MANAGER),
            Method::DELETE,
            &format!("{base}/{mine}"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _, _) = e
        .call(
            Some(MANAGER),
            Method::DELETE,
            &format!("/users/@me/storage/mounts/{id}"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _, _) = e.call(Some(MANAGER), Method::DELETE, &item, None).await;
    assert_eq!(st, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn bots_are_refused_and_bad_input_is_400() {
    let e = env().await;
    let base = "/users/@me/storage/mounts";
    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/v1{base}"))
        .header("authorization", "Bot botty")
        .body(Body::empty())
        .unwrap();
    let st = e.app.clone().oneshot(req).await.unwrap().status();
    assert_eq!(st, StatusCode::FORBIDDEN);

    let bad = |f: fn(&mut Value)| {
        let mut b = create_body("Ok");
        f(&mut b);
        b
    };
    let cases: Vec<(&str, Value)> = vec![
        ("provider", bad(|b| b["provider"] = json!("gcs"))),
        ("label empty", bad(|b| b["label"] = json!("  "))),
        ("bucket chars", bad(|b| b["bucket"] = json!("a/b"))),
        ("prefix dotdot", bad(|b| b["prefix"] = json!("a/../b"))),
        ("secret empty", bad(|b| b["secret_access_key"] = json!(""))),
        ("key id spaces", bad(|b| b["access_key_id"] = json!("a b"))),
        (
            "endpoint missing",
            bad(|b| {
                b["provider"] = json!("minio");
                b["endpoint"] = Value::Null;
            }),
        ),
        (
            "endpoint path",
            bad(|b| b["endpoint"] = json!("http://127.0.0.1:3900/x")),
        ),
        (
            "endpoint scheme",
            bad(|b| b["endpoint"] = json!("ftp://127.0.0.1")),
        ),
    ];
    for (name, body) in cases {
        let (st, resp, text) = e.call(Some(MEMBER), Method::POST, base, Some(body)).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{name}: {resp}");
        assert!(!text.contains(SECRET), "{name}");
    }
}

#[tokio::test]
async fn private_endpoints_are_refused_by_the_default_policy() {
    // The default policy (no test allowance) refuses loopback and private addresses and plain http.
    let e = env_with_default_policy().await;
    for endpoint in [
        "https://127.0.0.1:3900",
        "https://10.0.0.5",
        "http://s3.example.test",
    ] {
        let mut body = create_body("X");
        body["endpoint"] = json!(endpoint);
        let (st, resp, _) = e
            .call(
                Some(MEMBER),
                Method::POST,
                "/users/@me/storage/mounts",
                Some(body),
            )
            .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{endpoint}: {resp}");
    }
}

async fn env_with_default_policy() -> Env {
    // Same wiring as `env` but with the production SSRF policy.
    let e = env().await;
    let sessions = Arc::new(MemorySessions::new());
    let t = SessionToken::generate().unwrap();
    sessions
        .create(SessionRecord {
            id: 900,
            user_id: i64::try_from(MEMBER).unwrap(),
            token_hash: t.hash(),
            created_at: 0,
            last_seen_at: i64::MAX / 4,
            expires_at: i64::MAX / 4,
            device_name: None,
        })
        .await
        .unwrap();
    let next = Arc::new(AtomicU64::new(8000));
    let mounts = MountsDeps::new(
        MemoryMountRepo::shared(),
        Arc::new(XorSealer),
        Arc::new(move || next.fetch_add(1, Ordering::SeqCst)),
        sr_net::Policy::default(),
    );
    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let state = AppState::builder()
        .settings(settings)
        .sessions(sessions)
        .extension(Arc::new(mounts))
        .build();
    Env {
        app: build_router(state, router()),
        tokens: vec![(MEMBER, t.expose().to_owned())],
        guild: e.guild,
    }
}
