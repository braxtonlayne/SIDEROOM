//! Housekeeping of the review pipeline (REV-CORE-009), registered on the shared job runner.
//!
//! * `review.housekeeping` (hourly): warns members whose draft has been idle for 23 days,
//!   abandons drafts idle for 30 days (after a warning), and purges the files of submissions
//!   rejected more than 14 days ago (`ReviewService::housekeeping`). Every step is idempotent.

use std::time::Duration;

use serde_json::json;
use sqlx::PgPool;
use sr_db::DbResult;
use sr_db::jobs::{self, Job};
use sr_db::runner::{HandlerError, JobRunner};
use sr_review::ReviewService;

/// Kind of the housekeeping job.
pub const HOUSEKEEPING_KIND: &str = "review.housekeeping";
/// Reserved singleton id (below any Snowflake).
pub const HOUSEKEEPING_JOB_ID: i64 = 701;
pub const HOUSEKEEPING_INTERVAL: Duration = Duration::from_secs(3600);

async fn housekeeping(pool: PgPool, review: ReviewService, job: Job) -> Result<(), HandlerError> {
    let report = review
        .housekeeping()
        .await
        .map_err(|e| -> HandlerError { Box::new(std::io::Error::other(e.to_string())) })?;
    if report.warned + report.abandoned + report.purged + report.failed > 0 {
        tracing::info!(
            warned = report.warned,
            abandoned = report.abandoned,
            purged = report.purged,
            failed = report.failed,
            "review housekeeping"
        );
    }
    jobs::reschedule_running(&pool, job.id, HOUSEKEEPING_INTERVAL).await?;
    Ok(())
}

/// Register `review.housekeeping`. Pair with [`schedule`].
pub fn register(runner: JobRunner, pool: &PgPool, review: ReviewService) -> JobRunner {
    let pool = pool.clone();
    runner.register(HOUSEKEEPING_KIND, 1, move |job| {
        Box::pin(housekeeping(pool.clone(), review.clone(), job))
    })
}

/// Create or re-arm the singleton. Call only when [`register`] is on the runner.
pub async fn schedule(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(
        pool,
        HOUSEKEEPING_JOB_ID,
        HOUSEKEEPING_KIND,
        &json!({}),
        HOUSEKEEPING_INTERVAL,
    )
    .await?;
    Ok(())
}

/// The daily submission upload allowance from the platform setting
/// `submissions.daily_upload_bytes` (D-196; Admin -> Limits). Falls back to the default when the
/// setting is unset or unreadable; `0` means unlimited.
pub struct PlatformAllowance {
    pool: sqlx::PgPool,
}

impl PlatformAllowance {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl sr_review::service::AllowanceSource for PlatformAllowance {
    async fn daily_upload_bytes(&self) -> i64 {
        let stored: Result<Option<serde_json::Value>, _> =
            sqlx::query_scalar("SELECT value FROM platform_settings WHERE key = $1")
                .bind("submissions.daily_upload_bytes")
                .fetch_optional(&self.pool)
                .await;
        match stored {
            Ok(Some(v)) => v
                .as_i64()
                .unwrap_or(sr_review::service::MAX_UPLOAD_BYTES_PER_DAY),
            Ok(None) => sr_review::service::MAX_UPLOAD_BYTES_PER_DAY,
            Err(e) => {
                tracing::warn!(error = %e, "reading submissions.daily_upload_bytes failed");
                sr_review::service::MAX_UPLOAD_BYTES_PER_DAY
            }
        }
    }
}
