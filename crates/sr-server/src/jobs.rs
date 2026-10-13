//! Job handlers for sr-server (docs/02 section 6). Handlers and boot-time
//! scheduling only; the runner itself is [`sr_db::runner::JobRunner`].
//!
//! Recurring jobs are singletons with reserved ids (below any Snowflake), so
//! re-arming them on every boot is idempotent. A handler re-queues itself on
//! success via [`jobs::reschedule_running`].

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::json;
use sr_chat::settings::audit::{AUDIT_RETENTION_SETTING, AuditRetention};
use sr_chat::settings::repo::AuditRepository;
use sr_db::jobs::{self, Job};
use sr_db::partitions;
use sr_db::runner::{HandlerError, JobRunner};
use sr_db::settings_repo::PgSettingsRepo;
use sr_db::{DbResult, PgPool};
use sr_storage::ObjectStorage;
use std::sync::Arc;

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

/// Reserved singleton id for `attachments.gc_unreferenced`.
pub const ATTACHMENT_GC_JOB_ID: i64 = 3;
/// Kind of the hourly sweep of abandoned upload slots (open question 153).
pub const ATTACHMENT_GC_KIND: &str = "attachments.gc_unreferenced";
const ATTACHMENT_GC_INTERVAL: Duration = Duration::from_secs(3600);
/// Pending slots older than this are abandoned.
const ATTACHMENT_GC_AGE_MS: i64 = 24 * 60 * 60 * 1000;
const ATTACHMENT_GC_BATCH: i64 = 500;

/// Deletes pending upload slots older than 24 hours and their stored objects. An
/// object that fails to delete keeps its row, so the next run retries it.
async fn attachment_gc_handler(
    pool: PgPool,
    storage: Arc<dyn ObjectStorage>,
    job: Job,
) -> Result<(), HandlerError> {
    let cutoff = now_unix_ms() - ATTACHMENT_GC_AGE_MS;
    let stale =
        sr_db::chat_repo::stale_pending_attachments(&pool, cutoff, ATTACHMENT_GC_BATCH).await?;
    let mut removable = Vec::with_capacity(stale.len());
    for (id, key) in stale {
        match storage.delete(&key).await {
            Ok(()) => removable.push(id),
            Err(e) => {
                tracing::warn!(error = %e, key = %key, "deleting an abandoned upload failed; retrying later");
            }
        }
    }
    let removed = sr_db::chat_repo::delete_attachment_rows(&pool, &removable).await?;
    if removed > 0 {
        tracing::info!(removed, "garbage-collected abandoned uploads");
    }
    jobs::reschedule_running(&pool, job.id, ATTACHMENT_GC_INTERVAL).await?;
    Ok(())
}

/// Register the attachment sweep on `runner`. It needs the object store, so it is
/// separate from [`register`]. Pair with [`schedule_attachment_gc`].
pub fn register_attachment_gc(
    runner: JobRunner,
    pool: &PgPool,
    storage: Arc<dyn ObjectStorage>,
) -> JobRunner {
    let pool = pool.clone();
    runner.register(ATTACHMENT_GC_KIND, 1, move |job| {
        Box::pin(attachment_gc_handler(pool.clone(), storage.clone(), job))
    })
}

/// Create or re-arm the attachment sweep singleton. Call only when
/// [`register_attachment_gc`] is on the runner, or the job is dead-lettered.
pub async fn schedule_attachment_gc(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(
        pool,
        ATTACHMENT_GC_JOB_ID,
        ATTACHMENT_GC_KIND,
        &json!({}),
        Duration::ZERO,
    )
    .await?;
    Ok(())
}
/// Reserved singleton id for `audit.purge`.
pub const AUDIT_PURGE_JOB_ID: i64 = 4;
/// Kind of the daily guild audit-log retention purge (open question 133).
pub const AUDIT_PURGE_KIND: &str = "audit.purge";
const AUDIT_PURGE_HOUR_UTC: u32 = 3;
const AUDIT_PURGE_MINUTE_UTC: u32 = 30;

/// Entries created before the returned instant are purged; `None` keeps everything.
/// `setting` is the stored `moderation.audit_retention_days` value (default 45,
/// `0` = forever).
fn audit_cutoff(setting: Option<&serde_json::Value>, now_ms: i64) -> Option<i64> {
    match AuditRetention::from_setting(setting) {
        Ok(r) => r.cutoff_ms(now_ms),
        Err(e) => {
            // A bad operator value must never cause an unintended purge.
            tracing::warn!(error = %e, "invalid moderation.audit_retention_days; skipping audit purge");
            None
        }
    }
}

fn audit_purge_delay() -> Duration {
    jobs::delay_until_daily_utc(now_unix_ms(), AUDIT_PURGE_HOUR_UTC, AUDIT_PURGE_MINUTE_UTC)
}

async fn audit_purge_handler(pool: PgPool, job: Job) -> Result<(), HandlerError> {
    let setting: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT value FROM platform_settings WHERE key = $1")
            .bind(AUDIT_RETENTION_SETTING)
            .fetch_optional(&pool)
            .await?;
    if let Some(cutoff) = audit_cutoff(setting.as_ref(), now_unix_ms()) {
        let purged = PgSettingsRepo::new(pool.clone())
            .purge_audit_before(cutoff)
            .await?;
        if purged > 0 {
            tracing::info!(purged, "purged expired audit log entries");
        }
    }
    jobs::reschedule_running(&pool, job.id, audit_purge_delay()).await?;
    Ok(())
}

/// Register the audit-log retention purge on `runner`. Pair with
/// [`schedule_audit_purge`].
pub fn register_audit_purge(runner: JobRunner, pool: &PgPool) -> JobRunner {
    let pool = pool.clone();
    runner.register(AUDIT_PURGE_KIND, 1, move |job| {
        Box::pin(audit_purge_handler(pool.clone(), job))
    })
}

/// Create or re-arm the audit purge singleton. Call only when
/// [`register_audit_purge`] is on the runner.
pub async fn schedule_audit_purge(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(
        pool,
        AUDIT_PURGE_JOB_ID,
        AUDIT_PURGE_KIND,
        &json!({}),
        audit_purge_delay(),
    )
    .await?;
    Ok(())
}

/// Reserved singleton id for `threads.auto_archive`.
pub const THREAD_ARCHIVE_JOB_ID: i64 = 110;
/// Kind of the recurring sweep that archives idle threads (LNG-THR-001).
pub const THREAD_ARCHIVE_KIND: &str = "threads.auto_archive";
/// The shortest auto-archive duration is an hour, so a five-minute sweep is plenty.
const THREAD_ARCHIVE_INTERVAL: Duration = Duration::from_secs(300);

async fn thread_archive_handler(
    pool: PgPool,
    threads: Arc<sr_api::routes::threads::ThreadsDeps>,
    job: Job,
) -> Result<(), HandlerError> {
    // Loop until a short batch: a backlog after downtime clears in one run.
    for _ in 0..50 {
        let n = threads
            .auto_archive()
            .await
            .map_err(|e| HandlerError::from(format!("auto-archive failed: {e}")))?;
        if n > 0 {
            tracing::info!(archived = n, "auto-archived idle threads");
        }
        if n < sr_api::routes::threads::deps::AUTO_ARCHIVE_BATCH {
            break;
        }
    }
    jobs::reschedule_running(&pool, job.id, THREAD_ARCHIVE_INTERVAL).await?;
    Ok(())
}

/// Register the idle-thread sweep on `runner`. Pair with [`schedule_thread_archive`].
pub fn register_thread_archive(
    runner: JobRunner,
    pool: &PgPool,
    threads: Arc<sr_api::routes::threads::ThreadsDeps>,
) -> JobRunner {
    let pool = pool.clone();
    runner.register(THREAD_ARCHIVE_KIND, 1, move |job| {
        Box::pin(thread_archive_handler(pool.clone(), threads.clone(), job))
    })
}

/// Create or re-arm the idle-thread sweep singleton. Call only when
/// [`register_thread_archive`] is on the runner.
pub async fn schedule_thread_archive(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(
        pool,
        THREAD_ARCHIVE_JOB_ID,
        THREAD_ARCHIVE_KIND,
        &json!({}),
        THREAD_ARCHIVE_INTERVAL,
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_retention_setting_drives_the_cutoff() {
        let now = 100 * 86_400_000_i64;
        // Missing setting: the 45-day default.
        assert_eq!(audit_cutoff(None, now), Some(55 * 86_400_000));
        assert_eq!(audit_cutoff(Some(&json!(7)), now), Some(93 * 86_400_000));
        // 0 keeps entries forever.
        assert_eq!(audit_cutoff(Some(&json!(0)), now), None);
        // Garbage never purges.
        assert_eq!(audit_cutoff(Some(&json!("soon")), now), None);
    }

    #[test]
    fn partitions_run_at_0010_utc() {
        let midnight = 1_772_323_200_000_i64;
        assert_eq!(partitions_delay(midnight), Duration::from_secs(600));
    }
}
