//! Postgres implementation of `sr_storage::uploads::UploadLedger` (BYOS-BRW-001, migration 1440).

use async_trait::async_trait;
use sqlx::{PgPool, Row, postgres::PgRow};
use sr_storage::uploads::{LedgerError, MountUpload, UploadLedger, UploadState};

const COLS: &str =
    "id, mount_id, user_id, object_key, s3_upload_id, size_bytes, part_size, part_count,
    content_type, state,
    (extract(epoch FROM created_at) * 1000)::bigint AS created_ms,
    (extract(epoch FROM updated_at) * 1000)::bigint AS updated_ms";

/// Postgres-backed upload ledger.
#[derive(Clone)]
pub struct PgUploadLedger {
    pool: PgPool,
}

impl PgUploadLedger {
    /// Wrap a pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn err(e: sqlx::Error) -> LedgerError {
    LedgerError(e.to_string())
}

fn row_to_upload(r: &PgRow) -> Result<MountUpload, LedgerError> {
    let state: String = r.try_get("state").map_err(err)?;
    let size: i64 = r.try_get("size_bytes").map_err(err)?;
    let part_size: i64 = r.try_get("part_size").map_err(err)?;
    let part_count: i32 = r.try_get("part_count").map_err(err)?;
    Ok(MountUpload {
        id: r.try_get("id").map_err(err)?,
        mount_id: r.try_get("mount_id").map_err(err)?,
        user_id: r.try_get("user_id").map_err(err)?,
        key: r.try_get("object_key").map_err(err)?,
        s3_upload_id: r.try_get("s3_upload_id").map_err(err)?,
        size: u64::try_from(size).unwrap_or(0),
        part_size: u64::try_from(part_size).unwrap_or(0),
        part_count: u32::try_from(part_count).unwrap_or(0),
        content_type: r.try_get("content_type").map_err(err)?,
        state: UploadState::parse(&state)
            .ok_or_else(|| LedgerError(format!("unknown state {state}")))?,
        created_at: r.try_get("created_ms").map_err(err)?,
        updated_at: r.try_get("updated_ms").map_err(err)?,
    })
}

fn ts(ms: i64) -> f64 {
    ms as f64 / 1000.0
}

#[async_trait]
impl UploadLedger for PgUploadLedger {
    async fn insert(&self, u: MountUpload) -> Result<(), LedgerError> {
        sqlx::query(
            "INSERT INTO storage_mount_uploads
               (id, mount_id, user_id, object_key, s3_upload_id, size_bytes, part_size, part_count,
                content_type, state, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, to_timestamp($11), to_timestamp($12))",
        )
        .bind(u.id)
        .bind(u.mount_id)
        .bind(u.user_id)
        .bind(&u.key)
        .bind(&u.s3_upload_id)
        .bind(i64::try_from(u.size).unwrap_or(i64::MAX))
        .bind(i64::try_from(u.part_size).unwrap_or(i64::MAX))
        .bind(i32::try_from(u.part_count).unwrap_or(i32::MAX))
        .bind(&u.content_type)
        .bind(u.state.as_str())
        .bind(ts(u.created_at))
        .bind(ts(u.updated_at))
        .execute(&self.pool)
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn get(&self, mount_id: i64, id: i64) -> Result<Option<MountUpload>, LedgerError> {
        let sql =
            format!("SELECT {COLS} FROM storage_mount_uploads WHERE id = $1 AND mount_id = $2");
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .bind(mount_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(err)?;
        row.as_ref().map(row_to_upload).transpose()
    }

    async fn list_open(
        &self,
        mount_id: i64,
        user_id: i64,
    ) -> Result<Vec<MountUpload>, LedgerError> {
        let sql = format!(
            "SELECT {COLS} FROM storage_mount_uploads
             WHERE mount_id = $1 AND user_id = $2 AND state = 'open' ORDER BY id DESC LIMIT 200"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(mount_id)
            .bind(user_id)
            .fetch_all(&self.pool)
            .await
            .map_err(err)?;
        rows.iter().map(row_to_upload).collect()
    }

    async fn count_open(&self, user_id: i64) -> Result<usize, LedgerError> {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM storage_mount_uploads WHERE user_id = $1 AND state = 'open'",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(err)?;
        Ok(usize::try_from(n).unwrap_or(0))
    }

    async fn finish(
        &self,
        mount_id: i64,
        id: i64,
        state: UploadState,
        at_ms: i64,
    ) -> Result<bool, LedgerError> {
        let r = sqlx::query(
            "UPDATE storage_mount_uploads SET state = $3, updated_at = to_timestamp($4)
             WHERE id = $1 AND mount_id = $2 AND state = 'open'",
        )
        .bind(id)
        .bind(mount_id)
        .bind(state.as_str())
        .bind(ts(at_ms))
        .execute(&self.pool)
        .await
        .map_err(err)?;
        Ok(r.rows_affected() > 0)
    }

    async fn open_before(
        &self,
        cutoff_ms: i64,
        limit: usize,
    ) -> Result<Vec<MountUpload>, LedgerError> {
        let sql = format!(
            "SELECT {COLS} FROM storage_mount_uploads
             WHERE state = 'open' AND updated_at < to_timestamp($1) ORDER BY updated_at LIMIT $2"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(ts(cutoff_ms))
            .bind(i64::try_from(limit).unwrap_or(i64::MAX))
            .fetch_all(&self.pool)
            .await
            .map_err(err)?;
        rows.iter().map(row_to_upload).collect()
    }

    async fn note_parts(
        &self,
        user_id: i64,
        upload_id: i64,
        n: u32,
        at_ms: i64,
    ) -> Result<(), LedgerError> {
        let mut tx = self.pool.begin().await.map_err(err)?;
        sqlx::query(
            "INSERT INTO storage_mount_part_notes (user_id, noted_at, parts) VALUES ($1, to_timestamp($2), $3)",
        )
        .bind(user_id)
        .bind(ts(at_ms))
        .bind(i32::try_from(n).unwrap_or(i32::MAX))
        .execute(&mut *tx)
        .await
        .map_err(err)?;
        sqlx::query("UPDATE storage_mount_uploads SET updated_at = to_timestamp($2) WHERE id = $1")
            .bind(upload_id)
            .bind(ts(at_ms))
            .execute(&mut *tx)
            .await
            .map_err(err)?;
        tx.commit().await.map_err(err)
    }

    async fn parts_since(&self, user_id: i64, since_ms: i64) -> Result<u64, LedgerError> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COALESCE(sum(parts), 0)::bigint FROM storage_mount_part_notes
             WHERE user_id = $1 AND noted_at >= to_timestamp($2)",
        )
        .bind(user_id)
        .bind(ts(since_ms))
        .fetch_one(&self.pool)
        .await
        .map_err(err)?;
        Ok(u64::try_from(n).unwrap_or(0))
    }

    async fn purge_part_notes(&self, before_ms: i64) -> Result<u64, LedgerError> {
        let r =
            sqlx::query("DELETE FROM storage_mount_part_notes WHERE noted_at < to_timestamp($1)")
                .bind(ts(before_ms))
                .execute(&self.pool)
                .await
                .map_err(err)?;
        Ok(r.rows_affected())
    }
}
