//! In-process loopback: real str0m clients talk to the SFU core through a simulated
//! network (no sockets), proving DTLS-SRTP, ICE-lite, SDP negotiation and audio
//! forwarding end to end. The clock is virtual and advances in 5 ms steps.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use sr_core::{ChannelId, UserId};
use sr_voice::core::{
    CloseReason, CoreEvent, CoreOutput, Flags, JoinParams, ParticipantId, SfuCore,
};
use sr_voice_proto::wire::{self, Message, Speaking, UserRef, op, speaking};
use str0m::change::{SdpAnswer, SdpOffer};
use str0m::format::Codec;
use str0m::media::{Direction, MediaKind, Mid};
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, Input, Output, Rtc};

const SFU_ADDR: &str = "10.0.0.1:50000";
const CHANNEL: u64 = 77;

struct Client {
    user: UserId,
    pid: ParticipantId,
    rtc: Rtc,
    addr: SocketAddr,
    mic: Mid,
    /// Frames received: (stream id of the m-line, payload).
    heard: Vec<(String, Vec<u8>)>,
    signals: Vec<Message>,
    tx_seq: u32,
}

struct World {
    core: SfuCore,
    now: Instant,
    start: Instant,
    clients: Vec<Client>,
    closed: Vec<(UserId, CloseReason)>,
}

fn sfu_addr() -> SocketAddr {
    SFU_ADDR.parse().unwrap()
}

impl World {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            core: SfuCore::new(100, 99),
            now,
            start: now,
            clients: Vec::new(),
            closed: Vec::new(),
        }
    }

    fn flags() -> Flags {
        Flags {
            speak: true,
            deaf: false,
            ptt_required: false,
            priority: false,
        }
    }

    /// Join a client the way the voice WebSocket would: its first offer, then the answer.
    fn join(&mut self, user: u64, flags: Flags) -> usize {
        let user = UserId::new(user);
        let idx = self.clients.len();
        let addr: SocketAddr = format!("10.0.1.{}:4000{idx}", idx + 2).parse().unwrap();
        let mut rtc = Rtc::builder().build(self.now);
        rtc.add_local_candidate(Candidate::host(addr, "udp").unwrap());
        let mut change = rtc.sdp_api();
        let mic = change.add_media(MediaKind::Audio, Direction::SendOnly, None, None, None);
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
            heard: Vec::new(),
            signals: Vec::new(),
            tx_seq: 0,
        });
        idx
    }

    /// One 5 ms tick of the whole world.
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
                        let stream = c.rtc.media(d.mid).unwrap().stream_id().to_owned();
                        c.heard.push((stream, d.data.to_vec()));
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
                CoreOutput::Event(CoreEvent::Closed { user, reason, .. }) => {
                    self.closed.push((user, reason));
                }
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
        for _ in 0..2000 {
            self.step();
            if self.clients.iter().all(|c| c.rtc.is_connected()) {
                // Let the subscription renegotiation settle.
                self.run(Duration::from_millis(500));
                return;
            }
        }
        panic!("clients did not connect");
    }

    /// Send one 20 ms Opus-sized frame from client `i`, tagged so receivers can tell who sent it.
    fn send_frame(&mut self, i: usize, level: i8) -> Vec<u8> {
        let wall = self.now;
        let time = (self.now - self.start).into();
        let c = &mut self.clients[i];
        c.tx_seq += 1;
        let mut payload = vec![c.user.get() as u8; 80];
        payload[1..5].copy_from_slice(&c.tx_seq.to_be_bytes());
        let mut w = c.rtc.writer(c.mic).unwrap();
        let pt = w
            .payload_params()
            .find(|p| p.spec().codec == Codec::Opus)
            .unwrap()
            .pt();
        w = w.audio_level(level, true);
        w.write(pt, wall, time, payload.clone()).unwrap();
        payload
    }

    /// Talk from client `i` for `ms` milliseconds at 20 ms per frame.
    fn talk(&mut self, i: usize, ms: u64, level: i8) {
        for _ in 0..(ms / 20) {
            self.send_frame(i, level);
            for _ in 0..4 {
                self.step();
            }
        }
    }

    fn frames_from(&self, listener: usize, speaker: usize) -> usize {
        let stream = self.clients[speaker].user.to_string();
        self.clients[listener]
            .heard
            .iter()
            .filter(|(s, _)| *s == stream)
            .count()
    }

    fn speaking_events(&self, listener: usize, speaker: u64) -> Vec<u8> {
        self.clients[listener]
            .signals
            .iter()
            .filter(|m| m.op == op::SPEAKING)
            .filter_map(|m| m.data::<Speaking>().ok())
            .filter(|s| s.user_id == Some(UserId::new(speaker)))
            .map(|s| s.speaking)
            .collect()
    }
}

#[test]
fn audio_from_a_reaches_b_and_never_loops_back() {
    let mut w = World::new();
    let a = w.join(1, World::flags());
    let b = w.join(2, World::flags());
    w.connect_all();

    // B has a receive line labelled with A's user id (msid stream id).
    w.talk(a, 1000, -30);
    w.run(Duration::from_millis(200));

    assert!(
        w.frames_from(b, a) >= 30,
        "B heard only {} of A's frames",
        w.frames_from(b, a)
    );
    // Payloads arrive bit-exact: the SFU forwards opaque Opus, it does not touch it.
    let first = &w.clients[b].heard.iter().find(|(s, _)| s == "1").unwrap().1;
    assert_eq!(first.len(), 80);
    assert_eq!(first[0], 1, "first byte carries the sender id");
    assert!(first[5..].iter().all(|x| *x == 1));
    // A hears nothing of itself.
    assert_eq!(w.frames_from(a, a), 0);
    assert!(w.clients[a].heard.is_empty());
}

#[test]
fn both_directions_and_a_late_joiner() {
    let mut w = World::new();
    let a = w.join(1, World::flags());
    let b = w.join(2, World::flags());
    w.connect_all();
    w.talk(a, 400, -30);
    w.talk(b, 400, -30);
    assert!(w.frames_from(b, a) >= 10 && w.frames_from(a, b) >= 10);

    // C joins an established room and hears both; both hear C.
    let c = w.join(3, World::flags());
    w.connect_all();
    w.talk(a, 400, -30);
    w.talk(b, 400, -30);
    w.talk(c, 400, -30);
    w.run(Duration::from_millis(200));
    assert!(
        w.frames_from(c, a) >= 10,
        "C heard {} from A",
        w.frames_from(c, a)
    );
    assert!(
        w.frames_from(c, b) >= 10,
        "C heard {} from B",
        w.frames_from(c, b)
    );
    assert!(
        w.frames_from(a, c) >= 10,
        "A heard {} from C",
        w.frames_from(a, c)
    );
    assert!(
        w.frames_from(b, c) >= 10,
        "B heard {} from C",
        w.frames_from(b, c)
    );
}

#[test]
fn speaking_events_follow_the_audio_level() {
    let mut w = World::new();
    let a = w.join(1, World::flags());
    let b = w.join(2, World::flags());
    w.connect_all();

    // Background noise below the threshold never counts as speech.
    w.talk(a, 400, -80);
    assert!(w.speaking_events(b, 1).is_empty());

    // Real speech: one "on", then one "off" after the hold period.
    w.talk(a, 400, -25);
    w.run(Duration::from_millis(600));
    assert_eq!(w.speaking_events(b, 1), vec![speaking::MICROPHONE, 0]);
    // The speaker is not told about themselves.
    assert!(w.speaking_events(a, 1).is_empty());
    // And B, who stayed silent, produced nothing.
    assert!(w.speaking_events(a, 2).is_empty());
}

#[test]
fn priority_speaker_sets_the_priority_bit() {
    let mut w = World::new();
    let a = w.join(
        1,
        Flags {
            priority: true,
            ..World::flags()
        },
    );
    let b = w.join(2, World::flags());
    w.connect_all();
    w.talk(a, 200, -20);
    assert_eq!(
        w.speaking_events(b, 1).first().copied(),
        Some(speaking::MICROPHONE | speaking::PRIORITY)
    );
}

#[test]
fn server_mute_drops_audio_and_ends_speaking() {
    let mut w = World::new();
    let a = w.join(1, World::flags());
    let b = w.join(2, World::flags());
    w.connect_all();
    w.talk(a, 400, -30);
    let before = w.frames_from(b, a);
    assert!(before >= 10);

    let muted = Flags {
        speak: false,
        ..World::flags()
    };
    w.core
        .set_flags(w.now, ChannelId::new(CHANNEL), UserId::new(1), muted);
    w.run(Duration::from_millis(100));
    let at_mute = w.frames_from(b, a);
    w.talk(a, 600, -30);
    w.run(Duration::from_millis(200));
    assert_eq!(
        w.frames_from(b, a),
        at_mute,
        "muted audio must not be forwarded"
    );
    // Muting a speaker silences their indicator immediately.
    assert_eq!(w.speaking_events(b, 1).last().copied(), Some(0));

    // Unmuting restores the flow.
    w.core.set_flags(
        w.now,
        ChannelId::new(CHANNEL),
        UserId::new(1),
        World::flags(),
    );
    w.talk(a, 400, -30);
    assert!(w.frames_from(b, a) > at_mute + 5);
}

#[test]
fn deafened_listener_receives_nothing() {
    let mut w = World::new();
    let a = w.join(1, World::flags());
    let b = w.join(
        2,
        Flags {
            deaf: true,
            ..World::flags()
        },
    );
    let c = w.join(3, World::flags());
    w.connect_all();
    w.talk(a, 600, -30);
    w.run(Duration::from_millis(200));
    assert_eq!(w.frames_from(b, a), 0, "deafened B got audio");
    assert!(w.frames_from(c, a) >= 15, "C should still hear A");
}

#[test]
fn push_to_talk_gates_audio_when_voice_activity_is_denied() {
    let mut w = World::new();
    let a = w.join(
        1,
        Flags {
            ptt_required: true,
            ..World::flags()
        },
    );
    let b = w.join(2, World::flags());
    w.connect_all();

    // Key not held: even loud audio is dropped.
    w.talk(a, 400, -20);
    assert_eq!(w.frames_from(b, a), 0);

    // Key held: audio flows.
    w.core
        .set_client_speaking(w.now, w.clients[a].pid, speaking::MICROPHONE);
    w.talk(a, 400, -20);
    let held = w.frames_from(b, a);
    assert!(held >= 10, "only {held} frames while PTT was held");

    // Released: a short grace for late packets, then nothing.
    w.core.set_client_speaking(w.now, w.clients[a].pid, 0);
    w.run(Duration::from_millis(500));
    let after_release = w.frames_from(b, a);
    w.talk(a, 400, -20);
    assert_eq!(w.frames_from(b, a), after_release);
}

#[test]
fn leaving_notifies_the_room_and_stops_the_line() {
    let mut w = World::new();
    let a = w.join(1, World::flags());
    let b = w.join(2, World::flags());
    w.connect_all();
    w.talk(a, 200, -30);
    let offers_before = w.clients[b]
        .signals
        .iter()
        .filter(|m| m.op == op::SDP_OFFER)
        .count();

    w.core.leave(w.clients[a].pid, CloseReason::Left);
    w.run(Duration::from_millis(500));

    let b_signals = &w.clients[b].signals;
    assert!(
        b_signals.iter().any(|m| m.op == op::CLIENT_DISCONNECT
            && m.data::<UserRef>().unwrap().user_id == UserId::new(1))
    );
    assert!(
        b_signals.iter().filter(|m| m.op == op::SDP_OFFER).count() > offers_before,
        "the server must renegotiate to stop A's line"
    );
    assert_eq!(w.core.participant_count(), 1);
    assert_eq!(w.core.room_count(), 1);
    assert!(w.closed.contains(&(UserId::new(1), CloseReason::Left)));

    w.core.leave(w.clients[b].pid, CloseReason::Left);
    assert_eq!(w.core.room_count(), 0, "an empty room is destroyed");
}

#[test]
fn rejoining_replaces_the_old_participant() {
    let mut w = World::new();
    w.join(1, World::flags());
    w.join(2, World::flags());
    let first = w.core.participant_of(UserId::new(1)).unwrap();
    w.connect_all();
    w.join(1, World::flags());
    assert_ne!(w.core.participant_of(UserId::new(1)), Some(first));
    assert!(w.closed.is_empty() || w.closed.iter().all(|(_, r)| *r == CloseReason::Replaced));
    w.step();
    assert!(w.closed.contains(&(UserId::new(1), CloseReason::Replaced)));
    assert_eq!(w.core.participant_count(), 2);
}

#[test]
fn snapshot_counts_rooms_participants_and_traffic() {
    let mut w = World::new();
    let a = w.join(1, World::flags());
    w.join(2, World::flags());
    w.connect_all();
    w.talk(a, 2500, -30);
    let s = w.core.snapshot();
    assert_eq!((s.rooms, s.participants), (1, 2));
    assert!(s.forwarded_packets >= 100);
    assert!(s.egress_bps > 0.0, "egress {}", s.egress_bps);
    assert!(s.ingress_bps > 0.0, "ingress {}", s.ingress_bps);
    assert!((0.0..=1.0).contains(&s.loss_ratio));
}

#[test]
fn room_and_sfu_limits_are_enforced() {
    let mut core = SfuCore::new(3, 2);
    let join = |core: &mut SfuCore, user: u64, ch: u64| {
        core.join(
            Instant::now(),
            JoinParams {
                user: UserId::new(user),
                guild: None,
                channel: ChannelId::new(ch),
                flags: World::flags(),
                candidates: vec![],
            },
        )
    };
    assert!(join(&mut core, 1, 10).is_ok());
    assert!(join(&mut core, 2, 10).is_ok());
    assert!(matches!(
        join(&mut core, 3, 10),
        Err(sr_voice::core::SfuError::RoomFull)
    ));
    assert!(join(&mut core, 3, 11).is_ok());
    assert!(matches!(
        join(&mut core, 4, 12),
        Err(sr_voice::core::SfuError::Full)
    ));
}

#[test]
fn garbage_signalling_is_rejected_not_fatal() {
    let mut w = World::new();
    let a = w.join(1, World::flags());
    let pid = w.clients[a].pid;
    assert!(w.core.handle_offer(w.now, pid, "not sdp").is_err());
    assert!(
        w.core
            .handle_offer(w.now, pid, &"x".repeat(100_000))
            .is_err()
    );
    assert!(w.core.handle_answer(w.now, pid, "nope").is_err());
    assert!(
        w.core
            .handle_offer(w.now, ParticipantId(9999), "v=0")
            .is_err()
    );
    // A valid-looking answer with nothing pending is ignored.
    assert!(!w.core.handle_input(
        w.now,
        Protocol::Udp,
        "1.2.3.4:5".parse().unwrap(),
        sfu_addr(),
        b"random bytes"
    ));
}
