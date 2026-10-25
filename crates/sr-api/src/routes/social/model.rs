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
    /// The server tag this user displays (LNG-SSET-012); Discord's `primary_guild`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_guild: Option<PrimaryGuildObject>,
    /// Patron badge and ring show for this user (D-256): the Patron entitlement is active or in
    /// grace and the badge is not hidden. Always present; never a third state.
    pub sr_patron: bool,
}

/// A displayed server tag: `{identity_guild_id, identity_enabled, tag, badge, color}`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PrimaryGuildObject {
    pub identity_guild_id: String,
    pub identity_enabled: bool,
    pub tag: String,
    /// Badge icon id (0 to 15).
    pub badge: u8,
    /// Badge colour, 24-bit RGB.
    pub color: u32,
}

impl PrimaryGuildObject {
    pub fn of(p: &sr_chat::settings::tags::PrimaryGuild) -> Self {
        Self {
            identity_guild_id: p.guild_id.to_string(),
            identity_enabled: true,
            tag: p.tag.clone(),
            badge: p.badge,
            color: p.color,
        }
    }
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
                primary_guild: u.primary_guild.as_ref().map(PrimaryGuildObject::of),
                sr_patron: u.patron,
            },
            None => Self {
                id: id.to_string(),
                username: None,
                global_name: None,
                avatar: None,
                banner: None,
                bot: false,
                primary_guild: None,
                sr_patron: false,
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
    /// With `is_message_request`: `server_filter` or `new_account` when a server's Protection
    /// settings sent the conversation there (D-224), or `spam` when the recipient's DM spam
    /// filter flagged it or the recipient filed it as spam (it then lists under "Likely spam").
    /// SIDE ROOM extension.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sr_request_reason: Option<String>,
    /// The largest file the viewer may upload in this conversation, in bytes (D-233).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sr_max_upload_bytes: Option<u64>,
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
            sr_request_reason: None,
            sr_max_upload_bytes: None,
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
    /// Link to the account's public page, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Whether the owner shows stats / activity of this connection.
    pub show_activity: bool,
    /// Public stats (`{"followers": 12}`); empty unless `show_activity`.
    #[schema(value_type = Object)]
    pub stats: serde_json::Value,
    /// Hidden connections are only listed to their owner.
    pub visible: bool,
}

/// The only stats keys a connection card shows (G-53); the client translates them.
pub const STAT_KEYS: [&str; 5] = ["followers", "subscribers", "karma", "stars", "repos"];

/// `stats` restricted to [`STAT_KEYS`] (rows stored before the catalogue keep `public_repos`,
/// which reads as `repos`).
pub fn clean_stats(stats: &serde_json::Value) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    if let Some(m) = stats.as_object() {
        for (k, v) in m {
            let key = if k == "public_repos" {
                "repos"
            } else {
                k.as_str()
            };
            if STAT_KEYS.contains(&key) && v.is_number() {
                out.insert(key.to_owned(), v.clone());
            }
        }
    }
    serde_json::Value::Object(out)
}

impl From<&Connection> for ConnectionObject {
    fn from(c: &Connection) -> Self {
        Self {
            provider: c.provider.clone(),
            id: c.external_id.clone(),
            name: c.display_name.clone(),
            verified: c.verified,
            url: c.url.clone(),
            show_activity: c.show_activity,
            stats: if c.show_activity {
                clean_stats(&c.stats)
            } else {
                serde_json::json!({})
            },
            visible: c.visible,
        }
    }
}

/// A public profile badge: the catalogue id with English fallback text and an icon name
/// (G-53). Clients translate the ids they know and fall back to `description`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BadgeObject {
    pub id: String,
    pub description: String,
    pub icon: String,
}

impl BadgeObject {
    pub fn of(id: &str) -> Self {
        match sr_admin::badge(id) {
            Some(b) => Self {
                id: b.id.to_owned(),
                description: b.description.to_owned(),
                icon: b.icon.to_owned(),
            },
            None => Self {
                id: id.to_owned(),
                description: String::new(),
                icon: "info".to_owned(),
            },
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserProfileFields {
    pub bio: Option<String>,
    pub pronouns: Option<String>,
    pub banner: Option<String>,
    pub accent_color: Option<u32>,
    /// Slug of the avatar decoration, if one is set.
    pub avatar_decoration: Option<String>,
    /// Slug of the profile effect, if one is set.
    pub profile_effect: Option<String>,
    /// The two profile theme colours (0xRRGGBB), primary first.
    pub theme_colors: Option<[u32; 2]>,
}

impl From<&Profile> for UserProfileFields {
    fn from(p: &Profile) -> Self {
        Self {
            bio: p.bio.clone(),
            pronouns: p.pronouns.clone(),
            banner: p.banner_key.clone(),
            accent_color: p.accent_color,
            avatar_decoration: p.avatar_decoration.clone(),
            profile_effect: p.profile_effect.clone(),
            theme_colors: p.theme_colors,
        }
    }
}

/// The member's per-server profile (`GET /users/{id}/profile?guild_id=`).
#[derive(Debug, Serialize, ToSchema)]
pub struct GuildMemberProfileObject {
    pub guild_id: String,
    pub nick: Option<String>,
    pub avatar: Option<String>,
    pub banner: Option<String>,
    pub bio: Option<String>,
    /// Per-server pronouns, theme, effect and decoration (G-52); `null` means the account's own.
    pub pronouns: Option<String>,
    pub theme_colors: Option<[u32; 2]>,
    pub profile_effect: Option<String>,
    pub avatar_decoration: Option<String>,
    pub joined_at: String,
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
    /// Public badges: `owner`, `staff`, `patron`, `contributor`, `early_member`,
    /// `bug_hunter`, `honorary`, `app_developer`, `bot`.
    pub badges: Vec<BadgeObject>,
    /// When the account was created ("Member since").
    pub member_since: String,
    /// The owner's profile in `guild_id` (only when the viewer shares that server).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_member_profile: Option<GuildMemberProfileObject>,
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
    /// Slug of an approved avatar decoration (lowercase letters, digits, `-`, 48 at most);
    /// `null` removes it.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub avatar_decoration: Option<Option<String>>,
    /// Slug of an approved profile effect; `null` removes it.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub profile_effect: Option<Option<String>>,
    /// The profile theme as two 0xRRGGBB colours `[primary, accent]`; `null` clears it.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<Vec<u32>>)]
    pub theme_colors: Option<Option<[u32; 2]>>,
    /// `null` clears the custom status.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<CustomStatusBody>)]
    pub custom_status: Option<Option<CustomStatusBody>>,
    /// The account's interface language as a BCP 47 tag (`de`, `pt-BR`, `zh-Hant-TW`); mails,
    /// push text and system DMs follow it. Malformed tags and the pseudo-locales (`en-XA`,
    /// `ar-XB`) are refused; tags without a translation fall back to English when used.
    #[serde(default)]
    pub locale: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SelfObject {
    pub user: UserObject,
    pub bio: Option<String>,
    pub pronouns: Option<String>,
    pub banner: Option<String>,
    pub accent_color: Option<u32>,
    pub avatar_decoration: Option<String>,
    pub profile_effect: Option<String>,
    pub theme_colors: Option<[u32; 2]>,
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

#[cfg(test)]
mod catalogue_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn badges_carry_the_catalogue_text_and_icon() {
        let b = BadgeObject::of("bug_hunter");
        assert_eq!(
            (b.id.as_str(), b.description.as_str()),
            ("bug_hunter", "Bug Hunter")
        );
        assert_eq!(b.icon, "alert");
        let unknown = BadgeObject::of("brand-new");
        assert_eq!(
            (unknown.id.as_str(), unknown.description.as_str()),
            ("brand-new", "")
        );
        // Every id profiles can send is in the one catalogue.
        for id in [
            "owner",
            "staff",
            "patron",
            "early_member",
            "contributor",
            "app_developer",
            "bot",
        ] {
            assert!(sr_admin::badge(id).is_some(), "{id}");
        }
    }

    #[test]
    fn stats_keep_only_the_translated_keys() {
        let cleaned = clean_stats(&json!({
            "followers": 3, "public_repos": 8, "following": 9, "videos": 2, "karma": "x",
            "stars": 5,
        }));
        assert_eq!(cleaned, json!({"followers": 3, "repos": 8, "stars": 5}));
    }
}
