//! BYOS-LEG-001: the legal side of user-owned buckets. Delisting never touches the member's bucket (the platform
//! must not and cannot delete a member's property): it is a row of ours that every read consults.
//!
//! * [`delisted_keys`] answers `ScopeFacts::delisted` from the takedown register (items of kind `storage_object`
//!   whose state is `disabled`; a restored or withdrawn notice leaves them `restored`, which ends the delisting).
//! * [`delist_byte_matches`] is the indexer's hook (D-571): the same bytes under another name or in another bucket
//!   are delisted under the notice that took them down.
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

/// One listed object offered to the byte matcher (D-571, D-583): its mount-relative key, its size and the
/// fingerprints known for it. A takedown item matches on (tag, size) or on (sha256, size).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    pub key: String,
    pub tag: Option<String>,
    pub sha256: Option<String>,
    pub size: i64,
}

/// D-571 (migration 2450): which of the listed objects of `mount_id` carry the same bytes as an object a standing
/// takedown delisted (same `object_hash` and `object_size` on a `disabled` storage item, on any mount). The match is
/// on the entity tag, the cheap path; [`delist_hash_matches`] is the same on the bytes' sha256 (D-583). Each match
/// is recorded, in one transaction, as a new item of the SAME notice, already `disabled`, with `derived_from` the
/// item it matched; the mount's CDN path of the key is blocked under that notice and the notice's timeline says
/// so. Every reader of disabled items then delists it, and the notice's restore lifts it with the rest.
///
/// D-584: the derived item names the owner of the object as its subject (`subject_account_id`: the mount's owner,
/// on a server's bucket the uploader as P28-LEG-1 resolves it), so the notice's restore DM reaches them and
/// the removal DM can be sent (the item's `notified_at` says whether it was). It never names a Person: no strike.
/// `objects` are `(key, content tag, size)`. Returns the matched keys.
pub async fn delist_byte_matches(
    pool: &PgPool,
    ids: &(dyn Fn() -> i64 + Send + Sync),
    mount_id: i64,
    objects: &[(String, String, i64)],
) -> DbResult<Vec<String>> {
    let probes: Vec<Probe> = objects
        .iter()
        .map(|(k, t, s)| Probe {
            key: k.clone(),
            tag: Some(t.clone()),
            sha256: None,
            size: *s,
        })
        .collect();
    delist_probes(pool, ids, mount_id, &probes).await
}

/// D-583: [`delist_byte_matches`] for objects the indexer read: matched on the sha256 the upheld item recorded
/// (and, as before, on the tag when the probe carries one). Returns the matched keys.
pub async fn delist_hash_matches(
    pool: &PgPool,
    ids: &(dyn Fn() -> i64 + Send + Sync),
    mount_id: i64,
    objects: &[Probe],
) -> DbResult<Vec<String>> {
    delist_probes(pool, ids, mount_id, objects).await
}

/// The sieve (D-583): of `(key, tag, size)` candidates, the ones whose size equals a standing byte takedown's that
/// recorded a sha256, with the sha256 already stored for the object at this tag and size when there is one.
pub async fn hash_candidates(
    pool: &PgPool,
    mount_id: i64,
    objects: &[(String, Option<String>, i64)],
) -> DbResult<Vec<(String, Option<String>)>> {
    if objects.is_empty() {
        return Ok(Vec::new());
    }
    let keys: Vec<&str> = objects.iter().map(|o| o.0.as_str()).collect();
    let tags: Vec<Option<&str>> = objects.iter().map(|o| o.1.as_deref()).collect();
    let sizes: Vec<i64> = objects.iter().map(|o| o.2).collect();
    let rows = sqlx::query(
        "SELECT c.key,
                CASE WHEN c.tag IS NOT NULL AND o.content_tag = c.tag AND o.size_bytes = c.size
                     THEN o.content_sha256 END AS known
         FROM unnest($2::text[], $3::text[], $4::bigint[]) AS c(key, tag, size)
         LEFT JOIN storage_objects o ON o.mount_id = $1 AND o.key = c.key
         WHERE EXISTS (SELECT 1 FROM takedown_items i
                       WHERE i.kind = 'storage_object' AND i.state = 'disabled'
                         AND i.object_sha256 IS NOT NULL AND i.object_size = c.size)",
    )
    .bind(mount_id)
    .bind(&keys)
    .bind(&tags)
    .bind(&sizes)
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|r| Ok((r.try_get("key")?, r.try_get("known")?)))
        .collect()
}

/// Who answers for the object `key` of `mount_id`: the mount's owner; on a server's bucket the member whose latest
/// completed upload of that key the ledger knows (P28-LEG-1), else the server owner. `None` for the instance bucket.
pub async fn owner_of_object(
    conn: &mut PgConnection,
    mount_id: i64,
    key: &str,
) -> DbResult<Option<i64>> {
    let Some(r) = sqlx::query(
        "SELECT m.owner_kind, m.owner_id, g.owner_id AS guild_owner
         FROM storage_mounts m LEFT JOIN guilds g ON m.owner_kind = 'guild' AND g.id = m.owner_id
         WHERE m.id = $1 AND m.deleted_at IS NULL",
    )
    .bind(mount_id)
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };
    let kind: String = r.try_get("owner_kind")?;
    match kind.as_str() {
        "user" => Ok(Some(r.try_get("owner_id")?)),
        "guild" => {
            let uploader: Option<i64> = sqlx::query_scalar(
                "SELECT user_id FROM storage_mount_uploads
                 WHERE mount_id = $1 AND object_key = $2 AND state = 'completed'
                 ORDER BY updated_at DESC LIMIT 1",
            )
            .bind(mount_id)
            .bind(key)
            .fetch_optional(&mut *conn)
            .await?;
            match uploader {
                Some(u) => Ok(Some(u)),
                None => Ok(r.try_get("guild_owner")?),
            }
        }
        _ => Ok(None),
    }
}

async fn delist_probes(
    pool: &PgPool,
    ids: &(dyn Fn() -> i64 + Send + Sync),
    mount_id: i64,
    objects: &[Probe],
) -> DbResult<Vec<String>> {
    if objects.is_empty() {
        return Ok(Vec::new());
    }
    let keys: Vec<&str> = objects.iter().map(|o| o.key.as_str()).collect();
    let tags: Vec<Option<&str>> = objects.iter().map(|o| o.tag.as_deref()).collect();
    let shas: Vec<Option<&str>> = objects.iter().map(|o| o.sha256.as_deref()).collect();
    let sizes: Vec<i64> = objects.iter().map(|o| o.size).collect();
    let mut tx = pool.begin().await?;
    let rows = sqlx::query(
        "SELECT DISTINCT ON (c.key, i.notice_id) c.key, c.tag, c.sha, c.size, i.notice_id,
                COALESCE(i.derived_from, i.id) AS source, i.label AS source_label,
                i.object_sha256 AS source_sha
         FROM unnest($1::text[], $2::text[], $3::text[], $4::bigint[]) AS c(key, tag, sha, size)
         JOIN takedown_items i ON i.kind = 'storage_object' AND i.state = 'disabled' AND i.object_size = c.size
                              AND (i.object_hash = c.tag OR i.object_sha256 = c.sha)
         ORDER BY c.key, i.notice_id, COALESCE(i.derived_from, i.id)",
    )
    .bind(&keys)
    .bind(&tags)
    .bind(&shas)
    .bind(&sizes)
    .fetch_all(&mut *tx)
    .await?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let mount_label: String = sqlx::query_scalar("SELECT label FROM storage_mounts WHERE id = $1")
        .bind(mount_id)
        .fetch_optional(&mut *tx)
        .await?
        .unwrap_or_default();
    let mut matched: Vec<String> = Vec::new();
    for r in &rows {
        let key: String = r.try_get("key")?;
        let tag: Option<String> = r.try_get("tag")?;
        let sha: Option<String> = r.try_get("sha")?;
        let size: i64 = r.try_get("size")?;
        let notice: i64 = r.try_get("notice_id")?;
        let source: i64 = r.try_get("source")?;
        let source_label: String = r.try_get("source_label")?;
        let by = if sha.is_some() { "sha256" } else { "tag" };
        // Both fingerprints are of the same bytes: keep the sha256 the upheld item recorded.
        let source_sha: Option<String> = r.try_get("source_sha")?;
        let item_sha = sha.or(source_sha);
        let label: String =
            format!("Object {key} in bucket {mount_label} (the same bytes as: {source_label})")
                .chars()
                .take(300)
                .collect();
        let owner = owner_of_object(&mut tx, mount_id, &key).await?;
        let inserted = sqlx::query(
            "INSERT INTO takedown_items (id, notice_id, kind, target_id, label, object_key, state, disabled_at,
                                         object_hash, object_sha256, object_size, derived_from, subject_account_id)
             VALUES ($1, $2, 'storage_object', $3, $4, $5, 'disabled', now(), $6, $7, $8, $9, $10)
             ON CONFLICT DO NOTHING",
        )
        .bind(ids())
        .bind(notice)
        .bind(mount_id)
        .bind(&label)
        .bind(&key)
        .bind(&tag)
        .bind(&item_sha)
        .bind(size)
        .bind(source)
        .bind(owner)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if inserted > 0 {
            sqlx::query(
                "INSERT INTO blocked_objects (id, kind, value, notice_id) VALUES ($1, 'key', $2, $3)
                 ON CONFLICT DO NOTHING",
            )
            .bind(ids())
            .bind(format!("mount/{mount_id}/{key}"))
            .bind(notice)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT INTO takedown_events (id, notice_id, actor_id, kind, detail)
                 VALUES ($1, $2, NULL, 'byte_match', $3)",
            )
            .bind(ids())
            .bind(notice)
            .bind(json!({ "mount_id": mount_id.to_string(), "key": &key,
                          "derived_from": source.to_string(), "by": by }))
            .execute(&mut *tx)
            .await?;
        }
        if !matched.contains(&key) {
            matched.push(key);
        }
    }
    tx.commit().await?;
    Ok(matched)
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
