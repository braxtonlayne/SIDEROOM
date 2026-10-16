//! Library background work: the `library.ingest` job that converts uploaded chapter files, the
//! hourly `library.gc` sweep of unreferenced objects and abandoned upload slots, and the
//! Postgres-backed collaborators of the upload path (operator settings, job queue, access log).

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use sr_admin::{AdminService, Caller};
use sr_core::PlatformPermissions;
use sr_db::{
    DbResult, PgPool,
    admin_repo::PgAdminRepo,
    auth_repo::IdSource,
    jobs::{self, Job, NewJob},
    runner::{HandlerError, JobRunner},
};
use sr_library::{
    IngestQueue, IngestSettings, IngestSettingsSnapshot, LibraryAudit, LibraryService,
};

/// Kind of the per-upload conversion job (its id is the upload id).
pub const INGEST_KIND: &str = "library.ingest";
/// Kind and reserved singleton id of the storage collector.
pub const GC_KIND: &str = "library.gc";
pub const GC_JOB_ID: i64 = 150;
const GC_INTERVAL: Duration = Duration::from_secs(3600);
/// Attempts before an upload is failed instead of retried.
const INGEST_ATTEMPTS: i32 = 3;
/// Conversions at once (CPU and memory bound).
const INGEST_CONCURRENCY: usize = 2;

/// Platform setting keys (declared in the Admin settings catalog, group `library`).
pub const RAR_ENABLED_SETTING: &str = "library.ingest.rar_enabled";
pub const MAX_UPLOAD_MB_SETTING: &str = "library.ingest.max_upload_mb";

/// Reads the operator settings from `platform_settings`, defaults when absent or unreadable.
pub struct PgIngestSettings {
    pool: PgPool,
}

impl PgIngestSettings {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn value(&self, key: &str) -> Option<Value> {
        match sqlx::query_scalar::<_, Value>("SELECT value FROM platform_settings WHERE key = $1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, key, "reading a library setting failed; using the default");
                None
            }
        }
    }
}

/// Turn the stored values into limits. Unreadable values fall back to the defaults.
pub fn snapshot_from(rar: Option<&Value>, max_mb: Option<&Value>) -> IngestSettingsSnapshot {
    let d = IngestSettingsSnapshot::default();
    IngestSettingsSnapshot {
        rar_enabled: rar.and_then(Value::as_bool).unwrap_or(d.rar_enabled),
        max_upload_bytes: max_mb
            .and_then(Value::as_u64)
            .filter(|mb| (1..=4096).contains(mb))
            .map_or(d.max_upload_bytes, |mb| mb * 1024 * 1024),
    }
}

#[async_trait]
impl IngestSettings for PgIngestSettings {
    async fn snapshot(&self) -> IngestSettingsSnapshot {
        let rar = self.value(RAR_ENABLED_SETTING).await;
        let max = self.value(MAX_UPLOAD_MB_SETTING).await;
        snapshot_from(rar.as_ref(), max.as_ref())
    }
}

/// Queues `library.ingest` on the durable job queue.
pub struct PgIngestQueue {
    pool: PgPool,
}

impl PgIngestQueue {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl IngestQueue for PgIngestQueue {
    async fn enqueue(&self, upload_id: i64) -> Result<(), String> {
        let mut job = NewJob::new(upload_id, INGEST_KIND, json!({ "upload": upload_id }));
        job.max_attempts = INGEST_ATTEMPTS;
        jobs::enqueue(&self.pool, &job)
            .await
            .map_err(|e| e.to_string())
    }
}

/// Writes ingest outcomes to the staff access log as the uploading publisher.
pub struct AdminLibraryAudit {
    admin: AdminService,
}

impl AdminLibraryAudit {
    pub fn new(pool: PgPool, ids: IdSource) -> Self {
        Self {
            admin: AdminService::new(Arc::new(PgAdminRepo::new(pool, ids))),
        }
    }
}

#[async_trait]
impl LibraryAudit for AdminLibraryAudit {
    async fn record(&self, user: i64, action: &str, target: (&str, i64), detail: Value) {
        let caller = Caller {
            user_id: user,
            permissions: PlatformPermissions::PUBLISH_READING,
            is_owner: false,
            ip: None,
        };
        if let Err(e) = self
            .admin
            .record_action(&caller, action, Some(target), detail)
            .await
        {
            tracing::error!(error = %e, action, "library ingest was not written to the access log");
        }
    }
}

async fn ingest_handler(library: LibraryService, job: Job) -> Result<(), HandlerError> {
    let upload = job
        .payload
        .get("upload")
        .and_then(Value::as_i64)
        .unwrap_or(job.id);
    let final_attempt = job.attempts >= job.max_attempts;
    library
        .run_ingest(upload, final_attempt)
        .await
        .map_err(|e| -> HandlerError { Box::new(std::io::Error::other(e.to_string())) })
}

async fn gc_handler(pool: PgPool, library: LibraryService, job: Job) -> Result<(), HandlerError> {
    let report = library
        .run_gc()
        .await
        .map_err(|e| -> HandlerError { Box::new(std::io::Error::other(e.to_string())) })?;
    if report.objects_deleted > 0 || report.uploads_expired > 0 {
        tracing::info!(
            deleted = report.objects_deleted,
            expired = report.uploads_expired,
            failed = report.failed,
            "library storage swept"
        );
    }
    jobs::reschedule_running(&pool, job.id, GC_INTERVAL).await?;
    Ok(())
}

/// Register `library.ingest` and `library.gc` on `runner`. Pair with [`schedule_gc`].
pub fn register(runner: JobRunner, pool: &PgPool, library: LibraryService) -> JobRunner {
    let gc_pool = pool.clone();
    let ingest_lib = library.clone();
    runner
        .register(INGEST_KIND, INGEST_CONCURRENCY, move |job| {
            Box::pin(ingest_handler(ingest_lib.clone(), job))
        })
        .register(GC_KIND, 1, move |job| {
            Box::pin(gc_handler(gc_pool.clone(), library.clone(), job))
        })
}

/// Create or re-arm the collector singleton. Call only when [`register`] is on the runner.
pub async fn schedule_gc(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(pool, GC_JOB_ID, GC_KIND, &json!({}), GC_INTERVAL).await?;
    Ok(())
}

/// Kind and reserved singleton id of the ingest source scanner (REV-UP-005).
pub const SOURCES_KIND: &str = "library.sources";
pub const SOURCES_JOB_ID: i64 = 895;
/// How often the scanner looks for sources that are due (each has its own poll interval).
const SOURCES_INTERVAL: Duration = Duration::from_secs(30);

async fn sources_handler(
    pool: PgPool,
    scanner: sr_library::sources::SourceScanner,
    job: Job,
) -> Result<(), HandlerError> {
    let n = scanner.scan_due().await;
    if n > 0 {
        tracing::debug!(sources = n, "ingest sources scanned");
    }
    jobs::reschedule_running(&pool, job.id, SOURCES_INTERVAL).await?;
    Ok(())
}

/// Register `library.sources` on `runner`. Pair with [`schedule_sources`].
pub fn register_sources(
    runner: JobRunner,
    pool: &PgPool,
    scanner: sr_library::sources::SourceScanner,
) -> JobRunner {
    let pool = pool.clone();
    runner.register(SOURCES_KIND, 1, move |job| {
        Box::pin(sources_handler(pool.clone(), scanner.clone(), job))
    })
}

/// Create or re-arm the scanner singleton.
pub async fn schedule_sources(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(
        pool,
        SOURCES_JOB_ID,
        SOURCES_KIND,
        &json!({}),
        SOURCES_INTERVAL,
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_default_and_validate() {
        let d = snapshot_from(None, None);
        assert!(d.rar_enabled);
        assert_eq!(d.max_upload_bytes, 512 * 1024 * 1024);
        let s = snapshot_from(Some(&json!(false)), Some(&json!(100)));
        assert!(!s.rar_enabled);
        assert_eq!(s.max_upload_bytes, 100 * 1024 * 1024);
        // Garbage never loosens the limits.
        let g = snapshot_from(Some(&json!("no")), Some(&json!(-5)));
        assert_eq!(g, d);
        assert_eq!(snapshot_from(None, Some(&json!(0))), d);
        assert_eq!(snapshot_from(None, Some(&json!(99_999))), d);
    }

    /// The keys the jobs read are declared in the Admin settings catalog with matching defaults.
    #[test]
    fn settings_are_in_the_admin_catalog() {
        let rar = sr_admin::settings::find(RAR_ENABLED_SETTING).expect("rar setting declared");
        assert_eq!(rar.default_value(), json!(true));
        assert_eq!(rar.group, "library");
        assert!(rar.validate(&json!(false)).is_ok());
        assert!(rar.validate(&json!("off")).is_err());
        let max = sr_admin::settings::find(MAX_UPLOAD_MB_SETTING).expect("limit declared");
        assert_eq!(max.default_value(), json!(512));
        assert!(max.validate(&json!(0)).is_err());
        assert!(max.validate(&json!(1024)).is_ok());
        assert_eq!(
            snapshot_from(Some(&rar.default_value()), Some(&max.default_value())),
            IngestSettingsSnapshot::default()
        );
    }
}
