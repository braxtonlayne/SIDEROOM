//! Per-connection state machine, independent of the WebSocket library.
//!
//! `HELLO -> (IDENTIFY | RESUME) -> READY/RESUMED -> heartbeat/dispatch loop`.

use std::sync::Arc;

use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep_until};

/// Metric key of the dispatch size histogram (same string as
/// `sr_api::observability::names::GATEWAY_DISPATCH_BYTES`).
pub const DISPATCH_BYTES: &str = "sr_gateway_dispatch_bytes";

use crate::compress::{Compression, zstd_compress};
use crate::hub::Gateway;
use crate::protocol::{Identify, Payload, PresenceUpdate, Resume, close, intents, op};
use crate::session::{Out, Session, SessionHandle};
use crate::traits::{AuthError, AuthedUser};

/// Transport-level frame. The axum adapter maps WebSocket messages to these;
/// tests drive them directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    Text(String),
    Binary(Vec<u8>),
    Close { code: u16, reason: String },
}

#[derive(Debug, Clone, Default)]
pub struct ConnectParams {
    pub compression: Compression,
    /// Value of the `sr_session` cookie on the upgrade request, if any.
    pub session_cookie: Option<String>,
    /// `Origin` header of the upgrade request, if any.
    pub origin: Option<String>,
}

/// Inbound client ops allowed per connection in each window (docs/05 section 6.2).
const RATE_LIMIT_OPS: u32 = 120;
const RATE_LIMIT_WINDOW: std::time::Duration = std::time::Duration::from_secs(60);

struct Conn {
    gw: Gateway,
    out: mpsc::Sender<Frame>,
    params: ConnectParams,
    id: u64,
    session: Option<Arc<Session>>,
    session_tx: Option<mpsc::Sender<Out>>,
    cap: usize,
    deadline: Instant,
    window_start: Instant,
    window_ops: u32,
    /// Earliest time a heartbeat may mark the user seen again.
    next_seen: Instant,
}

/// A live session refreshes the user's `last_seen_at` at most this often (D-166).
const SEEN_REFRESH: std::time::Duration = std::time::Duration::from_secs(600);

/// Why the loop ended.
enum Stop {
    /// Close with this code.
    Close(u16, &'static str),
    /// Peer is gone; nothing to send.
    Gone,
}

/// Serve one connection until it ends. `incoming` is what the client sent,
/// `outgoing` is what the server writes back.
pub async fn run_connection(
    gw: Gateway,
    params: ConnectParams,
    mut incoming: mpsc::Receiver<Frame>,
    outgoing: mpsc::Sender<Frame>,
) {
    let id = gw.next_conn_id();
    let cap = gw.config().resume_buffer.max(1024) + 64;
    let (session_tx, mut session_rx) = mpsc::channel::<Out>(cap);
    let deadline = Instant::now() + gw.config().heartbeat_interval + gw.config().heartbeat_grace;
    let mut c = Conn {
        gw,
        out: outgoing,
        params,
        id,
        session: None,
        session_tx: Some(session_tx),
        cap,
        deadline,
        window_start: Instant::now(),
        window_ops: 0,
        next_seen: Instant::now(),
    };
    let stop = c.run(&mut incoming, &mut session_rx).await;
    if let Stop::Close(code, reason) = stop {
        let _ = c
            .out
            .send(Frame::Close {
                code,
                reason: reason.into(),
            })
            .await;
    }
    if let Some(s) = c.session.clone() {
        s.detach(c.id);
        c.gw.session_detached(&s).await;
    }
}

impl Conn {
    async fn run(
        &mut self,
        incoming: &mut mpsc::Receiver<Frame>,
        session_rx: &mut mpsc::Receiver<Out>,
    ) -> Stop {
        let interval = self.gw.config().heartbeat_interval;
        let hello = Payload::new(
            op::HELLO,
            json!({ "heartbeat_interval": u64::try_from(interval.as_millis()).unwrap_or(u64::MAX) }),
        );
        if let Err(s) = self.send_text(hello.to_text()).await {
            return s;
        }
        loop {
            let step = tokio::select! {
                () = sleep_until(self.deadline) => Err(Stop::Close(close::SESSION_TIMED_OUT, "session timed out")),
                m = session_rx.recv(), if self.session.is_some() => match m {
                    Some(Out::Frame(t)) => {
                        self.record_dispatch_bytes(t.len());
                        self.send_text(t.to_string()).await
                    }
                    Some(Out::Reconnect) => {
                        let _ = self.send_text(Payload::new(op::RECONNECT, Value::Null).to_text()).await;
                        Err(Stop::Close(close::UNKNOWN_ERROR, "reconnect"))
                    }
                    // Taken over by a newer connection, or fell too far behind.
                    None => Err(Stop::Close(close::UNKNOWN_ERROR, "session detached")),
                },
                f = incoming.recv() => match f {
                    None | Some(Frame::Close { .. }) => Err(Stop::Gone),
                    Some(Frame::Binary(_)) => Err(Stop::Close(close::DECODE_ERROR, "binary frames unsupported")),
                    Some(Frame::Text(_)) if !self.count_op() => {
                        Err(Stop::Close(close::RATE_LIMITED, "rate limited"))
                    }
                    Some(Frame::Text(t)) => self.on_text(&t, session_rx).await,
                },
            };
            if let Err(s) = step {
                return s;
            }
        }
    }

    /// Count one inbound op; `false` once the window's budget is spent.
    fn count_op(&mut self) -> bool {
        if self.window_start.elapsed() >= RATE_LIMIT_WINDOW {
            self.window_start = Instant::now();
            self.window_ops = 0;
        }
        self.window_ops += 1;
        self.window_ops <= RATE_LIMIT_OPS
    }

    /// Resolve IDENTIFY/RESUME credentials. An empty token means cookie auth: the
    /// upgrade must carry `sr_session` and an `Origin` in the allowed list.
    async fn authenticate(&self, token: &str) -> Result<AuthedUser, AuthError> {
        if !token.is_empty() {
            return self.gw.inner.auth.authenticate(token).await;
        }
        let cfg = self.gw.config();
        let origin = self.params.origin.as_deref().ok_or(AuthError::Invalid)?;
        if !cfg.allowed_origins.iter().any(|o| o == origin) {
            return Err(AuthError::Invalid);
        }
        let cookie = self
            .params
            .session_cookie
            .as_deref()
            .ok_or(AuthError::Invalid)?;
        self.gw.inner.auth.authenticate_session_cookie(cookie).await
    }

    /// Feed the `sr_gateway_dispatch_bytes` histogram (uncompressed size).
    fn record_dispatch_bytes(&self, len: usize) {
        let compression = match self.params.compression {
            Compression::None => "none",
            Compression::ZstdStream => "zstd",
        };
        metrics::histogram!(DISPATCH_BYTES, "compression" => compression).record(len as f64);
    }

    async fn send_text(&self, text: String) -> Result<(), Stop> {
        let frame = match self.params.compression {
            Compression::None => Frame::Text(text),
            Compression::ZstdStream => Frame::Binary(zstd_compress(&text)),
        };
        self.out.send(frame).await.map_err(|_| Stop::Gone)
    }

    async fn send_payload(&self, op: u8, d: Value) -> Result<(), Stop> {
        self.send_text(Payload::new(op, d).to_text()).await
    }

    async fn on_text(
        &mut self,
        text: &str,
        session_rx: &mut mpsc::Receiver<Out>,
    ) -> Result<(), Stop> {
        let p: Payload = serde_json::from_str(text)
            .map_err(|_| Stop::Close(close::DECODE_ERROR, "decode error"))?;
        if !op::is_client_op(p.op) {
            return Err(Stop::Close(close::UNKNOWN_OPCODE, "unknown opcode"));
        }
        match p.op {
            op::HEARTBEAT => {
                let cfg = self.gw.config();
                self.deadline = Instant::now() + cfg.heartbeat_interval + cfg.heartbeat_grace;
                self.touch_seen();
                self.send_payload(op::HEARTBEAT_ACK, Value::Null).await
            }
            op::IDENTIFY => self.identify(p.d).await,
            op::RESUME => self.resume(p.d, session_rx).await,
            _ if self.session.is_none() => {
                Err(Stop::Close(close::NOT_AUTHENTICATED, "not authenticated"))
            }
            op::PRESENCE_UPDATE => {
                let upd: PresenceUpdate = serde_json::from_value(p.d)
                    .map_err(|_| Stop::Close(close::DECODE_ERROR, "bad presence"))?;
                if let Some(s) = &self.session {
                    self.gw.set_presence(s.user, upd).await;
                }
                Ok(())
            }
            op::VOICE_STATE_UPDATE => {
                if let Some(s) = self.session.clone() {
                    self.gw.voice_state_update(&s, p.d).await;
                }
                Ok(())
            }
            // Member requests, SR ops: routed to registered handlers; unclaimed
            // opcodes are ignored.
            _ => {
                if let Some(s) = self.session.clone() {
                    let handle = SessionHandle::new(s);
                    for h in self.gw.config().handlers.clone() {
                        if h.ops().contains(&p.op) {
                            h.handle(&handle, p.op, p.d.clone()).await;
                        }
                    }
                }
                Ok(())
            }
        }
    }

    /// Mark the session's user seen, at most once per [`SEEN_REFRESH`].
    fn touch_seen(&mut self) {
        let now = Instant::now();
        if now < self.next_seen {
            return;
        }
        if let Some(s) = &self.session {
            self.gw.mark_seen(s.user);
            self.next_seen = now + SEEN_REFRESH;
        }
    }

    async fn identify(&mut self, d: Value) -> Result<(), Stop> {
        if self.session.is_some() {
            return Err(Stop::Close(
                close::ALREADY_AUTHENTICATED,
                "already authenticated",
            ));
        }
        let id: Identify = serde_json::from_value(d)
            .map_err(|_| Stop::Close(close::DECODE_ERROR, "decode error"))?;
        let user = self
            .authenticate(&id.token)
            .await
            .map_err(|_| Stop::Close(close::AUTHENTICATION_FAILED, "authentication failed"))?;
        // There is no sharding: a bot may name shard [0, 1] (what libraries send by default).
        if user.bot && id.shard.as_deref().is_some_and(|s| s != [0, 1].as_slice()) {
            return Err(Stop::Close(close::INVALID_SHARD, "invalid shard"));
        }
        // User clients send no intents and receive everything.
        let wanted = if user.bot {
            if id.intents & !intents::ALL_KNOWN != 0 {
                return Err(Stop::Close(close::INVALID_INTENTS, "invalid intents"));
            }
            id.intents
        } else {
            intents::ALL_KNOWN
        };
        if user.bot && wanted & intents::PRIVILEGED & !user.allowed_privileged_intents != 0 {
            return Err(Stop::Close(close::DISALLOWED_INTENTS, "disallowed intents"));
        }
        let tx = self
            .session_tx
            .take()
            .ok_or(Stop::Close(close::UNKNOWN_ERROR, "no session channel"))?;
        let session = self.gw.create_session(&user, wanted, self.id, tx).await;
        self.session = Some(session);
        self.touch_seen();
        if let Some(p) = id.presence {
            self.gw.set_presence(user.user_id, p).await;
        }
        Ok(())
    }

    async fn resume(&mut self, d: Value, session_rx: &mut mpsc::Receiver<Out>) -> Result<(), Stop> {
        if self.session.is_some() {
            return Err(Stop::Close(
                close::ALREADY_AUTHENTICATED,
                "already authenticated",
            ));
        }
        let r: Resume = serde_json::from_value(d)
            .map_err(|_| Stop::Close(close::DECODE_ERROR, "decode error"))?;
        let user = self
            .authenticate(&r.token)
            .await
            .map_err(|_| Stop::Close(close::AUTHENTICATION_FAILED, "authentication failed"))?;
        let found = self.gw.find_resumable(&r.session_id, user.user_id);
        // A fresh channel: frames replayed now must not mix with leftovers.
        let (tx, rx) = mpsc::channel::<Out>(self.cap);
        match found {
            Some(s) if s.resume(self.id, tx.clone(), r.seq) => {
                *session_rx = rx;
                self.session_tx = None;
                s.deliver_marker("RESUMED");
                self.gw.refresh_presence(s.user, false).await;
                self.session = Some(s);
                self.touch_seen();
                Ok(())
            }
            _ => self.send_payload(op::INVALID_SESSION, json!(false)).await,
        }
    }
}
