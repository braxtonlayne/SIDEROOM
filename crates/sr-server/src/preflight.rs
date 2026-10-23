//! The preflight checks for `POST /admin/system/preflight` (RDY-OPS-005): the same list `sr-cli preflight`
//! runs, over the live pool and storage of this process. The checks that send mail or post cards stay with the
//! command line.

use std::sync::Arc;

use async_trait::async_trait;
use sqlx::PgPool;
use sr_api::routes::admin::PreflightRunner;
use sr_core::config::{Config, is_development_host};
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
    /// URL (the production smoke stack's Caddy). It is honoured only on a development host; anywhere else the
    /// server refuses to start (D-357).
    pub fn new(
        pool: PgPool,
        storage: Arc<dyn ObjectStorage>,
        cfg: &Config,
    ) -> anyhow::Result<Self> {
        let env = |k: &str| std::env::var(k).ok();
        let mut settings = Settings::from_config(cfg, env("SR_CONFIG").as_deref(), &env);
        settings.insecure_tls = insecure_tls_allowed(
            env("SR_PREFLIGHT_INSECURE_TLS").as_deref(),
            &cfg.server.public_url,
        )?;
        Ok(Self {
            db: PgDb(pool),
            storage,
            settings,
        })
    }
}

/// Reads `SR_PREFLIGHT_INSECURE_TLS`: `1` turns certificate verification off for the public-URL row, and is an
/// error unless `server.public_url` is a development host (D-357).
pub fn insecure_tls_allowed(value: Option<&str>, public_url: &str) -> anyhow::Result<bool> {
    let on = value.is_some_and(|v| v.trim() == "1");
    if on && !is_development_host(public_url) {
        anyhow::bail!(
            "SR_PREFLIGHT_INSECURE_TLS is only honoured when server.public_url is a localhost address; unset it on this host"
        );
    }
    Ok(on)
}

#[cfg(test)]
mod tests {
    use super::insecure_tls_allowed;

    #[test]
    fn insecure_tls_only_on_a_development_host() {
        assert!(insecure_tls_allowed(Some("1"), "https://smoke.localhost").unwrap());
        assert!(insecure_tls_allowed(Some("1"), "http://localhost:5173").unwrap());
        assert!(!insecure_tls_allowed(None, "https://chat.example.org").unwrap());
        assert!(!insecure_tls_allowed(Some("0"), "https://chat.example.org").unwrap());
        let err = insecure_tls_allowed(Some("1"), "https://chat.example.org").unwrap_err();
        assert!(
            err.to_string().contains("SR_PREFLIGHT_INSECURE_TLS"),
            "{err}"
        );
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
