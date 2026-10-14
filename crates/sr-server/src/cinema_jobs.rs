//! Periodic jobs of the Cinema social backend, registered on the shared job runner.
//!
//! * `cinema.presence_sweep` (every minute): removes lapsed "watching" rows and tells their audiences
//!   (`CinemaService::sweep_presence`). Reads already ignore expired rows, so this only keeps the table small
//!   and delivers the "stopped" event for a player that vanished without a goodbye.

use std::time::Duration;

use serde_json::json;
use sqlx::PgPool;
use sr_cinema_social::CinemaService;
use sr_db::DbResult;
use sr_db::jobs::{self, Job};
use sr_db::runner::{HandlerError, JobRunner};

/// Kind of the presence sweep.
pub const PRESENCE_SWEEP_KIND: &str = "cinema.presence_sweep";
/// Reserved singleton id (below any Snowflake).
pub const PRESENCE_SWEEP_JOB_ID: i64 = 160;
/// Presence rows live 45 s; sweeping each minute keeps the "stopped" event close to the lapse.
pub const PRESENCE_SWEEP_INTERVAL: Duration = Duration::from_secs(60);

async fn presence_sweep(pool: PgPool, social: CinemaService, job: Job) -> Result<(), HandlerError> {
    let swept = social
        .sweep_presence()
        .await
        .map_err(|e| -> HandlerError { Box::new(std::io::Error::other(e.to_string())) })?;
    if swept > 0 {
        tracing::debug!(swept, "cinema presence swept");
    }
    jobs::reschedule_running(&pool, job.id, PRESENCE_SWEEP_INTERVAL).await?;
    Ok(())
}

/// Register `cinema.presence_sweep`. Pair with [`schedule`].
pub fn register(runner: JobRunner, pool: &PgPool, social: CinemaService) -> JobRunner {
    let pool = pool.clone();
    runner.register(PRESENCE_SWEEP_KIND, 1, move |job| {
        Box::pin(presence_sweep(pool.clone(), social.clone(), job))
    })
}

/// Create or re-arm the sweep singleton. Call only when [`register`] is on the runner.
pub async fn schedule(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(
        pool,
        PRESENCE_SWEEP_JOB_ID,
        PRESENCE_SWEEP_KIND,
        &json!({}),
        PRESENCE_SWEEP_INTERVAL,
    )
    .await?;
    Ok(())
}
