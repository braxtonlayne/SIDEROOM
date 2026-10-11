//! User profiles: bio, pronouns, banner, accent colour, connections placeholder
//! (LNG-PRF-001/002). Avatar upload and decorations belong to other modules.

use super::{SocialError, SocialLimits, SocialResult, clean_text};
use sr_core::UserId;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Profile {
    pub user: UserId,
    pub avatar_key: Option<String>,
    pub banner_key: Option<String>,
    /// 0xRRGGBB.
    pub accent_color: Option<u32>,
    pub bio: Option<String>,
    pub pronouns: Option<String>,
    pub connections: Vec<Connection>,
}

/// A linked external account. Only OAuth-verified connections set `verified`
/// (docs/specs/lounge-chat.md section 4); the rest are plain "link" entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connection {
    pub user: UserId,
    /// Lowercase provider slug, e.g. `github`.
    pub provider: String,
    pub external_id: String,
    pub display_name: String,
    pub verified: bool,
    pub visible: bool,
    pub created_ms: i64,
}

/// Partial profile update. Outer `None` = leave alone, inner `None` = clear.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileEdit {
    pub banner_key: Option<Option<String>>,
    pub accent_color: Option<Option<u32>>,
    pub bio: Option<Option<String>>,
    pub pronouns: Option<Option<String>>,
}

/// Apply an edit with validation, returning the new profile fields.
pub fn apply_edit(
    mut current: Profile,
    edit: &ProfileEdit,
    limits: &SocialLimits,
) -> SocialResult<Profile> {
    if let Some(bio) = &edit.bio {
        current.bio = clean_text("bio", bio.as_deref(), limits.max_bio_chars)?;
    }
    if let Some(p) = &edit.pronouns {
        current.pronouns = clean_text("pronouns", p.as_deref(), limits.max_pronouns_chars)?;
    }
    if let Some(color) = edit.accent_color {
        if color.is_some_and(|c| c > 0x00FF_FFFF) {
            return Err(SocialError::invalid(
                "accent_color",
                "NUMBER_TYPE_MAX",
                "must be a 24-bit RGB value",
            ));
        }
        current.accent_color = color;
    }
    if let Some(key) = &edit.banner_key {
        current.banner_key = clean_text("banner", key.as_deref(), 256)?;
    }
    Ok(current)
}

/// Validate a connection before upsert.
pub fn validate_connection(c: &Connection) -> SocialResult<()> {
    let slug_ok = !c.provider.is_empty()
        && c.provider.len() <= 32
        && c.provider
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if !slug_ok {
        return Err(SocialError::invalid(
            "provider",
            "BASE_TYPE_BAD_FORMAT",
            "provider must be a lowercase slug",
        ));
    }
    if c.external_id.trim().is_empty() || c.external_id.chars().count() > 128 {
        return Err(SocialError::invalid(
            "external_id",
            "BASE_TYPE_BAD_LENGTH",
            "must be 1-128 characters",
        ));
    }
    if c.display_name.trim().is_empty() || c.display_name.chars().count() > 100 {
        return Err(SocialError::invalid(
            "name",
            "BASE_TYPE_BAD_LENGTH",
            "must be 1-100 characters",
        ));
    }
    Ok(())
}

/// What a viewer may see: hidden connections are only shown to their owner.
pub fn visible_connections(profile: &Profile, viewer: UserId) -> Vec<Connection> {
    profile
        .connections
        .iter()
        .filter(|c| c.visible || viewer == profile.user)
        .cloned()
        .collect()
}
