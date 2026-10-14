//! Voice WebSocket messages. Text frames, one JSON object each: `{"op": n, "d": {...}}`.
//! Ids are decimal strings (snowflakes), as everywhere in the API.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sr_core::{ChannelId, GuildId, UserId};

/// Opcodes.
pub mod op {
    /// C->S, first message: `{token}`.
    pub const IDENTIFY: u8 = 0;
    /// S->C: join accepted ([`super::Ready`]).
    pub const READY: u8 = 1;
    /// C->S: `{t}`.
    pub const HEARTBEAT: u8 = 2;
    /// S->C: echoes `{t}`.
    pub const HEARTBEAT_ACK: u8 = 3;
    /// Both directions: `{sdp}`. The client offers first; the server offers when
    /// it adds or removes a remote participant's audio line.
    pub const SDP_OFFER: u8 = 4;
    /// Both directions: `{sdp}`, the reply to an offer.
    pub const SDP_ANSWER: u8 = 5;
    /// C->S: trickle candidate `{candidate, sdp_mid, sdp_mline_index}`; `candidate: null`
    /// ends the list. S->C: reserved (the SFU is ICE-lite and lists every candidate in its answer).
    pub const ICE_CANDIDATE: u8 = 6;
    /// C->S: `{speaking}` bitmask. S->C: `{user_id, speaking, audio_level}`.
    pub const SPEAKING: u8 = 7;
    /// C->S: `{session_id, resume_token}`, to re-attach a dropped WebSocket.
    pub const RESUME: u8 = 8;
    /// S->C: resume accepted ([`super::Ready`] with the current participants).
    pub const RESUMED: u8 = 9;
    /// S->C: `{user_id}`, a participant joined the room.
    pub const CLIENT_CONNECT: u8 = 10;
    /// S->C: `{user_id}`, a participant left.
    pub const CLIENT_DISCONNECT: u8 = 11;
    /// Reserved: video publish/subscribe (simulcast layers, screen share).
    pub const VIDEO: u8 = 12;
    /// Reserved: egress governor and quality notices.
    pub const QUALITY: u8 = 13;
}

/// WebSocket close codes the SFU uses.
pub mod close {
    pub const UNKNOWN_OPCODE: u16 = 4001;
    pub const DECODE_ERROR: u16 = 4002;
    /// An op other than IDENTIFY/RESUME before authenticating, or no IDENTIFY within 10 s.
    pub const NOT_AUTHENTICATED: u16 = 4003;
    /// Bad, forged or expired token.
    pub const AUTHENTICATION_FAILED: u16 = 4004;
    pub const ALREADY_AUTHENTICATED: u16 = 4005;
    /// RESUME for an unknown or expired session: rejoin through the gateway.
    pub const SESSION_INVALID: u16 = 4006;
    /// More than 200 client messages in 10 s.
    pub const RATE_LIMITED: u16 = 4008;
    pub const HEARTBEAT_TIMEOUT: u16 = 4009;
    /// Removed from the call (disconnected by a moderator, moved, left, channel deleted). Do not reconnect.
    pub const DISCONNECTED: u16 = 4014;
    /// The SFU is draining or restarting; resume (or rejoin) shortly.
    pub const SERVER_RESTARTING: u16 = 4015;
    /// The same user joined again (another tab or device); this connection lost.
    pub const SESSION_REPLACED: u16 = 4016;
    /// The SFU cannot take more participants.
    pub const SERVER_FULL: u16 = 4017;
}

/// `speaking` bitmask (Discord values).
pub mod speaking {
    pub const MICROPHONE: u8 = 1;
    pub const SOUNDSHARE: u8 = 2;
    pub const PRIORITY: u8 = 4;
}

/// The envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub op: u8,
    #[serde(default)]
    pub d: Value,
}

impl Message {
    pub fn new(op: u8, d: impl Serialize) -> Self {
        Self {
            op,
            d: serde_json::to_value(d).unwrap_or(Value::Null),
        }
    }

    pub fn to_text(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Decode the payload as `T`.
    pub fn data<T: for<'de> Deserialize<'de>>(&self) -> Result<T, serde_json::Error> {
        T::deserialize(&self.d)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Identify {
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resume {
    pub session_id: String,
    pub resume_token: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Heartbeat {
    #[serde(default)]
    pub t: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sdp {
    pub sdp: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IceCandidate {
    /// `None` marks the end of candidates.
    pub candidate: Option<String>,
    #[serde(default)]
    pub sdp_mid: Option<String>,
    #[serde(default)]
    pub sdp_mline_index: Option<u32>,
}

/// C->S (`user_id` absent) and S->C.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Speaking {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<UserId>,
    /// Bitmask, see [`speaking`]. 0 = silent.
    pub speaking: u8,
    /// S->C only: last audio level in dBov (-127 quiet .. 0 loud) when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_level: Option<i8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserRef {
    pub user_id: UserId,
}

/// One participant in READY.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParticipantInfo {
    pub user_id: UserId,
    pub speaking: u8,
    /// Server mute (or no SPEAK permission): the SFU drops this participant's audio.
    pub muted: bool,
    /// Server deafen: the SFU sends this participant nothing.
    pub deafened: bool,
}

/// S->C, answers IDENTIFY (READY) and RESUME (RESUMED).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ready {
    pub session_id: String,
    /// Secret for RESUME on this session. Never log it.
    pub resume_token: String,
    /// Milliseconds between client heartbeats.
    pub heartbeat_interval: u64,
    pub user_id: UserId,
    pub channel_id: ChannelId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<GuildId>,
    pub participants: Vec<ParticipantInfo>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn envelope_round_trip() {
        let m = Message::new(
            op::SPEAKING,
            Speaking {
                user_id: Some(UserId::new(7)),
                speaking: 1,
                audio_level: Some(-30),
            },
        );
        let text = m.to_text();
        assert_eq!(
            serde_json::from_str::<Value>(&text).unwrap(),
            json!({"op": 7, "d": {"user_id": "7", "speaking": 1, "audio_level": -30}})
        );
        let back: Message = serde_json::from_str(&text).unwrap();
        assert_eq!(
            back.data::<Speaking>().unwrap().user_id,
            Some(UserId::new(7))
        );
    }

    #[test]
    fn client_speaking_has_no_user_id() {
        let m: Message = serde_json::from_str(r#"{"op":7,"d":{"speaking":1}}"#).unwrap();
        let s = m.data::<Speaking>().unwrap();
        assert_eq!((s.user_id, s.speaking, s.audio_level), (None, 1, None));
    }

    #[test]
    fn missing_payload_decodes_as_null() {
        let m: Message = serde_json::from_str(r#"{"op":2}"#).unwrap();
        assert_eq!(m.d, Value::Null);
        assert!(m.data::<Heartbeat>().is_err());
    }
}
