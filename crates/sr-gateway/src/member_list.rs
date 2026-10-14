//! The lazy member list (LNG-GLD-010): op 102 asks for ranges of a channel's member sidebar and
//! the gateway answers with `GUILD_MEMBER_LIST_UPDATE` and keeps those ranges current.
//!
//! The list is the one Discord draws: members who can see the channel, grouped under their
//! highest hoisted role (online members only), then everybody else online, then offline,
//! each group headed by a `{group: {id, count}}` item and sorted by display name. A client
//! subscribes to index ranges (`[[0, 99]]`); it gets `SYNC` for every range at once, then
//! `UPDATE`, `INSERT` and `DELETE` ops that keep its slice equal to the server's as members
//! join, leave, change or go online and offline.
//!
//! This module is pure (build, diff, parse); the registry and delivery live in
//! [`crate::hub`].

use std::collections::BTreeMap;

use serde_json::{Value, json};
use sr_core::UserId;

use crate::protocol::PresenceUpdate;

/// Most ranges one request may name.
pub const MAX_RANGES: usize = 8;
/// Most items one range may cover.
pub const MAX_RANGE_LEN: usize = 200;
/// Group id of online members without a hoisted role.
pub const GROUP_ONLINE: &str = "online";
/// Group id of offline members.
pub const GROUP_OFFLINE: &str = "offline";

/// A member who can see the channel, as the directory describes them.
#[derive(Debug, Clone, PartialEq)]
pub struct ListMember {
    pub user: UserId,
    /// The Discord member object (`user`, `nick`, `roles`, ...), without `presence`.
    pub member: Value,
    /// The highest hoisted role the member holds: its id and position.
    pub hoist: Option<(String, i32)>,
    /// What the list sorts on inside a group: nickname, else display name, else username.
    pub sort_key: String,
}

/// What a directory returns for one channel's member list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MemberListSource {
    pub members: Vec<ListMember>,
    /// Every member of the server can see the channel: the list is Discord's `everyone` list.
    pub everyone: bool,
}

/// One row of the list: a group header or a member. `key` identifies the row across rebuilds
/// (so a changed row is an `UPDATE`, a moved or new one an `INSERT`).
#[derive(Debug, Clone, PartialEq)]
pub struct ListItem {
    pub key: String,
    pub json: Value,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Built {
    pub items: Vec<ListItem>,
    /// `(group id, member count)` in list order, for the dispatch's `groups`.
    pub groups: Vec<(String, usize)>,
    pub member_count: usize,
    pub online_count: usize,
}

/// A change to a client's slice, in absolute list indices.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Sync {
        range: (usize, usize),
        items: Vec<Value>,
    },
    Update {
        index: usize,
        item: Value,
    },
    Insert {
        index: usize,
        item: Value,
    },
    Delete {
        index: usize,
    },
}

impl Op {
    pub fn to_json(&self) -> Value {
        match self {
            Self::Sync { range, items } => {
                json!({ "op": "SYNC", "range": [range.0, range.1], "items": items })
            }
            Self::Update { index, item } => json!({ "op": "UPDATE", "index": index, "item": item }),
            Self::Insert { index, item } => json!({ "op": "INSERT", "index": index, "item": item }),
            Self::Delete { index } => json!({ "op": "DELETE", "index": index }),
        }
    }
}

/// Build the list from the members and their live presence. `presence` answers with the
/// status other users see (`offline` for anybody without a session).
pub fn build(members: &[ListMember], presence: &dyn Fn(UserId) -> PresenceUpdate) -> Built {
    // Role groups are ordered by position, highest first; ties by role id for stability.
    let mut role_groups: BTreeMap<
        (std::cmp::Reverse<i32>, String),
        Vec<(&ListMember, PresenceUpdate)>,
    > = BTreeMap::new();
    let mut online: Vec<(&ListMember, PresenceUpdate)> = Vec::new();
    let mut offline: Vec<(&ListMember, PresenceUpdate)> = Vec::new();
    for m in members {
        let p = presence(m.user);
        if p.visible_status() == "offline" {
            offline.push((m, p));
        } else if let Some((id, pos)) = &m.hoist {
            role_groups
                .entry((std::cmp::Reverse(*pos), id.clone()))
                .or_default()
                .push((m, p));
        } else {
            online.push((m, p));
        }
    }
    let order = |v: &mut Vec<(&ListMember, PresenceUpdate)>| {
        v.sort_by(|a, b| {
            a.0.sort_key
                .to_lowercase()
                .cmp(&b.0.sort_key.to_lowercase())
                .then(a.0.user.cmp(&b.0.user))
        });
    };
    let mut out = Built {
        member_count: members.len(),
        ..Built::default()
    };
    let push_group = |id: &str, mut rows: Vec<(&ListMember, PresenceUpdate)>, out: &mut Built| {
        if rows.is_empty() {
            return;
        }
        order(&mut rows);
        out.groups.push((id.to_owned(), rows.len()));
        out.items.push(ListItem {
            key: format!("g:{id}"),
            json: json!({ "group": { "id": id, "count": rows.len() } }),
        });
        for (m, p) in rows {
            let mut member = m.member.clone();
            if let Some(o) = member.as_object_mut() {
                o.insert(
                    "presence".into(),
                    json!({
                        "user": { "id": m.user.to_string() },
                        "status": p.visible_status(),
                        "activities": p.activities,
                        "client_status": {},
                    }),
                );
            }
            out.items.push(ListItem {
                key: format!("m:{}", m.user),
                json: json!({ "member": member }),
            });
        }
    };
    let mut online_total = 0;
    for ((_, id), rows) in role_groups {
        online_total += rows.len();
        push_group(&id, rows, &mut out);
    }
    online_total += online.len();
    push_group(GROUP_ONLINE, online, &mut out);
    push_group(GROUP_OFFLINE, offline, &mut out);
    out.online_count = online_total;
    out
}

/// Parse `[[0, 99], [100, 199]]`: at most [`MAX_RANGES`] ranges of at most
/// [`MAX_RANGE_LEN`] items, `start <= end`, nothing else. Overlapping ranges are kept as sent
/// (the client asked for them), invalid ones are dropped.
pub fn parse_ranges(raw: &Value) -> Vec<(usize, usize)> {
    let Some(list) = raw.as_array() else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|r| {
            let r = r.as_array()?;
            let (a, b) = (r.first()?.as_u64()?, r.get(1)?.as_u64()?);
            let (a, b) = (usize::try_from(a).ok()?, usize::try_from(b).ok()?);
            (a <= b && b - a < MAX_RANGE_LEN && r.len() == 2).then_some((a, b))
        })
        .take(MAX_RANGES)
        .collect()
}

fn slice(items: &[ListItem], (a, b): (usize, usize)) -> &[ListItem] {
    if a >= items.len() {
        return &[];
    }
    &items[a..=b.min(items.len() - 1)]
}

/// The full `SYNC` of one range.
pub fn sync(items: &[ListItem], range: (usize, usize)) -> Op {
    Op::Sync {
        range,
        items: slice(items, range).iter().map(|i| i.json.clone()).collect(),
    }
}

/// The ops that turn what a client holds for `range` (`old`) into what it should hold (`new`).
/// Rows are matched by key: unchanged rows cost nothing, a changed row is an `UPDATE`, rows
/// that appeared or vanished (or moved) are `DELETE`s then `INSERT`s. When that would take as
/// many ops as the range has rows, one `SYNC` is sent instead.
pub fn diff_range(old: &[ListItem], new: &[ListItem], range: (usize, usize)) -> Vec<Op> {
    let (o, n) = (slice(old, range), slice(new, range));
    if o == n {
        return Vec::new();
    }
    let start = range.0;
    let prefix = o.iter().zip(n).take_while(|(a, b)| a.key == b.key).count();
    let max_suffix = o.len().min(n.len()) - prefix;
    let suffix = o
        .iter()
        .rev()
        .zip(n.iter().rev())
        .take(max_suffix)
        .take_while(|(a, b)| a.key == b.key)
        .count();
    let mut ops = Vec::new();
    // The differing middle: delete the old rows, insert the new ones, all at the same spot.
    for _ in prefix..o.len() - suffix {
        ops.push(Op::Delete {
            index: start + prefix,
        });
    }
    for (k, item) in n[prefix..n.len() - suffix].iter().enumerate() {
        ops.push(Op::Insert {
            index: start + prefix + k,
            item: item.json.clone(),
        });
    }
    // Rows that kept their key but changed (a group count, a status) are updated in place at
    // their final index.
    for (i, (a, b)) in o.iter().zip(n).enumerate().take(prefix) {
        if a != b {
            ops.push(Op::Update {
                index: start + i,
                item: b.json.clone(),
            });
        }
    }
    for k in 0..suffix {
        let (a, b) = (&o[o.len() - suffix + k], &n[n.len() - suffix + k]);
        if a != b {
            ops.push(Op::Update {
                index: start + n.len() - suffix + k,
                item: b.json.clone(),
            });
        }
    }
    if ops.len() >= n.len().max(1) {
        return vec![sync(new, range)];
    }
    ops
}

/// The dispatch body of `GUILD_MEMBER_LIST_UPDATE`.
pub fn dispatch_body(guild: &str, list_id: &str, built: &Built, ops: &[Op]) -> Value {
    json!({
        "guild_id": guild,
        "id": list_id,
        "member_count": built.member_count,
        "online_count": built.online_count,
        "groups": built
            .groups
            .iter()
            .map(|(id, count)| json!({ "id": id, "count": count }))
            .collect::<Vec<_>>(),
        "ops": ops.iter().map(Op::to_json).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(id: u64, name: &str, hoist: Option<(&str, i32)>) -> ListMember {
        ListMember {
            user: UserId::new(id),
            member: json!({ "user": { "id": id.to_string() }, "nick": name }),
            hoist: hoist.map(|(r, p)| (r.to_owned(), p)),
            sort_key: name.to_owned(),
        }
    }

    fn status(map: &[(u64, &str)]) -> impl Fn(UserId) -> PresenceUpdate {
        let map: std::collections::HashMap<u64, String> =
            map.iter().map(|(u, s)| (*u, (*s).to_owned())).collect();
        move |u| match map.get(&u.get()) {
            Some(s) => PresenceUpdate {
                status: s.clone(),
                ..PresenceUpdate::default()
            },
            None => PresenceUpdate::offline(),
        }
    }

    fn keys(b: &Built) -> Vec<&str> {
        b.items.iter().map(|i| i.key.as_str()).collect()
    }

    fn sample() -> Vec<ListMember> {
        vec![
            member(1, "Zed", Some(("mods", 5))),
            member(2, "amy", Some(("mods", 5))),
            member(3, "Bob", Some(("vips", 3))),
            member(4, "cat", None),
            member(5, "Dan", None),
            member(6, "eve", Some(("mods", 5))),
        ]
    }

    #[test]
    fn groups_by_hoisted_role_then_online_then_offline() {
        // 1, 2, 3, 4 online; 5 idle; 6 is a moderator but offline.
        let b = build(
            &sample(),
            &status(&[
                (1, "online"),
                (2, "dnd"),
                (3, "online"),
                (4, "online"),
                (5, "idle"),
            ]),
        );
        assert_eq!(
            keys(&b),
            [
                "g:mods",
                "m:2",
                "m:1", // sorted ignoring case: amy, Zed
                "g:vips",
                "m:3",
                "g:online",
                "m:4",
                "m:5",
                "g:offline",
                "m:6",
            ]
        );
        assert_eq!(
            b.groups,
            [
                ("mods".to_owned(), 2),
                ("vips".to_owned(), 1),
                ("online".to_owned(), 2),
                ("offline".to_owned(), 1)
            ]
        );
        assert_eq!((b.member_count, b.online_count), (6, 5));
        assert_eq!(
            b.items[0].json,
            json!({ "group": { "id": "mods", "count": 2 } })
        );
        let amy = &b.items[1].json["member"];
        assert_eq!(amy["nick"], "amy");
        assert_eq!(amy["presence"]["status"], "dnd");
        assert_eq!(amy["presence"]["user"]["id"], "2");
    }

    #[test]
    fn invisible_counts_as_offline_and_empty_groups_vanish() {
        let b = build(&sample(), &status(&[(4, "invisible")]));
        assert_eq!(keys(&b)[0], "g:offline");
        assert_eq!(b.online_count, 0);
        assert_eq!(b.groups.len(), 1);
        assert_eq!(build(&[], &status(&[])), Built::default());
    }

    #[test]
    fn ranges_are_parsed_defensively() {
        assert_eq!(
            parse_ranges(&json!([[0, 99], [100, 199]])),
            vec![(0, 99), (100, 199)]
        );
        assert_eq!(
            parse_ranges(&json!([
                [5, 4],
                [0, 200],
                "x",
                [1],
                [0, 1, 2],
                [-1, 3],
                [0, 199]
            ])),
            vec![(0, 199)]
        );
        assert!(parse_ranges(&json!({})).is_empty());
        let many = Value::Array((0..20).map(|n| json!([n, n])).collect());
        assert_eq!(parse_ranges(&many).len(), MAX_RANGES);
    }

    fn items(spec: &[(&str, i32)]) -> Vec<ListItem> {
        spec.iter()
            .map(|(k, v)| ListItem {
                key: (*k).to_owned(),
                json: json!({ "v": v }),
            })
            .collect()
    }

    /// Apply ops to a client's slice (which starts at list index `start`), like a client would.
    fn apply(mut slice: Vec<Value>, ops: &[Op], start: usize) -> Vec<Value> {
        for op in ops {
            match op {
                Op::Sync { items, .. } => slice = items.clone(),
                Op::Update { index, item } => slice[*index - start] = item.clone(),
                Op::Insert { index, item } => slice.insert(*index - start, item.clone()),
                Op::Delete { index } => {
                    slice.remove(*index - start);
                }
            }
        }
        slice
    }

    fn check(old: &[(&str, i32)], new: &[(&str, i32)], range: (usize, usize)) -> Vec<Op> {
        let (o, n) = (items(old), items(new));
        let ops = diff_range(&o, &n, range);
        let held: Vec<Value> = slice(&o, range).iter().map(|i| i.json.clone()).collect();
        let want: Vec<Value> = slice(&n, range).iter().map(|i| i.json.clone()).collect();
        assert_eq!(apply(held, &ops, range.0), want, "{ops:?}");
        ops
    }

    #[test]
    fn unchanged_lists_send_nothing() {
        assert!(check(&[("a", 1), ("b", 1)], &[("a", 1), ("b", 1)], (0, 9)).is_empty());
        assert!(check(&[], &[], (0, 9)).is_empty());
    }

    #[test]
    fn changed_rows_update_in_place() {
        let ops = check(
            &[("g", 1), ("a", 1), ("b", 1), ("c", 1)],
            &[("g", 2), ("a", 1), ("b", 5), ("c", 1)],
            (0, 9),
        );
        assert_eq!(ops.len(), 2);
        assert!(ops.iter().all(|o| matches!(o, Op::Update { .. })));
    }

    #[test]
    fn inserts_and_deletes_keep_the_slice_consistent() {
        let base = [("a", 1), ("b", 1), ("c", 1), ("d", 1), ("e", 1), ("f", 1)];
        // A member appears in the middle.
        let ops = check(
            &base,
            &[
                ("a", 1),
                ("b", 1),
                ("x", 1),
                ("c", 1),
                ("d", 1),
                ("e", 1),
                ("f", 1),
            ],
            (0, 9),
        );
        assert_eq!(
            ops,
            vec![Op::Insert {
                index: 2,
                item: json!({ "v": 1 })
            }]
        );
        // One leaves.
        let ops = check(
            &base,
            &[("a", 1), ("b", 1), ("d", 1), ("e", 1), ("f", 1)],
            (0, 9),
        );
        assert_eq!(ops, vec![Op::Delete { index: 2 }]);
        // The first range only holds four rows: a row inserted before them pushes one out and
        // the one behind the range comes in.
        let ops = check(
            &base,
            &[
                ("x", 1),
                ("a", 1),
                ("b", 1),
                ("c", 1),
                ("d", 1),
                ("e", 1),
                ("f", 1),
            ],
            (0, 3),
        );
        assert!(ops.len() <= 2, "{ops:?}");
        // A later range is untouched by changes above it only when they do not shift it.
        let ops = check(
            &base,
            &[("a", 1), ("b", 1), ("c", 1), ("d", 1), ("e", 1), ("g", 1)],
            (0, 3),
        );
        assert!(ops.is_empty());
    }

    #[test]
    fn a_member_that_moves_group_is_deleted_and_inserted() {
        let before = [
            ("g:on", 1),
            ("m:1", 1),
            ("m:2", 1),
            ("g:off", 1),
            ("m:3", 1),
        ];
        let after = [
            ("g:on", 2),
            ("m:1", 1),
            ("m:2", 1),
            ("m:3", 2),
            ("g:off", 0),
            ("m:9", 1),
        ];
        check(&before, &after, (0, 9));
        check(&before, &after, (1, 3));
    }

    #[test]
    fn big_changes_fall_back_to_one_sync() {
        let ops = check(
            &[("a", 1), ("b", 1), ("c", 1)],
            &[("x", 1), ("y", 1), ("z", 1)],
            (0, 9),
        );
        assert_eq!(ops.len(), 1);
        assert!(matches!(ops[0], Op::Sync { range: (0, 9), .. }));
        // Joining an empty slice also syncs.
        let ops = check(&[], &[("a", 1), ("b", 1)], (0, 9));
        assert!(matches!(ops[0], Op::Sync { .. }));
    }

    #[test]
    fn random_walks_always_converge() {
        // A tiny deterministic generator drives many list mutations over several ranges.
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = move |n: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % n as u64) as usize
        };
        let mut old: Vec<(String, i32)> = (0..30).map(|i| (format!("m{i}"), 0)).collect();
        for _ in 0..300 {
            let mut new = old.clone();
            for _ in 0..=next(3) {
                match next(3) {
                    0 if !new.is_empty() => {
                        new.remove(next(new.len()));
                    }
                    1 => {
                        let at = next(new.len() + 1);
                        new.insert(at, (format!("n{}", next(1000)), 0));
                    }
                    _ if !new.is_empty() => {
                        let at = next(new.len());
                        new[at].1 += 1;
                    }
                    _ => {}
                }
            }
            new.dedup_by(|a, b| a.0 == b.0);
            let to_items = |v: &[(String, i32)]| -> Vec<ListItem> {
                v.iter()
                    .map(|(k, n)| ListItem {
                        key: k.clone(),
                        json: json!({ "n": n }),
                    })
                    .collect()
            };
            let (o, n) = (to_items(&old), to_items(&new));
            for range in [(0, 9), (5, 14), (10, 39)] {
                let ops = diff_range(&o, &n, range);
                let held: Vec<Value> = slice(&o, range).iter().map(|i| i.json.clone()).collect();
                let want: Vec<Value> = slice(&n, range).iter().map(|i| i.json.clone()).collect();
                assert_eq!(apply(held, &ops, range.0), want, "{range:?} {ops:?}");
            }
            old = new;
        }
    }

    #[test]
    fn dispatch_body_shape() {
        let b = build(&sample(), &status(&[(1, "online")]));
        let body = dispatch_body("7", "everyone", &b, &[sync(&b.items, (0, 2))]);
        assert_eq!(body["guild_id"], "7");
        assert_eq!(body["id"], "everyone");
        assert_eq!(body["member_count"], 6);
        assert_eq!(body["online_count"], 1);
        assert_eq!(body["groups"][0], json!({ "id": "mods", "count": 1 }));
        assert_eq!(body["ops"][0]["op"], "SYNC");
        assert_eq!(body["ops"][0]["range"], json!([0, 2]));
        assert_eq!(body["ops"][0]["items"].as_array().unwrap().len(), 3);
    }
}
