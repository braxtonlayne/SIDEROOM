//! Counters behind `POST /internal/csp-report` and the Admin > System card (RDY-SEC-009).

use crate::DbResult;
use sqlx::PgPool;

/// Most keys one kind keeps; the rest count under [`OTHER`].
pub const MAX_KEYS_PER_KIND: i64 = 200;
/// The overflow key.
pub const OTHER: &str = "(other)";

/// One counter row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CounterRow {
    pub kind: String,
    pub key: String,
    pub count: i64,
    pub last_seen_ms: i64,
}

#[derive(Clone)]
pub struct PgCspReports {
    pool: PgPool,
}

impl PgCspReports {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Count one report: `directive` (the violated directive name), the blocked `origin` and the document `path`.
    /// Empty values are skipped.
    pub async fn record(&self, directive: &str, origin: &str, path: &str) -> DbResult<()> {
        for (kind, key) in [("directive", directive), ("origin", origin), ("path", path)] {
            if key.is_empty() {
                continue;
            }
            sqlx::query(
                "INSERT INTO csp_report_counters (kind, key, count)
                 SELECT $1,
                        CASE WHEN EXISTS (SELECT 1 FROM csp_report_counters WHERE kind = $1 AND key = $2)
                                  OR (SELECT count(*) FROM csp_report_counters WHERE kind = $1) < $3
                             THEN $2 ELSE $4 END,
                        1
                 ON CONFLICT (kind, key) DO UPDATE
                    SET count = csp_report_counters.count + 1, last_seen = now()",
            )
            .bind(kind)
            .bind(key)
            .bind(MAX_KEYS_PER_KIND)
            .bind(OTHER)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    /// The top `limit` keys of every kind, most counted first.
    pub async fn top(&self, limit: i64) -> DbResult<Vec<CounterRow>> {
        let rows: Vec<(String, String, i64, i64)> = sqlx::query_as(
            "SELECT kind, key, count, (extract(epoch FROM last_seen) * 1000)::bigint
               FROM (SELECT *, row_number() OVER (PARTITION BY kind ORDER BY count DESC, key) AS n
                       FROM csp_report_counters) t
              WHERE n <= $1
              ORDER BY kind, count DESC, key",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(kind, key, count, last_seen_ms)| CounterRow {
                kind,
                key,
                count,
                last_seen_ms,
            })
            .collect())
    }
}
