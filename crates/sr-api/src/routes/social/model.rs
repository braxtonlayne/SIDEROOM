//! Request and response bodies for the social endpoints (Discord JSON shapes).

use crate::routes::messages::model::iso;
use serde::{Deserialize, Deserializer, Serialize};
use sr_chat::repo::UserSummary;
use sr_chat::social::dm::{DmChannel, DmKind};
use sr_chat::social::notifications::{ChannelOverride, Mute, MuteDuration, ScopeSettings};
use sr_chat::social::presence::{CustomStatus, PresencePrefs};
use sr_chat::social::profile::{Connection, Profile};
use sr_chat::social::read_state::ReadState;
use sr_chat::social::relationships::Relationship;
use sr_core::UserId;
use std::collections::HashMap;
use utoipa::ToSchema;

/// Distinguishes an absent JSON field (`None`) from an explicit `null` (`Some(None)`).
pub fn nullable<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UserObject {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_name: Option<String>,
    /// Avatar hash (`/cdn/avatars/{id}/{hash}.png`), when one is set.
    pub avatar: Option<String>,
    /// Profile banner hash (`/cdn/avatars/{id}/{hash}.png`), when one is set.
    pub banner: Option<String>,
    /// Present (true) for bot accounts only.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub bot: bool,
}

impl UserObject {
    pub fn new(id: UserId, users: &HashMap<UserId, UserSummary>) -> Self {
        match users.get(&id) {
            Some(u) => Self {
                id: id.to_string(),
                username: Some(u.username.clone()),
                global_name: u.display_name.clone(),
                avatar: u.avatar_key.clone(),
                banner: u.banner_key.clone(),
                bot: u.bot,
            },
            None => Self {
                id: id.to_string(),
                username: None,
                global_name: None,
                avatar: None,
                banner: None,
                bot: false,
            },
        }
    }
}

// ---- relationships -------------------------------------------------------

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RelationshipObject {
    /// The other user's id.
    pub id: String,
    /// 1 friend, 2 blocked, 3 incoming request, 4 outgoing request.
    #[serde(rename = "type")]
    pub kind: i16,
    pub nickname: Option<String>,
    pub since: String,
    pub user: UserObject,
}

impl RelationshipObject {
    pub fn new(r: &Relationship, users: &HashMap<UserId, UserSummary>) -> Self {
        Self {
            id: r.target.to_string(),
            kind: r.kind.code(),
            nickname: r.nickname.clone(),
            since: iso(r.since_ms),
            user: UserObject::new(r.target, users),
        }
    }
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct PutRelationshipBody {
    /// 1 (default) sends or accepts a friend request, 2 blocks.
    #[serde(rename = "type")]
    pub kind: Option<i16>,
}

/// `POST /users/@me/relationships`: send a friend request by username.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AddRelationshipBody {
    /// The target's username, case-insensitive. The platform has no
    /// discriminators; a `discriminator` field is accepted and ignored.
    pub username: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct NicknameBody {
    /// Empty or null clears the nickname (max 32 characters).
    pub nickname: Option<String>,
}

// ---- notes ------------------------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
pub struct NoteObject {
    pub user_id: String,
    pub note_user_id: String,
    pub note: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct NoteBody {
    /// Empty or null clears the note (max 256 characters).
    pub note: Option<String>,
}

// ---- DMs --------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DmChannelObject {
    pub id: String,
    /// 1 DM, 3 group DM.
    #[serde(rename = "type")]
    pub kind: u8,
    /// Everyone except the viewer, in join order.
    pub recipients: Vec<UserObject>,
    pub last_message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Set only on a conversation sitting in the viewer's Message Requests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_message_request: Option<bool>,
}

impl DmChannelObject {
    pub fn new(dm: &DmChannel, viewer: UserId, users: &HashMap<UserId, UserSummary>) -> Self {
        Self {
            id: dm.id.to_string(),
            kind: if dm.kind == DmKind::Group { 3 } else { 1 },
            recipients: dm
                .recipients
                .iter()
                .filter(|u| **u != viewer)
                .map(|u| UserObject::new(*u, users))
                .collect(),
            last_message_id: dm.last_message_id.map(|m| m.to_string()),
            owner_id: dm.owner.map(|o| o.to_string()),
            name: dm.name.clone(),
            icon: dm.icon_key.clone(),
            is_message_request: None,
        }
    }
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct CreateDmBody {
    /// A 1:1 DM with this user.
    pub recipient_id: Option<String>,
    /// A group DM with these users (the caller becomes the owner).
    pub recipients: Option<Vec<String>>,
    pub name: Option<String>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct EditDmBody {
    pub name: Option<String>,
    /// Icon key; `null` removes the icon.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub icon: Option<Option<String>>,
    /// Transfers ownership (owner only).
    pub owner_id: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MessageRequestBody {
    /// `accepted`, `ignored` or `spam`.
    pub state: String,
}

// ---- read state ---------------------------------------------------------------

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct AckBody {
    /// With `manual`, the mention badge to restore.
    pub mention_count: Option<u32>,
    /// "Mark as unread": move the marker to exactly this message.
    pub manual: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AckResponse {
    pub token: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct BulkAckEntry {
    pub channel_id: String,
    pub message_id: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct BulkAckBody {
    pub read_states: Vec<BulkAckEntry>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReadStateObject {
    /// The channel id.
    pub id: String,
    pub last_message_id: Option<String>,
    pub mention_count: u32,
    pub last_pin_timestamp: Option<String>,
}

impl From<&ReadState> for ReadStateObject {
    fn from(s: &ReadState) -> Self {
        Self {
            id: s.channel.to_string(),
            last_message_id: s.last_read.map(|m| m.to_string()),
            mention_count: s.mention_count,
            last_pin_timestamp: s.last_pin_ms.map(iso),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReadStatesResponse {
    pub entries: Vec<ReadStateObject>,
}

// ---- notification settings ----------------------------------------------------

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MuteConfigObject {
    /// `null` when muted until turned off.
    pub end_time: Option<String>,
    /// `-1` when muted until turned off.
    pub selected_time_window: Option<i64>,
}

fn mute_config(m: &Mute) -> Option<MuteConfigObject> {
    m.muted.then(|| MuteConfigObject {
        end_time: m.end_ms.map(iso),
        selected_time_window: m.end_ms.is_none().then_some(-1),
    })
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ChannelOverrideObject {
    pub channel_id: String,
    pub message_notifications: Option<i16>,
    pub muted: bool,
    pub mute_config: Option<MuteConfigObject>,
}

impl From<&ChannelOverride> for ChannelOverrideObject {
    fn from(o: &ChannelOverride) -> Self {
        Self {
            channel_id: o.channel.to_string(),
            message_notifications: o.level.map(|l| l.code()),
            muted: o.mute.muted,
            mute_config: mute_config(&o.mute),
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UserGuildSettingsObject {
    /// `null` for the DM scope.
    pub guild_id: Option<String>,
    /// 0 all messages, 1 only mentions, 2 nothing; `null` inherits the guild default.
    pub message_notifications: Option<i16>,
    pub muted: bool,
    pub mute_config: Option<MuteConfigObject>,
    pub suppress_everyone: bool,
    pub suppress_roles: bool,
    pub suppress_highlights: bool,
    pub mute_scheduled_events: bool,
    pub mobile_push: bool,
    pub channel_overrides: Vec<ChannelOverrideObject>,
}

impl UserGuildSettingsObject {
    pub fn new(s: &ScopeSettings, overrides: &[ChannelOverride]) -> Self {
        Self {
            guild_id: s.guild.map(|g| g.to_string()),
            message_notifications: s.level.map(|l| l.code()),
            muted: s.mute.muted,
            mute_config: mute_config(&s.mute),
            suppress_everyone: s.suppress_everyone,
            suppress_roles: s.suppress_roles,
            suppress_highlights: s.suppress_highlights,
            mute_scheduled_events: s.mute_scheduled_events,
            mobile_push: s.mobile_push,
            channel_overrides: overrides.iter().map(ChannelOverrideObject::from).collect(),
        }
    }
}

/// Mute window of a change that sets `muted: true`.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct MuteConfigIn {
    /// 900, 3600, 10800, 28800, 86400 seconds, or -1 for "until I turn it back on".
    pub selected_time_window: Option<i64>,
}

impl MuteConfigIn {
    /// `None` for a window that is not one of the presets.
    pub fn duration(&self) -> Option<MuteDuration> {
        Some(match self.selected_time_window {
            Some(900) => MuteDuration::Minutes15,
            Some(3600) => MuteDuration::Hour1,
            Some(10_800) => MuteDuration::Hours3,
            Some(28_800) => MuteDuration::Hours8,
            Some(86_400) => MuteDuration::Hours24,
            Some(-1) | None => MuteDuration::UntilTurnedOff,
            Some(_) => return None,
        })
    }
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ChannelOverrideBody {
    /// 0, 1 or 2; `null` inherits.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<i16>)]
    pub message_notifications: Option<Option<i16>>,
    /// `false` unmutes; `true` mutes for `mute_config` (default until turned off).
    pub muted: Option<bool>,
    pub mute_config: Option<MuteConfigIn>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct GuildSettingsBody {
    /// 0, 1 or 2; `null` inherits the guild default.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<i16>)]
    pub message_notifications: Option<Option<i16>>,
    pub muted: Option<bool>,
    pub mute_config: Option<MuteConfigIn>,
    pub suppress_everyone: Option<bool>,
    pub suppress_roles: Option<bool>,
    pub suppress_highlights: Option<bool>,
    pub mute_scheduled_events: Option<bool>,
    pub mobile_push: Option<bool>,
    /// Channel id to override.
    #[schema(value_type = Option<Object>)]
    pub channel_overrides: Option<HashMap<String, ChannelOverrideBody>>,
}

// ---- profile and presence -----------------------------------------------------

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CustomStatusObject {
    pub text: Option<String>,
    pub emoji_name: Option<String>,
    pub emoji_id: Option<String>,
    pub expires_at: Option<String>,
}

impl CustomStatusObject {
    pub fn new(c: &CustomStatus) -> Option<Self> {
        (!c.is_empty()).then(|| Self {
            text: c.text.clone(),
            emoji_name: c.emoji_name.clone(),
            emoji_id: c.emoji_id.map(|i| i.to_string()),
            expires_at: c.expires_ms.map(iso),
        })
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ConnectionObject {
    #[serde(rename = "type")]
    pub provider: String,
    pub id: String,
    pub name: String,
    pub verified: bool,
}

impl From<&Connection> for ConnectionObject {
    fn from(c: &Connection) -> Self {
        Self {
            provider: c.provider.clone(),
            id: c.external_id.clone(),
            name: c.display_name.clone(),
            verified: c.verified,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserProfileFields {
    pub bio: Option<String>,
    pub pronouns: Option<String>,
    pub banner: Option<String>,
    pub accent_color: Option<u32>,
}

impl From<&Profile> for UserProfileFields {
    fn from(p: &Profile) -> Self {
        Self {
            bio: p.bio.clone(),
            pronouns: p.pronouns.clone(),
            banner: p.banner_key.clone(),
            accent_color: p.accent_color,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ProfileObject {
    pub user: UserObject,
    pub user_profile: UserProfileFields,
    pub connected_accounts: Vec<ConnectionObject>,
    /// Friends of both the viewer and the profile owner (omitted on the viewer's own
    /// profile).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mutual_friends: Option<Vec<UserObject>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mutual_friends_count: Option<usize>,
    /// Guilds both belong to, with the owner's nickname and join time there.
    /// Present when `with_mutual_guilds` is true (the default) and the profile is
    /// not the viewer's own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mutual_guilds: Option<Vec<MutualGuildObject>>,
    /// When the viewer and the owner became friends.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub friends_since: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MutualGuildObject {
    pub id: String,
    /// The profile owner's nickname in that guild.
    pub nick: Option<String>,
    /// When the profile owner joined that guild.
    pub member_since: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CustomStatusBody {
    pub text: Option<String>,
    pub emoji_name: Option<String>,
    pub emoji_id: Option<String>,
    /// ISO 8601 expiry; omitted keeps it until cleared.
    pub expires_at: Option<String>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct EditSelfBody {
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub bio: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub pronouns: Option<Option<String>>,
    /// Display name (Discord `global_name`, 32 characters at most); `null` clears it so
    /// the username shows. The username itself changes through `PUT /sr/auth/username`.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub global_name: Option<Option<String>>,
    /// New avatar as a `data:image/{png,jpeg,gif,webp};base64,...` URI (8 MiB at
    /// most; cropped to a centred square, EXIF stripped); `null` removes it.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub avatar: Option<Option<String>>,
    /// New profile banner as an image data URI (aspect ratio kept, at most
    /// 1920x1080); `null` removes it.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub banner: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<u32>)]
    pub accent_color: Option<Option<u32>>,
    /// `null` clears the custom status.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<CustomStatusBody>)]
    pub custom_status: Option<Option<CustomStatusBody>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SelfObject {
    pub user: UserObject,
    pub bio: Option<String>,
    pub pronouns: Option<String>,
    pub banner: Option<String>,
    pub accent_color: Option<u32>,
    pub custom_status: Option<CustomStatusObject>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PresenceObject {
    /// `online`, `idle`, `dnd` or `invisible`.
    pub status: String,
    pub status_expires_at: Option<String>,
    pub custom_status: Option<CustomStatusObject>,
}

impl From<&PresencePrefs> for PresenceObject {
    fn from(p: &PresencePrefs) -> Self {
        Self {
            status: p.status.as_str().to_owned(),
            status_expires_at: p.status_expires_ms.map(iso),
            custom_status: CustomStatusObject::new(&p.custom),
        }
    }
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct PresenceBody {
    /// `online`, `idle`, `dnd` or `invisible`.
    pub status: Option<String>,
    /// Revert to `online` after this many seconds (for example "DND for 1 hour").
    pub duration_seconds: Option<i64>,
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<CustomStatusBody>)]
    pub custom_status: Option<Option<CustomStatusBody>>,
}
