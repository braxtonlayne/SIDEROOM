//! sr-server: wires config, logging and the HTTP API together.
//! See docs/02-architecture.md section 3.

use anyhow::Context;
use sr_api::{ApiSettings, AppState, build_router, internal_router, observability};
use sr_core::config::Config;
use std::net::SocketAddr;

/// Address of the loopback-only operations listener (`/internal/metrics`).
const INTERNAL_BIND: &str = "127.0.0.1:9100";

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = Config::load(None).context("loading configuration")?;
    observability::init_tracing(&cfg.log);

    // Backends are in-memory stubs until sr-db implements the repositories.
    let state = AppState::builder()
        .settings(ApiSettings::for_public_url(&cfg.server.public_url))
        .build();

    let public = tokio::net::TcpListener::bind(&cfg.server.bind)
        .await
        .with_context(|| format!("binding {}", cfg.server.bind))?;
    let internal = tokio::net::TcpListener::bind(INTERNAL_BIND)
        .await
        .with_context(|| format!("binding {INTERNAL_BIND}"))?;
    tracing::info!(bind = %cfg.server.bind, internal = INTERNAL_BIND, "listening");

    let internal_app = internal_router(state.clone());
    tokio::spawn(async move {
        if let Err(e) = axum::serve(internal, internal_app).await {
            tracing::error!(error = %e, "internal listener failed");
        }
    });

    axum::serve(
        public,
        build_router(state, axum::Router::new())
            .into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("serving")
}
