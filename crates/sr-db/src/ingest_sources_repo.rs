//! Postgres implementation of the ingest sources (migration 0892) for [`PgLibraryRepo`].

use async_trait::async_trait;
use serde_json::Value;
use sqlx::{Row, postgres::PgRow};
use sr_library::error::LibraryResult;
use sr_library::sources::{Backend, FileState, IngestSource, LedgerFile, SourceKind, SourceStore};

use crate::library_repo::{PgLibraryRepo, db};

const COLS: &str = "id, label, kind, backend, location, owner_id, enabled, poll_secs, stable_secs, \
     (extract(epoch FROM last_scan_at) * 1000)::bigint AS last_scan_ms, last_scan, last_error, \
     (extract(epoch FROM created_at) * 1000)::bigint AS created_ms";

fn source_from(r: &PgRow) -> LibraryResult<IngestSource> {
    let kind: String = r.try_get("kind").map_err(db)?;
    let backend: String = r.try_get("backend").map_err(db)?;
    Ok(IngestSource {
        id: r.try_get("id").map_err(db)?,
        label: r.try_get("label").map_err(db)?,
        kind: SourceKind::parse(&kind).unwrap_or(SourceKind::WatchedFolder),
        backend: Backend::parse(&backend).unwrap_or(Backend::Local),
        location: r.try_get("location").map_err(db)?,
        owner_id: r.try_get("owner_id").map_err(db)?,
        enabled: r.try_get("enabled").map_err(db)?,
        poll_secs: r.try_get("poll_secs").map_err(db)?,
        stable_secs: r.try_get("stable_secs").map_err(db)?,
        last_scan_at_ms: r.try_get("last_scan_ms").map_err(db)?,
        last_scan: r.try_get("last_scan").map_err(db)?,
        last_error: r.try_get("last_error").map_err(db)?,
        created_at_ms: r.try_get("created_ms").map_err(db)?,
    })
}

fn q(sql: String) -> sqlx::query::Query<'static, sqlx::Postgres, sqlx::postgres::PgArguments> {
    sqlx::query(sqlx::AssertSqlSafe(sql))
}

#[async_trait]
impl SourceStore for PgLibraryRepo {
    async fn list(&self) -> LibraryResult<Vec<IngestSource>> {
        let rows = q(format!("SELECT {COLS} FROM ingest_sources ORDER BY id"))
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter().map(source_from).collect()
    }

    async fn get(&self, id: i64) -> LibraryResult<Option<IngestSource>> {
        let row = q(format!("SELECT {COLS} FROM ingest_sources WHERE id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        row.as_ref().map(source_from).transpose()
    }

    async fn create(&self, s: &IngestSource) -> LibraryResult<()> {
        sqlx::query(
            "INSERT INTO ingest_sources \
               (id, label, kind, backend, location, owner_id, enabled, poll_secs, stable_secs) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(s.id)
        .bind(&s.label)
        .bind(s.kind.as_str())
        .bind(s.backend.as_str())
        .bind(&s.location)
        .bind(s.owner_id)
        .bind(s.enabled)
        .bind(s.poll_secs)
        .bind(s.stable_secs)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn update(&self, s: &IngestSource) -> LibraryResult<bool> {
        let done = sqlx::query(
            "UPDATE ingest_sources SET label = $2, owner_id = $3, enabled = $4, poll_secs = $5, \
               stable_secs = $6, updated_at = now() WHERE id = $1",
        )
        .bind(s.id)
        .bind(&s.label)
        .bind(s.owner_id)
        .bind(s.enabled)
        .bind(s.poll_secs)
        .bind(s.stable_secs)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(done.rows_affected() > 0)
    }

    async fn delete(&self, id: i64) -> LibraryResult<bool> {
        let done = sqlx::query("DELETE FROM ingest_sources WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(done.rows_affected() > 0)
    }

    async fn due(&self, now_ms: i64) -> LibraryResult<Vec<IngestSource>> {
        let rows = q(format!(
            "SELECT {COLS} FROM ingest_sources \
             WHERE enabled AND kind = 'watched_folder' \
               AND (last_scan_at IS NULL \
                    OR last_scan_at <= to_timestamp(($1::float8 / 1000.0) - poll_secs)) \
             ORDER BY last_scan_at NULLS FIRST LIMIT 20"
        ))
        .bind(now_ms)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(source_from).collect()
    }

    async fn is_staff(&self, user: i64) -> LibraryResult<bool> {
        let found: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM staff_appointments WHERE account_id = $1 AND revoked_at IS NULL",
        )
        .bind(user)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        Ok(found.is_some())
    }

    async fn ledger(&self, source: i64) -> LibraryResult<Vec<LedgerFile>> {
        let rows = sqlx::query(
            "SELECT rel_path, size_bytes, mtime_ms, etag, stable_since_ms, state, submission_id, error \
             FROM ingest_files WHERE source_id = $1 ORDER BY rel_path",
        )
        .bind(source)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(|r| {
                let state: String = r.try_get("state").map_err(db)?;
                let size: i64 = r.try_get("size_bytes").map_err(db)?;
                Ok(LedgerFile {
                    rel_path: r.try_get("rel_path").map_err(db)?,
                    size: u64::try_from(size).unwrap_or(0),
                    mtime_ms: r.try_get("mtime_ms").map_err(db)?,
                    etag: r.try_get("etag").map_err(db)?,
                    stable_since_ms: r.try_get("stable_since_ms").map_err(db)?,
                    state: FileState::parse(&state).unwrap_or(FileState::Seen),
                    submission_id: r.try_get("submission_id").map_err(db)?,
                    error: r.try_get("error").map_err(db)?,
                })
            })
            .collect()
    }

    async fn put_files(&self, source: i64, files: &[LedgerFile]) -> LibraryResult<()> {
        if files.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        for chunk in files.chunks(500) {
            let paths: Vec<&str> = chunk.iter().map(|f| f.rel_path.as_str()).collect();
            let sizes: Vec<i64> = chunk
                .iter()
                .map(|f| i64::try_from(f.size).unwrap_or(i64::MAX))
                .collect();
            let mtimes: Vec<i64> = chunk.iter().map(|f| f.mtime_ms).collect();
            let etags: Vec<Option<&str>> = chunk.iter().map(|f| f.etag.as_deref()).collect();
            let stable: Vec<i64> = chunk.iter().map(|f| f.stable_since_ms).collect();
            let states: Vec<&str> = chunk.iter().map(|f| f.state.as_str()).collect();
            let subs: Vec<Option<i64>> = chunk.iter().map(|f| f.submission_id).collect();
            let errors: Vec<Option<&str>> = chunk.iter().map(|f| f.error.as_deref()).collect();
            sqlx::query(
                "INSERT INTO ingest_files \
                   (source_id, rel_path, size_bytes, mtime_ms, etag, stable_since_ms, state, submission_id, error) \
                 SELECT $1, * FROM UNNEST($2::text[], $3::bigint[], $4::bigint[], $5::text[], \
                                          $6::bigint[], $7::text[], $8::bigint[], $9::text[]) \
                 ON CONFLICT (source_id, rel_path) DO UPDATE SET \
                   size_bytes = EXCLUDED.size_bytes, mtime_ms = EXCLUDED.mtime_ms, \
                   etag = EXCLUDED.etag, stable_since_ms = EXCLUDED.stable_since_ms, \
                   state = EXCLUDED.state, submission_id = EXCLUDED.submission_id, \
                   error = EXCLUDED.error, updated_at = now()",
            )
            .bind(source)
            .bind(&paths)
            .bind(&sizes)
            .bind(&mtimes)
            .bind(&etags)
            .bind(&stable)
            .bind(&states)
            .bind(&subs)
            .bind(&errors)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        tx.commit().await.map_err(db)?;
        Ok(())
    }

    async fn finish_scan(
        &self,
        source: i64,
        now_ms: i64,
        summary: &Value,
        error: Option<&str>,
    ) -> LibraryResult<()> {
        sqlx::query(
            "UPDATE ingest_sources SET last_scan_at = to_timestamp($2::float8 / 1000.0), \
               last_scan = $3, last_error = $4, updated_at = now() WHERE id = $1",
        )
        .bind(source)
        .bind(now_ms)
        .bind(summary)
        .bind(error)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }
}
