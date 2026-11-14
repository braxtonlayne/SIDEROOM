//! The bug report routes through the real router (session, CSRF, module gate, body limit, staff context), over an
//! in-memory store and the in-memory admin repository (whose hash-chained access log is the audit trail).

use super::*;
use crate::auth::{
    csrf,
    memory::{MemoryBots, MemorySessions},
};
use crate::module_gate::{allows, rule_for};
use crate::routes::admin::AdminDeps;
use crate::routes::settings::features::{GuildFeatures, RoleAssigner};
use crate::{ApiSettings, AppState, build_router};
use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Method, Request, header},
};
use data_encoding::BASE64;
use http_body_util::BodyExt;
use serde_json::json;
use sr_admin::AdminRepository;
use sr_admin::{AccountState, AdminService, Caller, SYSTEM_USER_ID, memory::MemoryAdminRepo};
use sr_auth::{
    repo::{SessionRecord, SessionRepository},
    session::SessionToken,
};
use sr_chat::{ChatError, ChatResult};
use sr_core::PlatformPermissions as P;
use sr_core::{Clock, GuildId, ManualClock, RoleId, UserId};
use sr_events::InProcessBus;
use sr_features::testing::FakeActions;
use sr_features::{
    BETA_REPORTS_ID, FeatureCtx, FeatureRegistry, FeatureService, InMemorySettingsStore, ScopeRef,
};
use tower::ServiceExt;

const NOW_MS: i64 = 1_800_000_000_000;
const SECRET: &str = "srg_9f8e7d6c5b4a39281706f5e4d3c2b1a0";
/// A member who reports, a staff member with the basic bit only, the Owner, and somebody never appointed.
const MEMBER: i64 = 5;
const OWNER: i64 = 10;
const STAFF: i64 = 12;
const OUTSIDER: i64 = 30;
const R_OWNER: i64 = 1;
const R_BASIC: i64 = 2;

struct NoRoles;
#[async_trait]
impl RoleAssigner for NoRoles {
    async fn assign(&self, _: UserId, _: GuildId, _: UserId, _: RoleId, _: bool) -> ChatResult<()> {
        Err(ChatError::MissingPermissions)
    }
}

struct Stored {
    id: i64,
    new: NewBug,
    status: &'static str,
    closed_by: Option<i64>,
}

/// A working in-memory store: what the route hands it is what staff read back.
#[derive(Default)]
struct MemoryBugs {
    rows: Mutex<Vec<Stored>>,
    accounts: std::sync::atomic::AtomicI64,
}

impl MemoryBugs {
    fn view(s: &Stored, full: bool) -> BugRow {
        BugRow {
            id: s.id,
            created_ms: NOW_MS,
            status: s.status.to_owned(),
            route: s.new.route.clone(),
            commit_sha: s.new.commit_sha.clone(),
            reporter: BugPerson {
                id: s.new.reporter_id,
                username: format!("user{}", s.new.reporter_id),
            },
            has_screenshot: s.new.screenshot.is_some(),
            description: full.then(|| s.new.description.clone()),
            bundle: full.then(|| s.new.bundle.clone()),
            closed_ms: s.closed_by.map(|_| NOW_MS),
            closed_by: s.closed_by.map(|id| BugPerson {
                id,
                username: format!("user{id}"),
            }),
        }
    }
}

#[async_trait]
impl BugStore for MemoryBugs {
    async fn insert(&self, new: &NewBug) -> Result<i64, String> {
        let mut rows = self.rows.lock().unwrap();
        let id = i64::try_from(rows.len()).unwrap() + 101;
        rows.push(Stored {
            id,
            new: new.clone(),
            status: "open",
            closed_by: None,
        });
        Ok(id)
    }
    async fn list(&self, status: Option<&str>, limit: i64) -> Result<(Vec<BugRow>, i64), String> {
        let rows = self.rows.lock().unwrap();
        let open = rows.iter().filter(|s| s.status == "open").count();
        let out = rows
            .iter()
            .rev()
            .filter(|s| status.is_none_or(|w| w == s.status))
            .take(usize::try_from(limit).unwrap())
            .map(|s| Self::view(s, false))
            .collect();
        Ok((out, i64::try_from(open).unwrap()))
    }
    async fn active_accounts(&self) -> Result<i64, String> {
        Ok(self.accounts.load(std::sync::atomic::Ordering::SeqCst))
    }
    async fn get(&self, id: i64) -> Result<Option<BugRow>, String> {
        let rows = self.rows.lock().unwrap();
        Ok(rows
            .iter()
            .find(|s| s.id == id)
            .map(|s| Self::view(s, true)))
    }
    async fn screenshot(&self, id: i64) -> Result<Option<Vec<u8>>, String> {
        let rows = self.rows.lock().unwrap();
        Ok(rows
            .iter()
            .find(|s| s.id == id)
            .and_then(|s| s.new.screenshot.clone()))
    }
    async fn close(&self, id: i64, by: i64) -> Result<bool, String> {
        let mut rows = self.rows.lock().unwrap();
        let Some(s) = rows.iter_mut().find(|s| s.id == id) else {
            return Ok(false);
        };
        s.status = "closed";
        s.new.screenshot = None;
        s.closed_by.get_or_insert(by);
        Ok(true)
    }
}

struct Fx {
    app: Router,
    store: Arc<MemoryBugs>,
    repo: Arc<MemoryAdminRepo>,
    features: FeatureService,
    tokens: HashMap<i64, String>,
}

async fn fixture(switch_on: bool, limits: BugLimits) -> Fx {
    let sessions = Arc::new(MemorySessions::new());
    let clock = Arc::new(ManualClock::new(NOW_MS));
    let now = NOW_MS / 1000;
    let mut tokens = HashMap::new();
    for (n, uid) in [MEMBER, OWNER, STAFF, OUTSIDER].into_iter().enumerate() {
        let t = SessionToken::generate().unwrap();
        sessions
            .create(SessionRecord {
                id: 99 + n as i64,
                user_id: uid,
                token_hash: t.hash(),
                created_at: now - 10,
                last_seen_at: now - 10,
                expires_at: now + 1000,
                device_name: None,
            })
            .await
            .unwrap();
        tokens.insert(uid, t.expose().to_owned());
    }
    // The staff side: the Owner and one appointed member with the basic bit only.
    let repo = Arc::new(MemoryAdminRepo::new(clock.clone()));
    repo.add_account(
        SYSTEM_USER_ID,
        "sideroom",
        AccountState::Deactivated,
        None,
        None,
    );
    repo.add_account(OWNER, "owner", AccountState::Active, None, None);
    for (id, name) in [(MEMBER, "member"), (STAFF, "staff"), (OUTSIDER, "outsider")] {
        repo.add_account(id, name, AccountState::Active, None, Some(OWNER));
    }
    repo.add_role(R_OWNER, "Owner", 3, P::OWNER);
    repo.add_role(R_BASIC, "Basic", 1, P::STAFF_BASIC);
    repo.set_owner(OWNER);
    let svc = AdminService::new(repo.clone())
        .with_extras(repo.clone())
        .with_clock(clock.clone());
    svc.appoint(
        &Caller::operator(SYSTEM_USER_ID),
        STAFF,
        &[R_BASIC],
        Some("test"),
    )
    .await
    .unwrap();

    let mut registry = FeatureRegistry::new();
    sr_features::wave_modules::register_all(&mut registry).unwrap();
    let features = FeatureService::new(
        Arc::new(registry),
        Arc::new(InMemorySettingsStore::new()),
        None,
    );
    if switch_on {
        features
            .set_enabled(BETA_REPORTS_ID, &ScopeRef::Platform, Some(true))
            .await
            .unwrap();
    }
    let dyn_clock: Arc<dyn Clock> = clock;
    let gf = GuildFeatures {
        ctx: FeatureCtx {
            bus: Arc::new(InProcessBus::new()),
            settings: features.clone(),
            clock: dyn_clock.clone(),
            actions: Arc::new(FakeActions::new()),
            state: Arc::new(sr_features::state::InMemoryState::new()),
        },
        roles: Arc::new(NoRoles),
    };
    let store = Arc::new(MemoryBugs::default());
    let deps = BugDeps::new(store.clone()).with_limits(limits);
    let mut api = ApiSettings::default();
    api.rate_limit.enabled = false;
    let state = AppState::builder()
        .settings(api)
        .sessions(sessions)
        .bots(Arc::new(MemoryBots::new()))
        .clock(dyn_clock)
        .extension(Arc::new(gf))
        .extension(Arc::new(AdminDeps::new(svc)))
        .extension(Arc::new(deps))
        .build();
    Fx {
        app: build_router(state, router()),
        store,
        repo,
        features,
        tokens,
    }
}

impl Fx {
    async fn raw(
        &self,
        who: Option<i64>,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, HeaderMap, Vec<u8>) {
        let mut r = Request::builder()
            .method(method.clone())
            .uri(format!("/api/v1{path}"))
            .header("origin", "http://localhost:8080");
        if let Some(uid) = who {
            let raw = &self.tokens[&uid];
            r = r.header(header::COOKIE, format!("sr_session={raw}"));
            if method != Method::GET {
                r = r.header("x-sr-csrf", csrf::token_for(raw));
            }
        }
        let body = match body {
            Some(b) => {
                r = r.header(header::CONTENT_TYPE, "application/json");
                Body::from(b.to_string())
            }
            None => Body::empty(),
        };
        let res = self
            .app
            .clone()
            .oneshot(r.body(body).unwrap())
            .await
            .unwrap();
        let (parts, body) = res.into_parts();
        (
            parts.status,
            parts.headers,
            body.collect().await.unwrap().to_bytes().to_vec(),
        )
    }

    async fn call(
        &self,
        who: Option<i64>,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let (s, _, b) = self.raw(who, method, path, body).await;
        (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
    }

    /// The member's POST.
    async fn post(&self, signed_in: bool, body: Value) -> (StatusCode, Value) {
        self.call(
            signed_in.then_some(MEMBER),
            Method::POST,
            "/bug-reports",
            Some(body),
        )
        .await
    }

    fn stored(&self) -> Vec<NewBug> {
        let rows = self.store.rows.lock().unwrap();
        rows.iter().map(|s| s.new.clone()).collect()
    }

    /// The platform audit entries of the bug reports: (action, actor, target).
    fn audit(&self) -> Vec<(String, i64, Option<i64>)> {
        self.repo
            .log_rows()
            .into_iter()
            .filter(|r| r.action.starts_with("bug."))
            .map(|r| (r.action, r.actor_id, r.target_id))
            .collect()
    }
}

fn jpeg(len: usize) -> Vec<u8> {
    let mut v = vec![0xFF, 0xD8, 0xFF, 0xE0];
    v.resize(len - 2, 0x11);
    v.extend([0xFF, 0xD9]);
    v
}

fn report() -> Value {
    json!({
        "description": "The send button does nothing.",
        "bundle": { "commit": "abcdef1", "route": "lounge.:guildId", "at": NOW_MS,
                    "viewport": { "w": 1280, "h": 800, "dpr": 1 }, "caps": { "engine": "firefox" } }
    })
}

fn with_picture(bytes: &[u8]) -> Value {
    let mut r = report();
    r["screenshot"] = json!({ "type": "image/jpeg", "data": BASE64.encode(bytes), "masked": 1 });
    r
}

// ---- the member's route ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_bug_report_before_sign_in_is_refused() {
    // With the switch on and with it off, an anonymous call is the plain 401, and nothing is stored.
    for on in [true, false] {
        let f = fixture(on, BugLimits::default()).await;
        let (s, _) = f.post(false, report()).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "switch on: {on}");
        assert!(f.stored().is_empty());
    }
}

#[tokio::test]
async fn a_bug_report_is_refused_while_the_switch_is_off() {
    let f = fixture(false, BugLimits::default()).await;
    let (s, v) = f.post(true, report()).await;
    assert_eq!(
        s,
        StatusCode::NOT_FOUND,
        "answers as a switched-off module: {v}"
    );
    assert!(f.stored().is_empty());
    // The operator turns it on: the same call is accepted; off again: refused again.
    f.features
        .set_enabled(BETA_REPORTS_ID, &ScopeRef::Platform, Some(true))
        .await
        .unwrap();
    let (s, v) = f.post(true, report()).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    f.features
        .set_enabled(BETA_REPORTS_ID, &ScopeRef::Platform, Some(false))
        .await
        .unwrap();
    assert_eq!(f.post(true, report()).await.0, StatusCode::NOT_FOUND);
    assert_eq!(f.stored().len(), 1);
}

#[tokio::test]
async fn the_switch_gates_only_the_member_route() {
    let rule = rule_for("/bug-reports").expect("the submit route has a rule");
    assert_eq!(rule.all, vec![BETA_REPORTS_ID]);
    for staff in [
        "/admin/bug-reports",
        "/admin/bug-reports/{bug_id}",
        "/admin/bug-reports/{bug_id}/screenshot",
        "/admin/bug-reports/{bug_id}/close",
    ] {
        assert!(rule_for(staff).is_none(), "{staff} must stay readable");
    }
    // Off by default; on once the operator says so.
    let f = fixture(false, BugLimits::default()).await;
    assert!(!allows(&f.features, &rule, None).await);
    f.features
        .set_enabled(BETA_REPORTS_ID, &ScopeRef::Platform, Some(true))
        .await
        .unwrap();
    assert!(allows(&f.features, &rule, None).await);
}

#[tokio::test]
async fn a_bug_report_keeps_only_allow_listed_bundle_fields() {
    let f = fixture(true, BugLimits::default()).await;
    let mut r = report();
    r["token"] = json!(SECRET);
    r["bundle"]["session_id"] = json!(SECRET);
    r["bundle"]["gateway"] = json!({ "status": "connected", "resume_url": SECRET });
    r["bundle"]["route"] = json!(format!("lounge.{SECRET}"));
    r["bundle"]["failed_requests"] = json!([
        { "method": "GET", "path": format!("/api/v1/x?token={SECRET}"), "status": 500, "ms": 1, "ago_s": 1 },
        { "method": "GET", "path": "/api/v1/users/@me", "status": 401, "ms": 1, "ago_s": 1, "body": SECRET },
    ]);
    let (s, v) = f.post(true, r).await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(v["id"], "101");
    let rows = f.stored();
    assert_eq!(rows.len(), 1);
    let b = &rows[0];
    assert_eq!(
        b.reporter_id, MEMBER,
        "the reporter is the session's account"
    );
    let everything = format!("{}{}{}{}", b.bundle, b.route, b.commit_sha, b.description);
    assert!(!everything.contains(SECRET), "{everything}");
    assert!(!everything.contains("resume_url") && !everything.contains("session_id"));
    assert_eq!(b.route, "unknown");
    assert_eq!(b.bundle["failed_requests"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn a_failed_request_path_with_an_id_is_dropped() {
    let f = fixture(true, BugLimits::default()).await;
    let mut r = report();
    r["bundle"]["failed_requests"] = json!([
        { "method": "GET", "path": "/api/v1/users/123456789/profile", "status": 500, "ms": 1, "ago_s": 1 },
        { "method": "?", "path": "/api/v1/users/:id/profile", "status": 500, "ms": 1, "ago_s": 1 },
    ]);
    assert_eq!(f.post(true, r).await.0, StatusCode::CREATED);
    let b = f.stored().remove(0).bundle;
    assert_eq!(
        b["failed_requests"],
        json!([{ "method": "?", "path": "/api/v1/users/:id/profile", "status": 500, "ms": 1, "ago_s": 1 }])
    );
}

#[tokio::test]
async fn a_picture_that_is_not_a_jpeg_is_refused() {
    let f = fixture(true, BugLimits::default()).await;
    let png = [
        0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0, 0xFF, 0xD9,
    ];
    let html = b"<html><body onload=alert(1)></body></html>";
    for (what, shot) in [
        (
            "a PNG",
            json!({ "type": "image/png", "data": BASE64.encode(&png) }),
        ),
        (
            "a PNG named image/jpeg",
            json!({ "type": "image/jpeg", "data": BASE64.encode(&png) }),
        ),
        (
            "HTML named image/jpeg",
            json!({ "type": "image/jpeg", "data": BASE64.encode(html) }),
        ),
        ("not base64", json!({ "type": "image/jpeg", "data": "***" })),
    ] {
        let mut r = report();
        r["screenshot"] = shot;
        let (s, v) = f.post(true, r).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{what}: {v}");
    }
    assert!(f.stored().is_empty(), "a refused report stores nothing");
    // A real JPEG goes in, with its bytes.
    let bytes = jpeg(3000);
    assert_eq!(
        f.post(true, with_picture(&bytes)).await.0,
        StatusCode::CREATED
    );
    let row = f.stored().remove(0);
    assert_eq!(row.screenshot.as_deref(), Some(bytes.as_slice()));
    assert_eq!(row.bundle["masked"], 1);
}

#[tokio::test]
async fn a_picture_over_the_size_is_refused() {
    let f = fixture(true, BugLimits::default()).await;
    let (s, _) = f
        .post(true, with_picture(&jpeg(allow_list::MAX_PICTURE_BYTES + 1)))
        .await;
    assert_eq!(s, StatusCode::PAYLOAD_TOO_LARGE);
    // A body over the request limit is refused before it is read in full.
    let mut r = report();
    r["screenshot"] =
        json!({ "type": "image/jpeg", "data": "A".repeat(allow_list::MAX_BODY_BYTES + 1) });
    let (s, _) = f.post(true, r).await;
    assert_eq!(s, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(f.stored().is_empty());
}

#[tokio::test]
async fn a_report_without_a_picture_is_accepted() {
    let f = fixture(true, BugLimits::default()).await;
    let (s, v) = f.post(true, report()).await;
    assert_eq!((s, v["id"].as_str().is_some()), (StatusCode::CREATED, true));
    assert!(f.stored()[0].screenshot.is_none());
}

#[tokio::test]
async fn the_sixth_report_in_an_hour_is_refused() {
    let f = fixture(true, BugLimits::default()).await;
    for i in 0..5 {
        let (s, v) = f.post(true, report()).await;
        assert_eq!(s, StatusCode::CREATED, "report {i}: {v}");
    }
    let (s, _) = f.post(true, report()).await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(f.stored().len(), 5);
}

#[tokio::test]
async fn a_refused_report_does_not_use_up_the_hour() {
    let f = fixture(true, BugLimits::default()).await;
    for _ in 0..8 {
        let mut r = report();
        r["description"] = json!("");
        assert_eq!(f.post(true, r).await.0, StatusCode::BAD_REQUEST);
    }
    for _ in 0..5 {
        assert_eq!(f.post(true, report()).await.0, StatusCode::CREATED);
    }
}

#[tokio::test]
async fn a_description_is_required_and_capped() {
    let f = fixture(true, BugLimits::default()).await;
    for bad in [
        json!(""),
        json!("   "),
        Value::Null,
        json!("x".repeat(2001)),
    ] {
        let mut r = report();
        r["description"] = bad.clone();
        let (s, v) = f.post(true, r).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad}: {v}");
    }
    let mut r = report();
    r.as_object_mut().unwrap().remove("description");
    assert_eq!(f.post(true, r).await.0, StatusCode::BAD_REQUEST);
    let mut r = report();
    r["description"] = json!("x".repeat(2000));
    assert_eq!(f.post(true, r).await.0, StatusCode::CREATED);
    assert_eq!(f.stored()[0].description.len(), 2000);
}

#[test]
fn each_account_and_the_platform_have_a_figure() {
    let l = BugLimiter::new(BugLimits {
        per_user: 2,
        per_user_window: Duration::from_secs(60),
        global: 3,
        global_window: Duration::from_secs(60),
    });
    assert!(l.allow_report(1));
    assert!(l.allow_report(1));
    // The account is over its figure; another account is not.
    assert!(!l.allow_report(1));
    assert!(l.allow_report(2));
    // The platform is over its figure.
    assert!(!l.allow_report(3));
    let d = BugLimits::default();
    assert_eq!(
        (d.per_user, d.per_user_window),
        (5, Duration::from_secs(3600))
    );
    assert_eq!(
        (d.global, d.global_window),
        (100, Duration::from_secs(86_400))
    );
}

// ---- the staff routes -----------------------------------------------------------------------------------------------

/// A fixture with the switch ON, one report with a picture (id 101) and one without (id 102).
async fn with_two_reports() -> Fx {
    let f = fixture(true, BugLimits::default()).await;
    let mut shot = with_picture(&jpeg(2048));
    shot["description"] = json!("The picture report, with a private sentence.");
    assert_eq!(f.post(true, shot).await.0, StatusCode::CREATED);
    assert_eq!(f.post(true, report()).await.0, StatusCode::CREATED);
    f
}

#[tokio::test]
async fn staff_without_an_appointment_cannot_list_bug_reports() {
    let f = with_two_reports().await;
    for path in [
        "/admin/bug-reports",
        "/admin/bug-reports/101",
        "/admin/bug-reports/101/screenshot",
    ] {
        // Not signed in: 401. Signed in but never appointed (a plain member, and another account): 403.
        assert_eq!(
            f.call(None, Method::GET, path, None).await.0,
            StatusCode::UNAUTHORIZED,
            "{path}"
        );
        for who in [MEMBER, OUTSIDER] {
            let (s, v) = f.call(Some(who), Method::GET, path, None).await;
            assert_eq!(s, StatusCode::FORBIDDEN, "{path} as {who}: {v}");
        }
    }
    for who in [MEMBER, OUTSIDER] {
        let (s, _) = f
            .call(
                Some(who),
                Method::POST,
                "/admin/bug-reports/101/close",
                None,
            )
            .await;
        assert_eq!(s, StatusCode::FORBIDDEN);
    }
    // Nothing was read or closed, so no `bug.*` entry says so.
    assert!(f.audit().is_empty(), "{:?}", f.audit());
    assert_eq!(f.store.rows.lock().unwrap()[0].status, "open");
}

#[tokio::test]
async fn the_list_carries_no_description_and_no_picture() {
    let f = with_two_reports().await;
    let (s, h, b) = f
        .raw(Some(STAFF), Method::GET, "/admin/bug-reports", None)
        .await;
    assert_eq!(s, StatusCode::OK);
    assert!(
        h[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("application/json")
    );
    let text = String::from_utf8(b).unwrap();
    assert!(
        !text.contains("private sentence") && !text.contains("description"),
        "{text}"
    );
    assert!(!text.contains("bundle") && !text.contains("\"screenshot\""));
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["open_count"], 2);
    let rows = v["reports"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    // Newest first, and each row names its reporter and whether a picture is kept.
    assert_eq!(rows[0]["id"], "102");
    assert_eq!(rows[0]["has_screenshot"], false);
    assert_eq!(rows[1]["id"], "101");
    assert_eq!(rows[1]["has_screenshot"], true);
    assert_eq!(
        rows[1]["reporter"],
        json!({ "id": "5", "username": "user5" })
    );
    assert_eq!(rows[1]["commit"], "abcdef1");
    assert_eq!(rows[1]["status"], "open");
    // The status filter, and a refused one. The list leaves no entry.
    let (_, v) = f
        .call(
            Some(STAFF),
            Method::GET,
            "/admin/bug-reports?status=closed",
            None,
        )
        .await;
    assert_eq!(v["reports"], json!([]));
    assert_eq!(v["open_count"], 2, "the badge counts all open reports");
    let (s, _) = f
        .call(
            Some(STAFF),
            Method::GET,
            "/admin/bug-reports?status=bogus",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(f.audit().is_empty());
}

#[tokio::test]
async fn opening_a_bug_report_writes_an_audit_entry() {
    let f = with_two_reports().await;
    let (s, v) = f
        .call(Some(STAFF), Method::GET, "/admin/bug-reports/101", None)
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(
        v["description"],
        "The picture report, with a private sentence."
    );
    assert_eq!(v["bundle"]["commit"], "abcdef1");
    assert_eq!(v["closed_by"], Value::Null);
    assert_eq!(v["has_screenshot"], true);
    assert_eq!(
        f.audit(),
        [("bug.report.read".to_owned(), STAFF, Some(101))]
    );
    // An unknown id and a malformed one are 404s.
    for path in ["/admin/bug-reports/999", "/admin/bug-reports/abc"] {
        let (s, _) = f.call(Some(STAFF), Method::GET, path, None).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{path}");
    }
    assert!(f.repo.verify_log().await.unwrap().ok);
}

#[tokio::test]
async fn opening_a_screenshot_writes_an_audit_entry_and_serves_a_download() {
    let f = with_two_reports().await;
    let (s, h, b) = f
        .raw(
            Some(STAFF),
            Method::GET,
            "/admin/bug-reports/101/screenshot",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b, jpeg(2048));
    assert_eq!(h[header::CONTENT_TYPE], "image/jpeg");
    assert_eq!(
        h[header::CONTENT_DISPOSITION],
        "attachment; filename=\"bug-101.jpg\""
    );
    assert_eq!(h["x-content-type-options"], "nosniff");
    assert_eq!(h[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        f.audit(),
        [("bug.screenshot.read".to_owned(), STAFF, Some(101))]
    );
    // A report without a picture answers 404.
    let (s, _) = f
        .call(
            Some(STAFF),
            Method::GET,
            "/admin/bug-reports/102/screenshot",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn closing_a_report_deletes_its_picture_and_writes_an_audit_entry() {
    let f = with_two_reports().await;
    let path = "/admin/bug-reports/101/close";
    let (s, _, b) = f
        .raw(Some(STAFF), Method::POST, path, Some(json!({})))
        .await;
    assert_eq!((s, b.len()), (StatusCode::NO_CONTENT, 0));
    // The report is closed, by whom, and its picture is gone.
    let (_, v) = f
        .call(Some(STAFF), Method::GET, "/admin/bug-reports/101", None)
        .await;
    assert_eq!(v["status"], "closed");
    assert_eq!(v["closed_by"], json!({ "id": "12", "username": "user12" }));
    assert_eq!(v["has_screenshot"], false);
    let (s, _) = f
        .call(
            Some(STAFF),
            Method::GET,
            "/admin/bug-reports/101/screenshot",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // Closing again is fine, an unknown report is a 404, and the badge moved.
    let (s, _) = f
        .call(Some(STAFF), Method::POST, path, Some(json!({})))
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = f
        .call(
            Some(STAFF),
            Method::POST,
            "/admin/bug-reports/999/close",
            Some(json!({})),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (_, v) = f
        .call(Some(STAFF), Method::GET, "/admin/bug-reports", None)
        .await;
    assert_eq!(v["open_count"], 1);
    let closes: Vec<_> = f
        .audit()
        .into_iter()
        .filter(|e| e.0 == "bug.report.close")
        .collect();
    assert_eq!(closes[0], ("bug.report.close".to_owned(), STAFF, Some(101)));
    assert!(closes.len() >= 2, "every close is an entry: {closes:?}");
}

#[tokio::test]
async fn staff_can_read_reports_while_the_switch_is_off() {
    let f = with_two_reports().await;
    f.features
        .set_enabled(BETA_REPORTS_ID, &ScopeRef::Platform, Some(false))
        .await
        .unwrap();
    // Members can no longer send...
    assert_eq!(f.post(true, report()).await.0, StatusCode::NOT_FOUND);
    // ...but staff can list, read, download and close.
    assert_eq!(
        f.call(Some(STAFF), Method::GET, "/admin/bug-reports", None)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        f.call(Some(STAFF), Method::GET, "/admin/bug-reports/101", None)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        f.raw(
            Some(STAFF),
            Method::GET,
            "/admin/bug-reports/101/screenshot",
            None
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        f.call(
            Some(STAFF),
            Method::POST,
            "/admin/bug-reports/101/close",
            Some(json!({}))
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
}

#[test]
fn the_picture_is_an_inert_download_with_fixed_headers() {
    let res = picture_response(42, jpeg(100));
    let h = res.headers();
    assert_eq!(h[header::CONTENT_TYPE], "image/jpeg");
    assert_eq!(
        h[header::CONTENT_DISPOSITION],
        "attachment; filename=\"bug-42.jpg\""
    );
    assert_eq!(h["x-content-type-options"], "nosniff");
    assert_eq!(h[header::CACHE_CONTROL], "no-store");
}

/// SEC-AUD-195 (D-647, D-665): the list says, with one boolean and no number, when the beta reporter is on and the
/// platform has 100 or more active accounts. STAFF_BASIC staff read it.
#[tokio::test]
async fn the_list_says_when_the_beta_is_on_with_100_accounts() {
    let fx = fixture(true, BugLimits::default()).await;
    let set = |n: i64| {
        fx.store
            .accounts
            .store(n, std::sync::atomic::Ordering::SeqCst)
    };
    let flag = |fx: &Fx| {
        let fx = fx;
        async move {
            let (s, v) = fx
                .call(Some(STAFF), Method::GET, "/admin/bug-reports", None)
                .await;
            assert_eq!(s, StatusCode::OK);
            assert!(v.get("accounts").is_none() && v.get("account_count").is_none());
            v["beta_over_100"].as_bool().unwrap()
        }
    };
    set(99);
    assert!(!flag(&fx).await, "99 accounts: no notice");
    set(100);
    assert!(flag(&fx).await, "100 accounts: the notice");
    fx.features
        .set_enabled(BETA_REPORTS_ID, &ScopeRef::Platform, Some(false))
        .await
        .unwrap();
    assert!(!flag(&fx).await, "switch off: no notice");
}
