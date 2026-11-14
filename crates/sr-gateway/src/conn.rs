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
use crate::hub::{Gateway, UnidentifiedSlot};
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
    /// The client's address (behind a trusted proxy, the forwarded one), for the per-address cap on
    /// sockets that have not identified yet (SEC-AUD-010). `None`: not capped per address.
    pub peer: Option<std::net::IpAddr>,
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
    /// Last frame of any kind from the client, and the longest a write to it blocked: logged when the
    /// server closes the connection, to tell a silent client from a stalled write path (RDY-VOX-002).
    last_in: Instant,
    send_blocked_max: std::time::Duration,
    /// The token IDENTIFY/RESUME authenticated with (empty: the `sr_session` cookie), re-checked every
    /// [`crate::GatewayConfig::session_recheck`] and on a `SessionsRevoked` event (SEC-AUD-002).
    credential: Option<String>,
    recheck_at: Instant,
    /// Until IDENTIFY/RESUME succeeds: the socket's place in the unidentified count, and the time
    /// it must have identified by (heartbeats do not extend it; SEC-AUD-010).
    unidentified: Option<UnidentifiedSlot>,
    identify_by: Instant,
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
/// `outgoing` is what the server writes back. Over the per-address cap on sockets that have not
/// identified yet, the connection is closed at once with 4008.
pub async fn run_connection(
    gw: Gateway,
    params: ConnectParams,
    incoming: mpsc::Receiver<Frame>,
    outgoing: mpsc::Sender<Frame>,
) {
    let Some(slot) = gw.admit(params.peer) else {
        let _ = outgoing
            .send(Frame::Close {
                code: close::RATE_LIMITED,
                reason: "too many connections".into(),
            })
            .await;
        return;
    };
    run_admitted(gw, params, slot, incoming, outgoing).await;
}

/// [`run_connection`] for a socket already counted by [`Gateway::admit`].
pub(crate) async fn run_admitted(
    gw: Gateway,
    params: ConnectParams,
    slot: UnidentifiedSlot,
    mut incoming: mpsc::Receiver<Frame>,
    outgoing: mpsc::Sender<Frame>,
) {
    let id = gw.next_conn_id();
    let cap = gw.config().resume_buffer.max(1024) + 64;
    let (session_tx, mut session_rx) = mpsc::channel::<Out>(cap);
    let deadline = Instant::now() + gw.config().heartbeat_interval + gw.config().heartbeat_grace;
    let identify_by = Instant::now() + gw.config().identify_timeout;
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
        last_in: Instant::now(),
        send_blocked_max: std::time::Duration::ZERO,
        credential: None,
        recheck_at: Instant::now(),
        unidentified: Some(slot),
        identify_by,
    };
    let stop = c.run(&mut incoming, &mut session_rx).await;
    // Which side ended a gateway connection, and why (RDY-VOX-002: run 40237146331 lost five members'
    // gateways at once and nothing said why). A detached session keeps its voice state for the grace period.
    let user = c.session.as_ref().map(|s| s.user.to_string());
    match &stop {
        Stop::Close(code, reason) => {
            tracing::info!(
                conn = id,
                ?user,
                code,
                reason,
                silent_ms = u64::try_from(c.last_in.elapsed().as_millis()).unwrap_or(u64::MAX),
                send_blocked_max_ms =
                    u64::try_from(c.send_blocked_max.as_millis()).unwrap_or(u64::MAX),
                "gateway connection closed by the server"
            );
        }
        Stop::Gone => tracing::debug!(conn = id, ?user, "gateway connection closed by the client"),
    }
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
                () = sleep_until(self.recheck_at), if self.session.is_some() => self.recheck().await,
                () = sleep_until(self.identify_by), if self.session.is_none() => {
                    Err(Stop::Close(close::SESSION_TIMED_OUT, "identify timed out"))
                }
                m = session_rx.recv(), if self.session.is_some() => match m {
                    Some(Out::Frame(t)) => {
                        self.record_dispatch_bytes(t.len());
                        let started = Instant::now();
                        let r = self.send_text(t.to_string()).await;
                        self.send_blocked_max = self.send_blocked_max.max(started.elapsed());
                        r
                    }
                    Some(Out::Reconnect) => {
                        let _ = self.send_text(Payload::new(op::RECONNECT, Value::Null).to_text()).await;
                        Err(Stop::Close(close::UNKNOWN_ERROR, "reconnect"))
                    }
                    Some(Out::Recheck) => self.recheck().await,
                    // Taken over by a newer connection, or fell too far behind.
                    None => Err(Stop::Close(close::UNKNOWN_ERROR, "session detached")),
                },
                f = incoming.recv() => {
                    self.last_in = Instant::now();
                    match f {
                        None | Some(Frame::Close { .. }) => Err(Stop::Gone),
                        Some(Frame::Binary(_)) => Err(Stop::Close(close::DECODE_ERROR, "binary frames unsupported")),
                        Some(Frame::Text(_)) if !self.count_op() => {
                            Err(Stop::Close(close::RATE_LIMITED, "rate limited"))
                        }
                        Some(Frame::Text(t)) => self.on_text(&t, session_rx).await,
                    }
                }
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

    /// Re-check the credential this connection identified with (SEC-AUD-002). A revoked or expired
    /// session, a suspended account or a reset bot token closes the socket with 4004, which the web
    /// client treats as signed out. A backend outage keeps the socket (the next check retries).
    async fn recheck(&mut self) -> Result<(), Stop> {
        self.recheck_at = Instant::now() + self.gw.config().session_recheck;
        let (Some(s), Some(token)) = (self.session.as_ref(), self.credential.as_deref()) else {
            return Ok(());
        };
        match self.authenticate(token).await {
            Ok(u) if u.user_id == s.user => {
                s.set_user_json(u.user);
                Ok(())
            }
            Ok(_) | Err(AuthError::Invalid) => {
                Err(Stop::Close(close::AUTHENTICATION_FAILED, "session revoked"))
            }
            Err(AuthError::Unavailable(e)) => {
                tracing::warn!(conn = self.id, error = %e, "gateway session re-check unavailable");
                Ok(())
            }
        }
    }

    /// Mark the credential that authenticated this connection, for [`Self::recheck`].
    fn authenticated_with(&mut self, token: &str) {
        self.unidentified = None;
        self.credential = Some(token.to_owned());
        self.recheck_at = Instant::now() + self.gw.config().session_recheck;
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
                if let Some(s) = self.session.clone() {
                    self.gw.set_session_presence(&s, upd).await;
                }
                Ok(())
            }
            op::VOICE_STATE_UPDATE => {
                if let Some(s) = self.session.clone() {
                    self.gw.voice_state_update(&s, p.d).await;
                }
                Ok(())
            }
            op::LAZY_REQUEST => {
                if let Some(s) = self.session.clone()
                    && s.intents & intents::GUILD_MEMBERS != 0
                {
                    self.gw.lazy_request(&s, &p.d).await;
                }
                Ok(())
            }
            op::REQUEST_GUILD_MEMBERS => {
                if let Some(s) = self.session.clone()
                    && let Err(code) = self.gw.request_guild_members(&s, &p.d).await
                {
                    let reason = if code == close::DECODE_ERROR {
                        "bad member request"
                    } else {
                        "disallowed intents"
                    };
                    return Err(Stop::Close(code, reason));
                }
                Ok(())
            }
            op::E2EE_MLS => {
                if let Some(s) = self.session.clone() {
                    self.gw.e2ee_mls(&s, p.d).await;
                }
                Ok(())
            }
            op::REQUEST_SOUNDBOARD_SOUNDS => {
                if let Some(s) = self.session.clone() {
                    self.gw.soundboard_request(&s, p.d).await;
                }
                Ok(())
            }
            op::STREAM_CREATE | op::STREAM_DELETE | op::STREAM_WATCH | op::STREAM_SET_PAUSED => {
                if let Some(s) = self.session.clone() {
                    self.gw.stream_op(&s, p.op, p.d).await;
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
        // A bot may name `[shard_id, shard_count]`; user clients never shard.
        let shard = match (user.bot, id.shard.as_deref()) {
            (true, Some(&[i, n])) if (1..=crate::protocol::MAX_SHARDS).contains(&n) && i < n => {
                Some((i, n))
            }
            (true, Some(_)) => {
                return Err(Stop::Close(close::INVALID_SHARD, "invalid shard"));
            }
            (_, _) => None,
        };
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
        let session = self
            .gw
            .create_session(&user, wanted, self.id, tx, shard)
            .await;
        self.gw
            .set_session_platform(&session, crate::presence::platform_of(&id.properties));
        self.session = Some(session.clone());
        self.authenticated_with(&id.token);
        self.touch_seen();
        if let Some(p) = id.presence {
            self.gw.set_session_presence(&session, p).await;
        } else {
            // `create_session` announced the user before the platform was known (a new session counts as `web`);
            // announce again when the platform changes what others see (D-702: the web app sends no presence at IDENTIFY).
            self.gw.refresh_presence(session.user, false).await;
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
                s.set_user_json(user.user.clone());
                *session_rx = rx;
                self.session_tx = None;
                s.deliver_marker("RESUMED");
                self.gw.refresh_presence(s.user, false).await;
                self.session = Some(s);
                self.authenticated_with(&r.token);
                self.touch_seen();
                Ok(())
            }
            _ => self.send_payload(op::INVALID_SESSION, json!(false)).await,
        }
    }
}
