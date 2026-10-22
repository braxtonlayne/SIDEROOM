//! Endpoint tests of the moderation routes. The access matrix and the flag run without a database (a pool that
//! never connects); the full flow (report, queue, case, hold, release, denial log) is `#[ignore]` and needs
//! `SR_TEST_DATABASE_URL`:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-api moderation::tests -- --ignored --test-threads=1`

use super::{ModerationDeps, router};
use crate::{
    ApiSettings, AppState,
    auth::{csrf, memory::MemorySessions},
    build_router,
    routes::admin::AdminDeps,
};
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sr_admin::{
    AccountState, AdminRepository, AdminService, Caller, SYSTEM_USER_ID, memory::MemoryAdminRepo,
};
use sr_auth::{
    repo::{SessionRecord, SessionRepository},
    session::SessionToken,
};
use sr_core::PlatformPermissions as P;
use sr_features::{
    FeatureRegistry, FeatureService, InMemorySettingsStore, ModerationReportsFeature,
};
use sr_moderation::Moderation;
use std::{collections::HashMap, sync::Arc};
use tower::ServiceExt;

const OWNER: i64 = 10;
const CASE_STAFF: i64 = 11;
const BASIC: i64 = 12;
const REVIEWER: i64 = 13;
const CLERK: i64 = 14;
const MEMBER: i64 = 20;
const TARGET: i64 = 21;

struct Env {
    app: Router,
    admin: Arc<MemoryAdminRepo>,
    features: FeatureService,
    tokens: HashMap<i64, String>,
    actions: Arc<Fake>,
}

async fn env(pool: sqlx::PgPool) -> Env {
    let admin = Arc::new(MemoryAdminRepo::default());
    admin.add_account(
        SYSTEM_USER_ID,
        "sideroom",
        AccountState::Deactivated,
        None,
        None,
    );
    admin.add_account(OWNER, "owner", AccountState::Active, None, None);
    for (id, name) in [
        (CASE_STAFF, "casestaff"),
        (BASIC, "basic"),
        (REVIEWER, "reviewer"),
        (CLERK, "clerk"),
        (MEMBER, "member"),
        (TARGET, "target"),
    ] {
        admin.add_account(id, name, AccountState::Active, None, Some(OWNER));
    }
    admin.add_role(1, "Owner", 3, P::OWNER);
    admin.add_role(2, "Moderator", 2, P::STAFF_BASIC | P::MANAGE_CASES);
    admin.add_role(3, "Basic", 1, P::STAFF_BASIC);
    admin.add_role(4, "Reader", 1, P::STAFF_BASIC | P::REVIEW_READING);
    admin.add_role(5, "Takedown clerk", 2, P::STAFF_BASIC | P::MANAGE_TAKEDOWNS);
    admin.set_owner(OWNER);
    let svc = AdminService::new(admin.clone());
    let op = Caller::operator(SYSTEM_USER_ID);
    svc.appoint(&op, CASE_STAFF, &[2], Some("test"))
        .await
        .unwrap();
    svc.appoint(&op, BASIC, &[3], Some("test")).await.unwrap();
    svc.appoint(&op, REVIEWER, &[4], Some("test"))
        .await
        .unwrap();
    svc.appoint(&op, CLERK, &[5], Some("test")).await.unwrap();

    let sessions = Arc::new(MemorySessions::new());
    let mut tokens = HashMap::new();
    for (n, uid) in [OWNER, CASE_STAFF, BASIC, REVIEWER, CLERK, MEMBER, TARGET]
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
    let mut registry = FeatureRegistry::new();
    registry.register(ModerationReportsFeature).unwrap();
    let features = FeatureService::new(
        Arc::new(registry),
        Arc::new(InMemorySettingsStore::new()),
        None,
    );
    let ids = std::sync::atomic::AtomicI64::new(5_000_000);
    let actions = Arc::new(Fake::default());
    let moderation = Moderation::new(
        pool,
        Arc::new(move || ids.fetch_add(1, std::sync::atomic::Ordering::Relaxed)),
    )
    .with_case_actions(actions.clone());
    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let state = AppState::builder()
        .settings(settings)
        .sessions(sessions)
        .extension(Arc::new(AdminDeps::new(svc)))
        .extension(Arc::new(
            ModerationDeps::new(moderation).with_features(features.clone()),
        ))
        .build();
    Env {
        app: build_router(state, router()),
        admin,
        features,
        tokens,
        actions,
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

fn lazy() -> sqlx::PgPool {
    // A pool that never connects: these requests are answered before the first query.
    sqlx::PgPool::connect_lazy("postgres://nobody@127.0.0.1:1/none").unwrap()
}

/// Staff-only routes: a member and a staff member without the bit are refused, and the refusal of the latter is
/// written to the access log (MOD-PERM-002).
#[tokio::test]
async fn staff_routes_refuse_members_and_staff_without_the_bit() {
    let e = env(lazy()).await;
    let routes: Vec<(Method, &str, Option<Value>)> = vec![
        (Method::GET, "/moderation/cases", None),
        (
            Method::POST,
            "/moderation/cases",
            Some(json!({"category": "spam", "target_kind": "other"})),
        ),
        (Method::GET, "/moderation/holds", None),
        (Method::GET, "/moderation/dashboard", None),
        (Method::GET, "/moderation/dashboard/inbox", None),
        (
            Method::POST,
            "/moderation/dashboard/inbox/dismiss",
            Some(json!({"case_ids": ["5"], "reason": "x"})),
        ),
        (
            Method::POST,
            "/moderation/cases/5/holds",
            Some(json!({"kind": "message", "id": "7", "reason": "x"})),
        ),
        (
            Method::POST,
            "/moderation/holds/5/release",
            Some(json!({"reason": "x"})),
        ),
        (Method::POST, "/moderation/holds/5/reviewed", None),
        (Method::GET, "/moderation/appeals", None),
        (Method::GET, "/moderation/takedowns", None),
        (Method::GET, "/moderation/takedowns/export", None),
        (Method::GET, "/moderation/takedowns/5", None),
        (
            Method::POST,
            "/moderation/takedowns",
            Some(
                json!({"kind": "copyright", "notifier_name": "n", "notifier_contact": "c", "legal_basis": "b", "original_text": "t"}),
            ),
        ),
        (
            Method::POST,
            "/moderation/takedowns/5/uphold",
            Some(json!({"reason": "x"})),
        ),
        (
            Method::POST,
            "/moderation/appeals/5/decision",
            Some(json!({"decision": "upheld", "reason": "x"})),
        ),
        (
            Method::POST,
            "/moderation/cases/5/warn",
            Some(json!({"text": "x"})),
        ),
        (
            Method::POST,
            "/moderation/cases/5/ban",
            Some(json!({"reason_category": "spam", "note": "x"})),
        ),
    ];
    for (m, p, b) in routes {
        let (s, _) = e.call(None, m.clone(), p, b.clone()).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{m} {p}");
        let (s, body) = e.call(Some(MEMBER), m.clone(), p, b.clone()).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{m} {p}: {body}");
        assert_eq!(body["code"], 900002, "{m} {p}: staff only");
    }
}

#[tokio::test]
async fn a_staff_member_without_manage_cases_is_refused_by_name_and_logged() {
    let e = env(lazy()).await;
    let (s, body) = e
        .call(Some(BASIC), Method::GET, "/moderation/cases", None)
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("MANAGE_CASES"),
        "{body}"
    );
    let log = e
        .admin
        .list_log(&sr_admin::AccessLogFilter {
            limit: 50,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        log.iter()
            .any(|r| r.action == "moderation.case.list" && r.actor_id == BASIC),
        "the denial is in the access log: {log:?}"
    );
}

#[tokio::test]
async fn reporting_is_off_until_the_flag_is_on_and_bad_bodies_are_client_errors() {
    let e = env(lazy()).await;
    let body = json!({"target_kind": "user", "target_id": TARGET.to_string(), "category": "spam"});
    let (s, b) = e
        .call(
            Some(MEMBER),
            Method::POST,
            "/moderation/reports",
            Some(body.clone()),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    assert_eq!(b["code"], 900001, "the feature is disabled");
    // Turn the flag on: the next failure is about the body, not the flag (no database is needed to see it).
    e.features
        .set_enabled(
            sr_features::REPORTS_FEATURE_ID,
            &sr_features::ScopeRef::Platform,
            Some(true),
        )
        .await
        .unwrap();
    let (s, _) = e
        .call(
            Some(MEMBER),
            Method::POST,
            "/moderation/reports",
            Some(json!({"target_kind": "user"})),
        )
        .await;
    assert!(s.is_client_error(), "{s}");
}

#[tokio::test]
async fn malformed_case_ids_are_unknown_cases() {
    let e = env(lazy()).await;
    let (s, b) = e
        .call(Some(MEMBER), Method::GET, "/moderation/cases/nope", None)
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "{b}");
}

// ---- the full flow -----------------------------------------------------------------------------------------

async fn pg_account(pool: &sqlx::PgPool, id: i64, name: &str) {
    let ok = sr_db::accounts::create_account(
        pool,
        &sr_db::accounts::NewAccount {
            id,
            person_id: id + 1_000,
            username: name,
            display_name: None,
            email: &format!("{name}@example.test"),
            password_hash: "x",
            state: "active",
            email_verified: true,
            initial_credits: 0,
            flags: 0,
        },
        id + 2_000,
    )
    .await
    .unwrap();
    assert!(ok);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn report_to_case_to_hold_to_release_over_http() {
    let db = sr_db::testing::TestDb::new().await.expect("test db");
    let pool = db.pool().clone();
    for (id, name) in [
        (OWNER, "owner"),
        (CASE_STAFF, "casestaff"),
        (BASIC, "basic"),
        (MEMBER, "member"),
        (TARGET, "target"),
    ] {
        pg_account(&pool, id, name).await;
    }
    let e = env(pool.clone()).await;
    e.features
        .set_enabled(
            sr_features::REPORTS_FEATURE_ID,
            &sr_features::ScopeRef::Platform,
            Some(true),
        )
        .await
        .unwrap();

    // A DM between the two, and a message the report points at.
    sqlx::query("INSERT INTO channels (id, type) VALUES (900, 1)")
        .execute(&pool)
        .await
        .unwrap();
    let _: String = sqlx::query_scalar("SELECT sr_ensure_message_partition(CURRENT_DATE)")
        .fetch_one(&pool)
        .await
        .unwrap();
    let mid: i64 =
        sqlx::query_scalar("SELECT sr_snowflake_floor(date_trunc('month', now())) + 1000")
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query(
        "INSERT INTO messages (id, channel_id, author_id, content) VALUES ($1, 900, $2, 'rude')",
    )
    .bind(mid)
    .bind(TARGET)
    .execute(&pool)
    .await
    .unwrap();

    // A user report from a member.
    let (s, b) = e
        .call(
            Some(MEMBER),
            Method::POST,
            "/moderation/reports",
            Some(
                json!({"target_kind": "user", "target_id": TARGET.to_string(),
                        "category": "harassment", "note": "again and again"}),
            ),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(b["state"], "received");
    let (s, b) = e
        .call(
            Some(MEMBER),
            Method::POST,
            "/moderation/reports",
            Some(
                json!({"target_kind": "user", "target_id": TARGET.to_string(), "category": "spam"}),
            ),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "one open report per target: {b}");
    let (s, mine) = e
        .call(Some(MEMBER), Method::GET, "/moderation/reports/mine", None)
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(mine[0]["state"], "received");
    assert!(
        mine[0].get("case_id").is_none(),
        "the reporter sees no case"
    );

    // Staff with the bit see it; staff without it, members and the reported person do not.
    let (s, page) = e
        .call(Some(CASE_STAFF), Method::GET, "/moderation/cases", None)
        .await;
    assert_eq!(s, StatusCode::OK, "{page}");
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    let case = page["items"][0]["id"].as_str().unwrap().to_owned();
    for who in [MEMBER, TARGET, BASIC] {
        let (s, _) = e
            .call(
                Some(who),
                Method::GET,
                &format!("/moderation/cases/{case}"),
                None,
            )
            .await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{who} cannot open the case");
    }
    let (s, detail) = e
        .call(
            Some(CASE_STAFF),
            Method::GET,
            &format!("/moderation/cases/{case}"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{detail}");
    assert_eq!(detail["reports"][0]["reporter"]["username"], "member");
    assert_eq!(detail["access"], "manage");

    // Work it: claim, note, state.
    let (s, _) = e
        .call(
            Some(CASE_STAFF),
            Method::PUT,
            &format!("/moderation/cases/{case}/assignee"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, n) = e
        .call(
            Some(CASE_STAFF),
            Method::POST,
            &format!("/moderation/cases/{case}/notes"),
            Some(json!({"body": "checking"})),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{n}");
    let (s, st) = e
        .call(
            Some(CASE_STAFF),
            Method::POST,
            &format!("/moderation/cases/{case}/state"),
            Some(json!({"to": "investigating"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{st}");
    assert_eq!(st["state"], "investigating");

    // Hold the message for the case; the same message again is a conflict; list; release needs a reason.
    let hold_body = json!({"kind": "message", "id": mid.to_string(), "reason": "evidence"});
    let (s, h) = e
        .call(
            Some(CASE_STAFF),
            Method::POST,
            &format!("/moderation/cases/{case}/holds"),
            Some(hold_body.clone()),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{h}");
    let hold = h["hold_id"].as_str().unwrap().to_owned();
    let (s, _) = e
        .call(
            Some(CASE_STAFF),
            Method::POST,
            &format!("/moderation/cases/{case}/holds"),
            Some(hold_body),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, list) = e
        .call(Some(CASE_STAFF), Method::GET, "/moderation/holds", None)
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);
    let (s, _) = e
        .call(
            Some(CASE_STAFF),
            Method::POST,
            &format!("/moderation/holds/{hold}/release"),
            Some(json!({})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "a reason is required");
    let (s, r) = e
        .call(
            Some(CASE_STAFF),
            Method::POST,
            &format!("/moderation/holds/{hold}/release"),
            Some(json!({"reason": "done"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");

    // Closing needs the outcome; the reporter then reads the line.
    let (s, _) = e
        .call(
            Some(CASE_STAFF),
            Method::POST,
            &format!("/moderation/cases/{case}/state"),
            Some(json!({"to": "dismissed", "outcome": "no breach"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, closed) = e
        .call(
            Some(CASE_STAFF),
            Method::POST,
            &format!("/moderation/cases/{case}/state"),
            Some(json!({"to": "closed", "outcome": "no breach",
                        "reporter_line": "Thank you, nothing broke the rules."})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{closed}");
    let (_, mine) = e
        .call(Some(MEMBER), Method::GET, "/moderation/reports/mine", None)
        .await;
    assert_eq!(mine[0]["state"], "closed");
    assert_eq!(
        mine[0]["outcome_line"],
        "Thank you, nothing broke the rules."
    );
}

/// N-23: a reviewer (REVIEW_READING) gets the queue of the cases routed to reviewers, and nothing of the rest.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_reviewer_sees_only_the_cases_routed_to_reviewers() {
    let db = sr_db::testing::TestDb::new().await.expect("test db");
    let pool = db.pool().clone();
    for (id, name) in [
        (OWNER, "owner"),
        (CASE_STAFF, "casestaff"),
        (BASIC, "basic"),
        (REVIEWER, "reviewer"),
        (MEMBER, "member"),
        (TARGET, "target"),
    ] {
        pg_account(&pool, id, name).await;
    }
    let e = env(pool.clone()).await;
    e.features
        .set_enabled(
            sr_features::REPORTS_FEATURE_ID,
            &sr_features::ScopeRef::Platform,
            Some(true),
        )
        .await
        .unwrap();
    sqlx::query("INSERT INTO reading_series (id, kind, title) VALUES (600, 'comic', 'Comic')")
        .execute(&pool)
        .await
        .unwrap();
    let report =
        |kind: &str, id: &str| json!({"target_kind": kind, "target_id": id, "category": "spam"});
    for body in [
        report("library", "600"),
        report("user", &TARGET.to_string()),
    ] {
        let (s, b) = e
            .call(
                Some(MEMBER),
                Method::POST,
                "/moderation/reports",
                Some(body),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
    }
    let (s, page) = e
        .call(Some(REVIEWER), Method::GET, "/moderation/cases", None)
        .await;
    assert_eq!(s, StatusCode::OK, "{page}");
    let items = page["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{page}");
    assert_eq!(items[0]["routing"], "reviewers");
    let id = items[0]["id"].as_str().unwrap();
    // Work, but not the manage actions.
    let (s, d) = e
        .call(
            Some(REVIEWER),
            Method::GET,
            &format!("/moderation/cases/{id}"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{d}");
    assert_eq!(d["access"], "work");
    let (s, _) = e
        .call(
            Some(REVIEWER),
            Method::POST,
            &format!("/moderation/cases/{id}/warn"),
            Some(json!({"text": "x"})),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // The platform case (a user report) stays out of the reviewer's sight.
    let (_, all) = e
        .call(Some(CASE_STAFF), Method::GET, "/moderation/cases", None)
        .await;
    assert_eq!(all["items"].as_array().unwrap().len(), 2);
}

/// Records what the services were asked to do.
#[derive(Default)]
struct Fake(std::sync::Mutex<Vec<String>>);

impl Fake {
    fn note(&self, s: String) -> Result<String, sr_moderation::ActionError> {
        self.0.lock().unwrap().push(s.clone());
        Ok(s)
    }
}

#[async_trait::async_trait]
impl sr_moderation::CaseActions for Fake {
    async fn delete_message(
        &self,
        _: &Caller,
        case: i64,
        _: i64,
        m: i64,
    ) -> Result<String, sr_moderation::ActionError> {
        self.note(format!("delete {case} {m}"))
    }
    async fn restore_message(
        &self,
        by: &Caller,
        case: i64,
        ch: i64,
        m: i64,
    ) -> Result<String, sr_moderation::ActionError> {
        self.note(format!("restore {} {case} {ch} {m}", by.user_id))
    }
    async fn remove_attachment(
        &self,
        by: &Caller,
        case: i64,
        ch: i64,
        m: i64,
        a: i64,
    ) -> Result<String, sr_moderation::ActionError> {
        self.note(format!(
            "remove_attachment {} {case} {ch} {m} {a}",
            by.user_id
        ))
    }
    async fn timeout_member(
        &self,
        _: &Caller,
        case: i64,
        g: i64,
        u: i64,
        _: i64,
    ) -> Result<String, sr_moderation::ActionError> {
        self.note(format!("timeout {case} {g} {u}"))
    }
    async fn kick_member(
        &self,
        _: &Caller,
        case: i64,
        g: i64,
        u: i64,
    ) -> Result<String, sr_moderation::ActionError> {
        self.note(format!("kick {case} {g} {u}"))
    }
    async fn guild_ban(
        &self,
        _: &Caller,
        case: i64,
        g: i64,
        u: i64,
        _: &str,
    ) -> Result<String, sr_moderation::ActionError> {
        self.note(format!("ban {case} {g} {u}"))
    }
    async fn suspend_account(
        &self,
        _: &Caller,
        case: i64,
        a: i64,
        _: &str,
    ) -> Result<String, sr_moderation::ActionError> {
        self.note(format!("suspend {case} {a}"))
    }
}

/// N-21: each kind of action over HTTP reaches the service with the case's own target and lands on the timeline.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn actions_from_a_case_reach_the_services() {
    let db = sr_db::testing::TestDb::new().await.expect("test db");
    let pool = db.pool().clone();
    for (id, name) in [
        (OWNER, "owner"),
        (CASE_STAFF, "casestaff"),
        (BASIC, "basic"),
        (REVIEWER, "reviewer"),
        (MEMBER, "member"),
        (TARGET, "target"),
    ] {
        pg_account(&pool, id, name).await;
    }
    let e = env(pool).await;
    let (st, case) = e
        .call(
            Some(CASE_STAFF),
            Method::POST,
            "/moderation/cases",
            Some(json!({"category": "harassment", "target_kind": "user",
                        "target_id": TARGET.to_string(), "subject_account_id": TARGET.to_string(),
                        "guild_id": "77"})),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{case}");
    let id = case["id"].as_str().unwrap().to_owned();
    let act = |action: &str, extra: Value| {
        let mut b = json!({"action": action, "perform": true, "note": "because"});
        if let (Some(m), Some(x)) = (b.as_object_mut(), extra.as_object()) {
            m.extend(x.clone());
        }
        b
    };
    let path = format!("/moderation/cases/{id}/actions");
    for (action, extra) in [
        ("timeout", json!({"timeout_minutes": 10})),
        ("kick", json!({})),
        ("guild_ban", json!({})),
        ("suspend_account", json!({})),
    ] {
        let (s, b) = e
            .call(
                Some(CASE_STAFF),
                Method::POST,
                &path,
                Some(act(action, extra)),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{action}: {b}");
    }
    let t = TARGET;
    let calls = e.actions.0.lock().unwrap().clone();
    assert_eq!(
        calls,
        vec![
            format!("timeout {id} 77 {t}"),
            format!("kick {id} 77 {t}"),
            format!("ban {id} 77 {t}"),
            format!("suspend {id} {t}"),
        ]
    );
    // A bad request, an unsupported action and a person with no standing on the case.
    for (who, body, want) in [
        (
            CASE_STAFF,
            act("explode", json!({})),
            StatusCode::BAD_REQUEST,
        ),
        (
            CASE_STAFF,
            act("timeout", json!({})),
            StatusCode::BAD_REQUEST,
        ),
        (
            CASE_STAFF,
            act("restore_message", json!({})),
            StatusCode::CONFLICT,
        ),
        (
            CASE_STAFF,
            act("delete_message", json!({})),
            StatusCode::CONFLICT,
        ),
        (MEMBER, act("kick", json!({})), StatusCode::NOT_FOUND),
        (BASIC, act("kick", json!({})), StatusCode::NOT_FOUND),
    ] {
        let (s, b) = e.call(Some(who), Method::POST, &path, Some(body)).await;
        assert_eq!(s, want, "{who}: {b}");
    }
    let (_, d) = e
        .call(
            Some(CASE_STAFF),
            Method::GET,
            &format!("/moderation/cases/{id}"),
            None,
        )
        .await;
    let kinds: Vec<&str> = d["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds.iter().filter(|k| **k == "action_performed").count(),
        4,
        "{kinds:?}"
    );
    assert_eq!(d["case"]["state"], "actioned");
}

/// N-21: the message actions (delete, restore, remove one attachment) over HTTP reach the service with the case's
/// own message and channel, an attachment is required for the removal, and the case lands on `actioned`.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn message_actions_from_a_case_reach_the_services() {
    let db = sr_db::testing::TestDb::new().await.expect("test db");
    let pool = db.pool().clone();
    for (id, name) in [
        (OWNER, "owner"),
        (CASE_STAFF, "casestaff"),
        (BASIC, "basic"),
        (MEMBER, "member"),
        (TARGET, "target"),
    ] {
        pg_account(&pool, id, name).await;
    }
    let e = env(pool.clone()).await;
    let (st, case) = e
        .call(
            Some(CASE_STAFF),
            Method::POST,
            "/moderation/cases",
            Some(
                json!({"category": "spam", "target_kind": "message", "target_id": "555",
                        "subject_account_id": TARGET.to_string(), "guild_id": "77"}),
            ),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{case}");
    let id = case["id"].as_str().unwrap().to_owned();
    // Reports fill the channel in; a case opened by hand does not.
    sqlx::query("UPDATE moderation_cases SET target_channel_id = 444 WHERE id = $1")
        .bind(id.parse::<i64>().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let path = format!("/moderation/cases/{id}/actions");
    let act = |action: &str, extra: Value| {
        let mut b = json!({"action": action, "perform": true, "note": "because"});
        if let (Some(m), Some(x)) = (b.as_object_mut(), extra.as_object()) {
            m.extend(x.clone());
        }
        b
    };
    // Removing an attachment needs one, and a bad id is not found.
    for (body, want) in [
        (act("remove_attachment", json!({})), StatusCode::BAD_REQUEST),
        (
            act("remove_attachment", json!({"attachment_id": "x"})),
            StatusCode::NOT_FOUND,
        ),
    ] {
        let (s, b) = e
            .call(Some(CASE_STAFF), Method::POST, &path, Some(body))
            .await;
        assert_eq!(s, want, "{b}");
    }
    assert!(e.actions.0.lock().unwrap().is_empty());
    for body in [
        act("delete_message", json!({})),
        act("restore_message", json!({})),
        act("remove_attachment", json!({"attachment_id": "9001"})),
    ] {
        let (s, b) = e
            .call(Some(CASE_STAFF), Method::POST, &path, Some(body))
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
    }
    let staff = CASE_STAFF;
    assert_eq!(
        e.actions.0.lock().unwrap().clone(),
        vec![
            format!("delete {id} 555"),
            format!("restore {staff} {id} 444 555"),
            format!("remove_attachment {staff} {id} 444 555 9001"),
        ]
    );
    // A person with no standing on the case sees nothing of it.
    let (s, _) = e
        .call(
            Some(MEMBER),
            Method::POST,
            &path,
            Some(act("restore_message", json!({}))),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (_, d) = e
        .call(
            Some(CASE_STAFF),
            Method::GET,
            &format!("/moderation/cases/{id}"),
            None,
        )
        .await;
    assert_eq!(d["case"]["state"], "actioned");
}

// ---- the takedown register -----------------------------------------------------------------------------------

/// MANAGE_CASES is not MANAGE_TAKEDOWNS (D-210): the case moderator is refused by name, and it is logged.
#[tokio::test]
async fn the_register_needs_manage_takedowns_not_manage_cases() {
    let e = env(lazy()).await;
    let (s, body) = e
        .call(Some(CASE_STAFF), Method::GET, "/moderation/takedowns", None)
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("MANAGE_TAKEDOWNS"),
        "{body}"
    );
    let log = e
        .admin
        .list_log(&sr_admin::AccessLogFilter {
            limit: 50,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        log.iter()
            .any(|r| r.action == "moderation.takedown.list" && r.actor_id == CASE_STAFF),
        "the denial is in the access log: {log:?}"
    );
}

#[tokio::test]
async fn takedown_inputs_that_name_nothing_are_client_errors() {
    let e = env(lazy()).await;
    let (s, _) = e
        .call(
            Some(CLERK),
            Method::GET,
            "/moderation/takedowns?state=nonsense",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = e
        .call(
            Some(CLERK),
            Method::GET,
            "/moderation/takedowns/export?format=xml",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = e
        .call(
            Some(MEMBER),
            Method::GET,
            "/moderation/item-status?kind=message&id=5",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "only library and cinema items");
    let (s, _) = e
        .call(Some(CLERK), Method::GET, "/moderation/takedowns/nope", None)
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // Signed out: the item question needs a session.
    let (s, _) = e
        .call(
            None,
            Method::GET,
            "/moderation/item-status?kind=library_series&id=5",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn takedown_notice_to_counter_notice_to_export_over_http() {
    let db = sr_db::testing::TestDb::new().await.expect("test db");
    let pool = db.pool().clone();
    for (id, name) in [
        (OWNER, "owner"),
        (CASE_STAFF, "casestaff"),
        (BASIC, "basic"),
        (CLERK, "clerk"),
        (MEMBER, "member"),
        (TARGET, "target"),
    ] {
        pg_account(&pool, id, name).await;
    }
    let e = env(pool.clone()).await;
    // A published series made by TARGET.
    sqlx::query(
        "INSERT INTO reading_series (id, kind, title, created_by, published, published_at) \
         VALUES (700, 'manga', 'Borrowed Saga', $1, TRUE, now())",
    )
    .bind(TARGET)
    .execute(&pool)
    .await
    .unwrap();

    let (s, n) = e
        .call(
            Some(CLERK),
            Method::POST,
            "/moderation/takedowns",
            Some(json!({"kind": "copyright", "notifier_name": "Rights Ltd",
                        "notifier_contact": "legal@rights.example",
                        "legal_basis": "Owner of Borrowed Saga",
                        "locations": ["https://x.example/library/700"],
                        "original_text": "Please remove."})),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{n}");
    let id = n["id"].as_str().unwrap().to_owned();
    assert_eq!(n["state"], "received");
    let base = format!("/moderation/takedowns/{id}");

    // Uphold before triage is a conflict; then name the content and uphold.
    let (s, _) = e
        .call(
            Some(CLERK),
            Method::POST,
            &format!("{base}/uphold"),
            Some(json!({"reason": "valid"})),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, d) = e
        .call(
            Some(CLERK),
            Method::POST,
            &format!("{base}/items"),
            Some(json!({"kind": "library_series", "id": "700"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{d}");
    assert_eq!(d["state"], "triage");
    assert_eq!(d["items"][0]["label"], "Series Borrowed Saga");
    let (s, up) = e
        .call(
            Some(CLERK),
            Method::POST,
            &format!("{base}/uphold"),
            Some(json!({"reason": "valid"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{up}");
    assert_eq!(up["items_disabled"], 1);

    // Any signed-in person can ask whether the item is down; the answer is only the state.
    let (s, st) = e
        .call(
            Some(MEMBER),
            Method::GET,
            "/moderation/item-status?kind=library_series&id=700",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(st["state"], "taken_down");

    let (s, c) = e
        .call(
            Some(CLERK),
            Method::POST,
            &format!("{base}/counter-notice"),
            Some(json!({"text": "It is mine"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{c}");
    assert_eq!(c["state"], "counter_noticed");
    assert!(c["restore_after_ms"].as_i64().unwrap() > c["counter_noticed_at_ms"].as_i64().unwrap());
    let (s, c) = e
        .call(
            Some(CLERK),
            Method::POST,
            &format!("{base}/court-filing"),
            Some(json!({"noted": true, "reason": "filed on the 3rd"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{c}");
    assert_eq!(c["court_filing_noted"], true);

    // The register lists it and exports CSV and JSON.
    let (s, list) = e
        .call(Some(CLERK), Method::GET, "/moderation/takedowns", None)
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);
    let (s, json_rows) = e
        .call(
            Some(CLERK),
            Method::GET,
            "/moderation/takedowns/export?format=json",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(json_rows[0]["notifier_name"], "Rights Ltd");
    let raw = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/moderation/takedowns/export")
        .header("cookie", format!("sr_session={}", e.tokens[&CLERK]))
        .body(Body::empty())
        .unwrap();
    let resp = e.app.clone().oneshot(raw).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        resp.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/csv")
    );
    let csv = String::from_utf8(
        resp.into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(csv.starts_with("reference,kind,state,"));
    assert!(csv.contains("Rights Ltd"));

    // Restoring by hand ends it.
    let (s, r) = e
        .call(
            Some(CLERK),
            Method::POST,
            &format!("{base}/restore"),
            Some(json!({"reason": "court dismissed"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["state"], "restored");
    let (_, st) = e
        .call(
            Some(MEMBER),
            Method::GET,
            "/moderation/item-status?kind=library_series&id=700",
            None,
        )
        .await;
    assert_eq!(st["state"], "none");
}

// ---- the dashboards ----------------------------------------------------------------------------------------

/// A staff member with none of the dashboard bits is refused by name, and the denial is logged; the inbox and its
/// bulk dismissal are `MANAGE_CASES` only, so the takedown clerk is refused there too.
#[tokio::test]
async fn the_dashboard_needs_one_of_its_bits_and_the_inbox_needs_manage_cases() {
    let e = env(lazy()).await;
    let (s, body) = e
        .call(Some(BASIC), Method::GET, "/moderation/dashboard", None)
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("MANAGE_CASES"),
        "{body}"
    );
    for (m, p, b) in [
        (Method::GET, "/moderation/dashboard/inbox", None),
        (
            Method::POST,
            "/moderation/dashboard/inbox/dismiss",
            Some(json!({"case_ids": ["5"], "reason": "x"})),
        ),
    ] {
        let (s, body) = e.call(Some(CLERK), m.clone(), p, b).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{m} {p}: {body}");
        assert!(
            body["message"]
                .as_str()
                .unwrap_or_default()
                .contains("MANAGE_CASES"),
            "{body}"
        );
    }
    let log = e
        .admin
        .list_log(&sr_admin::AccessLogFilter {
            limit: 50,
            ..Default::default()
        })
        .await
        .unwrap();
    for (action, actor) in [
        ("moderation.dashboard", BASIC),
        ("moderation.inbox.list", CLERK),
        ("moderation.inbox.dismiss", CLERK),
    ] {
        assert!(
            log.iter()
                .any(|r| r.action == action && r.actor_id == actor),
            "{action} denial is in the access log: {log:?}"
        );
    }
}

#[tokio::test]
async fn bulk_dismiss_inputs_that_name_nothing_are_client_errors() {
    let e = env(lazy()).await;
    let (s, b) = e
        .call(
            Some(CASE_STAFF),
            Method::POST,
            "/moderation/dashboard/inbox/dismiss",
            Some(json!({"case_ids": ["nope"], "reason": "x"})),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "{b}");
    let (s, b) = e
        .call(
            Some(CASE_STAFF),
            Method::GET,
            "/moderation/dashboard/inbox?cursor=abc",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");
}

/// Each staff member gets the sections their bits read, over HTTP, and a bulk dismissal reaches the case.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_dashboard_sections_follow_the_bits_over_http() {
    let db = sr_db::testing::TestDb::new().await.expect("test db");
    let pool = db.pool().clone();
    for (id, name) in [
        (OWNER, "owner"),
        (CASE_STAFF, "casestaff"),
        (CLERK, "clerk"),
        (MEMBER, "member"),
    ] {
        pg_account(&pool, id, name).await;
    }
    let e = env(pool.clone()).await;

    let (s, b) = e
        .call(
            Some(CASE_STAFF),
            Method::GET,
            "/moderation/dashboard?days=7",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["window_days"], 7);
    assert_eq!(b["health"]["reports_per_day"].as_array().unwrap().len(), 7);
    for k in ["inbox", "health", "retention", "age"] {
        assert!(b[k].is_object(), "{k}: {b}");
    }
    for k in ["bans", "takedowns", "data_requests"] {
        assert!(b[k].is_null(), "{k} is the clerk's or the banner's: {b}");
    }

    let (s, b) = e
        .call(Some(CLERK), Method::GET, "/moderation/dashboard", None)
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert!(
        b["takedowns"].is_object() && b["data_requests"].is_object(),
        "{b}"
    );
    assert!(b["health"].is_null() && b["inbox"].is_null(), "{b}");

    // A case staff opened without a report is not in the inbox (it has no report); a reported one is.
    sqlx::query(
        "INSERT INTO moderation_cases (id, category, severity, routing, summary, target_kind, opened_by_kind)          VALUES (7001, 'child_safety', 'serious', 'platform', 'x', 'other', 'staff')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO moderation_reports (id, case_id, reporter_id, target_kind, target_id, category)          VALUES (7002, 7001, $1, 'other', 9, 'child_safety')",
    )
    .bind(MEMBER)
    .execute(&pool)
    .await
    .unwrap();
    let (s, b) = e
        .call(
            Some(CASE_STAFF),
            Method::GET,
            "/moderation/dashboard/inbox",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["items"].as_array().unwrap().len(), 1, "{b}");
    assert_eq!(b["items"][0]["case_id"], "7001");
    assert_eq!(b["items"][0]["reports"], 1);
    assert!(
        !b.to_string().contains(&MEMBER.to_string()),
        "no reporter in the inbox: {b}"
    );

    let (s, b) = e
        .call(
            Some(CASE_STAFF),
            Method::POST,
            "/moderation/dashboard/inbox/dismiss",
            Some(json!({"case_ids": ["7001", "7777"]})),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "a reason is required: {b}");
    let (s, b) = e
        .call(
            Some(CASE_STAFF),
            Method::POST,
            "/moderation/dashboard/inbox/dismiss",
            Some(json!({"case_ids": ["7001", "7777"], "reason": "Not a violation"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["dismissed"], json!(["7001"]));
    assert_eq!(b["skipped"][0]["case_id"], "7777");
    let state: String = sqlx::query_scalar("SELECT state FROM moderation_cases WHERE id = 7001")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(state, "dismissed");
}
