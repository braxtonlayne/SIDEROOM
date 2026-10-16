//! End to end over real sockets: str0m clients speak the voice WebSocket protocol to the
//! actual SFU (axum WebSocket, UDP and ICE-TCP sockets on loopback) and audio RTP from one
//! client must reach the other.

use std::net::SocketAddr;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures::{SinkExt, StreamExt};
use sr_core::{ChannelId, GuildId, UserId};
use sr_voice::{SfuConfig, SfuHandle};
use sr_voice_proto::wire::{self, Message, Ready, Speaking, UserRef, close, op, speaking};
use sr_voice_proto::{JoinClaims, VoiceCommand, VoiceRelay};
use str0m::change::{SdpAnswer, SdpOffer, SdpPendingOffer};
use str0m::format::Codec;
use str0m::media::{Direction, MediaKind, Mid};
use str0m::net::{Protocol, Receive, TcpType};
use str0m::{Candidate, Event, Input, Output, Rtc};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use tokio_tungstenite::tungstenite::Message as TMsg;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

const SECRET: &[u8] = b"test-secret-test-secret-test-secret";
const CHANNEL: u64 = 500;

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

struct Server {
    handle: SfuHandle,
    ws_url: String,
}

async fn start_server(tcp: bool) -> Server {
    start_server_with(tcp, Duration::from_secs(20)).await
}

async fn start_server_with(tcp: bool, resume_window: Duration) -> Server {
    let handle = sr_voice::start(SfuConfig {
        resume_window,
        bind_ip: [127, 0, 0, 1].into(),
        udp_port_min: 0,
        udp_port_max: 0,
        tcp_port: tcp.then_some(0),
        token_secret: SECRET.to_vec(),
        ..SfuConfig::default()
    })
    .await
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = sr_voice::router(handle.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    Server {
        handle,
        ws_url: format!("ws://{addr}/voice/ws"),
    }
}

fn token(user: u64, exp: u64, speak: bool) -> String {
    token_in(user, CHANNEL, exp, speak)
}

fn token_in(user: u64, channel: u64, exp: u64, speak: bool) -> String {
    sr_voice_proto::TokenSigner::new(SECRET.to_vec())
        .sign(&JoinClaims {
            user_id: UserId::new(user),
            guild_id: Some(GuildId::new(9)),
            channel_id: ChannelId::new(channel),
            session_id: format!("gw-{user}"),
            exp,
            speak,
            deaf: false,
            ptt_required: false,
            priority: false,
            camera: false,
            screen: false,
            tier: 0,
        })
        .unwrap()
}

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

enum Transport {
    Udp(UdpSocket),
    Tcp(TcpStream),
}

struct Vc {
    user: u64,
    ws: Ws,
    rtc: Rtc,
    transport: Transport,
    local: SocketAddr,
    server_tcp: Option<SocketAddr>,
    mic: Mid,
    pending: Option<SdpPendingOffer>,
    heard: Vec<(String, Vec<u8>)>,
    signals: Vec<Message>,
    ready: Option<Ready>,
    closed: Option<u16>,
    talk: Option<i8>,
    next_frame: Instant,
    start: Instant,
    seq: u32,
    tcp_buf: Vec<u8>,
}

impl Vc {
    async fn connect(server: &Server, user: u64, tcp_only: bool) -> Vc {
        Self::connect_in(server, user, CHANNEL, tcp_only).await
    }

    async fn connect_in(server: &Server, user: u64, channel: u64, tcp_only: bool) -> Vc {
        let (mut ws, _) = tokio_tungstenite::connect_async(&server.ws_url)
            .await
            .unwrap();
        let ready = Self::identify(&mut ws, &token_in(user, channel, unix_now() + 60, true)).await;
        let mut vc = Self::with_ws(server, user, ws, tcp_only).await;
        vc.ready = Some(ready);
        vc
    }

    async fn identify(ws: &mut Ws, token: &str) -> Ready {
        send(
            ws,
            Message::new(
                op::IDENTIFY,
                wire::Identify {
                    token: token.into(),
                },
            ),
        )
        .await;
        let m = recv(ws).await.expect("READY");
        assert_eq!(m.op, op::READY);
        m.data().unwrap()
    }

    async fn with_ws(server: &Server, user: u64, ws: Ws, tcp_only: bool) -> Vc {
        let (transport, local) = if tcp_only {
            let s = TcpStream::connect(("127.0.0.1", server.handle.tcp_port().unwrap()))
                .await
                .unwrap();
            let a = s.local_addr().unwrap();
            (Transport::Tcp(s), a)
        } else {
            let s = UdpSocket::bind("127.0.0.1:0").await.unwrap();
            let a = s.local_addr().unwrap();
            (Transport::Udp(s), a)
        };
        let mut rtc = Rtc::builder().build(Instant::now());
        if tcp_only {
            rtc.add_local_candidate(
                Candidate::builder()
                    .tcp()
                    .host(local)
                    .tcptype(TcpType::Active)
                    .build()
                    .unwrap(),
            );
        } else {
            rtc.add_local_candidate(Candidate::host(local, "udp").unwrap());
        }
        let mut change = rtc.sdp_api();
        let mic = change.add_media(MediaKind::Audio, Direction::SendOnly, None, None, None);
        let (offer, pending) = change.apply().unwrap();
        let mut vc = Vc {
            user,
            ws,
            rtc,
            transport,
            local,
            server_tcp: server
                .handle
                .tcp_port()
                .map(|p| SocketAddr::from(([127, 0, 0, 1], p))),
            mic,
            pending: Some(pending),
            heard: Vec::new(),
            signals: Vec::new(),
            ready: None,
            closed: None,
            talk: None,
            next_frame: Instant::now(),
            start: Instant::now(),
            seq: 0,
            tcp_buf: Vec::new(),
        };
        send(
            &mut vc.ws,
            Message::new(
                op::SDP_OFFER,
                wire::Sdp {
                    sdp: offer.to_sdp_string(),
                },
            ),
        )
        .await;
        vc
    }

    async fn on_ws(&mut self, m: Option<Result<TMsg, tokio_tungstenite::tungstenite::Error>>) {
        match m {
            Some(Ok(TMsg::Text(t))) => {
                let msg: Message = serde_json::from_str(t.as_str()).unwrap();
                match msg.op {
                    op::SDP_ANSWER => {
                        let s: wire::Sdp = msg.data().unwrap();
                        if let Some(p) = self.pending.take() {
                            self.rtc
                                .sdp_api()
                                .accept_answer(p, SdpAnswer::from_sdp_string(&s.sdp).unwrap())
                                .unwrap();
                        }
                    }
                    op::SDP_OFFER => {
                        let s: wire::Sdp = msg.data().unwrap();
                        let answer = self
                            .rtc
                            .sdp_api()
                            .accept_offer(SdpOffer::from_sdp_string(&s.sdp).unwrap())
                            .unwrap();
                        send(
                            &mut self.ws,
                            Message::new(
                                op::SDP_ANSWER,
                                wire::Sdp {
                                    sdp: answer.to_sdp_string(),
                                },
                            ),
                        )
                        .await;
                    }
                    _ => self.signals.push(msg),
                }
            }
            Some(Ok(TMsg::Close(Some(f)))) => self.closed = Some(u16::from(f.code)),
            Some(Ok(TMsg::Close(None))) | None | Some(Err(_)) => {
                self.closed.get_or_insert(0);
            }
            _ => {}
        }
    }

    async fn transmit(&mut self, t: str0m::net::Transmit) {
        match (&mut self.transport, t.proto) {
            (Transport::Udp(s), Protocol::Udp) => {
                let _ = s.send_to(&t.contents, t.destination).await;
            }
            (Transport::Tcp(s), Protocol::Tcp) => {
                let len = u16::try_from(t.contents.len()).unwrap().to_be_bytes();
                let _ = s.write_all(&len).await;
                let _ = s.write_all(&t.contents).await;
            }
            _ => {}
        }
    }

    fn feed(&mut self, proto: Protocol, source: SocketAddr, data: &[u8]) {
        let Ok(contents) = data.try_into() else {
            return;
        };
        let _ = self.rtc.handle_input(Input::Receive(
            Instant::now(),
            Receive {
                proto,
                source,
                destination: self.local,
                contents,
            },
        ));
    }

    /// Run this client's event loop for `d`.
    async fn run_for(&mut self, d: Duration) {
        let end = Instant::now() + d;
        let mut buf = vec![0u8; 2048];
        while Instant::now() < end && self.closed.is_none() {
            let mut next = Instant::now() + Duration::from_millis(50);
            loop {
                match self.rtc.poll_output().unwrap() {
                    Output::Transmit(t) => self.transmit(t).await,
                    Output::Event(Event::MediaData(m)) => {
                        let stream = self.rtc.media(m.mid).unwrap().stream_id().to_owned();
                        self.heard.push((stream, m.data.to_vec()));
                    }
                    Output::Event(_) => {}
                    Output::Timeout(t) => {
                        next = next.min(t);
                        break;
                    }
                }
            }
            if let Some(level) = self.talk
                && self.rtc.is_connected()
                && Instant::now() >= self.next_frame
            {
                self.seq += 1;
                let mut payload = vec![self.user as u8; 80];
                payload[1..5].copy_from_slice(&self.seq.to_be_bytes());
                let mut w = self.rtc.writer(self.mic).unwrap();
                let pt = w
                    .payload_params()
                    .find(|p| p.spec().codec == Codec::Opus)
                    .unwrap()
                    .pt();
                w = w.audio_level(level, true);
                let _ = w.write(pt, Instant::now(), self.start.elapsed().into(), payload);
                self.next_frame += Duration::from_millis(20);
            }
            if self.talk.is_some() {
                next = next.min(self.next_frame);
            }
            let wait = next
                .saturating_duration_since(Instant::now())
                .min(end.saturating_duration_since(Instant::now()));
            let tcp_peer = self.server_tcp;
            tokio::select! {
                r = async {
                    match &mut self.transport {
                        Transport::Udp(s) => s.recv_from(&mut buf).await.map(|(n, a)| (Protocol::Udp, a, buf[..n].to_vec())),
                        Transport::Tcp(s) => {
                            // RFC 4571 framing.
                            let mut len = [0u8; 2];
                            s.read_exact(&mut len).await?;
                            let mut data = vec![0u8; usize::from(u16::from_be_bytes(len))];
                            s.read_exact(&mut data).await?;
                            Ok((Protocol::Tcp, tcp_peer.unwrap(), data))
                        }
                    }
                } => {
                    if let Ok((proto, src, data)) = r {
                        self.feed(proto, src, &data);
                    }
                }
                m = self.ws.next() => self.on_ws(m).await,
                () = tokio::time::sleep(wait) => {
                    let _ = self.rtc.handle_input(Input::Timeout(Instant::now()));
                }
            }
        }
        let _ = &self.tcp_buf;
    }

    fn frames_from(&self, speaker: u64) -> usize {
        let s = speaker.to_string();
        self.heard.iter().filter(|(st, _)| *st == s).count()
    }

    fn speaking_events(&self, speaker: u64) -> Vec<u8> {
        self.signals
            .iter()
            .filter(|m| m.op == op::SPEAKING)
            .filter_map(|m| m.data::<Speaking>().ok())
            .filter(|s| s.user_id == Some(UserId::new(speaker)))
            .map(|s| s.speaking)
            .collect()
    }
}

async fn send(ws: &mut Ws, m: Message) {
    ws.send(TMsg::text(m.to_text())).await.unwrap();
}

async fn recv(ws: &mut Ws) -> Option<Message> {
    loop {
        match tokio::time::timeout(Duration::from_secs(5), ws.next()).await {
            Ok(Some(Ok(TMsg::Text(t)))) => return serde_json::from_str(t.as_str()).ok(),
            Ok(Some(Ok(TMsg::Close(_)))) | Ok(None) | Err(_) => return None,
            Ok(Some(Ok(_))) => {}
            Ok(Some(Err(_))) => return None,
        }
    }
}

async fn close_code(ws: &mut Ws) -> Option<u16> {
    loop {
        match tokio::time::timeout(Duration::from_secs(5), ws.next()).await {
            Ok(Some(Ok(TMsg::Close(Some(f))))) => return Some(u16::from(f.code)),
            Ok(Some(Ok(_))) => {}
            _ => return None,
        }
    }
}

/// Run two clients concurrently.
async fn both(a: &mut Vc, b: &mut Vc, d: Duration) {
    tokio::join!(a.run_for(d), b.run_for(d));
}

async fn settle(a: &mut Vc, b: &mut Vc) {
    for _ in 0..40 {
        both(a, b, Duration::from_millis(250)).await;
        if a.rtc.is_connected() && b.rtc.is_connected() {
            // Let the subscription lines negotiate.
            both(a, b, Duration::from_millis(1000)).await;
            return;
        }
    }
    panic!("clients did not connect");
}

#[tokio::test]
async fn audio_rtp_flows_between_two_clients_over_real_sockets() {
    let server = start_server(false).await;
    let mut a = Vc::connect(&server, 1, false).await;
    let mut b = Vc::connect(&server, 2, false).await;
    // READY lists who was already there.
    assert!(a.ready.as_ref().unwrap().participants.is_empty());
    assert_eq!(
        b.ready
            .as_ref()
            .unwrap()
            .participants
            .first()
            .map(|p| p.user_id),
        Some(UserId::new(1))
    );
    settle(&mut a, &mut b).await;

    a.talk = Some(-28);
    both(&mut a, &mut b, Duration::from_millis(1500)).await;
    a.talk = None;
    both(&mut a, &mut b, Duration::from_millis(600)).await;

    assert!(
        b.frames_from(1) >= 40,
        "B received {} of A's Opus frames",
        b.frames_from(1)
    );
    assert!(a.heard.is_empty(), "A must not hear itself");
    // CLIENT_CONNECT told A about B, and speaking started/stopped for B's indicator.
    assert!(
        a.signals.iter().any(|m| m.op == op::CLIENT_CONNECT
            && m.data::<UserRef>().unwrap().user_id == UserId::new(2))
    );
    assert_eq!(b.speaking_events(1), vec![speaking::MICROPHONE, 0]);
    server.handle.shutdown().await;
}

#[tokio::test]
async fn audio_flows_over_ice_tcp_when_udp_is_unavailable() {
    let server = start_server(true).await;
    let mut a = Vc::connect(&server, 1, true).await;
    let mut b = Vc::connect(&server, 2, false).await;
    settle(&mut a, &mut b).await;
    // B listens over UDP while A is TCP-only: the SFU bridges both transports.
    a.talk = Some(-28);
    both(&mut a, &mut b, Duration::from_millis(1500)).await;
    assert!(
        b.frames_from(1) >= 30,
        "B received {} frames from the TCP-only client",
        b.frames_from(1)
    );
    server.handle.shutdown().await;
}

#[tokio::test]
async fn identify_rejects_bad_expired_and_misordered_input() {
    let server = start_server(false).await;
    let connect = || async {
        tokio_tungstenite::connect_async(&server.ws_url)
            .await
            .unwrap()
            .0
    };

    let mut ws = connect().await;
    send(
        &mut ws,
        Message::new(
            op::IDENTIFY,
            wire::Identify {
                token: "nope".into(),
            },
        ),
    )
    .await;
    assert_eq!(
        close_code(&mut ws).await,
        Some(close::AUTHENTICATION_FAILED)
    );

    let mut ws = connect().await;
    let expired = token(1, unix_now() - 5, true);
    send(
        &mut ws,
        Message::new(op::IDENTIFY, wire::Identify { token: expired }),
    )
    .await;
    assert_eq!(
        close_code(&mut ws).await,
        Some(close::AUTHENTICATION_FAILED)
    );

    let mut ws = connect().await;
    send(
        &mut ws,
        Message::new(op::HEARTBEAT, serde_json::json!({"t": 1})),
    )
    .await;
    assert_eq!(close_code(&mut ws).await, Some(close::NOT_AUTHENTICATED));

    let mut ws = connect().await;
    ws.send(TMsg::text("{not json")).await.unwrap();
    assert_eq!(close_code(&mut ws).await, Some(close::DECODE_ERROR));

    // After a good IDENTIFY: heartbeat is echoed, a second IDENTIFY is refused.
    let mut ws = connect().await;
    let ready = Vc::identify(&mut ws, &token(5, unix_now() + 60, true)).await;
    assert_eq!(ready.user_id, UserId::new(5));
    assert_eq!(ready.heartbeat_interval, 13_750);
    send(
        &mut ws,
        Message::new(op::HEARTBEAT, serde_json::json!({"t": "x"})),
    )
    .await;
    let ack = recv(&mut ws).await.unwrap();
    assert_eq!(
        (ack.op, ack.d),
        (op::HEARTBEAT_ACK, serde_json::json!({"t": "x"}))
    );
    send(
        &mut ws,
        Message::new(
            op::IDENTIFY,
            wire::Identify {
                token: token(5, unix_now() + 60, true),
            },
        ),
    )
    .await;
    assert_eq!(
        close_code(&mut ws).await,
        Some(close::ALREADY_AUTHENTICATED)
    );

    // An unknown opcode.
    let mut ws = connect().await;
    Vc::identify(&mut ws, &token(6, unix_now() + 60, true)).await;
    send(&mut ws, Message::new(99, serde_json::json!({}))).await;
    assert_eq!(close_code(&mut ws).await, Some(close::UNKNOWN_OPCODE));
    server.handle.shutdown().await;
}

#[tokio::test]
async fn a_dropped_websocket_resumes_and_media_keeps_flowing() {
    let server = start_server(false).await;
    let mut a = Vc::connect(&server, 1, false).await;
    let mut b = Vc::connect(&server, 2, false).await;
    settle(&mut a, &mut b).await;
    let ready = a.ready.clone().unwrap();

    // A's socket dies; its media connection stays up.
    a.ws.close(None).await.ok();
    let (mut ws2, _) = tokio_tungstenite::connect_async(&server.ws_url)
        .await
        .unwrap();
    send(
        &mut ws2,
        Message::new(
            op::RESUME,
            wire::Resume {
                session_id: ready.session_id.clone(),
                resume_token: ready.resume_token.clone(),
            },
        ),
    )
    .await;
    let resumed = recv(&mut ws2).await.expect("RESUMED");
    assert_eq!(resumed.op, op::RESUMED);
    let r: Ready = resumed.data().unwrap();
    assert_eq!(r.session_id, ready.session_id);
    assert_eq!(r.participants.len(), 1);
    a.ws = ws2;
    a.closed = None;

    a.talk = Some(-28);
    both(&mut a, &mut b, Duration::from_millis(1200)).await;
    assert!(
        b.frames_from(1) >= 30,
        "only {} frames after resume",
        b.frames_from(1)
    );

    // A wrong resume token and an unknown session are refused.
    let (mut ws3, _) = tokio_tungstenite::connect_async(&server.ws_url)
        .await
        .unwrap();
    send(
        &mut ws3,
        Message::new(
            op::RESUME,
            wire::Resume {
                session_id: ready.session_id.clone(),
                resume_token: "0".repeat(32),
            },
        ),
    )
    .await;
    assert_eq!(close_code(&mut ws3).await, Some(close::SESSION_INVALID));
    server.handle.shutdown().await;
}

#[tokio::test]
async fn gateway_commands_mute_and_disconnect_on_the_media_path() {
    let server = start_server(false).await;
    let mut a = Vc::connect(&server, 1, false).await;
    let mut b = Vc::connect(&server, 2, false).await;
    settle(&mut a, &mut b).await;

    a.talk = Some(-28);
    both(&mut a, &mut b, Duration::from_millis(800)).await;
    let before = b.frames_from(1);
    assert!(before >= 20);

    // Server mute.
    server
        .handle
        .apply(VoiceCommand::SetFlags {
            channel: ChannelId::new(CHANNEL),
            user: UserId::new(1),
            speak: false,
            deaf: false,
            ptt_required: false,
            priority: false,
            camera: false,
            screen: false,
        })
        .await;
    // Frames already in flight when the mute landed may still arrive; let them drain
    // (generously, the test machine can be busy) before counting.
    both(&mut a, &mut b, Duration::from_millis(1500)).await;
    let at_mute = b.frames_from(1);
    both(&mut a, &mut b, Duration::from_millis(800)).await;
    assert_eq!(b.frames_from(1), at_mute, "muted audio kept flowing");

    // Disconnect: A's WebSocket closes with 4014, B hears about it.
    server
        .handle
        .apply(VoiceCommand::Disconnect {
            channel: ChannelId::new(CHANNEL),
            user: UserId::new(1),
        })
        .await;
    both(&mut a, &mut b, Duration::from_millis(500)).await;
    assert_eq!(a.closed, Some(close::DISCONNECTED));
    assert!(
        b.signals.iter().any(|m| m.op == op::CLIENT_DISCONNECT
            && m.data::<UserRef>().unwrap().user_id == UserId::new(1))
    );
    server.handle.shutdown().await;
}

#[tokio::test]
async fn a_second_session_of_the_same_user_replaces_the_first() {
    let server = start_server(false).await;
    let mut first = Vc::connect(&server, 1, false).await;
    let mut other = Vc::connect(&server, 2, false).await;
    settle(&mut first, &mut other).await;
    let _second = Vc::connect(&server, 1, false).await;
    both(&mut first, &mut other, Duration::from_millis(300)).await;
    assert_eq!(first.closed, Some(close::SESSION_REPLACED));
    server.handle.shutdown().await;
}

#[tokio::test]
async fn draining_closes_clients_and_refuses_new_joins() {
    let server = start_server(false).await;
    let mut a = Vc::connect(&server, 1, false).await;
    // Let the handshake finish: closing with unread client data would reset the connection.
    a.run_for(Duration::from_millis(500)).await;
    server.handle.drain().await;
    a.run_for(Duration::from_millis(300)).await;
    assert_eq!(a.closed, Some(close::SERVER_RESTARTING));
    let (mut ws, _) = tokio_tungstenite::connect_async(&server.ws_url)
        .await
        .unwrap();
    send(
        &mut ws,
        Message::new(
            op::IDENTIFY,
            wire::Identify {
                token: token(2, unix_now() + 60, true),
            },
        ),
    )
    .await;
    assert_eq!(close_code(&mut ws).await, Some(close::SERVER_RESTARTING));
    server.handle.shutdown().await;
}

#[tokio::test]
async fn a_mute_issued_before_the_client_connects_still_applies() {
    let server = start_server(false).await;
    // The gateway muted user 1 after minting the token but before the client reached the SFU.
    server
        .handle
        .apply(VoiceCommand::SetFlags {
            channel: ChannelId::new(CHANNEL),
            user: UserId::new(1),
            speak: false,
            deaf: false,
            ptt_required: false,
            priority: false,
            camera: false,
            screen: false,
        })
        .await;
    let mut a = Vc::connect(&server, 1, false).await;
    let mut b = Vc::connect(&server, 2, false).await;
    settle(&mut a, &mut b).await;
    assert!(a.ready.as_ref().unwrap().participants.is_empty());
    a.talk = Some(-28);
    both(&mut a, &mut b, Duration::from_millis(1000)).await;
    assert_eq!(
        b.frames_from(1),
        0,
        "the token said speak=true, the later mute wins"
    );
    server.handle.shutdown().await;
}

#[tokio::test]
async fn a_participant_whose_websocket_never_returns_is_reported_lost() {
    let server = start_server_with(false, Duration::from_millis(600)).await;
    let mut events = server.handle.subscribe();
    let mut a = Vc::connect(&server, 1, false).await;
    let mut b = Vc::connect(&server, 2, false).await;
    settle(&mut a, &mut b).await;

    // A's socket dies and nobody resumes it.
    a.ws.close(None).await.ok();
    let lost = tokio::time::timeout(Duration::from_secs(6), events.recv())
        .await
        .expect("the SFU should report the lost participant")
        .unwrap();
    assert_eq!(lost.user, UserId::new(1));
    assert_eq!(lost.channel, ChannelId::new(CHANNEL));
    // B is told A left.
    both(&mut a, &mut b, Duration::from_millis(300)).await;
    assert!(
        b.signals.iter().any(|m| m.op == op::CLIENT_DISCONNECT
            && m.data::<UserRef>().unwrap().user_id == UserId::new(1))
    );

    // Commands from the gateway (leave, move) are not echoed back as "lost".
    server
        .handle
        .apply(VoiceCommand::Disconnect {
            channel: ChannelId::new(CHANNEL),
            user: UserId::new(2),
        })
        .await;
    both(&mut a, &mut b, Duration::from_millis(500)).await;
    assert!(events.try_recv().is_err());
    server.handle.shutdown().await;
}

// ---- sharded media actors (VOX-SFU-011) -------------------------------------------------------

/// A UDP port `p` such that `p` and `p + 1` can both be bound right now.
fn free_port_pair() -> u16 {
    for _ in 0..50 {
        let a = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let p = a.local_addr().unwrap().port();
        if p < u16::MAX && std::net::UdpSocket::bind(("127.0.0.1", p + 1)).is_ok() {
            return p;
        }
    }
    panic!("no pair of free UDP ports");
}

async fn start_sharded(shards: usize, tcp: bool) -> Server {
    // The shard count is capped by the sockets actually bound. Another test running in parallel can
    // take the second port between `free_port_pair` and `start`, so try again with a fresh pair.
    let mut attempt = 0;
    let handle = loop {
        let p = free_port_pair();
        let handle = sr_voice::start(SfuConfig {
            bind_ip: [127, 0, 0, 1].into(),
            udp_port_min: p,
            udp_port_max: p + 1,
            tcp_port: tcp.then_some(0),
            token_secret: SECRET.to_vec(),
            shards,
            ..SfuConfig::default()
        })
        .await
        .unwrap();
        attempt += 1;
        if handle.shard_count() == shards || attempt >= 20 {
            break handle;
        }
        handle.shutdown().await;
    };
    assert_eq!(handle.shard_count(), shards);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = sr_voice::router(handle.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    Server {
        handle,
        ws_url: format!("ws://{addr}/voice/ws"),
    }
}

/// Two channel ids that live on different shards.
fn channels_on_both_shards(server: &Server) -> (u64, u64) {
    let first = 7_000u64;
    let s0 = server.handle.shard_of_channel(ChannelId::new(first));
    let second = (first + 1..first + 200)
        .find(|c| server.handle.shard_of_channel(ChannelId::new(*c)) != s0)
        .expect("a channel on the other shard");
    (first, second)
}

async fn four(a: &mut Vc, b: &mut Vc, c: &mut Vc, d: &mut Vc, ms: u64) {
    let t = Duration::from_millis(ms);
    tokio::join!(a.run_for(t), b.run_for(t), c.run_for(t), d.run_for(t));
}

async fn settle_four(a: &mut Vc, b: &mut Vc, c: &mut Vc, d: &mut Vc) {
    for _ in 0..40 {
        four(a, b, c, d, 250).await;
        if [&*a, &*b, &*c, &*d].iter().all(|v| v.rtc.is_connected()) {
            four(a, b, c, d, 1000).await;
            return;
        }
    }
    panic!("clients did not connect");
}

#[tokio::test]
async fn rooms_on_different_shards_both_carry_audio_and_stay_apart() {
    let server = start_sharded(2, false).await;
    let (x, y) = channels_on_both_shards(&server);
    let mut a = Vc::connect_in(&server, 1, x, false).await;
    let mut b = Vc::connect_in(&server, 2, x, false).await;
    let mut c = Vc::connect_in(&server, 3, y, false).await;
    let mut d = Vc::connect_in(&server, 4, y, false).await;
    settle_four(&mut a, &mut b, &mut c, &mut d).await;

    a.talk = Some(-28);
    c.talk = Some(-28);
    four(&mut a, &mut b, &mut c, &mut d, 1500).await;
    a.talk = None;
    c.talk = None;
    four(&mut a, &mut b, &mut c, &mut d, 600).await;

    assert!(b.frames_from(1) >= 40, "B heard {}", b.frames_from(1));
    assert!(d.frames_from(3) >= 40, "D heard {}", d.frames_from(3));
    // Rooms are apart: nobody hears the other channel.
    assert_eq!(b.frames_from(3) + d.frames_from(1), 0);
    assert!(a.heard.is_empty() && c.heard.is_empty());
    server.handle.shutdown().await;
}

#[tokio::test]
async fn joining_a_channel_on_another_shard_replaces_the_old_session() {
    let server = start_sharded(2, false).await;
    let (x, y) = channels_on_both_shards(&server);
    let mut first = Vc::connect_in(&server, 1, x, false).await;
    let mut other = Vc::connect_in(&server, 2, x, false).await;
    settle(&mut first, &mut other).await;
    let _second = Vc::connect_in(&server, 1, y, false).await;
    both(&mut first, &mut other, Duration::from_millis(500)).await;
    assert_eq!(first.closed, Some(close::SESSION_REPLACED));
    server.handle.shutdown().await;
}

#[tokio::test]
async fn resume_finds_its_shard_and_control_commands_follow_the_channel() {
    let server = start_sharded(2, false).await;
    let (x, y) = channels_on_both_shards(&server);
    // Whatever shard y lives on, resume must reach the shard that issued the session.
    for channel in [x, y] {
        let user = channel; // unique per iteration
        let mut a = Vc::connect_in(&server, user, channel, false).await;
        let mut b = Vc::connect_in(&server, user + 10_000, channel, false).await;
        settle(&mut a, &mut b).await;
        let ready = a.ready.clone().unwrap();
        a.ws.close(None).await.ok();
        let (mut ws2, _) = tokio_tungstenite::connect_async(&server.ws_url)
            .await
            .unwrap();
        send(
            &mut ws2,
            Message::new(
                op::RESUME,
                wire::Resume {
                    session_id: ready.session_id.clone(),
                    resume_token: ready.resume_token.clone(),
                },
            ),
        )
        .await;
        let resumed = recv(&mut ws2).await.expect("RESUMED");
        assert_eq!(resumed.op, op::RESUMED, "channel {channel}");
        a.ws = ws2;
        a.closed = None;
        // A moderator disconnect addressed by channel reaches the right shard.
        server
            .handle
            .apply(VoiceCommand::Disconnect {
                channel: ChannelId::new(channel),
                user: UserId::new(user),
            })
            .await;
        both(&mut a, &mut b, Duration::from_millis(500)).await;
        assert_eq!(a.closed, Some(close::DISCONNECTED), "channel {channel}");
    }
    server.handle.shutdown().await;
}

#[tokio::test]
async fn ice_tcp_reaches_the_shard_that_owns_the_participant() {
    let server = start_sharded(2, true).await;
    let (x, y) = channels_on_both_shards(&server);
    for (n, channel) in [x, y].into_iter().enumerate() {
        let base = 100 * (n as u64 + 1);
        let mut a = Vc::connect_in(&server, base + 1, channel, true).await;
        let mut b = Vc::connect_in(&server, base + 2, channel, true).await;
        settle(&mut a, &mut b).await;
        a.talk = Some(-28);
        both(&mut a, &mut b, Duration::from_millis(1500)).await;
        assert!(
            b.frames_from(base + 1) >= 30,
            "channel {channel}: B heard {} frames over TCP",
            b.frames_from(base + 1)
        );
    }
    server.handle.shutdown().await;
}
