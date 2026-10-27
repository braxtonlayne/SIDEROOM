//! Op 101 `E2EE_MLS`: the MLS delivery service for voice and video E2EE (VOX-E2E-004, spec
//! docs/specs/lounge-voice-video.md section 5.2, D-249).
//!
//! The gateway relays MLS messages between the members of one voice channel or call, orders
//! commits per epoch ([`sr_e2ee::sequencer`]) and, as the group's *external sender*, proposes
//! the removal of members who left. It never decodes the members' MLS data: `data` and
//! `welcome` are opaque base64, only checked for size and alphabet. It holds no group secret
//! and no media key; its only key signs Remove proposals.
//!
//! Client to server, `{"op": 101, "d": {"type", "channel_id", ...}}`:
//!
//! - `hello`: I joined the channel. Answered with `call` (below).
//! - `key_package` `{data}`: sent to every other member.
//! - `proposal` `{data}`: sent to every other member.
//! - `commit` `{epoch, data, welcome?, to?}`: a commit based on `epoch`. The first one per epoch
//!   wins: everyone (the committer too, as the acknowledgement) gets it with the new epoch, the
//!   users in `to` get the welcome. A commit on any other epoch gets `commit_rejected`.
//! - `ready` `{epoch, leaf}`: I hold the keys of `epoch`, my leaf index is `leaf`. The leaf is
//!   what the server's Remove proposal names when this member leaves.
//!
//! Server to client, dispatch `E2EE_MLS` `{"type", "channel_id", ...}`:
//!
//! - `call` `{group_id, epoch, create, external_sender: {key, identity}}` (base64): the call's
//!   MLS group id and the server's external sender. `create: true` tells exactly one member to
//!   start the group (the first, or the one left when the group lost all its members); the
//!   others send a key package and wait for a welcome.
//! - `key_package`, `proposal`, `commit`, `welcome` (with `from`; commits and welcomes with the
//!   new `epoch`; a proposal from the server has `from: null`), `commit_rejected` (`epoch`: the
//!   current one), `execute` (`epoch`: switch your encryptor now; everyone is ready or 5 s
//!   passed).
//!
//! Only members of the channel, on the session that joined it, may send, and only members
//! receive. The service runs wherever voice does: end-to-end encryption is the only voice mode.
//!
//! The SFU forwards a member's media only once the member has sent a `key_package` or reported
//! `ready` (OPS-IMM-001): the gateway tells it with `VoiceCommand::E2eeKeyed`.
//!
//! A join (op 4) must carry `sr_e2ee: true` (answered with `SR_VOICE_ERROR`
//! `e2ee_required` and no token otherwise), and a member who has been given `VOICE_SERVER_UPDATE`
//! but does not send `hello` within [`HELLO_TIMEOUT`] is removed from the channel with the same
//! code (D-263).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use serde::Deserialize;
use serde_json::{Value, json};
use sr_core::{ChannelId, UserId};
use sr_e2ee::mls::ExternalSenderKey;
use sr_e2ee::sequencer::{CallSequencer, CommitDecision};

use crate::hub::Gateway;
use crate::session::Session;
use crate::voice::VoiceState;

/// Largest `data` or `welcome` field (base64 characters). A key package or a commit for a
/// 99-person call fits with room to spare.
pub const MAX_MLS_CHARS: usize = 96 * 1024;
/// Most recipients a welcome may name.
pub const MAX_WELCOME_TO: usize = 100;
/// Largest leaf index accepted (a call has at most a few hundred members).
const MAX_LEAF: u32 = 4096;

/// How long a member may take, after `VOICE_SERVER_UPDATE`, to start the MLS handshake with `hello`.
pub const HELLO_TIMEOUT: Duration = Duration::from_secs(10);

/// A member who joined an E2EE-required channel and has not said `hello` yet.
#[derive(Debug, Clone)]
pub(crate) struct PendingHello {
    session_id: String,
    channel: ChannelId,
    deadline: Instant,
}

/// One call's delivery-service state.
#[derive(Debug)]
pub(crate) struct E2eeCall {
    seq: CallSequencer,
    group_id: Vec<u8>,
    /// Who was told to create the group.
    creator: Option<UserId>,
    /// Members' self-reported leaf indexes.
    leaves: HashMap<UserId, u32>,
}

impl E2eeCall {
    fn new() -> Self {
        let mut gid = vec![0u8; 16];
        // A fresh random group id per call; a failing RNG still yields a usable (zero) id.
        let _ = getrandom::fill(&mut gid);
        E2eeCall {
            seq: CallSequencer::new(),
            group_id: gid,
            creator: None,
            leaves: HashMap::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct MlsIn {
    #[serde(rename = "type")]
    kind: String,
    channel_id: ChannelId,
    #[serde(default)]
    epoch: Option<u64>,
    #[serde(default)]
    leaf: Option<u32>,
    #[serde(default)]
    data: Option<String>,
    #[serde(default)]
    welcome: Option<String>,
    #[serde(default)]
    to: Vec<UserId>,
}

/// Opaque MLS bytes as base64 (standard or URL-safe alphabet, optional padding).
fn opaque_ok(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_MLS_CHARS
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=' | b'-' | b'_'))
}

fn ms() -> u64 {
    u64::try_from(crate::voice::unix_ms()).unwrap_or(0)
}

impl Gateway {
    /// Voice is configured. End-to-end encryption is not optional (OPS-IMM-001): no flag turns the
    /// delivery service or the join rule off.
    pub(crate) fn e2ee_enabled(&self) -> bool {
        self.voice_cfg().is_some()
    }

    /// Tell the SFU `user` has taken part in the handshake, so their media may be forwarded.
    async fn e2ee_keyed(&self, channel: ChannelId, user: UserId) {
        self.relay(sr_voice_proto::VoiceCommand::E2eeKeyed {
            channel,
            user,
            keyed: true,
        })
        .await;
    }

    /// The delivery service's external-sender key, made on first use (per node, in memory).
    fn e2ee_ds(&self) -> Option<Arc<ExternalSenderKey>> {
        let mut st = self.st();
        if st.vx.e2ee_ds.is_none() {
            st.vx.e2ee_ds = ExternalSenderKey::generate().ok().map(Arc::new);
        }
        st.vx.e2ee_ds.clone()
    }

    /// Deliver `E2EE_MLS` to the members of `channel` that `want` picks.
    fn e2ee_send(&self, members: &[VoiceState], want: impl Fn(UserId) -> bool, d: &Value) {
        let sessions: Vec<Arc<Session>> = {
            let st = self.st();
            members
                .iter()
                .filter(|m| want(m.user_id))
                .filter_map(|m| st.sessions.get(&m.session_id).cloned())
                .collect()
        };
        for s in sessions {
            s.deliver("E2EE_MLS", d.clone());
        }
    }

    fn e2ee_execute(&self, channel: ChannelId, members: &[VoiceState], epoch: u64) {
        let d = json!({ "type": "execute", "channel_id": channel, "epoch": epoch });
        self.e2ee_send(members, |_| true, &d);
    }

    /// The `call` message for `user`, deciding whether they create the group.
    fn e2ee_call_msg(&self, channel: ChannelId, user: UserId) -> Value {
        let ds = self.e2ee_ds();
        let mut st = self.st();
        let call = st.vx.e2ee.entry(channel).or_insert_with(E2eeCall::new);
        let create = call.creator.is_none();
        if create {
            call.creator = Some(user);
        }
        json!({
            "type": "call",
            "channel_id": channel,
            "group_id": B64.encode(&call.group_id),
            "epoch": call.seq.epoch(),
            "create": create,
            "external_sender": ds.map(|k| json!({
                "key": B64.encode(k.public_key()),
                "identity": B64.encode(k.identity()),
            })),
        })
    }

    /// Op 101 from `s`.
    pub(crate) async fn e2ee_mls(&self, s: &Arc<Session>, d: Value) {
        if !self.e2ee_enabled() {
            return;
        }
        let Ok(m) = serde_json::from_value::<MlsIn>(d) else {
            return;
        };
        let me = s.user;
        // Only a member of the channel, on the session that joined it.
        if !self
            .voice_state(me)
            .is_some_and(|v| v.channel_id == m.channel_id && v.session_id == s.id)
        {
            return;
        }
        let ch = m.channel_id;
        let members = self.voice_occupants(ch);
        match m.kind.as_str() {
            "hello" => {
                self.st().vx.e2ee_pending.remove(&me);
                let out = self.e2ee_call_msg(ch, me);
                self.e2ee_send(&members, |u| u == me, &out);
            }
            "key_package" | "proposal" => {
                let Some(data) = m.data.filter(|d| opaque_ok(d)) else {
                    return;
                };
                let out = json!({ "type": m.kind, "channel_id": ch, "from": me, "data": data });
                if m.kind == "key_package" {
                    // Before the peers hear of it: the SFU must be ready to forward when they react.
                    self.e2ee_keyed(ch, me).await;
                }
                self.e2ee_send(&members, |u| u != me, &out);
            }
            "commit" => {
                let (Some(based_on), Some(data)) = (m.epoch, m.data.filter(|d| opaque_ok(d)))
                else {
                    return;
                };
                let welcome = match m.welcome {
                    Some(w) if !opaque_ok(&w) => return,
                    w => w,
                };
                if m.to.len() > MAX_WELCOME_TO {
                    return;
                }
                let ids: Vec<u64> = members.iter().map(|v| v.user_id.get()).collect();
                let decision = {
                    let mut st = self.st();
                    let call = st.vx.e2ee.entry(ch).or_insert_with(E2eeCall::new);
                    call.seq.commit(based_on, me.get(), ids, ms())
                };
                match decision {
                    CommitDecision::Rejected { current } => {
                        let out = json!({ "type": "commit_rejected", "channel_id": ch, "epoch": current });
                        self.e2ee_send(&members, |u| u == me, &out);
                    }
                    CommitDecision::Accepted { epoch, execute_now } => {
                        let out = json!({
                            "type": "commit", "channel_id": ch, "from": me,
                            "epoch": epoch, "data": data,
                        });
                        self.e2ee_send(&members, |_| true, &out);
                        if let Some(w) = welcome {
                            let out = json!({
                                "type": "welcome", "channel_id": ch, "from": me,
                                "epoch": epoch, "data": w,
                            });
                            let to = m.to;
                            self.e2ee_send(&members, |u| u != me && to.contains(&u), &out);
                        }
                        if execute_now {
                            self.e2ee_execute(ch, &members, epoch);
                        }
                    }
                }
            }
            "ready" => {
                let Some(epoch) = m.epoch else { return };
                let done = {
                    let mut st = self.st();
                    let Some(call) = st.vx.e2ee.get_mut(&ch) else {
                        return;
                    };
                    if let Some(leaf) = m.leaf.filter(|l| *l <= MAX_LEAF) {
                        call.leaves.insert(me, leaf);
                    }
                    call.seq.ready(me.get(), epoch)
                };
                // The member that starts the group sends no key package: `ready` is its proof.
                self.e2ee_keyed(ch, me).await;
                if let Some(epoch) = done {
                    self.e2ee_execute(ch, &members, epoch);
                }
            }
            _ => {}
        }
    }

    /// A member left `old.channel_id`: propose their removal (as the external sender), stop
    /// waiting for them, and forget the call when it empties. If nobody who reported a leaf is
    /// left, the group is gone: the remaining members are told to start a new one.
    pub(crate) fn e2ee_member_left(&self, old: &VoiceState) {
        self.st().vx.e2ee_pending.remove(&old.user_id);
        let ch = old.channel_id;
        let members = self.voice_occupants(ch);
        let ds = self.e2ee_ds_if_enabled();
        let (done, proposal, restart) = {
            let mut st = self.st();
            if members.is_empty() {
                st.vx.e2ee.remove(&ch);
                return;
            }
            let Some(call) = st.vx.e2ee.get_mut(&ch) else {
                return;
            };
            let leaf = call.leaves.remove(&old.user_id);
            let proposal = leaf.zip(ds).and_then(|(leaf, ds)| {
                ds.remove_proposal(&call.group_id, call.seq.epoch(), leaf)
                    .ok()
            });
            let done = call.seq.member_left(old.user_id.get());
            // Nobody known to hold the group is left: the last reported member, or the creator
            // before it reported, just went.
            let restart =
                call.leaves.is_empty() && (leaf.is_some() || call.creator == Some(old.user_id));
            if restart {
                st.vx.e2ee.insert(ch, E2eeCall::new());
            }
            (done, proposal, restart)
        };
        if restart {
            // The group lost its last member: hand out a fresh group; the first gets `create`.
            for m in &members {
                let out = self.e2ee_call_msg(ch, m.user_id);
                self.e2ee_send(&members, |u| u == m.user_id, &out);
            }
            return;
        }
        if let Some(p) = proposal {
            let out = json!({
                "type": "proposal", "channel_id": ch, "from": Value::Null,
                "data": B64.encode(p),
            });
            self.e2ee_send(&members, |_| true, &out);
        }
        if let Some(epoch) = done {
            self.e2ee_execute(ch, &members, epoch);
        }
    }

    fn e2ee_ds_if_enabled(&self) -> Option<Arc<ExternalSenderKey>> {
        if self.e2ee_enabled() {
            self.e2ee_ds()
        } else {
            None
        }
    }

    /// A member was given `VOICE_SERVER_UPDATE` in a channel that requires E2EE: they owe `hello`
    /// within [`HELLO_TIMEOUT`].
    pub(crate) fn e2ee_expect_hello(&self, st: &VoiceState) {
        if !self.e2ee_enabled() {
            return;
        }
        self.st().vx.e2ee_pending.insert(
            st.user_id,
            PendingHello {
                session_id: st.session_id.clone(),
                channel: st.channel_id,
                deadline: Instant::now() + HELLO_TIMEOUT,
            },
        );
    }

    /// Remove members whose `hello` is overdue at `now`, telling them why.
    pub(crate) async fn e2ee_hello_sweep(&self, now: Instant) {
        let due: Vec<(UserId, PendingHello)> = {
            let mut st = self.st();
            let due: Vec<UserId> = st
                .vx
                .e2ee_pending
                .iter()
                .filter(|(_, p)| p.deadline <= now)
                .map(|(u, _)| *u)
                .collect();
            due.into_iter()
                .filter_map(|u| st.vx.e2ee_pending.remove(&u).map(|p| (u, p)))
                .collect()
        };
        for (user, p) in due {
            // Only the very join that owed the hello: a member who left or moved is not touched.
            let Some(v) = self.voice_state(user) else {
                continue;
            };
            if v.session_id != p.session_id || v.channel_id != p.channel {
                continue;
            }
            let session = self.st().sessions.get(&p.session_id).cloned();
            if let Some(s) = session {
                self.voice_error(
                    &s,
                    v.guild_id,
                    Some(p.channel),
                    crate::voice::JoinDenied::E2eeRequired,
                );
            }
            self.voice_leave(user).await;
        }
    }

    /// Execute transitions whose wait timed out and drop members whose `hello` is overdue (from the
    /// voice sweep).
    pub(crate) async fn e2ee_sweep(&self) {
        self.e2ee_hello_sweep(Instant::now()).await;
        self.e2ee_sequencer_sweep();
    }

    fn e2ee_sequencer_sweep(&self) {
        let now = ms();
        let due: Vec<(ChannelId, u64)> = self
            .st()
            .vx
            .e2ee
            .iter_mut()
            .filter_map(|(ch, c)| c.seq.tick(now).map(|e| (*ch, e)))
            .collect();
        for (ch, epoch) in due {
            let members = self.voice_occupants(ch);
            self.e2ee_execute(ch, &members, epoch);
        }
    }

    /// The epoch commits in `channel` must be based on (tests, diagnostics).
    pub fn e2ee_epoch(&self, channel: ChannelId) -> u64 {
        self.st().vx.e2ee.get(&channel).map_or(0, |c| c.seq.epoch())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_fields_are_bounded_base64() {
        assert!(opaque_ok("AAECAw=="));
        assert!(opaque_ok("AA-_"));
        assert!(!opaque_ok(""));
        assert!(!opaque_ok("not base64!"));
        assert!(!opaque_ok(&"A".repeat(MAX_MLS_CHARS + 1)));
    }
}
