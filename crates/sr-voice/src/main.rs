//! Standalone SFU node. Configuration comes from the environment:
//!
//! | Variable | Meaning | Default |
//! |---|---|---|
//! | `SR_VOICE_HTTP_BIND` | WebSocket + `/metrics` listener | `127.0.0.1:8081` |
//! | `SR_VOICE_TOKEN_SECRET` | shared join-token secret (required) | none |
//! | `SR_VOICE_TOKEN_SECRET_FILE` | file holding the secret (wins over the variable above) | none |
//! | `SR_VOICE_UDP_MIN` / `SR_VOICE_UDP_MAX` | media UDP range | `50000` / `50099` |
//! | `SR_VOICE_TCP_PORT` | ICE-TCP port (0 = off) | off |
//! | `SR_VOICE_ANNOUNCE_IPS` | comma list of candidate IPs | `127.0.0.1` |
//!
//! In v1 the gateway reaches the SFU only in-process (`sr-server` embeds it), so a
//! standalone node cannot receive server-mute or disconnect commands yet; see
//! `sr_voice_proto::control`.

use std::net::IpAddr;

use anyhow::Context;

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let mut cfg = sr_voice::SfuConfig {
        token_secret: token_secret()?.into_bytes(),
        ..sr_voice::SfuConfig::default()
    };
    if let Some(v) = env("SR_VOICE_UDP_MIN") {
        cfg.udp_port_min = v.parse().context("SR_VOICE_UDP_MIN")?;
    }
    if let Some(v) = env("SR_VOICE_UDP_MAX") {
        cfg.udp_port_max = v.parse().context("SR_VOICE_UDP_MAX")?;
    }
    if let Some(v) = env("SR_VOICE_TCP_PORT") {
        let p: u16 = v.parse().context("SR_VOICE_TCP_PORT")?;
        cfg.tcp_port = (p != 0).then_some(p);
    }
    if let Some(v) = env("SR_VOICE_ANNOUNCE_IPS") {
        cfg.announce_ips = v
            .split(',')
            .map(|s| s.trim().parse::<IpAddr>())
            .collect::<Result<_, _>>()
            .context("SR_VOICE_ANNOUNCE_IPS")?;
    }
    let bind = env("SR_VOICE_HTTP_BIND").unwrap_or_else(|| "127.0.0.1:8081".into());

    let prom = metrics_exporter_prometheus::PrometheusBuilder::new()
        .install_recorder()
        .context("installing the metrics recorder")?;
    let handle = sr_voice::start(cfg).await?;
    let app = sr_voice::router(handle.clone())
        .route(
            "/metrics",
            axum::routing::get(move || {
                let prom = prom.clone();
                async move { prom.render() }
            }),
        )
        // OPS-IDX-001: the layer inside `router` does not cover the route added here.
        .layer(axum::middleware::map_response(sr_voice::noindex));
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .with_context(|| format!("binding {bind}"))?;
    tracing::info!(%bind, udp = ?handle.udp_ports().first(), "sr-voice listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    handle.drain().await;
    handle.shutdown().await;
    Ok(())
}

/// The join-token secret from `SR_VOICE_TOKEN_SECRET_FILE` (Docker/Kubernetes secret
/// mount, trailing newline trimmed) or `SR_VOICE_TOKEN_SECRET`.
fn token_secret() -> anyhow::Result<String> {
    if let Some(path) = env("SR_VOICE_TOKEN_SECRET_FILE") {
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading SR_VOICE_TOKEN_SECRET_FILE ({path})"))?;
        let s = raw.trim_end_matches(['\r', '\n']).to_owned();
        anyhow::ensure!(!s.is_empty(), "SR_VOICE_TOKEN_SECRET_FILE is empty");
        return Ok(s);
    }
    env("SR_VOICE_TOKEN_SECRET")
        .context("SR_VOICE_TOKEN_SECRET (or SR_VOICE_TOKEN_SECRET_FILE) is required")
}
