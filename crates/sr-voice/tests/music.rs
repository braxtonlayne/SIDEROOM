//! In-process loopback for the music track (MUS-JB-001): real str0m clients publish a
//! microphone and a stereo music line to the SFU core over a simulated network and other
//! clients subscribe. Frames are tagged byte strings (the SFU never decodes, D-201), the clock
//! is virtual, 5 ms per step.
//!
//! What this proves: two audio m-lines per member, the music line forwarded on its own msid
//! stream (`<user>-music`), the per-listener mute (a `music` subscription hint), SPEAK and the
//! channel's `music_allowed` gating, the E2EE gate (nothing before the handshake, same as the
//! microphone), the stereo fmtp the SFU advertises, and the music byte accounting that feeds
//! the egress governor.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use sr_core::{ChannelId, UserId};
use sr_voice::core::{
    CoreEvent, CoreOutput, Flags, JoinParams, ParticipantId, SfuCore, TrackSource,
};
use sr_voice_proto::video::{Source, SubscriptionHint, Video};
use sr_voice_proto::wire::{self, Message, op};
use str0m::change::{SdpAnswer, SdpOffer};
use str0m::format::Codec;
use str0m::media::{Direction, Frequency, MediaKind, MediaTime, Mid};
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, Input, Output, Rtc};

const SFU_ADDR: &str = "10.0.0.1:50000";
const CHANNEL: u64 = 77;
/// 320 kbit/s of Opus payload at 50 frames per second.
const MUSIC_FRAME: usize = 800;

struct Got {
    stream: String,
    data: Vec<u8>,
}

struct Client {
    user: UserId,
    pid: ParticipantId,
    rtc: Rtc,
    addr: SocketAddr,
    mic: Mid,
    music: Mid,
    audio: Vec<Got>,
    signals: Vec<Message>,
    offers: Vec<String>,
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

fn speaker() -> Flags {
    Flags {
        speak: true,
        ..Flags::default()
    }
}

fn is_music_published(v: &Video) -> bool {
    matches!(
        v,
        Video::Published {
            source: Source::Music,
            active: true,
            ..
        }
    )
}

impl World {
    fn new() -> Self {
        Self {
            core: SfuCore::new(100, 99),
            now: Instant::now(),
            clients: Vec::new(),
            frame: 0,
        }
    }

    fn join(&mut self, user: u64, flags: Flags, keyed: bool) -> usize {
        let user = UserId::new(user);
        let idx = self.clients.len();
        let addr: SocketAddr = format!("10.0.1.{}:4000{idx}", idx + 2).parse().unwrap();
        let mut rtc = Rtc::builder().build(self.now);
        rtc.add_local_candidate(Candidate::host(addr, "udp").unwrap());
        let mut change = rtc.sdp_api();
        let mic = change.add_media(MediaKind::Audio, Direction::SendOnly, None, None, None);
        let music = change.add_media(MediaKind::Audio, Direction::SendOnly, None, None, None);
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
                    e2ee_keyed: keyed,
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
            music,
            audio: Vec::new(),
            signals: Vec::new(),
            offers: Vec::new(),
        });
        idx
    }

    /// Name the music m-line, as a client's `VIDEO publish` does.
    fn publish_music(&mut self, i: usize, active: bool) {
        let c = &self.clients[i];
        self.core
            .handle_video(
                self.now,
                c.pid,
                Video::Publish {
                    mid: c.music.to_string(),
                    source: Source::Music,
                    active,
                    width: 0,
                    height: 0,
                    fps: 0,
                },
            )
            .unwrap();
    }

    fn hint_music(&mut self, listener: usize, from: usize, enabled: bool) {
        let pid = self.clients[listener].pid;
        let user = self.clients[from].user;
        self.core
            .handle_video(
                self.now,
                pid,
                Video::Subscribe {
                    subscriptions: vec![SubscriptionHint {
                        user_id: user,
                        source: Source::Music,
                        enabled,
                        layer: None,
                        viewport: None,
                    }],
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
                        c.audio.push(Got {
                            stream: m.stream_id().to_owned(),
                            data: d.data.to_vec(),
                        });
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

    /// One 20 ms audio period: each listed sender writes a microphone frame (tag 0x11) and a
    /// music frame (tag 0x22, 320 kbit/s).
    fn audio_period(&mut self, senders: &[usize], mic: bool, music: bool) {
        self.frame += 1;
        let rtp = MediaTime::new(u64::from(self.frame) * 960, Frequency::FORTY_EIGHT_KHZ);
        for &i in senders {
            let now = self.now;
            let c = &mut self.clients[i];
            let lines = [
                (c.mic, 0x11u8, 60usize, mic),
                (c.music, 0x22, MUSIC_FRAME, music),
            ];
            for (mid, tag, len, on) in lines {
                if !on {
                    continue;
                }
                let pt = c
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
                    .write(pt, now, rtp, vec![tag; len])
                    .unwrap();
            }
        }
        for _ in 0..4 {
            self.step();
        }
    }

    fn stream_audio(&mut self, d: Duration, senders: &[usize], mic: bool, music: bool) {
        let end = self.now + d;
        while self.now < end {
            self.audio_period(senders, mic, music);
        }
    }

    /// Frames client `i` received on `stream` with the given tag byte.
    fn received(&self, i: usize, stream: &str, tag: u8) -> usize {
        self.clients[i]
            .audio
            .iter()
            .filter(|g| g.stream == stream && g.data.first() == Some(&tag))
            .count()
    }

    fn user_stream(&self, i: usize) -> String {
        self.clients[i].user.to_string()
    }

    fn music_stream(&self, i: usize) -> String {
        format!("{}-music", self.clients[i].user)
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

/// A DJ (0) and two listeners (1, 2), the DJ's music line declared.
fn room() -> World {
    let mut w = World::new();
    w.join(1, speaker(), true);
    w.join(2, speaker(), true);
    w.join(3, speaker(), true);
    w.connect_all();
    w.publish_music(0, true);
    w.run(Duration::from_millis(600));
    w
}

#[test]
fn a_second_audio_line_carries_music_next_to_the_microphone() {
    let mut w = room();
    w.stream_audio(Duration::from_secs(1), &[0], true, true);
    let (mic, music) = (w.user_stream(0), w.music_stream(0));
    for l in [1, 2] {
        assert!(w.received(l, &mic, 0x11) > 30, "listener {l} hears the mic");
        assert!(
            w.received(l, &music, 0x22) > 30,
            "listener {l} gets the music"
        );
        // The two lines are separate: no music frame on the mic stream and vice versa.
        assert_eq!(w.received(l, &mic, 0x22), 0);
        assert_eq!(w.received(l, &music, 0x11), 0);
    }
    // The DJ gets nothing back.
    assert!(w.clients[0].audio.is_empty());
    // The room was told music is live.
    assert!(w.videos(1).iter().any(is_music_published));
}

#[test]
fn music_alone_needs_no_microphone_frames() {
    let mut w = room();
    w.stream_audio(Duration::from_secs(1), &[0], false, true);
    let music = w.music_stream(0);
    assert!(w.received(1, &music, 0x22) > 30);
    assert_eq!(w.received(1, &w.user_stream(0), 0x11), 0);
}

#[test]
fn a_listener_can_mute_the_music_and_the_others_still_get_it() {
    let mut w = room();
    w.hint_music(1, 0, false);
    w.run(Duration::from_millis(600));
    let (mic, music) = (w.user_stream(0), w.music_stream(0));
    w.stream_audio(Duration::from_secs(1), &[0], true, true);
    assert_eq!(
        w.received(1, &music, 0x22),
        0,
        "a muted listener gets no music"
    );
    assert!(w.received(1, &mic, 0x11) > 30, "but still hears the mic");
    assert!(
        w.received(2, &music, 0x22) > 30,
        "the other listener is unaffected"
    );
    let (dj, l1, l2) = (w.clients[0].pid, w.clients[1].pid, w.clients[2].pid);
    assert!(!w.core.subscribes(l1, dj, TrackSource::Music));
    assert!(w.core.subscribes(l2, dj, TrackSource::Music));
    // Unmuting brings it back.
    w.hint_music(1, 0, true);
    w.run(Duration::from_millis(600));
    w.stream_audio(Duration::from_secs(1), &[0], true, true);
    assert!(w.received(1, &music, 0x22) > 30);
}

#[test]
fn music_needs_speak() {
    let mut w = World::new();
    w.join(1, Flags::default(), true); // no SPEAK
    w.join(2, speaker(), true);
    w.connect_all();
    w.publish_music(0, true);
    w.run(Duration::from_millis(600));
    w.stream_audio(Duration::from_secs(1), &[0], true, true);
    assert_eq!(w.received(1, &w.music_stream(0), 0x22), 0);
    assert!(
        !w.videos(1).iter().any(is_music_published),
        "a member without SPEAK is never announced as playing music"
    );
}

#[test]
fn the_channel_setting_music_allowed_switches_music_off_and_on() {
    let mut w = room();
    let music = w.music_stream(0);
    let channel = ChannelId::new(CHANNEL);
    w.core.set_music(w.now, channel, false, 256);
    w.run(Duration::from_millis(600));
    w.stream_audio(Duration::from_secs(1), &[0], true, true);
    assert_eq!(
        w.received(1, &music, 0x22),
        0,
        "music is off in this channel"
    );
    assert!(
        w.received(1, &w.user_stream(0), 0x11) > 30,
        "voice is unaffected"
    );
    assert!(!w.core.music_of(channel).allowed);

    w.core.set_music(w.now, channel, true, 320);
    w.run(Duration::from_millis(600));
    w.stream_audio(Duration::from_secs(1), &[0], true, true);
    assert!(w.received(1, &music, 0x22) > 30, "music is back");
    assert_eq!(w.core.music_of(channel).bitrate_kbps, 320);
    // The bitrate is clamped to the accepted range.
    w.core.set_music(w.now, channel, true, 5000);
    assert_eq!(w.core.music_of(channel).bitrate_kbps, 320);
}

#[test]
fn nothing_is_forwarded_before_the_e2ee_handshake() {
    // The same gate as the microphone (OPS-IMM-001, D-249): an unkeyed sender is dropped.
    let mut w = World::new();
    w.join(1, speaker(), false);
    w.join(2, speaker(), true);
    w.connect_all();
    w.publish_music(0, true);
    w.run(Duration::from_millis(600));
    w.stream_audio(Duration::from_secs(1), &[0], true, true);
    assert_eq!(w.received(1, &w.music_stream(0), 0x22), 0);
    assert!(w.core.snapshot().unkeyed_frames > 0);
    // Once keyed it flows.
    let user = w.clients[0].user;
    assert!(
        w.core
            .set_e2ee_keyed(w.now, ChannelId::new(CHANNEL), user, true)
    );
    w.run(Duration::from_millis(600));
    w.stream_audio(Duration::from_secs(1), &[0], true, true);
    assert!(w.received(1, &w.music_stream(0), 0x22) > 30);
}

#[test]
fn the_sfu_advertises_stereo_opus_to_listeners() {
    let mut w = room();
    w.stream_audio(Duration::from_millis(500), &[0], true, true);
    let offer = w.clients[1].offers.last().expect("the SFU offered lines");
    let fmtp = offer
        .lines()
        .find(|l| l.starts_with("a=fmtp:111"))
        .expect("an opus fmtp line");
    assert!(fmtp.contains("stereo=1"), "{fmtp}");
    assert!(fmtp.contains("sprop-stereo=1"), "{fmtp}");
    assert!(fmtp.contains("useinbandfec=1"), "{fmtp}");
    assert!(offer.contains("opus/48000/2"));
}

#[test]
fn music_bytes_are_counted_for_the_governor() {
    let mut w = room();
    w.stream_audio(Duration::from_secs(4), &[0], true, true);
    let s = w.core.snapshot();
    // Two listeners at 320 kbit/s of payload each.
    let want = 2.0 * 320_000.0;
    assert!(
        s.music_egress_bps > want * 0.8 && s.music_egress_bps < want * 1.2,
        "music egress {} vs {want}",
        s.music_egress_bps
    );
    assert!(s.music_packets > 300, "{}", s.music_packets);
}
