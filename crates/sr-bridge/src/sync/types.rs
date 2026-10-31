//! Shared types of the sync (design sections 2, 3, 11).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `bridges.mode` (migration 1720). There is no "off": off is the absence of a bridge row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncMode {
    /// "Only bridge these": channel, category and thread maps; no role or permission sync.
    Selective,
    /// "Bridge the whole server": one server on both platforms.
    WholeGuild,
}

impl SyncMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Selective => "selective",
            Self::WholeGuild => "whole_guild",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "selective" => Some(Self::Selective),
            "whole_guild" => Some(Self::WholeGuild),
            _ => None,
        }
    }

    /// Roles, `@everyone` and overwrites are synced (SIDE ROOM to Discord) only in whole_guild mode.
    pub const fn syncs_permissions(self) -> bool {
        matches!(self, Self::WholeGuild)
    }
}

/// `bridges.sync_state` (migration 1750).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncState {
    /// Selective mode: no structure or permission sync is running.
    Off,
    /// The seed job is running.
    Seeding,
    Live,
    /// A manager paused it. Pushes and reverts stop; SIDE ROOM is unaffected.
    Paused,
    /// The bot lost ADMINISTRATOR or its top role position; sync stops before a single 403 is spent.
    PausedDiscordAccess,
    /// The per-bridge invalid-request breaker tripped (200 in 10 minutes).
    PausedInvalidRequests,
}

impl SyncState {
    pub const ALL: [SyncState; 6] = [
        Self::Off,
        Self::Seeding,
        Self::Live,
        Self::Paused,
        Self::PausedDiscordAccess,
        Self::PausedInvalidRequests,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Seeding => "seeding",
            Self::Live => "live",
            Self::Paused => "paused",
            Self::PausedDiscordAccess => "paused_discord_access",
            Self::PausedInvalidRequests => "paused_invalid_requests",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|x| x.as_str() == s)
    }

    /// Ops may run (structure pushes, reverts, applies).
    pub const fn runs_ops(self) -> bool {
        matches!(self, Self::Seeding | Self::Live)
    }
}

/// What a sync op is about (`bridge_sync_ops.object_kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    Role,
    /// The order of all roles, one bulk call.
    RoleOrder,
    /// A channel or category (and its non-permission fields).
    Channel,
    /// The order of the children of one parent, one bulk call.
    ChannelOrder,
    /// The permission overwrites of one channel, one `PATCH`.
    Overwrites,
    Thread,
    ForumTags,
    /// A full three-way pass over everything.
    Reconcile,
}

impl ObjectKind {
    pub const ALL: [ObjectKind; 8] = [
        Self::Role,
        Self::RoleOrder,
        Self::Channel,
        Self::ChannelOrder,
        Self::Overwrites,
        Self::Thread,
        Self::ForumTags,
        Self::Reconcile,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Role => "role",
            Self::RoleOrder => "role_order",
            Self::Channel => "channel",
            Self::ChannelOrder => "channel_order",
            Self::Overwrites => "overwrites",
            Self::Thread => "thread",
            Self::ForumTags => "forum_tags",
            Self::Reconcile => "reconcile",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|x| x.as_str() == s)
    }
}

/// Which side the change came from (`bridge_sync_ops.direction`). The payload is always computed when the op
/// runs, so the direction only says which side's event queued the look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpDirection {
    /// A SIDE ROOM change to push.
    ToDiscord,
    /// A Discord event to apply (or revert).
    ToSr,
    /// A scheduled reconcile: compare both ways.
    Look,
}

impl OpDirection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ToDiscord => "to_discord",
            Self::ToSr => "to_sr",
            Self::Look => "look",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "to_discord" => Some(Self::ToDiscord),
            "to_sr" => Some(Self::ToSr),
            "look" => Some(Self::Look),
            _ => None,
        }
    }
}

/// `bridge_sync_ops.state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpState {
    Pending,
    Running,
    Done,
    /// After [`SyncOp::MAX_ATTEMPTS`] failures; a dead-op conflict shows it and "Retry" requeues it.
    Dead,
}

impl OpState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Done => "done",
            Self::Dead => "dead",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [Self::Pending, Self::Running, Self::Done, Self::Dead]
            .into_iter()
            .find(|x| x.as_str() == s)
    }
}

/// Run order and debounce of an op (`bridge_sync_ops.priority`): reverts first, then structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum OpPriority {
    /// A revert of a Discord edit to a SIDE ROOM-authoritative field: due 1 s after the last enqueue.
    Revert = 0,
    /// Ordinary structure: due 2 s after the last enqueue (the debounce).
    Structure = 1,
    /// Background (reconcile, seed): no debounce.
    Background = 2,
}

impl OpPriority {
    pub const fn as_i16(self) -> i16 {
        self as i16
    }

    pub fn from_i16(v: i16) -> Option<Self> {
        match v {
            0 => Some(Self::Revert),
            1 => Some(Self::Structure),
            2 => Some(Self::Background),
            _ => None,
        }
    }

    /// Milliseconds the op waits after its last enqueue (design section 11.2).
    pub const fn debounce_ms(self) -> i64 {
        match self {
            Self::Revert => 1_000,
            Self::Structure => 2_000,
            Self::Background => 0,
        }
    }
}

/// One row of `bridge_sync_ops`: "look at object X". It carries no payload of the change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SyncOp {
    pub id: i64,
    pub guild_id: i64,
    pub kind: ObjectKind,
    /// The SIDE ROOM id of the object (a Discord id for an object with no SIDE ROOM twin yet).
    pub object_id: i64,
    pub direction: OpDirection,
    pub state: OpState,
    pub seed_run: Option<i64>,
    pub step: i16,
    pub priority: OpPriority,
    pub attempts: i32,
    pub due_at_ms: i64,
    pub last_error: Option<String>,
}

impl SyncOp {
    /// After this many failed attempts a row becomes dead (design section 11.5).
    pub const MAX_ATTEMPTS: i32 = 8;

    /// Backoff before the next attempt: 1 s, doubling, capped at 10 minutes.
    pub fn backoff_ms(attempts: i32) -> i64 {
        let doublings = attempts.clamp(0, 20) as u32;
        (1_000_i64 << doublings).min(600_000)
    }
}

/// One field's last agreed value, each side as that side expresses it (a Discord channel name is lowercased with
/// hyphens, a clamped bitrate stays clamped, tag ids differ per side).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldState {
    pub sr: Value,
    pub discord: Value,
}

/// `synced_state` of a map row: the last state both sides agreed on, by field name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SyncedState(pub BTreeMap<String, FieldState>);

impl SyncedState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, field: &str) -> Option<&FieldState> {
        self.0.get(field)
    }

    /// Record that both sides now hold these values.
    pub fn record(&mut self, field: &str, sr: Value, discord: Value) {
        self.0.insert(field.to_owned(), FieldState { sr, discord });
    }

    pub fn forget(&mut self, field: &str) {
        self.0.remove(field);
    }

    pub fn from_json(v: &Value) -> Self {
        serde_json::from_value(v.clone()).unwrap_or_default()
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn names_round_trip() {
        for s in SyncState::ALL {
            assert_eq!(SyncState::parse(s.as_str()), Some(s));
        }
        for k in ObjectKind::ALL {
            assert_eq!(ObjectKind::parse(k.as_str()), Some(k));
        }
        for m in [SyncMode::Selective, SyncMode::WholeGuild] {
            assert_eq!(SyncMode::parse(m.as_str()), Some(m));
        }
        for d in [OpDirection::ToDiscord, OpDirection::ToSr, OpDirection::Look] {
            assert_eq!(OpDirection::parse(d.as_str()), Some(d));
        }
        assert!(
            SyncMode::parse("off").is_none(),
            "off is no row, not a mode"
        );
    }

    #[test]
    fn only_whole_guild_syncs_permissions_and_only_live_states_run() {
        assert!(!SyncMode::Selective.syncs_permissions());
        assert!(SyncMode::WholeGuild.syncs_permissions());
        assert!(SyncState::Live.runs_ops() && SyncState::Seeding.runs_ops());
        assert!(!SyncState::Off.runs_ops());
        assert!(!SyncState::Paused.runs_ops());
        assert!(!SyncState::PausedDiscordAccess.runs_ops());
        assert!(!SyncState::PausedInvalidRequests.runs_ops());
    }

    #[test]
    fn debounce_and_backoff_follow_the_design() {
        assert_eq!(OpPriority::Revert.debounce_ms(), 1_000);
        assert_eq!(OpPriority::Structure.debounce_ms(), 2_000);
        assert!(OpPriority::Revert < OpPriority::Structure);
        assert_eq!(SyncOp::backoff_ms(0), 1_000);
        assert_eq!(SyncOp::backoff_ms(3), 8_000);
        assert_eq!(SyncOp::backoff_ms(40), 600_000);
        assert_eq!(OpPriority::from_i16(7), None);
    }

    #[test]
    fn synced_state_records_each_side_as_that_side_expresses_it() {
        let mut s = SyncedState::new();
        s.record("name", json!("General Chat"), json!("general-chat"));
        let back = SyncedState::from_json(&s.to_json());
        assert_eq!(back, s);
        assert_eq!(back.get("name").unwrap().discord, json!("general-chat"));
        let mut back = back;
        back.forget("name");
        assert!(back.get("name").is_none());
        assert_eq!(SyncedState::from_json(&json!("junk")), SyncedState::new());
    }
}
