//! Endpoint tests over the in-memory backends: the credential policy matrix, the portal, the
//! OAuth2 flow, commands and interactions, driven through the real router.

use super::{DevDeps, PasswordConfirm, router};
use crate::{
    ApiSettings, AppState,
    auth::{csrf, memory::MemorySessions},
    build_router,
    routes::{
        Ext,
        admin::AdminDeps,
        messages::{
            FixedEntitlements, MessagesDeps,
            error::{ApiJson, ApiQuery, MsgError, MsgResult},
        },
    },
};
use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use futures::{StreamExt, stream::BoxStream};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sr_admin::{AccountState, AdminService, Caller, SYSTEM_USER_ID, memory::MemoryAdminRepo};
use sr_apps::{AppsService, auth::AppsAuth, memory::MemoryAppsRepo};
use sr_auth::{
    repo::{SessionRecord, SessionRepository},
    session::SessionToken,
};
use sr_chat::{
    Member,
    guild_rules::plan_new_guild,
    memory::MemoryBackend,
    repo::{ChatRepository, GuildRepository, UserSummary},
};
use sr_core::{
    ApiError, ChannelId, Clock, ErrorCode, GuildId, ManualClock, PlatformPermissions as P,
    SnowflakeGenerator, UserId,
};
use sr_events::{DomainEvent, EventBus, InProcessBus, Topic};
use sr_storage::ObjectStoreStorage;
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};
use tower::ServiceExt;

pub(super) const T0: i64 = 1_800_000_000_000;
pub(super) const OWNER: i64 = 1;
pub(super) const BOB: i64 = 2;
pub(super) const CAROL: i64 = 3; // not in the guild
pub(super) const STAFF: i64 = 4;
const REVIEWER_ROLE: i64 = 77;
pub(super) const CB: &str = "https://client.test/cb";

/// `?wait` of the stand-in webhook route.
#[derive(serde::Deserialize, Default)]
struct WaitQuery {
    wait: Option<bool>,
}

async fn followup_route(
    Ext(dev): Ext<DevDeps>,
    Ext(messages): Ext<MessagesDeps>,
    axum::extract::Path((id, token)): axum::extract::Path<(String, String)>,
    ApiQuery(q): ApiQuery<WaitQuery>,
    ApiJson(body): ApiJson<crate::routes::settings::webhooks::ExecuteWebhookBody>,
) -> MsgResult<axum::response::Response> {
    match super::try_followup(&dev, &messages, &id, &token, q.wait == Some(true), &body).await? {
        Some(r) => Ok(r),
        None => Err(MsgError::Api(ApiError::new(ErrorCode::UnknownWebhook))),
    }
}

struct Pw;

#[async_trait]
impl PasswordConfirm for Pw {
    async fn confirm(&self, _now: i64, _user: i64, password: &str) -> MsgResult<()> {
        if password == "correct" {
            Ok(())
        } else {
            Err(MsgError::Api(ApiError::new(ErrorCode::SrReauthRequired)))
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Who<'a> {
    Anon,
    Session(i64),
    Bot(&'a str),
    Bearer(&'a str),
}

pub(super) struct Env {
    pub(super) app: Router,
    pub(super) apps: Arc<MemoryAppsRepo>,
    pub(super) svc: AppsService,
    pub(super) backend: MemoryBackend,
    pub(super) clock: Arc<ManualClock>,
    pub(super) bus: Arc<InProcessBus>,
    pub(super) tokens: HashMap<i64, String>,
    pub(super) guild: GuildId,
    pub(super) ch: ChannelId,
}

pub(super) fn summary(id: i64, name: &str, bot: bool) -> UserSummary {
    UserSummary {
        id: UserId::new(id as u64),
        username: name.to_owned(),
        display_name: None,
        avatar_key: None,
        banner_key: None,
        bot,
        primary_guild: None,
    }
}

pub(super) async fn env_with(enforce: bool) -> Env {
    env_full(enforce, None).await
}

pub(super) async fn env_full(
    enforce: bool,
    endpoint: Option<Arc<dyn super::EndpointClient>>,
) -> Env {
    let clock = Arc::new(ManualClock::new(T0));
    let dyn_clock: Arc<dyn Clock> = clock.clone();
    let backend = MemoryBackend::new();
    let bus = Arc::new(InProcessBus::new());
    let storage = Arc::new(ObjectStoreStorage::in_memory());

    let gen_ids = SnowflakeGenerator::new(dyn_clock.clone(), 9, 9).unwrap();
    let bp = plan_new_guild(UserId::new(OWNER as u64), "Club", T0, || gen_ids.generate()).unwrap();
    let (guild, ch) = (bp.guild.id, bp.channels[2].id);
    backend.insert_guild(&bp).await.unwrap();
    backend.data().members.push(Member {
        guild_id: guild,
        user_id: UserId::new(BOB as u64),
        nick: None,
        role_ids: vec![],
        joined_at_ms: T0,
        communication_disabled_until_ms: None,
        pending: false,
        avatar_key: None,
        banner_key: None,
        bio: None,
    });
    for (id, name) in [
        (OWNER, "owner"),
        (BOB, "bob"),
        (CAROL, "carol"),
        (STAFF, "staff"),
    ] {
        backend
            .data()
            .users
            .insert(UserId::new(id as u64), summary(id, name, false));
    }

    // Platform side: accounts, the guild, and a reviewer.
    let apps = Arc::new(MemoryAppsRepo::new());
    for (id, name) in [
        (OWNER, "owner"),
        (BOB, "bob"),
        (CAROL, "carol"),
        (STAFF, "staff"),
    ] {
        apps.add_account(id, name);
    }
    apps.add_guild(guild.get() as i64, OWNER, &[BOB]);
    let counter = Arc::new(AtomicI64::new(10_000));
    let svc = AppsService::new(
        apps.clone(),
        dyn_clock.clone(),
        Arc::new(move || counter.fetch_add(1, Ordering::SeqCst)),
    );

    let admin_repo = Arc::new(MemoryAdminRepo::default());
    admin_repo.add_account(
        SYSTEM_USER_ID,
        "sideroom",
        AccountState::Deactivated,
        None,
        None,
    );
    admin_repo.add_account(OWNER, "owner", AccountState::Active, None, None);
    admin_repo.add_account(STAFF, "staff", AccountState::Active, None, Some(OWNER));
    admin_repo.add_role(
        REVIEWER_ROLE,
        "App Reviewer",
        1,
        P::STAFF_BASIC | P::REVIEW_APPS,
    );
    admin_repo.set_owner(OWNER);
    let admin = AdminService::new(admin_repo);
    admin
        .appoint(
            &Caller::operator(SYSTEM_USER_ID),
            STAFF,
            &[REVIEWER_ROLE],
            Some("test"),
        )
        .await
        .unwrap();

    let sessions = Arc::new(MemorySessions::new());
    let mut tokens = HashMap::new();
    for (n, uid) in [OWNER, BOB, CAROL, STAFF].into_iter().enumerate() {
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

    let chat: Arc<dyn ChatRepository> = Arc::new(backend.clone());
    let mut dev = DevDeps::new(
        svc.clone(),
        chat,
        bus.clone(),
        Arc::new(Pw),
        dyn_clock.clone(),
        "https://side.test",
    )
    .with_ack_window(Duration::from_millis(150));
    if let Some(c) = endpoint {
        dev = dev.with_endpoint_client(c);
    }
    let messages = MessagesDeps::new(
        backend.clone(),
        Arc::new(backend.clone()),
        storage,
        bus.clone(),
        Arc::new(FixedEntitlements(30 * 1024 * 1024)),
        Arc::new(backend.clone()),
        None,
        dyn_clock.clone(),
        1,
        1,
    )
    .unwrap();

    let auth = Arc::new(AppsAuth::new(Arc::new(svc.clone())));
    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    settings.enforce_credential_policy = enforce;
    let state = AppState::builder()
        .sessions(sessions)
        .bots(auth.clone())
        .bearers(auth)
        .clock(dyn_clock)
        .settings(settings)
        .extension(Arc::new(dev))
        .extension(Arc::new(messages))
        .extension(Arc::new(AdminDeps::new(admin)))
        .build();
    // Stands in for the settings module's `POST /webhooks/{id}/{token}`, which asks the
    // developer module first.
    let followups = Router::new().route(
        "/webhooks/{id}/{token}",
        axum::routing::post(followup_route),
    );
    let app = build_router(
        state,
        router()
            .merge(crate::routes::messages::router())
            .merge(followups),
    );
    Env {
        app,
        apps,
        svc,
        backend,
        clock,
        bus,
        tokens,
        guild,
        ch,
    }
}

pub(super) async fn env() -> Env {
    env_with(true).await
}

pub(super) fn json_of(bytes: &[u8]) -> Value {
    if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(bytes).unwrap_or(Value::Null)
    }
}

impl Env {
    pub(super) async fn raw(
        &self,
        who: Who<'_>,
        method: Method,
        path: &str,
        content_type: Option<&str>,
        body: Vec<u8>,
        extra: &[(&str, String)],
    ) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
        let mut req = Request::builder()
            .method(method.clone())
            .uri(format!("/api/v1{path}"))
            .header("origin", "http://localhost:8080");
        match who {
            Who::Anon => {}
            Who::Session(uid) => {
                let raw = &self.tokens[&uid];
                req = req.header("cookie", format!("sr_session={raw}"));
                if method != Method::GET {
                    req = req.header("x-sr-csrf", csrf::token_for(raw));
                }
            }
            Who::Bot(t) => req = req.header("authorization", format!("Bot {t}")),
            Who::Bearer(t) => req = req.header("authorization", format!("Bearer {t}")),
        }
        if let Some(ct) = content_type {
            req = req.header("content-type", ct);
        }
        for (k, v) in extra {
            req = req.header(*k, v);
        }
        let resp = self
            .app
            .clone()
            .oneshot(req.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        let (parts, body) = resp.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes().to_vec();
        (parts.status, parts.headers, bytes)
    }

    pub(super) async fn call(
        &self,
        who: Who<'_>,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let (ct, bytes) = match body {
            Some(b) => (Some("application/json"), b.to_string().into_bytes()),
            None => (None, Vec::new()),
        };
        let (s, _, b) = self.raw(who, method, path, ct, bytes, &[]).await;
        (s, json_of(&b))
    }

    pub(super) async fn get(&self, who: Who<'_>, path: &str) -> (StatusCode, Value) {
        self.call(who, Method::GET, path, None).await
    }
    pub(super) async fn post(&self, who: Who<'_>, path: &str, body: Value) -> (StatusCode, Value) {
        self.call(who, Method::POST, path, Some(body)).await
    }
    pub(super) async fn patch(&self, who: Who<'_>, path: &str, body: Value) -> (StatusCode, Value) {
        self.call(who, Method::PATCH, path, Some(body)).await
    }
    pub(super) async fn put(&self, who: Who<'_>, path: &str, body: Value) -> (StatusCode, Value) {
        self.call(who, Method::PUT, path, Some(body)).await
    }
    pub(super) async fn delete(
        &self,
        who: Who<'_>,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        self.call(who, Method::DELETE, path, body).await
    }

    /// Create an application as OWNER; returns (app id, bot id, bot token, client secret).
    pub(super) async fn new_app(&self) -> (String, String, String, String) {
        let (s, v) = self
            .post(
                Who::Session(OWNER),
                "/applications",
                json!({"name": "Helper", "description": "x"}),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{v}");
        let id = v["application"]["id"].as_str().unwrap().to_owned();
        let bot = v["application"]["bot"]["id"].as_str().unwrap().to_owned();
        // The bot account is a person-like user for the chat side.
        self.backend.data().users.insert(
            UserId::new(bot.parse().unwrap()),
            summary(bot.parse().unwrap(), "bot", true),
        );
        (
            id,
            bot,
            v["bot_token"].as_str().unwrap().to_owned(),
            v["client_secret"].as_str().unwrap().to_owned(),
        )
    }

    /// Register a redirect URI, make the bot public.
    pub(super) async fn configure(&self, id: &str) {
        let (s, v) = self
            .patch(
                Who::Session(OWNER),
                &format!("/applications/{id}"),
                json!({"redirect_uris": [CB], "bot_public": true}),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{v}");
    }

    /// Install the bot in the guild as OWNER through the authorize flow, and mirror the
    /// membership into the chat backend (the in-memory platform repository does not).
    pub(super) async fn install(&self, id: &str, bot: &str, permissions: u64) {
        let (s, v) = self
            .post(
                Who::Session(OWNER),
                "/oauth2/authorize",
                json!({
                    "client_id": id, "scope": "bot applications.commands",
                    "guild_id": self.guild.to_string(), "permissions": permissions.to_string(),
                    "authorize": true,
                }),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{v}");
        self.backend.data().members.push(Member {
            guild_id: self.guild,
            user_id: UserId::new(bot.parse().unwrap()),
            nick: None,
            role_ids: vec![],
            joined_at_ms: T0,
            communication_disabled_until_ms: None,
            pending: false,
            avatar_key: None,
            banner_key: None,
            bio: None,
        });
    }

    pub(super) fn user_events(&self, user: i64) -> BoxStream<'static, DomainEvent> {
        self.bus.subscribe(Topic::User(UserId::new(user as u64)))
    }
}

pub(super) async fn next_named(s: &mut BoxStream<'static, DomainEvent>, name: &str) -> Value {
    loop {
        let ev = tokio::time::timeout(Duration::from_secs(2), s.next())
            .await
            .unwrap_or_else(|_| panic!("timed out waiting for {name}"))
            .expect("stream ended");
        if let DomainEvent::UserScoped {
            name: n, payload, ..
        } = ev
            && n == name
        {
            return payload;
        }
    }
}

pub(super) async fn no_event(s: &mut BoxStream<'static, DomainEvent>, name: &str) {
    while let Ok(Some(ev)) = tokio::time::timeout(Duration::from_millis(250), s.next()).await {
        if let DomainEvent::UserScoped { name: n, .. } = ev {
            assert_ne!(n, name, "unexpected {name}");
        }
    }
}

pub(super) fn code_of(v: &Value) -> i64 {
    v["code"].as_i64().unwrap_or(-1)
}

// ======================================================================= credential policy

#[tokio::test]
async fn bots_are_kept_out_of_portal_staff_and_consent_routes() {
    let e = env().await;
    let (id, bot, token, _) = e.new_app().await;
    e.configure(&id).await;
    e.install(&id, &bot, 0).await;
    let denied: Vec<(Method, String, Option<Value>)> = vec![
        (
            Method::POST,
            "/developers/apply".into(),
            Some(json!({"pitch": "x", "accepted_terms": true})),
        ),
        (Method::GET, "/developers/status".into(), None),
        (
            Method::POST,
            "/applications".into(),
            Some(json!({"name": "Evil"})),
        ),
        (Method::GET, "/applications".into(), None),
        (Method::GET, format!("/applications/{id}"), None),
        (
            Method::PATCH,
            format!("/applications/{id}"),
            Some(json!({"name": "Hijack"})),
        ),
        (
            Method::POST,
            format!("/applications/{id}/reset-secret"),
            Some(json!({"password": "correct"})),
        ),
        (
            Method::POST,
            format!("/applications/{id}/bot/reset-token"),
            Some(json!({"password": "correct"})),
        ),
        (
            Method::DELETE,
            format!("/applications/{id}"),
            Some(json!({"password": "correct"})),
        ),
        (Method::GET, format!("/applications/{id}/audit-log"), None),
        (
            Method::POST,
            format!("/applications/{id}/versions"),
            Some(json!({})),
        ),
        (Method::GET, "/admin/developers/requests".into(), None),
        (
            Method::POST,
            format!("/admin/developers/apps/{id}/suspend"),
            Some(json!({"reason": "x"})),
        ),
        (
            Method::GET,
            "/oauth2/authorize?client_id=1&scope=identify".into(),
            None,
        ),
        (
            Method::POST,
            "/oauth2/authorize".into(),
            Some(json!({"client_id": id, "scope": "bot", "authorize": true})),
        ),
        (Method::GET, "/oauth2/tokens".into(), None),
        (
            Method::POST,
            "/interactions".into(),
            Some(json!({"type": 2, "application_id": id, "channel_id": "1", "data": {}})),
        ),
    ];
    for (m, path, body) in denied {
        let (s, v) = e.call(Who::Bot(&token), m.clone(), &path, body).await;
        assert_eq!(
            (s, code_of(&v)),
            (StatusCode::FORBIDDEN, 20001),
            "{m} {path}"
        );
    }
    // ...while its own routes work
    assert_eq!(
        e.get(Who::Bot(&token), "/gateway/bot").await.0,
        StatusCode::OK
    );
    assert_eq!(
        e.get(Who::Bot(&token), "/applications/@me").await.0,
        StatusCode::OK
    );
    // and nothing it did changed the application
    let (_, v) = e
        .get(Who::Session(OWNER), &format!("/applications/{id}"))
        .await;
    assert_eq!(v["name"], "Helper");
}

#[tokio::test]
async fn bot_routes_refuse_sessions_and_strangers() {
    let e = env().await;
    let (id, _, token, _) = e.new_app().await;
    for who in [
        Who::Session(OWNER),
        Who::Anon,
        Who::Bearer("x"),
        Who::Bot("junk"),
    ] {
        let (s, _) = e.get(who, "/gateway/bot").await;
        assert!(
            matches!(s, StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED),
            "{s}"
        );
        let (s, _) = e.get(who, &format!("/applications/{id}/commands")).await;
        assert!(
            matches!(s, StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED),
            "{s}"
        );
    }
    assert_eq!(
        e.get(Who::Bot(&token), "/gateway/bot").await.0,
        StatusCode::OK
    );
    let (s, v) = e.get(Who::Session(OWNER), "/gateway/bot").await;
    assert_eq!((s, code_of(&v)), (StatusCode::FORBIDDEN, 20002));
    // a bot can never touch another application's commands
    let other = e.svc.create_application(OWNER, "Other", "").await.unwrap();
    let (s, v) = e
        .get(
            Who::Bot(&token),
            &format!("/applications/{}/commands", other.application.id),
        )
        .await;
    assert_eq!((s, code_of(&v)), (StatusCode::FORBIDDEN, 50001));
    let (s, _) = e
        .post(
            Who::Bot(&token),
            &format!("/applications/{}/commands", other.application.id),
            json!({"name": "x", "description": "x"}),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn gateway_discovery() {
    let e = env().await;
    let (s, v) = e.get(Who::Anon, "/gateway").await;
    assert_eq!(
        (s, v["url"].as_str()),
        (StatusCode::OK, Some("wss://side.test/gateway"))
    );
    let (_, token_app) = (0, e.new_app().await);
    let (s, v) = e.get(Who::Bot(&token_app.2), "/gateway/bot").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["shards"], 1);
    assert_eq!(v["session_start_limit"]["max_concurrency"], 1);
}

// ======================================================================= portal

#[tokio::test]
async fn developer_onboarding_and_staff_review() {
    let e = env().await;
    // status before applying
    let (s, v) = e.get(Who::Session(BOB), "/developers/status").await;
    assert_eq!(
        (s, v["is_developer"].clone()),
        (StatusCode::OK, json!(false))
    );
    // validation
    let (s, v) = e
        .post(
            Who::Session(BOB),
            "/developers/apply",
            json!({"pitch": "", "accepted_terms": false}),
        )
        .await;
    assert_eq!((s, code_of(&v)), (StatusCode::BAD_REQUEST, 50035));
    assert!(v["errors"]["pitch"].is_object() && v["errors"]["accepted_terms"].is_object());
    let (s, v) = e
        .post(
            Who::Session(BOB),
            "/developers/apply",
            json!({"pitch": "A music bot", "accepted_terms": true}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let rid = v["id"].as_str().unwrap().to_owned();
    assert_eq!(v["state"], "pending");
    let (s, _) = e
        .post(
            Who::Session(BOB),
            "/developers/apply",
            json!({"pitch": "again", "accepted_terms": true}),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    // people who are not staff cannot see or decide
    for who in [Who::Session(BOB), Who::Anon] {
        let (s, _) = e.get(who, "/admin/developers/requests").await;
        assert!(matches!(
            s,
            StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED
        ));
        let (s, _) = e
            .post(
                who,
                &format!("/admin/developers/requests/{rid}/decision"),
                json!({"approve": true}),
            )
            .await;
        assert!(matches!(
            s,
            StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED
        ));
    }
    // the reviewer sees the pitch and decides
    let (s, v) = e
        .get(Who::Session(STAFF), "/admin/developers/requests")
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v[0]["account"]["username"], "bob");
    let (s, _) = e
        .post(
            Who::Session(STAFF),
            &format!("/admin/developers/requests/{rid}/decision"),
            json!({"approve": false}),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "a rejection needs a note");
    let (s, v) = e
        .post(
            Who::Session(STAFF),
            &format!("/admin/developers/requests/{rid}/decision"),
            json!({"approve": true}),
        )
        .await;
    assert_eq!((s, v["state"].clone()), (StatusCode::OK, json!("approved")));
    let (_, v) = e.get(Who::Session(BOB), "/developers/status").await;
    assert_eq!(v["is_developer"], true);
    let (s, _) = e
        .post(
            Who::Session(STAFF),
            &format!("/admin/developers/requests/{rid}/decision"),
            json!({"approve": true}),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
}

#[tokio::test]
async fn application_lifecycle_secrets_and_ownership() {
    let e = env().await;
    let (s, v) = e
        .post(Who::Session(OWNER), "/applications", json!({"name": "x"}))
        .await;
    assert_eq!((s, code_of(&v)), (StatusCode::BAD_REQUEST, 50035));
    let (id, _bot, token, secret) = e.new_app().await;
    assert!(token.split('.').count() == 3 && secret.len() == 43);
    // the listing and the object never repeat the secrets
    let (_, v) = e.get(Who::Session(OWNER), "/applications").await;
    let dump = v.to_string();
    assert!(!dump.contains(&secret) && !dump.contains(&token));
    assert_eq!(v[0]["release_state"], "development");
    assert!(
        v[0]["client_secret_hint"]
            .as_str()
            .unwrap()
            .starts_with("...")
    );
    // strangers get the same 404 as a missing id
    for who in [Who::Session(BOB), Who::Session(CAROL)] {
        for (m, p, b) in [
            (Method::GET, format!("/applications/{id}"), None),
            (
                Method::PATCH,
                format!("/applications/{id}"),
                Some(json!({"name": "Stolen"})),
            ),
            (Method::GET, format!("/applications/{id}/audit-log"), None),
            (Method::GET, format!("/applications/{id}/installs"), None),
            (
                Method::POST,
                format!("/applications/{id}/reset-secret"),
                Some(json!({"password": "correct"})),
            ),
            (
                Method::POST,
                format!("/applications/{id}/bot/reset-token"),
                Some(json!({"password": "correct"})),
            ),
            (
                Method::DELETE,
                format!("/applications/{id}"),
                Some(json!({"password": "correct"})),
            ),
            (
                Method::POST,
                format!("/applications/{id}/versions"),
                Some(json!({})),
            ),
        ] {
            let (s, v) = e.call(who, m.clone(), &p, b).await;
            assert_eq!((s, code_of(&v)), (StatusCode::NOT_FOUND, 10002), "{m} {p}");
        }
    }
    let (s, v) = e
        .get(Who::Session(OWNER), "/applications/not-a-number")
        .await;
    assert_eq!((s, code_of(&v)), (StatusCode::NOT_FOUND, 10002));
    // patching validates
    let (s, v) = e
        .patch(
            Who::Session(OWNER),
            &format!("/applications/{id}"),
            json!({"redirect_uris": ["http://evil.test/cb"]}),
        )
        .await;
    assert_eq!((s, code_of(&v)), (StatusCode::BAD_REQUEST, 50035));
    assert!(v["errors"]["redirect_uris"].is_object());
    let (s, v) = e
        .patch(
            Who::Session(OWNER),
            &format!("/applications/{id}"),
            json!({"description": "new", "privileged_intents": ["MESSAGE_CONTENT"], "tags": ["music"]}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(
        v["privileged_intents"]["requested"],
        json!(["MESSAGE_CONTENT"])
    );
    assert_eq!(v["privileged_intents"]["approved"], json!([]));
    assert_eq!(
        v["flags"],
        1u64 << 19,
        "limited message content while in development"
    );
    // renaming the bot
    let (s, _) = e
        .patch(
            Who::Session(OWNER),
            &format!("/applications/{id}/bot"),
            json!({"username": "Helper Bot"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = e
        .patch(
            Who::Session(OWNER),
            &format!("/applications/{id}/bot"),
            json!({"username": "x"}),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // resets need the password and revoke the old credential
    let (s, v) = e
        .post(
            Who::Session(OWNER),
            &format!("/applications/{id}/bot/reset-token"),
            json!({"password": "wrong"}),
        )
        .await;
    assert_eq!((s, code_of(&v)), (StatusCode::FORBIDDEN, 900004));
    assert_eq!(
        e.get(Who::Bot(&token), "/gateway/bot").await.0,
        StatusCode::OK,
        "wrong password changed nothing"
    );
    let (s, v) = e
        .post(
            Who::Session(OWNER),
            &format!("/applications/{id}/bot/reset-token"),
            json!({"password": "correct"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let new_token = v["token"].as_str().unwrap().to_owned();
    assert_eq!(
        e.get(Who::Bot(&token), "/gateway/bot").await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        e.get(Who::Bot(&new_token), "/gateway/bot").await.0,
        StatusCode::OK
    );
    let (s, v) = e
        .post(
            Who::Session(OWNER),
            &format!("/applications/{id}/reset-secret"),
            json!({"password": "correct"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_ne!(v["client_secret"], json!(secret));
    // the audit trail records them without secrets
    let (_, v) = e
        .get(
            Who::Session(OWNER),
            &format!("/applications/{id}/audit-log"),
        )
        .await;
    let actions: Vec<&str> = v
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["action"].as_str().unwrap())
        .collect();
    assert_eq!(&actions[..2], &["app.secret_reset", "app.token_reset"]);
    assert!(!v.to_string().contains(&new_token));

    // deleting needs the password too
    let (s, _) = e
        .delete(
            Who::Session(OWNER),
            &format!("/applications/{id}"),
            Some(json!({"password": "wrong"})),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = e
        .delete(
            Who::Session(OWNER),
            &format!("/applications/{id}"),
            Some(json!({"password": "correct"})),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(
        e.get(Who::Bot(&new_token), "/gateway/bot").await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        e.get(Who::Session(OWNER), &format!("/applications/{id}"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn release_review_flow() {
    let e = env().await;
    let (id, _, token, _) = e.new_app().await;
    // a privacy policy is required
    let (s, v) = e
        .post(
            Who::Session(OWNER),
            &format!("/applications/{id}/versions"),
            json!({}),
        )
        .await;
    assert_eq!((s, code_of(&v)), (StatusCode::BAD_REQUEST, 50035));
    e.patch(
        Who::Session(OWNER),
        &format!("/applications/{id}"),
        json!({"privacy_policy_url": "https://a.test/privacy", "privileged_intents": ["GUILD_MEMBERS"]}),
    )
    .await;
    let (s, v) = e
        .post(
            Who::Session(OWNER),
            &format!("/applications/{id}/versions"),
            json!({"note": "ready"}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let vid = v["id"].as_str().unwrap().to_owned();
    assert_eq!(v["needs_review_reasons"], json!(["first_release"]));
    assert!(v["diff"].as_array().unwrap().len() > 3);
    let (_, v) = e
        .get(Who::Session(OWNER), &format!("/applications/{id}"))
        .await;
    assert_eq!(v["release_state"], "in_review");
    // only a reviewer decides, and only on what was requested
    let (s, _) = e
        .post(
            Who::Session(OWNER),
            &format!("/admin/developers/versions/{vid}/decision"),
            json!({"decision": "approve"}),
        )
        .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "owners cannot approve their own app"
    );
    let (s, v) = e
        .get(Who::Session(STAFF), "/admin/developers/versions")
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v[0]["application"]["id"], json!(id));
    let (s, _) = e
        .post(
            Who::Session(STAFF),
            &format!("/admin/developers/versions/{vid}/decision"),
            json!({"decision": "approve", "approved_intents": ["MESSAGE_CONTENT"]}),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = e
        .post(
            Who::Session(STAFF),
            &format!("/admin/developers/versions/{vid}/decision"),
            json!({"decision": "maybe"}),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, v) = e
        .post(
            Who::Session(STAFF),
            &format!("/admin/developers/versions/{vid}/decision"),
            json!({"decision": "approve", "approved_intents": ["GUILD_MEMBERS"]}),
        )
        .await;
    assert_eq!((s, v["state"].clone()), (StatusCode::OK, json!("approved")));
    let (_, v) = e
        .get(Who::Session(OWNER), &format!("/applications/{id}"))
        .await;
    assert_eq!(v["release_state"], "released");
    assert_eq!(
        v["privileged_intents"]["approved"],
        json!(["GUILD_MEMBERS"])
    );
    assert_eq!(v["flags"], 1u64 << 14);
    // suspension switches the bot off
    let (s, _) = e
        .post(
            Who::Session(STAFF),
            &format!("/admin/developers/apps/{id}/suspend"),
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "a reason is required");
    let (s, v) = e
        .post(
            Who::Session(STAFF),
            &format!("/admin/developers/apps/{id}/suspend"),
            json!({"reason": "abuse"}),
        )
        .await;
    assert_eq!(
        (s, v["release_state"].clone()),
        (StatusCode::OK, json!("suspended"))
    );
    assert_eq!(
        e.get(Who::Bot(&token), "/gateway/bot").await.0,
        StatusCode::UNAUTHORIZED
    );
    let (s, v) = e
        .post(
            Who::Session(STAFF),
            &format!("/admin/developers/apps/{id}/unsuspend"),
            json!({}),
        )
        .await;
    assert_eq!(
        (s, v["release_state"].clone()),
        (StatusCode::OK, json!("released"))
    );
    assert_eq!(
        e.get(Who::Bot(&token), "/gateway/bot").await.0,
        StatusCode::OK
    );
    // versions list for the owner
    let (_, v) = e
        .get(Who::Session(OWNER), &format!("/applications/{id}/versions"))
        .await;
    assert_eq!(v[0]["state"], "approved");
}
