//! Donations at runtime: the payment service built from the environment, the invoice watcher job
//! (`payments.watch`, every 15 s) and the entitlement housekeeping job (`payments.housekeeping`,
//! every 5 minutes: server level grace, Patron expiry notices).
//!
//! Configuration (all optional; without an Esplora endpoint donations stay switched off):
//!
//! * `SR_PAYMENTS_ESPLORA_URL`, the Esplora REST base (`https://mempool.space/api`, or a self-hosted
//!   electrs/mempool). Read-only: the server only asks about addresses it derived.
//! * `SR_PAYMENTS_RATE_SOURCES`, a JSON array of price sources,
//!   `[{"name":"a","url":"https://...","pointer":"/data/amount"}]` (`pointer` selects the USD price).
//! * `SR_PAYMENTS_DESCRIPTOR` and `SR_PAYMENTS_NETWORK`, a WATCH-ONLY descriptor to register when no
//!   wallet exists yet (Admin > Donations can set it instead). Private keys are refused.
//!
//! The legacy download fallback (`SR_DOWNLOADS_LEGACY_FALLBACK` and `cinema_download_grants`) was removed
//! with migration 0871: only entitlements grant the Patron download perk.
//!
//! Nothing in this module signs or broadcasts anything.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use sr_api::routes::donations::{BusPaymentEvents, ChatSlot};
use sr_db::{
    DbResult, PgPool,
    auth_repo::IdSource,
    jobs::{self, Job},
    payments_repo::PgPaymentRepo,
    runner::{HandlerError, JobRunner},
};
use sr_events::EventBus;
use sr_payments::{
    EntitlementService, PayError, PayResult, PaymentService,
    chain::{ChainSource, ChainTx, EsploraChain, HttpGet, ReqwestGet},
    descriptor::Network,
    rates::{HttpJsonRate, RateService, RateSettings, RateSource},
    repo::{PaymentRepo, RepoSource},
};

pub const WATCH_KIND: &str = "payments.watch";
pub const HOUSEKEEP_KIND: &str = "payments.housekeeping";
pub const WATCH_JOB_ID: i64 = 540;
pub const HOUSEKEEP_JOB_ID: i64 = 541;
const WATCH_INTERVAL: Duration = Duration::from_secs(15);
const HOUSEKEEP_INTERVAL: Duration = Duration::from_secs(300);
const WATCH_BATCH: u32 = 200;

/// Stands in when no Esplora endpoint is configured.
struct NoChain;

#[async_trait]
impl ChainSource for NoChain {
    async fn tip_height(&self) -> PayResult<u32> {
        Err(PayError::Chain("no Esplora endpoint is configured".into()))
    }

    async fn address_txs(&self, _address: &str) -> PayResult<Vec<ChainTx>> {
        Err(PayError::Chain("no Esplora endpoint is configured".into()))
    }
}

#[derive(Deserialize)]
struct RateSpec {
    name: String,
    url: String,
    pointer: String,
}

/// What the composition root needs from the donations runtime.
pub struct PaymentsRuntime {
    pub service: Arc<PaymentService>,
    pub entitlements: Arc<EntitlementService>,
    /// Fill with the chat module once built so a server-level payment posts its notice.
    pub chat: ChatSlot,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(0))
        .unwrap_or(0)
}

/// Build the service from the environment. Misconfiguration is logged, never fatal: donations
/// simply stay off.
pub async fn build(pool: &PgPool, ids: IdSource, bus: Arc<dyn EventBus>) -> PaymentsRuntime {
    let chat = ChatSlot::default();
    let repo: Arc<dyn PaymentRepo> = Arc::new(PgPaymentRepo::new(pool.clone(), ids.clone()));
    let clock: Arc<dyn Fn() -> i64 + Send + Sync> = Arc::new(now_ms);
    let allow_private = std::env::var("SR_PAYMENTS_ALLOW_PRIVATE_HOSTS")
        .is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    let http: Option<Arc<dyn HttpGet>> = Some(Arc::new(ReqwestGet::new(allow_private)));
    let esplora = std::env::var("SR_PAYMENTS_ESPLORA_URL")
        .ok()
        .filter(|u| !u.trim().is_empty());
    let chain: Arc<dyn ChainSource> = match (&esplora, &http) {
        (Some(url), Some(http)) => Arc::new(EsploraChain::new(url.trim(), http.clone())),
        _ => Arc::new(NoChain),
    };
    let mut sources: Vec<Arc<dyn RateSource>> = Vec::new();
    if let (Some(raw), Some(http)) = (
        std::env::var("SR_PAYMENTS_RATE_SOURCES")
            .ok()
            .filter(|r| !r.trim().is_empty()),
        &http,
    ) {
        match serde_json::from_str::<Vec<RateSpec>>(&raw) {
            Ok(specs) => {
                for s in specs.into_iter().take(5) {
                    sources.push(Arc::new(HttpJsonRate::new(
                        &s.name,
                        &s.url,
                        &s.pointer,
                        http.clone(),
                    )));
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "payments: SR_PAYMENTS_RATE_SOURCES is not valid")
            }
        }
    }
    let rates = Arc::new(RateService::new(sources, RateSettings::default()));
    let entitlements = Arc::new(EntitlementService::new(
        Arc::new(RepoSource(repo.clone())),
        clock.clone(),
        Duration::from_secs(60),
    ));
    let service = Arc::new(
        PaymentService::new(
            repo.clone(),
            chain,
            rates,
            Arc::new(BusPaymentEvents::new(bus).with_chat_slot(chat.clone())),
            entitlements.clone(),
            clock,
            ids,
        )
        .with_chain_configured(esplora.is_some()),
    );
    if esplora.is_none() {
        tracing::info!("payments: SR_PAYMENTS_ESPLORA_URL is not set; donations stay off");
    }
    seed_wallet(&service).await;
    PaymentsRuntime {
        service,
        entitlements,
        chat,
    }
}

/// Registers the wallet from the environment when none exists. The descriptor is never logged.
async fn seed_wallet(service: &PaymentService) {
    let Ok(descriptor) = std::env::var("SR_PAYMENTS_DESCRIPTOR") else {
        return;
    };
    match service.repo().active_wallet().await {
        Ok(Some(_)) => return,
        Ok(None) => {}
        Err(e) => {
            tracing::error!(error = %e, "payments: reading the wallet failed");
            return;
        }
    }
    let network = std::env::var("SR_PAYMENTS_NETWORK")
        .ok()
        .and_then(|n| Network::parse(n.trim()))
        .unwrap_or(Network::Mainnet);
    match service
        .set_wallet(&descriptor, network, 20, sr_admin::SYSTEM_USER_ID)
        .await
    {
        Ok((w, _)) => {
            tracing::info!(fingerprint = %w.fingerprint, network = network.as_str(), "payments: wallet registered from the environment")
        }
        Err(e) => tracing::error!(error = %e, "payments: SR_PAYMENTS_DESCRIPTOR was refused"),
    }
}

/// Register both handlers; pair with [`schedule`].
pub fn register(runner: JobRunner, pool: &PgPool, service: Arc<PaymentService>) -> JobRunner {
    let (watch_pool, keep_pool) = (pool.clone(), pool.clone());
    let (watch, keep) = (service.clone(), service);
    runner
        .register(WATCH_KIND, 1, move |job: Job| {
            let (pool, svc) = (watch_pool.clone(), watch.clone());
            Box::pin(async move {
                let started = now_ms();
                let mut status = svc.repo().watcher_status().await.unwrap_or_default();
                status.last_run_ms = Some(started);
                match svc.poll_due(WATCH_BATCH).await {
                    Ok(sum) => {
                        status.polled = sum.polled;
                        if sum.tip.is_some() {
                            status.tip_height = sum.tip;
                        }
                        if sum.errors == 0 {
                            status.last_ok_ms = Some(started);
                            status.last_error = None;
                        } else {
                            status.last_error = Some(format!(
                                "{} of {} invoices could not be read",
                                sum.errors, sum.polled
                            ));
                        }
                    }
                    Err(e) => status.last_error = Some(e.to_string()),
                }
                let _ = svc.repo().set_watcher_status(&status).await;
                jobs::reschedule_running(&pool, job.id, WATCH_INTERVAL).await?;
                Ok(())
            })
        })
        .register(HOUSEKEEP_KIND, 1, move |job: Job| {
            let (pool, svc) = (keep_pool.clone(), keep.clone());
            Box::pin(async move {
                if let Err(e) = svc.housekeeping().await {
                    tracing::warn!(error = %e, "payments housekeeping failed");
                }
                jobs::reschedule_running(&pool, job.id, HOUSEKEEP_INTERVAL).await?;
                Ok::<(), HandlerError>(())
            })
        })
}

/// Create or re-arm the periodic singletons. Call only when [`register`] is on the runner.
pub async fn schedule(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(pool, WATCH_JOB_ID, WATCH_KIND, &json!({}), WATCH_INTERVAL).await?;
    jobs::schedule_singleton(
        pool,
        HOUSEKEEP_JOB_ID,
        HOUSEKEEP_KIND,
        &json!({}),
        HOUSEKEEP_INTERVAL,
    )
    .await?;
    Ok(())
}
