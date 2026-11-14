//! SEC-AUD-254: a package upload that cannot be left unfinishable. The session half over real tables, with the
//! Library's in-memory part store behind a wrapper that behaves like a real one (an upload the store has completed
//! is gone: asking for its parts, or completing it again, fails), a fake checker and a queue that only records.
//! Run with a scratch Postgres that may CREATE DATABASE:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-activities --test package_upload_db -- --ignored --test-threads=1`

use std::collections::BTreeSet;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicI64, Ordering},
};

use async_trait::async_trait;
use bytes::Bytes;
use sr_activities::model::BundleDetail;
use sr_activities::package_upload::{
    CHECK_ALLOWANCE_HOURS, IngestProgress, PackageIngest, PackageLimits, PackageQueue, PackageTier,
    PackageUploadState, PackageUploads, PartsWiring,
};
use sr_activities::{ActivityError, ActivityResult};
use sr_db::PgPool;
use sr_db::accounts::{self, NewAccount};
use sr_db::testing::TestDb;
use sr_library::multipart::{MemoryPartStore, PartStore, StoredPart};
use sr_storage::{ObjectStorage, ObjectStoreStorage};

static SEQ: AtomicI64 = AtomicI64::new(0);

fn next() -> i64 {
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|t| t.as_micros() as i64)
        .unwrap_or(1);
    base * 1000 + SEQ.fetch_add(1, Ordering::Relaxed) % 1000
}

async fn account(pool: &PgPool, name: &str) -> i64 {
    let id = next();
    let n = format!("{name}{id}");
    assert!(
        accounts::create_account(
            pool,
            &NewAccount {
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

/// A store like a real one: once an upload is completed or aborted, the store no longer knows it. Completing is
/// serialised (the second of two concurrent calls finds the upload gone, as against S3).
struct RealisticStore {
    inner: MemoryPartStore,
    gone: tokio::sync::Mutex<BTreeSet<String>>,
}

#[async_trait]
impl PartStore for RealisticStore {
    async fn create(&self, key: &str, content_type: &str) -> Result<String, String> {
        self.inner.create(key, content_type).await
    }
    fn presign_part(
        &self,
        key: &str,
        upload_id: &str,
        part_number: u32,
        ttl: std::time::Duration,
    ) -> Result<String, String> {
        self.inner.presign_part(key, upload_id, part_number, ttl)
    }
    async fn list_parts(&self, key: &str, upload_id: &str) -> Result<Vec<StoredPart>, String> {
        if self.gone.lock().await.contains(upload_id) {
            return Err("NoSuchUpload".into());
        }
        self.inner.list_parts(key, upload_id).await
    }
    async fn complete(
        &self,
        key: &str,
        upload_id: &str,
        parts: &[StoredPart],
    ) -> Result<(), String> {
        let mut gone = self.gone.lock().await;
        if gone.contains(upload_id) {
            return Err("NoSuchUpload".into());
        }
        // The window in which a second call would pass its own check.
        tokio::time::sleep(std::time::Duration::from_millis(60)).await;
        self.inner.complete(key, upload_id, parts).await?;
        gone.insert(upload_id.to_owned());
        Ok(())
    }
    async fn abort(&self, key: &str, upload_id: &str) -> Result<(), String> {
        let mut gone = self.gone.lock().await;
        if !gone.insert(upload_id.to_owned()) {
            return Err("NoSuchUpload".into());
        }
        self.inner.abort(key, upload_id).await
    }
}

struct FakeIngest;

#[async_trait]
impl PackageIngest for FakeIngest {
    async fn limits(&self, _app: i64) -> ActivityResult<PackageLimits> {
        Ok(PackageLimits {
            max_bytes: 512 * 1024 * 1024,
            max_files: 20_000,
            tier: PackageTier::Standard,
        })
    }
    async fn ingest(
        &self,
        _app: i64,
        _user: i64,
        _staged_key: &str,
        _progress: Option<&dyn IngestProgress>,
    ) -> ActivityResult<BundleDetail> {
        Err(ActivityError::Storage("the check does not run here".into()))
    }
}

#[derive(Default)]
struct RecordingQueue {
    queued: Mutex<Vec<i64>>,
    fail: Mutex<bool>,
}

#[async_trait]
impl PackageQueue for RecordingQueue {
    async fn enqueue(&self, upload_id: i64) -> Result<(), String> {
        if *self.fail.lock().unwrap() {
            return Err("the queue is down".into());
        }
        self.queued.lock().unwrap().push(upload_id);
        Ok(())
    }
}

struct Rig {
    uploads: PackageUploads,
    storage: Arc<dyn ObjectStorage>,
    store: Arc<RealisticStore>,
    queue: Arc<RecordingQueue>,
    pool: PgPool,
    app: i64,
    owner: i64,
    _db: TestDb,
}

/// Parts of 4 bytes, so a 10 byte zip is three parts.
async fn rig() -> Rig {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let owner = account(&pool, "owner").await;
    let app = application(&pool, owner).await;
    let storage: Arc<dyn ObjectStorage> = Arc::new(ObjectStoreStorage::in_memory());
    let store = Arc::new(RealisticStore {
        inner: MemoryPartStore::new(storage.clone()),
        gone: tokio::sync::Mutex::new(BTreeSet::new()),
    });
    let queue = Arc::new(RecordingQueue::default());
    let uploads = PackageUploads::new(
        pool.clone(),
        Arc::new(next),
        Arc::new(FakeIngest),
        storage.clone(),
        Some(PartsWiring::new(store.clone()).with_part_size(4)),
        queue.clone(),
    );
    Rig {
        uploads,
        storage,
        store,
        queue,
        pool,
        app,
        owner,
        _db: db,
    }
}

impl Rig {
    /// A new upload with all three parts put; returns its id and its object key.
    async fn upload_with_parts(&self) -> (i64, String) {
        let slot = self
            .uploads
            .begin(self.app, self.owner, 10, "game.zip")
            .await
            .unwrap();
        let id: i64 = slot.upload_id.parse().unwrap();
        let mp: String =
            sqlx::query_scalar("SELECT multipart_id FROM activity_package_uploads WHERE id = $1")
                .bind(id)
                .fetch_one(&self.pool)
                .await
                .unwrap();
        for n in 1..=3u32 {
            let len = if n == 3 { 2 } else { 4 };
            self.store
                .inner
                .put_part(&mp, n, Bytes::from(vec![7u8; len]));
        }
        (id, sr_activities::package_upload::staged_key(self.app, id))
    }

    async fn there(&self, key: &str) -> bool {
        self.storage.head(key).await.is_ok()
    }

    async fn state(&self, id: i64) -> String {
        sqlx::query_scalar("SELECT state FROM activity_package_uploads WHERE id = $1")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    /// Make the row's update to `checking` fail, as a database that went away between the store and the row would.
    async fn break_the_update(&self) {
        for q in [
            "CREATE OR REPLACE FUNCTION p71_refuse_checking() RETURNS trigger LANGUAGE plpgsql AS \
             $$ BEGIN RAISE EXCEPTION 'the row cannot be updated'; END $$",
            "CREATE TRIGGER p71_refuse_checking BEFORE UPDATE ON activity_package_uploads \
             FOR EACH ROW WHEN (NEW.state = 'checking') EXECUTE FUNCTION p71_refuse_checking()",
        ] {
            sqlx::query(q).execute(&self.pool).await.unwrap();
        }
    }

    async fn mend_the_update(&self) {
        sqlx::query("DROP TRIGGER p71_refuse_checking ON activity_package_uploads")
            .execute(&self.pool)
            .await
            .unwrap();
    }
}

/// (a) The store assembled the object and the row's update failed: before the fix a second `complete` failed for
/// ever ("The storage did not accept the uploaded parts"), because the store no longer had the upload.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn complete_can_be_asked_again_after_the_row_failed_to_move() {
    let r = rig().await;
    let (id, key) = r.upload_with_parts().await;
    r.break_the_update().await;
    assert!(r.uploads.complete(r.app, id).await.is_err());
    assert!(r.there(&key).await, "the store did its half");
    assert_eq!(r.state(id).await, "uploading");
    assert!(r.queue.queued.lock().unwrap().is_empty());
    r.mend_the_update().await;

    let done = r.uploads.complete(r.app, id).await.unwrap();
    assert_eq!(done.state, PackageUploadState::Checking);
    assert_eq!(r.state(id).await, "checking");
    assert_eq!(r.queue.queued.lock().unwrap().as_slice(), &[id]);
    // And once more: the current state, no second check.
    let again = r.uploads.complete(r.app, id).await.unwrap();
    assert_eq!(again.state, PackageUploadState::Checking);
    assert_eq!(r.queue.queued.lock().unwrap().len(), 1);
}

/// (b) Giving the upload up after that leaves nothing in the store, whatever state the row was in.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn giving_an_upload_up_leaves_nothing_in_the_store_in_every_state() {
    // The row never moved (the object is assembled, the state says uploading).
    let r = rig().await;
    let (id, key) = r.upload_with_parts().await;
    r.break_the_update().await;
    assert!(r.uploads.complete(r.app, id).await.is_err());
    r.mend_the_update().await;
    assert!(r.there(&key).await);
    r.uploads.abort(r.app, id).await.unwrap();
    assert!(!r.there(&key).await, "the assembled object is deleted");
    assert_eq!(r.state(id).await, "aborted");

    // Still uploading, parts only: the parts are aborted, there is no object to delete.
    let (id, key) = r.upload_with_parts().await;
    let mp: String =
        sqlx::query_scalar("SELECT multipart_id FROM activity_package_uploads WHERE id = $1")
            .bind(id)
            .fetch_one(&r.pool)
            .await
            .unwrap();
    r.uploads.abort(r.app, id).await.unwrap();
    assert!(r.store.inner.aborted().contains(&mp));
    assert!(!r.there(&key).await);

    // Checking: the staged object goes.
    let (id, key) = r.upload_with_parts().await;
    r.uploads.complete(r.app, id).await.unwrap();
    assert_eq!(r.state(id).await, "checking");
    assert!(r.there(&key).await);
    r.uploads.abort(r.app, id).await.unwrap();
    assert!(!r.there(&key).await);
    assert_eq!(r.state(id).await, "aborted");
    // A second abort is quiet.
    r.uploads.abort(r.app, id).await.unwrap();
}

/// (c) A check that was queued late in the upload's day is not expired under it: `complete` moves `expires_at` out
/// by the check's allowance. (Before, it kept the 24 hours of `begin`: an upload completed in its last minutes was
/// expired, and its staged zip deleted, by the next sweep while its check still waited.)
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_check_queued_late_is_not_expired_under_it() {
    let r = rig().await;
    let (id, key) = r.upload_with_parts().await;
    sqlx::query(
        "UPDATE activity_package_uploads SET expires_at = now() + interval '10 minutes' WHERE id = $1",
    )
    .bind(id)
    .execute(&r.pool)
    .await
    .unwrap();
    r.uploads.complete(r.app, id).await.unwrap();
    let hours_left: f64 = sqlx::query_scalar(
        "SELECT (extract(epoch FROM expires_at - now()) / 3600)::float8 \
         FROM activity_package_uploads WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&r.pool)
    .await
    .unwrap();
    let want = CHECK_ALLOWANCE_HOURS as f64;
    assert!(
        hours_left > want - 0.1 && hours_left < want + 0.1,
        "{hours_left} hours left, wanted about {want}"
    );
    // The sweep leaves it and its object alone.
    assert_eq!(r.uploads.sweep().await.unwrap(), (0, 0));
    assert_eq!(r.state(id).await, "checking");
    assert!(r.there(&key).await);
    // An upload that had longer than the allowance is not shortened.
    let (long, _) = r.upload_with_parts().await;
    sqlx::query(
        "UPDATE activity_package_uploads SET expires_at = now() + interval '20 hours' WHERE id = $1",
    )
    .bind(long)
    .execute(&r.pool)
    .await
    .unwrap();
    r.uploads.complete(r.app, long).await.unwrap();
    let kept: bool = sqlx::query_scalar(
        "SELECT expires_at > now() + interval '19 hours' FROM activity_package_uploads WHERE id = $1",
    )
    .bind(long)
    .fetch_one(&r.pool)
    .await
    .unwrap();
    assert!(kept, "the allowance never shortens the upload's own time");
    // A check that is stuck past the allowance is still ended by the sweep, and its object freed.
    sqlx::query(
        "UPDATE activity_package_uploads SET expires_at = now() - interval '1 minute' WHERE id = $1",
    )
    .bind(id)
    .execute(&r.pool)
    .await
    .unwrap();
    assert_eq!(r.uploads.sweep().await.unwrap(), (1, 0));
    assert_eq!(r.state(id).await, "expired");
    assert!(!r.there(&key).await);
}

/// The sweep frees the assembled object of an upload whose row never moved on, too.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_sweep_frees_the_object_of_an_upload_the_row_never_moved_on() {
    let r = rig().await;
    let (id, key) = r.upload_with_parts().await;
    r.break_the_update().await;
    assert!(r.uploads.complete(r.app, id).await.is_err());
    r.mend_the_update().await;
    assert!(r.there(&key).await);
    sqlx::query(
        "UPDATE activity_package_uploads SET expires_at = now() - interval '1 minute' WHERE id = $1",
    )
    .bind(id)
    .execute(&r.pool)
    .await
    .unwrap();
    assert_eq!(r.uploads.sweep().await.unwrap(), (1, 0));
    assert!(!r.there(&key).await);
}

/// (d) Two `complete` calls at once: one assembles, the other finds the store no longer has the upload; before the
/// fix it answered "The storage did not accept the uploaded parts" to a request that had in fact succeeded.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn two_complete_calls_at_once_both_succeed_and_queue_one_check() {
    let r = rig().await;
    let (id, key) = r.upload_with_parts().await;
    let (a, b) = tokio::join!(r.uploads.complete(r.app, id), r.uploads.complete(r.app, id));
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.state, PackageUploadState::Checking);
    assert_eq!(b.state, PackageUploadState::Checking);
    assert_eq!(r.queue.queued.lock().unwrap().as_slice(), &[id]);
    assert!(r.there(&key).await);
}

/// A check that cannot be queued leaves no staged object behind.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_check_that_cannot_be_queued_frees_the_staged_object() {
    let r = rig().await;
    let (id, key) = r.upload_with_parts().await;
    *r.queue.fail.lock().unwrap() = true;
    assert!(r.uploads.complete(r.app, id).await.is_err());
    assert_eq!(r.state(id).await, "refused");
    assert!(!r.there(&key).await);
}

/// Parts still missing is still the same answer, and the upload stays open.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn complete_with_a_part_missing_is_still_refused_and_the_upload_stays_open() {
    let r = rig().await;
    let slot = r
        .uploads
        .begin(r.app, r.owner, 10, "game.zip")
        .await
        .unwrap();
    let id: i64 = slot.upload_id.parse().unwrap();
    let e = r.uploads.complete(r.app, id).await.unwrap_err();
    assert!(matches!(
        e,
        sr_activities::package_upload::PackageUploadError::PartsMissing(_)
    ));
    assert_eq!(r.state(id).await, "uploading");
}
