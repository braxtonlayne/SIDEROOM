//! The figures behind `sr_storage_bytes` for the object store and the Library and Cinema media (RDY-OBS-002).

use crate::DbResult;
use sqlx::PgPool;

/// One measured bucket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageFigure {
    pub bucket: String,
    pub bytes: i64,
    pub objects: i64,
    /// The walk stopped at its cap or time budget: the figures are a lower bound.
    pub truncated: bool,
    pub measured_at_ms: i64,
}

/// Store the figures of one run (insert or replace, in one transaction).
pub async fn save(pool: &PgPool, figures: &[(&str, i64, i64, bool)]) -> DbResult<()> {
    let mut tx = pool.begin().await?;
    for (bucket, bytes, objects, truncated) in figures {
        sqlx::query(
            "INSERT INTO storage_figures (bucket, bytes, objects, truncated, measured_at)
             VALUES ($1, $2, $3, $4, now())
             ON CONFLICT (bucket) DO UPDATE
                SET bytes = $2, objects = $3, truncated = $4, measured_at = now()",
        )
        .bind(bucket)
        .bind(bytes)
        .bind(objects)
        .bind(truncated)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Every stored figure, by bucket name.
pub async fn list(pool: &PgPool) -> DbResult<Vec<StorageFigure>> {
    let rows: Vec<(String, i64, i64, bool, i64)> = sqlx::query_as(
        "SELECT bucket, bytes, objects, truncated, (extract(epoch FROM measured_at) * 1000)::bigint
         FROM storage_figures ORDER BY bucket",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(bucket, bytes, objects, truncated, measured_at_ms)| StorageFigure {
                bucket,
                bytes,
                objects,
                truncated,
                measured_at_ms,
            },
        )
        .collect())
}
