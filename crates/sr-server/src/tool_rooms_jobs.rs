//! UTL-COL-001: the `tool_rooms.sweep` singleton deletes tool rooms past their 7-day life. Reads already
//! hide expired rooms, so the sweep only reclaims the rows.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::json;
use sr_chat::tool_rooms::ToolRoomStore;
use sr_db::jobs;
use sr_db::runner::JobRunner;
use sr_db::{DbResult, PgPool};

/// Reserved singleton id of the sweep (unique among job ids).
pub const SWEEP_JOB_ID: i64 = 1660;
/// Kind of the sweep.
pub const SWEEP_KIND: &str = "tool_rooms.sweep";
const SWEEP_INTERVAL: Duration = Duration::from_secs(3600);

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Register the sweep handler. Pair with [`schedule`].
pub fn register(runner: JobRunner, pool: &PgPool, store: Arc<dyn ToolRoomStore>) -> JobRunner {
    let pool = pool.clone();
    runner.register(SWEEP_KIND, 1, move |job| {
        let store = store.clone();
        let pool = pool.clone();
        Box::pin(async move {
            let n = store.sweep(now_ms()).await.map_err(|e| e.0)?;
            if n > 0 {
                tracing::info!(removed = n, "tool room sweep removed expired rooms");
            }
            jobs::reschedule_running(&pool, job.id, SWEEP_INTERVAL).await?;
            Ok(())
        })
    })
}

/// Create or re-arm the sweep singleton. Safe on every boot.
pub async fn schedule(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(
        pool,
        SWEEP_JOB_ID,
        SWEEP_KIND,
        &json!({}),
        Duration::from_secs(60),
    )
    .await?;
    Ok(())
}
