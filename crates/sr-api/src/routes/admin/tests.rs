//! Endpoint tests over the in-memory admin repository: 401/403 matrices for every
//! route plus the main flows, driven through the real router with tower `oneshot`.

use super::{AdminDeps, router};
use crate::{
    ApiSettings, AppState,
    auth::{
        BotIdentity, csrf,
        memory::{MemoryBots, MemorySessions},
    },
    build_router,
};
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sr_admin::{AccountState, AdminService, Caller, SYSTEM_USER_ID, memory::MemoryAdminRepo};
use sr_auth::{
    repo::{SessionRecord, SessionRepository},
    session::SessionToken,
};
use sr_core::PlatformPermissions as P;
use sr_features::{
    Feature, FeatureMeta, FeatureRegistry, FeatureService, Field, FieldKind, InMemorySettingsStore,
    Scope, SettingsSchema,
};
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

struct Env {
    app: Router,
    repo: Arc<MemoryAdminRepo>,
    tokens: HashMap<i64, String>,
    bot: &'static str,
}

async fn env() -> Env {
    let repo = Arc::new(MemoryAdminRepo::default());
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
    let svc = AdminService::new(repo.clone());
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
        .with_features(features)
        .with_gateway_sessions(Arc::new(|| 7));

    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let state = AppState::builder()
        .settings(settings)
        .sessions(sessions)
        .bots(bots)
        .extension(Arc::new(deps))
        .build();
    Env {
        app: build_router(state, router()),
        repo,
        tokens,
        bot: "bot-owner",
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
