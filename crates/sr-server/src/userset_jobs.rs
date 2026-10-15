//! Background work of the user-settings backend (phase 11):
//!
//! * `user.data_export` builds a member's data package (LNG-USET-020, doc 09 section 4) into
//!   a temporary file, stores the zip under `exports/<user>/<id>.zip` and offers it for
//!   7 days. Retried with backoff; after the last attempt the request is marked failed so the
//!   member can ask again at once.
//! * `user.data_export.sweep` (hourly singleton) deletes packages whose link lapsed.
//! * `user.connections.refresh` (hourly singleton) re-reads the public stats of GitHub and
//!   Reddit connections older than a day (LNG-USET-024); no token is stored, so other
//!   providers keep the numbers read when the account was linked.

use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use bytes::Bytes;
use serde_json::json;
use sr_api::routes::social::SocialDeps;
use sr_api::routes::userset::connections::{REFRESHABLE, refresh_stats};
use sr_api::routes::userset::data_export::{DataExportStore, ExportRecord, Refusal};
use sr_db::data_export as db;
use sr_db::jobs::{self, Job};
use sr_db::runner::{HandlerError, JobRunner};
use sr_db::{DbResult, PgPool};
use sr_net::SafeClient;
use sr_storage::ObjectStorage;

/// Reserved singleton id of the export sweep.
pub const EXPORT_SWEEP_JOB_ID: i64 = 660;
/// Kind of the export sweep.
pub const EXPORT_SWEEP_KIND: &str = "user.data_export.sweep";
/// Reserved singleton id of the stats refresh.
pub const CONNECTIONS_JOB_ID: i64 = 661;
/// Kind of the stats refresh.
pub const CONNECTIONS_KIND: &str = "user.connections.refresh";
const SWEEP_INTERVAL: Duration = Duration::from_secs(3600);
/// Stats older than this are re-read.
const STATS_MAX_AGE_MS: i64 = 24 * 3_600_000;
/// Connections refreshed per run (GitHub allows 60 anonymous requests an hour).
const REFRESH_BATCH: usize = 30;
/// Pause between two provider requests.
const REFRESH_PAUSE: Duration = Duration::from_millis(500);
/// Part size of the multipart upload of a large package.
const PART: usize = 8 * 1024 * 1024;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn to_record(r: db::ExportRecord) -> ExportRecord {
    ExportRecord {
        id: r.id,
        state: r.state,
        requested_ms: r.requested_ms,
        ready_ms: r.ready_ms,
        expires_ms: r.expires_ms,
        storage_key: r.storage_key,
        size_bytes: r.size_bytes,
    }
}

/// Postgres behind the export routes.
pub struct PgExportStore {
    pool: PgPool,
    ids: sr_db::auth_repo::IdSource,
}

impl PgExportStore {
    pub fn new(pool: PgPool, ids: sr_db::auth_repo::IdSource) -> Self {
        Self { pool, ids }
    }
}

#[async_trait]
impl DataExportStore for PgExportStore {
    async fn latest(&self, user: i64) -> Result<Option<ExportRecord>, String> {
        db::latest(&self.pool, user)
            .await
            .map(|r| r.map(to_record))
            .map_err(|e| e.to_string())
    }

    async fn get(&self, user: i64, id: i64) -> Result<Option<ExportRecord>, String> {
        db::get(&self.pool, user, id)
            .await
            .map(|r| r.map(to_record))
            .map_err(|e| e.to_string())
    }

    async fn request(&self, user: i64) -> Result<Result<ExportRecord, Refusal>, String> {
        let done = db::request(&self.pool, (self.ids)(), (self.ids)(), user)
            .await
            .map_err(|e| e.to_string())?;
        Ok(match done {
            Ok(r) => Ok(to_record(r)),
            Err(db::RequestRefusal::InProgress(r)) => Err(Refusal::InProgress(to_record(r))),
            Err(db::RequestRefusal::TooSoon { retry_after_secs }) => {
                Err(Refusal::TooSoon { retry_after_secs })
            }
        })
    }
}

/// Build the package for `export` and store it; returns the object key and size.
async fn build_and_store(
    pool: &PgPool,
    storage: &dyn ObjectStorage,
    export: i64,
    user: i64,
) -> Result<(String, i64), HandlerError> {
    let mut file = tempfile::tempfile()?;
    db::build_package(pool, user, &mut file, now_ms()).await?;
    let size = file.seek(SeekFrom::End(0))?;
    file.seek(SeekFrom::Start(0))?;
    let key = format!("exports/{user}/{export}.zip");
    if size as usize <= PART {
        let mut buf = Vec::with_capacity(size as usize);
        file.read_to_end(&mut buf)?;
        storage.put(&key, Bytes::from(buf)).await?;
    } else {
        let mut upload = storage.multipart_create(&key).await?;
        let mut buf = vec![0_u8; PART];
        loop {
            let mut filled = 0;
            while filled < PART {
                let n = file.read(&mut buf[filled..])?;
                if n == 0 {
                    break;
                }
                filled += n;
            }
            if filled == 0 {
                break;
            }
            if let Err(e) = upload.put_part(Bytes::copy_from_slice(&buf[..filled])).await {
                let _ = upload.abort().await;
                return Err(e.into());
            }
            if filled < PART {
                break;
            }
        }
        upload.complete().await?;
    }
    Ok((key, i64::try_from(size).unwrap_or(i64::MAX)))
}

/// The build job.
pub(crate) async fn run_export(
    pool: &PgPool,
    storage: &dyn ObjectStorage,
    job: &Job,
) -> Result<(), HandlerError> {
    let id: i64 = job.payload["export_id"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or("export job without an id")?;
    // Gone or already finished: nothing to do.
    let Some(user) = db::begin(pool, id).await? else {
        return Ok(());
    };
    match build_and_store(pool, storage, id, user).await {
        Ok((key, size)) => {
            db::finish(pool, id, &key, size).await?;
            tracing::info!(export = id, bytes = size, "data package built");
            Ok(())
        }
        Err(e) => {
            tracing::warn!(export = id, error = %e, "data package build failed");
            if job.attempts >= job.max_attempts {
                db::fail(pool, id, &e.to_string()).await?;
                return Ok(());
            }
            Err(e)
        }
    }
}

/// Delete packages whose 7 days are over.
pub(crate) async fn sweep_exports(pool: &PgPool, storage: &dyn ObjectStorage) -> DbResult<usize> {
    let lapsed = db::lapsed(pool).await?;
    let n = lapsed.len();
    for (id, key) in lapsed {
        match storage.delete(&key).await {
            Ok(()) | Err(sr_storage::StorageError::NotFound(_)) => db::mark_expired(pool, id).await?,
            // Try again next hour.
            Err(e) => tracing::warn!(export = id, error = %e, "could not delete an expired package"),
        }
    }
    Ok(n)
}

/// Re-read the stats of stale GitHub / Reddit connections.
async fn refresh_connections(social: &SocialDeps, client: &SafeClient) -> usize {
    let stale = match social
        .service
        .stale_connections(now_ms() - STATS_MAX_AGE_MS, REFRESHABLE, REFRESH_BATCH)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "could not list connections to refresh");
            return 0;
        }
    };
    let mut updated = 0;
    for c in &stale {
        // A failure keeps the old numbers but moves the connection to the back of the queue.
        let stats = refresh_stats(client, c).await;
        if stats.is_some() {
            updated += 1;
        }
        let stats = stats.unwrap_or_else(|| c.stats.clone());
        if let Err(e) = social.service.refresh_connection_stats(c, &stats).await {
            tracing::warn!(error = %e, "could not store refreshed connection stats");
        }
        tokio::time::sleep(REFRESH_PAUSE).await;
    }
    updated
}

/// Register the three handlers. Pair with [`schedule`].
pub fn register(
    runner: JobRunner,
    pool: &PgPool,
    storage: Arc<dyn ObjectStorage>,
    social: Arc<SocialDeps>,
    client: SafeClient,
) -> JobRunner {
    let (export_pool, export_storage) = (pool.clone(), storage.clone());
    let runner = runner.register(db::JOB_KIND, 1, move |job: Job| {
        let (pool, storage) = (export_pool.clone(), export_storage.clone());
        Box::pin(async move { run_export(&pool, storage.as_ref(), &job).await })
    });
    let (sweep_pool, sweep_storage) = (pool.clone(), storage);
    let runner = runner.register(EXPORT_SWEEP_KIND, 1, move |job: Job| {
        let (pool, storage) = (sweep_pool.clone(), sweep_storage.clone());
        Box::pin(async move {
            match sweep_exports(&pool, storage.as_ref()).await {
                Ok(0) => {}
                Ok(n) => tracing::info!(packages = n, "lapsed data packages handled"),
                Err(e) => tracing::warn!(error = %e, "data package sweep failed"),
            }
            jobs::reschedule_running(&pool, job.id, SWEEP_INTERVAL)
                .await
                .map_err(HandlerError::from)?;
            Ok(())
        })
    });
    let refresh_pool = pool.clone();
    runner.register(CONNECTIONS_KIND, 1, move |job: Job| {
        let (pool, social, client) = (refresh_pool.clone(), social.clone(), client.clone());
        Box::pin(async move {
            let n = refresh_connections(&social, &client).await;
            if n > 0 {
                tracing::info!(connections = n, "connection stats refreshed");
            }
            jobs::reschedule_running(&pool, job.id, SWEEP_INTERVAL)
                .await
                .map_err(HandlerError::from)?;
            Ok(())
        })
    })
}

/// Create or re-arm the two recurring singletons. Call only when [`register`] is on the runner.
pub async fn schedule(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(
        pool,
        EXPORT_SWEEP_JOB_ID,
        EXPORT_SWEEP_KIND,
        &json!({}),
        SWEEP_INTERVAL,
    )
    .await?;
    jobs::schedule_singleton(
        pool,
        CONNECTIONS_JOB_ID,
        CONNECTIONS_KIND,
        &json!({}),
        SWEEP_INTERVAL,
    )
    .await?;
    Ok(())
}
