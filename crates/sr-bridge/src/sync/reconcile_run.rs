//! The reconcile pass (design sections 2.1, 6.2 to 6.4, 11.1 and 11.4): a three-way look at every mapped object,
//! then the objects that exist on one side only.
//!
//! What it does, per mapped role and channel, for the two-way fields: SIDE ROOM changed -> push; Discord changed
//! -> apply here through the [`SrStructure`] port; both changed to different values -> SIDE ROOM wins. A role or
//! channel that vanished from Discord follows the guarded rules (a role that denies something in a channel here
//! is recreated, any other is deleted here; a channel is held). An object that exists on one side only is
//! created on the other (a role created from Discord has NO permissions here).
//!
//! What it does not do: permission words, overwrites, role order and member overwrites. Those are BRG-SYN-002's
//! (`sync::perms`, `sync::overwrites`), reached through the [`OverwritePush`] hook for channels.

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use serde_json::{Value, json};

use super::engine::{AUDIT_SYNC_RUN, SyncEngine};
use super::holds;
use super::patch::{ChannelPatch, RoleStructurePatch, SrStructuralPatch};
use super::port::{BridgeOrigin, SrNewChannel, SrNewRole};
use super::reconcile::{Authority, Decision, FieldView, decide, discord_name_for, field_authority};
use super::store::{self, NewStructureMap, Origin, RoleMap};
use super::types::{ObjectKind, SyncMode, SyncedState};
use crate::discord_structure::{ChannelParams, GuildChannel, RoleParams, reason_for_op};
use crate::error::{BridgeError, BridgeResult};
use crate::model::get_bridge;
use crate::service::{AuditRow, SYSTEM_USER_ID};
use sr_core::permissions::{DISCORD_ONLY_MASK, Permissions};

/// What one pass did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ReconcileReport {
    pub pushed: u32,
    pub applied: u32,
    pub converged: u32,
    /// Both sides changed a field; SIDE ROOM's value won.
    pub conflicts: u32,
    /// Roles whose permission word on Discord differed from SIDE ROOM's and was put back.
    pub reverted: u32,
    pub created_here: u32,
    pub created_on_discord: u32,
    pub held: u32,
    pub roles_deleted_here: u32,
    pub roles_recreated_on_discord: u32,
}

impl ReconcileReport {
    pub fn changed(&self) -> bool {
        self.pushed
            + self.applied
            + self.created_here
            + self.created_on_discord
            + self.held
            + self.roles_deleted_here
            + self.roles_recreated_on_discord
            > 0
    }
}

/// One field of one object, ready for [`decide`].
struct Field {
    name: &'static str,
    sr: Value,
    discord: Value,
    /// How a SIDE ROOM value is held on Discord (a channel name is normalized).
    to_discord: fn(&Value, i16) -> Value,
}

fn same(v: &Value, _k: i16) -> Value {
    v.clone()
}

fn channel_name(v: &Value, kind: i16) -> Value {
    json!(discord_name_for(kind, v.as_str().unwrap_or_default()))
}

fn opt_str(s: &Option<String>) -> Value {
    s.as_ref().map_or(Value::Null, |s| json!(s))
}

impl SyncEngine {
    /// One full three-way pass over a whole_guild bridge that is allowed to run ops.
    pub async fn reconcile_guild(&self, guild: i64) -> BridgeResult<ReconcileReport> {
        let sync = store::get_sync(self.pool(), guild)
            .await?
            .ok_or(BridgeError::NotFound("bridge"))?;
        if sync.mode != SyncMode::WholeGuild || !sync.state.runs_ops() {
            return Err(BridgeError::Conflict(
                "Sync is not running for this server.".into(),
            ));
        }
        let bridge = get_bridge(self.pool(), guild)
            .await?
            .ok_or(BridgeError::NotFound("bridge"))?;
        let token = self.svc.open_bot_token(guild).await?;
        let dg = bridge.discord_guild_id;
        let api = &self.svc.discord;
        let op = self.svc.next_id();
        let reason = reason_for_op(op);
        let origin = BridgeOrigin::bridge(op);
        let d = self.read_discord(&token, dg).await?;
        let info = api.guild_info(&token, dg).await?;
        let mut rep = ReconcileReport::default();

        // ---- roles ----
        let sr_roles = self.sr_roles(guild).await?;
        let d_roles: HashMap<i64, &crate::discord_structure::GuildRole> =
            d.roles.iter().map(|r| (r.id, r)).collect();
        let role_maps = store::list_role_maps(self.pool(), guild).await?;
        for m in &role_maps {
            if m.sync_status == "frozen" {
                continue;
            }
            let Some(sr) = sr_roles.get(&m.role_id) else {
                continue;
            };
            let Some(dr) = d_roles.get(&m.discord_role_id) else {
                // Deleted on Discord (design 6.3): recreate when it protects something, else delete here.
                if m.is_everyone {
                    continue;
                }
                if self.sr.role_has_denies(guild, m.role_id).await? {
                    let word = Permissions::from_bits_truncate(sr.permissions as u64)
                        .intersection(Permissions::BRIDGE_SHARED)
                        .bits()
                        | m.discord_only_bits;
                    let created = api
                        .create_role(
                            &token,
                            dg,
                            &RoleParams {
                                name: Some(sr.name.clone()),
                                permissions: Some(word),
                                color: Some(i64::from(sr.color)),
                                hoist: Some(sr.hoist),
                                mentionable: Some(sr.mentionable),
                                unicode_emoji: sr.unicode_emoji.clone().map(Some),
                            },
                            &reason,
                        )
                        .await?;
                    sqlx::query("UPDATE bridge_role_maps SET discord_role_id = $2, updated_at = now() WHERE id = $1")
                        .bind(m.id)
                        .bind(created.id)
                        .execute(self.pool())
                        .await?;
                    rep.roles_recreated_on_discord += 1;
                } else {
                    self.sr.delete_role(guild, m.role_id, &origin).await?;
                    sqlx::query("DELETE FROM bridge_role_maps WHERE id = $1")
                        .bind(m.id)
                        .execute(self.pool())
                        .await?;
                    rep.roles_deleted_here += 1;
                }
                continue;
            };
            let fields = [
                Field {
                    name: "name",
                    sr: json!(sr.name),
                    discord: json!(dr.name),
                    to_discord: same,
                },
                Field {
                    name: "color",
                    sr: json!(sr.color),
                    discord: json!(dr.color),
                    to_discord: same,
                },
                Field {
                    name: "hoist",
                    sr: json!(sr.hoist),
                    discord: json!(dr.hoist),
                    to_discord: same,
                },
                Field {
                    name: "mentionable",
                    sr: json!(sr.mentionable),
                    discord: json!(dr.mentionable),
                    to_discord: same,
                },
            ];
            let mut synced = m.synced.clone();
            let mut push = RoleParams::default();
            let mut patch = RoleStructurePatch::default();
            let mut touched = false;
            for f in &fields {
                if m.is_everyone && f.name == "name" {
                    continue; // @everyone's name is fixed on both platforms
                }
                let dec = self.decide_field(ObjectKind::Role, f, 0, synced.get(f.name));
                match dec {
                    Decision::Nothing => {}
                    Decision::Converged => {
                        synced.record(f.name, f.sr.clone(), f.discord.clone());
                        rep.converged += 1;
                    }
                    Decision::PushToDiscord | Decision::Conflict | Decision::Revert => {
                        if dec == Decision::Conflict {
                            rep.conflicts += 1;
                        }
                        match f.name {
                            "name" => push.name = Some(sr.name.clone()),
                            "color" => push.color = Some(i64::from(sr.color)),
                            "hoist" => push.hoist = Some(sr.hoist),
                            _ => push.mentionable = Some(sr.mentionable),
                        }
                        synced.record(f.name, f.sr.clone(), f.sr.clone());
                        touched = true;
                        rep.pushed += 1;
                    }
                    Decision::ApplyToSr => {
                        match f.name {
                            "name" => patch.name = Some(dr.name.clone()),
                            "color" => patch.color = Some(i32::try_from(dr.color).unwrap_or(0)),
                            "hoist" => patch.hoist = Some(dr.hoist),
                            _ => patch.mentionable = Some(dr.mentionable),
                        }
                        synced.record(f.name, f.discord.clone(), f.discord.clone());
                        rep.applied += 1;
                    }
                }
            }
            if touched {
                api.edit_role(&token, dg, dr.id, &push, &reason).await?;
            }
            if !patch.is_empty() {
                self.sr
                    .apply(
                        guild,
                        &SrStructuralPatch::Role {
                            role_id: m.role_id,
                            patch,
                        },
                        &origin,
                    )
                    .await?;
            }
            // Permissions are SIDE ROOM's: a Discord edit is put back (design 5.3). The conflict row and audit entry
            // belong to BRG-SYN-002's conflict log; this keeps Discord equal to the master copy.
            let want = Permissions::from_bits_truncate(sr.permissions as u64)
                .intersection(Permissions::BRIDGE_SHARED)
                .bits()
                | (dr.permissions & DISCORD_ONLY_MASK);
            if dr.permissions != want {
                api.edit_role(
                    &token,
                    dg,
                    dr.id,
                    &RoleParams {
                        permissions: Some(want),
                        ..RoleParams::default()
                    },
                    &reason,
                )
                .await?;
                rep.reverted += 1;
            }
            let only = dr.permissions & DISCORD_ONLY_MASK;
            if synced != m.synced || only != m.discord_only_bits {
                store::save_role_state(self.pool(), m.id, &synced, only).await?;
            }
        }

        // ---- roles on one side only ----
        let mapped_sr: HashSet<i64> = role_maps.iter().map(|m| m.role_id).collect();
        let mapped_d: HashSet<i64> = role_maps.iter().map(|m| m.discord_role_id).collect();
        for (sid, sr) in sr_roles
            .iter()
            .filter(|(id, _)| !mapped_sr.contains(id) && **id != guild)
        {
            // (managed SIDE ROOM roles are not in this table's scope: the reader excludes them below)
            let managed: bool = sqlx::query_scalar("SELECT managed FROM roles WHERE id = $1")
                .bind(sid)
                .fetch_one(self.pool())
                .await?;
            if managed {
                continue;
            }
            let word = Permissions::from_bits_truncate(sr.permissions as u64)
                .intersection(Permissions::BRIDGE_SHARED)
                .bits();
            let created = api
                .create_role(
                    &token,
                    dg,
                    &RoleParams {
                        name: Some(sr.name.clone()),
                        permissions: Some(word),
                        color: Some(i64::from(sr.color)),
                        hoist: Some(sr.hoist),
                        mentionable: Some(sr.mentionable),
                        unicode_emoji: sr.unicode_emoji.clone().map(Some),
                    },
                    &reason,
                )
                .await?;
            let mut synced = SyncedState::new();
            synced.record("name", json!(sr.name), json!(created.name));
            store::insert_role_map(
                self.pool(),
                &RoleMap {
                    id: self.svc.next_id(),
                    guild_id: guild,
                    role_id: *sid,
                    discord_role_id: created.id,
                    is_everyone: false,
                    origin: Origin::Sr,
                    discord_only_bits: 0,
                    synced,
                    sync_status: "mapped".into(),
                },
            )
            .await?;
            rep.created_on_discord += 1;
        }
        for dr in d
            .roles
            .iter()
            .filter(|r| !r.managed && r.id != dg && !mapped_d.contains(&r.id))
        {
            // Design 6.2: created here with NO permissions; Discord's copy keeps only its Discord-only bits.
            let sid = self
                .sr
                .create_role(
                    guild,
                    SrNewRole {
                        name: dr.name.clone(),
                        color: i32::try_from(dr.color).unwrap_or(0),
                        hoist: dr.hoist,
                        mentionable: dr.mentionable,
                        unicode_emoji: dr.unicode_emoji.clone(),
                    },
                    &origin,
                )
                .await?;
            let keep = dr.permissions & DISCORD_ONLY_MASK;
            if dr.permissions != keep {
                api.edit_role(
                    &token,
                    dg,
                    dr.id,
                    &RoleParams {
                        permissions: Some(keep),
                        ..RoleParams::default()
                    },
                    &reason,
                )
                .await?;
            }
            let mut synced = SyncedState::new();
            synced.record("name", json!(dr.name), json!(dr.name));
            store::insert_role_map(
                self.pool(),
                &RoleMap {
                    id: self.svc.next_id(),
                    guild_id: guild,
                    role_id: sid,
                    discord_role_id: dr.id,
                    is_everyone: false,
                    origin: Origin::Discord,
                    discord_only_bits: keep,
                    synced,
                    sync_status: "mapped".into(),
                },
            )
            .await?;
            rep.created_here += 1;
        }

        // ---- channels ----
        let sr_ch = self.sr_channels(guild).await?;
        let held: HashSet<i64> = holds::list_held(self.pool(), guild)
            .await?
            .into_iter()
            .map(|h| h.channel_id)
            .collect();
        let d_all: HashMap<i64, &GuildChannel> = d
            .channels
            .iter()
            .chain(d.threads.iter())
            .map(|c| (c.id, c))
            .collect();
        let maps = store::list_structure_maps(self.pool(), guild).await?;
        for m in &maps {
            if m.sync_status == "frozen"
                || m.sync_status == "pending_delete"
                || held.contains(&m.channel_id)
            {
                continue;
            }
            let Some(src) = sr_ch.get(&m.channel_id) else {
                continue;
            };
            let Some(dc) = d_all.get(&m.discord_channel_id) else {
                // Deleted on Discord: hold it here (hide now, delete later), never delete at once.
                if self
                    .hold_deleted_on_discord(guild, m.discord_channel_id, None, self.hold_hours)
                    .await?
                {
                    rep.held += 1;
                }
                continue;
            };
            let kind = src.kind;
            let fields = [
                Field {
                    name: "name",
                    sr: json!(src.name),
                    discord: json!(dc.name),
                    to_discord: channel_name,
                },
                Field {
                    name: "topic",
                    sr: opt_str(&src.topic),
                    discord: opt_str(&dc.topic),
                    to_discord: same,
                },
                Field {
                    name: "nsfw",
                    sr: json!(src.nsfw),
                    discord: json!(dc.nsfw),
                    to_discord: same,
                },
                Field {
                    name: "rate_limit_per_user",
                    sr: json!(src.rate_limit),
                    discord: json!(dc.rate_limit_per_user),
                    to_discord: same,
                },
            ];
            let mut synced = m.synced.clone();
            let mut push = ChannelParams::default();
            let mut patch = ChannelPatch::default();
            let (mut pushed_any, mut applied_any) = (false, false);
            for f in &fields {
                if matches!(kind, 4 | 2 | 13) && f.name == "topic" {
                    continue;
                }
                match self.decide_field(ObjectKind::Channel, f, kind, synced.get(f.name)) {
                    Decision::Nothing => {}
                    Decision::Converged => {
                        synced.record(f.name, f.sr.clone(), f.discord.clone());
                        rep.converged += 1;
                    }
                    dec @ (Decision::PushToDiscord | Decision::Conflict | Decision::Revert) => {
                        if dec == Decision::Conflict {
                            rep.conflicts += 1;
                        }
                        match f.name {
                            "name" => push.name = Some(discord_name_for(kind, &src.name)),
                            "topic" => push.topic = Some(src.topic.clone()),
                            "nsfw" => push.nsfw = Some(src.nsfw),
                            _ => push.rate_limit_per_user = Some(i64::from(src.rate_limit)),
                        }
                        synced.record(f.name, f.sr.clone(), (f.to_discord)(&f.sr, kind));
                        pushed_any = true;
                        rep.pushed += 1;
                    }
                    Decision::ApplyToSr => {
                        match f.name {
                            "name" => patch.name = Some(dc.name.clone()),
                            "topic" => patch.topic = Some(dc.topic.clone()),
                            "nsfw" => patch.nsfw = Some(dc.nsfw),
                            _ => {
                                patch.rate_limit_per_user =
                                    Some(i32::try_from(dc.rate_limit_per_user).unwrap_or(0))
                            }
                        }
                        synced.record(f.name, f.discord.clone(), f.discord.clone());
                        applied_any = true;
                        rep.applied += 1;
                    }
                }
            }
            if pushed_any {
                api.edit_channel(&token, dc.id, &push, &reason).await?;
            }
            if applied_any {
                self.sr
                    .apply(
                        guild,
                        &SrStructuralPatch::Channel {
                            channel_id: m.channel_id,
                            patch,
                        },
                        &origin,
                    )
                    .await?;
            }
            if synced != m.synced {
                store::save_map_state(
                    self.pool(),
                    m.id,
                    &synced,
                    &m.sync_status,
                    m.degraded_reason.as_deref(),
                )
                .await?;
            }
        }

        // ---- channels on one side only (categories first, then children; threads are not created here) ----
        let maps = store::list_structure_maps(self.pool(), guild).await?;
        let mut sr_to_d: HashMap<i64, i64> = maps
            .iter()
            .map(|m| (m.channel_id, m.discord_channel_id))
            .collect();
        let mut d_to_sr: HashMap<i64, i64> = maps
            .iter()
            .map(|m| (m.discord_channel_id, m.channel_id))
            .collect();
        let snap = self.read_sr(guild).await?;
        let mut sr_new: Vec<_> = snap
            .channels
            .iter()
            .filter(|c| {
                !sr_to_d.contains_key(&c.id) && !matches!(c.kind, 10..=12) && !held.contains(&c.id)
            })
            .filter(|c| {
                super::eligible::bridgeable(Some(guild), c.kind, c.e2ee, snap.staff_guild).is_ok()
            })
            .collect();
        sr_new.sort_by_key(|c| (c.kind != 4, c.position, c.id));
        for c in sr_new {
            let src = &sr_ch[&c.id];
            let parent_d = src.parent_id.and_then(|p| sr_to_d.get(&p).copied());
            let kind = i64::from(src.kind);
            let kind = if info.is_community() {
                kind
            } else {
                match kind {
                    5 | 15 | 16 => 0,
                    13 => 2,
                    k => k,
                }
            };
            let created = api
                .create_channel(
                    &token,
                    dg,
                    &ChannelParams {
                        kind: Some(kind),
                        name: Some(discord_name_for(src.kind, &src.name)),
                        topic: src.topic.clone().map(Some),
                        parent_id: if src.kind == 4 {
                            None
                        } else {
                            parent_d.map(Some)
                        },
                        nsfw: Some(src.nsfw),
                        ..ChannelParams::default()
                    },
                    &reason,
                )
                .await?;
            let mut synced = SyncedState::new();
            synced.record("name", json!(src.name), json!(created.name));
            let parent_map = match src.parent_id {
                Some(p) => store::structure_map_for_channel(self.pool(), p)
                    .await?
                    .map(|m| m.id),
                None => None,
            };
            store::insert_structure_map(
                self.pool(),
                &NewStructureMap {
                    id: self.svc.next_id(),
                    guild_id: guild,
                    channel_id: c.id,
                    discord_channel_id: created.id,
                    discord_channel_name: created.name.clone(),
                    kind: if src.kind == 4 { "category" } else { "channel" },
                    selected: false,
                    origin: Origin::Sr,
                    parent_map_id: parent_map,
                    created_by: SYSTEM_USER_ID,
                    synced,
                },
            )
            .await?;
            self.overwrites
                .push_channel(guild, c.id, created.id, &reason)
                .await?;
            sr_to_d.insert(c.id, created.id);
            d_to_sr.insert(created.id, c.id);
            rep.created_on_discord += 1;
        }
        let mut d_new: Vec<&GuildChannel> = d
            .channels
            .iter()
            .filter(|c| {
                !d_to_sr.contains_key(&c.id) && matches!(c.kind, 0 | 2 | 4 | 5 | 13 | 15 | 16)
            })
            .collect();
        d_new.sort_by_key(|c| (c.kind != 4, c.position, c.id));
        for dc in d_new {
            let parent_sr = dc.parent_id.and_then(|p| d_to_sr.get(&p).copied());
            let kind = i16::try_from(dc.kind).unwrap_or(0);
            // The port copies the SIDE ROOM parent's overwrites; nothing else is granted.
            let sid = self
                .sr
                .create_channel(
                    guild,
                    SrNewChannel {
                        kind,
                        name: dc.name.clone(),
                        topic: dc.topic.clone(),
                        parent_id: parent_sr,
                        position: i32::try_from(dc.position).unwrap_or(0),
                        nsfw: dc.nsfw,
                        rate_limit_per_user: i32::try_from(dc.rate_limit_per_user).unwrap_or(0),
                        bitrate: dc.bitrate.and_then(|b| i32::try_from(b).ok()),
                        user_limit: dc.user_limit.and_then(|b| i32::try_from(b).ok()),
                    },
                    &origin,
                )
                .await?;
            let mut synced = SyncedState::new();
            synced.record("name", json!(dc.name), json!(dc.name));
            let parent_map = match parent_sr {
                Some(p) => store::structure_map_for_channel(self.pool(), p)
                    .await?
                    .map(|m| m.id),
                None => None,
            };
            store::insert_structure_map(
                self.pool(),
                &NewStructureMap {
                    id: self.svc.next_id(),
                    guild_id: guild,
                    channel_id: sid,
                    discord_channel_id: dc.id,
                    discord_channel_name: dc.name.clone(),
                    kind: if kind == 4 { "category" } else { "channel" },
                    selected: false,
                    origin: Origin::Discord,
                    parent_map_id: parent_map,
                    created_by: SYSTEM_USER_ID,
                    synced,
                },
            )
            .await?;
            // Make Discord's overwrites equal ours (a private channel created on Discord is not private here).
            self.overwrites
                .push_channel(guild, sid, dc.id, &reason)
                .await?;
            sr_to_d.insert(sid, dc.id);
            d_to_sr.insert(dc.id, sid);
            rep.created_here += 1;
        }

        store::touch_reconcile(self.pool(), guild).await?;
        if rep.changed() {
            let mut conn = self.pool().acquire().await?;
            self.svc
                .audit(
                    &mut conn,
                    AuditRow {
                        guild,
                        actor: SYSTEM_USER_ID,
                        action: AUDIT_SYNC_RUN,
                        target: Some(guild),
                        changes: json!([]),
                        options: json!({"seed": false, "counts": &rep}),
                    },
                )
                .await?;
        }
        Ok(rep)
    }

    fn decide_field(
        &self,
        kind: ObjectKind,
        f: &Field,
        sr_kind: i16,
        synced: Option<&super::types::FieldState>,
    ) -> Decision {
        let authority: Authority = field_authority(kind, f.name);
        let to_d = f.to_discord;
        decide(
            authority,
            &FieldView {
                sr_now: &f.sr,
                discord_now: &f.discord,
                synced,
            },
            |v| to_d(v, sr_kind),
        )
    }
}
