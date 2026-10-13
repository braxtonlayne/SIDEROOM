//! Nightly verification of the staff access-log hash chain (ADM-STF-008, doc 07
//! section 5). A break raises an `AccessLogChainBroken` staff alert through the
//! admin service. The job re-queues itself like the other recurring singletons.

use std::sync::Arc;

use serde_json::json;
use sr_admin::AdminService;
use sr_db::{
    DbResult, PgPool,
    admin_repo::PgAdminRepo,
    auth_repo::IdSource,
    jobs::{self, Job},
    runner::{HandlerError, JobRunner},
};
use sr_events::EventBus;

/// Reserved singleton id (below any Snowflake).
pub const VERIFY_JOB_ID: i64 = 130;
pub const VERIFY_KIND: &str = "admin.access_log.verify";
const HOUR_UTC: u32 = 4;
const MINUTE_UTC: u32 = 15;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(0))
        .unwrap_or(0)
}

fn delay() -> std::time::Duration {
    jobs::delay_until_daily_utc(now_ms(), HOUR_UTC, MINUTE_UTC)
}

/// Register the handler on `runner`; pair with [`schedule`].
pub fn register(
    runner: JobRunner,
    pool: &PgPool,
    ids: IdSource,
    bus: Arc<dyn EventBus>,
) -> JobRunner {
    let pool = pool.clone();
    runner.register(VERIFY_KIND, 1, move |job: Job| {
        let pool = pool.clone();
        let service = AdminService::new(Arc::new(PgAdminRepo::new(pool.clone(), ids.clone())))
            .with_bus(bus.clone());
        Box::pin(async move {
            let report = service
                .verify_and_alert()
                .await
                .map_err(|e| -> HandlerError { Box::new(std::io::Error::other(e.to_string())) })?;
            if report.ok {
                tracing::info!(rows = report.rows_checked, "staff access log verified");
            } else {
                tracing::error!(
                    seq = report.first_bad_seq,
                    "staff access log chain is BROKEN"
                );
            }
            jobs::reschedule_running(&pool, job.id, delay()).await?;
            Ok(())
        })
    })
}

/// Create or re-arm the nightly verification singleton.
pub async fn schedule(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(pool, VERIFY_JOB_ID, VERIFY_KIND, &json!({}), delay()).await?;
    Ok(())
}
