//! Server tags (LNG-GLD-011): a short tag and a badge icon a server owns, shown next to the
//! name of every member who chose that server as their primary one.
//!
//! The tag belongs to the guild (`guilds.tag`, `guilds.tag_badge`, set by a member with
//! `MANAGE_GUILD`; unlike Discord it is free, there is no boost level to reach). Showing it is
//! the member's choice (`accounts.primary_guild_id` and `primary_guild_enabled`), and they must
//! still be a member of that server.

use crate::error::{ChatError, ChatResult};
use sr_core::GuildId;

/// Badge icons a tag may carry, as slugs the web client maps to glyphs.
pub const BADGES: &[&str] = &[
    "sword", "shield", "heart", "star", "flame", "leaf", "skull", "bolt", "crown", "gem", "moon",
    "sun", "paw", "music", "game", "book", "brush", "camera", "rocket", "flask", "coffee", "wave",
    "tree", "flag",
];

/// What other users see next to a member's name (Discord's `primary_guild`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrimaryGuild {
    pub guild_id: GuildId,
    pub tag: String,
    pub badge: Option<String>,
}

fn invalid(field: &str, code: &'static str, msg: &str) -> ChatError {
    ChatError::invalid(field, code, msg)
}

/// Normalise a tag: 2 to 4 ASCII letters or digits, kept as typed. `None` or an empty
/// string clears it.
pub fn validate_tag(tag: Option<&str>) -> ChatResult<Option<String>> {
    let Some(t) = tag.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    if !(2..=4).contains(&t.chars().count()) || !t.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(invalid(
            "tag",
            "BASE_TYPE_BAD_FORMAT",
            "Must be 2 to 4 letters or digits.",
        ));
    }
    Ok(Some(t.to_owned()))
}

/// A badge must come from [`BADGES`]; `None` or an empty string clears it.
pub fn validate_badge(badge: Option<&str>) -> ChatResult<Option<String>> {
    let Some(b) = badge.map(str::trim).filter(|b| !b.is_empty()) else {
        return Ok(None);
    };
    if !BADGES.contains(&b) {
        return Err(invalid(
            "tag_badge",
            "ENUM_TYPE_BASE_TYPE",
            "Not one of the available badges.",
        ));
    }
    Ok(Some(b.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags() {
        assert_eq!(validate_tag(None).unwrap(), None);
        assert_eq!(validate_tag(Some("  ")).unwrap(), None);
        assert_eq!(validate_tag(Some("SR")).unwrap().as_deref(), Some("SR"));
        assert_eq!(validate_tag(Some("a1b2")).unwrap().as_deref(), Some("a1b2"));
        for bad in ["a", "abcde", "a b", "é!", "ab-c"] {
            assert!(validate_tag(Some(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn badges() {
        assert_eq!(validate_badge(Some("")).unwrap(), None);
        assert_eq!(
            validate_badge(Some("crown")).unwrap().as_deref(),
            Some("crown")
        );
        assert!(validate_badge(Some("nope")).is_err());
        // The migration's slug check must accept every badge.
        assert!(BADGES.iter().all(|b| b.len() >= 2 && b.len() <= 32));
    }
}
