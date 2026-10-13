//! Pure rules for guild administration: guild edits, member edits, channel and
//! role ordering (LNG-GLD-004/005/007, LNG-CHN-002).

use crate::admin_repo::{MemberFilter, PruneCriteria};
use crate::domain::{Channel, ChannelType, Guild, Member, Role};
use crate::error::{ChatError, ChatResult};
use crate::guild_rules::{
    ModAction, check_moderation, check_transfer_ownership, validate_guild_name,
    validate_timeout_until,
};
use crate::perms::GuildActor;
use crate::role_rules::{check_assign_role, check_reposition};
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, GuildId, RoleId, UserId};

pub const NICK_MAX: usize = 32;
pub const GUILD_DESCRIPTION_MAX: usize = 120;
pub const BAN_REASON_MAX: usize = 512;
/// Upper bound for one bulk reorder request.
pub const MAX_REORDER_ENTRIES: usize = 500;

#[derive(Debug, Clone, Default)]
pub struct GuildPatch {
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub system_channel_id: Option<Option<ChannelId>>,
    pub verification_level: Option<u8>,
    pub default_message_notifications: Option<u8>,
    pub explicit_content_filter: Option<u8>,
    pub preferred_locale: Option<String>,
    pub owner_id: Option<UserId>,
    /// Content hashes from the media pipeline; `Some(None)` removes the image.
    pub icon_key: Option<Option<String>>,
    pub banner_key: Option<Option<String>>,
    pub splash_key: Option<Option<String>>,
}

impl GuildPatch {
    fn only_owner(&self) -> bool {
        self.name.is_none()
            && self.description.is_none()
            && self.system_channel_id.is_none()
            && self.verification_level.is_none()
            && self.default_message_notifications.is_none()
            && self.explicit_content_filter.is_none()
            && self.preferred_locale.is_none()
            && self.icon_key.is_none()
            && self.banner_key.is_none()
            && self.splash_key.is_none()
    }
}

fn range_u8(field: &str, v: u8, max: u8) -> ChatResult<u8> {
    if v > max {
        return Err(ChatError::invalid(
            field,
            "NUMBER_TYPE_MAX",
            format!("Must be {max} or less."),
        ));
    }
    Ok(v)
}

/// Apply a guild patch. Needs `MANAGE_GUILD`; changing the owner needs the owner
/// and an existing member as target (`new_owner`). `system_channel_ok` tells
/// whether a requested system channel is a text channel of this guild.
pub fn plan_edit_guild(
    actor_user: UserId,
    actor: &GuildActor,
    guild: &Guild,
    patch: GuildPatch,
    new_owner: Option<&Member>,
    system_channel_ok: bool,
) -> ChatResult<Guild> {
    let changes_owner = patch.owner_id.is_some_and(|o| o != guild.owner_id);
    // A bare ownership transfer only needs ownership (checked below).
    if !actor.perms.has(Permissions::MANAGE_GUILD) && !(changes_owner && patch.only_owner()) {
        return Err(ChatError::MissingPermissions);
    }
    let mut out = guild.clone();
    if changes_owner {
        let target = new_owner.ok_or(ChatError::UnknownMember)?;
        check_transfer_ownership(guild, actor_user, target)?;
        out.owner_id = target.user_id;
    }
    if let Some(n) = patch.name {
        out.name = validate_guild_name(&n)?;
    }
    if let Some(d) = patch.description {
        out.description = match d.map(|s| s.trim().to_owned()) {
            Some(s) if s.chars().count() > GUILD_DESCRIPTION_MAX => {
                return Err(ChatError::invalid(
                    "description",
                    "BASE_TYPE_MAX_LENGTH",
                    format!("Must be {GUILD_DESCRIPTION_MAX} or fewer in length."),
                ));
            }
            Some(s) if !s.is_empty() => Some(s),
            _ => None,
        };
    }
    if let Some(s) = patch.system_channel_id {
        if s.is_some() && !system_channel_ok {
            return Err(ChatError::invalid(
                "system_channel_id",
                "CHANNEL_INVALID",
                "Must be a text channel in this server.",
            ));
        }
        out.system_channel_id = s;
    }
    if let Some(v) = patch.verification_level {
        out.verification_level = range_u8("verification_level", v, 4)?;
    }
    if let Some(v) = patch.default_message_notifications {
        out.default_message_notifications = range_u8("default_message_notifications", v, 1)?;
    }
    if let Some(v) = patch.explicit_content_filter {
        out.explicit_content_filter = range_u8("explicit_content_filter", v, 2)?;
    }
    if let Some(k) = patch.icon_key {
        out.icon_key = k;
    }
    if let Some(k) = patch.banner_key {
        out.banner_key = k;
    }
    if let Some(k) = patch.splash_key {
        out.splash_key = k;
    }
    if let Some(l) = patch.preferred_locale {
        let l = l.trim();
        if l.is_empty() || l.len() > 16 {
            return Err(ChatError::invalid(
                "preferred_locale",
                "BASE_TYPE_BAD_LENGTH",
                "Must be a locale such as en-US.",
            ));
        }
        out.preferred_locale = l.to_owned();
    }
    Ok(out)
}

/// Only the owner deletes a guild, and the Staff Server can never be deleted (doc 07 section 4).
pub fn check_delete_guild(guild: &Guild, actor: UserId) -> ChatResult<()> {
    if guild.owner_id != actor {
        return Err(ChatError::MissingPermissions);
    }
    if guild.is_staff_guild {
        return Err(ChatError::invalid(
            "guild_id",
            "STAFF_GUILD_PROTECTED",
            "The Staff Server cannot be deleted.",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Default)]
pub struct MemberPatch {
    /// `Some(None)` (or an empty string) clears the nickname.
    pub nick: Option<Option<String>>,
    pub roles: Option<Vec<RoleId>>,
    /// `Some(None)` clears the timeout.
    pub communication_disabled_until_ms: Option<Option<i64>>,
}

/// The member being edited plus what the hierarchy checks need to know about them.
pub struct MemberTarget<'a> {
    pub member: &'a Member,
    pub top_position: i32,
    pub perms: Permissions,
}

/// Apply a member patch following Discord's rules: nickname (self with
/// `CHANGE_NICKNAME`, others with `MANAGE_NICKNAMES` and outranking), roles
/// (`MANAGE_ROLES`, every changed role below the actor), timeout
/// (`MODERATE_MEMBERS`, hierarchy, no administrators).
pub fn plan_edit_member(
    guild: &Guild,
    actor_user: UserId,
    actor: &GuildActor,
    target: &MemberTarget,
    roles: &[Role],
    patch: MemberPatch,
    now_ms: i64,
) -> ChatResult<Member> {
    let mut out = target.member.clone();
    let is_self = target.member.user_id == actor_user;

    if let Some(nick) = patch.nick {
        let allowed = if is_self {
            actor.perms.has(Permissions::CHANGE_NICKNAME)
                || actor.perms.has(Permissions::MANAGE_NICKNAMES)
        } else {
            actor.perms.has(Permissions::MANAGE_NICKNAMES)
                && target.member.user_id != guild.owner_id
                && actor.outranks_position(target.top_position)
        };
        if !allowed {
            return Err(ChatError::MissingPermissions);
        }
        out.nick = match nick.map(|n| n.trim().to_owned()) {
            Some(n) if n.chars().count() > NICK_MAX => {
                return Err(ChatError::invalid(
                    "nick",
                    "BASE_TYPE_MAX_LENGTH",
                    format!("Must be {NICK_MAX} or fewer in length."),
                ));
            }
            Some(n) if !n.is_empty() => Some(n),
            _ => None,
        };
    }

    if let Some(new_roles) = patch.roles {
        let mut wanted: Vec<RoleId> = Vec::new();
        for id in new_roles {
            let role = roles
                .iter()
                .find(|r| r.id == id)
                .ok_or(ChatError::UnknownRole)?;
            if !role.is_everyone() && !wanted.contains(&id) {
                wanted.push(id);
            }
        }
        let added = wanted
            .iter()
            .filter(|r| !target.member.role_ids.contains(r));
        let removed = target
            .member
            .role_ids
            .iter()
            .filter(|r| !wanted.contains(r));
        let changed: Vec<RoleId> = added.chain(removed).copied().collect();
        if !changed.is_empty() && !actor.perms.has(Permissions::MANAGE_ROLES) {
            return Err(ChatError::MissingPermissions);
        }
        for id in changed {
            // A role deleted concurrently simply stays removed.
            if let Some(role) = roles.iter().find(|r| r.id == id) {
                check_assign_role(actor, role)?;
            }
        }
        out.role_ids = wanted;
    }

    if let Some(until) = patch.communication_disabled_until_ms {
        check_moderation(
            guild,
            ModAction::Timeout,
            actor_user,
            actor,
            target.member,
            target.top_position,
            target.perms,
        )?;
        validate_timeout_until(until, now_ms)?;
        out.communication_disabled_until_ms = until;
    }
    Ok(out)
}

/// The position/parent part of a channel edit or one entry of a bulk reorder.
#[derive(Debug, Clone, Default)]
pub struct ChannelMove {
    pub position: Option<i32>,
    /// `Some(None)` moves the channel out of its category.
    pub parent_id: Option<Option<ChannelId>>,
    /// Copy the (new) parent's overwrites onto the channel.
    pub lock_permissions: bool,
}

/// Validate and apply moves on top of the guild's channel list. Returns the
/// channels that changed. Needs `MANAGE_CHANNELS`.
pub fn plan_channel_moves(
    actor: &GuildActor,
    guild: GuildId,
    channels: &[Channel],
    moves: &[(ChannelId, ChannelMove)],
) -> ChatResult<Vec<Channel>> {
    if !actor.perms.has(Permissions::MANAGE_CHANNELS) {
        return Err(ChatError::MissingPermissions);
    }
    if moves.len() > MAX_REORDER_ENTRIES {
        return Err(ChatError::invalid(
            "channels",
            "BASE_TYPE_MAX_LENGTH",
            "Too many entries.",
        ));
    }
    let mut changed: Vec<Channel> = Vec::new();
    for (id, mv) in moves {
        let current = changed
            .iter()
            .find(|c| c.id == *id)
            .or_else(|| channels.iter().find(|c| c.id == *id))
            .ok_or(ChatError::UnknownChannel)?;
        if current.guild_id != Some(guild) {
            return Err(ChatError::UnknownChannel);
        }
        let mut next = current.clone();
        if let Some(p) = mv.position {
            if p < 0 {
                return Err(ChatError::invalid(
                    "position",
                    "NUMBER_TYPE_MIN",
                    "Must be 0 or greater.",
                ));
            }
            next.position = p;
        }
        if let Some(parent) = mv.parent_id {
            if let Some(pid) = parent {
                let ok = next.kind != ChannelType::Category
                    && channels.iter().any(|c| {
                        c.id == pid && c.kind == ChannelType::Category && c.guild_id == Some(guild)
                    });
                if !ok {
                    return Err(ChatError::invalid(
                        "parent_id",
                        "CHANNEL_PARENT_INVALID",
                        "Parent must be a category in this server.",
                    ));
                }
            }
            next.parent_id = parent;
        }
        changed.retain(|c| c.id != *id);
        changed.push(next);
    }
    Ok(changed)
}

/// Reorder roles. Each entry moves a role to `position` (>= 1), shifting others;
/// the actor must outrank both the old and new position. Returns roles whose
/// position changed, with positions renumbered 1..n above `@everyone`.
pub fn plan_role_order(
    actor: &GuildActor,
    roles: &[Role],
    moves: &[(RoleId, i32)],
) -> ChatResult<Vec<Role>> {
    if moves.len() > MAX_REORDER_ENTRIES {
        return Err(ChatError::invalid(
            "roles",
            "BASE_TYPE_MAX_LENGTH",
            "Too many entries.",
        ));
    }
    let mut order: Vec<Role> = roles.iter().filter(|r| !r.is_everyone()).cloned().collect();
    order.sort_by_key(|r| (r.position, r.id));
    for (id, pos) in moves {
        let original = roles
            .iter()
            .find(|r| r.id == *id)
            .ok_or(ChatError::UnknownRole)?;
        check_reposition(actor, original, *pos)?;
        let idx = order
            .iter()
            .position(|r| r.id == *id)
            .ok_or(ChatError::UnknownRole)?;
        let role = order.remove(idx);
        let at = usize::try_from(*pos - 1).unwrap_or(0).min(order.len());
        order.insert(at, role);
    }
    let mut changed = Vec::new();
    for (i, r) in order.iter_mut().enumerate() {
        let new_pos = i32::try_from(i + 1).unwrap_or(i32::MAX);
        if r.position != new_pos {
            r.position = new_pos;
            changed.push(r.clone());
        }
    }
    Ok(changed)
}

/// Pre-emptive ban of someone who is not a member: only the permission is
/// needed, and the owner and the actor stay untouchable.
pub fn check_ban_non_member(
    guild: &Guild,
    actor_user: UserId,
    actor: &GuildActor,
    target: UserId,
) -> ChatResult<()> {
    if !actor.perms.has(Permissions::BAN_MEMBERS)
        || target == actor_user
        || target == guild.owner_id
    {
        return Err(ChatError::MissingPermissions);
    }
    Ok(())
}

/// Trim a ban reason; empty becomes `None`.
pub fn validate_ban_reason(reason: Option<String>) -> ChatResult<Option<String>> {
    match reason.map(|r| r.trim().to_owned()) {
        Some(r) if r.chars().count() > BAN_REASON_MAX => Err(ChatError::invalid(
            "reason",
            "BASE_TYPE_MAX_LENGTH",
            format!("Must be {BAN_REASON_MAX} or fewer in length."),
        )),
        Some(r) if !r.is_empty() => Ok(Some(r)),
        _ => Ok(None),
    }
}

/// Longest member search text.
pub const MEMBER_QUERY_MAX: usize = 64;
/// Prune window in days (Discord: 1 to 30).
pub const PRUNE_DAYS_MAX: u32 = 30;
/// Most members one prune call removes (and one preview counts). A larger backlog takes
/// several calls.
pub const MAX_PRUNE_BATCH: usize = 10_000;

/// Trim the search text and check that a role filter names a role of this guild.
pub fn validate_member_filter(mut f: MemberFilter, roles: &[Role]) -> ChatResult<MemberFilter> {
    if let Some(q) = f.query.take() {
        let q = q.trim();
        if q.chars().count() > MEMBER_QUERY_MAX {
            return Err(ChatError::invalid(
                "query",
                "BASE_TYPE_MAX_LENGTH",
                format!("Must be {MEMBER_QUERY_MAX} or fewer in length."),
            ));
        }
        f.query = (!q.is_empty()).then(|| q.to_owned());
    }
    if let Some(r) = f.role
        && !roles.iter().any(|x| x.id == r)
    {
        return Err(ChatError::UnknownRole);
    }
    Ok(f)
}

/// The selection criteria of a prune of `days` days now (`now_ms`). `include_roles` must be
/// roles of the guild and not `@everyone`.
pub fn plan_prune(
    days: u32,
    include_roles: &[RoleId],
    roles: &[Role],
    now_ms: i64,
) -> ChatResult<PruneCriteria> {
    if !(1..=PRUNE_DAYS_MAX).contains(&days) {
        return Err(ChatError::invalid(
            "days",
            "NUMBER_TYPE_MAX",
            format!("Must be between 1 and {PRUNE_DAYS_MAX}."),
        ));
    }
    for r in include_roles {
        if !roles.iter().any(|x| x.id == *r && !x.is_everyone()) {
            return Err(ChatError::UnknownRole);
        }
    }
    let cutoff = now_ms.saturating_sub(i64::from(days) * 86_400_000);
    Ok(PruneCriteria {
        joined_before_ms: cutoff,
        active_since_ms: cutoff,
        include_roles: include_roles.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perms::fixtures::*;

    fn actor_with(perms: Permissions, top: i32) -> GuildActor {
        GuildActor {
            is_owner: false,
            perms,
            top_position: top,
        }
    }

    #[test]
    fn guild_edit_rules() {
        let g = guild();
        let manager = actor_with(Permissions::MANAGE_GUILD, 1);
        let out = plan_edit_guild(
            UserId::new(5),
            &manager,
            &g,
            GuildPatch {
                name: Some("  New Name ".into()),
                verification_level: Some(3),
                ..Default::default()
            },
            None,
            false,
        )
        .unwrap();
        assert_eq!((out.name.as_str(), out.verification_level), ("New Name", 3));
        let plain = actor_with(Permissions::empty(), 0);
        let rename = GuildPatch {
            name: Some("zzz".into()),
            ..Default::default()
        };
        assert!(plan_edit_guild(UserId::new(5), &plain, &g, rename, None, false).is_err());
        let bad = GuildPatch {
            verification_level: Some(9),
            ..Default::default()
        };
        assert!(plan_edit_guild(UserId::new(5), &manager, &g, bad, None, false).is_err());
        // Ownership: only the owner, and only to a member.
        let owner = GuildActor {
            is_owner: true,
            perms: Permissions::ALL,
            top_position: 0,
        };
        let to = member(5, &[]);
        let t = GuildPatch {
            owner_id: Some(to.user_id),
            ..Default::default()
        };
        let out =
            plan_edit_guild(UserId::new(OWNER), &owner, &g, t.clone(), Some(&to), false).unwrap();
        assert_eq!(out.owner_id, to.user_id);
        assert!(plan_edit_guild(UserId::new(OWNER), &owner, &g, t.clone(), None, false).is_err());
        assert!(plan_edit_guild(UserId::new(9), &manager, &g, t, Some(&to), false).is_err());
    }

    #[test]
    fn delete_guild_rules() {
        let mut g = guild();
        assert!(check_delete_guild(&g, UserId::new(OWNER)).is_ok());
        assert!(check_delete_guild(&g, UserId::new(5)).is_err());
        g.is_staff_guild = true;
        assert!(check_delete_guild(&g, UserId::new(OWNER)).is_err());
    }

    #[test]
    fn member_edit_rules() {
        let g = guild();
        let roles = [
            everyone(),
            role(
                200,
                5,
                Permissions::MANAGE_ROLES | Permissions::MANAGE_NICKNAMES,
            ),
            role(201, 2, Permissions::empty()),
            role(202, 8, Permissions::empty()),
        ];
        let actor_m = member(10, &[200]);
        let actor = GuildActor::new(&g, &actor_m, &roles);
        let target_m = member(11, &[]);
        let target = MemberTarget {
            member: &target_m,
            top_position: 0,
            perms: Permissions::empty(),
        };
        let out = plan_edit_member(
            &g,
            actor_m.user_id,
            &actor,
            &target,
            &roles,
            MemberPatch {
                nick: Some(Some(" Bob ".into())),
                roles: Some(vec![RoleId::new(201)]),
                ..Default::default()
            },
            0,
        )
        .unwrap();
        assert_eq!(out.nick.as_deref(), Some("Bob"));
        assert_eq!(out.role_ids, vec![RoleId::new(201)]);
        let high = MemberPatch {
            roles: Some(vec![RoleId::new(202)]),
            ..Default::default()
        };
        assert!(plan_edit_member(&g, actor_m.user_id, &actor, &target, &roles, high, 0).is_err());
        let unk = MemberPatch {
            roles: Some(vec![RoleId::new(999)]),
            ..Default::default()
        };
        assert!(matches!(
            plan_edit_member(&g, actor_m.user_id, &actor, &target, &roles, unk, 0),
            Err(ChatError::UnknownRole)
        ));
        let t = MemberPatch {
            communication_disabled_until_ms: Some(Some(1000)),
            ..Default::default()
        };
        assert!(plan_edit_member(&g, actor_m.user_id, &actor, &target, &roles, t, 0).is_err());
    }

    #[test]
    fn channel_moves_validate_parents() {
        let a = actor_with(Permissions::MANAGE_CHANNELS, 1);
        let gid = GuildId::new(GUILD);
        let mk = |id: u64, kind| Channel::new(ChannelId::new(id), Some(gid), kind);
        let chans = vec![
            mk(1, ChannelType::Category),
            mk(2, ChannelType::Text),
            mk(3, ChannelType::Category),
        ];
        let mv = ChannelMove {
            position: Some(4),
            parent_id: Some(Some(ChannelId::new(1))),
            ..Default::default()
        };
        let out = plan_channel_moves(&a, gid, &chans, &[(ChannelId::new(2), mv.clone())]).unwrap();
        assert_eq!(
            (out[0].position, out[0].parent_id),
            (4, Some(ChannelId::new(1)))
        );
        // Categories cannot be nested; parents must be categories.
        assert!(plan_channel_moves(&a, gid, &chans, &[(ChannelId::new(3), mv.clone())]).is_err());
        let bad_parent = ChannelMove {
            parent_id: Some(Some(ChannelId::new(2))),
            ..Default::default()
        };
        assert!(plan_channel_moves(&a, gid, &chans, &[(ChannelId::new(2), bad_parent)]).is_err());
        assert!(plan_channel_moves(&a, gid, &chans, &[(ChannelId::new(77), mv)]).is_err());
        let none = actor_with(Permissions::empty(), 0);
        assert!(plan_channel_moves(&none, gid, &chans, &[]).is_err());
    }

    #[test]
    fn role_reorder_renumbers() {
        let roles = [
            everyone(),
            role(200, 1, Permissions::empty()),
            role(201, 2, Permissions::empty()),
            role(202, 3, Permissions::empty()),
        ];
        let a = actor_with(Permissions::MANAGE_ROLES, 10);
        let out = plan_role_order(&a, &roles, &[(RoleId::new(202), 1)]).unwrap();
        let pos = |id: u64| out.iter().find(|r| r.id.get() == id).map(|r| r.position);
        assert_eq!(pos(202), Some(1));
        assert_eq!(pos(200), Some(2));
        assert_eq!(pos(201), Some(3));
        let low = actor_with(Permissions::MANAGE_ROLES, 2);
        assert!(plan_role_order(&low, &roles, &[(RoleId::new(200), 3)]).is_err());
        assert!(plan_role_order(&a, &roles, &[(RoleId::new(GUILD), 1)]).is_err());
    }

    #[test]
    fn ban_helpers() {
        let g = guild();
        let a = actor_with(Permissions::BAN_MEMBERS, 1);
        assert!(check_ban_non_member(&g, UserId::new(5), &a, UserId::new(9)).is_ok());
        assert!(check_ban_non_member(&g, UserId::new(5), &a, UserId::new(OWNER)).is_err());
        assert!(check_ban_non_member(&g, UserId::new(5), &a, UserId::new(5)).is_err());
        assert_eq!(validate_ban_reason(Some("  ".into())).unwrap(), None);
        assert!(validate_ban_reason(Some("x".repeat(513))).is_err());
    }
    #[test]
    fn member_filter_and_prune_plan_validate() {
        let roles = [everyone(), role(200, 1, Permissions::empty())];
        let f = validate_member_filter(
            MemberFilter {
                query: Some("  Ann ".into()),
                role: Some(RoleId::new(200)),
            },
            &roles,
        )
        .unwrap();
        assert_eq!(f.query.as_deref(), Some("Ann"));
        let blank = validate_member_filter(
            MemberFilter {
                query: Some("   ".into()),
                role: None,
            },
            &roles,
        )
        .unwrap();
        assert!(blank.is_empty());
        let too_long = MemberFilter {
            query: Some("x".repeat(MEMBER_QUERY_MAX + 1)),
            role: None,
        };
        assert!(validate_member_filter(too_long, &roles).is_err());
        let unknown = MemberFilter {
            query: None,
            role: Some(RoleId::new(999)),
        };
        assert!(matches!(
            validate_member_filter(unknown, &roles),
            Err(ChatError::UnknownRole)
        ));

        let now = 40 * 86_400_000;
        let p = plan_prune(7, &[RoleId::new(200)], &roles, now).unwrap();
        assert_eq!(p.joined_before_ms, now - 7 * 86_400_000);
        assert_eq!(p.active_since_ms, p.joined_before_ms);
        for bad in [0, 31] {
            assert!(plan_prune(bad, &[], &roles, now).is_err());
        }
        assert!(plan_prune(7, &[RoleId::new(999)], &roles, now).is_err());
        // @everyone cannot be "included": it is not a real assignment.
        assert!(plan_prune(7, &[RoleId::new(GUILD)], &roles, now).is_err());
    }
}
