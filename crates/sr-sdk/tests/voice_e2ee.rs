//! DEV-BOT-007: the bots-in-voice key agreement against a delivery service that behaves like the gateway's op
//! 101 relay (first commit per epoch wins, `ready`/`execute`, welcomes only to the named users). Members are real
//! `sr_sdk::voice::E2eeCall` state machines, so these tests run the whole MLS exchange: group creation, joins by
//! key package and welcome, a leave, a lost commit race and a frame round trip under the derived keys.
//!
//! Run with `cargo test -p sr-sdk --features voice-e2ee --test voice_e2ee`.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Value, json};
use sr_e2ee::Codec;
use sr_e2ee::mls::ExternalSenderKey;
use sr_e2ee::sequencer::{CallSequencer, CommitDecision};
use sr_sdk::voice::{Action, E2eeCall, E2eeMedia, Identity};

const CHANNEL: &str = "900";

/// A member of the call: its protocol state machine, its media transform and what it has seen.
struct Member {
    call: E2eeCall,
    media: E2eeMedia,
    /// The server's `execute` epochs it switched to.
    executed: Vec<u64>,
    /// Failures reported to the application.
    failures: Vec<String>,
}

/// The delivery service: ordering and relaying, nothing else.
struct Relay {
    ds: ExternalSenderKey,
    group_id: Vec<u8>,
    seq: CallSequencer,
    members: BTreeMap<u64, Member>,
    /// Messages waiting for a member: (to, payload).
    queue: Vec<(u64, Value)>,
    /// Gateway-side log of what each member sent, for assertions.
    sent: Vec<(u64, String)>,
    now: Instant,
}

fn secret_for(id: u64) -> [u8; 32] {
    [id as u8 + 11; 32]
}

impl Relay {
    fn new() -> Self {
        Self {
            ds: ExternalSenderKey::generate().unwrap(),
            group_id: vec![7; 16],
            seq: CallSequencer::new(),
            members: BTreeMap::new(),
            queue: Vec::new(),
            sent: Vec::new(),
            now: Instant::now(),
        }
    }

    fn roster(&self) -> HashSet<u64> {
        self.members.keys().copied().collect()
    }

    /// A bot joins the voice channel and says hello.
    fn join(&mut self, id: u64) {
        let identity = Identity::from_secret(&secret_for(id)).unwrap();
        let call = E2eeCall::new(id, CHANNEL, &identity).unwrap();
        let hello = call.hello();
        self.members.insert(
            id,
            Member {
                call,
                media: E2eeMedia::new(id, [id as u8 + 1, 2, 3, 4]),
                executed: Vec::new(),
                failures: Vec::new(),
            },
        );
        self.apply(id, vec![hello]);
        // Everybody already in the call learns the roster changed.
        let roster = self.roster();
        let ids: Vec<u64> = self.members.keys().copied().filter(|m| *m != id).collect();
        for m in ids {
            let now = self.now;
            let acts = self
                .members
                .get_mut(&m)
                .unwrap()
                .call
                .roster_changed(&roster, now);
            self.apply(m, acts);
        }
    }

    /// A bot leaves: the delivery service proposes its removal, as the gateway does. Returns the leaver.
    fn leave(&mut self, id: u64) -> Member {
        let leaf = self.leaf_of(id);
        let gone = self.members.remove(&id).expect("a member");
        if let Some(leaf) = leaf {
            let epoch = self.seq.epoch();
            let proposal = self
                .ds
                .remove_proposal(&self.group_id, epoch, leaf)
                .unwrap();
            let remaining: Vec<u64> = self.members.keys().copied().collect();
            for m in remaining {
                self.queue.push((
                    m,
                    json!({"type": "proposal", "channel_id": CHANNEL, "data": B64.encode(&proposal), "from": null}),
                ));
            }
        }
        if let Some(epoch) = self.seq.member_left(id) {
            self.broadcast(json!({"type": "execute", "channel_id": CHANNEL, "epoch": epoch}));
        }
        let roster = self.roster();
        let ids: Vec<u64> = roster.iter().copied().collect();
        for m in ids {
            let now = self.now;
            let acts = self
                .members
                .get_mut(&m)
                .unwrap()
                .call
                .roster_changed(&roster, now);
            self.apply(m, acts);
        }
        gone
    }

    /// The leaf a member reported with `ready`.
    fn leaf_of(&self, id: u64) -> Option<u32> {
        self.sent.iter().rev().find_map(|(who, s)| {
            let v: Value = serde_json::from_str(s).ok()?;
            (*who == id && v["type"] == "ready").then(|| v["leaf"].as_u64().map(|l| l as u32))?
        })
    }

    fn broadcast(&mut self, d: Value) {
        let to: Vec<u64> = self.members.keys().copied().collect();
        for m in to {
            self.queue.push((m, d.clone()));
        }
    }

    /// Handle what a member produced.
    fn apply(&mut self, id: u64, actions: Vec<Action>) {
        for a in actions {
            match a {
                Action::Send(d) => self.server(id, d),
                Action::Keys(keys) => {
                    let m = self.members.get_mut(&id).unwrap();
                    m.media.prepare(&keys, 0).unwrap();
                }
                Action::Execute(epoch) => {
                    let m = self.members.get_mut(&id).unwrap();
                    assert!(
                        m.media.execute(epoch),
                        "keys of epoch {epoch} were prepared"
                    );
                    m.executed.push(epoch);
                }
                Action::Failed(why) => self.members.get_mut(&id).unwrap().failures.push(why),
            }
        }
    }

    /// The gateway's side of op 101.
    fn server(&mut self, from: u64, d: Value) {
        self.sent.push((from, d.to_string()));
        assert_eq!(d["channel_id"], CHANNEL);
        let kind = d["type"].as_str().unwrap().to_owned();
        let with_from = |mut v: Value| {
            v["channel_id"] = json!(CHANNEL);
            v["from"] = json!(from.to_string());
            v
        };
        match kind.as_str() {
            "hello" => {
                let first = self.members.len() == 1;
                let call = json!({
                    "type": "call", "channel_id": CHANNEL, "group_id": B64.encode(&self.group_id),
                    "epoch": self.seq.epoch(), "create": first,
                    "external_sender": {
                        "key": B64.encode(self.ds.public_key()),
                        "identity": B64.encode(self.ds.identity()),
                    },
                });
                self.queue.push((from, call));
            }
            "key_package" | "proposal" => {
                let others: Vec<u64> = self
                    .members
                    .keys()
                    .copied()
                    .filter(|m| *m != from)
                    .collect();
                for m in others {
                    self.queue.push((m, with_from(d.clone())));
                }
            }
            "commit" => {
                let based_on = d["epoch"].as_u64().unwrap();
                let members: Vec<u64> = self.members.keys().copied().collect();
                match self.seq.commit(based_on, from, members, 0) {
                    CommitDecision::Accepted { epoch, execute_now } => {
                        let commit =
                            with_from(json!({"type": "commit", "data": d["data"], "epoch": epoch}));
                        self.broadcast(commit);
                        if let Some(w) = d.get("welcome") {
                            for u in d["to"].as_array().unwrap() {
                                let u: u64 = u.as_str().unwrap().parse().unwrap();
                                self.queue.push((
                                    u,
                                    with_from(
                                        json!({"type": "welcome", "data": w, "epoch": epoch}),
                                    ),
                                ));
                            }
                        }
                        if execute_now {
                            self.broadcast(
                                json!({"type": "execute", "channel_id": CHANNEL, "epoch": epoch}),
                            );
                        }
                    }
                    CommitDecision::Rejected { current } => {
                        self.queue.push((
                            from,
                            json!({"type": "commit_rejected", "channel_id": CHANNEL, "epoch": current}),
                        ));
                    }
                }
            }
            "ready" => {
                let epoch = d["epoch"].as_u64().unwrap();
                if let Some(e) = self.seq.ready(from, epoch) {
                    self.broadcast(json!({"type": "execute", "channel_id": CHANNEL, "epoch": e}));
                }
            }
            other => panic!("unexpected op 101 type {other}"),
        }
    }

    /// Deliver queued messages until nothing is left to say.
    fn settle(&mut self) {
        for _ in 0..200 {
            if self.queue.is_empty() {
                return;
            }
            let (to, msg) = self.queue.remove(0);
            if !self.members.contains_key(&to) {
                continue;
            }
            let roster: HashSet<u64> = self.members.keys().copied().collect();
            let acts = self
                .members
                .get_mut(&to)
                .unwrap()
                .call
                .handle(&msg, &roster, self.now);
            self.apply(to, acts);
        }
        panic!("the exchange did not settle");
    }

    /// Time passes: non-leaders whose fallback came due commit.
    fn advance(&mut self, by: Duration) {
        self.now += by;
        let roster = self.roster();
        let ids: Vec<u64> = roster.iter().copied().collect();
        for m in ids {
            let acts = self
                .members
                .get_mut(&m)
                .unwrap()
                .call
                .poll(&roster, self.now);
            self.apply(m, acts);
        }
    }
}

fn epochs(r: &Relay) -> BTreeSet<Option<u64>> {
    r.members.values().map(|m| m.call.epoch()).collect()
}

#[test]
fn two_bots_agree_on_keys_and_exchange_frames() {
    let mut r = Relay::new();
    r.join(1);
    r.settle();
    // Alone: the group is at epoch 0 and the bot may send at once.
    assert_eq!(r.members[&1].executed, vec![0]);

    r.join(2);
    r.settle();
    assert_eq!(epochs(&r), BTreeSet::from([Some(1)]));
    assert_eq!(r.members[&1].executed, vec![0, 1]);
    assert_eq!(r.members[&2].executed, vec![1]);
    // Both see the same two verified members and the same privacy-code input.
    let m1 = r.members[&1].call.members().unwrap();
    let m2 = r.members[&2].call.members().unwrap();
    assert_eq!(
        m1.iter().map(|(u, _)| *u).collect::<BTreeSet<_>>(),
        BTreeSet::from([1, 2])
    );
    assert_eq!(
        m1.iter().collect::<BTreeSet<_>>(),
        m2.iter().collect::<BTreeSet<_>>()
    );
    assert_eq!(
        r.members[&1].call.epoch_authenticator().unwrap(),
        r.members[&2].call.epoch_authenticator().unwrap()
    );
    let id1 = Identity::from_secret(&secret_for(1)).unwrap();
    let hex_key: String = id1
        .public_key()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert!(m2.contains(&(1, hex_key)));

    // Frames: each one's audio opens for the other and for nobody outside the call.
    let opus = vec![0xF8, 1, 2, 3, 4, 5, 6, 7, 8, 9];
    let sealed = r
        .members
        .get_mut(&1)
        .unwrap()
        .media
        .encrypt(Codec::Opus, &opus)
        .unwrap();
    assert_ne!(sealed, opus);
    let got = r
        .members
        .get_mut(&2)
        .unwrap()
        .media
        .decrypt(1, Codec::Opus, &sealed, 10)
        .unwrap();
    assert_eq!(got, opus);
    let back = r
        .members
        .get_mut(&2)
        .unwrap()
        .media
        .encrypt(Codec::Opus, &opus)
        .unwrap();
    assert_eq!(
        r.members
            .get_mut(&1)
            .unwrap()
            .media
            .decrypt(2, Codec::Opus, &back, 10)
            .unwrap(),
        opus
    );
    // A member cannot pass a frame off as somebody else's.
    assert!(
        r.members
            .get_mut(&2)
            .unwrap()
            .media
            .decrypt(2, Codec::Opus, &sealed, 10)
            .is_err()
    );
    // An outsider with other keys gets nothing.
    let mut outsider = E2eeMedia::new(1, [9, 9, 9, 9]);
    assert!(outsider.decrypt(1, Codec::Opus, &sealed, 10).is_err());
    assert!(
        outsider.encrypt(Codec::Opus, &opus).is_err(),
        "no encryption before the first execute"
    );
}

#[test]
fn a_third_bot_joins_and_a_leaver_is_removed_so_it_cannot_follow_the_next_epoch() {
    let mut r = Relay::new();
    r.join(1);
    r.settle();
    r.join(2);
    r.settle();
    r.join(3);
    r.settle();
    assert_eq!(epochs(&r), BTreeSet::from([Some(2)]));
    assert_eq!(r.members[&3].call.members().unwrap().len(), 3);

    // Bot 2 leaves with a frame in flight under epoch 2; the removal moves the group on.
    let opus = vec![0xF8, 9, 9, 9, 9, 9, 9, 9];
    let in_flight = r
        .members
        .get_mut(&2)
        .unwrap()
        .media
        .encrypt(Codec::Opus, &opus)
        .unwrap();
    let mut gone = r.leave(2);
    r.settle();
    r.advance(Duration::from_secs(4));
    r.settle();
    let after = epochs(&r);
    assert_eq!(after.len(), 1, "the remaining members agree");
    assert!(after.iter().next().unwrap().unwrap() >= 3);
    let left: BTreeSet<u64> = r.members[&1]
        .call
        .members()
        .unwrap()
        .iter()
        .map(|(u, _)| *u)
        .collect();
    assert_eq!(left, BTreeSet::from([1, 3]));

    // The leaver cannot read what the remaining members send under the new epoch...
    let fresh = r
        .members
        .get_mut(&1)
        .unwrap()
        .media
        .encrypt(Codec::Opus, &opus)
        .unwrap();
    assert!(gone.media.decrypt(1, Codec::Opus, &fresh, 20).is_err());
    // ...while the others still decode a frame that was in flight just before the switch (grace window).
    assert_eq!(
        r.members
            .get_mut(&3)
            .unwrap()
            .media
            .decrypt(2, Codec::Opus, &in_flight, 20)
            .unwrap(),
        opus
    );
    assert_eq!(
        r.members
            .get_mut(&3)
            .unwrap()
            .media
            .decrypt(1, Codec::Opus, &fresh, 20)
            .unwrap(),
        opus
    );
}

#[test]
fn a_dispatch_for_another_channel_is_ignored_and_garbage_makes_the_member_ask_again() {
    let mut r = Relay::new();
    r.join(1);
    r.settle();
    let now = r.now;
    let roster = r.roster();
    let other = json!({"type": "commit", "channel_id": "123", "data": "AAAA", "from": "2"});
    assert!(
        r.members
            .get_mut(&1)
            .unwrap()
            .call
            .handle(&other, &roster, now)
            .is_empty()
    );

    // A welcome that is not MLS: MLS refuses it, the member leaves its copy and sends a fresh key package.
    let bad = json!({"type": "welcome", "channel_id": CHANNEL, "data": B64.encode(b"not mls"), "from": "2", "epoch": 5});
    let acts = r
        .members
        .get_mut(&1)
        .unwrap()
        .call
        .handle(&bad, &roster, now);
    assert!(matches!(acts[0], Action::Failed(_)));
    assert!(matches!(&acts[1], Action::Send(d) if d["type"] == "key_package"));
    // And it never encrypts under keys it no longer has a group for... the old encryptor is kept until the
    // next epoch executes, so the caller decides; the call itself has no epoch now.
    assert_eq!(r.members[&1].call.epoch(), None);
}

#[test]
fn identity_keys_round_trip_and_never_print() {
    let a = Identity::from_secret(&[5; 32]).unwrap();
    let b = Identity::from_secret(&a.secret()).unwrap();
    assert_eq!(a.public_key(), b.public_key());
    assert_eq!(a.public_key().len(), 65);
    assert_eq!(format!("{a:?}"), "Identity(..)");
    assert!(Identity::from_secret(&[0; 32]).is_err());
    assert!(Identity::from_secret(&[1; 5]).is_err());
}
