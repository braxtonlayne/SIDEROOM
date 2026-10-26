//! The daily `storage.measure` job against real Postgres and the in-memory object store (RDY-OBS-002; run with
//! `SR_TEST_DATABASE_URL`, `--ignored`, `--test-threads=1`).

use std::time::Duration;

use bytes::Bytes;
use sr_admin::AdminRepository;
use sr_db::{admin_repo::PgAdminRepo, auth_repo::IdSource, storage_figures, testing::TestDb};
use sr_storage::{ObjectStorage, ObjectStoreStorage};

use crate::admin_jobs::{STORAGE_WALK_BUDGET, measure_storage};

async fn filled_store() -> ObjectStoreStorage {
    let s = ObjectStoreStorage::in_memory();
    for (key, size) in [
        ("library/1/cover.webp", 10usize),
        ("library/1/2/0000.webp", 20),
        ("library/1/2/0000_720.webp", 5),
        ("cinema/7/poster.webp", 100),
        ("originals/7/source.mkv", 1000),
        ("transcode/7/1/v/a.m4s", 400),
        ("att/1/2/file.bin", 7),
        ("avatars/3/hash", 3),
    ] {
        s.put(key, Bytes::from(vec![0u8; size])).await.unwrap();
    }
    s
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_job_stores_one_row_per_bucket_and_the_gauge_and_system_page_read_them() {
    // The recorder must exist before the job sets the gauge.
    let metrics = sr_api::observability::Metrics::global();
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let store = filled_store().await;

    let out = measure_storage(&pool, &store, 1_000, STORAGE_WALK_BUDGET)
        .await
        .unwrap();
    assert_eq!(
        out,
        vec![
            ("objects", 1545, 8, false),
            ("library", 35, 3, false),
            ("cinema", 1500, 3, false),
        ]
    );
    let stored = storage_figures::list(&pool).await.unwrap();
    let by = |b: &str| stored.iter().find(|f| f.bucket == b).unwrap().clone();
    assert_eq!((by("objects").bytes, by("objects").objects), (1545, 8));
    assert_eq!((by("library").bytes, by("library").objects), (35, 3));
    assert_eq!((by("cinema").bytes, by("cinema").objects), (1500, 3));
    assert!(stored.iter().all(|f| !f.truncated && f.measured_at_ms > 0));

    // The existing gauge carries the bucket labels.
    let text = metrics.render();
    for series in [
        "sr_storage_bytes{bucket=\"objects\"} 1545",
        "sr_storage_bytes{bucket=\"library\"} 35",
        "sr_storage_bytes{bucket=\"cinema\"} 1500",
    ] {
        assert!(text.contains(series), "missing {series} in:\n{text}");
    }

    // Admin > System reads the same rows.
    let repo = PgAdminRepo::new(pool.clone(), std::sync::Arc::new(|| 1_i64) as IdSource);
    let counts = repo.counts().await.unwrap();
    assert_eq!(counts.storage_figures.len(), 3);
    assert_eq!(
        counts
            .storage_figures
            .iter()
            .find(|f| f.bucket == "library")
            .map(|f| f.bytes),
        Some(35)
    );

    // A second run replaces the rows (no duplicates) and sees a deleted object.
    store.delete("originals/7/source.mkv").await.unwrap();
    measure_storage(&pool, &store, 1_000, STORAGE_WALK_BUDGET)
        .await
        .unwrap();
    let stored = storage_figures::list(&pool).await.unwrap();
    assert_eq!(stored.len(), 3);
    assert_eq!(
        stored.iter().find(|f| f.bucket == "cinema").unwrap().bytes,
        500
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_walk_that_hits_its_cap_or_budget_is_flagged_as_a_lower_bound() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let store = filled_store().await;

    let out = measure_storage(&pool, &store, 3, STORAGE_WALK_BUDGET)
        .await
        .unwrap();
    assert!(out.iter().all(|f| f.3), "capped: every figure is flagged");
    assert_eq!(out[0].2, 3, "only three objects were counted");
    assert!(
        storage_figures::list(&pool)
            .await
            .unwrap()
            .iter()
            .all(|f| f.truncated)
    );

    // The time budget stops it too, and a complete run clears the flag again.
    let out = measure_storage(&pool, &store, 1_000, Duration::ZERO)
        .await
        .unwrap();
    assert!(out.iter().all(|f| f.3));
    let out = measure_storage(&pool, &store, 1_000, STORAGE_WALK_BUDGET)
        .await
        .unwrap();
    assert!(out.iter().all(|f| !f.3));
}
