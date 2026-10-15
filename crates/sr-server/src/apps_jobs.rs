//! Housekeeping of the developer platform: `apps.housekeeping` (hourly) forgets tester declines
//! older than the 24 hour re-invite cooldown (G-84). Recurring singleton with a reserved id.

use std::time::Duration;

use serde_json::json;
use sr_apps::AppsService;
use sr_db::jobs::{self, Job};
use sr_db::runner::{HandlerError, JobRunner};
use sr_db::{DbResult, PgPool};

/// Reserved singleton id for `apps.housekeeping`.
pub const HOUSEKEEPING_JOB_ID: i64 = 810;
/// Kind of the hourly sweep.
pub const HOUSEKEEPING_KIND: &str = "apps.housekeeping";
const INTERVAL: Duration = Duration::from_secs(3600);

/// Register the sweep on `runner`. Pair with [`schedule`].
pub fn register(runner: JobRunner, pool: &PgPool, apps: AppsService) -> JobRunner {
    let pool = pool.clone();
    runner.register(HOUSEKEEPING_KIND, 1, move |job: Job| {
        let pool = pool.clone();
        let apps = apps.clone();
        Box::pin(async move {
            match apps.purge_tester_declines().await {
                Ok(0) => {}
                Ok(n) => tracing::info!(removed = n, "old tester declines forgotten"),
                // Retried at the next tick, not dead-lettered.
                Err(e) => tracing::warn!(error = %e, "apps housekeeping failed"),
            }
            jobs::reschedule_running(&pool, job.id, INTERVAL)
                .await
                .map_err(HandlerError::from)?;
            Ok(())
        })
    })
}

/// Create or re-arm the singleton. Call only when [`register`] is on the runner.
pub async fn schedule(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(
        pool,
        HOUSEKEEPING_JOB_ID,
        HOUSEKEEPING_KIND,
        &json!({}),
        INTERVAL,
    )
    .await?;
    Ok(())
}
