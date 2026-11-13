//! The `bugs.retention` job (SEC-AUD-167, D-650): the daily retention of beta bug reports.
//!
//! One leased singleton (id 2550) that runs once a day. A pass deletes the picture of every report older than
//! [`SCREENSHOT_DAYS`] days and then every report older than [`REPORT_DAYS`] days. A closed report already lost its
//! picture when staff closed it. A failed pass is logged and retried the next day: it never stops the singleton.

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;
use sqlx::PgPool;
use sr_db::DbResult;
use sr_db::bug_reports::{PgBugReports, REPORT_DAYS, SCREENSHOT_DAYS};
use sr_db::jobs::{self, Job};
use sr_db::runner::{HandlerError, JobRunner};

/// Kind of the retention job.
pub const BUGS_KIND: &str = "bugs.retention";
/// Reserved singleton id (below any Snowflake; unique among the `*_JOB_ID` constants).
pub const BUGS_JOB_ID: i64 = 2550;
const HOUR_UTC: u32 = 3;
const MINUTE_UTC: u32 = 40;

fn delay() -> std::time::Duration {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0);
    jobs::delay_until_daily_utc(now, HOUR_UTC, MINUTE_UTC)
}

async fn handler(pool: PgPool, job: Job) -> Result<(), HandlerError> {
    match PgBugReports::new(pool.clone())
        .purge(SCREENSHOT_DAYS, REPORT_DAYS)
        .await
    {
        Ok(p) => tracing::info!(
            pictures = p.pictures,
            reports = p.reports,
            "bug report retention"
        ),
        Err(e) => tracing::warn!(error = %e, "bug report retention failed"),
    }
    jobs::reschedule_running(&pool, job.id, delay()).await?;
    Ok(())
}

/// Register `bugs.retention`. Pair with [`schedule`].
pub fn register(runner: JobRunner, pool: &PgPool) -> JobRunner {
    let pool = pool.clone();
    runner.register(BUGS_KIND, 1, move |job| {
        Box::pin(handler(pool.clone(), job))
    })
}

/// Create or re-arm the singleton. Call only when [`register`] is on the runner.
pub async fn schedule(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(pool, BUGS_JOB_ID, BUGS_KIND, &json!({}), delay()).await?;
    Ok(())
}
