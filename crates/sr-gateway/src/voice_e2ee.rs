//! Op 101 `E2EE_MLS`: the delivery service for voice and video E2EE (VOX-E2E-004, spec
//! docs/specs/lounge-voice-video.md section 5.2).
//!
//! The gateway relays MLS messages between the members of one voice channel or call and
//! orders commits per epoch ([`sr_e2ee::sequencer`]). It never decodes the MLS data: `data`
//! and `welcome` are opaque base64 strings, only checked for size and alphabet, so the server
//! never holds key material or media keys.
//!
//! Client to server, `{"op": 101, "d": {...}}`:
//!
//! - `{"type": "key_package", "channel_id", "data"}`: a joiner's key package, sent to every
//!   other member of the channel.
//! - `{"type": "proposal", "channel_id", "data"}`: sent to every other member.
//! - `{"type": "commit", "channel_id", "epoch", "data", "welcome"?, "to"?}`: a commit based on
//!   `epoch`. The first one per epoch wins: everyone in the channel (the committer too, as
//!   the acknowledgement) gets it with the new epoch, the users in `to` get the welcome. A
//!   commit on any other epoch is answered with `commit_rejected` and the current epoch.
//! - `{"type": "ready", "channel_id", "epoch"}`: this member processed the commit for
//!   `epoch` and holds the new keys.
//!
//! Server to client, dispatch `E2EE_MLS` with `{"type", "channel_id", ...}`: `key_package`,
//! `proposal`, `commit`, `welcome` (each with `from` and `data`, the last two with `epoch`),
//! `commit_rejected` (`epoch`: the current one) and `execute` (`epoch`: switch your
//! encryptor now; everyone is ready or the wait timed out).
//!
//! Only members of the channel may send, and only members receive. The whole op is off
//! unless `voice.e2ee` is set (default off): MLS itself is not built yet (VOX-E2E-001).

use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Value, json};
use sr_core::{ChannelId, UserId};
use sr_e2ee::sequencer::{CallSequencer, CommitDecision};

use crate::hub::Gateway;
use crate::session::Session;
use crate::voice::VoiceState;

/// Largest `data` or `welcome` field (base64 characters). A key package or a commit for a
/// 99-person call fits with room to spare.
pub const MAX_MLS_CHARS: usize = 96 * 1024;
/// Most recipients a welcome may name.
pub const MAX_WELCOME_TO: usize = 100;

#[derive(Debug, Deserialize)]
struct MlsIn {
    #[serde(rename = "type")]
    kind: String,
    channel_id: ChannelId,
    #[serde(default)]
    epoch: Option<u64>,
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
    fn e2ee_enabled(&self) -> bool {
        self.voice_cfg().is_some_and(|v| v.e2ee)
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
            "key_package" | "proposal" => {
                let Some(data) = m.data.filter(|d| opaque_ok(d)) else {
                    return;
                };
                let out = json!({ "type": m.kind, "channel_id": ch, "from": me, "data": data });
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
                let decision =
                    self.st()
                        .vx
                        .e2ee
                        .entry(ch)
                        .or_default()
                        .commit(based_on, me.get(), ids, ms());
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
                let done = self
                    .st()
                    .vx
                    .e2ee
                    .get_mut(&ch)
                    .and_then(|q| q.ready(me.get(), epoch));
                if let Some(epoch) = done {
                    self.e2ee_execute(ch, &members, epoch);
                }
            }
            _ => {}
        }
    }

    /// A member left `old.channel_id`: stop waiting for them; forget the call when empty.
    pub(crate) fn e2ee_member_left(&self, old: &VoiceState) {
        let ch = old.channel_id;
        let members = self.voice_occupants(ch);
        let done = {
            let mut st = self.st();
            if members.is_empty() {
                st.vx.e2ee.remove(&ch);
                None
            } else {
                st.vx
                    .e2ee
                    .get_mut(&ch)
                    .and_then(|q| q.member_left(old.user_id.get()))
            }
        };
        if let Some(epoch) = done {
            self.e2ee_execute(ch, &members, epoch);
        }
    }

    /// Execute transitions whose wait timed out (from the voice sweep).
    pub(crate) fn e2ee_sweep(&self) {
        let now = ms();
        let due: Vec<(ChannelId, u64)> = self
            .st()
            .vx
            .e2ee
            .iter_mut()
            .filter_map(|(ch, q)| q.tick(now).map(|e| (*ch, e)))
            .collect();
        for (ch, epoch) in due {
            let members = self.voice_occupants(ch);
            self.e2ee_execute(ch, &members, epoch);
        }
    }

    /// The epoch commits in `channel` must be based on (tests, diagnostics).
    pub fn e2ee_epoch(&self, channel: ChannelId) -> u64 {
        self.st()
            .vx
            .e2ee
            .get(&channel)
            .map_or(0, CallSequencer::epoch)
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
