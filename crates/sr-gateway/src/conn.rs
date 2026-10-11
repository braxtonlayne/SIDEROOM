//! Per-connection state machine, independent of the WebSocket library.
//!
//! `HELLO -> (IDENTIFY | RESUME) -> READY/RESUMED -> heartbeat/dispatch loop`.

use std::sync::Arc;

use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep_until};

use crate::compress::{Compression, zstd_compress};
use crate::hub::Gateway;
use crate::protocol::{Identify, Payload, PresenceUpdate, Resume, close, intents, op};
use crate::session::{Out, Session};

/// Transport-level frame. The axum adapter maps WebSocket messages to these;
/// tests drive them directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    Text(String),
    Binary(Vec<u8>),
    Close { code: u16, reason: String },
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ConnectParams {
    pub compression: Compression,
}

struct Conn {
    gw: Gateway,
    out: mpsc::Sender<Frame>,
    params: ConnectParams,
    id: u64,
    session: Option<Arc<Session>>,
    session_tx: Option<mpsc::Sender<Out>>,
    cap: usize,
    deadline: Instant,
}

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
    if let Some(s) = &c.session {
        s.detach(c.id);
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
                    Some(Out::Frame(t)) => self.send_text(t.to_string()).await,
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
                    Some(Frame::Text(t)) => self.on_text(&t, session_rx).await,
                },
            };
            if let Err(s) = step {
                return s;
            }
        }
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
            // Voice, member requests, SR ops: owned by other crates; accepted.
            _ => Ok(()),
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
            .gw
            .inner
            .auth
            .authenticate(&id.token)
            .await
            .map_err(|_| Stop::Close(close::AUTHENTICATION_FAILED, "authentication failed"))?;
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
            .gw
            .inner
            .auth
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
                self.session = Some(s);
                Ok(())
            }
            _ => self.send_payload(op::INVALID_SESSION, json!(false)).await,
        }
    }
}
