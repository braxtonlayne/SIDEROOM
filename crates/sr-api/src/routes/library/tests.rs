//! Endpoint tests of the Library router over a stub repository: authentication and
//! staff matrices, permission mapping, the age gate reaching the repository, and the
//! JSON shape. Behaviour against real tables is covered by `sr-db/tests/library_repo.rs`.

use super::{LibraryDeps, LibraryPrefs, router};
use crate::{
    ApiSettings, AppState,
    auth::{
        BotIdentity, csrf,
        memory::{MemoryBots, MemorySessions},
    },
    build_router,
    routes::{admin::AdminDeps, search::AgeGate},
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
use sr_core::{PlatformPermissions as P, SystemClock, UserId};
use sr_library::error::{LibraryError, LibraryResult};
use sr_library::model::*;
use sr_library::repo::{LibraryRepository, Neighbours, NewSeries, Rows, UpdateRecord};
use sr_library::{LibraryService, SeriesIndex};
use sr_storage::cdn::CdnSigner;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};
use tower::ServiceExt;

const OWNER: i64 = 10;
const CURATOR: i64 = 11;
const BASIC: i64 = 12;
const MEMBER: i64 = 20;
const ADULT: i64 = 21;
/// Confirmed adult who set `library.hide_age_restricted`.
const HIDER: i64 = 22;
const R_OWNER: i64 = 1;
const R_CURATOR: i64 = 2;
const R_BASIC: i64 = 3;

fn record(id: i64, restricted: bool) -> SeriesRecord {
    SeriesRecord {
        id,
        kind: SeriesKind::Manga,
        title: "Stub".into(),
        alt_titles: vec![],
        authors: vec![],
        artists: vec![],
        status: SeriesStatus::Ongoing,
        year: None,
        synopsis: String::new(),
        genres: vec![],
        tags: vec![],
        reading_direction: ReadingDirection::Rtl,
        age_restricted: restricted,
        cover_key: Some(format!("library/{id}/cover.webp")),
        published: true,
        published_at_ms: Some(1),
        merged_into: None,
        chapter_count: 0,
        last_chapter_at_ms: None,
        follower_count: 0,
        created_at_ms: 1,
        updated_at_ms: 1,
    }
}

/// Answers the few calls the tests make and records what the service asked.
#[derive(Default)]
struct Stub {
    browses: Mutex<Vec<(Scope, AgeFilter)>>,
    /// `include_restricted` of every `latest_updates` call.
    updates: Mutex<Vec<bool>>,
}

/// Accounts that confirmed their age; the test adds to it like `POST /sr/age-gate` does.
struct Confirmed(Mutex<HashSet<UserId>>);

#[async_trait]
impl AgeGate for Confirmed {
    async fn age_confirmed(&self, user: UserId) -> bool {
        self.0.lock().unwrap().contains(&user)
    }
}

struct Hiders(HashSet<UserId>);

#[async_trait]
impl LibraryPrefs for Hiders {
    async fn hide_age_restricted(&self, user: UserId) -> bool {
        self.0.contains(&user)
    }
}

/// Chapter 90 belongs to the restricted series 9, chapter 70 to the plain series 7.
fn chapter_record(id: i64) -> ChapterRecord {
    ChapterRecord {
        id,
        series_id: if id == 90 { 9 } else { 7 },
        edition_id: 1,
        volume_id: None,
        number: 1.0,
        title: None,
        page_count: 0,
        format: ChapterFormat::Images,
        source_key: None,
        release_group: None,
        published: true,
        published_at_ms: Some(1),
        series_published: true,
        series_age_restricted: id == 90,
        series_direction: ReadingDirection::Rtl,
        series_merged: false,
        progress: None,
    }
}

fn unsupported<T>() -> LibraryResult<T> {
    Err(LibraryError::Storage("not stubbed".into()))
}

#[async_trait]
impl LibraryRepository for Stub {
    async fn browse(&self, q: &BrowseQuery, scope: Scope) -> LibraryResult<Rows<SeriesRecord>> {
        self.browses.lock().unwrap().push((scope, q.age));
        Ok(Rows {
            items: vec![record(7, false)],
            total: 1,
        })
    }
    async fn get_series(&self, id: i64, scope: Scope) -> LibraryResult<Option<SeriesRecord>> {
        // Series 9 is age-restricted, everything else is plain.
        let r = record(id, id == 9);
        let _ = scope;
        Ok(Some(r))
    }
    async fn series_by_ids(&self, _: &[i64], _: Scope) -> LibraryResult<Vec<SeriesRecord>> {
        Ok(vec![])
    }
    async fn list_editions(&self, _: i64) -> LibraryResult<Vec<Edition>> {
        Ok(vec![])
    }
    async fn list_volumes(&self, _: i64) -> LibraryResult<Vec<Volume>> {
        Ok(vec![])
    }
    async fn list_chapters(
        &self,
        _: i64,
        _: Option<i64>,
        _: Option<i64>,
        _: Scope,
    ) -> LibraryResult<Vec<ChapterRecord>> {
        Ok(vec![])
    }
    async fn get_chapter(&self, id: i64, _: Option<i64>) -> LibraryResult<Option<ChapterRecord>> {
        Ok(matches!(id, 70 | 90).then(|| chapter_record(id)))
    }
    async fn list_pages(&self, _: i64) -> LibraryResult<Vec<PageRecord>> {
        Ok(vec![])
    }
    async fn neighbours(&self, _: i64) -> LibraryResult<Neighbours> {
        Ok(Neighbours::default())
    }
    async fn latest_updates(
        &self,
        include_restricted: bool,
        _: Option<i64>,
        _: Option<(i64, i64)>,
        _: u32,
    ) -> LibraryResult<Vec<UpdateRecord>> {
        self.updates.lock().unwrap().push(include_restricted);
        Ok(vec![])
    }
    async fn get_follow(&self, _: i64, _: i64) -> LibraryResult<Option<Follow>> {
        Ok(None)
    }
    async fn set_follow(
        &self,
        _: i64,
        _: i64,
        shelf: Shelf,
        notify: bool,
    ) -> LibraryResult<Follow> {
        Ok(Follow {
            shelf,
            notify,
            created_at_ms: 5,
        })
    }
    async fn remove_follow(&self, _: i64, _: i64) -> LibraryResult<bool> {
        Ok(false)
    }
    async fn list_follows(
        &self,
        _: i64,
        _: Option<Shelf>,
        _: bool,
        _: u32,
        _: u32,
    ) -> LibraryResult<Rows<(SeriesRecord, Follow)>> {
        Ok(Rows {
            items: vec![],
            total: 0,
        })
    }
    async fn create_series(&self, s: &NewSeries) -> LibraryResult<SeriesRecord> {
        let mut r = record(s.id, s.input.age_restricted);
        r.title = s.input.title.clone();
        r.published = false;
        Ok(r)
    }
    async fn update_series(
        &self,
        _: i64,
        _: &SeriesInput,
        _: ReadingDirection,
    ) -> LibraryResult<Option<SeriesRecord>> {
        unsupported()
    }
    async fn set_series_published(&self, _: i64, _: bool) -> LibraryResult<Option<SeriesRecord>> {
        Ok(None)
    }
    async fn delete_series(&self, _: i64) -> LibraryResult<bool> {
        Ok(true)
    }
    async fn merge_series(&self, _: i64, _: i64) -> LibraryResult<SeriesRecord> {
        unsupported()
    }
    async fn create_edition(&self, _: i64, _: i64, _: &EditionInput) -> LibraryResult<Edition> {
        unsupported()
    }
    async fn update_edition(&self, _: i64, _: &EditionInput) -> LibraryResult<Option<Edition>> {
        unsupported()
    }
    async fn get_edition(&self, _: i64) -> LibraryResult<Option<Edition>> {
        Ok(None)
    }
    async fn delete_edition(&self, _: i64) -> LibraryResult<bool> {
        unsupported()
    }
    async fn create_volume(&self, _: i64, _: i64, _: &VolumeInput) -> LibraryResult<Volume> {
        unsupported()
    }
    async fn delete_volume(&self, _: i64) -> LibraryResult<bool> {
        unsupported()
    }
    async fn create_chapter(
        &self,
        _: i64,
        _: i64,
        _: &ChapterInput,
        _: i64,
    ) -> LibraryResult<ChapterRecord> {
        unsupported()
    }
    async fn update_chapter(
        &self,
        _: i64,
        _: &ChapterInput,
    ) -> LibraryResult<Option<ChapterRecord>> {
        unsupported()
    }
    async fn set_chapter_published(&self, _: i64, _: bool) -> LibraryResult<Option<ChapterRecord>> {
        unsupported()
    }
    async fn delete_chapter(&self, _: i64) -> LibraryResult<bool> {
        unsupported()
    }
    async fn replace_pages(
        &self,
        _: i64,
        _: &[PageRecord],
    ) -> LibraryResult<Option<ChapterRecord>> {
        unsupported()
    }
    async fn index_batch(&self, _: i64, _: u32) -> LibraryResult<Vec<SeriesRecord>> {
        Ok(vec![])
    }
}

struct Env {
    app: Router,
    stub: Arc<Stub>,
    admin: Arc<MemoryAdminRepo>,
    age: Arc<Confirmed>,
    tokens: HashMap<i64, String>,
}

async fn env_with(library: bool) -> Env {
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
        (CURATOR, "curator"),
        (BASIC, "basic"),
        (MEMBER, "member"),
        (ADULT, "adult"),
        (HIDER, "hider"),
    ] {
        admin.add_account(id, name, AccountState::Active, None, Some(OWNER));
    }
    admin.add_role(R_OWNER, "Owner", 3, P::OWNER);
    admin.add_role(
        R_CURATOR,
        "Library Curator",
        2,
        P::STAFF_BASIC | P::PUBLISH_READING,
    );
    admin.add_role(R_BASIC, "Basic", 1, P::STAFF_BASIC);
    admin.set_owner(OWNER);
    let svc = AdminService::new(admin.clone());
    let op = Caller::operator(SYSTEM_USER_ID);
    svc.appoint(&op, CURATOR, &[R_CURATOR], Some("test"))
        .await
        .unwrap();
    svc.appoint(&op, BASIC, &[R_BASIC], Some("test"))
        .await
        .unwrap();

    let sessions = Arc::new(MemorySessions::new());
    let mut tokens = HashMap::new();
    for (n, uid) in [OWNER, CURATOR, BASIC, MEMBER, ADULT, HIDER]
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

    let stub = Arc::new(Stub::default());
    let service = LibraryService::new(
        stub.clone(),
        CdnSigner::new(vec![3u8; 32]),
        Arc::new(SystemClock),
        Arc::new(|| 4242),
    )
    .with_index(Arc::new(SeriesIndex::in_ram().unwrap()));
    let age = Arc::new(Confirmed(Mutex::new(HashSet::from([
        UserId::new(ADULT as u64),
        UserId::new(HIDER as u64),
    ]))));
    let deps = LibraryDeps::new(service)
        .with_age_gate(age.clone())
        .with_prefs(Arc::new(Hiders(HashSet::from([UserId::new(HIDER as u64)]))));

    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let mut b = AppState::builder()
        .settings(settings)
        .sessions(sessions)
        .bots(bots)
        .extension(Arc::new(AdminDeps::new(svc)));
    if library {
        b = b.extension(Arc::new(deps));
    }
    Env {
        app: build_router(b.build(), router()),
        stub,
        admin,
        age,
        tokens,
    }
}

async fn env() -> Env {
    env_with(true).await
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

fn series_body() -> Value {
    json!({ "kind": "manga", "title": "  New   Series ", "genres": ["Action"] })
}

fn member_routes() -> Vec<(Method, &'static str, Option<Value>)> {
    vec![
        (Method::GET, "/library/series", None),
        (Method::GET, "/library/series/7", None),
        (Method::GET, "/library/series/7/chapters", None),
        (Method::PUT, "/library/series/7/follow", Some(json!({}))),
        (Method::DELETE, "/library/series/7/follow", None),
        (Method::GET, "/library/chapters/5/pages", None),
        (Method::GET, "/library/search?q=ab", None),
        (Method::GET, "/library/updates", None),
        (Method::GET, "/library/me/follows", None),
    ]
}

fn admin_routes() -> Vec<(Method, &'static str, Option<Value>)> {
    vec![
        (Method::GET, "/admin/library/series", None),
        (Method::POST, "/admin/library/series", Some(series_body())),
        (Method::GET, "/admin/library/series/7", None),
        (Method::PUT, "/admin/library/series/7", Some(series_body())),
        (Method::DELETE, "/admin/library/series/7", None),
        (Method::POST, "/admin/library/series/7/publish", None),
        (Method::POST, "/admin/library/series/7/unpublish", None),
        (
            Method::POST,
            "/admin/library/series/7/merge",
            Some(json!({ "target_id": "8" })),
        ),
        (Method::GET, "/admin/library/series/7/chapters", None),
        (
            Method::POST,
            "/admin/library/series/7/editions",
            Some(json!({ "language": "en", "label": "x" })),
        ),
        (
            Method::PUT,
            "/admin/library/editions/7",
            Some(json!({ "language": "en", "label": "x" })),
        ),
        (Method::DELETE, "/admin/library/editions/7", None),
        (
            Method::POST,
            "/admin/library/editions/7/volumes",
            Some(json!({ "number": 1 })),
        ),
        (Method::DELETE, "/admin/library/volumes/7", None),
        (
            Method::POST,
            "/admin/library/editions/7/chapters",
            Some(json!({ "number": 1 })),
        ),
        (
            Method::PUT,
            "/admin/library/chapters/7",
            Some(json!({ "number": 1 })),
        ),
        (Method::DELETE, "/admin/library/chapters/7", None),
        (Method::POST, "/admin/library/chapters/7/publish", None),
        (Method::POST, "/admin/library/chapters/7/unpublish", None),
        (Method::GET, "/admin/library/chapters/7/pages", None),
        (
            Method::PUT,
            "/admin/library/chapters/7/pages",
            Some(json!([])),
        ),
    ]
}

#[tokio::test]
async fn member_routes_need_a_signed_in_user() {
    let e = env().await;
    for (m, p, b) in member_routes() {
        let (s, _) = e.call(None, m.clone(), p, b.clone()).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{m} {p}");
    }
    // Bots are refused.
    let resp = e
        .app
        .clone()
        .oneshot(
            Request::get("/api/v1/library/series")
                .header("authorization", "Bot bot")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_routes_need_staff_with_the_right_permission() {
    let e = env().await;
    for (m, p, b) in admin_routes() {
        let (s, _) = e.call(None, m.clone(), p, b.clone()).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "anonymous {m} {p}");
        // Not staff at all.
        let (s, body) = e.call(Some(MEMBER), m.clone(), p, b.clone()).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "member {m} {p}");
        assert_eq!(body["code"], 900002, "{m} {p}");
        // Staff without library permissions.
        let (s, body) = e.call(Some(BASIC), m.clone(), p, b.clone()).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "basic {m} {p}");
        assert_eq!(body["code"], 50013, "{m} {p}");
    }
}

#[tokio::test]
async fn a_curator_cannot_delete_but_the_owner_can() {
    let e = env().await;
    let (s, body) = e
        .call(
            Some(CURATOR),
            Method::DELETE,
            "/admin/library/series/7",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert!(body["message"].as_str().unwrap().contains("DELETE_CONTENT"));
    let (s, _) = e
        .call(Some(OWNER), Method::DELETE, "/admin/library/series/7", None)
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    // Unpublishing is open to the curator (PUBLISH_READING) and a missing series is 404.
    let (s, body) = e
        .call(
            Some(CURATOR),
            Method::POST,
            "/admin/library/series/7/unpublish",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "{body}");
}

#[tokio::test]
async fn creating_a_series_normalises_returns_string_ids_and_is_logged() {
    let e = env().await;
    let (s, body) = e
        .call(
            Some(CURATOR),
            Method::POST,
            "/admin/library/series",
            Some(series_body()),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{body}");
    assert_eq!(body["id"], "4242");
    assert_eq!(body["title"], "New Series");
    assert_eq!(body["published"], false);
    assert!(
        body["cover_url"]
            .as_str()
            .unwrap()
            .starts_with("/cdn/library/4242/cover.webp?ex=")
    );
    let log = e.admin.log_rows();
    let row = log
        .iter()
        .find(|r| r.action == "library.series.create")
        .unwrap();
    assert_eq!(row.actor_id, CURATOR);
    assert_eq!(row.target_id, Some(4242));
    // Bad input is a 400, not a 500.
    let (s, _) = e
        .call(
            Some(CURATOR),
            Method::POST,
            "/admin/library/series",
            Some(json!({ "kind": "manga", "title": "   " })),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn age_filter_defaults_to_showing_everything() {
    let e = env().await;
    let (s, body) = e
        .call(
            Some(MEMBER),
            Method::GET,
            "/library/series?age=include&genres=Action,Drama&limit=500",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["items"][0]["id"], "7");
    assert_eq!(body["limit"], 100);
    for p in [
        "/library/series",
        "/library/series?age=hide",
        "/library/series?age=only",
    ] {
        let (s, _) = e.call(Some(MEMBER), Method::GET, p, None).await;
        assert_eq!(s, StatusCode::OK, "{p}");
    }
    // A member who hides restricted titles gets `hide` by default, and can still ask.
    let (s, _) = e
        .call(Some(HIDER), Method::GET, "/library/series", None)
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = e
        .call(
            Some(HIDER),
            Method::GET,
            "/library/series?age=include",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let seen = e.stub.browses.lock().unwrap().clone();
    assert_eq!(
        seen,
        [
            (Scope::Public, AgeFilter::Include),
            (Scope::Public, AgeFilter::Include),
            (Scope::Public, AgeFilter::Hide),
            (Scope::Public, AgeFilter::Only),
            (Scope::Public, AgeFilter::Hide),
            (Scope::Public, AgeFilter::Include),
        ]
    );
    // Updates follow the preference too.
    for who in [MEMBER, HIDER] {
        let (s, _) = e
            .call(Some(who), Method::GET, "/library/updates", None)
            .await;
        assert_eq!(s, StatusCode::OK);
    }
    assert_eq!(*e.stub.updates.lock().unwrap(), [true, false]);
}

#[tokio::test]
async fn restricted_series_are_visible_to_unconfirmed_accounts() {
    let e = env().await;
    for who in [MEMBER, ADULT] {
        let (s, body) = e
            .call(Some(who), Method::GET, "/library/series/9", None)
            .await;
        assert_eq!(s, StatusCode::OK, "{body}");
        assert_eq!(body["series"]["age_restricted"], true);
    }
}

#[tokio::test]
async fn opening_restricted_pages_needs_age_confirmation() {
    let e = env().await;
    // Unconfirmed: a documented 403 with its own code, not a 404.
    let (s, body) = e
        .call(
            Some(MEMBER),
            Method::GET,
            "/library/chapters/90/pages",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], 900011);
    // Plain series and confirmed adults are not gated.
    for (who, ch) in [(MEMBER, 70), (ADULT, 90)] {
        let (s, body) = e
            .call(
                Some(who),
                Method::GET,
                &format!("/library/chapters/{ch}/pages"),
                None,
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{body}");
    }
    // Unknown chapters stay 404, never the age error.
    let (s, _) = e
        .call(Some(MEMBER), Method::GET, "/library/chapters/5/pages", None)
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // Confirming (POST /sr/age-gate sets age_confirmed_at) lets the retry succeed.
    e.age.0.lock().unwrap().insert(UserId::new(MEMBER as u64));
    let (s, body) = e
        .call(
            Some(MEMBER),
            Method::GET,
            "/library/chapters/90/pages",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn bad_parameters_are_client_errors() {
    let e = env().await;
    for p in [
        "/library/series?order=sideways",
        "/library/series?year_from=2020&year_to=2010",
        "/library/series?language=%21%21",
        "/library/updates?before=nope",
    ] {
        let (s, _) = e.call(Some(MEMBER), Method::GET, p, None).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{p}");
    }
    let (s, _) = e
        .call(Some(MEMBER), Method::GET, "/library/series/abc", None)
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = e
        .call(
            Some(MEMBER),
            Method::GET,
            "/library/series?kind=podcast",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn follow_round_trip_and_unfollow_of_nothing() {
    let e = env().await;
    let (s, body) = e
        .call(
            Some(MEMBER),
            Method::PUT,
            "/library/series/7/follow",
            Some(json!({ "shelf": "plan_to_read", "notify": false })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["shelf"], "plan_to_read");
    assert_eq!(body["notify"], false);
    let (s, _) = e
        .call(
            Some(MEMBER),
            Method::DELETE,
            "/library/series/7/follow",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_missing_service_is_a_server_error_not_a_client_error() {
    let e = env_with(false).await;
    let (s, _) = e
        .call(Some(MEMBER), Method::GET, "/library/series", None)
        .await;
    assert_eq!(s, StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn openapi_lists_every_library_path() {
    let doc = serde_json::to_value(super::openapi()).unwrap();
    let paths = doc["paths"].as_object().unwrap();
    for p in [
        "/library/series",
        "/library/series/{series_id}",
        "/library/series/{series_id}/chapters",
        "/library/series/{series_id}/follow",
        "/library/chapters/{chapter_id}/pages",
        "/library/search",
        "/library/updates",
        "/library/me/follows",
        "/admin/library/series",
        "/admin/library/series/{series_id}/merge",
        "/admin/library/chapters/{chapter_id}/pages",
    ] {
        assert!(paths.contains_key(p), "{p}");
    }
    assert!(doc["components"]["schemas"]["PageManifest"].is_object());
    assert!(paths["/library/chapters/{chapter_id}/pages"]["get"]["responses"]["403"].is_object());
}
