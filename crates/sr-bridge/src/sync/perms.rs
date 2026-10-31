//! Role permissions across the bridge (BRG-SYN-002; design sections 2 to 6).
//!
//! SIDE ROOM is the master copy. A role's permission word goes SIDE ROOM to Discord only:
//! `discord = (sr & BRIDGE_SHARED) | discord_only_bits`. SIDE ROOM-only bits (56 to 62) are never sent and never
//! cleared; the Discord-only bits (47, 53 to 55) are kept raw next to the map and preserved on every push.
//!
//! There is deliberately no function here that returns a SIDE ROOM permission word computed from a Discord word,
//! except [`owner_confirmed_import`], which only the initial-sync plan calls, on the owner's explicit confirmation.

use sr_core::permissions::{ClassifiedBits, Overwrite, OverwriteKind, Permissions};
use sr_core::snowflake::Snowflake;

/// The bits the plan shows in red when they would be imported from Discord (design section 9.4, Q4).
pub const DANGEROUS: Permissions = Permissions::ADMINISTRATOR
    .union(Permissions::MANAGE_GUILD)
    .union(Permissions::MANAGE_ROLES)
    .union(Permissions::MANAGE_CHANNELS)
    .union(Permissions::MANAGE_WEBHOOKS)
    .union(Permissions::BAN_MEMBERS)
    .union(Permissions::KICK_MEMBERS)
    .union(Permissions::MODERATE_MEMBERS)
    .union(Permissions::MENTION_EVERYONE);

/// The word to put on Discord for a SIDE ROOM role: its shared bits plus the Discord-only bits kept for it.
pub const fn push_word(sr: Permissions, preserved_discord_only: u64) -> u64 {
    ClassifiedBits::to_discord(sr, preserved_discord_only)
}

/// How a role's Discord word compares with SIDE ROOM's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleDrift {
    /// SIDE ROOM's shared bits.
    pub sr_shared: Permissions,
    /// The shared bits Discord has now.
    pub discord_shared: Permissions,
    /// The Discord-only bits Discord has now: Discord's alone, kept as they are.
    pub discord_only: u64,
}

impl RoleDrift {
    /// Discord's shared bits differ from SIDE ROOM's: they are put back.
    pub fn needs_revert(&self) -> bool {
        self.discord_shared != self.sr_shared
    }

    /// Shared bits Discord has that SIDE ROOM does not (the part a proposal would grant).
    pub fn discord_added(&self) -> Permissions {
        self.discord_shared & !self.sr_shared
    }

    /// Shared bits SIDE ROOM has that Discord lost.
    pub fn discord_removed(&self) -> Permissions {
        self.sr_shared & !self.discord_shared
    }
}

/// Compare a role's Discord word with the SIDE ROOM role's.
pub fn compare_role(sr: Permissions, discord_raw: u64) -> RoleDrift {
    let d = ClassifiedBits::from_discord(discord_raw);
    RoleDrift {
        sr_shared: sr & Permissions::BRIDGE_SHARED,
        discord_shared: d.shared,
        discord_only: d.discord_only,
    }
}

/// A role Discord created, as it is created here: no permissions at all, directly above `@everyone`. What the
/// Discord creator set is kept as a proposal, never applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleFromDiscord {
    /// Always empty.
    pub permissions: Permissions,
    /// Always 1.
    pub position: i32,
    /// The shared bits Discord's copy had, for the proposal row.
    pub proposed: Permissions,
    /// What to keep on Discord's copy besides the shared bits.
    pub discord_only: u64,
}

pub fn role_from_discord(discord_raw: u64) -> RoleFromDiscord {
    let d = ClassifiedBits::from_discord(discord_raw);
    RoleFromDiscord {
        permissions: Permissions::empty(),
        position: 1,
        proposed: d.shared,
        discord_only: d.discord_only,
    }
}

/// The Discord permissions of a role that exists only on Discord, imported into a new SIDE ROOM role at the
/// initial sync. Only the owner's confirmation of the plan may call this (the owner is the one granting; the
/// audit row says so). Returns the shared bits only: SIDE ROOM-only bits cannot come from Discord.
pub fn owner_confirmed_import(discord_raw: u64) -> Permissions {
    ClassifiedBits::from_discord(discord_raw).shared
}

/// The part of an import the plan marks red.
pub fn dangerous_in(p: Permissions) -> Permissions {
    p & DANGEROUS
}

/// What deleting a role on Discord would do here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeletionVerdict {
    /// No overwrite denies anything to this role: deleting it can only remove access. Mirror it.
    Mirror,
    /// Some overwrite denies something to this role (a "Muted" role): deleting it would give its holders access
    /// back. Recreate the role on Discord instead; the channels (SIDE ROOM ids) are named in the conflict row.
    Refuse { channels: Vec<i64> },
}

/// The guard of design section 2.3. `overwrites` are the role overwrites of the server's channels, as
/// `(channel id, overwrite)`. `@everyone` (id == guild id) is never deletable.
pub fn role_deletion_verdict<'a>(
    guild_id: Snowflake,
    role: Snowflake,
    overwrites: impl IntoIterator<Item = (i64, &'a Overwrite)>,
) -> DeletionVerdict {
    if role == guild_id {
        return DeletionVerdict::Refuse {
            channels: Vec::new(),
        };
    }
    let mut channels: Vec<i64> = overwrites
        .into_iter()
        .filter(|(_, o)| o.kind == OverwriteKind::Role && o.id == role && !o.deny.is_empty())
        .map(|(c, _)| c)
        .collect();
    channels.sort_unstable();
    channels.dedup();
    if channels.is_empty() {
        DeletionVerdict::Mirror
    } else {
        DeletionVerdict::Refuse { channels }
    }
}

/// The order SIDE ROOM wants on Discord: the mapped roles by SIDE ROOM position, as `(discord role id, position)`.
/// Unmapped Discord roles (managed roles, the bot's own) are not in it.
pub fn desired_order(
    sr_roles: &[(Snowflake, i32)],
    to_discord: impl Fn(Snowflake) -> Option<u64>,
) -> Vec<(u64, i32)> {
    let mut v: Vec<(u64, i32)> = sr_roles
        .iter()
        .filter_map(|(id, pos)| to_discord(*id).map(|d| (d, *pos)))
        .collect();
    v.sort_by_key(|(d, pos)| (*pos, *d));
    v
}

/// Whether Discord's relative order of the mapped roles differs from `desired` (a Discord-side reorder, which is
/// put back and never applied here, design section 2.3). `discord` is `(discord role id, position)` for every
/// role Discord has; only the mapped ones are looked at.
pub fn order_drifted(desired: &[(u64, i32)], discord: &[(u64, i32)]) -> bool {
    let mut seen: Vec<(u64, i32)> = discord
        .iter()
        .filter(|(d, _)| desired.iter().any(|(x, _)| x == d))
        .copied()
        .collect();
    seen.sort_by_key(|(d, pos)| (*pos, *d));
    let a: Vec<u64> = desired.iter().map(|(d, _)| *d).collect();
    let b: Vec<u64> = seen.iter().map(|(d, _)| *d).collect();
    a != b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sf(n: u64) -> Snowflake {
        Snowflake::new(n)
    }

    #[test]
    fn a_push_never_carries_a_sideroom_only_bit_and_keeps_discord_only_bits() {
        let sr = Permissions::SEND_MESSAGES
            | Permissions::START_WATCH_PARTY
            | Permissions::MANAGE_EXTENSIONS;
        let w = push_word(sr, 1 << 47);
        assert_eq!(w, Permissions::SEND_MESSAGES.bits() | (1 << 47));
        assert_eq!(w & Permissions::SR_ONLY.bits(), 0);
    }

    #[test]
    fn drift_ignores_discord_only_bits_and_flags_shared_ones() {
        let sr = Permissions::SEND_MESSAGES | Permissions::START_WATCH_PARTY;
        let d = compare_role(sr, Permissions::SEND_MESSAGES.bits() | (1 << 47));
        assert!(!d.needs_revert());
        assert_eq!(d.discord_only, 1 << 47);
        let d = compare_role(sr, Permissions::BAN_MEMBERS.bits());
        assert!(d.needs_revert());
        assert_eq!(d.discord_added(), Permissions::BAN_MEMBERS);
        assert_eq!(d.discord_removed(), Permissions::SEND_MESSAGES);
    }

    #[test]
    fn a_hostile_word_with_sideroom_bits_cannot_name_one() {
        let d = compare_role(Permissions::empty(), u64::MAX);
        assert!(!d.discord_shared.intersects(Permissions::SR_ONLY));
        assert_eq!(owner_confirmed_import(u64::MAX), Permissions::BRIDGE_SHARED);
    }

    #[test]
    fn a_role_from_discord_has_no_permissions() {
        let r = role_from_discord(Permissions::ADMINISTRATOR.bits() | (1 << 53));
        assert!(r.permissions.is_empty());
        assert_eq!(r.position, 1);
        assert_eq!(r.proposed, Permissions::ADMINISTRATOR);
        assert_eq!(r.discord_only, 1 << 53);
    }

    #[test]
    fn dangerous_bits_are_named() {
        let p = Permissions::SEND_MESSAGES | Permissions::BAN_MEMBERS | Permissions::ADMINISTRATOR;
        assert_eq!(
            dangerous_in(p),
            Permissions::BAN_MEMBERS | Permissions::ADMINISTRATOR
        );
    }

    #[test]
    fn a_role_that_denies_something_is_not_deleted_from_discord() {
        let muted = sf(50);
        let deny = Overwrite {
            id: muted,
            kind: OverwriteKind::Role,
            allow: Permissions::empty(),
            deny: Permissions::SEND_MESSAGES,
        };
        let allow_only = Overwrite {
            id: muted,
            kind: OverwriteKind::Role,
            allow: Permissions::SEND_MESSAGES,
            deny: Permissions::empty(),
        };
        let other = Overwrite {
            id: sf(51),
            kind: OverwriteKind::Role,
            allow: Permissions::empty(),
            deny: Permissions::VIEW_CHANNEL,
        };
        assert_eq!(
            role_deletion_verdict(sf(1), muted, [(7, &allow_only), (8, &other)]),
            DeletionVerdict::Mirror
        );
        assert_eq!(
            role_deletion_verdict(sf(1), muted, [(9, &deny), (7, &allow_only), (3, &deny)]),
            DeletionVerdict::Refuse {
                channels: vec![3, 9]
            }
        );
        let member = Overwrite {
            kind: OverwriteKind::Member,
            ..deny
        };
        assert_eq!(
            role_deletion_verdict(sf(1), muted, [(9, &member)]),
            DeletionVerdict::Mirror
        );
        assert!(matches!(
            role_deletion_verdict(sf(1), sf(1), []),
            DeletionVerdict::Refuse { .. }
        ));
    }

    #[test]
    fn a_discord_reorder_is_seen_and_the_sideroom_order_is_wanted() {
        let sr = [(sf(10), 1), (sf(11), 2), (sf(12), 3)];
        let map = |id: Snowflake| match id.get() {
            10 => Some(110),
            11 => Some(111),
            12 => Some(112),
            _ => None,
        };
        let want = desired_order(&sr, map);
        assert_eq!(want, vec![(110, 1), (111, 2), (112, 3)]);
        let ok = [(110, 1), (900, 2), (111, 3), (112, 4), (901, 5)];
        assert!(!order_drifted(&want, &ok));
        let swapped = [(111, 1), (110, 2), (112, 3)];
        assert!(order_drifted(&want, &swapped));
    }
}
