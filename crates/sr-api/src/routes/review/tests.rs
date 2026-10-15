//! Endpoint tests of the review router over the in-memory repository and a stub kind:
//! authentication and staff matrices, permission mapping, the JSON shape, the timeline
//! visibility rules, the access log trail and `SR_SUBMISSION_UPDATE`. The Postgres behaviour
//! and the Library kinds are covered by `sr-db/tests/review_repo.rs`.

use super::{ReviewDeps, router};
use crate::{
    ApiSettings, AppState,
    auth::{
        BotIdentity, csrf,
        memory::{MemoryBots, MemorySessions},
    },
    build_router,
    routes::admin::AdminDeps,
};
use async_trait::async_trait;
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
use sr_core::{PlatformPermissions as P, SystemClock};
use sr_review::memory::MemoryReviewRepo;
use sr_review::model::{Submission, SubmissionItem};
use sr_review::repo::NewItem;
use sr_review::{
    KindDraft, KindRegistry, ReviewError, ReviewEvents, ReviewResult, ReviewService, SubmissionKind,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tower::ServiceExt;

const OWNER: i64 = 10;
const REVIEWER: i64 = 11;
const BASIC: i64 = 12;
const MEMBER: i64 = 20;
const OTHER: i64 = 21;
const R_OWNER: i64 = 1;
const R_REVIEWER: i64 = 2;
const R_BASIC: i64 = 3;

/// Stub `library.series`: one fixed item, ready unless the title is "broken".
struct StubKind;

#[async_trait]
impl SubmissionKind for StubKind {
    fn id(&self) -> &'static str {
        "library.series"
    }
    fn label(&self) -> &'static str {
        "Library series"
    }
    fn review_permission(&self) -> P {
        P::REVIEW_READING
    }
    fn publish_permission(&self) -> P {
        P::PUBLISH_READING
    }
    async fn create(&self, _: i64, payload: &Value) -> ReviewResult<KindDraft> {
        let title = payload["title"].as_str().unwrap_or("").to_owned();
        if title.is_empty() {
            return Err(ReviewError::Invalid("title is required".into()));
        }
        Ok(KindDraft {
            title,
            ref_id: Some(7),
            payload: json!({ "series_id": "7" }),
        })
    }
    async fn items(&self, _: &Submission, _: &[SubmissionItem]) -> ReviewResult<Vec<NewItem>> {
        Ok(vec![NewItem {
            ref_id: 1,
            label: "Chapter 1".into(),
            position: 0,
        }])
    }
    async fn validate(&self, s: &Submission, _: &[NewItem]) -> ReviewResult<Vec<String>> {
        Ok(if s.title == "broken" {
            vec!["Chapter 1 has no pages yet".into()]
        } else {
            vec![]
        })
    }
    async fn preview(&self, s: &Submission) -> ReviewResult<Value> {
        Ok(json!({ "title": s.title }))
    }
    async fn publish(&self, _: &Submission, _: &[SubmissionItem]) -> ReviewResult<()> {
        Ok(())
    }
}

#[derive(Default)]
struct Sink(Mutex<Vec<(Vec<i64>, Value)>>);

#[async_trait]
impl ReviewEvents for Sink {
    async fn emit(&self, users: Vec<i64>, payload: Value) {
        self.0.lock().unwrap().push((users, payload));
    }
}

struct Env {
    app: Router,
    admin: Arc<MemoryAdminRepo>,
    tokens: HashMap<i64, String>,
    sink: Arc<Sink>,
}

async fn env() -> Env {
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
        (REVIEWER, "reviewer"),
        (BASIC, "basic"),
        (MEMBER, "member"),
        (OTHER, "other"),
    ] {
        admin.add_account(id, name, AccountState::Active, None, Some(OWNER));
    }
    admin.add_role(R_OWNER, "Owner", 3, P::OWNER);
    admin.add_role(
        R_REVIEWER,
        "Library Curator",
        2,
        P::STAFF_BASIC | P::REVIEW_READING,
    );
    admin.add_role(R_BASIC, "Basic", 1, P::STAFF_BASIC);
    admin.set_owner(OWNER);
    let svc = AdminService::new(admin.clone());
    let op = Caller::operator(SYSTEM_USER_ID);
    svc.appoint(&op, REVIEWER, &[R_REVIEWER], Some("test"))
        .await
        .unwrap();
    svc.appoint(&op, BASIC, &[R_BASIC], Some("test"))
        .await
        .unwrap();

    let sessions = Arc::new(MemorySessions::new());
    let mut tokens = HashMap::new();
    for (n, uid) in [OWNER, REVIEWER, BASIC, MEMBER, OTHER]
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
        "bot",
        BotIdentity {
            user_id: MEMBER,
            application_id: 1,
        },
    );

    let sink = Arc::new(Sink::default());
    let next = Arc::new(std::sync::atomic::AtomicI64::new(5000));
    let review = ReviewService::new(
        Arc::new(MemoryReviewRepo::new()),
        KindRegistry::new().with(Arc::new(StubKind)),
        Arc::new(SystemClock),
        Arc::new(move || next.fetch_add(1, std::sync::atomic::Ordering::SeqCst)),
    )
    .with_events(sink.clone());

    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let state = AppState::builder()
        .settings(settings)
        .sessions(sessions)
        .bots(bots)
        .extension(Arc::new(AdminDeps::new(svc)))
        .extension(Arc::new(ReviewDeps::new(review)))
        .build();
    Env {
        app: build_router(state, router()),
        admin,
        tokens,
        sink,
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

    async fn create(&self, who: i64, title: &str) -> String {
        let (st, b) = self
            .call(
                Some(who),
                Method::POST,
                "/submissions",
                Some(json!({ "kind": "library.series", "payload": { "title": title } })),
            )
            .await;
        assert_eq!(st, StatusCode::CREATED, "{b}");
        b["submission"]["id"].as_str().unwrap().to_owned()
    }

    fn actions(&self) -> Vec<String> {
        self.admin
            .log_rows()
            .iter()
            .map(|r| r.action.clone())
            .collect()
    }
}

fn member_routes() -> Vec<(Method, &'static str, Option<Value>)> {
    vec![
        (Method::GET, "/submissions/kinds", None),
        (Method::GET, "/submissions/quota", None),
        (
            Method::POST,
            "/submissions/naming/parse",
            Some(json!({ "section": "cinema", "names": ["a.mkv"] })),
        ),
        (Method::GET, "/submissions/7/credits", None),
        (Method::GET, "/submissions", None),
        (
            Method::POST,
            "/submissions",
            Some(json!({ "kind": "library.series", "payload": { "title": "x" } })),
        ),
        (Method::GET, "/submissions/7", None),
        (
            Method::PATCH,
            "/submissions/7",
            Some(json!({ "title": "y" })),
        ),
        (Method::GET, "/submissions/7/preview", None),
        (Method::POST, "/submissions/7/submit", None),
        (Method::POST, "/submissions/7/withdraw", None),
        (
            Method::POST,
            "/submissions/7/comments",
            Some(json!({ "body": "hi" })),
        ),
        (Method::GET, "/submissions/7/library/chapters", None),
        (Method::GET, "/requests", None),
        (
            Method::POST,
            "/requests",
            Some(json!({ "section": "library", "title": "x" })),
        ),
        (Method::PUT, "/requests/7/vote", None),
        (Method::DELETE, "/requests/7/vote", None),
        (
            Method::PUT,
            "/requests/7/claim",
            Some(json!({ "note": "mine" })),
        ),
        (Method::DELETE, "/requests/7/claim", None),
        (
            Method::POST,
            "/requests/7/fulfil",
            Some(json!({ "submission_id": "7" })),
        ),
    ]
}

fn reviewer_routes() -> Vec<(Method, &'static str, Option<Value>)> {
    vec![
        (Method::GET, "/admin/review/queue", None),
        (Method::GET, "/admin/review/submissions/7", None),
        (Method::GET, "/admin/review/submissions/7/preview", None),
        (Method::GET, "/admin/review/submissions/7/credits", None),
        (
            Method::POST,
            "/admin/review/submissions/7/revoke-credits",
            Some(json!({ "reason": "x" })),
        ),
        (Method::POST, "/admin/review/submissions/7/claim", None),
        (Method::POST, "/admin/review/submissions/7/release", None),
        (
            Method::POST,
            "/admin/review/submissions/7/comments",
            Some(json!({ "body": "hi" })),
        ),
        (
            Method::PUT,
            "/admin/review/submissions/7/items/1",
            Some(json!({ "state": "approved" })),
        ),
        (
            Method::POST,
            "/admin/review/submissions/7/request-changes",
            Some(json!({ "reason": "x" })),
        ),
        (
            Method::POST,
            "/admin/review/submissions/7/reject",
            Some(json!({ "reason": "x" })),
        ),
        (Method::POST, "/admin/review/submissions/7/approve", None),
        (Method::POST, "/admin/review/submissions/7/publish", None),
        (
            Method::POST,
            "/admin/review/submissions/7/publish-now",
            None,
        ),
        (Method::POST, "/admin/review/requests/7/decline", None),
    ]
}

#[tokio::test]
async fn every_route_needs_a_session() {
    let e = env().await;
    for (m, p, b) in member_routes().into_iter().chain(reviewer_routes()) {
        let (st, _) = e.call(None, m.clone(), p, b).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED, "{m} {p}");
    }
}

#[tokio::test]
async fn bots_cannot_submit_or_review() {
    let e = env().await;
    for p in ["/submissions", "/admin/review/queue"] {
        let resp = e
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1{p}"))
                    .header("authorization", "Bot bot")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(
            resp.status() == StatusCode::FORBIDDEN || resp.status() == StatusCode::UNAUTHORIZED,
            "{p}: {}",
            resp.status()
        );
    }
}

#[tokio::test]
async fn reviewer_routes_are_staff_only_and_need_the_kind_permission() {
    let e = env().await;
    let id = e.create(MEMBER, "Series").await;
    for (m, p, b) in reviewer_routes() {
        let (st, _) = e.call(Some(MEMBER), m.clone(), p, b).await;
        assert_eq!(st, StatusCode::FORBIDDEN, "{m} {p}");
    }
    // Staff without REVIEW_READING: the queue is empty, everything else is refused.
    let (st, b) = e
        .call(Some(BASIC), Method::GET, "/admin/review/queue", None)
        .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(b["total"], 0);
    for (m, p) in [
        (Method::GET, format!("/admin/review/submissions/{id}")),
        (
            Method::POST,
            format!("/admin/review/submissions/{id}/claim"),
        ),
    ] {
        let (st, b) = e.call(Some(BASIC), m.clone(), &p, None).await;
        assert_eq!(st, StatusCode::FORBIDDEN, "{m} {p} {b}");
    }
}

#[tokio::test]
async fn the_whole_review_runs_over_http() {
    let e = env().await;
    let id = e.create(MEMBER, "My Series").await;

    // Strangers see nothing of it.
    let (st, _) = e
        .call(
            Some(OTHER),
            Method::GET,
            &format!("/submissions/{id}"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (_, mine) = e.call(Some(OTHER), Method::GET, "/submissions", None).await;
    assert_eq!(mine["total"], 0);

    // Not ready: problems are field errors.
    let bad = e.create(MEMBER, "broken").await;
    let (st, b) = e
        .call(
            Some(MEMBER),
            Method::POST,
            &format!("/submissions/{bad}/submit"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{b}");
    assert!(b.to_string().contains("Chapter 1 has no pages yet"), "{b}");

    // Submit; the submission is no longer editable; a reviewer sees it queued.
    let (st, b) = e
        .call(
            Some(MEMBER),
            Method::POST,
            &format!("/submissions/{id}/submit"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["submission"]["state"], "submitted");
    let (st, _) = e
        .call(
            Some(MEMBER),
            Method::PATCH,
            &format!("/submissions/{id}"),
            Some(json!({ "title": "late" })),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);
    let (st, q) = e
        .call(
            Some(REVIEWER),
            Method::GET,
            "/admin/review/queue?kind=library.series&sort=submitted&order=asc&limit=10",
            None,
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{q}");
    assert_eq!(q["total"], 1);
    assert_eq!(q["items"][0]["id"], id);
    // Bad parameters are 400s.
    for p in [
        "/admin/review/queue?state=nope",
        "/admin/review/queue?order=sideways",
        "/admin/review/queue?claimed=maybe",
        "/admin/review/queue?submitter=abc",
        "/admin/review/queue?sort=color",
    ] {
        let (st, _) = e.call(Some(REVIEWER), Method::GET, p, None).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{p}");
    }

    // Deciding needs the claim first.
    let base = format!("/admin/review/submissions/{id}");
    let (st, _) = e
        .call(
            Some(REVIEWER),
            Method::POST,
            &format!("{base}/approve"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);
    let (st, b) = e
        .call(Some(REVIEWER), Method::POST, &format!("{base}/claim"), None)
        .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["submission"]["claimed_by"], REVIEWER.to_string());
    let (_, prev) = e
        .call(
            Some(REVIEWER),
            Method::GET,
            &format!("{base}/preview"),
            None,
        )
        .await;
    assert_eq!(prev["preview"]["title"], "My Series");

    // An internal note stays with the reviewers, a public one reaches the submitter.
    e.call(
        Some(REVIEWER),
        Method::POST,
        &format!("{base}/comments"),
        Some(json!({ "body": "check the scan quality", "internal": true })),
    )
    .await;
    e.call(
        Some(REVIEWER),
        Method::POST,
        &format!("{base}/comments"),
        Some(json!({ "body": "thanks for this" })),
    )
    .await;
    let (_, mine) = e
        .call(
            Some(MEMBER),
            Method::GET,
            &format!("/submissions/{id}"),
            None,
        )
        .await;
    let text = mine.to_string();
    assert!(text.contains("thanks for this") && !text.contains("check the scan quality"));
    let (_, theirs) = e.call(Some(REVIEWER), Method::GET, &base, None).await;
    assert!(theirs.to_string().contains("check the scan quality"));

    // Request changes needs a reason.
    let (st, _) = e
        .call(
            Some(REVIEWER),
            Method::POST,
            &format!("{base}/request-changes"),
            Some(json!({})),
        )
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, b) = e
        .call(
            Some(REVIEWER),
            Method::POST,
            &format!("{base}/request-changes"),
            Some(json!({ "reason": "Add the cover" })),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["submission"]["state"], "changes_requested");

    // Resubmit, claim, approve: published.
    e.call(
        Some(MEMBER),
        Method::POST,
        &format!("/submissions/{id}/submit"),
        None,
    )
    .await;
    e.call(Some(REVIEWER), Method::POST, &format!("{base}/claim"), None)
        .await;
    let item = theirs["items"][0]["id"].as_str().unwrap().to_owned();
    let (st, b) = e
        .call(
            Some(REVIEWER),
            Method::PUT,
            &format!("{base}/items/{item}"),
            Some(json!({ "state": "approved" })),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let (st, b) = e
        .call(
            Some(REVIEWER),
            Method::POST,
            &format!("{base}/approve"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["submission"]["state"], "published");
    assert_eq!(b["items"][0]["state"], "published");

    // The new item earned the submitter a credit; the allowance endpoint answers.
    let (st, c) = e
        .call(
            Some(MEMBER),
            Method::GET,
            &format!("/submissions/{id}/credits"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{c}");
    assert_eq!(c.as_array().unwrap().len(), 1, "{c}");
    let (st, q) = e
        .call(Some(MEMBER), Method::GET, "/submissions/quota", None)
        .await;
    assert_eq!(st, StatusCode::OK, "{q}");
    assert_eq!(q["limit_bytes"], 50_000_000_000_i64);
    // Another member sees nothing of it; a plain reviewer cannot revoke.
    let (st, _) = e
        .call(
            Some(OTHER),
            Method::GET,
            &format!("/submissions/{id}/credits"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _) = e
        .call(
            Some(REVIEWER),
            Method::POST,
            &format!("{base}/revoke-credits"),
            Some(json!({ "reason": "spam" })),
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // The access log has the decision trail.
    let actions = e.actions();
    for want in [
        "review.claim",
        "review.request_changes",
        "review.item",
        "review.approve",
    ] {
        assert!(actions.contains(&want.to_owned()), "{want} in {actions:?}");
    }
    // The submitter got the gateway nudges.
    let types: Vec<String> = e
        .sink
        .0
        .lock()
        .unwrap()
        .iter()
        .map(|(_, p)| p["type"].as_str().unwrap().to_owned())
        .collect();
    assert!(types.contains(&"changes_requested".to_owned()), "{types:?}");
    assert!(types.contains(&"published".to_owned()), "{types:?}");

    // The final submission is listed with the state filter.
    let (_, l) = e
        .call(
            Some(MEMBER),
            Method::GET,
            "/submissions?state=published&sort=updated&order=desc",
            None,
        )
        .await;
    assert_eq!(l["total"], 1);
}

#[tokio::test]
async fn own_submissions_cannot_be_reviewed_and_withdrawing_works() {
    let e = env().await;
    // A reviewer's own submission is not reviewable by them.
    let id = e.create(REVIEWER, "Mine").await;
    e.call(
        Some(REVIEWER),
        Method::POST,
        &format!("/submissions/{id}/submit"),
        None,
    )
    .await;
    let (st, _) = e
        .call(
            Some(REVIEWER),
            Method::POST,
            &format!("/admin/review/submissions/{id}/claim"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (_, q) = e
        .call(Some(REVIEWER), Method::GET, "/admin/review/queue", None)
        .await;
    assert_eq!(q["total"], 0);
    // Withdraw once, then it is final.
    let (st, b) = e
        .call(
            Some(REVIEWER),
            Method::POST,
            &format!("/submissions/{id}/withdraw"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["submission"]["state"], "withdrawn");
    let (st, _) = e
        .call(
            Some(REVIEWER),
            Method::POST,
            &format!("/submissions/{id}/withdraw"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);
    // Malformed ids name nothing.
    let (st, _) = e
        .call(Some(MEMBER), Method::GET, "/submissions/abc", None)
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_library_draft_routes_answer_when_the_library_is_off() {
    let e = env().await;
    let id = e.create(MEMBER, "Series").await;
    let (st, _) = e
        .call(
            Some(MEMBER),
            Method::GET,
            &format!("/submissions/{id}/library/chapters"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);
}

#[tokio::test]
async fn requests_board_over_http() {
    let e = env().await;
    let (st, b) = e
        .call(
            Some(MEMBER),
            Method::POST,
            "/requests",
            Some(json!({ "section": "library", "title": "Dungeon Meshi", "link": "https://example.com" })),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{b}");
    let rid = b["id"].as_str().unwrap().to_owned();
    for bad in [
        json!({ "section": "tv", "title": "x" }),
        json!({ "section": "library", "title": "" }),
        json!({ "section": "library", "title": "x", "link": "ftp://nope" }),
    ] {
        let (st, _) = e
            .call(Some(MEMBER), Method::POST, "/requests", Some(bad))
            .await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
    }
    let (_, v) = e
        .call(
            Some(OTHER),
            Method::PUT,
            &format!("/requests/{rid}/vote"),
            None,
        )
        .await;
    assert_eq!(v["vote_count"], 2);
    let (_, l) = e
        .call(
            Some(OTHER),
            Method::GET,
            "/requests?status=open&sort=votes",
            None,
        )
        .await;
    assert_eq!(l["items"][0]["voted"], true);
    let (st, _) = e
        .call(Some(OTHER), Method::GET, "/requests?status=weird", None)
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (_, v) = e
        .call(
            Some(OTHER),
            Method::DELETE,
            &format!("/requests/{rid}/vote"),
            None,
        )
        .await;
    assert_eq!(v["vote_count"], 1);
    // "I'll upload this": one member at a time.
    let (st, c) = e
        .call(
            Some(OTHER),
            Method::PUT,
            &format!("/requests/{rid}/claim"),
            Some(json!({ "note": "scanning vol 1" })),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{c}");
    assert_eq!(c["claim_note"], "scanning vol 1");
    assert_eq!(c["claimed_by_me"], true);
    let (st, _) = e
        .call(
            Some(MEMBER),
            Method::PUT,
            &format!("/requests/{rid}/claim"),
            Some(json!({})),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);
    let (_, l) = e
        .call(Some(MEMBER), Method::GET, "/requests?status=open", None)
        .await;
    assert_eq!(l["items"][0]["claimed_by_me"], false);
    assert!(l["items"][0]["claimed_by"].is_string());
    let (st, c) = e
        .call(
            Some(OTHER),
            Method::DELETE,
            &format!("/requests/{rid}/claim"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{c}");
    assert!(c["claimed_by"].is_null());
    // Declining is for reviewers only, and is logged.
    let (st, _) = e
        .call(
            Some(MEMBER),
            Method::POST,
            &format!("/admin/review/requests/{rid}/decline"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, b) = e
        .call(
            Some(REVIEWER),
            Method::POST,
            &format!("/admin/review/requests/{rid}/decline"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["status"], "declined");
    assert!(e.actions().contains(&"review.request.decline".to_owned()));
    // A declined request takes no votes.
    let (st, _) = e
        .call(
            Some(OTHER),
            Method::PUT,
            &format!("/requests/{rid}/vote"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);
}

#[test]
fn openapi_lists_the_review_paths() {
    let doc = serde_json::to_value(super::openapi()).unwrap();
    let paths = doc["paths"].as_object().unwrap();
    for p in [
        "/submissions",
        "/submissions/{submission_id}/submit",
        "/submissions/{submission_id}/library/chapters/{chapter_id}/uploads",
        "/requests/{request_id}/vote",
        "/admin/review/queue",
        "/submissions/quota",
        "/submissions/naming/parse",
        "/submissions/{submission_id}/credits",
        "/admin/review/submissions/{submission_id}/credits",
        "/admin/review/submissions/{submission_id}/revoke-credits",
        "/admin/review/submissions/{submission_id}/approve",
        "/admin/review/submissions/{submission_id}/items/{item_id}",
    ] {
        assert!(paths.contains_key(p), "{p}");
    }
    let schemas = doc["components"]["schemas"].as_object().unwrap();
    for s in [
        "Submission",
        "SubmissionDetail",
        "SubmissionState",
        "ContentRequest",
    ] {
        assert!(schemas.contains_key(s), "{s}");
    }
}

#[tokio::test]
async fn the_naming_tester_parses_both_sections() {
    let e = env().await;
    let (st, b) = e
        .call(
            Some(MEMBER),
            Method::POST,
            "/submissions/naming/parse",
            Some(json!({ "section": "cinema", "names": ["Show - s01e02 - Pilot.mkv"] })),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["items"][0]["cinema"]["season"], 1);
    assert_eq!(b["items"][0]["cinema"]["episodes"][0], 2);
    assert!(b["items"][0]["library"].is_null());
    let (st, b) = e
        .call(
            Some(MEMBER),
            Method::POST,
            "/submissions/naming/parse",
            Some(json!({ "section": "library", "names": ["Series v02 c013 [Grp].cbz"] })),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["items"][0]["library"]["chapter"], 13.0);
    assert_eq!(b["items"][0]["library"]["group"], "Grp");
    let (st, _) = e
        .call(
            Some(MEMBER),
            Method::POST,
            "/submissions/naming/parse",
            Some(json!({ "section": "tv", "names": [] })),
        )
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let many: Vec<String> = (0..201).map(|i| format!("{i}.mkv")).collect();
    let (st, _) = e
        .call(
            Some(MEMBER),
            Method::POST,
            "/submissions/naming/parse",
            Some(json!({ "section": "cinema", "names": many })),
        )
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}
