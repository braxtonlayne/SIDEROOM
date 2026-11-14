//! SEC-AUD-176: the package upload sessions over real tables, with the Library's in-memory part store and a fake
//! checker. They need Postgres:
//! `SR_TEST_DATABASE_URL=... DATABASE_URL=... cargo test -p sr-api --lib routes::activities::uploads_tests -- --ignored --test-threads=1`

use super::uploads::up_err;
use crate::routes::messages::error::MsgError;
use async_trait::async_trait;
use bytes::Bytes;
use sqlx::PgPool;
use sr_activities::package_upload::{
    CHECK_FAILED, CODE_NO_PARTS, CODE_PARTS_MISSING, IngestProgress, PackageIngest, PackageLimits,
    PackageQueue, PackageTier, PackageUploadError, PackageUploadState, PackageUploads, PartsWiring,
};
use sr_activities::{ActivityError, ActivityResult, model::BundleDetail};
use sr_library::MemoryPartStore;
use sr_storage::{ObjectStorage, ObjectStoreStorage};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicI64, Ordering},
};

const MIB: u64 = 1024 * 1024;

/// What the fake checker answers.
#[derive(Clone)]
enum Outcome {
    Ready,
    Refuse(&'static str),
    Broken,
}

struct FakeIngest {
    tier: PackageTier,
    max_bytes: u64,
    outcome: Mutex<Outcome>,
    seen: Mutex<Vec<String>>,
}

impl FakeIngest {
    fn new(tier: PackageTier, max_bytes: u64) -> Arc<Self> {
        Arc::new(Self {
            tier,
            max_bytes,
            outcome: Mutex::new(Outcome::Ready),
            seen: Mutex::new(Vec::new()),
        })
    }
    fn set(&self, o: Outcome) {
        *self.outcome.lock().unwrap() = o;
    }
}

#[async_trait]
impl PackageIngest for FakeIngest {
    async fn limits(&self, _app: i64) -> ActivityResult<PackageLimits> {
        Ok(PackageLimits {
            max_bytes: self.max_bytes,
            max_files: 20_000,
            tier: self.tier,
        })
    }

    async fn ingest(
        &self,
        _app: i64,
        _user: i64,
        staged_key: &str,
        progress: Option<&dyn IngestProgress>,
    ) -> ActivityResult<BundleDetail> {
        self.seen.lock().unwrap().push(staged_key.to_owned());
        if let Some(p) = progress {
            p.bytes(5, 10);
        }
        match self.outcome.lock().unwrap().clone() {
            Outcome::Ready => Ok(serde_json::from_value(serde_json::json!({
                "bundle": {
                    "id": "4242", "sha256": "a".repeat(64), "byte_size": 10, "entry_count": 1,
                    "state": "draft", "uploaded_at_ms": 1, "reviewed_at_ms": null, "note": null
                },
                "entries": [{ "path": "index.html", "size": 10, "content_type": "text/html" }]
            }))
            .unwrap()),
            Outcome::Refuse(t) => Err(ActivityError::Invalid("bundle", t.into())),
            Outcome::Broken => Err(ActivityError::Storage("the bucket is gone".into())),
        }
    }
}

/// Runs the check as soon as `complete` queues it (the job system's role).
#[derive(Default)]
struct InlineQueue {
    uploads: Mutex<Option<PackageUploads>>,
    queued: Mutex<Vec<i64>>,
}

#[async_trait]
impl PackageQueue for InlineQueue {
    async fn enqueue(&self, upload_id: i64) -> Result<(), String> {
        self.queued.lock().unwrap().push(upload_id);
        if let Some(u) = self.uploads.lock().unwrap().clone() {
            tokio::spawn(async move {
                let _ = u.run_ingest(upload_id, true).await;
            });
        }
        Ok(())
    }
}

struct Rig {
    uploads: PackageUploads,
    parts: Arc<MemoryPartStore>,
    ingest: Arc<FakeIngest>,
    queue: Arc<InlineQueue>,
    pool: PgPool,
    app: i64,
    other_app: i64,
    owner: i64,
    _db: sr_db::testing::TestDb,
}

static SEQ: AtomicI64 = AtomicI64::new(0);

fn next() -> i64 {
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|t| t.as_micros() as i64)
        .unwrap_or(1);
    base * 1000 + SEQ.fetch_add(1, Ordering::Relaxed)
}

async fn account(pool: &PgPool, name: &str) -> i64 {
    let id = next();
    let n = format!("{name}{id}");
    assert!(
        sr_db::accounts::create_account(
            pool,
            &sr_db::accounts::NewAccount {
                id,
                person_id: next(),
                username: &n,
                display_name: None,
                email: &format!("{n}@example.test"),
                password_hash: "x",
                state: "active",
                email_verified: true,
                initial_credits: 0,
                flags: 0,
            },
            next(),
        )
        .await
        .unwrap()
    );
    id
}

async fn application(pool: &PgPool, owner: i64) -> i64 {
    let (id, bot) = (next(), account(pool, "bot").await);
    sqlx::query(
        "INSERT INTO applications (id, owner_id, bot_user_id, name, client_secret_hash) \
         VALUES ($1, $2, $3, 'Big game', decode(repeat('ab', 32), 'hex'))",
    )
    .bind(id)
    .bind(owner)
    .bind(bot)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// Parts of 4 bytes, so a 10 byte zip is three parts.
async fn rig(tier: PackageTier, max_bytes: u64, signing: bool) -> Rig {
    let db = sr_db::testing::TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let owner = account(&pool, "owner").await;
    let (app, other_app) = (
        application(&pool, owner).await,
        application(&pool, owner).await,
    );
    // The bundle the fake checker "makes" (a finished session references it).
    sqlx::query(
        "INSERT INTO application_activity_bundles (id, application_id, sha256, byte_size, entry_count) \
         VALUES (4242, $1, repeat('a', 64), 10, 1)",
    )
    .bind(app)
    .execute(&pool)
    .await
    .unwrap();
    let storage: Arc<dyn ObjectStorage> = Arc::new(ObjectStoreStorage::in_memory());
    let parts = Arc::new(MemoryPartStore::new(storage.clone()));
    let ingest = FakeIngest::new(tier, max_bytes);
    let queue = Arc::new(InlineQueue::default());
    let uploads = PackageUploads::new(
        pool.clone(),
        Arc::new(next),
        ingest.clone(),
        storage,
        signing.then(|| PartsWiring::new(parts.clone()).with_part_size(4)),
        queue.clone(),
    );
    *queue.uploads.lock().unwrap() = Some(uploads.clone());
    Rig {
        uploads,
        parts,
        ingest,
        queue,
        pool,
        app,
        other_app,
        owner,
        _db: db,
    }
}

impl Rig {
    /// The store's id of the open multipart upload of `upload`.
    async fn multipart_id(&self, upload: &str) -> String {
        sqlx::query_scalar(
            "SELECT multipart_id FROM activity_package_uploads WHERE id = $1::text::bigint",
        )
        .bind(upload)
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }

    async fn put_parts(&self, upload: &str, numbers: &[u32]) {
        let mp = self.multipart_id(upload).await;
        for n in numbers {
            // 10 bytes in parts of 4: 4, 4, 2.
            let len = if *n == 3 { 2 } else { 4 };
            self.parts.put_part(&mp, *n, Bytes::from(vec![7u8; len]));
        }
    }

    async fn begin(&self) -> String {
        self.uploads
            .begin(self.app, self.owner, 10, "game.zip")
            .await
            .unwrap()
            .upload_id
    }

    async fn id(&self, upload: &str) -> i64 {
        upload.parse().unwrap()
    }

    async fn settle(&self, upload: &str) -> sr_activities::package_upload::PackageUploadStatus {
        let id = self.id(upload).await;
        for _ in 0..100 {
            let s = self.uploads.status(self.app, id).await.unwrap();
            if !matches!(
                s.state,
                PackageUploadState::Checking | PackageUploadState::Uploading
            ) {
                return s;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        panic!("the check did not end");
    }
}

fn api_of(e: PackageUploadError) -> sr_core::ApiError {
    match up_err(e) {
        MsgError::Api(a) => a,
        MsgError::RetryAfter { api, .. } => api,
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn an_upload_over_the_application_limit_is_refused_with_413_and_names_the_limit() {
    let r = rig(PackageTier::Standard, 512 * MIB, true).await;
    let e = r
        .uploads
        .begin(r.app, r.owner, 600 * MIB, "big.zip")
        .await
        .unwrap_err();
    let api = api_of(e);
    assert_eq!(api.status(), 413);
    assert!(api.message.contains("512 MiB"), "{}", api.message);
    assert!(api.message.contains("standard limit"), "{}", api.message);
    assert!(
        api.message
            .ends_with("The operator of this server can raise the limit."),
        "{}",
        api.message
    );
    // At the limit is fine.
    assert!(
        r.uploads
            .begin(r.app, r.owner, 512 * MIB, "ok.zip")
            .await
            .is_ok()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn an_upload_of_someone_elses_application_is_404() {
    let r = rig(PackageTier::Standard, 512 * MIB, true).await;
    let upload = r.begin().await;
    let id = r.id(&upload).await;
    // The same id asked through another application: unknown, whatever the call.
    assert_eq!(
        r.uploads.status(r.other_app, id).await.unwrap_err(),
        ActivityError::NotFound("upload")
    );
    assert_eq!(
        r.uploads.abort(r.other_app, id).await.unwrap_err(),
        ActivityError::NotFound("upload")
    );
    let e = r.uploads.complete(r.other_app, id).await.unwrap_err();
    assert_eq!(api_of(e).status(), 404);
    // Nothing happened to the real upload.
    assert_eq!(
        r.uploads.status(r.app, id).await.unwrap().state,
        PackageUploadState::Uploading
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn complete_with_a_missing_part_is_409_and_lists_it() {
    let r = rig(PackageTier::Standard, 512 * MIB, true).await;
    let upload = r.begin().await;
    r.put_parts(&upload, &[1, 3]).await;
    let e = r
        .uploads
        .complete(r.app, r.id(&upload).await)
        .await
        .unwrap_err();
    assert!(
        matches!(&e, PackageUploadError::PartsMissing(m) if m.contains('2') && !m.contains("1,"))
    );
    let api = api_of(e);
    assert_eq!(api.status(), 409);
    assert_eq!(api.fields[0].1.code, CODE_PARTS_MISSING);
    assert!(api.message.contains("Parts 2 of 3"), "{}", api.message);
    // The upload is still open and the status names the part to send.
    let s = r.uploads.status(r.app, r.id(&upload).await).await.unwrap();
    assert_eq!(s.state, PackageUploadState::Uploading);
    assert_eq!(s.received, vec![1, 3]);
    assert_eq!(
        s.parts
            .unwrap()
            .iter()
            .map(|p| p.part_number)
            .collect::<Vec<_>>(),
        vec![2]
    );
    assert!(r.queue.queued.lock().unwrap().is_empty());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn complete_queues_the_check_and_the_status_ends_ready_with_the_bundle() {
    let r = rig(PackageTier::Reviewed, 4096 * MIB, true).await;
    let upload = r.begin().await;
    r.put_parts(&upload, &[1, 2, 3]).await;
    let id = r.id(&upload).await;
    let done = r.uploads.complete(r.app, id).await.unwrap();
    assert_eq!(done.state, PackageUploadState::Checking);
    assert_eq!(done.total_bytes, "10");
    assert_eq!(r.queue.queued.lock().unwrap().as_slice(), &[id]);
    // Asking again does not queue a second check.
    r.uploads.complete(r.app, id).await.unwrap();
    assert_eq!(r.queue.queued.lock().unwrap().len(), 1);
    let s = r.settle(&upload).await;
    assert_eq!(s.state, PackageUploadState::Ready);
    assert_eq!(s.bundle_id.as_deref(), Some("4242"));
    assert_eq!(s.checked_bytes, "10");
    assert_eq!(
        r.ingest.seen.lock().unwrap().as_slice(),
        &[format!("activities/{}/uploads/{upload}.zip", r.app)]
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_refused_package_shows_its_reason_in_the_status() {
    let r = rig(PackageTier::Standard, 512 * MIB, true).await;
    r.ingest.set(Outcome::Refuse(
        "levels/a.exe: this file type is not allowed",
    ));
    let upload = r.begin().await;
    r.put_parts(&upload, &[1, 2, 3]).await;
    r.uploads
        .complete(r.app, r.id(&upload).await)
        .await
        .unwrap();
    let s = r.settle(&upload).await;
    assert_eq!(s.state, PackageUploadState::Refused);
    assert_eq!(
        s.error.as_deref(),
        Some("levels/a.exe: this file type is not allowed")
    );
    assert!(s.bundle_id.is_none());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_failing_check_is_retried_and_ends_refused_with_a_plain_text_after_the_last_attempt() {
    let r = rig(PackageTier::Standard, 512 * MIB, true).await;
    r.ingest.set(Outcome::Broken);
    // The queue here never runs the job: drive it by hand.
    *r.queue.uploads.lock().unwrap() = None;
    let upload = r.begin().await;
    r.put_parts(&upload, &[1, 2, 3]).await;
    let id = r.id(&upload).await;
    r.uploads.complete(r.app, id).await.unwrap();
    assert!(r.uploads.run_ingest(id, false).await.is_err());
    assert_eq!(
        r.uploads.status(r.app, id).await.unwrap().state,
        PackageUploadState::Checking
    );
    r.uploads.run_ingest(id, true).await.unwrap();
    let s = r.uploads.status(r.app, id).await.unwrap();
    assert_eq!(s.state, PackageUploadState::Refused);
    assert_eq!(s.error.as_deref(), Some(CHECK_FAILED));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_seventh_upload_in_an_hour_is_429() {
    let r = rig(PackageTier::Standard, 512 * MIB, true).await;
    for _ in 0..6 {
        r.uploads.begin(r.app, r.owner, 10, "g.zip").await.unwrap();
    }
    let e = r
        .uploads
        .begin(r.app, r.owner, 10, "g.zip")
        .await
        .unwrap_err();
    assert_eq!(e, PackageUploadError::RateLimited);
    assert_eq!(api_of(e).status(), 429);
    // Another application has its own allowance.
    assert!(
        r.uploads
            .begin(r.other_app, r.owner, 10, "g.zip")
            .await
            .is_ok()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_third_open_upload_aborts_the_oldest() {
    let r = rig(PackageTier::Standard, 512 * MIB, true).await;
    let first = r.begin().await;
    let second = r.begin().await;
    let first_mp = r.multipart_id(&first).await;
    let third = r.begin().await;
    assert!(r.parts.aborted().contains(&first_mp));
    let state = |u: &str| {
        let (pool, u) = (r.pool.clone(), u.to_owned());
        async move {
            sqlx::query_scalar::<_, String>(
                "SELECT state FROM activity_package_uploads WHERE id = $1::text::bigint",
            )
            .bind(u)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    assert_eq!(state(&first).await, "aborted");
    assert_eq!(state(&second).await, "uploading");
    assert_eq!(state(&third).await, "uploading");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_store_without_part_signing_answers_409() {
    let r = rig(PackageTier::Standard, 512 * MIB, false).await;
    let e = r
        .uploads
        .begin(r.app, r.owner, 10, "g.zip")
        .await
        .unwrap_err();
    assert_eq!(e, PackageUploadError::NoParts);
    let api = api_of(e);
    assert_eq!(api.status(), 409);
    assert_eq!(api.fields[0].1.code, CODE_NO_PARTS);
    assert!(!r.uploads.limits(r.app).await.unwrap().resumable);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn an_abandoned_upload_expires_and_its_parts_are_aborted() {
    let r = rig(PackageTier::Standard, 512 * MIB, true).await;
    let upload = r.begin().await;
    let mp = r.multipart_id(&upload).await;
    let finished = r.begin().await;
    r.put_parts(&finished, &[1, 2, 3]).await;
    r.uploads
        .complete(r.app, r.id(&finished).await)
        .await
        .unwrap();
    r.settle(&finished).await;
    sqlx::query("UPDATE activity_package_uploads SET expires_at = now() - interval '1 minute'")
        .execute(&r.pool)
        .await
        .unwrap();
    let (expired, deleted) = r.uploads.sweep().await.unwrap();
    assert_eq!(
        (expired, deleted),
        (1, 0),
        "the finished upload is left alone"
    );
    assert!(r.parts.aborted().contains(&mp));
    let s = r.uploads.status(r.app, r.id(&upload).await).await.unwrap();
    assert_eq!(s.state, PackageUploadState::Expired);
    assert_eq!(
        r.uploads
            .status(r.app, r.id(&finished).await)
            .await
            .unwrap()
            .state,
        PackageUploadState::Ready
    );
    // Rows past seven days go.
    sqlx::query("UPDATE activity_package_uploads SET created_at = now() - interval '8 days'")
        .execute(&r.pool)
        .await
        .unwrap();
    assert_eq!(r.uploads.sweep().await.unwrap(), (0, 2));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_limits_route_reports_the_tier() {
    for (tier, word) in [
        (PackageTier::Standard, "standard"),
        (PackageTier::Reviewed, "reviewed"),
        (PackageTier::Platform, "platform"),
    ] {
        let r = rig(tier, 4096 * MIB, true).await;
        let v = serde_json::to_value(r.uploads.limits(r.app).await.unwrap()).unwrap();
        assert_eq!(v["tier"], word);
        assert_eq!(v["max_bytes"], (4096 * MIB).to_string());
        assert_eq!(v["max_files"], 20_000);
        assert_eq!(v["resumable"], true);
        assert_eq!(v["part_size"], 4);
    }
}

/// The whole path with the real checker: a zip of a few hundred KiB goes in three parts of a small injected size,
/// is completed from the parts the store holds, checked by `ingest_stored_zip` and ends `ready` with a draft bundle
/// that lists its files.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_zip_sent_in_three_parts_is_checked_by_the_real_checker_and_becomes_a_bundle() {
    use std::io::Write;
    let db = sr_db::testing::TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let owner = account(&pool, "owner").await;
    let app = application(&pool, owner).await;
    let storage: Arc<dyn ObjectStorage> = Arc::new(ObjectStoreStorage::in_memory());
    let parts = Arc::new(MemoryPartStore::new(storage.clone()));
    let service = sr_activities::ActivityService::new(pool.clone(), Arc::new(next))
        .with_storage(storage.clone());
    let queue = Arc::new(InlineQueue::default());
    let uploads = PackageUploads::new(
        pool.clone(),
        Arc::new(next),
        Arc::new(service.clone()),
        storage.clone(),
        Some(PartsWiring::new(parts.clone()).with_part_size(120 * 1024)),
        queue.clone(),
    );
    *queue.uploads.lock().unwrap() = Some(uploads.clone());

    // About 300 KiB that does not compress away.
    let mut noise = Vec::new();
    let mut x: u32 = 12345;
    for _ in 0..(300 * 1024) {
        x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        noise.push((x >> 16) as u8);
    }
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    z.start_file("index.html", opts).unwrap();
    z.write_all(b"<!doctype html><script src=\"app.js\"></script>")
        .unwrap();
    z.start_file("app.js", opts).unwrap();
    z.write_all(&noise).unwrap();
    let zipped = z.finish().unwrap().into_inner();
    assert!(zipped.len() > 240 * 1024, "{}", zipped.len());

    let slot = uploads
        .begin(app, owner, zipped.len() as u64, "game.zip")
        .await
        .unwrap();
    assert_eq!(slot.part_size, 120 * 1024);
    assert_eq!(slot.part_count, 3);
    let id: i64 = slot.upload_id.parse().unwrap();
    let mp: String =
        sqlx::query_scalar("SELECT multipart_id FROM activity_package_uploads WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    for (i, chunk) in zipped.chunks(120 * 1024).enumerate() {
        parts.put_part(&mp, i as u32 + 1, Bytes::copy_from_slice(chunk));
    }
    uploads.complete(app, id).await.unwrap();
    let mut status = uploads.status(app, id).await.unwrap();
    for _ in 0..200 {
        if !matches!(status.state, PackageUploadState::Checking) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        status = uploads.status(app, id).await.unwrap();
    }
    assert_eq!(
        status.state,
        PackageUploadState::Ready,
        "{:?}",
        status.error
    );
    let bundle: i64 = status.bundle_id.unwrap().parse().unwrap();
    let detail = service.bundle_detail(app, bundle).await.unwrap();
    let mut names: Vec<_> = detail.entries.iter().map(|e| e.path.clone()).collect();
    names.sort();
    assert_eq!(names, ["app.js", "index.html"]);
    // The staged zip is gone.
    assert!(
        storage
            .head(&format!("activities/{app}/uploads/{id}.zip"))
            .await
            .is_err()
    );
}
