//! The hourly sweep of the external image proxy cache (F-09, D-176): copies of
//! third-party images live under `external/` for 30 days, then this job deletes
//! them. Recurring singleton with a reserved id, re-armed on every boot.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::json;
use sr_db::jobs::{self, Job};
use sr_db::runner::{HandlerError, JobRunner};
use sr_db::{DbResult, PgPool};
use sr_storage::ObjectStorage;

/// Reserved singleton id for `external_images.gc`.
pub const EXTERNAL_GC_JOB_ID: i64 = 141;
/// Kind of the hourly cache sweep.
pub const EXTERNAL_GC_KIND: &str = "external_images.gc";
const INTERVAL: Duration = Duration::from_secs(3600);

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Register the sweep on `runner`. Pair with [`schedule`].
pub fn register(runner: JobRunner, pool: &PgPool, storage: Arc<dyn ObjectStorage>) -> JobRunner {
    let pool = pool.clone();
    runner.register(EXTERNAL_GC_KIND, 1, move |job: Job| {
        let pool = pool.clone();
        let storage = storage.clone();
        Box::pin(async move {
            match sr_api::routes::cdn::external::gc_expired(storage.as_ref(), now_ms()).await {
                Ok(0) => {}
                Ok(n) => tracing::info!(removed = n, "expired proxied images deleted"),
                // A storage hiccup is retried at the next tick, not dead-lettered.
                Err(e) => tracing::warn!(error = %e, "proxied image sweep failed"),
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
    jobs::schedule_singleton(
        pool,
        EXTERNAL_GC_JOB_ID,
        EXTERNAL_GC_KIND,
        &json!({}),
        INTERVAL,
    )
    .await?;
    Ok(())
}
