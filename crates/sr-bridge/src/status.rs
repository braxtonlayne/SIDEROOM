//! What the owner's screen and the members' banner read, and the per-member opt-out.
//!
//! * [`BridgeService::status`]: the module and platform switches, the bridge, each map with its opt-out count and
//!   queue depth, the dead-letter counter and the webhooks a teardown left behind.
//! * [`BridgeService::public_channels`]: the bridged channels a member can see (the banner, the privacy page).
//! * The opt-out ("Don't mirror my messages"): a member's messages in one bridged channel stay here only.
//! * The data-request side: [`BridgeService::export_for`] lists a member's bridged messages (ids only);
//!   [`BridgeService::erase_for`] deletes the Discord copies the bridge posted for them (best effort, logged).

use serde::Serialize;
use sqlx::Row;

use crate::discord::CallError;
use crate::error::{BridgeError, BridgeResult};
use crate::model::{Bridge, ChannelMap, Direction, get_bridge, list_maps, map_for_channel};
use crate::service::BridgeService;
use crate::teardown::Leftover;

/// One map with the numbers beside it.
#[derive(Debug, Clone, Serialize)]
pub struct MapView {
    #[serde(flatten)]
    pub map: ChannelMap,
    /// Members who opted out of being mirrored in this channel.
    pub optouts: i64,
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

/// A bridged channel as members see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PublicChannel {
    pub channel_id: i64,
    pub direction: Direction,
    pub discord_guild_name: String,
    pub discord_channel_name: String,
    /// The viewer opted out of being mirrored here.
    pub opted_out: bool,
}

/// A member's bridged messages and opt-outs, ids only.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct MemberExport {
    pub message_ids: Vec<i64>,
    pub optout_channels: Vec<i64>,
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
        let optouts = sqlx::query(
            "SELECT channel_id, count(*) AS n FROM bridge_optouts WHERE guild_id = $1 GROUP BY channel_id",
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
            let o = optouts
                .iter()
                .find(|r| r.try_get::<i64, _>("channel_id").ok() == Some(map.channel_id))
                .and_then(|r| r.try_get::<i64, _>("n").ok())
                .unwrap_or(0);
            queued += q;
            dead_letters += d;
            views.push(MapView {
                map,
                optouts: o,
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

    /// The bridged channels of a server, for the banner, the privacy page and the channel menu. Empty while the
    /// module is off. Whether the viewer may see each channel is the caller's check.
    pub async fn public_channels(
        &self,
        guild: i64,
        viewer: i64,
    ) -> BridgeResult<Vec<PublicChannel>> {
        if !self.platform_on().await? || !self.module_on(guild).await? {
            return Ok(Vec::new());
        }
        let Some(bridge) = get_bridge(&self.pool, guild).await? else {
            return Ok(Vec::new());
        };
        let maps = list_maps(&self.pool, guild).await?;
        let out: Vec<i64> = sqlx::query_scalar(
            "SELECT channel_id FROM bridge_optouts WHERE guild_id = $1 AND user_id = $2",
        )
        .bind(guild)
        .bind(viewer)
        .fetch_all(&self.pool)
        .await?;
        Ok(maps
            .into_iter()
            .map(|m| PublicChannel {
                channel_id: m.channel_id,
                direction: m.direction,
                discord_guild_name: bridge.discord_guild_name.clone(),
                discord_channel_name: m.discord_channel_name.clone(),
                opted_out: out.contains(&m.channel_id),
            })
            .collect())
    }

    /// "Don't mirror my messages" in one bridged channel (or take it back). Their messages stay here only, and
    /// posts of theirs still waiting in the queue are dropped.
    pub async fn set_optout(&self, user: i64, channel: i64, opt_out: bool) -> BridgeResult<()> {
        let map = map_for_channel(&self.pool, channel)
            .await?
            .ok_or(BridgeError::NotFound("bridged channel"))?;
        if opt_out {
            sqlx::query(
                "INSERT INTO bridge_optouts (user_id, channel_id, guild_id) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
            )
            .bind(user)
            .bind(channel)
            .bind(map.guild_id)
            .execute(&self.pool)
            .await?;
            sqlx::query(
                "DELETE FROM bridge_outbox WHERE map_id = $1 AND kind = 'create' \
                 AND (payload ->> 'author_id')::bigint = $2",
            )
            .bind(map.id)
            .bind(user)
            .execute(&self.pool)
            .await?;
        } else {
            sqlx::query("DELETE FROM bridge_optouts WHERE user_id = $1 AND channel_id = $2")
                .bind(user)
                .bind(channel)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    pub(crate) async fn opted_out(&self, user: i64, channel: i64) -> BridgeResult<bool> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM bridge_optouts WHERE user_id = $1 AND channel_id = $2)",
        )
        .bind(user)
        .bind(channel)
        .fetch_one(&self.pool)
        .await?)
    }

    /// Data request: a member's bridged messages (ids only) and opt-outs.
    pub async fn export_for(&self, user: i64) -> BridgeResult<MemberExport> {
        let message_ids = sqlx::query_scalar(
            "SELECT sr_message_id FROM bridge_message_map WHERE author_id = $1 ORDER BY sr_message_id",
        )
        .bind(user)
        .fetch_all(&self.pool)
        .await?;
        let optout_channels = sqlx::query_scalar(
            "SELECT channel_id FROM bridge_optouts WHERE user_id = $1 ORDER BY channel_id",
        )
        .bind(user)
        .fetch_all(&self.pool)
        .await?;
        Ok(MemberExport {
            message_ids,
            optout_channels,
        })
    }

    /// Erasure: delete the Discord copies the bridge posted for the member (best effort, each failure logged by
    /// message id) and forget the map rows, the queue rows and the opt-outs. Returns how many copies were removed.
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
        sqlx::query("DELETE FROM bridge_outbox WHERE kind = 'create' AND (payload ->> 'author_id')::bigint = $1")
            .bind(user)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM bridge_optouts WHERE user_id = $1")
            .bind(user)
            .execute(&self.pool)
            .await?;
        Ok(removed)
    }
}
