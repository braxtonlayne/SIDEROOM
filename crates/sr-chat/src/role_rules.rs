//! Pure role CRUD rules and hierarchy enforcement (LNG-GLD-007).

use crate::domain::Role;
use crate::error::{ChatError, ChatResult};
use crate::limits::ChatLimits;
use crate::perms::GuildActor;
use sr_core::permissions::Permissions;
use sr_core::{GuildId, RoleId};

pub const ROLE_NAME_MAX: usize = 100;
pub const MAX_COLOR: u32 = 0x00FF_FFFF;

/// The gradient stops of a role (`colors` in the API, J-08). `primary` mirrors `color`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleColors {
    pub primary: u32,
    pub secondary: Option<u32>,
    pub tertiary: Option<u32>,
}

/// Discord's fixed holographic triple (primary, secondary, tertiary).
pub const HOLOGRAPHIC: RoleColors = RoleColors {
    primary: 11_127_295,
    secondary: Some(16_759_788),
    tertiary: Some(16_761_760),
};

/// Check the stops: each fits 24 bits and a tertiary stop needs a secondary one.
pub fn validate_colors(c: RoleColors) -> ChatResult<RoleColors> {
    validate_color(c.primary)?;
    for (field, v) in [
        ("colors.secondary_color", c.secondary),
        ("colors.tertiary_color", c.tertiary),
    ] {
        if v.is_some_and(|v| v > MAX_COLOR) {
            return Err(ChatError::invalid(
                field,
                "NUMBER_TYPE_MAX",
                "Color must be at most 16777215.",
            ));
        }
    }
    if c.tertiary.is_some() && c.secondary.is_none() {
        return Err(ChatError::invalid(
            "colors.tertiary_color",
            "TERTIARY_NEEDS_SECONDARY",
            "A tertiary color needs a secondary color.",
        ));
    }
    Ok(c)
}

/// Fields for a new role; unset fields use Discord's defaults.
#[derive(Debug, Clone, Default)]
pub struct NewRole {
    pub name: Option<String>,
    pub permissions: Option<Permissions>,
    pub color: Option<u32>,
    /// Gradient stops; wins over `color` when both are given.
    pub colors: Option<RoleColors>,
    pub hoist: bool,
    pub mentionable: bool,
    pub unicode_emoji: Option<String>,
}

/// A partial update. `None` leaves a field unchanged.
#[derive(Debug, Clone, Default)]
pub struct RolePatch {
    pub name: Option<String>,
    pub permissions: Option<Permissions>,
    pub color: Option<u32>,
    /// Gradient stops; replaces all three and wins over `color` when both are given.
    pub colors: Option<RoleColors>,
    pub hoist: Option<bool>,
    pub mentionable: Option<bool>,
    /// `Some(None)` clears the emoji. Setting one clears the image icon.
    pub unicode_emoji: Option<Option<String>>,
    /// Hash of an icon the caller already stored; `Some(None)` clears it.
    /// Setting one clears the emoji.
    pub icon_key: Option<Option<String>>,
}

/// Longest role emoji, in characters (a ZWJ sequence is several).
pub const ROLE_EMOJI_MAX: usize = 16;

fn validate_emoji(raw: &str) -> ChatResult<String> {
    let e = raw.trim();
    let n = e.chars().count();
    if n == 0 || n > ROLE_EMOJI_MAX || e.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(ChatError::invalid(
            "unicode_emoji",
            "INVALID_EMOJI",
            "Must be a single emoji.",
        ));
    }
    Ok(e.to_owned())
}

fn validate_name(name: &str) -> ChatResult<String> {
    let name = name.trim();
    let n = name.chars().count();
    if n == 0 || n > ROLE_NAME_MAX {
        return Err(ChatError::invalid(
            "name",
            "BASE_TYPE_BAD_LENGTH",
            format!("Must be between 1 and {ROLE_NAME_MAX} in length."),
        ));
    }
    Ok(name.to_owned())
}

fn validate_color(color: u32) -> ChatResult<u32> {
    if color > MAX_COLOR {
        return Err(ChatError::invalid(
            "color",
            "NUMBER_TYPE_MAX",
            "Color must be at most 16777215.",
        ));
    }
    Ok(color)
}

fn require_manage_roles(actor: &GuildActor) -> ChatResult<()> {
    if actor.perms.has(Permissions::MANAGE_ROLES) {
        Ok(())
    } else {
        Err(ChatError::MissingPermissions)
    }
}

/// New roles land directly above `@everyone` (position 1); the repository shifts
/// existing roles at or above that position up by one.
pub fn plan_create_role(
    actor: &GuildActor,
    guild_id: GuildId,
    id: RoleId,
    existing_role_count: usize,
    input: NewRole,
    limits: &ChatLimits,
) -> ChatResult<Role> {
    require_manage_roles(actor)?;
    if existing_role_count >= limits.max_roles_per_guild {
        return Err(ChatError::MaxRoles);
    }
    let permissions = input.permissions.unwrap_or_default();
    if !actor.can_grant(permissions) {
        return Err(ChatError::MissingPermissions);
    }
    let colors = match input.colors {
        Some(c) => validate_colors(c)?,
        None => RoleColors {
            primary: validate_color(input.color.unwrap_or(0))?,
            secondary: None,
            tertiary: None,
        },
    };
    Ok(Role {
        id,
        guild_id,
        name: match input.name {
            Some(n) => validate_name(&n)?,
            None => "new role".to_owned(),
        },
        color: colors.primary,
        secondary_color: colors.secondary,
        tertiary_color: colors.tertiary,
        hoist: input.hoist,
        position: 1,
        permissions,
        mentionable: input.mentionable,
        managed: false,
        unicode_emoji: input.unicode_emoji,
        icon_key: None,
    })
}

/// Apply a patch to `role`. `@everyone` can only change permissions (and not name,
/// colour, hoist, mentionable); managed roles cannot change permissions.
pub fn plan_edit_role(actor: &GuildActor, role: &Role, patch: RolePatch) -> ChatResult<Role> {
    require_manage_roles(actor)?;
    if !actor.outranks_position(role.position) {
        return Err(ChatError::MissingPermissions);
    }
    let mut out = role.clone();
    if role.is_everyone()
        && (patch.name.is_some()
            || patch.color.is_some()
            || patch.colors.is_some()
            || patch.hoist.is_some()
            || patch.mentionable.is_some()
            || patch.unicode_emoji.is_some()
            || patch.icon_key.is_some())
    {
        return Err(ChatError::invalid(
            "role",
            "EVERYONE_IMMUTABLE",
            "Only the permissions of @everyone can be changed.",
        ));
    }
    if let Some(n) = patch.name {
        out.name = validate_name(&n)?;
    }
    if let Some(c) = patch.color {
        out.color = validate_color(c)?;
    }
    if let Some(c) = patch.colors {
        let c = validate_colors(c)?;
        out.color = c.primary;
        out.secondary_color = c.secondary;
        out.tertiary_color = c.tertiary;
    }
    if let Some(h) = patch.hoist {
        out.hoist = h;
    }
    if let Some(m) = patch.mentionable {
        out.mentionable = m;
    }
    if let Some(e) = patch.unicode_emoji {
        out.unicode_emoji = e.as_deref().map(validate_emoji).transpose()?;
        if out.unicode_emoji.is_some() {
            out.icon_key = None;
        }
    }
    if let Some(k) = patch.icon_key {
        out.icon_key = k;
        if out.icon_key.is_some() {
            out.unicode_emoji = None;
        }
    }
    if let Some(p) = patch.permissions {
        if role.managed {
            return Err(ChatError::invalid(
                "permissions",
                "MANAGED_ROLE",
                "Managed roles cannot be edited this way.",
            ));
        }
        // Changing bits requires holding every bit that changes.
        if !actor.can_grant((p | role.permissions) & !(p & role.permissions)) {
            return Err(ChatError::MissingPermissions);
        }
        out.permissions = p;
    }
    Ok(out)
}

/// `@everyone` and managed roles cannot be deleted.
pub fn check_delete_role(actor: &GuildActor, role: &Role) -> ChatResult<()> {
    require_manage_roles(actor)?;
    if role.is_everyone() || role.managed {
        return Err(ChatError::invalid(
            "role",
            "CANNOT_DELETE",
            "This role cannot be deleted.",
        ));
    }
    if !actor.outranks_position(role.position) {
        return Err(ChatError::MissingPermissions);
    }
    Ok(())
}

/// Move `role` to `new_position`: the actor must outrank both the old and the new
/// position, and `@everyone` stays at 0.
pub fn check_reposition(actor: &GuildActor, role: &Role, new_position: i32) -> ChatResult<()> {
    require_manage_roles(actor)?;
    if role.is_everyone() || new_position < 1 {
        return Err(ChatError::invalid(
            "position",
            "BAD_POSITION",
            "Position must be 1 or greater and @everyone cannot move.",
        ));
    }
    if !actor.outranks_position(role.position) || !actor.outranks_position(new_position) {
        return Err(ChatError::MissingPermissions);
    }
    Ok(())
}

/// Whether the actor may add or remove `role` on a member.
pub fn check_assign_role(actor: &GuildActor, role: &Role) -> ChatResult<()> {
    require_manage_roles(actor)?;
    if role.is_everyone() || role.managed || !actor.outranks_position(role.position) {
        return Err(ChatError::MissingPermissions);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perms::fixtures::*;

    fn actor(perms: Permissions, top: i32) -> GuildActor {
        GuildActor {
            is_owner: false,
            perms,
            top_position: top,
        }
    }

    fn owner() -> GuildActor {
        GuildActor {
            is_owner: true,
            perms: Permissions::ALL,
            top_position: 0,
        }
    }

    #[test]
    fn create_defaults_and_limits() {
        let l = ChatLimits::default();
        let a = actor(Permissions::MANAGE_ROLES, 3);
        let r = plan_create_role(
            &a,
            GuildId::new(GUILD),
            RoleId::new(9),
            1,
            NewRole::default(),
            &l,
        )
        .unwrap();
        assert_eq!(
            (r.name.as_str(), r.position, r.permissions),
            ("new role", 1, Permissions::empty())
        );
        assert!(matches!(
            plan_create_role(
                &a,
                GuildId::new(GUILD),
                RoleId::new(9),
                250,
                NewRole::default(),
                &l
            ),
            Err(ChatError::MaxRoles)
        ));
        assert!(matches!(
            plan_create_role(
                &actor(Permissions::empty(), 3),
                GuildId::new(GUILD),
                RoleId::new(9),
                1,
                NewRole::default(),
                &l
            ),
            Err(ChatError::MissingPermissions)
        ));
    }

    #[test]
    fn cannot_grant_permissions_you_lack() {
        let l = ChatLimits::default();
        let a = actor(Permissions::MANAGE_ROLES | Permissions::KICK_MEMBERS, 3);
        let ok = NewRole {
            permissions: Some(Permissions::KICK_MEMBERS),
            ..Default::default()
        };
        assert!(plan_create_role(&a, GuildId::new(GUILD), RoleId::new(9), 1, ok, &l).is_ok());
        let bad = NewRole {
            permissions: Some(Permissions::BAN_MEMBERS),
            ..Default::default()
        };
        assert!(plan_create_role(&a, GuildId::new(GUILD), RoleId::new(9), 1, bad, &l).is_err());
        let admin = NewRole {
            permissions: Some(Permissions::ADMINISTRATOR),
            ..Default::default()
        };
        assert!(
            plan_create_role(
                &a,
                GuildId::new(GUILD),
                RoleId::new(9),
                1,
                admin.clone(),
                &l
            )
            .is_err()
        );
        assert!(
            plan_create_role(&owner(), GuildId::new(GUILD), RoleId::new(9), 1, admin, &l).is_ok()
        );
    }

    #[test]
    fn name_and_color_validation() {
        let l = ChatLimits::default();
        let a = owner();
        for name in ["", "   ", &"n".repeat(101)] {
            let i = NewRole {
                name: Some(name.to_owned()),
                ..Default::default()
            };
            assert!(plan_create_role(&a, GuildId::new(GUILD), RoleId::new(9), 1, i, &l).is_err());
        }
        let i = NewRole {
            color: Some(MAX_COLOR + 1),
            ..Default::default()
        };
        assert!(plan_create_role(&a, GuildId::new(GUILD), RoleId::new(9), 1, i, &l).is_err());
    }

    #[test]
    fn edit_requires_outranking() {
        let a = actor(Permissions::MANAGE_ROLES, 3);
        let low = role(300, 2, Permissions::empty());
        let same = role(301, 3, Permissions::empty());
        let patch = || RolePatch {
            name: Some("x".into()),
            ..Default::default()
        };
        assert_eq!(plan_edit_role(&a, &low, patch()).unwrap().name, "x");
        assert!(plan_edit_role(&a, &same, patch()).is_err());
    }

    #[test]
    fn edit_permission_delta_must_be_held() {
        let a = actor(Permissions::MANAGE_ROLES | Permissions::KICK_MEMBERS, 5);
        // The target already has BAN_MEMBERS, which the actor lacks. Keeping it is fine,
        // adding KICK is fine, removing BAN is not (changing a bit you don't hold).
        let target = role(300, 2, Permissions::BAN_MEMBERS);
        let keep = RolePatch {
            permissions: Some(Permissions::BAN_MEMBERS | Permissions::KICK_MEMBERS),
            ..Default::default()
        };
        assert!(plan_edit_role(&a, &target, keep).is_ok());
        let drop = RolePatch {
            permissions: Some(Permissions::KICK_MEMBERS),
            ..Default::default()
        };
        assert!(plan_edit_role(&a, &target, drop).is_err());
    }

    #[test]
    fn everyone_is_restricted() {
        let a = owner();
        let e = everyone();
        let rename = RolePatch {
            name: Some("x".into()),
            ..Default::default()
        };
        assert!(plan_edit_role(&a, &e, rename).is_err());
        let perms = RolePatch {
            permissions: Some(Permissions::VIEW_CHANNEL),
            ..Default::default()
        };
        assert!(plan_edit_role(&a, &e, perms).is_ok());
        assert!(check_delete_role(&a, &e).is_err());
        assert!(check_reposition(&a, &e, 3).is_err());
    }

    #[test]
    fn managed_roles_are_protected() {
        let a = owner();
        let mut m = role(300, 2, Permissions::empty());
        m.managed = true;
        let p = RolePatch {
            permissions: Some(Permissions::VIEW_CHANNEL),
            ..Default::default()
        };
        assert!(plan_edit_role(&a, &m, p).is_err());
        assert!(check_delete_role(&a, &m).is_err());
        assert!(check_assign_role(&a, &m).is_err());
    }

    #[test]
    fn emoji_and_icon_replace_each_other_and_text_is_refused() {
        let a = actor(Permissions::MANAGE_ROLES, 5);
        let low = role(300, 2, Permissions::empty());
        let with_icon = plan_edit_role(
            &a,
            &low,
            RolePatch {
                icon_key: Some(Some("h".into())),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(with_icon.icon_key.as_deref(), Some("h"));
        let with_emoji = plan_edit_role(
            &a,
            &with_icon,
            RolePatch {
                unicode_emoji: Some(Some("🎉".into())),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(with_emoji.unicode_emoji.as_deref(), Some("🎉"));
        assert!(with_emoji.icon_key.is_none());
        for bad in ["", "vip", "123", "🎉🎉🎉🎉🎉🎉🎉🎉🎉🎉🎉🎉🎉🎉🎉🎉🎉"]
        {
            let p = RolePatch {
                unicode_emoji: Some(Some(bad.into())),
                ..Default::default()
            };
            assert!(plan_edit_role(&a, &low, p).is_err(), "{bad:?}");
        }
        let cleared = plan_edit_role(
            &a,
            &with_emoji,
            RolePatch {
                unicode_emoji: Some(None),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(cleared.unicode_emoji.is_none());
    }

    #[test]
    fn delete_assign_reposition_hierarchy() {
        let a = actor(Permissions::MANAGE_ROLES, 4);
        let low = role(300, 2, Permissions::empty());
        let high = role(301, 4, Permissions::empty());
        assert!(check_delete_role(&a, &low).is_ok());
        assert!(check_delete_role(&a, &high).is_err());
        assert!(check_assign_role(&a, &low).is_ok());
        assert!(check_assign_role(&a, &high).is_err());
        assert!(check_reposition(&a, &low, 3).is_ok());
        assert!(check_reposition(&a, &low, 4).is_err());
        assert!(check_reposition(&a, &low, 0).is_err());
        assert!(check_reposition(&actor(Permissions::empty(), 9), &low, 3).is_err());
    }

    #[test]
    fn gradient_colors_follow_discords_rules() {
        let a = actor(Permissions::MANAGE_ROLES, 4);
        let low = role(300, 2, Permissions::empty());
        let two = RoleColors {
            primary: 0xff0000,
            secondary: Some(0x0000ff),
            tertiary: None,
        };
        let out = plan_edit_role(
            &a,
            &low,
            RolePatch {
                colors: Some(two),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            (out.color, out.secondary_color, out.tertiary_color),
            (0xff0000, Some(0x0000ff), None)
        );
        // The holographic preset is the fixed triple.
        let holo = plan_edit_role(
            &a,
            &low,
            RolePatch {
                colors: Some(HOLOGRAPHIC),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            (holo.color, holo.secondary_color, holo.tertiary_color),
            (11_127_295, Some(16_759_788), Some(16_761_760))
        );
        // A tertiary stop needs a secondary one, and every stop fits 24 bits.
        for bad in [
            RoleColors {
                primary: 1,
                secondary: None,
                tertiary: Some(2),
            },
            RoleColors {
                primary: 1,
                secondary: Some(0x0100_0000),
                tertiary: None,
            },
            RoleColors {
                primary: 1,
                secondary: Some(2),
                tertiary: Some(0x0100_0000),
            },
            RoleColors {
                primary: 0x0100_0000,
                secondary: None,
                tertiary: None,
            },
        ] {
            let p = RolePatch {
                colors: Some(bad),
                ..Default::default()
            };
            assert!(plan_edit_role(&a, &low, p).is_err(), "{bad:?}");
        }
        // Sending colors with no secondary stop clears the gradient.
        let cleared = plan_edit_role(
            &a,
            &holo,
            RolePatch {
                colors: Some(RoleColors {
                    primary: 5,
                    secondary: None,
                    tertiary: None,
                }),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!((cleared.color, cleared.secondary_color), (5, None));
        // A create takes the same object and it wins over `color`.
        let l = ChatLimits::default();
        let made = plan_create_role(
            &a,
            GuildId::new(GUILD),
            RoleId::new(9),
            1,
            NewRole {
                color: Some(1),
                colors: Some(two),
                ..Default::default()
            },
            &l,
        )
        .unwrap();
        assert_eq!(
            (made.color, made.secondary_color),
            (0xff0000, Some(0x0000ff))
        );
    }
}
