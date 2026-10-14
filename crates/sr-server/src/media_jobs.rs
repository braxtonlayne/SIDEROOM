//! Cinema media background work and its Postgres collaborators: the `cinema.probe` job queued when
//! an upload completes, the hourly `cinema.media.sweep` (retention, expired slots and downloads,
//! unreferenced transcode prefixes), the `cinema.transcode.reap` job that requeues jobs whose
//! worker lease ran out, the operator settings, and the Postgres playback source behind `/cdn/hls`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use sr_db::{
    DbResult, PgPool,
    jobs::{self, Job, NewJob},
    media_repo::PgMediaRepo,
    runner::{HandlerError, JobRunner},
};
use sr_media::assets::MediaRepository;
use sr_media::downloads::DownloadService;
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

/// The Postgres source of what the player's playback endpoints and the CDN playlist renderer need: the ready
/// asset's renditions, mapped from the media repository's playable form. The asset is addressed by id in URLs
/// (`/cdn/hls/<asset>/...`); `storage_prefix` says where the worker left its files.
pub struct PgPlaybackSource {
    repo: Arc<PgMediaRepo>,
}

impl PgPlaybackSource {
    pub fn new(repo: Arc<PgMediaRepo>) -> Self {
        Self { repo }
    }
}

/// Map the repository's playable form to the playback model.
fn to_playback_asset(pb: sr_media::assets::AssetPlayback) -> sr_media::playback::PlaybackAsset {
    use sr_media::playback::{PlaybackAsset, SubtitleExtra};
    let subtitle_extras = pb
        .media
        .subtitles
        .iter()
        .enumerate()
        .map(|(i, s)| SubtitleExtra {
            vtt_path: s.vtt_path.clone(),
            id: i.to_string(),
            ass_path: pb
                .ass_paths
                .iter()
                .find(|(vtt, _)| *vtt == s.vtt_path)
                .map(|(_, ass)| ass.clone()),
        })
        .collect();
    PlaybackAsset {
        id: pb.asset_id,
        storage_prefix: pb.base_path.trim_start_matches("/cdn/").to_owned(),
        duration_ms: Some((pb.duration_secs * 1000.0).round() as i64),
        media: pb.media,
        subtitle_extras,
        sprite: pb.sprite,
    }
}

#[async_trait]
impl sr_media::playback::PlaybackSource for PgPlaybackSource {
    async fn asset(
        &self,
        asset_id: i64,
    ) -> Result<Option<sr_media::playback::PlaybackAsset>, sr_media::playback::PlaybackError> {
        self.repo
            .playback(asset_id)
            .await
            .map(|o| o.map(to_playback_asset))
            .map_err(|e| sr_media::playback::PlaybackError::Storage(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_storage_prefix_is_the_cdn_base_without_the_cdn_root() {
        let pb = sr_media::assets::AssetPlayback {
            asset_id: 9,
            base_path: "/cdn/transcode/9/1".to_owned(),
            duration_secs: 18.5,
            profile_version: 1,
            media: sr_media::playlist::MediaSet::default(),
            sprite: None,
            ass_paths: vec![],
        };
        let a = to_playback_asset(pb);
        assert_eq!(a.storage_prefix, "transcode/9/1");
        assert_eq!(a.duration_ms, Some(18_500));
    }
}
