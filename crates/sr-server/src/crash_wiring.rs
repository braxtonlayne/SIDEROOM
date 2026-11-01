//! The crash store behind `POST /crash-reports` (OPS-CRASH-001): `sr-api` defines the trait, this adapts the Postgres
//! table of migration 2160 to it.

use serde_json::Value;
use sr_api::routes::crash::{CrashRecorded, CrashRow, CrashStore};
use sr_db::crash_reports::{CrashRow as DbRow, PgCrashReports};

pub struct PgCrashStore(pub PgCrashReports);

fn row(r: DbRow) -> CrashRow {
    CrashRow {
        id: r.id,
        fingerprint: r.fingerprint,
        commit_sha: r.commit_sha,
        kind: r.kind,
        route: r.route,
        count: r.count,
        first_seen_ms: r.first_seen_ms,
        last_seen_ms: r.last_seen_ms,
        card: r.card,
        replay: r.replay,
    }
}

#[async_trait::async_trait]
impl CrashStore for PgCrashStore {
    async fn record(
        &self,
        fingerprint: &str,
        commit_sha: &str,
        kind: &str,
        route: &str,
        card: &Value,
        replay: &Value,
    ) -> Result<CrashRecorded, String> {
        let r = self
            .0
            .record(fingerprint, commit_sha, kind, route, card, replay)
            .await
            .map_err(|e| e.to_string())?;
        Ok(CrashRecorded {
            id: r.id,
            count: r.count,
            is_new: r.is_new,
            channel_id: r.channel_id,
            message_id: r.message_id,
        })
    }

    async fn set_message(&self, id: i64, channel: i64, message: i64) -> Result<(), String> {
        self.0
            .set_message(id, channel, message)
            .await
            .map_err(|e| e.to_string())
    }

    async fn list(&self, limit: i64) -> Result<Vec<CrashRow>, String> {
        let rows = self.0.list(limit).await.map_err(|e| e.to_string())?;
        Ok(rows.into_iter().map(row).collect())
    }

    async fn get(&self, id: i64) -> Result<Option<CrashRow>, String> {
        let r = self.0.get(id).await.map_err(|e| e.to_string())?;
        Ok(r.map(row))
    }
}
