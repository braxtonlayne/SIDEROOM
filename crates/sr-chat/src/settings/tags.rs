//! Server tags (LNG-SSET-011/012): a short tag and badge a guild defines, which members
//! may adopt as their one displayed tag ("primary guild" in Discord's user object).

use serde::{Deserialize, Serialize};
use sr_core::GuildId;

use super::error::{SettingsError, SettingsResult};

/// The tag text is 2 to 4 characters (the inventory says at most 4; Discord needs 2).
pub const TAG_MIN_CHARS: usize = 2;
pub const TAG_MAX_CHARS: usize = 4;
/// Badge icons are picked from a fixed set of this many (ids `0..BADGE_COUNT`).
pub const BADGE_COUNT: u8 = 16;
pub const MAX_COLOR: u32 = 0x00FF_FFFF;

/// What a guild defines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuildTag {
    pub guild_id: GuildId,
    pub tag: String,
    pub badge: u8,
    pub color: u32,
    /// "Members can choose to display" (LNG-SSET-011). While false, nobody shows it.
    pub members_can_display: bool,
}

/// A user's adopted tag as other users see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrimaryGuild {
    pub guild_id: GuildId,
    pub tag: String,
    pub badge: u8,
    pub color: u32,
}

impl From<&GuildTag> for PrimaryGuild {
    fn from(t: &GuildTag) -> Self {
        Self {
            guild_id: t.guild_id,
            tag: t.tag.clone(),
            badge: t.badge,
            color: t.color,
        }
    }
}

/// Validate and normalise a tag (trimmed, letters and digits only).
pub fn validate_tag(mut t: GuildTag) -> SettingsResult<GuildTag> {
    let bad = |field: &str, msg: &str| SettingsError::invalid(field, "GUILD_TAG_INVALID", msg);
    t.tag = t.tag.trim().to_owned();
    let n = t.tag.chars().count();
    if !(TAG_MIN_CHARS..=TAG_MAX_CHARS).contains(&n) {
        return Err(bad("tag", "the tag is 2 to 4 characters"));
    }
    if !t.tag.chars().all(char::is_alphanumeric) {
        return Err(bad("tag", "the tag may only contain letters and digits"));
    }
    if t.badge >= BADGE_COUNT {
        return Err(bad("badge", "unknown badge"));
    }
    if t.color > MAX_COLOR {
        return Err(bad("color", "color is a 24-bit RGB value"));
    }
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(text: &str) -> GuildTag {
        GuildTag {
            guild_id: GuildId::new(1),
            tag: text.into(),
            badge: 3,
            color: 0x112233,
            members_can_display: true,
        }
    }

    #[test]
    fn tag_rules() {
        assert_eq!(validate_tag(tag(" Ab1 ")).unwrap().tag, "Ab1");
        assert!(validate_tag(tag("a")).is_err());
        assert!(validate_tag(tag("abcde")).is_err());
        assert!(validate_tag(tag("a b")).is_err());
        assert!(validate_tag(tag("a!")).is_err());
        let mut t = tag("ok");
        t.badge = BADGE_COUNT;
        assert!(validate_tag(t).is_err());
        let mut t = tag("ok");
        t.color = MAX_COLOR + 1;
        assert!(validate_tag(t).is_err());
    }
}
