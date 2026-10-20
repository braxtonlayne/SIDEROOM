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
const CLERK: i64 = 13;
const MEMBER: i64 = 20;
const TARGET: i64 = 21;

struct Env {
    app: Router,
    admin: Arc<MemoryAdminRepo>,
    features: FeatureService,
    tokens: HashMap<i64, String>,
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
        (CLERK, "clerk"),
        (MEMBER, "member"),
        (TARGET, "target"),
    ] {
        admin.add_account(id, name, AccountState::Active, None, Some(OWNER));
    }
    admin.add_role(1, "Owner", 3, P::OWNER);
    admin.add_role(2, "Moderator", 2, P::STAFF_BASIC | P::MANAGE_CASES);
    admin.add_role(3, "Basic", 1, P::STAFF_BASIC);
    admin.add_role(4, "Takedown clerk", 2, P::STAFF_BASIC | P::MANAGE_TAKEDOWNS);
    admin.set_owner(OWNER);
    let svc = AdminService::new(admin.clone());
    let op = Caller::operator(SYSTEM_USER_ID);
    svc.appoint(&op, CASE_STAFF, &[2], Some("test"))
        .await
        .unwrap();
    svc.appoint(&op, BASIC, &[3], Some("test")).await.unwrap();
    svc.appoint(&op, CLERK, &[4], Some("test")).await.unwrap();

    let sessions = Arc::new(MemorySessions::new());
    let mut tokens = HashMap::new();
    for (n, uid) in [OWNER, CASE_STAFF, BASIC, CLERK, MEMBER, TARGET]
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
    let moderation = Moderation::new(
        pool,
        Arc::new(move || ids.fetch_add(1, std::sync::atomic::Ordering::Relaxed)),
    );
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
