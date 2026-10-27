//! BYOS-LEG-001: the legal side of user-owned buckets. Delisting never touches the member's bucket (the platform
//! must not and cannot delete a member's property): it is a row of ours that every read consults.
//!
//! * [`delisted_keys`] answers `ScopeFacts::delisted` from the takedown register (items of kind `storage_object`
//!   whose state is `disabled`; a restored or withdrawn notice leaves them `restored`, which ends the delisting).
//! * [`delist_mount`] / [`relist_mount`] switch a whole mount to the status `delisted` and back.
//! * [`admin_list`] is the Admin > Storage table: every mount, its owner and status. It never carries the endpoint
//!   credentials, only the key hint.
//! * [`export_for`] is the data-subject export: labels, buckets and counts, no object keys, and the member's shares.
//! * [`erase_for`] is the erasure: the mounts are soft-deleted, their sealed secrets wiped, their shares removed.

use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool, Row};

use crate::DbResult;

/// Which of `keys` are delisted on `mount_id`.
pub async fn delisted_keys(pool: &PgPool, mount_id: i64, keys: &[String]) -> DbResult<Vec<String>> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    Ok(sqlx::query_scalar(
        "SELECT DISTINCT object_key FROM takedown_items
         WHERE kind = 'storage_object' AND state = 'disabled' AND target_id = $1 AND object_key = ANY($2)",
    )
    .bind(mount_id)
    .bind(keys)
    .fetch_all(pool)
    .await?)
}

/// A mount as the legal paths see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountLegal {
    pub id: i64,
    pub owner_kind: String,
    pub owner_id: i64,
    pub label: String,
    pub status: String,
    pub delist_reason: String,
}

fn legal_from(r: &sqlx::postgres::PgRow) -> DbResult<MountLegal> {
    Ok(MountLegal {
        id: r.try_get("id")?,
        owner_kind: r.try_get("owner_kind")?,
        owner_id: r.try_get("owner_id")?,
        label: r.try_get("label")?,
        status: r.try_get("status")?,
        delist_reason: r.try_get("delist_reason")?,
    })
}

const LEGAL_COLS: &str = "id, owner_kind, owner_id, label, status, delist_reason";

/// Delist a whole mount. `None` when there is no such live mount or it is the instance bucket (mount 1 is the
/// platform's own; staff disable it instead). Delisting twice keeps the first prior status and updates the reason.
pub async fn delist_mount(
    conn: &mut PgConnection,
    mount_id: i64,
    actor: i64,
    reason: &str,
) -> DbResult<Option<MountLegal>> {
    let sql = format!(
        "UPDATE storage_mounts SET
            delist_prior = CASE WHEN status = 'delisted' THEN delist_prior ELSE status END,
            status = 'delisted', delisted_at = COALESCE(delisted_at, now()), delisted_by = $2,
            delist_reason = $3, updated_at = now()
         WHERE id = $1 AND deleted_at IS NULL AND owner_kind <> 'platform'
         RETURNING {LEGAL_COLS}"
    );
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(mount_id)
        .bind(actor)
        .bind(reason)
        .fetch_optional(&mut *conn)
        .await?
        .as_ref()
        .map(legal_from)
        .transpose()
}

/// Put a delisted mount back to the status it had. `None` when it is not delisted.
pub async fn relist_mount(conn: &mut PgConnection, mount_id: i64) -> DbResult<Option<MountLegal>> {
    let sql = format!(
        "UPDATE storage_mounts SET
            status = COALESCE(delist_prior, 'active'), delist_prior = NULL, delisted_at = NULL,
            delisted_by = NULL, delist_reason = '', updated_at = now()
         WHERE id = $1 AND deleted_at IS NULL AND status = 'delisted'
         RETURNING {LEGAL_COLS}"
    );
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(mount_id)
        .fetch_optional(&mut *conn)
        .await?
        .as_ref()
        .map(legal_from)
        .transpose()
}

/// One row of Admin > Storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminMountRow {
    pub id: i64,
    pub owner_kind: String,
    pub owner_id: i64,
    /// The owner's username (the owning guild's name for a guild mount; empty for the platform).
    pub owner_name: String,
    pub label: String,
    pub provider: String,
    pub bucket: String,
    pub status: String,
    pub key_hint: String,
    pub created_at_ms: i64,
    pub delisted_at_ms: Option<i64>,
    pub delist_reason: String,
    /// Objects of this mount with a standing delisting.
    pub delisted_objects: i64,
}

/// Every live mount, newest first, optionally one owner's or one status. `limit` is clamped to 1..=500.
pub async fn admin_list(
    pool: &PgPool,
    owner: Option<(&str, i64)>,
    status: Option<&str>,
    limit: i64,
    before: Option<i64>,
) -> DbResult<Vec<AdminMountRow>> {
    let rows = sqlx::query(
        "SELECT m.id, m.owner_kind, m.owner_id, m.label, m.provider, m.bucket, m.status, m.key_hint,
                COALESCE(a.username::text, g.name, '') AS owner_name,
                (extract(epoch FROM m.created_at) * 1000)::bigint AS created_ms,
                (extract(epoch FROM m.delisted_at) * 1000)::bigint AS delisted_ms,
                m.delist_reason,
                (SELECT count(DISTINCT i.object_key) FROM takedown_items i
                  WHERE i.kind = 'storage_object' AND i.state = 'disabled' AND i.target_id = m.id) AS delisted_objects
         FROM storage_mounts m
         LEFT JOIN accounts a ON m.owner_kind = 'user' AND a.id = m.owner_id
         LEFT JOIN guilds g ON m.owner_kind = 'guild' AND g.id = m.owner_id
         WHERE m.deleted_at IS NULL
           AND ($1::text IS NULL OR (m.owner_kind = $1 AND m.owner_id = $2))
           AND ($3::text IS NULL OR m.status = $3)
           AND ($4::bigint IS NULL OR m.id < $4)
         ORDER BY m.id DESC LIMIT $5",
    )
    .bind(owner.map(|o| o.0))
    .bind(owner.map_or(0, |o| o.1))
    .bind(status)
    .bind(before)
    .bind(limit.clamp(1, 500))
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|r| {
            Ok(AdminMountRow {
                id: r.try_get("id")?,
                owner_kind: r.try_get("owner_kind")?,
                owner_id: r.try_get("owner_id")?,
                owner_name: r.try_get("owner_name")?,
                label: r.try_get("label")?,
                provider: r.try_get("provider")?,
                bucket: r.try_get("bucket")?,
                status: r.try_get("status")?,
                key_hint: r.try_get("key_hint")?,
                created_at_ms: r.try_get("created_ms")?,
                delisted_at_ms: r.try_get("delisted_ms")?,
                delist_reason: r.try_get("delist_reason")?,
                delisted_objects: r.try_get("delisted_objects")?,
            })
        })
        .collect()
}

/// The data-subject export section (`storage.json`): the member's own mounts (label, provider, bucket, prefix,
/// status, the shelves they shared and the number of delisted objects; no endpoint secrets and no object keys)
/// and the shelves friends have shared with them.
pub async fn export_for(pool: &PgPool, user: i64) -> DbResult<Value> {
    let mounts: Value = sqlx::query_scalar(
        "SELECT COALESCE(jsonb_agg(t ORDER BY t.id), '[]'::jsonb) FROM (
            SELECT m.id::text AS id, m.label, m.provider, m.bucket, m.prefix, m.status, m.created_at, m.delisted_at,
                (SELECT count(DISTINCT i.object_key) FROM takedown_items i
                  WHERE i.kind = 'storage_object' AND i.state = 'disabled' AND i.target_id = m.id) AS delisted_objects,
                (SELECT COALESCE(jsonb_agg(jsonb_build_object('friend_id', s.friend_id::text, 'cinema', s.cinema,
                        'library', s.library, 'music', s.music, 'since', s.created_at) ORDER BY s.friend_id), '[]'::jsonb)
                   FROM storage_mount_shares s WHERE s.mount_id = m.id) AS shares
            FROM storage_mounts m WHERE m.owner_kind = 'user' AND m.owner_id = $1 AND m.deleted_at IS NULL) t",
    )
    .bind(user)
    .fetch_one(pool)
    .await?;
    let received: Value = sqlx::query_scalar(
        "SELECT COALESCE(jsonb_agg(t ORDER BY t.mount_id), '[]'::jsonb) FROM (
            SELECT s.mount_id::text AS mount_id, m.label, s.cinema, s.library, s.music, s.created_at AS since
            FROM storage_mount_shares s JOIN storage_mounts m ON m.id = s.mount_id
            WHERE s.friend_id = $1 AND m.deleted_at IS NULL) t",
    )
    .bind(user)
    .fetch_one(pool)
    .await?;
    Ok(json!({ "mounts": mounts, "shared_with_you": received }))
}

/// What erasure did to a member's mounts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Erased {
    pub mounts: i64,
    pub shares: i64,
}

/// Erase a member's mounts inside the deletion transaction: soft-delete, wipe the sealed secret and the key id
/// (the associated data binds a blob to its row, so an empty blob can never be opened), and remove the shares
/// they gave or received. The objects stay in the member's bucket: it is theirs.
pub async fn erase_for(conn: &mut PgConnection, user: i64) -> DbResult<Erased> {
    let shares = sqlx::query(
        "DELETE FROM storage_mount_shares s USING storage_mounts m
         WHERE m.id = s.mount_id AND (s.friend_id = $1 OR (m.owner_kind = 'user' AND m.owner_id = $1))",
    )
    .bind(user)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    let mounts = sqlx::query(
        "UPDATE storage_mounts SET secret_key_enc = '\\x'::bytea, access_key_id = 'erased', key_hint = '',
            status = 'disabled', delist_prior = NULL, cors_ok_at = NULL, last_test = NULL,
            deleted_at = COALESCE(deleted_at, now()), updated_at = now()
         WHERE owner_kind = 'user' AND owner_id = $1 AND (deleted_at IS NULL OR access_key_id <> 'erased')",
    )
    .bind(user)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    Ok(Erased {
        mounts: i64::try_from(mounts).unwrap_or(i64::MAX),
        shares: i64::try_from(shares).unwrap_or(i64::MAX),
    })
}
