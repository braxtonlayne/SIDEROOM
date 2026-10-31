//! The deletion hold and the Discord snapshot (`bridge_pending_deletes`, `bridge_discord_snapshot`, 1753; design
//! sections 6.4 and 12).
//!
//! A channel, category or thread deleted on Discord is held, not deleted: the row hides it at once, and after
//! `bridge.delete_hold_hours` (default 24, 0 to 168, 0 means immediate) the hold job deletes it through the normal
//! channel delete path. Until then it can be restored or deleted now.

use serde_json::Value;
use sqlx::{PgPool, Row};

use crate::error::BridgeResult;

/// Default `bridge.delete_hold_hours`.
pub const DEFAULT_HOLD_HOURS: i64 = 24;
pub const MAX_HOLD_HOURS: i64 = 168;

/// Clamp a configured hold to its range.
pub fn clamp_hold_hours(h: i64) -> i64 {
    h.clamp(0, MAX_HOLD_HOURS)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hold {
    pub channel_id: i64,
    pub guild_id: i64,
    pub discord_channel_id: i64,
    pub map_id: Option<i64>,
    pub discord_actor_id: Option<i64>,
    pub held_at_ms: i64,
    pub delete_after_ms: i64,
}

const COLS: &str = "channel_id, guild_id, discord_channel_id, map_id, discord_actor_id, \
     (extract(epoch from held_at) * 1000)::bigint AS held_ms, (extract(epoch from delete_after) * 1000)::bigint AS after_ms";

fn hold_from(r: &sqlx::postgres::PgRow) -> BridgeResult<Hold> {
    Ok(Hold {
        channel_id: r.try_get("channel_id")?,
        guild_id: r.try_get("guild_id")?,
        discord_channel_id: r.try_get("discord_channel_id")?,
        map_id: r.try_get("map_id")?,
        discord_actor_id: r.try_get("discord_actor_id")?,
        held_at_ms: r.try_get("held_ms")?,
        delete_after_ms: r.try_get("after_ms")?,
    })
}

/// Hold a SIDE ROOM channel whose Discord twin was deleted. Idempotent: a second Discord delete event keeps the
/// first deadline. Marks the map `pending_delete`.
pub async fn hold(
    pool: &PgPool,
    guild: i64,
    channel: i64,
    discord_channel: i64,
    map_id: Option<i64>,
    actor: Option<i64>,
    hours: i64,
) -> BridgeResult<bool> {
    let hours = clamp_hold_hours(hours);
    let mut tx = pool.begin().await?;
    let r = sqlx::query(
        "INSERT INTO bridge_pending_deletes (channel_id, guild_id, discord_channel_id, map_id, discord_actor_id, delete_after) \
         VALUES ($1, $2, $3, $4, $5, now() + ($6::bigint * interval '1 hour')) ON CONFLICT DO NOTHING",
    )
    .bind(channel)
    .bind(guild)
    .bind(discord_channel)
    .bind(map_id)
    .bind(actor)
    .bind(hours)
    .execute(&mut *tx)
    .await?;
    if let Some(m) = map_id {
        sqlx::query("UPDATE bridge_channel_maps SET sync_status = 'pending_delete' WHERE id = $1")
            .bind(m)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(r.rows_affected() == 1)
}

/// The channel is hidden by a hold. Read paths treat it as gone.
pub async fn is_held(pool: &PgPool, channel: i64) -> BridgeResult<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM bridge_pending_deletes WHERE channel_id = $1)",
    )
    .bind(channel)
    .fetch_one(pool)
    .await?)
}

pub async fn list_held(pool: &PgPool, guild: i64) -> BridgeResult<Vec<Hold>> {
    let sql = format!(
        "SELECT {COLS} FROM bridge_pending_deletes WHERE guild_id = $1 ORDER BY delete_after, channel_id"
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(guild)
        .fetch_all(pool)
        .await?;
    rows.iter().map(hold_from).collect()
}

/// Holds whose time is up (the hold job deletes them, children before parents is the caller's order: threads
/// and channels of a category are listed first).
pub async fn due(pool: &PgPool, limit: i64) -> BridgeResult<Vec<Hold>> {
    let sql = format!(
        "SELECT p.channel_id, p.guild_id, p.discord_channel_id, p.map_id, p.discord_actor_id, \
         (extract(epoch from p.held_at) * 1000)::bigint AS held_ms, \
         (extract(epoch from p.delete_after) * 1000)::bigint AS after_ms \
         FROM bridge_pending_deletes p JOIN channels c ON c.id = p.channel_id \
         WHERE p.delete_after <= now() ORDER BY (c.type = 4), p.delete_after, p.channel_id LIMIT $1"
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(limit)
        .fetch_all(pool)
        .await?;
    rows.iter().map(hold_from).collect()
}

/// Lift a hold (restore). The channel is visible again; the caller recreates the Discord side and re-maps.
pub async fn release(pool: &PgPool, channel: i64) -> BridgeResult<Option<Hold>> {
    let mut tx = pool.begin().await?;
    let sql = format!("SELECT {COLS} FROM bridge_pending_deletes WHERE channel_id = $1 FOR UPDATE");
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(channel)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(row) = row else { return Ok(None) };
    let h = hold_from(&row)?;
    sqlx::query("DELETE FROM bridge_pending_deletes WHERE channel_id = $1")
        .bind(channel)
        .execute(&mut *tx)
        .await?;
    if let Some(m) = h.map_id {
        sqlx::query("UPDATE bridge_channel_maps SET sync_status = 'mapped' WHERE id = $1")
            .bind(m)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Some(h))
}

/// Cancel every hold of a server (the teardown: a hold assumed a live bridge).
pub async fn cancel_all(pool: &PgPool, guild: i64) -> BridgeResult<u64> {
    Ok(
        sqlx::query("DELETE FROM bridge_pending_deletes WHERE guild_id = $1")
            .bind(guild)
            .execute(pool)
            .await?
            .rows_affected(),
    )
}

/// Store (or replace) one snapshot row. Taken before the seed writes anything.
pub async fn save_snapshot(
    pool: &PgPool,
    guild: i64,
    kind: &str,
    discord_id: i64,
    data: &Value,
) -> BridgeResult<()> {
    sqlx::query(
        "INSERT INTO bridge_discord_snapshot (guild_id, kind, discord_id, data) VALUES ($1, $2, $3, $4) \
         ON CONFLICT (guild_id, kind, discord_id) DO NOTHING",
    )
    .bind(guild)
    .bind(kind)
    .bind(discord_id)
    .bind(data)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn load_snapshot(
    pool: &PgPool,
    guild: i64,
    kind: &str,
) -> BridgeResult<Vec<(i64, Value)>> {
    let rows = sqlx::query(
        "SELECT discord_id, data FROM bridge_discord_snapshot WHERE guild_id = $1 AND kind = $2 ORDER BY discord_id",
    )
    .bind(guild)
    .bind(kind)
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|r| Ok((r.try_get("discord_id")?, r.try_get("data")?)))
        .collect()
}

pub async fn delete_snapshot(pool: &PgPool, guild: i64) -> BridgeResult<u64> {
    Ok(
        sqlx::query("DELETE FROM bridge_discord_snapshot WHERE guild_id = $1")
            .bind(guild)
            .execute(pool)
            .await?
            .rows_affected(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hold_hours_are_clamped() {
        assert_eq!(clamp_hold_hours(-5), 0);
        assert_eq!(clamp_hold_hours(24), 24);
        assert_eq!(clamp_hold_hours(1000), MAX_HOLD_HOURS);
    }
}
