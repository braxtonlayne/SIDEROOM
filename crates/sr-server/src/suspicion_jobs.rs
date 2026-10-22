//! Housekeeping of the suspicion tooling (MOD-SUS-003; doc 09).
//!
//! `moderation.suspicion_housekeeping` runs once a day. It deletes sign-in history older than the 180-day
//! window and scans the Persons who signed in recently for evasion signals. The scan only raises review items
//! for staff; it never bans, freezes or hides anything.

use std::time::Duration;

use serde_json::json;
use sqlx::PgPool;
use sr_db::DbResult;
use sr_db::jobs::{self, Job};
use sr_db::runner::{HandlerError, JobRunner};
use sr_moderation::{Moderation, SCAN_BATCH};

/// Kind of the housekeeping job.
pub const KIND: &str = "moderation.suspicion_housekeeping";
/// Reserved singleton id (below any Snowflake; unique in this repository, see the grep in the handoff).
pub const JOB_ID: i64 = 1150;
const _: () = assert!(JOB_ID < 1 << 40, "singleton ids stay below the snowflakes");
pub const INTERVAL: Duration = Duration::from_secs(24 * 3600);
/// People who signed in during the last this-many hours are scanned (a little over the interval).
const SCAN_HOURS: i32 = 30;

async fn handler(pool: PgPool, moderation: Moderation, job: Job) -> Result<(), HandlerError> {
    let io = |e: sr_moderation::ModError| -> HandlerError {
        Box::new(std::io::Error::other(e.to_string()))
    };
    let purged = moderation.purge_login_events().await.map_err(io)?;
    let raised = moderation
        .scan_recent_logins(SCAN_HOURS, SCAN_BATCH)
        .await
        .map_err(io)?;
    tracing::info!(purged, raised, "suspicion housekeeping");
    jobs::reschedule_running(&pool, job.id, INTERVAL).await?;
    Ok(())
}

/// Register `moderation.suspicion_housekeeping`. Pair with [`schedule`].
pub fn register(runner: JobRunner, pool: &PgPool, moderation: Moderation) -> JobRunner {
    let pool = pool.clone();
    runner.register(KIND, 1, move |job| {
        Box::pin(handler(pool.clone(), moderation.clone(), job))
    })
}

/// Create or re-arm the singleton. Call only when [`register`] is on the runner.
pub async fn schedule(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(pool, JOB_ID, KIND, &json!({}), Duration::ZERO).await?;
    Ok(())
}
