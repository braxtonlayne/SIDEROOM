//! Bounded transactions for background jobs at scale (RDY-PERF-003).
//!
//! A job that works through a big table does it in batches, and each batch runs in a transaction opened here:
//! `lock_timeout` makes a statement that would wait for a row lock for more than a second fail (and the job
//! retry on its next tick) instead of queueing behind a member's edit, and `statement_timeout` stops one
//! runaway statement from holding its locks for minutes. Both are `SET LOCAL`, so they end with the
//! transaction and never leak onto the pooled connection.

use sqlx::{PgPool, Postgres, Transaction};

use crate::DbResult;

/// A batch statement that has to wait this long for a lock gives up (the one-second rule of the roadmap).
pub const LOCK_TIMEOUT_MS: u32 = 1_000;
/// One batch statement may run this long.
pub const STATEMENT_TIMEOUT_MS: u32 = 30_000;

/// Begin a transaction with [`LOCK_TIMEOUT_MS`] and [`STATEMENT_TIMEOUT_MS`] applied to it.
pub async fn begin(pool: &PgPool) -> DbResult<Transaction<'static, Postgres>> {
    begin_with(pool, LOCK_TIMEOUT_MS, STATEMENT_TIMEOUT_MS).await
}

/// [`begin`] with explicit limits (milliseconds; `0` disables that limit).
pub async fn begin_with(
    pool: &PgPool,
    lock_ms: u32,
    statement_ms: u32,
) -> DbResult<Transaction<'static, Postgres>> {
    let mut tx = pool.begin().await?;
    // set_config(..., true) is SET LOCAL as a single statement (a prepared statement takes only one).
    sqlx::query(
        "SELECT set_config('lock_timeout', $1, true), set_config('statement_timeout', $2, true)",
    )
    .bind(lock_ms.to_string())
    .bind(statement_ms.to_string())
    .execute(&mut *tx)
    .await?;
    Ok(tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestDb;

    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn the_limits_apply_to_the_transaction_only() {
        let db = TestDb::new().await.unwrap();
        let pool = db.pool().clone();
        let mut tx = begin_with(&pool, 250, 5_000).await.unwrap();
        let (lock, stmt): (String, String) = sqlx::query_as(
            "SELECT current_setting('lock_timeout'), current_setting('statement_timeout')",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!((lock.as_str(), stmt.as_str()), ("250ms", "5s"));
        tx.rollback().await.unwrap();
        // The pooled connection is back to the defaults.
        let lock: String = sqlx::query_scalar("SELECT current_setting('lock_timeout')")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(lock, "0");
        db.cleanup().await.unwrap();
    }

    /// A statement that waits on a held row lock fails after the lock timeout instead of queueing.
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn a_blocked_batch_gives_up_instead_of_waiting() {
        let db = TestDb::new().await.unwrap();
        let pool = db.pool().clone();
        sqlx::query("CREATE TABLE bounded_t (id INT PRIMARY KEY)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO bounded_t VALUES (1)")
            .execute(&pool)
            .await
            .unwrap();
        let mut holder = pool.begin().await.unwrap();
        sqlx::query("SELECT id FROM bounded_t WHERE id = 1 FOR UPDATE")
            .execute(&mut *holder)
            .await
            .unwrap();
        let mut batch = begin_with(&pool, 200, 5_000).await.unwrap();
        let started = std::time::Instant::now();
        let blocked = sqlx::query("DELETE FROM bounded_t WHERE id = 1")
            .execute(&mut *batch)
            .await;
        assert!(blocked.is_err(), "the delete should hit the lock timeout");
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
        batch.rollback().await.unwrap();
        // SKIP LOCKED passes the same row by.
        let mut batch = begin_with(&pool, 200, 5_000).await.unwrap();
        let free: Vec<i32> = sqlx::query_scalar("SELECT id FROM bounded_t FOR UPDATE SKIP LOCKED")
            .fetch_all(&mut *batch)
            .await
            .unwrap();
        assert!(free.is_empty());
        batch.rollback().await.unwrap();
        holder.rollback().await.unwrap();
        db.cleanup().await.unwrap();
    }
}
