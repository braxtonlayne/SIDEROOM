//! Queries over the sync columns and tables of migrations 1750 to 1753 (everything except the ops queue, the
//! plans and the holds, which have their own modules).

use serde_json::Value;
use sqlx::{PgPool, Row, postgres::PgRow};

use super::types::{SyncMode, SyncState, SyncedState};
use crate::error::{BridgeError, BridgeResult};

/// The sync-related columns of a bridge row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeSync {
    pub guild_id: i64,
    pub discord_guild_id: i64,
    pub mode: SyncMode,
    pub state: SyncState,
    pub mode_changed_by: Option<i64>,
    pub mode_changed_at_ms: Option<i64>,
    pub mode_plan_id: Option<i64>,
    pub last_reconcile_ms: Option<i64>,
}

pub async fn get_sync(pool: &PgPool, guild: i64) -> BridgeResult<Option<BridgeSync>> {
    let row = sqlx::query(
        "SELECT guild_id, discord_guild_id, mode, sync_state, mode_changed_by, mode_plan_id, \
         (extract(epoch from mode_changed_at) * 1000)::bigint AS changed_ms, \
         (extract(epoch from last_reconcile_at) * 1000)::bigint AS reconcile_ms \
         FROM bridges WHERE guild_id = $1",
    )
    .bind(guild)
    .fetch_optional(pool)
    .await?;
    row.map(|r| -> BridgeResult<BridgeSync> {
        Ok(BridgeSync {
            guild_id: r.try_get("guild_id")?,
            discord_guild_id: r.try_get("discord_guild_id")?,
            mode: SyncMode::parse(&r.try_get::<String, _>("mode")?)
                .ok_or_else(|| BridgeError::Storage("unknown bridge mode".into()))?,
            state: SyncState::parse(&r.try_get::<String, _>("sync_state")?)
                .ok_or_else(|| BridgeError::Storage("unknown sync state".into()))?,
            mode_changed_by: r.try_get("mode_changed_by")?,
            mode_changed_at_ms: r.try_get("changed_ms")?,
            mode_plan_id: r.try_get("mode_plan_id")?,
            last_reconcile_ms: r.try_get("reconcile_ms")?,
        })
    })
    .transpose()
}

pub async fn set_sync_state(pool: &PgPool, guild: i64, state: SyncState) -> BridgeResult<()> {
    sqlx::query("UPDATE bridges SET sync_state = $2, updated_at = now() WHERE guild_id = $1")
        .bind(guild)
        .bind(state.as_str())
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn touch_reconcile(pool: &PgPool, guild: i64) -> BridgeResult<()> {
    sqlx::query("UPDATE bridges SET last_reconcile_at = now() WHERE guild_id = $1")
        .bind(guild)
        .execute(pool)
        .await?;
    Ok(())
}

/// Bridges that are whole_guild and in a state that runs ops (what the periodic jobs walk).
pub async fn live_whole_guild(pool: &PgPool) -> BridgeResult<Vec<i64>> {
    Ok(sqlx::query_scalar(
        "SELECT guild_id FROM bridges WHERE mode = 'whole_guild' AND sync_state IN ('seeding', 'live') ORDER BY guild_id",
    )
    .fetch_all(pool)
    .await?)
}

/// Who made a pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Matched,
    /// The bridge created the Discord object.
    Sr,
    /// The bridge created the SIDE ROOM object.
    Discord,
}

impl Origin {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Matched => "matched",
            Self::Sr => "sr",
            Self::Discord => "discord",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "matched" => Some(Self::Matched),
            "sr" => Some(Self::Sr),
            "discord" => Some(Self::Discord),
            _ => None,
        }
    }
}

/// A channel map with its sync columns (1750).
#[derive(Debug, Clone, PartialEq)]
pub struct StructureMap {
    pub id: i64,
    pub guild_id: i64,
    pub channel_id: i64,
    pub discord_channel_id: i64,
    pub kind: String,
    pub selected: bool,
    pub origin: Origin,
    pub parent_map_id: Option<i64>,
    pub synced: SyncedState,
    pub sync_status: String,
    pub degraded_reason: Option<String>,
}

const SMAP_COLS: &str = "id, guild_id, channel_id, discord_channel_id, kind, selected, origin, parent_map_id, \
     synced_state, sync_status, degraded_reason";

fn smap_from(r: &PgRow) -> BridgeResult<StructureMap> {
    Ok(StructureMap {
        id: r.try_get("id")?,
        guild_id: r.try_get("guild_id")?,
        channel_id: r.try_get("channel_id")?,
        discord_channel_id: r.try_get("discord_channel_id")?,
        kind: r.try_get("kind")?,
        selected: r.try_get("selected")?,
        origin: Origin::parse(&r.try_get::<String, _>("origin")?)
            .ok_or_else(|| BridgeError::Storage("unknown map origin".into()))?,
        parent_map_id: r.try_get("parent_map_id")?,
        synced: SyncedState::from_json(&r.try_get::<Value, _>("synced_state")?),
        sync_status: r.try_get("sync_status")?,
        degraded_reason: r.try_get("degraded_reason")?,
    })
}

pub async fn list_structure_maps(pool: &PgPool, guild: i64) -> BridgeResult<Vec<StructureMap>> {
    let sql =
        format!("SELECT {SMAP_COLS} FROM bridge_channel_maps WHERE guild_id = $1 ORDER BY id");
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(guild)
        .fetch_all(pool)
        .await?;
    rows.iter().map(smap_from).collect()
}

pub async fn structure_map_for_channel(
    pool: &PgPool,
    channel: i64,
) -> BridgeResult<Option<StructureMap>> {
    let sql = format!("SELECT {SMAP_COLS} FROM bridge_channel_maps WHERE channel_id = $1");
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(channel)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(smap_from).transpose()
}

pub async fn structure_map_for_discord(
    pool: &PgPool,
    guild: i64,
    discord_channel: i64,
) -> BridgeResult<Option<StructureMap>> {
    let sql = format!(
        "SELECT {SMAP_COLS} FROM bridge_channel_maps WHERE guild_id = $1 AND discord_channel_id = $2"
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(guild)
        .bind(discord_channel)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(smap_from).transpose()
}

/// A map to insert for a channel that has no webhook (category, thread, voice, stage, or a whole_guild text
/// channel before its first outbound message: webhooks are created lazily).
#[derive(Debug, Clone)]
pub struct NewStructureMap {
    pub id: i64,
    pub guild_id: i64,
    pub channel_id: i64,
    pub discord_channel_id: i64,
    pub discord_channel_name: String,
    pub kind: &'static str,
    pub selected: bool,
    pub origin: Origin,
    pub parent_map_id: Option<i64>,
    pub created_by: i64,
    pub synced: SyncedState,
}

/// Insert a structure map (direction `both`). Returns false when either side is already mapped.
pub async fn insert_structure_map(pool: &PgPool, m: &NewStructureMap) -> BridgeResult<bool> {
    let name: String = m.discord_channel_name.chars().take(200).collect();
    let r = sqlx::query(
        "INSERT INTO bridge_channel_maps (id, guild_id, channel_id, discord_channel_id, discord_channel_name, \
         kind, direction, created_by, selected, origin, parent_map_id, synced_state) \
         VALUES ($1, $2, $3, $4, $5, $6, 'both', $7, $8, $9, $10, $11) ON CONFLICT DO NOTHING",
    )
    .bind(m.id)
    .bind(m.guild_id)
    .bind(m.channel_id)
    .bind(m.discord_channel_id)
    .bind(name)
    .bind(m.kind)
    .bind(m.created_by)
    .bind(m.selected)
    .bind(m.origin.as_str())
    .bind(m.parent_map_id)
    .bind(m.synced.to_json())
    .execute(pool)
    .await?;
    Ok(r.rows_affected() == 1)
}

pub async fn save_map_state(
    pool: &PgPool,
    map_id: i64,
    synced: &SyncedState,
    status: &str,
    degraded_reason: Option<&str>,
) -> BridgeResult<()> {
    sqlx::query(
        "UPDATE bridge_channel_maps SET synced_state = $2, sync_status = $3, degraded_reason = $4 WHERE id = $1",
    )
    .bind(map_id)
    .bind(synced.to_json())
    .bind(status)
    .bind(degraded_reason.map(|s| s.chars().take(200).collect::<String>()))
    .execute(pool)
    .await?;
    Ok(())
}

/// A role map (1751).
#[derive(Debug, Clone, PartialEq)]
pub struct RoleMap {
    pub id: i64,
    pub guild_id: i64,
    pub role_id: i64,
    pub discord_role_id: i64,
    pub is_everyone: bool,
    pub origin: Origin,
    pub discord_only_bits: u64,
    pub synced: SyncedState,
    pub sync_status: String,
}

const RMAP_COLS: &str = "id, guild_id, role_id, discord_role_id, is_everyone, origin, discord_only_bits, synced_state, sync_status";

fn rmap_from(r: &PgRow) -> BridgeResult<RoleMap> {
    Ok(RoleMap {
        id: r.try_get("id")?,
        guild_id: r.try_get("guild_id")?,
        role_id: r.try_get("role_id")?,
        discord_role_id: r.try_get("discord_role_id")?,
        is_everyone: r.try_get("is_everyone")?,
        origin: Origin::parse(&r.try_get::<String, _>("origin")?)
            .ok_or_else(|| BridgeError::Storage("unknown role map origin".into()))?,
        discord_only_bits: u64::try_from(r.try_get::<i64, _>("discord_only_bits")?).unwrap_or(0),
        synced: SyncedState::from_json(&r.try_get::<Value, _>("synced_state")?),
        sync_status: r.try_get("sync_status")?,
    })
}

pub async fn list_role_maps(pool: &PgPool, guild: i64) -> BridgeResult<Vec<RoleMap>> {
    let sql = format!("SELECT {RMAP_COLS} FROM bridge_role_maps WHERE guild_id = $1 ORDER BY id");
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(guild)
        .fetch_all(pool)
        .await?;
    rows.iter().map(rmap_from).collect()
}

pub async fn role_map_for_role(pool: &PgPool, role: i64) -> BridgeResult<Option<RoleMap>> {
    let sql = format!("SELECT {RMAP_COLS} FROM bridge_role_maps WHERE role_id = $1");
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(role)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(rmap_from).transpose()
}

pub async fn role_map_for_discord(
    pool: &PgPool,
    guild: i64,
    discord_role: i64,
) -> BridgeResult<Option<RoleMap>> {
    let sql = format!(
        "SELECT {RMAP_COLS} FROM bridge_role_maps WHERE guild_id = $1 AND discord_role_id = $2"
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(guild)
        .bind(discord_role)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(rmap_from).transpose()
}

fn bits_i64(bits: u64) -> i64 {
    i64::try_from(bits & (i64::MAX as u64)).unwrap_or(0)
}

/// Insert a role map. Returns false when either side is already mapped.
pub async fn insert_role_map(pool: &PgPool, m: &RoleMap) -> BridgeResult<bool> {
    let r = sqlx::query(
        "INSERT INTO bridge_role_maps (id, guild_id, role_id, discord_role_id, is_everyone, origin, \
         discord_only_bits, synced_state) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT DO NOTHING",
    )
    .bind(m.id)
    .bind(m.guild_id)
    .bind(m.role_id)
    .bind(m.discord_role_id)
    .bind(m.is_everyone)
    .bind(m.origin.as_str())
    .bind(bits_i64(m.discord_only_bits))
    .bind(m.synced.to_json())
    .execute(pool)
    .await?;
    Ok(r.rows_affected() == 1)
}

pub async fn save_role_state(
    pool: &PgPool,
    map_id: i64,
    synced: &SyncedState,
    discord_only_bits: u64,
) -> BridgeResult<()> {
    sqlx::query(
        "UPDATE bridge_role_maps SET synced_state = $2, discord_only_bits = $3, updated_at = now() WHERE id = $1",
    )
    .bind(map_id)
    .bind(synced.to_json())
    .bind(bits_i64(discord_only_bits))
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_role_maps(pool: &PgPool, guild: i64) -> BridgeResult<u64> {
    Ok(
        sqlx::query("DELETE FROM bridge_role_maps WHERE guild_id = $1")
            .bind(guild)
            .execute(pool)
            .await?
            .rows_affected(),
    )
}

/// Delete plans a day after they expired, failed or were superseded (doc 09). Confirmed plans stay with the bridge.
pub async fn prune_plans(pool: &PgPool) -> BridgeResult<u64> {
    Ok(sqlx::query(
        "DELETE FROM bridge_sync_plans WHERE status <> 'confirmed' AND expires_at < now() - interval '1 day'",
    )
    .execute(pool)
    .await?
    .rows_affected())
}

/// Bridges left in `seeding` (a restart during the seed): the worker runs their seed again, which resumes.
pub async fn seeding_guilds(pool: &PgPool) -> BridgeResult<Vec<i64>> {
    Ok(sqlx::query_scalar(
        "SELECT guild_id FROM bridges WHERE mode = 'whole_guild' AND sync_state = 'seeding'",
    )
    .fetch_all(pool)
    .await?)
}
