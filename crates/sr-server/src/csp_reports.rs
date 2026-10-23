//! Postgres counters behind `POST /internal/csp-report` (RDY-SEC-009).

use async_trait::async_trait;
use sr_api::routes::internal::{CspCounter, CspReportStore, CspSnapshot, Violation};
use sr_db::{PgPool, csp_reports::PgCspReports};

pub struct PgCspStore(PgCspReports);

impl PgCspStore {
    pub fn new(pool: PgPool) -> Self {
        Self(PgCspReports::new(pool))
    }
}

#[async_trait]
impl CspReportStore for PgCspStore {
    async fn record(&self, v: &Violation) {
        // A counter that cannot be written must not turn a browser report into an error page.
        if let Err(e) = self.0.record(&v.directive, &v.origin, &v.path).await {
            tracing::warn!(error = %e, "csp report was not counted");
        }
    }

    async fn top(&self, limit: usize) -> Result<CspSnapshot, String> {
        let rows = self
            .0
            .top(i64::try_from(limit).unwrap_or(10))
            .await
            .map_err(|e| e.to_string())?;
        let mut out = CspSnapshot::default();
        for r in rows {
            let c = CspCounter {
                key: r.key,
                count: u64::try_from(r.count).unwrap_or(0),
                last_seen_ms: r.last_seen_ms,
            };
            match r.kind.as_str() {
                "directive" => out.directives.push(c),
                "origin" => out.origins.push(c),
                "path" => out.paths.push(c),
                _ => {}
            }
        }
        Ok(out)
    }
}
