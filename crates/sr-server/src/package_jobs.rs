//! SEC-AUD-176 (D-654): the jobs behind the Activity package upload sessions.
//!
//! * `activities.package_ingest`: checks the stored zip of one upload (payload `{"upload_id"}`), one at a time
//!   per node. A package that breaks a rule ends the session `refused` with the checker's text; a storage or
//!   internal failure is retried by the job system and ends `refused` with "The package could not be checked.
//!   Try again." after the last attempt.
//! * `activities.package_sweep`: a daily singleton that expires uploads nobody finished (24 hours) and aborts
//!   their parts, and deletes session rows after seven days.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde_json::json;
use sr_activities::package_upload::{PackageQueue, PackageUploads};
use sr_activities::service::IdSource;
use sr_db::jobs::{self, Job, NewJob};
use sr_db::runner::{HandlerError, JobRunner};
use sr_db::{DbResult, PgPool};

/// Kind of the check of one stored package.
pub const INGEST_KIND: &str = "activities.package_ingest";
/// Kind of the daily sweep.
pub const SWEEP_KIND: &str = "activities.package_sweep";
/// Reserved singleton id for [`SWEEP_KIND`].
pub const SWEEP_JOB_ID: i64 = 966;
const SWEEP_HOUR_UTC: u32 = 4;
const SWEEP_MINUTE_UTC: u32 = 40;
/// Attempts of one check before it ends `refused` (the first plus two retries with the job system's backoff).
const INGEST_ATTEMPTS: i32 = 3;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Queues the check job of an upload.
pub struct PgPackageQueue {
    pool: PgPool,
    ids: IdSource,
}

impl PgPackageQueue {
    pub fn new(pool: PgPool, ids: IdSource) -> Self {
        Self { pool, ids }
    }
}

#[async_trait]
impl PackageQueue for PgPackageQueue {
    async fn enqueue(&self, upload_id: i64) -> Result<(), String> {
        let mut job = NewJob::new((self.ids)(), INGEST_KIND, json!({ "upload_id": upload_id }));
        job.max_attempts = INGEST_ATTEMPTS;
        jobs::enqueue(&self.pool, &job)
            .await
            .map_err(|e| e.to_string())
    }
}

/// Register both handlers on `runner`. Pair with [`schedule`].
pub fn register(runner: JobRunner, pool: &PgPool, uploads: PackageUploads) -> JobRunner {
    let sweep_uploads = uploads.clone();
    let sweep_pool = pool.clone();
    runner
        // One package check at a time per node: a check streams a whole zip through the node.
        .register(INGEST_KIND, 1, move |job: Job| {
            let uploads = uploads.clone();
            Box::pin(async move {
                let Some(id) = job.payload.get("upload_id").and_then(serde_json::Value::as_i64)
                else {
                    // Nothing to check; do not retry a job that can never work.
                    tracing::error!(job = job.id, "a package check job has no upload id");
                    return Ok(());
                };
                let last = job.attempts >= job.max_attempts;
                uploads
                    .run_ingest(id, last)
                    .await
                    .map_err(HandlerError::from)
            })
        })
        .register(SWEEP_KIND, 1, move |job: Job| {
            let (pool, uploads) = (sweep_pool.clone(), sweep_uploads.clone());
            Box::pin(async move {
                match uploads.sweep().await {
                    Ok((0, 0)) => {}
                    Ok((expired, deleted)) => {
                        tracing::info!(expired, deleted, "package uploads swept");
                    }
                    // A failed sweep is tried again tomorrow.
                    Err(e) => tracing::warn!(error = %e, "package upload sweep failed"),
                }
                jobs::reschedule_running(
                    &pool,
                    job.id,
                    jobs::delay_until_daily_utc(now_ms(), SWEEP_HOUR_UTC, SWEEP_MINUTE_UTC),
                )
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
        SWEEP_JOB_ID,
        SWEEP_KIND,
        &json!({}),
        jobs::delay_until_daily_utc(now_ms(), SWEEP_HOUR_UTC, SWEEP_MINUTE_UTC),
    )
    .await?;
    Ok(())
}

/// The queue as a trait object.
pub fn queue(pool: PgPool, ids: IdSource) -> Arc<dyn PackageQueue> {
    Arc::new(PgPackageQueue::new(pool, ids))
}
