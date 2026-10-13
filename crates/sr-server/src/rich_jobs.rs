//! Wiring of the rich message features (LNG-MSG-010, LNG-MSG-014): the `unfurl`
//! feature module, the job scheduler over the durable queue, and the handlers of
//! `messages.unfurl`, `messages.poll_finalize` and the `messages.poll_sweep`
//! singleton.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use sr_api::routes::messages::MessagesDeps;
use sr_api::routes::messages::rich::{
    JOB_POLL_FINALIZE, JOB_POLL_SWEEP, JOB_UNFURL, JobScheduler, POLL_SWEEP_JOB_ID,
    UNFURL_FEATURE_ID, UnfurlGate,
};
use sr_api::routes::messages::rich_jobs;
use sr_core::{ChannelId, GuildId, MessageId};
use sr_db::jobs::{self, Job, NewJob};
use sr_db::runner::{HandlerError, JobRunner};
use sr_db::{DbResult, PgPool};
use sr_features::{Feature, FeatureMeta, FeatureService, Scope, ScopeRef, SettingsSchema};

use crate::search_jobs::JobIds;

/// Interval of the poll sweep; the per-poll job normally finalises on time and the
/// sweep catches polls whose job was lost.
const SWEEP_INTERVAL: Duration = Duration::from_secs(30);

/// The `unfurl` feature module (default on): switches link previews off for the
/// whole platform (kill switch) or for single guilds.
#[derive(Default)]
pub struct UnfurlFeature;

impl Feature for UnfurlFeature {
    fn id(&self) -> &'static str {
        UNFURL_FEATURE_ID
    }
    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: "Link previews".into(),
            description: "Fetch a title, description and image for links posted in messages."
                .into(),
            section: "Lounge".into(),
            phase: 1,
            default_enabled: true,
        }
    }
    fn settings_schema(&self) -> SettingsSchema {
        SettingsSchema::default()
    }
    fn scopes(&self) -> &[Scope] {
        &[Scope::Platform, Scope::Guild]
    }
}

/// Reads the `unfurl` flag of a guild.
pub struct FeatureUnfurlGate(pub FeatureService);

#[async_trait]
impl UnfurlGate for FeatureUnfurlGate {
    async fn enabled(&self, guild: GuildId) -> bool {
        // A failing flag lookup must not stop previews or take messages down.
        self.0
            .is_enabled(UNFURL_FEATURE_ID, &ScopeRef::Guild { guild })
            .await
            .unwrap_or(true)
    }
}

/// Enqueues onto the Postgres job queue.
pub struct PgJobScheduler {
    pub pool: PgPool,
    pub ids: JobIds,
}

#[async_trait]
impl JobScheduler for PgJobScheduler {
    async fn enqueue(&self, kind: &str, payload: Value, delay: Duration) -> Result<(), String> {
        let mut job = NewJob::new((self.ids)(), kind, payload);
        job.delay = delay;
        jobs::enqueue(&self.pool, &job)
            .await
            .map_err(|e| e.to_string())
    }
}

fn ids_of(job: &Job) -> Result<(ChannelId, MessageId), HandlerError> {
    let get = |k: &str| job.payload.get(k).and_then(Value::as_u64);
    match (get("channel_id"), get("message_id")) {
        (Some(c), Some(m)) => Ok((ChannelId::new(c), MessageId::new(m))),
        _ => Err("job payload needs channel_id and message_id".into()),
    }
}

fn err(e: impl std::fmt::Debug) -> HandlerError {
    format!("{e:?}").into()
}

/// Register the handlers. Pair with [`schedule_poll_sweep`].
pub fn register(runner: JobRunner, pool: &PgPool, deps: Arc<MessagesDeps>) -> JobRunner {
    let d1 = deps.clone();
    let d2 = deps.clone();
    let pool = pool.clone();
    runner
        .register(JOB_UNFURL, 4, move |job| {
            let deps = d1.clone();
            Box::pin(async move {
                let (c, m) = ids_of(&job)?;
                rich_jobs::run_unfurl(&deps, c, m).await.map_err(err)?;
                Ok(())
            })
        })
        .register(JOB_POLL_FINALIZE, 2, move |job| {
            let deps = d2.clone();
            Box::pin(async move {
                let (c, m) = ids_of(&job)?;
                rich_jobs::finalize_poll(&deps, c, m).await.map_err(err)?;
                Ok(())
            })
        })
        .register(JOB_POLL_SWEEP, 1, move |job| {
            let deps = deps.clone();
            let pool = pool.clone();
            Box::pin(async move {
                let n = rich_jobs::sweep_polls(&deps).await.map_err(err)?;
                if n > 0 {
                    tracing::info!(finalized = n, "poll sweep finalised polls");
                }
                jobs::reschedule_running(&pool, job.id, SWEEP_INTERVAL).await?;
                Ok(())
            })
        })
}

/// Create or re-arm the poll sweep singleton. Safe on every boot.
pub async fn schedule_poll_sweep(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(
        pool,
        POLL_SWEEP_JOB_ID,
        JOB_POLL_SWEEP,
        &json!({}),
        Duration::from_secs(5),
    )
    .await?;
    Ok(())
}
