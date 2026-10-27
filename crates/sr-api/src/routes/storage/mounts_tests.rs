//! Endpoint tests for the mount routes over the in-memory repositories: the whole CRUD, the caps, the
//! cross-tenant 404, the guild 403s, session-only access, input validation, and that no body has the secret.

use super::mounts::MountsDeps;
use super::router;
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

/// Operator policy for a test: settings rows, and the standing the fake member lookup reports.
#[derive(Default)]
struct Extra {
    settings: Vec<(&'static str, Value)>,
    standing: Option<(bool, bool)>,
}

struct FakeStanding(bool, bool);
#[async_trait::async_trait]
impl sr_storage::mount_policy::MemberStanding for FakeStanding {
    async fn is_patron(&self, _user: i64) -> Result<bool, String> {
        Ok(self.0)
    }
    async fn is_staff(&self, _user: i64) -> Result<bool, String> {
        Ok(self.1)
    }
}

async fn env() -> Env {
    env_with(Extra::default()).await
}

async fn env_with(extra: Extra) -> Env {
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
    )
    .with_public_origin("https://chat.example.test");
    mounts.max_per_user = 2;
    mounts.max_per_guild = 1;
    if let Some((patron, staff)) = extra.standing {
        mounts = mounts.with_standing(Arc::new(FakeStanding(patron, staff)));
    }
    let admin = if extra.settings.is_empty() {
        None
    } else {
        let repo = Arc::new(sr_admin::memory::MemoryAdminRepo::default());
        let values: Vec<(String, Value)> = extra
            .settings
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect();
        let audit = sr_admin::AuditEntry {
            actor_id: 1,
            kind: sr_admin::AccessKind::AdminAction,
            action: "settings.test".into(),
            target_kind: None,
            target_id: None,
            reason: "test".into(),
            ip: None,
            detail: "{}".into(),
        };
        sr_admin::AdminRepository::put_settings(repo.as_ref(), &values, 1, &audit)
            .await
            .unwrap();
        Some(Arc::new(crate::routes::admin::AdminDeps::new(
            sr_admin::AdminService::new(repo),
        )))
    };

    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let mut builder = AppState::builder()
        .settings(settings)
        .sessions(sessions)
        .bots(bots)
        .extension(Arc::new(settings_deps))
        .extension(Arc::new(mounts));
    if let Some(a) = admin {
        builder = builder.extension(a);
    }
    let state = builder.build();
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

// ---------------------------------------------------------------- test and CORS routes

use std::sync::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// A fake S3 for one bucket named `shelf`: a healthy key whose CORS document is whatever was PUT last.
/// `deny_put` makes writes fail so the test goes red.
async fn fake_s3(deny_put: bool) -> (String, Arc<Mutex<Option<String>>>) {
    let doc: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let d = doc.clone();
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            let d = d.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let n = sock.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf).to_string();
                    let Some((head, body)) = text.split_once("\r\n\r\n") else {
                        continue;
                    };
                    let want = head
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|v| v.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if body.len() < want {
                        continue;
                    }
                    let mut first = head.lines().next().unwrap_or("").split(' ');
                    let (m, t) = (first.next().unwrap_or(""), first.next().unwrap_or(""));
                    let (status, reply) = match (m, t) {
                        ("HEAD", "/shelf") => (200, String::new()),
                        ("GET", t) if t.contains("list-type=2") => {
                            (200, "<ListBucketResult/>".to_owned())
                        }
                        ("PUT", t) if t.ends_with(".sideroom-probe") => {
                            if deny_put {
                                (403, "<Error><Code>AccessDenied</Code></Error>".to_owned())
                            } else {
                                (200, String::new())
                            }
                        }
                        ("GET", t) if t.ends_with(".sideroom-probe") => {
                            (200, "sideroom probe".to_owned())
                        }
                        ("DELETE", _) => (204, String::new()),
                        ("POST", t) if t.contains("uploads") => {
                            (200, "<R><UploadId>u1</UploadId></R>".to_owned())
                        }
                        ("GET", "/shelf?cors") => match d.lock().unwrap().clone() {
                            Some(x) => (200, x),
                            None => (
                                404,
                                "<Error><Code>NoSuchCORSConfiguration</Code></Error>".to_owned(),
                            ),
                        },
                        ("PUT", "/shelf?cors") => {
                            *d.lock().unwrap() = Some(body.to_owned());
                            (200, String::new())
                        }
                        ("GET", "/") => {
                            (403, "<Error><Code>AccessDenied</Code></Error>".to_owned())
                        }
                        _ => (500, String::new()),
                    };
                    let reply = if m == "HEAD" { String::new() } else { reply };
                    let resp = format!(
                        "HTTP/1.1 {status} X\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}",
                        reply.len()
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                    let _ = sock.shutdown().await;
                    return;
                }
            });
        }
    });
    (format!("http://127.0.0.1:{port}"), doc)
}

async fn connect(e: &Env, who: u64, base: &str, endpoint: &str) -> String {
    let mut body = create_body("Fake");
    body["endpoint"] = json!(endpoint);
    let (st, m, _) = e.call(Some(who), Method::POST, base, Some(body)).await;
    assert_eq!(st, StatusCode::CREATED, "{m}");
    m["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn the_connection_test_stores_its_result_and_flips_the_status() {
    let e = env().await;
    let base = "/users/@me/storage/mounts";
    let (good, _) = fake_s3(false).await;
    let id = connect(&e, MEMBER, base, &good).await;

    let (st, r, t) = e
        .call(
            Some(MEMBER),
            Method::POST,
            &format!("{base}/{id}/test"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert!(!t.contains(SECRET));
    let rows = r["rows"].as_array().unwrap();
    let ids: Vec<&str> = rows.iter().map(|x| x["id"].as_str().unwrap()).collect();
    assert_eq!(
        ids,
        [
            "head_bucket",
            "list",
            "put",
            "multipart",
            "cors",
            "list_buckets"
        ]
    );
    assert!(
        rows.iter()
            .all(|x| x["message"].as_str().is_some_and(|m| !m.is_empty()))
    );
    assert!(r["at"].as_str().unwrap().contains('T'));

    // The same object is the mount's `last_test`, and the mount stays active.
    let (_, list, _) = e.call(Some(MEMBER), Method::GET, base, None).await;
    let m = &list.as_array().unwrap()[0];
    assert_eq!(m["status"], "active");
    assert_eq!(m["last_test"]["rows"].as_array().unwrap().len(), 6);
    assert!(m["last_test_at"].is_string());

    // A failing key turns it `failing`; a clean run turns it back.
    let (bad, _) = fake_s3(true).await;
    let (st, _, _) = e
        .call(
            Some(MEMBER),
            Method::PATCH,
            &format!("{base}/{id}"),
            Some(json!({"endpoint": bad})),
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    let (_, r, _) = e
        .call(
            Some(MEMBER),
            Method::POST,
            &format!("{base}/{id}/test"),
            None,
        )
        .await;
    assert_eq!(r["rows"][2]["status"], "fail");
    let (_, list, _) = e.call(Some(MEMBER), Method::GET, base, None).await;
    assert_eq!(list[0]["status"], "failing");
    e.call(
        Some(MEMBER),
        Method::PATCH,
        &format!("{base}/{id}"),
        Some(json!({"endpoint": good})),
    )
    .await;
    e.call(
        Some(MEMBER),
        Method::POST,
        &format!("{base}/{id}/test"),
        None,
    )
    .await;
    let (_, list, _) = e.call(Some(MEMBER), Method::GET, base, None).await;
    assert_eq!(list[0]["status"], "active");

    // Not yours: 404. Unauthenticated: 401.
    let (st, _, _) = e
        .call(
            Some(OUTSIDER),
            Method::POST,
            &format!("{base}/{id}/test"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _, _) = e
        .call(None, Method::POST, &format!("{base}/{id}/test"), None)
        .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn cors_check_apply_and_the_presign_flag() {
    let e = env().await;
    let base = "/users/@me/storage/mounts";
    let (ep, doc) = fake_s3(false).await;
    let id = connect(&e, MEMBER, base, &ep).await;
    let check = format!("{base}/{id}/cors/check");
    let apply = format!("{base}/{id}/cors/apply");

    let (st, r, _) = e.call(Some(MEMBER), Method::POST, &check, None).await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert_eq!(r["state"], "missing");
    assert_eq!(r["found"].as_array().unwrap().len(), 0);
    assert!(
        r["required"][0]
            .as_str()
            .unwrap()
            .contains("chat.example.test")
    );
    assert!(r["checked_at"].is_string());
    let (_, list, _) = e.call(Some(MEMBER), Method::GET, base, None).await;
    assert!(list[0]["cors_ok_at"].is_null());

    let (st, r, _) = e.call(Some(MEMBER), Method::POST, &apply, None).await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert_eq!(r["state"], "ok");
    let (_, list, _) = e.call(Some(MEMBER), Method::GET, base, None).await;
    assert!(list[0]["cors_ok_at"].is_string(), "ok stamps cors_ok_at");

    // The rules are replaced behind our back: check says different and clears the stamp.
    *doc.lock().unwrap() =
        Some(sr_storage::s3admin::cors_xml(&["https://other.example.test".to_owned()]).unwrap());
    let (_, r, _) = e.call(Some(MEMBER), Method::POST, &check, None).await;
    assert_eq!(r["state"], "different");
    assert!(
        r["found"][0]
            .as_str()
            .unwrap()
            .contains("other.example.test")
    );
    let (_, list, _) = e.call(Some(MEMBER), Method::GET, base, None).await;
    assert!(list[0]["cors_ok_at"].is_null());

    // Changing the bucket clears the proof too.
    e.call(Some(MEMBER), Method::POST, &apply, None).await;
    let (_, list, _) = e.call(Some(MEMBER), Method::GET, base, None).await;
    assert!(list[0]["cors_ok_at"].is_string());
    e.call(
        Some(MEMBER),
        Method::PATCH,
        &format!("{base}/{id}"),
        Some(json!({"prefix": "other/"})),
    )
    .await;
    let (_, list, _) = e.call(Some(MEMBER), Method::GET, base, None).await;
    assert!(list[0]["cors_ok_at"].is_null());

    // Guild scope: a plain member is refused, the owner may.
    let gbase = format!("/guilds/{}/storage/mounts", e.guild);
    let gid = connect(&e, OWNER, &gbase, &ep).await;
    let (st, _, _) = e
        .call(
            Some(MEMBER),
            Method::POST,
            &format!("{gbase}/{gid}/cors/apply"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, r, _) = e
        .call(
            Some(OWNER),
            Method::POST,
            &format!("{gbase}/{gid}/cors/apply"),
            None,
        )
        .await;
    assert_eq!((st, r["state"].as_str()), (StatusCode::OK, Some("ok")));
    // A user mount through the guild route is 404.
    let (st, _, _) = e
        .call(
            Some(OWNER),
            Method::POST,
            &format!("{gbase}/{id}/test"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------- BYOS-FLG-001: operator policy

#[tokio::test]
async fn the_endpoint_allow_list_refuses_other_hosts() {
    let allow = |v: &str| Extra {
        settings: vec![(sr_admin::settings::BYOS_ENDPOINT_ALLOW, json!(v))],
        standing: None,
    };
    let base = "/users/@me/storage/mounts";
    // Not on the list: 400 ENDPOINT_INVALID, nothing stored.
    let e = env_with(allow("s3.example.com, *.r2.example.net")).await;
    let (st, body, _) = e
        .call(Some(MEMBER), Method::POST, base, Some(create_body("Home")))
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.to_string().contains("ENDPOINT_INVALID"), "{body}");
    let (_, list, _) = e.call(Some(MEMBER), Method::GET, base, None).await;
    assert!(list.as_array().unwrap().is_empty());
    // On the list: accepted.
    let e = env_with(allow("127.0.0.1")).await;
    let (st, body, _) = e
        .call(Some(MEMBER), Method::POST, base, Some(create_body("Home")))
        .await;
    assert_eq!(st, StatusCode::CREATED, "{body}");
    // A change of address is held to the same list.
    let id = body["id"].as_str().unwrap().to_owned();
    let (st, _, _) = e
        .call(
            Some(MEMBER),
            Method::PATCH,
            &format!("{base}/{id}"),
            Some(json!({"endpoint": "http://localhost:3900"})),
        )
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    // Guild buckets too.
    let e = env_with(allow("s3.example.com")).await;
    let (st, _, _) = e
        .call(
            Some(OWNER),
            Method::POST,
            &format!("/guilds/{}/storage/mounts", e.guild),
            Some(create_body("Shelf")),
        )
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn who_may_mount_limits_who_can_connect_a_bucket() {
    let with = |who: &str, standing: Option<(bool, bool)>| Extra {
        settings: vec![(sr_admin::settings::BYOS_WHO_MAY_MOUNT, json!(who))],
        standing,
    };
    let base = "/users/@me/storage/mounts";
    let try_create = |extra: Extra| async move {
        let e = env_with(extra).await;
        let (st, body, _) = e
            .call(Some(MEMBER), Method::POST, base, Some(create_body("Home")))
            .await;
        // Listing never depends on the setting.
        let (ls, _, _) = e.call(Some(MEMBER), Method::GET, base, None).await;
        assert_eq!(ls, StatusCode::OK);
        (st, body)
    };
    // all (and the empty default): anyone.
    assert_eq!(try_create(with("all", None)).await.0, StatusCode::CREATED);
    assert_eq!(try_create(with("", None)).await.0, StatusCode::CREATED);
    // patrons: a plain member is refused, a patron and staff are not.
    assert_eq!(
        try_create(with("patrons", Some((false, false)))).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        try_create(with("patrons", Some((true, false)))).await.0,
        StatusCode::CREATED
    );
    assert_eq!(
        try_create(with("patrons", Some((false, true)))).await.0,
        StatusCode::CREATED
    );
    // staff: a patron is refused too.
    assert_eq!(
        try_create(with("staff", Some((true, false)))).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        try_create(with("staff", Some((false, true)))).await.0,
        StatusCode::CREATED
    );
    // Without a standing source a restricted setting fails closed.
    assert_eq!(
        try_create(with("patrons", None)).await.0,
        StatusCode::FORBIDDEN
    );
}
