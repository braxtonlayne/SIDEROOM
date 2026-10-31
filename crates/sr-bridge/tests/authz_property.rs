//! BRG-SYN-002, design section 16.3: the authorization property of the whole-guild bridge.
//!
//! SIDE ROOM is the master copy of permissions. Whatever happens on Discord, in any order, no SIDE ROOM member's
//! computed permission in any channel that existed before may grow, no existing role's permission word, no
//! existing overwrite's allow set and no existing role's relative position may change, a SIDE ROOM-only bit never
//! appears in a request body to Discord, Discord-only bits survive the pushes, and after a pass Discord's shared
//! bits equal SIDE ROOM's.
//!
//! The model below is a small in-memory SIDE ROOM guild and Discord guild. Discord events mutate the Discord side
//! only; `step` is the bridge's permission pass built from the real planners of `sr_bridge::sync`. The SIDE ROOM
//! guild is only ever changed by the three things the design allows Discord to cause: a new role with no
//! permissions, a new channel with its parent's overwrites, and the deletion of a role no overwrite denies
//! anything to. Cases: `SR_PROP_CASES` (default 500; CI nightly 10000). Pure, no database.

use proptest::prelude::*;
use sr_bridge::sync::overwrites::{
    DiscordOverwrite, Protected, RoleMapping, inherit_from_parent, plan_push,
};
use sr_bridge::sync::perms::{
    DeletionVerdict, compare_role, push_word, role_deletion_verdict, role_from_discord,
};
use sr_core::permissions::{
    ComputeInput, DISCORD_ONLY_MASK, Overwrite, OverwriteKind, Permissions, compute,
};
use sr_core::snowflake::Snowflake;

const G: u64 = 1;
const DG: u64 = 1001;
const BOT_USER: u64 = 4242;
const MANAGED: u64 = 900;

fn sf(n: u64) -> Snowflake {
    Snowflake::new(n)
}

#[derive(Clone, Debug)]
struct SrRole {
    id: u64,
    perms: Permissions,
    position: i32,
}

#[derive(Clone, Debug)]
struct SrChannel {
    id: u64,
    overwrites: Vec<Overwrite>,
}

#[derive(Clone, Debug)]
struct Sr {
    roles: Vec<SrRole>, // not @everyone
    everyone: Permissions,
    members: Vec<(u64, Vec<u64>)>,
    channels: Vec<SrChannel>,
}

#[derive(Clone, Debug)]
struct DRole {
    id: u64,
    raw: u64,
    position: i32,
}

#[derive(Clone, Debug)]
struct DChannel {
    id: u64,
    sr: Option<u64>,
    overwrites: Vec<DiscordOverwrite>,
}

#[derive(Clone, Debug)]
struct Discord {
    roles: Vec<DRole>,
    everyone_raw: u64,
    channels: Vec<DChannel>,
    next_id: u64,
    /// Every body sent to Discord: (allow, deny) words and role words.
    sent_words: Vec<u64>,
}

#[derive(Debug, Clone)]
struct World {
    sr: Sr,
    d: Discord,
    pairs: Vec<(u64, u64)>,
}

impl World {
    fn mapping(&self) -> RoleMapping {
        RoleMapping {
            sr_guild: G,
            discord_guild: DG,
            pairs: self.pairs.clone(),
        }
    }

    fn compute_for(&self, member: usize, chan: u64) -> Permissions {
        let (mid, roles) = &self.sr.members[member];
        let role_perms: Vec<Permissions> = roles
            .iter()
            .filter_map(|r| self.sr.roles.iter().find(|x| x.id == *r))
            .map(|r| r.perms)
            .collect();
        let ids: Vec<Snowflake> = roles
            .iter()
            .filter(|r| self.sr.roles.iter().any(|x| x.id == **r))
            .map(|r| sf(*r))
            .collect();
        let ows = self
            .sr
            .channels
            .iter()
            .find(|c| c.id == chan)
            .map(|c| c.overwrites.clone())
            .unwrap_or_default();
        compute(&ComputeInput {
            is_owner: false,
            everyone: self.sr.everyone,
            role_perms: &role_perms,
            guild_id: sf(G),
            member_id: sf(*mid),
            member_role_ids: &ids,
            overwrites: &ows,
            timed_out: false,
        })
    }
}

// ---- Discord events --------------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Ev {
    SetRolePerms(usize, u64),
    SetEveryone(u64),
    SetOverwrite {
        chan: usize,
        target: u8,
        allow: u64,
        deny: u64,
    },
    DeleteOverwrite {
        chan: usize,
        target: u8,
    },
    Swap(usize, usize),
    DeleteRole(usize),
    CreateRole(u64),
    CreateChannel {
        parent: usize,
        private: bool,
    },
}

fn any_word() -> impl Strategy<Value = u64> {
    prop_oneof![
        any::<u64>(),
        Just(Permissions::ADMINISTRATOR.bits()),
        Just(u64::MAX),
        (0u32..64).prop_map(|b| 1u64 << b),
    ]
}

fn ev() -> impl Strategy<Value = Ev> {
    prop_oneof![
        (0usize..6, any_word()).prop_map(|(i, w)| Ev::SetRolePerms(i, w)),
        any_word().prop_map(Ev::SetEveryone),
        (0usize..4, 0u8..12, any_word(), any_word()).prop_map(|(chan, target, allow, deny)| {
            Ev::SetOverwrite {
                chan,
                target,
                allow,
                deny,
            }
        }),
        (0usize..4, 0u8..12).prop_map(|(chan, target)| Ev::DeleteOverwrite { chan, target }),
        (0usize..6, 0usize..6).prop_map(|(a, b)| Ev::Swap(a, b)),
        (0usize..6).prop_map(Ev::DeleteRole),
        any_word().prop_map(Ev::CreateRole),
        (0usize..4, any::<bool>())
            .prop_map(|(parent, private)| Ev::CreateChannel { parent, private }),
    ]
}

/// A target number to a Discord overwrite target: the @everyone role, a mapped role, a managed role, a Discord
/// member, the bot, or a role that does not exist.
fn target(w: &World, t: u8) -> (u64, OverwriteKind) {
    match t % 12 {
        0 => (DG, OverwriteKind::Role),
        1..=4 => {
            let i = (t as usize) % w.d.roles.len().max(1);
            (w.d.roles.get(i).map_or(7777, |r| r.id), OverwriteKind::Role)
        }
        5 => (MANAGED, OverwriteKind::Role),
        6 | 7 => (7000 + u64::from(t), OverwriteKind::Member),
        8 => (BOT_USER, OverwriteKind::Member),
        _ => (8000 + u64::from(t), OverwriteKind::Role),
    }
}

fn apply_event(w: &mut World, e: &Ev) {
    match e {
        Ev::SetRolePerms(i, v) => {
            let n = w.d.roles.len();
            if n > 0 {
                w.d.roles[i % n].raw = *v;
            }
        }
        Ev::SetEveryone(v) => w.d.everyone_raw = *v,
        Ev::SetOverwrite {
            chan,
            target: t,
            allow,
            deny,
        } => {
            let n = w.d.channels.len();
            let (id, kind) = target(w, *t);
            let ch = &mut w.d.channels[chan % n];
            ch.overwrites.retain(|o| !(o.id == id && o.kind == kind));
            ch.overwrites.push(DiscordOverwrite {
                id,
                kind,
                allow: *allow,
                deny: *deny,
            });
        }
        Ev::DeleteOverwrite { chan, target: t } => {
            let n = w.d.channels.len();
            let (id, kind) = target(w, *t);
            w.d.channels[chan % n]
                .overwrites
                .retain(|o| !(o.id == id && o.kind == kind));
        }
        Ev::Swap(a, b) => {
            let n = w.d.roles.len();
            if n > 1 {
                let (a, b) = (a % n, b % n);
                let (pa, pb) = (w.d.roles[a].position, w.d.roles[b].position);
                w.d.roles[a].position = pb;
                w.d.roles[b].position = pa;
            }
        }
        Ev::DeleteRole(i) => {
            let n = w.d.roles.len();
            if n > 0 {
                let id = w.d.roles.remove(i % n).id;
                for c in &mut w.d.channels {
                    c.overwrites
                        .retain(|o| !(o.kind == OverwriteKind::Role && o.id == id));
                }
            }
        }
        Ev::CreateRole(raw) => {
            let id = w.d.next_id;
            w.d.next_id += 1;
            let position = w.d.roles.iter().map(|r| r.position).max().unwrap_or(0) + 1;
            w.d.roles.push(DRole {
                id,
                raw: *raw,
                position,
            });
        }
        Ev::CreateChannel { parent, private } => {
            let id = w.d.next_id;
            w.d.next_id += 1;
            let n = w.d.channels.len();
            let p = w.d.channels[parent % n].sr;
            let mut overwrites = Vec::new();
            if *private {
                overwrites.push(DiscordOverwrite {
                    id: DG,
                    kind: OverwriteKind::Role,
                    allow: 0,
                    deny: Permissions::VIEW_CHANNEL.bits(),
                });
            }
            // The Discord creator may also hand out access to a member.
            overwrites.push(DiscordOverwrite {
                id: 7001,
                kind: OverwriteKind::Member,
                allow: Permissions::VIEW_CHANNEL.bits(),
                deny: 0,
            });
            // A channel is "created under" a parent: remember the parent's SIDE ROOM id through `sr`.
            w.d.channels.push(DChannel {
                id,
                sr: None,
                overwrites,
            });
            // Stash the parent in `next_parent` by a side table (see `step`).
            PARENT.with(|c| c.borrow_mut().insert(id, p));
        }
    }
}

thread_local! {
    static PARENT: std::cell::RefCell<std::collections::HashMap<u64, Option<u64>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

// ---- the bridge's permission pass --------------------------------------------------------------------

fn step(w: &mut World) {
    // 1. Roles that exist on Discord but are not mapped (and are not managed): created here with no permissions.
    let new: Vec<DRole> =
        w.d.roles
            .iter()
            .filter(|r| r.id != MANAGED && !w.pairs.iter().any(|(_, d)| *d == r.id))
            .cloned()
            .collect();
    for r in new {
        let rf = role_from_discord(r.raw);
        let sr_id = 10_000 + r.id;
        // Directly above @everyone; the repository shifts the others up, so their relative order is unchanged.
        for x in &mut w.sr.roles {
            x.position += 1;
        }
        w.sr.roles.push(SrRole {
            id: sr_id,
            perms: rf.permissions,
            position: rf.position,
        });
        w.pairs.push((sr_id, r.id));
    }
    // 2. Mapped roles Discord no longer has: deleted here if no overwrite denies them anything, else recreated.
    let gone: Vec<(u64, u64)> = w
        .pairs
        .iter()
        .filter(|(_, d)| !w.d.roles.iter().any(|r| r.id == *d))
        .copied()
        .collect();
    for (sr_id, d_id) in gone {
        let all: Vec<(i64, &Overwrite)> =
            w.sr.channels
                .iter()
                .flat_map(|c| c.overwrites.iter().map(move |o| (c.id as i64, o)))
                .collect();
        match role_deletion_verdict(sf(G), sf(sr_id), all) {
            DeletionVerdict::Mirror => {
                w.sr.roles.retain(|r| r.id != sr_id);
                for c in &mut w.sr.channels {
                    c.overwrites
                        .retain(|o| !(o.kind == OverwriteKind::Role && o.id == sf(sr_id)));
                }
                for m in &mut w.sr.members {
                    m.1.retain(|r| *r != sr_id);
                }
                w.pairs.retain(|(s, _)| *s != sr_id);
            }
            DeletionVerdict::Refuse { .. } => {
                let new_id = w.d.next_id;
                w.d.next_id += 1;
                let sr =
                    w.sr.roles
                        .iter()
                        .find(|r| r.id == sr_id)
                        .expect("mapped role exists");
                let pos = sr.position;
                let word = push_word(sr.perms, 0);
                w.d.sent_words.push(word);
                w.d.roles.push(DRole {
                    id: new_id,
                    raw: word,
                    position: pos,
                });
                for p in &mut w.pairs {
                    if p.0 == sr_id {
                        p.1 = new_id;
                    }
                }
                let _ = d_id;
            }
        }
    }
    // 3. Channels Discord created: here with the SIDE ROOM parent's overwrites.
    let fresh: Vec<u64> =
        w.d.channels
            .iter()
            .filter(|c| c.sr.is_none())
            .map(|c| c.id)
            .collect();
    for did in fresh {
        let parent = PARENT.with(|c| c.borrow().get(&did).copied().flatten());
        let inherited = parent
            .and_then(|p| w.sr.channels.iter().find(|c| c.id == p))
            .map(|c| inherit_from_parent(&c.overwrites))
            .unwrap_or_default();
        let sr_id = 20_000 + did;
        w.sr.channels.push(SrChannel {
            id: sr_id,
            overwrites: inherited,
        });
        if let Some(c) = w.d.channels.iter_mut().find(|c| c.id == did) {
            c.sr = Some(sr_id);
        }
    }
    // 4. Role words: SIDE ROOM's shared bits, Discord-only bits preserved.
    for (sr_id, d_id) in w.pairs.clone() {
        let sr =
            w.sr.roles
                .iter()
                .find(|r| r.id == sr_id)
                .expect("mapped")
                .perms;
        if let Some(dr) = w.d.roles.iter_mut().find(|r| r.id == d_id) {
            let drift = compare_role(sr, dr.raw);
            let word = push_word(sr, drift.discord_only);
            if word != dr.raw {
                w.d.sent_words.push(word);
                dr.raw = word;
            }
        }
    }
    let ev_drift = compare_role(w.sr.everyone, w.d.everyone_raw);
    let word = push_word(w.sr.everyone, ev_drift.discord_only);
    if word != w.d.everyone_raw {
        w.d.sent_words.push(word);
        w.d.everyone_raw = word;
    }
    // 5. Order: SIDE ROOM's, on the mapped roles.
    let mut by_sr: Vec<(u64, i32)> = w.sr.roles.iter().map(|r| (r.id, r.position)).collect();
    by_sr.sort_by_key(|(id, p)| (*p, *id));
    let slots: Vec<i32> = {
        let mut s: Vec<i32> = w
            .pairs
            .iter()
            .filter_map(|(_, d)| w.d.roles.iter().find(|r| r.id == *d).map(|r| r.position))
            .collect();
        s.sort_unstable();
        s
    };
    for ((sr_id, _), pos) in by_sr.iter().zip(slots) {
        if let Some((_, d)) = w.pairs.iter().find(|(s, _)| s == sr_id)
            && let Some(r) = w.d.roles.iter_mut().find(|r| r.id == *d)
        {
            r.position = pos;
        }
    }
    // 6. Overwrites of every mapped channel.
    let map = w.mapping();
    let prot = Protected {
        bot_user: BOT_USER,
        bot_roles: vec![],
    };
    for i in 0..w.d.channels.len() {
        let Some(sr_id) = w.d.channels[i].sr else {
            continue;
        };
        let sr_ows =
            w.sr.channels
                .iter()
                .find(|c| c.id == sr_id)
                .map(|c| c.overwrites.clone())
                .unwrap_or_default();
        let plan = plan_push(&sr_ows, &w.d.channels[i].overwrites, &map, &prot);
        if plan.changed {
            for o in &plan.desired {
                w.d.sent_words.push(o.allow);
                w.d.sent_words.push(o.deny);
            }
            w.d.channels[i].overwrites = plan.desired;
        }
    }
}

// ---- the initial state -------------------------------------------------------------------------------

fn sr_word() -> impl Strategy<Value = Permissions> {
    any::<u64>().prop_map(|w| {
        let p = Permissions::from_bits_truncate(w);
        // Keep administrators rare so the subset check means something.
        if p.contains(Permissions::ADMINISTRATOR) && w % 3 != 0 {
            p - Permissions::ADMINISTRATOR
        } else {
            p
        }
    })
}

fn world() -> impl Strategy<Value = World> {
    let roles = proptest::collection::vec(sr_word(), 4);
    let ows = || proptest::collection::vec((0u8..6, sr_word(), sr_word()), 0..6);
    (
        sr_word(),
        roles,
        proptest::collection::vec(proptest::collection::vec(0usize..4, 0..3), 5),
        ows(),
        ows(),
    )
        .prop_map(|(everyone, roles, members, ows_a, ows_b)| {
            let sr_roles: Vec<SrRole> = roles
                .iter()
                .enumerate()
                .map(|(i, p)| SrRole {
                    id: 10 + i as u64,
                    perms: *p,
                    position: i as i32 + 1,
                })
                .collect();
            let mk = |list: &[(u8, Permissions, Permissions)]| -> Vec<Overwrite> {
                let mut out: Vec<Overwrite> = Vec::new();
                for (t, a, d) in list {
                    let (id, kind) = match t % 6 {
                        0 => (G, OverwriteKind::Role),
                        1..=4 => (10 + u64::from(*t % 4), OverwriteKind::Role),
                        _ => (500, OverwriteKind::Member),
                    };
                    out.retain(|o| !(o.id == sf(id) && o.kind == kind));
                    out.push(Overwrite {
                        id: sf(id),
                        kind,
                        allow: *a & !*d,
                        deny: *d,
                    });
                }
                out
            };
            let members = members
                .iter()
                .enumerate()
                .map(|(i, rs)| {
                    let mut ids: Vec<u64> = rs.iter().map(|r| 10 + *r as u64).collect();
                    ids.sort_unstable();
                    ids.dedup();
                    (100 + i as u64, ids)
                })
                .collect();
            let channels = vec![
                SrChannel {
                    id: 51,
                    overwrites: mk(&ows_a),
                },
                SrChannel {
                    id: 52,
                    overwrites: mk(&ows_b),
                },
                SrChannel {
                    id: 53,
                    overwrites: vec![],
                },
            ];
            let pairs: Vec<(u64, u64)> = sr_roles.iter().map(|r| (r.id, r.id + 100)).collect();
            let d = Discord {
                roles: sr_roles
                    .iter()
                    .map(|r| DRole {
                        id: r.id + 100,
                        raw: push_word(r.perms, 0),
                        position: r.position * 2,
                    })
                    .chain([DRole {
                        id: MANAGED,
                        raw: 0,
                        position: 1,
                    }])
                    .collect(),
                everyone_raw: push_word(everyone, 0),
                channels: channels
                    .iter()
                    .map(|c| DChannel {
                        id: c.id + 100,
                        sr: Some(c.id),
                        overwrites: Vec::new(),
                    })
                    .collect(),
                next_id: 5000,
                sent_words: Vec::new(),
            };
            let mut w = World {
                sr: Sr {
                    roles: sr_roles,
                    everyone,
                    members,
                    channels,
                },
                d,
                pairs,
            };
            step(&mut w); // the seed: Discord now mirrors SIDE ROOM
            w.d.sent_words.clear();
            w
        })
}

fn cases() -> u32 {
    std::env::var("SR_PROP_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(cases()))]

    /// Never raised, never sent, always converging.
    #[test]
    fn no_discord_event_raises_any_sideroom_permission(w0 in world(), events in proptest::collection::vec(ev(), 1..25)) {
        let mut w = w0;
        let orig_roles = w.sr.roles.clone();
        let orig_channels: Vec<(u64, Vec<Overwrite>)> = w.sr.channels.iter().map(|c| (c.id, c.overwrites.clone())).collect();
        let orig_everyone = w.sr.everyone;
        let before: Vec<Vec<Permissions>> = (0..w.sr.members.len())
            .map(|m| orig_channels.iter().map(|(c, _)| w.compute_for(m, *c)).collect())
            .collect();
        let mut sent = 0usize;
        for e in &events {
            apply_event(&mut w, e);
            // Randomly the bridge was offline for a few events: it runs after each, which is the strictest case.
            step(&mut w);

            // (1) No member's computed permission in any pre-existing channel grew.
            for (m, row) in before.iter().enumerate() {
                for (ci, (c, _)) in orig_channels.iter().enumerate() {
                    let now = w.compute_for(m, *c);
                    prop_assert!(row[ci].contains(now) || row[ci].contains(Permissions::ADMINISTRATOR),
                        "member {m} channel {c}: {now:?} is not within {:?} after {e:?}", row[ci]);
                }
            }
            // (2) Words, allow sets and everyone are untouched.
            prop_assert_eq!(w.sr.everyone, orig_everyone);
            for r in &orig_roles {
                if let Some(now) = w.sr.roles.iter().find(|x| x.id == r.id) {
                    prop_assert_eq!(now.perms, r.perms);
                }
            }
            for (c, ows) in &orig_channels {
                let now = &w.sr.channels.iter().find(|x| x.id == *c).unwrap().overwrites;
                for o in now {
                    let was = ows.iter().find(|x| x.id == o.id && x.kind == o.kind);
                    prop_assert_eq!(was.map(|x| (x.allow, x.deny)), Some((o.allow, o.deny)), "overwrite changed or appeared");
                }
            }
            // (3) Relative order of surviving original roles is unchanged.
            let mut surv: Vec<&SrRole> = w.sr.roles.iter().filter(|r| orig_roles.iter().any(|o| o.id == r.id)).collect();
            surv.sort_by_key(|r| r.position);
            let ids: Vec<u64> = surv.iter().map(|r| r.id).collect();
            let mut want: Vec<u64> = ids.clone();
            want.sort_unstable();
            prop_assert_eq!(ids, want, "role order changed");
            // (4) Nothing SIDE ROOM-only was ever sent, and Discord's shared bits equal SIDE ROOM's.
            sent = w.d.sent_words.len();
            for word in &w.d.sent_words {
                prop_assert_eq!(word & Permissions::SR_ONLY.bits(), 0);
                prop_assert_eq!(word & (1u64 << 63), 0);
            }
            for (s, d) in &w.pairs {
                let sr = w.sr.roles.iter().find(|r| r.id == *s).unwrap().perms;
                let dr = w.d.roles.iter().find(|r| r.id == *d).unwrap();
                prop_assert_eq!(compare_role(sr, dr.raw).needs_revert(), false);
            }
            prop_assert!(!compare_role(w.sr.everyone, w.d.everyone_raw).needs_revert());
        }
        let _ = sent;
    }

    /// Discord-only bits on a role survive any number of passes.
    #[test]
    fn discord_only_bits_survive(w0 in world(), bits in 0u64..16) {
        let mut w = w0;
        let only = ((bits & 1) << 47) | (((bits >> 1) & 7) << 53);
        let d = w.pairs[0].1;
        let r = w.d.roles.iter_mut().find(|r| r.id == d).unwrap();
        r.raw |= only;
        step(&mut w);
        step(&mut w);
        let r = w.d.roles.iter().find(|r| r.id == d).unwrap();
        prop_assert_eq!(r.raw & DISCORD_ONLY_MASK, only);
    }

    /// SIDE ROOM member overwrites are never sent and Discord member allows never survive on a mirrored channel.
    #[test]
    fn member_overwrites_follow_the_rules(w0 in world(), allow in any::<u64>(), deny in any::<u64>()) {
        let mut w = w0;
        apply_event(&mut w, &Ev::SetOverwrite { chan: 0, target: 6, allow, deny });
        step(&mut w);
        for c in &w.d.channels {
            for o in &c.overwrites {
                if o.kind == OverwriteKind::Member && o.id != BOT_USER {
                    prop_assert_eq!(o.allow & Permissions::BRIDGE_SHARED.bits(), 0);
                    // SIDE ROOM's member overwrite (id 500) never appears on Discord.
                    prop_assert_ne!(o.id, 500);
                }
            }
        }
    }
}

#[test]
fn a_muted_role_is_recreated_not_deleted() {
    // Role 10 denies Send Messages in channel 51: a Discord-side deletion must not remove it here.
    let sr = Sr {
        roles: vec![SrRole {
            id: 10,
            perms: Permissions::empty(),
            position: 1,
        }],
        everyone: Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES,
        members: vec![(100, vec![10])],
        channels: vec![SrChannel {
            id: 51,
            overwrites: vec![Overwrite {
                id: sf(10),
                kind: OverwriteKind::Role,
                allow: Permissions::empty(),
                deny: Permissions::SEND_MESSAGES,
            }],
        }],
    };
    let d = Discord {
        roles: vec![DRole {
            id: 110,
            raw: 0,
            position: 1,
        }],
        everyone_raw: 0,
        channels: vec![DChannel {
            id: 151,
            sr: Some(51),
            overwrites: vec![],
        }],
        next_id: 5000,
        sent_words: vec![],
    };
    let mut w = World {
        sr,
        d,
        pairs: vec![(10, 110)],
    };
    step(&mut w);
    apply_event(&mut w, &Ev::DeleteRole(0));
    step(&mut w);
    assert!(
        w.sr.roles.iter().any(|r| r.id == 10),
        "the role is still here"
    );
    assert!(!w.compute_for(0, 51).contains(Permissions::SEND_MESSAGES));
    assert_eq!(w.pairs.len(), 1);
    assert!(
        w.d.roles.iter().any(|r| r.id == w.pairs[0].1),
        "recreated on Discord"
    );
}

#[test]
fn a_sideroom_only_bit_is_never_in_a_body_and_stays_here() {
    let sr = Sr {
        roles: vec![SrRole {
            id: 10,
            perms: Permissions::SEND_MESSAGES
                | Permissions::START_WATCH_PARTY
                | Permissions::MANAGE_EXTENSIONS,
            position: 1,
        }],
        everyone: Permissions::USE_EXPRESSIONS_EXTERNALLY | Permissions::VIEW_CHANNEL,
        members: vec![(100, vec![10])],
        channels: vec![SrChannel {
            id: 51,
            overwrites: vec![Overwrite {
                id: sf(10),
                kind: OverwriteKind::Role,
                allow: Permissions::CONTROL_WATCH_PARTY | Permissions::SEND_MESSAGES,
                deny: Permissions::USE_LIBRARY_EMBEDS,
            }],
        }],
    };
    let d = Discord {
        roles: vec![DRole {
            id: 110,
            raw: 0,
            position: 1,
        }],
        everyone_raw: 0,
        channels: vec![DChannel {
            id: 151,
            sr: Some(51),
            overwrites: vec![],
        }],
        next_id: 5000,
        sent_words: vec![],
    };
    let mut w = World {
        sr,
        d,
        pairs: vec![(10, 110)],
    };
    // Seed, a Discord admin tries to grant bit 56 and 62, then everything is reconciled again.
    step(&mut w);
    apply_event(
        &mut w,
        &Ev::SetRolePerms(0, (1 << 56) | (1 << 62) | Permissions::BAN_MEMBERS.bits()),
    );
    step(&mut w);
    assert!(!w.d.sent_words.is_empty());
    assert!(
        w.d.sent_words
            .iter()
            .all(|x| x & Permissions::SR_ONLY.bits() == 0)
    );
    let r = &w.sr.roles[0];
    assert!(
        r.perms
            .contains(Permissions::START_WATCH_PARTY | Permissions::MANAGE_EXTENSIONS)
    );
    assert!(!r.perms.contains(Permissions::BAN_MEMBERS));
    assert!(
        w.sr.everyone
            .contains(Permissions::USE_EXPRESSIONS_EXTERNALLY)
    );
}
