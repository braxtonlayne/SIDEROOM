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

/// Return jobs stuck in `running` longer than `older_than` (crashed workers) to the queue.
pub async fn reap_stale<'e>(ex: impl PgExecutor<'e>, older_than: Duration) -> DbResult<u64> {
    let r = sqlx::query(
        "UPDATE jobs SET state = 'queued', locked_at = NULL, locked_by = NULL, updated_at = now() \
         WHERE state = 'running' AND locked_at < now() - make_interval(secs => $1)",
    )
    .bind(older_than.as_secs_f64())
    .execute(ex)
    .await?;
    Ok(r.rows_affected())
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
}
