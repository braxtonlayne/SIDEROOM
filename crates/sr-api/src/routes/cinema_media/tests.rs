//! Endpoint tests of the media router over a stub repository: authentication, the permission
//! split between uploads (`PUBLISH_MEDIA`) and the transcoding admin (`MANAGE_TRANSCODE`), the
//! Patron gate on downloads, the staff access log, token secrets and the OpenAPI document.
//! Behaviour against real tables is covered by `sr-db/tests/media_repo.rs`.

use super::{MediaDeps, router};
use crate::{
    ApiSettings, AppState,
    auth::{
        csrf,
        memory::{MemoryBots, MemorySessions},
    },
    build_router,
    routes::{
        admin::AdminDeps,
        cinema::{CinemaDeps, NoCinemaPrefs},
    },
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
use sr_cinema::{CinemaService, TitleIndex};
use sr_core::{PlatformPermissions as P, SystemClock};
use sr_media::assets::*;
use sr_media::downloads::*;
use sr_media::output_storage::SinglePutStorage;
use sr_media::runner::FakeRunner;
use sr_media::service::*;
use sr_media::worker_api::*;
use sr_storage::{ObjectStoreStorage, cdn::CdnSigner};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tower::ServiceExt;

const OWNER: i64 = 10;
const PUBLISHER: i64 = 11;
const TRANSCODER: i64 = 12;
const BASIC: i64 = 13;
const MEMBER: i64 = 20;
const PATRON: i64 = 21;

fn unsupported<T>() -> Result<T, MediaError> {
    Err(MediaError::Backend("not stubbed".into()))
}

fn store_unsupported<T>() -> Result<T, StoreError> {
    Err(StoreError::Backend("not stubbed".into()))
}

/// Remembers tokens; everything else is empty or unsupported.
#[derive(Default)]
struct Stub {
    tokens: Mutex<Vec<(WorkerToken, String)>>,
}

#[async_trait]
impl JobStore for Stub {
    async fn authenticate(&self, _: &str) -> Result<Option<TokenInfo>, StoreError> {
        Ok(None)
    }
    async fn upsert_worker(
        &self,
        _: &TokenInfo,
        _: &RegisterRequest,
        _: u64,
    ) -> Result<WorkerRecord, StoreError> {
        store_unsupported()
    }
    async fn get_worker(&self, _: &str) -> Result<Option<WorkerRecord>, StoreError> {
        Ok(None)
    }
    async fn touch_worker(&self, _: &str, _: u64) -> Result<(), StoreError> {
        store_unsupported()
    }
    async fn cancel_requests_for(&self, _: &str) -> Result<Vec<String>, StoreError> {
        Ok(vec![])
    }
    async fn enqueue(&self, _: NewJob) -> Result<String, StoreError> {
        store_unsupported()
    }
    async fn claim_next(
        &self,
        _: &str,
        _: &[JobKind],
        _: u64,
        _: u64,
    ) -> Result<Option<JobRecord>, StoreError> {
        Ok(None)
    }
    async fn get_job(&self, _: &str) -> Result<Option<JobRecord>, StoreError> {
        Ok(None)
    }
    async fn report_progress(
        &self,
        _: &str,
        _: &str,
        _: &ProgressRequest,
        _: u64,
        _: u64,
    ) -> Result<JobRecord, StoreError> {
        store_unsupported()
    }
    async fn add_upload(&self, _: &str, _: UploadRecord) -> Result<(), StoreError> {
        store_unsupported()
    }
    async fn finish_upload(&self, _: &str, _: &str) -> Result<UploadRecord, StoreError> {
        store_unsupported()
    }
    async fn uploads(&self, _: &str) -> Result<Vec<UploadRecord>, StoreError> {
        Ok(vec![])
    }
    async fn complete_job(&self, _: &str, _: &str, _: &CompleteRequest) -> Result<(), StoreError> {
        store_unsupported()
    }
    async fn fail_job(
        &self,
        _: &str,
        _: &str,
        _: &FailRequest,
        _: u32,
    ) -> Result<FailOutcome, StoreError> {
        store_unsupported()
    }
    async fn request_cancel(&self, _: &str) -> Result<(), StoreError> {
        Err(StoreError::NotFound)
    }
    async fn reap_expired(&self, _: u64, _: u32) -> Result<Vec<String>, StoreError> {
        Ok(vec![])
    }
}

#[async_trait]
impl MediaRepository for Stub {
    async fn create_asset(&self, _: &NewAsset) -> MediaResult<Asset> {
        unsupported()
    }
    async fn get_asset(&self, _: i64) -> MediaResult<Option<Asset>> {
        Ok(None)
    }
    async fn list_assets(&self, _: &AssetQuery) -> MediaResult<Vec<Asset>> {
        Ok(vec![])
    }
    async fn transition_asset(
        &self,
        _: i64,
        _: &[AssetState],
        _: AssetState,
        _: Option<&str>,
    ) -> MediaResult<Option<Asset>> {
        Ok(None)
    }
    async fn mark_uploaded(&self, _: i64, _: i64) -> MediaResult<Option<Asset>> {
        Ok(None)
    }
    async fn record_probe(&self, _: i64, _: &Value, _: i64) -> MediaResult<()> {
        unsupported()
    }
    async fn asset_probe(&self, _: i64) -> MediaResult<Option<Value>> {
        Ok(None)
    }
    async fn link_asset(
        &self,
        _: i64,
        _: Option<i64>,
        _: Option<i64>,
    ) -> MediaResult<Option<Asset>> {
        Ok(None)
    }
    async fn set_original_downloads(&self, _: i64, _: bool) -> MediaResult<Option<Asset>> {
        Ok(None)
    }
    async fn expire_slots(&self, _: i64, _: u32) -> MediaResult<Vec<Asset>> {
        Ok(vec![])
    }
    async fn list_jobs(&self, _: &JobQuery) -> MediaResult<Vec<JobSummary>> {
        Ok(vec![])
    }
    async fn job_detail(&self, _: i64) -> MediaResult<Option<JobDetail>> {
        Ok(None)
    }
    async fn retry_job(&self, _: i64) -> MediaResult<bool> {
        Ok(false)
    }
    async fn queue_counts(&self) -> MediaResult<QueueCounts> {
        Ok(QueueCounts {
            queued: 2,
            ..QueueCounts::default()
        })
    }
    async fn create_token(
        &self,
        id: i64,
        name: &str,
        sha256: &str,
        created_by: i64,
    ) -> MediaResult<WorkerToken> {
        let t = WorkerToken {
            id,
            name: name.to_owned(),
            created_by: Some(created_by),
            created_at_ms: 1,
            last_used_at_ms: None,
            revoked_at_ms: None,
        };
        self.tokens
            .lock()
            .unwrap()
            .push((t.clone(), sha256.to_owned()));
        Ok(t)
    }
    async fn list_tokens(&self) -> MediaResult<Vec<WorkerToken>> {
        Ok(self
            .tokens
            .lock()
            .unwrap()
            .iter()
            .map(|(t, _)| t.clone())
            .collect())
    }
    async fn revoke_token(&self, _: i64) -> MediaResult<bool> {
        Ok(false)
    }
    async fn list_workers(&self) -> MediaResult<Vec<WorkerView>> {
        Ok(vec![])
    }
    async fn set_worker_drain(&self, _: &str, _: bool) -> MediaResult<bool> {
        Ok(false)
    }
    async fn playback(&self, _: i64) -> MediaResult<Option<AssetPlayback>> {
        Ok(None)
    }
    async fn playback_by_prefix(&self, _: &str) -> MediaResult<Option<AssetPlayback>> {
        Ok(None)
    }
    async fn retention_facts(&self, _: u32) -> MediaResult<Vec<sr_media::retention::AssetFacts>> {
        Ok(vec![])
    }
    async fn retranscode_summaries(
        &self,
        _: Option<i64>,
        _: Option<i64>,
        _: u32,
    ) -> MediaResult<Vec<sr_media::retranscode::AssetSummary>> {
        Ok(vec![])
    }
    async fn mark_original_deleted(&self, _: i64) -> MediaResult<()> {
        Ok(())
    }
    async fn mark_proxy_deleted(&self, _: i64) -> MediaResult<()> {
        Ok(())
    }
    async fn mark_cold(&self, _: i64) -> MediaResult<()> {
        Ok(())
    }
    async fn queue_gc(&self, _: &[String], _: Option<i64>) -> MediaResult<()> {
        Ok(())
    }
    async fn gc_due(&self, _: i64, _: u32) -> MediaResult<Vec<String>> {
        Ok(vec![])
    }
    async fn gc_done(&self, _: &[String]) -> MediaResult<()> {
        Ok(())
    }
}

#[async_trait]
impl DownloadRepository for Stub {
    async fn count_since(&self, _: i64, _: i64) -> MediaResult<u32> {
        Ok(0)
    }
    async fn renditions(&self, _: i64) -> MediaResult<Vec<StoredRendition>> {
        Ok(vec![])
    }
    async fn remux_request(
        &self,
        _: i64,
        _: &str,
    ) -> MediaResult<Option<sr_media::remux::RemuxRequest>> {
        Ok(None)
    }
    async fn live_cache(&self, _: i64, _: &str) -> MediaResult<Option<CacheEntry>> {
        Ok(None)
    }
    async fn create_cache(
        &self,
        _: i64,
        _: i64,
        _: &str,
        _: NewJob,
    ) -> MediaResult<Option<CacheEntry>> {
        Ok(None)
    }
    async fn create_download(&self, _: &DownloadRow) -> MediaResult<()> {
        unsupported()
    }
    async fn download(&self, _: i64) -> MediaResult<Option<(DownloadRow, Option<CacheEntry>)>> {
        Ok(None)
    }
    async fn expire_cache(&self, _: i64, _: u32) -> MediaResult<Vec<CacheEntry>> {
        Ok(vec![])
    }
}

struct NoQueue;

#[async_trait]
impl ProbeQueue for NoQueue {
    async fn enqueue_probe(&self, _: i64) -> Result<(), String> {
        Ok(())
    }
}

/// Only the Patron may download.
struct OnlyPatron;

#[async_trait]
impl DownloadEntitlements for OnlyPatron {
    async fn allowance(&self, user: i64) -> MediaResult<Option<Allowance>> {
        Ok((user == PATRON).then_some(Allowance { per_day: 3 }))
    }
}

struct Env {
    app: Router,
    admin: Arc<MemoryAdminRepo>,
    stub: Arc<Stub>,
    tokens: HashMap<i64, String>,
}

async fn env_with(media: bool) -> Env {
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
        (PUBLISHER, "publisher"),
        (TRANSCODER, "transcoder"),
        (BASIC, "basic"),
        (MEMBER, "member"),
        (PATRON, "patron"),
    ] {
        admin.add_account(id, name, AccountState::Active, None, Some(OWNER));
    }
    admin.add_role(1, "Owner", 3, P::OWNER);
    admin.add_role(2, "Publisher", 2, P::STAFF_BASIC | P::PUBLISH_MEDIA);
    admin.add_role(3, "Transcode", 2, P::STAFF_BASIC | P::MANAGE_TRANSCODE);
    admin.add_role(4, "Basic", 1, P::STAFF_BASIC);
    admin.set_owner(OWNER);
    let svc = AdminService::new(admin.clone());
    let op = Caller::operator(SYSTEM_USER_ID);
    for (u, r) in [(PUBLISHER, 2), (TRANSCODER, 3), (BASIC, 4)] {
        svc.appoint(&op, u, &[r], Some("test")).await.unwrap();
    }
    let sessions = Arc::new(MemorySessions::new());
    let mut tokens = HashMap::new();
    for (n, uid) in [OWNER, PUBLISHER, TRANSCODER, BASIC, MEMBER, PATRON]
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
    let stub = Arc::new(Stub::default());
    let storage = Arc::new(ObjectStoreStorage::in_memory());
    let ids = Arc::new(Mutex::new(1000i64));
    let id_src: IdSource = {
        let ids = ids.clone();
        Arc::new(move || {
            let mut g = ids.lock().unwrap();
            *g += 1;
            *g
        })
    };
    let media_svc = MediaService::new(
        stub.clone(),
        storage.clone(),
        Arc::new(SinglePutStorage::new(storage.clone())),
        Arc::new(FakeRunner::fixed("")),
        Arc::new(StaticMediaSettings(MediaSettings::default())),
        Arc::new(NoQueue),
        id_src.clone(),
        Arc::new(|| 1_700_000_000),
        Arc::new(|| Some([9u8; 32])),
    );
    let downloads = DownloadService::new(
        stub.clone(),
        stub.clone(),
        Arc::new(OnlyPatron),
        Arc::new(CdnSigner::new(vec![3u8; 32])),
        storage.clone(),
        id_src,
        Arc::new(|| 1_700_000_000),
    );
    let cinema = CinemaDeps::new(
        CinemaService::new(
            Arc::new(crate::routes::cinema::tests::Stub::default()),
            CdnSigner::new(vec![3u8; 32]),
            Arc::new(SystemClock),
            Arc::new(|| 4242),
        )
        .with_index(Arc::new(TitleIndex::in_ram().unwrap())),
    )
    .with_prefs(Arc::new(NoCinemaPrefs));
    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let mut b = AppState::builder()
        .settings(settings)
        .sessions(sessions)
        .bots(Arc::new(MemoryBots::new()))
        .extension(Arc::new(AdminDeps::new(svc)))
        .extension(Arc::new(cinema));
    if media {
        b = b.extension(Arc::new(MediaDeps::new(media_svc, Arc::new(downloads))));
    }
    Env {
        app: build_router(b.build(), router()),
        admin,
        stub,
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

type Route = (Method, &'static str, Option<Value>);

fn asset_routes() -> Vec<Route> {
    vec![
        (Method::GET, "/admin/cinema/assets", None),
        (
            Method::POST,
            "/admin/cinema/assets",
            Some(json!({ "filename": "a.mkv", "size_bytes": 5 })),
        ),
        (Method::GET, "/admin/cinema/assets/7", None),
        (
            Method::POST,
            "/admin/cinema/assets/7/complete",
            Some(json!({ "upload_id": "u", "parts": [] })),
        ),
        (Method::POST, "/admin/cinema/assets/7/probe", None),
        (Method::POST, "/admin/cinema/assets/7/transcode", None),
        (
            Method::PUT,
            "/admin/cinema/assets/7/link",
            Some(json!({ "title_id": "3" })),
        ),
        (
            Method::PUT,
            "/admin/cinema/assets/7/original-downloads",
            Some(json!({ "enabled": true })),
        ),
    ]
}

fn transcode_routes() -> Vec<Route> {
    vec![
        (Method::GET, "/admin/transcode/tokens", None),
        (
            Method::POST,
            "/admin/transcode/tokens",
            Some(json!({ "name": "gpu" })),
        ),
        (Method::DELETE, "/admin/transcode/tokens/5", None),
        (Method::GET, "/admin/transcode/workers", None),
        (
            Method::PUT,
            "/admin/transcode/workers/w1/drain",
            Some(json!({ "drain": true })),
        ),
        (Method::GET, "/admin/transcode/queue", None),
        (Method::GET, "/admin/transcode/jobs", None),
        (Method::GET, "/admin/transcode/jobs/5", None),
        (Method::POST, "/admin/transcode/jobs/5/cancel", None),
        (Method::POST, "/admin/transcode/jobs/5/retry", None),
        (
            Method::POST,
            "/admin/transcode/retranscode",
            Some(json!({ "scope": "all" })),
        ),
        (Method::POST, "/admin/transcode/sweep", None),
    ]
}

fn download_routes() -> Vec<Route> {
    vec![
        (Method::GET, "/cinema/titles/7/downloads", None),
        (
            Method::POST,
            "/cinema/titles/7/downloads",
            Some(json!({ "rendition_id": "1080p" })),
        ),
        (Method::GET, "/cinema/downloads/5", None),
        (Method::GET, "/cinema/downloads/5/file", None),
    ]
}

#[tokio::test]
async fn staff_routes_need_the_right_permission() {
    let e = env_with(true).await;
    for (routes, allowed, denied) in [
        (asset_routes(), PUBLISHER, TRANSCODER),
        (transcode_routes(), TRANSCODER, PUBLISHER),
    ] {
        for (m, p, b) in routes {
            let (s, _) = e.call(None, m.clone(), p, b.clone()).await;
            assert_eq!(s, StatusCode::UNAUTHORIZED, "anonymous {m} {p}");
            let (s, body) = e.call(Some(MEMBER), m.clone(), p, b.clone()).await;
            assert_eq!(
                (s, &body["code"]),
                (StatusCode::FORBIDDEN, &json!(900002)),
                "member {m} {p}"
            );
            for who in [BASIC, denied] {
                let (s, body) = e.call(Some(who), m.clone(), p, b.clone()).await;
                assert_eq!(s, StatusCode::FORBIDDEN, "{who} {m} {p}");
                assert_eq!(body["code"], 50013, "{who} {m} {p}");
            }
            // The right permission gets past the gate (whatever the stub then answers).
            let (s, body) = e.call(Some(allowed), m.clone(), p, b.clone()).await;
            assert_ne!(body["code"], 50013, "{m} {p}: {s}");
            assert_ne!(s, StatusCode::UNAUTHORIZED, "{m} {p}");
            // The owner may do everything.
            let (_, body) = e.call(Some(OWNER), m.clone(), p, b.clone()).await;
            assert_ne!(body["code"], 50013, "owner {m} {p}");
        }
    }
}

#[tokio::test]
async fn download_routes_need_a_session_and_the_patron_perk() {
    let e = env_with(true).await;
    for (m, p, b) in download_routes() {
        let (s, _) = e.call(None, m.clone(), p, b.clone()).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "anonymous {m} {p}");
    }
    // Options and start refuse non-Patrons with 403 before looking at the catalog.
    for (m, p, b) in &download_routes()[..2] {
        let (s, body) = e.call(Some(MEMBER), m.clone(), p, b.clone()).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{m} {p}");
        assert_eq!(body["code"], 50001);
        assert_eq!(body["message"], "Downloads are for Patrons.");
    }
    // A Patron on a title that does not exist gets a catalog error, not a download.
    let (s, _) = e
        .call(
            Some(PATRON),
            Method::GET,
            "/cinema/titles/7/downloads",
            None,
        )
        .await;
    assert_ne!(s, StatusCode::OK);
    // Someone else's (here: unknown) download is a 404.
    let (s, _) = e
        .call(Some(PATRON), Method::GET, "/cinema/downloads/5", None)
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = e
        .call(Some(MEMBER), Method::GET, "/cinema/downloads/5/file", None)
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "no perk, no file");
}

#[tokio::test]
async fn tokens_show_the_secret_once_and_creation_is_logged() {
    let e = env_with(true).await;
    let (s, body) = e
        .call(
            Some(TRANSCODER),
            Method::POST,
            "/admin/transcode/tokens",
            Some(json!({ "name": "  rented gpu " })),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{body}");
    let secret = body["secret"].as_str().unwrap();
    assert!(secret.starts_with("srw_") && secret.len() == 4 + 64);
    assert_eq!(body["token"]["name"], "rented gpu");
    assert!(body["token"]["id"].is_string());
    // Only the hash is kept.
    let stored = e.stub.tokens.lock().unwrap()[0].1.clone();
    assert_eq!(stored, token_hash(secret));
    // The list never carries the secret.
    let (_, list) = e
        .call(
            Some(TRANSCODER),
            Method::GET,
            "/admin/transcode/tokens",
            None,
        )
        .await;
    assert!(!list.to_string().contains(secret));
    assert!(!list.to_string().contains(&stored));
    assert!(
        e.admin
            .log_rows()
            .iter()
            .any(|l| l.action == "transcode.token.create"),
        "the access log has the creation"
    );
    let (s, _) = e
        .call(
            Some(TRANSCODER),
            Method::POST,
            "/admin/transcode/tokens",
            Some(json!({ "name": "" })),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn queue_and_empty_lists_have_the_documented_shape() {
    let e = env_with(true).await;
    let (s, q) = e
        .call(
            Some(TRANSCODER),
            Method::GET,
            "/admin/transcode/queue",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(q["queued"], 2);
    let (s, jobs) = e
        .call(
            Some(TRANSCODER),
            Method::GET,
            "/admin/transcode/jobs?state=failed&kind=ladder&limit=500",
            None,
        )
        .await;
    assert_eq!((s, jobs), (StatusCode::OK, json!([])));
    let (s, _) = e
        .call(
            Some(TRANSCODER),
            Method::GET,
            "/admin/transcode/jobs?state=weird",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // Re-transcode needs an id for the asset and title scopes.
    let (s, _) = e
        .call(
            Some(TRANSCODER),
            Method::POST,
            "/admin/transcode/retranscode",
            Some(json!({ "scope": "asset" })),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, out) = e
        .call(
            Some(TRANSCODER),
            Method::POST,
            "/admin/transcode/retranscode",
            Some(json!({ "scope": "all", "reason": "profile 2" })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{out}");
    assert_eq!(out["enqueued"], json!([]));
}

#[tokio::test]
async fn uploads_validate_before_touching_storage() {
    let e = env_with(true).await;
    for body in [
        json!({ "filename": "a.exe", "size_bytes": 5 }),
        json!({ "filename": "a.mkv", "size_bytes": 0 }),
        json!({ "filename": "a.mkv", "size_bytes": 99_000_000_000_000u64 }),
    ] {
        let (s, out) = e
            .call(
                Some(PUBLISHER),
                Method::POST,
                "/admin/cinema/assets",
                Some(body.clone()),
            )
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{body}: {out}");
    }
    let (s, _) = e
        .call(
            Some(PUBLISHER),
            Method::GET,
            "/admin/cinema/assets/abc",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_missing_service_is_a_server_error_not_a_client_error() {
    let e = env_with(false).await;
    let (s, _) = e
        .call(Some(OWNER), Method::GET, "/admin/transcode/queue", None)
        .await;
    assert_eq!(s, StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn openapi_lists_every_media_path_with_unique_operation_ids() {
    let doc = serde_json::to_value(super::openapi()).unwrap();
    let paths = doc["paths"].as_object().unwrap();
    for (_, p, _) in asset_routes()
        .into_iter()
        .chain(transcode_routes())
        .chain(download_routes())
    {
        let templated = regex_free_template(p);
        assert!(paths.contains_key(&templated), "{templated}");
    }
    let mut ops = Vec::new();
    for item in paths.values() {
        for m in item.as_object().unwrap().values() {
            if let Some(id) = m["operationId"].as_str() {
                ops.push(id.to_owned());
            }
        }
    }
    let n = ops.len();
    ops.sort();
    ops.dedup();
    assert_eq!(ops.len(), n, "duplicate operation ids");
    assert!(
        ops.iter()
            .all(|o| o.starts_with("admin_") || o.starts_with("cinema_"))
    );
}

/// `/admin/cinema/assets/7/complete` to `/admin/cinema/assets/{asset_id}/complete`.
fn regex_free_template(path: &str) -> String {
    let path = path.split('?').next().unwrap_or(path);
    let segs: Vec<&str> = path.split('/').collect();
    let name = |prev: &str| match prev {
        "assets" => "{asset_id}",
        "tokens" => "{token_id}",
        "workers" => "{worker_id}",
        "jobs" if path.starts_with("/admin/") => "{job_id}",
        "titles" => "{title_id}",
        "downloads" => "{job_id}",
        _ => "",
    };
    let mut out = Vec::new();
    for (i, s) in segs.iter().enumerate() {
        let numeric = !s.is_empty() && (s.chars().all(|c| c.is_ascii_digit()) || *s == "w1");
        if i > 0 && numeric {
            let n = name(segs[i - 1]);
            out.push(if n.is_empty() {
                (*s).to_owned()
            } else {
                n.to_owned()
            });
        } else {
            out.push((*s).to_owned());
        }
    }
    out.join("/")
}
