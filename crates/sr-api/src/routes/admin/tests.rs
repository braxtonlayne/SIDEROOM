//! Endpoint tests over the in-memory admin repository: 401/403 matrices for every
//! route plus the main flows, driven through the real router with tower `oneshot`.

use super::{AdminDeps, IntegrationStatus, router};
use crate::routes::auth::{
    AccountRecord, AccountState as AuthState, AuthConfig, AuthService, AuthStore,
    mail::MemoryMailer, memory::MemoryAuthStore,
};
use crate::{
    ApiSettings, AppState,
    auth::{
        BotIdentity, csrf,
        memory::{MemoryBots, MemorySessions},
    },
    build_router,
};
use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sr_admin::{
    AccountState, AdminService, Announcement, Caller, SYSTEM_USER_ID, StaffNotifier, StaffOffer,
    memory::MemoryAdminRepo,
};
use sr_auth::{
    password::{PasswordHasher, PasswordParams},
    totp::{self, TotpSecret},
};
use sr_auth::{
    repo::{SessionRecord, SessionRepository},
    session::SessionToken,
};
use sr_core::PlatformPermissions as P;
use sr_core::{Clock, ManualClock};
use sr_features::{
    Feature, FeatureMeta, FeatureRegistry, FeatureService, Field, FieldKind, InMemorySettingsStore,
    Scope, SettingsSchema,
};
use std::sync::Mutex;
use std::{collections::HashMap, sync::Arc};
use tower::ServiceExt;

const OWNER: i64 = 10;
const MODER: i64 = 11;
const BASIC: i64 = 12;
const ALICE: i64 = 20;
const BOB: i64 = 21;
const OUTSIDER: i64 = 30;
const R_OWNER: i64 = 1;
const R_MOD: i64 = 2;
const R_BASIC: i64 = 3;
/// The moderator's authenticator secret (20 bytes).
const TOTP_SECRET: [u8; 20] = *b"0123456789abcdefghij";

struct Demo;

impl Feature for Demo {
    fn id(&self) -> &'static str {
        "demo.thing"
    }
    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: "Demo".into(),
            description: "d".into(),
            section: "Lounge".into(),
            phase: 1,
            default_enabled: true,
        }
    }
    fn settings_schema(&self) -> SettingsSchema {
        SettingsSchema::new(vec![
            Field::new(
                "limit",
                "Limit",
                FieldKind::Int {
                    min: Some(1),
                    max: Some(100),
                },
            )
            .default_value(json!(10)),
        ])
    }
    fn scopes(&self) -> &[Scope] {
        &[Scope::Platform, Scope::Guild]
    }
}

/// Records what the system user would have said.
#[derive(Default)]
struct Notes {
    offers: Mutex<Vec<StaffOffer>>,
    notices: Mutex<Vec<String>>,
    broadcasts: Mutex<Vec<Announcement>>,
}

#[async_trait]
impl StaffNotifier for Notes {
    async fn appointment_offer(&self, offer: &StaffOffer) -> Option<(i64, i64)> {
        self.offers.lock().unwrap().push(offer.clone());
        Some((7_000 + offer.id, 8_000 + offer.id))
    }
    async fn staff_notice(&self, text: &str) {
        self.notices.lock().unwrap().push(text.to_owned());
    }
    fn broadcast_announcement(&self, a: &Announcement) {
        self.broadcasts.lock().unwrap().push(a.clone());
    }
}

const T0: i64 = 1_800_000_000_000;

struct Env {
    app: Router,
    repo: Arc<MemoryAdminRepo>,
    tokens: HashMap<i64, String>,
    bot: &'static str,
    notes: Arc<Notes>,
    clock: Arc<ManualClock>,
    auth: Arc<MemoryAuthStore>,
    mailer: Arc<MemoryMailer>,
}

async fn env() -> Env {
    env_with(false).await
}

/// `reauth`: demand the 15-minute second-factor window, with a real `AuthService` behind it.
async fn env_with(reauth: bool) -> Env {
    let clock = Arc::new(ManualClock::new(T0));
    let repo = Arc::new(MemoryAdminRepo::new(clock.clone()));
    repo.add_account(
        SYSTEM_USER_ID,
        "sideroom",
        AccountState::Deactivated,
        None,
        None,
    );
    repo.add_account(OWNER, "owner", AccountState::Active, None, None);
    for (id, name) in [
        (MODER, "mod"),
        (BASIC, "basic"),
        (ALICE, "alice"),
        (BOB, "bob"),
        (OUTSIDER, "outsider"),
    ] {
        repo.add_account(id, name, AccountState::Active, None, Some(OWNER));
    }
    repo.add_role(R_OWNER, "Owner", 3, P::OWNER);
    repo.add_role(
        R_MOD,
        "Moderator",
        2,
        P::STAFF_BASIC | P::MANAGE_ACCOUNTS | P::PLATFORM_BAN,
    );
    repo.add_role(R_BASIC, "Basic", 1, P::STAFF_BASIC);
    repo.set_owner(OWNER);
    let notes = Arc::new(Notes::default());
    let svc = AdminService::new(repo.clone())
        .with_extras(repo.clone())
        .with_notifier(notes.clone())
        .with_clock(clock.clone());
    let op = Caller::operator(SYSTEM_USER_ID);
    svc.appoint(&op, MODER, &[R_MOD], Some("test"))
        .await
        .unwrap();
    svc.appoint(&op, BASIC, &[R_BASIC], Some("test"))
        .await
        .unwrap();

    let sessions = Arc::new(MemorySessions::new());
    let mut tokens = HashMap::new();
    for (n, uid) in [OWNER, MODER, BASIC, ALICE, BOB, OUTSIDER]
        .into_iter()
        .enumerate()
    {
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
    let bots = Arc::new(MemoryBots::new());
    bots.insert(
        "bot-owner",
        BotIdentity {
            user_id: OWNER,
            application_id: 1,
        },
    );

    let mut registry = FeatureRegistry::new();
    registry.register(Demo).unwrap();
    let features = FeatureService::new(
        Arc::new(registry),
        Arc::new(InMemorySettingsStore::new()),
        None,
    );
    let deps = AdminDeps::new(svc)
        .with_integrations(vec![IntegrationStatus {
            id: "smtp".into(),
            configured: true,
            detail: Some("mail.example.test:587".into()),
        }])
        .with_features(features)
        .with_gateway_sessions(Arc::new(|| 7))
        .with_reauth(reauth);
    // The account service behind the second-factor window: every test user exists there, and
    // the moderator has a confirmed authenticator.
    let auth = Arc::new(MemoryAuthStore::new(sessions.clone()));
    let hasher = PasswordHasher::new(PasswordParams {
        m_cost_kib: 64,
        t_cost: 1,
        p_cost: 1,
    })
    .unwrap();
    for uid in [OWNER, MODER, BASIC, ALICE, BOB, OUTSIDER] {
        auth.add_account(AccountRecord {
            id: uid,
            person_id: uid,
            username: format!("u{uid}"),
            display_name: None,
            email: format!("u{uid}@x.test"),
            email_verified: true,
            password_hash: hasher.hash("pw").unwrap(),
            state: AuthState::Active,
            age_confirmed: None,
            age_confirmed_at: None,
            locale: "en-US".into(),
            flags: 0,
        });
    }
    // D-198: accepting an appointment needs a second factor, so the offer tests' invitees have one;
    // OUTSIDER has none.
    for uid in [MODER, ALICE, BOB] {
        auth.totp_put_pending(uid, &TOTP_SECRET).await.unwrap();
        auth.totp_confirm(uid, 1).await.unwrap();
    }
    let next = Arc::new(std::sync::atomic::AtomicI64::new(50_000));
    let mailer = Arc::new(MemoryMailer::new());
    let auth_svc = Arc::new(
        AuthService::new(
            auth.clone(),
            mailer.clone(),
            Arc::new(move || next.fetch_add(1, std::sync::atomic::Ordering::SeqCst)),
        )
        .with_hasher(hasher)
        .with_config(AuthConfig::default()),
    );

    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let state = AppState::builder()
        .settings(settings)
        .sessions(sessions)
        .bots(bots)
        .clock(clock.clone())
        .extension(Arc::new(deps))
        .extension(auth_svc)
        .build();
    Env {
        app: build_router(state, router()),
        repo,
        tokens,
        bot: "bot-owner",
        notes,
        clock,
        auth,
        mailer,
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
        self.call_with(who, method, path, body, true, &[]).await
    }

    async fn call_with(
        &self,
        who: Option<i64>,
        method: Method,
        path: &str,
        body: Option<Value>,
        csrf_header: bool,
        extra: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method.clone())
            .uri(format!("/api/v1{path}"))
            .header("origin", "http://localhost:8080");
        if let Some(uid) = who {
            let raw = &self.tokens[&uid];
            req = req.header("cookie", format!("sr_session={raw}"));
            if csrf_header && method != Method::GET {
                req = req.header("x-sr-csrf", csrf::token_for(raw));
            }
        }
        for (k, v) in extra {
            req = req.header(*k, *v);
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
        let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, json)
    }
}

/// (method, path, permission the endpoint needs, body). Ids point at seeded rows.
fn endpoints() -> Vec<(Method, String, Option<&'static str>, Option<Value>)> {
    let reason = || Some(json!({ "reason": "because" }));
    vec![
        (Method::GET, "/admin/me".into(), None, None),
        (
            Method::GET,
            "/admin/staff".into(),
            Some("APPOINT_STAFF"),
            None,
        ),
        (
            Method::GET,
            "/admin/staff/roles".into(),
            Some("APPOINT_STAFF"),
            None,
        ),
        (
            Method::POST,
            "/admin/staff".into(),
            Some("APPOINT_STAFF"),
            Some(
                json!({ "account_id": ALICE.to_string(), "role_ids": [R_BASIC.to_string()], "reason": "r" }),
            ),
        ),
        (
            Method::GET,
            "/admin/staff/former".into(),
            Some("APPOINT_STAFF"),
            None,
        ),
        (
            Method::PUT,
            format!("/admin/staff/{BASIC}/roles"),
            Some("APPOINT_STAFF"),
            Some(json!({ "role_ids": [R_BASIC.to_string()], "reason": "r" })),
        ),
        (
            Method::DELETE,
            format!("/admin/staff/{BASIC}"),
            Some("APPOINT_STAFF"),
            reason(),
        ),
        (
            Method::GET,
            "/admin/accounts".into(),
            Some("MANAGE_ACCOUNTS"),
            None,
        ),
        (
            Method::GET,
            format!("/admin/accounts/{ALICE}"),
            Some("MANAGE_ACCOUNTS"),
            None,
        ),
        (
            Method::POST,
            format!("/admin/accounts/{ALICE}/suspend"),
            Some("MANAGE_ACCOUNTS"),
            reason(),
        ),
        (
            Method::POST,
            format!("/admin/accounts/{BOB}/unsuspend"),
            Some("MANAGE_ACCOUNTS"),
            reason(),
        ),
        (
            Method::POST,
            format!("/admin/accounts/{ALICE}/force-logout"),
            Some("MANAGE_ACCOUNTS"),
            reason(),
        ),
        (
            Method::POST,
            format!("/admin/accounts/{ALICE}/reset-2fa"),
            Some("MANAGE_ACCOUNTS"),
            reason(),
        ),
        (
            Method::POST,
            format!("/admin/accounts/{ALICE}/delete-request"),
            Some("MANAGE_ACCOUNTS"),
            reason(),
        ),
        (
            Method::POST,
            format!("/admin/accounts/{ALICE}/ban"),
            Some("PLATFORM_BAN"),
            Some(json!({ "category": "standard", "reason": "r" })),
        ),
        (
            Method::GET,
            format!("/admin/persons/{}", ALICE + 500_000),
            Some("MANAGE_ACCOUNTS"),
            None,
        ),
        (
            Method::POST,
            format!("/admin/persons/{}/ban", BOB + 500_000),
            Some("PLATFORM_BAN"),
            Some(
                json!({ "category": "serious", "reason": "r", "source_account_id": BOB.to_string() }),
            ),
        ),
        (
            Method::POST,
            format!("/admin/persons/{}/unban", BOB + 500_000),
            Some("PLATFORM_BAN"),
            reason(),
        ),
        (
            Method::GET,
            "/admin/bans".into(),
            Some("PLATFORM_BAN"),
            None,
        ),
        (
            Method::GET,
            format!("/admin/accounts/{ALICE}/invite-tree"),
            Some("MANAGE_INVITES"),
            None,
        ),
        (
            Method::GET,
            format!("/admin/accounts/{OWNER}/invite-descendants?depth=2"),
            Some("MANAGE_INVITES"),
            None,
        ),
        (
            Method::GET,
            format!("/admin/accounts/{ALICE}/credits"),
            Some("MANAGE_INVITES"),
            None,
        ),
        (
            Method::POST,
            format!("/admin/accounts/{ALICE}/credits"),
            Some("MANAGE_INVITES"),
            Some(json!({ "delta": 2, "reason": "r" })),
        ),
        (
            Method::GET,
            "/admin/invites".into(),
            Some("MANAGE_INVITES"),
            None,
        ),
        (
            Method::DELETE,
            "/admin/invites/999".into(),
            Some("MANAGE_INVITES"),
            reason(),
        ),
        (
            Method::GET,
            "/admin/suspicion".into(),
            Some("MANAGE_INVITES"),
            None,
        ),
        (
            Method::POST,
            format!("/admin/persons/{}/clear-suspicion", ALICE + 500_000),
            Some("MANAGE_INVITES"),
            reason(),
        ),
        (
            Method::GET,
            "/admin/settings".into(),
            Some("MANAGE_FEATURES"),
            None,
        ),
        (
            Method::PATCH,
            "/admin/settings".into(),
            Some("MANAGE_FEATURES"),
            Some(json!({ "values": { "accounts.cap": 100 } })),
        ),
        (
            Method::GET,
            "/admin/features".into(),
            Some("MANAGE_FEATURES"),
            None,
        ),
        (
            Method::PATCH,
            "/admin/features/demo.thing".into(),
            Some("MANAGE_FEATURES"),
            Some(json!({ "enabled": false })),
        ),
        (Method::GET, "/admin/stats".into(), None, None),
        (Method::GET, "/admin/system".into(), Some("OPERATOR"), None),
        (
            Method::POST,
            "/admin/system/email-test".into(),
            Some("OPERATOR"),
            None,
        ),
        // ---- phase 13
        (
            Method::GET,
            format!("/admin/accounts/{ALICE}/sessions"),
            Some("MANAGE_ACCOUNTS"),
            None,
        ),
        (
            Method::GET,
            format!("/admin/accounts/{ALICE}/guilds"),
            Some("MANAGE_ACCOUNTS"),
            None,
        ),
        (
            Method::POST,
            format!("/admin/accounts/{ALICE}/freeze-invites"),
            Some("MANAGE_INVITES"),
            reason(),
        ),
        (
            Method::POST,
            format!("/admin/accounts/{ALICE}/unfreeze-invites"),
            Some("MANAGE_INVITES"),
            reason(),
        ),
        (
            Method::POST,
            format!("/admin/accounts/{ALICE}/unlink"),
            Some("MANAGE_ACCOUNTS"),
            reason(),
        ),
        (
            Method::PUT,
            format!("/admin/persons/{}/notes", ALICE + 500_000),
            Some("MANAGE_ACCOUNTS"),
            Some(json!({ "notes": "n" })),
        ),
        (
            Method::POST,
            format!("/admin/persons/{}/link", BOB + 500_000),
            Some("MANAGE_ACCOUNTS"),
            Some(json!({ "account_id": ALICE.to_string(), "reason": "r" })),
        ),
        (
            Method::GET,
            format!("/admin/persons/{}/suspicion-log", ALICE + 500_000),
            Some("MANAGE_INVITES"),
            None,
        ),
        (
            Method::GET,
            "/admin/guilds".into(),
            Some("MANAGE_GUILDS_GLOBAL"),
            None,
        ),
        (
            Method::GET,
            "/admin/guilds/900".into(),
            Some("MANAGE_GUILDS_GLOBAL"),
            None,
        ),
        (
            Method::GET,
            "/admin/jobs".into(),
            Some("MANAGE_TRANSCODE"),
            None,
        ),
        (
            Method::POST,
            "/admin/jobs/999/retry".into(),
            Some("MANAGE_TRANSCODE"),
            reason(),
        ),
        (
            Method::DELETE,
            "/admin/jobs/999".into(),
            Some("MANAGE_TRANSCODE"),
            reason(),
        ),
        (
            Method::GET,
            "/admin/access-log".into(),
            Some("VIEW_ACCESS_LOG"),
            None,
        ),
        (
            Method::GET,
            "/admin/access-log/verify".into(),
            Some("VIEW_ACCESS_LOG"),
            None,
        ),
        // ---- phase 10
        (
            Method::GET,
            "/admin/staff/offers".into(),
            Some("APPOINT_STAFF"),
            None,
        ),
        (
            Method::POST,
            "/admin/staff/offers".into(),
            Some("APPOINT_STAFF"),
            Some(
                json!({ "account_id": ALICE.to_string(), "role_ids": [R_BASIC.to_string()], "reason": "r" }),
            ),
        ),
        (
            Method::DELETE,
            "/admin/staff/offers/999".into(),
            Some("APPOINT_STAFF"),
            reason(),
        ),
        (
            Method::GET,
            "/admin/staff/delegations".into(),
            Some("APPOINT_STAFF"),
            None,
        ),
        (
            Method::PUT,
            format!("/admin/staff/{MODER}/delegation"),
            Some("OWNER"),
            Some(json!({ "max_permissions": "1", "reason": "r" })),
        ),
        (
            Method::DELETE,
            format!("/admin/staff/{MODER}/delegation"),
            Some("OWNER"),
            reason(),
        ),
        (
            Method::POST,
            format!("/admin/accounts/{ALICE}/rename"),
            Some("MANAGE_ACCOUNTS"),
            Some(json!({ "username": "alice2", "reason": "r" })),
        ),
        (
            Method::POST,
            format!("/admin/accounts/{ALICE}/change-email"),
            Some("MANAGE_ACCOUNTS"),
            Some(json!({ "email": "new@example.test", "reason": "r" })),
        ),
        (
            Method::POST,
            format!("/admin/accounts/{ALICE}/badges"),
            Some("MANAGE_ACCOUNTS"),
            Some(json!({ "grant": ["contributor"], "reason": "r" })),
        ),
        (
            Method::POST,
            format!("/admin/accounts/{ALICE}/force-password-reset"),
            Some("MANAGE_ACCOUNTS"),
            reason(),
        ),
        (Method::GET, "/admin/reauth".into(), None, None),
        (
            Method::GET,
            "/admin/private/channels/999/messages?reason=r".into(),
            Some("READ_PRIVATE_CONTENT"),
            None,
        ),
        (
            Method::GET,
            format!("/admin/private/accounts/{ALICE}/dms?reason=r"),
            Some("READ_PRIVATE_CONTENT"),
            None,
        ),
        (
            Method::GET,
            "/admin/private/guilds/999/channels?reason=r".into(),
            Some("READ_PRIVATE_CONTENT"),
            None,
        ),
        (
            Method::GET,
            "/admin/announcements".into(),
            Some("MANAGE_FEATURES"),
            None,
        ),
        (
            Method::POST,
            "/admin/announcements".into(),
            Some("MANAGE_FEATURES"),
            Some(json!({ "title": "t", "body": "b" })),
        ),
        (
            Method::DELETE,
            "/admin/announcements/999".into(),
            Some("MANAGE_FEATURES"),
            reason(),
        ),
        (Method::GET, "/admin/alerts".into(), None, None),
        (Method::POST, "/admin/alerts/999/ack".into(), None, None),
    ]
}

#[tokio::test]
async fn every_endpoint_enforces_the_auth_matrix() {
    for (method, path, perm, body) in endpoints() {
        let label = format!("{method} {path}");
        let e = env().await;
        // No credentials: 401.
        let (s, _) = e.call(None, method.clone(), &path, body.clone()).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{label}: anonymous");
        // Non-staff members (plain accounts): 403 staff-only.
        for who in [OUTSIDER, ALICE] {
            let (s, b) = e.call(Some(who), method.clone(), &path, body.clone()).await;
            assert_eq!(
                (s, b["code"].as_u64()),
                (StatusCode::FORBIDDEN, Some(900_002)),
                "{label}: user {who}"
            );
        }
        // A bot token never has platform standing.
        let (s, b) = e
            .call_with(
                None,
                method.clone(),
                &path,
                body.clone(),
                false,
                &[("authorization", &format!("Bot {}", e.bot))],
            )
            .await;
        assert_eq!(
            (s, b["code"].as_u64()),
            (StatusCode::FORBIDDEN, Some(900_002)),
            "{label}: bot"
        );
        // Staff lacking the permission: 403 missing permissions (50013), naming the bit.
        if let Some(p) = perm {
            let (s, b) = e
                .call(Some(BASIC), method.clone(), &path, body.clone())
                .await;
            assert_eq!(
                (s, b["code"].as_u64()),
                (StatusCode::FORBIDDEN, Some(50_013)),
                "{label}: basic staff"
            );
            assert!(
                b["message"].as_str().unwrap().contains(p),
                "{label}: names {p}: {b}"
            );
        }
        // The Owner gets through: anything but an auth failure or a server error.
        let (s, b) = e
            .call(Some(OWNER), method.clone(), &path, body.clone())
            .await;
        assert!(
            !matches!(
                s,
                StatusCode::UNAUTHORIZED
                    | StatusCode::FORBIDDEN
                    | StatusCode::INTERNAL_SERVER_ERROR
            ),
            "{label}: owner got {s}: {b}"
        );
    }
}

#[tokio::test]
async fn a_mutation_without_csrf_is_refused() {
    let e = env().await;
    let (s, _) = e
        .call_with(
            Some(OWNER),
            Method::POST,
            &format!("/admin/accounts/{ALICE}/suspend"),
            Some(json!({"reason":"x"})),
            false,
            &[],
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(e.repo.state_of(ALICE), Some(AccountState::Active));
}

#[tokio::test]
async fn moderator_acts_within_permissions_and_is_logged() {
    let e = env().await;
    assert_eq!(e.repo.sessions_of(ALICE), 2);
    let (s, b) = e
        .call(
            Some(MODER),
            Method::POST,
            &format!("/admin/accounts/{ALICE}/suspend"),
            Some(json!({"reason":"spam"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["state"], "deactivated");
    assert_eq!(e.repo.sessions_of(ALICE), 0);
    // Reason is mandatory, from the body or the header.
    let (s, b) = e
        .call(
            Some(MODER),
            Method::POST,
            &format!("/admin/accounts/{ALICE}/unsuspend"),
            None,
        )
        .await;
    assert_eq!(
        (s, b["code"].as_u64()),
        (StatusCode::BAD_REQUEST, Some(900_003))
    );
    let (s, _) = e
        .call_with(
            Some(MODER),
            Method::POST,
            &format!("/admin/accounts/{ALICE}/unsuspend"),
            None,
            true,
            &[("x-audit-log-reason", "mistake")],
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    // Outside the role.
    for path in [
        "/admin/settings",
        "/admin/suspicion",
        "/admin/access-log",
        "/admin/staff",
    ] {
        let (s, b) = e.call(Some(MODER), Method::GET, path, None).await;
        assert_eq!(
            (s, b["code"].as_u64()),
            (StatusCode::FORBIDDEN, Some(50_013)),
            "{path}"
        );
    }
    // Staff accounts are the Owner's to manage.
    let (s, _) = e
        .call(
            Some(MODER),
            Method::POST,
            &format!("/admin/accounts/{BASIC}/suspend"),
            Some(json!({"reason":"x"})),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // The Owner reads the log: the suspension, the denials, and a verified chain.
    let (s, b) = e
        .call(
            Some(OWNER),
            Method::GET,
            "/admin/access-log?action=account.suspend",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["items"][0]["reason"], "spam");
    assert_eq!(b["items"][0]["actor_id"], MODER.to_string());
    let (_, b) = e
        .call(
            Some(OWNER),
            Method::GET,
            "/admin/access-log?kind=denied",
            None,
        )
        .await;
    assert!(b["items"].as_array().unwrap().len() >= 4);
    let (s, b) = e
        .call(
            Some(OWNER),
            Method::GET,
            "/admin/access-log?kind=bogus",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");
    let (s, b) = e
        .call(Some(OWNER), Method::GET, "/admin/access-log/verify", None)
        .await;
    assert_eq!((s, b["ok"].clone()), (StatusCode::OK, json!(true)));
    // Tampering is reported by the endpoint.
    e.repo.tamper_log(1, "edited");
    let (_, b) = e
        .call(Some(OWNER), Method::GET, "/admin/access-log/verify", None)
        .await;
    assert_eq!(
        (b["ok"].clone(), b["first_bad_seq"].clone()),
        (json!(false), json!(1))
    );
}

#[tokio::test]
async fn appointing_and_revoking_over_http_changes_access_at_once() {
    let e = env().await;
    let (s, _) = e
        .call(Some(ALICE), Method::GET, "/admin/accounts", None)
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, b) = e
        .call(
            Some(OWNER),
            Method::POST,
            "/admin/staff",
            Some(json!({ "account_id": ALICE.to_string(), "role_ids": [R_MOD.to_string()], "reason": "welcome" })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["roles"][0]["name"], "Moderator");
    let (s, _) = e
        .call(Some(ALICE), Method::GET, "/admin/accounts", None)
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = e
        .call(Some(OWNER), Method::POST, "/admin/staff", Some(json!({ "account_id": BOB.to_string(), "role_ids": [R_OWNER.to_string()], "reason": "x" })))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, b) = e
        .call(
            Some(OWNER),
            Method::POST,
            "/admin/staff",
            Some(json!({ "account_id": BOB.to_string(), "role_ids": [R_MOD.to_string()] })),
        )
        .await;
    assert_eq!(
        (s, b["code"].as_u64()),
        (StatusCode::BAD_REQUEST, Some(900_003))
    );
    // Revoke: the very next request is refused.
    let (s, _) = e
        .call(
            Some(OWNER),
            Method::DELETE,
            &format!("/admin/staff/{ALICE}"),
            Some(json!({ "reason": "left" })),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, b) = e
        .call(Some(ALICE), Method::GET, "/admin/accounts", None)
        .await;
    assert_eq!(
        (s, b["code"].as_u64()),
        (StatusCode::FORBIDDEN, Some(900_002))
    );
    // The Owner is immutable.
    let (s, _) = e
        .call(
            Some(OWNER),
            Method::DELETE,
            &format!("/admin/staff/{OWNER}"),
            Some(json!({ "reason": "x" })),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (_, b) = e.call(Some(OWNER), Method::GET, "/admin/me", None).await;
    assert_eq!(b["is_owner"], true);
}

#[tokio::test]
async fn accounts_search_paginates_and_validates() {
    let e = env().await;
    let (s, b) = e
        .call(Some(MODER), Method::GET, "/admin/accounts?limit=2", None)
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["items"].as_array().unwrap().len(), 2);
    let cursor = b["next_after"].as_str().unwrap().to_owned();
    let (_, b) = e
        .call(
            Some(MODER),
            Method::GET,
            &format!("/admin/accounts?limit=100&after={cursor}"),
            None,
        )
        .await;
    let c: i64 = cursor.parse().unwrap();
    assert!(
        b["items"].as_array().unwrap().iter().all(|a| a["id"]
            .as_str()
            .unwrap()
            .parse::<i64>()
            .unwrap()
            > c)
    );
    let (_, b) = e
        .call(Some(MODER), Method::GET, "/admin/accounts?q=ALI", None)
        .await;
    assert_eq!(b["items"][0]["username"], "alice");
    let (s, _) = e
        .call(Some(MODER), Method::GET, "/admin/accounts?state=nope", None)
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = e
        .call(Some(MODER), Method::GET, "/admin/accounts/not-an-id", None)
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, b) = e
        .call(Some(MODER), Method::GET, "/admin/accounts/424242", None)
        .await;
    assert_eq!(
        (s, b["code"].as_u64()),
        (StatusCode::NOT_FOUND, Some(10_001))
    );
}

#[tokio::test]
async fn ban_invite_graph_and_credits_over_http() {
    let e = env().await;
    e.repo
        .add_account(40, "carol", AccountState::Active, None, Some(ALICE));
    e.repo.add_credits(ALICE, 3);
    let (s, b) = e
        .call(
            Some(MODER),
            Method::POST,
            "/admin/accounts/40/ban",
            Some(json!({ "category": "serious", "reason": "fraud" })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["inviter_suspicion"]["level"], 1);
    assert_eq!(e.repo.state_of(40), Some(AccountState::PlatformBanned));
    let (s, _) = e
        .call(
            Some(MODER),
            Method::POST,
            "/admin/accounts/40/ban",
            Some(json!({ "category": "serious", "reason": "again" })),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (_, b) = e
        .call(
            Some(MODER),
            Method::GET,
            "/admin/bans?active_only=true",
            None,
        )
        .await;
    assert_eq!(b["items"].as_array().unwrap().len(), 1);
    let (s, _) = e
        .call(Some(MODER), Method::GET, "/admin/suspicion", None)
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (_, b) = e
        .call(Some(OWNER), Method::GET, "/admin/suspicion", None)
        .await;
    assert_eq!(b[0]["suspicion_level"], 1);
    let (_, b) = e
        .call(
            Some(OWNER),
            Method::GET,
            "/admin/accounts/40/invite-tree",
            None,
        )
        .await;
    let path: Vec<_> = b["ancestors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["username"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(path, ["owner", "alice"]);
    let (_, b) = e
        .call(
            Some(OWNER),
            Method::GET,
            &format!("/admin/accounts/{OWNER}/invite-descendants?depth=2"),
            None,
        )
        .await;
    assert!(b["nodes"].as_array().unwrap().len() >= 5);
    let (s, b) = e
        .call(
            Some(OWNER),
            Method::POST,
            &format!("/admin/accounts/{ALICE}/credits"),
            Some(json!({ "delta": -10, "reason": "abuse" })),
        )
        .await;
    assert_eq!(
        (s, b["code"].as_u64()),
        (StatusCode::CONFLICT, Some(900_010))
    );
    let (_, b) = e
        .call(
            Some(OWNER),
            Method::POST,
            &format!("/admin/accounts/{ALICE}/credits"),
            Some(json!({ "delta": 2, "reason": "gift" })),
        )
        .await;
    assert_eq!(b["balance"], 5);
    let (_, b) = e
        .call(
            Some(OWNER),
            Method::GET,
            &format!("/admin/accounts/{ALICE}/credits"),
            None,
        )
        .await;
    assert_eq!(b["entries"][0]["reason"], "staff_grant");
    let (s, _) = e
        .call(
            Some(OWNER),
            Method::POST,
            &format!("/admin/persons/{}/clear-suspicion", ALICE + 500_000),
            Some(json!({ "reason": "ok" })),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, b) = e
        .call(
            Some(MODER),
            Method::POST,
            &format!("/admin/persons/{}/unban", 40 + 500_000),
            Some(json!({ "reason": "appeal" })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["accounts_reactivated"], 1);
}

#[tokio::test]
async fn settings_features_system_and_jobs() {
    let e = env().await;
    let (_, b) = e
        .call(Some(OWNER), Method::GET, "/admin/settings", None)
        .await;
    let cap = b
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["key"] == "accounts.cap")
        .unwrap();
    assert_eq!(
        (cap["value"].clone(), cap["is_default"].clone()),
        (json!(20000), json!(true))
    );
    let (s, b) = e
        .call(
            Some(OWNER),
            Method::PATCH,
            "/admin/settings",
            Some(json!({ "values": { "accounts.cap": 123 }, "reason": "tune" })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert!(
        b.as_array()
            .unwrap()
            .iter()
            .any(|s| s["key"] == "accounts.cap" && s["value"] == 123)
    );
    let (s, b) = e
        .call(
            Some(OWNER),
            Method::PATCH,
            "/admin/settings",
            Some(json!({ "values": { "accounts.cap": 0, "bogus": 1 } })),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");
    // Features.
    let (_, b) = e
        .call(Some(OWNER), Method::GET, "/admin/features", None)
        .await;
    assert_eq!(b[0]["id"], "demo.thing");
    assert_eq!(
        (b[0]["enabled"].clone(), b[0]["settings"]["limit"].clone()),
        (json!(true), json!(10))
    );
    let (s, b) = e
        .call(
            Some(OWNER),
            Method::PATCH,
            "/admin/features/demo.thing",
            Some(json!({ "enabled": false, "settings": { "limit": 50 } })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(
        (b["enabled"].clone(), b["settings"]["limit"].clone()),
        (json!(false), json!(50))
    );
    let (s, _) = e
        .call(
            Some(OWNER),
            Method::PATCH,
            "/admin/features/demo.thing",
            Some(json!({ "settings": { "limit": 5000 } })),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (_, b) = e
        .call(
            Some(OWNER),
            Method::PATCH,
            "/admin/features/demo.thing",
            Some(json!({ "reset_enabled": true })),
        )
        .await;
    assert_eq!(b["enabled"], true);
    let (s, b) = e
        .call(
            Some(OWNER),
            Method::PATCH,
            "/admin/features/nope",
            Some(json!({ "enabled": true })),
        )
        .await;
    assert_eq!(
        (s, b["code"].as_u64()),
        (StatusCode::NOT_FOUND, Some(900_009))
    );
    // System and stats.
    let (s, b) = e
        .call(Some(OWNER), Method::GET, "/admin/system", None)
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["gateway_sessions"], 7);
    let (_, b) = e.call(Some(BASIC), Method::GET, "/admin/stats", None).await;
    assert!(b["accounts_active"].as_i64().unwrap() >= 5);
    // Jobs.
    let dead = e.repo.add_job("media.transcode", "dead");
    e.repo.add_job("media.transcode", "queued");
    let (_, b) = e
        .call(Some(OWNER), Method::GET, "/admin/jobs?state=dead", None)
        .await;
    assert_eq!(b["items"].as_array().unwrap().len(), 1);
    let (s, _) = e
        .call(Some(OWNER), Method::GET, "/admin/jobs?state=bogus", None)
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = e
        .call(
            Some(OWNER),
            Method::POST,
            &format!("/admin/jobs/{dead}/retry"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = e
        .call(
            Some(OWNER),
            Method::DELETE,
            &format!("/admin/jobs/{dead}"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "no longer dead");
    let (_, b) = e
        .call(
            Some(OWNER),
            Method::GET,
            "/admin/access-log?action=features.patch",
            None,
        )
        .await;
    assert!(b["items"].as_array().unwrap().len() >= 2);
}

#[test]
fn openapi_lists_every_admin_path() {
    let doc = serde_json::to_value(crate::routes::openapi()).unwrap();
    let paths = doc["paths"].as_object().unwrap();
    for (_, path, _, _) in endpoints() {
        let p = path.split('?').next().unwrap();
        let found = paths.keys().any(|k| {
            let re: Vec<&str> = k.split('/').collect();
            let got: Vec<&str> = p.split('/').collect();
            re.len() == got.len()
                && re
                    .iter()
                    .zip(&got)
                    .all(|(a, b)| a == b || (a.starts_with('{') && !b.is_empty()))
        });
        assert!(found, "{p} missing from the OpenAPI document");
    }
    assert!(paths.contains_key("/admin/access-log/verify"));
}

// ---- phase 10 -------------------------------------------------------------------------

#[tokio::test]
async fn an_appointment_offer_is_answered_from_the_system_dm() {
    let e = env().await;
    // Only someone with APPOINT_STAFF may offer; the Owner offers Alice a basic role.
    let (s, offer) = e
        .call(
            Some(OWNER),
            Method::POST,
            "/admin/staff/offers",
            Some(json!({ "account_id": ALICE.to_string(), "role_ids": [R_BASIC.to_string()], "reason": "she is great" })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{offer}");
    assert_eq!(offer["state"], "pending");
    assert_eq!(offer["username"], "alice");
    let oid = offer["id"].as_str().unwrap().to_owned();
    // The system user DMed her: the offer remembers the DM.
    assert_eq!(e.notes.offers.lock().unwrap().len(), 1);
    assert!(offer["dm_channel_id"].is_string());
    // Nobody is staff yet.
    let (s, _) = e.call(Some(ALICE), Method::GET, "/admin/me", None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // Bob cannot answer the offer made to Alice, and gets the same 404 as for an unknown one.
    let (s, b) = e
        .call(
            Some(BOB),
            Method::POST,
            &format!("/staff/offers/{oid}/respond"),
            Some(json!({ "accept": true })),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "{b}");
    // Alice accepts and is staff from her next request.
    let (s, b) = e
        .call(
            Some(ALICE),
            Method::POST,
            &format!("/staff/offers/{oid}/respond"),
            Some(json!({ "accept": true })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["state"], "accepted");
    let (s, me) = e.call(Some(ALICE), Method::GET, "/admin/me", None).await;
    assert_eq!(s, StatusCode::OK, "{me}");
    assert!(e.notes.notices.lock().unwrap()[0].contains("alice joined the staff as Basic"));
    // Answering again conflicts; the Owner sees it in the list as accepted.
    let (s, _) = e
        .call(
            Some(ALICE),
            Method::POST,
            &format!("/staff/offers/{oid}/respond"),
            Some(json!({ "accept": false })),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (_, list) = e
        .call(Some(OWNER), Method::GET, "/admin/staff/offers", None)
        .await;
    assert_eq!(list[0]["state"], "accepted");
    // Anonymous callers cannot respond.
    let (s, _) = e
        .call(
            None,
            Method::POST,
            &format!("/staff/offers/{oid}/respond"),
            Some(json!({ "accept": true })),
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn declined_and_cancelled_offers_grant_nothing() {
    let e = env().await;
    let offer = |who: i64| json!({ "account_id": who.to_string(), "role_ids": [R_BASIC.to_string()], "reason": "r" });
    let (_, a) = e
        .call(
            Some(OWNER),
            Method::POST,
            "/admin/staff/offers",
            Some(offer(ALICE)),
        )
        .await;
    let (s, b) = e
        .call(
            Some(ALICE),
            Method::POST,
            &format!("/staff/offers/{}/respond", a["id"].as_str().unwrap()),
            Some(json!({ "accept": false })),
        )
        .await;
    assert_eq!((s, b["state"].as_str()), (StatusCode::OK, Some("declined")));
    let (s, _) = e.call(Some(ALICE), Method::GET, "/admin/me", None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // A fresh offer to Bob, withdrawn before he answers.
    let (_, c) = e
        .call(
            Some(OWNER),
            Method::POST,
            "/admin/staff/offers",
            Some(offer(BOB)),
        )
        .await;
    let cid = c["id"].as_str().unwrap();
    let (s, b) = e
        .call(
            Some(OWNER),
            Method::DELETE,
            &format!("/admin/staff/offers/{cid}"),
            Some(json!({ "reason": "changed my mind" })),
        )
        .await;
    assert_eq!(
        (s, b["state"].as_str()),
        (StatusCode::OK, Some("cancelled"))
    );
    let (s, _) = e
        .call(
            Some(BOB),
            Method::POST,
            &format!("/staff/offers/{cid}/respond"),
            Some(json!({ "accept": true })),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    // Offers need a reason.
    let (s, b) = e
        .call(
            Some(OWNER),
            Method::POST,
            "/admin/staff/offers",
            Some(json!({ "account_id": BOB.to_string(), "role_ids": [R_BASIC.to_string()] })),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");
}

#[tokio::test]
async fn delegating_lets_a_moderator_appoint_inside_the_ceiling() {
    let e = env().await;
    let ceiling = (P::STAFF_BASIC | P::MANAGE_ACCOUNTS).bits().to_string();
    let (s, d) = e
        .call(
            Some(OWNER),
            Method::PUT,
            &format!("/admin/staff/{MODER}/delegation"),
            Some(json!({ "max_permissions": ceiling, "reason": "trusted" })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{d}");
    assert_eq!(d["max_permissions"], ceiling);
    let (_, list) = e
        .call(Some(OWNER), Method::GET, "/admin/staff/delegations", None)
        .await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    // The moderator can now offer the Basic role, but not one beyond the ceiling.
    let (s, b) = e
        .call(
            Some(MODER),
            Method::POST,
            "/admin/staff/offers",
            Some(json!({ "account_id": ALICE.to_string(), "role_ids": [R_BASIC.to_string()], "reason": "r" })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, b) = e
        .call(
            Some(MODER),
            Method::POST,
            "/admin/staff/offers",
            Some(json!({ "account_id": BOB.to_string(), "role_ids": [R_MOD.to_string()], "reason": "r" })),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    assert!(b["message"].as_str().unwrap().contains("delegated"));
    // Garbage and owner-only bits are refused; the moderator cannot delegate.
    for bad in ["x", "0", &P::READ_PRIVATE_CONTENT.bits().to_string()] {
        let (s, _) = e
            .call(
                Some(OWNER),
                Method::PUT,
                &format!("/admin/staff/{BASIC}/delegation"),
                Some(json!({ "max_permissions": bad, "reason": "r" })),
            )
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad}");
    }
    let (s, _) = e
        .call(
            Some(MODER),
            Method::PUT,
            &format!("/admin/staff/{BASIC}/delegation"),
            Some(json!({ "max_permissions": "1", "reason": "r" })),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // Ending it removes the right at once.
    let (s, _) = e
        .call(
            Some(OWNER),
            Method::DELETE,
            &format!("/admin/staff/{MODER}/delegation"),
            Some(json!({ "reason": "done" })),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = e
        .call(Some(MODER), Method::GET, "/admin/staff/offers", None)
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn account_edits_over_http() {
    let e = env().await;
    let a = |act: &str| format!("/admin/accounts/{ALICE}/{act}");
    let (s, b) = e
        .call(
            Some(MODER),
            Method::POST,
            &a("rename"),
            Some(json!({ "username": "Alicia", "display_name": "Ali", "reason": "asked" })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(
        (b["username"].as_str(), b["display_name"].as_str()),
        (Some("alicia"), Some("Ali"))
    );
    // null clears the display name; a missing key leaves it.
    let (_, b) = e
        .call(
            Some(MODER),
            Method::POST,
            &a("rename"),
            Some(json!({ "username": "alice3", "reason": "asked" })),
        )
        .await;
    assert_eq!(b["display_name"], "Ali");
    let (_, b) = e
        .call(
            Some(MODER),
            Method::POST,
            &a("rename"),
            Some(json!({ "display_name": null, "reason": "asked" })),
        )
        .await;
    assert!(b["display_name"].is_null());
    let (s, _) = e
        .call(
            Some(MODER),
            Method::POST,
            &a("rename"),
            Some(json!({ "username": "bob", "reason": "r" })),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, _) = e
        .call(
            Some(MODER),
            Method::POST,
            &a("rename"),
            Some(json!({ "username": "x y", "reason": "r" })),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    let (s, b) = e
        .call(
            Some(MODER),
            Method::POST,
            &a("change-email"),
            Some(json!({ "email": "new@example.test", "reason": "lost" })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(
        (b["email"].as_str(), b["email_verified"].as_bool()),
        (Some("new@example.test"), Some(false))
    );

    let (s, b) = e
        .call(
            Some(MODER),
            Method::POST,
            &a("badges"),
            Some(json!({ "grant": ["early_member"], "reason": "founder" })),
        )
        .await;
    assert_eq!(
        (s, b["flags"].as_i64()),
        (StatusCode::OK, Some(1 << 8)),
        "{b}"
    );
    let (s, _) = e
        .call(
            Some(MODER),
            Method::POST,
            &a("badges"),
            Some(json!({ "grant": ["moderator"], "reason": "r" })),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    let (s, b) = e
        .call(
            Some(MODER),
            Method::POST,
            &a("force-password-reset"),
            Some(json!({ "reason": "phished" })),
        )
        .await;
    assert_eq!(
        (s, b["sessions_revoked"].as_u64()),
        (StatusCode::OK, Some(2)),
        "{b}"
    );
    assert!(e.repo.password_reset_forced(ALICE));
    // The audit trail has one row per edit with the moderator as actor.
    let rows = e.repo.log_rows();
    for act in [
        "account.rename",
        "account.change_email",
        "account.badges",
        "account.force_password_reset",
    ] {
        assert!(
            rows.iter().any(|r| r.action == act && r.actor_id == MODER),
            "{act}"
        );
    }
    // The email address is not in the log in the clear.
    assert!(rows.iter().all(|r| !r.detail.contains("new@example")));
}

#[tokio::test]
async fn announcements_reach_every_signed_in_user_and_can_be_ended() {
    let e = env().await;
    let (s, _) = e
        .call(None, Method::GET, "/announcements/active", None)
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (_, none) = e
        .call(Some(OUTSIDER), Method::GET, "/announcements/active", None)
        .await;
    assert_eq!(none.as_array().unwrap().len(), 0);
    // A moderator cannot publish.
    let (s, _) = e
        .call(
            Some(MODER),
            Method::POST,
            "/admin/announcements",
            Some(json!({ "title": "x", "body": "y" })),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, a) = e
        .call(
            Some(OWNER),
            Method::POST,
            "/admin/announcements",
            Some(json!({
                "title": "Downtime tonight",
                "body": "We restart at 22:00 UTC.",
                "severity": "warning",
                "send_dm": true,
                "ends_at_ms": T0 + 3_600_000,
            })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{a}");
    assert_eq!(a["severity"], "warning");
    assert_eq!(
        e.notes.broadcasts.lock().unwrap().len(),
        1,
        "send_dm asks for the DM"
    );
    let (_, seen) = e
        .call(Some(OUTSIDER), Method::GET, "/announcements/active", None)
        .await;
    assert_eq!(seen[0]["title"], "Downtime tonight");
    // It lapses by itself...
    e.clock.advance_ms(2 * 3_600_000);
    let (_, seen) = e
        .call(Some(OUTSIDER), Method::GET, "/announcements/active", None)
        .await;
    assert!(seen.as_array().unwrap().is_empty());
    // ...and can be ended early.
    let (_, b) = e
        .call(
            Some(OWNER),
            Method::POST,
            "/admin/announcements",
            Some(json!({ "title": "Second", "body": "again" })),
        )
        .await;
    let id = b["id"].as_str().unwrap();
    let (s, ended) = e
        .call(
            Some(OWNER),
            Method::DELETE,
            &format!("/admin/announcements/{id}"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{ended}");
    assert!(ended["ended_at_ms"].is_i64());
    let (_, seen) = e
        .call(Some(OUTSIDER), Method::GET, "/announcements/active", None)
        .await;
    assert!(seen.as_array().unwrap().is_empty());
    // Bad input.
    let (s, _) = e
        .call(
            Some(OWNER),
            Method::POST,
            "/admin/announcements",
            Some(json!({ "title": "", "body": "x" })),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

fn private_dm(e: &Env) {
    use sr_admin::{AccountRef, PrivateChannel, PrivateMessage};
    e.repo.add_private_channel(PrivateChannel {
        id: 700,
        kind: 1,
        guild_id: None,
        name: None,
        recipients: vec![
            AccountRef {
                id: ALICE,
                username: "alice".into(),
            },
            AccountRef {
                id: BOB,
                username: "bob".into(),
            },
        ],
        private: true,
    });
    for (id, text, vis, deleted) in [
        (1, "hi", "visible", None),
        (2, "oops", "user_deleted", Some(T0 - 3_600_000)),
    ] {
        e.repo.add_private_message(
            700,
            PrivateMessage {
                id,
                author_id: ALICE,
                author_name: "alice".into(),
                content: text.into(),
                created_at_ms: T0 - 7_200_000,
                edited_at_ms: None,
                visibility: vis.into(),
                deleted_at_ms: deleted,
                attachment_count: 0,
                edits: vec![],
            },
        );
    }
}

#[tokio::test]
async fn private_reads_need_a_reason_and_leave_a_chained_row() {
    let e = env().await;
    private_dm(&e);
    // A moderator holds no READ_PRIVATE_CONTENT.
    let (s, _) = e
        .call(
            Some(MODER),
            Method::GET,
            "/admin/private/channels/700/messages?reason=r",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // No reason: refused, nothing logged as a read.
    let before = e.repo.log_rows().len();
    let (s, b) = e
        .call(
            Some(OWNER),
            Method::GET,
            "/admin/private/channels/700/messages",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");
    assert_eq!(
        e.repo
            .log_rows()
            .iter()
            .skip(before)
            .filter(|r| r.action == "private.read")
            .count(),
        0
    );
    // Query reason and header reason both work.
    let (s, page) = e
        .call(
            Some(OWNER),
            Method::GET,
            "/admin/private/channels/700/messages?reason=report%20%2342",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{page}");
    assert_eq!(page["items"].as_array().unwrap().len(), 2);
    assert_eq!(page["items"][0]["visibility"], "user_deleted");
    assert_eq!(page["channel"]["recipients"].as_array().unwrap().len(), 2);
    let (s, _) = e
        .call_with(
            Some(OWNER),
            Method::GET,
            "/admin/private/channels/700/messages",
            None,
            true,
            &[("x-audit-log-reason", "second look")],
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let reads: Vec<_> = e
        .repo
        .log_rows()
        .into_iter()
        .filter(|r| r.action == "private.read" && r.kind == sr_admin::AccessKind::PrivateRead)
        .collect();
    assert!(reads.len() >= 2);
    assert_eq!(reads[0].reason, "report #42");
    assert_eq!(reads[0].target_id, Some(700));
    assert!(
        e.repo.log_rows().iter().all(|r| !r.detail.contains("oops")),
        "content never enters the log"
    );
    // Listing someone's DMs is logged too.
    let (s, list) = e
        .call(
            Some(OWNER),
            Method::GET,
            &format!("/admin/private/accounts/{ALICE}/dms?reason=r"),
            None,
        )
        .await;
    assert_eq!(
        (s, list.as_array().map(Vec::len)),
        (StatusCode::OK, Some(1))
    );
    assert!(
        e.repo
            .log_rows()
            .iter()
            .any(|r| r.action == "private.list_dms")
    );
    // Unknown channel: 404, still logged.
    let n = e.repo.log_rows().len();
    let (s, _) = e
        .call(
            Some(OWNER),
            Method::GET,
            "/admin/private/channels/12345/messages?reason=r",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert_eq!(e.repo.log_rows().len(), n + 1);
}

fn totp_now(e: &Env) -> String {
    let secret = TotpSecret::from_bytes(&TOTP_SECRET).unwrap();
    totp::current_code(
        &secret,
        "SIDE ROOM",
        "u11",
        (e.clock.now_unix_ms() / 1000) as u64,
    )
    .unwrap()
}

#[tokio::test]
async fn sensitive_routes_wait_for_a_fresh_second_factor() {
    let e = env_with(true).await;
    private_dm(&e);
    // Give the moderator the private-content bit through a role the Owner may grant.
    e.repo.add_role(
        4,
        "Reader",
        2,
        P::STAFF_BASIC | P::READ_PRIVATE_CONTENT | P::MANAGE_ACCOUNTS,
    );
    let op = Caller::operator(SYSTEM_USER_ID);
    let svc = AdminService::new(e.repo.clone()).with_extras(e.repo.clone());
    svc.set_staff_roles(&op, MODER, &[4], Some("test"))
        .await
        .unwrap();

    let read = "/admin/private/channels/700/messages?reason=case%209";
    let (s, b) = e.call(Some(MODER), Method::GET, read, None).await;
    assert_eq!(
        (s, b["code"].as_u64()),
        (StatusCode::FORBIDDEN, Some(900_004)),
        "{b}"
    );
    // The refusal is in the access log.
    assert!(
        e.repo
            .log_rows()
            .iter()
            .any(|r| r.action == "private.read" && r.kind == sr_admin::AccessKind::Denied)
    );
    // Status says: enrolled, not fresh.
    let (_, st) = e
        .call(Some(MODER), Method::GET, "/admin/reauth", None)
        .await;
    assert_eq!(
        (
            st["enrolled"].as_bool(),
            st["fresh"].as_bool(),
            st["required"].as_bool()
        ),
        (Some(true), Some(false), Some(true))
    );
    // A wrong code does not open the window.
    let (s, _) = e
        .call(
            Some(MODER),
            Method::POST,
            "/admin/reauth",
            Some(json!({ "method": "totp", "code": "000000" })),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = e.call(Some(MODER), Method::GET, read, None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // The right one does.
    let (s, st) = e
        .call(
            Some(MODER),
            Method::POST,
            "/admin/reauth",
            Some(json!({ "method": "totp", "code": totp_now(&e) })),
        )
        .await;
    assert_eq!(
        (s, st["fresh"].as_bool()),
        (StatusCode::OK, Some(true)),
        "{st}"
    );
    let (s, page) = e.call(Some(MODER), Method::GET, read, None).await;
    assert_eq!(s, StatusCode::OK, "{page}");
    // Other sensitive routes share the window.
    let (s, _) = e
        .call(
            Some(MODER),
            Method::POST,
            &format!("/admin/accounts/{ALICE}/force-password-reset"),
            Some(json!({ "reason": "r" })),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    // Fourteen minutes later it still holds; at sixteen it is closed again.
    e.clock.advance_ms(14 * 60_000);
    let (s, _) = e.call(Some(MODER), Method::GET, read, None).await;
    assert_eq!(s, StatusCode::OK);
    e.clock.advance_ms(2 * 60_000);
    let (s, b) = e.call(Some(MODER), Method::GET, read, None).await;
    assert_eq!(
        (s, b["code"].as_u64()),
        (StatusCode::FORBIDDEN, Some(900_004))
    );
    // The Owner has no authenticator: the sensitive actions stay closed until they enrol.
    let (s, b) = e.call(Some(OWNER), Method::GET, read, None).await;
    assert_eq!(
        (s, b["code"].as_u64()),
        (StatusCode::FORBIDDEN, Some(900_004))
    );
    assert!(b["message"].as_str().unwrap().contains("two-factor"));
    let (s, st) = e
        .call(Some(OWNER), Method::GET, "/admin/reauth", None)
        .await;
    assert_eq!((s, st["enrolled"].as_bool()), (StatusCode::OK, Some(false)));
    // Reads that are not sensitive are unaffected.
    let (s, _) = e
        .call(Some(MODER), Method::GET, "/admin/accounts", None)
        .await;
    assert_eq!(s, StatusCode::OK);
    let _ = &e.auth;
}

#[tokio::test]
async fn alerts_list_and_acknowledge() {
    let e = env().await;
    let svc = AdminService::new(e.repo.clone()).with_extras(e.repo.clone());
    let alert = svc.record_alert("disk_low", "Disk 91% full").await.unwrap();
    let (s, list) = e
        .call(Some(BASIC), Method::GET, "/admin/alerts", None)
        .await;
    assert_eq!(s, StatusCode::OK, "{list}");
    assert_eq!(list[0]["kind"], "disk_low");
    let (_, stats) = e.call(Some(BASIC), Method::GET, "/admin/stats", None).await;
    assert_eq!(stats["alerts_unacked"], 1);
    assert_eq!(stats["accounts_cap"], 20_000);
    let (s, a) = e
        .call(
            Some(BASIC),
            Method::POST,
            &format!("/admin/alerts/{}/ack", alert.id),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{a}");
    assert_eq!(a["acked_by"], BASIC.to_string());
    let (s, _) = e
        .call(Some(BASIC), Method::POST, "/admin/alerts/1/ack", None)
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = e
        .call(Some(OUTSIDER), Method::GET, "/admin/alerts", None)
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn direct_appointment_is_owner_only() {
    let e = env().await;
    // A delegate holding APPOINT_STAFF is refused; offers are their path.
    e.repo
        .add_role(77, "Delegate", 2, P::STAFF_BASIC | P::APPOINT_STAFF);
    let svc = AdminService::new(e.repo.clone()).with_clock(e.clock.clone());
    svc.appoint(
        &Caller::operator(SYSTEM_USER_ID),
        BOB,
        &[77],
        Some("delegate"),
    )
    .await
    .unwrap();
    let body = json!({ "account_id": ALICE.to_string(), "role_ids": [R_BASIC.to_string()], "reason": "r" });
    let (s, _) = e
        .call(Some(BOB), Method::POST, "/admin/staff", Some(body.clone()))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = e
        .call(Some(OWNER), Method::POST, "/admin/staff", Some(body))
        .await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test]
async fn ban_reasons_set_the_default_category() {
    // D-211: illegal content, child safety, fraud and platform abuse default to serious;
    // harassment and ban evasion to standard; an explicit category always wins.
    for (reason_category, explicit, want) in [
        ("fraud", None, "serious"),
        ("child_safety", None, "serious"),
        ("harassment", None, "standard"),
        ("ban_evasion", None, "standard"),
        ("harassment", Some("serious"), "serious"),
        ("fraud", Some("standard"), "standard"),
    ] {
        let e = env().await;
        let mut body = json!({ "reason_category": reason_category, "reason": "because" });
        if let Some(c) = explicit {
            body["category"] = json!(c);
        }
        let (s, b) = e
            .call(
                Some(OWNER),
                Method::POST,
                &format!("/admin/accounts/{ALICE}/ban"),
                Some(body),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{reason_category}: {b}");
        assert_eq!(b["ban"]["category"], want, "{reason_category} {explicit:?}");
    }
    // Neither a category nor a reason category: refused.
    let e = env().await;
    let (s, _) = e
        .call(
            Some(OWNER),
            Method::POST,
            &format!("/admin/accounts/{ALICE}/ban"),
            Some(json!({ "reason": "because" })),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn accepting_an_appointment_needs_a_second_factor() {
    let e = env().await;
    let (s, offer) = e
        .call(
            Some(OWNER),
            Method::POST,
            "/admin/staff/offers",
            Some(json!({ "account_id": OUTSIDER.to_string(), "role_ids": [R_BASIC.to_string()], "reason": "r" })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{offer}");
    let oid = offer["id"].as_str().unwrap();
    // No authenticator and no passkey: refused with 900014, and the offer stays pending.
    let (s, b) = e
        .call(
            Some(OUTSIDER),
            Method::POST,
            &format!("/staff/offers/{oid}/respond"),
            Some(json!({ "accept": true })),
        )
        .await;
    assert_eq!(
        (s, b["code"].as_u64()),
        (StatusCode::FORBIDDEN, Some(900_014)),
        "{b}"
    );
    let (_, list) = e
        .call(Some(OWNER), Method::GET, "/admin/staff/offers", None)
        .await;
    assert_eq!(list[0]["state"], "pending");
    // Declining needs no second factor.
    let (s, b) = e
        .call(
            Some(OUTSIDER),
            Method::POST,
            &format!("/staff/offers/{oid}/respond"),
            Some(json!({ "accept": false })),
        )
        .await;
    assert_eq!((s, b["state"].as_str()), (StatusCode::OK, Some("declined")));
}

#[tokio::test]
async fn entrance_copy_is_public_to_read_and_staff_to_write() {
    let e = env().await;
    let (s, v) = e.call(None, Method::GET, "/sr/entrance/copy", None).await;
    assert_eq!((s, v), (StatusCode::OK, json!({})));

    let put = |who: Option<i64>, key: &str, body: Value| {
        let path = format!("/admin/entrance/copy/{key}");
        let e = &e;
        async move { e.call(who, Method::PUT, &path, Some(body)).await }
    };
    // Needs staff with MANAGE_FEATURES.
    assert_eq!(
        put(None, "house-notes", json!({"markdown": "x"})).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        put(Some(MODER), "house-notes", json!({"markdown": "x"}))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    // Unknown keys and oversize text are refused.
    assert_eq!(
        put(Some(OWNER), "nope", json!({"markdown": "x"})).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        put(
            Some(OWNER),
            "house-notes",
            json!({"markdown": "x".repeat(16_385)})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );

    assert_eq!(
        put(
            Some(OWNER),
            "no-invitation",
            json!({"markdown": "  # Ask a friend\n\nInvites are personal.  "})
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    let (_, v) = e.call(None, Method::GET, "/sr/entrance/copy", None).await;
    assert_eq!(
        v,
        json!({"no-invitation": "# Ask a friend\n\nInvites are personal."})
    );
    // DELETE and a blank body both reset to the default (the key disappears); GET is cacheable.
    assert_eq!(
        e.call(
            Some(OWNER),
            Method::DELETE,
            "/admin/entrance/copy/no-invitation",
            None
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        e.call(None, Method::GET, "/sr/entrance/copy", None).await.1,
        json!({})
    );
    put(Some(OWNER), "no-invitation", json!({"markdown": "again"})).await;
    assert_eq!(
        put(Some(OWNER), "no-invitation", json!({"markdown": "  "}))
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    let (_, v) = e.call(None, Method::GET, "/sr/entrance/copy", None).await;
    assert_eq!(v, json!({}));
}

#[tokio::test]
async fn people_views_notes_links_freeze_and_server_list() {
    let e = env().await;
    let call = |who: i64, m: Method, p: &str, b: Option<Value>| {
        let (e, p) = (&e, p.to_owned());
        async move { e.call(Some(who), m, &p, b).await }
    };

    // Sessions and servers: MANAGE_ACCOUNTS; the drawer read is logged.
    let (s, v) = call(
        MODER,
        Method::GET,
        &format!("/admin/accounts/{ALICE}/sessions"),
        None,
    )
    .await;
    assert_eq!((s, v), (StatusCode::OK, json!([])));
    assert_eq!(
        call(
            MODER,
            Method::GET,
            &format!("/admin/accounts/{ALICE}/guilds"),
            None
        )
        .await
        .1,
        json!([])
    );
    assert_eq!(
        call(
            BASIC,
            Method::GET,
            &format!("/admin/accounts/{ALICE}/sessions"),
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );

    // Search by login IP: validated, and the lookup is logged.
    let (s, _) = call(MODER, Method::GET, "/admin/accounts?ip=not-an-ip", None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, v) = call(
        MODER,
        Method::GET,
        "/admin/accounts?ip=203.0.113.9&discord_id=1",
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");

    // Person notes: replace, cap, permission.
    let person = |uid: i64| uid + 500_000;
    let notes = format!("/admin/persons/{}/notes", person(ALICE));
    let (s, _) = call(
        MODER,
        Method::PUT,
        &notes,
        Some(json!({"notes": "knows Bob"})),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, p) = call(
        MODER,
        Method::GET,
        &format!("/admin/persons/{}", person(ALICE)),
        None,
    )
    .await;
    assert_eq!(p["notes"], "knows Bob");
    let long = "n".repeat(4_001);
    assert_eq!(
        call(MODER, Method::PUT, &notes, Some(json!({"notes": long})))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(BASIC, Method::PUT, &notes, Some(json!({"notes": "x"})))
            .await
            .0,
        StatusCode::FORBIDDEN
    );

    // Link needs a reason; the emptied Person is merged (notes carry over).
    let link = format!("/admin/persons/{}/link", person(BOB));
    let body = |reason: Option<&str>| json!({"account_id": ALICE.to_string(), "reason": reason});
    assert_eq!(
        call(MODER, Method::POST, &link, Some(body(None))).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(
            MODER,
            Method::POST,
            &link,
            Some(body(Some("same household")))
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    let (_, p) = call(
        MODER,
        Method::GET,
        &format!("/admin/persons/{}", person(BOB)),
        None,
    )
    .await;
    assert_eq!(p["accounts"].as_array().map(Vec::len), Some(2));
    assert_eq!(p["notes"], "knows Bob");
    assert_eq!(
        call(
            MODER,
            Method::GET,
            &format!("/admin/persons/{}", person(ALICE)),
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    // Linking to the Person it is already in is a conflict.
    assert_eq!(
        call(MODER, Method::POST, &link, Some(body(Some("again"))))
            .await
            .0,
        StatusCode::CONFLICT
    );

    // Unlink gives the account a Person of its own; the last account of a Person cannot split.
    let unlink = format!("/admin/accounts/{ALICE}/unlink");
    let (s, v) = call(
        MODER,
        Method::POST,
        &unlink,
        Some(json!({"reason": "mistake"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let new_person = v["person_id"].as_str().unwrap().to_owned();
    assert_ne!(new_person, person(BOB).to_string());
    assert_eq!(
        call(
            MODER,
            Method::POST,
            &unlink,
            Some(json!({"reason": "again"}))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );

    // Manual invite freeze: MANAGE_INVITES, reported on the drawer.
    let freeze = format!("/admin/accounts/{ALICE}/freeze-invites");
    assert_eq!(
        call(
            MODER,
            Method::POST,
            &freeze,
            Some(json!({"reason": "spam"}))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            OWNER,
            Method::POST,
            &freeze,
            Some(json!({"reason": "spam"}))
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    let (_, d) = call(
        OWNER,
        Method::GET,
        &format!("/admin/accounts/{ALICE}"),
        None,
    )
    .await;
    assert_eq!(d["invites_frozen"], true);
    assert_eq!(
        call(
            OWNER,
            Method::POST,
            &format!("/admin/accounts/{ALICE}/unfreeze-invites"),
            Some(json!({"reason": "resolved"}))
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    let (_, d) = call(
        OWNER,
        Method::GET,
        &format!("/admin/accounts/{ALICE}"),
        None,
    )
    .await;
    assert_eq!(d["invites_frozen"], false);
    assert_eq!(
        call(
            OWNER,
            Method::GET,
            &format!("/admin/persons/{new_person}/suspicion-log"),
            None
        )
        .await
        .1,
        json!([])
    );

    // Server list: MANAGE_GUILDS_GLOBAL, search and cursor.
    e.repo.add_guild(900, "Chess Club", OWNER, 5);
    e.repo.add_guild(901, "Chessboxing", OWNER, 2);
    e.repo.add_guild(902, "Gardening", OWNER, 9);
    assert_eq!(
        call(MODER, Method::GET, "/admin/guilds", None).await.0,
        StatusCode::FORBIDDEN
    );
    let (s, page) = call(OWNER, Method::GET, "/admin/guilds?q=chess", None).await;
    assert_eq!(s, StatusCode::OK, "{page}");
    assert_eq!(page["items"].as_array().map(Vec::len), Some(2));
    assert_eq!(page["items"][0]["owner_username"], "owner");
    let (_, page) = call(OWNER, Method::GET, "/admin/guilds?limit=2", None).await;
    assert_eq!(page["next_after"], "901");
    let (_, page) = call(OWNER, Method::GET, "/admin/guilds?limit=2&after=901", None).await;
    assert_eq!(page["items"][0]["name"], "Gardening");
    let (s, g) = call(OWNER, Method::GET, "/admin/guilds/900", None).await;
    assert_eq!((s, g["member_count"].as_i64()), (StatusCode::OK, Some(5)));
    assert_eq!(
        call(OWNER, Method::GET, "/admin/guilds/12345", None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn setup_checklist_lists_integrations_and_the_mail_test_reaches_the_caller() {
    let e = env().await;
    let (s, v) = e
        .call(Some(OWNER), Method::GET, "/admin/system", None)
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["integrations"][0]["id"], "smtp");
    assert_eq!(v["integrations"][0]["configured"], true);

    // OPERATOR only: a moderator is refused and nothing is sent.
    let (s, _) = e
        .call(Some(MODER), Method::POST, "/admin/system/email-test", None)
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert!(e.mailer.sent().is_empty());

    let (s, v) = e
        .call(Some(OWNER), Method::POST, "/admin/system/email-test", None)
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["sent_to"], "u***@x.test");
    let sent = e.mailer.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].to, format!("u{OWNER}@x.test"));
    assert!(
        !v.to_string().contains(&format!("u{OWNER}@")),
        "the address is masked"
    );
}
