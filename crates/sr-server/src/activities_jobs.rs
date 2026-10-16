//! The hourly `activities.gc` sweep (J-12, D-247): deletes the stored files of Activity bundles that are
//! superseded (replaced by a newer upload or submission, or withdrawn by a change of the URL mappings).
//! Recurring singleton with a reserved id, re-armed on every boot.

use std::time::Duration;

use serde_json::json;
use sr_activities::ActivityService;
use sr_db::jobs::{self, Job};
use sr_db::runner::{HandlerError, JobRunner};
use sr_db::{DbResult, PgPool};

/// Reserved singleton id for `activities.gc`.
pub const GC_JOB_ID: i64 = 965;
/// Kind of the hourly sweep.
pub const GC_KIND: &str = "activities.gc";
const INTERVAL: Duration = Duration::from_secs(3600);

/// Register the sweep on `runner`. Pair with [`schedule`].
pub fn register(runner: JobRunner, pool: &PgPool, service: ActivityService) -> JobRunner {
    let pool = pool.clone();
    runner.register(GC_KIND, 1, move |job: Job| {
        let pool = pool.clone();
        let service = service.clone();
        Box::pin(async move {
            match service.gc().await {
                Ok(0) => {}
                Ok(n) => tracing::info!(removed = n, "superseded activity bundle files deleted"),
                // A storage hiccup is retried at the next tick, not dead-lettered.
                Err(e) => tracing::warn!(error = %e, "activity bundle sweep failed"),
            }
            jobs::reschedule_running(&pool, job.id, INTERVAL)
                .await
                .map_err(HandlerError::from)?;
            Ok(())
        })
    })
}

/// Create or re-arm the sweep singleton. Call only when [`register`] is on the runner.
pub async fn schedule(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(pool, GC_JOB_ID, GC_KIND, &json!({}), INTERVAL).await?;
    Ok(())
}
