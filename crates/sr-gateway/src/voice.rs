//! Voice state on the gateway (VOX-SIG-001..004, 007): op 4 `VOICE_STATE_UPDATE`, the
//! `VOICE_STATE_UPDATE` and `VOICE_SERVER_UPDATE` dispatches, join checks, moderator
//! actions and cleanup. Wire details: docs/05-api-and-gateway.md section 6.5.
//!
//! # Storage
//!
//! Voice states live **in memory on the node that holds the session**, in
//! [`Gateway`]'s state map keyed by user (a user has at most one voice connection, as on
//! Discord). They are lost on restart: clients reconnect, resume or rejoin, and the SFU
//! drops participants whose WebSocket never returns. Server mute and deafen are part of
//! the state, so they also reset when the user leaves voice. Persisting them needs a table
//! (follow-up); nothing here blocks that. A multi-node gateway needs the store behind a
//! shared bus first, as presence does.
//!
//! The decisions (who may join, who may speak, what a moderator may do) are pure
//! functions at the top of this file so they are unit-tested without a gateway.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde_json::{Value, json};
use sr_core::{ChannelId, GuildId, Permissions, UserId};
use sr_events::DomainEvent;
use sr_voice_proto::select::select_endpoint;
use sr_voice_proto::token::TOKEN_TTL_SECS;
use sr_voice_proto::{JoinClaims, TokenSigner, TurnConfig, VoiceCommand, VoiceRelay};
use tokio::time::Instant;

use crate::hub::Gateway;
use crate::session::Session;

/// Wiring for voice, set in [`crate::GatewayConfig::voice`].
#[derive(Clone)]
pub struct VoiceConfig {
    /// Mints join tokens; the SFU holds the same secret.
    pub signer: TokenSigner,
    /// SFU WebSocket URLs (`wss://host/voice/ws`). A channel's room is pinned to one of
    /// them by rendezvous hashing ([`select_endpoint`]).
    pub endpoints: Vec<String>,
    /// STUN/TURN servers and the coturn secret for `ice_servers`.
    pub turn: TurnConfig,
    /// How the gateway reaches the SFU (server mute, disconnect). `None`: tokens only.
    pub relay: Option<Arc<dyn VoiceRelay>>,
    /// How long a voice state survives its gateway session losing its socket. Past it the
    /// user is removed from the channel.
    pub session_grace: Duration,
}

impl std::fmt::Debug for VoiceConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VoiceConfig")
            .field("endpoints", &self.endpoints)
            .field("session_grace", &self.session_grace)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceChannelKind {
    Voice,
    /// Stage channels are later work (VOX-STG-001): joins are refused with `stage_unsupported`.
    Stage,
    /// Not a voice channel at all.
    Other,
}

/// What the join checks need to know about a channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceChannelInfo {
    pub kind: VoiceChannelKind,
    /// 0 = unlimited.
    pub user_limit: u32,
    pub bitrate: Option<u32>,
}

/// Discord's voice state object, plus nothing SIDE ROOM specific.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceState {
    pub guild_id: GuildId,
    pub channel_id: ChannelId,
    pub user_id: UserId,
    /// The gateway session that holds the connection.
    pub session_id: String,
    /// Server mute / deafen (moderators).
    pub mute: bool,
    pub deaf: bool,
    pub self_mute: bool,
    pub self_deaf: bool,
    pub self_video: bool,
    pub self_stream: bool,
    /// Stage audience (reserved).
    pub suppress: bool,
}

impl VoiceState {
    /// Discord shape. `GUILD_CREATE` entries omit `guild_id`.
    pub fn to_json(&self, with_guild: bool, member: Option<Value>) -> Value {
        let mut v = json!({
            "channel_id": self.channel_id,
            "user_id": self.user_id,
            "session_id": self.session_id,
            "deaf": self.deaf,
            "mute": self.mute,
            "self_deaf": self.self_deaf,
            "self_mute": self.self_mute,
            "self_stream": self.self_stream,
            "self_video": self.self_video,
            "suppress": self.suppress,
            "request_to_speak_timestamp": null,
        });
        if let Some(o) = v.as_object_mut() {
            if with_guild {
                o.insert("guild_id".into(), json!(self.guild_id));
            }
            if let Some(m) = member {
                o.insert("member".into(), m);
            }
        }
        v
    }
}

// ---- join checks -------------------------------------------------------------------------

/// Why a join (op 4) was refused. Sent as `SR_VOICE_ERROR.code`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinDenied {
    NotMember,
    UnknownChannel,
    NotVoiceChannel,
    StageUnsupported,
    MissingPermission,
    ChannelFull,
    Unavailable,
    BadRequest,
}

impl JoinDenied {
    pub const fn code(self) -> &'static str {
        match self {
            Self::NotMember => "not_member",
            Self::UnknownChannel => "unknown_channel",
            Self::NotVoiceChannel => "not_voice_channel",
            Self::StageUnsupported => "stage_unsupported",
            Self::MissingPermission => "missing_permission",
            Self::ChannelFull => "channel_full",
            Self::Unavailable => "voice_unavailable",
            Self::BadRequest => "bad_request",
        }
    }

    pub const fn message(self) -> &'static str {
        match self {
            Self::NotMember => "You are not a member of this server.",
            Self::UnknownChannel => "This channel does not exist.",
            Self::NotVoiceChannel => "This is not a voice channel.",
            Self::StageUnsupported => "Stage channels are not available yet.",
            Self::MissingPermission => "You do not have permission to join this voice channel.",
            Self::ChannelFull => "This voice channel is full.",
            Self::Unavailable => "Voice is not available on this server right now.",
            Self::BadRequest => "The voice state update was malformed.",
        }
    }
}

/// Who may enter `channel`. `perms` are the member's effective channel permissions;
/// `occupants` excludes the joiner; `already_inside` is a re-send for the same channel.
pub fn check_join(
    info: &VoiceChannelInfo,
    perms: Permissions,
    occupants: usize,
    already_inside: bool,
) -> Result<(), JoinDenied> {
    match info.kind {
        VoiceChannelKind::Other => return Err(JoinDenied::NotVoiceChannel),
        VoiceChannelKind::Stage => return Err(JoinDenied::StageUnsupported),
        VoiceChannelKind::Voice => {}
    }
    if !perms.contains(Permissions::VIEW_CHANNEL | Permissions::CONNECT) {
        return Err(JoinDenied::MissingPermission);
    }
    let bypass = perms.intersects(
        Permissions::MOVE_MEMBERS | Permissions::MANAGE_CHANNELS | Permissions::ADMINISTRATOR,
    );
    if info.user_limit > 0 && !already_inside && occupants >= info.user_limit as usize && !bypass {
        return Err(JoinDenied::ChannelFull);
    }
    Ok(())
}

/// What the SFU enforces for one participant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaFlags {
    pub speak: bool,
    pub deaf: bool,
    pub ptt_required: bool,
    pub priority: bool,
}

/// Media-path flags from permissions and state. Self-mute also stops forwarding, so a
/// client that claims to be muted can never leak audio.
pub fn media_flags(perms: Permissions, st: &VoiceState) -> MediaFlags {
    MediaFlags {
        speak: perms.contains(Permissions::SPEAK) && !st.mute && !st.self_mute && !st.suppress,
        deaf: st.deaf || st.self_deaf,
        // D-046: voice activity or push-to-talk, and a channel can force push-to-talk by denying USE_VAD.
        ptt_required: !perms.contains(Permissions::USE_VAD),
        priority: perms.contains(Permissions::PRIORITY_SPEAKER),
    }
}

// ---- moderation -------------------------------------------------------------------------------

/// A moderator's change to someone else's voice state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModPatch {
    pub mute: Option<bool>,
    pub deaf: Option<bool>,
    /// `Some(None)` disconnects, `Some(Some(c))` moves.
    pub channel: Option<Option<ChannelId>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModError {
    NotInVoice,
    MissingPermissions,
    UnknownChannel,
    NotVoiceChannel,
    StageUnsupported,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModAction {
    Muted(bool),
    Deafened(bool),
    Moved { from: ChannelId, to: ChannelId },
    Disconnected { from: ChannelId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModOutcome {
    /// The state after the change; `None` when the user was disconnected.
    pub state: Option<VoiceState>,
    pub actions: Vec<ModAction>,
}

/// Permission rules for [`ModPatch`]. `src` are the actor's permissions in the target's
/// current channel, `dst` in the destination channel when moving.
pub fn authorize(
    patch: &ModPatch,
    src: Permissions,
    dst: Option<Permissions>,
) -> Result<(), ModError> {
    if patch.mute.is_some() && !src.contains(Permissions::MUTE_MEMBERS) {
        return Err(ModError::MissingPermissions);
    }
    if patch.deaf.is_some() && !src.contains(Permissions::DEAFEN_MEMBERS) {
        return Err(ModError::MissingPermissions);
    }
    match patch.channel {
        None => {}
        Some(None) => {
            if !src.contains(Permissions::MOVE_MEMBERS) {
                return Err(ModError::MissingPermissions);
            }
        }
        Some(Some(_)) => {
            let dst = dst.ok_or(ModError::MissingPermissions)?;
            if !src.contains(Permissions::MOVE_MEMBERS)
                || !dst.contains(Permissions::MOVE_MEMBERS | Permissions::CONNECT)
            {
                return Err(ModError::MissingPermissions);
            }
        }
    }
    Ok(())
}

// ---- gateway side ------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct VoiceStateIn {
    guild_id: Option<GuildId>,
    channel_id: Option<ChannelId>,
    #[serde(default)]
    self_mute: bool,
    #[serde(default)]
    self_deaf: bool,
    #[serde(default)]
    self_video: bool,
    #[serde(default)]
    self_stream: bool,
}

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Gateway {
    fn voice_cfg(&self) -> Option<&VoiceConfig> {
        self.inner.cfg.voice.as_ref()
    }

    /// The voice state of `user`, if connected to a voice channel.
    pub fn voice_state(&self, user: UserId) -> Option<VoiceState> {
        self.st().voice.get(&user).cloned()
    }

    /// Everyone in `channel`.
    pub fn voice_occupants(&self, channel: ChannelId) -> Vec<VoiceState> {
        self.st()
            .voice
            .values()
            .filter(|s| s.channel_id == channel)
            .cloned()
            .collect()
    }

    async fn relay(&self, cmd: VoiceCommand) {
        if let Some(r) = self.voice_cfg().and_then(|v| v.relay.clone()) {
            r.apply(cmd).await;
        }
    }

    async fn relay_flags(&self, st: &VoiceState, f: MediaFlags) {
        self.relay(VoiceCommand::SetFlags {
            channel: st.channel_id,
            user: st.user_id,
            speak: f.speak,
            deaf: f.deaf,
            ptt_required: f.ptt_required,
            priority: f.priority,
        })
        .await;
    }

    fn voice_error(
        &self,
        s: &Session,
        guild: Option<GuildId>,
        channel: Option<ChannelId>,
        e: JoinDenied,
    ) {
        s.deliver(
            "SR_VOICE_ERROR",
            json!({
                "guild_id": guild, "channel_id": channel,
                "code": e.code(), "message": e.message(),
            }),
        );
    }

    /// `voice_states` for `GUILD_CREATE`: the states in `guild` whose channel `user` can see.
    pub(crate) async fn voice_states_for(&self, user: UserId, guild: GuildId) -> Vec<Value> {
        let states: Vec<VoiceState> = self
            .st()
            .voice
            .values()
            .filter(|s| s.guild_id == guild)
            .cloned()
            .collect();
        let mut seen: HashMap<ChannelId, bool> = HashMap::new();
        let mut out = Vec::new();
        for s in states {
            let ok = match seen.get(&s.channel_id) {
                Some(v) => *v,
                None => {
                    let v = self
                        .inner
                        .dir
                        .can_view_channel(user, guild, s.channel_id)
                        .await;
                    seen.insert(s.channel_id, v);
                    v
                }
            };
            if ok || s.user_id == user {
                out.push(s.to_json(false, None));
            }
        }
        out
    }

    /// Deliver a `VOICE_STATE_UPDATE` to every session in `guild` that may see one of
    /// `channels`, and always to `always` (the affected user).
    async fn dispatch_voice_state(
        &self,
        guild: GuildId,
        body: Value,
        channels: &[ChannelId],
        always: &[UserId],
    ) {
        let targets: Vec<Arc<Session>> = self
            .guild_sessions(guild)
            .into_iter()
            .filter(|s| s.wants("VOICE_STATE_UPDATE"))
            .collect();
        let users: HashSet<UserId> = targets.iter().map(|s| s.user).collect();
        let checks = users.into_iter().map(|u| {
            let dir = self.inner.dir.clone();
            async move {
                if always.contains(&u) {
                    return (u, true);
                }
                for c in channels {
                    if dir.can_view_channel(u, guild, *c).await {
                        return (u, true);
                    }
                }
                (u, false)
            }
        });
        let allowed: HashMap<UserId, bool> = futures::future::join_all(checks)
            .await
            .into_iter()
            .collect();
        for s in targets {
            if allowed.get(&s.user).copied().unwrap_or(false) {
                s.deliver("VOICE_STATE_UPDATE", body.clone());
            }
        }
    }

    /// `VOICE_SERVER_UPDATE` to the session that holds the connection.
    fn voice_server_update(&self, st: &VoiceState, flags: MediaFlags) -> Result<(), JoinDenied> {
        let vc = self.voice_cfg().ok_or(JoinDenied::Unavailable)?;
        let endpoint =
            select_endpoint(st.channel_id, &vc.endpoints).ok_or(JoinDenied::Unavailable)?;
        let now = unix_secs();
        let token = vc
            .signer
            .sign(&JoinClaims {
                user_id: st.user_id,
                guild_id: Some(st.guild_id),
                channel_id: st.channel_id,
                session_id: st.session_id.clone(),
                exp: now + TOKEN_TTL_SECS,
                speak: flags.speak,
                deaf: flags.deaf,
                ptt_required: flags.ptt_required,
                priority: flags.priority,
            })
            .ok_or(JoinDenied::Unavailable)?;
        let session = self.st().sessions.get(&st.session_id).cloned();
        if let Some(s) = session {
            s.deliver(
                "VOICE_SERVER_UPDATE",
                json!({
                    "token": token,
                    "guild_id": st.guild_id,
                    "channel_id": st.channel_id,
                    "endpoint": endpoint,
                    "ice_servers": vc.turn.ice_servers(st.user_id, now),
                }),
            );
        }
        Ok(())
    }

    /// Gateway op 4.
    pub(crate) async fn voice_state_update(&self, s: &Arc<Session>, d: Value) {
        let parsed = d
            .as_object()
            .filter(|o| o.contains_key("channel_id"))
            .and_then(|_| serde_json::from_value::<VoiceStateIn>(d.clone()).ok());
        let Some(inp) = parsed else {
            self.voice_error(s, None, None, JoinDenied::BadRequest);
            return;
        };
        match inp.channel_id {
            None => {
                self.voice_leave(s.user).await;
            }
            Some(channel) => {
                let Some(guild) = inp.guild_id else {
                    self.voice_error(s, None, Some(channel), JoinDenied::BadRequest);
                    return;
                };
                if let Err(e) = self.voice_join(s, guild, channel, &inp).await {
                    self.voice_error(s, Some(guild), Some(channel), e);
                }
            }
        }
    }

    async fn voice_join(
        &self,
        s: &Arc<Session>,
        guild: GuildId,
        channel: ChannelId,
        inp: &VoiceStateIn,
    ) -> Result<(), JoinDenied> {
        if self.voice_cfg().is_none() {
            return Err(JoinDenied::Unavailable);
        }
        let dir = &self.inner.dir;
        let info = dir
            .voice_channel(guild, channel)
            .await
            .ok_or(JoinDenied::UnknownChannel)?;
        let perms = dir
            .channel_permissions(s.user, guild, channel)
            .await
            .ok_or(JoinDenied::NotMember)?;
        let (cur, occupants) = {
            let st = self.st();
            (
                st.voice.get(&s.user).cloned(),
                st.voice
                    .values()
                    .filter(|v| v.channel_id == channel && v.user_id != s.user)
                    .count(),
            )
        };
        let inside = cur.as_ref().is_some_and(|c| c.channel_id == channel);
        check_join(&info, perms, occupants, inside)?;

        // Server mute and deafen follow the member within a guild.
        let carry = cur.as_ref().filter(|c| c.guild_id == guild);
        let new = VoiceState {
            guild_id: guild,
            channel_id: channel,
            user_id: s.user,
            session_id: s.id.clone(),
            mute: carry.is_some_and(|c| c.mute),
            deaf: carry.is_some_and(|c| c.deaf),
            self_mute: inp.self_mute,
            self_deaf: inp.self_deaf,
            self_video: inp.self_video,
            self_stream: inp.self_stream,
            suppress: false,
        };
        let flags = media_flags(perms, &new);
        // Refuse before changing anything if no SFU can be named.
        let fresh_join = !(inside && cur.as_ref().is_some_and(|c| c.session_id == s.id));
        if fresh_join {
            self.voice_server_update_check(&new)?;
        }

        if let Some(old) = &cur
            && old.channel_id != channel
        {
            self.relay(VoiceCommand::Disconnect {
                channel: old.channel_id,
                user: s.user,
            })
            .await;
        }
        self.st().voice.insert(s.user, new.clone());

        let member = dir.member_payload(guild, s.user).await;
        let mut channels = vec![channel];
        if let Some(old) = &cur {
            if old.guild_id == guild {
                channels.push(old.channel_id);
            } else {
                // Moving between guilds: the old guild sees a leave.
                self.announce_left(old).await;
            }
        }
        self.dispatch_voice_state(guild, new.to_json(true, member), &channels, &[s.user])
            .await;
        if fresh_join {
            self.voice_server_update(&new, flags)?;
        } else {
            self.relay_flags(&new, flags).await;
        }
        Ok(())
    }

    /// Fail early when a token cannot be issued.
    fn voice_server_update_check(&self, st: &VoiceState) -> Result<(), JoinDenied> {
        let vc = self.voice_cfg().ok_or(JoinDenied::Unavailable)?;
        select_endpoint(st.channel_id, &vc.endpoints)
            .map(|_| ())
            .ok_or(JoinDenied::Unavailable)
    }

    async fn announce_left(&self, old: &VoiceState) {
        let mut body = old.to_json(true, None);
        if let Some(o) = body.as_object_mut() {
            o.insert("channel_id".into(), Value::Null);
        }
        self.dispatch_voice_state(old.guild_id, body, &[old.channel_id], &[old.user_id])
            .await;
    }

    /// Remove `user` from voice and tell everyone. Returns the removed state.
    pub async fn voice_leave(&self, user: UserId) -> Option<VoiceState> {
        let old = self.st().voice.remove(&user)?;
        self.relay(VoiceCommand::Disconnect {
            channel: old.channel_id,
            user,
        })
        .await;
        self.announce_left(&old).await;
        Some(old)
    }

    /// A moderator changes someone else's voice state (REST: `PATCH /guilds/{id}/voice-states/{user}`).
    pub async fn moderate_voice(
        &self,
        actor: UserId,
        guild: GuildId,
        target: UserId,
        patch: ModPatch,
    ) -> Result<ModOutcome, ModError> {
        if self.voice_cfg().is_none() {
            return Err(ModError::Unavailable);
        }
        let dir = &self.inner.dir;
        // Strangers learn nothing about who is in voice: the actor must be a member first.
        if !dir
            .has_guild_permission(actor, guild, Permissions::empty())
            .await
        {
            return Err(ModError::MissingPermissions);
        }
        let cur = self
            .voice_state(target)
            .filter(|s| s.guild_id == guild)
            .ok_or(ModError::NotInVoice)?;
        let src = dir
            .channel_permissions(actor, guild, cur.channel_id)
            .await
            .ok_or(ModError::MissingPermissions)?;
        let mut dst = None;
        if let Some(Some(to)) = patch.channel {
            let info = dir
                .voice_channel(guild, to)
                .await
                .ok_or(ModError::UnknownChannel)?;
            match info.kind {
                VoiceChannelKind::Voice => {}
                VoiceChannelKind::Stage => return Err(ModError::StageUnsupported),
                VoiceChannelKind::Other => return Err(ModError::NotVoiceChannel),
            }
            dst = dir.channel_permissions(actor, guild, to).await;
        }
        authorize(&patch, src, dst)?;

        let mut actions = Vec::new();
        let mut new = cur.clone();
        if let Some(m) = patch.mute.filter(|m| *m != cur.mute) {
            new.mute = m;
            actions.push(ModAction::Muted(m));
        }
        if let Some(d) = patch.deaf.filter(|d| *d != cur.deaf) {
            new.deaf = d;
            actions.push(ModAction::Deafened(d));
        }

        match patch.channel {
            Some(None) => {
                // Re-read under the lock so two moderators do not both "disconnect" one user.
                if self.voice_leave(target).await.is_none() {
                    return Err(ModError::NotInVoice);
                }
                actions.push(ModAction::Disconnected {
                    from: cur.channel_id,
                });
                return Ok(ModOutcome {
                    state: None,
                    actions,
                });
            }
            Some(Some(to)) if to != cur.channel_id => {
                new.channel_id = to;
                actions.push(ModAction::Moved {
                    from: cur.channel_id,
                    to,
                });
            }
            _ => {}
        }
        if actions.is_empty() {
            return Ok(ModOutcome {
                state: Some(cur),
                actions,
            });
        }

        let moved = new.channel_id != cur.channel_id;
        if moved {
            self.relay(VoiceCommand::Disconnect {
                channel: cur.channel_id,
                user: target,
            })
            .await;
        }
        self.st().voice.insert(target, new.clone());
        let target_perms = dir
            .channel_permissions(target, guild, new.channel_id)
            .await
            .unwrap_or_else(Permissions::empty);
        let flags = media_flags(target_perms, &new);
        let member = dir.member_payload(guild, target).await;
        let channels = if moved {
            vec![new.channel_id, cur.channel_id]
        } else {
            vec![new.channel_id]
        };
        self.dispatch_voice_state(guild, new.to_json(true, member), &channels, &[target])
            .await;
        if moved {
            // The moved member reconnects to the new room with a fresh token.
            let _ = self.voice_server_update(&new, flags);
        } else {
            self.relay_flags(&new, flags).await;
        }
        Ok(ModOutcome {
            state: Some(new),
            actions,
        })
    }

    /// Remove states whose session is gone or stayed detached past the grace period.
    pub(crate) async fn voice_sweep(&self) {
        let Some(grace) = self.voice_cfg().map(|v| v.session_grace) else {
            return;
        };
        let now = Instant::now();
        let stale: Vec<UserId> = {
            let st = self.st();
            st.voice
                .values()
                .filter(|v| match st.sessions.get(&v.session_id) {
                    None => true,
                    Some(s) => s.detached_for(now).is_some_and(|d| d > grace),
                })
                .map(|v| v.user_id)
                .collect()
        };
        for u in stale {
            self.voice_leave(u).await;
        }
    }

    pub(crate) async fn voice_member_left(&self, guild: GuildId, user: UserId) {
        if self.voice_state(user).is_some_and(|s| s.guild_id == guild) {
            self.voice_leave(user).await;
        }
    }

    pub(crate) async fn voice_guild_deleted(&self, guild: GuildId) {
        let removed: Vec<VoiceState> = {
            let mut st = self.st();
            let users: Vec<UserId> = st
                .voice
                .values()
                .filter(|v| v.guild_id == guild)
                .map(|v| v.user_id)
                .collect();
            users.iter().filter_map(|u| st.voice.remove(u)).collect()
        };
        let channels: HashSet<ChannelId> = removed.iter().map(|v| v.channel_id).collect();
        for c in channels {
            self.relay(VoiceCommand::CloseRoom { channel: c }).await;
        }
    }

    async fn voice_channel_deleted(&self, guild: GuildId, channel: ChannelId) {
        let users: Vec<UserId> = self
            .voice_occupants(channel)
            .into_iter()
            .filter(|v| v.guild_id == guild)
            .map(|v| v.user_id)
            .collect();
        for u in users {
            self.voice_leave(u).await;
        }
        self.relay(VoiceCommand::CloseRoom { channel }).await;
    }

    /// Permissions changed somewhere in the guild: drop members who lost access and
    /// refresh everyone's media flags.
    async fn voice_revalidate(&self, guild: GuildId) {
        let states: Vec<VoiceState> = self
            .st()
            .voice
            .values()
            .filter(|v| v.guild_id == guild)
            .cloned()
            .collect();
        for s in states {
            let perms = self
                .inner
                .dir
                .channel_permissions(s.user_id, guild, s.channel_id)
                .await;
            match perms {
                Some(p) if p.contains(Permissions::VIEW_CHANNEL | Permissions::CONNECT) => {
                    self.relay_flags(&s, media_flags(p, &s)).await;
                }
                _ => {
                    self.voice_leave(s.user_id).await;
                }
            }
        }
    }

    /// Hook called for every guild-topic event.
    pub(crate) async fn voice_on_guild_event(&self, guild: GuildId, ev: &DomainEvent) {
        if self.voice_cfg().is_none() || !self.st().voice.values().any(|v| v.guild_id == guild) {
            return;
        }
        match ev {
            DomainEvent::ChannelDeleted { channel, .. } => {
                self.voice_channel_deleted(guild, *channel).await;
            }
            DomainEvent::ChannelUpdated { .. }
            | DomainEvent::RoleUpdated { .. }
            | DomainEvent::RoleDeleted { .. }
            | DomainEvent::MemberUpdated { .. } => self.voice_revalidate(guild).await,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Permissions as P;

    fn info(limit: u32) -> VoiceChannelInfo {
        VoiceChannelInfo {
            kind: VoiceChannelKind::Voice,
            user_limit: limit,
            bitrate: Some(64_000),
        }
    }

    fn member() -> Permissions {
        P::VIEW_CHANNEL | P::CONNECT | P::SPEAK | P::USE_VAD
    }

    fn state() -> VoiceState {
        VoiceState {
            guild_id: GuildId::new(1),
            channel_id: ChannelId::new(2),
            user_id: UserId::new(3),
            session_id: "s".into(),
            mute: false,
            deaf: false,
            self_mute: false,
            self_deaf: false,
            self_video: false,
            self_stream: false,
            suppress: false,
        }
    }

    #[test]
    fn connect_and_view_are_required() {
        assert_eq!(check_join(&info(0), member(), 0, false), Ok(()));
        assert_eq!(
            check_join(&info(0), member() - P::CONNECT, 0, false),
            Err(JoinDenied::MissingPermission)
        );
        assert_eq!(
            check_join(&info(0), member() - P::VIEW_CHANNEL, 0, false),
            Err(JoinDenied::MissingPermission)
        );
        assert_eq!(
            check_join(&info(0), P::empty(), 0, false),
            Err(JoinDenied::MissingPermission)
        );
    }

    #[test]
    fn user_limit_blocks_unless_bypassed_or_already_inside() {
        assert_eq!(check_join(&info(2), member(), 1, false), Ok(()));
        assert_eq!(
            check_join(&info(2), member(), 2, false),
            Err(JoinDenied::ChannelFull)
        );
        // Re-sending op 4 for the channel you are in is not a new join.
        assert_eq!(check_join(&info(2), member(), 2, true), Ok(()));
        // Move Members, Manage Channels and Administrator walk past the limit.
        for bypass in [
            P::MOVE_MEMBERS,
            P::MANAGE_CHANNELS,
            P::ADMINISTRATOR,
            P::ALL,
        ] {
            assert_eq!(check_join(&info(2), member() | bypass, 5, false), Ok(()));
        }
        // 0 means unlimited.
        assert_eq!(check_join(&info(0), member(), 500, false), Ok(()));
    }

    #[test]
    fn only_voice_channels_can_be_joined() {
        let mut i = info(0);
        i.kind = VoiceChannelKind::Other;
        assert_eq!(
            check_join(&i, member(), 0, false),
            Err(JoinDenied::NotVoiceChannel)
        );
        i.kind = VoiceChannelKind::Stage;
        assert_eq!(
            check_join(&i, member(), 0, false),
            Err(JoinDenied::StageUnsupported)
        );
    }

    #[test]
    fn media_flags_follow_permissions_and_state() {
        let st = state();
        let f = media_flags(member(), &st);
        assert_eq!(
            f,
            MediaFlags {
                speak: true,
                deaf: false,
                ptt_required: false,
                priority: false
            }
        );
        // No SPEAK: listen only.
        assert!(!media_flags(member() - P::SPEAK, &st).speak);
        // Server mute and self mute both stop forwarding.
        assert!(
            !media_flags(
                member(),
                &VoiceState {
                    mute: true,
                    ..state()
                }
            )
            .speak
        );
        assert!(
            !media_flags(
                member(),
                &VoiceState {
                    self_mute: true,
                    ..state()
                }
            )
            .speak
        );
        // Either deafen means the SFU sends nothing.
        assert!(
            media_flags(
                member(),
                &VoiceState {
                    deaf: true,
                    ..state()
                }
            )
            .deaf
        );
        assert!(
            media_flags(
                member(),
                &VoiceState {
                    self_deaf: true,
                    ..state()
                }
            )
            .deaf
        );
        // D-046: denying USE_VAD forces push-to-talk.
        assert!(media_flags(member() - P::USE_VAD, &st).ptt_required);
        assert!(media_flags(member() | P::PRIORITY_SPEAKER, &st).priority);
    }

    #[test]
    fn moderation_needs_the_matching_permission() {
        let mute = ModPatch {
            mute: Some(true),
            ..ModPatch::default()
        };
        assert_eq!(authorize(&mute, P::MUTE_MEMBERS, None), Ok(()));
        assert_eq!(
            authorize(&mute, P::DEAFEN_MEMBERS | P::MOVE_MEMBERS, None),
            Err(ModError::MissingPermissions)
        );
        let deaf = ModPatch {
            deaf: Some(true),
            ..ModPatch::default()
        };
        assert_eq!(authorize(&deaf, P::DEAFEN_MEMBERS, None), Ok(()));
        assert_eq!(
            authorize(&deaf, P::MUTE_MEMBERS, None),
            Err(ModError::MissingPermissions)
        );
        // Muting and deafening in one request needs both.
        let both = ModPatch {
            mute: Some(true),
            deaf: Some(true),
            ..ModPatch::default()
        };
        assert_eq!(
            authorize(&both, P::MUTE_MEMBERS, None),
            Err(ModError::MissingPermissions)
        );
        assert_eq!(
            authorize(&both, P::MUTE_MEMBERS | P::DEAFEN_MEMBERS, None),
            Ok(())
        );
    }

    #[test]
    fn disconnect_and_move_need_move_members() {
        let disconnect = ModPatch {
            channel: Some(None),
            ..ModPatch::default()
        };
        assert_eq!(authorize(&disconnect, P::MOVE_MEMBERS, None), Ok(()));
        assert_eq!(
            authorize(&disconnect, P::MUTE_MEMBERS, None),
            Err(ModError::MissingPermissions)
        );
        let mv = ModPatch {
            channel: Some(Some(ChannelId::new(9))),
            ..ModPatch::default()
        };
        let dst_ok = P::MOVE_MEMBERS | P::CONNECT;
        assert_eq!(authorize(&mv, P::MOVE_MEMBERS, Some(dst_ok)), Ok(()));
        // Move Members on the source alone is not enough: the destination must allow it too.
        assert_eq!(
            authorize(&mv, P::MOVE_MEMBERS, Some(P::CONNECT)),
            Err(ModError::MissingPermissions)
        );
        assert_eq!(
            authorize(&mv, P::MOVE_MEMBERS, Some(P::MOVE_MEMBERS)),
            Err(ModError::MissingPermissions)
        );
        assert_eq!(
            authorize(&mv, P::MOVE_MEMBERS, None),
            Err(ModError::MissingPermissions)
        );
        assert_eq!(
            authorize(&mv, P::CONNECT, Some(dst_ok)),
            Err(ModError::MissingPermissions)
        );
        // Owners and administrators hold every bit.
        assert_eq!(authorize(&mv, P::ALL, Some(P::ALL)), Ok(()));
    }

    #[test]
    fn json_matches_discords_shape() {
        let v = state().to_json(true, Some(json!({"nick": "n"})));
        for k in [
            "guild_id",
            "channel_id",
            "user_id",
            "session_id",
            "deaf",
            "mute",
            "self_deaf",
            "self_mute",
            "self_stream",
            "self_video",
            "suppress",
            "member",
        ] {
            assert!(v.get(k).is_some(), "missing {k}");
        }
        assert_eq!(v["user_id"], "3");
        assert!(v["request_to_speak_timestamp"].is_null());
        assert!(state().to_json(false, None).get("guild_id").is_none());
    }

    #[test]
    fn error_codes_are_stable() {
        assert_eq!(JoinDenied::ChannelFull.code(), "channel_full");
        assert_eq!(JoinDenied::MissingPermission.code(), "missing_permission");
        assert_eq!(JoinDenied::Unavailable.code(), "voice_unavailable");
        assert_eq!(JoinDenied::StageUnsupported.code(), "stage_unsupported");
    }
}
