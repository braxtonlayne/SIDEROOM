//! [`SyncEngine`]: plans, the mode switch, the seed and the hold jobs (design sections 3, 6.4, 9 and 12).
//!
//! The engine owns no Discord HTTP call (they are `DiscordApi` methods, BRG-SYN-003) and no SIDE ROOM write
//! (they go through the [`SrStructure`] port, so Discord-originated changes carry `Origin::Bridge`).

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use sr_core::permissions::{DISCORD_ONLY_MASK, Permissions};

use super::holds;
use super::plan::{
    Action, Blocker, DiscordSnapshot, ItemKind, Pinned, Plan, PreflightFacts, SrChannelIn,
    SrRoleIn, SrSnapshot, blockers, build, discord_hash, sr_hash,
};
use super::port::{
    BridgeOrigin, NoOverwrites, OverwritePush, SrNewChannel, SrNewRole, SrStructure,
};
use super::reconcile::discord_name_for;
use super::store::{self, NewStructureMap, Origin, RoleMap};
use super::types::{SyncMode, SyncState, SyncedState};
use crate::discord_structure::{
    ChannelParams, ChannelPosition, GuildChannel, RoleParams, channel_type, reason_for_op,
};
use crate::error::{BridgeError, BridgeResult};
use crate::model::get_bridge;
use crate::service::{AuditRow, BridgeService, SYSTEM_USER_ID};

/// Audit numbers of the bridge (`AuditAction::BridgeModeUpdate`, `BridgeSyncRun`; BRG-SYN-002 owns the enum).
pub const AUDIT_MODE_UPDATE: i16 = 220;
pub const AUDIT_SYNC_RUN: i16 = 221;

/// A plan expires after 15 minutes.
pub const PLAN_TTL_MINUTES: i64 = 15;

/// What `plan` stores: the diff, and the facts the blockers were computed from (so an edit recomputes them).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredPlan {
    pub plan: Plan,
    pub facts: PreflightFacts,
}

/// A plan row as the routes show it.
#[derive(Debug, Clone, Serialize)]
pub struct PlanView {
    pub id: i64,
    pub guild_id: i64,
    pub status: String,
    pub seed_structure: String,
    pub plan: Plan,
    pub blockers: Vec<Blocker>,
    pub plan_hash: String,
    pub created_by: i64,
    pub expires_at_ms: i64,
}

/// One edit of a plan the owner (or a manager preparing it) makes.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PlanEdit {
    Unlink {
        index: usize,
    },
    Link {
        kind: ItemKind,
        sr_id: i64,
        discord_id: i64,
    },
    Import {
        index: usize,
        import: bool,
    },
    SeedStructure {
        value: String,
    },
}

/// Counts the seed reports (audit 221 `options`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SeedReport {
    pub roles_matched: u32,
    pub roles_created_on_discord: u32,
    pub roles_created_here: u32,
    pub channels_matched: u32,
    pub channels_created_on_discord: u32,
    pub channels_created_here: u32,
    pub degraded: u32,
    /// Roles whose Discord-side permissions the owner chose to import, with the shared bits imported.
    pub imported: Vec<(i64, u64)>,
}

#[derive(Clone)]
pub struct SyncEngine {
    pub svc: BridgeService,
    pub sr: Arc<dyn SrStructure>,
    pub overwrites: Arc<dyn OverwritePush>,
}

struct SrRoleRow {
    name: String,
    color: i32,
    hoist: bool,
    mentionable: bool,
    unicode_emoji: Option<String>,
    permissions: i64,
}

struct SrChanRow {
    id: i64,
    kind: i16,
    name: String,
    topic: Option<String>,
    parent_id: Option<i64>,
    position: i32,
    nsfw: bool,
    rate_limit: i32,
    bitrate: Option<i32>,
    user_limit: Option<i32>,
}

impl SyncEngine {
    pub fn new(svc: BridgeService, sr: Arc<dyn SrStructure>) -> Self {
        Self {
            svc,
            sr,
            overwrites: Arc::new(NoOverwrites),
        }
    }

    pub fn with_overwrites(mut self, o: Arc<dyn OverwritePush>) -> Self {
        self.overwrites = o;
        self
    }

    fn pool(&self) -> &sqlx::PgPool {
        &self.svc.pool
    }

    // ---- reading both sides ----

    /// SIDE ROOM's side: roles, and channels without held ones and archived threads.
    pub async fn read_sr(&self, guild: i64) -> BridgeResult<SrSnapshot> {
        let roles = sqlx::query(
            "SELECT id, name, managed, position, permissions FROM roles WHERE guild_id = $1",
        )
        .bind(guild)
        .fetch_all(self.pool())
        .await?;
        let mut snap = SrSnapshot::default();
        for r in &roles {
            let id: i64 = r.try_get("id")?;
            snap.roles.push(SrRoleIn {
                id,
                name: r.try_get("name")?,
                managed: r.try_get("managed")?,
                position: r.try_get("position")?,
                is_everyone: id == guild,
                permissions: r.try_get("permissions")?,
            });
        }
        let chans = sqlx::query(
            "SELECT c.id, c.type, c.name, c.parent_id, c.position FROM channels c \
             LEFT JOIN thread_metadata t ON t.channel_id = c.id \
             WHERE c.guild_id = $1 AND COALESCE(t.archived, false) = false \
               AND NOT EXISTS (SELECT 1 FROM bridge_pending_deletes h WHERE h.channel_id = c.id)",
        )
        .bind(guild)
        .fetch_all(self.pool())
        .await?;
        for c in &chans {
            snap.channels.push(SrChannelIn {
                id: c.try_get("id")?,
                kind: c.try_get("type")?,
                name: c.try_get::<Option<String>, _>("name")?.unwrap_or_default(),
                parent_id: c.try_get("parent_id")?,
                position: c.try_get("position")?,
                e2ee: false,
            });
        }
        snap.staff_guild = sqlx::query_scalar("SELECT is_staff_guild FROM guilds WHERE id = $1")
            .bind(guild)
            .fetch_optional(self.pool())
            .await?
            .unwrap_or(false);
        Ok(snap)
    }

    /// Discord's side. Threads come from the active-thread list (archived ones are not imported, Q7).
    pub async fn read_discord(
        &self,
        token: &str,
        discord_guild: i64,
    ) -> BridgeResult<DiscordSnapshot> {
        let d = &self.svc.discord;
        Ok(DiscordSnapshot {
            discord_guild_id: discord_guild,
            roles: d.guild_roles(token, discord_guild).await?,
            channels: d.guild_channels_all(token, discord_guild).await?,
            threads: d.active_threads(token, discord_guild).await?,
        })
    }

    async fn pinned(&self, guild: i64) -> BridgeResult<Pinned> {
        let maps = store::list_structure_maps(self.pool(), guild).await?;
        Ok(Pinned {
            channels: maps
                .into_iter()
                .map(|m| (m.channel_id, m.discord_channel_id))
                .collect(),
        })
    }

    // ---- plans ----

    /// Build a plan (a dry run: nothing is written to either side). A manager may do this for the owner. A new
    /// plan supersedes the open one (at most one per server).
    pub async fn create_plan(
        &self,
        guild: i64,
        actor: i64,
        seed_structure: &str,
    ) -> BridgeResult<i64> {
        if !matches!(seed_structure, "sr" | "discord") {
            return Err(BridgeError::Invalid(
                "seed_structure: use \"sr\" or \"discord\".".into(),
            ));
        }
        let bridge = get_bridge(self.pool(), guild)
            .await?
            .ok_or_else(|| BridgeError::Conflict("Connect the bot token first.".into()))?;
        let token = self.svc.open_bot_token(guild).await?;
        let id = self.svc.next_id();
        let mut tx = self.pool().begin().await?;
        sqlx::query(
            "UPDATE bridge_sync_plans SET status = 'superseded' WHERE guild_id = $1 AND status IN ('building', 'ready')",
        )
        .bind(guild)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO bridge_sync_plans (id, guild_id, mode, seed_structure, created_by, expires_at) \
             VALUES ($1, $2, 'whole_guild', $3, $4, now() + ($5::bigint * interval '1 minute'))",
        )
        .bind(id)
        .bind(guild)
        .bind(seed_structure)
        .bind(actor)
        .bind(PLAN_TTL_MINUTES)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        match self
            .compute_plan(
                guild,
                &token,
                bridge.discord_guild_id,
                bridge.bot_user_id,
                bridge.intents_granted,
                seed_structure,
            )
            .await
        {
            Ok((stored, sr_h, d_h)) => {
                let blk = blockers(&stored.plan, &stored.facts);
                sqlx::query(
                    "UPDATE bridge_sync_plans SET status = 'ready', plan = $2, blockers = $3, sr_hash = $4, \
                     discord_hash = $5, plan_hash = $6 WHERE id = $1 AND status = 'building'",
                )
                .bind(id)
                .bind(serde_json::to_value(&stored).map_err(|e| BridgeError::Storage(e.to_string()))?)
                .bind(serde_json::to_value(&blk).map_err(|e| BridgeError::Storage(e.to_string()))?)
                .bind(sr_h)
                .bind(d_h)
                .bind(stored.plan.hash())
                .execute(self.pool())
                .await?;
                Ok(id)
            }
            Err(e) => {
                sqlx::query("UPDATE bridge_sync_plans SET status = 'failed' WHERE id = $1")
                    .bind(id)
                    .execute(self.pool())
                    .await?;
                Err(e)
            }
        }
    }

    async fn compute_plan(
        &self,
        guild: i64,
        token: &str,
        discord_guild: i64,
        bot_user: i64,
        intents: i64,
        seed_structure: &str,
    ) -> BridgeResult<(StoredPlan, String, String)> {
        let sr = self.read_sr(guild).await?;
        let d = self.read_discord(token, discord_guild).await?;
        let access = self
            .svc
            .discord
            .bot_access(token, discord_guild, bot_user)
            .await?;
        let taken: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM bridges WHERE discord_guild_id = $1 AND mode = 'whole_guild' AND guild_id <> $2)",
        )
        .bind(discord_guild)
        .bind(guild)
        .fetch_one(self.pool())
        .await?;
        let pinned = self.pinned(guild).await?;
        let plan = build(
            &sr,
            &d,
            &pinned,
            seed_structure,
            Permissions::BRIDGE_SHARED.bits(),
        );
        let facts = PreflightFacts {
            bot_administrator: access.administrator,
            bot_is_top_role: access.is_top_role(),
            intents_granted: u64::try_from(intents).unwrap_or(0),
            discord_guild_taken: taken,
        };
        Ok((StoredPlan { plan, facts }, sr_hash(&sr), discord_hash(&d)))
    }

    pub async fn get_plan(&self, guild: i64, plan_id: i64) -> BridgeResult<PlanView> {
        let row = sqlx::query(
            "SELECT id, guild_id, status, seed_structure, plan, blockers, plan_hash, created_by, \
             (extract(epoch from expires_at) * 1000)::bigint AS exp_ms, (expires_at < now()) AS expired \
             FROM bridge_sync_plans WHERE id = $1 AND guild_id = $2",
        )
        .bind(plan_id)
        .bind(guild)
        .fetch_optional(self.pool())
        .await?
        .ok_or(BridgeError::NotFound("plan"))?;
        let mut status: String = row.try_get("status")?;
        if row.try_get::<bool, _>("expired")? && matches!(status.as_str(), "building" | "ready") {
            sqlx::query("UPDATE bridge_sync_plans SET status = 'expired' WHERE id = $1")
                .bind(plan_id)
                .execute(self.pool())
                .await?;
            status = "expired".into();
        }
        let stored: StoredPlan = serde_json::from_value(row.try_get::<Value, _>("plan")?)
            .unwrap_or(StoredPlan {
                plan: Plan::default(),
                facts: PreflightFacts {
                    bot_administrator: false,
                    bot_is_top_role: false,
                    intents_granted: 0,
                    discord_guild_taken: false,
                },
            });
        Ok(PlanView {
            id: row.try_get("id")?,
            guild_id: row.try_get("guild_id")?,
            status,
            seed_structure: row.try_get("seed_structure")?,
            blockers: serde_json::from_value(row.try_get::<Value, _>("blockers")?)
                .unwrap_or_default(),
            plan: stored.plan,
            plan_hash: row.try_get("plan_hash")?,
            created_by: row.try_get("created_by")?,
            expires_at_ms: row.try_get("exp_ms")?,
        })
    }

    /// Apply one edit to a ready plan; the plan hash changes with it.
    pub async fn edit_plan(
        &self,
        guild: i64,
        plan_id: i64,
        edit: PlanEdit,
    ) -> BridgeResult<PlanView> {
        let view = self.get_plan(guild, plan_id).await?;
        if view.status != "ready" {
            return Err(BridgeError::Conflict(
                "That plan is not open any more.".into(),
            ));
        }
        let row = sqlx::query("SELECT plan FROM bridge_sync_plans WHERE id = $1")
            .bind(plan_id)
            .fetch_one(self.pool())
            .await?;
        let mut stored: StoredPlan = serde_json::from_value(row.try_get::<Value, _>("plan")?)
            .map_err(|e| BridgeError::Storage(e.to_string()))?;
        let bad = |m: &'static str| BridgeError::Invalid(m.to_owned());
        match edit {
            PlanEdit::Unlink { index } => stored.plan.unlink(index).map_err(bad)?,
            PlanEdit::Link {
                kind,
                sr_id,
                discord_id,
            } => stored.plan.link(kind, sr_id, discord_id).map_err(bad)?,
            PlanEdit::Import { index, import } => {
                stored.plan.set_import(index, import).map_err(bad)?
            }
            PlanEdit::SeedStructure { value } => {
                if !matches!(value.as_str(), "sr" | "discord") {
                    return Err(bad("seed_structure: use \"sr\" or \"discord\"."));
                }
                stored.plan.seed_structure = value;
            }
        }
        let blk = blockers(&stored.plan, &stored.facts);
        sqlx::query(
            "UPDATE bridge_sync_plans SET plan = $2, blockers = $3, plan_hash = $4, seed_structure = $5 WHERE id = $1",
        )
        .bind(plan_id)
        .bind(serde_json::to_value(&stored).map_err(|e| BridgeError::Storage(e.to_string()))?)
        .bind(serde_json::to_value(&blk).map_err(|e| BridgeError::Storage(e.to_string()))?)
        .bind(stored.plan.hash())
        .bind(&stored.plan.seed_structure)
        .execute(self.pool())
        .await?;
        self.get_plan(guild, plan_id).await
    }

    // ---- the mode switch ----

    /// Owner + MFA (the route checks both): confirm a plan and switch to whole_guild. Recomputes both sides'
    /// hashes; a stale plan is refused with [`BridgeError::PlanStale`]. Returns with `sync_state = 'seeding'`;
    /// the seed job calls [`run_seed`](Self::run_seed).
    pub async fn confirm_mode(
        &self,
        guild: i64,
        actor: i64,
        plan_id: i64,
        plan_hash: &str,
    ) -> BridgeResult<()> {
        let view = self.get_plan(guild, plan_id).await?;
        if view.status != "ready" {
            return Err(BridgeError::PlanStale);
        }
        if view.plan_hash != plan_hash {
            return Err(BridgeError::PlanStale);
        }
        if !view.blockers.is_empty() {
            return Err(BridgeError::Conflict(
                "The plan still has blockers; fix them and check again.".into(),
            ));
        }
        let bridge = get_bridge(self.pool(), guild)
            .await?
            .ok_or_else(|| BridgeError::Conflict("Connect the bot token first.".into()))?;
        if bridge.mode == SyncMode::WholeGuild.as_str() {
            return Err(BridgeError::Conflict(
                "This server is already bridged as a whole.".into(),
            ));
        }
        let token = self.svc.open_bot_token(guild).await?;
        let sr = self.read_sr(guild).await?;
        let d = self.read_discord(&token, bridge.discord_guild_id).await?;
        let row = sqlx::query("SELECT sr_hash, discord_hash FROM bridge_sync_plans WHERE id = $1")
            .bind(plan_id)
            .fetch_one(self.pool())
            .await?;
        if row.try_get::<String, _>("sr_hash")? != sr_hash(&sr)
            || row.try_get::<String, _>("discord_hash")? != discord_hash(&d)
        {
            sqlx::query("UPDATE bridge_sync_plans SET status = 'expired' WHERE id = $1")
                .bind(plan_id)
                .execute(self.pool())
                .await?;
            return Err(BridgeError::PlanStale);
        }
        let mut tx = self.pool().begin().await?;
        let r = sqlx::query(
            "UPDATE bridges SET mode = 'whole_guild', sync_state = 'seeding', mode_changed_by = $2, \
             mode_changed_at = now(), mode_plan_id = $3, mode_plan_hash = $4, updated_at = now() WHERE guild_id = $1",
        )
        .bind(guild)
        .bind(actor)
        .bind(plan_id)
        .bind(plan_hash)
        .execute(&mut *tx)
        .await;
        match r {
            Ok(_) => {}
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
                return Err(BridgeError::Conflict(
                    "Another SIDE ROOM server already mirrors this Discord server.".into(),
                ));
            }
            Err(e) => return Err(e.into()),
        }
        sqlx::query(
            "UPDATE bridge_sync_plans SET status = 'confirmed', confirmed_at = now() WHERE id = $1",
        )
        .bind(plan_id)
        .execute(&mut *tx)
        .await?;
        self.svc
            .audit(
                &mut tx,
                AuditRow {
                    guild,
                    actor,
                    action: AUDIT_MODE_UPDATE,
                    target: Some(guild),
                    changes: json!([{"key": "mode", "old_value": "selective", "new_value": "whole_guild"}]),
                    options: json!({"plan_id": plan_id.to_string(), "plan_hash": plan_hash}),
                },
            )
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Owner + MFA: whole_guild back to selective. Role maps and the snapshot go; channel maps stay (top-level
    /// ones become selected); holds are cancelled; **nothing is deleted on either side**.
    pub async fn switch_to_selective(&self, guild: i64, actor: i64) -> BridgeResult<()> {
        let bridge = get_bridge(self.pool(), guild)
            .await?
            .ok_or(BridgeError::NotFound("bridge"))?;
        if bridge.mode != SyncMode::WholeGuild.as_str() {
            return Err(BridgeError::Conflict(
                "This server is already bridged selectively.".into(),
            ));
        }
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM bridge_role_maps WHERE guild_id = $1")
            .bind(guild)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM bridge_discord_snapshot WHERE guild_id = $1")
            .bind(guild)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM bridge_pending_deletes WHERE guild_id = $1")
            .bind(guild)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM bridge_sync_ops WHERE guild_id = $1")
            .bind(guild)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM bridge_sync_plans WHERE guild_id = $1")
            .bind(guild)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE bridge_channel_maps SET selected = (parent_map_id IS NULL), sync_status = 'mapped' WHERE guild_id = $1")
            .bind(guild)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE bridges SET mode = 'selective', sync_state = 'off', mode_changed_by = $2, mode_changed_at = now(), \
             mode_plan_id = NULL, mode_plan_hash = NULL, updated_at = now() WHERE guild_id = $1",
        )
        .bind(guild)
        .bind(actor)
        .execute(&mut *tx)
        .await?;
        self.svc
            .audit(
                &mut tx,
                AuditRow {
                    guild,
                    actor,
                    action: AUDIT_MODE_UPDATE,
                    target: Some(guild),
                    changes: json!([{"key": "mode", "old_value": "whole_guild", "new_value": "selective"}]),
                    options: json!({}),
                },
            )
            .await?;
        tx.commit().await?;
        Ok(())
    }

    // ---- the seed ----

    async fn sr_roles(&self, guild: i64) -> BridgeResult<HashMap<i64, SrRoleRow>> {
        let rows = sqlx::query(
            "SELECT id, name, color, hoist, mentionable, unicode_emoji, permissions FROM roles WHERE guild_id = $1",
        )
        .bind(guild)
        .fetch_all(self.pool())
        .await?;
        let mut out = HashMap::new();
        for r in &rows {
            let id: i64 = r.try_get("id")?;
            out.insert(
                id,
                SrRoleRow {
                    name: r.try_get("name")?,
                    color: r.try_get("color")?,
                    hoist: r.try_get("hoist")?,
                    mentionable: r.try_get("mentionable")?,
                    unicode_emoji: r.try_get("unicode_emoji")?,
                    permissions: r.try_get("permissions")?,
                },
            );
        }
        Ok(out)
    }

    async fn sr_channels(&self, guild: i64) -> BridgeResult<HashMap<i64, SrChanRow>> {
        let rows = sqlx::query(
            "SELECT id, type, name, topic, parent_id, position, nsfw, rate_limit_per_user, bitrate, user_limit \
             FROM channels WHERE guild_id = $1",
        )
        .bind(guild)
        .fetch_all(self.pool())
        .await?;
        let mut out = HashMap::new();
        for r in &rows {
            let id: i64 = r.try_get("id")?;
            out.insert(
                id,
                SrChanRow {
                    id,
                    kind: r.try_get("type")?,
                    name: r.try_get::<Option<String>, _>("name")?.unwrap_or_default(),
                    topic: r.try_get("topic")?,
                    parent_id: r.try_get("parent_id")?,
                    position: r.try_get("position")?,
                    nsfw: r.try_get("nsfw")?,
                    rate_limit: r.try_get("rate_limit_per_user")?,
                    bitrate: r.try_get("bitrate")?,
                    user_limit: r.try_get("user_limit")?,
                },
            );
        }
        Ok(out)
    }

    /// Run (or resume) the seed of the confirmed plan. Idempotent: an item whose map exists is skipped, so a
    /// crash mid-seed and a restart creates nothing twice (the one window is between a Discord create and the
    /// insert of its map row, a single statement).
    pub async fn run_seed(&self, guild: i64) -> BridgeResult<SeedReport> {
        let bridge = get_bridge(self.pool(), guild)
            .await?
            .ok_or(BridgeError::NotFound("bridge"))?;
        let sync = store::get_sync(self.pool(), guild)
            .await?
            .ok_or(BridgeError::NotFound("bridge"))?;
        if sync.mode != SyncMode::WholeGuild || sync.state != SyncState::Seeding {
            return Err(BridgeError::Conflict("There is no seed to run.".into()));
        }
        let plan_id = sync
            .mode_plan_id
            .ok_or_else(|| BridgeError::Conflict("The confirmed plan is gone.".into()))?;
        let view = self.get_plan_any(guild, plan_id).await?;
        let owner = sync.mode_changed_by.unwrap_or(SYSTEM_USER_ID);
        let token = self.svc.open_bot_token(guild).await?;
        let dg = bridge.discord_guild_id;
        let api = &self.svc.discord;
        let run = self.svc.next_id();
        let reason = reason_for_op(run);
        let origin = BridgeOrigin {
            op_id: run,
            discord_user_id: None,
            sr_actor: Some(owner),
        };
        let info = api.guild_info(&token, dg).await?;
        let d = self.read_discord(&token, dg).await?;
        let mut report = SeedReport::default();

        // 1. Snapshot Discord before anything is written (the first snapshot wins).
        for r in &d.roles {
            holds::save_snapshot(
                self.pool(),
                guild,
                "role",
                r.id,
                &json!({"name": r.name, "permissions": r.permissions.to_string(), "position": r.position}),
            )
            .await?;
        }
        for c in &d.channels {
            let ow: Vec<Value> = c
                .overwrites
                .iter()
                .map(|o| json!({"id": o.id.to_string(), "type": o.kind, "allow": o.allow.to_string(), "deny": o.deny.to_string()}))
                .collect();
            holds::save_snapshot(
                self.pool(),
                guild,
                "channel",
                c.id,
                &json!({"name": c.name, "overwrites": ow}),
            )
            .await?;
        }

        // 2. Roles.
        let sr_roles = self.sr_roles(guild).await?;
        let mut role_sr_to_d: HashMap<i64, i64> = store::list_role_maps(self.pool(), guild)
            .await?
            .into_iter()
            .map(|m| (m.role_id, m.discord_role_id))
            .collect();
        let d_roles: HashMap<i64, &crate::discord_structure::GuildRole> =
            d.roles.iter().map(|r| (r.id, r)).collect();
        for item in view.plan.items.iter().filter(|i| i.kind == ItemKind::Role) {
            if item.sr_id.is_some_and(|s| role_sr_to_d.contains_key(&s)) {
                continue;
            }
            match item.action {
                Action::Match | Action::Pinned => {
                    let (Some(sid), Some(did)) = (item.sr_id, item.discord_id) else {
                        continue;
                    };
                    let Some(dr) = d_roles.get(&did) else {
                        continue;
                    };
                    let mut synced = SyncedState::new();
                    if let Some(sr) = sr_roles.get(&sid) {
                        synced.record("name", json!(sr.name), json!(dr.name));
                    }
                    let m = RoleMap {
                        id: self.svc.next_id(),
                        guild_id: guild,
                        role_id: sid,
                        discord_role_id: did,
                        is_everyone: sid == guild,
                        origin: Origin::Matched,
                        discord_only_bits: dr.permissions & DISCORD_ONLY_MASK,
                        synced,
                        sync_status: "mapped".into(),
                    };
                    if store::insert_role_map(self.pool(), &m).await? {
                        role_sr_to_d.insert(sid, did);
                        report.roles_matched += 1;
                    }
                }
                Action::CreateOnDiscord => {
                    let Some(sid) = item.sr_id else { continue };
                    let Some(sr) = sr_roles.get(&sid) else {
                        continue;
                    };
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
                    let m = RoleMap {
                        id: self.svc.next_id(),
                        guild_id: guild,
                        role_id: sid,
                        discord_role_id: created.id,
                        is_everyone: false,
                        origin: Origin::Sr,
                        discord_only_bits: 0,
                        synced,
                        sync_status: "mapped".into(),
                    };
                    store::insert_role_map(self.pool(), &m).await?;
                    role_sr_to_d.insert(sid, created.id);
                    report.roles_created_on_discord += 1;
                }
                Action::CreateHere => {
                    let Some(did) = item.discord_id else { continue };
                    let Some(dr) = d_roles.get(&did) else {
                        continue;
                    };
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
                    if item.import {
                        // The owner confirmed the plan: this is the owner granting, not Discord (design 9.4).
                        let shared = Permissions::from_bits_truncate(dr.permissions)
                            .intersection(Permissions::BRIDGE_SHARED);
                        if !shared.is_empty() {
                            self.sr
                                .import_role_permissions(guild, sid, shared, &origin)
                                .await?;
                            report.imported.push((sid, shared.bits()));
                        }
                    }
                    let mut synced = SyncedState::new();
                    synced.record("name", json!(dr.name), json!(dr.name));
                    let m = RoleMap {
                        id: self.svc.next_id(),
                        guild_id: guild,
                        role_id: sid,
                        discord_role_id: did,
                        is_everyone: false,
                        origin: Origin::Discord,
                        discord_only_bits: dr.permissions & DISCORD_ONLY_MASK,
                        synced,
                        sync_status: "mapped".into(),
                    };
                    store::insert_role_map(self.pool(), &m).await?;
                    role_sr_to_d.insert(sid, did);
                    report.roles_created_here += 1;
                }
            }
        }

        // 3. Role order: SIDE ROOM's, always.
        let mut order: Vec<(i64, i64)> = Vec::new();
        {
            let rows = sqlx::query(
                "SELECT id FROM roles WHERE guild_id = $1 AND id <> $1 ORDER BY position, id",
            )
            .bind(guild)
            .fetch_all(self.pool())
            .await?;
            let mut pos = 1;
            for r in &rows {
                let id: i64 = r.try_get("id")?;
                if let Some(did) = role_sr_to_d.get(&id) {
                    order.push((*did, pos));
                    pos += 1;
                }
            }
        }
        if !order.is_empty() {
            api.set_role_positions(&token, dg, &order, &reason).await?;
        }

        // 4. Categories, channels and threads.
        let sr_ch = self.sr_channels(guild).await?;
        let d_ch: HashMap<i64, &GuildChannel> = d
            .channels
            .iter()
            .chain(d.threads.iter())
            .map(|c| (c.id, c))
            .collect();
        let mut sr_to_d: HashMap<i64, i64> = HashMap::new();
        let mut d_to_sr: HashMap<i64, i64> = HashMap::new();
        for m in store::list_structure_maps(self.pool(), guild).await? {
            sr_to_d.insert(m.channel_id, m.discord_channel_id);
            d_to_sr.insert(m.discord_channel_id, m.channel_id);
        }
        // Parents before children: categories, channels, threads.
        for pass in [ItemKind::Category, ItemKind::Channel, ItemKind::Thread] {
            for item in view.plan.items.iter().filter(|i| i.kind == pass) {
                if item.sr_id.is_some_and(|s| sr_to_d.contains_key(&s))
                    || item.discord_id.is_some_and(|x| d_to_sr.contains_key(&x))
                {
                    continue;
                }
                match item.action {
                    Action::Pinned => {}
                    Action::Match => {
                        let (Some(sid), Some(did)) = (item.sr_id, item.discord_id) else {
                            continue;
                        };
                        let Some(src) = sr_ch.get(&sid) else { continue };
                        let name = d_ch.get(&did).map(|c| c.name.clone()).unwrap_or_default();
                        let mut synced = SyncedState::new();
                        synced.record("name", json!(src.name), json!(name));
                        let parent_map = self.parent_map_of(src.parent_id).await?;
                        let ok = store::insert_structure_map(
                            self.pool(),
                            &NewStructureMap {
                                id: self.svc.next_id(),
                                guild_id: guild,
                                channel_id: sid,
                                discord_channel_id: did,
                                discord_channel_name: name,
                                kind: map_kind(src.kind),
                                selected: false,
                                origin: Origin::Matched,
                                parent_map_id: parent_map,
                                created_by: owner,
                                synced,
                            },
                        )
                        .await?;
                        if ok {
                            sr_to_d.insert(sid, did);
                            d_to_sr.insert(did, sid);
                            report.channels_matched += 1;
                        }
                    }
                    Action::CreateOnDiscord => {
                        let Some(sid) = item.sr_id else { continue };
                        let Some(src) = sr_ch.get(&sid) else { continue };
                        let parent_d = src.parent_id.and_then(|p| sr_to_d.get(&p).copied());
                        // Types Discord only has with Community degrade to the nearest type.
                        let (kind, degraded) = degrade(src.kind as i64, info.is_community());
                        let mut params = ChannelParams {
                            kind: Some(kind),
                            name: Some(discord_name_for(src.kind, &src.name)),
                            topic: src.topic.clone().map(Some),
                            nsfw: Some(src.nsfw),
                            rate_limit_per_user: Some(i64::from(src.rate_limit)),
                            ..ChannelParams::default()
                        };
                        if src.kind != 4 {
                            params.parent_id = parent_d.map(Some);
                        }
                        if let Some(b) = src.bitrate {
                            params.bitrate = Some(i64::from(b).min(info.max_bitrate()));
                        }
                        params.user_limit = src.user_limit.map(i64::from);
                        let created = if channel_type::is_thread(kind) {
                            let Some(pd) = parent_d else { continue };
                            api.create_thread(&token, pd, &params, &reason).await?
                        } else {
                            api.create_channel(&token, dg, &params, &reason).await?
                        };
                        let mut synced = SyncedState::new();
                        synced.record("name", json!(src.name), json!(created.name));
                        let parent_map = self.parent_map_of(src.parent_id).await?;
                        store::insert_structure_map(
                            self.pool(),
                            &NewStructureMap {
                                id: self.svc.next_id(),
                                guild_id: guild,
                                channel_id: sid,
                                discord_channel_id: created.id,
                                discord_channel_name: created.name.clone(),
                                kind: map_kind(src.kind),
                                selected: false,
                                origin: Origin::Sr,
                                parent_map_id: parent_map,
                                created_by: owner,
                                synced,
                            },
                        )
                        .await?;
                        if let Some(why) = degraded {
                            if let Some(m) =
                                store::structure_map_for_channel(self.pool(), sid).await?
                            {
                                store::save_map_state(
                                    self.pool(),
                                    m.id,
                                    &m.synced,
                                    "degraded",
                                    Some(why),
                                )
                                .await?;
                            }
                            report.degraded += 1;
                        }
                        sr_to_d.insert(sid, created.id);
                        d_to_sr.insert(created.id, sid);
                        report.channels_created_on_discord += 1;
                    }
                    Action::CreateHere => {
                        let Some(did) = item.discord_id else { continue };
                        let Some(dc) = d_ch.get(&did) else { continue };
                        let parent_sr = dc.parent_id.and_then(|p| d_to_sr.get(&p).copied());
                        let kind = i16::try_from(dc.kind).unwrap_or(0);
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
                                    rate_limit_per_user: i32::try_from(dc.rate_limit_per_user)
                                        .unwrap_or(0),
                                    bitrate: dc.bitrate.and_then(|b| i32::try_from(b).ok()),
                                    user_limit: dc.user_limit.and_then(|b| i32::try_from(b).ok()),
                                },
                                &origin,
                            )
                            .await?;
                        let mut synced = SyncedState::new();
                        synced.record("name", json!(dc.name), json!(dc.name));
                        let parent_map = self.parent_map_of(parent_sr).await?;
                        store::insert_structure_map(
                            self.pool(),
                            &NewStructureMap {
                                id: self.svc.next_id(),
                                guild_id: guild,
                                channel_id: sid,
                                discord_channel_id: did,
                                discord_channel_name: dc.name.clone(),
                                kind: map_kind(kind),
                                selected: false,
                                origin: Origin::Discord,
                                parent_map_id: parent_map,
                                created_by: owner,
                                synced,
                            },
                        )
                        .await?;
                        sr_to_d.insert(sid, did);
                        d_to_sr.insert(did, sid);
                        report.channels_created_here += 1;
                    }
                }
            }
        }

        // 5. Positions. With the SIDE ROOM structure the order of every parent goes to Discord, in one call per
        // parent; with the Discord structure matched channels take Discord's order here instead (names and
        // order are two-way fields; the port applies them as the bridge).
        if view.plan.seed_structure == "sr" {
            let mut by_parent: HashMap<Option<i64>, Vec<&SrChanRow>> = HashMap::new();
            for c in sr_ch
                .values()
                .filter(|c| !channel_type::is_thread(c.kind as i64))
            {
                if sr_to_d.contains_key(&c.id) {
                    by_parent.entry(c.parent_id).or_default().push(c);
                }
            }
            for (parent, mut kids) in by_parent {
                kids.sort_by_key(|c| (c.position, c.id));
                let pd = parent.and_then(|p| sr_to_d.get(&p).copied());
                let order: Vec<ChannelPosition> = kids
                    .iter()
                    .enumerate()
                    .map(|(i, c)| ChannelPosition {
                        id: sr_to_d[&c.id],
                        position: i as i64,
                        parent_id: Some(pd),
                        lock_permissions: false,
                    })
                    .collect();
                if !order.is_empty() {
                    api.set_channel_positions(&token, dg, &order, &reason)
                        .await?;
                }
            }
        } else {
            for item in view
                .plan
                .items
                .iter()
                .filter(|i| i.action == Action::Match && i.kind != ItemKind::Role)
            {
                let (Some(sid), Some(did)) = (item.sr_id, item.discord_id) else {
                    continue;
                };
                let (Some(src), Some(dc)) = (sr_ch.get(&sid), d_ch.get(&did)) else {
                    continue;
                };
                let patch = super::patch::ChannelPatch {
                    name: (src.name != dc.name).then(|| dc.name.clone()),
                    position: Some(i32::try_from(dc.position).unwrap_or(0)),
                    ..Default::default()
                };
                self.sr
                    .apply(
                        guild,
                        &super::patch::SrStructuralPatch::Channel {
                            channel_id: sid,
                            patch,
                        },
                        &origin,
                    )
                    .await?;
            }
        }

        // 6. Overwrites of every mapped channel (BRG-SYN-002's pusher), then the permission words of matched roles.
        for (sid, did) in &sr_to_d {
            if sr_ch
                .get(sid)
                .is_some_and(|c| !channel_type::is_thread(c.kind as i64))
            {
                self.overwrites
                    .push_channel(guild, *sid, *did, &reason)
                    .await?;
            }
        }
        for item in view
            .plan
            .items
            .iter()
            .filter(|i| i.kind == ItemKind::Role && i.action == Action::Match)
        {
            let (Some(sid), Some(did)) = (item.sr_id, item.discord_id) else {
                continue;
            };
            let Some(sr) = sr_roles.get(&sid) else {
                continue;
            };
            let Some(dr) = d_roles.get(&did) else {
                continue;
            };
            // Shared bits from SIDE ROOM, Discord-only bits preserved. SIDE ROOM-only bits are cut by the mask.
            let word = Permissions::from_bits_truncate(sr.permissions as u64)
                .intersection(Permissions::BRIDGE_SHARED)
                .bits()
                | (dr.permissions & DISCORD_ONLY_MASK);
            if word != dr.permissions {
                api.edit_role(
                    &token,
                    dg,
                    did,
                    &RoleParams {
                        permissions: Some(word),
                        ..RoleParams::default()
                    },
                    &reason,
                )
                .await?;
            }
        }

        // 7. Live.
        store::set_sync_state(self.pool(), guild, SyncState::Live).await?;
        store::touch_reconcile(self.pool(), guild).await?;
        let mut conn = self.pool().acquire().await?;
        self.svc
            .audit(
                &mut conn,
                AuditRow {
                    guild,
                    actor: owner,
                    action: AUDIT_SYNC_RUN,
                    target: Some(guild),
                    changes: json!([]),
                    options: json!({
                        "seed": true,
                        "plan_id": plan_id.to_string(),
                        "counts": &report,
                    }),
                },
            )
            .await?;
        Ok(report)
    }

    async fn get_plan_any(&self, guild: i64, plan_id: i64) -> BridgeResult<PlanView> {
        // A confirmed plan keeps its row; read it without the expiry rewrite of `get_plan`.
        let row = sqlx::query(
            "SELECT id, guild_id, status, seed_structure, plan, blockers, plan_hash, created_by, \
             (extract(epoch from expires_at) * 1000)::bigint AS exp_ms FROM bridge_sync_plans WHERE id = $1 AND guild_id = $2",
        )
        .bind(plan_id)
        .bind(guild)
        .fetch_optional(self.pool())
        .await?
        .ok_or(BridgeError::NotFound("plan"))?;
        let stored: StoredPlan = serde_json::from_value(row.try_get::<Value, _>("plan")?)
            .map_err(|e| BridgeError::Storage(e.to_string()))?;
        Ok(PlanView {
            id: row.try_get("id")?,
            guild_id: row.try_get("guild_id")?,
            status: row.try_get("status")?,
            seed_structure: row.try_get("seed_structure")?,
            blockers: vec![],
            plan: stored.plan,
            plan_hash: row.try_get("plan_hash")?,
            created_by: row.try_get("created_by")?,
            expires_at_ms: row.try_get("exp_ms")?,
        })
    }

    async fn parent_map_of(&self, parent_sr: Option<i64>) -> BridgeResult<Option<i64>> {
        match parent_sr {
            Some(p) => Ok(store::structure_map_for_channel(self.pool(), p)
                .await?
                .map(|m| m.id)),
            None => Ok(None),
        }
    }

    // ---- the deletion hold ----

    /// A channel, category or thread was deleted on Discord: hold the SIDE ROOM one (hide now, delete later).
    pub async fn hold_deleted_on_discord(
        &self,
        guild: i64,
        discord_channel: i64,
        actor: Option<i64>,
        hours: i64,
    ) -> BridgeResult<bool> {
        let Some(m) = store::structure_map_for_discord(self.pool(), guild, discord_channel).await?
        else {
            return Ok(false);
        };
        holds::hold(
            self.pool(),
            guild,
            m.channel_id,
            discord_channel,
            Some(m.id),
            actor,
            hours,
        )
        .await
    }

    /// The hold job: delete every hold that is due, through the normal path (categories last). Returns how many.
    pub async fn run_due_holds(&self) -> BridgeResult<u32> {
        let mut n = 0;
        for h in holds::due(self.pool(), 50).await? {
            let origin = BridgeOrigin {
                op_id: self.svc.next_id(),
                discord_user_id: h.discord_actor_id,
                sr_actor: None,
            };
            match self
                .sr
                .delete_channel(h.guild_id, h.channel_id, &origin)
                .await
            {
                Ok(()) => n += 1,
                Err(e) => {
                    tracing::warn!(error = %e, channel = h.channel_id, "held deletion failed")
                }
            }
        }
        Ok(n)
    }

    /// "Restore": lift the hold and recreate the Discord side from SIDE ROOM's state; the map points at the new
    /// Discord channel. Needs `MANAGE_CHANNELS` here (the route checks).
    pub async fn restore_held(&self, guild: i64, channel: i64) -> BridgeResult<i64> {
        let hold = holds::release(self.pool(), channel)
            .await?
            .ok_or(BridgeError::NotFound("held channel"))?;
        let token = self.svc.open_bot_token(guild).await?;
        let bridge = get_bridge(self.pool(), guild)
            .await?
            .ok_or(BridgeError::NotFound("bridge"))?;
        let sr_ch = self.sr_channels(guild).await?;
        let src = sr_ch
            .get(&channel)
            .ok_or(BridgeError::NotFound("channel"))?;
        let parent_d = match src.parent_id {
            Some(p) => store::structure_map_for_channel(self.pool(), p)
                .await?
                .map(|m| m.discord_channel_id),
            None => None,
        };
        let params = ChannelParams {
            kind: Some(i64::from(src.kind)),
            name: Some(discord_name_for(src.kind, &src.name)),
            topic: src.topic.clone().map(Some),
            parent_id: if src.kind == 4 {
                None
            } else {
                parent_d.map(Some)
            },
            nsfw: Some(src.nsfw),
            ..ChannelParams::default()
        };
        let reason = reason_for_op(self.svc.next_id());
        let created = if channel_type::is_thread(i64::from(src.kind)) {
            let pd = parent_d
                .ok_or_else(|| BridgeError::Conflict("The parent channel is not mapped.".into()))?;
            self.svc
                .discord
                .create_thread(&token, pd, &params, &reason)
                .await?
        } else {
            self.svc
                .discord
                .create_channel(&token, bridge.discord_guild_id, &params, &reason)
                .await?
        };
        if let Some(map) = hold.map_id {
            sqlx::query(
                "UPDATE bridge_channel_maps SET discord_channel_id = $2, discord_channel_name = $3, origin = 'sr', \
                 sync_status = 'mapped' WHERE id = $1",
            )
            .bind(map)
            .bind(created.id)
            .bind(created.name.chars().take(200).collect::<String>())
            .execute(self.pool())
            .await?;
        }
        Ok(created.id)
    }
}

fn map_kind(sr_kind: i16) -> &'static str {
    match sr_kind {
        4 => "category",
        10..=12 => "thread",
        _ => "channel",
    }
}

/// The type Discord will accept when the server has no Community, and why it was changed.
fn degrade(kind: i64, community: bool) -> (i64, Option<&'static str>) {
    if community {
        return (kind, None);
    }
    match kind {
        channel_type::ANNOUNCEMENT => (
            channel_type::TEXT,
            Some("Announcement channels need Community on Discord; mirrored as a text channel."),
        ),
        channel_type::STAGE => (
            channel_type::VOICE,
            Some("Stage channels need Community on Discord; mirrored as a voice channel."),
        ),
        channel_type::FORUM | channel_type::MEDIA => (
            channel_type::TEXT,
            Some("Forum and media channels need Community on Discord; mirrored as a text channel."),
        ),
        _ => (kind, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_degrade_without_community() {
        assert_eq!(degrade(5, false).0, 0);
        assert_eq!(degrade(13, false).0, 2);
        assert_eq!(degrade(15, false).0, 0);
        assert_eq!(degrade(16, false).0, 0);
        assert_eq!(degrade(5, true), (5, None));
        assert_eq!(degrade(0, false), (0, None));
        assert_eq!(map_kind(4), "category");
        assert_eq!(map_kind(11), "thread");
        assert_eq!(map_kind(2), "channel");
    }
}
