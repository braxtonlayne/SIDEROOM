//! Wave 53 (server-1 HIGH 2): a moderation hold placed while a retention job is deleting stored objects must
//! never leave a held row whose file is gone. The jobs lock and re-check their rows before any object is
//! deleted, so a hold placed in that window waits for the batch (and then finds the row gone) instead of
//! landing between the object delete and the row update. Run with `SR_TEST_DATABASE_URL`, `--ignored`.
//!
//! The race is made deterministic with a bucket whose `delete` first tries to place the hold from another
//! connection (with a short lock timeout), exactly in the window the audit names.

use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use sqlx::PgPool;
use sr_db::testing::TestDb;
use sr_storage::{
    MultipartSession, ObjectInfo, ObjectStorage, ObjectStoreStorage, ObjectStream, PresignMethod,
};

/// A bucket that places a hold (`sql` with `$1 = id`) from another connection whenever an object is deleted.
struct HoldOnDelete {
    inner: ObjectStoreStorage,
    pool: PgPool,
    sql: &'static str,
    id: i64,
    /// Whether each hold attempt went through (it must not while the job holds the row).
    landed: Mutex<Vec<bool>>,
}

impl HoldOnDelete {
    fn new(pool: &PgPool, sql: &'static str, id: i64) -> Self {
        Self {
            inner: ObjectStoreStorage::in_memory(),
            pool: pool.clone(),
            sql,
            id,
            landed: Mutex::new(Vec::new()),
        }
    }
    async fn try_hold(&self) -> bool {
        let Ok(mut tx) = self.pool.begin().await else {
            return false;
        };
        if sqlx::query("SELECT set_config('lock_timeout', '300', true)")
            .execute(&mut *tx)
            .await
            .is_err()
        {
            return false;
        }
        let r = sqlx::query(sqlx::AssertSqlSafe(self.sql.to_owned()))
            .bind(self.id)
            .execute(&mut *tx)
            .await;
        match r {
            Ok(done) if done.rows_affected() > 0 => tx.commit().await.is_ok(),
            _ => false,
        }
    }
}

#[async_trait]
impl ObjectStorage for HoldOnDelete {
    async fn put(&self, key: &str, data: Bytes) -> sr_storage::Result<()> {
        self.inner.put(key, data).await
    }
    async fn get(&self, key: &str) -> sr_storage::Result<Bytes> {
        self.inner.get(key).await
    }
    async fn get_range(&self, key: &str, s: u64, e: u64) -> sr_storage::Result<Bytes> {
        self.inner.get_range(key, s, e).await
    }
    async fn get_stream(
        &self,
        key: &str,
        range: Option<std::ops::Range<u64>>,
    ) -> sr_storage::Result<ObjectStream> {
        self.inner.get_stream(key, range).await
    }
    async fn head(&self, key: &str) -> sr_storage::Result<ObjectInfo> {
        self.inner.head(key).await
    }
    async fn delete(&self, key: &str) -> sr_storage::Result<()> {
        let landed = self.try_hold().await;
        self.landed.lock().unwrap().push(landed);
        self.inner.delete(key).await
    }
    async fn list(&self, prefix: &str) -> sr_storage::Result<Vec<ObjectInfo>> {
        self.inner.list(prefix).await
    }
    async fn presign(
        &self,
        m: PresignMethod,
        key: &str,
        ttl: Duration,
    ) -> sr_storage::Result<String> {
        self.inner.presign(m, key, ttl).await
    }
    async fn multipart_create(&self, key: &str) -> sr_storage::Result<Box<dyn MultipartSession>> {
        self.inner.multipart_create(key).await
    }
}

async fn exec(pool: &PgPool, sql: &'static str, binds: &[i64]) {
    let mut q = sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()));
    for b in binds {
        q = q.bind(*b);
    }
    q.execute(pool).await.expect(sql);
}

/// An account, a hold (id 7), a DM channel (10) and a message id in this month's partition.
async fn seed(pool: &PgPool) -> i64 {
    exec(pool, "INSERT INTO persons (id) VALUES (1)", &[]).await;
    exec(
        pool,
        "INSERT INTO accounts (id, person_id, username, email, password_hash) \
         VALUES (1, 1, 'u1', 'u1@example.test', 'x')",
        &[],
    )
    .await;
    exec(
        pool,
        "INSERT INTO moderation_holds (id, reason) VALUES (7, 'case')",
        &[],
    )
    .await;
    exec(pool, "INSERT INTO channels (id, type) VALUES (10, 1)", &[]).await;
    let _: String = sqlx::query_scalar("SELECT sr_ensure_message_partition(CURRENT_DATE)")
        .fetch_one(pool)
        .await
        .expect("partition");
    sqlx::query_scalar("SELECT sr_snowflake_floor(date_trunc('month', now())) + 1000")
        .fetch_one(pool)
        .await
        .expect("floor")
}

/// No row under a hold has lost its stored object.
async fn held_rows_keep_their_objects(pool: &PgPool, bucket: &HoldOnDelete) {
    let keys: Vec<String> = sqlx::query_scalar(
        "SELECT a.storage_key FROM attachments a LEFT JOIN messages m ON m.id = a.message_id \
         WHERE a.hold_id IS NOT NULL OR m.hold_id IS NOT NULL",
    )
    .fetch_all(pool)
    .await
    .expect("held keys");
    for k in keys {
        assert!(
            bucket.inner.head(&k).await.is_ok(),
            "held attachment {k} lost its object"
        );
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_hold_placed_during_the_message_purge_never_keeps_a_row_without_its_file() {
    let db = TestDb::new().await.expect("test db");
    let pool = db.pool();
    let msg = seed(pool).await;
    exec(
        pool,
        "INSERT INTO messages (id, channel_id, author_id, visibility, deleted_at, purge_after) \
         VALUES ($1, 10, 1, 'user_deleted', now() - interval '31 days', now() - interval '1 day')",
        &[msg],
    )
    .await;
    exec(
        pool,
        "INSERT INTO attachments (id, message_id, uploader_id, filename, size_bytes, storage_key, state) \
         VALUES (100, $1, 1, 'a.png', 1, 'att/100', 'ready')",
        &[msg],
    )
    .await;
    let bucket = HoldOnDelete::new(
        pool,
        "UPDATE messages SET hold_id = 7 WHERE id = $1 AND hold_id IS NULL",
        msg,
    );
    bucket
        .put("att/100", Bytes::from_static(b"x"))
        .await
        .unwrap();
    let purged = crate::jobs::purge_deleted_messages(pool, &bucket, now_ms())
        .await
        .expect("purge");
    held_rows_keep_their_objects(pool, &bucket).await;
    assert_eq!(
        *bucket.landed.lock().unwrap(),
        vec![false],
        "the hold waits for the batch"
    );
    assert_eq!(purged, 1, "the message went with its file");
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_hold_placed_during_the_removed_attachment_purge_never_keeps_a_row_without_its_file() {
    let db = TestDb::new().await.expect("test db");
    let pool = db.pool();
    seed(pool).await;
    exec(
        pool,
        "INSERT INTO attachments (id, message_id, uploader_id, filename, size_bytes, storage_key, state, removed_at) \
         VALUES (100, NULL, 1, 'a.png', 1, 'att/100', 'ready', now() - interval '1 day')",
        &[],
    )
    .await;
    let bucket = HoldOnDelete::new(
        pool,
        "UPDATE attachments SET hold_id = 7 WHERE id = $1 AND hold_id IS NULL",
        100,
    );
    bucket
        .put("att/100", Bytes::from_static(b"x"))
        .await
        .unwrap();
    let n = crate::jobs::purge_removed_attachments(pool, &bucket)
        .await
        .expect("purge");
    held_rows_keep_their_objects(pool, &bucket).await;
    assert_eq!(*bucket.landed.lock().unwrap(), vec![false]);
    assert_eq!(n, 1);
    db.cleanup().await.expect("cleanup");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_hold_placed_during_the_large_attachment_expiry_never_keeps_a_row_without_its_file() {
    let db = TestDb::new().await.expect("test db");
    let pool = db.pool();
    let msg = seed(pool).await;
    exec(
        pool,
        "INSERT INTO messages (id, channel_id, author_id, content) VALUES ($1, 10, 1, 'x')",
        &[msg],
    )
    .await;
    exec(
        pool,
        "INSERT INTO attachments (id, message_id, uploader_id, filename, size_bytes, storage_key, state, \
                                  is_large, expires_at) \
         VALUES (100, $1, 1, 'big.mkv', 1, 'att/100', 'ready', TRUE, now() - interval '1 day')",
        &[msg],
    )
    .await;
    let bucket = HoldOnDelete::new(
        pool,
        "UPDATE attachments SET hold_id = 7 WHERE id = $1 AND hold_id IS NULL",
        100,
    );
    bucket
        .put("att/100", Bytes::from_static(b"x"))
        .await
        .unwrap();
    let expired = crate::jobs::expire_large_attachments(pool, &bucket, now_ms())
        .await
        .expect("expire");
    // The hold attempt timed out on the job's lock instead of landing on a `ready` row whose object went.
    assert_eq!(*bucket.landed.lock().unwrap(), vec![false]);
    assert_eq!(expired.len(), 1);
    let state: String = sqlx::query_scalar("SELECT state::text FROM attachments WHERE id = 100")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(state, "expired");
    db.cleanup().await.expect("cleanup");
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
