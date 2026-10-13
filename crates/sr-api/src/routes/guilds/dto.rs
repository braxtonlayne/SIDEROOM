//! Discord-shaped request and response bodies for the guild endpoints.
//! Ids are decimal strings, permissions are decimal strings, timestamps ISO8601.

use serde::{Deserialize, Deserializer, Serialize};
use sr_chat::admin_repo::Ban;
use sr_chat::domain::{Channel, ChannelWithOverwrites, Guild, Member, Role};
use sr_chat::repo::UserSummary;
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
pub struct UserRef {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_name: Option<String>,
}

impl UserRef {
    /// The bare reference (id only).
    pub fn new(id: impl ToString) -> Self {
        Self {
            id: id.to_string(),
            username: None,
            global_name: None,
        }
    }

    /// The reference for `id`, with name fields when the lookup knows the user.
    pub fn of(id: UserId, users: &HashMap<UserId, UserSummary>) -> Self {
        match users.get(&id) {
            Some(u) => Self {
                id: id.to_string(),
                username: Some(u.username.clone()),
                global_name: u.display_name.clone(),
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
}

impl ChannelObject {
    pub fn new(c: &Channel, overwrites: &[Overwrite]) -> Self {
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
            flags: 0,
        }
    }
}

impl From<&ChannelWithOverwrites> for ChannelObject {
    fn from(c: &ChannelWithOverwrites) -> Self {
        Self::new(&c.channel, &c.overwrites)
    }
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
    pub verification_level: u8,
    pub default_message_notifications: u8,
    pub explicit_content_filter: u8,
    pub preferred_locale: String,
    pub features: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<RoleObject>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<Vec<ChannelObject>>,
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
            verification_level: g.verification_level,
            default_message_notifications: g.default_message_notifications,
            explicit_content_filter: g.explicit_content_filter,
            preferred_locale: g.preferred_locale.clone(),
            features: Vec::new(),
            roles: roles.map(|r| r.iter().map(RoleObject::from).collect()),
            channels: channels.map(<[ChannelObject]>::to_vec),
        }
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
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MemberObject {
    pub user: UserRef,
    pub nick: Option<String>,
    pub avatar: Option<String>,
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
}

impl MemberObject {
    pub fn new(m: &Member, with_guild_id: bool, users: &HashMap<UserId, UserSummary>) -> Self {
        Self {
            user: UserRef::of(m.user_id, users),
            nick: m.nick.clone(),
            avatar: None,
            roles: m.role_ids.iter().map(ToString::to_string).collect(),
            joined_at: iso8601(m.joined_at_ms),
            premium_since: None,
            deaf: false,
            mute: false,
            flags: 0,
            pending: m.pending,
            communication_disabled_until: m.communication_disabled_until_ms.map(iso8601),
            guild_id: with_guild_id.then(|| m.guild_id.to_string()),
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
    /// Accepted for compatibility; icon upload arrives with the media pipeline.
    #[serde(default)]
    pub icon: Option<String>,
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
    pub verification_level: Option<u8>,
    pub default_message_notifications: Option<u8>,
    pub explicit_content_filter: Option<u8>,
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
    #[serde(default)]
    pub permission_overwrites: Vec<OverwriteBody>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ModifyChannelBody {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub topic: Option<Option<String>>,
    pub nsfw: Option<bool>,
    pub rate_limit_per_user: Option<u32>,
    pub bitrate: Option<u32>,
    pub user_limit: Option<u32>,
    pub position: Option<i32>,
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub parent_id: Option<Option<String>>,
    /// Copy the category's overwrites onto this channel.
    #[serde(default)]
    pub lock_permissions: bool,
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
    pub hoist: Option<bool>,
    pub mentionable: Option<bool>,
    pub unicode_emoji: Option<String>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ModifyRoleBody {
    pub name: Option<String>,
    pub permissions: Option<String>,
    pub color: Option<u32>,
    pub hoist: Option<bool>,
    pub mentionable: Option<bool>,
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
