//! Recurring admin jobs: the nightly verification of the staff access-log hash chain
//! (ADM-STF-008, doc 07 section 5), the health check that raises disk, backup and transcode
//! alerts (ADM-ALR-001) and the hourly expiry of unanswered staff offers (ADM-STF-004). Each
//! re-queues itself like the other recurring singletons.

use std::{path::Path, sync::Arc, time::Duration};

use serde_json::json;
use sr_admin::{AdminService, AlertSettings};
use sr_db::{
    DbResult, PgPool,
    admin_repo::PgAdminRepo,
    auth_repo::IdSource,
    jobs::{self, Job},
    runner::{HandlerError, JobRunner},
};
use sr_events::{DomainEvent, EventBus, StaffAlertKind, Topic};

use crate::staff_feed::{gib, used_percent};

/// Health check singleton: disk, backups, failed transcodes.
pub const HEALTH_JOB_ID: i64 = 131;
pub const HEALTH_KIND: &str = "admin.health.check";
const HEALTH_EVERY: Duration = Duration::from_secs(15 * 60);
/// Offer expiry singleton.
pub const OFFERS_JOB_ID: i64 = 132;
pub const OFFERS_KIND: &str = "admin.offers.expire";
const OFFERS_EVERY: Duration = Duration::from_secs(3600);
/// The same alert is not raised again within this many hours.
const REPEAT_AFTER_HOURS: i32 = 24;

/// Reserved singleton id (below any Snowflake).
pub const VERIFY_JOB_ID: i64 = 130;
pub const VERIFY_KIND: &str = "admin.access_log.verify";
const HOUR_UTC: u32 = 4;
const MINUTE_UTC: u32 = 15;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(0))
        .unwrap_or(0)
}

fn delay() -> std::time::Duration {
    jobs::delay_until_daily_utc(now_ms(), HOUR_UTC, MINUTE_UTC)
}

/// Register the handler on `runner`; pair with [`schedule`].
pub fn register(
    runner: JobRunner,
    pool: &PgPool,
    ids: IdSource,
    bus: Arc<dyn EventBus>,
) -> JobRunner {
    let pool = pool.clone();
    let repo = Arc::new(PgAdminRepo::new(pool.clone(), ids));
    let admin = AdminService::new(repo.clone())
        .with_extras(repo)
        .with_bus(bus.clone());
    let runner = {
        let (pool, admin, bus) = (pool.clone(), admin.clone(), bus.clone());
        runner.register(HEALTH_KIND, 1, move |job: Job| {
            let (pool, admin, bus) = (pool.clone(), admin.clone(), bus.clone());
            Box::pin(async move {
                run_health(&pool, &admin, &bus).await;
                jobs::reschedule_running(&pool, job.id, HEALTH_EVERY).await?;
                Ok(())
            })
        })
    };
    let runner = {
        let (pool, admin) = (pool.clone(), admin.clone());
        runner.register(OFFERS_KIND, 1, move |job: Job| {
            let (pool, admin) = (pool.clone(), admin.clone());
            Box::pin(async move {
                match admin.expire_offers().await {
                    Ok(0) => {}
                    Ok(n) => tracing::info!(expired = n, "staff offers expired"),
                    Err(e) => tracing::warn!(error = %e, "could not expire staff offers"),
                }
                jobs::reschedule_running(&pool, job.id, OFFERS_EVERY).await?;
                Ok(())
            })
        })
    };
    runner.register(VERIFY_KIND, 1, move |job: Job| {
        let pool = pool.clone();
        let service = admin.clone();
        Box::pin(async move {
            let report = service
                .verify_and_alert()
                .await
                .map_err(|e| -> HandlerError { Box::new(std::io::Error::other(e.to_string())) })?;
            if report.ok {
                tracing::info!(rows = report.rows_checked, "staff access log verified");
            } else {
                tracing::error!(
                    seq = report.first_bad_seq,
                    "staff access log chain is BROKEN"
                );
            }
            jobs::reschedule_running(&pool, job.id, delay()).await?;
            Ok(())
        })
    })
}

/// Create or re-arm the recurring admin singletons.
pub async fn schedule(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(pool, VERIFY_JOB_ID, VERIFY_KIND, &json!({}), delay()).await?;
    jobs::schedule_singleton(
        pool,
        HEALTH_JOB_ID,
        HEALTH_KIND,
        &json!({}),
        Duration::from_secs(60),
    )
    .await?;
    jobs::schedule_singleton(
        pool,
        OFFERS_JOB_ID,
        OFFERS_KIND,
        &json!({}),
        Duration::from_secs(300),
    )
    .await?;
    Ok(())
}

// ---- health check (ADM-ALR-001) ---------------------------------------------------

/// The disk alert text when the disk holding `path` is `threshold` percent full or more.
pub fn disk_alert(path: &str, total: u64, available: u64, threshold: i64) -> Option<String> {
    let used = used_percent(total, available)?;
    (i64::from(used) >= threshold).then(|| {
        format!(
            "The disk holding {path} is {used}% full ({} free of {}).",
            gib(available),
            gib(total)
        )
    })
}

/// The backup alert text: no backup at all, or the newest one too old.
pub fn backup_alert(dir: &str, newest_age: Option<Duration>, max_hours: i64) -> Option<String> {
    match newest_age {
        None => Some(format!("No backup found in {dir}.")),
        Some(age) => {
            let hours = age.as_secs() / 3600;
            (i64::try_from(hours).unwrap_or(i64::MAX) > max_hours).then(|| {
                format!(
                    "The newest backup in {dir} is {hours} hours old (the limit is {max_hours})."
                )
            })
        }
    }
}

/// Age of the newest backup file (`*.dump` or `*.json` manifest) in `dir` or `dir/db`.
pub fn newest_backup_age(dir: &Path) -> Option<Duration> {
    newest_backup_modified(dir).and_then(|m| m.elapsed().ok())
}

/// Modification time of the newest backup file in `dir` or `dir/db`.
pub fn newest_backup_modified(dir: &Path) -> Option<std::time::SystemTime> {
    let mut newest: Option<std::time::SystemTime> = None;
    for d in [dir.to_path_buf(), dir.join("db")] {
        let Ok(entries) = std::fs::read_dir(d) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("");
            if !matches!(ext, "dump" | "json") {
                continue;
            }
            if let Ok(m) = e.metadata().and_then(|m| m.modified())
                && newest.is_none_or(|n| m > n)
            {
                newest = Some(m);
            }
        }
    }
    newest
}

fn unix_ms(t: std::time::SystemTime) -> Option<i64> {
    t.duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
}

/// When a marker file was last written (the restore drill and the object snapshot leave one in the backup
/// folder: `restore-drill.ok`, `snapshot.ok`).
fn marker_ms(path: &Path) -> Option<i64> {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(unix_ms)
}

/// Backup times and disk space for the System page (K-10, ADM-PLT-006). Reads the backup folder the health
/// check watches and the disks of the data folder and the ingest roots; a path that cannot be read is reported
/// with null sizes.
pub struct FsProbe {
    ingest_roots: Vec<String>,
}

impl FsProbe {
    pub fn new(ingest_roots: Vec<String>) -> Self {
        Self { ingest_roots }
    }
}

impl sr_api::routes::admin::SystemProbe for FsProbe {
    fn backups(&self, dir: &str) -> sr_api::routes::admin::BackupsView {
        if dir.is_empty() {
            return Default::default();
        }
        let dir = Path::new(dir);
        sr_api::routes::admin::BackupsView {
            last_run_ms: newest_backup_modified(dir).and_then(unix_ms),
            last_verify_ms: marker_ms(&dir.join("restore-drill.ok")),
            snapshot_ms: marker_ms(&dir.join("snapshot.ok")),
        }
    }

    fn disks(&self, data_dir: &str) -> Vec<sr_api::routes::admin::DiskView> {
        let mut paths: Vec<String> = vec![if data_dir.is_empty() {
            ".".to_owned()
        } else {
            data_dir.to_owned()
        }];
        for r in &self.ingest_roots {
            if !paths.contains(r) {
                paths.push(r.clone());
            }
        }
        paths
            .into_iter()
            .map(|path| {
                // A path that is not a folder here reports no sizes (some systems answer for the
                // volume of a missing path).
                let folder = Path::new(&path).is_dir();
                sr_api::routes::admin::DiskView {
                    total_bytes: folder.then(|| fs4::total_space(&path).ok()).flatten(),
                    free_bytes: folder.then(|| fs4::available_space(&path).ok()).flatten(),
                    path,
                }
            })
            .collect()
    }
}

/// Disk check for the configured (or working) directory.
fn check_disk(s: &AlertSettings) -> Option<String> {
    let path = if s.disk_path.is_empty() {
        "."
    } else {
        s.disk_path.as_str()
    };
    let total = fs4::total_space(path).ok()?;
    let available = fs4::available_space(path).ok()?;
    let label = if s.disk_path.is_empty() {
        "the server data"
    } else {
        s.disk_path.as_str()
    };
    disk_alert(label, total, available, s.disk_percent)
}

/// Raise an alert unless the same kind was raised in the last day.
async fn alert_once(pool: &PgPool, bus: &Arc<dyn EventBus>, kind: StaffAlertKind, detail: String) {
    let recent: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM staff_alerts WHERE kind = $1 \
                        AND created_at > now() - make_interval(hours => $2))",
    )
    .bind(kind.as_str())
    .bind(REPEAT_AFTER_HOURS)
    .fetch_one(pool)
    .await
    .unwrap_or(true);
    if recent {
        return;
    }
    if let Err(e) = bus
        .publish(Topic::Platform, DomainEvent::StaffAlert { kind, detail })
        .await
    {
        tracing::warn!(error = %e, "could not publish a health alert");
    }
}

/// Failed transcode jobs nobody was told about yet; one alert per job.
async fn failed_transcodes(pool: &PgPool, bus: &Arc<dyn EventBus>) {
    let rows: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT j.id, j.kind, COALESCE(left(j.error, 160), '') FROM transcode_jobs j \
         WHERE j.state = 'failed' AND j.finished_at > now() - interval '2 days' \
           AND NOT EXISTS (SELECT 1 FROM staff_alerts a WHERE a.kind = 'transcode_failed' \
                           AND a.detail LIKE 'Transcode job ' || j.id::text || ' %') \
         ORDER BY j.id LIMIT 20",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (id, kind, error) in rows {
        let detail = if error.is_empty() {
            format!("Transcode job {id} ({kind}) failed.")
        } else {
            format!("Transcode job {id} ({kind}) failed: {error}")
        };
        let ev = DomainEvent::StaffAlert {
            kind: StaffAlertKind::TranscodeFailed,
            detail,
        };
        if let Err(e) = bus.publish(Topic::Platform, ev).await {
            tracing::warn!(error = %e, "could not publish a transcode alert");
        }
    }
}

/// Statement timeout for the gauge queries: a slow count must never hold the health pass.
const GAUGE_QUERY_TIMEOUT: &str = "3s";

/// Set the job, backup, restore, WAL, disk and storage gauges (RDY-OBS-001). Each run sets them from the
/// same figures the System page shows; nothing is computed on scrape. A query that fails leaves the old value.
async fn record_gauges(pool: &PgPool, s: &AlertSettings) {
    use sr_api::observability as obs;
    // Jobs waiting, per kind, and the age of the oldest one that is already due.
    if let Ok(rows) = sqlx::query_as::<_, (String, i64)>(
        "SELECT kind, count(*) FROM jobs WHERE state = 'queued' GROUP BY kind",
    )
    .fetch_all(pool)
    .await
    {
        let oldest: f64 = sqlx::query_scalar(
            "SELECT COALESCE(EXTRACT(EPOCH FROM now() - min(run_at)), 0)::float8 \
             FROM jobs WHERE state = 'queued' AND run_at <= now()",
        )
        .fetch_one(pool)
        .await
        .unwrap_or(0.0);
        obs::record_jobs_queued(&rows, oldest.max(0.0));
    }
    // Backup and restore drill: 0 when there is none, so the stale rule fires.
    let dir = s.backup_dir.clone();
    let (backup, restore) = tokio::task::spawn_blocking(move || {
        if dir.is_empty() {
            return (None, None);
        }
        let d = Path::new(&dir);
        let secs = |ms: Option<i64>| ms.map(|m| m as f64 / 1000.0);
        (
            secs(newest_backup_modified(d).and_then(unix_ms)),
            secs(marker_ms(&d.join("restore-drill.ok"))),
        )
    })
    .await
    .unwrap_or((None, None));
    obs::record_backup_times(backup, restore);
    // The WAL archiver; the series stays absent while archive_mode is off.
    if let Ok((on, last, failed)) = sqlx::query_as::<_, (bool, Option<f64>, i64)>(
        "SELECT current_setting('archive_mode') <> 'off', \
                EXTRACT(EPOCH FROM last_archived_time)::float8, failed_count \
         FROM pg_stat_archiver",
    )
    .fetch_one(pool)
    .await
        && on
    {
        obs::record_wal_archive(last, u64::try_from(failed).unwrap_or(0));
    }
    // The disk holding the data folder.
    let path = if s.disk_path.is_empty() {
        ".".to_owned()
    } else {
        s.disk_path.clone()
    };
    let p = path.clone();
    if let Ok(Some((total, free))) = tokio::task::spawn_blocking(move || {
        Some((fs4::total_space(&p).ok()?, fs4::available_space(&p).ok()?))
    })
    .await
    {
        obs::record_disk_free(&path, free, total);
    }
    // Stored attachment bytes, under a short statement timeout.
    if let Ok(mut tx) = pool.begin().await {
        let set = format!("SET LOCAL statement_timeout = '{GAUGE_QUERY_TIMEOUT}'");
        if sqlx::query(sqlx::AssertSqlSafe(set))
            .execute(&mut *tx)
            .await
            .is_ok()
            && let Ok(bytes) = sqlx::query_scalar::<_, i64>(
                "SELECT COALESCE(sum(size_bytes), 0)::bigint FROM attachments",
            )
            .fetch_one(&mut *tx)
            .await
        {
            obs::record_storage_bytes("attachments", u64::try_from(bytes).unwrap_or(0));
        }
        let _ = tx.rollback().await;
    }
}

/// One pass of the health check.
pub async fn run_health(pool: &PgPool, admin: &AdminService, bus: &Arc<dyn EventBus>) {
    let settings = match admin.alert_settings().await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "health check: settings unreadable");
            return;
        }
    };
    let disk_settings = settings.clone();
    if let Ok(Some(detail)) = tokio::task::spawn_blocking(move || check_disk(&disk_settings)).await
    {
        alert_once(pool, bus, StaffAlertKind::DiskLow, detail).await;
    }
    if !settings.backup_dir.is_empty() {
        let (dir, max) = (settings.backup_dir.clone(), settings.backup_max_age_hours);
        let age = {
            let dir = dir.clone();
            tokio::task::spawn_blocking(move || newest_backup_age(Path::new(&dir)))
                .await
                .ok()
                .flatten()
        };
        if let Some(detail) = backup_alert(&dir, age, max) {
            alert_once(pool, bus, StaffAlertKind::BackupFailed, detail).await;
        }
    }
    record_gauges(pool, &settings).await;
    failed_transcodes(pool, bus).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_disk_alert_fires_at_the_threshold() {
        assert!(disk_alert("/data", 100, 21, 80).is_none());
        let d = disk_alert("/data", 100, 20, 80).unwrap();
        assert!(d.contains("80% full") && d.contains("/data"));
        assert!(disk_alert("/data", 0, 0, 80).is_none());
        assert!(disk_alert("/data", 100, 0, 99).is_some());
    }

    #[test]
    fn the_backup_alert_covers_missing_and_stale_backups() {
        let h = |n: u64| Some(Duration::from_secs(n * 3600));
        assert!(backup_alert("/b", h(2), 36).is_none());
        assert!(backup_alert("/b", h(36), 36).is_none());
        let stale = backup_alert("/b", h(40), 36).unwrap();
        assert!(stale.contains("40 hours old") && stale.contains("limit is 36"));
        assert!(
            backup_alert("/b", None, 36)
                .unwrap()
                .contains("No backup found")
        );
    }

    #[test]
    fn the_newest_backup_is_found_in_the_dir_or_its_db_folder() {
        let dir = std::env::temp_dir().join(format!("sr-backup-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("db")).unwrap();
        assert!(newest_backup_age(&dir).is_none());
        std::fs::write(dir.join("db").join("sideroom-1.dump"), b"x").unwrap();
        std::fs::write(dir.join("notes.txt"), b"x").unwrap();
        let age = newest_backup_age(&dir).unwrap();
        assert!(age < Duration::from_secs(60));
        assert!(newest_backup_age(&dir.join("missing")).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_probe_reports_backup_times_and_disk_space_without_failing() {
        use sr_api::routes::admin::SystemProbe;
        let dir = std::env::temp_dir().join(format!("sr-probe-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("db")).unwrap();
        let probe = FsProbe::new(vec![
            dir.to_string_lossy().into_owned(),
            "/definitely/not/here".to_owned(),
        ]);
        // Nothing configured, or nothing written yet: every time is null.
        assert_eq!(probe.backups(""), Default::default());
        assert_eq!(probe.backups(&dir.to_string_lossy()), Default::default());
        std::fs::write(dir.join("db").join("sideroom-1.dump"), b"x").unwrap();
        std::fs::write(dir.join("restore-drill.ok"), b"1").unwrap();
        let b = probe.backups(&dir.to_string_lossy());
        let now = unix_ms(std::time::SystemTime::now()).unwrap();
        assert!(b.last_run_ms.is_some_and(|t| (now - t).abs() < 60_000));
        assert!(b.last_verify_ms.is_some_and(|t| (now - t).abs() < 60_000));
        assert_eq!(b.snapshot_ms, None);
        // The data folder is listed first, then each ingest root once; an unreadable path has null sizes.
        let d = probe.disks(&dir.to_string_lossy());
        assert_eq!(d.len(), 2, "{d:?}");
        assert!(d[0].total_bytes.is_some_and(|t| t > 0) && d[0].free_bytes.is_some());
        assert_eq!(d[1].path, "/definitely/not/here");
        assert_eq!((d[1].total_bytes, d[1].free_bytes), (None, None));
        assert_eq!(probe.disks("")[0].path, ".");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
