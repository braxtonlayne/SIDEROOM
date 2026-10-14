//! Endpoint tests of the staff moderation routes. The access matrix runs without a database (a pool that
//! never connects); the full flow (queue, hide, restore, resolve, access log) is `#[ignore]` and needs
//! `SR_TEST_DATABASE_URL`:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-api cinema_social::moderation_tests -- --ignored --test-threads=1`

use super::{CinemaSocialDeps, router};
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
use sr_admin::{AccountState, AdminService, Caller, SYSTEM_USER_ID, memory::MemoryAdminRepo};
use sr_auth::{
    repo::{SessionRecord, SessionRepository},
    session::SessionToken,
};
use sr_cinema_social::{CinemaService, MemoryCatalog, TitleKind};
use sr_core::PlatformPermissions as P;
use std::{collections::HashMap, sync::Arc};
use tower::ServiceExt;

const OWNER: i64 = 10;
const MODERATOR: i64 = 11;
const BASIC: i64 = 12;
const MEMBER: i64 = 20;

struct Env {
    app: Router,
    admin: Arc<MemoryAdminRepo>,
    tokens: HashMap<i64, String>,
    social: CinemaService,
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
    for (id, name) in [(MODERATOR, "mod"), (BASIC, "basic"), (MEMBER, "member")] {
        admin.add_account(id, name, AccountState::Active, None, Some(OWNER));
    }
    admin.add_role(1, "Owner", 3, P::OWNER);
    admin.add_role(2, "Moderator", 2, P::STAFF_BASIC | P::DELETE_CONTENT);
    admin.add_role(3, "Basic", 1, P::STAFF_BASIC);
    admin.set_owner(OWNER);
    let svc = AdminService::new(admin.clone());
    let op = Caller::operator(SYSTEM_USER_ID);
    svc.appoint(&op, MODERATOR, &[2], Some("test"))
        .await
        .unwrap();
    svc.appoint(&op, BASIC, &[3], Some("test")).await.unwrap();

    let sessions = Arc::new(MemorySessions::new());
    let mut tokens = HashMap::new();
    for (n, uid) in [OWNER, MODERATOR, BASIC, MEMBER].into_iter().enumerate() {
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
    let catalog = MemoryCatalog::new();
    catalog.add_title(MemoryCatalog::title_with(1, TitleKind::Movie, "Akira", &[]));
    let ids = std::sync::atomic::AtomicI64::new(1000);
    let social = CinemaService::new(
        pool,
        Arc::new(catalog),
        Arc::new(move || ids.fetch_add(1, std::sync::atomic::Ordering::Relaxed)),
    );
    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let state = AppState::builder()
        .settings(settings)
        .sessions(sessions)
        .extension(Arc::new(AdminDeps::new(svc)))
        .extension(Arc::new(CinemaSocialDeps::new(social.clone())))
        .build();
    Env {
        app: build_router(state, router()),
        admin,
        tokens,
        social,
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

fn routes() -> Vec<(Method, &'static str, Option<Value>)> {
    vec![
        (Method::GET, "/admin/cinema/reports", None),
        (
            Method::POST,
            "/admin/cinema/reports/5/resolve",
            Some(json!({ "status": "dismissed" })),
        ),
        (
            Method::PUT,
            "/admin/cinema/reviews/5/hidden",
            Some(json!({ "hidden": true })),
        ),
        (
            Method::PUT,
            "/admin/cinema/comments/5/hidden",
            Some(json!({ "hidden": true })),
        ),
    ]
}

fn lazy() -> sqlx::PgPool {
    // A pool that never connects: these requests are answered before the first query.
    sqlx::PgPool::connect_lazy("postgres://nobody@127.0.0.1:1/none").unwrap()
}

#[tokio::test]
async fn moderation_needs_staff_with_delete_content() {
    let e = env(lazy()).await;
    for (m, p, b) in routes() {
        let (s, _) = e.call(None, m.clone(), p, b.clone()).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{m} {p}");
        let (s, body) = e.call(Some(MEMBER), m.clone(), p, b.clone()).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{m} {p}: {body}");
        let (s, body) = e.call(Some(BASIC), m.clone(), p, b.clone()).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{m} {p}: {body}");
        assert!(
            body["message"]
                .as_str()
                .unwrap_or_default()
                .contains("DELETE_CONTENT"),
            "{body}"
        );
    }
}

#[tokio::test]
async fn moderators_get_client_errors_for_bad_requests() {
    let e = env(lazy()).await;
    let (s, _) = e
        .call(
            Some(MODERATOR),
            Method::GET,
            "/admin/cinema/reports?status=bogus",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    for (m, p, b) in [
        (
            Method::PUT,
            "/admin/cinema/reviews/abc/hidden",
            Some(json!({ "hidden": true })),
        ),
        (
            Method::PUT,
            "/admin/cinema/comments/0/hidden",
            Some(json!({ "hidden": true })),
        ),
        (
            Method::POST,
            "/admin/cinema/reports/-1/resolve",
            Some(json!({ "status": "dismissed" })),
        ),
    ] {
        let (s, _) = e.call(Some(MODERATOR), m.clone(), p, b).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{m} {p}");
    }
    let (s, _) = e
        .call(
            Some(MODERATOR),
            Method::PUT,
            "/admin/cinema/reviews/5/hidden",
            Some(json!({ "hidden": "yes" })),
        )
        .await;
    assert!(s.is_client_error(), "{s}");
}

#[test]
fn openapi_lists_the_moderation_paths() {
    let doc = serde_json::to_value(super::openapi()).unwrap();
    let paths = doc["paths"].as_object().unwrap();
    for p in [
        "/admin/cinema/reports",
        "/admin/cinema/reports/{report_id}/resolve",
        "/admin/cinema/reviews/{review_id}/hidden",
        "/admin/cinema/comments/{comment_id}/hidden",
    ] {
        assert!(paths.contains_key(p), "{p}");
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_queue_hide_restore_and_resolve_flow_is_logged() {
    use sr_cinema_social::ContentKind;
    use sr_cinema_social::model::{CommentInput, ReportInput, Viewer};
    let db = sr_db::testing::TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    for id in [MEMBER, 21] {
        let name = format!("u{id}");
        let ok = sr_db::accounts::create_account(
            &pool,
            &sr_db::accounts::NewAccount {
                id,
                person_id: id + 1000,
                username: &name,
                display_name: None,
                email: &format!("{name}@example.test"),
                password_hash: "x",
                state: "active",
                email_verified: true,
                initial_credits: 0,
                flags: 0,
            },
            id + 2000,
        )
        .await
        .unwrap();
        assert!(ok);
    }
    let e = env(pool.clone()).await;
    let author = Viewer {
        user_id: MEMBER,
        adult: false,
    };
    let reporter = Viewer {
        user_id: 21,
        adult: false,
    };
    let c = e
        .social
        .post_comment(
            author,
            1,
            0,
            CommentInput {
                body: "a rude comment".into(),
                spoiler: None,
                parent_id: None,
            },
        )
        .await
        .unwrap();
    e.social
        .report(
            reporter,
            ContentKind::Comment,
            c.id,
            ReportInput {
                reason: "rude".into(),
            },
        )
        .await
        .unwrap();

    let (s, body) = e
        .call(Some(MODERATOR), Method::GET, "/admin/cinema/reports", None)
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    let item = &body["items"][0];
    assert_eq!(item["target_kind"], "comment");
    assert_eq!(item["status"], "open");
    assert_eq!(item["content"]["excerpt"], "a rude comment");
    assert_eq!(item["content"]["title_id"], "1");

    // Hide: 204, the open report becomes actioned, the access log has the entry.
    let (s, body) = e
        .call(
            Some(MODERATOR),
            Method::PUT,
            &format!("/admin/cinema/comments/{}/hidden", c.id),
            Some(json!({ "hidden": true, "reason": "abuse" })),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{body}");
    let (_, open) = e
        .call(Some(MODERATOR), Method::GET, "/admin/cinema/reports", None)
        .await;
    assert!(open["items"].as_array().unwrap().is_empty());
    let (_, done) = e
        .call(
            Some(MODERATOR),
            Method::GET,
            "/admin/cinema/reports?status=actioned",
            None,
        )
        .await;
    assert_eq!(done["items"][0]["content"]["hidden"], true);
    let log = e.admin.log_rows();
    let row = log
        .iter()
        .find(|r| r.action == "cinema.comment.hide")
        .expect("hide is logged");
    assert_eq!((row.actor_id, row.target_id), (MODERATOR, Some(c.id)));
    assert!(row.detail.contains("abuse"), "{}", row.detail);

    // Restore, then close the report as dismissed.
    let (s, _) = e
        .call(
            Some(MODERATOR),
            Method::PUT,
            &format!("/admin/cinema/comments/{}/hidden", c.id),
            Some(json!({ "hidden": false })),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert!(log_has(&e, "cinema.comment.unhide"));
    let rid = done["items"][0]["id"].as_str().unwrap().to_owned();
    let (s, _) = e
        .call(
            Some(MODERATOR),
            Method::POST,
            &format!("/admin/cinema/reports/{rid}/resolve"),
            Some(json!({ "status": "dismissed" })),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert!(log_has(&e, "cinema.report.resolve"));

    // Unknown targets are 404.
    let (s, _) = e
        .call(
            Some(MODERATOR),
            Method::PUT,
            "/admin/cinema/reviews/999999/hidden",
            Some(json!({ "hidden": true })),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = e
        .call(
            Some(MODERATOR),
            Method::POST,
            "/admin/cinema/reports/999999/resolve",
            Some(json!({ "status": "dismissed" })),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

fn log_has(e: &Env, action: &str) -> bool {
    e.admin.log_rows().iter().any(|r| r.action == action)
}
