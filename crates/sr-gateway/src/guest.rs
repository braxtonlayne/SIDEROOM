//! The guest socket (ARC-GST-001, roadmap P28 wave 43, D-385, D-397, D-399): `GET /gateway/guest?v=1`.
//!
//! A person who opened a `/play/<lobby_token>` link holds a guest session, not an account, and this socket is the
//! only thing that accepts it. The isolation rule is **enforced here, at the relay**, not in the SDK a game runs
//! on: a guest socket holds exactly one topic, `game:<lobby_token>`, and any frame that names another topic, uses
//! an op this module does not list, or is not JSON ends the connection before anything reaches the lobby. This
//! module knows nothing of guilds, channels, accounts or the call: it is not given a [`crate::Gateway`] and
//! cannot subscribe to one.
//!
//! # Frames (JSON text, one object each)
//!
//! ```text
//! client -> relay
//!   {"op":"hello","guest_token":"srg_..."}            a guest (the token from POST /play/{lobby_token}/join)
//!   {"op":"hello","lobby_token":"<64 hex>"}           a signed-in member of the Activity, with the sr_session
//!                                                     cookie and an allowed Origin (a member's seat)
//!   {"op":"subscribe","topic":"game:<lobby_token>"}   only the one topic; acknowledged, nothing more
//!   {"op":"publish","topic":"game:<lobby_token>","data":"<base64>"}   opaque bytes for the other seats
//!   {"op":"ping"}
//! relay -> client
//!   {"op":"ready","seat":n,"role":"guest|member","callsign":"..","key_id":"<32 hex>","epoch":0,
//!    "scope":{"data":true,"media":false},"peers":[{"seat":n,"role":"..","callsign":".."}]}
//!   {"op":"subscribed","topic":"game:.."}   {"op":"pong"}
//!   {"op":"data","from":n,"data":"<base64>"}   (the seat holds one topic, so frames carry none)
//!   {"op":"peer","event":"join","peer":{..}}   {"op":"peer","event":"leave","seat":n}
//!   {"op":"closed","reason":"revoked|ended"}
//! ```
//!
//! The first frame must be `hello`, within [`GuestConfig::hello_timeout`]. The topic is implied by the credential
//! (the lobby it belongs to); `subscribe` and `publish` must repeat that very topic. The relay holds only the
//! SHA-256 of the lobby token, so it compares the digest of the topic's token to the digest of the seat's lobby.
//!
//! # Close codes
//!
//! * [`CLOSE_BAD_FRAME`] 4400: not JSON, or a frame without an `op`.
//! * [`CLOSE_UNAUTHORIZED`] 4401: no or a bad credential, or no hello in time.
//! * [`CLOSE_FORBIDDEN`] 4403: a second topic, an unknown op, a binary frame, a frame before hello: the "403/Drop".
//! * [`CLOSE_FULL`] 4409: the lobby has no free seat.
//! * [`CLOSE_GONE`] 4410: the link was revoked, expired, or the Activity ended (also announced as `closed`).
//! * [`CLOSE_TOO_LARGE`] 4413: a frame over [`MAX_FRAME`].
//!
//! The credential is checked again every [`GuestConfig::recheck`], so a revoked link, an expired session or an
//! ended Activity disconnects the seat within that time.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use axum::Router;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::header::{COOKIE, ORIGIN};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sr_core::UserId;
use sr_voice::lobby::{CloseReason, Event, LobbyHub, LobbyId, PeerInfo, Role};
use tokio::sync::mpsc;

use crate::conn::{ConnectParams, Frame};
use crate::traits::Authenticator;
use crate::ws::SESSION_COOKIE;

pub const CLOSE_BAD_FRAME: u16 = 4400;
pub const CLOSE_UNAUTHORIZED: u16 = 4401;
pub const CLOSE_FORBIDDEN: u16 = 4403;
pub const CLOSE_FULL: u16 = 4409;
pub const CLOSE_GONE: u16 = 4410;
pub const CLOSE_TOO_LARGE: u16 = 4413;

/// The largest text frame accepted: a full lobby payload as base64, plus the envelope.
pub const MAX_FRAME: usize = 32 * 1024;
/// The topic prefix; the same as `sr_activities::guest::TOPIC_PREFIX`.
pub const TOPIC_PREFIX: &str = "game:";

/// Where a credential seats someone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seating {
    /// SHA-256 of the lobby token.
    pub lobby: [u8; 32],
    /// A signed-in member of the Activity (`true`) or a guest (`false`).
    pub member: bool,
    /// What the lobby shows for the seat.
    pub callsign: String,
}

/// Decides who a credential is. Implemented over the database in `sr-server`; every answer is "works now".
#[async_trait]
pub trait GuestAuthority: Send + Sync {
    /// The seat a guest token holds, if its session and link both work now.
    async fn guest_seating(&self, guest_token: &str) -> Result<Option<Seating>, String>;

    /// The seat `user` holds in the lobby `lobby_token` names, if they are a current participant of its Activity
    /// and the link works now.
    async fn member_seating(
        &self,
        user: UserId,
        lobby_token: &str,
    ) -> Result<Option<Seating>, String>;
}

/// Tunables of the guest socket.
#[derive(Debug, Clone)]
pub struct GuestConfig {
    /// How long a socket may take to send `hello`.
    pub hello_timeout: Duration,
    /// How often the credential is checked again.
    pub recheck: Duration,
    /// `Origin` values allowed to seat a member with the `sr_session` cookie. Empty refuses member seats.
    pub allowed_origins: Vec<String>,
    /// Sockets open at once (all lobbies).
    pub max_connections: usize,
}

impl Default for GuestConfig {
    fn default() -> Self {
        Self {
            hello_timeout: Duration::from_secs(10),
            recheck: Duration::from_secs(5),
            allowed_origins: Vec::new(),
            max_connections: 2000,
        }
    }
}

struct Inner {
    auth: Arc<dyn Authenticator>,
    authority: Arc<dyn GuestAuthority>,
    hub: LobbyHub,
    cfg: GuestConfig,
    open: AtomicUsize,
}

/// The guest socket's state: the lobby relay and the two seams. Cheap to clone.
#[derive(Clone)]
pub struct GuestGateway {
    inner: Arc<Inner>,
}

impl GuestGateway {
    pub fn new(
        auth: Arc<dyn Authenticator>,
        authority: Arc<dyn GuestAuthority>,
        hub: LobbyHub,
        cfg: GuestConfig,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                auth,
                authority,
                hub,
                cfg,
                open: AtomicUsize::new(0),
            }),
        }
    }

    /// The lobby relay (for tests and for closing a lobby from the process).
    pub fn hub(&self) -> &LobbyHub {
        &self.inner.hub
    }

    /// Sockets open now.
    pub fn open_sockets(&self) -> usize {
        self.inner.open.load(Ordering::Relaxed)
    }
}

/// The digest of a topic's lobby token, if `topic` is `game:<64 lowercase hex digits>`.
fn topic_lobby(topic: &str) -> Option<[u8; 32]> {
    let t = topic.strip_prefix(TOPIC_PREFIX)?;
    if t.len() != 64 || !t.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return None;
    }
    Some(Sha256::digest(t.as_bytes()).into())
}

#[derive(Debug, Deserialize)]
pub struct GuestQuery {
    #[serde(default)]
    pub v: Option<u32>,
}

/// The route: `GET /gateway/guest?v=1`.
pub fn routes() -> Router<GuestGateway> {
    Router::new().route("/gateway/guest", get(guest_handler))
}

pub async fn guest_handler(
    State(gg): State<GuestGateway>,
    Query(q): Query<GuestQuery>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    if q.v != Some(1) {
        return (StatusCode::BAD_REQUEST, "unsupported guest socket version").into_response();
    }
    if gg.open_sockets() >= gg.inner.cfg.max_connections {
        return (StatusCode::SERVICE_UNAVAILABLE, "too many guest sockets").into_response();
    }
    let params = ConnectParams {
        compression: crate::compress::Compression::default(),
        session_cookie: headers
            .get_all(COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|s| s.split(';'))
            .find_map(|kv| {
                let (n, v) = kv.trim().split_once('=')?;
                (n == SESSION_COOKIE && !v.is_empty()).then(|| v.to_owned())
            }),
        origin: headers
            .get(ORIGIN)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned),
    };
    ws.max_message_size(MAX_FRAME)
        .on_upgrade(move |socket| serve_socket(gg, params, socket))
}

async fn serve_socket(gg: GuestGateway, params: ConnectParams, socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();
    let (in_tx, in_rx) = mpsc::channel::<Frame>(64);
    let (out_tx, mut out_rx) = mpsc::channel::<Frame>(64);
    let reader = tokio::spawn(async move {
        while let Some(Ok(msg)) = stream.next().await {
            let f = match msg {
                Message::Text(t) => Frame::Text(t.as_str().to_owned()),
                Message::Binary(b) => Frame::Binary(b.to_vec()),
                Message::Close(_) => break,
                Message::Ping(_) | Message::Pong(_) => continue,
            };
            if in_tx.send(f).await.is_err() {
                break;
            }
        }
    });
    let writer = tokio::spawn(async move {
        while let Some(f) = out_rx.recv().await {
            let (msg, last) = match f {
                Frame::Text(t) => (Message::Text(t.into()), false),
                Frame::Binary(b) => (Message::Binary(b.into()), false),
                Frame::Close { code, reason } => (
                    Message::Close(Some(CloseFrame {
                        code,
                        reason: reason.into(),
                    })),
                    true,
                ),
            };
            if sink.send(msg).await.is_err() || last {
                break;
            }
        }
        let _ = sink.close().await;
    });
    run_guest(gg, params, in_rx, out_tx).await;
    let _ = writer.await;
    reader.abort();
}

/// Who holds the socket, for the periodic check.
enum Holder {
    Guest(String),
    Member { user: UserId, lobby_token: String },
}

fn peer_json(p: &PeerInfo) -> Value {
    json!({"seat": p.seat, "role": role_name(p.role), "callsign": p.callsign})
}

fn role_name(r: Role) -> &'static str {
    match r {
        Role::Member => "member",
        Role::Guest => "guest",
    }
}

async fn close(tx: &mpsc::Sender<Frame>, code: u16, reason: &str) {
    let _ = tx
        .send(Frame::Close {
            code,
            reason: reason.to_owned(),
        })
        .await;
}

async fn say(tx: &mpsc::Sender<Frame>, v: Value) -> bool {
    tx.send(Frame::Text(v.to_string())).await.is_ok()
}

/// The transport-agnostic state machine of one guest socket. Returns when the socket is over.
pub async fn run_guest(
    gg: GuestGateway,
    params: ConnectParams,
    mut rx: mpsc::Receiver<Frame>,
    tx: mpsc::Sender<Frame>,
) {
    gg.inner.open.fetch_add(1, Ordering::Relaxed);
    run_guest_inner(&gg, params, &mut rx, &tx).await;
    gg.inner.open.fetch_sub(1, Ordering::Relaxed);
}

async fn run_guest_inner(
    gg: &GuestGateway,
    params: ConnectParams,
    rx: &mut mpsc::Receiver<Frame>,
    tx: &mpsc::Sender<Frame>,
) {
    let cfg = &gg.inner.cfg;
    // 1. hello, in time.
    let first = match tokio::time::timeout(cfg.hello_timeout, rx.recv()).await {
        Ok(Some(f)) => f,
        Ok(None) => return,
        Err(_) => return close(tx, CLOSE_UNAUTHORIZED, "hello timeout").await,
    };
    let Frame::Text(text) = first else {
        return close(tx, CLOSE_FORBIDDEN, "text frames only").await;
    };
    if text.len() > MAX_FRAME {
        return close(tx, CLOSE_TOO_LARGE, "frame too large").await;
    }
    let Ok(v) = serde_json::from_str::<Value>(&text) else {
        return close(tx, CLOSE_BAD_FRAME, "not json").await;
    };
    if v.get("op").and_then(Value::as_str) != Some("hello") {
        return close(tx, CLOSE_FORBIDDEN, "hello first").await;
    }
    // 2. the credential.
    let (holder, seating) = match resolve(gg, &params, &v).await {
        Ok(Some(x)) => x,
        Ok(None) => return close(tx, CLOSE_UNAUTHORIZED, "unauthorized").await,
        Err(()) => return close(tx, CLOSE_UNAUTHORIZED, "unavailable").await,
    };
    let lobby = LobbyId(seating.lobby);
    let role = if seating.member {
        Role::Member
    } else {
        Role::Guest
    };
    // 3. the seat. Taking it opens the lobby (with its own keys) when this is its first seat.
    let mut att = match gg.inner.hub.attach(lobby, role, &seating.callsign) {
        Ok(a) => a,
        Err(_) => return close(tx, CLOSE_FULL, "lobby full").await,
    };
    let seat = att.seat;
    let ready = json!({
        "op": "ready", "seat": seat, "role": role_name(role), "callsign": seating.callsign,
        "key_id": att.keys.key_id_hex(), "epoch": att.keys.epoch,
        "scope": {"data": att.scope.data, "media": false},
        "peers": att.peers.iter().map(peer_json).collect::<Vec<_>>(),
    });
    if !say(tx, ready).await {
        gg.inner.hub.detach(lobby, seat);
        return;
    }
    // 4. the loop.
    let mut recheck = tokio::time::interval(cfg.recheck);
    recheck.tick().await;
    let end: Option<(u16, &'static str)> = loop {
        tokio::select! {
            inbound = rx.recv() => {
                let Some(f) = inbound else { break None };
                match handle_frame(gg, lobby, seat, f, tx).await {
                    Ok(()) => {}
                    Err((code, why)) => break Some((code, why)),
                }
            }
            ev = att.events.recv() => {
                match ev {
                    Some(Event::Data { from, payload }) => {
                        let frame = json!({
                            "op": "data", "from": from, "data": B64.encode(&*payload),
                        });
                        if !say(tx, frame).await { break None }
                    }
                    Some(Event::Joined(p)) => {
                        if !say(tx, json!({"op": "peer", "event": "join", "peer": peer_json(&p)})).await { break None }
                    }
                    Some(Event::Left(s)) => {
                        if !say(tx, json!({"op": "peer", "event": "leave", "seat": s})).await { break None }
                    }
                    Some(Event::Closed(r)) => {
                        let reason = match r { CloseReason::Revoked => "revoked", CloseReason::Ended => "ended" };
                        let _ = say(tx, json!({"op": "closed", "reason": reason})).await;
                        break Some((CLOSE_GONE, "gone"));
                    }
                    None => break Some((CLOSE_GONE, "gone")),
                }
            }
            _ = recheck.tick() => {
                if still_valid(gg, &holder, lobby).await == Some(false) {
                    let _ = say(tx, json!({"op": "closed", "reason": "revoked"})).await;
                    break Some((CLOSE_GONE, "gone"));
                }
            }
        }
    };
    gg.inner.hub.detach(lobby, seat);
    if let Some((code, why)) = end {
        close(tx, code, why).await;
    }
}

/// Check the credential in a `hello` and say where it seats.
async fn resolve(
    gg: &GuestGateway,
    params: &ConnectParams,
    hello: &Value,
) -> Result<Option<(Holder, Seating)>, ()> {
    let inner = &gg.inner;
    if let Some(token) = hello.get("guest_token").and_then(Value::as_str) {
        let s = inner.authority.guest_seating(token).await.map_err(|e| {
            tracing::warn!(error = %e, "guest seating lookup failed");
        })?;
        return Ok(s.map(|s| (Holder::Guest(token.to_owned()), s)));
    }
    let Some(lobby_token) = hello.get("lobby_token").and_then(Value::as_str) else {
        return Ok(None);
    };
    // A member's seat rides the browser session, so it needs the same origin check as the main gateway.
    let origin_ok = params
        .origin
        .as_deref()
        .is_some_and(|o| inner.cfg.allowed_origins.iter().any(|a| a == o));
    let (true, Some(cookie)) = (origin_ok, params.session_cookie.as_deref()) else {
        return Ok(None);
    };
    let Ok(user) = inner.auth.authenticate_session_cookie(cookie).await else {
        return Ok(None);
    };
    if user.bot {
        return Ok(None);
    }
    let s = inner
        .authority
        .member_seating(user.user_id, lobby_token)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "member seating lookup failed");
        })?;
    Ok(s.map(|s| {
        (
            Holder::Member {
                user: user.user_id,
                lobby_token: lobby_token.to_owned(),
            },
            s,
        )
    }))
}

/// `Some(false)` when the credential no longer works or now names another lobby; `None` when the lookup failed
/// (keep the seat; the next check tries again).
async fn still_valid(gg: &GuestGateway, holder: &Holder, lobby: LobbyId) -> Option<bool> {
    let s = match holder {
        Holder::Guest(t) => gg.inner.authority.guest_seating(t).await,
        Holder::Member { user, lobby_token } => {
            gg.inner.authority.member_seating(*user, lobby_token).await
        }
    };
    match s {
        Ok(s) => Some(s.is_some_and(|s| s.lobby == lobby.0)),
        Err(_) => None,
    }
}

/// One frame after `ready`. `Err` ends the socket with a close code: the relay's "403/Drop".
async fn handle_frame(
    gg: &GuestGateway,
    lobby: LobbyId,
    seat: u32,
    f: Frame,
    tx: &mpsc::Sender<Frame>,
) -> Result<(), (u16, &'static str)> {
    let text = match f {
        Frame::Text(t) => t,
        _ => return Err((CLOSE_FORBIDDEN, "text frames only")),
    };
    if text.len() > MAX_FRAME {
        return Err((CLOSE_TOO_LARGE, "frame too large"));
    }
    let v: Value = serde_json::from_str(&text).map_err(|_| (CLOSE_BAD_FRAME, "not json"))?;
    let op = v
        .get("op")
        .and_then(Value::as_str)
        .ok_or((CLOSE_BAD_FRAME, "no op"))?;
    // The one topic this seat holds. Any other, or a topic that is not a lobby topic at all, is dropped.
    let on_topic = |v: &Value| -> Result<String, (u16, &'static str)> {
        let topic = v
            .get("topic")
            .and_then(Value::as_str)
            .ok_or((CLOSE_FORBIDDEN, "topic required"))?;
        if topic_lobby(topic) == Some(lobby.0) {
            Ok(topic.to_owned())
        } else {
            Err((CLOSE_FORBIDDEN, "forbidden topic"))
        }
    };
    match op {
        "ping" => {
            say(tx, json!({"op": "pong"})).await;
        }
        "subscribe" => {
            let topic = on_topic(&v)?;
            say(tx, json!({"op": "subscribed", "topic": topic})).await;
        }
        "publish" => {
            on_topic(&v)?;
            let data = v
                .get("data")
                .and_then(Value::as_str)
                .ok_or((CLOSE_BAD_FRAME, "data required"))?;
            let bytes = B64
                .decode(data)
                .map_err(|_| (CLOSE_BAD_FRAME, "data is not base64"))?;
            // Too large and over-budget frames are dropped (the game's own state sync recovers); the seat stays.
            let _ = gg.inner.hub.send(lobby, seat, &bytes, Instant::now());
        }
        _ => return Err((CLOSE_FORBIDDEN, "unknown op")),
    }
    Ok(())
}
