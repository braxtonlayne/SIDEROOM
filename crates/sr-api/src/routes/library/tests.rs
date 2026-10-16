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
use sr_library::reading::Progress;
use sr_library::reading::{Bookmark, HistoryRecord};
use sr_library::repo::{
    ChapterListQuery, LibraryRepository, Neighbours, NewSeries, Rows, UpdateRecord,
};
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
    progress: Mutex<HashMap<(i64, i64), Progress>>,
    bookmarks: Mutex<Vec<(i64, Bookmark)>>,
    uploads: Mutex<HashMap<i64, UploadRecord>>,
    multipart: Mutex<HashMap<i64, (String, u64)>>,
    /// Keys queued for the storage collector.
    gc: Mutex<Vec<String>>,
    clock: std::sync::atomic::AtomicI64,
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
        _: &ChapterListQuery,
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

#[async_trait]
impl sr_library::ReadingStateRepository for Stub {
    async fn get_progress(&self, user: i64, chapter: i64) -> LibraryResult<Option<Progress>> {
        Ok(self.progress.lock().unwrap().get(&(user, chapter)).cloned())
    }
    async fn put_progress(
        &self,
        user: i64,
        series: i64,
        chapter: i64,
        w: &sr_library::reading::ProgressWrite,
        force: bool,
    ) -> LibraryResult<(Progress, bool)> {
        let mut map = self.progress.lock().unwrap();
        if let (Some(cur), Some(base), false) =
            (map.get(&(user, chapter)), w.base_updated_at_ms, force)
            && cur.updated_at_ms > base
        {
            return Ok((cur.clone(), false));
        }
        let now = self
            .clock
            .fetch_add(10, std::sync::atomic::Ordering::SeqCst)
            + 10;
        let p = Progress {
            chapter_id: chapter,
            series_id: series,
            page_index: w.page_index,
            percent: w.percent,
            cfi: w.cfi.clone(),
            completed: w.completed,
            updated_at_ms: now,
        };
        map.insert((user, chapter), p.clone());
        Ok((p, true))
    }
    async fn clear_progress(&self, user: i64, chapter: i64) -> LibraryResult<bool> {
        Ok(self
            .progress
            .lock()
            .unwrap()
            .remove(&(user, chapter))
            .is_some())
    }
    async fn mark_read(
        &self,
        _: i64,
        _: &ChapterRecord,
        _: bool,
        up_to: bool,
    ) -> LibraryResult<u64> {
        Ok(if up_to { 3 } else { 1 })
    }
    async fn history(
        &self,
        _: i64,
        _: bool,
        _: bool,
        _: Option<(i64, i64)>,
        _: u32,
    ) -> LibraryResult<Vec<HistoryRecord>> {
        Ok(vec![])
    }
    async fn list_bookmarks(&self, user: i64, chapter: i64) -> LibraryResult<Vec<Bookmark>> {
        Ok(self
            .bookmarks
            .lock()
            .unwrap()
            .iter()
            .filter(|(u, b)| *u == user && b.chapter_id == chapter)
            .map(|(_, b)| b.clone())
            .collect())
    }
    async fn put_bookmark(
        &self,
        id: i64,
        user: i64,
        chapter: &ChapterRecord,
        page_index: i32,
        note: &str,
    ) -> LibraryResult<(sr_library::reading::BookmarkOutcome, Option<Bookmark>)> {
        let b = Bookmark {
            id,
            chapter_id: chapter.id,
            series_id: chapter.series_id,
            page_index,
            note: note.to_owned(),
            created_at_ms: 1,
        };
        self.bookmarks.lock().unwrap().push((user, b.clone()));
        Ok((sr_library::reading::BookmarkOutcome::Saved, Some(b)))
    }
    async fn delete_bookmark(&self, user: i64, id: i64) -> LibraryResult<Option<Bookmark>> {
        let mut all = self.bookmarks.lock().unwrap();
        let at = all.iter().position(|(u, b)| *u == user && b.id == id);
        Ok(at.map(|i| all.remove(i).1))
    }
    async fn all_bookmarks(
        &self,
        _: i64,
        _: bool,
        _: Option<(i64, i64)>,
        _: u32,
    ) -> LibraryResult<Vec<sr_library::reading::BookmarkRecord>> {
        Ok(vec![])
    }
    async fn followers_to_notify(&self, _: i64, after: i64, _: u32) -> LibraryResult<Vec<i64>> {
        Ok([MEMBER, ADULT].into_iter().filter(|u| *u > after).collect())
    }
}

#[async_trait]
impl sr_library::UploadRepository for Stub {
    async fn create_upload(&self, u: &UploadRecord) -> LibraryResult<()> {
        self.uploads.lock().unwrap().insert(u.id, u.clone());
        Ok(())
    }
    async fn get_upload(&self, id: i64) -> LibraryResult<Option<UploadRecord>> {
        Ok(self.uploads.lock().unwrap().get(&id).cloned())
    }
    async fn list_uploads(&self, chapter: i64, _: u32) -> LibraryResult<Vec<UploadRecord>> {
        Ok(self
            .uploads
            .lock()
            .unwrap()
            .values()
            .filter(|u| u.chapter_id == chapter)
            .cloned()
            .collect())
    }
    async fn transition_upload(
        &self,
        id: i64,
        from: &[UploadStatus],
        to: UploadStatus,
    ) -> LibraryResult<Option<UploadRecord>> {
        let mut all = self.uploads.lock().unwrap();
        Ok(all
            .get_mut(&id)
            .filter(|u| from.contains(&u.status))
            .map(|u| {
                u.status = to;
                u.clone()
            }))
    }
    async fn finish_upload(
        &self,
        id: i64,
        status: UploadStatus,
        error: Option<&str>,
        warnings: &[String],
        page_count: Option<i32>,
        metadata: &serde_json::Value,
    ) -> LibraryResult<Option<UploadRecord>> {
        let mut all = self.uploads.lock().unwrap();
        Ok(all.get_mut(&id).map(|u| {
            u.status = status;
            u.error = error.map(str::to_owned);
            u.warnings = warnings.to_vec();
            u.page_count = page_count;
            u.metadata = metadata.clone();
            u.clone()
        }))
    }
    async fn set_chapter_source(&self, _: i64, _: &str, _: i32) -> LibraryResult<bool> {
        Ok(true)
    }
    async fn set_multipart(&self, upload: i64, id: &str, part_size: u64) -> LibraryResult<()> {
        self.multipart
            .lock()
            .unwrap()
            .insert(upload, (id.to_owned(), part_size));
        Ok(())
    }
    async fn get_multipart(&self, upload: i64) -> LibraryResult<Option<(String, u64)>> {
        Ok(self.multipart.lock().unwrap().get(&upload).cloned())
    }
    async fn expire_uploads(&self, _: i64, _: u32) -> LibraryResult<Vec<UploadRecord>> {
        Ok(vec![])
    }
    async fn queue_gc(&self, keys: &[String], _: Option<i64>) -> LibraryResult<()> {
        self.gc.lock().unwrap().extend_from_slice(keys);
        Ok(())
    }
    async fn gc_due(&self, _: i64, _: u32) -> LibraryResult<Vec<String>> {
        Ok(vec![])
    }
    async fn gc_done(&self, _: &[String]) -> LibraryResult<u64> {
        Ok(0)
    }
}

struct Env {
    app: Router,
    stub: Arc<Stub>,
    admin: Arc<MemoryAdminRepo>,
    age: Arc<Confirmed>,
    tokens: HashMap<i64, String>,
    storage: Arc<sr_storage::ObjectStoreStorage>,
    queue: Arc<RecordingQueue>,
    parts: Arc<sr_library::MemoryPartStore>,
}

/// Remembers which uploads were queued for conversion.
#[derive(Default)]
struct RecordingQueue(Mutex<Vec<i64>>);

#[async_trait]
impl sr_library::IngestQueue for RecordingQueue {
    async fn enqueue(&self, upload_id: i64) -> Result<(), String> {
        self.0.lock().unwrap().push(upload_id);
        Ok(())
    }
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
    let storage = Arc::new(sr_storage::ObjectStoreStorage::in_memory());
    let queue = Arc::new(RecordingQueue::default());
    let parts = Arc::new(sr_library::MemoryPartStore::new(storage.clone()));
    let service = LibraryService::new(
        stub.clone(),
        CdnSigner::new(vec![3u8; 32]),
        Arc::new(SystemClock),
        Arc::new(|| 4242),
    )
    .with_index(Arc::new(SeriesIndex::in_ram().unwrap()))
    .with_ingest(
        storage.clone(),
        Arc::new(sr_library::StaticIngestSettings(
            sr_library::IngestSettingsSnapshot::default(),
        )),
        queue.clone(),
    )
    .with_multipart(sr_library::MultipartWiring::new(parts.clone()).with_part_size(5));
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
        storage,
        queue,
        parts,
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

fn new_member_routes() -> Vec<(Method, &'static str, Option<Value>)> {
    vec![
        (Method::GET, "/library/chapters/70/progress", None),
        (
            Method::PUT,
            "/library/chapters/70/progress",
            Some(json!({ "page_index": 0, "percent": 1 })),
        ),
        (Method::DELETE, "/library/chapters/70/progress", None),
        (
            Method::POST,
            "/library/chapters/70/read",
            Some(json!({ "read": true })),
        ),
        (Method::GET, "/library/me/progress", None),
        (Method::GET, "/library/chapters/70/bookmarks", None),
        (
            Method::PUT,
            "/library/chapters/70/bookmarks",
            Some(json!({ "page_index": 0 })),
        ),
        (Method::DELETE, "/library/bookmarks/5", None),
        (Method::GET, "/library/me/bookmarks", None),
        (Method::GET, "/library/series/70/related", None),
        (Method::GET, "/library/series/70/contributors", None),
        (Method::GET, "/library/recommendations", None),
        (Method::GET, "/library/series/70/comments", None),
        (
            Method::POST,
            "/library/series/70/comments",
            Some(json!({ "body": "hi" })),
        ),
        (Method::GET, "/library/chapters/70/comments", None),
        (
            Method::POST,
            "/library/chapters/70/comments",
            Some(json!({ "body": "hi" })),
        ),
        (Method::GET, "/library/comments/5/replies", None),
        (
            Method::PATCH,
            "/library/comments/5",
            Some(json!({ "body": "x" })),
        ),
        (Method::DELETE, "/library/comments/5", None),
        (Method::PUT, "/library/comments/5/like", None),
        (Method::DELETE, "/library/comments/5/like", None),
        (
            Method::POST,
            "/library/comments/5/report",
            Some(json!({ "reason": "spam" })),
        ),
        (Method::GET, "/library/me/downloads", None),
        (Method::POST, "/library/chapters/70/download", None),
        (Method::PUT, "/library/chapters/70/presence", None),
        (Method::DELETE, "/library/me/presence", None),
        (Method::GET, "/library/friends/reading", None),
        (Method::GET, "/library/me/notifications", None),
        (Method::POST, "/library/me/notifications/seen", None),
    ]
}

fn new_admin_routes() -> Vec<(Method, &'static str, Option<Value>)> {
    vec![
        (
            Method::POST,
            "/admin/library/chapters/70/uploads",
            Some(json!({ "filename": "a.cbz", "size_bytes": 10 })),
        ),
        (Method::GET, "/admin/library/chapters/70/uploads", None),
        (Method::GET, "/admin/library/uploads/5", None),
        (Method::POST, "/admin/library/uploads/5/complete", None),
        (Method::GET, "/admin/library/comment-reports", None),
        (
            Method::POST,
            "/admin/library/comment-reports/5/resolve",
            Some(json!({ "hide": true })),
        ),
    ]
}

#[tokio::test]
async fn the_new_routes_keep_the_auth_matrices() {
    let e = env().await;
    for (m, p, b) in new_member_routes() {
        let (s, _) = e.call(None, m.clone(), p, b.clone()).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{m} {p}");
    }
    for (m, p, b) in new_admin_routes() {
        let (s, _) = e.call(None, m.clone(), p, b.clone()).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "anonymous {m} {p}");
        let (s, body) = e.call(Some(MEMBER), m.clone(), p, b.clone()).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "member {m} {p}");
        assert_eq!(body["code"], 900002, "{m} {p}");
        let (s, body) = e.call(Some(BASIC), m.clone(), p, b.clone()).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "basic {m} {p}");
        assert_eq!(body["code"], 50013, "{m} {p}");
    }
}

#[tokio::test]
async fn downloads_are_for_patrons_server_side() {
    // Nobody holds the download perk in this environment, so the answer is 403 before anything is built.
    let e = env().await;
    let (s, body) = e
        .call(
            Some(MEMBER),
            Method::POST,
            "/library/chapters/70/download",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], 50013);
}

#[tokio::test]
async fn progress_round_trip_with_conflict() {
    let e = env().await;
    let path = "/library/chapters/70/progress";
    let (s, _) = e.call(Some(MEMBER), Method::GET, path, None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = e.call(Some(MEMBER), Method::DELETE, path, None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    let (s, body) = e
        .call(
            Some(MEMBER),
            Method::PUT,
            path,
            Some(json!({ "page_index": 2, "percent": 33.5, "base_updated_at_ms": 0 })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["applied"], true);
    assert_eq!(body["progress"]["chapter_id"], "70");
    assert_eq!(body["progress"]["series_id"], "7");
    let at = body["progress"]["updated_at_ms"].as_i64().unwrap();

    // Another device still holds an older base: the stored position wins, and says so.
    let (s, body) = e
        .call(
            Some(MEMBER),
            Method::PUT,
            path,
            Some(json!({ "page_index": 0, "percent": 0, "base_updated_at_ms": at - 1 })),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["applied"], false);
    assert_eq!(body["progress"]["page_index"], 2);
    // `force` overwrites.
    let (_, body) = e
        .call(
            Some(MEMBER),
            Method::PUT,
            path,
            Some(json!({ "page_index": 0, "percent": 0, "base_updated_at_ms": at - 1, "force": true })),
        )
        .await;
    assert_eq!(body["applied"], true);
    assert_eq!(body["progress"]["page_index"], 0);

    // Progress is per account.
    let (s, _) = e.call(Some(ADULT), Method::GET, path, None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, body) = e.call(Some(MEMBER), Method::GET, path, None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["percent"], 0.0);

    // Bad bodies are client errors, never 500s.
    for b in [
        json!({ "page_index": -1, "percent": 1 }),
        json!({ "page_index": 0, "percent": 101 }),
        json!({ "page_index": 0, "percent": 5, "cfi": "x".repeat(3000) }),
        json!({ "percent": 5 }),
    ] {
        let (s, _) = e
            .call(Some(MEMBER), Method::PUT, path, Some(b.clone()))
            .await;
        assert!(s.is_client_error(), "{b}: {s}");
    }
    let (s, _) = e.call(Some(MEMBER), Method::DELETE, path, None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    // Unknown chapter, malformed id.
    for p in [
        "/library/chapters/5/progress",
        "/library/chapters/abc/progress",
    ] {
        let (s, _) = e.call(Some(MEMBER), Method::GET, p, None).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{p}");
    }
}

#[tokio::test]
async fn reading_state_of_a_restricted_chapter_needs_age_confirmation() {
    let e = env().await;
    for (m, p, b) in [
        (
            Method::PUT,
            "/library/chapters/90/progress",
            Some(json!({ "page_index": 0, "percent": 1 })),
        ),
        (
            Method::PUT,
            "/library/chapters/90/bookmarks",
            Some(json!({ "page_index": 0 })),
        ),
        (Method::GET, "/library/chapters/90/bookmarks", None),
        (
            Method::POST,
            "/library/chapters/90/read",
            Some(json!({ "read": true })),
        ),
    ] {
        let (s, body) = e.call(Some(MEMBER), m.clone(), p, b.clone()).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{m} {p}");
        assert_eq!(body["code"], 900011, "{m} {p}");
        let (s, body) = e.call(Some(ADULT), m.clone(), p, b.clone()).await;
        assert_eq!(s, StatusCode::OK, "adult {m} {p}: {body}");
    }
}

#[tokio::test]
async fn read_marks_history_and_bookmarks_round_trip() {
    let e = env().await;
    let (s, body) = e
        .call(
            Some(MEMBER),
            Method::POST,
            "/library/chapters/70/read",
            Some(json!({ "read": true, "up_to": true })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["changed"], 3);
    let (s, body) = e
        .call(Some(MEMBER), Method::GET, "/library/me/progress", None)
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["items"], json!([]));
    let (s, _) = e
        .call(
            Some(MEMBER),
            Method::GET,
            "/library/me/progress?before=zzz",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    let (s, body) = e
        .call(
            Some(MEMBER),
            Method::PUT,
            "/library/chapters/70/bookmarks",
            Some(json!({ "page_index": 3, "note": "  fav " })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["id"], "4242");
    assert_eq!(body["note"], "fav");
    assert_eq!(body["chapter_id"], "70");
    let (_, list) = e
        .call(
            Some(MEMBER),
            Method::GET,
            "/library/chapters/70/bookmarks",
            None,
        )
        .await;
    assert_eq!(list["bookmarks"].as_array().unwrap().len(), 1);
    let (_, other) = e
        .call(
            Some(ADULT),
            Method::GET,
            "/library/chapters/70/bookmarks",
            None,
        )
        .await;
    assert_eq!(other["bookmarks"], json!([]));
    // A bookmark belongs to its owner.
    let (s, _) = e
        .call(Some(ADULT), Method::DELETE, "/library/bookmarks/4242", None)
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = e
        .call(
            Some(MEMBER),
            Method::DELETE,
            "/library/bookmarks/4242",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = e
        .call(
            Some(MEMBER),
            Method::PUT,
            "/library/chapters/70/bookmarks",
            Some(json!({ "page_index": 0, "note": "x".repeat(1001) })),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, body) = e
        .call(Some(MEMBER), Method::GET, "/library/me/bookmarks", None)
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["items"], json!([]));
}

#[tokio::test]
async fn chapter_list_paging_parameters_are_checked() {
    let e = env().await;
    for p in [
        "/library/series/7/chapters?after=garbage",
        "/library/series/7/chapters?order=sideways",
        "/library/series/7/chapters?limit=2&after=1_2",
    ] {
        let (s, _) = e.call(Some(MEMBER), Method::GET, p, None).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{p}");
    }
    let (s, body) = e
        .call(
            Some(MEMBER),
            Method::GET,
            "/library/series/7/chapters?limit=50&after=1_100_9&order=desc",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert!(body["next_cursor"].is_null());
    let (s, _) = e
        .call(
            Some(CURATOR),
            Method::GET,
            "/admin/library/series/7/chapters?limit=5&order=asc",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test]
async fn the_upload_flow_is_logged_and_validated() {
    use sr_storage::ObjectStorage;
    let e = env().await;
    let begin = |name: &str, size: u64| json!({ "filename": name, "size_bytes": size });
    let uploads = "/admin/library/chapters/70/uploads";
    // Wrong extension for an image chapter, empty and oversized files: 400s.
    for b in [
        begin("a.pdf", 10),
        begin("a.cbz", 0),
        begin("a.cbz", 600 * 1024 * 1024),
    ] {
        let (s, _) = e
            .call(Some(CURATOR), Method::POST, uploads, Some(b.clone()))
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");
    }
    let (s, _) = e
        .call(
            Some(CURATOR),
            Method::POST,
            "/admin/library/chapters/5/uploads",
            Some(begin("a.cbz", 10)),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    let (s, slot) = e
        .call(
            Some(CURATOR),
            Method::POST,
            uploads,
            Some(begin("Ch 1.cbz", 12)),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{slot}");
    assert_eq!(slot["upload"]["id"], "4242");
    assert_eq!(slot["upload"]["status"], "awaiting_upload");
    assert_eq!(slot["upload_method"], "PUT");
    assert_eq!(slot["max_bytes"], 512 * 1024 * 1024);
    let url = slot["upload_url"].as_str().unwrap();
    assert!(url.contains("library/7/70/src-4242.cbz"), "{url}");

    // Completing before the bytes arrive is a 400 with a reason.
    let complete = "/admin/library/uploads/4242/complete";
    let (s, body) = e.call(Some(CURATOR), Method::POST, complete, None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{body}");
    e.storage
        .put(
            "library/7/70/src-4242.cbz",
            bytes::Bytes::from_static(b"hello world!"),
        )
        .await
        .unwrap();
    let (s, body) = e.call(Some(CURATOR), Method::POST, complete, None).await;
    assert_eq!(s, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body["status"], "queued");
    assert_eq!(e.queue.0.lock().unwrap().as_slice(), [4242]);
    // Completing twice is harmless and does not queue twice.
    let (s, _) = e.call(Some(CURATOR), Method::POST, complete, None).await;
    assert_eq!(s, StatusCode::ACCEPTED);
    assert_eq!(e.queue.0.lock().unwrap().len(), 1);

    let (s, body) = e
        .call(
            Some(CURATOR),
            Method::GET,
            "/admin/library/uploads/4242",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["filename"], "Ch 1.cbz");
    let (s, list) = e.call(Some(CURATOR), Method::GET, uploads, None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(list["uploads"].as_array().unwrap().len(), 1);
    let (s, _) = e
        .call(
            Some(CURATOR),
            Method::GET,
            "/admin/library/uploads/999",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    let actions: Vec<String> = e
        .admin
        .log_rows()
        .iter()
        .map(|r| r.action.clone())
        .collect();
    assert!(
        actions.contains(&"library.upload.begin".to_owned()),
        "{actions:?}"
    );
    assert!(
        actions.contains(&"library.upload.complete".to_owned()),
        "{actions:?}"
    );
}

#[test]
fn openapi_lists_the_phase_4_paths() {
    let doc = serde_json::to_value(super::openapi()).unwrap();
    let paths = doc["paths"].as_object().unwrap();
    for p in [
        "/library/chapters/{chapter_id}/progress",
        "/library/chapters/{chapter_id}/read",
        "/library/chapters/{chapter_id}/bookmarks",
        "/library/bookmarks/{bookmark_id}",
        "/library/me/progress",
        "/library/me/bookmarks",
        "/admin/library/chapters/{chapter_id}/uploads",
        "/admin/library/uploads/{upload_id}",
        "/admin/library/uploads/{upload_id}/complete",
    ] {
        assert!(paths.contains_key(p), "{p}");
    }
    for s in [
        "ProgressSaved",
        "ReadingBookmark",
        "LibraryUploadSlot",
        "Upload",
        "HistoryPage",
    ] {
        assert!(doc["components"]["schemas"][s].is_object(), "{s}");
    }
    assert!(doc["components"]["schemas"]["ChapterList"]["properties"]["next_cursor"].is_object());
}

#[tokio::test]
async fn a_multipart_upload_resumes_and_completes_from_the_stored_parts() {
    use sr_storage::ObjectStorage;
    let e = env().await;
    let uploads = "/admin/library/chapters/70/uploads";
    // 12 bytes in parts of 5: three parts (5 + 5 + 2).
    let begin = json!({ "filename": "Ch 1.cbz", "size_bytes": 12, "multipart": true });
    let (s, slot) = e
        .call(Some(CURATOR), Method::POST, uploads, Some(begin))
        .await;
    assert_eq!(s, StatusCode::CREATED, "{slot}");
    assert_eq!(slot["upload_url"], "");
    assert_eq!(slot["multipart"]["part_size"], 5);
    assert_eq!(slot["multipart"]["part_count"], 3);
    let urls = slot["multipart"]["parts"].as_array().unwrap();
    assert_eq!(urls.len(), 3);
    assert!(urls[0]["url"].as_str().unwrap().contains("partNumber=1"));

    // The browser sent parts 1 and 3, then the page was refreshed: resume asks what is missing.
    e.parts
        .put_part("mem-1", 1, bytes::Bytes::from_static(b"hello"));
    e.parts
        .put_part("mem-1", 3, bytes::Bytes::from_static(b"!!"));
    let parts_path = "/admin/library/uploads/4242/parts";
    let (s, got) = e.call(Some(CURATOR), Method::GET, parts_path, None).await;
    assert_eq!(s, StatusCode::OK, "{got}");
    assert_eq!(got["received"].as_array().unwrap().len(), 2);
    let missing = got["missing"].as_array().unwrap();
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0]["part_number"], 2);

    // Completing now names the missing part and does not queue the conversion.
    let complete = "/admin/library/uploads/4242/complete";
    let (s, body) = e.call(Some(CURATOR), Method::POST, complete, None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["message"].as_str().unwrap().contains("part 2"),
        "{body}"
    );
    assert!(e.queue.0.lock().unwrap().is_empty());

    // A short middle part is refused too.
    e.parts
        .put_part("mem-1", 2, bytes::Bytes::from_static(b"wor"));
    let (s, got) = e.call(Some(CURATOR), Method::GET, parts_path, None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(got["missing"][0]["part_number"], 2, "{got}");
    let (s, _) = e.call(Some(CURATOR), Method::POST, complete, None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    e.parts
        .put_part("mem-1", 2, bytes::Bytes::from_static(b" wor1"));
    let (s, body) = e.call(Some(CURATOR), Method::POST, complete, None).await;
    assert_eq!(s, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body["status"], "queued");
    let stored = e.storage.get("library/7/70/src-4242.cbz").await.unwrap();
    assert_eq!(&stored[..], b"hello wor1!!");
    // Once queued the parts route and the cancel route refuse.
    let (s, _) = e.call(Some(CURATOR), Method::GET, parts_path, None).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, _) = e
        .call(
            Some(CURATOR),
            Method::DELETE,
            "/admin/library/uploads/4242",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
}

#[tokio::test]
async fn cancelling_a_multipart_upload_frees_its_parts() {
    let e = env().await;
    let begin = json!({ "filename": "Ch 1.cbz", "size_bytes": 12, "multipart": true });
    let (s, _) = e
        .call(
            Some(CURATOR),
            Method::POST,
            "/admin/library/chapters/70/uploads",
            Some(begin),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, body) = e
        .call(
            Some(CURATOR),
            Method::DELETE,
            "/admin/library/uploads/4242",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "expired");
    assert_eq!(e.parts.aborted(), vec!["mem-1".to_owned()]);
    // Not a resumable upload, and not a publisher: nothing to resume.
    let (s, _) = e
        .call(
            Some(MEMBER),
            Method::GET,
            "/admin/library/uploads/4242/parts",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_plain_upload_has_no_parts_to_resume() {
    let e = env().await;
    // The plain single-PUT slot still works and carries no multipart block.
    let (s, slot) = e
        .call(
            Some(CURATOR),
            Method::POST,
            "/admin/library/chapters/70/uploads",
            Some(json!({ "filename": "Ch 1.cbz", "size_bytes": 12 })),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    assert!(slot["multipart"].is_null());
    let (s, _) = e
        .call(
            Some(CURATOR),
            Method::GET,
            "/admin/library/uploads/4242/parts",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
}

#[tokio::test]
async fn an_uploaded_archive_can_be_listed_before_it_is_converted() {
    use sr_storage::ObjectStorage;
    use std::io::Write;
    let e = env().await;
    let png = {
        let img = image::RgbImage::from_pixel(4, 4, image::Rgb([9, 9, 9]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    };
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for name in ["Ch 1/001.png", "Ch 1/002.png", "Ch 2/001.png"] {
        w.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        w.write_all(&png).unwrap();
    }
    let bytes = w.finish().unwrap().into_inner();
    let (s, _) = e
        .call(
            Some(CURATOR),
            Method::POST,
            "/admin/library/chapters/70/uploads",
            Some(json!({ "filename": "Ch 1.cbz", "size_bytes": bytes.len() })),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    let inspect = "/admin/library/uploads/4242/inspect";
    // Nothing uploaded yet.
    let (s, _) = e.call(Some(CURATOR), Method::GET, inspect, None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    e.storage
        .put("library/7/70/src-4242.cbz", bytes::Bytes::from(bytes))
        .await
        .unwrap();
    let (s, r) = e.call(Some(CURATOR), Method::GET, inspect, None).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["format"], "zip");
    assert_eq!(r["pages"], 3);
    assert_eq!(r["folders"][0]["path"], "Ch 1");
    assert_eq!(r["folders"][0]["pages"], 2);
    assert_eq!(r["folders"][1]["pages"], 1);
    // Members without the publisher permission are refused.
    let (s, _) = e.call(Some(MEMBER), Method::GET, inspect, None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn ingest_source_routes_need_staff_and_a_scanner() {
    let e = env().await;
    let paths = [
        (Method::GET, "/admin/ingest-sources"),
        (Method::POST, "/admin/ingest-sources/7/dry-run"),
        (Method::POST, "/admin/ingest-sources/7/scan"),
        (Method::DELETE, "/admin/ingest-sources/7"),
    ];
    for (m, p) in paths {
        let (s, _) = e.call(None, m.clone(), p, None).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{m} {p}");
        let (s, _) = e.call(Some(MEMBER), m.clone(), p, None).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{m} {p}");
        // A curator is allowed in, but this server has no scanner configured.
        let (s, b) = e.call(Some(CURATOR), m.clone(), p, None).await;
        assert_eq!(s, StatusCode::CONFLICT, "{m} {p}: {b}");
    }
}
