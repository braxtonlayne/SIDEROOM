//! Standalone SFU node. Configuration comes from the environment:
//!
//! | Variable | Meaning | Default |
//! |---|---|---|
//! | `SR_VOICE_HTTP_BIND` | WebSocket + `/metrics` listener | `127.0.0.1:8081` |
//! | `SR_VOICE_TOKEN_SECRET` | shared join-token secret (required) | none |
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
        token_secret: env("SR_VOICE_TOKEN_SECRET")
            .context("SR_VOICE_TOKEN_SECRET is required")?
            .into_bytes(),
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
    let app = sr_voice::router(handle.clone()).route(
        "/metrics",
        axum::routing::get(move || {
            let prom = prom.clone();
            async move { prom.render() }
        }),
    );
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
