//! Postgres implementation of [`UploadSessions`] (migration 1240): the multipart upload behind a
//! member's draft video (RDY-MED-001).

use async_trait::async_trait;
use sqlx::{PgPool, Row, postgres::PgRow};
use sr_cinema::drafts::{Target, UploadSession, UploadSessions};

#[derive(Clone)]
pub struct PgUploadSessions {
    pool: PgPool,
}

impl PgUploadSessions {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const COLS: &str = "asset_id, submission_id, target_kind, target_id, s3_upload_id, part_size, part_count, size_bytes";

fn session(r: &PgRow) -> Result<UploadSession, String> {
    let e = |e: sqlx::Error| e.to_string();
    let kind: String = r.try_get("target_kind").map_err(e)?;
    let id: i64 = r.try_get("target_id").map_err(e)?;
    Ok(UploadSession {
        asset_id: r.try_get("asset_id").map_err(e)?,
        submission_id: r.try_get("submission_id").map_err(e)?,
        target: Target::parse(&kind, id).ok_or_else(|| format!("unknown target kind {kind}"))?,
        s3_upload_id: r.try_get("s3_upload_id").map_err(e)?,
        part_size: u64::try_from(r.try_get::<i64, _>("part_size").map_err(e)?).unwrap_or(0),
        part_count: u32::try_from(r.try_get::<i32, _>("part_count").map_err(e)?).unwrap_or(0),
        size_bytes: u64::try_from(r.try_get::<i64, _>("size_bytes").map_err(e)?).unwrap_or(0),
    })
}

#[async_trait]
impl UploadSessions for PgUploadSessions {
    async fn put(&self, s: &UploadSession) -> Result<(), String> {
        sqlx::query(
            "INSERT INTO cinema_submission_uploads \
             (asset_id, submission_id, target_kind, target_id, s3_upload_id, part_size, part_count, size_bytes) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
             ON CONFLICT (asset_id) DO UPDATE SET submission_id = EXCLUDED.submission_id, \
             target_kind = EXCLUDED.target_kind, target_id = EXCLUDED.target_id, \
             s3_upload_id = EXCLUDED.s3_upload_id, part_size = EXCLUDED.part_size, \
             part_count = EXCLUDED.part_count, size_bytes = EXCLUDED.size_bytes",
        )
        .bind(s.asset_id)
        .bind(s.submission_id)
        .bind(s.target.as_str())
        .bind(s.target.id())
        .bind(&s.s3_upload_id)
        .bind(i64::try_from(s.part_size).unwrap_or(i64::MAX))
        .bind(i32::try_from(s.part_count).unwrap_or(i32::MAX))
        .bind(i64::try_from(s.size_bytes).unwrap_or(i64::MAX))
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
    }

    async fn get(&self, asset_id: i64) -> Result<Option<UploadSession>, String> {
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {COLS} FROM cinema_submission_uploads WHERE asset_id = $1"
        )))
        .bind(asset_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        row.as_ref().map(session).transpose()
    }

    async fn for_submission(&self, submission_id: i64) -> Result<Vec<UploadSession>, String> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {COLS} FROM cinema_submission_uploads WHERE submission_id = $1 ORDER BY asset_id"
        )))
        .bind(submission_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        rows.iter().map(session).collect()
    }

    async fn remove(&self, asset_id: i64) -> Result<(), String> {
        sqlx::query("DELETE FROM cinema_submission_uploads WHERE asset_id = $1")
            .bind(asset_id)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}
