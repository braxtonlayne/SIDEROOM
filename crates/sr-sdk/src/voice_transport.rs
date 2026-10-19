//! Bots in voice, the media transport (DEV-BOT-007, P15-DEV-5). Behind the `voice-transport` feature.
//!
//! [`crate::voice`] is the key-agreement half. This is the other half: joining the SFU that a gateway
//! `VOICE_SERVER_UPDATE` names and moving Opus frames, on the `str0m` WebRTC stack the SFU itself uses.
//!
//! The flow, with the gateway side done by the caller through [`crate::GatewayHandle`] / the shard:
//!
//! 1. send gateway op 4 ([`voice_state_body`], or `E2eeCall::join_state` of [`crate::voice`] which also declares
//!    `sr_e2ee: true`);
//! 2. the gateway answers with a `VOICE_SERVER_UPDATE` dispatch: parse it with [`VoiceServerUpdate::from_dispatch`];
//! 3. [`VoiceSession::connect`] opens the SFU WebSocket (IDENTIFY with the join token), offers one send-only
//!    audio line and answers the SFU's offers for everyone else's audio;
//! 4. [`VoiceSession::pump`] drives the connection and returns [`VoiceEvent`]s (incoming Opus frames by
//!    user, joins, leaves, speaking); [`VoiceSession::send_opus`] publishes one 20 ms frame.
//!
//! End-to-end encryption: when the dispatch says `sr_e2ee: true` the session refuses to send a frame unless a
//! [`FrameTransform`] is installed (with `voice-e2ee`, [`crate::voice::E2eeMedia`] is one), so audio is never
//! sent in the clear into an encrypted call, and incoming frames are dropped rather than played when they
//! cannot be decrypted. Encoded frames are encrypted before RTP packetisation exactly as in the browser
//! (the SFU only ever sees ciphertext).
//!
//! The session is one task's worth of state, not a thread: call `pump` in a loop. It does not reconnect;
//! on [`VoiceEvent::Closed`] ask the gateway for a new `VOICE_SERVER_UPDATE` and build a new session.

use std::collections::VecDeque;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use str0m::change::{SdpAnswer, SdpOffer, SdpPendingOffer};
use str0m::format::Codec;
use str0m::media::{Direction, MediaKind, Mid};
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, Input, Output, Rtc};
use tokio::net::{TcpStream, UdpSocket};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

/// SFU WebSocket opcodes (docs/specs/lounge-voice-video.md section 4).
mod op {
    pub const IDENTIFY: u8 = 0;
    pub const READY: u8 = 1;
    pub const HEARTBEAT: u8 = 2;
    pub const SDP_OFFER: u8 = 4;
    pub const SDP_ANSWER: u8 = 5;
    pub const SPEAKING: u8 = 7;
    pub const CLIENT_CONNECT: u8 = 10;
    pub const CLIENT_DISCONNECT: u8 = 11;
}

/// What went wrong in the transport.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("the voice endpoint is not a ws:// or wss:// URL")]
    BadEndpoint,
    #[error("voice websocket: {0}")]
    Socket(String),
    #[error("the SFU refused the join (close code {0})")]
    Refused(u16),
    #[error("the SFU did not answer READY in time")]
    NoReady,
    #[error("webrtc: {0}")]
    Rtc(String),
    #[error("the connection is not up yet")]
    NotConnected,
    #[error("this call is end-to-end encrypted: install a frame transform before sending audio")]
    EncryptionRequired,
    #[error("frame transform: {0}")]
    Transform(String),
}

/// Encrypts the bot's own frames and decrypts everyone else's.
pub trait FrameTransform: Send {
    fn encrypt(&mut self, frame: &[u8]) -> Result<Vec<u8>, String>;
    /// `sender` is the user id of the frame's author.
    fn decrypt(&mut self, sender: u64, frame: &[u8]) -> Result<Vec<u8>, String>;
}

#[cfg(feature = "voice-e2ee")]
impl FrameTransform for crate::voice::E2eeMedia {
    fn encrypt(&mut self, frame: &[u8]) -> Result<Vec<u8>, String> {
        crate::voice::E2eeMedia::encrypt(self, sr_e2ee::Codec::Opus, frame)
            .map_err(|e| e.to_string())
    }

    fn decrypt(&mut self, sender: u64, frame: &[u8]) -> Result<Vec<u8>, String> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        crate::voice::E2eeMedia::decrypt(self, sender, sr_e2ee::Codec::Opus, frame, now_ms)
            .map_err(|e| e.to_string())
    }
}

/// The gateway op 4 `d` body that joins (or, with `channel_id: None`, leaves) a voice channel. Declares
/// `sr_e2ee: true`, which is what a call that is end-to-end encrypted expects of every member.
pub fn voice_state_body(
    guild_id: Option<&str>,
    channel_id: Option<&str>,
    self_mute: bool,
    self_deaf: bool,
    e2ee: bool,
) -> Value {
    let mut d = json!({
        "channel_id": channel_id,
        "self_mute": self_mute,
        "self_deaf": self_deaf,
        "sr_e2ee": e2ee,
    });
    if let Some(g) = guild_id {
        d["guild_id"] = json!(g);
    }
    d
}

/// The `VOICE_SERVER_UPDATE` dispatch: where the SFU is and the short-lived token that admits this session.
#[derive(Clone, PartialEq, Eq)]
pub struct VoiceServerUpdate {
    pub token: String,
    /// WebSocket URL of the SFU node.
    pub endpoint: String,
    pub guild_id: Option<String>,
    pub channel_id: String,
    /// The call is end-to-end encrypted (`sr_e2ee`). Absent counts as encrypted: voice E2EE is on by default
    /// (D-249, D-263), and a bot must not fall back to plaintext because a field is missing.
    pub e2ee: bool,
}

impl std::fmt::Debug for VoiceServerUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VoiceServerUpdate")
            .field("endpoint", &self.endpoint)
            .field("channel_id", &self.channel_id)
            .field("e2ee", &self.e2ee)
            .finish_non_exhaustive()
    }
}

impl VoiceServerUpdate {
    /// Parse the dispatch's `d`.
    pub fn from_dispatch(d: &Value) -> Option<Self> {
        let s = |k: &str| d.get(k).and_then(Value::as_str).map(str::to_owned);
        Some(Self {
            token: s("token")?,
            endpoint: s("endpoint")?,
            guild_id: s("guild_id"),
            channel_id: s("channel_id")?,
            e2ee: d.get("sr_e2ee").and_then(Value::as_bool).unwrap_or(true),
        })
    }
}

/// What a session reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceEvent {
    /// ICE and DTLS are up; audio can be sent.
    Connected,
    /// One decoded-from-RTP Opus frame (already decrypted when a transform is installed).
    Audio {
        user: u64,
        frame: Vec<u8>,
    },
    /// A frame that could not be decrypted was dropped.
    Undecryptable {
        user: u64,
    },
    Joined(u64),
    Left(u64),
    /// `speaking` is the bitmask of the voice protocol (1 microphone); 0 is silence.
    Speaking {
        user: u64,
        speaking: u8,
    },
    /// The SFU closed the connection (code 0: dropped without a code). Do not keep pumping.
    Closed(u16),
}

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

fn user_of(v: &Value) -> Option<u64> {
    v.get("user_id")?.as_str()?.parse().ok()
}

/// One bot's connection to one voice channel.
pub struct VoiceSession {
    ws: Ws,
    rtc: Rtc,
    socket: UdpSocket,
    local: SocketAddr,
    mic: Mid,
    pending: Option<SdpPendingOffer>,
    start: Instant,
    heartbeat_every: Duration,
    next_heartbeat: Instant,
    beats: u64,
    reported_up: bool,
    closed: Option<u16>,
    e2ee: bool,
    transform: Option<Box<dyn FrameTransform>>,
    out: Vec<Value>,
    events: VecDeque<VoiceEvent>,
    /// Users who were in the channel when we joined (from READY).
    pub initial_participants: Vec<u64>,
}

impl std::fmt::Debug for VoiceSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VoiceSession")
            .field("local", &self.local)
            .field("connected", &self.rtc.is_connected())
            .field("e2ee", &self.e2ee)
            .finish_non_exhaustive()
    }
}

impl VoiceSession {
    /// Join: open the SFU WebSocket, IDENTIFY, wait for READY and send the offer. `bind_ip` is the local
    /// address media is sent from (it becomes the ICE host candidate; use the interface the SFU can reach).
    pub async fn connect(
        update: &VoiceServerUpdate,
        bind_ip: IpAddr,
    ) -> Result<Self, TransportError> {
        if !(update.endpoint.starts_with("ws://") || update.endpoint.starts_with("wss://")) {
            return Err(TransportError::BadEndpoint);
        }
        let (mut ws, _) = tokio_tungstenite::connect_async(update.endpoint.as_str())
            .await
            .map_err(|e| TransportError::Socket(e.to_string()))?;
        send_ws(
            &mut ws,
            json!({"op": op::IDENTIFY, "d": {"token": update.token}}),
        )
        .await?;
        let ready = loop {
            match tokio::time::timeout(Duration::from_secs(10), ws.next()).await {
                Ok(Some(Ok(WsMessage::Text(t)))) => {
                    let v: Value = serde_json::from_str(t.as_str())
                        .map_err(|e| TransportError::Socket(e.to_string()))?;
                    if v["op"] == op::READY {
                        break v["d"].clone();
                    }
                }
                Ok(Some(Ok(WsMessage::Close(f)))) => {
                    return Err(TransportError::Refused(f.map_or(0, |f| u16::from(f.code))));
                }
                Ok(Some(Ok(_))) => {}
                Ok(Some(Err(e))) => return Err(TransportError::Socket(e.to_string())),
                Ok(None) => return Err(TransportError::Refused(0)),
                Err(_) => return Err(TransportError::NoReady),
            }
        };
        let socket = UdpSocket::bind((bind_ip, 0))
            .await
            .map_err(|e| TransportError::Socket(e.to_string()))?;
        let local = socket
            .local_addr()
            .map_err(|e| TransportError::Socket(e.to_string()))?;
        let mut rtc = Rtc::builder().build(Instant::now());
        rtc.add_local_candidate(
            Candidate::host(local, "udp").map_err(|e| TransportError::Rtc(e.to_string()))?,
        );
        let mut change = rtc.sdp_api();
        let mic = change.add_media(MediaKind::Audio, Direction::SendOnly, None, None, None);
        let (offer, pending) = change
            .apply()
            .ok_or_else(|| TransportError::Rtc("nothing to offer".into()))?;
        send_ws(
            &mut ws,
            json!({"op": op::SDP_OFFER, "d": {"sdp": offer.to_sdp_string()}}),
        )
        .await?;
        let heartbeat_every = Duration::from_millis(
            ready["heartbeat_interval"]
                .as_u64()
                .unwrap_or(13_750)
                .clamp(1000, 60_000)
                * 4
                / 5,
        );
        let initial_participants = ready["participants"]
            .as_array()
            .map(|a| a.iter().filter_map(user_of).collect())
            .unwrap_or_default();
        Ok(Self {
            ws,
            rtc,
            socket,
            local,
            mic,
            pending: Some(pending),
            start: Instant::now(),
            heartbeat_every,
            next_heartbeat: Instant::now() + heartbeat_every,
            beats: 0,
            reported_up: false,
            closed: None,
            e2ee: update.e2ee,
            transform: None,
            out: Vec::new(),
            events: VecDeque::new(),
            initial_participants,
        })
    }

    /// Install the frame transform (end-to-end encryption).
    pub fn with_transform(mut self, t: impl FrameTransform + 'static) -> Self {
        self.transform = Some(Box::new(t));
        self
    }

    /// Replace the transform (for instance after the key agreement produced one).
    pub fn set_transform(&mut self, t: impl FrameTransform + 'static) {
        self.transform = Some(Box::new(t));
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local
    }

    pub fn is_connected(&self) -> bool {
        self.rtc.is_connected()
    }

    /// Publish one Opus frame (20 ms). `audio_level` is the frame's level in dBov (-127 quiet .. 0 loud);
    /// the SFU's speaking indicator follows it. In an encrypted call this fails without a transform.
    pub fn send_opus(&mut self, frame: &[u8], audio_level: i8) -> Result<(), TransportError> {
        if !self.rtc.is_connected() {
            return Err(TransportError::NotConnected);
        }
        let payload = match self.transform.as_mut() {
            Some(t) => t.encrypt(frame).map_err(TransportError::Transform)?,
            None if self.e2ee => return Err(TransportError::EncryptionRequired),
            None => frame.to_vec(),
        };
        let mut w = self
            .rtc
            .writer(self.mic)
            .ok_or_else(|| TransportError::Rtc("no audio line".into()))?;
        let pt = w
            .payload_params()
            .find(|p| p.spec().codec == Codec::Opus)
            .ok_or_else(|| TransportError::Rtc("Opus was not negotiated".into()))?
            .pt();
        w = w.audio_level(audio_level, true);
        w.write(pt, Instant::now(), self.start.elapsed().into(), payload)
            .map_err(|e| TransportError::Rtc(e.to_string()))
    }

    /// Tell the SFU (and through it the channel) that the bot starts or stops speaking.
    pub fn set_speaking(&mut self, speaking: bool) {
        self.out
            .push(json!({"op": op::SPEAKING, "d": {"speaking": u8::from(speaking)}}));
    }

    /// Drive the connection for up to `max` and return what happened. Returns early as soon as there is
    /// at least one event.
    pub async fn pump(&mut self, max: Duration) -> Result<Vec<VoiceEvent>, TransportError> {
        let end = Instant::now() + max;
        let mut buf = vec![0u8; 2048];
        loop {
            let mut next = Instant::now() + Duration::from_millis(50);
            loop {
                match self
                    .rtc
                    .poll_output()
                    .map_err(|e| TransportError::Rtc(e.to_string()))?
                {
                    Output::Transmit(t) => {
                        if t.proto == Protocol::Udp {
                            let _ = self.socket.send_to(&t.contents, t.destination).await;
                        }
                    }
                    Output::Event(Event::MediaData(m)) => {
                        let stream = self
                            .rtc
                            .media(m.mid)
                            .map(|x| x.stream_id().to_owned())
                            .unwrap_or_default();
                        self.on_media(&stream, &m.data);
                    }
                    Output::Event(_) => {}
                    Output::Timeout(t) => {
                        next = next.min(t);
                        break;
                    }
                }
            }
            if self.rtc.is_connected() && !self.reported_up {
                self.reported_up = true;
                self.events.push_back(VoiceEvent::Connected);
            }
            for m in std::mem::take(&mut self.out) {
                send_ws(&mut self.ws, m).await?;
            }
            if Instant::now() >= self.next_heartbeat {
                self.beats += 1;
                send_ws(
                    &mut self.ws,
                    json!({"op": op::HEARTBEAT, "d": {"t": self.beats}}),
                )
                .await?;
                self.next_heartbeat = Instant::now() + self.heartbeat_every;
            }
            if !self.events.is_empty() || self.closed.is_some() || Instant::now() >= end {
                break;
            }
            let wait = next
                .min(self.next_heartbeat)
                .min(end)
                .saturating_duration_since(Instant::now());
            tokio::select! {
                r = self.socket.recv_from(&mut buf) => {
                    if let Ok((n, source)) = r
                        && let Ok(contents) = (&buf[..n]).try_into()
                    {
                        let _ = self.rtc.handle_input(Input::Receive(
                            Instant::now(),
                            Receive {
                                proto: Protocol::Udp,
                                source,
                                destination: self.local,
                                contents,
                            },
                        ));
                    }
                }
                m = self.ws.next() => self.on_ws(m).await?,
                () = tokio::time::sleep(wait) => {
                    let _ = self.rtc.handle_input(Input::Timeout(Instant::now()));
                }
            }
        }
        if let Some(code) = self.closed.take() {
            self.events.push_back(VoiceEvent::Closed(code));
        }
        Ok(self.events.drain(..).collect())
    }

    fn on_media(&mut self, stream: &str, data: &[u8]) {
        // The SFU names each incoming audio line after its speaker's user id.
        let Ok(user) = stream.parse::<u64>() else {
            return;
        };
        match self.transform.as_mut() {
            Some(t) => match t.decrypt(user, data) {
                Ok(frame) => self.events.push_back(VoiceEvent::Audio { user, frame }),
                Err(_) => self.events.push_back(VoiceEvent::Undecryptable { user }),
            },
            // An encrypted call without a transform: never hand ciphertext to the application as audio.
            None if self.e2ee => self.events.push_back(VoiceEvent::Undecryptable { user }),
            None => self.events.push_back(VoiceEvent::Audio {
                user,
                frame: data.to_vec(),
            }),
        }
    }

    async fn on_ws(
        &mut self,
        m: Option<Result<WsMessage, tokio_tungstenite::tungstenite::Error>>,
    ) -> Result<(), TransportError> {
        match m {
            Some(Ok(WsMessage::Text(t))) => {
                let Ok(v) = serde_json::from_str::<Value>(t.as_str()) else {
                    return Ok(());
                };
                let d = &v["d"];
                match v["op"].as_u64().and_then(|o| u8::try_from(o).ok()) {
                    Some(op::SDP_ANSWER) => {
                        if let (Some(p), Some(sdp)) = (self.pending.take(), d["sdp"].as_str())
                            && let Ok(answer) = SdpAnswer::from_sdp_string(sdp)
                        {
                            self.rtc
                                .sdp_api()
                                .accept_answer(p, answer)
                                .map_err(|e| TransportError::Rtc(e.to_string()))?;
                        }
                    }
                    Some(op::SDP_OFFER) => {
                        if let Some(sdp) = d["sdp"].as_str()
                            && let Ok(offer) = SdpOffer::from_sdp_string(sdp)
                        {
                            let answer = self
                                .rtc
                                .sdp_api()
                                .accept_offer(offer)
                                .map_err(|e| TransportError::Rtc(e.to_string()))?;
                            send_ws(
                                &mut self.ws,
                                json!({"op": op::SDP_ANSWER, "d": {"sdp": answer.to_sdp_string()}}),
                            )
                            .await?;
                        }
                    }
                    Some(op::CLIENT_CONNECT) => {
                        if let Some(u) = user_of(d) {
                            self.events.push_back(VoiceEvent::Joined(u));
                        }
                    }
                    Some(op::CLIENT_DISCONNECT) => {
                        if let Some(u) = user_of(d) {
                            self.events.push_back(VoiceEvent::Left(u));
                        }
                    }
                    Some(op::SPEAKING) => {
                        if let (Some(user), Some(s)) = (user_of(d), d["speaking"].as_u64()) {
                            self.events.push_back(VoiceEvent::Speaking {
                                user,
                                speaking: u8::try_from(s).unwrap_or(0),
                            });
                        }
                    }
                    _ => {}
                }
            }
            Some(Ok(WsMessage::Close(f))) => self.closed = Some(f.map_or(0, |f| u16::from(f.code))),
            Some(Ok(_)) => {}
            Some(Err(_)) | None => {
                self.closed.get_or_insert(0);
            }
        }
        Ok(())
    }

    /// Leave: close the WebSocket. Send gateway op 4 with `channel_id: null` too (see [`voice_state_body`]).
    pub async fn leave(mut self) {
        let _ = self.ws.close(None).await;
    }
}

async fn send_ws(ws: &mut Ws, v: Value) -> Result<(), TransportError> {
    ws.send(WsMessage::text(v.to_string()))
        .await
        .map_err(|e| TransportError::Socket(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_server_update_parses_and_defaults_to_encrypted() {
        let d = json!({"token": "secret-token", "endpoint": "wss://v.example/voice/ws", "channel_id": "5",
            "guild_id": "9", "ice_servers": []});
        let u = VoiceServerUpdate::from_dispatch(&d).unwrap();
        assert_eq!(u.channel_id, "5");
        assert_eq!(u.guild_id.as_deref(), Some("9"));
        assert!(u.e2ee, "a missing sr_e2ee counts as encrypted");
        let mut off = d.clone();
        off["sr_e2ee"] = json!(false);
        assert!(!VoiceServerUpdate::from_dispatch(&off).unwrap().e2ee);
        assert!(VoiceServerUpdate::from_dispatch(&json!({"token": "t"})).is_none());
        assert!(!format!("{u:?}").contains("secret-token"));
    }

    #[test]
    fn the_join_body_declares_e2ee_and_leaves_with_a_null_channel() {
        let j = voice_state_body(Some("9"), Some("5"), false, false, true);
        assert_eq!(j["sr_e2ee"], true);
        assert_eq!(j["guild_id"], "9");
        let l = voice_state_body(None, None, false, false, true);
        assert!(l["channel_id"].is_null() && l.get("guild_id").is_none());
    }

    #[tokio::test]
    async fn a_non_websocket_endpoint_is_refused_before_any_io() {
        let u = VoiceServerUpdate {
            token: "t".into(),
            endpoint: "https://example.test/voice".into(),
            guild_id: None,
            channel_id: "1".into(),
            e2ee: true,
        };
        assert!(matches!(
            VoiceSession::connect(&u, [127, 0, 0, 1].into()).await,
            Err(TransportError::BadEndpoint)
        ));
    }
}
