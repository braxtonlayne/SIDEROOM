//! Wave 35b integration: the joints between the engine (BRG-SYN-001), the permission planners and conflict log
//! (BRG-SYN-002) and the dispatch handling (BRG-SYN-003).
//!
//! * [`Attributions`]: `GUILD_AUDIT_LOG_ENTRY_CREATE` names the Discord user behind a change; the conflict rows the
//!   reconcile writes a few seconds later carry that name (design 5.3: matched by target id within 10 s).
//! * [`SyncEngine::note`]: every revert, conflict, refused role deletion and held deletion goes through it: one
//!   `bridge_conflicts` row (coalesced, audit 222/223) and the ping-pong freeze (design 11.4: more than five
//!   overrides of one object in ten minutes freeze its map).
//! * [`SyncEngine::push_overwrites`] and [`PlannedOverwrites`]: sync-perms' `overwrites::plan_push` is the only code
//!   that writes a Discord overwrite array (the [`OverwritePush`] hook of the seed and the reconcile).
//! * [`EngineSink`]: the [`StructureSink`] the inbound shard calls: a structural dispatch of a whole-guild bridge
//!   queues a look at the object; nothing else happens on the gateway thread.
//! * [`SyncEngine::handle_sr_event`]: a SIDE ROOM role, channel or thread change queues a look the same way.
//! * [`mention_maps`]: Discord role and channel ids to SIDE ROOM ids for inbound mentions (whole-guild only).
//!
//! None of these reads or writes a SIDE ROOM permission: the overwrite planner reads SIDE ROOM's overwrites and
//! writes only to Discord; conflict rows are display data (no authorization code reads `bridge_conflicts`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::{Value, json};
use sqlx::Row;
use sr_core::permissions::{Overwrite, OverwriteKind, Permissions};
use sr_core::snowflake::Snowflake;
use sr_events::DomainEvent;

use super::conflicts::{ConflictKind, NewConflict, ObjectKind as CObj};
use super::engine::SyncEngine;
use super::overwrites::{DiscordOverwrite, Protected, RoleMapping, plan_push};
use super::port::OverwritePush;
use super::store;
use super::types::{ObjectKind, OpDirection, OpPriority, SyncMode};
use crate::discord_structure::{GuildRole, Overwrite as DOverwrite};
use crate::error::{BridgeError, BridgeResult};
use crate::inbound::convert::MentionMaps;
use crate::inbound::structure::{StructureEvent, StructureKind, StructureSink};
use crate::model::{Bridge, get_bridge};

/// How long an audit-log entry names the actor of a change to its target (design 5.3).
pub const ATTRIBUTION_WINDOW: Duration = Duration::from_secs(10);

/// A map is frozen when one object was overridden more than this many times inside the coalescing window
/// (`conflicts::COALESCE_SECS`, ten minutes; design 11.4).
pub const FREEZE_AFTER: i32 = 5;

/// Discord object id -> (Discord user id, when Discord's audit log said so). In memory: an attribution older than
/// [`ATTRIBUTION_WINDOW`] is worthless, so nothing is lost by a restart.
#[derive(Clone, Default)]
pub struct Attributions(Arc<Mutex<HashMap<i64, (i64, Instant)>>>);

impl Attributions {
    pub fn note(&self, target: i64, user: i64) {
        let Ok(mut m) = self.0.lock() else {
            return;
        };
        if m.len() > 4096 {
            m.retain(|_, (_, at)| at.elapsed() <= ATTRIBUTION_WINDOW);
        }
        m.insert(target, (user, Instant::now()));
    }

    /// The Discord user behind a recent change of `target`, if the audit log named one in the window.
    pub fn who(&self, target: i64) -> Option<i64> {
        let m = self.0.lock().ok()?;
        m.get(&target)
            .filter(|(_, at)| at.elapsed() <= ATTRIBUTION_WINDOW)
            .map(|(u, _)| *u)
    }
}

fn word(p: Permissions) -> Value {
    json!(p.bits().to_string())
}

impl SyncEngine {
    /// Record a conflict row (coalesced, with its audit row) and apply the ping-pong freeze. A failure to record
    /// is logged, never fatal: the revert itself already happened.
    pub async fn note(&self, mut c: NewConflict) {
        if c.discord_actor_id.is_none()
            && let Some(d) = c.discord_object_id
        {
            c.discord_actor_id = self.attributions.who(d);
        }
        let counts = matches!(
            c.kind,
            ConflictKind::PermissionReverted
                | ConflictKind::StructureConflict
                | ConflictKind::OverwriteTrimmed
        );
        match self.svc.record_conflict(c.clone()).await {
            Ok(rec) if counts && rec.conflict.count > FREEZE_AFTER => {
                if let Err(e) = self.freeze(&c).await {
                    tracing::warn!(error = %e, guild = c.guild, "bridge sync: could not freeze a map");
                }
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(error = %e, guild = c.guild, kind = c.kind.as_str(), "bridge sync: conflict not recorded")
            }
        }
    }

    /// Freeze the map of the object a conflict names (design 11.4): the bridge stops pushing it; SIDE ROOM's state is
    /// untouched (nothing from Discord was ever applied to its permissions). Writes a `frozen` row once.
    async fn freeze(&self, c: &NewConflict) -> BridgeResult<()> {
        let Some(sr) = c.sr_object_id else {
            return Ok(());
        };
        let frozen = if c.object_kind == CObj::Role {
            sqlx::query(
                "UPDATE bridge_role_maps SET sync_status = 'frozen', updated_at = now() \
                 WHERE guild_id = $1 AND role_id = $2 AND sync_status <> 'frozen'",
            )
            .bind(c.guild)
            .bind(sr)
            .execute(self.pool())
            .await?
            .rows_affected()
        } else {
            sqlx::query(
                "UPDATE bridge_channel_maps SET sync_status = 'frozen' \
                 WHERE guild_id = $1 AND channel_id = $2 AND sync_status IN ('mapped', 'degraded')",
            )
            .bind(c.guild)
            .bind(sr)
            .execute(self.pool())
            .await?
            .rows_affected()
        };
        if frozen > 0 {
            // The rows that counted to the freeze close with it, so an unfrozen map starts counting afresh.
            sqlx::query(
                "UPDATE bridge_conflicts SET status = 'superseded', resolution = 'superseded', \
                 resolved_at = now() WHERE guild_id = $1 AND kind = $2 AND sr_object_id = $3 AND status = 'open'",
            )
            .bind(c.guild)
            .bind(c.kind.as_str())
            .bind(sr)
            .execute(self.pool())
            .await?;
            let mut f = NewConflict::new(
                c.guild,
                ConflictKind::Frozen,
                if c.object_kind == CObj::Role {
                    CObj::Role
                } else {
                    CObj::Channel
                },
                "",
            );
            f.sr_object_id = Some(sr);
            f.discord_object_id = c.discord_object_id;
            f.discord_actor_id = c.discord_actor_id;
            self.svc.record_conflict(f).await?;
        }
        Ok(())
    }

    /// "Unfreeze" (a conflict of kind `frozen`): the map runs again from the next pass. Returns false when the map
    /// was not frozen (or is gone).
    pub async fn unfreeze(
        &self,
        guild: i64,
        object_kind: &str,
        sr_object: i64,
    ) -> BridgeResult<bool> {
        let n = if object_kind == CObj::Role.as_str() {
            sqlx::query(
                "UPDATE bridge_role_maps SET sync_status = 'mapped', updated_at = now() \
                 WHERE guild_id = $1 AND role_id = $2 AND sync_status = 'frozen'",
            )
            .bind(guild)
            .bind(sr_object)
            .execute(self.pool())
            .await?
            .rows_affected()
        } else {
            sqlx::query(
                "UPDATE bridge_channel_maps SET sync_status = \
                 CASE WHEN degraded_reason IS NULL THEN 'mapped' ELSE 'degraded' END \
                 WHERE guild_id = $1 AND channel_id = $2 AND sync_status = 'frozen'",
            )
            .bind(guild)
            .bind(sr_object)
            .execute(self.pool())
            .await?
            .rows_affected()
        };
        if n > 0 {
            // Look again soon, so the unfrozen object converges without waiting for the scheduled pass.
            self.enqueue_look(
                guild,
                ObjectKind::Reconcile,
                guild,
                OpDirection::Look,
                OpPriority::Structure,
            )
            .await?;
        }
        Ok(n > 0)
    }

    // ---- overwrites (SIDE ROOM to Discord only) ----

    /// SIDE ROOM's overwrites of a channel, as `sr-core` values.
    async fn sr_overwrites(&self, channel: i64) -> BridgeResult<Vec<Overwrite>> {
        let rows = sqlx::query(
            "SELECT target_id, target_type::text AS kind, allow, deny FROM permission_overwrites \
             WHERE channel_id = $1",
        )
        .bind(channel)
        .fetch_all(self.pool())
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let id: i64 = r.try_get("target_id")?;
            let kind: String = r.try_get("kind")?;
            let allow: i64 = r.try_get("allow")?;
            let deny: i64 = r.try_get("deny")?;
            out.push(Overwrite {
                id: Snowflake::new(u64::try_from(id).unwrap_or(0)),
                kind: if kind == "member" {
                    OverwriteKind::Member
                } else {
                    OverwriteKind::Role
                },
                allow: Permissions::from_bits_truncate(allow as u64),
                deny: Permissions::from_bits_truncate(deny as u64),
            });
        }
        Ok(out)
    }

    async fn role_mapping(&self, guild: i64, discord_guild: i64) -> BridgeResult<RoleMapping> {
        let maps = store::list_role_maps(self.pool(), guild).await?;
        Ok(RoleMapping {
            sr_guild: u64::try_from(guild).unwrap_or(0),
            discord_guild: u64::try_from(discord_guild).unwrap_or(0),
            pairs: maps
                .iter()
                .filter(|m| !m.is_everyone)
                .map(|m| {
                    (
                        u64::try_from(m.role_id).unwrap_or(0),
                        u64::try_from(m.discord_role_id).unwrap_or(0),
                    )
                })
                .collect(),
        })
    }

    /// Make one Discord channel's overwrites equal SIDE ROOM's through sync-perms' planner (shared bits only,
    /// Discord-only bits kept, SIDE ROOM member overwrites never sent, Discord member and managed-role allows
    /// trimmed). Conflict rows: `overwrite_trimmed` for every trim, `permission_reverted` for a mapped role's
    /// overwrite Discord changed since the last push (the last push is `synced_state.overwrites`).
    #[allow(clippy::too_many_arguments)]
    pub async fn push_overwrites(
        &self,
        guild: i64,
        token: &str,
        sr_channel: i64,
        discord_channel: i64,
        current: &[DOverwrite],
        discord_roles: &[GuildRole],
        reason: &str,
    ) -> BridgeResult<bool> {
        let bridge = get_bridge(self.pool(), guild)
            .await?
            .ok_or(BridgeError::NotFound("bridge"))?;
        let map = self.role_mapping(guild, bridge.discord_guild_id).await?;
        let sr = self.sr_overwrites(sr_channel).await?;
        let to_d = |o: &DOverwrite| DiscordOverwrite {
            id: u64::try_from(o.id).unwrap_or(0),
            kind: if o.kind == DOverwrite::MEMBER {
                OverwriteKind::Member
            } else {
                OverwriteKind::Role
            },
            allow: o.allow,
            deny: o.deny,
        };
        let cur: Vec<DiscordOverwrite> = current.iter().map(to_d).collect();
        let bot = u64::try_from(bridge.bot_user_id).unwrap_or(0);
        let protected = Protected {
            bot_user: bot,
            bot_roles: discord_roles
                .iter()
                .filter(|r| r.managed && r.bot_id == Some(bridge.bot_user_id))
                .map(|r| u64::try_from(r.id).unwrap_or(0))
                .collect(),
        };
        let plan = plan_push(&sr, &cur, &map, &protected);
        let as_json = |v: &[DiscordOverwrite]| {
            let mut v: Vec<Value> = v
                .iter()
                .map(|o| {
                    json!([
                        o.id.to_string(),
                        u8::from(o.kind == OverwriteKind::Member),
                        o.allow.to_string(),
                        o.deny.to_string()
                    ])
                })
                .collect();
            v.sort_by_key(|x| x.to_string());
            Value::Array(v)
        };
        let smap = store::structure_map_for_channel(self.pool(), sr_channel).await?;
        let last = smap
            .as_ref()
            .and_then(|m| m.synced.get("overwrites"))
            .map(|f| f.discord.clone());
        let discord_changed = last.as_ref().is_some_and(|l| *l != as_json(&cur));
        if discord_changed {
            for r in &plan.reverted {
                let mut c = NewConflict::new(
                    guild,
                    ConflictKind::PermissionReverted,
                    CObj::Overwrite,
                    "overwrite",
                );
                c.sr_object_id = Some(sr_channel);
                c.discord_object_id = Some(discord_channel);
                c.target_id = i64::try_from(r.sr_role.get()).ok();
                c.discord_value =
                    Some(json!({"allow": word(r.discord_allow), "deny": word(r.discord_deny)}));
                c.sr_value = Some(json!({"allow": word(r.sr_allow), "deny": word(r.sr_deny)}));
                self.note(c).await;
            }
        }
        for t in &plan.trimmed {
            let mut c = NewConflict::new(
                guild,
                ConflictKind::OverwriteTrimmed,
                CObj::Overwrite,
                "allow",
            );
            c.sr_object_id = Some(sr_channel);
            c.discord_object_id = Some(discord_channel);
            c.target_id = i64::try_from(t.target).ok();
            c.discord_value = Some(word(t.stripped_allow));
            self.note(c).await;
        }
        if plan.changed {
            let body: Vec<DOverwrite> = plan
                .desired
                .iter()
                .map(|o| DOverwrite {
                    id: i64::try_from(o.id).unwrap_or(0),
                    kind: if o.kind == OverwriteKind::Member {
                        DOverwrite::MEMBER
                    } else {
                        DOverwrite::ROLE
                    },
                    allow: o.allow,
                    deny: o.deny,
                })
                .collect();
            self.svc
                .discord
                .set_overwrites(token, discord_channel, &body, reason)
                .await?;
        }
        if let Some(m) = smap {
            let mut synced = m.synced.clone();
            synced.record("overwrites", Value::Null, as_json(&plan.desired));
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
        Ok(plan.changed)
    }

    // ---- SIDE ROOM changes ----

    /// A SIDE ROOM role, channel or thread change of a whole-guild bridge queues a look (design 11.1). Everything
    /// else is ignored at once; the reconcile decides what, if anything, to send.
    pub async fn handle_sr_event(&self, ev: &DomainEvent) {
        let (guild, kind, id) = match ev {
            DomainEvent::RoleCreated { guild, role, .. }
            | DomainEvent::RoleUpdated { guild, role, .. }
            | DomainEvent::RoleDeleted { guild, role, .. } => {
                (guild.get(), ObjectKind::Role, role.get())
            }
            DomainEvent::ChannelCreated { guild, channel, .. }
            | DomainEvent::ChannelUpdated { guild, channel, .. }
            | DomainEvent::ChannelDeleted { guild, channel, .. } => {
                (guild.get(), ObjectKind::Channel, channel.get())
            }
            DomainEvent::ThreadCreated { guild, thread, .. }
            | DomainEvent::ThreadUpdated { guild, thread, .. }
            | DomainEvent::ThreadDeleted { guild, thread, .. } => {
                (guild.get(), ObjectKind::Thread, thread.get())
            }
            _ => return,
        };
        let (Ok(guild), Ok(id)) = (i64::try_from(guild), i64::try_from(id)) else {
            return;
        };
        if let Err(e) = self
            .look_if_live(guild, kind, id, OpDirection::ToDiscord)
            .await
        {
            tracing::warn!(error = %e, guild, "bridge sync: could not queue a SIDE ROOM change");
        }
    }

    /// Queue a look at an object when the guild has a whole-guild bridge whose sync runs. Returns whether it did.
    pub async fn look_if_live(
        &self,
        guild: i64,
        kind: ObjectKind,
        id: i64,
        direction: OpDirection,
    ) -> BridgeResult<bool> {
        let Some(s) = store::get_sync(self.pool(), guild).await? else {
            return Ok(false);
        };
        if s.mode != SyncMode::WholeGuild || !s.state.runs_ops() {
            return Ok(false);
        }
        self.enqueue_look(guild, kind, id, direction, OpPriority::Structure)
            .await
    }
}

/// The [`OverwritePush`] hook of the seed: reads Discord's channel and roles, then [`SyncEngine::push_overwrites`].
pub struct PlannedOverwrites {
    pub engine: SyncEngine,
}

#[async_trait]
impl OverwritePush for PlannedOverwrites {
    async fn push_channel(
        &self,
        guild: i64,
        sr_channel: i64,
        discord_channel: i64,
        reason: &str,
    ) -> BridgeResult<()> {
        let e = &self.engine;
        let bridge = get_bridge(e.pool(), guild)
            .await?
            .ok_or(BridgeError::NotFound("bridge"))?;
        let token = e.svc.open_bot_token(guild).await?;
        let ch = e.svc.discord.get_channel(&token, discord_channel).await?;
        let roles = e
            .svc
            .discord
            .guild_roles(&token, bridge.discord_guild_id)
            .await?;
        e.push_overwrites(
            guild,
            &token,
            sr_channel,
            discord_channel,
            &ch.overwrites,
            &roles,
            reason,
        )
        .await
        .map(|_| ())
    }
}

/// The inbound shard's [`StructureSink`]: queue a look per structural dispatch of a whole-guild bridge; remember
/// who did what for the conflict rows.
pub struct EngineSink {
    pub engine: SyncEngine,
}

#[async_trait]
impl StructureSink for EngineSink {
    async fn look_at(&self, bridge: &Bridge, events: Vec<StructureEvent>) {
        if bridge.mode != SyncMode::WholeGuild.as_str() {
            return;
        }
        for ev in events {
            let kind = match ev.kind {
                StructureKind::Role => ObjectKind::Role,
                StructureKind::Channel => ObjectKind::Channel,
                StructureKind::Thread => ObjectKind::Thread,
            };
            if let Err(e) = self
                .engine
                .look_if_live(bridge.guild_id, kind, ev.id, OpDirection::ToSr)
                .await
            {
                tracing::warn!(error = %e, guild = bridge.guild_id, "bridge sync: could not queue a Discord change");
            }
        }
    }

    async fn attribute(&self, _bridge: &Bridge, target: i64, user: i64, _action_type: i64) {
        self.engine.attributions.note(target, user);
    }
}

/// Discord role and channel ids to the SIDE ROOM ids they are mapped to, for an inbound message of a whole-guild
/// bridge (`convert::clean_content_mapped`). Empty for a selective bridge: wave 35's plain-text mentions stay.
pub async fn mention_maps(pool: &sqlx::PgPool, guild: i64) -> BridgeResult<MentionMaps> {
    let mut maps = MentionMaps::default();
    let whole: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM bridges WHERE guild_id = $1 AND mode = 'whole_guild')",
    )
    .bind(guild)
    .fetch_one(pool)
    .await?;
    if !whole {
        return Ok(maps);
    }
    for m in store::list_role_maps(pool, guild).await? {
        if !m.is_everyone {
            maps.roles.insert(m.discord_role_id, m.role_id);
        }
    }
    for m in store::list_structure_maps(pool, guild).await? {
        maps.channels.insert(m.discord_channel_id, m.channel_id);
    }
    Ok(maps)
}
