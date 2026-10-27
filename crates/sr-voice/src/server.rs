//! The tokio shell around [`SfuCore`]: media sockets (UDP range, ICE-TCP), one actor task
//! that owns the core, and the handle the WebSocket layer and the gateway talk to.
//!
//! Media work is sharded (VOX-SFU-011): `SfuConfig::shards` actors, each owning an [`SfuCore`],
//! a disjoint subset of the UDP sockets and the rooms whose channel id hashes to it. A
//! participant id encodes its shard (`(id - 1) % shards`), so the handle routes every command
//! without a lookup. The few cross-shard facts (a user joining a second channel on another
//! shard, ICE-TCP connections whose shard is only known from their first STUN packet, node
//! wide participant cap and metrics) live in [`Shared`].

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use sr_core::{ChannelId, UserId};
use sr_voice_proto::video::Video;
use sr_voice_proto::wire::{self, Message, Ready, close, op};
use sr_voice_proto::{JoinClaims, TokenSigner, VoiceCommand, VoiceRelay};
use str0m::Candidate;
use str0m::net::{Protocol, TcpType};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::{mpsc, oneshot};

use crate::config::{MAX_UDP_SOCKETS, SfuConfig};
use crate::core::{
    CloseReason, CoreEvent, CoreOutput, Flags, JoinParams, ParticipantId, SfuCore, Snapshot,
};
use crate::metrics;

/// What the actor sends down a voice WebSocket.
#[derive(Debug, Clone)]
pub enum WsOut {
    Msg(Message),
    Close(u16, &'static str),
}

/// One WebSocket attachment to a participant.
#[derive(Debug, Clone)]
pub struct ConnTx {
    pub id: u64,
    pub tx: mpsc::Sender<WsOut>,
}

/// Result of a successful IDENTIFY or RESUME.
#[derive(Debug, Clone)]
pub struct Joined {
    pub pid: ParticipantId,
    pub ready: Ready,
}

pub(crate) enum Cmd {
    Join {
        claims: JoinClaims,
        conn: ConnTx,
        reply: oneshot::Sender<Result<Joined, u16>>,
    },
    Resume {
        session_id: String,
        resume_token: String,
        conn: ConnTx,
        reply: oneshot::Sender<Result<Joined, u16>>,
    },
    Offer {
        pid: ParticipantId,
        conn_id: u64,
        sdp: String,
    },
    Answer {
        pid: ParticipantId,
        conn_id: u64,
        sdp: String,
    },
    Candidate {
        pid: ParticipantId,
        conn_id: u64,
        line: String,
    },
    Speaking {
        pid: ParticipantId,
        conn_id: u64,
        bits: u8,
    },
    Video {
        pid: ParticipantId,
        conn_id: u64,
        msg: Box<Video>,
    },
    WsLost {
        pid: ParticipantId,
        conn_id: u64,
    },
    Control(VoiceCommand),
    TcpOpen {
        peer: SocketAddr,
        tx: mpsc::Sender<Vec<u8>>,
    },
    TcpClosed {
        peer: SocketAddr,
    },
    /// Drop the user's participant here: they joined a channel that lives on another shard.
    Evict {
        user: UserId,
    },
    Drain,
    Shutdown(oneshot::Sender<()>),
}

struct Packet {
    proto: Protocol,
    src: SocketAddr,
    dst: SocketAddr,
    data: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum StartError {
    #[error("no UDP port in {0}..={1} could be bound")]
    NoUdpPort(u16, u16),
    #[error("binding the ICE-TCP port: {0}")]
    Tcp(std::io::Error),
    #[error("voice token secret is empty")]
    NoSecret,
    #[error("announce_ips is empty")]
    NoAnnounce,
}

/// What an [`SfuEvent`] reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SfuEventKind {
    /// The SFU dropped the participant without being told to (media or WebSocket gone).
    Lost,
    /// The participant started speaking.
    Active,
}

/// Something the gateway should hear about: a participant the SFU lost without being told to
/// drop it (its WebSocket never came back within the resume window, or its media connection
/// died), so the gateway clears the user's voice state; or one who started speaking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SfuEvent {
    pub kind: SfuEventKind,
    pub channel: ChannelId,
    pub user: UserId,
}

/// Cheap clone: the way into a running SFU.
#[derive(Clone)]
pub struct SfuHandle {
    shards: Arc<Vec<mpsc::Sender<Cmd>>>,
    shared: Arc<Shared>,
    live: Arc<LiveStats>,
    signer: TokenSigner,
    cfg: Arc<SfuConfig>,
    conn_ids: Arc<AtomicU64>,
    udp_ports: Arc<Vec<u16>>,
    tcp_port: Option<u16>,
    events: tokio::sync::broadcast::Sender<SfuEvent>,
}

/// Counts the admin dashboard reads without asking the SFU threads: refreshed once a second.
#[derive(Debug, Default)]
pub struct LiveStats {
    rooms: AtomicU64,
    egress_bytes: AtomicU64,
}

impl LiveStats {
    /// Voice rooms with at least one participant right now.
    pub fn rooms(&self) -> u64 {
        self.rooms.load(Ordering::Relaxed)
    }

    /// Bytes sent to participants since the SFU started (the egress rate integrated per second).
    pub fn egress_bytes(&self) -> u64 {
        self.egress_bytes.load(Ordering::Relaxed)
    }
}

/// State the shards share. Everything here is touched on joins, ICE-TCP connects and the
/// once-a-second tick, never per media packet.
struct Shared {
    /// The shard each user last joined, so a join on another shard can evict the old one.
    user_shard: Mutex<HashMap<UserId, usize>>,
    /// ICE-TCP peers already matched to a shard (the first STUN packet is offered to all).
    tcp_shard: Mutex<HashMap<SocketAddr, usize>>,
    /// Participants on the node, for `max_participants`.
    total: AtomicUsize,
    /// Each shard's latest snapshot; the metrics publish their sum.
    snaps: Mutex<Vec<Snapshot>>,
}

fn locked<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Which shard owns a channel. A multiplicative hash keeps the spread even whatever the id
/// scheme (snowflakes differ mostly in their high bits within one guild).
fn shard_for_channel(channel: ChannelId, shards: usize) -> usize {
    let h = channel.get().wrapping_mul(0x9E37_79B9_7F4A_7C15);
    usize::try_from(h >> 32).unwrap_or(0) % shards.max(1)
}

/// The shard that issued a participant id (see [`SfuCore::with_id_space`]).
fn shard_for_pid(pid: ParticipantId, shards: usize) -> usize {
    usize::try_from(pid.0.saturating_sub(1)).unwrap_or(0) % shards.max(1)
}

/// How many actors `SfuConfig::shards` asks for, given the sockets that could be bound.
fn effective_shards(want: usize, sockets: usize) -> usize {
    let want = if want == 0 {
        std::thread::available_parallelism().map_or(1, |n| (n.get() / 2).clamp(1, 8))
    } else {
        want
    };
    want.min(sockets).max(1)
}

enum Target {
    One(usize),
    All,
}

impl SfuHandle {
    /// Counts for the admin dashboard; cheap to read from any thread.
    pub fn live_stats(&self) -> Arc<LiveStats> {
        self.live.clone()
    }

    /// Subscribe to [`SfuEvent`]s.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<SfuEvent> {
        self.events.subscribe()
    }

    pub fn signer(&self) -> &TokenSigner {
        &self.signer
    }

    pub fn config(&self) -> &SfuConfig {
        &self.cfg
    }

    /// UDP ports actually bound.
    pub fn udp_ports(&self) -> &[u16] {
        &self.udp_ports
    }

    /// ICE-TCP port actually bound.
    pub fn tcp_port(&self) -> Option<u16> {
        self.tcp_port
    }

    /// Media actors running (the configured `shards` after the socket limit).
    pub fn shard_count(&self) -> usize {
        self.shards.len()
    }

    /// The shard (media actor) that hosts a channel's room.
    pub fn shard_of_channel(&self, channel: ChannelId) -> usize {
        shard_for_channel(channel, self.shards.len())
    }

    pub(crate) fn next_conn_id(&self) -> u64 {
        self.conn_ids.fetch_add(1, Ordering::Relaxed)
    }

    fn target(&self, cmd: &Cmd) -> Target {
        let n = self.shards.len();
        match cmd {
            Cmd::Join { claims, .. } => Target::One(shard_for_channel(claims.channel_id, n)),
            // The shard is the first byte of the session id; anything else is refused by
            // shard 0 as an unknown session.
            Cmd::Resume { session_id, .. } => Target::One(
                session_id
                    .get(..2)
                    .and_then(|h| usize::from_str_radix(h, 16).ok())
                    .filter(|s| *s < n)
                    .unwrap_or(0),
            ),
            Cmd::Offer { pid, .. }
            | Cmd::Answer { pid, .. }
            | Cmd::Candidate { pid, .. }
            | Cmd::Speaking { pid, .. }
            | Cmd::Video { pid, .. }
            | Cmd::WsLost { pid, .. } => Target::One(shard_for_pid(*pid, n)),
            Cmd::Control(c) => Target::One(shard_for_channel(control_channel(c), n)),
            // Sent straight to the old shard by the join below, never routed.
            Cmd::Evict { .. } => Target::One(0),
            Cmd::TcpOpen { .. } | Cmd::TcpClosed { .. } | Cmd::Drain | Cmd::Shutdown(_) => {
                Target::All
            }
        }
    }

    /// Route a command to the shard that owns what it is about.
    pub(crate) async fn send(&self, cmd: Cmd) -> Result<(), ()> {
        match self.target(&cmd) {
            Target::One(i) => {
                if let Cmd::Join { claims, .. } = &cmd {
                    let prev = locked(&self.shared.user_shard).insert(claims.user_id, i);
                    if let Some(old) = prev.filter(|o| *o != i) {
                        let _ = self.shards[old]
                            .send(Cmd::Evict {
                                user: claims.user_id,
                            })
                            .await;
                    }
                }
                self.shards[i].send(cmd).await.map_err(|_| ())
            }
            Target::All => {
                for tx in self.shards.iter() {
                    let copy = match &cmd {
                        Cmd::Evict { user } => Cmd::Evict { user: *user },
                        Cmd::TcpOpen { peer, tx } => Cmd::TcpOpen {
                            peer: *peer,
                            tx: tx.clone(),
                        },
                        Cmd::TcpClosed { peer } => Cmd::TcpClosed { peer: *peer },
                        _ => Cmd::Drain,
                    };
                    tx.send(copy).await.map_err(|_| ())?;
                }
                Ok(())
            }
        }
    }

    /// Tell every client to reconnect elsewhere (close 4015) and refuse new joins
    /// (VOX-SFU-010). Used before a restart.
    pub async fn drain(&self) {
        let _ = self.send(Cmd::Drain).await;
    }

    /// Close every room and stop the actors.
    pub async fn shutdown(&self) {
        for tx in self.shards.iter() {
            let (done, rx) = oneshot::channel();
            if tx.send(Cmd::Shutdown(done)).await.is_ok() {
                let _ = rx.await;
            }
        }
    }
}

fn control_channel(c: &VoiceCommand) -> ChannelId {
    match c {
        VoiceCommand::SetFlags { channel, .. }
        | VoiceCommand::Disconnect { channel, .. }
        | VoiceCommand::StreamViewers { channel, .. }
        | VoiceCommand::E2eeKeyed { channel, .. }
        | VoiceCommand::CloseRoom { channel } => *channel,
    }
}

#[async_trait]
impl VoiceRelay for SfuHandle {
    async fn apply(&self, command: VoiceCommand) {
        let _ = self.send(Cmd::Control(command)).await;
    }
}

/// Bind the sockets and start the actors.
pub async fn start(cfg: SfuConfig) -> Result<SfuHandle, StartError> {
    if cfg.token_secret.is_empty() {
        return Err(StartError::NoSecret);
    }
    let Some(announce0) = cfg.announce_ips.first().copied() else {
        return Err(StartError::NoAnnounce);
    };
    metrics::describe();

    let mut socks = Vec::new();
    let mut ports = Vec::new();
    for port in cfg.udp_port_min..=cfg.udp_port_max {
        if socks.len() >= MAX_UDP_SOCKETS {
            break;
        }
        match UdpSocket::bind((cfg.bind_ip, port)).await {
            Ok(s) => {
                let actual = s.local_addr().map_or(port, |a| a.port());
                ports.push(actual);
                socks.push(Arc::new(s));
            }
            Err(e) => tracing::warn!(port, error = %e, "voice UDP port unavailable"),
        }
    }
    if socks.is_empty() {
        return Err(StartError::NoUdpPort(cfg.udp_port_min, cfg.udp_port_max));
    }
    let n = effective_shards(cfg.shards, socks.len());

    let mut pkt_txs = Vec::with_capacity(n);
    let mut cmd_txs = Vec::with_capacity(n);
    let mut inboxes = Vec::with_capacity(n);
    for _ in 0..n {
        let (pkt_tx, pkt_rx) = mpsc::channel::<Packet>(8192);
        let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>(1024);
        pkt_txs.push(pkt_tx);
        cmd_txs.push(cmd_tx);
        inboxes.push((cmd_rx, pkt_rx));
    }

    // Socket i belongs to shard i % n, so every shard gets a disjoint, even slice and the
    // candidates it hands out always lead back to it.
    let mut shard_socks: Vec<Vec<Arc<UdpSocket>>> = vec![Vec::new(); n];
    let mut shard_ports: Vec<Vec<u16>> = vec![Vec::new(); n];
    for (i, (s, port)) in socks.iter().zip(&ports).enumerate() {
        let k = i % n;
        shard_socks[k].push(s.clone());
        shard_ports[k].push(*port);
        tokio::spawn(udp_reader(
            s.clone(),
            SocketAddr::new(announce0, *port),
            pkt_txs[k].clone(),
        ));
    }

    let shared = Arc::new(Shared {
        user_shard: Mutex::new(HashMap::new()),
        tcp_shard: Mutex::new(HashMap::new()),
        total: AtomicUsize::new(0),
        snaps: Mutex::new(vec![Snapshot::default(); n]),
    });

    let mut tcp_port = None;
    if let Some(p) = cfg.tcp_port {
        let l = TcpListener::bind((cfg.bind_ip, p))
            .await
            .map_err(StartError::Tcp)?;
        let actual = l.local_addr().map_err(StartError::Tcp)?.port();
        tcp_port = Some(actual);
        tokio::spawn(tcp_acceptor(
            l,
            SocketAddr::new(announce0, actual),
            TcpRoute {
                pkts: pkt_txs.clone(),
                cmds: cmd_txs.clone(),
                shared: shared.clone(),
            },
        ));
    }

    let cfg = Arc::new(cfg);
    let (events, _) = tokio::sync::broadcast::channel(256);
    let live = Arc::new(LiveStats::default());
    let handle = SfuHandle {
        shards: Arc::new(cmd_txs),
        shared: shared.clone(),
        live: live.clone(),
        signer: TokenSigner::new(cfg.token_secret.clone()),
        cfg: cfg.clone(),
        conn_ids: Arc::new(AtomicU64::new(1)),
        udp_ports: Arc::new(ports.clone()),
        tcp_port,
        events: events.clone(),
    };
    for (k, ((cmd_rx, pkt_rx), (socks, ports))) in inboxes
        .into_iter()
        .zip(shard_socks.into_iter().zip(shard_ports))
        .enumerate()
    {
        // The egress budget is the node's; each shard polices its even share of it.
        let budget = cfg.egress_budget_mbps / n as f64;
        let actor = Actor {
            core: SfuCore::new(cfg.max_participants, cfg.max_room_size)
                .with_id_space(k as u64, n as u64)
                .with_bwe(cfg.video_bwe)
                .with_governor(crate::governor::GovernorConfig::with_budget_mbps(budget)),
            cfg: cfg.clone(),
            shard: k,
            shards: n,
            shared: shared.clone(),
            socks,
            port_idx: ports.iter().enumerate().map(|(i, p)| (*p, i)).collect(),
            ports,
            tcp_port,
            sessions: HashMap::new(),
            by_session: HashMap::new(),
            tcp: HashMap::new(),
            next_sock: 0,
            draining: false,
            last_counts: (0, 0, 0, 0),
            pending_flags: HashMap::new(),
            pending_keyed: HashMap::new(),
            events: events.clone(),
            live: live.clone(),
        };
        tokio::spawn(actor.run(cmd_rx, pkt_rx));
    }
    Ok(handle)
}

async fn udp_reader(sock: Arc<UdpSocket>, dst: SocketAddr, tx: mpsc::Sender<Packet>) {
    let mut buf = vec![0u8; 2048];
    // Datagrams dropped since the last warning: a full queue means the shard cannot keep up, and a
    // newcomer's ICE checks are lost with the media (RDY-VOX-002).
    let mut dropped: u64 = 0;
    let mut warned: Option<Instant> = None;
    loop {
        match sock.recv_from(&mut buf).await {
            Ok((n, src)) => {
                // A full queue drops the datagram: UDP semantics, and better than unbounded memory.
                let sent = tx.try_send(Packet {
                    proto: Protocol::Udp,
                    src,
                    dst,
                    data: buf[..n].to_vec(),
                });
                if matches!(sent, Err(mpsc::error::TrySendError::Full(_))) {
                    dropped += 1;
                    let now = Instant::now();
                    if warned.is_none_or(|t| now.saturating_duration_since(t) >= DROP_WARN_EVERY) {
                        tracing::warn!(
                            port = dst.port(),
                            dropped,
                            "voice packet queue full: datagrams dropped"
                        );
                        warned = Some(now);
                        dropped = 0;
                    }
                }
            }
            // Windows reports ICMP port-unreachable as a recv error on UDP sockets.
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => {}
            Err(e) => {
                tracing::error!(error = %e, "voice UDP socket failed");
                break;
            }
        }
    }
}

/// At most one "packet queue full" warning per UDP socket this often.
const DROP_WARN_EVERY: std::time::Duration = std::time::Duration::from_secs(5);

/// How long a flag change for a not-yet-joined participant is remembered (the token lifetime).
const PENDING_FLAGS_TTL: std::time::Duration = std::time::Duration::from_secs(60);
/// Most keyed notices held for members who have not joined yet.
const MAX_PENDING_KEYED: usize = 8192;

/// Largest RFC 4571 frame accepted (a WebRTC datagram never exceeds the MTU).
const TCP_MAX_FRAME: usize = 2048;
/// An ICE-TCP connection that stays silent this long is dropped.
const TCP_IDLE: std::time::Duration = std::time::Duration::from_secs(30);

/// Where ICE-TCP traffic goes: every shard hears about connections (any may write to them),
/// and each datagram goes to the shard that accepted this peer's first packet, or to all of
/// them until one does.
#[derive(Clone)]
struct TcpRoute {
    pkts: Vec<mpsc::Sender<Packet>>,
    cmds: Vec<mpsc::Sender<Cmd>>,
    shared: Arc<Shared>,
}

impl TcpRoute {
    fn deliver(&self, peer: SocketAddr, dst: SocketAddr, data: Vec<u8>) {
        let known = if self.pkts.len() == 1 {
            Some(0)
        } else {
            locked(&self.shared.tcp_shard).get(&peer).copied()
        };
        let packet = |data| Packet {
            proto: Protocol::Tcp,
            src: peer,
            dst,
            data,
        };
        match known {
            Some(i) => {
                let _ = self.pkts[i].try_send(packet(data));
            }
            None => {
                for tx in &self.pkts {
                    let _ = tx.try_send(packet(data.clone()));
                }
            }
        }
    }
}

async fn tcp_acceptor(l: TcpListener, dst: SocketAddr, route: TcpRoute) {
    loop {
        let Ok((stream, peer)) = l.accept().await else {
            continue;
        };
        let _ = stream.set_nodelay(true);
        let (mut rd, mut wr) = stream.into_split();
        let (wtx, mut wrx) = mpsc::channel::<Vec<u8>>(256);
        for c in &route.cmds {
            if c.send(Cmd::TcpOpen {
                peer,
                tx: wtx.clone(),
            })
            .await
            .is_err()
            {
                return;
            }
        }
        drop(wtx);
        tokio::spawn(async move {
            while let Some(frame) = wrx.recv().await {
                let Ok(len) = u16::try_from(frame.len()) else {
                    continue;
                };
                if wr.write_all(&len.to_be_bytes()).await.is_err()
                    || wr.write_all(&frame).await.is_err()
                {
                    break;
                }
            }
        });
        let route = route.clone();
        tokio::spawn(async move {
            loop {
                let mut len = [0u8; 2];
                let read = async {
                    rd.read_exact(&mut len).await?;
                    let n = usize::from(u16::from_be_bytes(len));
                    if n == 0 || n > TCP_MAX_FRAME {
                        return Err(std::io::ErrorKind::InvalidData.into());
                    }
                    let mut data = vec![0u8; n];
                    rd.read_exact(&mut data).await?;
                    Ok::<_, std::io::Error>(data)
                };
                match tokio::time::timeout(TCP_IDLE, read).await {
                    Ok(Ok(data)) => route.deliver(peer, dst, data),
                    _ => break,
                }
            }
            locked(&route.shared.tcp_shard).remove(&peer);
            for c in &route.cmds {
                let _ = c.send(Cmd::TcpClosed { peer }).await;
            }
        });
    }
}

struct Session {
    session_id: String,
    resume_token: String,
    conn: Option<ConnTx>,
    lost_at: Option<Instant>,
}

struct Actor {
    core: SfuCore,
    cfg: Arc<SfuConfig>,
    shard: usize,
    shards: usize,
    shared: Arc<Shared>,
    socks: Vec<Arc<UdpSocket>>,
    ports: Vec<u16>,
    port_idx: HashMap<u16, usize>,
    tcp_port: Option<u16>,
    sessions: HashMap<ParticipantId, Session>,
    by_session: HashMap<String, ParticipantId>,
    tcp: HashMap<SocketAddr, mpsc::Sender<Vec<u8>>>,
    next_sock: usize,
    draining: bool,
    last_counts: (u64, u64, u64, u64),
    /// Flag changes that arrived before the participant joined (a moderator muted someone
    /// between `VOICE_SERVER_UPDATE` and their connection). Applied at join, dropped after
    /// the token lifetime.
    pending_flags: HashMap<(ChannelId, UserId), (Flags, Instant)>,
    /// E2EE-keyed notices that arrived before the member joined the SFU (OPS-IMM-001), with their time.
    pending_keyed: HashMap<(ChannelId, UserId), Instant>,
    events: tokio::sync::broadcast::Sender<SfuEvent>,
    live: Arc<LiveStats>,
}

/// The node-wide picture from every shard's last snapshot (what the gauges show).
fn merge_snapshots(all: &[Snapshot]) -> Snapshot {
    let participants: usize = all.iter().map(|s| s.participants).sum();
    let loss: f64 = all
        .iter()
        .map(|s| s.loss_ratio * s.participants as f64)
        .sum();
    Snapshot {
        rooms: all.iter().map(|s| s.rooms).sum(),
        participants,
        egress_bps: all.iter().map(|s| s.egress_bps).sum(),
        ingress_bps: all.iter().map(|s| s.ingress_bps).sum(),
        loss_ratio: if participants == 0 {
            0.0
        } else {
            loss / participants as f64
        },
        forwarded_packets: all.iter().map(|s| s.forwarded_packets).sum(),
        forwarded_frames: all.iter().map(|s| s.forwarded_frames).sum(),
        governor_severity: all.iter().map(|s| s.governor_severity).max().unwrap_or(0),
        keyframe_requests: all.iter().map(|s| s.keyframe_requests).sum(),
        unkeyed_frames: all.iter().map(|s| s.unkeyed_frames).sum(),
    }
}

fn random_hex() -> String {
    let mut b = [0u8; 16];
    if getrandom::fill(&mut b).is_err() {
        // An OS RNG failure: fall back to the clock so ids stay unique; resume tokens are
        // additionally bound to a session id the attacker would have to guess.
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        b.copy_from_slice(&n.to_le_bytes());
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Compare secrets without an early exit.
fn ct_eq(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

fn close_code(reason: CloseReason) -> (u16, &'static str) {
    match reason {
        CloseReason::Left | CloseReason::Disconnected => (close::DISCONNECTED, "disconnected"),
        CloseReason::RoomClosed => (close::DISCONNECTED, "room closed"),
        CloseReason::Replaced => (close::SESSION_REPLACED, "replaced"),
        CloseReason::Connection => (close::SESSION_INVALID, "media connection lost"),
    }
}

impl Actor {
    async fn run(mut self, mut cmds: mpsc::Receiver<Cmd>, mut pkts: mpsc::Receiver<Packet>) {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            let now = Instant::now();
            let wait = self
                .core
                .next_timeout()
                .map_or(std::time::Duration::from_millis(50), |t| {
                    t.saturating_duration_since(now)
                })
                .clamp(
                    std::time::Duration::from_millis(1),
                    std::time::Duration::from_millis(50),
                );
            tokio::select! {
                cmd = cmds.recv() => match cmd {
                    Some(Cmd::Shutdown(done)) => {
                        self.core.close_all();
                        self.drain_output();
                        let _ = done.send(());
                        return;
                    }
                    Some(c) => self.on_cmd(c),
                    None => return,
                },
                Some(p) = pkts.recv() => {
                    self.input(&p);
                    // Batch whatever else is already queued, then run the room once for all of it.
                    for _ in 0..64 {
                        let Ok(p) = pkts.try_recv() else { break };
                        self.input(&p);
                    }
                    self.core.flush(Instant::now());
                }
                () = tokio::time::sleep(wait) => self.core.handle_timeout(Instant::now()),
                _ = tick.tick() => self.on_tick(),
            }
            self.drain_output();
        }
    }

    fn input(&mut self, p: &Packet) {
        let taken = self
            .core
            .queue_input(Instant::now(), p.proto, p.src, p.dst, &p.data);
        // An ICE-TCP peer is offered to every shard until one's participant accepts it.
        if taken && p.proto == Protocol::Tcp && self.shards > 1 {
            locked(&self.shared.tcp_shard).insert(p.src, self.shard);
        }
    }

    fn on_tick(&mut self) {
        let now = Instant::now();
        let own = self.core.snapshot();
        let snap = {
            let mut all = locked(&self.shared.snaps);
            all[self.shard] = own;
            merge_snapshots(&all)
        };
        // Counter deltas stay this shard's own; the gauges carry the node total.
        let snap = Snapshot {
            forwarded_packets: own.forwarded_packets,
            forwarded_frames: own.forwarded_frames,
            keyframe_requests: own.keyframe_requests,
            unkeyed_frames: own.unkeyed_frames,
            ..snap
        };
        let counts = (
            snap.forwarded_packets,
            snap.forwarded_frames,
            snap.keyframe_requests,
            snap.unkeyed_frames,
        );
        let deltas = metrics::Deltas {
            packets: counts.0.saturating_sub(self.last_counts.0),
            frames: counts.1.saturating_sub(self.last_counts.1),
            keyframes: counts.2.saturating_sub(self.last_counts.2),
            unkeyed: counts.3.saturating_sub(self.last_counts.3),
        };
        self.last_counts = counts;
        metrics::publish(&snap, deltas);
        self.live.rooms.store(snap.rooms as u64, Ordering::Relaxed);
        // One tick a second: the egress rate in bits per second is bytes per tick divided by 8.
        self.live
            .egress_bytes
            .fetch_add((own.egress_bps.max(0.0) / 8.0) as u64, Ordering::Relaxed);
        self.pending_flags
            .retain(|_, (_, at)| now.saturating_duration_since(*at) <= PENDING_FLAGS_TTL);
        self.pending_keyed
            .retain(|_, at| now.saturating_duration_since(*at) <= PENDING_FLAGS_TTL);
        // Participants whose WebSocket stayed away past the resume window are dropped.
        let expired: Vec<_> = self
            .sessions
            .iter()
            .filter(|(_, s)| {
                s.lost_at
                    .is_some_and(|t| now.saturating_duration_since(t) > self.cfg.resume_window)
            })
            .map(|(p, _)| *p)
            .collect();
        for pid in expired {
            tracing::debug!(pid = pid.0, "voice WebSocket not resumed within the window");
            self.core.leave(pid, CloseReason::Left);
        }
    }

    fn send_ws(&mut self, pid: ParticipantId, out: WsOut) {
        let Some(s) = self.sessions.get_mut(&pid) else {
            return;
        };
        let Some(c) = &s.conn else { return };
        if c.tx.try_send(out).is_err() {
            // Slow or gone: treat as a lost socket; the client resumes and re-syncs.
            s.conn = None;
            s.lost_at = Some(Instant::now());
        }
    }

    fn candidates(&mut self) -> Vec<Candidate> {
        let idx = self.next_sock % self.ports.len();
        self.next_sock = self.next_sock.wrapping_add(1);
        let port = self.ports[idx];
        let mut out = Vec::new();
        for ip in &self.cfg.announce_ips {
            if let Ok(c) = Candidate::host(SocketAddr::new(*ip, port), "udp") {
                out.push(c);
            }
            if let Some(tp) = self.tcp_port
                && let Ok(c) = Candidate::builder()
                    .tcp()
                    .host(SocketAddr::new(*ip, tp))
                    .tcptype(TcpType::Passive)
                    .build()
            {
                out.push(c);
            }
        }
        out
    }

    fn on_cmd(&mut self, cmd: Cmd) {
        let now = Instant::now();
        match cmd {
            Cmd::Join {
                claims,
                conn,
                reply,
            } => {
                let (user, channel) = (claims.user_id, claims.channel_id);
                let res = self.join(now, claims, conn);
                match &res {
                    Ok(j) => {
                        tracing::debug!(%user, %channel, pid = j.pid.0, shard = self.shard, "voice participant joined")
                    }
                    Err(code) => tracing::debug!(%user, %channel, code, "voice join refused"),
                }
                let _ = reply.send(res);
            }
            Cmd::Resume {
                session_id,
                resume_token,
                conn,
                reply,
            } => {
                let res = self.resume(now, &session_id, &resume_token, conn);
                match &res {
                    Ok(j) => tracing::debug!(pid = j.pid.0, "voice session resumed"),
                    Err(code) => tracing::debug!(code, "voice resume refused"),
                }
                let _ = reply.send(res);
            }
            Cmd::Offer { pid, conn_id, sdp } => {
                if !self.is_current(pid, conn_id) {
                    return;
                }
                match self.core.handle_offer(now, pid, &sdp) {
                    // Sent before the core's own follow-up offer is drained, so the client sees
                    // the answer first.
                    Ok(answer) => {
                        tracing::debug!(pid = pid.0, "voice client offer answered");
                        self.send_ws(
                            pid,
                            WsOut::Msg(Message::new(op::SDP_ANSWER, wire::Sdp { sdp: answer })),
                        );
                    }
                    Err(e) => {
                        tracing::debug!(error = %e, "rejected client offer");
                        self.send_ws(pid, WsOut::Close(close::DECODE_ERROR, "bad offer"));
                    }
                }
            }
            Cmd::Answer { pid, conn_id, sdp } => {
                if self.is_current(pid, conn_id) {
                    match self.core.handle_answer(now, pid, &sdp) {
                        Ok(()) => tracing::debug!(pid = pid.0, "voice client answer applied"),
                        Err(e) => {
                            tracing::debug!(error = %e, "rejected client answer");
                            self.send_ws(pid, WsOut::Close(close::DECODE_ERROR, "bad answer"));
                        }
                    }
                }
            }
            Cmd::Candidate { pid, conn_id, line } => {
                if self.is_current(pid, conn_id) {
                    // Harmless when it fails: ICE-lite learns the client's address from its checks.
                    let _ = self.core.add_remote_candidate(pid, &line);
                }
            }
            Cmd::Speaking { pid, conn_id, bits } => {
                if self.is_current(pid, conn_id) {
                    self.core.set_client_speaking(now, pid, bits);
                }
            }
            Cmd::Video { pid, conn_id, msg } => {
                if self.is_current(pid, conn_id)
                    && let Err(e) = self.core.handle_video(now, pid, *msg)
                {
                    tracing::debug!(error = %e, "rejected video message");
                    self.send_ws(pid, WsOut::Close(close::DECODE_ERROR, "bad video message"));
                }
            }
            Cmd::WsLost { pid, conn_id } => {
                if let Some(s) = self.sessions.get_mut(&pid)
                    && s.conn.as_ref().is_some_and(|c| c.id == conn_id)
                {
                    s.conn = None;
                    s.lost_at = Some(now);
                    tracing::debug!(
                        pid = pid.0,
                        resume_window_s = self.cfg.resume_window.as_secs(),
                        "voice WebSocket lost; waiting for a resume"
                    );
                }
            }
            Cmd::Control(c) => self.control(now, c),
            Cmd::TcpOpen { peer, tx } => {
                self.tcp.insert(peer, tx);
            }
            Cmd::TcpClosed { peer } => {
                self.tcp.remove(&peer);
            }
            Cmd::Evict { user } => {
                if let Some(pid) = self.core.participant_of(user) {
                    self.core.leave(pid, CloseReason::Replaced);
                }
            }
            Cmd::Drain => {
                self.draining = true;
                let pids: Vec<_> = self.sessions.keys().copied().collect();
                for pid in pids {
                    self.send_ws(
                        pid,
                        WsOut::Close(close::SERVER_RESTARTING, "server restarting"),
                    );
                }
            }
            Cmd::Shutdown(_) => {}
        }
    }

    fn is_current(&self, pid: ParticipantId, conn_id: u64) -> bool {
        self.sessions
            .get(&pid)
            .and_then(|s| s.conn.as_ref())
            .is_some_and(|c| c.id == conn_id)
    }

    fn join(&mut self, now: Instant, c: JoinClaims, conn: ConnTx) -> Result<Joined, u16> {
        if self.draining {
            return Err(close::SERVER_RESTARTING);
        }
        // The cap is the node's, not the shard's. A rejoin replaces the old participant, so
        // only a user who is not on this shard yet adds one.
        if self.core.participant_of(c.user_id).is_none()
            && self.shared.total.load(Ordering::Relaxed) >= self.cfg.max_participants
        {
            return Err(close::SERVER_FULL);
        }
        let candidates = self.candidates();
        let mut flags = Flags {
            speak: c.speak,
            deaf: c.deaf,
            ptt_required: c.ptt_required,
            priority: c.priority,
            camera: c.camera,
            screen: c.screen,
            tier: c.tier,
            video_full: c.video_quality == 2,
        };
        if let Some((f, at)) = self.pending_flags.remove(&(c.channel_id, c.user_id))
            && now.saturating_duration_since(at) <= PENDING_FLAGS_TTL
        {
            flags = f;
        }
        let e2ee_keyed = self
            .pending_keyed
            .remove(&(c.channel_id, c.user_id))
            .is_some_and(|at| now.saturating_duration_since(at) <= PENDING_FLAGS_TTL);
        let pid = self
            .core
            .join(
                now,
                JoinParams {
                    user: c.user_id,
                    guild: c.guild_id,
                    channel: c.channel_id,
                    flags,
                    e2ee_keyed,
                    candidates,
                },
            )
            .map_err(|_| close::SERVER_FULL)?;
        self.shared.total.fetch_add(1, Ordering::Relaxed);
        // The first byte names the shard so a RESUME finds its way back here.
        let session_id = format!("{:02x}{}", self.shard, random_hex());
        let resume_token = random_hex();
        self.by_session.insert(session_id.clone(), pid);
        self.sessions.insert(
            pid,
            Session {
                session_id: session_id.clone(),
                resume_token: resume_token.clone(),
                conn: Some(conn),
                lost_at: None,
            },
        );
        Ok(Joined {
            pid,
            ready: self.ready(
                pid,
                session_id,
                resume_token,
                c.user_id,
                c.channel_id,
                c.guild_id,
            ),
        })
    }

    fn ready(
        &self,
        pid: ParticipantId,
        session_id: String,
        resume_token: String,
        user: UserId,
        channel: ChannelId,
        guild: Option<sr_core::GuildId>,
    ) -> Ready {
        Ready {
            session_id,
            resume_token,
            heartbeat_interval: u64::try_from(self.cfg.heartbeat_interval.as_millis())
                .unwrap_or(13_750),
            user_id: user,
            channel_id: channel,
            guild_id: guild,
            participants: self.core.others(pid),
        }
    }

    fn resume(
        &mut self,
        now: Instant,
        session_id: &str,
        token: &str,
        conn: ConnTx,
    ) -> Result<Joined, u16> {
        let pid = *self
            .by_session
            .get(session_id)
            .ok_or(close::SESSION_INVALID)?;
        let s = self.sessions.get_mut(&pid).ok_or(close::SESSION_INVALID)?;
        if !ct_eq(&s.resume_token, token) {
            return Err(close::SESSION_INVALID);
        }
        // A still-attached older socket loses.
        if let Some(old) = s.conn.replace(conn) {
            let _ = old
                .tx
                .try_send(WsOut::Close(close::SESSION_REPLACED, "resumed elsewhere"));
        }
        s.lost_at = None;
        let (sid, rt) = (s.session_id.clone(), s.resume_token.clone());
        let (Some(user), Some(channel)) = (self.core.user_of(pid), self.core.channel_of(pid))
        else {
            return Err(close::SESSION_INVALID);
        };
        // Whatever offer was in flight is gone with the old socket; make it again.
        self.core.resend_pending_offer(pid);
        self.core.handle_timeout(now);
        let guild = self.core.guild_of(pid);
        Ok(Joined {
            pid,
            ready: self.ready(pid, sid, rt, user, channel, guild),
        })
    }

    fn control(&mut self, now: Instant, c: VoiceCommand) {
        match c {
            VoiceCommand::SetFlags {
                channel,
                user,
                speak,
                deaf,
                ptt_required,
                priority,
                camera,
                screen,
            } => {
                // The tier is fixed at join; a level change applies from the next join.
                let (tier, video_full) = self
                    .core
                    .participant_of(user)
                    .and_then(|p| self.core.flags_of(p))
                    .map_or((0, false), |f| (f.tier, f.video_full));
                let flags = Flags {
                    speak,
                    deaf,
                    ptt_required,
                    priority,
                    camera,
                    screen,
                    tier,
                    video_full,
                };
                if !self.core.set_flags(now, channel, user, flags) {
                    self.pending_flags.insert((channel, user), (flags, now));
                }
            }
            VoiceCommand::Disconnect { channel, user } => {
                self.pending_flags.remove(&(channel, user));
                self.pending_keyed.remove(&(channel, user));
                self.core
                    .disconnect_user(channel, user, CloseReason::Disconnected);
            }
            VoiceCommand::StreamViewers {
                channel,
                owner,
                viewers,
                live,
            } => self
                .core
                .set_stream_viewers(now, channel, owner, viewers, live),
            VoiceCommand::E2eeKeyed {
                channel,
                user,
                keyed,
            } => {
                if !self.core.set_e2ee_keyed(now, channel, user, keyed) {
                    // Said before the member reached the SFU (or after they left): remember it for the join.
                    if keyed {
                        if self.pending_keyed.len() < MAX_PENDING_KEYED {
                            self.pending_keyed.insert((channel, user), now);
                        }
                    } else {
                        self.pending_keyed.remove(&(channel, user));
                    }
                }
            }
            VoiceCommand::CloseRoom { channel } => {
                self.pending_keyed.retain(|(c, _), _| *c != channel);
                self.core.close_room(channel);
            }
        }
    }

    /// Flush everything the core produced: datagrams to sockets, signals to WebSockets.
    fn drain_output(&mut self) {
        while let Some(out) = self.core.poll() {
            match out {
                CoreOutput::Transmit(p) => match p.proto {
                    Protocol::Udp => {
                        if let Some(i) = self.port_idx.get(&p.source.port()) {
                            // Non-blocking: a full socket buffer drops the datagram.
                            let _ = self.socks[*i].try_send_to(&p.data, p.destination);
                        }
                    }
                    Protocol::Tcp => {
                        if let Some(tx) = self.tcp.get(&p.destination) {
                            let _ = tx.try_send(p.data);
                        }
                    }
                    _ => {}
                },
                CoreOutput::Event(CoreEvent::Signal { to, msg }) => {
                    self.send_ws(to, WsOut::Msg(msg));
                }
                CoreOutput::Event(CoreEvent::Active { user, channel }) => {
                    let _ = self.events.send(SfuEvent {
                        kind: SfuEventKind::Active,
                        channel,
                        user,
                    });
                }
                CoreOutput::Event(CoreEvent::Closed {
                    pid,
                    reason,
                    user,
                    channel,
                }) => {
                    let _ =
                        self.shared
                            .total
                            .try_update(Ordering::Relaxed, Ordering::Relaxed, |t| {
                                Some(t.saturating_sub(1))
                            });
                    // A user evicted by a join elsewhere has a newer entry; leave it be.
                    if !matches!(reason, CloseReason::Replaced) {
                        let mut map = locked(&self.shared.user_shard);
                        if map.get(&user) == Some(&self.shard) {
                            map.remove(&user);
                        }
                    }
                    // Not asked for by the gateway: tell it, so it clears the voice state.
                    if matches!(reason, CloseReason::Left | CloseReason::Connection) {
                        tracing::info!(
                            %user,
                            %channel,
                            pid = pid.0,
                            ?reason,
                            "voice participant lost; the gateway clears its voice state"
                        );
                    } else {
                        tracing::debug!(%user, %channel, pid = pid.0, ?reason, "voice participant closed");
                    }
                    if matches!(reason, CloseReason::Left | CloseReason::Connection) {
                        let _ = self.events.send(SfuEvent {
                            kind: SfuEventKind::Lost,
                            channel,
                            user,
                        });
                    }
                    if let Some(s) = self.sessions.remove(&pid) {
                        self.by_session.remove(&s.session_id);
                        if let Some(c) = s.conn {
                            let (code, why) = close_code(reason);
                            let _ = c.tx.try_send(WsOut::Close(code, why));
                        }
                    }
                }
            }
        }
    }
}

/// Resolve the addresses a client may reach, for logs and tests.
pub fn advertised(cfg: &SfuConfig, ports: &[u16]) -> Vec<SocketAddr> {
    ports
        .iter()
        .flat_map(|p| {
            cfg.announce_ips
                .iter()
                .map(move |ip: &IpAddr| SocketAddr::new(*ip, *p))
        })
        .collect()
}
