//! Pure rules of the staff model: which permissions only the Owner may grant,
//! who may touch whom, and the default staff roles.

use crate::{
    error::{AdminError, AdminResult},
    model::{Caller, StaffMember, StaffRole},
};
use sr_core::PlatformPermissions as P;

/// Bits only the Owner can grant, hold through a delegate, or revoke from others
/// (doc 07 section 3: `READ_PRIVATE_CONTENT` is Owner-granted; appointment rights
/// are Owner-delegated, D-132; the access log and infrastructure are Owner-held by default).
pub const OWNER_ONLY: P = P::READ_PRIVATE_CONTENT
    .union(P::APPOINT_STAFF)
    .union(P::VIEW_ACCESS_LOG)
    .union(P::OPERATOR);

/// Name of the permission bit for error messages.
pub fn perm_name(p: P) -> &'static str {
    p.iter_names().next().map_or("UNKNOWN", |(n, _)| n)
}

/// A default staff role. These are DATA: `migrations/0130` seeds them into
/// `staff_role_templates` and `sr-cli bootstrap` instantiates them in the Staff
/// Server. Doc 07 names the roles but not their bits, so the bits below are
/// proposed defaults that the owner must confirm.
#[derive(Debug, Clone, Copy)]
pub struct RoleTemplate {
    pub name: &'static str,
    pub position: i32,
    pub color: i32,
    pub permissions: P,
    pub description: &'static str,
}

/// The seven roles of doc 07 section 4, lowest first.
pub fn default_staff_roles() -> Vec<RoleTemplate> {
    // Everything except OWNER, money (MANAGE_DONATIONS), the access log and OPERATOR.
    let senior = P::all()
        .difference(P::OWNER)
        .difference(P::MANAGE_DONATIONS)
        .difference(P::VIEW_ACCESS_LOG)
        .difference(P::OPERATOR);
    vec![
        RoleTemplate {
            name: "Support",
            position: 1,
            color: 0x5865F2,
            permissions: P::STAFF_BASIC | P::MANAGE_ACCOUNTS | P::MANAGE_INVITES,
            description: "Account help: deactivate, reset 2FA, invite credits and codes.",
        },
        RoleTemplate {
            name: "App Reviewer",
            position: 2,
            color: 0x9B59B6,
            permissions: P::STAFF_BASIC | P::REVIEW_APPS | P::REVIEW_PACKAGES,
            description: "Approve developer applications and packages.",
        },
        RoleTemplate {
            name: "Library Curator",
            position: 3,
            color: 0x1ABC9C,
            permissions: P::STAFF_BASIC
                | P::REVIEW_READING
                | P::PUBLISH_READING
                | P::MANAGE_COLLECTIONS,
            description: "Review and publish Library content.",
        },
        RoleTemplate {
            name: "Media Curator",
            position: 4,
            color: 0xE67E22,
            permissions: P::STAFF_BASIC
                | P::REVIEW_MEDIA
                | P::PUBLISH_MEDIA
                | P::MANAGE_COLLECTIONS
                | P::MANAGE_TV,
            description: "Review and publish Cinema content and Live TV.",
        },
        RoleTemplate {
            name: "Moderator",
            position: 5,
            color: 0x2ECC71,
            permissions: P::STAFF_BASIC
                | P::MANAGE_DIRECTORY
                | P::MANAGE_ACCOUNTS
                | P::PLATFORM_BAN,
            description: "Accounts, platform bans and the server directory.",
        },
        RoleTemplate {
            name: "Senior Staff",
            position: 6,
            color: 0xE74C3C,
            permissions: senior,
            description: "Everything except donations, the access log and infrastructure.",
        },
        RoleTemplate {
            name: "Owner",
            position: 7,
            color: 0xF1C40F,
            permissions: P::OWNER,
            description: "The installation owner. Implicit for the Staff Server owner.",
        },
    ]
}

/// May `caller` hand out `roles`? Never the Owner role; owner-only bits only by the
/// Owner; everyone else may grant only bits they hold themselves.
pub fn check_assignable(caller: &Caller, roles: &[StaffRole]) -> AdminResult<()> {
    check_assignable_within(caller, roles, None)
}

/// [`check_assignable`] for a delegate of the Owner: nothing beyond `ceiling` either (D-132).
pub fn check_assignable_within(
    caller: &Caller,
    roles: &[StaffRole],
    ceiling: Option<P>,
) -> AdminResult<()> {
    for r in roles {
        let p = r.permissions();
        if p.contains(P::OWNER) {
            return Err(AdminError::Forbidden(
                "The Owner role cannot be assigned; use `sr-cli staff transfer-owner`".into(),
            ));
        }
        if caller.is_owner {
            continue;
        }
        if let Some(ceiling) = ceiling
            && !ceiling.contains(p)
        {
            return Err(AdminError::Forbidden(format!(
                "The role {} goes beyond the permissions the Owner delegated to you",
                r.name
            )));
        }
        if p.intersects(OWNER_ONLY) {
            return Err(AdminError::Forbidden(format!(
                "Only the Owner can grant the role {} (it holds an Owner-only permission)",
                r.name
            )));
        }
        if !caller.permissions.contains(p) {
            return Err(AdminError::Forbidden(format!(
                "You cannot grant the role {} because it holds permissions you do not have",
                r.name
            )));
        }
    }
    Ok(())
}

/// May `caller` revoke or re-role `target`? The Owner is immutable; otherwise the
/// target must hold no Owner-only bit and nothing the caller lacks.
pub fn check_manageable(caller: &Caller, target: &StaffMember) -> AdminResult<()> {
    if target.is_owner {
        return Err(AdminError::Forbidden(
            "The Owner cannot be changed; use `sr-cli staff transfer-owner`".into(),
        ));
    }
    if caller.is_owner {
        return Ok(());
    }
    let bits: u64 = target.platform_permissions.parse().unwrap_or(0);
    let tp = P::from_bits_truncate(bits);
    if tp.intersects(OWNER_ONLY) || !caller.permissions.contains(tp) {
        return Err(AdminError::Forbidden(
            "This staff member holds permissions you do not have".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn role(name: &str, p: P) -> StaffRole {
        StaffRole {
            id: 1,
            name: name.into(),
            position: 1,
            platform_permissions: p.bits().to_string(),
        }
    }

    fn caller(p: P, owner: bool) -> Caller {
        Caller {
            user_id: 9,
            permissions: p,
            is_owner: owner,
            ip: None,
        }
    }

    #[test]
    fn defaults_cover_the_documented_roles_and_keep_owner_only_bits_apart() {
        let roles = default_staff_roles();
        let names: Vec<_> = roles.iter().map(|r| r.name).collect();
        assert_eq!(
            names,
            [
                "Support",
                "App Reviewer",
                "Library Curator",
                "Media Curator",
                "Moderator",
                "Senior Staff",
                "Owner"
            ]
        );
        for r in &roles {
            if r.name != "Owner" {
                assert!(!r.permissions.contains(P::OWNER), "{}", r.name);
                assert!(r.permissions.contains(P::STAFF_BASIC), "{}", r.name);
            }
        }
        let senior = roles.iter().find(|r| r.name == "Senior Staff").unwrap();
        assert!(senior.permissions.contains(P::APPOINT_STAFF));
        assert!(senior.permissions.contains(P::READ_PRIVATE_CONTENT));
        assert!(!senior.permissions.contains(P::VIEW_ACCESS_LOG));
        let positions: Vec<_> = roles.iter().map(|r| r.position).collect();
        assert!(positions.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn assignment_rules() {
        let delegate = caller(
            P::STAFF_BASIC | P::MANAGE_ACCOUNTS | P::APPOINT_STAFF,
            false,
        );
        // Within the delegate's own bits.
        assert!(
            check_assignable(
                &delegate,
                &[role("Support", P::STAFF_BASIC | P::MANAGE_ACCOUNTS)]
            )
            .is_ok()
        );
        // Beyond them.
        assert!(
            check_assignable(&delegate, &[role("Mod", P::STAFF_BASIC | P::PLATFORM_BAN)]).is_err()
        );
        // Owner-only bit, even if the delegate somehow holds it.
        let high = caller(P::READ_PRIVATE_CONTENT | P::STAFF_BASIC, false);
        assert!(check_assignable(&high, &[role("Reader", P::READ_PRIVATE_CONTENT)]).is_err());
        let owner = caller(P::OWNER, true);
        assert!(check_assignable(&owner, &[role("Reader", P::READ_PRIVATE_CONTENT)]).is_ok());
        assert!(check_assignable(&owner, &[role("Owner", P::OWNER)]).is_err());
    }

    #[test]
    fn a_delegate_stays_inside_the_ceiling() {
        let delegate = caller(
            P::STAFF_BASIC | P::MANAGE_ACCOUNTS | P::PLATFORM_BAN | P::APPOINT_STAFF,
            false,
        );
        let ceiling = Some(P::STAFF_BASIC | P::MANAGE_ACCOUNTS);
        assert!(
            check_assignable_within(
                &delegate,
                &[role("Support", P::STAFF_BASIC | P::MANAGE_ACCOUNTS)],
                ceiling
            )
            .is_ok()
        );
        // Held by the delegate but outside the ceiling.
        assert!(
            check_assignable_within(
                &delegate,
                &[role("Mod", P::STAFF_BASIC | P::PLATFORM_BAN)],
                ceiling
            )
            .is_err()
        );
    }

    #[test]
    fn manage_rules() {
        let member = |p: P, owner: bool| StaffMember {
            account_id: 5,
            username: "x".into(),
            display_name: None,
            is_owner: owner,
            roles: vec![],
            platform_permissions: p.bits().to_string(),
            appointed_by: None,
            appointed_at_ms: 0,
            reason: String::new(),
        };
        let delegate = caller(
            P::STAFF_BASIC | P::MANAGE_ACCOUNTS | P::APPOINT_STAFF,
            false,
        );
        assert!(check_manageable(&delegate, &member(P::STAFF_BASIC, false)).is_ok());
        assert!(check_manageable(&delegate, &member(P::PLATFORM_BAN, false)).is_err());
        assert!(check_manageable(&delegate, &member(P::APPOINT_STAFF, false)).is_err());
        let owner = caller(P::OWNER, true);
        assert!(check_manageable(&owner, &member(P::READ_PRIVATE_CONTENT, false)).is_ok());
        assert!(check_manageable(&owner, &member(P::OWNER, true)).is_err());
    }
}
