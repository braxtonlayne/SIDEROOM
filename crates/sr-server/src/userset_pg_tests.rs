//! The user-settings jobs against real Postgres (run with `SR_TEST_DATABASE_URL`,
//! `--ignored`, `--test-threads=1`): the export job from request to download, failure after
//! the last attempt, the sweep of lapsed packages.

use std::io::{Cursor, Read};

use sr_api::routes::userset::data_export::{DataExportStore, Refusal};
use sr_db::jobs::Job;
use sr_db::testing::TestDb;
use sr_storage::{ObjectStorage, ObjectStoreStorage};
use std::sync::Arc;

use crate::userset_jobs::{PgExportStore, run_export, sweep_exports};

async fn account(pool: &sqlx::PgPool, id: i64, name: &str) {
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $1, $2, $3, 'x')",
    )
    .bind(id)
    .bind(name)
    .bind(format!("{name}@example.test"))
    .execute(pool)
    .await
    .unwrap();
}

fn job_for(export: i64, attempts: i32) -> Job {
    Job {
        id: 1,
        kind: "user.data_export".into(),
        payload: serde_json::json!({ "export_id": export.to_string() }),
        state: "running".into(),
        priority: 0,
        attempts,
        max_attempts: 5,
        last_error: None,
    }
}

fn ids() -> sr_db::auth_repo::IdSource {
    let n = std::sync::atomic::AtomicI64::new(9_000);
    Arc::new(move || n.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_export_job_builds_stores_and_the_sweep_removes_it() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    account(&pool, 1, "writer").await;
    let storage = ObjectStoreStorage::in_memory();
    let store = PgExportStore::new(pool.clone(), ids());

    let rec = store.request(1).await.unwrap().unwrap();
    assert_eq!(rec.state, "pending");
    assert!(matches!(
        store.request(1).await.unwrap(),
        Err(Refusal::InProgress(_))
    ));
    // The request queued exactly one build job.
    let queued: i64 =
        sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = 'user.data_export'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(queued, 1);

    run_export(&pool, &storage, &job_for(rec.id, 1))
        .await
        .unwrap();
    let done = store.get(1, rec.id).await.unwrap().unwrap();
    assert_eq!(done.state, "ready");
    let key = done.storage_key.clone().unwrap();
    assert_eq!(key, format!("exports/1/{}.zip", rec.id));
    let bytes = storage.get(&key).await.unwrap();
    assert_eq!(done.size_bytes, Some(bytes.len() as i64));
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).unwrap();
    let mut s = String::new();
    zip.by_name("account.json")
        .unwrap()
        .read_to_string(&mut s)
        .unwrap();
    assert!(s.contains("writer"));

    // Running it again (a duplicate job) changes nothing.
    run_export(&pool, &storage, &job_for(rec.id, 1))
        .await
        .unwrap();

    // Not lapsed yet: the sweep leaves it alone; after seven days it deletes the file.
    assert_eq!(sweep_exports(&pool, &storage).await.unwrap(), 0);
    sqlx::query("UPDATE data_exports SET expires_at = now() - interval '1 minute'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(sweep_exports(&pool, &storage).await.unwrap(), 1);
    assert!(storage.get(&key).await.is_err(), "the package is deleted");
    let after = store.get(1, rec.id).await.unwrap().unwrap();
    assert_eq!(after.state, "expired");
    assert!(after.storage_key.is_none());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_build_that_keeps_failing_is_marked_failed_after_the_last_attempt() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    account(&pool, 1, "writer").await;
    let store = PgExportStore::new(pool.clone(), ids());
    let rec = store.request(1).await.unwrap().unwrap();

    // Break a table the package reads so the build cannot succeed.
    sqlx::query("ALTER TABLE reading_bookmarks RENAME TO reading_bookmarks_gone")
        .execute(&pool)
        .await
        .unwrap();
    let storage = ObjectStoreStorage::in_memory();
    // Early attempts surface the error for a retry and leave the request building.
    assert!(
        run_export(&pool, &storage, &job_for(rec.id, 1))
            .await
            .is_err()
    );
    assert_eq!(
        store.get(1, rec.id).await.unwrap().unwrap().state,
        "building"
    );
    // The last attempt gives up and frees the member to ask again.
    run_export(&pool, &storage, &job_for(rec.id, 5))
        .await
        .unwrap();
    assert_eq!(store.get(1, rec.id).await.unwrap().unwrap().state, "failed");
    assert!(
        store.request(1).await.unwrap().is_ok(),
        "a failed build does not use up the day"
    );
}
