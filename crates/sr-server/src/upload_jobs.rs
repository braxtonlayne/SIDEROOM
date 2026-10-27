//! The sweep of abandoned browser uploads (BYOS-BRW-001): an upload nobody touched for 24 hours is aborted at
//! the member's bucket (so the provider stops billing its parts) and closed in the ledger; part-URL notes older
//! than an hour are dropped. Hourly singleton, re-queues itself like the other recurring jobs.

use std::{sync::Arc, time::Duration};

use serde_json::json;
use sr_db::{
    DbResult, PgPool,
    jobs::{self, Job},
    runner::{HandlerError, JobRunner},
};
use sr_storage::mounts::MountResolver;
use sr_storage::uploads::{ABANDON_AFTER_MS, MountIo, UploadLedger, UploadState};

/// Singleton id of the sweep.
pub const UPLOADS_SWEEP_JOB_ID: i64 = 138;
/// Job kind of the sweep.
pub const UPLOADS_SWEEP_KIND: &str = "storage.uploads_sweep";
const EVERY: Duration = Duration::from_secs(3600);
/// Most uploads one pass closes.
const BATCH: usize = 200;
/// An upload whose bucket cannot be reached is retried hourly for this long, then closed anyway (the provider's
/// own lifecycle rule is then the owner's to set).
const GIVE_UP_AFTER_MS: i64 = 7 * 24 * 3600 * 1000;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(0))
        .unwrap_or(0)
}

/// What one pass did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SweepReport {
    /// Uploads aborted at the bucket and closed.
    pub aborted: usize,
    /// Uploads closed without a bucket call (the mount is gone) or given up on.
    pub closed_only: usize,
    /// Uploads left open for the next pass (the bucket did not answer).
    pub retry: usize,
}

/// One pass at `now`.
pub async fn sweep_once(
    ledger: &dyn UploadLedger,
    mounts: &dyn MountResolver,
    io: &dyn MountIo,
    now: i64,
) -> Result<SweepReport, String> {
    let mut report = SweepReport::default();
    let stale = ledger
        .open_before(now - ABANDON_AFTER_MS, BATCH)
        .await
        .map_err(|e| e.to_string())?;
    for up in stale {
        let mount = mounts.by_id(up.mount_id).await.map_err(|e| e.to_string())?;
        let Some(mount) = mount else {
            // The mount was removed: there are no credentials to abort with.
            ledger
                .finish(up.mount_id, up.id, UploadState::Aborted, now)
                .await
                .map_err(|e| e.to_string())?;
            report.closed_only += 1;
            continue;
        };
        let aborted = tokio::time::timeout(
            Duration::from_secs(20),
            io.abort(&mount, &up.key, &up.s3_upload_id),
        )
        .await;
        match aborted {
            Ok(Ok(())) => {
                ledger
                    .finish(up.mount_id, up.id, UploadState::Aborted, now)
                    .await
                    .map_err(|e| e.to_string())?;
                report.aborted += 1;
            }
            _ if now - up.created_at > GIVE_UP_AFTER_MS => {
                ledger
                    .finish(up.mount_id, up.id, UploadState::Aborted, now)
                    .await
                    .map_err(|e| e.to_string())?;
                report.closed_only += 1;
            }
            _ => report.retry += 1,
        }
    }
    ledger
        .purge_part_notes(now - 3_600_000)
        .await
        .map_err(|e| e.to_string())?;
    Ok(report)
}

/// Register the `storage.uploads_sweep` handler; pair with [`schedule`].
pub fn register(
    runner: JobRunner,
    pool: &PgPool,
    ledger: Arc<dyn UploadLedger>,
    mounts: Arc<dyn MountResolver>,
    io: Arc<dyn MountIo>,
) -> JobRunner {
    let pool = pool.clone();
    runner.register(UPLOADS_SWEEP_KIND, 1, move |job: Job| {
        let (pool, ledger, mounts, io) = (pool.clone(), ledger.clone(), mounts.clone(), io.clone());
        Box::pin(async move {
            match sweep_once(ledger.as_ref(), mounts.as_ref(), io.as_ref(), now_ms()).await {
                Ok(r) if r != SweepReport::default() => {
                    tracing::info!(report = ?r, "abandoned uploads swept");
                }
                Ok(_) => {}
                // A hiccup is retried at the next tick, not dead-lettered.
                Err(e) => tracing::warn!(error = %e, "upload sweep failed"),
            }
            jobs::reschedule_running(&pool, job.id, EVERY)
                .await
                .map_err(HandlerError::from)?;
            Ok(())
        })
    })
}

/// Create or re-arm the singleton. Call only when [`register`] is on the runner.
pub async fn schedule(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(
        pool,
        UPLOADS_SWEEP_JOB_ID,
        UPLOADS_SWEEP_KIND,
        &json!({}),
        EVERY,
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_storage::mounts::{
        MemoryMountRepo, MountError, MountInput, MountOwner, MountRepo, NewMount, Provider, Sealer,
        Secret,
    };
    use sr_storage::uploads::{MemoryLedger, MemoryMountIo, MountUpload, PART_SIZE};

    struct Plain;
    impl Sealer for Plain {
        fn seal(&self, _aad: &[u8], plain: &[u8]) -> Result<Vec<u8>, MountError> {
            Ok(plain.to_vec())
        }
        fn open(&self, _aad: &[u8], blob: &[u8]) -> Result<Vec<u8>, MountError> {
            Ok(blob.to_vec())
        }
    }

    const DAY: i64 = 24 * 3600 * 1000;

    async fn mount(repo: &Arc<dyn MountRepo>, id: i64) {
        let new = NewMount::seal(
            &Plain,
            id,
            MountInput {
                owner: MountOwner::User(7),
                label: format!("m{id}"),
                provider: Provider::Garage,
                endpoint: "http://127.0.0.1:9".into(),
                region: "garage".into(),
                bucket: "b".into(),
                prefix: String::new(),
                path_style: true,
                access_key_id: "AK".into(),
            },
            &Secret::new("s"),
        )
        .unwrap();
        repo.create(new, None).await.unwrap();
    }

    fn upload(id: i64, mount_id: i64, s3: &str, at: i64) -> MountUpload {
        MountUpload {
            id,
            mount_id,
            user_id: 7,
            key: format!("k{id}"),
            s3_upload_id: s3.into(),
            size: 1,
            part_size: PART_SIZE,
            part_count: 1,
            content_type: "x".into(),
            state: UploadState::Open,
            created_at: at,
            updated_at: at,
        }
    }

    #[tokio::test]
    async fn only_uploads_idle_for_a_day_are_aborted_at_the_bucket() {
        let repo: Arc<dyn MountRepo> = MemoryMountRepo::shared();
        mount(&repo, 10).await;
        let io = MemoryMountIo::new();
        let ledger = MemoryLedger::new();
        let m = repo.by_id(10).await.unwrap().unwrap();
        let old_s3 = io.begin(&m, "k1", "x").await.unwrap();
        let fresh_s3 = io.begin(&m, "k2", "x").await.unwrap();
        let t0 = 5 * DAY;
        ledger.insert(upload(1, 10, &old_s3, t0)).await.unwrap();
        ledger
            .insert(upload(2, 10, &fresh_s3, t0 + DAY))
            .await
            .unwrap();
        ledger.note_parts(7, 2, 1, t0 + DAY).await.unwrap();

        let r = sweep_once(&ledger, repo.as_ref(), &io, t0 + DAY + 1)
            .await
            .unwrap();
        assert_eq!(
            r,
            SweepReport {
                aborted: 1,
                closed_only: 0,
                retry: 0
            }
        );
        assert!(!io.is_open(&old_s3), "the parts were freed at the bucket");
        assert!(io.is_open(&fresh_s3), "a fresh upload is left alone");
        assert_eq!(ledger.count_open(7).await.unwrap(), 1);
        // A second pass has nothing to do.
        let r = sweep_once(&ledger, repo.as_ref(), &io, t0 + DAY + 2)
            .await
            .unwrap();
        assert_eq!(r, SweepReport::default());
        // The hourly note is gone two hours later.
        sweep_once(&ledger, repo.as_ref(), &io, t0 + DAY + 7_200_000)
            .await
            .unwrap();
        assert_eq!(ledger.parts_since(7, 0).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn a_removed_mount_is_closed_and_a_dead_bucket_is_retried_then_given_up() {
        let repo: Arc<dyn MountRepo> = MemoryMountRepo::shared();
        mount(&repo, 10).await;
        let io = MemoryMountIo::new();
        let ledger = MemoryLedger::new();
        let t0 = 10 * DAY;
        ledger.insert(upload(1, 99, "gone", t0)).await.unwrap();
        ledger.insert(upload(2, 10, "x", t0)).await.unwrap();
        *io.broken.lock().unwrap() = true;
        let r = sweep_once(&ledger, repo.as_ref(), &io, t0 + 2 * DAY)
            .await
            .unwrap();
        assert_eq!(
            r,
            SweepReport {
                aborted: 0,
                closed_only: 1,
                retry: 1
            }
        );
        assert_eq!(ledger.count_open(7).await.unwrap(), 1);
        // A week on, the dead bucket is given up on.
        let r = sweep_once(&ledger, repo.as_ref(), &io, t0 + 8 * DAY)
            .await
            .unwrap();
        assert_eq!(r.closed_only, 1);
        assert_eq!(ledger.count_open(7).await.unwrap(), 0);
    }
}
