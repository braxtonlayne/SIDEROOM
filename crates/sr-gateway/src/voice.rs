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
    /// Told when a DM call ends, to write the call record. `None`: no record.
    pub calls: Option<Arc<dyn crate::voice_call::CallHooks>>,
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
    /// Stage channel: the audience is suppressed until a moderator invites them to speak
    /// (VOX-STG-001).
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
    /// Age-restricted (NSFW) channel: joining needs the 18+ confirmation (D-032).
    pub age_restricted: bool,
}

/// Per-guild voice settings the gateway acts on (VOX-SIG-003).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VoiceGuildInfo {
    /// Channel idle members are moved to. Members there can listen but not speak.
    pub afk_channel: Option<ChannelId>,
    /// Seconds of silence before the move; 0 turns the move off.
    pub afk_timeout_secs: u32,
}

/// The video allowance of one member in one place (doc 08 section 5, D-050..D-052).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceLimits {
    /// Simultaneous video senders (camera or screen share, one each) per voice channel.
    pub video_senders: u16,
    /// Viewers per stream.
    pub stream_viewers: u16,
    /// Patrons never count against the sender allowance of a channel (D-052).
    pub exempt: bool,
    /// Allowed `(height, fps)` ceilings; a request fits when one ceiling covers it.
    pub profiles: Vec<(u16, u8)>,
    /// Governor tier: the server level (0 to 3), or 4 for a Patron.
    pub tier: u8,
}

impl VoiceLimits {
    /// Level 0: 5 senders, 25 viewers, 720p30.
    pub fn free() -> Self {
        Self {
            video_senders: 5,
            stream_viewers: 25,
            exempt: false,
            profiles: vec![(720, 30)],
            tier: 0,
        }
    }

    /// Whether a stream or camera of `height` x `fps` is allowed.
    pub fn allows(&self, height: u16, fps: u8) -> bool {
        self.profiles.iter().any(|(h, f)| height <= *h && fps <= *f)
    }
}

/// Discord's voice state object, plus nothing SIDE ROOM specific.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceState {
    /// `None` for a DM or group DM call.
    pub guild_id: Option<GuildId>,
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
    /// Stage audience: connected but not allowed to speak.
    pub suppress: bool,
    /// Stage: when the member asked to speak (unix ms), `None` when not asking.
    pub request_to_speak_ms: Option<i64>,
    /// In the guild's AFK channel: listens only.
    pub afk: bool,
    /// Does not count against the channel's sender allowance (Patron, D-052).
    pub exempt: bool,
    /// The allowance tier for the egress governor: the server level, or 4 for a Patron.
    pub tier: u8,
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
            "request_to_speak_timestamp": self.request_to_speak_ms.map(crate::hub::iso_utc),
        });
        if let Some(o) = v.as_object_mut() {
            if with_guild && let Some(g) = self.guild_id {
                o.insert("guild_id".into(), json!(g));
            }
            if let Some(m) = member {
                o.insert("member".into(), m);
            }
        }
        v
    }

    /// Counts toward the channel's video-sender allowance.
    pub fn video_sends(&self) -> u16 {
        u16::from(self.self_video) + u16::from(self.self_stream)
    }
}

// ---- join checks -------------------------------------------------------------------------

/// Why a join (op 4) was refused. Sent as `SR_VOICE_ERROR.code`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinDenied {
    NotMember,
    UnknownChannel,
    NotVoiceChannel,
    MissingPermission,
    ChannelFull,
    Unavailable,
    BadRequest,
    /// Age-restricted channel and the account has not answered the 18+ question (D-032).
    AgeConfirmationRequired,
    /// Starting a camera or stream would exceed the channel sender allowance (D-050).
    VideoLimitReached,
    /// The requested stream quality is above what the member may send.
    StreamQualityNotAllowed,
    /// The stream already has as many viewers as its cap allows.
    StreamFull,
    UnknownStream,
    /// The op needs the caller to be in the voice channel it names.
    NotInVoice,
    /// The caller already shares this screen.
    AlreadyStreaming,
}

impl JoinDenied {
    pub const fn code(self) -> &'static str {
        match self {
            Self::NotMember => "not_member",
            Self::UnknownChannel => "unknown_channel",
            Self::NotVoiceChannel => "not_voice_channel",
            Self::AgeConfirmationRequired => "age_confirmation_required",
            Self::VideoLimitReached => "video_limit_reached",
            Self::StreamQualityNotAllowed => "stream_quality_not_allowed",
            Self::StreamFull => "stream_full",
            Self::UnknownStream => "unknown_stream",
            Self::NotInVoice => "not_in_voice",
            Self::AlreadyStreaming => "already_streaming",
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
            Self::AgeConfirmationRequired => {
                "Confirm that you are 18 or older to join this channel."
            }
            Self::VideoLimitReached => "This channel has reached its video limit.",
            Self::StreamQualityNotAllowed => "That stream quality is not available to you here.",
            Self::StreamFull => "This stream is full.",
            Self::UnknownStream => "This stream does not exist.",
            Self::NotInVoice => "Join the voice channel first.",
            Self::AlreadyStreaming => "You are already sharing your screen.",
            Self::MissingPermission => "You do not have permission to join this voice channel.",
            Self::ChannelFull => "This voice channel is full.",
            Self::Unavailable => "Voice is not available on this server right now.",
            Self::BadRequest => "The voice state update was malformed.",
        }
    }
}

/// Who may enter `channel`. `perms` are the member's effective channel permissions;
/// `occupants` excludes the joiner; `already_inside` is a re-send for the same channel;
/// `age_confirmed` is the account's 18+ answer (VOX-SIG-003).
pub fn check_join(
    info: &VoiceChannelInfo,
    perms: Permissions,
    occupants: usize,
    already_inside: bool,
    age_confirmed: bool,
) -> Result<(), JoinDenied> {
    if info.kind == VoiceChannelKind::Other {
        return Err(JoinDenied::NotVoiceChannel);
    }
    if !perms.contains(Permissions::VIEW_CHANNEL | Permissions::CONNECT) {
        return Err(JoinDenied::MissingPermission);
    }
    if info.age_restricted && !age_confirmed {
        return Err(JoinDenied::AgeConfirmationRequired);
    }
    let bypass = perms.intersects(
        Permissions::MOVE_MEMBERS | Permissions::MANAGE_CHANNELS | Permissions::ADMINISTRATOR,
    );
    if info.user_limit > 0 && !already_inside && occupants >= info.user_limit as usize && !bypass {
        return Err(JoinDenied::ChannelFull);
    }
    Ok(())
}

/// Stage moderators (Discord): all of Manage Channels, Mute Members and Move Members.
/// They speak at once and run the stage.
pub fn is_stage_moderator(perms: Permissions) -> bool {
    perms.contains(
        Permissions::MANAGE_CHANNELS | Permissions::MUTE_MEMBERS | Permissions::MOVE_MEMBERS,
    )
}

/// What the SFU enforces for one participant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaFlags {
    pub speak: bool,
    pub deaf: bool,
    pub ptt_required: bool,
    pub priority: bool,
    /// The sender allowance let this member start a camera.
    pub camera: bool,
    /// ... or a screen share.
    pub screen: bool,
}

/// Media-path flags from permissions and state. Self-mute also stops forwarding, so a
/// client that claims to be muted can never leak audio. The stage audience and the AFK
/// channel listen only.
pub fn media_flags(perms: Permissions, st: &VoiceState) -> MediaFlags {
    MediaFlags {
        speak: perms.contains(Permissions::SPEAK)
            && !st.mute
            && !st.self_mute
            && !st.suppress
            && !st.afk,
        deaf: st.deaf || st.self_deaf,
        // D-046: voice activity or push-to-talk, and a channel can force push-to-talk by denying USE_VAD.
        ptt_required: !perms.contains(Permissions::USE_VAD),
        priority: perms.contains(Permissions::PRIORITY_SPEAKER),
        // The allowance checks already refused a camera or stream that does not fit, so the
        // flags on the state are what the SFU may accept. A stage audience, the AFK channel
        // and members without the permission publish nothing.
        camera: st.self_video && !st.suppress && !st.afk && perms.contains(Permissions::STREAM),
        screen: st.self_stream && !st.suppress && !st.afk && perms.contains(Permissions::STREAM),
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
    /// Stage only: `false` invites the member to speak, `true` moves them back to the
    /// audience. Needs Mute Members.
    pub suppress: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModError {
    NotInVoice,
    MissingPermissions,
    UnknownChannel,
    NotVoiceChannel,
    /// `suppress` only means something in a stage channel.
    NotStageChannel,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModAction {
    Muted(bool),
    Deafened(bool),
    Moved {
        from: ChannelId,
        to: ChannelId,
    },
    Disconnected {
        from: ChannelId,
    },
    /// Stage: invited to speak (`false`) or moved to the audience (`true`).
    Suppressed(bool),
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
    if (patch.mute.is_some() || patch.suppress.is_some())
        && !src.contains(Permissions::MUTE_MEMBERS)
    {
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

/// State that hangs off the voice states: AFK clocks, streams, stages and DM calls.
#[derive(Default)]
pub(crate) struct VoiceExtra {
    pub active: HashMap<UserId, Instant>,
    pub streams: HashMap<String, crate::voice_stream::StreamState>,
    pub stages: HashMap<ChannelId, crate::voice_stage::StageInstance>,
    pub calls: HashMap<ChannelId, crate::voice_call::CallState>,
    /// One soundboard play per member per few seconds.
    pub cooldown: sr_chat::settings::soundboard::PlayCooldown,
}

/// What a member of a DM call may do: everything a voice channel member can.
pub(crate) const DM_PERMS: Permissions = Permissions::VIEW_CHANNEL
    .union(Permissions::CONNECT)
    .union(Permissions::SPEAK)
    .union(Permissions::USE_VAD)
    .union(Permissions::STREAM);

/// Whether starting more video fits the channel's allowance (VOX-SIG-009). `others` is how
/// many non-exempt sends the rest of the channel has, `mine_after` what the member would
/// send. Patrons are exempt (D-052). The caller only asks when the member's own count goes up,
/// so lowering a limit never kicks anyone who already sends.
pub fn allowance_ok(others: u16, mine_after: u16, limit: u16, exempt: bool) -> bool {
    exempt || others.saturating_add(mine_after) <= limit
}

#[derive(Debug, Deserialize)]
pub(crate) struct VoiceStateIn {
    guild_id: Option<GuildId>,
    channel_id: Option<ChannelId>,
    #[serde(default)]
    self_mute: bool,
    #[serde(default)]
    self_deaf: bool,
    #[serde(default)]
    self_video: bool,
    // `self_stream` is not read: only STREAM_CREATE / STREAM_DELETE set it, after the
    // allowance checks.
}

pub(crate) fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub(crate) fn unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

impl Gateway {
    pub(crate) fn voice_cfg(&self) -> Option<&VoiceConfig> {
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

    pub(crate) async fn relay(&self, cmd: VoiceCommand) {
        if let Some(r) = self.voice_cfg().and_then(|v| v.relay.clone()) {
            r.apply(cmd).await;
        }
    }

    pub(crate) async fn relay_flags(&self, st: &VoiceState, f: MediaFlags) {
        self.relay(VoiceCommand::SetFlags {
            channel: st.channel_id,
            user: st.user_id,
            speak: f.speak,
            deaf: f.deaf,
            ptt_required: f.ptt_required,
            priority: f.priority,
            camera: f.camera,
            screen: f.screen,
        })
        .await;
    }

    pub(crate) fn voice_error(
        &self,
        s: &Session,
        guild: Option<GuildId>,
        channel: Option<ChannelId>,
        e: JoinDenied,
    ) {
        self.voice_error_with(s, guild, channel, e, Value::Null);
    }

    /// `SR_VOICE_ERROR`, with `extra` (an object) merged into the body: `limit` and
    /// `count` for the sender allowance, for example.
    pub(crate) fn voice_error_with(
        &self,
        s: &Session,
        guild: Option<GuildId>,
        channel: Option<ChannelId>,
        e: JoinDenied,
        extra: Value,
    ) {
        let mut body = json!({
            "guild_id": guild, "channel_id": channel,
            "code": e.code(), "message": e.message(),
        });
        if let (Some(b), Some(x)) = (body.as_object_mut(), extra.as_object()) {
            for (k, v) in x {
                b.insert(k.clone(), v.clone());
            }
        }
        s.deliver("SR_VOICE_ERROR", body);
    }

    /// `voice_states` for `GUILD_CREATE`: the states in `guild` whose channel `user` can see.
    pub(crate) async fn voice_states_for(&self, user: UserId, guild: GuildId) -> Vec<Value> {
        let states: Vec<VoiceState> = self
            .st()
            .voice
            .values()
            .filter(|s| s.guild_id == Some(guild))
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

    /// `VOICE_STATE_UPDATE` for `st`: to the guild's viewers of its channels, or to the
    /// members of a DM call.
    pub(crate) async fn dispatch_state(
        &self,
        st: &VoiceState,
        body: Value,
        channels: &[ChannelId],
        always: &[UserId],
    ) {
        match st.guild_id {
            Some(g) => self.dispatch_voice_state(g, body, channels, always).await,
            None => {
                let mut users = self.call_members(st.channel_id);
                users.extend(always.iter().copied());
                users.sort_unstable();
                users.dedup();
                for u in users {
                    self.dispatch_user(u, "VOICE_STATE_UPDATE", body.clone())
                        .await;
                }
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
                guild_id: st.guild_id,
                channel_id: st.channel_id,
                session_id: st.session_id.clone(),
                exp: now + TOKEN_TTL_SECS,
                speak: flags.speak,
                deaf: flags.deaf,
                ptt_required: flags.ptt_required,
                priority: flags.priority,
                camera: flags.camera,
                screen: flags.screen,
                tier: st.tier,
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
                // No guild: a DM or group DM call.
                if let Err(e) = self.voice_join(s, inp.guild_id, channel, &inp).await {
                    self.voice_error(s, inp.guild_id, Some(channel), e);
                }
            }
        }
    }

    /// Number of non-exempt video sends in `channel`, not counting `except`.
    pub(crate) fn sender_count(&self, channel: ChannelId, except: UserId) -> u16 {
        self.st()
            .voice
            .values()
            .filter(|v| v.channel_id == channel && v.user_id != except && !v.exempt)
            .map(VoiceState::video_sends)
            .fold(0u16, u16::saturating_add)
    }

    async fn voice_join(
        &self,
        s: &Arc<Session>,
        guild: Option<GuildId>,
        channel: ChannelId,
        inp: &VoiceStateIn,
    ) -> Result<(), JoinDenied> {
        if self.voice_cfg().is_none() {
            return Err(JoinDenied::Unavailable);
        }
        let dir = &self.inner.dir;
        let (info, perms) = match guild {
            Some(g) => {
                let info = dir
                    .voice_channel(g, channel)
                    .await
                    .ok_or(JoinDenied::UnknownChannel)?;
                let perms = dir
                    .channel_permissions(s.user, g, channel)
                    .await
                    .ok_or(JoinDenied::NotMember)?;
                (info, perms)
            }
            None => {
                let members = dir
                    .dm_call_members(channel, s.user)
                    .await
                    .ok_or(JoinDenied::NotMember)?;
                self.call_set_members(channel, members);
                let info = VoiceChannelInfo {
                    kind: VoiceChannelKind::Voice,
                    user_limit: 0,
                    bitrate: None,
                    age_restricted: false,
                };
                (info, DM_PERMS)
            }
        };
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
        let age_ok = !info.age_restricted || dir.age_confirmed(s.user).await;
        check_join(&info, perms, occupants, inside, age_ok)?;

        let limits = dir.voice_limits(s.user, guild).await;
        let afk = match guild {
            Some(g) => dir.voice_guild(g).await.afk_channel == Some(channel),
            None => false,
        };

        // Server mute and deafen follow the member within a guild.
        let carry = cur.as_ref().filter(|c| c.guild_id == guild);
        let same = inside && carry.is_some();
        let is_stage = info.kind == VoiceChannelKind::Stage;
        let suppress = if !is_stage {
            false
        } else if same {
            carry.is_some_and(|c| c.suppress)
        } else {
            // The audience listens until a moderator invites them; moderators speak at once.
            !is_stage_moderator(perms)
        };
        let self_stream = same && carry.is_some_and(|c| c.self_stream);

        // Starting a camera is checked against the channel's allowance; a refusal keeps the
        // join and drops only the video.
        let mut self_video = inp.self_video;
        let mut video_denied: Option<(JoinDenied, Value)> = None;
        if self_video && !(same && carry.is_some_and(|c| c.self_video)) {
            let others = self.sender_count(channel, s.user);
            let mine_after = 1 + u16::from(self_stream);
            if !perms.contains(Permissions::STREAM) {
                // "Video" in the permission list: camera and screen share alike.
                self_video = false;
                video_denied = Some((JoinDenied::MissingPermission, Value::Null));
            } else if !allowance_ok(others, mine_after, limits.video_senders, limits.exempt) {
                self_video = false;
                video_denied = Some((
                    JoinDenied::VideoLimitReached,
                    json!({ "limit": limits.video_senders, "count": others }),
                ));
            }
        }
        let new = VoiceState {
            guild_id: guild,
            channel_id: channel,
            user_id: s.user,
            session_id: s.id.clone(),
            mute: carry.is_some_and(|c| c.mute),
            deaf: carry.is_some_and(|c| c.deaf),
            self_mute: inp.self_mute,
            self_deaf: inp.self_deaf,
            self_video,
            self_stream,
            suppress,
            request_to_speak_ms: if is_stage && same {
                carry.and_then(|c| c.request_to_speak_ms)
            } else {
                None
            },
            afk,
            exempt: limits.exempt,
            tier: limits.tier,
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
        self.voice_touch(s.user);

        let member = match guild {
            Some(g) => dir.member_payload(g, s.user).await,
            None => None,
        };
        let mut channels = vec![channel];
        if let Some(old) = &cur {
            if old.guild_id == guild {
                channels.push(old.channel_id);
            } else {
                // Moving between guilds (or into a call): the old place sees a leave.
                self.announce_left(old).await;
            }
        }
        self.dispatch_state(&new, new.to_json(true, member), &channels, &[s.user])
            .await;
        if let Some(old) = &cur
            && old.channel_id != channel
        {
            self.voice_after_removal(old).await;
        }
        if guild.is_none() {
            self.call_member_joined(s.user, channel).await;
        }
        if fresh_join {
            self.voice_server_update(&new, flags)?;
        } else {
            self.relay_flags(&new, flags).await;
        }
        if let Some((why, extra)) = video_denied {
            self.voice_error_with(s, guild, Some(channel), why, extra);
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
        self.dispatch_state(old, body, &[old.channel_id], &[old.user_id])
            .await;
    }

    /// Remember that `user` did something in voice (speaking, a state change). Idle members
    /// are moved to the AFK channel.
    pub(crate) fn voice_touch(&self, user: UserId) {
        self.st().vx.active.insert(user, Instant::now());
    }

    /// The SFU heard `user` speak in `channel` (VOX-SIG-003: AFK timer).
    pub fn voice_activity(&self, user: UserId, channel: ChannelId) {
        if self
            .st()
            .voice
            .get(&user)
            .is_some_and(|s| s.channel_id == channel)
        {
            self.voice_touch(user);
        }
    }

    /// Remove `user` from voice and tell everyone. Returns the removed state.
    pub async fn voice_leave(&self, user: UserId) -> Option<VoiceState> {
        let old = {
            let mut st = self.st();
            st.vx.active.remove(&user);
            st.voice.remove(&user)?
        };
        self.relay(VoiceCommand::Disconnect {
            channel: old.channel_id,
            user,
        })
        .await;
        self.announce_left(&old).await;
        self.voice_after_removal(&old).await;
        Some(old)
    }

    /// Everything that hangs off a member being in a channel: their stream or the stream they
    /// watch, an otherwise empty stage, a call nobody is in any more.
    pub(crate) async fn voice_after_removal(&self, old: &VoiceState) {
        self.stream_member_left(old).await;
        self.stage_member_left(old).await;
        if old.guild_id.is_none() {
            self.call_member_left(old).await;
        }
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
            .filter(|s| s.guild_id == Some(guild))
            .ok_or(ModError::NotInVoice)?;
        let src = dir
            .channel_permissions(actor, guild, cur.channel_id)
            .await
            .ok_or(ModError::MissingPermissions)?;
        let mut dst = None;
        let mut dst_kind = None;
        if let Some(Some(to)) = patch.channel {
            let info = dir
                .voice_channel(guild, to)
                .await
                .ok_or(ModError::UnknownChannel)?;
            if info.kind == VoiceChannelKind::Other {
                return Err(ModError::NotVoiceChannel);
            }
            dst_kind = Some(info.kind);
            dst = dir.channel_permissions(actor, guild, to).await;
        }
        authorize(&patch, src, dst)?;
        if patch.suppress.is_some() {
            let kind = match dst_kind {
                Some(k) => k,
                None => dir
                    .voice_channel(guild, cur.channel_id)
                    .await
                    .map_or(VoiceChannelKind::Other, |i| i.kind),
            };
            if kind != VoiceChannelKind::Stage {
                return Err(ModError::NotStageChannel);
            }
        }

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
        let moved = new.channel_id != cur.channel_id;
        let target_perms = dir
            .channel_permissions(target, guild, new.channel_id)
            .await
            .unwrap_or_else(Permissions::empty);
        if moved {
            new.afk = dir.voice_guild(guild).await.afk_channel == Some(new.channel_id);
            new.request_to_speak_ms = None;
            new.suppress =
                dst_kind == Some(VoiceChannelKind::Stage) && !is_stage_moderator(target_perms);
            // A stream does not follow its owner into another room.
            new.self_stream = false;
        }
        if let Some(sup) = patch.suppress.filter(|v| *v != new.suppress) {
            new.suppress = sup;
            new.request_to_speak_ms = None;
            actions.push(ModAction::Suppressed(sup));
        } else if patch.suppress.is_some() {
            // Already in that state; still drop a pending hand.
            new.request_to_speak_ms = None;
        }
        if actions.is_empty() && new == cur {
            return Ok(ModOutcome {
                state: Some(cur),
                actions,
            });
        }

        self.commit_state_change(&cur, new.clone(), target_perms)
            .await;
        Ok(ModOutcome {
            state: Some(new),
            actions,
        })
    }

    /// Store `new` in place of `cur` when the channel stays the same: dispatch the change
    /// and refresh the SFU's flags. Never moves the member, so cleanup code can call it.
    pub(crate) async fn commit_state_inplace(
        &self,
        cur: &VoiceState,
        new: VoiceState,
        target_perms: Permissions,
    ) {
        self.st().voice.insert(cur.user_id, new.clone());
        let flags = media_flags(target_perms, &new);
        let member = match new.guild_id {
            Some(g) => self.inner.dir.member_payload(g, cur.user_id).await,
            None => None,
        };
        self.dispatch_state(
            &new,
            new.to_json(true, member),
            &[new.channel_id],
            &[cur.user_id],
        )
        .await;
        self.relay_flags(&new, flags).await;
    }

    /// Store `new` in place of `cur` (same user), tell the SFU and everyone who sees it.
    /// A move reconnects the member to the other room with a fresh token.
    pub(crate) async fn commit_state_change(
        &self,
        cur: &VoiceState,
        new: VoiceState,
        target_perms: Permissions,
    ) {
        let moved = new.channel_id != cur.channel_id;
        if moved {
            self.relay(VoiceCommand::Disconnect {
                channel: cur.channel_id,
                user: cur.user_id,
            })
            .await;
        }
        self.st().voice.insert(cur.user_id, new.clone());
        let flags = media_flags(target_perms, &new);
        let member = match new.guild_id {
            Some(g) => self.inner.dir.member_payload(g, cur.user_id).await,
            None => None,
        };
        let channels = if moved {
            vec![new.channel_id, cur.channel_id]
        } else {
            vec![new.channel_id]
        };
        self.dispatch_state(&new, new.to_json(true, member), &channels, &[cur.user_id])
            .await;
        if moved {
            self.voice_after_removal(cur).await;
            // The moved member reconnects to the new room with a fresh token.
            let _ = self.voice_server_update(&new, flags);
        } else {
            self.relay_flags(&new, flags).await;
        }
    }

    /// Remove states whose session is gone or stayed detached past the grace period, move
    /// idle members to the AFK channel and expire call rings.
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
        self.voice_afk_sweep(now).await;
        self.call_sweep(now).await;
    }

    /// Move members who have been silent for the guild's AFK timeout to its AFK channel
    /// (VOX-SIG-003). They stay connected and can listen.
    async fn voice_afk_sweep(&self, now: Instant) {
        let candidates: Vec<(VoiceState, Instant)> = {
            let st = self.st();
            st.voice
                .values()
                .filter(|v| v.guild_id.is_some() && !v.afk)
                .map(|v| {
                    (
                        v.clone(),
                        st.vx.active.get(&v.user_id).copied().unwrap_or(now),
                    )
                })
                .collect()
        };
        let mut by_guild: HashMap<GuildId, VoiceGuildInfo> = HashMap::new();
        for (v, since) in candidates {
            let Some(guild) = v.guild_id else { continue };
            let info = match by_guild.get(&guild) {
                Some(i) => *i,
                None => {
                    let i = self.inner.dir.voice_guild(guild).await;
                    by_guild.insert(guild, i);
                    i
                }
            };
            let Some(afk) = info.afk_channel else {
                continue;
            };
            if info.afk_timeout_secs == 0
                || afk == v.channel_id
                || now.saturating_duration_since(since)
                    < Duration::from_secs(u64::from(info.afk_timeout_secs))
            {
                continue;
            }
            self.afk_move(v, afk).await;
        }
    }

    async fn afk_move(&self, cur: VoiceState, afk: ChannelId) {
        let Some(guild) = cur.guild_id else { return };
        let dir = &self.inner.dir;
        // The AFK channel must still exist as a voice channel the member may join.
        let Some(info) = dir.voice_channel(guild, afk).await else {
            return;
        };
        let Some(perms) = dir.channel_permissions(cur.user_id, guild, afk).await else {
            return;
        };
        if info.kind != VoiceChannelKind::Voice
            || !perms.contains(Permissions::VIEW_CHANNEL | Permissions::CONNECT)
        {
            return;
        }
        // Re-read: the member may have left or moved while we looked things up.
        let Some(live) = self.voice_state(cur.user_id) else {
            return;
        };
        if live.channel_id != cur.channel_id || live.session_id != cur.session_id {
            return;
        }
        let mut new = live.clone();
        new.channel_id = afk;
        new.afk = true;
        new.suppress = false;
        new.request_to_speak_ms = None;
        new.self_stream = false;
        // The move counts as activity so the member is not bounced again at once.
        self.voice_touch(cur.user_id);
        self.commit_state_change(&live, new, perms).await;
    }

    /// The SFU lost `user`'s media session in `channel` (WebSocket gone past the resume
    /// window, or ICE failed). Clear the voice state if it still points there.
    pub async fn voice_media_lost(&self, user: UserId, channel: ChannelId) {
        if self
            .voice_state(user)
            .is_some_and(|s| s.channel_id == channel)
        {
            self.voice_leave(user).await;
        }
    }

    pub(crate) async fn voice_member_left(&self, guild: GuildId, user: UserId) {
        if self
            .voice_state(user)
            .is_some_and(|s| s.guild_id == Some(guild))
        {
            self.voice_leave(user).await;
        }
    }

    pub(crate) async fn voice_guild_deleted(&self, guild: GuildId) {
        let removed: Vec<VoiceState> = {
            let mut st = self.st();
            let users: Vec<UserId> = st
                .voice
                .values()
                .filter(|v| v.guild_id == Some(guild))
                .map(|v| v.user_id)
                .collect();
            for u in &users {
                st.vx.active.remove(u);
            }
            users.iter().filter_map(|u| st.voice.remove(u)).collect()
        };
        let channels: HashSet<ChannelId> = removed.iter().map(|v| v.channel_id).collect();
        {
            let mut st = self.st();
            st.vx.streams.retain(|_, s| s.guild_id != Some(guild));
            st.vx.stages.retain(|_, s| s.guild_id != guild);
        }
        for c in channels {
            self.relay(VoiceCommand::CloseRoom { channel: c }).await;
        }
    }

    async fn voice_channel_deleted(&self, guild: GuildId, channel: ChannelId) {
        let users: Vec<UserId> = self
            .voice_occupants(channel)
            .into_iter()
            .filter(|v| v.guild_id == Some(guild))
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
            .filter(|v| v.guild_id == Some(guild))
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
        // A deleted stage channel takes its stage with it, even when nobody is connected.
        if self.voice_cfg().is_some()
            && let DomainEvent::ChannelDeleted { channel, .. } = ev
        {
            self.stage_channel_deleted(*channel).await;
        }
        if self.voice_cfg().is_none()
            || !self.st().voice.values().any(|v| v.guild_id == Some(guild))
        {
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
            // The server's level changed: new limits apply to new senders and viewers only.
            DomainEvent::GuildEntitlementUpdated { .. } => {
                self.allowance_refresh(guild).await;
            }
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
            age_restricted: false,
        }
    }

    fn member() -> Permissions {
        P::VIEW_CHANNEL | P::CONNECT | P::SPEAK | P::USE_VAD
    }

    fn state() -> VoiceState {
        VoiceState {
            guild_id: Some(GuildId::new(1)),
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
            request_to_speak_ms: None,
            afk: false,
            exempt: false,
            tier: 0,
        }
    }

    #[test]
    fn connect_and_view_are_required() {
        assert_eq!(check_join(&info(0), member(), 0, false, true), Ok(()));
        assert_eq!(
            check_join(&info(0), member() - P::CONNECT, 0, false, true),
            Err(JoinDenied::MissingPermission)
        );
        assert_eq!(
            check_join(&info(0), member() - P::VIEW_CHANNEL, 0, false, true),
            Err(JoinDenied::MissingPermission)
        );
        assert_eq!(
            check_join(&info(0), P::empty(), 0, false, true),
            Err(JoinDenied::MissingPermission)
        );
    }

    #[test]
    fn user_limit_blocks_unless_bypassed_or_already_inside() {
        assert_eq!(check_join(&info(2), member(), 1, false, true), Ok(()));
        assert_eq!(
            check_join(&info(2), member(), 2, false, true),
            Err(JoinDenied::ChannelFull)
        );
        // Re-sending op 4 for the channel you are in is not a new join.
        assert_eq!(check_join(&info(2), member(), 2, true, true), Ok(()));
        // Move Members, Manage Channels and Administrator walk past the limit.
        for bypass in [
            P::MOVE_MEMBERS,
            P::MANAGE_CHANNELS,
            P::ADMINISTRATOR,
            P::ALL,
        ] {
            assert_eq!(
                check_join(&info(2), member() | bypass, 5, false, true),
                Ok(())
            );
        }
        // 0 means unlimited.
        assert_eq!(check_join(&info(0), member(), 500, false, true), Ok(()));
    }

    #[test]
    fn only_voice_and_stage_channels_can_be_joined() {
        let mut i = info(0);
        i.kind = VoiceChannelKind::Other;
        assert_eq!(
            check_join(&i, member(), 0, false, true),
            Err(JoinDenied::NotVoiceChannel)
        );
        i.kind = VoiceChannelKind::Stage;
        assert_eq!(check_join(&i, member(), 0, false, true), Ok(()));
    }

    #[test]
    fn age_restricted_channels_need_the_18_plus_answer() {
        let mut i = info(0);
        i.age_restricted = true;
        assert_eq!(
            check_join(&i, member(), 0, false, false),
            Err(JoinDenied::AgeConfirmationRequired)
        );
        assert_eq!(check_join(&i, member(), 0, false, true), Ok(()));
        // An ordinary channel never asks.
        assert_eq!(check_join(&info(0), member(), 0, false, false), Ok(()));
        // Permissions are checked first, so a stranger learns nothing about the gate.
        assert_eq!(
            check_join(&i, P::empty(), 0, false, false),
            Err(JoinDenied::MissingPermission)
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
                priority: false,
                camera: false,
                screen: false,
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
    fn the_stage_audience_and_the_afk_channel_only_listen() {
        let audience = VoiceState {
            suppress: true,
            ..state()
        };
        assert!(!media_flags(member(), &audience).speak);
        let afk = VoiceState {
            afk: true,
            ..state()
        };
        assert!(!media_flags(member(), &afk).speak);
        // Neither affects what they hear.
        assert!(!media_flags(member(), &afk).deaf);
    }

    #[test]
    fn inviting_to_speak_needs_mute_members() {
        let invite = ModPatch {
            suppress: Some(false),
            ..ModPatch::default()
        };
        assert_eq!(authorize(&invite, P::MUTE_MEMBERS, None), Ok(()));
        assert_eq!(
            authorize(&invite, P::MOVE_MEMBERS | P::DEAFEN_MEMBERS, None),
            Err(ModError::MissingPermissions)
        );
        assert!(is_stage_moderator(
            P::MANAGE_CHANNELS | P::MUTE_MEMBERS | P::MOVE_MEMBERS
        ));
        assert!(!is_stage_moderator(P::MUTE_MEMBERS | P::MOVE_MEMBERS));
    }

    #[test]
    fn a_raised_hand_is_an_iso_timestamp() {
        let v = VoiceState {
            request_to_speak_ms: Some(1_700_000_000_123),
            ..state()
        }
        .to_json(true, None);
        assert_eq!(
            v["request_to_speak_timestamp"],
            "2023-11-14T22:13:20.123000+00:00"
        );
    }

    #[test]
    fn limits_allow_a_quality_when_one_ceiling_covers_it() {
        let free = VoiceLimits::free();
        assert!(free.allows(720, 30));
        assert!(free.allows(480, 15));
        assert!(!free.allows(1080, 30));
        assert!(!free.allows(720, 60));
        let l1 = VoiceLimits {
            profiles: vec![(720, 60), (1080, 30)],
            ..VoiceLimits::free()
        };
        assert!(l1.allows(720, 60));
        assert!(l1.allows(1080, 30));
        // 1080p60 needs a ceiling that covers both numbers.
        assert!(!l1.allows(1080, 60));
    }

    #[test]
    fn video_sends_count_a_camera_and_a_stream_separately() {
        let none = state();
        assert_eq!(none.video_sends(), 0);
        let both = VoiceState {
            self_video: true,
            self_stream: true,
            ..state()
        };
        assert_eq!(both.video_sends(), 2);
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
        assert_eq!(
            JoinDenied::AgeConfirmationRequired.code(),
            "age_confirmation_required"
        );
        assert_eq!(JoinDenied::VideoLimitReached.code(), "video_limit_reached");
        assert_eq!(JoinDenied::StreamFull.code(), "stream_full");
    }
}
