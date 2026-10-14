//! In-process loopback for video (VOX-SFU-005..008): real str0m clients publish a simulcast
//! camera, a screen share and its audio to the SFU core over a simulated network, and other
//! clients subscribe. No sockets and no real codecs: frames are tagged byte strings that
//! str0m packetizes as VP8 (the first byte's low bit says delta (1) or key (0) frame, which
//! is how str0m's VP8 depacketizer finds keyframes). The clock is virtual, 5 ms per step.
//!
//! What this proves: layer selection per subscriber (hint, estimate, governor), switching
//! only on keyframes, keyframe request coalescing, screen share gated by the viewer list,
//! the screen and its audio sharing one stream group, the publish permission, and the
//! governor's notice. Real browsers, real encoders and real congestion are out of reach
//! here; see docs/dev/handoff/feat-p10-voice-be.md.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use sr_core::{ChannelId, UserId};
use sr_voice::core::{
    CoreEvent, CoreOutput, Flags, JoinParams, ParticipantId, SfuCore, TrackSource,
};
use sr_voice::governor::GovernorConfig;
use sr_voice_proto::video::{Layer, Quality, Source, SubscriptionHint, Video, Viewport};
use sr_voice_proto::wire::{self, Message, op};
use str0m::change::{SdpAnswer, SdpOffer};
use str0m::format::Codec;
use str0m::media::{
    Direction, Frequency, KeyframeRequestKind, MediaKind, MediaTime, Mid, Rid, Simulcast,
    SimulcastLayer,
};
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, Input, Output, Rtc};

const SFU_ADDR: &str = "10.0.0.1:50000";
const CHANNEL: u64 = 88;
const RIDS: [&str; 3] = ["q", "h", "f"];

/// What a client publishes in its first offer.
#[derive(Clone, Copy, Default)]
struct Tracks {
    camera: bool,
    screen: bool,
    screen_audio: bool,
}

struct Got {
    stream: String,
    #[allow(dead_code)]
    rid: Option<String>,
    data: Vec<u8>,
}

struct Client {
    user: UserId,
    pid: ParticipantId,
    rtc: Rtc,
    addr: SocketAddr,
    #[allow(dead_code)]
    mic: Mid,
    camera: Option<Mid>,
    screen: Option<Mid>,
    screen_audio: Option<Mid>,
    video: Vec<Got>,
    audio: Vec<Got>,
    signals: Vec<Message>,
    /// Keyframe requests this client received as a publisher: (mid, rid).
    keyframe_requests: Vec<(Mid, Option<String>)>,
    /// Layers a keyframe was asked for and not yet sent.
    keyframe_due: Vec<String>,
    offers: Vec<String>,
    /// Video lines the SFU opened towards this client (its receive mids).
    video_in: Vec<Mid>,
}

struct World {
    core: SfuCore,
    now: Instant,
    clients: Vec<Client>,
    frame: u32,
}

fn sfu_addr() -> SocketAddr {
    SFU_ADDR.parse().unwrap()
}

fn base_flags() -> Flags {
    Flags {
        speak: true,
        camera: true,
        screen: true,
        ..Flags::default()
    }
}

impl World {
    fn new() -> Self {
        Self::with(SfuCore::new(100, 99))
    }

    fn with(core: SfuCore) -> Self {
        Self {
            core,
            now: Instant::now(),
            clients: Vec::new(),
            frame: 0,
        }
    }

    fn join(&mut self, user: u64, flags: Flags, tracks: Tracks) -> usize {
        let user = UserId::new(user);
        let idx = self.clients.len();
        let addr: SocketAddr = format!("10.0.1.{}:4000{idx}", idx + 2).parse().unwrap();
        let mut rtc = Rtc::builder().build(self.now);
        rtc.add_local_candidate(Candidate::host(addr, "udp").unwrap());
        let mut change = rtc.sdp_api();
        let mic = change.add_media(MediaKind::Audio, Direction::SendOnly, None, None, None);
        let mut camera = None;
        if tracks.camera {
            let mut sim = Simulcast::new();
            for r in RIDS {
                sim.add_send_layer(SimulcastLayer::new(r));
            }
            camera = Some(change.add_media(
                MediaKind::Video,
                Direction::SendOnly,
                None,
                None,
                Some(sim),
            ));
        }
        let screen = tracks
            .screen
            .then(|| change.add_media(MediaKind::Video, Direction::SendOnly, None, None, None));
        let screen_audio = tracks
            .screen_audio
            .then(|| change.add_media(MediaKind::Audio, Direction::SendOnly, None, None, None));
        let (offer, pending) = change.apply().unwrap();

        let pid = self
            .core
            .join(
                self.now,
                JoinParams {
                    user,
                    guild: None,
                    channel: ChannelId::new(CHANNEL),
                    flags,
                    candidates: vec![Candidate::host(sfu_addr(), "udp").unwrap()],
                },
            )
            .unwrap();
        let answer = self
            .core
            .handle_offer(self.now, pid, &offer.to_sdp_string())
            .unwrap();
        rtc.sdp_api()
            .accept_answer(pending, SdpAnswer::from_sdp_string(&answer).unwrap())
            .unwrap();
        self.clients.push(Client {
            user,
            pid,
            rtc,
            addr,
            mic,
            camera,
            screen,
            screen_audio,
            video: Vec::new(),
            audio: Vec::new(),
            signals: Vec::new(),
            keyframe_requests: Vec::new(),
            keyframe_due: Vec::new(),
            offers: Vec::new(),
            video_in: Vec::new(),
        });
        idx
    }

    /// Tell the SFU which m-line carries what, as a client's `VIDEO publish` does.
    fn publish(&mut self, i: usize, source: Source, active: bool) {
        let c = &self.clients[i];
        let mid = match source {
            Source::Camera => c.camera,
            Source::Screen => c.screen,
            Source::ScreenAudio => c.screen_audio,
        }
        .unwrap();
        let (width, height, fps) = if source == Source::ScreenAudio {
            (0, 0, 0)
        } else {
            (1280, 720, 30)
        };
        self.core
            .handle_video(
                self.now,
                c.pid,
                Video::Publish {
                    mid: mid.to_string(),
                    source,
                    active,
                    width,
                    height,
                    fps,
                },
            )
            .unwrap();
    }

    fn subscribe(&mut self, i: usize, hints: Vec<SubscriptionHint>) {
        let pid = self.clients[i].pid;
        self.core
            .handle_video(
                self.now,
                pid,
                Video::Subscribe {
                    subscriptions: hints,
                },
            )
            .unwrap();
    }

    fn step(&mut self) {
        self.now += Duration::from_millis(5);
        let now = self.now;
        let mut to_core: Vec<(SocketAddr, Vec<u8>)> = Vec::new();
        for c in &mut self.clients {
            let _ = c.rtc.handle_input(Input::Timeout(now));
            loop {
                match c.rtc.poll_output().unwrap() {
                    Output::Transmit(t) => to_core.push((c.addr, t.contents.to_vec())),
                    Output::Event(Event::MediaData(d)) => {
                        let m = c.rtc.media(d.mid).unwrap();
                        let got = Got {
                            stream: m.stream_id().to_owned(),
                            rid: d.rid.map(|r| r.to_string()),
                            data: d.data.to_vec(),
                        };
                        if m.kind() == MediaKind::Video {
                            c.video.push(got);
                        } else {
                            c.audio.push(got);
                        }
                    }
                    Output::Event(Event::MediaAdded(m)) => {
                        if m.kind == MediaKind::Video && m.direction.is_receiving() {
                            c.video_in.push(m.mid);
                        }
                    }
                    Output::Event(Event::KeyframeRequest(k)) => {
                        let rid = k.rid.map(|r| r.to_string());
                        if let Some(r) = &rid {
                            c.keyframe_due.push(r.clone());
                        }
                        c.keyframe_requests.push((k.mid, rid));
                    }
                    Output::Event(_) => {}
                    Output::Timeout(_) => break,
                }
            }
        }
        for (from, data) in to_core {
            self.core
                .handle_input(now, Protocol::Udp, from, sfu_addr(), &data);
        }
        self.core.handle_timeout(now);
        while let Some(out) = self.core.poll() {
            match out {
                CoreOutput::Transmit(p) => {
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
                        c.offers.push(sdp.sdp.clone());
                        let answer = c
                            .rtc
                            .sdp_api()
                            .accept_offer(SdpOffer::from_sdp_string(&sdp.sdp).unwrap())
                            .unwrap();
                        let pid = c.pid;
                        self.core
                            .handle_answer(now, pid, &answer.to_sdp_string())
                            .unwrap();
                    }
                    c.signals.push(msg);
                }
                CoreOutput::Event(_) => {}
            }
        }
    }

    fn run(&mut self, d: Duration) {
        let end = self.now + d;
        while self.now < end {
            self.step();
        }
    }

    fn connect_all(&mut self) {
        for _ in 0..4000 {
            self.step();
            if self.clients.iter().all(|c| c.rtc.is_connected()) {
                self.run(Duration::from_millis(500));
                return;
            }
        }
        panic!("clients did not connect");
    }

    fn pt_vp8(&mut self, i: usize) -> str0m::media::Pt {
        let mid = self.clients[i].camera.or(self.clients[i].screen).unwrap();
        self.clients[i]
            .rtc
            .writer(mid)
            .unwrap()
            .payload_params()
            .find(|p| p.spec().codec == Codec::Vp8)
            .unwrap()
            .pt()
    }

    /// One 33 ms video frame period: every listed publisher sends a frame on each layer,
    /// the first one (and any layer a keyframe was asked for) as a keyframe.
    fn video_frame(&mut self, senders: &[(usize, Source)]) {
        let rtp = MediaTime::new(u64::from(self.frame) * 3_000, Frequency::NINETY_KHZ);
        let first = self.frame == 0;
        self.frame += 1;
        for &(i, source) in senders {
            let pt = self.pt_vp8(i);
            let now = self.now;
            let c = &mut self.clients[i];
            let due = std::mem::take(&mut c.keyframe_due);
            match source {
                Source::Camera => {
                    let mid = c.camera.unwrap();
                    for (code, rid) in RIDS.iter().enumerate() {
                        let key = first || due.iter().any(|d| d == rid);
                        // Byte 0: layer in the high nibble, delta (1) or key (0) in the low bit.
                        let tag = (u8::try_from(code).unwrap() << 4) | u8::from(!key);
                        let size = 100 * (code + 1);
                        let mut payload = vec![tag; size];
                        payload[1] = c.user.get() as u8;
                        c.rtc
                            .writer(mid)
                            .unwrap()
                            .rid(Rid::from(*rid))
                            .write(pt, now, rtp, payload)
                            .unwrap();
                    }
                }
                Source::Screen => {
                    let mid = c.screen.unwrap();
                    let key = first || !due.is_empty();
                    let tag = 0xA0 | u8::from(!key);
                    c.rtc
                        .writer(mid)
                        .unwrap()
                        .write(pt, now, rtp, vec![tag; 150])
                        .unwrap();
                }
                Source::ScreenAudio => {
                    let mid = c.screen_audio.unwrap();
                    let opus = c
                        .rtc
                        .writer(mid)
                        .unwrap()
                        .payload_params()
                        .find(|p| p.spec().codec == Codec::Opus)
                        .unwrap()
                        .pt();
                    c.rtc
                        .writer(mid)
                        .unwrap()
                        .write(
                            opus,
                            now,
                            MediaTime::new(u64::from(self.frame) * 960, Frequency::FORTY_EIGHT_KHZ),
                            vec![0xEE; 60],
                        )
                        .unwrap();
                }
            }
        }
        for _ in 0..6 {
            self.step();
        }
    }

    fn stream(&mut self, d: Duration, senders: &[(usize, Source)]) {
        let end = self.now + d;
        while self.now < end {
            self.video_frame(senders);
        }
    }

    /// Frames a client received on `stream`, by layer code (the high nibble of byte 0).
    fn frames_by_layer(&self, i: usize, stream: &str) -> [usize; 3] {
        let mut n = [0usize; 3];
        for g in self.clients[i].video.iter().filter(|g| g.stream == stream) {
            let code = usize::from(g.data[0] >> 4);
            if code < 3 {
                n[code] += 1;
            }
        }
        n
    }

    fn clear_video(&mut self, i: usize) {
        self.clients[i].video.clear();
    }

    fn quality_messages(&self, i: usize) -> Vec<Quality> {
        self.clients[i]
            .signals
            .iter()
            .filter(|m| m.op == op::QUALITY)
            .filter_map(|m| m.data::<Quality>().ok())
            .collect()
    }

    fn videos(&self, i: usize) -> Vec<Video> {
        self.clients[i]
            .signals
            .iter()
            .filter(|m| m.op == op::VIDEO)
            .filter_map(|m| m.data::<Video>().ok())
            .collect()
    }
}

fn camera_hint(user: u64, layer: Option<Layer>, vp: Option<u32>) -> SubscriptionHint {
    SubscriptionHint {
        user_id: UserId::new(user),
        source: Source::Camera,
        enabled: true,
        layer,
        viewport: vp.map(|h| Viewport {
            width: h * 16 / 9,
            height: h,
        }),
    }
}

const CAMERA_ONLY: Tracks = Tracks {
    camera: true,
    screen: false,
    screen_audio: false,
};

#[test]
fn a_camera_reaches_subscribers_as_a_thumbnail_until_they_ask_for_more() {
    let mut w = World::new();
    let p = w.join(1, base_flags(), CAMERA_ONLY);
    let s = w.join(2, base_flags(), Tracks::default());
    w.connect_all();
    w.publish(p, Source::Camera, true);
    w.run(Duration::from_millis(600));

    // The subscriber was told about the camera and got a line labelled with the group id.
    assert!(w.videos(s).iter().any(|v| matches!(
        v,
        Video::Published { user_id, source: Source::Camera, active: true, height: 720, .. }
            if *user_id == UserId::new(1)
    )));
    assert!(
        w.core
            .subscribes(w.clients[s].pid, w.clients[p].pid, TrackSource::Camera)
    );

    w.stream(Duration::from_secs(3), &[(p, Source::Camera)]);
    let n = w.frames_by_layer(s, "1-camera");
    assert!(n[0] > 40, "the thumbnail flows: {n:?}");
    assert_eq!(n[1] + n[2], 0, "no other layer is forwarded: {n:?}");
    assert_eq!(
        w.core
            .layer_of(w.clients[s].pid, w.clients[p].pid, TrackSource::Camera),
        Some(Layer::Low)
    );
    // The publisher never receives its own camera.
    assert!(w.clients[p].video.is_empty());
    // The SFU asked the publisher for a keyframe of the thumbnail layer.
    assert!(
        w.clients[p]
            .keyframe_requests
            .iter()
            .any(|(_, rid)| rid.as_deref() == Some("q"))
    );
    // Forwarded payloads are bit-exact: nothing is decoded or rewritten.
    let g = w.clients[s]
        .video
        .iter()
        .find(|g| g.data[0] >> 4 == 0)
        .unwrap();
    assert_eq!(g.data.len(), 100);
    assert_eq!(g.data[1], 1);
    assert!(g.data[2..].iter().all(|b| *b == g.data[0]));
}

#[test]
fn a_bigger_tile_moves_the_subscriber_up_on_a_keyframe_of_the_new_layer() {
    let mut w = World::new();
    let p = w.join(1, base_flags(), CAMERA_ONLY);
    let s = w.join(2, base_flags(), Tracks::default());
    w.connect_all();
    w.publish(p, Source::Camera, true);
    w.run(Duration::from_millis(600));
    w.stream(Duration::from_secs(2), &[(p, Source::Camera)]);
    let (sp, pp) = (w.clients[s].pid, w.clients[p].pid);
    assert_eq!(
        w.core.layer_of(sp, pp, TrackSource::Camera),
        Some(Layer::Low)
    );

    // The tile grows to 720p: the full layer is wanted, but only after the upgrade hold and
    // only once a keyframe of that layer arrives.
    w.subscribe(s, vec![camera_hint(1, None, Some(720))]);
    w.clear_video(s);
    w.stream(Duration::from_secs(2), &[(p, Source::Camera)]);
    assert_eq!(
        w.core.layer_of(sp, pp, TrackSource::Camera),
        Some(Layer::Low),
        "no upgrade before the hold"
    );
    w.stream(Duration::from_secs(5), &[(p, Source::Camera)]);
    assert_eq!(
        w.core.layer_of(sp, pp, TrackSource::Camera),
        Some(Layer::High)
    );
    assert!(
        w.clients[p]
            .keyframe_requests
            .iter()
            .any(|(_, rid)| rid.as_deref() == Some("f")),
        "the switch asked the publisher for a full-layer keyframe"
    );
    // Seen from the subscriber: thumbnails first, then the first full-layer frame is a
    // keyframe, and no thumbnail follows it.
    let seq: Vec<u8> = w.clients[s].video.iter().map(|g| g.data[0]).collect();
    let first_high = seq
        .iter()
        .position(|b| b >> 4 == 2)
        .expect("full layer arrived");
    assert_eq!(seq[first_high] & 1, 0, "the switch happened on a keyframe");
    assert!(seq[..first_high].iter().all(|b| b >> 4 == 0));
    assert!(seq[first_high..].iter().all(|b| b >> 4 == 2));
    // The client was told.
    assert!(w.videos(s).iter().any(|v| matches!(
        v,
        Video::Layer {
            layer: Layer::High,
            source: Source::Camera,
            ..
        }
    )));

    // Back to a thumbnail tile: down is immediate.
    w.subscribe(s, vec![camera_hint(1, Some(Layer::Low), None)]);
    w.stream(Duration::from_secs(1), &[(p, Source::Camera)]);
    assert_eq!(
        w.core.layer_of(sp, pp, TrackSource::Camera),
        Some(Layer::Low)
    );
}

#[test]
fn subscribers_choose_layers_independently() {
    let mut w = World::new();
    let p = w.join(1, base_flags(), CAMERA_ONLY);
    let a = w.join(2, base_flags(), Tracks::default());
    let b = w.join(3, base_flags(), Tracks::default());
    w.connect_all();
    w.publish(p, Source::Camera, true);
    w.run(Duration::from_millis(600));
    w.subscribe(a, vec![camera_hint(1, Some(Layer::High), None)]);
    w.stream(Duration::from_secs(7), &[(p, Source::Camera)]);
    let pp = w.clients[p].pid;
    assert_eq!(
        w.core.layer_of(w.clients[a].pid, pp, TrackSource::Camera),
        Some(Layer::High)
    );
    assert_eq!(
        w.core.layer_of(w.clients[b].pid, pp, TrackSource::Camera),
        Some(Layer::Low)
    );
}

#[test]
fn a_camera_can_be_turned_off_per_subscriber() {
    let mut w = World::new();
    let p = w.join(1, base_flags(), CAMERA_ONLY);
    let s = w.join(2, base_flags(), Tracks::default());
    w.connect_all();
    w.publish(p, Source::Camera, true);
    w.run(Duration::from_millis(600));
    let (sp, pp) = (w.clients[s].pid, w.clients[p].pid);
    assert!(w.core.subscribes(sp, pp, TrackSource::Camera));
    w.subscribe(
        s,
        vec![SubscriptionHint {
            enabled: false,
            ..camera_hint(1, None, None)
        }],
    );
    w.run(Duration::from_millis(600));
    assert!(!w.core.subscribes(sp, pp, TrackSource::Camera));
    w.clear_video(s);
    w.stream(Duration::from_secs(1), &[(p, Source::Camera)]);
    assert!(w.clients[s].video.is_empty());
    // And back on.
    w.subscribe(s, vec![camera_hint(1, None, None)]);
    w.run(Duration::from_millis(600));
    assert!(w.core.subscribes(sp, pp, TrackSource::Camera));
}

#[test]
fn a_publisher_without_permission_sends_nothing_until_the_gateway_allows_it() {
    let mut w = World::new();
    let p = w.join(
        1,
        Flags {
            camera: false,
            ..base_flags()
        },
        CAMERA_ONLY,
    );
    let s = w.join(2, base_flags(), Tracks::default());
    w.connect_all();
    w.publish(p, Source::Camera, true);
    w.run(Duration::from_millis(600));
    let (sp, pp) = (w.clients[s].pid, w.clients[p].pid);

    // Not allowed: nobody hears of it and the frames are dropped.
    assert!(
        w.videos(s)
            .iter()
            .all(|v| !matches!(v, Video::Published { .. }))
    );
    assert!(!w.core.subscribes(sp, pp, TrackSource::Camera));
    w.stream(Duration::from_secs(1), &[(p, Source::Camera)]);
    assert!(w.clients[s].video.is_empty());

    // The sender allowance lets it start: the same publish now goes live.
    w.core
        .set_flags(w.now, ChannelId::new(CHANNEL), UserId::new(1), base_flags());
    w.run(Duration::from_millis(600));
    assert!(w.core.subscribes(sp, pp, TrackSource::Camera));
    assert!(
        w.videos(s)
            .iter()
            .any(|v| matches!(v, Video::Published { active: true, .. }))
    );
    w.stream(Duration::from_secs(2), &[(p, Source::Camera)]);
    assert!(w.frames_by_layer(s, "1-camera")[0] > 20);

    // Taking it back stops the line and tells the room.
    w.core.set_flags(
        w.now,
        ChannelId::new(CHANNEL),
        UserId::new(1),
        Flags {
            camera: false,
            ..base_flags()
        },
    );
    w.run(Duration::from_millis(600));
    assert!(!w.core.subscribes(sp, pp, TrackSource::Camera));
    assert!(
        w.videos(s)
            .iter()
            .any(|v| matches!(v, Video::Published { active: false, .. }))
    );
}

#[test]
fn a_screen_share_goes_only_to_listed_viewers_who_asked_and_shares_a_stream_group_with_its_audio() {
    let mut w = World::new();
    let tracks = Tracks {
        camera: false,
        screen: true,
        screen_audio: true,
    };
    let p = w.join(1, base_flags(), tracks);
    let viewer = w.join(2, base_flags(), Tracks::default());
    let other = w.join(3, base_flags(), Tracks::default());
    w.connect_all();
    w.publish(p, Source::Screen, true);
    w.publish(p, Source::ScreenAudio, true);
    w.run(Duration::from_millis(600));
    let pp = w.clients[p].pid;
    let ch = ChannelId::new(CHANNEL);

    // Subscribing is not enough: the gateway has not listed anyone as a viewer.
    let screen_hint = |enabled| SubscriptionHint {
        user_id: UserId::new(1),
        source: Source::Screen,
        enabled,
        layer: None,
        viewport: None,
    };
    w.subscribe(viewer, vec![screen_hint(true)]);
    w.subscribe(other, vec![screen_hint(true)]);
    w.run(Duration::from_millis(600));
    assert!(
        !w.core
            .subscribes(w.clients[viewer].pid, pp, TrackSource::Screen)
    );

    // The gateway lists the viewer (the stream cap is its business): only they get it.
    w.core
        .set_stream_viewers(w.now, ch, UserId::new(1), vec![UserId::new(2)], true);
    w.run(Duration::from_millis(800));
    let vp = w.clients[viewer].pid;
    assert!(w.core.subscribes(vp, pp, TrackSource::Screen));
    assert!(w.core.subscribes(vp, pp, TrackSource::ScreenAudio));
    assert!(
        !w.core
            .subscribes(w.clients[other].pid, pp, TrackSource::Screen)
    );

    // Both lines carry the same msid stream id, so the browser plays them as one group.
    let screen_offer = w.clients[viewer]
        .offers
        .last()
        .cloned()
        .expect("the viewer was offered the screen lines");
    let groups = screen_offer.matches("1-screen").count();
    assert!(
        groups >= 2,
        "video and audio share a stream id:\n{screen_offer}"
    );

    // Frames flow to the viewer only, bit-exact.
    w.stream(
        Duration::from_secs(2),
        &[(p, Source::Screen), (p, Source::ScreenAudio)],
    );
    assert!(
        w.clients[viewer]
            .video
            .iter()
            .filter(|g| g.stream == "1-screen")
            .count()
            > 30
    );
    assert!(
        w.clients[viewer]
            .audio
            .iter()
            .filter(|g| g.stream == "1-screen")
            .count()
            > 30
    );
    assert!(
        w.clients[viewer]
            .audio
            .iter()
            .all(|g| g.data == vec![0xEE; 60])
    );
    assert!(w.clients[other].video.is_empty());
    assert!(
        w.clients[other]
            .audio
            .iter()
            .all(|g| g.stream != "1-screen")
    );
    // A screen is one layer: it is forwarded whole, whatever the hint says.
    assert_eq!(
        w.core.layer_of(vp, pp, TrackSource::Screen),
        Some(Layer::High)
    );

    // The viewer list shrinks (the cap, or the viewer stopped watching): the lines go.
    w.core
        .set_stream_viewers(w.now, ch, UserId::new(1), Vec::new(), true);
    w.run(Duration::from_millis(800));
    assert!(!w.core.subscribes(vp, pp, TrackSource::Screen));
    assert!(!w.core.subscribes(vp, pp, TrackSource::ScreenAudio));
    // Ending the stream clears the list for good.
    w.core
        .set_stream_viewers(w.now, ch, UserId::new(1), vec![UserId::new(2)], true);
    w.run(Duration::from_millis(800));
    assert!(w.core.subscribes(vp, pp, TrackSource::Screen));
    w.core
        .set_stream_viewers(w.now, ch, UserId::new(1), Vec::new(), false);
    w.run(Duration::from_millis(800));
    assert!(!w.core.subscribes(vp, pp, TrackSource::Screen));
}

#[test]
fn a_flood_of_keyframe_requests_costs_the_publisher_one_or_two() {
    let mut w = World::new();
    let p = w.join(1, base_flags(), CAMERA_ONLY);
    let subs: Vec<usize> = (2..=6)
        .map(|u| w.join(u, base_flags(), Tracks::default()))
        .collect();
    w.connect_all();
    w.publish(p, Source::Camera, true);
    w.run(Duration::from_millis(800));
    w.stream(Duration::from_secs(3), &[(p, Source::Camera)]);
    assert!(subs.iter().all(|s| w.clients[*s].video_in.len() == 1));

    // Everyone lost a packet they could not recover and asks for a keyframe, again and again.
    let before = w.clients[p].keyframe_requests.len();
    for _round in 0..8 {
        for s in &subs {
            let mid = w.clients[*s].video_in[0];
            let _ = w.clients[*s]
                .rtc
                .writer(mid)
                .unwrap()
                .request_keyframe(None, KeyframeRequestKind::Pli);
        }
        for _ in 0..4 {
            w.step();
        }
    }
    w.run(Duration::from_millis(300));
    let during = w.clients[p].keyframe_requests.len() - before;
    assert!(
        (1..=2).contains(&during),
        "40 PLIs from 5 subscribers reached the publisher as {during} requests"
    );
    // The requests are for the layer those subscribers receive.
    assert!(
        w.clients[p].keyframe_requests[before..]
            .iter()
            .all(|(_, rid)| rid.as_deref() == Some("q"))
    );
    assert!(w.core.snapshot().keyframe_requests >= 1);
}

#[test]
fn the_governor_caps_layers_tier_by_tier_and_says_so() {
    // A budget of 30 kbit/s: the video below exceeds it, idle RTCP does not. Steps are quick.
    let gov = GovernorConfig {
        budget_bps: 30_000.0,
        step_interval: Duration::from_secs(1),
        release_hold: Duration::from_secs(3),
        ..GovernorConfig::default()
    };
    let mut w = World::with(SfuCore::new(100, 99).with_governor(gov));
    let p = w.join(1, base_flags(), CAMERA_ONLY);
    // A free-tier viewer and a Patron, both asking for the full layer.
    let free = w.join(2, base_flags(), Tracks::default());
    let patron = w.join(
        3,
        Flags {
            tier: 4,
            ..base_flags()
        },
        Tracks::default(),
    );
    w.connect_all();
    w.publish(p, Source::Camera, true);
    w.run(Duration::from_millis(800));
    for s in [free, patron] {
        w.subscribe(s, vec![camera_hint(1, Some(Layer::High), None)]);
    }
    let pp = w.clients[p].pid;
    // The governor engages on the first second of measured egress.
    w.stream(Duration::from_millis(900), &[(p, Source::Camera)]);

    // Under load the governor steps up once a second: the free tier is capped first.
    w.stream(Duration::from_secs(5), &[(p, Source::Camera)]);
    let sev = w.core.snapshot().governor_severity;
    assert!(sev >= 2, "the governor engaged: severity {sev}");
    assert_eq!(
        w.core
            .layer_of(w.clients[free].pid, pp, TrackSource::Camera),
        Some(Layer::Low),
        "free tier is down to the thumbnail"
    );
    // The free viewer was told once ("Reduced quality: the room is busy").
    let q = w.quality_messages(free);
    assert_eq!(q.len(), 1, "{q:?}");
    assert!(q[0].reduced);
    assert!(q[0].max_layer < Layer::High);
    // A Patron is the last to be touched and, at this severity, not at all.
    assert!(w.quality_messages(patron).is_empty());
    assert_eq!(
        w.core
            .layer_of(w.clients[patron].pid, pp, TrackSource::Camera),
        Some(Layer::High)
    );

    // Load goes away: the caps lift one step at a time and the viewer is told.
    w.run(Duration::from_secs(40));
    assert_eq!(w.core.snapshot().governor_severity, 0);
    let q = w.quality_messages(free);
    assert!(!q.last().unwrap().reduced, "the notice was lifted: {q:?}");
}

#[test]
fn a_low_bandwidth_estimate_moves_a_subscriber_down_at_once_and_back_up_slowly() {
    let mut w = World::new();
    let p = w.join(1, base_flags(), CAMERA_ONLY);
    let s = w.join(2, base_flags(), Tracks::default());
    w.connect_all();
    w.publish(p, Source::Camera, true);
    w.run(Duration::from_millis(600));
    w.subscribe(s, vec![camera_hint(1, Some(Layer::High), None)]);
    w.stream(Duration::from_secs(8), &[(p, Source::Camera)]);
    let (sp, pp) = (w.clients[s].pid, w.clients[p].pid);
    assert_eq!(
        w.core.layer_of(sp, pp, TrackSource::Camera),
        Some(Layer::High)
    );

    // The link to this subscriber collapses (a TWCC or loss-based estimate): the layer
    // that fits is the thumbnail, and the switch happens within a second or two.
    w.core.set_bandwidth_estimate(sp, Some(50_000.0));
    w.stream(Duration::from_secs(2), &[(p, Source::Camera)]);
    assert_eq!(
        w.core.layer_of(sp, pp, TrackSource::Camera),
        Some(Layer::Low)
    );
    // The other direction is slow: a recovered estimate is not trusted for a few seconds.
    w.core.set_bandwidth_estimate(sp, None);
    w.stream(Duration::from_secs(2), &[(p, Source::Camera)]);
    assert_eq!(
        w.core.layer_of(sp, pp, TrackSource::Camera),
        Some(Layer::Low)
    );
    w.stream(Duration::from_secs(5), &[(p, Source::Camera)]);
    assert_eq!(
        w.core.layer_of(sp, pp, TrackSource::Camera),
        Some(Layer::High)
    );
}
