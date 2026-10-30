//! The `rings.expire` job (PRF-RNG-001): hard expiry of 24-hour status rings.
//!
//! One leased singleton (id 1710) that runs every ten minutes. A pass
//!
//! 1. deletes every ring past `expires_at` (and every ring its author deleted whose hold has ended) that no
//!    moderation hold freezes, and queues its object and evidence copy in `status_ring_purge`;
//! 2. copies the file of every held photo or clip that lives on a member's own mount into the instance bucket
//!    (`rings/evidence/<id>`), so the evidence outlives the member's bucket and connection;
//! 3. deletes the queued objects through the mounts: from the author's own mount, or from the instance bucket
//!    under `rings/`. A bucket that is down keeps the entry for the next pass (20 tries).
//!
//! A ring under a hold keeps its row and object until the hold ends; no reader sees it meanwhile
//! (`status_ring_visible_to` says no once `expires_at` has passed).

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;
use sqlx::PgPool;
use sr_db::DbResult;
use sr_db::jobs::{self, Job};
use sr_db::runner::{HandlerError, JobRunner};
use sr_storage::StorageError;
use sr_storage::mount_storage::MountStorages;
use sr_userfeat::UserFeatures;
use sr_userfeat::rings::{ObjectFailure, RingObjects};

/// Kind of the expiry job.
pub const RINGS_KIND: &str = "rings.expire";
/// Reserved singleton id (below any Snowflake; unique among `*_JOB_ID`, 1710 is the first of the rings range).
pub const RINGS_JOB_ID: i64 = 1710;
/// How often the job runs.
pub const INTERVAL: Duration = Duration::from_secs(10 * 60);
/// Rows deleted, evidence copies made and objects deleted per pass; the rest wait for the next one.
pub const BATCH: i64 = 500;

/// The object calls of the job over the mounts.
pub struct MountObjects {
    storages: Arc<MountStorages>,
}

impl MountObjects {
    pub fn new(storages: Arc<MountStorages>) -> Self {
        Self { storages }
    }
}

fn failure(e: StorageError) -> ObjectFailure {
    match e {
        // A deleted or erased mount, or an object that is already gone: nothing left to try.
        StorageError::NotFound(_) | StorageError::InvalidPath(_) => ObjectFailure::Gone,
        other => ObjectFailure::Retry(other.to_string()),
    }
}

#[async_trait]
impl RingObjects for MountObjects {
    async fn delete(&self, mount_id: i64, key: &str) -> Result<(), ObjectFailure> {
        let storage = self.storages.for_mount(mount_id).await.map_err(failure)?;
        match storage.delete(key).await {
            Ok(()) => Ok(()),
            Err(StorageError::NotFound(_)) => Ok(()),
            Err(e) => Err(failure(e)),
        }
    }

    async fn copy_to_instance(
        &self,
        mount_id: i64,
        key: &str,
        to_key: &str,
    ) -> Result<(), ObjectFailure> {
        let from = self.storages.for_mount(mount_id).await.map_err(failure)?;
        let bytes = from.get(key).await.map_err(failure)?;
        self.storages
            .instance()
            .put(to_key, bytes)
            .await
            .map_err(failure)
    }
}

/// One pass. Returns what it did, for the log.
pub async fn pass(uf: &UserFeatures, objects: &dyn RingObjects) -> Result<String, String> {
    let e = |e: sr_userfeat::UfError| e.to_string();
    let copied = uf
        .copy_held_ring_evidence(objects, BATCH)
        .await
        .map_err(e)?;
    let expired = uf.expire_rings(BATCH).await.map_err(e)?;
    let purged = uf.drain_ring_purge(objects, BATCH).await.map_err(e)?;
    Ok(format!(
        "{} rings deleted, {} held, {} evidence copies, {} objects deleted, {} retried, {} given up",
        expired.rows_deleted,
        expired.held_skipped,
        copied,
        purged.deleted,
        purged.retried,
        purged.gave_up
    ))
}

async fn handler(
    pool: PgPool,
    uf: UserFeatures,
    objects: Arc<MountObjects>,
    job: Job,
) -> Result<(), HandlerError> {
    match pass(&uf, objects.as_ref()).await {
        Ok(line) => tracing::debug!(%line, "rings.expire"),
        // The next pass retries; a failure must not stop the singleton.
        Err(why) => tracing::warn!(error = %why, "rings.expire failed"),
    }
    jobs::reschedule_running(&pool, job.id, INTERVAL).await?;
    Ok(())
}

/// Register `rings.expire`. Pair with [`schedule`].
pub fn register(
    runner: JobRunner,
    pool: &PgPool,
    uf: UserFeatures,
    storages: Arc<MountStorages>,
) -> JobRunner {
    let pool = pool.clone();
    let objects = Arc::new(MountObjects::new(storages));
    runner.register(RINGS_KIND, 1, move |job| {
        Box::pin(handler(pool.clone(), uf.clone(), objects.clone(), job))
    })
}

/// Create or re-arm the singleton. Call only when [`register`] is on the runner.
pub async fn schedule(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(pool, RINGS_JOB_ID, RINGS_KIND, &json!({}), INTERVAL).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_mount_or_object_is_gone_and_a_backend_error_is_retried() {
        assert_eq!(
            failure(StorageError::NotFound("m".into())),
            ObjectFailure::Gone
        );
        assert!(matches!(
            failure(StorageError::Backend("down".into())),
            ObjectFailure::Retry(_)
        ));
    }

    #[test]
    fn the_job_id_is_in_the_rings_migration_range() {
        assert_eq!(RINGS_JOB_ID, 1710);
        assert_eq!(INTERVAL, Duration::from_secs(600));
    }
}
