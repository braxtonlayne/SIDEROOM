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
/// Messages under a moderation hold are never purged, nor is a message with a held
/// attachment (deleting it would cascade to the held row; D-122, XC-MOD-001). `message_edits` has no
/// foreign key to the partitioned `messages` table, so it is deleted explicitly
/// by the same statement (open question 94). The data-modifying CTEs run in a
/// single statement, so the two deletes commit together.
pub(crate) const PURGE_SQL: &str = "\
WITH doomed AS ( \
    DELETE FROM messages WHERE id IN ( \
        SELECT id FROM messages \
        WHERE purge_after IS NOT NULL AND purge_after <= now() AND hold_id IS NULL \
          AND NOT EXISTS (SELECT 1 FROM attachments a \
                          WHERE a.message_id = messages.id AND a.hold_id IS NOT NULL) \
        ORDER BY purge_after \
        LIMIT $1) \
    RETURNING id), \
edits AS ( \
    DELETE FROM message_edits WHERE message_id IN (SELECT id FROM doomed) \
    RETURNING id) \
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

        let purged = purge_deleted_messages(pool).await.expect("purge");
        assert_eq!(purged, 1, "only the plain, due, unheld message goes");
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
        assert_eq!(purge_deleted_messages(pool).await.expect("purge"), 2);
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
