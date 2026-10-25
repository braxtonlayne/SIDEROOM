//! Durable job queue (docs/02 section 6). Claiming uses `FOR UPDATE SKIP LOCKED`.
//! Job ids are Snowflakes supplied by the caller.

use std::time::Duration;

use sqlx::{FromRow, PgExecutor};

use crate::DbResult;

const BACKOFF_BASE_SECS: u64 = 5;
const BACKOFF_CAP_SECS: u64 = 3600;

/// Delay before retrying after `attempts` failed attempts (exponential, capped).
/// Mirrors the SQL expression in [`fail`].
pub fn backoff(attempts: i32) -> Duration {
    let exp = u32::try_from(attempts.saturating_sub(1).clamp(0, 20)).unwrap_or(20);
    Duration::from_secs(
        BACKOFF_BASE_SECS
            .saturating_mul(1u64 << exp)
            .min(BACKOFF_CAP_SECS),
    )
}

#[derive(Debug, Clone, FromRow)]
pub struct Job {
    pub id: i64,
    pub kind: String,
    pub payload: serde_json::Value,
    pub state: String,
    pub priority: i16,
    pub attempts: i32,
    pub max_attempts: i32,
    pub last_error: Option<String>,
}

macro_rules! cols {
    () => {
        "id, kind, payload, state::text AS state, priority, attempts, max_attempts, last_error"
    };
}

#[derive(Debug, Clone)]
pub struct NewJob {
    pub id: i64,
    pub kind: String,
    pub payload: serde_json::Value,
    pub priority: i16,
    pub max_attempts: i32,
    pub delay: Duration,
}

impl NewJob {
    pub fn new(id: i64, kind: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            id,
            kind: kind.into(),
            payload,
            priority: 0,
            max_attempts: 5,
            delay: Duration::ZERO,
        }
    }
}

/// Enqueue a job. Pass a transaction to enqueue atomically with other writes.
pub async fn enqueue<'e>(ex: impl PgExecutor<'e>, job: &NewJob) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO jobs (id, kind, payload, priority, max_attempts, run_at) \
         VALUES ($1, $2, $3, $4, $5, now() + make_interval(secs => $6))",
    )
    .bind(job.id)
    .bind(&job.kind)
    .bind(&job.payload)
    .bind(job.priority)
    .bind(job.max_attempts)
    .bind(job.delay.as_secs_f64())
    .execute(ex)
    .await?;
    Ok(())
}

/// Atomically claim up to `limit` due jobs of the given kinds for `worker`.
pub async fn claim<'e>(
    ex: impl PgExecutor<'e>,
    kinds: &[String],
    worker: &str,
    limit: i64,
) -> DbResult<Vec<Job>> {
    let sql = concat!(
        "UPDATE jobs SET state = 'running', attempts = attempts + 1, locked_at = now(), \
                locked_by = $2, updated_at = now() \
         WHERE id IN ( \
             SELECT id FROM jobs \
             WHERE state = 'queued' AND run_at <= now() AND kind = ANY($1) \
             ORDER BY priority DESC, run_at, id \
             LIMIT $3 FOR UPDATE SKIP LOCKED) \
         RETURNING ",
        cols!()
    );
    Ok(sqlx::query_as::<_, Job>(sql)
        .bind(kinds)
        .bind(worker)
        .bind(limit)
        .fetch_all(ex)
        .await?)
}

/// Mark a running job succeeded.
pub async fn complete<'e>(ex: impl PgExecutor<'e>, id: i64) -> DbResult<bool> {
    let r = sqlx::query(
        "UPDATE jobs SET state = 'succeeded', finished_at = now(), locked_at = NULL, \
                locked_by = NULL, updated_at = now() \
         WHERE id = $1 AND state = 'running'",
    )
    .bind(id)
    .execute(ex)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// [`complete`] for several running jobs in one statement (a handler that batched them). Returns how many were
/// marked.
pub async fn complete_many<'e>(ex: impl PgExecutor<'e>, ids: &[i64]) -> DbResult<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    let r = sqlx::query(
        "UPDATE jobs SET state = 'succeeded', finished_at = now(), locked_at = NULL, \
                locked_by = NULL, updated_at = now() \
         WHERE id = ANY($1) AND state = 'running'",
    )
    .bind(ids)
    .execute(ex)
    .await?;
    Ok(r.rows_affected())
}

/// Record a failure. Re-queues with exponential backoff, or dead-letters once
/// `max_attempts` is reached. Returns the resulting state (`queued`/`dead`),
/// or `None` if the job was not running.
pub async fn fail<'e>(ex: impl PgExecutor<'e>, id: i64, error: &str) -> DbResult<Option<String>> {
    Ok(sqlx::query_scalar(
        "UPDATE jobs SET \
             state = CASE WHEN attempts >= max_attempts THEN 'dead'::job_state \
                          ELSE 'queued'::job_state END, \
             run_at = CASE WHEN attempts >= max_attempts THEN run_at \
                      ELSE now() + make_interval(secs => \
                          LEAST($3::float8 * power(2, LEAST(attempts - 1, 20)), $4::float8)) END, \
             finished_at = CASE WHEN attempts >= max_attempts THEN now() ELSE NULL END, \
             last_error = $2, locked_at = NULL, locked_by = NULL, updated_at = now() \
         WHERE id = $1 AND state = 'running' \
         RETURNING state::text",
    )
    .bind(id)
    .bind(error)
    .bind(BACKOFF_BASE_SECS as f64)
    .bind(BACKOFF_CAP_SECS as f64)
    .fetch_optional(ex)
    .await?)
}

/// Manually retry a dead-lettered job (resets attempts).
pub async fn retry<'e>(ex: impl PgExecutor<'e>, id: i64) -> DbResult<bool> {
    let r = sqlx::query(
        "UPDATE jobs SET state = 'queued', attempts = 0, run_at = now(), finished_at = NULL, \
                updated_at = now() \
         WHERE id = $1 AND state = 'dead'",
    )
    .bind(id)
    .execute(ex)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// Create or re-arm a recurring singleton job with a fixed `id`.
///
/// Singletons use reserved ids below 1024 (real Snowflakes are far larger), so
/// calling this on every boot is idempotent: a row that is already `queued` or
/// `running` is left alone; a `succeeded` or `dead` row is re-armed to run after
/// `delay`. Returns `true` if a row was inserted or re-armed.
pub async fn schedule_singleton<'e>(
    ex: impl PgExecutor<'e>,
    id: i64,
    kind: &str,
    payload: &serde_json::Value,
    delay: Duration,
) -> DbResult<bool> {
    let r = sqlx::query(
        "INSERT INTO jobs (id, kind, payload, max_attempts, run_at) \
         VALUES ($1, $2, $3, 5, now() + make_interval(secs => $4)) \
         ON CONFLICT (id) DO UPDATE SET \
             kind = EXCLUDED.kind, payload = EXCLUDED.payload, state = 'queued', \
             run_at = EXCLUDED.run_at, attempts = 0, last_error = NULL, finished_at = NULL, \
             locked_at = NULL, locked_by = NULL, updated_at = now() \
         WHERE jobs.state IN ('succeeded', 'dead')",
    )
    .bind(id)
    .bind(kind)
    .bind(payload)
    .bind(delay.as_secs_f64())
    .execute(ex)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// From inside its own handler, re-queue the running job `id` to run after
/// `delay` (used by recurring singletons). The runner's later `complete` then
/// finds the row no longer `running` and does nothing.
pub async fn reschedule_running<'e>(
    ex: impl PgExecutor<'e>,
    id: i64,
    delay: Duration,
) -> DbResult<bool> {
    let r = sqlx::query(
        "UPDATE jobs SET state = 'queued', run_at = now() + make_interval(secs => $2), \
                attempts = 0, last_error = NULL, finished_at = NULL, \
                locked_at = NULL, locked_by = NULL, updated_at = now() \
         WHERE id = $1 AND state = 'running'",
    )
    .bind(id)
    .bind(delay.as_secs_f64())
    .execute(ex)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// Time until the next occurrence of `hour:minute` UTC, strictly after `now_unix_ms`.
pub fn delay_until_daily_utc(now_unix_ms: i64, hour: u32, minute: u32) -> Duration {
    const DAY_MS: i64 = 86_400_000;
    let offset_ms = i64::from(hour) * 3_600_000 + i64::from(minute) * 60_000;
    let today_start = now_unix_ms.div_euclid(DAY_MS) * DAY_MS;
    let mut next = today_start + offset_ms;
    if next <= now_unix_ms {
        next += DAY_MS;
    }
    Duration::from_millis(u64::try_from(next - now_unix_ms).unwrap_or(0))
}

/// Return jobs stuck in `running` longer than `older_than` (crashed workers) to the queue.
/// Returns the `(id, kind)` of every job it returned, so the caller can log each one.
pub async fn reap_stale<'e>(
    ex: impl PgExecutor<'e>,
    older_than: Duration,
) -> DbResult<Vec<(i64, String)>> {
    Ok(sqlx::query_as::<_, (i64, String)>(
        "UPDATE jobs SET state = 'queued', locked_at = NULL, locked_by = NULL, updated_at = now() \
         WHERE state = 'running' AND locked_at < now() - make_interval(secs => $1) \
         RETURNING id, kind",
    )
    .bind(older_than.as_secs_f64())
    .fetch_all(ex)
    .await?)
}

/// List dead-lettered jobs, newest first.
pub async fn list_dead<'e>(ex: impl PgExecutor<'e>, limit: i64) -> DbResult<Vec<Job>> {
    let sql = concat!(
        "SELECT ",
        cols!(),
        " FROM jobs WHERE state = 'dead' ORDER BY finished_at DESC LIMIT $1"
    );
    Ok(sqlx::query_as::<_, Job>(sql)
        .bind(limit)
        .fetch_all(ex)
        .await?)
}

/// Kind of the scheduled-events clock (G-92).
pub const SCHEDULED_EVENTS_TICK_KIND: &str = "scheduled_events.tick";
/// Reserved singleton id of the scheduled-events clock.
pub const SCHEDULED_EVENTS_TICK_JOB_ID: i64 = 820;
/// How far ahead the clock re-enqueues itself.
pub const SCHEDULED_EVENTS_TICK_INTERVAL: Duration = Duration::from_secs(30);

/// Seed the `scheduled_events.tick` singleton if no node has yet. A row that is `queued` or
/// `running` is left alone, so any number of nodes may call this at startup: the claim
/// (`FOR UPDATE SKIP LOCKED`) then lets exactly one runner advance events per turn, and the
/// handler re-enqueues the job [`SCHEDULED_EVENTS_TICK_INTERVAL`] ahead. Returns `true` when
/// a row was inserted or re-armed.
pub async fn seed_scheduled_events_tick<'e>(ex: impl PgExecutor<'e>) -> DbResult<bool> {
    schedule_singleton(
        ex,
        SCHEDULED_EVENTS_TICK_JOB_ID,
        SCHEDULED_EVENTS_TICK_KIND,
        &serde_json::json!({}),
        Duration::ZERO,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(backoff(1), Duration::from_secs(5));
        assert_eq!(backoff(2), Duration::from_secs(10));
        assert_eq!(backoff(3), Duration::from_secs(20));
        assert_eq!(backoff(100), Duration::from_secs(3600));
        assert_eq!(backoff(0), Duration::from_secs(5));
    }

    #[test]
    fn daily_delay_is_next_utc_occurrence() {
        const DAY: i64 = 86_400_000;
        // 2026-03-02T00:00:00Z: 00:10 is ten minutes away.
        let midnight = 1_772_323_200_000_i64;
        assert_eq!(
            delay_until_daily_utc(midnight, 0, 10),
            Duration::from_secs(600)
        );
        // Exactly at 00:10 the next run is tomorrow, not zero.
        assert_eq!(
            delay_until_daily_utc(midnight + 600_000, 0, 10),
            Duration::from_secs(86_400)
        );
        // Just after 00:10 the next run is tomorrow.
        assert_eq!(
            delay_until_daily_utc(midnight + 600_001, 0, 10),
            Duration::from_millis(86_400_000 - 1)
        );
        // Just before midnight the next 00:10 is still ten minutes past midnight.
        assert_eq!(
            delay_until_daily_utc(midnight - 1, 0, 10),
            Duration::from_millis(600_001)
        );
        assert!(
            delay_until_daily_utc(midnight + 5 * DAY + 123, 23, 59) < Duration::from_secs(86_400)
        );
    }

    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn singleton_arms_once_and_rearms_after_success() {
        let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
        let pool = crate::connect(&crate::DbConfig::new(url))
            .await
            .expect("connect");
        crate::migrate(&pool).await.expect("migrate");
        // Reserved singleton id; the test owns it for its duration.
        let id = 900_001_i64;
        sqlx::query("DELETE FROM jobs WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .expect("clear");
        let payload = serde_json::json!({});
        let kind = "test.singleton";

        assert!(
            schedule_singleton(&pool, id, kind, &payload, Duration::ZERO)
                .await
                .expect("insert")
        );
        // Already queued: a second boot must not add or move it.
        assert!(
            !schedule_singleton(&pool, id, kind, &payload, Duration::from_secs(3600))
                .await
                .expect("noop")
        );

        sqlx::query("UPDATE jobs SET state = 'succeeded' WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .expect("mark succeeded");
        assert!(
            schedule_singleton(&pool, id, kind, &payload, Duration::from_secs(3600))
                .await
                .expect("rearm")
        );
        let (state, delay_ok): (String, bool) = sqlx::query_as(
            "SELECT state::text, run_at > now() + interval '59 minutes' FROM jobs WHERE id = $1",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("read");
        assert_eq!(state, "queued");
        assert!(delay_ok);

        // Not running, so self-reschedule is a no-op.
        assert!(
            !reschedule_running(&pool, id, Duration::ZERO)
                .await
                .expect("nop")
        );
    }
}
