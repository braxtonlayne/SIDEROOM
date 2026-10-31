//! Discord-shaped request and response bodies for the guild endpoints.
//! Ids are decimal strings, permissions are decimal strings, timestamps ISO8601.

use serde::{Deserialize, Deserializer, Serialize};
use sr_chat::admin_repo::Ban;
use sr_chat::domain::{Channel, ChannelType, ChannelWithOverwrites, Guild, Member, Role};
use sr_chat::guild_traits::GuildTrait;
use sr_chat::repo::UserSummary;
use sr_chat::threads::model::{ThreadInfo, ThreadMember};
use sr_core::UserId;
use sr_core::permissions::{Overwrite, OverwriteKind};
use std::collections::HashMap;
use utoipa::ToSchema;

// ------------------------------------------------------------------ time

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Unix milliseconds as `YYYY-MM-DDTHH:MM:SS.mmm+00:00`.
pub fn iso8601(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let t = secs.rem_euclid(86_400);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}+00:00",
        t / 3600,
        t % 3600 / 60,
        t % 60,
        ms.rem_euclid(1000)
    )
}

/// Parse an ISO8601 timestamp with `Z` or a numeric offset into Unix milliseconds.
pub fn parse_iso8601(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let num = |from: usize, len: usize| -> Option<i64> {
        let part = s.get(from..from + len)?;
        part.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| part.parse().ok())?
    };
    if b.len() < 20
        || b.get(4) != Some(&b'-')
        || b.get(7) != Some(&b'-')
        || !matches!(b.get(10), Some(b'T' | b't' | b' '))
        || b.get(13) != Some(&b':')
        || b.get(16) != Some(&b':')
    {
        return None;
    }
    let (y, mo, d) = (num(0, 4)?, num(5, 2)?, num(8, 2)?);
    let (h, mi, sec) = (num(11, 2)?, num(14, 2)?, num(17, 2)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    let mut i = 19;
    let mut frac_ms = 0;
    if b.get(i) == Some(&b'.') {
        let start = i + 1;
        i = start;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        let digits = s.get(start..i)?;
        if digits.is_empty() {
            return None;
        }
        let first3 = format!("{digits:0<3}");
        frac_ms = first3.get(..3)?.parse::<i64>().ok()?;
    }
    let offset_min = match b.get(i)? {
        b'Z' | b'z' if i + 1 == b.len() => 0,
        sign @ (b'+' | b'-') if b.len() == i + 6 && b.get(i + 3) == Some(&b':') => {
            let v = num(i + 1, 2)? * 60 + num(i + 4, 2)?;
            if *sign == b'-' { -v } else { v }
        }
        _ => return None,
    };
    let secs = days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + sec - offset_min * 60;
    Some(secs * 1000 + frac_ms)
}

// ------------------------------------------------------------ deserialize

/// Distinguishes an absent field (`None`) from an explicit `null` (`Some(None)`).
pub fn nullable<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

// --------------------------------------------------------------- responses

/// User object embedded in members, bans and messages. Name fields come from the
/// user lookup (open question 157); without one only the id is known.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[schema(as = GuildUserRef)]
pub struct UserRef {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_name: Option<String>,
    /// Present (true) for bot accounts only.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub bot: bool,
    /// The server tag the user shows next to their name (LNG-SSET-012, read from
    /// `guild_tags` + `user_primary_guilds`; set with `PUT /users/@me/primary-guild`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_guild: Option<crate::routes::social::model::PrimaryGuildObject>,
    /// Patron badge and ring show for this user (D-256): the Patron entitlement is active or in
    /// grace and the badge is not hidden. Always present; never a third state.
    pub sr_patron: bool,
}

impl UserRef {
    /// The bare reference (id only).
    pub fn new(id: impl ToString) -> Self {
        Self {
            id: id.to_string(),
            username: None,
            global_name: None,
            bot: false,
            primary_guild: None,
            sr_patron: false,
        }
    }

    /// The reference for `id`, with name fields when the lookup knows the user.
    pub fn of(id: UserId, users: &HashMap<UserId, UserSummary>) -> Self {
        match users.get(&id) {
            Some(u) => Self {
                id: id.to_string(),
                username: Some(u.username.clone()),
                global_name: u.display_name.clone(),
                bot: u.bot,
                primary_guild: u
                    .primary_guild
                    .as_ref()
                    .map(crate::routes::social::model::PrimaryGuildObject::of),
                sr_patron: u.patron,
            },
            None => Self::new(id),
        }
    }
}
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RoleObject {
    pub id: String,
    pub name: String,
    pub color: u32,
    pub hoist: bool,
    pub position: i32,
    /// Decimal permission bitfield.
    pub permissions: String,
    pub managed: bool,
    pub mentionable: bool,
    pub icon: Option<String>,
    pub unicode_emoji: Option<String>,
    pub flags: u32,
    /// Gradient stops (J-08); `primary_color` mirrors `color`.
    pub colors: RoleColorsObject,
    /// The storage grants of the role (BYOS): `view_cinema`, `view_library`, `view_music`, `upload`,
    /// `manage`, `view_games` (wave 37). Present on the role list; edited through `PUT /guilds/{guild_id}/storage/grants`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sr_storage_grants: Option<Vec<String>>,
}

/// The gradient stops of a role, Discord's `colors` object.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RoleColorsObject {
    pub primary_color: u32,
    pub secondary_color: Option<u32>,
    pub tertiary_color: Option<u32>,
}

/// `colors` in a role create or edit; replaces all three stops. A tertiary stop needs a
/// secondary one.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct RoleColorsBody {
    pub primary_color: u32,
    #[serde(default)]
    pub secondary_color: Option<u32>,
    #[serde(default)]
    pub tertiary_color: Option<u32>,
}

impl From<RoleColorsBody> for sr_chat::role_rules::RoleColors {
    fn from(b: RoleColorsBody) -> Self {
        Self {
            primary: b.primary_color,
            secondary: b.secondary_color,
            tertiary: b.tertiary_color,
        }
    }
}

impl From<&Role> for RoleObject {
    fn from(r: &Role) -> Self {
        Self {
            id: r.id.to_string(),
            name: r.name.clone(),
            color: r.color,
            hoist: r.hoist,
            position: r.position,
            permissions: r.permissions.bits().to_string(),
            managed: r.managed,
            mentionable: r.mentionable,
            icon: r.icon_key.clone(),
            unicode_emoji: r.unicode_emoji.clone(),
            flags: 0,
            colors: RoleColorsObject {
                primary_color: r.color,
                secondary_color: r.secondary_color,
                tertiary_color: r.tertiary_color,
            },
            sr_storage_grants: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct OverwriteObject {
    pub id: String,
    /// 0 = role, 1 = member.
    #[serde(rename = "type")]
    pub kind: u8,
    pub allow: String,
    pub deny: String,
}

impl From<&Overwrite> for OverwriteObject {
    fn from(o: &Overwrite) -> Self {
        Self {
            id: o.id.to_string(),
            kind: match o.kind {
                OverwriteKind::Role => 0,
                OverwriteKind::Member => 1,
            },
            allow: o.allow.bits().to_string(),
            deny: o.deny.bits().to_string(),
        }
    }
}

/// Thread-only fields of a channel object (`thread_metadata`).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ThreadMetadataObject {
    pub archived: bool,
    pub auto_archive_duration: u32,
    pub archive_timestamp: String,
    pub locked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invitable: Option<bool>,
    pub create_timestamp: String,
}

/// A thread member (`thread-members` endpoints and `member` of a thread object).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ThreadMemberObject {
    /// The thread id.
    pub id: String,
    pub user_id: String,
    pub join_timestamp: String,
    pub flags: u32,
}

impl From<&ThreadMember> for ThreadMemberObject {
    fn from(m: &ThreadMember) -> Self {
        Self {
            id: m.thread_id.to_string(),
            user_id: m.user_id.to_string(),
            join_timestamp: iso8601(m.joined_at_ms),
            flags: m.flags,
        }
    }
}

/// The bridge state of a channel: which way its messages flow.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BridgeChannelState {
    /// `both`, `to_discord` or `from_discord`.
    pub direction: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ChannelObject {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: i16,
    pub guild_id: Option<String>,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub position: i32,
    pub parent_id: Option<String>,
    pub nsfw: bool,
    pub rate_limit_per_user: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_limit: Option<u32>,
    pub last_message_id: Option<String>,
    pub permission_overwrites: Vec<OverwriteObject>,
    pub flags: u32,
    /// Creator of a thread.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_metadata: Option<ThreadMetadataObject>,
    /// Members of a thread.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_count: Option<u32>,
    /// Messages in a thread, capped at 50.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_count: Option<u32>,
    /// Every message ever sent in a thread.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_message_sent: Option<u32>,
    /// Forum tag ids applied to a post.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied_tags: Option<Vec<String>>,
    /// The requesting user's membership of a thread.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<ThreadMemberObject>,
    /// Forum and media channels: the tags a post can carry.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Vec<Object>>)]
    pub available_tags: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>)]
    pub default_reaction_emoji: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_sort_order: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_forum_layout: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_thread_rate_limit_per_user: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_auto_archive_duration: Option<u32>,
    /// Voice and stage channels: 1 auto (the adaptive default), 2 full (camera tracks may use the
    /// 720p tier whatever the occupancy).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_quality_mode: Option<u8>,
    /// Voice and stage channels: always `null` (this installation has one voice region).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub rtc_region: Option<Option<String>>,
    /// Voice channels: true for a Watch party room (D-208), a channel with a standing watch party.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sr_watch_party_room: Option<bool>,
    /// Voice and stage channels: members with Speak may play music into the call (MUS-JB-001).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sr_music_allowed: Option<bool>,
    /// Voice and stage channels: the music track's Opus bitrate in kbit/s (32 to 320, default 256).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sr_music_bitrate_kbps: Option<u16>,
    /// Present when this channel is mirrored to Discord by the bridge (BRG-COR-001): the client draws the
    /// banner from it. Absent when the channel is not mapped, the module is off or the operator switched the
    /// bridge off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sr_bridge: Option<BridgeChannelState>,
}

impl ChannelObject {
    pub fn new(c: &Channel, overwrites: &[Overwrite]) -> Self {
        let t = c.thread.as_ref();
        let forum = c.kind.is_forum_like();
        Self {
            id: c.id.to_string(),
            kind: c.kind.code(),
            guild_id: c.guild_id.map(|g| g.to_string()),
            name: c.name.clone(),
            topic: c.topic.clone(),
            position: c.position,
            parent_id: c.parent_id.map(|p| p.to_string()),
            nsfw: c.nsfw,
            rate_limit_per_user: c.rate_limit_per_user,
            bitrate: c.bitrate,
            user_limit: c.user_limit,
            last_message_id: c.last_message_id.map(|m| m.to_string()),
            permission_overwrites: overwrites.iter().map(OverwriteObject::from).collect(),
            flags: c.flags,
            owner_id: t.and(c.owner_id).map(|o| o.to_string()),
            thread_metadata: t.map(|t| ThreadMetadataObject {
                archived: t.archived,
                auto_archive_duration: t.auto_archive_duration,
                archive_timestamp: iso8601(t.archive_timestamp_ms),
                locked: t.locked,
                invitable: (c.kind == ChannelType::PrivateThread).then_some(t.invitable),
                create_timestamp: iso8601(t.create_timestamp_ms),
            }),
            member_count: t.map(|t| t.member_count),
            message_count: t.map(ThreadInfo::reported_message_count),
            total_message_sent: t.map(|t| t.total_message_sent),
            applied_tags: t.filter(|_| c.parent_id.is_some()).and_then(|t| {
                (!t.applied_tags.is_empty())
                    .then(|| t.applied_tags.iter().map(u64::to_string).collect())
            }),
            member: None,
            available_tags: forum.then(|| {
                c.forum
                    .available_tags
                    .iter()
                    .filter_map(|t| serde_json::to_value(t).ok())
                    .collect()
            }),
            default_reaction_emoji: c
                .forum
                .default_reaction_emoji
                .as_ref()
                .filter(|_| forum)
                .and_then(|r| serde_json::to_value(r).ok()),
            default_sort_order: c.forum.default_sort_order.filter(|_| forum),
            default_forum_layout: c.forum.default_forum_layout.filter(|_| forum),
            default_thread_rate_limit_per_user: c.forum.default_thread_rate_limit,
            default_auto_archive_duration: c.forum.default_auto_archive_duration,
            video_quality_mode: c.kind.is_voice_like().then_some(c.video_quality_mode),
            rtc_region: c.kind.is_voice_like().then_some(None),
            sr_watch_party_room: (c.kind == ChannelType::Voice).then_some(c.watch_party_room),
            sr_music_allowed: c.kind.is_voice_like().then_some(c.music_allowed),
            sr_music_bitrate_kbps: c.kind.is_voice_like().then_some(c.music_bitrate_kbps),
            sr_bridge: None,
        }
    }

    /// Attach the requesting user's thread membership.
    #[must_use]
    pub fn with_member(mut self, m: Option<&ThreadMember>) -> Self {
        self.member = m.map(ThreadMemberObject::from);
        self
    }
}

impl From<&ChannelWithOverwrites> for ChannelObject {
    fn from(c: &ChannelWithOverwrites) -> Self {
        Self::new(&c.channel, &c.overwrites)
    }
}

/// One server trait (J-07): a label with an optional emoji.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GuildTraitObject {
    /// Custom emoji of the server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji_id: Option<String>,
    /// Unicode emoji, or the custom emoji's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji_name: Option<String>,
    /// 1 to 24 characters, no mentions or markdown.
    pub label: String,
}

impl From<&GuildTrait> for GuildTraitObject {
    fn from(t: &GuildTrait) -> Self {
        Self {
            emoji_id: t.emoji_id.clone(),
            emoji_name: t.emoji_name.clone(),
            label: t.label.clone(),
        }
    }
}

impl From<GuildTraitObject> for GuildTrait {
    fn from(t: GuildTraitObject) -> Self {
        Self {
            emoji_id: t.emoji_id,
            emoji_name: t.emoji_name,
            label: t.label,
        }
    }
}

/// Side Room's server profile extras on the server object (`sr_profile`).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GuildSrProfileObject {
    /// At most five.
    pub traits: Vec<GuildTraitObject>,
}

/// Body of `PATCH /guilds/{id}/sr/profile`.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ModifySrProfileBody {
    /// Replaces all traits; at most five.
    pub traits: Option<Vec<GuildTraitObject>>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GuildObject {
    pub id: String,
    pub name: String,
    /// Icon hash: `/cdn/icons/{id}/{hash}.png` (`a_` prefix = animated).
    pub icon: Option<String>,
    /// Banner hash, same URL shape as the icon.
    pub banner: Option<String>,
    /// Invite splash hash, same URL shape as the icon.
    pub splash: Option<String>,
    pub description: Option<String>,
    pub owner_id: String,
    pub system_channel_id: Option<String>,
    /// Voice channel idle members are moved to.
    pub afk_channel_id: Option<String>,
    /// Seconds of voice inactivity before that move: 60, 300, 900, 1800 or 3600.
    pub afk_timeout: u32,
    pub verification_level: u8,
    pub default_message_notifications: u8,
    pub explicit_content_filter: u8,
    /// 0 none, 1 elevated: moderator actions need a recent second factor
    /// (`POST /guilds/{id}/mfa`).
    pub mfa_level: u8,
    /// Discord `system_channel_flags` bits (1 suppress join messages, 2 boost messages,
    /// 4 setup tips, 8 sticker reply buttons, 16 role subscription messages, 32 their replies).
    pub system_channel_flags: u32,
    pub widget_enabled: bool,
    pub widget_channel_id: Option<String>,
    pub preferred_locale: String,
    pub features: Vec<String>,
    /// Present (true) on the platform's Staff Server only (ADM-STF-010); clients frame it
    /// in the rail and show the staff badge.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_staff_guild: bool,
    /// Vanity invite code (LNG-INV-004).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vanity_url_code: Option<String>,
    /// Community channels (Discord shape, null when unset; G-76).
    pub rules_channel_id: Option<String>,
    pub public_updates_channel_id: Option<String>,
    pub safety_alerts_channel_id: Option<String>,
    /// Server Protection (D-224): a link whose text names another host is shown as its raw
    /// target with a warning. Visible to every member; set in `/sr/community`.
    pub hide_suspicious_links: bool,
    /// Incident actions (D-243): the timed invite and DM pauses and the last join-rate alert.
    /// Fields are `null` when unset; a time in the past means the pause ended.
    pub incidents_data: IncidentsDataObject,
    /// Only with `?with_counts=true` on `GET /guilds/{id}` (Discord shape).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_member_count: Option<u64>,
    /// Members visibly online; only with `with_counts`, and only for servers of up to 1000 members.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_presence_count: Option<u64>,
    /// The largest file the caller may upload here, in bytes (D-233). Only on `GET /guilds/{id}`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sr_max_upload_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<RoleObject>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<Vec<ChannelObject>>,
    /// Side Room's profile extras (server traits).
    pub sr_profile: GuildSrProfileObject,
}

impl GuildObject {
    pub fn new(g: &Guild, roles: Option<&[Role]>, channels: Option<&[ChannelObject]>) -> Self {
        Self {
            id: g.id.to_string(),
            name: g.name.clone(),
            icon: g.icon_key.clone(),
            banner: g.banner_key.clone(),
            splash: g.splash_key.clone(),
            description: g.description.clone(),
            owner_id: g.owner_id.to_string(),
            system_channel_id: g.system_channel_id.map(|c| c.to_string()),
            afk_channel_id: g.afk_channel_id.map(|c| c.to_string()),
            afk_timeout: g.afk_timeout,
            verification_level: g.verification_level,
            default_message_notifications: g.default_message_notifications,
            explicit_content_filter: g.explicit_content_filter,
            mfa_level: g.mfa_level,
            system_channel_flags: g.system_channel_flags,
            widget_enabled: g.widget_enabled,
            widget_channel_id: g.widget_channel_id.map(|c| c.to_string()),
            preferred_locale: g.preferred_locale.clone(),
            features: Vec::new(),
            is_staff_guild: g.is_staff_guild,
            vanity_url_code: g.vanity_code.clone(),
            rules_channel_id: g.rules_channel_id.map(|c| c.to_string()),
            public_updates_channel_id: g.public_updates_channel_id.map(|c| c.to_string()),
            safety_alerts_channel_id: g.safety_alerts_channel_id.map(|c| c.to_string()),
            hide_suspicious_links: g.hide_suspicious_links,
            incidents_data: IncidentsDataObject::of(&g.incidents),
            approximate_member_count: None,
            approximate_presence_count: None,
            sr_max_upload_bytes: None,
            roles: roles.map(|r| r.iter().map(RoleObject::from).collect()),
            channels: channels.map(<[ChannelObject]>::to_vec),
            sr_profile: GuildSrProfileObject {
                traits: g.traits.iter().map(GuildTraitObject::from).collect(),
            },
        }
    }
}

/// `incidents_data` of the guild object (D-243).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct IncidentsDataObject {
    /// Invites cannot be used until this time (ISO8601), or `null`.
    pub invites_disabled_until: Option<String>,
    /// Direct messages between members who are not friends are refused until this time, or `null`.
    pub dms_disabled_until: Option<String>,
    /// When the join-rate alert last fired, or `null`.
    pub raid_detected_at: Option<String>,
}

impl IncidentsDataObject {
    pub fn of(i: &sr_chat::domain::IncidentsData) -> Self {
        Self {
            invites_disabled_until: i.invites_disabled_until_ms.map(iso8601),
            dms_disabled_until: i.dms_disabled_until_ms.map(iso8601),
            raid_detected_at: i.raid_detected_at_ms.map(iso8601),
        }
    }

    /// From the saved community settings (what a write just stored).
    pub fn of_settings(s: &sr_chat::settings::community::CommunitySettings) -> Self {
        Self::of(&sr_chat::domain::IncidentsData {
            invites_disabled_until_ms: s.invites_disabled_until_ms,
            dms_disabled_until_ms: s.dms_disabled_until_ms,
            raid_detected_at_ms: s.raid_detected_at_ms,
        })
    }
}

/// Entry of `GET /users/@me/guilds`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GuildPreviewObject {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub owner: bool,
    pub permissions: String,
    pub features: Vec<String>,
    /// Present (true) on the platform's Staff Server only (ADM-STF-010); clients frame it
    /// in the rail and show the staff badge.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_staff_guild: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MemberObject {
    pub user: UserRef,
    pub nick: Option<String>,
    pub avatar: Option<String>,
    /// Per-server banner hash (LNG-GLD-012).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
    /// Per-server bio (LNG-GLD-012).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
    /// Per-server pronouns (G-52).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pronouns: Option<String>,
    /// Per-server profile theme, `[primary, accent]` as 0xRRGGBB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme_colors: Option<[u32; 2]>,
    /// Per-server profile effect slug.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_effect: Option<String>,
    /// Per-server avatar decoration slug.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_decoration: Option<String>,
    pub roles: Vec<String>,
    pub joined_at: String,
    pub premium_since: Option<String>,
    pub deaf: bool,
    pub mute: bool,
    pub flags: u32,
    pub pending: bool,
    pub communication_disabled_until: Option<String>,
    /// Only present on gateway payloads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<String>,
    /// How the member joined; only for callers with `MANAGE_GUILD` (D-240).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sr_join: Option<MemberJoinObject>,
    /// Facts about the member: `new_account`, `rejoined`; only for callers with `MANAGE_GUILD`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sr_signals: Option<Vec<String>>,
}

/// `sr_join` of a member object (D-240).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MemberJoinObject {
    /// `invite`, `vanity`, `directory`, `application`, `creator`, `staff`, `bot` or `unknown`.
    pub source: String,
    /// The invite or vanity code used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invite_code: Option<String>,
    /// Who made the invite (invite joins only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inviter_id: Option<String>,
}

impl MemberObject {
    /// Add `sr_join` and `sr_signals` for a viewer who may see them. `warn_days` is the server's
    /// `warn_new_account_days`; `now_ms` the clock.
    pub fn with_join(
        mut self,
        user: UserId,
        join: &sr_chat::admin_repo::MemberJoin,
        warn_days: u32,
        now_ms: i64,
    ) -> Self {
        let source = join.source.clone().unwrap_or_else(|| "unknown".to_owned());
        self.sr_join = Some(MemberJoinObject {
            inviter_id: (source == "invite")
                .then(|| join.ref_id.map(|r| r.to_string()))
                .flatten(),
            source,
            invite_code: join.invite_code.clone(),
        });
        self.sr_signals = Some(signals_of(user, join.rejoined, warn_days, now_ms));
        self
    }
}

/// The two member signals (D-240): the account is younger than the server's threshold, and an
/// earlier removal of this person from this server is in the audit log.
pub fn signals_of(user: UserId, rejoined: bool, warn_days: u32, now_ms: i64) -> Vec<String> {
    let mut out = Vec::new();
    let age_ms = now_ms - sr_core::Snowflake::new(user.get()).unix_ms();
    if age_ms < i64::from(warn_days) * 86_400_000 {
        out.push("new_account".to_owned());
    }
    if rejoined {
        out.push("rejoined".to_owned());
    }
    out
}

impl MemberObject {
    pub fn new(m: &Member, with_guild_id: bool, users: &HashMap<UserId, UserSummary>) -> Self {
        Self {
            user: UserRef::of(m.user_id, users),
            nick: m.nick.clone(),
            avatar: m.avatar_key.clone(),
            banner: m.banner_key.clone(),
            bio: m.bio.clone(),
            pronouns: m.extras.pronouns.clone(),
            theme_colors: m.extras.theme_colors,
            profile_effect: m.extras.profile_effect.clone(),
            avatar_decoration: m.extras.avatar_decoration.clone(),
            roles: m.role_ids.iter().map(ToString::to_string).collect(),
            joined_at: iso8601(m.joined_at_ms),
            premium_since: None,
            deaf: false,
            mute: false,
            flags: m.extras.flags,
            pending: m.pending,
            communication_disabled_until: m.communication_disabled_until_ms.map(iso8601),
            guild_id: with_guild_id.then(|| m.guild_id.to_string()),
            sr_join: None,
            sr_signals: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BanObject {
    pub reason: Option<String>,
    pub user: UserRef,
}

impl BanObject {
    pub fn new(b: &Ban, users: &HashMap<UserId, UserSummary>) -> Self {
        Self {
            reason: b.reason.clone(),
            user: UserRef::of(b.user_id, users),
        }
    }
}

// ---------------------------------------------------------------- requests

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateGuildBody {
    pub name: String,
    /// Image data URI for the server icon (the `PATCH` rules: PNG, JPEG, GIF or WebP, cropped
    /// to a square); a refused image fails the whole request and no server is left behind.
    #[serde(default)]
    pub icon: Option<String>,
    /// A built-in template id (see `GET /guilds/templates/builtin`) or a template code.
    /// The server is created from it in one step.
    #[serde(default)]
    pub template: Option<String>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ModifyGuildBody {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub description: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub system_channel_id: Option<Option<String>>,
    /// A voice channel of the server; `null` clears it.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub afk_channel_id: Option<Option<String>>,
    /// 60, 300, 900, 1800 or 3600 seconds.
    pub afk_timeout: Option<u32>,
    pub verification_level: Option<u8>,
    pub default_message_notifications: Option<u8>,
    pub explicit_content_filter: Option<u8>,
    /// Discord `system_channel_flags` bits (see the server object).
    pub system_channel_flags: Option<u32>,
    pub preferred_locale: Option<String>,
    /// Transfers ownership (owner only).
    pub owner_id: Option<String>,
    /// Image data URI (PNG, JPEG, GIF or WebP, 8 MiB at most; cropped to a square);
    /// `null` removes the icon.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub icon: Option<Option<String>>,
    /// Banner image data URI (aspect ratio kept, at most 1920x1080); `null` removes it.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub banner: Option<Option<String>>,
    /// Invite splash image data URI; `null` removes it.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub splash: Option<Option<String>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct OverwriteBody {
    /// Required when creating a channel, ignored on `PUT`.
    pub id: Option<String>,
    /// 0 = role, 1 = member.
    #[serde(rename = "type")]
    pub kind: u8,
    #[serde(default)]
    pub allow: Option<String>,
    #[serde(default)]
    pub deny: Option<String>,
}

/// A forum tag as sent by clients; the server assigns an id when absent.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct TagBody {
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub moderated: bool,
    pub emoji_id: Option<String>,
    pub emoji_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct DefaultReactionBody {
    pub emoji_id: Option<String>,
    pub emoji_name: Option<String>,
}

/// Forum and media settings, plus the thread defaults of text channels.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ForumSettingsBody {
    pub available_tags: Option<Vec<TagBody>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<DefaultReactionBody>)]
    pub default_reaction_emoji: Option<Option<DefaultReactionBody>>,
    /// 0 latest activity, 1 creation date; `null` clears.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<u8>)]
    pub default_sort_order: Option<Option<u8>>,
    /// 0 not set, 1 list, 2 gallery.
    pub default_forum_layout: Option<u8>,
    /// Slowmode (seconds) new posts start with.
    pub default_thread_rate_limit_per_user: Option<u32>,
    /// Minutes of inactivity before new threads auto-archive.
    pub default_auto_archive_duration: Option<u32>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateChannelBody {
    pub name: String,
    /// Discord channel type; defaults to 0 (text). 4 creates a category.
    #[serde(rename = "type", default)]
    pub kind: Option<i16>,
    pub topic: Option<String>,
    pub position: Option<i32>,
    pub parent_id: Option<String>,
    pub nsfw: Option<bool>,
    pub rate_limit_per_user: Option<u32>,
    pub bitrate: Option<u32>,
    pub user_limit: Option<u32>,
    /// Voice and stage channels: 1 auto (default), 2 full.
    pub video_quality_mode: Option<u8>,
    /// Only `null` is accepted: this installation has one voice region.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub rtc_region: Option<Option<String>>,
    #[serde(default)]
    pub permission_overwrites: Vec<OverwriteBody>,
    /// Forum and media channels: `REQUIRE_TAG` (1 << 4) and similar settings bits.
    pub flags: Option<u32>,
    #[serde(flatten)]
    pub forum: ForumSettingsBody,
    /// SIDE ROOM "Watch party room" preset (D-208): with a voice channel (`type` 2), also start a persistent
    /// watch party bound to it. `sr_watch_party_title_id` is optional (D-227): without it the room starts
    /// empty (`media: null`); the party survives being empty and the first person to join becomes host and
    /// picks the media.
    #[serde(default)]
    pub sr_watch_party_room: Option<bool>,
    /// The title the room starts with (a Cinema title snowflake).
    pub sr_watch_party_title_id: Option<String>,
    /// An episode of that title, for a series.
    pub sr_watch_party_episode_id: Option<String>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ModifyChannelBody {
    pub name: Option<String>,
    /// Convert a text channel (0) to an announcement channel (5) or back; no other change of
    /// type is allowed.
    #[serde(rename = "type", default)]
    pub kind: Option<i16>,
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub topic: Option<Option<String>>,
    pub nsfw: Option<bool>,
    pub rate_limit_per_user: Option<u32>,
    pub bitrate: Option<u32>,
    pub user_limit: Option<u32>,
    /// Voice and stage channels (`MANAGE_CHANNELS`): 1 auto, 2 full.
    pub video_quality_mode: Option<u8>,
    /// Voice and stage channels: allow members with Speak to play music into the call (MUS-JB-001).
    pub sr_music_allowed: Option<bool>,
    /// Voice and stage channels: the music track's bitrate in kbit/s, 32 to 320.
    pub sr_music_bitrate_kbps: Option<u16>,
    /// Only `null` is accepted: this installation has one voice region.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub rtc_region: Option<Option<String>>,
    pub position: Option<i32>,
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub parent_id: Option<Option<String>>,
    /// Copy the category's overwrites onto this channel.
    #[serde(default)]
    pub lock_permissions: bool,
    /// Forum channels: setting bits; threads: `PINNED` (1 << 1) for forum posts.
    pub flags: Option<u32>,
    #[serde(flatten)]
    pub forum: ForumSettingsBody,
    // Thread fields.
    pub archived: Option<bool>,
    pub auto_archive_duration: Option<u32>,
    pub locked: Option<bool>,
    pub invitable: Option<bool>,
    pub applied_tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ReorderChannelEntry {
    pub id: String,
    pub position: Option<i32>,
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub parent_id: Option<Option<String>>,
    #[serde(default)]
    pub lock_permissions: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateRoleBody {
    pub name: Option<String>,
    /// Decimal permission bitfield.
    pub permissions: Option<String>,
    pub color: Option<u32>,
    /// Gradient stops; wins over `color` when both are sent.
    pub colors: Option<RoleColorsBody>,
    pub hoist: Option<bool>,
    pub mentionable: Option<bool>,
    pub unicode_emoji: Option<String>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ModifyRoleBody {
    pub name: Option<String>,
    pub permissions: Option<String>,
    pub color: Option<u32>,
    /// Gradient stops; replaces all three and wins over `color` when both are sent.
    pub colors: Option<RoleColorsBody>,
    pub hoist: Option<bool>,
    pub mentionable: Option<bool>,
    /// A single emoji shown beside the role; `null` clears it. Replaces an image icon.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub unicode_emoji: Option<Option<String>>,
    /// Image data URI for the role icon (PNG, JPEG, GIF or WebP, cropped to a square);
    /// `null` removes it. Replaces the emoji. Served at `/cdn/icons/{guild_id}/{hash}.png`.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub icon: Option<Option<String>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ReorderRoleEntry {
    pub id: String,
    pub position: i32,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ModifyMemberBody {
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub nick: Option<Option<String>>,
    pub roles: Option<Vec<String>>,
    /// ISO8601 end of the timeout, or `null` to clear it.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub communication_disabled_until: Option<Option<String>>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct BanBody {
    /// Seconds of message history to delete, 0 to 604800.
    pub delete_message_seconds: Option<u32>,
    /// Deprecated Discord field, 0 to 7.
    pub delete_message_days: Option<u32>,
    /// Convenience; Discord carries the reason in `X-Audit-Log-Reason`.
    pub reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_round_trip() {
        let ms = 1_800_000_123_456; // 08:02:03.456
        let s = iso8601(ms);
        assert_eq!(s, "2027-01-15T08:02:03.456+00:00");
        assert_eq!(parse_iso8601(&s), Some(ms));
        assert_eq!(iso8601(0), "1970-01-01T00:00:00.000+00:00");
        assert_eq!(
            parse_iso8601("2024-02-29T12:30:00Z"),
            Some(1_709_209_800_000)
        );
        assert_eq!(
            parse_iso8601("2024-02-29T14:30:00.5+02:00"),
            Some(1_709_209_800_500)
        );
        assert_eq!(parse_iso8601("2024-13-01T00:00:00Z"), None);
        assert_eq!(parse_iso8601("garbage"), None);
        assert_eq!(parse_iso8601("2024-02-29T12:30:00"), None);
    }

    #[test]
    fn nullable_distinguishes_absent_and_null() {
        let a: ModifyChannelBody = serde_json::from_str("{}").unwrap();
        assert!(a.topic.is_none());
        let b: ModifyChannelBody = serde_json::from_str(r#"{"topic":null}"#).unwrap();
        assert_eq!(b.topic, Some(None));
        let c: ModifyChannelBody = serde_json::from_str(r#"{"topic":"hi"}"#).unwrap();
        assert_eq!(c.topic, Some(Some("hi".into())));
    }
}
