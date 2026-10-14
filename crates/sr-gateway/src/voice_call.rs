//! DM and group DM call lifecycle (VOX-SIG-008): ringing, ring timeout, `CALL_CREATE`,
//! `CALL_UPDATE`, `CALL_DELETE` and the missed-call record.
//!
//! # Model (Discord)
//!
//! * A **call** exists while at least one member is connected to the DM's voice room. The
//!   first member to join starts it; every other member of the DM is **rung** at that
//!   moment (`ringing`), for [`RING_TIMEOUT`].
//! * `CALL_CREATE` goes to every member of the DM with `{channel_id, message_id, region,
//!   ringing, voice_states}`. It carries the ringing list, so a client shows the incoming
//!   call overlay when its user is in `ringing`.
//! * Joining removes the member from `ringing`; declining
//!   (`POST /channels/{id}/call/stop-ringing`) does the same; a member inside the call can
//!   ring someone again (`POST /channels/{id}/call/ring`). Each change is a `CALL_UPDATE`.
//! * When the ring expires the remaining ringing members are dropped from `ringing` (a
//!   `CALL_UPDATE`); the call stays open for whoever is in it.
//! * When the last member leaves the call ends: `CALL_DELETE`, and the
//!   [`CallHooks::call_ended`] hook receives a [`CallSummary`] so the composition root can
//!   write the call message into the DM ("X started a call", or "missed call" for members
//!   who never joined).
//!
//! Calls live in memory on the node, like voice states.

use std::collections::BTreeSet;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use sr_core::{ChannelId, UserId};
use tokio::time::Instant;

use crate::hub::Gateway;
use crate::voice::{VoiceState, unix_ms};

/// How long a call rings before the ring stops.
pub const RING_TIMEOUT: Duration = Duration::from_secs(45);

/// What the composition root learns when a call ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallSummary {
    pub channel_id: ChannelId,
    /// Who started the call.
    pub initiator: UserId,
    pub started_ms: i64,
    pub ended_ms: i64,
    /// Everyone who was connected at some point.
    pub participants: Vec<UserId>,
    /// Members of the DM who never joined (they missed the call).
    pub missed_by: Vec<UserId>,
}

/// Writes the call record (a `CALL` system message) into the DM.
#[async_trait]
pub trait CallHooks: Send + Sync {
    async fn call_ended(&self, summary: CallSummary);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallError {
    Unavailable,
    /// No call is running in the channel, or the caller is not in the DM.
    NotFound,
    /// Only members connected to the call may ring or stop ringing others.
    NotInCall,
    /// The named user is not a member of the DM.
    NotAMember,
}

#[derive(Debug, Clone)]
pub(crate) struct CallState {
    pub channel_id: ChannelId,
    /// Everyone in the DM.
    pub members: Vec<UserId>,
    /// The call is on (someone connected). Entries that are not active only remember
    /// `members` for the moment between a join and its first dispatch.
    pub active: bool,
    pub initiator: Option<UserId>,
    pub started_ms: i64,
    pub ringing: BTreeSet<UserId>,
    pub ring_until: Option<Instant>,
    pub participants: BTreeSet<UserId>,
}

impl CallState {
    fn new(channel_id: ChannelId, members: Vec<UserId>) -> Self {
        Self {
            channel_id,
            members,
            active: false,
            initiator: None,
            started_ms: 0,
            ringing: BTreeSet::new(),
            ring_until: None,
            participants: BTreeSet::new(),
        }
    }

    fn ringing_ids(&self) -> Vec<UserId> {
        self.ringing.iter().copied().collect()
    }

    fn update_body(&self) -> Value {
        json!({
            "channel_id": self.channel_id,
            "message_id": null,
            "region": "auto",
            "ringing": self.ringing_ids(),
        })
    }
}

impl Gateway {
    /// Remember who is in the DM `channel` (called on every DM join).
    pub(crate) fn call_set_members(&self, channel: ChannelId, members: Vec<UserId>) {
        let mut st = self.st();
        let e = st
            .vx
            .calls
            .entry(channel)
            .or_insert_with(|| CallState::new(channel, Vec::new()));
        e.members = members;
    }

    /// The members of the DM behind `channel`, as far as the gateway knows.
    pub(crate) fn call_members(&self, channel: ChannelId) -> Vec<UserId> {
        self.st()
            .vx
            .calls
            .get(&channel)
            .map(|c| c.members.clone())
            .unwrap_or_default()
    }

    async fn call_tell(&self, members: &[UserId], name: &str, body: &Value) {
        for u in members {
            self.dispatch_user(*u, name, body.clone()).await;
        }
    }

    /// A member connected to the DM voice room.
    pub(crate) async fn call_member_joined(&self, user: UserId, channel: ChannelId) {
        enum Next {
            Created(CallState),
            Updated(CallState),
            Nothing,
        }
        let next = {
            let mut st = self.st();
            let occupied: Vec<UserId> = st
                .voice
                .values()
                .filter(|v| v.channel_id == channel && v.guild_id.is_none())
                .map(|v| v.user_id)
                .collect();
            let Some(c) = st.vx.calls.get_mut(&channel) else {
                return;
            };
            if !c.active {
                c.active = true;
                c.initiator = Some(user);
                c.started_ms = unix_ms();
                c.participants.insert(user);
                // Everyone else in the DM is rung, except who is connected already.
                c.ringing = c
                    .members
                    .iter()
                    .copied()
                    .filter(|m| *m != user && !occupied.contains(m))
                    .collect();
                c.ring_until = (!c.ringing.is_empty()).then(|| Instant::now() + RING_TIMEOUT);
                Next::Created(c.clone())
            } else {
                let new_participant = c.participants.insert(user);
                let was_ringing = c.ringing.remove(&user);
                if c.ringing.is_empty() {
                    c.ring_until = None;
                }
                if was_ringing || new_participant {
                    Next::Updated(c.clone())
                } else {
                    Next::Nothing
                }
            }
        };
        match next {
            Next::Created(c) => {
                let states: Vec<Value> = self
                    .voice_occupants(channel)
                    .iter()
                    .map(|s| s.to_json(false, None))
                    .collect();
                let mut body = c.update_body();
                if let Some(o) = body.as_object_mut() {
                    o.insert("voice_states".into(), Value::Array(states));
                }
                self.call_tell(&c.members, "CALL_CREATE", &body).await;
            }
            Next::Updated(c) => {
                self.call_tell(&c.members, "CALL_UPDATE", &c.update_body())
                    .await;
            }
            Next::Nothing => {}
        }
    }

    /// A member left the DM voice room; the call ends with the last one.
    pub(crate) async fn call_member_left(&self, old: &VoiceState) {
        let channel = old.channel_id;
        let ended = {
            let mut st = self.st();
            let empty = !st
                .voice
                .values()
                .any(|v| v.channel_id == channel && v.guild_id.is_none());
            if !empty {
                return;
            }
            st.vx.calls.remove(&channel)
        };
        let Some(c) = ended else { return };
        if !c.active {
            return;
        }
        self.call_tell(&c.members, "CALL_DELETE", &json!({ "channel_id": channel }))
            .await;
        let hooks = self.voice_cfg().and_then(|v| v.calls.clone());
        if let (Some(h), Some(initiator)) = (hooks, c.initiator) {
            let participants: Vec<UserId> = c.participants.iter().copied().collect();
            let missed_by = c
                .members
                .iter()
                .copied()
                .filter(|m| !c.participants.contains(m))
                .collect();
            h.call_ended(CallSummary {
                channel_id: channel,
                initiator,
                started_ms: c.started_ms,
                ended_ms: unix_ms(),
                participants,
                missed_by,
            })
            .await;
        }
    }

    /// Stop rings that ran out and forget calls nobody started.
    pub(crate) async fn call_sweep(&self, now: Instant) {
        let expired: Vec<CallState> = {
            let mut st = self.st();
            let occupied: BTreeSet<ChannelId> = st
                .voice
                .values()
                .filter(|v| v.guild_id.is_none())
                .map(|v| v.channel_id)
                .collect();
            st.vx
                .calls
                .retain(|ch, c| c.active || occupied.contains(ch));
            let mut out = Vec::new();
            for c in st.vx.calls.values_mut() {
                if c.ring_until.is_some_and(|t| t <= now) {
                    c.ring_until = None;
                    if !c.ringing.is_empty() {
                        c.ringing.clear();
                        out.push(c.clone());
                    }
                }
            }
            out
        };
        for c in expired {
            self.call_tell(&c.members, "CALL_UPDATE", &c.update_body())
                .await;
        }
    }

    /// The running call in `channel`, as `CALL_CREATE` shows it, for a member of the DM.
    pub fn call_state(&self, user: UserId, channel: ChannelId) -> Option<Value> {
        let c = {
            let st = self.st();
            st.vx.calls.get(&channel).filter(|c| c.active).cloned()?
        };
        if !c.members.contains(&user) {
            return None;
        }
        let states: Vec<Value> = self
            .voice_occupants(channel)
            .iter()
            .map(|s| s.to_json(false, None))
            .collect();
        let mut body = c.update_body();
        if let Some(o) = body.as_object_mut() {
            o.insert("voice_states".into(), Value::Array(states));
        }
        Some(body)
    }

    /// `POST /channels/{id}/call/ring`: ring `recipients` (default: everyone not connected).
    /// The caller must be connected to the call.
    pub async fn call_ring(
        &self,
        actor: UserId,
        channel: ChannelId,
        recipients: Option<Vec<UserId>>,
    ) -> Result<(), CallError> {
        if self.voice_cfg().is_none() {
            return Err(CallError::Unavailable);
        }
        let updated = {
            let mut st = self.st();
            let occupied: Vec<UserId> = st
                .voice
                .values()
                .filter(|v| v.channel_id == channel && v.guild_id.is_none())
                .map(|v| v.user_id)
                .collect();
            let c = st
                .vx
                .calls
                .get_mut(&channel)
                .filter(|c| c.active && c.members.contains(&actor))
                .ok_or(CallError::NotFound)?;
            if !occupied.contains(&actor) {
                return Err(CallError::NotInCall);
            }
            let targets: Vec<UserId> = match recipients {
                Some(r) => {
                    if r.iter().any(|u| !c.members.contains(u)) {
                        return Err(CallError::NotAMember);
                    }
                    r
                }
                None => c.members.clone(),
            };
            for u in targets {
                if !occupied.contains(&u) {
                    c.ringing.insert(u);
                }
            }
            if !c.ringing.is_empty() {
                c.ring_until = Some(Instant::now() + RING_TIMEOUT);
            }
            c.clone()
        };
        self.call_tell(&updated.members, "CALL_UPDATE", &updated.update_body())
            .await;
        Ok(())
    }

    /// `POST /channels/{id}/call/stop-ringing`: decline (own entry) or stop ringing others
    /// (from inside the call).
    pub async fn call_stop_ringing(
        &self,
        actor: UserId,
        channel: ChannelId,
        recipients: Option<Vec<UserId>>,
    ) -> Result<(), CallError> {
        if self.voice_cfg().is_none() {
            return Err(CallError::Unavailable);
        }
        let updated = {
            let mut st = self.st();
            let inside = st
                .voice
                .get(&actor)
                .is_some_and(|v| v.channel_id == channel && v.guild_id.is_none());
            let c = st
                .vx
                .calls
                .get_mut(&channel)
                .filter(|c| c.active && c.members.contains(&actor))
                .ok_or(CallError::NotFound)?;
            let targets: Vec<UserId> = match recipients {
                Some(r) => r,
                None if inside => c.ringing.iter().copied().collect(),
                None => vec![actor],
            };
            for u in targets {
                // Anyone may decline for themselves; stopping others needs the call.
                if u == actor || inside {
                    c.ringing.remove(&u);
                } else {
                    return Err(CallError::NotInCall);
                }
            }
            if c.ringing.is_empty() {
                c.ring_until = None;
            }
            c.clone()
        };
        self.call_tell(&updated.members, "CALL_UPDATE", &updated.update_body())
            .await;
        Ok(())
    }

    /// Calls by channel, for tests.
    #[cfg(test)]
    pub(crate) fn call_snapshot(
        &self,
    ) -> std::collections::HashMap<ChannelId, (bool, Vec<UserId>)> {
        self.st()
            .vx
            .calls
            .iter()
            .map(|(k, c)| (*k, (c.active, c.ringing_ids())))
            .collect()
    }
}
