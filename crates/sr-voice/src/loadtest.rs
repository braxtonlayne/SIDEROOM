//! Load test harness for the SFU core (VOX-SFU-011): headless publishers and viewers, all real
//! str0m clients, connected to one [`SfuCore`] through a simulated network.
//!
//! # What it measures
//!
//! Only the time spent **inside the core** (`handle_input`, `handle_timeout`, draining
//! output) is timed; the clients' own work is excluded. Together with the bytes the core
//! transmitted that gives:
//!
//! * `egress_mbps`: what the core sent to viewers, per simulated second;
//! * `core_cpu_fraction`: core time per simulated second, i.e. the share of **one core** the
//!   media path needs to keep up (the core is single threaded, one actor per node today);
//! * `cpu_fraction_at_target`: the same scaled linearly to the 300 Mbps target of the spec
//!   (section 4.8): the single-core share needed for 300 Mbit/s of egress.
//!
//! Real sockets, kernel UDP cost, encryption on the wire at line rate and real NICs are not in
//! the loop (SRTP encryption is: str0m does it inside the core), so the number is a lower
//! bound on a real box. Run it in release mode on the target hardware:
//!
//! ```text
//! cargo run --release -p sr-voice --example loadtest -- --publishers 5 --viewers 40 --seconds 10
//! ```
//!
//! The defaults give about 300 Mbit/s of egress: 5 publishers at 1.5 Mbit/s full layer, 40
//! viewers each watching all five at the full layer.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use sr_core::{ChannelId, UserId};
use sr_voice_proto::video::{Layer, Source, SubscriptionHint, Video};
use sr_voice_proto::wire::{self, op};
use str0m::change::{SdpAnswer, SdpOffer};
use str0m::format::Codec;
use str0m::media::{
    Direction, Frequency, MediaKind, MediaTime, Mid, Rid, Simulcast, SimulcastLayer,
};
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, Input, Output, Rtc};

use crate::core::{CoreEvent, CoreOutput, Flags, JoinParams, ParticipantId, SfuCore};

const SFU_ADDR: &str = "10.0.0.1:50000";
const CHANNEL: u64 = 9_000;
const RIDS: [&str; 3] = ["q", "h", "f"];
const STEP: Duration = Duration::from_millis(5);
const FPS: u64 = 30;

#[derive(Debug, Clone)]
pub struct LoadConfig {
    pub publishers: u32,
    pub viewers: u32,
    /// Bits per second of each simulcast layer a publisher sends (quarter, half, full).
    pub layer_bps: [u64; 3],
    /// The layer every viewer asks for.
    pub viewer_layer: Layer,
    /// Publishers also talk (Opus-sized frames at 50 per second).
    pub audio: bool,
    /// Simulated time before measuring (connect, negotiate, layer upgrades settle).
    pub warmup: Duration,
    /// Simulated time measured.
    pub measure: Duration,
}

impl Default for LoadConfig {
    fn default() -> Self {
        Self {
            publishers: 5,
            viewers: 40,
            layer_bps: [150_000, 500_000, 1_500_000],
            viewer_layer: Layer::High,
            audio: true,
            warmup: Duration::from_secs(8),
            measure: Duration::from_secs(10),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Report {
    pub participants: usize,
    /// Simulated seconds measured.
    pub seconds: f64,
    pub egress_mbps: f64,
    pub ingress_mbps: f64,
    /// Real seconds the core spent working during the measured interval.
    pub core_seconds: f64,
    /// `core_seconds / seconds`: share of one core.
    pub core_cpu_fraction: f64,
    /// The same, scaled to 300 Mbit/s of egress (0 when nothing was sent).
    pub cpu_fraction_at_target: f64,
    pub frames_forwarded: u64,
    /// Video frames viewers received during the measured interval.
    pub frames_received: u64,
    pub keyframe_requests: u64,
}

struct Client {
    pid: ParticipantId,
    rtc: Rtc,
    addr: SocketAddr,
    mic: Mid,
    camera: Option<Mid>,
    keyframe_due: Vec<String>,
    frames_received: u64,
}

struct Net {
    core: SfuCore,
    now: Instant,
    clients: Vec<Client>,
    core_time: Duration,
    sent_bytes: u64,
    recv_bytes: u64,
    frame_no: u64,
    audio_no: u64,
    last_frame: Option<Instant>,
    last_audio: Option<Instant>,
}

fn sfu_addr() -> SocketAddr {
    SFU_ADDR.parse().unwrap()
}

impl Net {
    fn add(&mut self, user: u64, publish: bool) -> usize {
        let idx = self.clients.len();
        // 10.<hi>.<lo>.x keeps every client address unique for thousands of clients.
        let addr: SocketAddr = format!("10.{}.{}.2:5000", 1 + idx / 250, idx % 250)
            .parse()
            .unwrap();
        let mut rtc = Rtc::builder().build(self.now);
        rtc.add_local_candidate(Candidate::host(addr, "udp").unwrap());
        let mut change = rtc.sdp_api();
        let mic = change.add_media(MediaKind::Audio, Direction::SendOnly, None, None, None);
        let camera = publish.then(|| {
            let mut sim = Simulcast::new();
            for r in RIDS {
                sim.add_send_layer(SimulcastLayer::new(r));
            }
            change.add_media(MediaKind::Video, Direction::SendOnly, None, None, Some(sim))
        });
        let (offer, pending) = change.apply().unwrap();
        let pid = self
            .core
            .join(
                self.now,
                JoinParams {
                    user: UserId::new(user),
                    guild: None,
                    channel: ChannelId::new(CHANNEL),
                    flags: Flags {
                        speak: true,
                        camera: true,
                        ..Flags::default()
                    },
                    candidates: vec![Candidate::host(sfu_addr(), "udp").unwrap()],
                },
            )
            .expect("room has space");
        let answer = self
            .core
            .handle_offer(self.now, pid, &offer.to_sdp_string())
            .expect("offer accepted");
        rtc.sdp_api()
            .accept_answer(pending, SdpAnswer::from_sdp_string(&answer).unwrap())
            .unwrap();
        if let Some(cam) = camera {
            self.core
                .handle_video(
                    self.now,
                    pid,
                    Video::Publish {
                        mid: cam.to_string(),
                        source: Source::Camera,
                        active: true,
                        width: 1280,
                        height: 720,
                        fps: 30,
                    },
                )
                .unwrap();
        }
        self.clients.push(Client {
            pid,
            rtc,
            addr,
            mic,
            camera,
            keyframe_due: Vec::new(),
            frames_received: 0,
        });
        idx
    }

    /// One virtual step. Returns the bytes sent to clients and received from them.
    fn step(&mut self) {
        self.now += STEP;
        let now = self.now;
        let mut to_core: Vec<(SocketAddr, Vec<u8>)> = Vec::new();
        for c in &mut self.clients {
            let _ = c.rtc.handle_input(Input::Timeout(now));
            loop {
                match c.rtc.poll_output().unwrap() {
                    Output::Transmit(t) => to_core.push((c.addr, t.contents.to_vec())),
                    Output::Event(Event::MediaData(d)) => {
                        if c.rtc
                            .media(d.mid)
                            .is_some_and(|m| m.kind() == MediaKind::Video)
                        {
                            c.frames_received += 1;
                        }
                    }
                    Output::Event(Event::KeyframeRequest(k)) => {
                        if let Some(r) = k.rid {
                            c.keyframe_due.push(r.to_string());
                        }
                    }
                    Output::Event(_) => {}
                    Output::Timeout(_) => break,
                }
            }
        }
        self.recv_bytes += to_core.iter().map(|(_, d)| d.len() as u64).sum::<u64>();

        // Only this block is timed.
        let started = Instant::now();
        // As the node's actor does: queue a batch, then run the room once.
        for (from, data) in &to_core {
            self.core
                .queue_input(now, Protocol::Udp, *from, sfu_addr(), data);
        }
        self.core.flush(now);
        self.core.handle_timeout(now);
        let mut outs = Vec::new();
        while let Some(out) = self.core.poll() {
            outs.push(out);
        }
        self.core_time += started.elapsed();

        for out in outs {
            match out {
                CoreOutput::Transmit(p) => {
                    self.sent_bytes += p.data.len() as u64;
                    if let Some(c) = self.clients.iter_mut().find(|c| c.addr == p.destination) {
                        let contents = p.data.as_slice().try_into().unwrap();
                        let _ = c.rtc.handle_input(Input::Receive(
                            now,
                            Receive {
                                proto: Protocol::Udp,
                                source: sfu_addr(),
                                destination: c.addr,
                                contents,
                            },
                        ));
                    }
                }
                CoreOutput::Event(CoreEvent::Signal { to, msg }) => {
                    let Some(c) = self.clients.iter_mut().find(|c| c.pid == to) else {
                        continue;
                    };
                    if msg.op == op::SDP_OFFER {
                        let sdp: wire::Sdp = msg.data().unwrap();
                        let answer = c
                            .rtc
                            .sdp_api()
                            .accept_offer(SdpOffer::from_sdp_string(&sdp.sdp).unwrap())
                            .unwrap();
                        let pid = c.pid;
                        let t = Instant::now();
                        self.core
                            .handle_answer(now, pid, &answer.to_sdp_string())
                            .unwrap();
                        self.core_time += t.elapsed();
                    }
                }
                CoreOutput::Event(_) => {}
            }
        }
    }

    /// Media for this step: a video frame every 33 ms and an audio frame every 20 ms.
    fn generate(&mut self, cfg: &LoadConfig) {
        let now = self.now;
        if self
            .last_frame
            .is_none_or(|t| now.saturating_duration_since(t) >= Duration::from_millis(1000 / FPS))
        {
            self.last_frame = Some(now);
            let rtp = MediaTime::new(self.frame_no * 3_000, Frequency::NINETY_KHZ);
            let first = self.frame_no == 0;
            self.frame_no += 1;
            for c in self.clients.iter_mut().filter(|c| c.camera.is_some()) {
                let mid = c.camera.unwrap();
                let due = std::mem::take(&mut c.keyframe_due);
                let Some(w) = c.rtc.writer(mid) else { continue };
                let Some(pt) = w
                    .payload_params()
                    .find(|p| p.spec().codec == Codec::Vp8)
                    .map(|p| p.pt())
                else {
                    continue;
                };
                for (code, rid) in RIDS.iter().enumerate() {
                    let key = first || due.iter().any(|d| d == rid);
                    let tag = (u8::try_from(code).unwrap() << 4) | u8::from(!key);
                    let size = (cfg.layer_bps[code] / 8 / FPS) as usize;
                    let payload = vec![tag; size.max(16)];
                    let _ = c
                        .rtc
                        .writer(mid)
                        .unwrap()
                        .rid(Rid::from(*rid))
                        .write(pt, now, rtp, payload);
                }
            }
        }
        if cfg.audio
            && self
                .last_audio
                .is_none_or(|t| now.saturating_duration_since(t) >= Duration::from_millis(20))
        {
            self.last_audio = Some(now);
            let rtp = MediaTime::new(self.audio_no * 960, Frequency::FORTY_EIGHT_KHZ);
            self.audio_no += 1;
            for c in self.clients.iter_mut().filter(|c| c.camera.is_some()) {
                let mic = c.mic;
                let Some(w) = c.rtc.writer(mic) else { continue };
                let Some(pt) = w
                    .payload_params()
                    .find(|p| p.spec().codec == Codec::Opus)
                    .map(|p| p.pt())
                else {
                    continue;
                };
                let _ = c.rtc.writer(mic).unwrap().audio_level(-30, true).write(
                    pt,
                    now,
                    rtp,
                    vec![0x55; 80],
                );
            }
        }
    }
}

/// Run one load test and report. Panics if the clients cannot connect (the harness, not the
/// SFU, is then at fault: lower the numbers).
pub fn run(cfg: &LoadConfig) -> Report {
    let total = (cfg.publishers + cfg.viewers) as usize;
    let mut net = Net {
        core: SfuCore::new(total + 8, total + 8),
        now: Instant::now(),
        clients: Vec::new(),
        core_time: Duration::ZERO,
        sent_bytes: 0,
        recv_bytes: 0,
        frame_no: 0,
        audio_no: 0,
        last_frame: None,
        last_audio: None,
    };
    for i in 0..cfg.publishers {
        net.add(u64::from(i) + 1, true);
    }
    for i in 0..cfg.viewers {
        net.add(u64::from(cfg.publishers + i) + 1, false);
    }
    // Connect everyone (ICE + DTLS), without media yet.
    for _ in 0..20_000 {
        net.step();
        if net.clients.iter().all(|c| c.rtc.is_connected()) {
            break;
        }
    }
    assert!(
        net.clients.iter().all(|c| c.rtc.is_connected()),
        "clients did not connect"
    );

    // Every viewer asks for the chosen layer of every camera.
    let hints: Vec<SubscriptionHint> = (0..cfg.publishers)
        .map(|i| SubscriptionHint {
            user_id: UserId::new(u64::from(i) + 1),
            source: Source::Camera,
            enabled: true,
            layer: Some(cfg.viewer_layer),
            viewport: None,
        })
        .collect();
    for i in cfg.publishers..(cfg.publishers + cfg.viewers) {
        let pid = net.clients[i as usize].pid;
        net.core
            .handle_video(
                net.now,
                pid,
                Video::Subscribe {
                    subscriptions: hints.clone(),
                },
            )
            .unwrap();
    }

    let run_for = |net: &mut Net, d: Duration| {
        let end = net.now + d;
        while net.now < end {
            net.generate(cfg);
            net.step();
        }
    };
    run_for(&mut net, cfg.warmup);

    let (core0, sent0, recv0) = (net.core_time, net.sent_bytes, net.recv_bytes);
    let frames0: u64 = net.clients.iter().map(|c| c.frames_received).sum();
    run_for(&mut net, cfg.measure);
    let secs = cfg.measure.as_secs_f64();
    let core_seconds = (net.core_time - core0).as_secs_f64();
    let egress_mbps = (net.sent_bytes - sent0) as f64 * 8.0 / secs / 1e6;
    let ingress_mbps = (net.recv_bytes - recv0) as f64 * 8.0 / secs / 1e6;
    let snap = net.core.snapshot();
    Report {
        participants: total,
        seconds: secs,
        egress_mbps,
        ingress_mbps,
        core_seconds,
        core_cpu_fraction: core_seconds / secs,
        cpu_fraction_at_target: if egress_mbps > 0.0 {
            core_seconds / secs * 300.0 / egress_mbps
        } else {
            0.0
        },
        frames_forwarded: snap.forwarded_frames,
        frames_received: net.clients.iter().map(|c| c.frames_received).sum::<u64>() - frames0,
        keyframe_requests: snap.keyframe_requests,
    }
}

/// What [`run_sharded`] reports: the sum over `shards` actors that each host their own rooms.
#[derive(Debug, Clone, PartialEq)]
pub struct ShardedReport {
    pub shards: Vec<Report>,
    pub participants: usize,
    pub egress_mbps: f64,
    pub ingress_mbps: f64,
    /// Core time of all shards per simulated second, i.e. how many cores' worth of work the
    /// node needs for this load.
    pub cores_busy: f64,
    /// The busiest single shard's share of its own core. Above 1.0 that shard cannot keep up.
    pub busiest_shard_fraction: f64,
    /// The node's total work scaled to 300 Mbit/s of egress, as cores' worth.
    pub cores_at_target: f64,
}

/// Run `shards` copies of the load at the same time, one thread each, the way the sharded
/// SFU runs its actors: every shard owns its own core, rooms and sockets and shares nothing
/// but memory bandwidth and caches with the others (VOX-SFU-011). `cfg` is the load of **one**
/// shard; the node carries `shards` times that.
pub fn run_sharded(cfg: &LoadConfig, shards: usize) -> ShardedReport {
    let shards = shards.max(1);
    let reports: Vec<Report> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..shards).map(|_| s.spawn(|| run(cfg))).collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("shard thread"))
            .collect()
    });
    let egress_mbps: f64 = reports.iter().map(|r| r.egress_mbps).sum();
    let cores_busy: f64 = reports.iter().map(|r| r.core_cpu_fraction).sum();
    ShardedReport {
        participants: reports.iter().map(|r| r.participants).sum(),
        egress_mbps,
        ingress_mbps: reports.iter().map(|r| r.ingress_mbps).sum(),
        cores_busy,
        busiest_shard_fraction: reports
            .iter()
            .map(|r| r.core_cpu_fraction)
            .fold(0.0, f64::max),
        cores_at_target: if egress_mbps > 0.0 {
            cores_busy * 300.0 / egress_mbps
        } else {
            0.0
        },
        shards: reports,
    }
}
