//! Typed views of the objects a bot handles. Fields the SDK does not model stay available in
//! `extra`, so nothing the server sends is lost.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Snowflake id (a decimal string on the wire).
pub type Id = String;

/// Intent bits (Discord positions).
pub mod intents {
    pub const GUILDS: u64 = 1 << 0;
    pub const GUILD_MEMBERS: u64 = 1 << 1;
    pub const GUILD_MODERATION: u64 = 1 << 2;
    pub const GUILD_EXPRESSIONS: u64 = 1 << 3;
    pub const GUILD_VOICE_STATES: u64 = 1 << 7;
    pub const GUILD_PRESENCES: u64 = 1 << 8;
    pub const GUILD_MESSAGES: u64 = 1 << 9;
    pub const GUILD_MESSAGE_REACTIONS: u64 = 1 << 10;
    pub const GUILD_MESSAGE_TYPING: u64 = 1 << 11;
    pub const MESSAGE_CONTENT: u64 = 1 << 15;
    /// Guilds and guild messages, no privileged intent: a safe default.
    pub const DEFAULT: u64 = GUILDS | GUILD_MESSAGES;
    /// Intents that need approval for a released application.
    pub const PRIVILEGED: u64 = GUILD_MEMBERS | GUILD_PRESENCES | MESSAGE_CONTENT;
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct User {
    pub id: Id,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub global_name: Option<String>,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub bot: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Message {
    pub id: Id,
    pub channel_id: Id,
    #[serde(default)]
    pub guild_id: Option<Id>,
    #[serde(default)]
    pub author: User,
    /// Empty when the bot lacks `MESSAGE_CONTENT` (except DMs, mentions and its own messages).
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub mentions: Vec<User>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Message {
    /// Whether `user_id` is mentioned.
    pub fn mentions_user(&self, user_id: &str) -> bool {
        self.mentions.iter().any(|u| u.id == user_id)
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Role {
    pub id: Id,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub permissions: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Channel {
    pub id: Id,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, rename = "type")]
    pub kind: u8,
    #[serde(default)]
    pub guild_id: Option<Id>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Guild {
    pub id: Id,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub unavailable: bool,
    #[serde(default)]
    pub channels: Vec<Channel>,
    #[serde(default)]
    pub roles: Vec<Role>,
}

/// The `READY` dispatch.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Ready {
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub resume_gateway_url: Option<String>,
    pub user: User,
    #[serde(default)]
    pub application: Option<ReadyApplication>,
    #[serde(default)]
    pub guilds: Vec<Guild>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ReadyApplication {
    pub id: Id,
    #[serde(default)]
    pub flags: u64,
}

/// Interaction types (callback side: [`CallbackType`]).
pub mod interaction_type {
    pub const PING: u8 = 1;
    pub const APPLICATION_COMMAND: u8 = 2;
    pub const MESSAGE_COMPONENT: u8 = 3;
    pub const AUTOCOMPLETE: u8 = 4;
    pub const MODAL_SUBMIT: u8 = 5;
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Interaction {
    pub id: Id,
    #[serde(default)]
    pub application_id: Id,
    #[serde(rename = "type")]
    pub kind: u8,
    #[serde(default)]
    pub data: Value,
    #[serde(default)]
    pub guild_id: Option<Id>,
    #[serde(default)]
    pub channel_id: Option<Id>,
    #[serde(default)]
    pub member: Option<Value>,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub message: Option<Value>,
    #[serde(default)]
    pub app_permissions: Option<String>,
}

impl Interaction {
    /// The invoking user (from `member.user`).
    pub fn user(&self) -> Option<User> {
        let v = self.member.as_ref()?.get("user")?;
        serde_json::from_value(v.clone()).ok()
    }

    /// Command name for application commands and autocomplete.
    pub fn command_name(&self) -> Option<&str> {
        self.data.get("name")?.as_str()
    }

    /// `custom_id` of a component or modal interaction.
    pub fn custom_id(&self) -> Option<&str> {
        self.data.get("custom_id")?.as_str()
    }
}

/// Interaction callback types (spec section 3.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CallbackType {
    Pong = 1,
    ChannelMessage = 4,
    DeferredChannelMessage = 5,
    DeferredUpdateMessage = 6,
    UpdateMessage = 7,
    Autocomplete = 8,
    Modal = 9,
}

/// Message flag: only the invoking user sees the response.
pub const FLAG_EPHEMERAL: u64 = 64;
