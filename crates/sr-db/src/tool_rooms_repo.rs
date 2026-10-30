//! Postgres implementation of [`sr_chat::tool_rooms::ToolRoomStore`] (UTL-COL-001, migration 1660).
//! Tests: `tests/tool_rooms_repo.rs` (needs `SR_TEST_DATABASE_URL`).

use async_trait::async_trait;
use serde_json::{Map, Value};
use sqlx::{PgPool, Row, postgres::PgRow};
use sr_chat::repo::{StoreError, StoreResult};
use sr_chat::tool_rooms::{
    MAX_ROOMS_PER_CHANNEL, Patch, RoomError, ToolRoom, ToolRoomStore, apply_patch,
};

#[derive(Clone)]
pub struct PgToolRooms {
    pool: PgPool,
}

impl PgToolRooms {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn sdb(e: sqlx::Error) -> StoreError {
    StoreError(e.to_string())
}

fn rerr(e: sqlx::Error) -> RoomError {
    RoomError::Store(e.to_string())
}

const COLS: &str = "id, channel_id, guild_id, kind, title, state, version, created_by, \
     (extract(epoch FROM created_at) * 1000)::bigint AS created_ms, \
     (extract(epoch FROM updated_at) * 1000)::bigint AS updated_ms, \
     (extract(epoch FROM expires_at) * 1000)::bigint AS expires_ms";

fn row(r: &PgRow) -> ToolRoom {
    let state: Value = r.get("state");
    ToolRoom {
        id: r.get::<i64, _>("id") as u64,
        channel_id: r.get::<i64, _>("channel_id") as u64,
        guild_id: r.get::<Option<i64>, _>("guild_id").map(|g| g as u64),
        kind: r.get("kind"),
        title: r.get("title"),
        state: match state {
            Value::Object(m) => m,
            _ => Map::new(),
        },
        version: r.get("version"),
        created_by: r.get::<Option<i64>, _>("created_by").map(|u| u as u64),
        created_ms: r.get("created_ms"),
        updated_ms: r.get("updated_ms"),
        expires_ms: r.get("expires_ms"),
    }
}

fn ts(ms: i64) -> f64 {
    ms as f64 / 1000.0
}

#[async_trait]
impl ToolRoomStore for PgToolRooms {
    async fn create(&self, room: ToolRoom, now_ms: i64) -> Result<ToolRoom, RoomError> {
        let mut tx = self.pool.begin().await.map_err(rerr)?;
        // Serialise creators per channel so the cap holds under concurrency.
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(room.channel_id as i64)
            .execute(&mut *tx)
            .await
            .map_err(rerr)?;
        let live: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM tool_rooms WHERE channel_id = $1 AND expires_at > to_timestamp($2)",
        )
        .bind(room.channel_id as i64)
        .bind(ts(now_ms))
        .fetch_one(&mut *tx)
        .await
        .map_err(rerr)?;
        if live as usize >= MAX_ROOMS_PER_CHANNEL {
            return Err(RoomError::TooMany);
        }
        let sql = format!(
            "INSERT INTO tool_rooms (id, channel_id, guild_id, kind, title, state, version, created_by, \
             created_at, updated_at, expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6, 1, $7, to_timestamp($8), to_timestamp($8), to_timestamp($9)) \
             RETURNING {COLS}"
        );
        let r = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(room.id as i64)
            .bind(room.channel_id as i64)
            .bind(room.guild_id.map(|g| g as i64))
            .bind(&room.kind)
            .bind(&room.title)
            .bind(Value::Object(room.state.clone()))
            .bind(room.created_by.map(|u| u as i64))
            .bind(ts(room.created_ms))
            .bind(ts(room.expires_ms))
            .fetch_one(&mut *tx)
            .await
            .map_err(rerr)?;
        tx.commit().await.map_err(rerr)?;
        Ok(row(&r))
    }

    async fn get(&self, channel: u64, id: u64, now_ms: i64) -> StoreResult<Option<ToolRoom>> {
        let sql = format!(
            "SELECT {COLS} FROM tool_rooms WHERE id = $1 AND channel_id = $2 AND expires_at > to_timestamp($3)"
        );
        let r = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id as i64)
            .bind(channel as i64)
            .bind(ts(now_ms))
            .fetch_optional(&self.pool)
            .await
            .map_err(sdb)?;
        Ok(r.as_ref().map(row))
    }

    async fn list(&self, channel: u64, now_ms: i64) -> StoreResult<Vec<ToolRoom>> {
        let sql = format!(
            "SELECT {COLS} FROM tool_rooms WHERE channel_id = $1 AND expires_at > to_timestamp($2) \
             ORDER BY id DESC LIMIT {MAX_ROOMS_PER_CHANNEL}"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(channel as i64)
            .bind(ts(now_ms))
            .fetch_all(&self.pool)
            .await
            .map_err(sdb)?;
        Ok(rows.iter().map(row).collect())
    }

    async fn patch(
        &self,
        channel: u64,
        id: u64,
        patch: &Patch,
        now_ms: i64,
    ) -> Result<ToolRoom, RoomError> {
        let mut tx = self.pool.begin().await.map_err(rerr)?;
        let sql = format!(
            "SELECT {COLS} FROM tool_rooms WHERE id = $1 AND channel_id = $2 \
             AND expires_at > to_timestamp($3) FOR UPDATE"
        );
        let cur = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id as i64)
            .bind(channel as i64)
            .bind(ts(now_ms))
            .fetch_optional(&mut *tx)
            .await
            .map_err(rerr)?
            .ok_or(RoomError::NotFound)?;
        let cur = row(&cur);
        // The version rule runs against the locked row, so two writers on one version cannot both win.
        let (state, title) = apply_patch(&cur, patch)?;
        let sql = format!(
            "UPDATE tool_rooms SET state = $2, title = $3, version = version + 1, \
             updated_at = to_timestamp($4) WHERE id = $1 RETURNING {COLS}"
        );
        let r = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id as i64)
            .bind(Value::Object(state))
            .bind(title)
            .bind(ts(now_ms))
            .fetch_one(&mut *tx)
            .await
            .map_err(rerr)?;
        tx.commit().await.map_err(rerr)?;
        Ok(row(&r))
    }

    async fn delete(&self, channel: u64, id: u64) -> StoreResult<bool> {
        let n = sqlx::query("DELETE FROM tool_rooms WHERE id = $1 AND channel_id = $2")
            .bind(id as i64)
            .bind(channel as i64)
            .execute(&self.pool)
            .await
            .map_err(sdb)?
            .rows_affected();
        Ok(n > 0)
    }

    async fn sweep(&self, now_ms: i64) -> StoreResult<u64> {
        Ok(
            sqlx::query("DELETE FROM tool_rooms WHERE expires_at <= to_timestamp($1)")
                .bind(ts(now_ms))
                .execute(&self.pool)
                .await
                .map_err(sdb)?
                .rows_affected(),
        )
    }
}
