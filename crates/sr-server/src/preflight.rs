//! The preflight checks for `POST /admin/system/preflight` (RDY-OPS-005): the same list `sr-cli preflight`
//! runs, over the live pool and storage of this process. The checks that send mail or post cards stay with the
//! command line.

use std::sync::Arc;

use async_trait::async_trait;
use sqlx::PgPool;
use sr_api::routes::admin::PreflightRunner;
use sr_core::config::Config;
use sr_preflight::{
    Ctx, Report, Settings,
    probes::{PgDb, RealHost, RealNet},
};
use sr_storage::ObjectStorage;

pub struct LivePreflight {
    db: PgDb,
    storage: Arc<dyn ObjectStorage>,
    settings: Settings,
}

impl LivePreflight {
    /// `SR_PREFLIGHT_INSECURE_TLS=1` accepts the certificate of a private CA when the box fetches its own public
    /// URL (the production smoke stack's Caddy).
    pub fn new(pool: PgPool, storage: Arc<dyn ObjectStorage>, cfg: &Config) -> Self {
        let env = |k: &str| std::env::var(k).ok();
        let mut settings = Settings::from_config(cfg, env("SR_CONFIG").as_deref(), &env);
        settings.insecure_tls = env("SR_PREFLIGHT_INSECURE_TLS").is_some_and(|v| v == "1");
        Self {
            db: PgDb(pool),
            storage,
            settings,
        }
    }
}

#[async_trait]
impl PreflightRunner for LivePreflight {
    async fn run(&self) -> Report {
        sr_preflight::run(&Ctx {
            db: &self.db,
            storage: self.storage.as_ref(),
            net: &RealNet,
            host: &RealHost,
            settings: &self.settings,
        })
        .await
    }
}
