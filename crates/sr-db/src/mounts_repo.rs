//! Postgres implementation of `sr_storage::mounts::{MountRepo, MountResolver}` (BYOS-MNT-001, migration 1400).
//!
//! The secret column holds the sealed blob only. Every read returns it still sealed; opening needs the
//! server's `SecretBox`. The per-owner cap is checked under a transaction-scoped advisory lock so two
//! concurrent creates cannot both pass it.

use async_trait::async_trait;
use sqlx::{PgPool, Row, postgres::PgRow};
use sr_storage::mounts::{
    Mount, MountError, MountOwner, MountPatch, MountRepo, MountResolver, MountResult, MountStatus,
    NewMount, Provider, SealedSecret,
};

const COLS: &str =
    "id, owner_kind, owner_id, label, provider, endpoint, region, bucket, prefix, path_style,
    access_key_id, secret_key_enc, key_hint, status,
    (extract(epoch FROM cors_ok_at) * 1000)::bigint AS cors_ok_ms,
    (extract(epoch FROM last_test_at) * 1000)::bigint AS last_test_ms,
    last_test::text AS last_test_text,
    (extract(epoch FROM created_at) * 1000)::bigint AS created_ms,
    (extract(epoch FROM updated_at) * 1000)::bigint AS updated_ms";

/// Postgres-backed mounts.
#[derive(Clone)]
pub struct PgMountRepo {
    pool: PgPool,
}

impl PgMountRepo {
    /// Wrap a pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn repo_err(e: sqlx::Error) -> MountError {
    MountError::Repo(e.to_string())
}

fn row_to_mount(r: &PgRow) -> MountResult<Mount> {
    let get = |e: sqlx::Error| MountError::Repo(e.to_string());
    let kind: String = r.try_get("owner_kind").map_err(get)?;
    let owner = MountOwner::from_parts(&kind, r.try_get("owner_id").map_err(get)?)
        .ok_or_else(|| MountError::Repo(format!("unknown owner_kind {kind}")))?;
    let provider: String = r.try_get("provider").map_err(get)?;
    let status: String = r.try_get("status").map_err(get)?;
    Ok(Mount {
        id: r.try_get("id").map_err(get)?,
        owner,
        label: r.try_get("label").map_err(get)?,
        provider: Provider::parse(&provider)
            .ok_or_else(|| MountError::Repo(format!("unknown provider {provider}")))?,
        endpoint: r.try_get("endpoint").map_err(get)?,
        region: r.try_get("region").map_err(get)?,
        bucket: r.try_get("bucket").map_err(get)?,
        prefix: r.try_get("prefix").map_err(get)?,
        path_style: r.try_get("path_style").map_err(get)?,
        access_key_id: r.try_get("access_key_id").map_err(get)?,
        secret_key_enc: SealedSecret::from_bytes(r.try_get("secret_key_enc").map_err(get)?),
        key_hint: r.try_get("key_hint").map_err(get)?,
        status: MountStatus::parse(&status)
            .ok_or_else(|| MountError::Repo(format!("unknown status {status}")))?,
        cors_ok_at: r.try_get("cors_ok_ms").map_err(get)?,
        last_test_at: r.try_get("last_test_ms").map_err(get)?,
        last_test: r.try_get("last_test_text").map_err(get)?,
        created_at: r.try_get("created_ms").map_err(get)?,
        updated_at: r.try_get("updated_ms").map_err(get)?,
    })
}

#[async_trait]
impl MountResolver for PgMountRepo {
    async fn by_id(&self, id: i64) -> MountResult<Option<Mount>> {
        let sql = format!("SELECT {COLS} FROM storage_mounts WHERE id = $1 AND deleted_at IS NULL");
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(repo_err)?;
        row.as_ref().map(row_to_mount).transpose()
    }

    async fn for_owner(&self, owner: MountOwner) -> MountResult<Vec<Mount>> {
        let sql = format!(
            "SELECT {COLS} FROM storage_mounts
             WHERE owner_kind = $1 AND owner_id = $2 AND deleted_at IS NULL ORDER BY created_at, id"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(owner.kind())
            .bind(owner.id())
            .fetch_all(&self.pool)
            .await
            .map_err(repo_err)?;
        rows.iter().map(row_to_mount).collect()
    }
}

/// Map a unique violation to the typed error by constraint name.
fn insert_err(e: sqlx::Error) -> MountError {
    if let Some(db) = e.as_database_error()
        && db.code().as_deref() == Some("23505")
    {
        return match db.constraint() {
            Some("storage_mounts_pkey") => MountError::IdTaken,
            _ => MountError::LabelTaken,
        };
    }
    repo_err(e)
}

#[async_trait]
impl MountRepo for PgMountRepo {
    async fn create(&self, new: NewMount, max_per_owner: Option<u32>) -> MountResult<Mount> {
        let i = &new.input;
        let mut tx = self.pool.begin().await.map_err(repo_err)?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('storage_mounts:' || $1 || ':' || $2::text, 0))")
            .bind(i.owner.kind())
            .bind(i.owner.id())
            .execute(&mut *tx)
            .await
            .map_err(repo_err)?;
        if let Some(max) = max_per_owner {
            let live: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM storage_mounts
                 WHERE owner_kind = $1 AND owner_id = $2 AND deleted_at IS NULL",
            )
            .bind(i.owner.kind())
            .bind(i.owner.id())
            .fetch_one(&mut *tx)
            .await
            .map_err(repo_err)?;
            if live >= i64::from(max) {
                return Err(MountError::CapReached(max));
            }
        }
        sqlx::query(
            "INSERT INTO storage_mounts
               (id, owner_kind, owner_id, label, provider, endpoint, region, bucket, prefix, path_style,
                access_key_id, secret_key_enc, key_hint)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(new.id)
        .bind(i.owner.kind())
        .bind(i.owner.id())
        .bind(&i.label)
        .bind(i.provider.as_str())
        .bind(&i.endpoint)
        .bind(&i.region)
        .bind(&i.bucket)
        .bind(&i.prefix)
        .bind(i.path_style)
        .bind(&i.access_key_id)
        .bind(new.secret_key_enc.as_bytes())
        .bind(&new.key_hint)
        .execute(&mut *tx)
        .await
        .map_err(insert_err)?;
        tx.commit().await.map_err(repo_err)?;
        self.by_id(new.id)
            .await?
            .ok_or_else(|| MountError::Repo("inserted mount vanished".into()))
    }

    async fn update(&self, id: i64, patch: MountPatch) -> MountResult<Option<Mount>> {
        let (key_id, enc, hint) = match patch.credentials {
            Some((k, e, h)) => (Some(k), Some(e.as_bytes().to_vec()), Some(h)),
            None => (None, None, None),
        };
        let res = sqlx::query(
            "UPDATE storage_mounts SET
                label = COALESCE($2, label), endpoint = COALESCE($3, endpoint),
                region = COALESCE($4, region), bucket = COALESCE($5, bucket),
                prefix = COALESCE($6, prefix), path_style = COALESCE($7, path_style),
                access_key_id = COALESCE($8, access_key_id), secret_key_enc = COALESCE($9, secret_key_enc),
                key_hint = COALESCE($10, key_hint),
                status = CASE WHEN status = 'delisted' THEN status ELSE COALESCE($11, status) END, updated_at = now()
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(id)
        .bind(patch.label)
        .bind(patch.endpoint)
        .bind(patch.region)
        .bind(patch.bucket)
        .bind(patch.prefix)
        .bind(patch.path_style)
        .bind(key_id)
        .bind(enc)
        .bind(hint)
        .bind(patch.status.map(MountStatus::as_str))
        .execute(&self.pool)
        .await
        .map_err(insert_err)?;
        if res.rows_affected() == 0 {
            return Ok(None);
        }
        self.by_id(id).await
    }

    async fn soft_delete(&self, id: i64) -> MountResult<bool> {
        let res = sqlx::query(
            "UPDATE storage_mounts SET deleted_at = now(), updated_at = now()
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(repo_err)?;
        Ok(res.rows_affected() > 0)
    }

    async fn record_test(
        &self,
        id: i64,
        at_ms: i64,
        result_json: String,
        status: Option<MountStatus>,
    ) -> MountResult<bool> {
        let res = sqlx::query(
            "UPDATE storage_mounts SET last_test_at = to_timestamp($2::double precision / 1000.0),
                last_test = $3::jsonb,
                status = CASE WHEN status = 'delisted' THEN status ELSE COALESCE($4, status) END, updated_at = now()
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(id)
        .bind(at_ms)
        .bind(result_json)
        .bind(status.map(MountStatus::as_str))
        .execute(&self.pool)
        .await
        .map_err(repo_err)?;
        Ok(res.rows_affected() > 0)
    }

    async fn set_cors_ok(&self, id: i64, at_ms: Option<i64>) -> MountResult<bool> {
        let res = sqlx::query(
            "UPDATE storage_mounts SET cors_ok_at = to_timestamp($2::double precision / 1000.0), updated_at = now()
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(id)
        .bind(at_ms)
        .execute(&self.pool)
        .await
        .map_err(repo_err)?;
        Ok(res.rows_affected() > 0)
    }

    async fn list_enabled(&self) -> MountResult<Vec<Mount>> {
        let sql = format!(
            "SELECT {COLS} FROM storage_mounts
             WHERE deleted_at IS NULL AND status <> 'disabled' ORDER BY id"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .fetch_all(&self.pool)
            .await
            .map_err(repo_err)?;
        rows.iter().map(row_to_mount).collect()
    }

    async fn list_all(&self) -> MountResult<Vec<Mount>> {
        let sql = format!("SELECT {COLS} FROM storage_mounts WHERE deleted_at IS NULL ORDER BY id");
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .fetch_all(&self.pool)
            .await
            .map_err(repo_err)?;
        rows.iter().map(row_to_mount).collect()
    }

    async fn set_sealed(&self, id: i64, sealed: SealedSecret) -> MountResult<bool> {
        let res = sqlx::query(
            "UPDATE storage_mounts SET secret_key_enc = $2, updated_at = now()
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(id)
        .bind(sealed.as_bytes())
        .execute(&self.pool)
        .await
        .map_err(repo_err)?;
        Ok(res.rows_affected() > 0)
    }
}
