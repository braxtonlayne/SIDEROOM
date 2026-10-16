//! Postgres-backed tests for the phase-13 follow-ups of agent h-a. Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test p13_h_a -- --ignored --test-threads=1`

use std::time::Duration;

use sr_db::jobs::{self, NewJob};
use sr_db::testing::TestDb;

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn reap_stale_returns_a_crashed_job_to_the_queue_and_names_it() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    for (id, kind) in [(9001_i64, "test.crashed"), (9002, "test.fresh")] {
        jobs::enqueue(&pool, &NewJob::new(id, kind, serde_json::json!({})))
            .await
            .unwrap();
    }
    let claimed = jobs::claim(
        &pool,
        &["test.crashed".to_owned(), "test.fresh".to_owned()],
        "node-a",
        10,
    )
    .await
    .unwrap();
    assert_eq!(claimed.len(), 2);
    // The crashed worker locked its job an hour ago; the other one just now.
    sqlx::query("UPDATE jobs SET locked_at = now() - interval '1 hour' WHERE id = 9001")
        .execute(&pool)
        .await
        .unwrap();
    let reaped = jobs::reap_stale(&pool, Duration::from_secs(30 * 60))
        .await
        .unwrap();
    assert_eq!(reaped, vec![(9001, "test.crashed".to_owned())]);
    let (state, locked_by): (String, Option<String>) =
        sqlx::query_as("SELECT state::text, locked_by FROM jobs WHERE id = 9001")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((state.as_str(), locked_by), ("queued", None));
    let state: String = sqlx::query_scalar("SELECT state::text FROM jobs WHERE id = 9002")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(state, "running", "a live job is left alone");
    assert!(
        jobs::reap_stale(&pool, Duration::from_secs(30 * 60))
            .await
            .unwrap()
            .is_empty()
    );
    db.cleanup().await.unwrap();
}

/// Put the table migration 0871 drops back, as an upgrade from a release before 0871 would find it.
async fn recreate_legacy_table(pool: &sqlx::PgPool) {
    sqlx::raw_sql(
        "CREATE TABLE cinema_download_grants (
             user_id BIGINT PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
             per_day INTEGER NOT NULL DEFAULT 20,
             expires_at TIMESTAMPTZ, note TEXT,
             created_at TIMESTAMPTZ NOT NULL DEFAULT now())",
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn account(pool: &sqlx::PgPool, id: i64) {
    let name = format!("t{id}");
    let ok = sr_db::accounts::create_account(
        pool,
        &sr_db::accounts::NewAccount {
            id,
            person_id: id + 1,
            username: &name,
            display_name: None,
            email: &format!("{name}@example.test"),
            password_hash: "x",
            state: "active",
            email_verified: true,
            initial_credits: 0,
            flags: 0,
        },
        id + 2,
    )
    .await
    .unwrap();
    assert!(ok);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn migration_0871_drops_the_legacy_table_only_when_entitlements_were_backfilled() {
    let sql = include_str!("../migrations/0871_drop_cinema_download_grants.sql");
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let table_exists = || async {
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT to_regclass('cinema_download_grants')::text",
        )
        .fetch_one(&pool)
        .await
        .unwrap()
        .is_some()
    };
    assert!(
        !table_exists().await,
        "the migrated database has no legacy table"
    );

    // Without grants (an empty table) it drops cleanly; without the table it is a no-op.
    recreate_legacy_table(&pool).await;
    sqlx::raw_sql(sql).execute(&pool).await.unwrap();
    assert!(!table_exists().await);
    sqlx::raw_sql(sql).execute(&pool).await.unwrap();

    // A grant newer than every entitlement: the upgrade skipped the backfill, so it refuses.
    account(&pool, 8_710_001).await;
    recreate_legacy_table(&pool).await;
    sqlx::query("INSERT INTO cinema_download_grants (user_id, created_at) VALUES (8710001, now())")
        .execute(&pool)
        .await
        .unwrap();
    let err = sqlx::raw_sql(sql)
        .execute(&pool)
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("run a Phase 12 release first"), "{err}");
    assert!(table_exists().await, "the table stays");

    // Once an entitlement at least as new exists (the 0542 backfill), it drops.
    sqlx::query(
        "INSERT INTO entitlement_grants (id, kind, user_id, source, permanent, starts_ms) \
         VALUES (-8710001, 'patron', 8710001, 'migrated', TRUE, (extract(epoch FROM now()) * 1000)::bigint + 1000)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::raw_sql(sql).execute(&pool).await.unwrap();
    assert!(!table_exists().await);
    db.cleanup().await.unwrap();
}
