//! Queries of the inbound half over the tables of migrations 1720 and 1731.

use sqlx::{PgPool, Row};

use crate::error::BridgeResult;
use crate::model::{
    Bridge, BridgeStatus, ChannelMap, MAP_COLS, MessageMapDirection, map_from, get_bridge,
};

/// The map of a Discord channel in this server's bridge.
pub async fn map_for_discord_channel(
    pool: &PgPool,
    guild: i64,
    discord_channel: i64,
) -> BridgeResult<Option<ChannelMap>> {
    let sql = format!(
        "SELECT {MAP_COLS} FROM bridge_channel_maps WHERE guild_id = $1 AND discord_channel_id = $2"
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(guild)
        .bind(discord_channel)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(map_from).transpose()
}

/// Whether `webhook` is one of the webhooks this server's bridge created (the loop guard).
pub async fn is_own_webhook(pool: &PgPool, guild: i64, webhook: i64) -> BridgeResult<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM bridge_channel_maps WHERE guild_id = $1 AND discord_webhook_id = $2)",
    )
    .bind(guild)
    .bind(webhook)
    .fetch_one(pool)
    .await?)
}

/// Our message and which way it started, for a Discord message of this map.
pub async fn sr_message_for_discord(
    pool: &PgPool,
    map: i64,
    discord_message: i64,
) -> BridgeResult<Option<(i64, MessageMapDirection)>> {
    let row = sqlx::query(
        "SELECT sr_message_id, direction FROM bridge_message_map \
         WHERE map_id = $1 AND discord_message_id = $2",
    )
    .bind(map)
    .bind(discord_message)
    .fetch_optional(pool)
    .await?;
    row.map(|r| -> BridgeResult<_> {
        let dir: String = r.try_get("direction")?;
        Ok((
            r.try_get("sr_message_id")?,
            MessageMapDirection::parse(&dir).unwrap_or(MessageMapDirection::In),
        ))
    })
    .transpose()
}

/// Remember an inbound pair. Repeating it is harmless.
pub async fn insert_inbound_map(
    pool: &PgPool,
    sr_message: i64,
    discord_message: i64,
    map: i64,
) -> BridgeResult<()> {
    sqlx::query(
        "INSERT INTO bridge_message_map (sr_message_id, discord_message_id, map_id, direction) \
         VALUES ($1, $2, $3, 'in') ON CONFLICT DO NOTHING",
    )
    .bind(sr_message)
    .bind(discord_message)
    .bind(map)
    .execute(pool)
    .await?;
    Ok(())
}

/// Forget a pair (the message was deleted on Discord) and the reactions kept for it.
pub async fn delete_inbound_map(pool: &PgPool, sr_message: i64) -> BridgeResult<()> {
    sqlx::query("DELETE FROM bridge_reactions WHERE sr_message_id = $1")
        .bind(sr_message)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM bridge_message_map WHERE sr_message_id = $1 AND direction = 'in'")
        .bind(sr_message)
        .execute(pool)
        .await?;
    Ok(())
}

/// Record that a Discord user reacted. True when this is the first Discord user with that emoji on the message
/// (the integration author's reaction has to be added now).
pub async fn add_reactor(
    pool: &PgPool,
    map: i64,
    sr_message: i64,
    emoji: &str,
    user: i64,
) -> BridgeResult<bool> {
    let inserted = sqlx::query(
        "INSERT INTO bridge_reactions (map_id, sr_message_id, emoji, discord_user_id) \
         VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
    )
    .bind(map)
    .bind(sr_message)
    .bind(emoji)
    .bind(user)
    .execute(pool)
    .await?
    .rows_affected()
        == 1;
    if !inserted {
        return Ok(false);
    }
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM bridge_reactions WHERE sr_message_id = $1 AND emoji = $2",
    )
    .bind(sr_message)
    .bind(emoji)
    .fetch_one(pool)
    .await?;
    Ok(n == 1)
}

/// Record that a Discord user took their reaction away. True when it was the last one (the integration
/// author's reaction has to go).
pub async fn remove_reactor(
    pool: &PgPool,
    sr_message: i64,
    emoji: &str,
    user: i64,
) -> BridgeResult<bool> {
    let removed = sqlx::query(
        "DELETE FROM bridge_reactions WHERE sr_message_id = $1 AND emoji = $2 AND discord_user_id = $3",
    )
    .bind(sr_message)
    .bind(emoji)
    .bind(user)
    .execute(pool)
    .await?
    .rows_affected()
        == 1;
    if !removed {
        return Ok(false);
    }
    let left: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM bridge_reactions WHERE sr_message_id = $1 AND emoji = $2",
    )
    .bind(sr_message)
    .bind(emoji)
    .fetch_one(pool)
    .await?;
    Ok(left == 0)
}

pub async fn set_status(pool: &PgPool, guild: i64, status: BridgeStatus) -> BridgeResult<()> {
    sqlx::query("UPDATE bridges SET status = $2, updated_at = now() WHERE guild_id = $1")
        .bind(guild)
        .bind(status.as_str())
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_intents_granted(pool: &PgPool, guild: i64, intents: u64) -> BridgeResult<()> {
    sqlx::query("UPDATE bridges SET intents_granted = $2, updated_at = now() WHERE guild_id = $1")
        .bind(guild)
        .bind(i64::try_from(intents).unwrap_or(0))
        .execute(pool)
        .await?;
    Ok(())
}

/// Bridges that should have a shard running, oldest first: every bridge whose token Discord has not refused.
pub async fn startable(pool: &PgPool) -> BridgeResult<Vec<Bridge>> {
    let rows = sqlx::query("SELECT guild_id FROM bridges WHERE status <> 'token_invalid' ORDER BY created_at, guild_id")
        .fetch_all(pool)
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        if let Some(b) = get_bridge(pool, r.try_get("guild_id")?).await? {
            out.push(b);
        }
    }
    Ok(out)
}
