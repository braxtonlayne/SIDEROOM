//! Wire types of the Watch Party API (docs/specs/watch-parties.md, "HTTP and gateway API as built").
//! Snowflakes travel as decimal strings; times are unix milliseconds on the server clock.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::ids::{id, opt_id};
use crate::state::Playback;

// ---- views -----------------------------------------------------------------------------------------

/// Playback state as sent to clients (`WATCH_PARTY_PLAYBACK`, and inside the party object).
///
/// The media should be at `position_ms + (server_clock - at_ms) * rate` while `playing`; `expected_position_ms`
/// is that value at `server_time_ms` (the moment the server built this object). Clients correct their clock
/// offset (`server_time_ms - (client_now - rtt/2)`) and apply drift correction against it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = WatchPartyPlayback)]
pub struct PlaybackView {
    /// Strictly increasing; a client ignores any state older than the newest it applied.
    pub version: i64,
    pub playing: bool,
    pub position_ms: i64,
    /// Playback speed (1.0 = normal).
    pub rate: f64,
    /// Server clock at which `position_ms` was true.
    pub at_ms: i64,
    /// Server clock when this view was built (every state carries it, for the clock offset).
    pub server_time_ms: i64,
    /// `position_ms` advanced to `server_time_ms`.
    pub expected_position_ms: i64,
    /// Paused by the buffering rule ("Waiting for ..."), not by a controller.
    pub auto_paused: bool,
    pub duration_ms: Option<i64>,
    pub live: bool,
}

impl PlaybackView {
    /// What a caller outside the party is told: nothing about the party (RDY-SEC-011: a heartbeat from a
    /// stranger used to answer with the real playback).
    pub fn hidden(now_ms: i64) -> Self {
        Self {
            version: 0,
            playing: false,
            position_ms: 0,
            rate: 1.0,
            at_ms: now_ms,
            server_time_ms: now_ms,
            expected_position_ms: 0,
            auto_paused: false,
            duration_ms: None,
            live: false,
        }
    }

    pub fn from_state(s: &Playback, now_ms: i64) -> Self {
        Self {
            version: s.version,
            playing: s.playing,
            position_ms: s.position_ms,
            rate: f64::from(s.rate_milli) / 1000.0,
            at_ms: s.at_ms,
            server_time_ms: now_ms,
            expected_position_ms: s.position_at(now_ms),
            auto_paused: s.auto_paused,
            duration_ms: s.duration_ms,
            live: s.live,
        }
    }
}

/// What the party is playing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = WatchPartyMedia)]
pub struct MediaView {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub title_id: i64,
    /// `"0"` for a movie.
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub episode_id: i64,
    #[serde(with = "opt_id", default)]
    #[schema(value_type = Option<String>)]
    pub variant_id: Option<i64>,
    /// The video asset; the viewer fetches HLS through `GET /cinema/assets/{asset_id}/playback`
    /// (the normal play checks apply to each viewer).
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub asset_id: i64,
    pub title: String,
    pub episode_label: Option<String>,
    pub duration_ms: Option<i64>,
    pub age_restricted: bool,
    pub live: bool,
}

/// A person in the party.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = WatchPartyParticipant)]
pub struct ParticipantView {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub user_id: i64,
    pub username: String,
    pub display_name: Option<String>,
    pub joined_at_ms: i64,
    pub is_host: bool,
    /// Granted by the host: may control playback when shared control is `controllers`.
    pub is_controller: bool,
    pub ready: bool,
    /// Currently stalled (reported by the member's player).
    pub buffering: bool,
    pub rtt_ms: Option<i32>,
}

/// An entry of the queue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = WatchPartyQueueItem)]
pub struct QueueItemView {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub position: i32,
    #[serde(flatten)]
    pub media: MediaView,
    #[serde(with = "opt_id", default)]
    #[schema(value_type = Option<String>)]
    pub added_by: Option<i64>,
    pub added_at_ms: i64,
}

/// Who may do something: the host alone, the host and members with `CONTROL_WATCH_PARTY` or the
/// host's explicit controllers, or everybody.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = WatchPartyControlMode)]
pub enum ControlMode {
    Host,
    Controllers,
    Everyone,
}

impl ControlMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Controllers => "controllers",
            Self::Everyone => "everyone",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "host" => Self::Host,
            "controllers" => Self::Controllers,
            "everyone" => Self::Everyone,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = WatchPartySettings)]
pub struct SettingsView {
    pub shared_control: ControlMode,
    pub queue_policy: ControlMode,
    /// Pause everybody when a participant buffers for more than 1.5 s.
    pub auto_pause_buffering: bool,
    /// Longest wait for a stalled participant before continuing without them.
    pub max_wait_ms: i32,
    /// A bound party that outlives its members (the Watch Party channel behaviour).
    pub persistent: bool,
    pub max_participants: i32,
}

/// Where the party lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = WatchPartyBinding)]
pub enum Binding {
    /// A guild text or voice channel.
    Channel,
    /// A DM or group DM.
    Dm,
    /// Not bound: joined by link.
    Private,
}

impl Binding {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Channel => "channel",
            Self::Dm => "dm",
            Self::Private => "private",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "channel" => Self::Channel,
            "dm" => Self::Dm,
            "private" => Self::Private,
            _ => return None,
        })
    }
}

/// A watch party, as returned by the API and carried by `WATCH_PARTY_UPDATE`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = WatchParty)]
pub struct PartyView {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub host_id: i64,
    pub binding: Binding,
    #[serde(with = "opt_id", default)]
    #[schema(value_type = Option<String>)]
    pub guild_id: Option<i64>,
    #[serde(with = "opt_id", default)]
    #[schema(value_type = Option<String>)]
    pub channel_id: Option<i64>,
    /// The secret of the share link. Only members see it (and only on private parties).
    pub invite_code: Option<String>,
    /// What is playing; `null` for an empty watch party room (state idle, D-227).
    pub media: Option<MediaView>,
    pub playback: PlaybackView,
    pub settings: SettingsView,
    pub participants: Vec<ParticipantView>,
    pub queue: Vec<QueueItemView>,
    pub created_at_ms: i64,
    pub ended: bool,
    /// What the viewing account may do (absent in gateway events, which go to everybody).
    pub you: Option<YouView>,
}

/// The viewing account's standing in the party.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = WatchPartyYou)]
pub struct YouView {
    pub member: bool,
    pub host: bool,
    /// May play, pause, seek and change the speed now.
    pub can_control: bool,
    /// May add to the queue.
    pub can_queue: bool,
    /// May end the party, hand over the host role, kick.
    pub can_manage: bool,
}

// ---- requests ----------------------------------------------------------------------------------------

/// Body of `POST /watch-parties`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[schema(as = WatchPartyCreate)]
pub struct CreateInput {
    /// The Cinema title. Omit it (or send "0") for a Live TV party and give `tv_channel_id` instead.
    #[serde(with = "id", default)]
    #[schema(value_type = String)]
    pub title_id: i64,
    /// An empty watch party room (D-227): no media yet, the first member to join hosts and picks one. Only
    /// a persistent party bound to a server channel can be empty; `title_id` and `tv_channel_id` stay unset.
    #[serde(default)]
    pub empty: bool,
    /// A Live TV channel to watch together (no seeking; everyone follows the live edge).
    #[serde(with = "opt_id", default)]
    #[schema(value_type = Option<String>)]
    pub tv_channel_id: Option<i64>,
    #[serde(with = "opt_id", default)]
    #[schema(value_type = Option<String>)]
    pub episode_id: Option<i64>,
    #[serde(with = "opt_id", default)]
    #[schema(value_type = Option<String>)]
    pub variant_id: Option<i64>,
    /// A guild channel (text or voice) or DM to bind to. Without it the party is private (link only).
    #[serde(with = "opt_id", default)]
    #[schema(value_type = Option<String>)]
    pub channel_id: Option<i64>,
    /// Where to start, in milliseconds (default 0).
    pub start_ms: Option<i64>,
    pub settings: Option<SettingsInput>,
}

/// Optional settings (on create and `PATCH`).
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[schema(as = WatchPartySettingsInput)]
pub struct SettingsInput {
    pub shared_control: Option<ControlMode>,
    pub queue_policy: Option<ControlMode>,
    pub auto_pause_buffering: Option<bool>,
    pub max_wait_ms: Option<i32>,
    pub persistent: Option<bool>,
    pub max_participants: Option<i32>,
}

/// Body of `POST /watch-parties/{id}/join`.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[schema(as = WatchPartyJoin)]
pub struct JoinInput {
    /// Required to join a private party.
    pub invite_code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = WatchPartyAction)]
pub enum Action {
    Play,
    Pause,
    Seek,
    Rate,
    /// The player reached the end (any participant may report; the server verifies).
    Ended,
}

/// Body of `POST /watch-parties/{id}/playback`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[schema(as = WatchPartyCommand)]
pub struct CommandInput {
    pub action: Action,
    /// For `seek`.
    pub position_ms: Option<i64>,
    /// For `rate` (0.5 to 2.0).
    pub rate: Option<f64>,
    /// The `version` the client last applied. A different current version answers 409 with the
    /// current state; omit it for last-writer-wins.
    pub expected_version: Option<i64>,
}

/// Body of `PUT /watch-parties/{id}/media`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[schema(as = WatchPartyChangeMedia)]
pub struct ChangeMediaInput {
    /// The Cinema title, or "0" with `tv_channel_id`.
    #[serde(with = "id", default)]
    #[schema(value_type = String)]
    pub title_id: i64,
    /// Switch to a Live TV channel.
    #[serde(with = "opt_id", default)]
    #[schema(value_type = Option<String>)]
    pub tv_channel_id: Option<i64>,
    #[serde(with = "opt_id", default)]
    #[schema(value_type = Option<String>)]
    pub episode_id: Option<i64>,
    #[serde(with = "opt_id", default)]
    #[schema(value_type = Option<String>)]
    pub variant_id: Option<i64>,
    pub start_ms: Option<i64>,
}

/// Body of `POST /watch-parties/{id}/queue`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[schema(as = WatchPartyQueueAdd)]
pub struct QueueAddInput {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub title_id: i64,
    #[serde(with = "opt_id", default)]
    #[schema(value_type = Option<String>)]
    pub episode_id: Option<i64>,
    #[serde(with = "opt_id", default)]
    #[schema(value_type = Option<String>)]
    pub variant_id: Option<i64>,
}

/// Body of `PATCH /watch-parties/{id}/queue/{item_id}`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[schema(as = WatchPartyQueueMove)]
pub struct QueueMoveInput {
    /// New zero-based index.
    pub position: i32,
}

/// Body of `POST /watch-parties/{id}/host`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[schema(as = WatchPartyTransfer)]
pub struct TransferInput {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub user_id: i64,
}

/// Body of `PUT /watch-parties/{id}/controllers/{user_id}`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[schema(as = WatchPartyControllerInput)]
pub struct ControllerInput {
    pub controller: bool,
}

/// Body of `POST /watch-parties/{id}/heartbeat`: the member's liveness, clock estimate and player state.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[schema(as = WatchPartyHeartbeat)]
pub struct HeartbeatInput {
    /// The member's round-trip estimate to the server.
    pub rtt_ms: Option<i32>,
    /// `true` when the player has been stalled (report once it passes ~1.5 s), `false` when it recovered.
    pub buffering: Option<bool>,
    pub ready: Option<bool>,
}

/// Answer of the heartbeat: the clock sample and the current playback (a fallback for a lost event).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = WatchPartyHeartbeatResult)]
pub struct HeartbeatResult {
    pub server_time_ms: i64,
    pub playback: PlaybackView,
    /// `false`: the caller is no longer in the party (removed or ended).
    pub member: bool,
}

/// Answer of `GET /watch-parties/time`.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = WatchPartyTime)]
pub struct TimeView {
    pub server_time_ms: i64,
}

/// Body of `POST /watch-parties/{id}/reactions`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[schema(as = WatchPartyReaction)]
pub struct ReactionInput {
    /// A unicode emoji (or `name:id` of a custom one), up to 32 bytes.
    pub emoji: String,
}

/// Body of `POST /watch-parties/{id}/messages` (private parties' own chat; ephemeral).
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[schema(as = WatchPartyMessageInput)]
pub struct MessageInput {
    pub content: String,
}

/// An audit entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = WatchPartyAuditEntry)]
pub struct AuditEntry {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "opt_id", default)]
    #[schema(value_type = Option<String>)]
    pub actor_id: Option<i64>,
    pub action: String,
    #[serde(with = "opt_id", default)]
    #[schema(value_type = Option<String>)]
    pub target_id: Option<i64>,
    #[schema(value_type = Object)]
    pub detail: serde_json::Value,
    pub at_ms: i64,
}

/// The acting account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewer {
    pub user_id: i64,
    /// Confirmed 18 or older (D-184).
    pub adult: bool,
}
