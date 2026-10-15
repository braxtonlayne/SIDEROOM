//! Gateway shard: the HELLO / IDENTIFY / RESUME / heartbeat protocol over a pluggable
//! connection, with reconnect and backoff. [`WsConnector`] is the real WebSocket transport.

use std::time::Duration;

use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::time::{Instant, interval_at, sleep, timeout};

use crate::error::{Error, Result};
use crate::model::{Guild, Interaction, Message, Ready};

pub const OP_DISPATCH: u64 = 0;
pub const OP_HEARTBEAT: u64 = 1;
pub const OP_IDENTIFY: u64 = 2;
pub const OP_RESUME: u64 = 6;
pub const OP_RECONNECT: u64 = 7;
pub const OP_INVALID_SESSION: u64 = 9;
pub const OP_HELLO: u64 = 10;
pub const OP_HEARTBEAT_ACK: u64 = 11;

/// One frame from the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incoming {
    Text(String),
    /// The peer closed (with its close code, when it sent one).
    Close(Option<u16>),
}

#[async_trait]
pub trait GatewayConnection: Send {
    /// Next frame. Must be cancel safe (it is raced against the heartbeat timer).
    async fn recv(&mut self) -> Result<Incoming>;
    async fn send(&mut self, text: String) -> Result<()>;
}

#[async_trait]
pub trait GatewayConnector: Send + Sync {
    async fn connect(&self, url: &str) -> Result<Box<dyn GatewayConnection>>;
}

/// Close codes a reconnect cannot cure (bad token, bad shard, bad or disallowed intents).
pub fn fatal_close(code: u16) -> Option<&'static str> {
    match code {
        4004 => Some("authentication failed"),
        4010 => Some("invalid shard"),
        4011 => Some("sharding required"),
        4012 => Some("invalid API version"),
        4013 => Some("invalid intents"),
        4014 => Some("disallowed intents"),
        _ => None,
    }
}

/// A decoded gateway event.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Ready(Box<Ready>),
    Resumed,
    GuildCreate(Box<Guild>),
    GuildDelete {
        id: String,
    },
    MessageCreate(Box<Message>),
    InteractionCreate(Box<Interaction>),
    /// Anything else, with its raw payload.
    Other {
        name: String,
        data: Value,
    },
}

impl Event {
    /// Decode a dispatch. A payload that does not fit its typed shape falls back to `Other`.
    pub fn from_dispatch(name: &str, data: Value) -> Event {
        fn typed<T: serde::de::DeserializeOwned>(data: &Value) -> Option<T> {
            serde_json::from_value(data.clone()).ok()
        }
        let fallback = |data: Value| Event::Other {
            name: name.to_owned(),
            data,
        };
        match name {
            "READY" => typed(&data).map_or_else(|| fallback(data), |r| Event::Ready(Box::new(r))),
            "RESUMED" => Event::Resumed,
            "GUILD_CREATE" => {
                typed(&data).map_or_else(|| fallback(data), |g| Event::GuildCreate(Box::new(g)))
            }
            "GUILD_DELETE" => match data.get("id").and_then(Value::as_str) {
                Some(id) => Event::GuildDelete { id: id.to_owned() },
                None => fallback(data),
            },
            "MESSAGE_CREATE" => {
                typed(&data).map_or_else(|| fallback(data), |m| Event::MessageCreate(Box::new(m)))
            }
            "INTERACTION_CREATE" => typed(&data)
                .map_or_else(|| fallback(data), |i| Event::InteractionCreate(Box::new(i))),
            _ => fallback(data),
        }
    }
}

/// A dispatch from one shard, raw.
#[derive(Debug, Clone, PartialEq)]
pub struct Dispatch {
    pub shard: u32,
    pub name: String,
    pub data: Value,
}

#[derive(Debug, Clone)]
pub struct ShardConfig {
    pub token: String,
    pub intents: u64,
    /// `(shard id, shard count)`.
    pub shard: (u32, u32),
    pub url: String,
    /// Base for the reconnect delay; doubles up to `max_backoff`.
    pub min_backoff: Duration,
    pub max_backoff: Duration,
}

impl ShardConfig {
    pub fn new(
        token: impl Into<String>,
        intents: u64,
        shard: (u32, u32),
        url: impl Into<String>,
    ) -> Self {
        Self {
            token: token.into(),
            intents,
            shard,
            url: url.into(),
            min_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(60),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct SessionInfo {
    id: String,
    seq: u64,
    resume_url: Option<String>,
}

enum Cycle {
    Reconnect {
        resume: bool,
    },
    Fatal(u16),
    /// The receiving side of the dispatch channel is gone: stop quietly.
    Stopped,
}

/// Run one shard until a fatal close or until `tx` is dropped. Reconnects and resumes on its own.
pub async fn run_shard(
    cfg: ShardConfig,
    connector: &dyn GatewayConnector,
    tx: mpsc::Sender<Dispatch>,
) -> Result<()> {
    let mut session: Option<SessionInfo> = None;
    let mut backoff = cfg.min_backoff;
    loop {
        let url = session
            .as_ref()
            .and_then(|s| s.resume_url.clone())
            .unwrap_or_else(|| cfg.url.clone());
        let mut conn = match connector.connect(&url).await {
            Ok(c) => c,
            Err(_) => {
                sleep(backoff).await;
                backoff = (backoff * 2).min(cfg.max_backoff);
                continue;
            }
        };
        let cycle = connection_cycle(&cfg, conn.as_mut(), &mut session, &tx, &mut backoff).await;
        match cycle {
            Cycle::Stopped => return Ok(()),
            Cycle::Fatal(code) => {
                return Err(Error::GatewayFatal {
                    code,
                    reason: fatal_close(code).unwrap_or("fatal"),
                });
            }
            Cycle::Reconnect { resume } => {
                if !resume {
                    session = None;
                }
                sleep(backoff).await;
                backoff = (backoff * 2).min(cfg.max_backoff);
            }
        }
    }
}

async fn connection_cycle(
    cfg: &ShardConfig,
    conn: &mut dyn GatewayConnection,
    session: &mut Option<SessionInfo>,
    tx: &mpsc::Sender<Dispatch>,
    backoff: &mut Duration,
) -> Cycle {
    // HELLO first.
    let hello_ms = loop {
        match timeout(Duration::from_secs(30), conn.recv()).await {
            Ok(Ok(Incoming::Text(t))) => {
                let Ok(v) = serde_json::from_str::<Value>(&t) else {
                    continue;
                };
                if v["op"].as_u64() == Some(OP_HELLO) {
                    break v["d"]["heartbeat_interval"].as_u64().unwrap_or(41_250);
                }
            }
            Ok(Ok(Incoming::Close(code))) => return close_cycle(code, session),
            Ok(Err(_)) | Err(_) => return Cycle::Reconnect { resume: true },
        }
    };

    // IDENTIFY, or RESUME when a session is known.
    let first = match session.as_ref() {
        Some(s) => {
            json!({"op": OP_RESUME, "d": {"token": cfg.token, "session_id": s.id, "seq": s.seq}})
        }
        None => json!({"op": OP_IDENTIFY, "d": {
            "token": cfg.token,
            "intents": cfg.intents,
            "shard": [cfg.shard.0, cfg.shard.1],
            "properties": {"os": std::env::consts::OS, "browser": "sr-sdk", "device": "sr-sdk"},
        }}),
    };
    if conn.send(first.to_string()).await.is_err() {
        return Cycle::Reconnect { resume: true };
    }

    let period = Duration::from_millis(hello_ms.max(1));
    let mut ticker = interval_at(Instant::now() + period, period);
    let mut acked = true;
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if !acked {
                    // Zombied connection: no ACK for the previous heartbeat.
                    return Cycle::Reconnect { resume: true };
                }
                acked = false;
                if conn.send(heartbeat(session)).await.is_err() {
                    return Cycle::Reconnect { resume: true };
                }
            }
            frame = conn.recv() => {
                let text = match frame {
                    Ok(Incoming::Text(t)) => t,
                    Ok(Incoming::Close(code)) => return close_cycle(code, session),
                    Err(_) => return Cycle::Reconnect { resume: true },
                };
                let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
                match v["op"].as_u64() {
                    Some(OP_DISPATCH) => {
                        let name = v["t"].as_str().unwrap_or_default().to_owned();
                        if let Some(s) = v["s"].as_u64()
                            && let Some(sess) = session.as_mut() {
                                sess.seq = s;
                            }
                        let data = v["d"].clone();
                        if name == "READY" {
                            *session = Some(SessionInfo {
                                id: data["session_id"].as_str().unwrap_or_default().to_owned(),
                                seq: v["s"].as_u64().unwrap_or(0),
                                resume_url: data["resume_gateway_url"].as_str().map(str::to_owned),
                            });
                            *backoff = cfg.min_backoff;
                        }
                        if tx.send(Dispatch { shard: cfg.shard.0, name, data }).await.is_err() {
                            return Cycle::Stopped;
                        }
                    }
                    Some(OP_HEARTBEAT) => {
                        if conn.send(heartbeat(session)).await.is_err() {
                            return Cycle::Reconnect { resume: true };
                        }
                    }
                    Some(OP_HEARTBEAT_ACK) => acked = true,
                    Some(OP_RECONNECT) => return Cycle::Reconnect { resume: true },
                    Some(OP_INVALID_SESSION) => {
                        let resumable = v["d"].as_bool().unwrap_or(false);
                        return Cycle::Reconnect { resume: resumable };
                    }
                    _ => {}
                }
            }
        }
    }
}

fn heartbeat(session: &Option<SessionInfo>) -> String {
    let seq = session.as_ref().map_or(Value::Null, |s| json!(s.seq));
    json!({"op": OP_HEARTBEAT, "d": seq}).to_string()
}

fn close_cycle(code: Option<u16>, _session: &mut Option<SessionInfo>) -> Cycle {
    match code {
        Some(c) if fatal_close(c).is_some() => Cycle::Fatal(c),
        // Session timed out (4009) or invalid seq (4007): the session is gone.
        Some(4007 | 4009) => Cycle::Reconnect { resume: false },
        _ => Cycle::Reconnect { resume: true },
    }
}

/// The real transport: a WebSocket over `tokio-tungstenite` with rustls.
#[derive(Debug, Default, Clone, Copy)]
pub struct WsConnector;

struct WsConn {
    inner: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
}

#[async_trait]
impl GatewayConnection for WsConn {
    async fn recv(&mut self) -> Result<Incoming> {
        use tokio_tungstenite::tungstenite::Message as Ws;
        loop {
            match self.inner.next().await {
                Some(Ok(Ws::Text(t))) => return Ok(Incoming::Text(t.to_string())),
                Some(Ok(Ws::Close(frame))) => {
                    return Ok(Incoming::Close(frame.map(|f| u16::from(f.code))));
                }
                Some(Ok(_)) => {}
                Some(Err(e)) => return Err(Error::Transport(e.to_string())),
                None => return Ok(Incoming::Close(None)),
            }
        }
    }

    async fn send(&mut self, text: String) -> Result<()> {
        use tokio_tungstenite::tungstenite::Message as Ws;
        self.inner
            .send(Ws::text(text))
            .await
            .map_err(|e| Error::Transport(e.to_string()))
    }
}

#[async_trait]
impl GatewayConnector for WsConnector {
    async fn connect(&self, url: &str) -> Result<Box<dyn GatewayConnection>> {
        let url = if url.contains('?') {
            url.to_owned()
        } else {
            format!("{}?v=10&encoding=json", url.trim_end_matches('/'))
        };
        let (inner, _) = tokio_tungstenite::connect_async(url.as_str())
            .await
            .map_err(|e| Error::Transport(e.to_string()))?;
        Ok(Box::new(WsConn { inner }))
    }
}

#[cfg(test)]
pub(crate) mod fake {
    //! An in-memory connection: the test plays the server.
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use super::*;

    pub struct Link {
        pub to_client: mpsc::UnboundedSender<Incoming>,
        pub from_client: Arc<Mutex<Vec<Value>>>,
    }

    pub struct FakeConn {
        rx: mpsc::UnboundedReceiver<Incoming>,
        sent: Arc<Mutex<Vec<Value>>>,
    }

    #[async_trait]
    impl GatewayConnection for FakeConn {
        async fn recv(&mut self) -> Result<Incoming> {
            Ok(self.rx.recv().await.unwrap_or(Incoming::Close(None)))
        }

        async fn send(&mut self, text: String) -> Result<()> {
            self.sent
                .lock()
                .expect("lock")
                .push(serde_json::from_str(&text).expect("json"));
            Ok(())
        }
    }

    /// Hands out pre-made connections, one per `connect`, and records the URLs asked for.
    pub struct FakeConnector {
        conns: Mutex<VecDeque<FakeConn>>,
        pub urls: Mutex<Vec<String>>,
    }

    impl FakeConnector {
        pub fn new(n: usize) -> (Self, Vec<Link>) {
            let mut conns = VecDeque::new();
            let mut links = Vec::new();
            for _ in 0..n {
                let (tx, rx) = mpsc::unbounded_channel();
                let sent = Arc::new(Mutex::new(Vec::new()));
                conns.push_back(FakeConn {
                    rx,
                    sent: sent.clone(),
                });
                links.push(Link {
                    to_client: tx,
                    from_client: sent,
                });
            }
            (
                Self {
                    conns: Mutex::new(conns),
                    urls: Mutex::new(Vec::new()),
                },
                links,
            )
        }
    }

    #[async_trait]
    impl GatewayConnector for FakeConnector {
        async fn connect(&self, url: &str) -> Result<Box<dyn GatewayConnection>> {
            self.urls.lock().expect("lock").push(url.to_owned());
            let c = self
                .conns
                .lock()
                .expect("lock")
                .pop_front()
                .ok_or_else(|| Error::Transport("no more connections".into()))?;
            Ok(Box::new(c))
        }
    }

    pub fn text(v: Value) -> Incoming {
        Incoming::Text(v.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::fake::*;
    use super::*;

    fn cfg() -> ShardConfig {
        let mut c = ShardConfig::new("tok", 513, (1, 4), "wss://gw.test/gateway");
        c.min_backoff = Duration::from_millis(10);
        c
    }

    fn ready(seq: u64) -> Incoming {
        text(json!({"op": 0, "s": seq, "t": "READY", "d": {
            "session_id": "sess1", "resume_gateway_url": "wss://resume.test/gw",
            "user": {"id": "9", "bot": true}, "application": {"id": "5", "flags": 0}, "guilds": []}}))
    }

    #[tokio::test(start_paused = true)]
    async fn identifies_with_token_intents_and_shard_then_forwards_dispatches() {
        let (conn, links) = FakeConnector::new(1);
        let (tx, mut rx) = mpsc::channel(8);
        let h = tokio::spawn(async move { run_shard(cfg(), &conn, tx).await });
        links[0]
            .to_client
            .send(text(json!({"op": 10, "d": {"heartbeat_interval": 40000}})))
            .expect("hello");
        links[0].to_client.send(ready(1)).expect("ready");
        let d = rx.recv().await.expect("dispatch");
        assert_eq!((d.shard, d.name.as_str()), (1, "READY"));
        {
            let sent = links[0].from_client.lock().expect("lock");
            assert_eq!(sent[0]["op"], 2);
            assert_eq!(sent[0]["d"]["token"], "tok");
            assert_eq!(sent[0]["d"]["intents"], 513);
            assert_eq!(sent[0]["d"]["shard"], json!([1, 4]));
        }
        drop(rx);
        links[0]
            .to_client
            .send(text(
                json!({"op": 0, "s": 2, "t": "MESSAGE_CREATE", "d": {}}),
            ))
            .expect("msg");
        assert!(
            h.await.expect("join").is_ok(),
            "dropping the receiver stops the shard cleanly"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn heartbeats_carry_the_last_sequence_and_a_missing_ack_reconnects_with_resume() {
        let (conn, links) = FakeConnector::new(2);
        let (tx, mut rx) = mpsc::channel(8);
        let conn = Arc::new(conn);
        let c2 = conn.clone();
        let h = tokio::spawn(async move { run_shard(cfg(), &*c2, tx).await });
        links[0]
            .to_client
            .send(text(json!({"op": 10, "d": {"heartbeat_interval": 1000}})))
            .expect("hello");
        links[0].to_client.send(ready(7)).expect("ready");
        rx.recv().await.expect("ready dispatch");
        // First beat at 1 s, no ACK: the second tick (2 s) declares the link dead.
        sleep(Duration::from_millis(1500)).await;
        assert_eq!(
            links[0].from_client.lock().expect("lock")[1],
            json!({"op": 1, "d": 7})
        );
        // Second connection: server says HELLO; client must RESUME on the resume URL.
        links[1]
            .to_client
            .send(text(json!({"op": 10, "d": {"heartbeat_interval": 40000}})))
            .expect("hello");
        sleep(Duration::from_millis(2500)).await;
        let sent = links[1].from_client.lock().expect("lock");
        assert_eq!(sent[0]["op"], 6);
        assert_eq!(
            sent[0]["d"],
            json!({"token": "tok", "session_id": "sess1", "seq": 7})
        );
        assert_eq!(conn.urls.lock().expect("lock")[1], "wss://resume.test/gw");
        h.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn invalid_session_false_forgets_the_session_and_identifies_again() {
        let (conn, links) = FakeConnector::new(2);
        let (tx, mut rx) = mpsc::channel(8);
        let conn = Arc::new(conn);
        let c2 = conn.clone();
        let h = tokio::spawn(async move { run_shard(cfg(), &*c2, tx).await });
        links[0]
            .to_client
            .send(text(json!({"op": 10, "d": {"heartbeat_interval": 40000}})))
            .expect("hello");
        links[0].to_client.send(ready(1)).expect("ready");
        rx.recv().await.expect("ready dispatch");
        links[0]
            .to_client
            .send(text(json!({"op": 9, "d": false})))
            .expect("invalid");
        links[1]
            .to_client
            .send(text(json!({"op": 10, "d": {"heartbeat_interval": 40000}})))
            .expect("hello");
        sleep(Duration::from_secs(2)).await;
        assert_eq!(links[1].from_client.lock().expect("lock")[0]["op"], 2);
        assert_eq!(conn.urls.lock().expect("lock")[1], "wss://gw.test/gateway");
        h.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn a_fatal_close_code_ends_the_shard_with_an_error() {
        let (conn, links) = FakeConnector::new(1);
        let (tx, _rx) = mpsc::channel(8);
        let h = tokio::spawn(async move { run_shard(cfg(), &conn, tx).await });
        links[0]
            .to_client
            .send(text(json!({"op": 10, "d": {"heartbeat_interval": 40000}})))
            .expect("hello");
        links[0]
            .to_client
            .send(Incoming::Close(Some(4014)))
            .expect("close");
        let err = h.await.expect("join").expect_err("fatal");
        assert!(matches!(err, Error::GatewayFatal { code: 4014, .. }));
    }

    #[tokio::test(start_paused = true)]
    async fn a_server_reconnect_op_resumes_on_a_new_connection() {
        let (conn, links) = FakeConnector::new(2);
        let (tx, mut rx) = mpsc::channel(8);
        let h = tokio::spawn(async move { run_shard(cfg(), &conn, tx).await });
        links[0]
            .to_client
            .send(text(json!({"op": 10, "d": {"heartbeat_interval": 40000}})))
            .expect("hello");
        links[0].to_client.send(ready(3)).expect("ready");
        rx.recv().await.expect("ready dispatch");
        links[0]
            .to_client
            .send(text(json!({"op": 7, "d": null})))
            .expect("reconnect");
        links[1]
            .to_client
            .send(text(json!({"op": 10, "d": {"heartbeat_interval": 40000}})))
            .expect("hello");
        sleep(Duration::from_secs(2)).await;
        assert_eq!(links[1].from_client.lock().expect("lock")[0]["op"], 6);
        h.abort();
    }

    #[test]
    fn events_decode_typed_with_a_raw_fallback() {
        let m = Event::from_dispatch(
            "MESSAGE_CREATE",
            json!({"id": "1", "channel_id": "2", "content": "x", "author": {"id": "3"}}),
        );
        assert!(matches!(m, Event::MessageCreate(ref m) if m.content == "x"));
        let bad = Event::from_dispatch("MESSAGE_CREATE", json!({"nope": 1}));
        assert!(matches!(bad, Event::Other { .. }));
        let other = Event::from_dispatch("TYPING_START", json!({"a": 1}));
        assert!(matches!(other, Event::Other { ref name, .. } if name == "TYPING_START"));
        assert_eq!(
            Event::from_dispatch("GUILD_DELETE", json!({"id": "4"})),
            Event::GuildDelete { id: "4".into() }
        );
    }

    #[test]
    fn only_credential_and_intent_closes_are_fatal() {
        for c in [4004, 4010, 4011, 4012, 4013, 4014] {
            assert!(fatal_close(c).is_some(), "{c}");
        }
        for c in [4000, 4001, 4002, 4003, 4005, 4007, 4008, 4009, 1000] {
            assert!(fatal_close(c).is_none(), "{c}");
        }
    }
}
