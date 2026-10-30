//! Status rings in the data-subject paths (PRF-RNG-001): the export section (`rings.json`) and the erasure step.
//! The rest of the feature is `sr_userfeat::rings`; this file only touches the tables of migration 1710.

use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};

use crate::DbResult;

/// The instance bucket (storage mount 1).
const INSTANCE_MOUNT: i64 = 1;

/// The export section: the member's active rings with their audience and viewer list, and the rings they
/// opened. Rings past their day are already gone, so they are not listed.
pub async fn export_for(pool: &PgPool, user: i64) -> DbResult<Value> {
    let mine: Value = sqlx::query_scalar(
        "SELECT COALESCE(jsonb_agg(t ORDER BY t.created_at), '[]'::jsonb) FROM ( \
           SELECT r.id::text AS id, r.kind, r.body, r.track, r.audience, r.content_type, r.bytes, r.duration_ms, \
                  r.created_at, r.expires_at, (r.hold_id IS NOT NULL) AS under_hold, \
                  (SELECT COALESCE(jsonb_agg(g.guild_id::text ORDER BY g.guild_id), '[]'::jsonb) \
                     FROM status_ring_guilds g WHERE g.ring_id = r.id) AS server_ids, \
                  (SELECT COALESCE(jsonb_agg(jsonb_build_object('user_id', v.viewer_id::text, 'viewed_at', v.viewed_at) \
                                             ORDER BY v.viewed_at), '[]'::jsonb) \
                     FROM status_ring_views v WHERE v.ring_id = r.id) AS viewers \
           FROM status_rings r WHERE r.author_id = $1 AND r.deleted_at IS NULL AND r.expires_at > now()) t",
    )
    .bind(user)
    .fetch_one(pool)
    .await?;
    let opened: Value = sqlx::query_scalar(
        "SELECT COALESCE(jsonb_agg(jsonb_build_object('ring_id', v.ring_id::text, 'viewed_at', v.viewed_at) \
                                   ORDER BY v.viewed_at), '[]'::jsonb) \
         FROM status_ring_views v JOIN status_rings r ON r.id = v.ring_id \
         WHERE v.viewer_id = $1 AND r.expires_at > now() AND r.deleted_at IS NULL",
    )
    .bind(user)
    .fetch_one(pool)
    .await?;
    Ok(json!({ "active_rings": mine, "rings_you_opened": opened }))
}

/// What erasure did to a member's rings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Erased {
    /// Rows deleted.
    pub rings: i64,
    /// Rows a moderation hold keeps: hidden at once, removed when the hold ends.
    pub held: i64,
}

/// Erase a member's rings inside the deletion transaction. Every ring that no hold freezes goes with its viewer
/// list and its server list; its objects are queued in `status_ring_purge` for the `rings.expire` job (the mount
/// of an erased member is gone by then, and such an object stays in the member's own bucket). The rings they
/// opened and their allowance rows are forgotten. A held ring is hidden at once and stays until its hold ends
/// (D-122); the request does not release it.
pub async fn erase_for(conn: &mut PgConnection, user: i64) -> DbResult<Erased> {
    let gone: Vec<(Option<i64>, Option<String>, Option<String>)> = sqlx::query_as(
        "DELETE FROM status_rings WHERE author_id = $1 AND hold_id IS NULL \
         RETURNING mount_id, object_key, evidence_key",
    )
    .bind(user)
    .fetch_all(&mut *conn)
    .await?;
    for (mount, key, evidence) in &gone {
        if let (Some(mount), Some(key)) = (mount, key) {
            sqlx::query("INSERT INTO status_ring_purge (mount_id, object_key) VALUES ($1, $2)")
                .bind(mount)
                .bind(key)
                .execute(&mut *conn)
                .await?;
        }
        if let Some(ev) = evidence {
            sqlx::query("INSERT INTO status_ring_purge (mount_id, object_key) VALUES ($1, $2)")
                .bind(INSTANCE_MOUNT)
                .bind(ev)
                .execute(&mut *conn)
                .await?;
        }
    }
    let held = sqlx::query(
        "UPDATE status_rings SET deleted_at = COALESCE(deleted_at, now()) WHERE author_id = $1",
    )
    .bind(user)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    sqlx::query("DELETE FROM status_ring_views WHERE viewer_id = $1")
        .bind(user)
        .execute(&mut *conn)
        .await?;
    sqlx::query("DELETE FROM status_ring_posts WHERE author_id = $1")
        .bind(user)
        .execute(&mut *conn)
        .await?;
    Ok(Erased {
        rings: i64::try_from(gone.len()).unwrap_or(i64::MAX),
        held: i64::try_from(held).unwrap_or(i64::MAX),
    })
}

/// Held rings of the account (rows a hold keeps through an erasure).
pub async fn held_rows(conn: &mut PgConnection, user: i64) -> DbResult<i64> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM status_rings WHERE author_id = $1 AND hold_id IS NOT NULL",
    )
    .bind(user)
    .fetch_one(&mut *conn)
    .await?)
}
