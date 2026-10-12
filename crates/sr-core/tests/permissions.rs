//! FND-CORE-008: permission computation vs a Discord reference fixture set,
//! plus property tests against an independent per-bit reference model.

use proptest::prelude::*;
use serde_json::Value;
use sr_core::Snowflake;
use sr_core::permissions::{
    Overwrite, OverwriteKind, Permissions, compute_base, compute_overwrites,
};

const GUILD: u64 = 100;
const OWNER: u64 = 1;
const MEMBER: u64 = 2;

fn compute(
    is_owner: bool,
    everyone: Permissions,
    role_perms: &[(u64, Permissions)],
    member_roles: &[u64],
    ows: &[Overwrite],
) -> Permissions {
    let held: Vec<Permissions> = role_perms
        .iter()
        .filter(|(id, _)| member_roles.contains(id))
        .map(|(_, p)| *p)
        .collect();
    let base = compute_base(is_owner, everyone, held);
    let ids: Vec<Snowflake> = member_roles.iter().map(|r| Snowflake(*r)).collect();
    compute_overwrites(base, Snowflake(GUILD), Snowflake(MEMBER), &ids, ows)
}

#[test]
fn discord_fixture_cases() {
    let v: Value = serde_json::from_str(include_str!("fixtures/discord_permissions.json")).unwrap();
    for case in v["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let everyone = Permissions::from_bits_retain(case["everyone"].as_u64().unwrap());
        let roles: Vec<(u64, Permissions)> = case["roles"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, p)| {
                (
                    k.parse().unwrap(),
                    Permissions::from_bits_retain(p.as_u64().unwrap()),
                )
            })
            .collect();
        let member_roles: Vec<u64> = case["member_roles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_u64().unwrap())
            .collect();
        let ows: Vec<Overwrite> = case["overwrites"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| Overwrite {
                id: Snowflake(o["id"].as_u64().unwrap()),
                kind: if o["type"] == "member" {
                    OverwriteKind::Member
                } else {
                    OverwriteKind::Role
                },
                allow: Permissions::from_bits_retain(o["allow"].as_u64().unwrap()),
                deny: Permissions::from_bits_retain(o["deny"].as_u64().unwrap()),
            })
            .collect();
        let is_owner = case["owner"].as_bool().unwrap_or(false);
        let expected = match &case["expected"] {
            Value::String(s) if s == "ALL" => Permissions::ALL,
            n => Permissions::from_bits_retain(n.as_u64().unwrap()),
        };
        let got = compute(is_owner, everyone, &roles, &member_roles, &ows);
        assert_eq!(got, expected, "fixture case: {name}");
    }
}

/// Independent reference: evaluates one bit at a time through the documented
/// precedence ladder instead of using set algebra.
fn reference(
    is_owner: bool,
    everyone: u64,
    role_perms: &[(u64, u64)],
    member_roles: &[u64],
    ows: &[Overwrite],
) -> u64 {
    let all = Permissions::ALL.bits();
    if is_owner {
        return all;
    }
    let mut out = 0u64;
    let admin = {
        let mut a = everyone & 8 != 0;
        for (id, p) in role_perms {
            if member_roles.contains(id) && p & 8 != 0 {
                a = true;
            }
        }
        a
    };
    if admin {
        return all;
    }
    for bit in 0..64 {
        let m = 1u64 << bit;
        if all & m == 0 {
            continue;
        }
        let mut on = everyone & m != 0;
        for (id, p) in role_perms {
            if member_roles.contains(id) && p & m != 0 {
                on = true;
            }
        }
        for o in ows
            .iter()
            .filter(|o| o.kind == OverwriteKind::Role && o.id.get() == GUILD)
        {
            if o.deny.bits() & m != 0 {
                on = false;
            }
            if o.allow.bits() & m != 0 {
                on = true;
            }
        }
        let mut role_deny = false;
        let mut role_allow = false;
        for o in ows.iter().filter(|o| {
            o.kind == OverwriteKind::Role
                && o.id.get() != GUILD
                && member_roles.contains(&o.id.get())
        }) {
            role_deny |= o.deny.bits() & m != 0;
            role_allow |= o.allow.bits() & m != 0;
        }
        if role_deny {
            on = false;
        }
        if role_allow {
            on = true;
        }
        for o in ows
            .iter()
            .filter(|o| o.kind == OverwriteKind::Member && o.id.get() == MEMBER)
        {
            if o.deny.bits() & m != 0 {
                on = false;
            }
            if o.allow.bits() & m != 0 {
                on = true;
            }
        }
        if on {
            out |= m;
        }
    }
    out
}

fn arb_perm() -> impl Strategy<Value = u64> {
    any::<u64>().prop_map(|b| Permissions::from_bits_truncate(b).bits())
}

fn arb_overwrite() -> impl Strategy<Value = Overwrite> {
    (
        prop_oneof![Just(GUILD), 200u64..204, Just(MEMBER), Just(999u64)],
        any::<bool>(),
        arb_perm(),
        arb_perm(),
    )
        .prop_map(|(id, member, allow, deny)| Overwrite {
            id: Snowflake(id),
            kind: if member && id == MEMBER {
                OverwriteKind::Member
            } else {
                OverwriteKind::Role
            },
            allow: Permissions::from_bits_retain(allow),
            deny: Permissions::from_bits_retain(deny),
        })
}

fn dedup(mut ows: Vec<Overwrite>) -> Vec<Overwrite> {
    // Discord allows at most one overwrite per (kind, id).
    let mut seen = std::collections::HashSet::new();
    ows.retain(|o| seen.insert((o.kind, o.id)));
    ows
}

proptest! {
    #[test]
    fn matches_reference_model(
        is_owner in any::<bool>(),
        everyone in arb_perm(),
        role_perms in proptest::collection::vec(arb_perm(), 4),
        member_roles in proptest::collection::vec(200u64..204, 0..4),
        ows in proptest::collection::vec(arb_overwrite(), 0..8),
    ) {
        let ows = dedup(ows);
        let mut member_roles = member_roles;
        member_roles.sort_unstable();
        member_roles.dedup();
        let roles: Vec<(u64, Permissions)> = role_perms.iter().enumerate()
            .map(|(i, p)| (200 + i as u64, Permissions::from_bits_retain(*p))).collect();
        let raw: Vec<(u64, u64)> = roles.iter().map(|(i, p)| (*i, p.bits())).collect();
        let got = compute(is_owner, Permissions::from_bits_retain(everyone), &roles, &member_roles, &ows);
        prop_assert_eq!(got.bits(), reference(is_owner, everyone, &raw, &member_roles, &ows));
    }

    #[test]
    fn administrator_always_gets_all(
        everyone in arb_perm(),
        ows in proptest::collection::vec(arb_overwrite(), 0..8),
    ) {
        let ows = dedup(ows);
        let admin = Permissions::from_bits_retain(everyone) | Permissions::ADMINISTRATOR;
        prop_assert_eq!(compute(false, admin, &[], &[], &ows), Permissions::ALL);
    }

    #[test]
    fn no_overwrites_equals_base(everyone in arb_perm(), r in arb_perm()) {
        let e = Permissions::from_bits_retain(everyone);
        let role = Permissions::from_bits_retain(r);
        let base = compute_base(false, e, [role]);
        prop_assert_eq!(compute_overwrites(base, Snowflake(GUILD), Snowflake(MEMBER), &[], &[]), base);
    }

    #[test]
    fn member_overwrite_is_final_word(
        everyone in arb_perm(),
        allow in arb_perm(),
        deny in arb_perm(),
    ) {
        let e = Permissions::from_bits_retain(everyone) - Permissions::ADMINISTRATOR;
        let ow = Overwrite {
            id: Snowflake(MEMBER),
            kind: OverwriteKind::Member,
            allow: Permissions::from_bits_retain(allow),
            deny: Permissions::from_bits_retain(deny),
        };
        let got = compute(false, e, &[], &[], &[ow]);
        prop_assert!(got.contains(ow.allow));
        prop_assert!((got & ow.deny & !ow.allow).is_empty());
    }

    #[test]
    fn permissions_serde_roundtrip(bits in any::<u64>()) {
        let p = Permissions::from_bits_truncate(bits);
        let s = serde_json::to_string(&p).unwrap();
        prop_assert_eq!(serde_json::from_str::<Permissions>(&s).unwrap(), p);
    }

    #[test]
    fn snowflake_serde_roundtrip(raw in any::<u64>()) {
        let s = Snowflake(raw);
        let j = serde_json::to_string(&s).unwrap();
        prop_assert_eq!(serde_json::from_str::<Snowflake>(&j).unwrap(), s);
    }
}

#[test]
fn owner_const_is_used() {
    // Keeps the fixture's owner id documented alongside GUILD/MEMBER.
    assert_ne!(OWNER, MEMBER);
}
