//! MOD-GRD-001 wiring: the restriction check the REST layer asks, the banner refresh after a notice, and the
//! `moderation.restrictions` job that expires restrictions and removes the unactivated accounts nobody cleared
//! after 30 days.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use serde_json::json;
use sqlx::PgPool;
use sr_admin::AdminService;
use sr_api::auth::AccountRestrictions;
use sr_db::DbResult;
use sr_db::jobs::{self, Job};
use sr_db::runner::{HandlerError, JobRunner};
use sr_moderation::{Moderation, NoticeSink};

/// Kind of the housekeeping job.
pub const SWEEP_KIND: &str = "moderation.restrictions";
/// Reserved singleton id: the first of this wave's migration range (2120 to 2129); unique among `*_JOB_ID`.
pub const SWEEP_JOB_ID: i64 = 2120;
/// How often the job runs.
pub const INTERVAL: Duration = Duration::from_secs(3600);

/// A timeout or a case freeze in force stops posting. A failed lookup is an error that the posting routes answer with 503 (D-598); it never lets a post through.
pub struct PgRestrictions(pub PgPool);

#[async_trait]
impl AccountRestrictions for PgRestrictions {
    async fn restricted_from_posting(&self, user_id: i64) -> Result<bool, String> {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM member_restrictions WHERE account_id = $1 AND state = 'active' \
             AND ends_at > now() AND kind IN ('timeout', 'case_freeze'))",
        )
        .bind(user_id)
        .fetch_one(&self.0)
        .await
        .map_err(|e| e.to_string())
    }
}

/// Tells every session to re-read the banners after a notice.
pub struct AdminNoticeSink(pub AdminService);

#[async_trait]
impl NoticeSink for AdminNoticeSink {
    async fn published(&self) {
        self.0.banners_changed().await;
    }
}

/// Register the handler on the runner.
pub fn register(runner: JobRunner, moderation: Moderation) -> JobRunner {
    runner.register(SWEEP_KIND, 1, move |job: Job| {
        let m = moderation.clone();
        Box::pin(async move {
            match m.sweep_restrictions().await {
                Ok((expired, removed)) => {
                    tracing::info!(expired, removed, "moderation.restrictions");
                }
                // The next pass retries; a failure must not stop the singleton.
                Err(e) => tracing::warn!(error = %e, "moderation.restrictions failed"),
            }
            jobs::reschedule_running(m.pool(), job.id, INTERVAL)
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
        SWEEP_JOB_ID,
        SWEEP_KIND,
        &json!({}),
        Duration::from_secs(120),
    )
    .await?;
    Ok(())
}

/// The restriction check as an `AppState` extension value.
pub fn gate(pool: PgPool) -> Arc<sr_api::auth::RestrictionGate> {
    Arc::new(sr_api::auth::RestrictionGate(Arc::new(PgRestrictions(
        pool,
    ))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_job_id_is_the_first_of_the_wave_range() {
        assert_eq!(SWEEP_JOB_ID, 2120);
        assert_eq!(SWEEP_KIND, "moderation.restrictions");
    }
}
