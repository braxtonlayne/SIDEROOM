//! Domain types for guilds, channels, roles and messages (docs/04 sections 2 and 3).
//!
//! Timestamps are Unix milliseconds so the logic stays clock-injectable and wasm-clean.

use crate::threads::model::{ForumConfig, ThreadInfo};
use sr_core::permissions::{Overwrite, Permissions};
use sr_core::{AttachmentId, ChannelId, GuildId, MessageId, RoleId, UserId};

/// Discord channel type numbers (docs/04 section 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelType {
    Text,
    Dm,
    Voice,
    GroupDm,
    Category,
    Announcement,
    AnnouncementThread,
    PublicThread,
    PrivateThread,
    Stage,
    Forum,
    Media,
}

impl ChannelType {
    pub const fn code(self) -> i16 {
        match self {
            Self::Text => 0,
            Self::Dm => 1,
            Self::Voice => 2,
            Self::GroupDm => 3,
            Self::Category => 4,
            Self::Announcement => 5,
            Self::AnnouncementThread => 10,
            Self::PublicThread => 11,
            Self::PrivateThread => 12,
            Self::Stage => 13,
            Self::Forum => 15,
            Self::Media => 16,
        }
    }

    pub const fn from_code(code: i16) -> Option<Self> {
        Some(match code {
            0 => Self::Text,
            1 => Self::Dm,
            2 => Self::Voice,
            3 => Self::GroupDm,
            4 => Self::Category,
            5 => Self::Announcement,
            10 => Self::AnnouncementThread,
            11 => Self::PublicThread,
            12 => Self::PrivateThread,
            13 => Self::Stage,
            15 => Self::Forum,
            16 => Self::Media,
            _ => return None,
        })
    }

    pub const fn is_dm(self) -> bool {
        matches!(self, Self::Dm | Self::GroupDm)
    }

    pub const fn is_thread(self) -> bool {
        matches!(
            self,
            Self::AnnouncementThread | Self::PublicThread | Self::PrivateThread
        )
    }

    /// Whether messages can be posted directly in this channel type
    /// (forum and media channels take posts as threads; categories take nothing).
    pub const fn accepts_messages(self) -> bool {
        matches!(
            self,
            Self::Text
                | Self::Dm
                | Self::Voice
                | Self::GroupDm
                | Self::Announcement
                | Self::AnnouncementThread
                | Self::PublicThread
                | Self::PrivateThread
                | Self::Stage
        )
    }

    /// Types that may be created through the guild channel endpoint (threads have their own).
    pub const fn creatable_in_guild(self) -> bool {
        matches!(
            self,
            Self::Text
                | Self::Voice
                | Self::Category
                | Self::Announcement
                | Self::Stage
                | Self::Forum
                | Self::Media
        )
    }

    /// Forum and media channels: posts are threads.
    pub const fn is_forum_like(self) -> bool {
        matches!(self, Self::Forum | Self::Media)
    }

    /// Parents that hold ordinary threads (started from a message or on their own).
    pub const fn hosts_threads(self) -> bool {
        matches!(self, Self::Text | Self::Announcement)
    }

    pub const fn is_voice_like(self) -> bool {
        matches!(self, Self::Voice | Self::Stage)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guild {
    pub id: GuildId,
    pub name: String,
    pub description: Option<String>,
    pub icon_key: Option<String>,
    /// Banner hash (`/cdn/icons/{guild}/{hash}.png`).
    pub banner_key: Option<String>,
    /// Invite splash hash.
    pub splash_key: Option<String>,
    pub owner_id: UserId,
    pub is_staff_guild: bool,
    pub system_channel_id: Option<ChannelId>,
    /// Voice channel that idle members are moved to (Discord `afk_channel_id`).
    pub afk_channel_id: Option<ChannelId>,
    /// Seconds of voice inactivity before the move: 60, 300, 900, 1800 or 3600.
    pub afk_timeout: u32,
    pub verification_level: u8,
    pub default_message_notifications: u8,
    pub explicit_content_filter: u8,
    pub preferred_locale: String,
    /// Vanity invite code (`/invite/<code>`), free of charge (LNG-INV-004).
    pub vanity_code: Option<String>,
    /// Community channels (G-76): where the rules, public updates and safety alerts live.
    pub rules_channel_id: Option<ChannelId>,
    pub public_updates_channel_id: Option<ChannelId>,
    pub safety_alerts_channel_id: Option<ChannelId>,
    /// Server Protection (D-224): links whose text names another host show their raw target
    /// and a warning. Read from `guild_community`; visible to every member.
    pub hide_suspicious_links: bool,
    /// 0 none, 1 elevated: moderator actions need a recent second factor (G-36).
    pub mfa_level: u8,
    /// Discord `system_channel_flags` bit field (see [`system_flags`]).
    pub system_channel_flags: u32,
    pub widget_enabled: bool,
    /// The channel the widget invites to.
    pub widget_channel_id: Option<ChannelId>,
    /// Server traits (J-07): at most five labels with an emoji.
    pub traits: Vec<crate::guild_traits::GuildTrait>,
    /// Timed incident actions and the last join-rate alert (D-243), read from `guild_community`.
    pub incidents: IncidentsData,
}

/// `incidents_data` of the guild object (D-243). A time in the past means the pause ended.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IncidentsData {
    pub invites_disabled_until_ms: Option<i64>,
    pub dms_disabled_until_ms: Option<i64>,
    pub raid_detected_at_ms: Option<i64>,
}

/// Bits of [`Guild::system_channel_flags`] (Discord values).
pub mod system_flags {
    pub const SUPPRESS_JOIN_NOTIFICATIONS: u32 = 1;
    pub const SUPPRESS_PREMIUM_SUBSCRIPTIONS: u32 = 1 << 1;
    pub const SUPPRESS_GUILD_REMINDER_NOTIFICATIONS: u32 = 1 << 2;
    pub const SUPPRESS_JOIN_NOTIFICATION_REPLIES: u32 = 1 << 3;
    pub const SUPPRESS_ROLE_SUBSCRIPTION_PURCHASE_NOTIFICATIONS: u32 = 1 << 4;
    pub const SUPPRESS_ROLE_SUBSCRIPTION_PURCHASE_NOTIFICATION_REPLIES: u32 = 1 << 5;
    /// Every defined bit.
    pub const ALL: u32 = 0b11_1111;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    pub id: RoleId,
    pub guild_id: GuildId,
    pub name: String,
    pub color: u32,
    /// Second gradient stop (J-08); `color` is the first.
    pub secondary_color: Option<u32>,
    /// Third gradient stop; only with a second one (the holographic preset).
    pub tertiary_color: Option<u32>,
    pub hoist: bool,
    pub position: i32,
    pub permissions: Permissions,
    pub mentionable: bool,
    pub managed: bool,
    pub unicode_emoji: Option<String>,
    pub icon_key: Option<String>,
}

impl Role {
    /// The `@everyone` role shares its id with the guild.
    pub fn is_everyone(&self) -> bool {
        self.id.get() == self.guild_id.get()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub guild_id: GuildId,
    pub user_id: UserId,
    pub nick: Option<String>,
    /// Explicit role assignments; the implicit `@everyone` role is not listed.
    pub role_ids: Vec<RoleId>,
    pub joined_at_ms: i64,
    pub communication_disabled_until_ms: Option<i64>,
    /// Membership screening not yet accepted (LNG-GLD-008): read-only until it is.
    pub pending: bool,
    /// Per-server profile (LNG-GLD-012): avatar hash, same URL shape as a user avatar.
    pub avatar_key: Option<String>,
    /// Per-server profile banner hash.
    pub banner_key: Option<String>,
    /// Per-server bio.
    pub bio: Option<String>,
    /// Per-server pronouns, theme, effect and decoration (G-52).
    pub extras: Box<MemberExtras>,
}

/// Per-server profile extras (D-044: free for everyone). `None` falls back to the account's
/// own value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemberExtras {
    pub pronouns: Option<String>,
    /// The two profile theme colours (0xRRGGBB), primary first; both or neither.
    pub theme_colors: Option<[u32; 2]>,
    pub profile_effect: Option<String>,
    pub avatar_decoration: Option<String>,
    /// Discord's `GuildMemberFlags` (`sr_core::member_flags`, D-245); 0 until a setter runs.
    pub flags: u32,
}

impl Member {
    pub fn is_timed_out(&self, now_ms: i64) -> bool {
        self.communication_disabled_until_ms
            .is_some_and(|until| until > now_ms)
    }
}

/// `video_quality_mode` 1: the adaptive default.
pub const VIDEO_QUALITY_AUTO: u8 = 1;
/// `video_quality_mode` 2: camera tracks may use the 720p tier regardless of occupancy.
pub const VIDEO_QUALITY_FULL: u8 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    pub id: ChannelId,
    /// `None` for DMs and group DMs.
    pub guild_id: Option<GuildId>,
    pub kind: ChannelType,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub position: i32,
    pub parent_id: Option<ChannelId>,
    pub nsfw: bool,
    pub rate_limit_per_user: u32,
    pub bitrate: Option<u32>,
    pub user_limit: Option<u32>,
    pub last_message_id: Option<MessageId>,
    pub owner_id: Option<UserId>,
    /// Discord channel flags (`PINNED`, `REQUIRE_TAG`, ...; see [`crate::threads::model::channel_flags`]).
    pub flags: u32,
    /// Present for thread channels (types 10/11/12).
    pub thread: Option<ThreadInfo>,
    /// Forum/media settings and thread defaults.
    pub forum: ForumConfig,
    /// Voice and stage channels: 1 auto (the adaptive default), 2 full (camera tracks may use
    /// the 720p tier whatever the occupancy). Discord's `video_quality_mode`; always 1 on other
    /// channels.
    pub video_quality_mode: u8,
    /// A voice channel with a standing (persistent, not ended) watch party: a Watch party room
    /// (D-208). Read from the party table with the channel; never stored on the channel itself.
    pub watch_party_room: bool,
}

impl Channel {
    pub fn new(id: ChannelId, guild_id: Option<GuildId>, kind: ChannelType) -> Self {
        Self {
            id,
            guild_id,
            kind,
            name: None,
            topic: None,
            position: 0,
            parent_id: None,
            nsfw: false,
            rate_limit_per_user: 0,
            bitrate: None,
            user_limit: None,
            last_message_id: None,
            owner_id: None,
            flags: 0,
            thread: None,
            forum: ForumConfig::default(),
            video_quality_mode: VIDEO_QUALITY_AUTO,
            watch_party_room: false,
        }
    }
}

/// A channel together with its permission overwrites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelWithOverwrites {
    pub channel: Channel,
    pub overwrites: Vec<Overwrite>,
}

/// Discord message types we implement (numbers match Discord).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageType {
    Default,
    RecipientAdd,
    RecipientRemove,
    Call,
    ChannelNameChange,
    ChannelIconChange,
    ChannelPinnedMessage,
    UserJoin,
    /// A member supported the server (Discord's boost type 8; tiers 9 to 11 follow the level).
    ServerSupport,
    ServerSupportLevel1,
    ServerSupportLevel2,
    ServerSupportLevel3,
    /// An announcement channel was followed into this channel (type 12).
    ChannelFollowAdd,
    ThreadCreated,
    Reply,
    ChatInputCommand,
    ThreadStarterMessage,
    ContextMenuCommand,
    /// An AutoMod alert or an incident alert the system user posted, with moderation buttons
    /// (type 24, D-243/D-244).
    AutoModerationAction,
    /// The live card of a watch party in its bound channel (SIDE ROOM type 200, WP-BE-008): the system
    /// user posts it when the party starts and rewrites it while the party changes and when it ends.
    WatchParty,
}

impl MessageType {
    pub const fn code(self) -> i16 {
        match self {
            Self::Default => 0,
            Self::RecipientAdd => 1,
            Self::RecipientRemove => 2,
            Self::Call => 3,
            Self::ChannelNameChange => 4,
            Self::ChannelIconChange => 5,
            Self::ChannelPinnedMessage => 6,
            Self::UserJoin => 7,
            Self::ServerSupport => 8,
            Self::ServerSupportLevel1 => 9,
            Self::ServerSupportLevel2 => 10,
            Self::ServerSupportLevel3 => 11,
            Self::ChannelFollowAdd => 12,
            Self::ThreadCreated => 18,
            Self::Reply => 19,
            Self::ChatInputCommand => 20,
            Self::ThreadStarterMessage => 21,
            Self::ContextMenuCommand => 23,
            Self::AutoModerationAction => 24,
            Self::WatchParty => 200,
        }
    }

    pub const fn from_code(code: i16) -> Option<Self> {
        Some(match code {
            0 => Self::Default,
            1 => Self::RecipientAdd,
            2 => Self::RecipientRemove,
            3 => Self::Call,
            4 => Self::ChannelNameChange,
            5 => Self::ChannelIconChange,
            6 => Self::ChannelPinnedMessage,
            7 => Self::UserJoin,
            8 => Self::ServerSupport,
            9 => Self::ServerSupportLevel1,
            10 => Self::ServerSupportLevel2,
            11 => Self::ServerSupportLevel3,
            12 => Self::ChannelFollowAdd,
            18 => Self::ThreadCreated,
            19 => Self::Reply,
            20 => Self::ChatInputCommand,
            21 => Self::ThreadStarterMessage,
            23 => Self::ContextMenuCommand,
            24 => Self::AutoModerationAction,
            200 => Self::WatchParty,
            _ => return None,
        })
    }

    /// Authors may edit only user-authored content types.
    pub const fn is_editable(self) -> bool {
        matches!(
            self,
            Self::Default | Self::Reply | Self::ChatInputCommand | Self::ContextMenuCommand
        )
    }

    /// Discord lets authors delete anything they wrote, but system messages
    /// (joins, pins, ...) only by users with MANAGE_MESSAGES.
    pub const fn is_system(self) -> bool {
        !matches!(
            self,
            Self::Default | Self::Reply | Self::ChatInputCommand | Self::ContextMenuCommand
        )
    }

    /// System messages other than these cannot be pinned.
    pub const fn is_pinnable(self) -> bool {
        !self.is_system()
    }
}

/// Id of the system user "SIDE ROOM" (`accounts.flags` bit 0; doc 06 section 8). It posts
/// as the feature modules and the platform but is never a visible member (D-188).
pub const SYSTEM_USER_ID: u64 = 1;

/// Discord message flag bits (subset).
pub mod message_flags {
    pub const CROSSPOSTED: u32 = 1 << 0;
    pub const IS_CROSSPOST: u32 = 1 << 1;
    pub const SUPPRESS_EMBEDS: u32 = 1 << 2;
    pub const EPHEMERAL: u32 = 1 << 6;
    pub const SUPPRESS_NOTIFICATIONS: u32 = 1 << 12;
    pub const IS_VOICE_MESSAGE: u32 = 1 << 13;
    /// Text-to-speech request (LNG-MSG-002); stored as a flag (open question 152).
    pub const TTS: u32 = 1 << 14;
    /// SIDE ROOM extension (D-188): the message was written by the system user, which the
    /// client labels with a `SYSTEM` badge. Set when a message object is built, never stored.
    pub const SYSTEM_TAG: u32 = 1 << 30;
}

/// D-121: user_deleted / staff_removed are hidden from user views.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Visible,
    UserDeleted,
    StaffRemoved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageReference {
    pub message_id: MessageId,
    pub channel_id: ChannelId,
    /// Present for forwards (`message_reference.type = 1`): the copy taken at forward time.
    pub forward: Option<crate::message::ForwardSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub id: MessageId,
    pub channel_id: ChannelId,
    pub guild_id: Option<GuildId>,
    /// A user id (webhook/app authors are out of scope for this module).
    pub author_id: UserId,
    pub kind: MessageType,
    pub content: String,
    pub flags: u32,
    pub edited_at_ms: Option<i64>,
    pub reference: Option<MessageReference>,
    pub attachment_ids: Vec<AttachmentId>,
    /// Embed payloads (open question 98); the count is `embeds.len()`.
    pub embeds: Vec<crate::message::Embed>,
    pub components: Vec<crate::message::Component>,
    pub sticker_ids: Vec<u64>,
    pub poll: Option<crate::message::Poll>,
    pub mention_everyone: bool,
    pub mention_user_ids: Vec<UserId>,
    pub mention_role_ids: Vec<RoleId>,
    pub mention_channel_ids: Vec<ChannelId>,
    pub pinned: bool,
    pub visibility: Visibility,
    pub deleted_at_ms: Option<i64>,
    pub purge_after_ms: Option<i64>,
    /// D-122: a moderation hold blocks purge.
    pub hold_id: Option<u64>,
}

impl Message {
    /// Whether the message carries a poll.
    pub fn has_poll(&self) -> bool {
        self.poll.is_some()
    }

    /// Number of embeds (the API reports the count against the limit).
    pub fn embed_count(&self) -> usize {
        self.embeds.len()
    }

    pub fn is_visible(&self) -> bool {
        self.visibility == Visibility::Visible
    }
}

/// Whether a message of this type counts towards slowmode. System messages
/// (joins, pins, call notices) do not (open question 154).
pub fn counts_for_slowmode(kind: MessageType) -> bool {
    matches!(kind, MessageType::Default | MessageType::Reply)
}
