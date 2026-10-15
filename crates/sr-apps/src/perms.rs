//! Permission rules of the developer platform: who may install an application, what the
//! bot's managed role gets, and who may invoke a command (permissions v2).

use crate::{
    error::{AppsError, AppsResult},
    model::{Application, CommandPermission, GuildStats, ReleaseState},
};
use sr_core::Permissions;

/// Largest guild a development-state application may be installed in (doc 11 section 2.1).
pub const TEST_SERVER_MAX_MEMBERS: i64 = 25;

/// Whether `installer` may add `app` to a guild right now.
///
/// * the application must not be suspended;
/// * the installer needs `MANAGE_GUILD` (the guild owner and `ADMINISTRATOR` count);
/// * a non-public bot can only be added by its owner (or, in development, an accepted tester);
/// * a development-state application can only be added by its owner or an accepted tester
///   (`installer_is_tester`, D-204), to a guild of at most 25 members (900120 otherwise).
pub fn check_install(
    app: &Application,
    installer: i64,
    installer_perms: Permissions,
    installer_is_tester: bool,
    guild: GuildStats,
) -> AppsResult<()> {
    if app.release_state == ReleaseState::Suspended {
        return Err(AppsError::Suspended);
    }
    let manages = installer == guild.owner_id
        || installer_perms.contains(Permissions::MANAGE_GUILD)
        || installer_perms.contains(Permissions::ADMINISTRATOR);
    if !manages {
        return Err(AppsError::MissingPermissions);
    }
    // Accepted testers count only while the application is in development.
    let trusted = installer == app.owner_id
        || (installer_is_tester && app.release_state == ReleaseState::Development);
    if app.release_state != ReleaseState::Released
        && (!trusted || guild.member_count > TEST_SERVER_MAX_MEMBERS)
    {
        return Err(AppsError::NotReleased);
    }
    if !app.bot_public && !trusted {
        return Err(AppsError::Forbidden(
            "This bot is private and can only be added by its owner.".into(),
        ));
    }
    Ok(())
}

/// Permissions the bot's managed role gets: what was asked for, limited to what the
/// installer holds themselves. The guild owner and `ADMINISTRATOR` holders may grant any
/// known permission. Nobody can hand out a permission they lack, so an installer cannot
/// escalate through a bot.
pub fn granted_permissions(
    requested: u64,
    installer_perms: Permissions,
    installer_is_owner: bool,
) -> u64 {
    let all = Permissions::all();
    let mask = if installer_is_owner || installer_perms.contains(Permissions::ADMINISTRATOR) {
        all
    } else {
        installer_perms
    };
    requested & mask.bits() & all.bits()
}

/// The invoking member, as the command permission check needs them.
#[derive(Debug, Clone, Copy)]
pub struct Invoker<'a> {
    /// User id.
    pub user_id: i64,
    /// Guild id (the `@everyone` role id, and `guild_id - 1` names "all channels").
    pub guild_id: i64,
    /// Channel the command is used in.
    pub channel_id: i64,
    /// Explicit role ids (without `@everyone`).
    pub roles: &'a [i64],
    /// The member's permissions in the channel (owner and administrator already resolved).
    pub permissions: Permissions,
}

/// Whether the invoker may use a command.
///
/// `command` is the command's own overwrite list, `app_default` the application-wide one,
/// `default_member_permissions` the command's default. Order: administrators always pass;
/// the command's own list replaces the application-wide list when it is not empty; a channel
/// entry (or the "all channels" entry) can deny; then a user entry decides, else specific
/// role entries (any allow wins), else the `@everyone` entry; with no applicable entry the
/// command default decides (`None` = everyone).
pub fn can_invoke(
    inv: Invoker<'_>,
    command: &[CommandPermission],
    app_default: &[CommandPermission],
    default_member_permissions: Option<u64>,
) -> bool {
    if inv.permissions.contains(Permissions::ADMINISTRATOR) {
        return true;
    }
    if !inv
        .permissions
        .contains(Permissions::USE_APPLICATION_COMMANDS)
    {
        return false;
    }
    let entries = if command.is_empty() {
        app_default
    } else {
        command
    };
    let all_channels = inv.guild_id - 1;
    let channel_entry = entries
        .iter()
        .find(|e| e.target_type == 3 && e.target_id == inv.channel_id)
        .or_else(|| {
            entries
                .iter()
                .find(|e| e.target_type == 3 && e.target_id == all_channels)
        });
    if channel_entry.is_some_and(|e| !e.allowed) {
        return false;
    }
    if let Some(e) = entries
        .iter()
        .find(|e| e.target_type == 2 && e.target_id == inv.user_id)
    {
        return e.allowed;
    }
    let specific: Vec<&CommandPermission> = entries
        .iter()
        .filter(|e| {
            e.target_type == 1 && e.target_id != inv.guild_id && inv.roles.contains(&e.target_id)
        })
        .collect();
    if !specific.is_empty() {
        return specific.iter().any(|e| e.allowed);
    }
    if let Some(e) = entries
        .iter()
        .find(|e| e.target_type == 1 && e.target_id == inv.guild_id)
    {
        return e.allowed;
    }
    match default_member_permissions {
        None => true,
        Some(bits) => bits != 0 && inv.permissions.bits() & bits == bits,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_auth::session::TokenHash;

    fn app(state: ReleaseState, public: bool) -> Application {
        Application {
            id: 10,
            owner_id: 1,
            bot_user_id: 11,
            name: "bot".into(),
            description: String::new(),
            icon_key: None,
            tags: vec![],
            bot_public: public,
            bot_require_code_grant: false,
            redirect_uris: vec![],
            install_scopes: vec![],
            install_permissions: 0,
            terms_url: None,
            privacy_url: None,
            release_state: state,
            suspended_reason: None,
            intents_requested: 0,
            intents_approved: 0,
            client_secret_hash: TokenHash::from_bytes(&[0; 32]).unwrap(),
            client_secret_hint: String::new(),
            created_at_ms: 0,
        }
    }

    const GUILD: GuildStats = GuildStats {
        owner_id: 1,
        member_count: 5,
    };

    #[test]
    fn install_needs_manage_guild() {
        let a = app(ReleaseState::Released, true);
        assert_eq!(
            check_install(&a, 2, Permissions::SEND_MESSAGES, false, GUILD),
            Err(AppsError::MissingPermissions)
        );
        assert!(check_install(&a, 2, Permissions::MANAGE_GUILD, false, GUILD).is_ok());
        assert!(check_install(&a, 2, Permissions::ADMINISTRATOR, false, GUILD).is_ok());
        // the guild owner needs no permission bits
        assert!(check_install(&a, 1, Permissions::empty(), false, GUILD).is_ok());
    }

    #[test]
    fn private_bots_are_owner_only() {
        let a = app(ReleaseState::Released, false);
        assert!(matches!(
            check_install(&a, 2, Permissions::MANAGE_GUILD, false, GUILD),
            Err(AppsError::Forbidden(_))
        ));
        assert!(check_install(&a, 1, Permissions::MANAGE_GUILD, false, GUILD).is_ok());
    }

    #[test]
    fn development_apps_stay_in_test_servers_of_their_owner() {
        let a = app(ReleaseState::Development, true);
        assert!(check_install(&a, 1, Permissions::MANAGE_GUILD, false, GUILD).is_ok());
        let big = GuildStats {
            owner_id: 2,
            member_count: 26,
        };
        assert_eq!(
            check_install(&a, 1, Permissions::MANAGE_GUILD, false, big),
            Err(AppsError::NotReleased)
        );
        let edge = GuildStats {
            owner_id: 2,
            member_count: 25,
        };
        assert!(check_install(&a, 1, Permissions::MANAGE_GUILD, false, edge).is_ok());
        // someone else cannot add a development app even to a small guild
        assert_eq!(
            check_install(&a, 2, Permissions::MANAGE_GUILD, false, GUILD),
            Err(AppsError::NotReleased)
        );
        let review = app(ReleaseState::InReview, true);
        assert_eq!(
            check_install(&review, 2, Permissions::MANAGE_GUILD, false, GUILD),
            Err(AppsError::NotReleased)
        );
        let s = app(ReleaseState::Suspended, true);
        assert_eq!(
            check_install(&s, 1, Permissions::MANAGE_GUILD, false, GUILD),
            Err(AppsError::Suspended)
        );
    }

    #[test]
    fn accepted_testers_install_development_apps_in_small_guilds() {
        let dev = app(ReleaseState::Development, true);
        // a tester with MANAGE_GUILD in a small guild they do not own
        assert!(check_install(&dev, 2, Permissions::MANAGE_GUILD, true, GUILD).is_ok());
        // still needs MANAGE_GUILD
        assert_eq!(
            check_install(&dev, 2, Permissions::SEND_MESSAGES, true, GUILD),
            Err(AppsError::MissingPermissions)
        );
        // still capped at 25 members (900120)
        let big = GuildStats {
            owner_id: 2,
            member_count: 26,
        };
        assert_eq!(
            check_install(&dev, 2, Permissions::MANAGE_GUILD, true, big),
            Err(AppsError::NotReleased)
        );
        // a private development bot may be used by its testers but not by others
        let private = app(ReleaseState::Development, false);
        assert!(check_install(&private, 2, Permissions::MANAGE_GUILD, true, GUILD).is_ok());
        assert!(check_install(&private, 3, Permissions::MANAGE_GUILD, false, GUILD).is_err());
        // testers do not help once the application is in review
        let review = app(ReleaseState::InReview, true);
        assert_eq!(
            check_install(&review, 2, Permissions::MANAGE_GUILD, true, GUILD),
            Err(AppsError::NotReleased)
        );
    }

    #[test]
    fn a_bot_never_gets_more_than_the_installer_holds() {
        let want =
            (Permissions::SEND_MESSAGES | Permissions::MANAGE_ROLES | Permissions::ADMINISTRATOR)
                .bits();
        let installer = Permissions::MANAGE_GUILD | Permissions::SEND_MESSAGES;
        assert_eq!(
            granted_permissions(want, installer, false),
            Permissions::SEND_MESSAGES.bits()
        );
        assert_eq!(
            granted_permissions(want, Permissions::ADMINISTRATOR, false),
            want
        );
        assert_eq!(granted_permissions(want, Permissions::empty(), true), want);
        // unknown high bits are dropped
        assert_eq!(
            granted_permissions(1 << 63, Permissions::ADMINISTRATOR, false),
            0
        );
    }

    fn inv<'a>(roles: &'a [i64], perms: Permissions) -> Invoker<'a> {
        Invoker {
            user_id: 5,
            guild_id: 100,
            channel_id: 200,
            roles,
            permissions: perms,
        }
    }

    fn e(target_type: i16, target_id: i64, allowed: bool) -> CommandPermission {
        CommandPermission {
            target_id,
            target_type,
            allowed,
        }
    }

    #[test]
    fn default_permissions_gate_commands() {
        let base = Permissions::USE_APPLICATION_COMMANDS | Permissions::SEND_MESSAGES;
        assert!(can_invoke(inv(&[], base), &[], &[], None));
        assert!(!can_invoke(
            inv(&[], base),
            &[],
            &[],
            Some(Permissions::BAN_MEMBERS.bits())
        ));
        assert!(can_invoke(
            inv(&[], base | Permissions::BAN_MEMBERS),
            &[],
            &[],
            Some(Permissions::BAN_MEMBERS.bits())
        ));
        // 0 = administrators only
        assert!(!can_invoke(inv(&[], base), &[], &[], Some(0)));
        assert!(can_invoke(
            inv(&[], Permissions::ADMINISTRATOR),
            &[],
            &[],
            Some(0)
        ));
        // everybody needs USE_APPLICATION_COMMANDS
        assert!(!can_invoke(
            inv(&[], Permissions::SEND_MESSAGES),
            &[],
            &[],
            None
        ));
    }

    #[test]
    fn overwrites_resolve_user_then_role_then_everyone() {
        let base = Permissions::USE_APPLICATION_COMMANDS;
        // @everyone denied, a held role allowed
        let cmd = [e(1, 100, false), e(1, 7, true)];
        assert!(can_invoke(inv(&[7], base), &cmd, &[], None));
        assert!(!can_invoke(inv(&[8], base), &cmd, &[], None));
        // user entry beats roles
        let cmd = [e(1, 7, true), e(2, 5, false)];
        assert!(!can_invoke(inv(&[7], base), &cmd, &[], None));
        let cmd = [e(1, 7, false), e(2, 5, true)];
        assert!(can_invoke(inv(&[7], base), &cmd, &[], None));
        // any allowed role wins over a denied role
        let cmd = [e(1, 7, false), e(1, 8, true)];
        assert!(can_invoke(inv(&[7, 8], base), &cmd, &[], None));
        // entries replace the default permission requirement
        let cmd = [e(1, 100, true)];
        assert!(can_invoke(
            inv(&[], base),
            &cmd,
            &[],
            Some(Permissions::BAN_MEMBERS.bits())
        ));
        // no applicable entry falls back to the default
        let cmd = [e(1, 9, true)];
        assert!(!can_invoke(
            inv(&[], base),
            &cmd,
            &[],
            Some(Permissions::BAN_MEMBERS.bits())
        ));
    }

    #[test]
    fn channel_entries_and_app_defaults() {
        let base = Permissions::USE_APPLICATION_COMMANDS;
        // denied in this channel, allowed elsewhere
        let cmd = [e(3, 200, false)];
        assert!(!can_invoke(inv(&[], base), &cmd, &[], None));
        let cmd = [e(3, 201, false)];
        assert!(can_invoke(inv(&[], base), &cmd, &[], None));
        // "all channels" is guild id - 1
        let cmd = [e(3, 99, false)];
        assert!(!can_invoke(inv(&[], base), &cmd, &[], None));
        // a specific channel entry beats the all-channels entry
        let cmd = [e(3, 99, false), e(3, 200, true)];
        assert!(can_invoke(inv(&[], base), &cmd, &[], None));
        // application-wide defaults apply when the command has none
        let app = [e(1, 100, false)];
        assert!(!can_invoke(inv(&[], base), &[], &app, None));
        let cmd = [e(1, 100, true)];
        assert!(can_invoke(inv(&[], base), &cmd, &app, None));
        // administrators ignore every overwrite
        assert!(can_invoke(
            inv(&[], Permissions::ADMINISTRATOR),
            &[e(2, 5, false)],
            &[],
            None
        ));
    }
}
