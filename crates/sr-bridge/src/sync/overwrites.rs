//! Channel permission overwrites across the bridge (BRG-SYN-002; design sections 4 and 5).
//!
//! SIDE ROOM to Discord only. For a channel, [`plan_push`] computes the full overwrite array Discord should have
//! (one `PATCH /channels/{id}` carries it), from SIDE ROOM's overwrites, Discord's current ones and the role map:
//!
//! * a SIDE ROOM role overwrite whose role is mapped is sent: `(sr & BRIDGE_SHARED) | the Discord-only bits
//!   Discord has on that overwrite`. One that carries only SIDE ROOM-only bits produces none;
//! * SIDE ROOM member overwrites have no Discord target: never sent;
//! * Discord member overwrites and overwrites of unmapped (managed) Discord roles keep their deny bits and lose
//!   their allow bits in the shared mask ("ceilings only shrink", design 5.4, Q3). The bot's own member and
//!   managed role are never touched;
//! * a Discord overwrite for a mapped role that SIDE ROOM does not have is removed (SIDE ROOM is the master),
//!   keeping only its Discord-only bits.

use sr_core::permissions::{
    ClassifiedBits, DISCORD_ONLY_MASK, Overwrite, OverwriteKind, Permissions,
};
use sr_core::snowflake::Snowflake;

/// An overwrite as Discord has it: raw words (they may carry Discord-only bits).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscordOverwrite {
    pub id: u64,
    pub kind: OverwriteKind,
    pub allow: u64,
    pub deny: u64,
}

/// The role map of a whole-guild bridge, `@everyone` included by identity.
#[derive(Debug, Clone, Default)]
pub struct RoleMapping {
    /// The SIDE ROOM guild id (also `@everyone`'s role id here).
    pub sr_guild: u64,
    /// The Discord guild id (also `@everyone`'s role id there).
    pub discord_guild: u64,
    /// `(sr role id, discord role id)` for every mapped role.
    pub pairs: Vec<(u64, u64)>,
}

impl RoleMapping {
    pub fn to_discord(&self, sr_role: Snowflake) -> Option<u64> {
        if sr_role.get() == self.sr_guild {
            return Some(self.discord_guild);
        }
        self.pairs
            .iter()
            .find(|(s, _)| *s == sr_role.get())
            .map(|(_, d)| *d)
    }

    pub fn to_sr(&self, discord_role: u64) -> Option<Snowflake> {
        if discord_role == self.discord_guild {
            return Some(Snowflake::new(self.sr_guild));
        }
        self.pairs
            .iter()
            .find(|(_, d)| *d == discord_role)
            .map(|(s, _)| Snowflake::new(*s))
    }
}

/// Discord objects the bridge never touches: its own bot member and its managed role.
#[derive(Debug, Clone, Default)]
pub struct Protected {
    pub bot_user: u64,
    pub bot_roles: Vec<u64>,
}

impl Protected {
    fn covers(&self, o: &DiscordOverwrite) -> bool {
        match o.kind {
            OverwriteKind::Member => o.id == self.bot_user,
            OverwriteKind::Role => self.bot_roles.contains(&o.id),
        }
    }
}

/// An overwrite whose allow bits were stripped (conflict kind `overwrite_trimmed`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trimmed {
    pub target: u64,
    pub kind: OverwriteKind,
    pub stripped_allow: Permissions,
}

/// A mapped-role overwrite whose shared bits on Discord differ from SIDE ROOM's (conflict kind
/// `permission_reverted`, field `allow` or `deny`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reverted {
    pub sr_role: Snowflake,
    pub discord_role: u64,
    pub sr_allow: Permissions,
    pub sr_deny: Permissions,
    pub discord_allow: Permissions,
    pub discord_deny: Permissions,
}

/// What to do for one channel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PushPlan {
    /// The complete overwrite array Discord should have, sorted by (kind, id).
    pub desired: Vec<DiscordOverwrite>,
    /// Allow bits stripped from targets SIDE ROOM cannot see.
    pub trimmed: Vec<Trimmed>,
    /// Mapped-role overwrites Discord changed (or removed, or added).
    pub reverted: Vec<Reverted>,
    /// SIDE ROOM member overwrites that stay here only.
    pub sr_only_members: Vec<Snowflake>,
    /// `desired` differs from what Discord has: a PATCH is due.
    pub changed: bool,
}

fn kind_rank(k: OverwriteKind) -> u8 {
    match k {
        OverwriteKind::Role => 0,
        OverwriteKind::Member => 1,
    }
}

fn sorted(mut v: Vec<DiscordOverwrite>) -> Vec<DiscordOverwrite> {
    v.sort_by_key(|o| (kind_rank(o.kind), o.id));
    v
}

/// Compute the push for one channel. See the module docs for the rules.
pub fn plan_push(
    sr: &[Overwrite],
    current: &[DiscordOverwrite],
    map: &RoleMapping,
    protected: &Protected,
) -> PushPlan {
    let mut plan = PushPlan::default();
    let mut desired: Vec<DiscordOverwrite> = Vec::new();
    let mut covered: Vec<u64> = Vec::new();

    for o in sr {
        match o.kind {
            OverwriteKind::Member => plan.sr_only_members.push(o.id),
            OverwriteKind::Role => {
                let Some(d) = map.to_discord(o.id) else {
                    continue; // a SIDE ROOM managed role: nothing on Discord to carry it
                };
                covered.push(d);
                let cur = current
                    .iter()
                    .find(|c| c.kind == OverwriteKind::Role && c.id == d);
                let keep_allow = cur.map_or(0, |c| c.allow & DISCORD_ONLY_MASK);
                let keep_deny = cur.map_or(0, |c| c.deny & DISCORD_ONLY_MASK);
                let allow = ClassifiedBits::to_discord(o.allow, keep_allow);
                let deny = ClassifiedBits::to_discord(o.deny, keep_deny);
                // Discord's shared view, to see a Discord-side change.
                let (ca, cd) = cur.map_or((Permissions::empty(), Permissions::empty()), |c| {
                    (
                        ClassifiedBits::from_discord(c.allow).shared,
                        ClassifiedBits::from_discord(c.deny).shared,
                    )
                });
                let (sa, sd) = (
                    o.allow & Permissions::BRIDGE_SHARED,
                    o.deny & Permissions::BRIDGE_SHARED,
                );
                if (ca, cd) != (sa, sd) {
                    plan.reverted.push(Reverted {
                        sr_role: o.id,
                        discord_role: d,
                        sr_allow: sa,
                        sr_deny: sd,
                        discord_allow: ca,
                        discord_deny: cd,
                    });
                }
                if allow | deny != 0 {
                    desired.push(DiscordOverwrite {
                        id: d,
                        kind: OverwriteKind::Role,
                        allow,
                        deny,
                    });
                }
            }
        }
    }

    for c in current {
        if c.kind == OverwriteKind::Role && covered.contains(&c.id) {
            continue;
        }
        if protected.covers(c) {
            desired.push(*c);
            continue;
        }
        let mapped_role = c.kind == OverwriteKind::Role && map.to_sr(c.id).is_some();
        if mapped_role {
            // SIDE ROOM has no overwrite for this role: Discord's shared bits go, its own bits stay.
            let cs = ClassifiedBits::from_discord(c.allow);
            let cd = ClassifiedBits::from_discord(c.deny);
            if !(cs.shared | cd.shared).is_empty() {
                plan.reverted.push(Reverted {
                    sr_role: map.to_sr(c.id).unwrap_or(Snowflake::new(0)),
                    discord_role: c.id,
                    sr_allow: Permissions::empty(),
                    sr_deny: Permissions::empty(),
                    discord_allow: cs.shared,
                    discord_deny: cd.shared,
                });
            }
            let (allow, deny) = (cs.discord_only, cd.discord_only);
            if allow | deny != 0 {
                desired.push(DiscordOverwrite { allow, deny, ..*c });
            }
        } else {
            // A Discord member, or a managed role: deny stays, allow in the shared mask goes.
            let stripped = ClassifiedBits::from_discord(c.allow).shared;
            if !stripped.is_empty() {
                plan.trimmed.push(Trimmed {
                    target: c.id,
                    kind: c.kind,
                    stripped_allow: stripped,
                });
            }
            let allow = c.allow & !Permissions::BRIDGE_SHARED.bits();
            if allow | c.deny != 0 {
                desired.push(DiscordOverwrite { allow, ..*c });
            }
        }
    }

    plan.desired = sorted(desired);
    plan.changed = plan.desired != sorted(current.to_vec());
    plan
}

/// A channel Discord created inside a mapped parent gets the SIDE ROOM parent's overwrites (as "synced to its
/// category"). That grants nothing the parent did not already grant.
pub fn inherit_from_parent(parent: &[Overwrite]) -> Vec<Overwrite> {
    parent.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: u64 = 1; // SIDE ROOM guild (and @everyone)
    const DG: u64 = 1001; // Discord guild (and @everyone)

    fn map() -> RoleMapping {
        RoleMapping {
            sr_guild: G,
            discord_guild: DG,
            pairs: vec![(10, 110), (11, 111)],
        }
    }

    fn role(id: u64, allow: Permissions, deny: Permissions) -> Overwrite {
        Overwrite {
            id: Snowflake::new(id),
            kind: OverwriteKind::Role,
            allow,
            deny,
        }
    }

    fn d(id: u64, kind: OverwriteKind, allow: u64, deny: u64) -> DiscordOverwrite {
        DiscordOverwrite {
            id,
            kind,
            allow,
            deny,
        }
    }

    #[test]
    fn everyone_maps_by_identity_and_sideroom_only_bits_are_never_sent() {
        let sr = [role(
            G,
            Permissions::SEND_MESSAGES | Permissions::START_WATCH_PARTY,
            Permissions::VIEW_CHANNEL | Permissions::MANAGE_EXTENSIONS,
        )];
        let p = plan_push(&sr, &[], &map(), &Protected::default());
        assert_eq!(
            p.desired,
            vec![d(
                DG,
                OverwriteKind::Role,
                Permissions::SEND_MESSAGES.bits(),
                Permissions::VIEW_CHANNEL.bits()
            )]
        );
        assert!(p.changed);
        for o in &p.desired {
            assert_eq!((o.allow | o.deny) & Permissions::SR_ONLY.bits(), 0);
        }
    }

    #[test]
    fn an_overwrite_of_only_sideroom_bits_produces_none_and_removes_the_old_one() {
        let sr = [role(
            10,
            Permissions::START_WATCH_PARTY,
            Permissions::empty(),
        )];
        let cur = [d(
            110,
            OverwriteKind::Role,
            Permissions::SEND_MESSAGES.bits(),
            0,
        )];
        let p = plan_push(&sr, &cur, &map(), &Protected::default());
        assert!(p.desired.is_empty());
        assert!(p.changed);
        assert_eq!(p.reverted.len(), 1);
    }

    #[test]
    fn discord_only_bits_survive_a_push() {
        let sr = [role(10, Permissions::SEND_MESSAGES, Permissions::empty())];
        let cur = [d(
            110,
            OverwriteKind::Role,
            (1 << 47) | Permissions::SEND_MESSAGES.bits(),
            1 << 54,
        )];
        let p = plan_push(&sr, &cur, &map(), &Protected::default());
        assert_eq!(
            p.desired,
            vec![d(
                110,
                OverwriteKind::Role,
                (1 << 47) | Permissions::SEND_MESSAGES.bits(),
                1 << 54
            )]
        );
        assert!(!p.changed);
        assert!(p.reverted.is_empty());
    }

    #[test]
    fn sideroom_member_overwrites_are_never_sent() {
        let sr = [Overwrite {
            id: Snowflake::new(500),
            kind: OverwriteKind::Member,
            allow: Permissions::VIEW_CHANNEL,
            deny: Permissions::empty(),
        }];
        let p = plan_push(&sr, &[], &map(), &Protected::default());
        assert!(p.desired.is_empty());
        assert_eq!(p.sr_only_members, vec![Snowflake::new(500)]);
        assert!(!p.changed);
    }

    #[test]
    fn discord_member_and_managed_role_allows_are_trimmed_and_denies_kept() {
        let cur = [
            d(
                7000,
                OverwriteKind::Member,
                Permissions::VIEW_CHANNEL.bits() | (1 << 47),
                Permissions::SEND_MESSAGES.bits(),
            ),
            d(
                900,
                OverwriteKind::Role,
                Permissions::VIEW_CHANNEL.bits(),
                0,
            ),
        ];
        let p = plan_push(&[], &cur, &map(), &Protected::default());
        assert_eq!(
            p.desired,
            vec![d(
                7000,
                OverwriteKind::Member,
                1 << 47,
                Permissions::SEND_MESSAGES.bits()
            )]
        );
        assert_eq!(p.trimmed.len(), 2);
        assert!(p.changed);
    }

    #[test]
    fn the_bots_own_overwrites_are_left_alone() {
        let prot = Protected {
            bot_user: 42,
            bot_roles: vec![901],
        };
        let cur = [
            d(
                42,
                OverwriteKind::Member,
                Permissions::VIEW_CHANNEL.bits(),
                0,
            ),
            d(
                901,
                OverwriteKind::Role,
                Permissions::SEND_MESSAGES.bits(),
                0,
            ),
        ];
        let p = plan_push(&[], &cur, &map(), &prot);
        assert_eq!(p.desired, sorted(cur.to_vec()));
        assert!(p.trimmed.is_empty());
        assert!(!p.changed);
    }

    #[test]
    fn a_discord_edit_of_a_mapped_role_overwrite_is_reverted() {
        let sr = [role(11, Permissions::SEND_MESSAGES, Permissions::empty())];
        let cur = [
            d(
                111,
                OverwriteKind::Role,
                (Permissions::SEND_MESSAGES | Permissions::MANAGE_CHANNELS).bits(),
                0,
            ),
            d(
                110,
                OverwriteKind::Role,
                Permissions::ADMINISTRATOR.bits(),
                0,
            ),
        ];
        let p = plan_push(&sr, &cur, &map(), &Protected::default());
        assert_eq!(
            p.desired,
            vec![d(
                111,
                OverwriteKind::Role,
                Permissions::SEND_MESSAGES.bits(),
                0
            )]
        );
        assert_eq!(p.reverted.len(), 2);
        assert!(p.changed);
    }

    #[test]
    fn a_child_inherits_exactly_the_parents_overwrites() {
        let parent = [role(10, Permissions::empty(), Permissions::VIEW_CHANNEL)];
        assert_eq!(inherit_from_parent(&parent), parent.to_vec());
    }
}
