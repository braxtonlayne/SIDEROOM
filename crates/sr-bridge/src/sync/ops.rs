//! The sync ops queue (`bridge_sync_ops`, migration 1752; design section 11.2).
//!
//! An op is "look at object X". One PENDING row exists per (guild, kind, object, direction): enqueueing again
//! pushes the due time out by the debounce and keeps the more urgent priority, so a burst of edits is one row and
//! the payload (computed when the op runs) is always the newest state.

use sqlx::{PgPool, Row, postgres::PgRow};

use super::types::{ObjectKind, OpDirection, OpPriority, OpState, SyncOp};
use crate::error::{BridgeError, BridgeResult};

const COLS: &str = "id, guild_id, object_kind, object_id, direction, state, seed_run, step, priority, attempts, \
     (extract(epoch from due_at) * 1000)::bigint AS due_ms, last_error";

fn op_from(r: &PgRow) -> BridgeResult<SyncOp> {
    let bad = |what: &str| BridgeError::Storage(format!("unknown sync op {what}"));
    Ok(SyncOp {
        id: r.try_get("id")?,
        guild_id: r.try_get("guild_id")?,
        kind: ObjectKind::parse(&r.try_get::<String, _>("object_kind")?)
            .ok_or_else(|| bad("kind"))?,
        object_id: r.try_get("object_id")?,
        direction: OpDirection::parse(&r.try_get::<String, _>("direction")?)
            .ok_or_else(|| bad("direction"))?,
        state: OpState::parse(&r.try_get::<String, _>("state")?).ok_or_else(|| bad("state"))?,
        seed_run: r.try_get("seed_run")?,
        step: r.try_get("step")?,
        priority: OpPriority::from_i16(r.try_get("priority")?).ok_or_else(|| bad("priority"))?,
        attempts: r.try_get("attempts")?,
        due_at_ms: r.try_get("due_ms")?,
        last_error: r.try_get("last_error")?,
    })
}

/// What to enqueue.
#[derive(Debug, Clone, Copy)]
pub struct NewOp {
    pub id: i64,
    pub guild_id: i64,
    pub kind: ObjectKind,
    pub object_id: i64,
    pub direction: OpDirection,
    pub priority: OpPriority,
    pub seed_run: Option<i64>,
    pub step: i16,
}

/// Queue a look at an object. Returns true when a new row was made, false when a pending one was coalesced.
pub async fn enqueue(pool: &PgPool, op: NewOp) -> BridgeResult<bool> {
    // `xmax = 0` is true for a freshly inserted row and false for one the conflict clause updated.
    let fresh: bool = sqlx::query_scalar(
        "INSERT INTO bridge_sync_ops (id, guild_id, object_kind, object_id, direction, priority, seed_run, step, due_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now() + ($9::bigint * interval '1 millisecond')) \
         ON CONFLICT (guild_id, object_kind, object_id, direction) WHERE state = 'pending' DO UPDATE SET \
           due_at = now() + ($9::bigint * interval '1 millisecond'), \
           priority = LEAST(bridge_sync_ops.priority, EXCLUDED.priority), updated_at = now() \
         RETURNING (xmax = 0)",
    )
    .bind(op.id)
    .bind(op.guild_id)
    .bind(op.kind.as_str())
    .bind(op.object_id)
    .bind(op.direction.as_str())
    .bind(op.priority.as_i16())
    .bind(op.seed_run)
    .bind(op.step)
    .bind(op.priority.debounce_ms())
    .fetch_one(pool)
    .await?;
    Ok(fresh)
}

/// Take up to `limit` due ops of bridges whose sync runs ops, most urgent first, and mark them running. Safe with
/// several workers (`SKIP LOCKED`).
pub async fn claim_due(pool: &PgPool, limit: i64) -> BridgeResult<Vec<SyncOp>> {
    let sql = format!(
        "UPDATE bridge_sync_ops SET state = 'running', updated_at = now() WHERE id IN ( \
           SELECT o.id FROM bridge_sync_ops o JOIN bridges b ON b.guild_id = o.guild_id \
           WHERE o.state = 'pending' AND o.due_at <= now() AND b.sync_state IN ('seeding', 'live') \
           ORDER BY o.priority, o.due_at, o.id LIMIT $1 FOR UPDATE OF o SKIP LOCKED) \
         RETURNING {COLS}"
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(limit)
        .fetch_all(pool)
        .await?;
    let mut ops = rows.iter().map(op_from).collect::<BridgeResult<Vec<_>>>()?;
    ops.sort_by_key(|o| (o.priority, o.due_at_ms, o.id));
    Ok(ops)
}

pub async fn complete(pool: &PgPool, id: i64) -> BridgeResult<()> {
    sqlx::query("UPDATE bridge_sync_ops SET state = 'done', updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// A failed run: back to pending with backoff, or dead after [`SyncOp::MAX_ATTEMPTS`]. Returns the new state.
pub async fn fail(pool: &PgPool, id: i64, error: &str) -> BridgeResult<OpState> {
    let error: String = error.chars().take(500).collect();
    let attempts: i32 = sqlx::query_scalar(
        "UPDATE bridge_sync_ops SET attempts = attempts + 1, last_error = $2, updated_at = now() \
         WHERE id = $1 RETURNING attempts",
    )
    .bind(id)
    .bind(&error)
    .fetch_one(pool)
    .await?;
    if attempts >= SyncOp::MAX_ATTEMPTS {
        sqlx::query("UPDATE bridge_sync_ops SET state = 'dead', updated_at = now() WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
        return Ok(OpState::Dead);
    }
    let back = sqlx::query(
        "UPDATE bridge_sync_ops SET state = 'pending', due_at = now() + ($2::bigint * interval '1 millisecond') \
         WHERE id = $1",
    )
    .bind(id)
    .bind(SyncOp::backoff_ms(attempts - 1))
    .execute(pool)
    .await;
    match back {
        Ok(_) => Ok(OpState::Pending),
        // A newer pending row for the same object exists (the unique index): it covers this one.
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            complete(pool, id).await?;
            Ok(OpState::Done)
        }
        Err(e) => Err(e.into()),
    }
}

/// "Retry" on a dead op. Returns false when it is not dead (or another pending row already covers it).
pub async fn retry_dead(pool: &PgPool, guild: i64, id: i64) -> BridgeResult<bool> {
    let r = sqlx::query(
        "UPDATE bridge_sync_ops SET state = 'pending', attempts = 0, due_at = now(), last_error = NULL \
         WHERE id = $1 AND guild_id = $2 AND state = 'dead'",
    )
    .bind(id)
    .bind(guild)
    .execute(pool)
    .await;
    match r {
        Ok(r) => Ok(r.rows_affected() == 1),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// On start: ops left running by a crashed process become pending again.
pub async fn recover_running(pool: &PgPool) -> BridgeResult<u64> {
    let r = sqlx::query(
        "UPDATE bridge_sync_ops SET state = 'pending', due_at = now() WHERE state = 'running' \
         AND NOT EXISTS (SELECT 1 FROM bridge_sync_ops p WHERE p.state = 'pending' AND p.guild_id = bridge_sync_ops.guild_id \
           AND p.object_kind = bridge_sync_ops.object_kind AND p.object_id = bridge_sync_ops.object_id \
           AND p.direction = bridge_sync_ops.direction)",
    )
    .execute(pool)
    .await?;
    Ok(r.rows_affected())
}

/// Finished ops older than a day are dropped.
pub async fn prune_done(pool: &PgPool) -> BridgeResult<u64> {
    Ok(sqlx::query(
        "DELETE FROM bridge_sync_ops WHERE state = 'done' AND updated_at < now() - interval '1 day'",
    )
    .execute(pool)
    .await?
    .rows_affected())
}

/// Queue figures for the status page.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpCounts {
    pub pending: i64,
    pub running: i64,
    pub dead: i64,
}

pub async fn counts(pool: &PgPool, guild: i64) -> BridgeResult<OpCounts> {
    let row = sqlx::query(
        "SELECT count(*) FILTER (WHERE state = 'pending') AS pending, \
                count(*) FILTER (WHERE state = 'running') AS running, \
                count(*) FILTER (WHERE state = 'dead') AS dead \
         FROM bridge_sync_ops WHERE guild_id = $1",
    )
    .bind(guild)
    .fetch_one(pool)
    .await?;
    Ok(OpCounts {
        pending: row.try_get("pending")?,
        running: row.try_get("running")?,
        dead: row.try_get("dead")?,
    })
}

pub async fn list_dead(pool: &PgPool, guild: i64, limit: i64) -> BridgeResult<Vec<SyncOp>> {
    let sql = format!(
        "SELECT {COLS} FROM bridge_sync_ops WHERE guild_id = $1 AND state = 'dead' ORDER BY id DESC LIMIT $2"
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(guild)
        .bind(limit)
        .fetch_all(pool)
        .await?;
    rows.iter().map(op_from).collect()
}
