//! Job handlers for sr-server (docs/02 section 6). Handlers and boot-time
//! scheduling only; the runner itself is [`sr_db::runner::JobRunner`].
//!
//! Recurring jobs are singletons with reserved ids (below any Snowflake), so
//! re-arming them on every boot is idempotent. A handler re-queues itself on
//! success via [`jobs::reschedule_running`].

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::json;
use sr_db::jobs::{self, Job};
use sr_db::partitions;
use sr_db::runner::{HandlerError, JobRunner};
use sr_db::{DbResult, PgPool};

/// Reserved singleton id for `db.partitions.ensure`.
pub const PARTITIONS_JOB_ID: i64 = 1;
/// Reserved singleton id for `messages.purge_deleted`.
pub const PURGE_JOB_ID: i64 = 2;
/// Kind of the hourly purge of deleted messages (open question 94).
pub const PURGE_DELETED_KIND: &str = "messages.purge_deleted";

/// Daily run time for partition maintenance, UTC.
const PARTITIONS_HOUR_UTC: u32 = 0;
const PARTITIONS_MINUTE_UTC: u32 = 10;
const PURGE_INTERVAL: Duration = Duration::from_secs(3600);

/// Rows deleted per statement; the handler loops over batches.
const PURGE_BATCH: i64 = 1000;
/// Upper bound on batches per run so one tick cannot monopolise the database.
const PURGE_MAX_BATCHES: usize = 100;

/// Deletes purge-due messages and their edit history in one statement.
///
/// Messages under a moderation hold are never purged. `message_edits` has no
/// foreign key to the partitioned `messages` table, so it is deleted explicitly
/// by the same statement (open question 94). The data-modifying CTEs run in a
/// single statement, so the two deletes commit together.
const PURGE_SQL: &str = "\
WITH doomed AS ( \
    DELETE FROM messages WHERE id IN ( \
        SELECT id FROM messages \
        WHERE purge_after IS NOT NULL AND purge_after <= now() AND hold_id IS NULL \
        ORDER BY purge_after \
        LIMIT $1) \
    RETURNING id), \
edits AS ( \
    DELETE FROM message_edits WHERE message_id IN (SELECT id FROM doomed) \
    RETURNING id) \
SELECT (SELECT count(*) FROM doomed)";

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn partitions_delay(now_unix_ms: i64) -> Duration {
    jobs::delay_until_daily_utc(now_unix_ms, PARTITIONS_HOUR_UTC, PARTITIONS_MINUTE_UTC)
}

async fn partitions_handler(pool: PgPool, job: Job) -> Result<(), HandlerError> {
    let created = partitions::run_job(&pool, &job, now_unix_ms()).await?;
    tracing::debug!(partitions = ?created, "message partitions ensured");
    jobs::reschedule_running(&pool, job.id, partitions_delay(now_unix_ms())).await?;
    Ok(())
}

async fn purge_deleted_messages(pool: &PgPool) -> DbResult<i64> {
    let mut total = 0_i64;
    for _ in 0..PURGE_MAX_BATCHES {
        let n: i64 = sqlx::query_scalar(PURGE_SQL)
            .bind(PURGE_BATCH)
            .fetch_one(pool)
            .await?;
        total += n;
        if n < PURGE_BATCH {
            break;
        }
    }
    Ok(total)
}

async fn purge_handler(pool: PgPool, job: Job) -> Result<(), HandlerError> {
    let purged = purge_deleted_messages(&pool).await?;
    if purged > 0 {
        tracing::info!(purged, "purged deleted messages");
    }
    jobs::reschedule_running(&pool, job.id, PURGE_INTERVAL).await?;
    Ok(())
}

/// Register every sr-server job kind on `runner`.
pub fn register(runner: JobRunner, pool: &PgPool) -> JobRunner {
    let partitions_pool = pool.clone();
    let purge_pool = pool.clone();
    runner
        .register(partitions::JOB_KIND, 1, move |job| {
            Box::pin(partitions_handler(partitions_pool.clone(), job))
        })
        .register(PURGE_DELETED_KIND, 1, move |job| {
            Box::pin(purge_handler(purge_pool.clone(), job))
        })
}

/// Create or re-arm the recurring singleton jobs. Call once on boot, before the
/// runner starts. Safe to call on every boot.
pub async fn schedule_recurring(pool: &PgPool) -> DbResult<()> {
    let payload = json!({
        "table": "messages",
        "months_ahead": partitions::DEFAULT_MONTHS_AHEAD,
    });
    jobs::schedule_singleton(
        pool,
        PARTITIONS_JOB_ID,
        partitions::JOB_KIND,
        &payload,
        partitions_delay(now_unix_ms()),
    )
    .await?;
    jobs::schedule_singleton(
        pool,
        PURGE_JOB_ID,
        PURGE_DELETED_KIND,
        &json!({}),
        Duration::ZERO,
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partitions_run_at_0010_utc() {
        let midnight = 1_772_323_200_000_i64;
        assert_eq!(partitions_delay(midnight), Duration::from_secs(600));
    }
}
