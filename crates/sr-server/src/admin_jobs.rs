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

/// Daily storage measurement singleton (RDY-OBS-002): one listing walk of the bucket with an object cap and a time
/// budget, stored in `storage_figures` and exported by `sr_storage_bytes{bucket}`. Not part of the 15-minute health pass.
pub const STORAGE_MEASURE_JOB_ID: i64 = 133;
pub const STORAGE_MEASURE_KIND: &str = "storage.measure";
const STORAGE_HOUR_UTC: u32 = 5;
const STORAGE_MINUTE_UTC: u32 = 20;

/// Daily CORS recheck singleton (BYOS-MNT-003): reads every live mount's bucket CORS rules, stamps `cors_ok_at` or
/// marks the mount failing with the reason.
pub const CORS_RECHECK_JOB_ID: i64 = 134;
pub const CORS_RECHECK_KIND: &str = "storage.cors_recheck";
const CORS_HOUR_UTC: u32 = 5;
const CORS_MINUTE_UTC: u32 = 40;
/// The walk stops after this long and stores what it has, flagged as a lower bound.
pub const STORAGE_WALK_BUDGET: Duration = Duration::from_secs(60);
/// The default of `ops.storage_measure_max_objects`, used when the setting cannot be read.
const STORAGE_MAX_OBJECTS_DEFAULT: u64 = 200_000;
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
    jobs::schedule_singleton(
        pool,
        STORAGE_MEASURE_JOB_ID,
        STORAGE_MEASURE_KIND,
        &json!({}),
        jobs::delay_until_daily_utc(now_ms(), STORAGE_HOUR_UTC, STORAGE_MINUTE_UTC),
    )
    .await?;
    jobs::schedule_singleton(
        pool,
        CORS_RECHECK_JOB_ID,
        CORS_RECHECK_KIND,
        &json!({}),
        jobs::delay_until_daily_utc(now_ms(), CORS_HOUR_UTC, CORS_MINUTE_UTC),
    )
    .await?;
    Ok(())
}

// ---- storage measurement (RDY-OBS-002) ---------------------------------------------

/// The part of the bucket a key belongs to, besides "everything": Library media (`library/`) or Cinema media (posters,
/// originals and transcoded renditions: `cinema/`, `originals/`, `transcode/`).
pub fn storage_part(key: &str) -> Option<&'static str> {
    let top = key.split('/').next().unwrap_or("");
    match top {
        "library" => Some("library"),
        "cinema" | "originals" | "transcode" => Some("cinema"),
        _ => None,
    }
}

/// One listing walk of the bucket, at most `max_objects` objects and `budget` long, stored as three figures
/// (`objects`, `library`, `cinema`) and set on the gauge. A walk that stops early stores what it saw with
/// `truncated` set. Returns the figures as `(bucket, bytes, objects, truncated)`.
pub async fn measure_storage(
    pool: &PgPool,
    storage: &dyn sr_storage::ObjectStorage,
    max_objects: u64,
    budget: Duration,
) -> Result<Vec<(&'static str, i64, i64, bool)>, String> {
    let mut tally = [(0u64, 0u64); 3]; // objects, library, cinema: (bytes, count)
    // The walk checks the clock between objects; the timeout also bounds one stuck listing request (the tally so far
    // is kept either way).
    let walked = tokio::time::timeout(
        budget + Duration::from_secs(5),
        storage.walk(max_objects, budget, &mut |key, size| {
            tally[0].0 += size;
            tally[0].1 += 1;
            let slot = match storage_part(key) {
                Some("library") => 1,
                Some(_) => 2,
                None => return,
            };
            tally[slot].0 += size;
            tally[slot].1 += 1;
        }),
    )
    .await;
    let complete = match walked {
        Ok(r) => r.map_err(|e| e.to_string())?,
        Err(_) => false,
    };
    let figures: Vec<_> = ["objects", "library", "cinema"]
        .into_iter()
        .zip(tally)
        .map(|(bucket, (bytes, count))| {
            (
                bucket,
                i64::try_from(bytes).unwrap_or(i64::MAX),
                i64::try_from(count).unwrap_or(i64::MAX),
                !complete,
            )
        })
        .collect();
    sr_db::storage_figures::save(pool, &figures)
        .await
        .map_err(|e| e.to_string())?;
    for (bucket, bytes, _, _) in &figures {
        sr_api::observability::record_storage_bytes(bucket, u64::try_from(*bytes).unwrap_or(0));
    }
    Ok(figures)
}

/// Register the `storage.measure` handler; pair with [`schedule`]. It needs the object store, so it is separate from
/// [`register`].
pub fn register_storage_measure(
    runner: JobRunner,
    pool: &PgPool,
    ids: IdSource,
    storage: Arc<dyn sr_storage::ObjectStorage>,
) -> JobRunner {
    let pool = pool.clone();
    let admin = AdminService::new(Arc::new(PgAdminRepo::new(pool.clone(), ids)));
    runner.register(STORAGE_MEASURE_KIND, 1, move |job: Job| {
        let (pool, storage, admin) = (pool.clone(), storage.clone(), admin.clone());
        Box::pin(async move {
            let max = admin
                .setting_value_text(sr_admin::settings::OPS_STORAGE_MEASURE_MAX_OBJECTS)
                .await
                .ok()
                .and_then(|v| v.trim().parse::<u64>().ok())
                .unwrap_or(STORAGE_MAX_OBJECTS_DEFAULT);
            match measure_storage(&pool, storage.as_ref(), max, STORAGE_WALK_BUDGET).await {
                Ok(f) => tracing::info!(figures = ?f, "storage measured"),
                // A failed walk keeps yesterday's figures and tries again tomorrow.
                Err(e) => tracing::warn!(error = %e, "storage measurement failed"),
            }
            jobs::reschedule_running(
                &pool,
                job.id,
                jobs::delay_until_daily_utc(now_ms(), STORAGE_HOUR_UTC, STORAGE_MINUTE_UTC),
            )
            .await?;
            Ok(())
        })
    })
}

/// Register the `storage.cors_recheck` handler; pair with [`schedule`].
pub fn register_cors_recheck(
    runner: JobRunner,
    pool: &PgPool,
    repo: Arc<dyn sr_storage::mounts::MountRepo>,
    sealer: Arc<dyn sr_storage::mounts::Sealer>,
    origin: String,
    guard: Arc<dyn sr_storage::guard::EndpointGuard>,
) -> JobRunner {
    let pool = pool.clone();
    runner.register(CORS_RECHECK_KIND, 1, move |job: Job| {
        let (pool, repo, sealer, origin, guard) = (
            pool.clone(),
            repo.clone(),
            sealer.clone(),
            origin.clone(),
            guard.clone(),
        );
        Box::pin(async move {
            if let Ok(origin) = sr_storage::s3admin::normalize_origin(&origin) {
                let iso = |ms: i64| sr_api::routes::developer::iso(ms);
                match sr_storage::recheck::recheck_all_guarded(
                    repo.as_ref(),
                    sealer.as_ref(),
                    &origin,
                    now_ms(),
                    &iso,
                    Some(guard.as_ref()),
                )
                .await
                {
                    Ok(done) => {
                        let bad = done
                            .iter()
                            .filter(|d| {
                                !matches!(d.result, Ok(sr_storage::cors_check::CorsState::Ok))
                            })
                            .count();
                        tracing::info!(
                            mounts = done.len(),
                            not_ok = bad,
                            "storage mounts CORS rechecked"
                        );
                    }
                    Err(e) => tracing::warn!(error = %e, "storage mounts CORS recheck failed"),
                }
            } else {
                tracing::warn!("server.public_url is not an origin; CORS recheck skipped");
            }
            jobs::reschedule_running(
                &pool,
                job.id,
                jobs::delay_until_daily_utc(now_ms(), CORS_HOUR_UTC, CORS_MINUTE_UTC),
            )
            .await?;
            Ok(())
        })
    })
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

    /// RDY-OBS-002: the new job id and kind are not used by any other job, and its setting key is not used by any other
    /// setting. Reads the sources of every crate, so a second `= 133;` anywhere fails here.
    #[test]
    fn the_storage_job_id_kind_and_setting_key_are_unique() {
        fn walk(dir: &Path, out: &mut Vec<(String, String)>) {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    out.push((
                        p.display().to_string(),
                        std::fs::read_to_string(&p).unwrap(),
                    ));
                }
            }
        }
        let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut files = Vec::new();
        for c in std::fs::read_dir(&crates).unwrap().flatten() {
            let src = c.path().join("src");
            if src.is_dir() {
                walk(&src, &mut files);
            }
        }
        // Every `const NAME_JOB_ID: i64 = N;` in the repository: no two share a number.
        let mut ids: Vec<(i64, String)> = Vec::new();
        for (path, text) in &files {
            for line in text.lines() {
                let l = line.trim_start();
                if let Some(rest) = l
                    .strip_prefix("pub const ")
                    .or_else(|| l.strip_prefix("const "))
                    && let Some((name, value)) = rest.split_once(": i64 = ")
                    && name.ends_with("JOB_ID")
                    && let Ok(n) = value.trim_end_matches(';').trim().parse::<i64>()
                {
                    ids.push((n, format!("{name} in {path}")));
                }
            }
        }
        assert!(ids.len() > 20, "found only {} job ids", ids.len());
        // Nothing else uses this id (two crates mirror one id on purpose elsewhere, so only ours is checked).
        let ours: Vec<_> = ids
            .iter()
            .filter(|(n, _)| *n == STORAGE_MEASURE_JOB_ID)
            .collect();
        assert_eq!(ours.len(), 1, "{ours:?}");
        assert!(ours[0].1.starts_with("STORAGE_MEASURE_JOB_ID"));
        // The kind string is registered once.
        let kind = format!("\"{STORAGE_MEASURE_KIND}\"");
        let uses: usize = files
            .iter()
            .map(|(_, t)| {
                t.lines()
                    .filter(|l| l.contains("_KIND: &str = ") && l.contains(&kind))
                    .count()
            })
            .sum();
        assert_eq!(uses, 1, "{STORAGE_MEASURE_KIND} is declared once");
        // The CORS recheck (BYOS-MNT-003): its id and kind are declared once too.
        let ours: Vec<_> = ids
            .iter()
            .filter(|(n, _)| *n == CORS_RECHECK_JOB_ID)
            .collect();
        assert_eq!(ours.len(), 1, "{ours:?}");
        let kind = format!("\"{CORS_RECHECK_KIND}\"");
        let uses: usize = files
            .iter()
            .map(|(_, t)| {
                t.lines()
                    .filter(|l| l.contains("_KIND: &str = ") && l.contains(&kind))
                    .count()
            })
            .sum();
        assert_eq!(uses, 1, "{CORS_RECHECK_KIND} is declared once");
        // The setting key is in the catalog once and the lookup finds it.
        let key = sr_admin::settings::OPS_STORAGE_MEASURE_MAX_OBJECTS;
        assert_eq!(
            sr_admin::settings::CATALOG
                .iter()
                .filter(|d| d.key == key)
                .count(),
            1
        );
        assert!(sr_admin::settings::find(key).is_some());
        // Only the owner of the key writes it out literally.
        let literal = format!("\"{key}\"");
        let owners = files
            .iter()
            .filter(|(p, t)| {
                !p.contains("admin_jobs.rs")
                    && t.contains(&format!(
                        "pub const OPS_STORAGE_MEASURE_MAX_OBJECTS: &str = {literal}"
                    ))
            })
            .count();
        assert_eq!(owners, 1);
    }
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
