//! The plan (dry run) of switching to whole_guild: the matching of design section 9.3, the diff, the blockers and
//! the hashes. Everything here is pure: the callers read both sides into [`SrSnapshot`] and [`DiscordSnapshot`]
//! and persist the result (`plan_store`).

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

use super::eligible::{Mirrored, bridgeable};
use crate::discord_structure::{GuildChannel, GuildRole, channel_type};

/// Discord's caps (and ours, `sr_chat::limits`).
pub const MAX_ROLES: usize = 250;
pub const MAX_CHANNELS: usize = 500;

/// Discord's rule for comparing names: NFKC, lowercase, trimmed, whitespace collapsed.
pub fn normalize_name(s: &str) -> String {
    let n: String = s.nfkc().collect();
    n.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A name the way Discord holds a text channel name, for comparison: normalized, spaces to hyphens.
fn text_key(s: &str) -> String {
    normalize_name(s).replace(' ', "-")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SrRoleIn {
    pub id: i64,
    pub name: String,
    pub managed: bool,
    pub position: i32,
    pub is_everyone: bool,
    pub permissions: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SrChannelIn {
    pub id: i64,
    pub kind: i16,
    pub name: String,
    pub parent_id: Option<i64>,
    pub position: i32,
    pub e2ee: bool,
}

/// SIDE ROOM's side of the guild. Held (pending delete) channels and archived threads are left out by the reader.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SrSnapshot {
    pub roles: Vec<SrRoleIn>,
    pub channels: Vec<SrChannelIn>,
    pub staff_guild: bool,
}

/// Discord's side: roles, channels, and the active threads.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiscordSnapshot {
    pub discord_guild_id: i64,
    pub roles: Vec<GuildRole>,
    pub channels: Vec<GuildChannel>,
    pub threads: Vec<GuildChannel>,
}

/// Pairs that already exist (selective maps), pinned through the matching.
#[derive(Debug, Clone, Default)]
pub struct Pinned {
    /// (SIDE ROOM channel id, Discord channel id).
    pub channels: Vec<(i64, i64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Role,
    Category,
    Channel,
    Thread,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Both sides exist and were matched by the rules.
    Match,
    /// An existing selective map.
    Pinned,
    /// Only on SIDE ROOM: created on Discord.
    CreateOnDiscord,
    /// Only on Discord: created here.
    CreateHere,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanItem {
    pub kind: ItemKind,
    pub action: Action,
    pub sr_id: Option<i64>,
    pub discord_id: Option<i64>,
    pub name: String,
    /// The SIDE ROOM channel type, or the Discord type for a create-here item.
    pub channel_type: Option<i64>,
    /// For a role: the shared permission word SIDE ROOM will push (matched) or Discord has (create here).
    pub sr_permissions: u64,
    pub discord_permissions: u64,
    /// For a create-here item: import Discord's shared permission bits (the owner can switch it off, Q4).
    pub import: bool,
    /// Why the type will be degraded on Discord (no Community), filled by the reader that knows the guild.
    pub degraded: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotMirrored {
    pub kind: ItemKind,
    pub sr_id: Option<i64>,
    pub discord_id: Option<i64>,
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Blocker {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub items: Vec<PlanItem>,
    pub not_mirrored: Vec<NotMirrored>,
    /// "sr" or "discord": whose names, topics and order win for matched channels.
    pub seed_structure: String,
    pub estimated_writes: u32,
}

impl Plan {
    /// Items of one kind and action.
    pub fn count(&self, kind: ItemKind, action: Action) -> usize {
        self.items
            .iter()
            .filter(|i| i.kind == kind && i.action == action)
            .count()
    }

    /// Roles on Discord after the seed (matched plus the union of creates), against Discord's cap.
    pub fn role_total(&self) -> usize {
        self.items
            .iter()
            .filter(|i| i.kind == ItemKind::Role)
            .count()
    }

    pub fn channel_total(&self) -> usize {
        self.items
            .iter()
            .filter(|i| matches!(i.kind, ItemKind::Category | ItemKind::Channel))
            .count()
    }

    /// The hash of the plan the owner confirms. The owner's edits are part of it.
    pub fn hash(&self) -> String {
        let v = serde_json::to_vec(self).unwrap_or_default();
        hex(&Sha256::digest(v))
    }

    fn recount(&mut self, writes_per_unit: u32) {
        let creates = self
            .items
            .iter()
            .filter(|i| i.action == Action::CreateOnDiscord)
            .count() as u32;
        let matched = self
            .items
            .iter()
            .filter(|i| matches!(i.action, Action::Match | Action::Pinned))
            .count() as u32;
        let parents = self
            .items
            .iter()
            .filter(|i| i.kind == ItemKind::Category)
            .count() as u32
            + 1;
        // Creates, one role-order call, one position call per parent, one overwrite or permission push per
        // matched role or channel.
        self.estimated_writes = (creates + 1 + parents + matched) * writes_per_unit;
    }

    /// Unlink a matched pair: both become creates. Pinned pairs (existing selective maps) cannot be unlinked.
    pub fn unlink(&mut self, index: usize) -> Result<(), &'static str> {
        let item = self.items.get(index).ok_or("no such row")?.clone();
        match item.action {
            Action::Match => {}
            Action::Pinned => return Err("an existing map cannot be unlinked here"),
            _ => return Err("only a matched pair can be unlinked"),
        }
        let mut sr_side = item.clone();
        sr_side.action = Action::CreateOnDiscord;
        sr_side.discord_id = None;
        sr_side.discord_permissions = 0;
        let mut d_side = item;
        d_side.action = Action::CreateHere;
        d_side.sr_id = None;
        d_side.sr_permissions = 0;
        d_side.import = true;
        self.items[index] = sr_side;
        self.items.push(d_side);
        self.recount(1);
        Ok(())
    }

    /// Link two unmatched objects of the same kind by hand.
    pub fn link(
        &mut self,
        kind: ItemKind,
        sr_id: i64,
        discord_id: i64,
    ) -> Result<(), &'static str> {
        let si = self
            .items
            .iter()
            .position(|i| {
                i.kind == kind && i.action == Action::CreateOnDiscord && i.sr_id == Some(sr_id)
            })
            .ok_or("that SIDE ROOM object is not waiting to be created on Discord")?;
        let di = self
            .items
            .iter()
            .position(|i| {
                i.kind == kind && i.action == Action::CreateHere && i.discord_id == Some(discord_id)
            })
            .ok_or("that Discord object is not waiting to be created here")?;
        let d = self.items[di].clone();
        let s = &mut self.items[si];
        s.action = Action::Match;
        s.discord_id = Some(discord_id);
        s.discord_permissions = d.discord_permissions;
        self.items.remove(di);
        self.recount(1);
        Ok(())
    }

    /// Switch the import of Discord's permissions on or off for one create-here row.
    pub fn set_import(&mut self, index: usize, import: bool) -> Result<(), &'static str> {
        let item = self.items.get_mut(index).ok_or("no such row")?;
        if item.action != Action::CreateHere {
            return Err("only objects that exist only on Discord have an import");
        }
        item.import = import;
        Ok(())
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Hash of SIDE ROOM's side of the guild, for the stale check at confirm.
pub fn sr_hash(sr: &SrSnapshot) -> String {
    let mut roles = sr.roles.clone();
    roles.sort_by_key(|r| r.id);
    let mut channels = sr.channels.clone();
    channels.sort_by_key(|c| c.id);
    let v = serde_json::to_vec(&(roles, channels)).unwrap_or_default();
    hex(&Sha256::digest(v))
}

/// Hash of Discord's side (names, ids, types, parents, positions, permission words and overwrites).
pub fn discord_hash(d: &DiscordSnapshot) -> String {
    let mut roles: Vec<_> = d
        .roles
        .iter()
        .map(|r| (r.id, r.name.clone(), r.position, r.permissions, r.managed))
        .collect();
    roles.sort();
    let mut chans: Vec<_> = d
        .channels
        .iter()
        .chain(d.threads.iter())
        .map(|c| {
            let mut ow: Vec<_> = c
                .overwrites
                .iter()
                .map(|o| (o.id, o.kind, o.allow, o.deny))
                .collect();
            ow.sort();
            (c.id, c.kind, c.name.clone(), c.parent_id, c.position, ow)
        })
        .collect();
    chans.sort();
    let v = serde_json::to_vec(&(roles, chans)).unwrap_or_default();
    hex(&Sha256::digest(v))
}

/// Pair two lists by key; equal keys are paired in position order. Returns the pairs (indices) and the leftovers.
fn pair_by<K: std::hash::Hash + Eq + Clone>(
    left: &[(K, i64)],
    right: &[(K, i64)],
) -> (Vec<(usize, usize)>, Vec<usize>, Vec<usize>) {
    let mut by_key: HashMap<K, Vec<usize>> = HashMap::new();
    for (j, (k, _)) in right.iter().enumerate() {
        by_key.entry(k.clone()).or_default().push(j);
    }
    for v in by_key.values_mut() {
        v.sort_by_key(|&j| (right[j].1, j));
        v.reverse(); // pop() takes the lowest position first
    }
    let mut order: Vec<usize> = (0..left.len()).collect();
    order.sort_by_key(|&i| (left[i].1, i));
    let mut pairs = Vec::new();
    let mut left_over = Vec::new();
    let mut used = HashSet::new();
    for i in order {
        match by_key.get_mut(&left[i].0).and_then(Vec::pop) {
            Some(j) => {
                used.insert(j);
                pairs.push((i, j));
            }
            None => left_over.push(i),
        }
    }
    let right_over = (0..right.len()).filter(|j| !used.contains(j)).collect();
    left_over.sort_unstable();
    (pairs, left_over, right_over)
}

fn family(kind: i64) -> u8 {
    match kind {
        0 | 5 => 0,
        15 | 16 => 1,
        2 | 13 => 2,
        4 => 3,
        _ => 4,
    }
}

/// Build the plan from both sides. `shared_mask` is the permission mask sent to Discord (`BRIDGE_SHARED`).
pub fn build(
    sr: &SrSnapshot,
    d: &DiscordSnapshot,
    pinned: &Pinned,
    seed_structure: &str,
    shared_mask: u64,
) -> Plan {
    let mut plan = Plan {
        seed_structure: seed_structure.to_owned(),
        ..Plan::default()
    };

    // ---- roles ----
    let mut sr_roles: Vec<&SrRoleIn> = Vec::new();
    for r in &sr.roles {
        if r.managed {
            plan.not_mirrored.push(NotMirrored {
                kind: ItemKind::Role,
                sr_id: Some(r.id),
                discord_id: None,
                name: r.name.clone(),
                reason: "A managed role is never mirrored.".into(),
            });
        } else {
            sr_roles.push(r);
        }
    }
    let mut d_roles: Vec<&GuildRole> = Vec::new();
    for r in &d.roles {
        if r.managed {
            plan.not_mirrored.push(NotMirrored {
                kind: ItemKind::Role,
                sr_id: None,
                discord_id: Some(r.id),
                name: r.name.clone(),
                reason: "A Discord managed role (bot, booster or integration) is never mirrored."
                    .into(),
            });
        } else {
            d_roles.push(r);
        }
    }
    let mut sr_free: Vec<&SrRoleIn> = Vec::new();
    let mut d_free: Vec<&GuildRole> = d_roles.clone();
    for r in sr_roles {
        if r.is_everyone {
            if let Some(pos) = d_free.iter().position(|x| x.id == d.discord_guild_id) {
                let x = d_free.remove(pos);
                plan.items.push(PlanItem {
                    kind: ItemKind::Role,
                    action: Action::Match,
                    sr_id: Some(r.id),
                    discord_id: Some(x.id),
                    name: "@everyone".into(),
                    channel_type: None,
                    sr_permissions: (r.permissions as u64) & shared_mask,
                    discord_permissions: x.permissions,
                    import: false,
                    degraded: None,
                });
                continue;
            }
        }
        sr_free.push(r);
    }
    let left: Vec<(String, i64)> = sr_free
        .iter()
        .map(|r| (normalize_name(&r.name), r.position as i64))
        .collect();
    let right: Vec<(String, i64)> = d_free
        .iter()
        .map(|r| (normalize_name(&r.name), r.position))
        .collect();
    let (pairs, l_over, r_over) = pair_by(&left, &right);
    for (i, j) in pairs {
        let (s, x) = (sr_free[i], d_free[j]);
        plan.items.push(PlanItem {
            kind: ItemKind::Role,
            action: Action::Match,
            sr_id: Some(s.id),
            discord_id: Some(x.id),
            name: s.name.clone(),
            channel_type: None,
            sr_permissions: (s.permissions as u64) & shared_mask,
            discord_permissions: x.permissions,
            import: false,
            degraded: None,
        });
    }
    for i in l_over {
        let s = sr_free[i];
        plan.items.push(PlanItem {
            kind: ItemKind::Role,
            action: Action::CreateOnDiscord,
            sr_id: Some(s.id),
            discord_id: None,
            name: s.name.clone(),
            channel_type: None,
            sr_permissions: (s.permissions as u64) & shared_mask,
            discord_permissions: 0,
            import: false,
            degraded: None,
        });
    }
    for j in r_over {
        let x = d_free[j];
        plan.items.push(PlanItem {
            kind: ItemKind::Role,
            action: Action::CreateHere,
            sr_id: None,
            discord_id: Some(x.id),
            name: x.name.clone(),
            channel_type: None,
            sr_permissions: 0,
            discord_permissions: x.permissions,
            import: true,
            degraded: None,
        });
    }

    // ---- channels: eligibility ----
    let mut sr_ch: Vec<&SrChannelIn> = Vec::new();
    for c in &sr.channels {
        match bridgeable(Some(1), c.kind, c.e2ee, sr.staff_guild) {
            Ok(Mirrored::Messages | Mirrored::StructureOnly) => sr_ch.push(c),
            Err(why) => plan.not_mirrored.push(NotMirrored {
                kind: ItemKind::Channel,
                sr_id: Some(c.id),
                discord_id: None,
                name: c.name.clone(),
                reason: why.reason().into(),
            }),
        }
    }
    let mut d_ch: Vec<&GuildChannel> = Vec::new();
    for c in &d.channels {
        if matches!(c.kind, 4 | 0 | 5 | 2 | 13 | 15 | 16) {
            d_ch.push(c);
        } else {
            plan.not_mirrored.push(NotMirrored {
                kind: ItemKind::Channel,
                sr_id: None,
                discord_id: Some(c.id),
                name: c.name.clone(),
                reason: "This Discord channel type has no SIDE ROOM counterpart.".into(),
            });
        }
    }

    // sr id -> discord id of everything matched so far (categories first, then children by parent).
    let mut matched: BTreeMap<i64, i64> = BTreeMap::new();
    let mut sr_left: Vec<&SrChannelIn> = sr_ch.clone();
    let mut d_left: Vec<&GuildChannel> = d_ch.clone();

    // Step 1: pinned selective pairs.
    for (sid, did) in &pinned.channels {
        let (Some(si), Some(di)) = (
            sr_left.iter().position(|c| c.id == *sid),
            d_left.iter().position(|c| c.id == *did),
        ) else {
            continue;
        };
        let (s, x) = (sr_left.remove(si), d_left.remove(di));
        matched.insert(s.id, x.id);
        plan.items.push(channel_item(s, Some(x), Action::Pinned));
    }
    // Step 4: categories by normalized name.
    {
        let l_idx: Vec<usize> = (0..sr_left.len())
            .filter(|&i| sr_left[i].kind == 4)
            .collect();
        let r_idx: Vec<usize> = (0..d_left.len()).filter(|&j| d_left[j].kind == 4).collect();
        let left: Vec<(String, i64)> = l_idx
            .iter()
            .map(|&i| (normalize_name(&sr_left[i].name), sr_left[i].position as i64))
            .collect();
        let right: Vec<(String, i64)> = r_idx
            .iter()
            .map(|&j| (normalize_name(&d_left[j].name), d_left[j].position))
            .collect();
        let (pairs, _, _) = pair_by(&left, &right);
        let mut take_l = Vec::new();
        let mut take_r = Vec::new();
        for (a, b) in pairs {
            let (i, j) = (l_idx[a], r_idx[b]);
            matched.insert(sr_left[i].id, d_left[j].id);
            plan.items
                .push(channel_item(sr_left[i], Some(d_left[j]), Action::Match));
            take_l.push(i);
            take_r.push(j);
        }
        sr_left = sr_left
            .into_iter()
            .enumerate()
            .filter(|(i, _)| !take_l.contains(i))
            .map(|(_, c)| c)
            .collect();
        d_left = d_left
            .into_iter()
            .enumerate()
            .filter(|(j, _)| !take_r.contains(j))
            .map(|(_, c)| c)
            .collect();
    }
    // Step 5: other channels by (name, family, matched parent). The parent key is the Discord parent id.
    {
        let l_idx: Vec<usize> = (0..sr_left.len())
            .filter(|&i| sr_left[i].kind != 4)
            .collect();
        let r_idx: Vec<usize> = (0..d_left.len()).filter(|&j| d_left[j].kind != 4).collect();
        let key_l = |c: &SrChannelIn| {
            (
                family(c.kind as i64),
                if family(c.kind as i64) == 2 {
                    normalize_name(&c.name)
                } else {
                    text_key(&c.name)
                },
                c.parent_id.and_then(|p| matched.get(&p).copied()),
                c.parent_id.is_some(),
            )
        };
        let key_r = |c: &GuildChannel| {
            (
                family(c.kind),
                if family(c.kind) == 2 {
                    normalize_name(&c.name)
                } else {
                    text_key(&c.name)
                },
                c.parent_id,
                c.parent_id.is_some(),
            )
        };
        let left: Vec<(_, i64)> = l_idx
            .iter()
            .map(|&i| (key_l(sr_left[i]), sr_left[i].position as i64))
            .collect();
        let right: Vec<(_, i64)> = r_idx
            .iter()
            .map(|&j| (key_r(d_left[j]), d_left[j].position))
            .collect();
        let (pairs, _, _) = pair_by(&left, &right);
        let mut take_l = Vec::new();
        let mut take_r = Vec::new();
        for (a, b) in pairs {
            let (i, j) = (l_idx[a], r_idx[b]);
            matched.insert(sr_left[i].id, d_left[j].id);
            plan.items
                .push(channel_item(sr_left[i], Some(d_left[j]), Action::Match));
            take_l.push(i);
            take_r.push(j);
        }
        sr_left = sr_left
            .into_iter()
            .enumerate()
            .filter(|(i, _)| !take_l.contains(i))
            .map(|(_, c)| c)
            .collect();
        d_left = d_left
            .into_iter()
            .enumerate()
            .filter(|(j, _)| !take_r.contains(j))
            .map(|(_, c)| c)
            .collect();
    }
    for c in sr_left {
        plan.items
            .push(channel_item(c, None, Action::CreateOnDiscord));
    }
    for x in d_left {
        plan.items.push(discord_only_item(x));
    }

    // ---- threads (step 6): by (matched parent, name) ----
    let sr_threads: Vec<&SrChannelIn> = sr
        .channels
        .iter()
        .filter(|c| matches!(c.kind, 10..=12))
        .collect();
    // Threads were put in `sr_ch` above (bridgeable says Messages); take them out of the channel items.
    plan.items
        .retain(|i| !(i.kind == ItemKind::Channel && matches!(i.channel_type, Some(10..=12))));
    let sr_t: Vec<&SrChannelIn> = sr_threads
        .into_iter()
        .filter(|t| t.parent_id.is_some_and(|p| matched.contains_key(&p)) || t.parent_id.is_some())
        .collect();
    let left: Vec<((Option<i64>, String), i64)> = sr_t
        .iter()
        .map(|t| {
            (
                (
                    t.parent_id.and_then(|p| matched.get(&p).copied()),
                    text_key(&t.name),
                ),
                t.position as i64,
            )
        })
        .collect();
    let right: Vec<((Option<i64>, String), i64)> = d
        .threads
        .iter()
        .map(|t| ((t.parent_id, text_key(&t.name)), t.position))
        .collect();
    let (pairs, l_over, r_over) = pair_by(&left, &right);
    for (i, j) in pairs {
        let mut it = channel_item(sr_t[i], Some(&d.threads[j]), Action::Match);
        it.kind = ItemKind::Thread;
        plan.items.push(it);
    }
    for i in l_over {
        let mut it = channel_item(sr_t[i], None, Action::CreateOnDiscord);
        it.kind = ItemKind::Thread;
        plan.items.push(it);
    }
    for j in r_over {
        let mut it = discord_only_item(&d.threads[j]);
        it.kind = ItemKind::Thread;
        plan.items.push(it);
    }

    plan.recount(1);
    plan
}

fn channel_item(s: &SrChannelIn, x: Option<&GuildChannel>, action: Action) -> PlanItem {
    PlanItem {
        kind: if s.kind == 4 {
            ItemKind::Category
        } else {
            ItemKind::Channel
        },
        action,
        sr_id: Some(s.id),
        discord_id: x.map(|c| c.id),
        name: s.name.clone(),
        channel_type: Some(s.kind as i64),
        sr_permissions: 0,
        discord_permissions: 0,
        import: false,
        degraded: None,
    }
}

fn discord_only_item(x: &GuildChannel) -> PlanItem {
    PlanItem {
        kind: if x.kind == channel_type::CATEGORY {
            ItemKind::Category
        } else {
            ItemKind::Channel
        },
        action: Action::CreateHere,
        sr_id: None,
        discord_id: Some(x.id),
        name: x.name.clone(),
        channel_type: Some(x.kind),
        sr_permissions: 0,
        discord_permissions: 0,
        import: true,
        degraded: None,
    }
}

/// The facts the preflight checks (design 9.2).
#[derive(Debug, Clone)]
pub struct PreflightFacts {
    pub bot_administrator: bool,
    pub bot_is_top_role: bool,
    pub intents_granted: u64,
    /// Another whole_guild bridge on this instance owns the same Discord server.
    pub discord_guild_taken: bool,
}

/// The blockers of a plan. Empty means the owner may confirm.
pub fn blockers(plan: &Plan, f: &PreflightFacts) -> Vec<Blocker> {
    use crate::discord_structure::intents;
    let mut out = Vec::new();
    let mut add = |code: &str, message: &str| {
        out.push(Blocker {
            code: code.into(),
            message: message.into(),
        })
    };
    if !f.bot_administrator {
        add(
            "bot_not_admin",
            "The bot needs the Administrator permission on the Discord server.",
        );
    }
    if !f.bot_is_top_role {
        add(
            "bot_role_not_top",
            "The bot's role must be the highest role on the Discord server; it can only edit roles below its own.",
        );
    }
    let need = intents::GUILD_MEMBERS | intents::MESSAGE_CONTENT;
    if f.intents_granted & need != need {
        add(
            "intents_missing",
            "The Server Members and Message Content intents must be enabled for the bot in the Discord developer portal.",
        );
    }
    if plan.role_total() > MAX_ROLES {
        add(
            "role_cap",
            "The two servers together have more than 250 roles.",
        );
    }
    if plan.channel_total() > MAX_CHANNELS {
        add(
            "channel_cap",
            "The two servers together have more than 500 channels.",
        );
    }
    if f.discord_guild_taken {
        add(
            "discord_guild_taken",
            "Another SIDE ROOM server on this instance already mirrors this Discord server.",
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discord_structure::Overwrite;

    fn role(id: i64, name: &str, pos: i32, perms: i64) -> SrRoleIn {
        SrRoleIn {
            id,
            name: name.into(),
            managed: false,
            position: pos,
            is_everyone: false,
            permissions: perms,
        }
    }

    fn drole(id: i64, name: &str, pos: i64, perms: u64, managed: bool) -> GuildRole {
        GuildRole {
            id,
            name: name.into(),
            color: 0,
            hoist: false,
            position: pos,
            permissions: perms,
            managed,
            mentionable: false,
            unicode_emoji: None,
            icon: None,
            bot_id: None,
        }
    }

    fn ch(id: i64, kind: i16, name: &str, parent: Option<i64>, pos: i32) -> SrChannelIn {
        SrChannelIn {
            id,
            kind,
            name: name.into(),
            parent_id: parent,
            position: pos,
            e2ee: false,
        }
    }

    fn dch(id: i64, kind: i64, name: &str, parent: Option<i64>, pos: i64) -> GuildChannel {
        GuildChannel {
            id,
            kind,
            name: name.into(),
            parent_id: parent,
            position: pos,
            topic: None,
            nsfw: false,
            rate_limit_per_user: 0,
            bitrate: None,
            user_limit: None,
            video_quality_mode: None,
            overwrites: vec![Overwrite::role(1, 0, 0)],
            default_auto_archive_duration: None,
            default_thread_rate_limit_per_user: None,
            available_tags: vec![],
            applied_tags: vec![],
            default_reaction_emoji_name: None,
            default_sort_order: None,
            default_forum_layout: None,
            flags: 0,
            thread: None,
            owner_id: None,
        }
    }

    const MASK: u64 = u64::MAX >> 1;

    fn world() -> (SrSnapshot, DiscordSnapshot) {
        let mut everyone = role(100, "@everyone", 0, 0x400);
        everyone.is_everyone = true;
        let sr = SrSnapshot {
            roles: vec![
                everyone,
                role(1, "Mods", 2, 0x2),
                role(2, "Admins", 3, 0x8),
                role(3, "Only Here", 1, 0),
            ],
            channels: vec![
                ch(10, 4, "Text Rooms", None, 0),
                ch(11, 0, "General Chat", Some(10), 0),
                ch(12, 0, "sr only", Some(10), 1),
                ch(13, 2, "Hangout", Some(10), 2),
                ch(14, 0, "Top Level", None, 1),
                ch(20, 11, "Plans", Some(11), 0),
                ch(21, 11, "Only Thread", Some(11), 1),
            ],
            staff_guild: false,
        };
        let d = DiscordSnapshot {
            discord_guild_id: 900,
            roles: vec![
                drole(900, "@everyone", 0, 0x400 | 0x1, false),
                drole(901, "mods", 2, 0x2 | 0x4, false),
                drole(902, "Only There", 1, 0x8, false),
                drole(903, "Server Booster", 5, 0, true),
            ],
            channels: vec![
                dch(910, 4, "text rooms", None, 0),
                dch(911, 0, "general-chat", Some(910), 0),
                dch(913, 2, "hangout", Some(910), 2),
                dch(915, 0, "there-only", Some(910), 3),
                dch(916, 14, "directory", None, 9),
            ],
            threads: vec![
                dch(930, 11, "plans", Some(911), 0),
                dch(931, 11, "Only Over There", Some(911), 1),
            ],
        };
        (sr, d)
    }

    fn find<'a>(p: &'a Plan, kind: ItemKind, name: &str) -> &'a PlanItem {
        p.items
            .iter()
            .find(|i| i.kind == kind && i.name == name)
            .unwrap_or_else(|| panic!("no {kind:?} {name}: {:#?}", p.items))
    }

    #[test]
    fn matching_follows_the_order_of_the_design() {
        let (sr, d) = world();
        let p = build(&sr, &d, &Pinned::default(), "sr", MASK);
        let ev = find(&p, ItemKind::Role, "@everyone");
        assert_eq!(
            (ev.action, ev.sr_id, ev.discord_id),
            (Action::Match, Some(100), Some(900))
        );
        assert_eq!(find(&p, ItemKind::Role, "Mods").discord_id, Some(901));
        assert_eq!(
            find(&p, ItemKind::Role, "Admins").action,
            Action::CreateOnDiscord
        );
        assert_eq!(
            find(&p, ItemKind::Role, "Only There").action,
            Action::CreateHere
        );
        assert_eq!(
            find(&p, ItemKind::Category, "Text Rooms").discord_id,
            Some(910)
        );
        assert_eq!(
            find(&p, ItemKind::Channel, "General Chat").discord_id,
            Some(911)
        );
        assert_eq!(find(&p, ItemKind::Channel, "Hangout").discord_id, Some(913));
        assert_eq!(
            find(&p, ItemKind::Channel, "sr only").action,
            Action::CreateOnDiscord
        );
        assert_eq!(
            find(&p, ItemKind::Channel, "there-only").action,
            Action::CreateHere
        );
        assert_eq!(
            find(&p, ItemKind::Channel, "Top Level").action,
            Action::CreateOnDiscord
        );
        assert_eq!(find(&p, ItemKind::Thread, "Plans").discord_id, Some(930));
        assert_eq!(
            find(&p, ItemKind::Thread, "Only Thread").action,
            Action::CreateOnDiscord
        );
        assert_eq!(
            find(&p, ItemKind::Thread, "Only Over There").action,
            Action::CreateHere
        );
        // Managed roles and directory channels are listed as not mirrored.
        assert!(p.not_mirrored.iter().any(|n| n.discord_id == Some(903)));
        assert!(p.not_mirrored.iter().any(|n| n.discord_id == Some(916)));
    }

    #[test]
    fn matched_roles_carry_the_shared_words_for_the_diff() {
        let (sr, d) = world();
        let p = build(&sr, &d, &Pinned::default(), "sr", MASK);
        let m = find(&p, ItemKind::Role, "Mods");
        assert_eq!((m.sr_permissions, m.discord_permissions), (0x2, 0x6));
        // Masked: an SIDE ROOM-only bit never shows as something Discord will get.
        let mut sr2 = sr.clone();
        sr2.roles[1].permissions = 0x2 | (1 << 56);
        let p2 = build(&sr2, &d, &Pinned::default(), "sr", (1u64 << 53) - 1);
        assert_eq!(find(&p2, ItemKind::Role, "Mods").sr_permissions, 0x2);
    }

    #[test]
    fn duplicates_pair_in_position_order() {
        let sr = SrSnapshot {
            roles: vec![role(1, "Mod", 5, 0), role(2, "mod", 3, 0)],
            ..SrSnapshot::default()
        };
        let d = DiscordSnapshot {
            discord_guild_id: 9,
            roles: vec![drole(11, "MOD", 4, 0, false), drole(12, "Mod", 2, 0, false)],
            ..DiscordSnapshot::default()
        };
        let p = build(&sr, &d, &Pinned::default(), "sr", MASK);
        let pair = |sid| {
            p.items
                .iter()
                .find(|i| i.sr_id == Some(sid))
                .unwrap()
                .discord_id
        };
        assert_eq!(
            pair(2),
            Some(12),
            "lowest SR position with the lowest Discord position"
        );
        assert_eq!(pair(1), Some(11));
    }

    #[test]
    fn existing_selective_maps_are_pinned_even_when_names_differ() {
        let (sr, d) = world();
        let pinned = Pinned {
            channels: vec![(14, 915)],
        };
        let p = build(&sr, &d, &pinned, "sr", MASK);
        let it = find(&p, ItemKind::Channel, "Top Level");
        assert_eq!((it.action, it.discord_id), (Action::Pinned, Some(915)));
        assert!(
            p.items
                .iter()
                .all(|i| i.discord_id != Some(915) || i.sr_id == Some(14))
        );
    }

    #[test]
    fn nothing_matches_across_families_or_parents() {
        let sr = SrSnapshot {
            channels: vec![ch(1, 0, "lobby", None, 0), ch(2, 2, "lobby", None, 1)],
            ..SrSnapshot::default()
        };
        let d = DiscordSnapshot {
            channels: vec![dch(5, 2, "lobby", None, 0)],
            ..DiscordSnapshot::default()
        };
        let p = build(&sr, &d, &Pinned::default(), "sr", MASK);
        assert_eq!(
            p.items.iter().find(|i| i.sr_id == Some(1)).unwrap().action,
            Action::CreateOnDiscord
        );
        assert_eq!(
            p.items
                .iter()
                .find(|i| i.sr_id == Some(2))
                .unwrap()
                .discord_id,
            Some(5)
        );
    }

    #[test]
    fn unlink_link_and_import_edits() {
        let (sr, d) = world();
        let mut p = build(&sr, &d, &Pinned::default(), "sr", MASK);
        let h0 = p.hash();
        let idx = p
            .items
            .iter()
            .position(|i| i.kind == ItemKind::Role && i.name == "Mods")
            .unwrap();
        p.unlink(idx).unwrap();
        assert_ne!(p.hash(), h0, "an edit changes the hash the owner confirms");
        assert_eq!(p.items[idx].action, Action::CreateOnDiscord);
        assert!(
            p.items
                .iter()
                .any(|i| i.action == Action::CreateHere && i.discord_id == Some(901))
        );
        p.link(ItemKind::Role, 1, 901).unwrap();
        assert_eq!(find(&p, ItemKind::Role, "Mods").action, Action::Match);
        assert!(p.link(ItemKind::Role, 1, 901).is_err());
        let up = p
            .items
            .iter()
            .position(|i| i.action == Action::CreateHere && i.name == "Only There")
            .unwrap();
        p.set_import(up, false).unwrap();
        assert!(!p.items[up].import);
        assert!(p.set_import(0, true).is_err());
        let pinned = Pinned {
            channels: vec![(14, 915)],
        };
        let mut p2 = build(&sr, &d, &pinned, "sr", MASK);
        let pi = p2
            .items
            .iter()
            .position(|i| i.action == Action::Pinned)
            .unwrap();
        assert!(p2.unlink(pi).is_err());
    }

    #[test]
    fn hashes_move_when_either_side_moves() {
        let (sr, d) = world();
        let (hs, hd) = (sr_hash(&sr), discord_hash(&d));
        let mut sr2 = sr.clone();
        sr2.roles[1].permissions = 0x40;
        assert_ne!(sr_hash(&sr2), hs);
        let mut d2 = d.clone();
        d2.channels[1].overwrites.push(Overwrite::role(5, 1, 0));
        assert_ne!(discord_hash(&d2), hd);
        let mut shuffled = d.clone();
        shuffled.roles.reverse();
        assert_eq!(
            discord_hash(&shuffled),
            hd,
            "order of the lists does not matter"
        );
    }

    #[test]
    fn blockers_cover_the_preflight() {
        use crate::discord_structure::intents;
        let (sr, d) = world();
        let p = build(&sr, &d, &Pinned::default(), "sr", MASK);
        let ok = PreflightFacts {
            bot_administrator: true,
            bot_is_top_role: true,
            intents_granted: intents::GUILD_MEMBERS | intents::MESSAGE_CONTENT,
            discord_guild_taken: false,
        };
        assert!(blockers(&p, &ok).is_empty());
        let bad = PreflightFacts {
            bot_administrator: false,
            bot_is_top_role: false,
            intents_granted: 0,
            discord_guild_taken: true,
        };
        let codes: Vec<_> = blockers(&p, &bad).into_iter().map(|b| b.code).collect();
        assert_eq!(
            codes,
            [
                "bot_not_admin",
                "bot_role_not_top",
                "intents_missing",
                "discord_guild_taken"
            ]
        );
        let mut big = p.clone();
        for i in 0..260 {
            big.items.push(PlanItem {
                kind: ItemKind::Role,
                action: Action::CreateHere,
                sr_id: None,
                discord_id: Some(i),
                name: format!("r{i}"),
                channel_type: None,
                sr_permissions: 0,
                discord_permissions: 0,
                import: true,
                degraded: None,
            });
        }
        assert!(blockers(&big, &ok).iter().any(|b| b.code == "role_cap"));
    }

    #[test]
    fn names_normalize_like_discord() {
        assert_eq!(normalize_name("  Ｇeneral   Chat "), "general chat");
        assert_eq!(text_key("General Chat"), "general-chat");
    }
}
