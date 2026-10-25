//! One virtual gateway client: connect with the session cookie, IDENTIFY, wait for READY, keep the heartbeat
//! going, and time every load-test message that is dispatched to it.
//!
//! The messages the senders post carry their own send time (`lt:<unix micros>:<seq>` in the content), so the
//! fan-out latency is measured from the POST to the dispatch arriving here, on one machine clock.

use crate::stats::Samples;
use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::watch;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};

/// Where and how to connect.
#[derive(Debug, Clone)]
pub struct Conn {
    /// `ws://host:port/gateway?v=10&encoding=json`
    pub url: String,
    /// Sent as `Origin`; the server checks it for cookie sessions.
    pub origin: String,
}

/// Sessions that reached READY, in total and per guild.
pub struct Ready {
    pub total: AtomicUsize,
    pub per_guild: Vec<AtomicUsize>,
}

impl Ready {
    pub fn new(guilds: usize) -> Self {
        Self {
            total: AtomicUsize::new(0),
            per_guild: (0..guilds).map(|_| AtomicUsize::new(0)).collect(),
        }
    }
}

/// What one client saw.
#[derive(Debug, Default)]
pub struct Report {
    pub ready: bool,
    /// Time from starting the connection to READY.
    pub ready_after: Option<Duration>,
    /// Size of the READY frame in bytes (RDY-PERF-004: the payload of a member of many servers).
    pub ready_bytes: usize,
    pub error: Option<String>,
    /// Load-test messages dispatched to this client.
    pub received: u64,
    /// Send-to-dispatch latency of those.
    pub fanout: Samples,
}

/// Unix time in microseconds.
pub fn now_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_micros()).unwrap_or(u64::MAX))
}

/// The content of a load-test message.
pub fn marker(send_us: u64, seq: u64) -> String {
    format!("lt:{send_us}:{seq}")
}

/// The send time inside a load-test message's content, if it is one.
pub fn parse_marker(content: &str) -> Option<u64> {
    let rest = content.strip_prefix("lt:")?;
    rest.split(':').next()?.parse().ok()
}

/// Run one client until `stop` flips. Never fails: problems are in the [`Report`].
pub async fn run(
    conn: &Conn,
    session_token: &str,
    guild: usize,
    ready: &Ready,
    mut stop: watch::Receiver<bool>,
) -> Report {
    let mut report = Report::default();
    if let Err(e) = session(conn, session_token, guild, ready, &mut stop, &mut report).await {
        // A close we asked for is not an error.
        if !*stop.borrow() {
            report.error = Some(format!("{e:#}"));
        }
    }
    report
}

async fn session(
    conn: &Conn,
    token: &str,
    guild: usize,
    ready: &Ready,
    stop: &mut watch::Receiver<bool>,
    report: &mut Report,
) -> Result<()> {
    let started = Instant::now();
    let mut req = conn
        .url
        .as_str()
        .into_client_request()
        .context("building the gateway request")?;
    req.headers_mut()
        .insert("Cookie", format!("sr_session={token}").parse()?);
    req.headers_mut().insert("Origin", conn.origin.parse()?);
    let (mut ws, _) = connect_async(req).await.context("gateway connect")?;

    let mut heartbeat: Option<tokio::time::Interval> = None;
    let mut seq: Option<u64> = None;
    loop {
        tokio::select! {
            _ = stop.changed() => {
                let _ = ws.close(None).await;
                return Ok(());
            }
            _ = async { heartbeat.as_mut().unwrap().tick().await }, if heartbeat.is_some() => {
                ws.send(Message::text(json!({ "op": 1, "d": seq }).to_string())).await?;
            }
            msg = ws.next() => {
                let Some(msg) = msg else { anyhow::bail!("the gateway closed the connection") };
                match msg? {
                    Message::Text(t) => {
                        let v: Value = serde_json::from_str(t.as_str()).context("gateway sent invalid JSON")?;
                        if let Some(s) = v["s"].as_u64() {
                            seq = Some(s);
                        }
                        match v["op"].as_u64() {
                            Some(10) => {
                                let ms = v["d"]["heartbeat_interval"].as_u64().unwrap_or(41_250).max(1000);
                                let mut iv = tokio::time::interval(Duration::from_millis(ms));
                                iv.tick().await; // the first tick is immediate
                                heartbeat = Some(iv);
                                ws.send(Message::text(identify().to_string())).await?;
                            }
                            Some(0) => match v["t"].as_str() {
                                Some("READY") => {
                                    report.ready_bytes = t.as_str().len();
                                    report.ready = true;
                                    report.ready_after = Some(started.elapsed());
                                    ready.total.fetch_add(1, Ordering::Relaxed);
                                    if let Some(g) = ready.per_guild.get(guild) {
                                        g.fetch_add(1, Ordering::Relaxed);
                                    }
                                }
                                Some("MESSAGE_CREATE") => {
                                    if let Some(sent) = v["d"]["content"].as_str().and_then(parse_marker) {
                                        report.received += 1;
                                        report.fanout.record_us(now_us().saturating_sub(sent));
                                    }
                                }
                                _ => {}
                            },
                            Some(7) => anyhow::bail!("the gateway asked this client to reconnect"),
                            Some(9) => anyhow::bail!("the gateway answered INVALID_SESSION"),
                            _ => {}
                        }
                    }
                    Message::Close(f) => {
                        anyhow::bail!("the gateway closed the connection: {f:?}");
                    }
                    _ => {}
                }
            }
        }
    }
}

/// IDENTIFY with an empty token: the session cookie authenticates (user clients send no intents).
fn identify() -> Value {
    json!({
        "op": 2,
        "d": {
            "token": "",
            "properties": { "os": "loadtest", "browser": "sr-loadtest", "device": "sr-loadtest" }
        }
    })
}

#[cfg(test)]
#[allow(clippy::result_large_err)] // the handshake callback signature is tungstenite's
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_hdr_async;
    use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};

    #[test]
    fn markers_round_trip() {
        let m = marker(1_700_000_000_123_456, 9);
        assert_eq!(m, "lt:1700000000123456:9");
        assert_eq!(parse_marker(&m), Some(1_700_000_000_123_456));
        assert_eq!(parse_marker("hello"), None);
        assert_eq!(parse_marker("lt:soon:1"), None);
    }

    /// A gateway stand-in: HELLO, expect IDENTIFY, READY, then one load-test message and one stranger.
    async fn fake_gateway(cookie_seen: tokio::sync::oneshot::Sender<String>) -> String {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        tokio::spawn(async move {
            let (s, _) = l.accept().await.unwrap();
            let mut cookie_seen = Some(cookie_seen);
            let mut ws = accept_hdr_async(s, move |req: &Request, resp: Response| {
                let c = req
                    .headers()
                    .get("cookie")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_owned();
                if let Some(tx) = cookie_seen.take() {
                    let _ = tx.send(c);
                }
                Ok(resp)
            })
            .await
            .unwrap();
            ws.send(Message::text(
                json!({"op": 10, "d": {"heartbeat_interval": 41250}}).to_string(),
            ))
            .await
            .unwrap();
            let id = ws.next().await.unwrap().unwrap().into_text().unwrap();
            let v: Value = serde_json::from_str(id.as_str()).unwrap();
            assert_eq!(v["op"], 2);
            assert_eq!(v["d"]["token"], "");
            ws.send(Message::text(
                json!({"op": 0, "s": 1, "t": "READY", "d": {}}).to_string(),
            ))
            .await
            .unwrap();
            let sent = now_us().saturating_sub(5_000);
            for content in [marker(sent, 1), "someone else".to_owned()] {
                ws.send(Message::text(
                    json!({"op": 0, "s": 2, "t": "MESSAGE_CREATE", "d": {"content": content}})
                        .to_string(),
                ))
                .await
                .unwrap();
            }
            // Stay until the client closes.
            while let Some(Ok(m)) = ws.next().await {
                if m.is_close() {
                    break;
                }
            }
        });
        format!("ws://{addr}/gateway?v=10&encoding=json")
    }

    #[tokio::test]
    async fn identifies_by_cookie_and_times_only_load_test_messages() {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let url = fake_gateway(tx).await;
        let conn = Conn {
            url,
            origin: "http://localhost".into(),
        };
        let ready = std::sync::Arc::new(Ready::new(2));
        let (stop_tx, stop_rx) = watch::channel(false);
        let r2 = ready.clone();
        let task = tokio::spawn(async move { run(&conn, "tok123", 1, &r2, stop_rx).await });
        // Wait for READY and the two dispatches.
        for _ in 0..200 {
            if ready.total.load(Ordering::Relaxed) == 1 {
                tokio::time::sleep(Duration::from_millis(50)).await;
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        stop_tx.send(true).unwrap();
        let report = task.await.unwrap();
        assert_eq!(rx.await.unwrap(), "sr_session=tok123");
        assert!(report.ready && report.error.is_none(), "{report:?}");
        assert_eq!(report.received, 1, "the stranger's message is not counted");
        assert!(report.fanout.percentile_ms(50.0) >= 4.0);
        assert_eq!(ready.per_guild[1].load(Ordering::Relaxed), 1);
        assert_eq!(ready.per_guild[0].load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn a_refused_connection_is_reported_not_raised() {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        drop(l);
        let conn = Conn {
            url: format!("ws://{addr}/gateway?v=10&encoding=json"),
            origin: "http://localhost".into(),
        };
        let (_tx, rx) = watch::channel(false);
        let ready = Ready::new(1);
        let report = run(&conn, "t", 0, &ready, rx).await;
        assert!(!report.ready);
        assert!(report.error.unwrap().contains("gateway connect"));
    }
}
