//! In-process loopback for the activity data channel (ACT-CH-001): real str0m clients open the SCTP data
//! channel to the SFU core over a simulated network, join activity instances and exchange opaque
//! messages. The clock is virtual, 5 ms per step.
//!
//! What this proves: the relay is scoped to (room, instance) and never reaches the sender, another
//! instance or another room; a sender that has not joined an instance, an unkeyed sender and a text frame are
//! dropped; the per-member budget (`voice.datachannel_kbps`) drops what is over it and refills with time; and
//! the SFU relays `sr-e2ee` data frames as opaque bytes (the receiver opens them, the plaintext never
//! appears on the wire).

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use sr_core::{ChannelId, UserId};
use sr_e2ee::data::{DataReceiver, DataSender};
use sr_e2ee::frame::InstanceId;
use sr_e2ee::keys::SenderSecret;
use sr_voice::core::{CoreOutput, Flags, JoinParams, ParticipantId, SfuCore};
use sr_voice::datachannel::{self as dc, HEADER_OUT};
use str0m::change::SdpAnswer;
use str0m::channel::ChannelId as RtcChannel;
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, Input, Output, Rtc};

const SFU_ADDR: &str = "10.0.0.1:50000";

struct Client {
    user: UserId,
    rtc: Rtc,
    addr: SocketAddr,
    channel: Option<RtcChannel>,
    /// Frames received on the data channel, as the SFU relayed them.
    got: Vec<Vec<u8>>,
}

struct World {
    core: SfuCore,
    now: Instant,
    clients: Vec<Client>,
}

fn sfu_addr() -> SocketAddr {
    SFU_ADDR.parse().unwrap()
}

fn frame(kind: u8, instance: u64, payload: &[u8]) -> Vec<u8> {
    let mut v = vec![kind];
    v.extend_from_slice(&instance.to_be_bytes());
    v.extend_from_slice(payload);
    v
}

impl World {
    fn new(kbps: u32) -> Self {
        Self {
            core: SfuCore::new(100, 99).with_datachannel_kbps(kbps),
            now: Instant::now(),
            clients: Vec::new(),
        }
    }

    fn join(&mut self, room: u64, user: u64, keyed: bool) -> usize {
        let idx = self.clients.len();
        let addr: SocketAddr = format!("10.0.1.{}:4000{idx}", idx + 2).parse().unwrap();
        let mut rtc = Rtc::builder().build(self.now);
        rtc.add_local_candidate(Candidate::host(addr, "udp").unwrap());
        let mut change = rtc.sdp_api();
        change.add_channel(dc::LABEL.to_string());
        let (offer, pending) = change.apply().unwrap();
        let pid: ParticipantId = self
            .core
            .join(
                self.now,
                JoinParams {
                    user: UserId::new(user),
                    guild: None,
                    channel: ChannelId::new(room),
                    flags: Flags {
                        speak: true,
                        ..Flags::default()
                    },
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
            user: UserId::new(user),
            rtc,
            addr,
            channel: None,
            got: Vec::new(),
        });
        idx
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
                    Output::Event(Event::ChannelOpen(id, label)) => {
                        assert_eq!(label, dc::LABEL);
                        c.channel = Some(id);
                    }
                    Output::Event(Event::ChannelData(d)) => c.got.push(d.data),
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
            if let CoreOutput::Transmit(p) = out
                && let Some(c) = self.clients.iter_mut().find(|c| c.addr == p.destination)
            {
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
            if self
                .clients
                .iter()
                .all(|c| c.rtc.is_connected() && c.channel.is_some())
            {
                self.run(Duration::from_millis(200));
                return;
            }
        }
        panic!("clients did not connect and open their data channel");
    }

    /// Write one frame on a client's data channel (binary unless `text`).
    fn send(&mut self, i: usize, buf: &[u8], binary: bool) {
        let c = &mut self.clients[i];
        let ch = c.channel.expect("channel open");
        assert!(c.rtc.channel(ch).unwrap().write(binary, buf).unwrap());
    }

    fn join_instance(&mut self, i: usize, instance: u64) {
        self.send(i, &frame(dc::JOIN, instance, b""), true);
        self.run(Duration::from_millis(100));
    }

    fn send_data(&mut self, i: usize, instance: u64, payload: &[u8]) {
        self.send(i, &frame(dc::DATA, instance, payload), true);
    }

    fn payloads(&self, i: usize) -> Vec<(u64, u64, Vec<u8>)> {
        self.clients[i]
            .got
            .iter()
            .map(|f| {
                assert_eq!(f[0], dc::DATA);
                (
                    u64::from_be_bytes(f[1..9].try_into().unwrap()),
                    u64::from_be_bytes(f[9..17].try_into().unwrap()),
                    f[HEADER_OUT..].to_vec(),
                )
            })
            .collect()
    }
}

#[test]
fn relay_is_scoped_to_room_and_instance() {
    let mut w = World::new(256);
    let a1 = w.join(77, 1, true);
    let a2 = w.join(77, 2, true);
    let a3 = w.join(77, 3, true);
    let b1 = w.join(78, 4, true);
    w.connect_all();
    for (c, inst) in [(a1, 5), (a2, 5), (a3, 6), (b1, 5)] {
        w.join_instance(c, inst);
    }
    w.send_data(a1, 5, b"hello instance five");
    w.run(Duration::from_millis(300));
    assert_eq!(
        w.payloads(a2),
        vec![(5, 1, b"hello instance five".to_vec())],
        "same room, same instance"
    );
    assert!(w.payloads(a3).is_empty(), "same room, other instance");
    assert!(
        w.payloads(b1).is_empty(),
        "other room, same instance number"
    );
    assert!(w.payloads(a1).is_empty(), "never echoed to the sender");
    assert_eq!(w.core.datachannel_stats().relayed, 1);

    // Leaving stops delivery; joining another instance does not widen the first.
    w.send(a2, &frame(dc::LEAVE, 5, b""), true);
    w.run(Duration::from_millis(100));
    w.send_data(a1, 5, b"after leave");
    w.run(Duration::from_millis(300));
    assert_eq!(w.payloads(a2).len(), 1);
    // The first member's own listening is unaffected: a2 can still talk to it after re-joining.
    w.join_instance(a2, 5);
    w.send_data(a2, 5, b"back");
    w.run(Duration::from_millis(300));
    assert_eq!(w.payloads(a1), vec![(5, 2, b"back".to_vec())]);
}

#[test]
fn a_sender_must_have_joined_and_be_keyed_and_binary() {
    let mut w = World::new(256);
    let a1 = w.join(77, 1, true);
    let a2 = w.join(77, 2, true);
    let a3 = w.join(77, 3, false); // has not completed the E2EE handshake
    w.connect_all();
    w.join_instance(a2, 9);
    w.join_instance(a3, 9);
    // a1 never joined instance 9.
    w.send_data(a1, 9, b"not a member");
    // a3 is unkeyed.
    w.send_data(a3, 9, b"unkeyed");
    // a text frame and a malformed frame.
    w.send(a1, b"text", false);
    w.send(a1, &[dc::DATA, 1], true);
    w.run(Duration::from_millis(400));
    assert!(w.payloads(a2).is_empty());
    assert!(w.payloads(a3).is_empty());
    let s = w.core.datachannel_stats();
    assert_eq!(s.relayed, 0);
    assert_eq!(s.dropped_scope, 4);
}

#[test]
fn an_unkeyed_receiver_is_skipped() {
    let mut w = World::new(256);
    let a1 = w.join(77, 1, true);
    let a2 = w.join(77, 2, false);
    w.connect_all();
    w.join_instance(a1, 3);
    w.join_instance(a2, 3);
    w.send_data(a1, 3, b"x");
    w.run(Duration::from_millis(300));
    assert!(w.payloads(a2).is_empty());
}

#[test]
fn the_per_member_budget_drops_what_is_over_it_and_refills() {
    let mut w = World::new(64); // 8 000 bytes per second, bucket of one full message
    let a1 = w.join(77, 1, true);
    let a2 = w.join(77, 2, true);
    w.connect_all();
    w.join_instance(a1, 1);
    w.join_instance(a2, 1);
    // 20 messages of 2 000 bytes at once: 40 kB against a bucket of about 16.4 kB.
    for _ in 0..20 {
        w.send_data(a1, 1, &[7u8; 2000]);
    }
    w.run(Duration::from_millis(400));
    let got = w.payloads(a2).len();
    let s = w.core.datachannel_stats();
    assert!((6..=9).contains(&got), "{got} relayed");
    assert_eq!(s.relayed as usize, got);
    assert_eq!(s.dropped_budget as usize, 20 - got);
    // Four seconds later the bucket holds 32 kB (capped at one full message): sending works again.
    w.run(Duration::from_secs(4));
    w.send_data(a1, 1, &[8u8; 2000]);
    w.run(Duration::from_millis(300));
    assert_eq!(w.payloads(a2).len(), got + 1);
    // Another member's budget is its own: a2 is not affected by a1's burst.
    w.send_data(a2, 1, &[9u8; 2000]);
    w.run(Duration::from_millis(300));
    assert_eq!(w.payloads(a1).len(), 1);
}

#[test]
fn a_zero_budget_turns_the_channel_off() {
    let mut w = World::new(0);
    let a1 = w.join(77, 1, true);
    let a2 = w.join(77, 2, true);
    w.connect_all();
    w.join_instance(a1, 1);
    w.join_instance(a2, 1);
    w.send_data(a1, 1, b"nope");
    w.run(Duration::from_millis(300));
    assert!(w.payloads(a2).is_empty());
    assert!(w.core.datachannel_stats().dropped_budget >= 1);
}

#[test]
fn the_sfu_relays_e2ee_data_frames_as_opaque_bytes() {
    let mut w = World::new(256);
    let a1 = w.join(77, 1, true);
    let a2 = w.join(77, 2, true);
    w.connect_all();
    w.join_instance(a1, 5);
    w.join_instance(a2, 5);

    let secret = SenderSecret::new(*b"0123456789abcdef");
    let mut tx = DataSender::new(&secret, InstanceId([1, 2, 3, 4])).unwrap();
    let mut rx = DataReceiver::new(1000);
    rx.set_epoch(1, [(1, SenderSecret::new(*b"0123456789abcdef"))], 0)
        .unwrap();

    let plain = br#"{"v":1,"t":"state","ops":[{"k":"hand","d":"ace of spades","ver":1,"by":"1"}]}"#;
    let sealed = tx.seal(plain).unwrap();
    w.send_data(a1, 5, &sealed);
    w.run(Duration::from_millis(300));

    let relayed = &w.clients[a2].got;
    assert_eq!(relayed.len(), 1);
    // What the SFU handled carried no readable plaintext...
    assert!(
        !relayed[0]
            .windows(b"ace of spades".len())
            .any(|x| x == b"ace of spades")
    );
    // ...and the receiver, who holds the sender's key, opens it, naming the sender the SFU stamped.
    let (instance, sender, payload) = w.payloads(a2).remove(0);
    assert_eq!((instance, sender), (5, w.clients[a1].user.get()));
    assert_eq!(rx.open(sender, &payload, 1).unwrap(), plain);
}
