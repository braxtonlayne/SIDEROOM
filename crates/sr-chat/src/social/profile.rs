//! User profiles: bio, pronouns, banner, accent colour, connections placeholder
//! (LNG-PRF-001/002). The avatar and banner are content hashes minted by the media
//! pipeline (`sr_storage::media`); decorations belong to other modules.

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
    /// Display name shown instead of the username (Discord `global_name`).
    pub display_name: Option<String>,
    /// Slug of an approved avatar decoration (LNG-PRF-009).
    pub avatar_decoration: Option<String>,
    /// Slug of an approved profile effect.
    pub profile_effect: Option<String>,
    /// Profile theme: two 0xRRGGBB colours (LNG-PRF-010), both or neither.
    pub theme_colors: Option<[u32; 2]>,
    /// Account creation time, unix ms ("Member since"); 0 when unknown.
    pub created_ms: i64,
    /// Public badges (`staff`, `owner`, `patron`, `early_member`, `app_developer`, `bot`).
    pub badges: Vec<String>,
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
    /// Public profile link of the account (shown as a link on the profile).
    pub url: Option<String>,
    /// Show the stats line / activity from this connection on the profile.
    pub show_activity: bool,
    /// Public stats captured at link time or by the refresh job, e.g. `{"followers": 12}`.
    pub stats: serde_json::Value,
}

/// Partial profile update. Outer `None` = leave alone, inner `None` = clear.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileEdit {
    pub avatar_key: Option<Option<String>>,
    pub banner_key: Option<Option<String>>,
    pub accent_color: Option<Option<u32>>,
    pub bio: Option<Option<String>>,
    pub pronouns: Option<Option<String>>,
    pub display_name: Option<Option<String>>,
    pub avatar_decoration: Option<Option<String>>,
    pub profile_effect: Option<Option<String>>,
    pub theme_colors: Option<Option<[u32; 2]>>,
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
    if let Some(name) = &edit.display_name {
        if name
            .as_deref()
            .is_some_and(|n| n.chars().any(char::is_control))
        {
            return Err(SocialError::invalid(
                "global_name",
                "BASE_TYPE_BAD_FORMAT",
                "must not contain control characters",
            ));
        }
        current.display_name = clean_text("global_name", name.as_deref(), 32)?;
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
    if let Some(slug) = &edit.avatar_decoration {
        current.avatar_decoration = clean_slug("avatar_decoration", slug.as_deref())?;
        if let Some(s) = &current.avatar_decoration
            && !crate::profile::catalog::is_decoration(s)
        {
            return Err(unknown_entry("avatar_decoration"));
        }
    }
    if let Some(slug) = &edit.profile_effect {
        current.profile_effect = clean_slug("profile_effect", slug.as_deref())?;
        if let Some(s) = &current.profile_effect
            && !crate::profile::catalog::is_effect(s)
        {
            return Err(unknown_entry("profile_effect"));
        }
    }
    if let Some(colors) = edit.theme_colors {
        if colors.is_some_and(|c| c.iter().any(|v| *v > 0x00FF_FFFF)) {
            return Err(SocialError::invalid(
                "theme_colors",
                "NUMBER_TYPE_MAX",
                "must be 24-bit RGB values",
            ));
        }
        current.theme_colors = colors;
    }
    if let Some(key) = &edit.avatar_key {
        current.avatar_key = clean_text("avatar", key.as_deref(), 256)?;
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

/// A decoration or effect slug: lowercase letters, digits and `-`, 1 to 48 characters.
fn unknown_entry(field: &str) -> SocialError {
    SocialError::invalid(
        field,
        "ENUM_TYPE_BASE_TYPE_BAD_VALUE",
        "not in the profile catalogue (GET /sr/profile-catalog)",
    )
}

fn clean_slug(field: &str, slug: Option<&str>) -> SocialResult<Option<String>> {
    let Some(s) = slug else { return Ok(None) };
    let ok = !s.is_empty()
        && s.len() <= 48
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if !ok {
        return Err(SocialError::invalid(
            field,
            "BASE_TYPE_BAD_FORMAT",
            "must be 1-48 lowercase letters, digits or dashes",
        ));
    }
    Ok(Some(s.to_owned()))
}
