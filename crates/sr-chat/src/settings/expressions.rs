//! Custom emoji and stickers: metadata and limits (LNG-SSET-021..024).
//! Image bytes live in object storage; only the key is kept here.

use serde::{Deserialize, Serialize};
use sr_core::{EmojiId, GuildId, RoleId, StickerId, UserId};

use super::error::{SettingsError, SettingsResult};

/// Operator-configurable slot and size limits (spec section 14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpressionLimits {
    pub max_static_emoji: usize,
    pub max_animated_emoji: usize,
    pub max_stickers: usize,
    pub max_soundboard_sounds: usize,
    pub max_emoji_bytes: u64,
    pub max_sticker_bytes: u64,
    pub max_roles_per_emoji: usize,
}

impl Default for ExpressionLimits {
    fn default() -> Self {
        Self {
            max_static_emoji: 250,
            max_animated_emoji: 250,
            max_stickers: 60,
            max_soundboard_sounds: 48,
            max_emoji_bytes: 256 * 1024,
            max_sticker_bytes: 512 * 1024,
            max_roles_per_emoji: 50,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Emoji {
    pub id: EmojiId,
    pub guild_id: GuildId,
    pub name: String,
    pub animated: bool,
    /// Unavailable emoji (e.g. after a limit drop) cannot be used but are kept.
    pub available: bool,
    /// Empty = everyone may use it; otherwise only members with one of these roles.
    pub role_ids: Vec<RoleId>,
    pub creator_id: UserId,
    pub media_key: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StickerFormat {
    Png,
    Apng,
    Lottie,
    Gif,
}

impl StickerFormat {
    pub const fn code(self) -> i16 {
        match self {
            Self::Png => 1,
            Self::Apng => 2,
            Self::Lottie => 3,
            Self::Gif => 4,
        }
    }

    pub const fn from_code(code: i16) -> Option<Self> {
        Some(match code {
            1 => Self::Png,
            2 => Self::Apng,
            3 => Self::Lottie,
            4 => Self::Gif,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sticker {
    pub id: StickerId,
    pub guild_id: GuildId,
    pub name: String,
    pub description: Option<String>,
    /// Autocomplete tags; the first is the related emoji (spec: "related emoji").
    pub tags: String,
    pub format: StickerFormat,
    pub available: bool,
    pub creator_id: UserId,
    pub media_key: String,
    pub created_at_ms: i64,
}

/// Emoji names: 2-32 chars, ASCII letters, digits and underscore.
pub fn validate_emoji_name(name: &str) -> SettingsResult<()> {
    let ok = (2..=32).contains(&name.chars().count())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if ok {
        Ok(())
    } else {
        Err(SettingsError::invalid(
            "name",
            "STRING_TYPE_REGEX",
            "emoji names must be 2-32 characters of letters, digits and underscores",
        ))
    }
}

pub fn validate_sticker(name: &str, description: Option<&str>, tags: &str) -> SettingsResult<()> {
    if !(2..=30).contains(&name.trim().chars().count()) {
        return Err(SettingsError::invalid(
            "name",
            "BASE_TYPE_BAD_LENGTH",
            "must be between 2 and 30 in length",
        ));
    }
    if let Some(d) = description
        && !d.is_empty()
        && !(2..=100).contains(&d.trim().chars().count())
    {
        return Err(SettingsError::invalid(
            "description",
            "BASE_TYPE_BAD_LENGTH",
            "must be between 2 and 100 in length",
        ));
    }
    if tags.trim().is_empty() || tags.chars().count() > 200 {
        return Err(SettingsError::invalid(
            "tags",
            "BASE_TYPE_BAD_LENGTH",
            "a related emoji or tag is required (at most 200 characters)",
        ));
    }
    Ok(())
}

/// Check a new emoji against size and slot limits.
pub fn check_new_emoji(
    existing: &[Emoji],
    name: &str,
    animated: bool,
    size_bytes: u64,
    role_ids: &[RoleId],
    limits: &ExpressionLimits,
) -> SettingsResult<()> {
    validate_emoji_name(name)?;
    if size_bytes > limits.max_emoji_bytes {
        return Err(SettingsError::invalid(
            "image",
            "BINARY_TYPE_MAX_SIZE",
            format!(
                "file cannot be larger than {} KiB",
                limits.max_emoji_bytes / 1024
            ),
        ));
    }
    if role_ids.len() > limits.max_roles_per_emoji {
        return Err(SettingsError::LimitReached("roles per emoji"));
    }
    let (used, cap) = emoji_slots(existing, animated, limits);
    if used >= cap {
        return Err(SettingsError::LimitReached(if animated {
            "animated emoji"
        } else {
            "static emoji"
        }));
    }
    Ok(())
}

pub fn check_new_sticker(
    existing: &[Sticker],
    size_bytes: u64,
    limits: &ExpressionLimits,
) -> SettingsResult<()> {
    if size_bytes > limits.max_sticker_bytes {
        return Err(SettingsError::invalid(
            "file",
            "BINARY_TYPE_MAX_SIZE",
            format!(
                "file cannot be larger than {} KiB",
                limits.max_sticker_bytes / 1024
            ),
        ));
    }
    if existing.len() >= limits.max_stickers {
        return Err(SettingsError::LimitReached("stickers"));
    }
    Ok(())
}

/// `(used, capacity)` for the static or animated emoji pool (slot counters, LNG-SSET-021).
pub fn emoji_slots(
    existing: &[Emoji],
    animated: bool,
    limits: &ExpressionLimits,
) -> (usize, usize) {
    let used = existing.iter().filter(|e| e.animated == animated).count();
    let cap = if animated {
        limits.max_animated_emoji
    } else {
        limits.max_static_emoji
    };
    (used, cap)
}

/// Whether `member_roles` may use the emoji in a message.
pub fn can_use_emoji(emoji: &Emoji, member_roles: &[RoleId]) -> bool {
    emoji.available
        && (emoji.role_ids.is_empty() || emoji.role_ids.iter().any(|r| member_roles.contains(r)))
}

/// After a limit drop, mark everything past the cap unavailable (oldest kept).
pub fn availability_after_limit_change(
    emoji: &mut [Emoji],
    limits: &ExpressionLimits,
) -> Vec<EmojiId> {
    emoji.sort_by_key(|e| (e.created_at_ms, e.id));
    let (mut s, mut a) = (0, 0);
    let mut changed = Vec::new();
    for e in emoji.iter_mut() {
        let (n, cap) = if e.animated {
            (&mut a, limits.max_animated_emoji)
        } else {
            (&mut s, limits.max_static_emoji)
        };
        let available = *n < cap;
        *n += 1;
        if e.available != available {
            e.available = available;
            changed.push(e.id);
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emoji(id: u64, animated: bool, roles: &[u64], at: i64) -> Emoji {
        Emoji {
            id: EmojiId::new(id),
            guild_id: GuildId::new(1),
            name: format!("e{id}"),
            animated,
            available: true,
            role_ids: roles.iter().map(|r| RoleId::new(*r)).collect(),
            creator_id: UserId::new(9),
            media_key: format!("k{id}"),
            created_at_ms: at,
        }
    }

    #[test]
    fn name_rules() {
        assert!(validate_emoji_name("ok_name1").is_ok());
        for bad in ["a", "has space", "dash-ed", "ünï", &"x".repeat(33)] {
            assert!(validate_emoji_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn slot_and_size_limits() {
        let limits = ExpressionLimits {
            max_static_emoji: 2,
            max_animated_emoji: 1,
            ..Default::default()
        };
        let existing = vec![
            emoji(1, false, &[], 1),
            emoji(2, false, &[], 2),
            emoji(3, true, &[], 3),
        ];
        assert_eq!(emoji_slots(&existing, false, &limits), (2, 2));
        assert!(check_new_emoji(&existing, "new_one", false, 10, &[], &limits).is_err());
        assert!(check_new_emoji(&existing[..1], "new_one", false, 10, &[], &limits).is_ok());
        assert!(check_new_emoji(&existing, "anim", true, 10, &[], &limits).is_err());
        assert!(check_new_emoji(&[], "big", false, 300 * 1024, &[], &limits).is_err());
    }

    #[test]
    fn role_restriction() {
        let e = emoji(1, false, &[5], 1);
        assert!(can_use_emoji(&e, &[RoleId::new(5)]));
        assert!(!can_use_emoji(&e, &[RoleId::new(6)]));
        assert!(can_use_emoji(&emoji(2, false, &[], 1), &[]));
        let mut off = emoji(3, false, &[], 1);
        off.available = false;
        assert!(!can_use_emoji(&off, &[]));
    }

    #[test]
    fn limit_drop_disables_newest() {
        let mut all = vec![
            emoji(1, false, &[], 1),
            emoji(2, false, &[], 2),
            emoji(3, false, &[], 3),
        ];
        let limits = ExpressionLimits {
            max_static_emoji: 2,
            ..Default::default()
        };
        let changed = availability_after_limit_change(&mut all, &limits);
        assert_eq!(changed, vec![EmojiId::new(3)]);
        assert!(!all[2].available);
        // Raising the limit restores it.
        let restored = availability_after_limit_change(&mut all, &ExpressionLimits::default());
        assert_eq!(restored, vec![EmojiId::new(3)]);
    }

    #[test]
    fn sticker_rules() {
        assert!(validate_sticker("wave", Some("A friendly wave"), "wave").is_ok());
        assert!(validate_sticker("w", None, "wave").is_err());
        assert!(validate_sticker("wave", Some("x"), "wave").is_err());
        assert!(validate_sticker("wave", None, "").is_err());
        let limits = ExpressionLimits {
            max_stickers: 0,
            ..Default::default()
        };
        assert!(check_new_sticker(&[], 10, &limits).is_err());
        assert!(check_new_sticker(&[], 600 * 1024, &ExpressionLimits::default()).is_err());
        assert_eq!(
            StickerFormat::from_code(StickerFormat::Lottie.code()),
            Some(StickerFormat::Lottie)
        );
    }
}
