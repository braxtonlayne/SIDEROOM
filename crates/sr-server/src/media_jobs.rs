//! Cinema media background work and its Postgres collaborators: the `cinema.probe` job queued when
//! an upload completes, the hourly `cinema.media.sweep` (retention, expired slots and downloads,
//! unreferenced transcode prefixes), the `cinema.transcode.reap` job that requeues jobs whose
//! worker lease ran out, the operator settings, and the playlist renderer behind `/cdn/hls`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use sr_api::routes::cdn::{PlaylistRenderer, RenderedPlaylist};
use sr_db::{
    DbResult, PgPool,
    jobs::{self, Job, NewJob},
    media_repo::PgMediaRepo,
    runner::{HandlerError, JobRunner},
};
use sr_media::assets::MediaRepository;
use sr_media::downloads::DownloadService;
use sr_media::playlist::UrlSigner;
use sr_media::service::{MediaService, MediaSettings, MediaSettingsSource, ProbeQueue};
use sr_media::worker_api::JobStore;

pub const PROBE_KIND: &str = "cinema.probe";
pub const SWEEP_KIND: &str = "cinema.media.sweep";
pub const REAP_KIND: &str = "cinema.transcode.reap";
pub const SWEEP_JOB_ID: i64 = 160;
pub const REAP_JOB_ID: i64 = 161;
const SWEEP_INTERVAL: Duration = Duration::from_secs(3600);
const REAP_INTERVAL: Duration = Duration::from_secs(30);
/// Attempts before an asset's probe is given up (the asset is then marked failed).
const PROBE_ATTEMPTS: i32 = 3;
/// Probes at once (each spools a whole original to disk).
const PROBE_CONCURRENCY: usize = 2;
/// Attempts a transcode job gets before it fails for good.
pub const MAX_JOB_ATTEMPTS: u32 = 3;

pub const MAX_UPLOAD_GB_SETTING: &str = "cinema.upload.max_gb";
const DEFAULT_MAX_GB: u64 = 60;

/// Operator settings: the upload limit from `platform_settings`; the `ffprobe` executable
/// (`SR_FFPROBE`, default `ffprobe`) and the spool directory (`SR_MEDIA_SPOOL`, default the OS temp
/// directory) from the environment, because they describe this machine.
pub struct PgMediaSettings {
    pool: PgPool,
    ffprobe: String,
    spool: PathBuf,
}

impl PgMediaSettings {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            ffprobe: std::env::var("SR_FFPROBE").unwrap_or_else(|_| "ffprobe".to_owned()),
            spool: std::env::var("SR_MEDIA_SPOOL")
                .map(PathBuf::from)
                .unwrap_or_else(|_| std::env::temp_dir().join("sideroom-probe")),
        }
    }
}

/// The limit from the stored value; garbage never loosens it.
pub fn max_upload_bytes(stored: Option<&Value>) -> u64 {
    let gb = stored
        .and_then(Value::as_u64)
        .filter(|g| (1..=2000).contains(g))
        .unwrap_or(DEFAULT_MAX_GB);
    gb * 1024 * 1024 * 1024
}

#[async_trait]
impl MediaSettingsSource for PgMediaSettings {
    async fn snapshot(&self) -> MediaSettings {
        let stored = sqlx::query_scalar::<_, Value>(
            "SELECT value FROM platform_settings WHERE key = $1",
        )
        .bind(MAX_UPLOAD_GB_SETTING)
        .fetch_optional(&self.pool)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "reading a cinema setting failed; using the default");
            None
        });
        MediaSettings {
            max_upload_bytes: max_upload_bytes(stored.as_ref()),
            ffprobe: self.ffprobe.clone(),
            spool_dir: self.spool.clone(),
            ..MediaSettings::default()
        }
    }
}

/// Queues `cinema.probe` on the durable job queue.
pub struct PgProbeQueue {
    pool: PgPool,
    ids: Arc<dyn Fn() -> i64 + Send + Sync>,
}

impl PgProbeQueue {
    pub fn new(pool: PgPool, ids: Arc<dyn Fn() -> i64 + Send + Sync>) -> Self {
        Self { pool, ids }
    }
}

#[async_trait]
impl ProbeQueue for PgProbeQueue {
    async fn enqueue_probe(&self, asset_id: i64) -> Result<(), String> {
        let mut job = NewJob::new((self.ids)(), PROBE_KIND, json!({ "asset": asset_id }));
        job.max_attempts = PROBE_ATTEMPTS;
        jobs::enqueue(&self.pool, &job)
            .await
            .map_err(|e| e.to_string())
    }
}

fn handler_err(e: impl std::fmt::Display) -> HandlerError {
    Box::new(std::io::Error::other(e.to_string()))
}

async fn probe_handler(media: Arc<MediaService>, job: Job) -> Result<(), HandlerError> {
    let Some(asset) = job.payload.get("asset").and_then(Value::as_i64) else {
        return Ok(());
    };
    let final_attempt = job.attempts >= job.max_attempts;
    media
        .run_probe(asset, final_attempt)
        .await
        .map_err(handler_err)
}

async fn sweep_handler(
    pool: PgPool,
    media: Arc<MediaService>,
    downloads: Arc<DownloadService>,
    job: Job,
) -> Result<(), HandlerError> {
    let report = media.sweep().await.map_err(handler_err)?;
    let expired = downloads.sweep().await.map_err(handler_err)?;
    if report != Default::default() || expired > 0 {
        tracing::info!(?report, expired_downloads = expired, "cinema media swept");
    }
    jobs::reschedule_running(&pool, job.id, SWEEP_INTERVAL).await?;
    Ok(())
}

async fn reap_handler(pool: PgPool, repo: Arc<PgMediaRepo>, job: Job) -> Result<(), HandlerError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let hit = repo
        .reap_expired(now, MAX_JOB_ATTEMPTS)
        .await
        .map_err(handler_err)?;
    if !hit.is_empty() {
        tracing::warn!(jobs = ?hit, "transcode jobs lost their worker lease");
    }
    jobs::reschedule_running(&pool, job.id, REAP_INTERVAL).await?;
    Ok(())
}

/// Register the three jobs on `runner`. Pair with [`schedule`].
pub fn register(
    runner: JobRunner,
    pool: &PgPool,
    repo: Arc<PgMediaRepo>,
    media: Arc<MediaService>,
    downloads: Arc<DownloadService>,
) -> JobRunner {
    let (sweep_pool, reap_pool) = (pool.clone(), pool.clone());
    let probe_media = media.clone();
    runner
        .register(PROBE_KIND, PROBE_CONCURRENCY, move |job| {
            Box::pin(probe_handler(probe_media.clone(), job))
        })
        .register(SWEEP_KIND, 1, move |job| {
            Box::pin(sweep_handler(
                sweep_pool.clone(),
                media.clone(),
                downloads.clone(),
                job,
            ))
        })
        .register(REAP_KIND, 1, move |job| {
            Box::pin(reap_handler(reap_pool.clone(), repo.clone(), job))
        })
}

/// Create or re-arm the periodic singletons. Call only when [`register`] is on the runner.
pub async fn schedule(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(pool, SWEEP_JOB_ID, SWEEP_KIND, &json!({}), SWEEP_INTERVAL).await?;
    jobs::schedule_singleton(pool, REAP_JOB_ID, REAP_KIND, &json!({}), REAP_INTERVAL).await?;
    Ok(())
}

/// Renders the HLS playlists, the subtitle wrappers and the sprite map behind
/// `/cdn/hls/<job>/<attempt>/...` from the database, signing every URL like the playback grant.
pub struct PgPlaylistRenderer {
    repo: Arc<PgMediaRepo>,
    signer: Arc<dyn UrlSigner>,
}

impl PgPlaylistRenderer {
    pub fn new(repo: Arc<PgMediaRepo>, signer: Arc<dyn UrlSigner>) -> Self {
        Self { repo, signer }
    }
}

/// Split `transcode/<job>/<attempt>/<rest>` and say whether `rest` can be a rendered file (so the
/// segments, which are most of the traffic, never cost a query).
fn split_key(key: &str) -> Option<(String, &str)> {
    let mut it = key.splitn(4, '/');
    let (root, job, attempt, rest) = (it.next()?, it.next()?, it.next()?, it.next()?);
    if root != "transcode" || job.parse::<u64>().is_err() || attempt.parse::<u32>().is_err() {
        return None;
    }
    let renderable = rest.ends_with(".m3u8") || rest == "sprites/sprites.vtt";
    renderable.then(|| (format!("transcode/{job}/{attempt}"), rest))
}

#[async_trait]
impl PlaylistRenderer for PgPlaylistRenderer {
    async fn render(&self, key: &str, now: u64) -> Option<RenderedPlaylist> {
        let (prefix, rest) = split_key(key)?;
        let pb = match self.repo.playback_by_prefix(&prefix).await {
            Ok(Some(pb)) => pb,
            Ok(None) => return None,
            Err(e) => {
                tracing::error!(error = %e, "loading playback data for a playlist failed");
                return None;
            }
        };
        sr_media::playback::render_file(&pb, rest, self.signer.as_ref(), now).map(|f| {
            RenderedPlaylist {
                content_type: f.content_type,
                body: f.body,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_playlists_and_the_sprite_map_are_rendered() {
        assert_eq!(
            split_key("transcode/9/1/master.m3u8"),
            Some(("transcode/9/1".to_owned(), "master.m3u8"))
        );
        assert_eq!(
            split_key("transcode/9/1/v/1080p/index.m3u8").map(|s| s.1.to_owned()),
            Some("v/1080p/index.m3u8".to_owned())
        );
        assert!(split_key("transcode/9/1/sprites/sprites.vtt").is_some());
        assert!(split_key("transcode/9/1/v/1080p/seg_00001.m4s").is_none());
        assert!(split_key("transcode/9/1/subs/00-eng.vtt").is_none());
        assert!(split_key("transcode/x/1/master.m3u8").is_none());
        assert!(split_key("originals/9/1/master.m3u8").is_none());
    }

    #[test]
    fn the_upload_limit_is_bounded() {
        let gib = 1024 * 1024 * 1024;
        assert_eq!(max_upload_bytes(None), 60 * gib);
        assert_eq!(max_upload_bytes(Some(&json!(100))), 100 * gib);
        assert_eq!(max_upload_bytes(Some(&json!(0))), 60 * gib);
        assert_eq!(max_upload_bytes(Some(&json!(99_999))), 60 * gib);
        assert_eq!(max_upload_bytes(Some(&json!("big"))), 60 * gib);
    }

    #[test]
    fn the_setting_is_in_the_admin_catalog() {
        let s = sr_admin::settings::find(MAX_UPLOAD_GB_SETTING).expect("declared");
        assert_eq!(s.default_value(), json!(60));
        assert!(s.validate(&json!(0)).is_err());
        assert!(s.validate(&json!(120)).is_ok());
    }
}
