//! Effective permission computation on top of `sr_core::permissions`
//! (docs/07 section 1) plus role-hierarchy helpers.

use crate::domain::{Guild, Member, Role};
use sr_core::permissions::{Overwrite, Permissions, compute_base, compute_overwrites};

/// Default `@everyone` permissions for a new guild (Discord's defaults).
pub fn default_everyone_permissions() -> Permissions {
    use Permissions as P;
    P::CREATE_INSTANT_INVITE
        | P::ADD_REACTIONS
        | P::STREAM
        | P::VIEW_CHANNEL
        | P::SEND_MESSAGES
        | P::SEND_TTS_MESSAGES
        | P::EMBED_LINKS
        | P::USE_LIBRARY_EMBEDS
        | P::ATTACH_FILES
        | P::READ_MESSAGE_HISTORY
        | P::USE_EXTERNAL_EMOJIS
        | P::CONNECT
        | P::SPEAK
        | P::USE_VAD
        | P::CHANGE_NICKNAME
        | P::USE_APPLICATION_COMMANDS
        | P::REQUEST_TO_SPEAK
        | P::CREATE_PUBLIC_THREADS
        | P::USE_EXTERNAL_STICKERS
        | P::SEND_MESSAGES_IN_THREADS
        | P::USE_EMBEDDED_ACTIVITIES
        | P::USE_SOUNDBOARD
        | P::USE_EXTERNAL_SOUNDS
        | P::SET_VOICE_CHANNEL_STATUS
        | P::CREATE_EVENTS
        | P::SEND_VOICE_MESSAGES
        | P::SEND_POLLS
        | P::USE_EXTERNAL_APPS
        | P::START_WATCH_PARTY
}

/// Permissions of each participant in a DM or group DM.
pub fn dm_permissions() -> Permissions {
    use Permissions as P;
    P::VIEW_CHANNEL
        | P::SEND_MESSAGES
        | P::EMBED_LINKS
        | P::USE_LIBRARY_EMBEDS
        | P::ATTACH_FILES
        | P::READ_MESSAGE_HISTORY
        | P::ADD_REACTIONS
        | P::USE_EXTERNAL_EMOJIS
        | P::USE_EXTERNAL_STICKERS
        | P::SEND_VOICE_MESSAGES
        | P::SEND_POLLS
        | P::CONNECT
        | P::SPEAK
        | P::STREAM
        | P::USE_VAD
}

fn member_roles<'a>(member: &'a Member, roles: &'a [Role]) -> impl Iterator<Item = &'a Role> {
    roles
        .iter()
        .filter(move |r| !r.is_everyone() && member.role_ids.contains(&r.id))
}

fn everyone_perms(guild: &Guild, roles: &[Role]) -> Permissions {
    roles
        .iter()
        .find(|r| r.id.get() == guild.id.get())
        .map_or_else(default_everyone_permissions, |r| r.permissions)
}

/// Guild-level permissions (no channel overwrites): owner and administrator bypass.
pub fn guild_permissions(guild: &Guild, member: &Member, roles: &[Role]) -> Permissions {
    compute_base(
        guild.owner_id == member.user_id,
        everyone_perms(guild, roles),
        member_roles(member, roles).map(|r| r.permissions),
    )
}

/// Effective permissions in a channel. Applies overwrites, drops everything when
/// `VIEW_CHANNEL` is missing, and restricts timed-out members to read-only access.
pub fn channel_permissions(
    guild: &Guild,
    member: &Member,
    roles: &[Role],
    overwrites: &[Overwrite],
    now_ms: i64,
) -> Permissions {
    let base = guild_permissions(guild, member, roles);
    if base == Permissions::ALL {
        // Owner or administrator: bypasses overwrites and timeouts.
        return base;
    }
    let role_ids: Vec<_> = member_roles(member, roles)
        .map(|r| r.id.snowflake())
        .collect();
    let mut perms = compute_overwrites(
        base,
        guild.id.snowflake(),
        member.user_id.snowflake(),
        &role_ids,
        overwrites,
    );
    if !perms.contains(Permissions::VIEW_CHANNEL) {
        return Permissions::empty();
    }
    if member.is_timed_out(now_ms) || member.pending {
        // Timed out, or still behind the membership screening (LNG-GLD-008): read-only.
        perms &= Permissions::VIEW_CHANNEL | Permissions::READ_MESSAGE_HISTORY;
    }
    perms
}

/// Highest role position held by the member (0 = only `@everyone`).
pub fn top_role_position(member: &Member, roles: &[Role]) -> i32 {
    member_roles(member, roles)
        .map(|r| r.position)
        .max()
        .unwrap_or(0)
}

/// Who is acting on a guild, for hierarchy decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuildActor {
    pub is_owner: bool,
    /// Guild-level permissions (owner/admin already expanded to ALL).
    pub perms: Permissions,
    pub top_position: i32,
}

impl GuildActor {
    pub fn new(guild: &Guild, member: &Member, roles: &[Role]) -> Self {
        Self {
            is_owner: guild.owner_id == member.user_id,
            perms: guild_permissions(guild, member, roles),
            top_position: top_role_position(member, roles),
        }
    }

    /// Owners outrank every role. Everyone else must be strictly above `position`.
    pub fn outranks_position(&self, position: i32) -> bool {
        self.is_owner || self.top_position > position
    }

    /// Whether the actor may grant or deny exactly these bits (Discord: only bits
    /// you hold yourself; owners and administrators may grant anything).
    pub fn can_grant(&self, bits: Permissions) -> bool {
        self.is_owner
            || self.perms.contains(Permissions::ADMINISTRATOR)
            || self.perms.contains(bits)
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;
    use sr_core::{GuildId, RoleId, UserId};

    pub const GUILD: u64 = 100;
    pub const OWNER: u64 = 1;

    pub fn guild() -> Guild {
        Guild {
            id: GuildId::new(GUILD),
            name: "Test".into(),
            description: None,
            icon_key: None,
            banner_key: None,
            splash_key: None,
            vanity_code: None,
            mfa_level: 0,
            system_channel_flags: 0,
            widget_enabled: false,
            widget_channel_id: None,
            rules_channel_id: None,
            public_updates_channel_id: None,
            safety_alerts_channel_id: None,
            hide_suspicious_links: false,
            owner_id: UserId::new(OWNER),
            is_staff_guild: false,
            system_channel_id: None,
            afk_channel_id: None,
            afk_timeout: crate::admin_rules::DEFAULT_AFK_TIMEOUT,
            verification_level: 0,
            default_message_notifications: 0,
            explicit_content_filter: 0,
            preferred_locale: "en-US".into(),
        }
    }

    pub fn role(id: u64, position: i32, permissions: Permissions) -> Role {
        Role {
            id: RoleId::new(id),
            guild_id: GuildId::new(GUILD),
            name: format!("r{id}"),
            color: 0,
            hoist: false,
            position,
            permissions,
            mentionable: false,
            managed: false,
            unicode_emoji: None,
            icon_key: None,
        }
    }

    pub fn everyone() -> Role {
        let mut r = role(GUILD, 0, default_everyone_permissions());
        r.name = "@everyone".into();
        r
    }

    pub fn member(user: u64, roles: &[u64]) -> Member {
        Member {
            guild_id: GuildId::new(GUILD),
            user_id: UserId::new(user),
            nick: None,
            role_ids: roles.iter().map(|r| RoleId::new(*r)).collect(),
            joined_at_ms: 0,
            communication_disabled_until_ms: None,
            pending: false,
            avatar_key: None,
            banner_key: None,
            bio: None,
            extras: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;
    use sr_core::Snowflake;
    use sr_core::permissions::OverwriteKind;

    fn ow(id: u64, kind: OverwriteKind, allow: Permissions, deny: Permissions) -> Overwrite {
        Overwrite {
            id: Snowflake::new(id),
            kind,
            allow,
            deny,
        }
    }

    #[test]
    fn everyone_defaults_apply_without_roles() {
        let p = channel_permissions(&guild(), &member(5, &[]), &[everyone()], &[], 0);
        assert!(p.contains(Permissions::SEND_MESSAGES | Permissions::VIEW_CHANNEL));
        assert!(!p.contains(Permissions::MANAGE_MESSAGES));
    }

    #[test]
    fn owner_and_admin_bypass() {
        let roles = [everyone(), role(200, 1, Permissions::ADMINISTRATOR)];
        let deny_all = [ow(
            GUILD,
            OverwriteKind::Role,
            Permissions::empty(),
            Permissions::ALL,
        )];
        assert_eq!(
            channel_permissions(&guild(), &member(OWNER, &[]), &roles, &deny_all, 0),
            Permissions::ALL
        );
        assert_eq!(
            channel_permissions(&guild(), &member(5, &[200]), &roles, &deny_all, 0),
            Permissions::ALL
        );
    }

    #[test]
    fn overwrites_deny_view_removes_everything() {
        let deny = [ow(
            GUILD,
            OverwriteKind::Role,
            Permissions::empty(),
            Permissions::VIEW_CHANNEL,
        )];
        let p = channel_permissions(&guild(), &member(5, &[]), &[everyone()], &deny, 0);
        assert!(p.is_empty());
    }

    #[test]
    fn member_overwrite_beats_role_overwrite() {
        let roles = [everyone(), role(200, 1, Permissions::empty())];
        let ows = [
            ow(
                200,
                OverwriteKind::Role,
                Permissions::empty(),
                Permissions::SEND_MESSAGES,
            ),
            ow(
                5,
                OverwriteKind::Member,
                Permissions::SEND_MESSAGES,
                Permissions::empty(),
            ),
        ];
        let p = channel_permissions(&guild(), &member(5, &[200]), &roles, &ows, 0);
        assert!(p.contains(Permissions::SEND_MESSAGES));
        let p2 = channel_permissions(&guild(), &member(6, &[200]), &roles, &ows, 0);
        assert!(!p2.contains(Permissions::SEND_MESSAGES));
    }

    #[test]
    fn timeout_limits_to_read_only() {
        let mut m = member(5, &[]);
        m.communication_disabled_until_ms = Some(2_000);
        let during = channel_permissions(&guild(), &m, &[everyone()], &[], 1_000);
        assert_eq!(
            during,
            Permissions::VIEW_CHANNEL | Permissions::READ_MESSAGE_HISTORY
        );
        let after = channel_permissions(&guild(), &m, &[everyone()], &[], 3_000);
        assert!(after.contains(Permissions::SEND_MESSAGES));
    }

    #[test]
    fn pending_screening_is_read_only_but_not_for_the_owner() {
        let mut m = member(5, &[]);
        m.pending = true;
        let p = channel_permissions(&guild(), &m, &[everyone()], &[], 0);
        assert_eq!(
            p,
            Permissions::VIEW_CHANNEL | Permissions::READ_MESSAGE_HISTORY
        );
        // Accepting the rules restores the ordinary permissions.
        m.pending = false;
        let p = channel_permissions(&guild(), &m, &[everyone()], &[], 0);
        assert!(p.contains(Permissions::SEND_MESSAGES));
        // The owner is never held back (they cannot be pending, and the bypass comes first).
        let mut owner = member(OWNER, &[]);
        owner.pending = true;
        assert_eq!(
            channel_permissions(&guild(), &owner, &[everyone()], &[], 0),
            Permissions::ALL
        );
    }

    #[test]
    fn hierarchy_helpers() {
        let roles = [
            everyone(),
            role(200, 3, Permissions::MANAGE_ROLES),
            role(201, 5, Permissions::empty()),
        ];
        let a = GuildActor::new(&guild(), &member(5, &[200]), &roles);
        assert_eq!(a.top_position, 3);
        assert!(a.outranks_position(2));
        assert!(!a.outranks_position(3));
        assert!(a.can_grant(Permissions::MANAGE_ROLES));
        assert!(!a.can_grant(Permissions::KICK_MEMBERS));
        let owner = GuildActor::new(&guild(), &member(OWNER, &[]), &roles);
        assert!(owner.outranks_position(1000));
        assert!(owner.can_grant(Permissions::ADMINISTRATOR));
    }
}
