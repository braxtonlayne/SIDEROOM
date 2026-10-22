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

/// Message ids due for purge at `$1` (Unix ms), oldest first, at most `$2`.
///
/// Messages under a moderation hold are never purged, nor is a message with a held
/// attachment (deleting it would cascade to the held row; D-122, XC-MOD-001).
pub(crate) const PURGE_DUE_SQL: &str = "\
SELECT id FROM messages \
WHERE purge_after IS NOT NULL \
  AND purge_after <= to_timestamp($1::double precision / 1000.0) AND hold_id IS NULL \
  AND NOT EXISTS (SELECT 1 FROM attachments a \
                  WHERE a.message_id = messages.id AND a.hold_id IS NOT NULL) \
ORDER BY purge_after \
LIMIT $2";

/// Deletes the messages `$2` (chosen by [`PURGE_DUE_SQL`]) and their edit history in one
/// statement, re-checking the due and hold conditions against `$1` (Unix ms) so a hold
/// placed after the selection still wins. `message_edits` has no foreign key to the
/// partitioned `messages` table, so it is deleted explicitly by the same statement
/// (open question 94); `message_webhook_authors` has none either and goes the same way
/// (D-235). The data-modifying CTEs commit together; attachment rows go
/// with the message by cascade, after their stored objects were removed.
pub(crate) const PURGE_SQL: &str = "\
WITH doomed AS ( \
    DELETE FROM messages WHERE id = ANY($2) \
      AND purge_after IS NOT NULL \
      AND purge_after <= to_timestamp($1::double precision / 1000.0) AND hold_id IS NULL \
      AND NOT EXISTS (SELECT 1 FROM attachments a \
                      WHERE a.message_id = messages.id AND a.hold_id IS NOT NULL) \
    RETURNING id), \
edits AS ( \
    DELETE FROM message_edits WHERE message_id IN (SELECT id FROM doomed) \
    RETURNING id), \
webhook_authors AS ( \
    DELETE FROM message_webhook_authors WHERE message_id IN (SELECT id FROM doomed) \
    RETURNING message_id) \
SELECT (SELECT count(*) FROM doomed)";

/// Hard-deletes soft-deleted guilds (LNG-SSET-073) whose messages are all gone. Held
/// messages are never purged (D-122), so a guild that still owns one stays; the cascade
/// then takes channels, roles and the guild's audit log with it.
pub(crate) const REAP_GUILDS_SQL: &str = "\
DELETE FROM guilds g WHERE g.deleted_at IS NOT NULL AND NOT g.is_staff_guild \
  AND NOT EXISTS (SELECT 1 FROM messages m JOIN channels c ON c.id = m.channel_id \
                  WHERE c.guild_id = g.id)";

async fn reap_deleted_guilds(pool: &PgPool) -> DbResult<u64> {
    Ok(sqlx::query(REAP_GUILDS_SQL)
        .execute(pool)
        .await?
        .rows_affected())
}

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

/// Remove a stored object; a missing object counts as removed (a retry after a
/// partly failed run, or an object that never finished uploading).
async fn remove_object(storage: &dyn ObjectStorage, key: &str) -> bool {
    match storage.delete(key).await {
        Ok(()) | Err(sr_storage::StorageError::NotFound(_)) => true,
        Err(e) => {
            tracing::warn!(error = %e, key, "deleting a stored object failed; retrying later");
            false
        }
    }
}

/// Purge every message due at `now_ms` (Unix ms): the stored attachment objects first,
/// then the message rows, their edit history and attachment rows (LNG-MSG-021).
/// A message whose objects could not all be deleted stays for the next run, so no
/// object is orphaned. Held messages and held attachments are never touched.
pub(crate) async fn purge_deleted_messages(
    pool: &PgPool,
    storage: &dyn ObjectStorage,
    now_ms: i64,
) -> DbResult<i64> {
    let mut total = 0_i64;
    for _ in 0..PURGE_MAX_BATCHES {
        let due: Vec<i64> = sqlx::query_scalar(PURGE_DUE_SQL)
            .bind(now_ms)
            .bind(PURGE_BATCH)
            .fetch_all(pool)
            .await?;
        if due.is_empty() {
            break;
        }
        let keys: Vec<(i64, String)> = sqlx::query_as(
            "SELECT message_id, storage_key FROM attachments \
             WHERE message_id = ANY($1) AND state <> 'pending'",
        )
        .bind(&due)
        .fetch_all(pool)
        .await?;
        let mut failed = std::collections::HashSet::new();
        for (message_id, key) in &keys {
            if !remove_object(storage, key).await {
                failed.insert(*message_id);
            }
        }
        let doomed: Vec<i64> = due
            .iter()
            .copied()
            .filter(|id| !failed.contains(id))
            .collect();
        let n: i64 = sqlx::query_scalar(PURGE_SQL)
            .bind(now_ms)
            .bind(&doomed)
            .fetch_one(pool)
            .await?;
        total += n;
        if n == 0 || i64::try_from(due.len()).unwrap_or(i64::MAX) < PURGE_BATCH {
            break;
        }
    }
    Ok(total)
}

/// Finish the removal of attachments a moderation case took off their message (N-21): no hold covers them any
/// more (a held one keeps its object and row for the evidence, D-122), so the stored object goes and then the
/// row. An object that cannot be deleted leaves its row for the next run. Returns how many rows were removed.
pub(crate) async fn purge_removed_attachments(
    pool: &PgPool,
    storage: &dyn ObjectStorage,
) -> DbResult<i64> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, storage_key FROM attachments \
         WHERE removed_at IS NOT NULL AND message_id IS NULL AND hold_id IS NULL \
         ORDER BY removed_at LIMIT $1",
    )
    .bind(PURGE_BATCH)
    .fetch_all(pool)
    .await?;
    let mut done: Vec<i64> = Vec::new();
    for (id, key) in &rows {
        if remove_object(storage, key).await {
            done.push(*id);
        }
    }
    if done.is_empty() {
        return Ok(0);
    }
    let n = sqlx::query(
        "DELETE FROM attachments WHERE id = ANY($1) AND removed_at IS NOT NULL AND hold_id IS NULL",
    )
    .bind(&done)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(i64::try_from(n).unwrap_or(i64::MAX))
}

async fn purge_handler(
    pool: PgPool,
    storage: Arc<dyn ObjectStorage>,
    job: Job,
) -> Result<(), HandlerError> {
    let purged = purge_deleted_messages(&pool, storage.as_ref(), now_unix_ms()).await?;
    let removed = purge_removed_attachments(&pool, storage.as_ref()).await?;
    if removed > 0 {
        tracing::info!(
            removed,
            "deleted attachments removed through moderation cases"
        );
    }
    let reaped = reap_deleted_guilds(&pool).await?;
    if reaped > 0 {
        tracing::info!(reaped, "removed deleted guilds");
    }
    if purged > 0 {
        tracing::info!(purged, "purged deleted messages");
    }
    jobs::reschedule_running(&pool, job.id, PURGE_INTERVAL).await?;
    Ok(())
}

/// Reserved singleton id for `attachments.expire_large`.
pub const EXPIRE_ATTACHMENTS_JOB_ID: i64 = 5;
/// Kind of the hourly expiry of large attachments (D-043, LNG-MSG-022).
pub const EXPIRE_ATTACHMENTS_KIND: &str = "attachments.expire_large";
const EXPIRE_BATCH: i64 = 200;
const EXPIRE_MAX_BATCHES: usize = 50;

/// A large attachment that just expired: where to announce the message update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExpiredAttachment {
    pub channel_id: i64,
    pub message_id: i64,
}

/// Expire every large attachment whose `expires_at` is at or before `now_ms` (Unix ms):
/// delete the stored object, mark the row `expired` and report its message so the
/// caller can dispatch `MESSAGE_UPDATE`. Attachments under a hold (their own, or their
/// message's) keep their object (D-122). Returns the distinct affected messages.
pub(crate) async fn expire_large_attachments(
    pool: &PgPool,
    storage: &dyn ObjectStorage,
    now_ms: i64,
) -> DbResult<Vec<ExpiredAttachment>> {
    let mut out: Vec<ExpiredAttachment> = Vec::new();
    for _ in 0..EXPIRE_MAX_BATCHES {
        let rows: Vec<(i64, i64, i64, String)> = sqlx::query_as(
            "SELECT a.id, a.message_id, m.channel_id, a.storage_key \
             FROM attachments a JOIN messages m ON m.id = a.message_id \
             WHERE a.is_large AND a.state = 'ready' AND a.hold_id IS NULL AND m.hold_id IS NULL \
               AND a.expires_at <= to_timestamp($1::double precision / 1000.0) \
             ORDER BY a.expires_at LIMIT $2",
        )
        .bind(now_ms)
        .bind(EXPIRE_BATCH)
        .fetch_all(pool)
        .await?;
        if rows.is_empty() {
            break;
        }
        let mut done: Vec<i64> = Vec::new();
        for (id, message_id, channel_id, key) in &rows {
            if remove_object(storage, key).await {
                done.push(*id);
                let hit = ExpiredAttachment {
                    channel_id: *channel_id,
                    message_id: *message_id,
                };
                if !out.contains(&hit) {
                    out.push(hit);
                }
            }
        }
        if done.is_empty() {
            break;
        }
        sqlx::query(
            "UPDATE attachments SET state = 'expired', updated_at = now() \
             WHERE id = ANY($1) AND state = 'ready' AND hold_id IS NULL",
        )
        .bind(&done)
        .execute(pool)
        .await?;
        if i64::try_from(rows.len()).unwrap_or(i64::MAX) < EXPIRE_BATCH {
            break;
        }
    }
    Ok(out)
}

async fn expire_attachments_handler(
    pool: PgPool,
    storage: Arc<dyn ObjectStorage>,
    messages: Arc<sr_api::routes::messages::MessagesDeps>,
    job: Job,
) -> Result<(), HandlerError> {
    let expired = expire_large_attachments(&pool, storage.as_ref(), now_unix_ms()).await?;
    for e in &expired {
        let (Ok(channel), Ok(message)) = (u64::try_from(e.channel_id), u64::try_from(e.message_id))
        else {
            continue;
        };
        if let Err(err) = sr_api::routes::messages::rich_jobs::announce_message_update(
            &messages,
            sr_core::ChannelId::new(channel),
            sr_core::MessageId::new(message),
        )
        .await
        {
            tracing::warn!(error = ?err, "announcing an expired attachment failed");
        }
    }
    if !expired.is_empty() {
        tracing::info!(messages = expired.len(), "expired large attachments");
    }
    jobs::reschedule_running(&pool, job.id, PURGE_INTERVAL).await?;
    Ok(())
}

/// Register every sr-server job kind on `runner`.
pub fn register(
    runner: JobRunner,
    pool: &PgPool,
    storage: Arc<dyn ObjectStorage>,
    messages: Arc<sr_api::routes::messages::MessagesDeps>,
) -> JobRunner {
    let partitions_pool = pool.clone();
    let purge_pool = pool.clone();
    let expire_pool = pool.clone();
    let purge_storage = storage.clone();
    runner
        .register(partitions::JOB_KIND, 1, move |job| {
            Box::pin(partitions_handler(partitions_pool.clone(), job))
        })
        .register(PURGE_DELETED_KIND, 1, move |job| {
            Box::pin(purge_handler(
                purge_pool.clone(),
                purge_storage.clone(),
                job,
            ))
        })
        .register(EXPIRE_ATTACHMENTS_KIND, 1, move |job| {
            Box::pin(expire_attachments_handler(
                expire_pool.clone(),
                storage.clone(),
                messages.clone(),
                job,
            ))
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
    jobs::schedule_singleton(
        pool,
        EXPIRE_ATTACHMENTS_JOB_ID,
        EXPIRE_ATTACHMENTS_KIND,
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
    parts: Option<Arc<dyn sr_library::multipart::PartStore>>,
    job: Job,
) -> Result<(), HandlerError> {
    let cutoff = now_unix_ms() - ATTACHMENT_GC_AGE_MS;
    let stale =
        sr_db::chat_repo::stale_pending_attachments(&pool, cutoff, ATTACHMENT_GC_BATCH).await?;
    // Free the parts of multipart uploads that were never completed (LNG-CMP-007).
    let ids: Vec<_> = stale.iter().map(|(id, _)| *id).collect();
    let multipart = sr_db::chat_repo::attachment_multipart_ids(&pool, &ids).await?;
    let mut removable = Vec::with_capacity(stale.len());
    for (id, key) in stale {
        if let (Some(store), Some((_, mp))) = (&parts, multipart.iter().find(|(m, _)| *m == id))
            && let Err(e) = store.abort(&key, mp).await
        {
            tracing::warn!(error = %e, key = %key, "aborting an abandoned multipart upload failed; retrying later");
            continue;
        }
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
    parts: Option<Arc<dyn sr_library::multipart::PartStore>>,
) -> JobRunner {
    let pool = pool.clone();
    runner.register(ATTACHMENT_GC_KIND, 1, move |job| {
        Box::pin(attachment_gc_handler(
            pool.clone(),
            storage.clone(),
            parts.clone(),
            job,
        ))
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

/// Reserved singleton id for `jobs.reap`.
pub const REAP_JOB_ID: i64 = 6;
/// Kind of the hourly return of crashed jobs to the queue (H-15).
pub const REAP_KIND: &str = "jobs.reap";
const REAP_INTERVAL: Duration = Duration::from_secs(3600);
/// A job `running` longer than this belongs to a crashed worker.
pub const REAP_THRESHOLD: Duration = Duration::from_secs(30 * 60);

/// Return jobs left `running` by a crashed node to the queue, logging each one.
pub async fn reap_stale_jobs(pool: &PgPool) -> DbResult<usize> {
    let reaped = jobs::reap_stale(pool, REAP_THRESHOLD).await?;
    for (id, kind) in &reaped {
        tracing::warn!(job_id = id, kind = %kind, "returned a stale running job to the queue");
    }
    Ok(reaped.len())
}

async fn reap_handler(pool: PgPool, job: Job) -> Result<(), HandlerError> {
    reap_stale_jobs(&pool).await?;
    jobs::reschedule_running(&pool, job.id, REAP_INTERVAL).await?;
    Ok(())
}

/// Register the hourly `jobs.reap` singleton on `runner`. Pair with [`schedule_reap`].
pub fn register_reap(runner: JobRunner, pool: &PgPool) -> JobRunner {
    let pool = pool.clone();
    runner.register(REAP_KIND, 1, move |job| {
        Box::pin(reap_handler(pool.clone(), job))
    })
}

/// Create or re-arm the reap singleton. Call only when [`register_reap`] is on the runner.
pub async fn schedule_reap(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(pool, REAP_JOB_ID, REAP_KIND, &json!({}), REAP_INTERVAL).await?;
    Ok(())
}

/// Reserved singleton id for `insights.daily`.
pub const INSIGHTS_JOB_ID: i64 = 630;
/// Kind of the daily Server Insights computation (LNG-SSET-068).
pub const INSIGHTS_KIND: &str = "insights.daily";
const INSIGHTS_HOUR_UTC: u32 = 0;
const INSIGHTS_MINUTE_UTC: u32 = 20;
/// Completed days recomputed on each run, so a missed run heals itself.
const INSIGHTS_CATCH_UP_DAYS: i64 = 2;

fn insights_delay() -> Duration {
    jobs::delay_until_daily_utc(now_unix_ms(), INSIGHTS_HOUR_UTC, INSIGHTS_MINUTE_UTC)
}

async fn insights_handler(pool: PgPool, job: Job) -> Result<(), HandlerError> {
    const DAY_MS: i64 = 86_400_000;
    let today = now_unix_ms().div_euclid(DAY_MS) * DAY_MS;
    for back in (1..=INSIGHTS_CATCH_UP_DAYS).rev() {
        let guilds = sr_db::settings_repo::compute_insights_day(&pool, today - back * DAY_MS)
            .await
            .map_err(|e| HandlerError::from(e.to_string()))?;
        tracing::debug!(guilds, back, "server insights computed");
    }
    jobs::reschedule_running(&pool, job.id, insights_delay()).await?;
    Ok(())
}

/// Register the daily Server Insights job on `runner`. Pair with [`schedule_insights`].
pub fn register_insights(runner: JobRunner, pool: &PgPool) -> JobRunner {
    let pool = pool.clone();
    runner.register(INSIGHTS_KIND, 1, move |job| {
        Box::pin(insights_handler(pool.clone(), job))
    })
}

/// Create or re-arm the insights singleton. Call only when [`register_insights`] is on
/// the runner.
pub async fn schedule_insights(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(
        pool,
        INSIGHTS_JOB_ID,
        INSIGHTS_KIND,
        &json!({}),
        insights_delay(),
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

    /// F-13: the setting the purge job reads is declared in the platform settings
    /// catalog, so Admin -> Limits renders and validates it (0 = forever, default 45).
    #[test]
    fn audit_retention_is_editable_in_admin_limits() {
        let def = sr_admin::settings::find(sr_chat::settings::audit::AUDIT_RETENTION_SETTING)
            .expect("declared in the Admin settings catalog");
        assert_eq!(def.default_value(), json!(45));
        assert_eq!(
            def.default_value(),
            json!(sr_chat::settings::audit::AuditRetention::default().keep_days)
        );
        assert_eq!(def.group, "retention");
        assert!(def.validate(&json!(0)).is_ok(), "0 keeps entries forever");
        assert!(def.validate(&json!(-1)).is_err());
        assert!(def.validate(&json!("soon")).is_err());
        // What the catalog accepts, the job understands.
        assert_eq!(
            audit_cutoff(Some(&json!(30)), 100 * 86_400_000),
            Some(70 * 86_400_000)
        );
    }

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

/// XC-MOD-001: the retention purge skips anything under a moderation hold (D-122).
#[cfg(test)]
mod hold_guard_tests {
    use super::*;
    use sr_db::testing::TestDb;

    async fn exec(pool: &PgPool, sql: &str, binds: &[i64]) {
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()));
        for b in binds {
            q = q.bind(*b);
        }
        q.execute(pool).await.expect(sql);
    }

    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn purge_skips_held_messages_and_messages_with_held_attachments() {
        let db = TestDb::new().await.expect("test db");
        let pool = db.pool();
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
        // Snowflakes in the current month: ids embed the time.
        let base: i64 =
            sqlx::query_scalar("SELECT sr_snowflake_floor(date_trunc('month', now())) + 1000")
                .fetch_one(pool)
                .await
                .expect("floor");
        let (plain, held, held_att, kept) = (base, base + 1, base + 2, base + 3);
        for id in [plain, held, held_att, kept] {
            exec(
                pool,
                "INSERT INTO messages (id, channel_id, author_id, visibility, deleted_at, purge_after, hold_id) \
                 VALUES ($1, 10, 1, 'user_deleted', now() - interval '31 days', \
                         now() - interval '1 day', NULL)",
                &[id],
            )
            .await;
        }
        // `kept` is not yet due.
        exec(
            pool,
            "UPDATE messages SET purge_after = now() + interval '1 day' WHERE id = $1",
            &[kept],
        )
        .await;
        exec(
            pool,
            "UPDATE messages SET hold_id = 7 WHERE id = $1",
            &[held],
        )
        .await;
        exec(
            pool,
            "INSERT INTO attachments (id, message_id, uploader_id, filename, size_bytes, storage_key, hold_id) \
             VALUES (100, $1, 1, 'a.png', 1, 'k', 7)",
            &[held_att],
        )
        .await;

        for id in [plain, held, kept] {
            exec(
                pool,
                "INSERT INTO message_webhook_authors (message_id, webhook_id, username) \
                 VALUES ($1, 5, 'Deploys')",
                &[id],
            )
            .await;
        }
        let purged = purge_deleted_messages(
            pool,
            &sr_storage::ObjectStoreStorage::in_memory(),
            now_unix_ms(),
        )
        .await
        .expect("purge");
        assert_eq!(purged, 1, "only the plain, due, unheld message goes");
        let authors: Vec<i64> =
            sqlx::query_scalar("SELECT message_id FROM message_webhook_authors ORDER BY 1")
                .fetch_all(pool)
                .await
                .expect("authors");
        assert_eq!(
            authors,
            vec![held, kept],
            "the purged message loses its webhook author row (D-235), the others keep theirs"
        );
        let left: Vec<i64> = sqlx::query_scalar("SELECT id FROM messages ORDER BY id")
            .fetch_all(pool)
            .await
            .expect("ids");
        assert_eq!(left, vec![held, held_att, kept]);
        let atts: i64 = sqlx::query_scalar("SELECT count(*) FROM attachments WHERE id = 100")
            .fetch_one(pool)
            .await
            .expect("count");
        assert_eq!(atts, 1, "the held attachment row survives");

        // Lifting the hold makes the message purgeable again.
        exec(
            pool,
            "UPDATE attachments SET hold_id = NULL WHERE id = 100",
            &[],
        )
        .await;
        exec(
            pool,
            "UPDATE messages SET hold_id = NULL WHERE id = $1",
            &[held],
        )
        .await;
        assert_eq!(
            purge_deleted_messages(
                pool,
                &sr_storage::ObjectStoreStorage::in_memory(),
                now_unix_ms()
            )
            .await
            .expect("purge"),
            2
        );
        let authors: Vec<i64> =
            sqlx::query_scalar("SELECT message_id FROM message_webhook_authors ORDER BY 1")
                .fetch_all(pool)
                .await
                .expect("authors");
        assert_eq!(
            authors,
            vec![kept],
            "the released message's row goes with it"
        );
        db.cleanup().await.expect("cleanup");
    }

    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn upload_gc_and_row_delete_skip_held_attachments() {
        let db = TestDb::new().await.expect("test db");
        let pool = db.pool();
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
        for (id, hold) in [(1_i64, "NULL"), (2, "7")] {
            exec(
                pool,
                &format!(
                    "INSERT INTO attachments (id, uploader_id, filename, size_bytes, storage_key, hold_id, created_at) \
                     VALUES ({id}, 1, 'a.png', 1, 'k{id}', {hold}, now() - interval '2 days')"
                ),
                &[],
            )
            .await;
        }
        let stale = sr_db::chat_repo::stale_pending_attachments(pool, now_unix_ms(), 10)
            .await
            .expect("stale");
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].0.get(), 1);
        let ids = [sr_core::AttachmentId::new(1), sr_core::AttachmentId::new(2)];
        let removed = sr_db::chat_repo::delete_attachment_rows(pool, &ids)
            .await
            .expect("delete");
        assert_eq!(removed, 1, "the held row is not deletable by the sweep");
        db.cleanup().await.expect("cleanup");
    }

    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn reap_removes_deleted_guilds_only_once_their_messages_are_gone() {
        let db = TestDb::new().await.expect("test db");
        let pool = db.pool();
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
        // 20: deleted and empty; 21: deleted with a held message; 22: not deleted.
        for (gid, deleted) in [(20_i64, "now()"), (21, "now()"), (22, "NULL")] {
            exec(
                pool,
                &format!(
                    "INSERT INTO guilds (id, name, owner_id, deleted_at) \
                     VALUES ({gid}, 'guild{gid}', 1, {deleted})"
                ),
                &[],
            )
            .await;
            exec(
                pool,
                &format!(
                    "INSERT INTO channels (id, guild_id, type) VALUES ({}, {gid}, 0)",
                    gid * 10
                ),
                &[],
            )
            .await;
        }
        let _: String = sqlx::query_scalar("SELECT sr_ensure_message_partition(CURRENT_DATE)")
            .fetch_one(pool)
            .await
            .expect("partition");
        let base: i64 =
            sqlx::query_scalar("SELECT sr_snowflake_floor(date_trunc('month', now())) + 2000")
                .fetch_one(pool)
                .await
                .expect("floor");
        exec(
            pool,
            "INSERT INTO messages (id, channel_id, author_id, visibility, deleted_at, hold_id) \
             VALUES ($1, 210, 1, 'user_deleted', now(), 7)",
            &[base],
        )
        .await;
        assert_eq!(reap_deleted_guilds(pool).await.expect("reap"), 1);
        let left: Vec<i64> = sqlx::query_scalar("SELECT id FROM guilds ORDER BY id")
            .fetch_all(pool)
            .await
            .expect("ids");
        assert_eq!(
            left,
            vec![21, 22],
            "held message keeps its guild; live guild stays"
        );
        db.cleanup().await.expect("cleanup");
    }
}

/// MOD-HOLD-003/005: a hold placed and released through `sr-moderation` (the real hold implementation) is
/// honoured by the purge, and releasing makes a message that was deleted while held purgeable at the next run.
#[cfg(test)]
mod moderation_release_tests {
    use super::*;
    use sr_db::testing::TestDb;
    use sr_moderation::{
        HoldScope, Moderation,
        model::{CaseSeverity, NewCase, ReportCategory, TargetKind, Who},
    };

    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn release_makes_a_message_deleted_while_held_purgeable() {
        let db = TestDb::new().await.expect("test db");
        let pool = db.pool();
        for (id, name) in [(1_i64, "author"), (2, "staff")] {
            sqlx::query("INSERT INTO persons (id) VALUES ($1)")
                .bind(id)
                .execute(pool)
                .await
                .expect("person");
            sqlx::query(
                "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $1, $2, $3, 'x')",
            )
            .bind(id)
            .bind(name)
            .bind(format!("{name}@example.test"))
            .execute(pool)
            .await
            .expect("account");
        }
        sqlx::query("INSERT INTO channels (id, type) VALUES (10, 1)")
            .execute(pool)
            .await
            .expect("channel");
        let _: String = sqlx::query_scalar("SELECT sr_ensure_message_partition(CURRENT_DATE)")
            .fetch_one(pool)
            .await
            .expect("partition");
        let msg: i64 =
            sqlx::query_scalar("SELECT sr_snowflake_floor(date_trunc('month', now())) + 1000")
                .fetch_one(pool)
                .await
                .expect("floor");
        // A visible message, held by a case, then deleted by its author: purge_after stays empty (D-122).
        sqlx::query(
            "INSERT INTO messages (id, channel_id, author_id, content) VALUES ($1, 10, 1, 'x')",
        )
        .bind(msg)
        .execute(pool)
        .await
        .expect("message");

        let ids = std::sync::atomic::AtomicI64::new(7_000_000);
        let moderation = Moderation::new(
            pool.clone(),
            std::sync::Arc::new(move || ids.fetch_add(1, std::sync::atomic::Ordering::Relaxed)),
        );
        let staff = sr_admin::Caller {
            user_id: 2,
            permissions: sr_core::PlatformPermissions::MANAGE_CASES,
            is_owner: false,
            ip: None,
        };
        let who = Who::Staff(&staff);
        let case = moderation
            .open_case(
                &who,
                &NewCase {
                    category: ReportCategory::IllegalContent,
                    target_kind: TargetKind::Message,
                    target_id: Some(msg),
                    subject_account_id: Some(1),
                    summary: String::new(),
                },
            )
            .await
            .expect("case");
        assert_eq!(case.severity, CaseSeverity::Serious);
        let hold = moderation
            .place_hold(&who, case.id, HoldScope::Message(msg), Some("evidence"))
            .await
            .expect("hold")
            .hold_id
            .expect("a hold");
        sqlx::query(
            "UPDATE messages SET visibility = 'user_deleted', deleted_at = now() - interval '90 days', \
             purge_after = CASE WHEN hold_id IS NULL THEN now() END WHERE id = $1",
        )
        .bind(msg)
        .execute(pool)
        .await
        .expect("delete");

        let storage = sr_storage::ObjectStoreStorage::in_memory();
        let later = now_unix_ms() + 1000;
        assert_eq!(
            purge_deleted_messages(pool, &storage, later)
                .await
                .expect("purge"),
            0,
            "held: never purged"
        );
        let released = moderation
            .release_hold(&who, hold, Some("case closed"))
            .await
            .expect("release");
        assert_eq!(released.purgeable_messages, 1);
        assert_eq!(
            purge_deleted_messages(pool, &storage, later)
                .await
                .expect("purge"),
            1,
            "released: purged at the next run"
        );
        db.cleanup().await.expect("cleanup");
    }

    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn dismissing_a_case_releases_the_report_hold_so_the_purge_takes_the_message() {
        use sr_moderation::{
            Transition,
            model::{CaseState, NewReport, ReportCategory},
        };
        let db = TestDb::new().await.expect("test db");
        let pool = db.pool();
        for (id, name) in [(1_i64, "author"), (2, "staff"), (3, "reporter")] {
            sqlx::query("INSERT INTO persons (id) VALUES ($1)")
                .bind(id)
                .execute(pool)
                .await
                .expect("person");
            sqlx::query(
                "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $1, $2, $3, 'x')",
            )
            .bind(id)
            .bind(name)
            .bind(format!("{name}@example.test"))
            .execute(pool)
            .await
            .expect("account");
        }
        sqlx::query("INSERT INTO channels (id, type) VALUES (10, 1)")
            .execute(pool)
            .await
            .expect("channel");
        for u in [1_i64, 3] {
            sqlx::query("INSERT INTO channel_recipients (channel_id, user_id) VALUES (10, $1)")
                .bind(u)
                .execute(pool)
                .await
                .expect("recipient");
        }
        let _: String = sqlx::query_scalar("SELECT sr_ensure_message_partition(CURRENT_DATE)")
            .fetch_one(pool)
            .await
            .expect("partition");
        let msg: i64 =
            sqlx::query_scalar("SELECT sr_snowflake_floor(date_trunc('month', now())) + 1000")
                .fetch_one(pool)
                .await
                .expect("floor");
        sqlx::query(
            "INSERT INTO messages (id, channel_id, author_id, content) VALUES ($1, 10, 1, 'x')",
        )
        .bind(msg)
        .execute(pool)
        .await
        .expect("message");
        let ids = std::sync::atomic::AtomicI64::new(8_000_000);
        let moderation = Moderation::new(
            pool.clone(),
            std::sync::Arc::new(move || ids.fetch_add(1, std::sync::atomic::Ordering::Relaxed)),
        );
        moderation
            .file_report(
                3,
                &NewReport {
                    target_kind: TargetKind::Message,
                    target_id: msg,
                    channel_id: Some(10),
                    guild_id: None,
                    category: ReportCategory::Spam,
                    note: String::new(),
                    moderators_may_see_name: false,
                },
            )
            .await
            .expect("report");
        // Deleted by its author while the report hold stands.
        sqlx::query(
            "UPDATE messages SET visibility = 'user_deleted', deleted_at = now() - interval '90 days', purge_after = NULL WHERE id = $1",
        )
        .bind(msg)
        .execute(pool)
        .await
        .expect("delete");
        let storage = sr_storage::ObjectStoreStorage::in_memory();
        let later = now_unix_ms() + 1000;
        assert_eq!(
            purge_deleted_messages(pool, &storage, later)
                .await
                .expect("purge"),
            0
        );

        let staff = sr_admin::Caller {
            user_id: 2,
            permissions: sr_core::PlatformPermissions::MANAGE_CASES,
            is_owner: false,
            ip: None,
        };
        let who = Who::Staff(&staff);
        let case: i64 = sqlx::query_scalar("SELECT id FROM moderation_cases")
            .fetch_one(pool)
            .await
            .expect("case");
        moderation
            .set_state(
                &who,
                case,
                &Transition {
                    to: CaseState::Dismissed,
                    outcome: Some("fine".into()),
                    reporter_line: None,
                },
            )
            .await
            .expect("dismiss");
        assert_eq!(
            purge_deleted_messages(pool, &storage, later)
                .await
                .expect("purge"),
            1,
            "dismissed: the report hold is gone, so the deleted message is purged"
        );
        db.cleanup().await.expect("cleanup");
    }
}

/// LNG-MSG-023: clock-override tests for the 30-day purge and the 7-day attachment
/// expiry, including holds. The jobs take `now_ms` as a parameter, so the tests move
/// the clock instead of the data.
#[cfg(test)]
mod retention_clock_tests {
    use super::*;
    use bytes::Bytes;
    use sr_db::testing::TestDb;
    use sr_storage::ObjectStoreStorage;

    const DAY_MS: i64 = 24 * 60 * 60 * 1000;
    /// 2031-03-01T00:00:00Z, far from the real clock.
    const T0_MS: i64 = 1_930_435_200_000;

    async fn exec(pool: &PgPool, sql: &str, binds: &[i64]) {
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()));
        for b in binds {
            q = q.bind(*b);
        }
        q.execute(pool).await.expect(sql);
    }

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

    async fn count(pool: &PgPool, sql: &str, bind: i64) -> i64 {
        sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
            .bind(bind)
            .fetch_one(pool)
            .await
            .expect(sql)
    }

    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn purge_runs_exactly_thirty_days_after_deletion_and_removes_objects() {
        let db = TestDb::new().await.expect("test db");
        let pool = db.pool();
        let base = seed(pool).await;
        let storage = ObjectStoreStorage::in_memory();
        let (plain, held) = (base, base + 1);
        for id in [plain, held] {
            // Deleted at T0, purge due at T0 + 30 days.
            exec(
                pool,
                "INSERT INTO messages (id, channel_id, author_id, visibility, deleted_at, purge_after) \
                 VALUES ($1, 10, 1, 'user_deleted', \
                         to_timestamp(1930435200), to_timestamp(1930435200 + 30 * 86400))",
                &[id],
            )
            .await;
            exec(
                pool,
                "INSERT INTO message_edits (message_id, previous_content) VALUES ($1, 'old')",
                &[id],
            )
            .await;
            exec(
                pool,
                &format!(
                    "INSERT INTO attachments (id, message_id, uploader_id, filename, size_bytes, \
                     storage_key, state) VALUES ({}, $1, 1, 'a.png', 1, 'att/{id}', 'ready')",
                    id + 500
                ),
                &[id],
            )
            .await;
            storage
                .put(&format!("att/{id}"), Bytes::from_static(b"x"))
                .await
                .expect("put");
        }
        exec(
            pool,
            "UPDATE messages SET hold_id = 7 WHERE id = $1",
            &[held],
        )
        .await;

        let due = T0_MS + 30 * DAY_MS;
        // One millisecond early: nothing happens.
        assert_eq!(
            purge_deleted_messages(pool, &storage, due - 1)
                .await
                .expect("purge"),
            0
        );
        assert_eq!(
            count(pool, "SELECT count(*) FROM messages WHERE id >= $1", base).await,
            2
        );
        assert!(storage.get(&format!("att/{plain}")).await.is_ok());

        // On the dot: the unheld message, its edits, attachment row and object go.
        assert_eq!(
            purge_deleted_messages(pool, &storage, due)
                .await
                .expect("purge"),
            1
        );
        assert_eq!(
            count(pool, "SELECT count(*) FROM messages WHERE id = $1", plain).await,
            0
        );
        assert_eq!(
            count(
                pool,
                "SELECT count(*) FROM message_edits WHERE message_id = $1",
                plain
            )
            .await,
            0
        );
        assert_eq!(
            count(
                pool,
                "SELECT count(*) FROM attachments WHERE message_id = $1",
                plain
            )
            .await,
            0
        );
        assert!(storage.get(&format!("att/{plain}")).await.is_err());

        // The held message keeps everything, even long after the due date.
        let later = due + 400 * DAY_MS;
        assert_eq!(
            purge_deleted_messages(pool, &storage, later)
                .await
                .expect("purge"),
            0
        );
        assert_eq!(
            count(pool, "SELECT count(*) FROM messages WHERE id = $1", held).await,
            1
        );
        assert_eq!(
            count(
                pool,
                "SELECT count(*) FROM message_edits WHERE message_id = $1",
                held
            )
            .await,
            1
        );
        assert!(storage.get(&format!("att/{held}")).await.is_ok());

        // Releasing the hold makes it purgeable at the next run.
        exec(
            pool,
            "UPDATE messages SET hold_id = NULL WHERE id = $1",
            &[held],
        )
        .await;
        assert_eq!(
            purge_deleted_messages(pool, &storage, later)
                .await
                .expect("purge"),
            1
        );
        assert!(storage.get(&format!("att/{held}")).await.is_err());
        db.cleanup().await.expect("cleanup");
    }

    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn large_attachments_expire_after_seven_days_unless_held() {
        let db = TestDb::new().await.expect("test db");
        let pool = db.pool();
        let base = seed(pool).await;
        let storage = ObjectStoreStorage::in_memory();
        let (plain, held_att, held_msg, small) = (base, base + 1, base + 2, base + 3);
        for id in [plain, held_att, held_msg, small] {
            exec(
                pool,
                "INSERT INTO messages (id, channel_id, author_id, visibility) \
                 VALUES ($1, 10, 1, 'visible')",
                &[id],
            )
            .await;
            let large = id != small;
            let expires = if large {
                // Uploaded at T0, expires at T0 + 7 days.
                "to_timestamp(1930435200 + 7 * 86400)"
            } else {
                "NULL"
            };
            exec(
                pool,
                &format!(
                    "INSERT INTO attachments (id, message_id, uploader_id, filename, size_bytes, \
                     storage_key, state, is_large, expires_at) \
                     VALUES ({}, $1, 1, 'big.bin', 1, 'att/{id}', 'ready', {large}, {expires})",
                    id + 500
                ),
                &[id],
            )
            .await;
            storage
                .put(&format!("att/{id}"), Bytes::from_static(b"x"))
                .await
                .expect("put");
        }
        exec(
            pool,
            "UPDATE attachments SET hold_id = 7 WHERE id = $1",
            &[held_att + 500],
        )
        .await;
        exec(
            pool,
            "UPDATE messages SET hold_id = 7 WHERE id = $1",
            &[held_msg],
        )
        .await;

        let due = T0_MS + 7 * DAY_MS;
        assert!(
            expire_large_attachments(pool, &storage, due - 1)
                .await
                .expect("expire")
                .is_empty()
        );
        let hit = expire_large_attachments(pool, &storage, due)
            .await
            .expect("expire");
        assert_eq!(
            hit,
            vec![ExpiredAttachment {
                channel_id: 10,
                message_id: plain
            }]
        );
        let state = |id: i64| async move {
            sqlx::query_scalar::<_, String>("SELECT state::text FROM attachments WHERE id = $1")
                .bind(id + 500)
                .fetch_one(pool)
                .await
                .expect("state")
        };
        assert_eq!(state(plain).await, "expired");
        assert!(storage.get(&format!("att/{plain}")).await.is_err());
        // Held attachment, held message and a small (non-expiring) attachment stay.
        for id in [held_att, held_msg, small] {
            assert_eq!(state(id).await, "ready");
            assert!(storage.get(&format!("att/{id}")).await.is_ok());
        }
        // Running again later does nothing more for them while the holds stand.
        assert!(
            expire_large_attachments(pool, &storage, due + 100 * DAY_MS)
                .await
                .expect("expire")
                .is_empty()
        );
        db.cleanup().await.expect("cleanup");
    }
}
