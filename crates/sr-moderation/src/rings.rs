//! Reports about status rings (PRF-RNG-001).
//!
//! A ring is reported like any other target (`target_kind = "status_ring"`). The reporter must be able to see the
//! ring right now (the audience rule `status_ring_visible_to`), so a ring that is hidden from them answers like
//! one that does not exist. Filing the report
//!
//! * snapshots what the ring says (kind, text, track, audience, times, file facts) into the case as private
//!   evidence, so the case can still act after the ring is gone;
//! * freezes the ring with a system hold (`HoldScope::StatusRing`): the `rings.expire` job and the author's
//!   delete leave row and object alone until the hold ends, and a photo or clip on the author's own mount is
//!   copied to the instance bucket by the job.
//!
//! Staff read the held file through [`Moderation::read_held_ring`], which writes the access-log row first.

use serde_json::{Value, json};
use sqlx::{PgConnection, Row};
use sr_admin::AccessKind;
use sr_core::PlatformPermissions as P;

use crate::error::{ModError, ModResult};
use crate::model::Who;
use crate::rules;
use crate::service::{Moderation, log_staff, require_staff};

/// The author of a ring `reporter` can see, or `None` when the ring is hidden from them or not there.
pub(crate) async fn visible_author(
    conn: &mut PgConnection,
    ring: i64,
    reporter: i64,
) -> ModResult<Option<i64>> {
    Ok(sqlx::query_scalar(
        "SELECT r.author_id FROM status_rings r WHERE r.id = $1 AND status_ring_visible_to(r.id, $2)",
    )
    .bind(ring)
    .bind(reporter)
    .fetch_optional(&mut *conn)
    .await?)
}

/// The evidence payload of a ring, taken at report time. It never carries the viewer list.
pub(crate) async fn ring_snapshot(conn: &mut PgConnection, ring: i64) -> ModResult<Value> {
    let r = sqlx::query(
        "SELECT r.author_id, r.kind, r.body, r.track, r.audience, r.content_type, r.bytes, r.duration_ms, \
                r.mount_id, (r.object_key IS NOT NULL) AS has_object, \
                (extract(epoch FROM r.created_at) * 1000)::bigint AS created_ms, \
                (extract(epoch FROM r.expires_at) * 1000)::bigint AS expires_ms \
         FROM status_rings r WHERE r.id = $1",
    )
    .bind(ring)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(ModError::NotFound("ring"))?;
    Ok(json!({
        "kind": "status_ring",
        "id": ring.to_string(),
        "author_id": r.try_get::<i64, _>("author_id")?.to_string(),
        "ring_kind": r.try_get::<String, _>("kind")?,
        "text": r.try_get::<String, _>("body")?,
        "track": r.try_get::<Option<Value>, _>("track")?,
        "audience": r.try_get::<String, _>("audience")?,
        "content_type": r.try_get::<Option<String>, _>("content_type")?,
        "bytes": r.try_get::<Option<i64>, _>("bytes")?,
        "duration_ms": r.try_get::<Option<i32>, _>("duration_ms")?,
        "stored_on": if r.try_get::<bool, _>("has_object")? {
            Some(if r.try_get::<Option<i64>, _>("mount_id")? == Some(1) { "instance_bucket" } else { "author_mount" })
        } else {
            None
        },
        "created_at_ms": r.try_get::<i64, _>("created_ms")?,
        "expires_at_ms": r.try_get::<i64, _>("expires_ms")?,
    }))
}

/// Where a held ring's file is, for the logged read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldRing {
    /// The storage mount; 1 for the instance bucket (also for an evidence copy).
    pub mount_id: i64,
    /// Mount-relative key.
    pub object_key: String,
    pub content_type: String,
}

impl Moderation {
    /// Staff read the file of a held ring (platform staff with `MANAGE_CASES`; a reason is required). The access
    /// log row is written before the pointer is handed out. `NotFound` when the ring is not under a hold, so a
    /// ring nobody reported stays unreadable to staff too: a hold is the only way in.
    pub async fn read_held_ring(
        &self,
        who: &Who<'_>,
        ring: i64,
        reason: &str,
    ) -> ModResult<HeldRing> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let reason = rules::required_text("reason", reason, rules::MAX_REASON_CHARS)?;
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT CASE WHEN evidence_key IS NOT NULL THEN 1 ELSE mount_id END AS mount_id, \
                    COALESCE(evidence_key, object_key) AS object_key, content_type, hold_id \
             FROM status_rings WHERE id = $1 AND hold_id IS NOT NULL AND object_key IS NOT NULL",
        )
        .bind(ring)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ModError::NotFound("ring"))?;
        log_staff(
            &mut tx,
            who,
            AccessKind::PrivateRead,
            "moderation.ring.read",
            ("status_ring", ring),
            &reason,
            json!({ "hold_id": row.try_get::<i64, _>("hold_id")?.to_string() }),
        )
        .await?;
        let out = HeldRing {
            mount_id: row.try_get("mount_id")?,
            object_key: row.try_get("object_key")?,
            content_type: row.try_get("content_type")?,
        };
        tx.commit().await?;
        Ok(out)
    }
}
