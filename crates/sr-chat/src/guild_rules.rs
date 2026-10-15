//! Pure guild rules: creation blueprint, ownership transfer, leave, moderation targets.

use crate::domain::{Channel, ChannelType, Guild, Member, Role};
use crate::error::{ChatError, ChatResult};
use crate::perms::{GuildActor, default_everyone_permissions};
use crate::settings::templates::{self, TemplateSnapshot};
use sr_core::permissions::{Overwrite, Permissions};
use sr_core::{ChannelId, GuildId, RoleId, Snowflake, UserId};

pub const GUILD_NAME_MIN: usize = 2;
pub const GUILD_NAME_MAX: usize = 100;
/// Discord caps communication timeouts at 28 days.
pub const MAX_TIMEOUT_MS: i64 = 28 * 86_400_000;
/// Maximum recipients in a group DM (spec section 14).
pub const GROUP_DM_MAX: usize = 10;

pub fn validate_guild_name(name: &str) -> ChatResult<String> {
    let name = name.trim();
    let n = name.chars().count();
    if !(GUILD_NAME_MIN..=GUILD_NAME_MAX).contains(&n) {
        return Err(ChatError::invalid(
            "name",
            "BASE_TYPE_BAD_LENGTH",
            format!("Must be between {GUILD_NAME_MIN} and {GUILD_NAME_MAX} in length."),
        ));
    }
    Ok(name.to_owned())
}

/// Everything created together with a new guild (LNG-GLD-002).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuildBlueprint {
    pub guild: Guild,
    pub everyone_role: Role,
    pub owner_member: Member,
    pub channels: Vec<Channel>,
    /// Roles beyond `@everyone` (only a template brings any).
    pub extra_roles: Vec<Role>,
    /// Channel permission overwrites (only a template brings any).
    pub overwrites: Vec<(ChannelId, Overwrite)>,
}

/// Plan a new guild: `@everyone`, categories "Text Channels" / "Voice Channels",
/// `#general` and the `General` voice channel (spec section 5.2).
pub fn plan_new_guild(
    owner: UserId,
    name: &str,
    now_ms: i64,
    mut next_id: impl FnMut() -> Snowflake,
) -> ChatResult<GuildBlueprint> {
    let name = validate_guild_name(name)?;
    let guild_id = GuildId::from(next_id());
    let text_cat = ChannelId::from(next_id());
    let voice_cat = ChannelId::from(next_id());
    let general = ChannelId::from(next_id());
    let voice = ChannelId::from(next_id());

    let mk = |id: ChannelId, kind, name: &str, position, parent: Option<ChannelId>| {
        let mut c = Channel::new(id, Some(guild_id), kind);
        c.name = Some(name.to_owned());
        c.position = position;
        c.parent_id = parent;
        c
    };
    let mut voice_ch = mk(voice, ChannelType::Voice, "General", 0, Some(voice_cat));
    voice_ch.bitrate = Some(64_000);
    let channels = vec![
        mk(text_cat, ChannelType::Category, "Text Channels", 0, None),
        mk(voice_cat, ChannelType::Category, "Voice Channels", 1, None),
        mk(general, ChannelType::Text, "general", 0, Some(text_cat)),
        voice_ch,
    ];
    let guild = Guild {
        id: guild_id,
        name,
        description: None,
        icon_key: None,
        banner_key: None,
        splash_key: None,
        vanity_code: None,
        mfa_level: 0,
        system_channel_flags: 0,
        widget_enabled: false,
        widget_channel_id: None,
        owner_id: owner,
        is_staff_guild: false,
        system_channel_id: Some(general),
        afk_channel_id: None,
        afk_timeout: crate::admin_rules::DEFAULT_AFK_TIMEOUT,
        verification_level: 0,
        default_message_notifications: 0,
        explicit_content_filter: 0,
        preferred_locale: "en-US".to_owned(),
    };
    let everyone_role = Role {
        id: RoleId::new(guild_id.get()),
        guild_id,
        name: "@everyone".to_owned(),
        color: 0,
        hoist: false,
        position: 0,
        permissions: default_everyone_permissions(),
        mentionable: false,
        managed: false,
        unicode_emoji: None,
        icon_key: None,
    };
    let owner_member = Member {
        guild_id,
        user_id: owner,
        nick: None,
        role_ids: Vec::new(),
        joined_at_ms: now_ms,
        communication_disabled_until_ms: None,
        pending: false,
        avatar_key: None,
        banner_key: None,
        bio: None,
    };
    Ok(GuildBlueprint {
        guild,
        everyone_role,
        owner_member,
        channels,
        extra_roles: Vec::new(),
        overwrites: Vec::new(),
    })
}

/// Plan a new guild from a template snapshot (D-180): the same guild row, owner
/// membership and `@everyone` as [`plan_new_guild`], but roles, channels, overwrites and
/// the settings the snapshot carries replace the default layout. A snapshot without an
/// `@everyone` role keeps the default one. Everything is created in one repository call.
pub fn plan_guild_from_snapshot(
    owner: UserId,
    name: &str,
    now_ms: i64,
    snapshot: &TemplateSnapshot,
    mut next_id: impl FnMut() -> Snowflake,
) -> ChatResult<GuildBlueprint> {
    let mut bp = plan_new_guild(owner, name, now_ms, &mut next_id)?;
    let inst = templates::instantiate(snapshot, bp.guild.id, || next_id().get());
    for r in inst.roles {
        if r.id.get() == bp.guild.id.get() {
            bp.everyone_role = r;
        } else {
            bp.extra_roles.push(r);
        }
    }
    bp.overwrites = inst
        .channels
        .iter()
        .flat_map(|c| c.overwrites.iter().map(|o| (c.channel.id, *o)))
        .collect();
    bp.channels = inst.channels.into_iter().map(|c| c.channel).collect();
    let s = &snapshot.settings;
    bp.guild.system_channel_id = inst.system_channel_id;
    bp.guild.verification_level = s.verification_level.min(4);
    bp.guild.default_message_notifications = s.default_message_notifications.min(1);
    bp.guild.explicit_content_filter = s.explicit_content_filter.min(2);
    if !s.preferred_locale.trim().is_empty() {
        bp.guild.preferred_locale = s.preferred_locale.clone();
    }
    Ok(bp)
}

/// Only the owner may transfer ownership, to another existing member.
pub fn check_transfer_ownership(
    guild: &Guild,
    actor: UserId,
    new_owner: &Member,
) -> ChatResult<()> {
    if guild.owner_id != actor {
        return Err(ChatError::MissingPermissions);
    }
    if new_owner.user_id == actor {
        return Err(ChatError::invalid(
            "owner_id",
            "ALREADY_OWNER",
            "You are already the owner.",
        ));
    }
    Ok(())
}

/// Owners must transfer ownership before leaving (LNG-GLD-027).
pub fn check_leave(guild: &Guild, user: UserId) -> ChatResult<()> {
    if guild.owner_id == user {
        return Err(ChatError::invalid(
            "guild_id",
            "OWNER_CANNOT_LEAVE",
            "Transfer ownership before leaving the server.",
        ));
    }
    Ok(())
}

/// What a moderation action does to a member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModAction {
    Kick,
    Ban,
    Timeout,
}

impl ModAction {
    const fn required(self) -> Permissions {
        match self {
            Self::Kick => Permissions::KICK_MEMBERS,
            Self::Ban => Permissions::BAN_MEMBERS,
            Self::Timeout => Permissions::MODERATE_MEMBERS,
        }
    }
}

/// Permission and hierarchy check for kick/ban/timeout. Nobody may act on the owner,
/// on themselves, or on a member whose top role is not strictly lower. Timeouts
/// additionally refuse administrators.
pub fn check_moderation(
    guild: &Guild,
    action: ModAction,
    actor_user: UserId,
    actor: &GuildActor,
    target: &Member,
    target_top_position: i32,
    target_perms: Permissions,
) -> ChatResult<()> {
    if !actor.perms.has(action.required()) {
        return Err(ChatError::MissingPermissions);
    }
    if target.user_id == actor_user || target.user_id == guild.owner_id {
        return Err(ChatError::MissingPermissions);
    }
    if !actor.outranks_position(target_top_position) {
        return Err(ChatError::MissingPermissions);
    }
    if action == ModAction::Timeout && target_perms.contains(Permissions::ADMINISTRATOR) {
        return Err(ChatError::MissingPermissions);
    }
    Ok(())
}

/// Validate a timeout end time relative to `now_ms` (None clears the timeout).
pub fn validate_timeout_until(until_ms: Option<i64>, now_ms: i64) -> ChatResult<()> {
    if let Some(u) = until_ms
        && (u <= now_ms || u - now_ms > MAX_TIMEOUT_MS)
    {
        return Err(ChatError::invalid(
            "communication_disabled_until",
            "TIMEOUT_RANGE",
            "Timeout must end in the future, within 28 days.",
        ));
    }
    Ok(())
}

/// Group DM recipient count, including the creator.
pub fn validate_group_dm_size(total_recipients: usize) -> ChatResult<()> {
    if !(2..=GROUP_DM_MAX).contains(&total_recipients) {
        return Err(ChatError::invalid(
            "recipients",
            "BASE_TYPE_BAD_LENGTH",
            format!("A group DM needs 2 to {GROUP_DM_MAX} people."),
        ));
    }
    Ok(())
}

/// Canonical (low, high) ordering for the 1:1 DM uniqueness key (`dm_channels`).
pub fn dm_pair(a: UserId, b: UserId) -> (UserId, UserId) {
    if a <= b { (a, b) } else { (b, a) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perms::fixtures::*;
    use sr_core::{ManualClock, SnowflakeGenerator};

    #[test]
    fn name_validation() {
        assert!(validate_guild_name("a").is_err());
        assert!(validate_guild_name("  ab  ").is_ok_and(|n| n == "ab"));
        assert!(validate_guild_name(&"x".repeat(101)).is_err());
        assert!(validate_guild_name(&"x".repeat(100)).is_ok());
        // Length is counted in characters, not bytes.
        assert!(validate_guild_name(&"é".repeat(100)).is_ok());
    }

    #[test]
    fn blueprint_has_default_layout() {
        let gen_ = SnowflakeGenerator::new(ManualClock::new(1_800_000_000_000), 1, 1).unwrap();
        let bp = plan_new_guild(UserId::new(7), "My Server", 5, || gen_.generate()).unwrap();
        assert_eq!(bp.everyone_role.id.get(), bp.guild.id.get());
        assert_eq!(bp.everyone_role.position, 0);
        assert_eq!(bp.guild.owner_id, UserId::new(7));
        assert_eq!(bp.owner_member.user_id, UserId::new(7));
        let names: Vec<_> = bp
            .channels
            .iter()
            .map(|c| (c.kind, c.name.clone().unwrap()))
            .collect();
        assert_eq!(
            names,
            vec![
                (ChannelType::Category, "Text Channels".into()),
                (ChannelType::Category, "Voice Channels".into()),
                (ChannelType::Text, "general".into()),
                (ChannelType::Voice, "General".into()),
            ]
        );
        let general = &bp.channels[2];
        assert_eq!(general.parent_id, Some(bp.channels[0].id));
        assert_eq!(bp.guild.system_channel_id, Some(general.id));
        // Unique ids.
        let mut ids: Vec<_> = bp.channels.iter().map(|c| c.id.get()).collect();
        ids.push(bp.guild.id.get());
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 5);
    }

    #[test]
    fn template_blueprint_replaces_the_default_layout() {
        let gen_ = SnowflakeGenerator::new(ManualClock::new(1_800_000_000_000), 1, 1).unwrap();
        let tmpl = templates::builtin("gaming").unwrap();
        let bp = plan_guild_from_snapshot(UserId::new(7), "Squad", 5, &tmpl.snapshot, || {
            gen_.generate()
        })
        .unwrap();
        let names: Vec<_> = bp.channels.iter().filter_map(|c| c.name.clone()).collect();
        assert!(names.contains(&"game-chat".to_owned()), "{names:?}");
        assert!(names.contains(&"general".to_owned()));
        // The system channel is the template's #general, and every parent exists.
        let sys = bp.guild.system_channel_id.unwrap();
        assert_eq!(
            bp.channels
                .iter()
                .find(|c| c.id == sys)
                .unwrap()
                .name
                .as_deref(),
            Some("general")
        );
        for c in &bp.channels {
            if let Some(p) = c.parent_id {
                assert!(bp.channels.iter().any(|x| x.id == p));
            }
        }
        // No @everyone in the snapshot: the default one stays.
        assert_eq!(bp.everyone_role.id.get(), bp.guild.id.get());
        assert_eq!(bp.everyone_role.permissions, default_everyone_permissions());
        assert!(bp.extra_roles.is_empty());
        let mut ids: Vec<_> = bp.channels.iter().map(|c| c.id.get()).collect();
        ids.push(bp.guild.id.get());
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "ids are unique");
    }

    #[test]
    fn template_blueprint_keeps_roles_and_overwrites() {
        use crate::settings::templates::{SnapChannel, SnapOverwrite, SnapRole, SnapSettings};
        let gen_ = SnowflakeGenerator::new(ManualClock::new(1_800_000_000_000), 1, 1).unwrap();
        let snap = TemplateSnapshot {
            settings: SnapSettings {
                preferred_locale: "de".into(),
                verification_level: 9,
                default_message_notifications: 1,
                explicit_content_filter: 2,
                system_channel_local_id: Some(1),
            },
            roles: vec![SnapRole {
                local_id: 1,
                name: "Mods".into(),
                color: 0,
                hoist: true,
                position: 1,
                permissions: Permissions::KICK_MEMBERS.bits(),
                mentionable: false,
                unicode_emoji: None,
            }],
            channels: vec![SnapChannel {
                local_id: 1,
                kind: 0,
                name: "mods-only".into(),
                topic: None,
                position: 0,
                parent_local_id: None,
                nsfw: false,
                rate_limit_per_user: 0,
                bitrate: None,
                user_limit: None,
                overwrites: vec![SnapOverwrite {
                    role_local_id: 1,
                    allow: Permissions::VIEW_CHANNEL.bits(),
                    deny: 0,
                }],
            }],
        };
        let bp =
            plan_guild_from_snapshot(UserId::new(7), "Mods", 5, &snap, || gen_.generate()).unwrap();
        assert_eq!(bp.extra_roles.len(), 1);
        assert_eq!(bp.extra_roles[0].name, "Mods");
        assert_eq!(bp.overwrites.len(), 1);
        assert_eq!(bp.overwrites[0].0, bp.channels[0].id);
        assert_eq!(bp.overwrites[0].1.id.get(), bp.extra_roles[0].id.get());
        assert_eq!(bp.guild.verification_level, 4, "clamped");
        assert_eq!(bp.guild.preferred_locale, "de");
    }

    #[test]
    fn blueprint_rejects_bad_name() {
        let gen_ = SnowflakeGenerator::new(ManualClock::new(1_800_000_000_000), 1, 1).unwrap();
        assert!(plan_new_guild(UserId::new(7), "x", 0, || gen_.generate()).is_err());
    }

    #[test]
    fn ownership_and_leave() {
        let g = guild();
        let m = member(5, &[]);
        assert!(check_transfer_ownership(&g, UserId::new(OWNER), &m).is_ok());
        assert!(matches!(
            check_transfer_ownership(&g, UserId::new(5), &m),
            Err(ChatError::MissingPermissions)
        ));
        assert!(check_transfer_ownership(&g, UserId::new(OWNER), &member(OWNER, &[])).is_err());
        assert!(check_leave(&g, UserId::new(OWNER)).is_err());
        assert!(check_leave(&g, UserId::new(5)).is_ok());
    }

    #[test]
    fn moderation_hierarchy() {
        let g = guild();
        let roles = [
            everyone(),
            role(
                200,
                5,
                Permissions::KICK_MEMBERS | Permissions::MODERATE_MEMBERS,
            ),
            role(201, 5, Permissions::empty()),
            role(202, 2, Permissions::empty()),
        ];
        let mod_m = member(10, &[200]);
        let actor = GuildActor::new(&g, &mod_m, &roles);
        let ok_target = member(11, &[202]);
        assert!(
            check_moderation(
                &g,
                ModAction::Kick,
                mod_m.user_id,
                &actor,
                &ok_target,
                2,
                Permissions::empty()
            )
            .is_ok()
        );
        // Equal rank refused.
        let peer = member(12, &[201]);
        assert!(
            check_moderation(
                &g,
                ModAction::Kick,
                mod_m.user_id,
                &actor,
                &peer,
                5,
                Permissions::empty()
            )
            .is_err()
        );
        // No BAN permission.
        assert!(
            check_moderation(
                &g,
                ModAction::Ban,
                mod_m.user_id,
                &actor,
                &ok_target,
                2,
                Permissions::empty()
            )
            .is_err()
        );
        // Owner and self are untouchable.
        let owner = member(OWNER, &[]);
        assert!(
            check_moderation(
                &g,
                ModAction::Kick,
                mod_m.user_id,
                &actor,
                &owner,
                0,
                Permissions::ALL
            )
            .is_err()
        );
        assert!(
            check_moderation(
                &g,
                ModAction::Kick,
                mod_m.user_id,
                &actor,
                &mod_m,
                5,
                Permissions::empty()
            )
            .is_err()
        );
        // Admins cannot be timed out.
        assert!(
            check_moderation(
                &g,
                ModAction::Timeout,
                mod_m.user_id,
                &actor,
                &ok_target,
                2,
                Permissions::ALL
            )
            .is_err()
        );
        assert!(
            check_moderation(
                &g,
                ModAction::Timeout,
                mod_m.user_id,
                &actor,
                &ok_target,
                2,
                Permissions::empty()
            )
            .is_ok()
        );
    }

    #[test]
    fn timeout_range() {
        assert!(validate_timeout_until(None, 100).is_ok());
        assert!(validate_timeout_until(Some(100), 100).is_err());
        assert!(validate_timeout_until(Some(101), 100).is_ok());
        assert!(validate_timeout_until(Some(100 + MAX_TIMEOUT_MS), 100).is_ok());
        assert!(validate_timeout_until(Some(101 + MAX_TIMEOUT_MS), 100).is_err());
    }

    #[test]
    fn dm_helpers() {
        assert_eq!(
            dm_pair(UserId::new(9), UserId::new(3)),
            (UserId::new(3), UserId::new(9))
        );
        assert!(validate_group_dm_size(1).is_err());
        assert!(validate_group_dm_size(10).is_ok());
        assert!(validate_group_dm_size(11).is_err());
    }
}
