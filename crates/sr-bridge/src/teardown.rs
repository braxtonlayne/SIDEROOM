//! The clean stop (BRG-COR-001, D-381): [`BridgeService::teardown`].
//!
//! Switching the module off (or the operator's kill switch) does, in this order:
//!
//! 1. the gateway shard disconnects (the [`TeardownHook`](crate::service::TeardownHook)s);
//! 2. every Discord webhook the bridge created is deleted on Discord, three attempts with backoff; a failure is
//!    logged with the webhook id and remembered by id in `bridge_teardown_residue`, so the owner can remove it by
//!    hand from Discord's Integrations page (the dialog names it);
//! 3. in one transaction the maps, the message map, the queue and the bridge row (with its sealed
//!    token) are **deleted, not kept**: a map that points at a webhook that no longer exists is only a
//!    liability, and re-enabling means pasting the token and mapping afresh. One audit-log row records the
//!    teardown with the count of maps removed;
//! 4. a system message "The Discord bridge was switched off" is posted in each formerly mapped channel.
//!
//! Messages already mirrored on either side stay where they are: the DISCORD-tagged messages here are messages,
//! and the webhook posts on Discord belong to Discord.

use serde::Serialize;
use serde_json::json;
use sr_features::modules::bridge::BRIDGE_MODULE_ID;

use crate::error::BridgeResult;
use crate::service::{AUDIT_INTEGRATION_DELETE, AuditRow, BridgeService, SYSTEM_USER_ID};

/// `AuditAction::BridgeObjectRemoved` (BRG-SYN-002 owns the enum).
pub const AUDIT_OBJECT_REMOVED: i16 = 224;

/// A Discord webhook the teardown could not delete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Leftover {
    pub webhook_id: i64,
    pub discord_channel_id: i64,
}

/// What a teardown did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TeardownReport {
    /// Whether there was a bridge to tear down.
    pub was_connected: bool,
    pub maps_removed: i64,
    pub webhooks_deleted: i64,
    /// Webhooks still on Discord, to remove by hand.
    pub leftovers: Vec<Leftover>,
    /// SIDE ROOM channels that were told.
    pub channels_notified: Vec<i64>,
    /// What the whole_guild options did on Discord.
    pub removal: RemovalReport,
}

/// The choices of the whole_guild teardown dialog (design section 12). The default leaves Discord as it is.
/// Discord objects the bridge did not create are never deleted by the bot: there is no option for it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct TeardownOptions {
    /// Also delete the roles, categories, channels and threads the bridge created on Discord (map origin `sr`).
    #[serde(default)]
    pub remove_created: bool,
    /// Also restore the permissions of roles and overwrites that existed before the bridge to the snapshot.
    #[serde(default)]
    pub restore_permissions: bool,
}

/// A Discord object the teardown could not remove or restore (ids only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FailedObject {
    pub kind: String,
    pub id: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RemovalReport {
    pub roles: i64,
    pub channels: i64,
    pub restored_roles: i64,
    pub restored_channels: i64,
    pub failed: Vec<FailedObject>,
}

impl RemovalReport {
    pub fn removed(&self) -> i64 {
        self.roles + self.channels
    }
}

/// Why the bridge is being torn down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeardownReason {
    /// The owner switched the module off (or revoked the token).
    Owner,
    /// The operator switched `platform.bridge` off.
    PlatformKill,
}

impl TeardownReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::PlatformKill => "platform_kill",
        }
    }
}

/// The text of the system message in a formerly mapped channel.
pub const SWITCHED_OFF_NOTICE: &str = "The Discord bridge was switched off";

impl BridgeService {
    /// Tear down the server's bridge with the default options (Discord's structure is left as it is).
    pub async fn teardown(
        &self,
        guild: i64,
        actor: i64,
        reason: TeardownReason,
    ) -> BridgeResult<TeardownReport> {
        self.teardown_with(guild, actor, reason, TeardownOptions::default())
            .await
    }

    /// Tear down the server's bridge. Idempotent: a server with no bridge yields an empty report. `opts` is the
    /// whole_guild dialog (design section 12).
    pub async fn teardown_with(
        &self,
        guild: i64,
        actor: i64,
        reason: TeardownReason,
        opts: TeardownOptions,
    ) -> BridgeResult<TeardownReport> {
        // Teardowns are serialised, so the owner's request and the bus consumer's backstop never race: the second
        // finds nothing left and the first has finished by the time its caller hears.
        let _one_at_a_time = self.teardown_lock.lock().await;

        let hooks: Vec<_> = self.hooks.read().map(|h| h.clone()).unwrap_or_default();
        for hook in hooks {
            hook.disconnect(guild).await;
        }

        let maps: Vec<(i64, i64, i64)> = sqlx::query_as(
            "SELECT id, channel_id, discord_channel_id FROM bridge_channel_maps WHERE guild_id = $1 ORDER BY id",
        )
        .bind(guild)
        .fetch_all(&self.pool)
        .await?;
        let has_bridge: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM bridges WHERE guild_id = $1)")
                .bind(guild)
                .fetch_one(&self.pool)
                .await?;
        if !has_bridge && maps.is_empty() {
            return Ok(TeardownReport::default());
        }

        let mut report = TeardownReport {
            was_connected: has_bridge,
            maps_removed: i64::try_from(maps.len()).unwrap_or(i64::MAX),
            ..TeardownReport::default()
        };
        for (map_id, _channel, discord_channel) in &maps {
            // A map whose token cannot be opened (a rotated key) is reported as a leftover by id, never skipped.
            let Some((webhook, token)) = self.webhook_of(*map_id).await.unwrap_or_else(|e| {
                tracing::error!(map_id, error = %e, "could not open a bridge webhook token");
                None
            }) else {
                if let Some(id) = self.webhook_id_of(*map_id).await {
                    report.leftovers.push(Leftover {
                        webhook_id: id,
                        discord_channel_id: *discord_channel,
                    });
                }
                continue;
            };
            match self.delete_webhook_with_retries(webhook, &token).await {
                Ok(()) => report.webhooks_deleted += 1,
                Err(_) => {
                    tracing::warn!(
                        guild,
                        webhook_id = webhook,
                        discord_channel = *discord_channel,
                        "could not delete a bridge webhook on Discord; the owner removes it by hand"
                    );
                    report.leftovers.push(Leftover {
                        webhook_id: webhook,
                        discord_channel_id: *discord_channel,
                    });
                }
            }
        }

        // The whole_guild options act on Discord while the maps and the snapshot still exist.
        if opts.remove_created || opts.restore_permissions {
            report.removal = self.teardown_discord_structure(guild, &opts).await;
        }

        let mut tx = self.pool.begin().await?;
        for l in &report.removal.failed {
            sqlx::query(
                "INSERT INTO bridge_teardown_residue (guild_id, discord_webhook_id, discord_channel_id, reason) \
                 VALUES ($1, $2, $2, $3) ON CONFLICT DO NOTHING",
            )
            .bind(guild)
            .bind(l.id)
            .bind(format!("{} not removed on Discord", l.kind))
            .execute(&mut *tx)
            .await?;
        }
        if opts.remove_created
            && (report.removal.removed() > 0 || !report.removal.failed.is_empty())
        {
            self.audit(
                &mut tx,
                AuditRow {
                    guild,
                    actor,
                    action: AUDIT_OBJECT_REMOVED,
                    target: None,
                    changes: json!([]),
                    options: json!({
                        "roles": report.removal.roles,
                        "channels": report.removal.channels,
                        "failed": report.removal.failed.iter().map(|f| f.id.to_string()).collect::<Vec<_>>(),
                    }),
                },
            )
            .await?;
        }
        for l in &report.leftovers {
            sqlx::query(
                "INSERT INTO bridge_teardown_residue (guild_id, discord_webhook_id, discord_channel_id, reason) \
                 VALUES ($1, $2, $3, 'could not be deleted on Discord') ON CONFLICT DO NOTHING",
            )
            .bind(guild)
            .bind(l.webhook_id)
            .bind(l.discord_channel_id)
            .execute(&mut *tx)
            .await?;
        }
        // Children first; the foreign keys would cascade, but the order says what is removed.
        for sql in [
            "DELETE FROM bridge_outbox WHERE guild_id = $1",
            "DELETE FROM bridge_message_map WHERE map_id IN (SELECT id FROM bridge_channel_maps WHERE guild_id = $1)",
            "DELETE FROM bridge_channel_maps WHERE guild_id = $1",
        ] {
            sqlx::query(sql).bind(guild).execute(&mut *tx).await?;
        }
        // The bridge row holds the sealed token: deleting it wipes the token.
        let removed = sqlx::query("DELETE FROM bridges WHERE guild_id = $1")
            .bind(guild)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if removed > 0 || report.maps_removed > 0 {
            self.audit(
                &mut tx,
                AuditRow {
                    guild,
                    actor,
                    action: AUDIT_INTEGRATION_DELETE,
                    target: None,
                    changes: json!([{"key": "bridge", "old_value": {"maps": report.maps_removed}, "new_value": null}]),
                    options: json!({
                        "integration": "discord_bridge",
                        "count": report.maps_removed,
                        "webhooks_deleted": report.webhooks_deleted,
                        "webhooks_left": report.leftovers.len(),
                        "reason": reason.as_str(),
                    }),
                },
            )
            .await?;
        }
        tx.commit().await?;

        for (_, channel, _) in &maps {
            self.notices
                .system_message(*channel, SWITCHED_OFF_NOTICE)
                .await;
            self.notices.channel_changed(*channel).await;
            report.channels_notified.push(*channel);
        }
        tracing::info!(
            guild,
            maps = report.maps_removed,
            webhooks_deleted = report.webhooks_deleted,
            webhooks_left = report.leftovers.len(),
            reason = reason.as_str(),
            "discord bridge torn down"
        );
        Ok(report)
    }

    async fn webhook_id_of(&self, map_id: i64) -> Option<i64> {
        sqlx::query_scalar("SELECT discord_webhook_id FROM bridge_channel_maps WHERE id = $1")
            .bind(map_id)
            .fetch_optional(&self.pool)
            .await
            .ok()
            .flatten()
            .flatten()
    }

    /// The operator switched `platform.bridge` off: every server's bridge is torn down (logged per server) and
    /// each server's module is written off explicitly, so it stays off when the feature returns.
    pub async fn teardown_all(&self) -> BridgeResult<Vec<(i64, TeardownReport)>> {
        let mut guilds: Vec<i64> = sqlx::query_scalar("SELECT guild_id FROM bridges")
            .fetch_all(&self.pool)
            .await?;
        // Servers that had chosen the module on, found whether or not the kill hides it.
        guilds.extend(
            self.features
                .guilds_with_flag(BRIDGE_MODULE_ID, true)
                .await?
                .into_iter()
                .map(|g| i64::try_from(g.get()).unwrap_or(i64::MAX)),
        );
        guilds.sort_unstable();
        guilds.dedup();
        let mut out = Vec::new();
        for g in guilds {
            match self
                .teardown(g, SYSTEM_USER_ID, TeardownReason::PlatformKill)
                .await
            {
                Ok(report) => {
                    tracing::info!(
                        guild = g,
                        maps = report.maps_removed,
                        "platform kill: bridge torn down"
                    );
                    out.push((g, report));
                }
                Err(e) => tracing::error!(guild = g, error = %e, "platform kill: teardown failed"),
            }
            if let Err(e) = self
                .features
                .set_enabled(BRIDGE_MODULE_ID, &Self::guild_scope(g), Some(false))
                .await
            {
                tracing::error!(guild = g, error = %e, "platform kill: could not write the module off");
            }
        }
        Ok(out)
    }
}
