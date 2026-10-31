//! What the periodic jobs call (design sections 11.1, 11.2 and 6.4): the ops runner, the scheduled reconcile, the
//! hold job, and the manager's pause, resume and "Reconcile now".
//!
//! The ops runner is deliberately coarse: any due op of a bridge triggers one [`reconcile_guild`] pass for that
//! bridge, which compares every mapped object and so also covers every other pending op of the same guild (they
//! are all marked done together). An op is "look at X"; a pass looks at everything, which is correct if more work
//! than strictly needed. A failed pass puts the ops back with backoff, and after 8 attempts they are dead.

use std::collections::BTreeMap;

use super::engine::SyncEngine;
use super::ops::{self, NewOp};
use super::store;
use super::types::{ObjectKind, OpDirection, OpPriority, SyncMode, SyncState};
use crate::error::{BridgeError, BridgeResult};

/// "Reconcile now" is limited to once per five minutes.
pub const RECONCILE_NOW_MIN_GAP_SECS: i64 = 300;

/// Default `bridge.reconcile_minutes`.
pub const DEFAULT_RECONCILE_MINUTES: i64 = 15;

/// Clamp `bridge.reconcile_minutes` to 5 to 60.
pub fn clamp_reconcile_minutes(m: i64) -> i64 {
    m.clamp(5, 60)
}

/// What one runner pass did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PassReport {
    pub guilds: u32,
    pub ops_done: u32,
    pub ops_failed: u32,
}

impl SyncEngine {
    /// Queue a look at one object (an event handler calls this).
    pub async fn enqueue_look(
        &self,
        guild: i64,
        kind: ObjectKind,
        object_id: i64,
        direction: OpDirection,
        priority: OpPriority,
    ) -> BridgeResult<bool> {
        ops::enqueue(
            self.pool(),
            NewOp {
                id: self.svc.next_id(),
                guild_id: guild,
                kind,
                object_id,
                direction,
                priority,
                seed_run: None,
                step: 0,
            },
        )
        .await
    }

    /// Take the due ops and run one reconcile per bridge that has any. Returns what happened.
    pub async fn run_ops_pass(&self, limit: i64) -> BridgeResult<PassReport> {
        let claimed = ops::claim_due(self.pool(), limit).await?;
        let mut by_guild: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
        for op in &claimed {
            by_guild.entry(op.guild_id).or_default().push(op.id);
        }
        let mut rep = PassReport::default();
        for (guild, ids) in by_guild {
            rep.guilds += 1;
            match self.reconcile_guild(guild).await {
                Ok(_) => {
                    for id in ids {
                        ops::complete(self.pool(), id).await?;
                        rep.ops_done += 1;
                    }
                }
                Err(e) => {
                    tracing::warn!(guild, error = %e, "bridge sync pass failed");
                    for id in ids {
                        ops::fail(self.pool(), id, &e.to_string()).await?;
                        rep.ops_failed += 1;
                    }
                }
            }
        }
        Ok(rep)
    }

    /// The scheduled reconcile: queue a background pass for every live whole_guild bridge whose last pass is older
    /// than `minutes`. Returns how many were queued.
    pub async fn enqueue_scheduled_reconciles(&self, minutes: i64) -> BridgeResult<u32> {
        let minutes = clamp_reconcile_minutes(minutes);
        let guilds: Vec<i64> = sqlx::query_scalar(
            "SELECT guild_id FROM bridges WHERE mode = 'whole_guild' AND sync_state = 'live' \
             AND (last_reconcile_at IS NULL OR last_reconcile_at < now() - ($1::bigint * interval '1 minute'))",
        )
        .bind(minutes)
        .fetch_all(self.pool())
        .await?;
        let mut n = 0;
        for g in guilds {
            if self
                .enqueue_look(
                    g,
                    ObjectKind::Reconcile,
                    g,
                    OpDirection::Look,
                    OpPriority::Background,
                )
                .await?
            {
                n += 1;
            }
        }
        Ok(n)
    }

    /// "Reconcile now" (manager): queues a pass at once, at most once per five minutes.
    pub async fn reconcile_now(&self, guild: i64) -> BridgeResult<()> {
        let sync = store::get_sync(self.pool(), guild)
            .await?
            .ok_or(BridgeError::NotFound("bridge"))?;
        if sync.mode != SyncMode::WholeGuild || !sync.state.runs_ops() {
            return Err(BridgeError::Conflict(
                "Sync is not running for this server.".into(),
            ));
        }
        let recent: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM bridge_sync_ops WHERE guild_id = $1 AND object_kind = 'reconcile' \
             AND direction = 'look' AND priority = 0 AND created_at > now() - ($2::bigint * interval '1 second'))",
        )
        .bind(guild)
        .bind(RECONCILE_NOW_MIN_GAP_SECS)
        .fetch_one(self.pool())
        .await?;
        if recent {
            return Err(BridgeError::Conflict(
                "A reconcile was requested a moment ago. Try again in a few minutes.".into(),
            ));
        }
        self.enqueue_look(
            guild,
            ObjectKind::Reconcile,
            guild,
            OpDirection::Look,
            OpPriority::Revert,
        )
        .await?;
        Ok(())
    }

    /// "Pause" (manager): pushes, reverts and applies stop; SIDE ROOM is unaffected.
    pub async fn pause(&self, guild: i64) -> BridgeResult<()> {
        self.set_running(guild, SyncState::Paused, &[SyncState::Live])
            .await
    }

    /// "Resume" (manager): from a manual pause or from the pauses that a fresh preflight can lift.
    pub async fn resume(&self, guild: i64) -> BridgeResult<()> {
        self.set_running(
            guild,
            SyncState::Live,
            &[
                SyncState::Paused,
                SyncState::PausedDiscordAccess,
                SyncState::PausedInvalidRequests,
            ],
        )
        .await
    }

    async fn set_running(&self, guild: i64, to: SyncState, from: &[SyncState]) -> BridgeResult<()> {
        let sync = store::get_sync(self.pool(), guild)
            .await?
            .ok_or(BridgeError::NotFound("bridge"))?;
        if sync.mode != SyncMode::WholeGuild || !from.contains(&sync.state) {
            return Err(BridgeError::Conflict(format!(
                "Sync cannot go from {} to {} right now.",
                sync.state.as_str(),
                to.as_str()
            )));
        }
        store::set_sync_state(self.pool(), guild, to).await
    }

    /// The breaker (design 11.5): pause a bridge whose client saw too many invalid responses. The caller passes
    /// the count of 401/403/429 answers in the last ten minutes.
    pub async fn trip_if_invalid(
        &self,
        guild: i64,
        invalid_in_ten_minutes: usize,
    ) -> BridgeResult<bool> {
        if invalid_in_ten_minutes < 200 {
            return Ok(false);
        }
        let sync = store::get_sync(self.pool(), guild).await?;
        if sync.is_some_and(|s| s.state.runs_ops()) {
            store::set_sync_state(self.pool(), guild, SyncState::PausedInvalidRequests).await?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Check the bot still has what the mirror needs and pause before a single 403 is spent (design 10).
    /// Returns true when the bridge is now paused for access.
    pub async fn check_access(&self, guild: i64) -> BridgeResult<bool> {
        let sync = store::get_sync(self.pool(), guild)
            .await?
            .ok_or(BridgeError::NotFound("bridge"))?;
        if sync.mode != SyncMode::WholeGuild || !sync.state.runs_ops() {
            return Ok(false);
        }
        let bridge = crate::model::get_bridge(self.pool(), guild)
            .await?
            .ok_or(BridgeError::NotFound("bridge"))?;
        let token = self.svc.open_bot_token(guild).await?;
        let access = self
            .svc
            .discord
            .bot_access(&token, bridge.discord_guild_id, bridge.bot_user_id)
            .await?;
        if access.administrator && access.is_top_role() {
            return Ok(false);
        }
        store::set_sync_state(self.pool(), guild, SyncState::PausedDiscordAccess).await?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconcile_minutes_are_clamped() {
        assert_eq!(clamp_reconcile_minutes(1), 5);
        assert_eq!(clamp_reconcile_minutes(15), 15);
        assert_eq!(clamp_reconcile_minutes(500), 60);
    }
}
