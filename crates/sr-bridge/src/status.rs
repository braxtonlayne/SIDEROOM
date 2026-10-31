//! What the owner's screen and the channel payload read, and the data-request side.
//!
//! There is no per-member opt-out (owner decision): a bridged channel is bridged, and every message in it is
//! mirrored per the map's direction.
//!
//! * [`BridgeService::status`]: the module and platform switches, the bridge, each map with its queue depth, the
//!   dead-letter counter and the webhooks a teardown left behind.
//! * [`BridgeService::directions_for_guild`] and [`BridgeService::direction_for_channel`]: the bridge state of a
//!   channel, which the channel payload carries as `sr_bridge` so the client draws the banner without a request.
//! * The data-request side: [`BridgeService::export_for`] lists a member's bridged messages (ids only);
//!   [`BridgeService::erase_for`] deletes the Discord copies the bridge posted for them (best effort, logged).

use std::collections::HashMap;

use serde::Serialize;
use sqlx::Row;

use crate::discord::CallError;
use crate::error::BridgeResult;
use crate::model::{Bridge, ChannelMap, Direction, get_bridge, list_maps};
use crate::service::BridgeService;
use crate::teardown::Leftover;

/// One map with the numbers beside it.
#[derive(Debug, Clone, Serialize)]
pub struct MapView {
    #[serde(flatten)]
    pub map: ChannelMap,
    /// Posts waiting for their turn under Discord's rate limits.
    pub queued: i64,
    /// Posts that failed for good (the dead-letter counter of this map).
    pub dead: i64,
}

/// Everything the owner's screen shows.
#[derive(Debug, Clone, Serialize)]
pub struct StatusReport {
    /// The operator's switch (`platform.bridge`).
    pub platform_on: bool,
    /// The server's module (`bridge.enabled`).
    pub module_on: bool,
    /// Whether a bot token is held. The token itself is never returned.
    pub connected: bool,
    pub bridge: Option<Bridge>,
    pub maps: Vec<MapView>,
    pub queued: i64,
    pub dead_letters: i64,
    pub leftovers: Vec<Leftover>,
}

/// A member's bridged messages, ids only.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct MemberExport {
    pub message_ids: Vec<i64>,
}

/// The numbers of the Admin > System tile.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Totals {
    /// The operator's switch.
    pub platform_on: bool,
    /// Servers holding a bot token.
    pub bridges: i64,
    /// Channel maps instance-wide.
    pub maps: i64,
    /// Posts waiting.
    pub queued: i64,
    /// Posts that failed for good: the dead-letter counter.
    pub dead_letters: i64,
}

impl BridgeService {
    /// The webhooks a teardown could not delete, to remove by hand.
    pub async fn leftovers(&self, guild: i64) -> BridgeResult<Vec<Leftover>> {
        let rows = sqlx::query(
            "SELECT discord_webhook_id, discord_channel_id FROM bridge_teardown_residue \
             WHERE guild_id = $1 ORDER BY failed_at, discord_webhook_id",
        )
        .bind(guild)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| {
                Ok(Leftover {
                    webhook_id: r.try_get("discord_webhook_id")?,
                    discord_channel_id: r.try_get("discord_channel_id")?,
                })
            })
            .collect()
    }

    /// The owner's screen.
    pub async fn status(&self, guild: i64) -> BridgeResult<StatusReport> {
        let bridge = get_bridge(&self.pool, guild).await?;
        let maps = list_maps(&self.pool, guild).await?;
        let counts = sqlx::query(
            "SELECT map_id, \
                    count(*) FILTER (WHERE state = 'pending') AS queued, \
                    count(*) FILTER (WHERE state = 'dead') AS dead \
             FROM bridge_outbox WHERE guild_id = $1 GROUP BY map_id",
        )
        .bind(guild)
        .fetch_all(&self.pool)
        .await?;
        let mut views = Vec::with_capacity(maps.len());
        let (mut queued, mut dead_letters) = (0, 0);
        for map in maps {
            let (q, d) = counts
                .iter()
                .find(|r| r.try_get::<i64, _>("map_id").ok() == Some(map.id))
                .map(|r| {
                    (
                        r.try_get::<i64, _>("queued").unwrap_or(0),
                        r.try_get::<i64, _>("dead").unwrap_or(0),
                    )
                })
                .unwrap_or((0, 0));
            queued += q;
            dead_letters += d;
            views.push(MapView {
                map,
                queued: q,
                dead: d,
            });
        }
        Ok(StatusReport {
            platform_on: self.platform_on().await?,
            module_on: self.module_on(guild).await?,
            connected: bridge.is_some(),
            bridge,
            maps: views,
            queued,
            dead_letters,
            leftovers: self.leftovers(guild).await?,
        })
    }

    /// Posts that failed for good, instance-wide: the number on Admin > System.
    pub async fn dead_letter_count(&self) -> BridgeResult<i64> {
        Ok(
            sqlx::query_scalar("SELECT count(*) FROM bridge_outbox WHERE state = 'dead'")
                .fetch_one(&self.pool)
                .await?,
        )
    }

    /// Posts waiting, instance-wide.
    pub async fn queued_count(&self) -> BridgeResult<i64> {
        Ok(
            sqlx::query_scalar("SELECT count(*) FROM bridge_outbox WHERE state = 'pending'")
                .fetch_one(&self.pool)
                .await?,
        )
    }

    /// Instance-wide figures for Admin > System.
    pub async fn totals(&self) -> BridgeResult<Totals> {
        let (bridges, maps): (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM bridges), (SELECT count(*) FROM bridge_channel_maps)",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(Totals {
            platform_on: self.platform_on().await?,
            bridges,
            maps,
            queued: self.queued_count().await?,
            dead_letters: self.dead_letter_count().await?,
        })
    }

    /// The bridge state of every bridged channel of a server (channel id to direction), for the channel payload
    /// (`sr_bridge`). Empty while the operator's switch or the server's module is off.
    pub async fn directions_for_guild(&self, guild: i64) -> BridgeResult<HashMap<i64, Direction>> {
        if !self.platform_on().await? || !self.module_on(guild).await? {
            return Ok(HashMap::new());
        }
        Ok(list_maps(&self.pool, guild)
            .await?
            .into_iter()
            .map(|m| (m.channel_id, m.direction))
            .collect())
    }

    /// The bridge state of one channel, or `None` when it is not bridged (or the bridge is off).
    pub async fn direction_for_channel(&self, channel: i64) -> BridgeResult<Option<Direction>> {
        let Some(map) = crate::model::map_for_channel(&self.pool, channel).await? else {
            return Ok(None);
        };
        if !self.platform_on().await? || !self.module_on(map.guild_id).await? {
            return Ok(None);
        }
        Ok(Some(map.direction))
    }

    /// Data request: a member's bridged messages (ids only).
    pub async fn export_for(&self, user: i64) -> BridgeResult<MemberExport> {
        let message_ids = sqlx::query_scalar(
            "SELECT sr_message_id FROM bridge_message_map WHERE author_id = $1 ORDER BY sr_message_id",
        )
        .bind(user)
        .fetch_all(&self.pool)
        .await?;
        Ok(MemberExport { message_ids })
    }

    /// Erasure: delete the Discord copies the bridge posted for the member (best effort, each failure logged by
    /// message id) and forget the map rows and the queue rows. Returns how many copies were removed.
    pub async fn erase_for(&self, user: i64) -> BridgeResult<u64> {
        let rows: Vec<(i64, i64, i64)> = sqlx::query_as(
            "SELECT sr_message_id, discord_message_id, map_id FROM bridge_message_map \
             WHERE author_id = $1 AND direction = 'out'",
        )
        .bind(user)
        .fetch_all(&self.pool)
        .await?;
        let mut removed = 0;
        for (sr_message, discord_message, map_id) in rows {
            if let Ok(Some((webhook, token))) = self.webhook_of(map_id).await {
                match self
                    .discord
                    .delete_webhook_message(webhook, &token, discord_message)
                    .await
                {
                    Ok(()) | Err(CallError::Gone(_)) => removed += 1,
                    Err(e) => tracing::warn!(
                        sr_message,
                        discord_message,
                        webhook_id = webhook,
                        error = %e,
                        "could not delete a mirrored message on Discord during erasure"
                    ),
                }
            }
            sqlx::query("DELETE FROM bridge_message_map WHERE sr_message_id = $1")
                .bind(sr_message)
                .execute(&self.pool)
                .await?;
        }
        sqlx::query(
            "DELETE FROM bridge_outbox WHERE kind = 'create' AND (payload ->> 'author_id')::bigint = $1",
        )
        .bind(user)
        .execute(&self.pool)
        .await?;
        Ok(removed)
    }
}
