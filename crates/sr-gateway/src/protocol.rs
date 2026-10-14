//! Wire types: opcodes, close codes, payload envelope, intents.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Discord-numbered opcodes (docs/05 section 6.1).
pub mod op {
    pub const DISPATCH: u8 = 0;
    pub const HEARTBEAT: u8 = 1;
    pub const IDENTIFY: u8 = 2;
    pub const PRESENCE_UPDATE: u8 = 3;
    pub const VOICE_STATE_UPDATE: u8 = 4;
    pub const RESUME: u8 = 6;
    pub const RECONNECT: u8 = 7;
    pub const REQUEST_GUILD_MEMBERS: u8 = 8;
    pub const INVALID_SESSION: u8 = 9;
    pub const HELLO: u8 = 10;
    pub const HEARTBEAT_ACK: u8 = 11;
    pub const STREAM_CREATE: u8 = 18;
    pub const STREAM_DELETE: u8 = 19;
    pub const STREAM_WATCH: u8 = 20;
    pub const STREAM_SET_PAUSED: u8 = 22;
    pub const REQUEST_SOUNDBOARD_SOUNDS: u8 = 31;
    pub const WATCH_PARTY_SYNC: u8 = 100;
    pub const E2EE_MLS: u8 = 101;
    pub const LAZY_REQUEST: u8 = 102;

    /// Opcodes a client may legally send. Those not handled by the gateway
    /// itself (voice, member requests, SR ops) are accepted and ignored here.
    pub fn is_client_op(op: u8) -> bool {
        matches!(
            op,
            HEARTBEAT
                | STREAM_CREATE
                | STREAM_DELETE
                | STREAM_WATCH
                | STREAM_SET_PAUSED
                | IDENTIFY
                | PRESENCE_UPDATE
                | VOICE_STATE_UPDATE
                | RESUME
                | REQUEST_GUILD_MEMBERS
                | REQUEST_SOUNDBOARD_SOUNDS
                | WATCH_PARTY_SYNC
                | E2EE_MLS
                | LAZY_REQUEST
        )
    }
}

/// Discord-numbered close codes.
pub mod close {
    pub const UNKNOWN_ERROR: u16 = 4000;
    pub const UNKNOWN_OPCODE: u16 = 4001;
    pub const DECODE_ERROR: u16 = 4002;
    pub const NOT_AUTHENTICATED: u16 = 4003;
    pub const AUTHENTICATION_FAILED: u16 = 4004;
    pub const ALREADY_AUTHENTICATED: u16 = 4005;
    /// IDENTIFY named a shard other than `[0, 1]` (this server does not shard).
    pub const INVALID_SHARD: u16 = 4010;
    pub const SESSION_TIMED_OUT: u16 = 4009;
    /// More than 120 client ops within 60 s on one connection.
    pub const RATE_LIMITED: u16 = 4008;
    pub const INVALID_INTENTS: u16 = 4013;
    pub const DISALLOWED_INTENTS: u16 = 4014;
}

/// Intent bits (Discord positions) the gateway filters on.
pub mod intents {
    pub const GUILDS: u64 = 1 << 0;
    pub const GUILD_MEMBERS: u64 = 1 << 1;
    pub const GUILD_MODERATION: u64 = 1 << 2;
    pub const GUILD_EXPRESSIONS: u64 = 1 << 3;
    pub const GUILD_WEBHOOKS: u64 = 1 << 5;
    pub const GUILD_INVITES: u64 = 1 << 6;
    pub const GUILD_VOICE_STATES: u64 = 1 << 7;
    pub const GUILD_PRESENCES: u64 = 1 << 8;
    pub const GUILD_MESSAGES: u64 = 1 << 9;
    pub const GUILD_MESSAGE_REACTIONS: u64 = 1 << 10;
    pub const GUILD_MESSAGE_TYPING: u64 = 1 << 11;
    pub const GUILD_MESSAGE_POLLS: u64 = 1 << 24;
    pub const MESSAGE_CONTENT: u64 = 1 << 15;
    pub const AUTO_MODERATION_CONFIGURATION: u64 = 1 << 20;
    pub const AUTO_MODERATION_EXECUTION: u64 = 1 << 21;
    /// Privileged: need approval for released apps (docs/05 section 6.2).
    pub const PRIVILEGED: u64 = GUILD_MEMBERS | GUILD_PRESENCES | MESSAGE_CONTENT;
    /// Every bit defined by Discord (0..=26 minus gaps is not worth modelling).
    pub const ALL_KNOWN: u64 = (1 << 27) - 1;

    /// Intent required to receive a dispatch, if any.
    pub fn required_for(event: &str) -> Option<u64> {
        match event {
            "CHANNEL_PINS_UPDATE" => Some(GUILDS),
            "MESSAGE_UPDATE" | "MESSAGE_DELETE" | "MESSAGE_DELETE_BULK" => Some(GUILD_MESSAGES),
            "MESSAGE_REACTION_ADD"
            | "MESSAGE_REACTION_REMOVE"
            | "MESSAGE_REACTION_REMOVE_ALL"
            | "MESSAGE_REACTION_REMOVE_EMOJI" => Some(GUILD_MESSAGE_REACTIONS),
            "GUILD_DELETE" | "GUILD_UPDATE" | "CHANNEL_CREATE" | "CHANNEL_UPDATE"
            | "CHANNEL_DELETE" | "GUILD_ROLE_CREATE" | "GUILD_ROLE_UPDATE"
            | "GUILD_ROLE_DELETE" => Some(GUILDS),
            "GUILD_BAN_ADD" | "GUILD_BAN_REMOVE" | "GUILD_AUDIT_LOG_ENTRY_CREATE" => {
                Some(GUILD_MODERATION)
            }
            "GUILD_EMOJIS_UPDATE" | "GUILD_STICKERS_UPDATE" => Some(GUILD_EXPRESSIONS),
            "WEBHOOKS_UPDATE" => Some(GUILD_WEBHOOKS),
            "INVITE_CREATE" | "INVITE_DELETE" => Some(GUILD_INVITES),
            "AUTO_MODERATION_RULE_CREATE"
            | "AUTO_MODERATION_RULE_UPDATE"
            | "AUTO_MODERATION_RULE_DELETE" => Some(AUTO_MODERATION_CONFIGURATION),
            "AUTO_MODERATION_ACTION_EXECUTION" => Some(AUTO_MODERATION_EXECUTION),
            "GUILD_MEMBER_ADD" | "GUILD_MEMBER_REMOVE" | "GUILD_MEMBER_UPDATE" => {
                Some(GUILD_MEMBERS)
            }
            "THREAD_CREATE"
            | "THREAD_UPDATE"
            | "THREAD_DELETE"
            | "THREAD_LIST_SYNC"
            | "THREAD_MEMBER_UPDATE" => Some(GUILDS),
            "THREAD_MEMBERS_UPDATE" => Some(GUILD_MEMBERS),
            "PRESENCE_UPDATE" => Some(GUILD_PRESENCES),
            "MESSAGE_POLL_VOTE_ADD" | "MESSAGE_POLL_VOTE_REMOVE" => Some(GUILD_MESSAGE_POLLS),
            "MESSAGE_CREATE" => Some(GUILD_MESSAGES),
            "TYPING_START" => Some(GUILD_MESSAGE_TYPING),
            "STAGE_INSTANCE_CREATE" | "STAGE_INSTANCE_UPDATE" | "STAGE_INSTANCE_DELETE" => {
                Some(GUILDS)
            }
            "VOICE_STATE_UPDATE" => Some(GUILD_VOICE_STATES),
            _ => None,
        }
    }
}

/// The envelope for every gateway message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Payload {
    pub op: u8,
    #[serde(default)]
    pub d: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub t: Option<String>,
}

impl Payload {
    pub fn new(op: u8, d: Value) -> Self {
        Self {
            op,
            d,
            s: None,
            t: None,
        }
    }

    pub fn to_text(&self) -> String {
        // A Payload always serializes (string keys only).
        serde_json::to_string(self).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Identify {
    pub token: String,
    #[serde(default)]
    pub intents: u64,
    #[serde(default)]
    pub properties: Value,
    #[serde(default)]
    pub presence: Option<PresenceUpdate>,
    /// `[shard_id, shard_count]`; only `[0, 1]` is accepted (no sharding).
    #[serde(default)]
    pub shard: Option<Vec<u32>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Resume {
    pub token: String,
    pub session_id: String,
    pub seq: u64,
}

/// Client-set presence (op 3) and also the stored form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresenceUpdate {
    #[serde(default = "online")]
    pub status: String,
    #[serde(default)]
    pub activities: Vec<Value>,
    #[serde(default)]
    pub since: Option<u64>,
    #[serde(default)]
    pub afk: bool,
}

fn online() -> String {
    "online".into()
}

impl Default for PresenceUpdate {
    fn default() -> Self {
        Self {
            status: online(),
            activities: Vec::new(),
            since: None,
            afk: false,
        }
    }
}

impl PresenceUpdate {
    pub fn offline() -> Self {
        Self {
            status: "offline".into(),
            ..Self::default()
        }
    }

    /// What other users see: `invisible` is shown as `offline`.
    pub fn visible_status(&self) -> &str {
        if self.status == "invisible" {
            "offline"
        } else {
            &self.status
        }
    }
}
