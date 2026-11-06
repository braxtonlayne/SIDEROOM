//! Postgres side of `sr_storage::shelf` (migration 2020, STU-ZK-001): the plaintext shelf catalog, account-bound
//! shelf grants with their hash-chained audit rows, and the staff delisting of one object with its strike.

use async_trait::async_trait;
use serde_json::json;
use sqlx::{PgConnection, PgPool, Row};
use sr_admin::{AccessKind, AuditEntry};
use sr_storage::scopes::FactsError;
use sr_storage::shelf::{
    CatalogEntry, GrantError, GrantRow, ShelfAccessStore, ShelfGrantFact, ShelfLabel,
};

use crate::DbResult;
use crate::index_repo::IdSource;

fn fe(e: sqlx::Error) -> FactsError {
    FactsError(e.to_string())
}

/// Postgres-backed [`ShelfAccessStore`].
#[derive(Clone)]
pub struct PgShelfAccess {
    pool: PgPool,
    ids: IdSource,
}

impl PgShelfAccess {
    /// Wrap a pool and an id source.
    pub fn new(pool: PgPool, ids: IdSource) -> Self {
        Self { pool, ids }
    }
}

const CATALOG_COLS: &str = "shelf_id, object_hash, path, size, kind, class, owner_title, owner_cover, owner_note, \
     listed, delisted_at IS NOT NULL AS delisted, (extract(epoch FROM updated_at) * 1000)::bigint AS updated_ms";

fn entry(r: &sqlx::postgres::PgRow) -> Result<CatalogEntry, FactsError> {
    Ok(CatalogEntry {
        shelf_id: r.try_get("shelf_id").map_err(fe)?,
        object_hash: r.try_get("object_hash").map_err(fe)?,
        path: r.try_get("path").map_err(fe)?,
        size: r.try_get("size").map_err(fe)?,
        kind: r.try_get("kind").map_err(fe)?,
        class: r.try_get("class").map_err(fe)?,
        owner_title: r.try_get("owner_title").map_err(fe)?,
        owner_cover: r.try_get("owner_cover").map_err(fe)?,
        owner_note: r.try_get("owner_note").map_err(fe)?,
        listed: r.try_get("listed").map_err(fe)?,
        delisted: r.try_get("delisted").map_err(fe)?,
        updated_ms: r.try_get("updated_ms").map_err(fe)?,
    })
}

const GRANT_COLS: &str = "id, shelf_id, grantee_kind, grantee_id, guild_id, granted_by, \
     (extract(epoch FROM created_at) * 1000)::bigint AS created_ms, \
     (extract(epoch FROM revoked_at) * 1000)::bigint AS revoked_ms";

fn grant(r: &sqlx::postgres::PgRow) -> Result<GrantRow, FactsError> {
    Ok(GrantRow {
        id: r.try_get("id").map_err(fe)?,
        shelf_id: r.try_get("shelf_id").map_err(fe)?,
        grantee_kind: r.try_get("grantee_kind").map_err(fe)?,
        grantee_id: r.try_get("grantee_id").map_err(fe)?,
        guild_id: r.try_get("guild_id").map_err(fe)?,
        granted_by: r.try_get("granted_by").map_err(fe)?,
        created_ms: r.try_get("created_ms").map_err(fe)?,
        revoked_ms: r.try_get("revoked_ms").map_err(fe)?,
    })
}

async fn audit(
    conn: &mut PgConnection,
    actor: i64,
    action: &str,
    shelf_id: i64,
    detail: serde_json::Value,
) -> Result<(), sqlx::Error> {
    crate::admin_repo::append_in(
        conn,
        &AuditEntry {
            actor_id: actor,
            kind: AccessKind::AdminAction,
            action: action.to_owned(),
            target_kind: Some("shelf".to_owned()),
            target_id: Some(shelf_id),
            reason: String::new(),
            ip: None,
            detail: detail.to_string(),
        },
    )
    .await
    .map(|_| ())
    .map_err(|e| sqlx::Error::Protocol(e.to_string()))
}

#[async_trait]
impl ShelfAccessStore for PgShelfAccess {
    async fn candidate_shelves(&self, viewer: i64) -> Result<Vec<i64>, FactsError> {
        sqlx::query_scalar(
            "SELECT m.id FROM storage_mounts m WHERE m.deleted_at IS NULL AND ( \
               (m.owner_kind = 'user' AND m.owner_id = $1) \
               OR EXISTS (SELECT 1 FROM storage_mount_shares s WHERE s.mount_id = m.id AND s.friend_id = $1) \
               OR EXISTS (SELECT 1 FROM shelf_grants g WHERE g.shelf_id = m.id AND g.revoked_at IS NULL AND ( \
                    (g.grantee_kind = 'friend' AND g.grantee_id = $1) \
                    OR (g.grantee_kind = 'guild_role' AND EXISTS ( \
                        SELECT 1 FROM guild_members gm WHERE gm.guild_id = g.guild_id AND gm.user_id = $1)))) \
               OR (m.owner_kind = 'guild' AND EXISTS ( \
                    SELECT 1 FROM guild_members gm WHERE gm.guild_id = m.owner_id AND gm.user_id = $1)) \
               OR EXISTS (SELECT 1 FROM storage_mount_attachments a JOIN guild_members gm ON gm.guild_id = a.guild_id \
                    WHERE a.mount_id = m.id AND gm.user_id = $1)) \
             ORDER BY m.id",
        )
        .bind(viewer)
        .fetch_all(&self.pool)
        .await
        .map_err(fe)
    }

    async fn labels(&self, shelf_ids: &[i64]) -> Result<Vec<ShelfLabel>, FactsError> {
        let rows = sqlx::query(
            "SELECT m.id, m.label, m.owner_kind, m.owner_id, a.username::text AS handle, \
                    (SELECT count(*) FROM shelf_objects o WHERE o.shelf_id = m.id AND o.listed \
                       AND o.delisted_at IS NULL) AS n \
             FROM storage_mounts m LEFT JOIN accounts a ON m.owner_kind = 'user' AND a.id = m.owner_id \
             WHERE m.id = ANY($1) ORDER BY m.id",
        )
        .bind(shelf_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(fe)?;
        rows.iter()
            .map(|r| {
                Ok(ShelfLabel {
                    shelf_id: r.try_get("id").map_err(fe)?,
                    label: r.try_get("label").map_err(fe)?,
                    owner_kind: r.try_get("owner_kind").map_err(fe)?,
                    owner_id: r.try_get("owner_id").map_err(fe)?,
                    owner_handle: r.try_get("handle").map_err(fe)?,
                    object_count: r.try_get("n").map_err(fe)?,
                })
            })
            .collect()
    }

    async fn guild_names(&self, guild_ids: &[i64]) -> Result<Vec<(i64, String)>, FactsError> {
        sqlx::query_as("SELECT id, name FROM guilds WHERE id = ANY($1)")
            .bind(guild_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(fe)
    }

    async fn catalog_page(
        &self,
        shelf_id: i64,
        after: Option<&str>,
        name: Option<&str>,
        limit: i64,
    ) -> Result<Vec<CatalogEntry>, FactsError> {
        // A substring pattern with the LIKE wildcards made literal.
        let pattern = name.map(|n| {
            let mut p = String::with_capacity(n.len() + 2);
            p.push('%');
            for c in n.chars() {
                if matches!(c, '\\' | '%' | '_') {
                    p.push('\\');
                }
                p.push(c);
            }
            p.push('%');
            p
        });
        // Listed, not delisted by staff, and not delisted by an upheld takedown item either.
        let sql = format!(
            "SELECT {CATALOG_COLS} FROM shelf_objects o \
             WHERE o.shelf_id = $1 AND o.listed AND o.delisted_at IS NULL AND o.object_hash > $2 \
               AND ($4::text IS NULL OR o.path ILIKE $4 OR o.owner_title ILIKE $4) \
               AND NOT EXISTS (SELECT 1 FROM takedown_items t WHERE t.kind = 'storage_object' \
                   AND t.state = 'disabled' AND t.target_id = o.shelf_id AND t.object_key = o.path) \
             ORDER BY o.object_hash LIMIT $3"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(shelf_id)
            .bind(after.unwrap_or(""))
            .bind(limit)
            .bind(pattern)
            .fetch_all(&self.pool)
            .await
            .map_err(fe)?;
        rows.iter().map(entry).collect()
    }

    async fn catalog_object(
        &self,
        shelf_id: i64,
        object_hash: &str,
    ) -> Result<Option<CatalogEntry>, FactsError> {
        let sql = format!(
            "SELECT {CATALOG_COLS} FROM shelf_objects WHERE shelf_id = $1 AND object_hash = $2"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(shelf_id)
            .bind(object_hash)
            .fetch_optional(&self.pool)
            .await
            .map_err(fe)?
            .as_ref()
            .map(entry)
            .transpose()
    }

    async fn register_delisted(&self, shelf_id: i64, path: &str) -> Result<bool, FactsError> {
        Ok(
            !crate::storage_legal::delisted_keys(&self.pool, shelf_id, &[path.to_owned()])
                .await
                .map_err(|e| FactsError(e.to_string()))?
                .is_empty(),
        )
    }

    async fn set_owner_meta(
        &self,
        shelf_id: i64,
        object_hash: &str,
        title: Option<&str>,
        cover: Option<&str>,
        note: Option<&str>,
    ) -> Result<bool, FactsError> {
        let r = sqlx::query(
            "UPDATE shelf_objects SET owner_title = COALESCE($3, owner_title), \
               owner_cover = COALESCE($4, owner_cover), owner_note = COALESCE($5, owner_note), \
               updated_at = now() WHERE shelf_id = $1 AND object_hash = $2 AND listed",
        )
        .bind(shelf_id)
        .bind(object_hash)
        .bind(title)
        .bind(cover)
        .bind(note)
        .execute(&self.pool)
        .await
        .map_err(fe)?;
        Ok(r.rows_affected() > 0)
    }

    async fn grants_of(&self, shelf_id: i64) -> Result<Vec<GrantRow>, FactsError> {
        let sql = format!(
            "SELECT {GRANT_COLS} FROM shelf_grants WHERE shelf_id = $1 ORDER BY created_at DESC, id DESC"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(shelf_id)
            .fetch_all(&self.pool)
            .await
            .map_err(fe)?;
        rows.iter().map(grant).collect()
    }

    async fn create_grant(&self, g: &GrantRow) -> Result<GrantRow, GrantError> {
        let missing = |_: sqlx::Error| GrantError::Missing;
        let mut tx = self.pool.begin().await.map_err(missing)?;
        let id = (self.ids)();
        let sql = format!(
            "INSERT INTO shelf_grants (id, shelf_id, grantee_kind, grantee_id, guild_id, granted_by) \
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING {GRANT_COLS}"
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .bind(g.shelf_id)
            .bind(&g.grantee_kind)
            .bind(g.grantee_id)
            .bind(g.guild_id)
            .bind(g.granted_by)
            .fetch_one(&mut *tx)
            .await
            .map_err(
                |e| match e.as_database_error().and_then(|d| d.code()).as_deref() {
                    Some("23505") => GrantError::Duplicate,
                    _ => GrantError::Missing,
                },
            )?;
        let out = grant(&row).map_err(|_| GrantError::Missing)?;
        audit(
            &mut tx,
            g.granted_by,
            "shelf.grant.create",
            g.shelf_id,
            json!({ "grant": id.to_string(), "kind": g.grantee_kind,
                    "grantee": g.grantee_id.to_string(),
                    "guild": g.guild_id.map(|x| x.to_string()) }),
        )
        .await
        .map_err(missing)?;
        tx.commit().await.map_err(missing)?;
        Ok(out)
    }

    async fn revoke_grant(
        &self,
        shelf_id: i64,
        grant_id: i64,
        by: i64,
    ) -> Result<bool, FactsError> {
        let mut tx = self.pool.begin().await.map_err(fe)?;
        let hit = sqlx::query(
            "UPDATE shelf_grants SET revoked_at = now(), revoked_by = $3 \
             WHERE id = $1 AND shelf_id = $2 AND revoked_at IS NULL RETURNING grantee_kind, grantee_id",
        )
        .bind(grant_id)
        .bind(shelf_id)
        .bind(by)
        .fetch_optional(&mut *tx)
        .await
        .map_err(fe)?;
        let Some(hit) = hit else {
            return Ok(false);
        };
        let kind: String = hit.try_get("grantee_kind").map_err(fe)?;
        let grantee: i64 = hit.try_get("grantee_id").map_err(fe)?;
        audit(
            &mut tx,
            by,
            "shelf.grant.revoke",
            shelf_id,
            json!({ "grant": grant_id.to_string(), "kind": kind, "grantee": grantee.to_string() }),
        )
        .await
        .map_err(fe)?;
        tx.commit().await.map_err(fe)?;
        Ok(true)
    }

    async fn account_active(&self, id: i64) -> Result<bool, FactsError> {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM accounts WHERE id = $1 AND state = 'active')",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(fe)
    }
}

/// The live whole-shelf grants of a mount, as the access decision reads them.
pub async fn live_grants(pool: &PgPool, mount_id: i64) -> Result<Vec<ShelfGrantFact>, FactsError> {
    let rows = sqlx::query(
        "SELECT grantee_kind, grantee_id, guild_id FROM shelf_grants \
         WHERE shelf_id = $1 AND revoked_at IS NULL",
    )
    .bind(mount_id)
    .fetch_all(pool)
    .await
    .map_err(fe)?;
    rows.iter()
        .map(|r| {
            let kind: String = r.try_get("grantee_kind").map_err(fe)?;
            Ok(ShelfGrantFact {
                guild_role: kind == "guild_role",
                grantee_id: r.try_get("grantee_id").map_err(fe)?,
                guild_id: r.try_get("guild_id").map_err(fe)?,
            })
        })
        .collect()
}

/// Object paths of a mount that staff delisted one by one (`shelf_objects.delisted_at`).
pub async fn delisted_paths(
    pool: &PgPool,
    mount_id: i64,
    paths: &[String],
) -> Result<Vec<String>, FactsError> {
    sqlx::query_scalar(
        "SELECT path FROM shelf_objects WHERE shelf_id = $1 AND path = ANY($2) AND delisted_at IS NOT NULL",
    )
    .bind(mount_id)
    .bind(paths)
    .fetch_all(pool)
    .await
    .map_err(fe)
}

/// A staff delisting of one object, with its strike.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delisted {
    /// The strike row.
    pub strike_id: i64,
    /// The shelf's owner account, when a member owns it.
    pub subject_account: Option<i64>,
    /// The object's path.
    pub path: String,
}

/// Delist `object_hash` of `shelf_id` and log the strike, on the caller's transaction. `None` when there is
/// no such object or it is already delisted. The bucket is untouched.
pub async fn delist_object(
    conn: &mut PgConnection,
    strike_id: i64,
    shelf_id: i64,
    object_hash: &str,
    notice_id: Option<i64>,
    reason: &str,
    staff: i64,
) -> DbResult<Option<Delisted>> {
    let row = sqlx::query(
        "SELECT o.path, CASE WHEN m.owner_kind = 'user' THEN m.owner_id END AS subject \
         FROM shelf_objects o JOIN storage_mounts m ON m.id = o.shelf_id \
         WHERE o.shelf_id = $1 AND o.object_hash = $2 AND o.delisted_at IS NULL FOR UPDATE OF o",
    )
    .bind(shelf_id)
    .bind(object_hash)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(row) = row else { return Ok(None) };
    let path: String = row.try_get("path")?;
    let subject: Option<i64> = row.try_get("subject")?;
    sqlx::query(
        "INSERT INTO shelf_delist_strikes (id, shelf_id, object_hash, path, subject_account, notice_id, reason, delisted_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(strike_id)
    .bind(shelf_id)
    .bind(object_hash)
    .bind(&path)
    .bind(subject)
    .bind(notice_id)
    .bind(reason)
    .bind(staff)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE shelf_objects SET delisted_at = now(), delist_strike_id = $3, updated_at = now() \
         WHERE shelf_id = $1 AND object_hash = $2",
    )
    .bind(shelf_id)
    .bind(object_hash)
    .bind(strike_id)
    .execute(&mut *conn)
    .await?;
    Ok(Some(Delisted {
        strike_id,
        subject_account: subject,
        path,
    }))
}

/// Put a delisted object back (the strike row stays, marked restored). `None` when it is not delisted.
pub async fn relist_object(
    conn: &mut PgConnection,
    shelf_id: i64,
    object_hash: &str,
    staff: i64,
) -> DbResult<Option<Delisted>> {
    let row = sqlx::query(
        "SELECT path, delist_strike_id FROM shelf_objects \
         WHERE shelf_id = $1 AND object_hash = $2 AND delisted_at IS NOT NULL FOR UPDATE",
    )
    .bind(shelf_id)
    .bind(object_hash)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(row) = row else { return Ok(None) };
    let path: String = row.try_get("path")?;
    let strike: i64 = row.try_get("delist_strike_id")?;
    sqlx::query(
        "UPDATE shelf_objects SET delisted_at = NULL, delist_strike_id = NULL, updated_at = now() \
         WHERE shelf_id = $1 AND object_hash = $2",
    )
    .bind(shelf_id)
    .bind(object_hash)
    .execute(&mut *conn)
    .await?;
    let subject: Option<i64> = sqlx::query_scalar(
        "UPDATE shelf_delist_strikes SET restored_at = now(), restored_by = $2 WHERE id = $1 RETURNING subject_account",
    )
    .bind(strike)
    .bind(staff)
    .fetch_one(&mut *conn)
    .await?;
    Ok(Some(Delisted {
        strike_id: strike,
        subject_account: subject,
        path,
    }))
}
