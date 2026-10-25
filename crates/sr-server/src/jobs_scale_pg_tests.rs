//! RDY-PERF-003: the batching rules of the jobs at scale, against real Postgres (run with
//! `SR_TEST_DATABASE_URL`, `--ignored`, `--test-threads=1`).

use std::time::{Duration, Instant};

use sqlx::PgPool;
use sr_db::testing::TestDb;

async fn seed_messages(pool: &PgPool, n: i64) -> Vec<i64> {
    sqlx::query("INSERT INTO persons (id) VALUES (1)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) \
         VALUES (1, 1, 'purger', 'purger@example.test', 'x')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO channels (id, type, name) VALUES (10, 1, 'c')")
        .execute(pool)
        .await
        .unwrap();
    // Snowflake-shaped ids inside this month's partition.
    let base: i64 = sqlx::query_scalar(
        "SELECT sr_snowflake_floor(date_trunc('month', now() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC')",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let ids: Vec<i64> = (0..n).map(|i| base + 1000 + i).collect();
    for id in &ids {
        sqlx::query(
            "INSERT INTO messages (id, channel_id, author_id, content, visibility, deleted_at, purge_after) \
             VALUES ($1, 10, 1, 'x', 'user_deleted', now() - interval '40 days', now() - interval '10 days')",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }
    ids
}

/// A message another transaction holds locked is passed by, not waited for; the next run takes it.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_purge_skips_a_locked_message_and_takes_it_next_run() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let ids = seed_messages(&pool, 3).await;
    let mut holder = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM messages WHERE id = $1 FOR UPDATE")
        .bind(ids[1])
        .execute(&mut *holder)
        .await
        .unwrap();

    let storage = sr_storage::ObjectStoreStorage::in_memory();
    let now = chrono_free_now_ms();
    let started = Instant::now();
    let purged = crate::jobs::purge_deleted_messages(&pool, &storage, now)
        .await
        .unwrap();
    assert_eq!(purged, 2, "the locked message is skipped");
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "the batch did not wait for the lock: {:?}",
        started.elapsed()
    );
    let left: Vec<i64> = sqlx::query_scalar("SELECT id FROM messages ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(left, [ids[1]]);

    holder.rollback().await.unwrap();
    let purged = crate::jobs::purge_deleted_messages(&pool, &storage, now)
        .await
        .unwrap();
    assert_eq!(purged, 1);
    db.cleanup().await.unwrap();
}

fn chrono_free_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(0))
        + 1000
}
