//! The staff side of the media pipeline: the upload path for cinema originals, the probe and
//! planning step, worker tokens, the queue, re-transcodes and the retention sweep
//! (CIN-ING-001..004, 009, 011).
//!
//! Flow of an original: [`MediaService::begin_upload`] reserves an asset and hands out presigned
//! multipart `PUT` URLs, the browser sends the file straight to storage,
//! [`MediaService::complete_upload`] checks the object and queues the `cinema.probe` job, and
//! [`MediaService::run_probe`] (the job handler) streams the original to a spool file, runs
//! `ffprobe` through the [`CommandRunner`], plans the review proxy and the ladder and enqueues
//! them for the workers. Everything fails with words an operator can act on, including a server
//! without `ffprobe`.
//!
//! Permission checks belong to the HTTP layer (`PUBLISH_MEDIA` for uploads, `MANAGE_TRANSCODE`
//! for the queue and tokens); the service takes plain user ids.

use crate::assets::*;
use crate::plan::{TranscodeSettings, plan_ladder_job, plan_proxy_job};
use crate::probe::{ProbeError, ProbeResult, probe_file};
use crate::retention::{self, RetentionPolicy};
use crate::retranscode::{JobPriority, RetranscodePlan, RetranscodeRequest, RetranscodeScope};
use crate::runner::CommandRunner;
use crate::worker_api::{
    JobSpec, MAX_PARTS, NewJob, OutputStorage, PartDone, PartUrl, TokenScope, token_hash,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sr_storage::ObjectStorage;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use utoipa::ToSchema;

/// Containers the upload path accepts (the extension is a hint; ffprobe has the last word).
pub const ALLOWED_EXTENSIONS: [&str; 14] = [
    "mkv", "mp4", "m4v", "mov", "avi", "webm", "ts", "m2ts", "mts", "wmv", "flv", "mpg", "mpeg",
    "ogv",
];
/// Size of one presigned upload part.
pub const UPLOAD_PART_SIZE: u64 = 128 * 1024 * 1024;
/// How long a presigned upload URL works.
pub const UPLOAD_URL_TTL_SECS: u64 = 6 * 3600;
/// An upload slot that was never completed expires after this long.
pub const SLOT_EXPIRY_MS: i64 = 24 * 60 * 60 * 1000;
/// Priorities of the jobs a probe enqueues (the review proxy goes first).
pub const PROXY_PRIORITY: u8 = 9;
pub const LADDER_PRIORITY: u8 = 5;
/// A member's conversion in their own bucket waits behind the instance's own jobs (BYOS-CNV-003).
pub const MOUNT_PRIORITY: u8 = 4;

pub const KIND_PROBE_JOB: &str = "cinema.probe";

/// Operator settings, read afresh for every upload and probe.
#[derive(Debug, Clone)]
pub struct MediaSettings {
    /// Largest original accepted (`cinema.upload.max_gb`).
    pub max_upload_bytes: u64,
    /// The `ffprobe` executable (`cinema.ffprobe_path`).
    pub ffprobe: String,
    /// Where originals are spooled for probing. Needs room for the largest original.
    pub spool_dir: PathBuf,
    pub retention: RetentionPolicy,
    pub transcode: TranscodeSettings,
}

impl Default for MediaSettings {
    fn default() -> Self {
        Self {
            max_upload_bytes: 60 * 1024 * 1024 * 1024,
            ffprobe: "ffprobe".to_owned(),
            spool_dir: std::env::temp_dir().join("sideroom-probe"),
            retention: RetentionPolicy::default(),
            transcode: TranscodeSettings::default(),
        }
    }
}

#[async_trait]
pub trait MediaSettingsSource: Send + Sync {
    async fn snapshot(&self) -> MediaSettings;
}

pub struct StaticMediaSettings(pub MediaSettings);

#[async_trait]
impl MediaSettingsSource for StaticMediaSettings {
    async fn snapshot(&self) -> MediaSettings {
        self.0.clone()
    }
}

/// Queues the `cinema.probe` job on the durable job queue.
#[async_trait]
pub trait ProbeQueue: Send + Sync {
    async fn enqueue_probe(&self, asset_id: i64) -> Result<(), String>;
}

pub type IdSource = Arc<dyn Fn() -> i64 + Send + Sync>;
/// Seconds since the Unix epoch.
pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;
/// 32 random bytes, or `None` when the system source failed.
pub type Random32 = Arc<dyn Fn() -> Option<[u8; 32]> + Send + Sync>;

// ---- wire types ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct UploadRequest {
    /// The file name on the member's disk; only its extension is used.
    pub filename: String,
    pub size_bytes: u64,
    /// The catalog item the video is for. Linking it as the playable asset is a separate step.
    #[serde(default, with = "ids::opt_id")]
    #[schema(value_type = Option<String>)]
    pub title_id: Option<i64>,
    #[serde(default, with = "ids::opt_id")]
    #[schema(value_type = Option<String>)]
    pub episode_id: Option<i64>,
    /// Queue the ladder right after the probe (default true for staff uploads).
    #[serde(default)]
    pub auto_ladder: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UploadSlot {
    pub asset: Asset,
    pub upload_id: String,
    pub part_size: u64,
    /// One presigned `PUT` per part (a file no larger than `part_size` has one). Send each part
    /// with `PUT` and keep the `ETag` response header.
    pub parts: Vec<PartUrl>,
    pub content_type: String,
    pub expires_in_secs: u64,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CompleteUploadRequest {
    pub upload_id: String,
    pub parts: Vec<PartDone>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CreatedToken {
    pub token: WorkerToken,
    /// The secret. It is shown once and only its SHA-256 is stored.
    pub secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AssetStatus {
    pub asset: Asset,
    pub jobs: Vec<JobSummary>,
    /// The probe, summarised for the editor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probe: Option<ProbeSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProbeSummary {
    pub container: String,
    pub duration_secs: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub video_codec: Option<String>,
    pub hdr: Option<String>,
    pub interlaced: bool,
    pub audio_languages: Vec<String>,
    pub subtitle_count: u32,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RetranscodeOutcome {
    #[serde(rename = "enqueued")]
    pub enqueued: Vec<String>,
    pub skipped: Vec<RetranscodeSkip>,
    pub estimated_gpu_secs: f64,
    pub truncated: bool,
    pub failed: Vec<RetranscodeSkip>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RetranscodeSkip {
    #[serde(with = "ids::id")]
    #[schema(value_type = String)]
    pub asset_id: i64,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SweepReport {
    pub originals_deleted: u32,
    pub proxies_deleted: u32,
    /// Cold-tier moves need a second bucket, which is not configured: reported, not done.
    pub cold_skipped: u32,
    pub slots_expired: u32,
    pub prefixes_collected: u32,
    pub bytes_freed: u64,
    pub failed: u32,
}

// ---- the service --------------------------------------------------------------------------

#[derive(Clone)]
pub struct MediaService {
    pub repo: Arc<dyn MediaRepository>,
    storage: Arc<dyn ObjectStorage>,
    outputs: Arc<dyn OutputStorage>,
    runner: Arc<dyn CommandRunner>,
    settings: Arc<dyn MediaSettingsSource>,
    queue: Arc<dyn ProbeQueue>,
    ids: IdSource,
    clock: Clock,
    random: Random32,
    part_size: u64,
}

fn ext_of(filename: &str) -> Option<String> {
    let base = filename.rsplit(['/', '\\']).next().unwrap_or(filename);
    let (stem, ext) = base.rsplit_once('.')?;
    let ext = ext.to_ascii_lowercase();
    (!stem.is_empty() && ALLOWED_EXTENSIONS.contains(&ext.as_str())).then_some(ext)
}

fn clean_filename(filename: &str) -> String {
    let base = filename.rsplit(['/', '\\']).next().unwrap_or(filename);
    base.chars().filter(|c| !c.is_control()).take(255).collect()
}

fn content_type_of(ext: &str) -> &'static str {
    match ext {
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        _ => "application/octet-stream",
    }
}

fn priority_value(p: JobPriority) -> u8 {
    match p {
        JobPriority::Low => 2,
        JobPriority::Normal => LADDER_PRIORITY,
        JobPriority::High => 8,
    }
}

fn storage_err(e: impl std::fmt::Display) -> MediaError {
    MediaError::Storage(e.to_string())
}

impl MediaService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repo: Arc<dyn MediaRepository>,
        storage: Arc<dyn ObjectStorage>,
        outputs: Arc<dyn OutputStorage>,
        runner: Arc<dyn CommandRunner>,
        settings: Arc<dyn MediaSettingsSource>,
        queue: Arc<dyn ProbeQueue>,
        ids: IdSource,
        clock: Clock,
        random: Random32,
    ) -> Self {
        Self {
            repo,
            storage,
            outputs,
            runner,
            settings,
            queue,
            ids,
            clock,
            random,
            part_size: UPLOAD_PART_SIZE,
        }
    }

    /// A different part size (storage that cannot presign parts uses one part for everything).
    #[must_use]
    pub fn with_part_size(mut self, part_size: u64) -> Self {
        self.part_size = part_size.max(1);
        self
    }

    fn now(&self) -> u64 {
        (self.clock)()
    }

    fn now_ms(&self) -> i64 {
        i64::try_from(self.now().saturating_mul(1000)).unwrap_or(i64::MAX)
    }

    // ---- uploads ----

    /// Reserve an asset and presign the upload of its original.
    pub async fn begin_upload(&self, user: i64, req: UploadRequest) -> MediaResult<UploadSlot> {
        let settings = self.settings.snapshot().await;
        let ext = ext_of(&req.filename).ok_or_else(|| {
            MediaError::Invalid(format!(
                "unsupported file type; use one of: {}",
                ALLOWED_EXTENSIONS.join(", ")
            ))
        })?;
        if req.size_bytes == 0 {
            return Err(MediaError::Invalid("the file is empty".into()));
        }
        if req.size_bytes > settings.max_upload_bytes {
            return Err(MediaError::Invalid(format!(
                "the file is larger than the {} GiB limit",
                settings.max_upload_bytes / (1024 * 1024 * 1024)
            )));
        }
        for id in [req.title_id, req.episode_id].into_iter().flatten() {
            if id <= 0 {
                return Err(MediaError::Invalid("ids must be positive".into()));
            }
        }
        let size = i64::try_from(req.size_bytes)
            .map_err(|_| MediaError::Invalid("the file is too large".into()))?;
        let n_parts = req.size_bytes.div_ceil(self.part_size);
        if n_parts > MAX_PARTS {
            return Err(MediaError::Invalid("the file is too large".into()));
        }
        let id = (self.ids)();
        let key = format!("originals/{id}/source.{ext}");
        let asset = self
            .repo
            .create_asset(&NewAsset {
                id,
                title_id: req.title_id,
                episode_id: req.episode_id,
                uploaded_by: Some(user),
                filename: clean_filename(&req.filename),
                original_key: key.clone(),
                original_ext: ext.clone(),
                original_size: size,
                auto_ladder: req.auto_ladder.unwrap_or(true),
            })
            .await?;
        let content_type = content_type_of(&ext);
        let started = async {
            let upload_id = self
                .outputs
                .init_multipart(&key, content_type)
                .await
                .map_err(storage_err)?;
            let mut parts = Vec::new();
            for n in 1..=n_parts {
                let part_number = u32::try_from(n).map_err(|_| storage_err("part number"))?;
                let url = self
                    .outputs
                    .presign_part(&key, &upload_id, part_number, UPLOAD_URL_TTL_SECS)
                    .await
                    .map_err(storage_err)?;
                parts.push(PartUrl { part_number, url });
            }
            Ok::<_, MediaError>((upload_id, parts))
        }
        .await;
        match started {
            Ok((upload_id, parts)) => Ok(UploadSlot {
                asset,
                upload_id,
                part_size: self.part_size,
                parts,
                content_type: content_type.to_owned(),
                expires_in_secs: UPLOAD_URL_TTL_SECS,
            }),
            Err(e) => {
                // Do not leave a slot behind that can never be uploaded.
                let _ = self
                    .repo
                    .transition_asset(
                        id,
                        &[AssetState::AwaitingUpload],
                        AssetState::Expired,
                        Some("the storage could not start the upload"),
                    )
                    .await;
                tracing::error!(error = %e, "cinema upload could not be presigned");
                Err(MediaError::Unavailable(
                    "object storage did not start the upload; check the S3 endpoint and credentials"
                        .into(),
                ))
            }
        }
    }

    /// Finish the multipart upload, check the object and queue the probe. Repeating the call
    /// for an asset that is past the upload returns it unchanged.
    pub async fn complete_upload(&self, id: i64, req: CompleteUploadRequest) -> MediaResult<Asset> {
        let asset = self
            .repo
            .get_asset(id)
            .await?
            .ok_or(MediaError::NotFound("asset"))?;
        match asset.state {
            AssetState::AwaitingUpload => {}
            AssetState::Expired => {
                return Err(MediaError::Conflict(
                    "the upload slot expired; start a new upload".into(),
                ));
            }
            _ => return Ok(asset),
        }
        if req.parts.is_empty() {
            return Err(MediaError::Invalid("no parts were sent".into()));
        }
        self.outputs
            .complete_multipart(&asset.original_key, &req.upload_id, &req.parts)
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, asset = id, "completing the multipart upload failed");
                MediaError::Invalid("the storage did not accept the uploaded parts".into())
            })?;
        let settings = self.settings.snapshot().await;
        let head = self
            .storage
            .head(&asset.original_key)
            .await
            .map_err(|_| MediaError::Invalid("the uploaded file is not in storage".into()))?;
        let declared = u64::try_from(asset.original_size).unwrap_or(0);
        let problem = if head.size == 0 {
            Some("the uploaded file is empty")
        } else if head.size > settings.max_upload_bytes {
            Some("the uploaded file is over the size limit")
        } else if head.size != declared {
            Some("the uploaded file is not the size that was announced")
        } else {
            None
        };
        if let Some(p) = problem {
            let _ = self.storage.delete(&asset.original_key).await;
            let _ = self
                .repo
                .transition_asset(
                    id,
                    &[AssetState::AwaitingUpload],
                    AssetState::Failed,
                    Some(p),
                )
                .await;
            return Err(MediaError::Invalid(p.into()));
        }
        let asset = self
            .repo
            .mark_uploaded(id, i64::try_from(head.size).unwrap_or(i64::MAX))
            .await?
            .ok_or_else(|| MediaError::Conflict("the asset changed state".into()))?;
        if let Err(e) = self.queue.enqueue_probe(id).await {
            // The asset stays `uploaded`; the re-probe action queues it again.
            tracing::error!(error = %e, asset = id, "could not queue the probe job");
        }
        Ok(asset)
    }

    /// Queue the probe again for an asset whose probe failed (for example after installing
    /// ffprobe) or never ran.
    pub async fn reprobe(&self, id: i64) -> MediaResult<Asset> {
        let asset = self.get(id).await?;
        if asset.original_deleted {
            return Err(MediaError::Conflict(
                "the original was deleted; upload it again".into(),
            ));
        }
        let moved = self
            .repo
            .transition_asset(
                id,
                &[
                    AssetState::Failed,
                    AssetState::Uploaded,
                    AssetState::Probing,
                ],
                AssetState::Uploaded,
                None,
            )
            .await?
            .ok_or_else(|| MediaError::Conflict("this asset cannot be probed again".into()))?;
        self.queue
            .enqueue_probe(id)
            .await
            .map_err(MediaError::Backend)?;
        Ok(moved)
    }

    pub async fn get(&self, id: i64) -> MediaResult<Asset> {
        self.repo
            .get_asset(id)
            .await?
            .ok_or(MediaError::NotFound("asset"))
    }

    /// The asset, its jobs and the probe summary (status polling).
    pub async fn status(&self, id: i64) -> MediaResult<AssetStatus> {
        let asset = self.get(id).await?;
        let jobs = self
            .repo
            .list_jobs(&JobQuery {
                asset_id: Some(id),
                limit: 20,
                ..JobQuery::default()
            })
            .await?;
        let probe = self
            .repo
            .asset_probe(id)
            .await?
            .and_then(|v| serde_json::from_value::<ProbeResult>(v).ok())
            .map(|p| summarise(&p));
        Ok(AssetStatus { asset, jobs, probe })
    }

    pub async fn list_assets(&self, q: AssetQuery) -> MediaResult<Vec<Asset>> {
        self.repo.list_assets(&q).await
    }

    /// Link the asset to a catalog item (the catalog service sets the playable reference).
    pub async fn link(
        &self,
        id: i64,
        title: Option<i64>,
        episode: Option<i64>,
    ) -> MediaResult<Asset> {
        self.repo
            .link_asset(id, title, episode)
            .await?
            .ok_or(MediaError::NotFound("asset"))
    }

    pub async fn set_original_downloads(&self, id: i64, on: bool) -> MediaResult<Asset> {
        self.repo
            .set_original_downloads(id, on)
            .await?
            .ok_or(MediaError::NotFound("asset"))
    }

    // ---- probe and planning ----

    /// The `cinema.probe` job: spool the original, run ffprobe, store the result and enqueue
    /// the proxy (and the ladder for `auto_ladder` assets). `Err` asks the job runner to retry;
    /// problems with the file or the tools are written to the asset instead and end the job.
    pub async fn run_probe(&self, id: i64, final_attempt: bool) -> MediaResult<()> {
        let Some(asset) = self.repo.get_asset(id).await? else {
            return Ok(());
        };
        if !matches!(asset.state, AssetState::Uploaded | AssetState::Probing) {
            return Ok(());
        }
        if asset.original_deleted {
            return Ok(());
        }
        self.repo
            .transition_asset(
                id,
                &[AssetState::Uploaded, AssetState::Probing],
                AssetState::Probing,
                None,
            )
            .await?;
        let settings = self.settings.snapshot().await;
        let spool = match self.spool(&asset, &settings).await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(error = %e, asset = id, "spooling the original for the probe failed");
                if final_attempt {
                    self.fail(id, "the original could not be read back from storage")
                        .await;
                }
                return Err(e);
            }
        };
        let runner = Arc::clone(&self.runner);
        let ffprobe = settings.ffprobe.clone();
        let path = spool.to_string_lossy().replace('\\', "/");
        let result = tokio::task::spawn_blocking(move || probe_file(&*runner, &ffprobe, &path))
            .await
            .map_err(|e| MediaError::Backend(e.to_string()))?;
        let _ = tokio::fs::remove_file(&spool).await;
        let probe = match result {
            Ok(p) => p,
            Err(e) => {
                self.fail(id, &probe_failure(&e, &settings.ffprobe)).await;
                return Ok(());
            }
        };
        self.after_probe(id, &asset, &probe, &settings).await
    }

    async fn after_probe(
        &self,
        id: i64,
        asset: &Asset,
        probe: &ProbeResult,
        settings: &MediaSettings,
    ) -> MediaResult<()> {
        let duration_ms = probe
            .format
            .duration_secs
            .map_or(0, |d| (d * 1000.0).round() as i64);
        let value = serde_json::to_value(probe).map_err(|e| MediaError::Backend(e.to_string()))?;
        self.repo.record_probe(id, &value, duration_ms).await?;
        let proxy = match plan_proxy_job(probe, &settings.transcode) {
            Ok(p) => p,
            Err(e) => {
                self.fail(id, &format!("this video cannot be transcoded: {e}"))
                    .await;
                return Ok(());
            }
        };
        self.enqueue_spec(
            asset,
            JobSpec::Proxy {
                plan: Box::new(proxy),
            },
            PROXY_PRIORITY,
        )
        .await?;
        if asset.auto_ladder {
            self.enqueue_ladder(asset, probe, settings, LADDER_PRIORITY)
                .await?;
            self.repo
                .transition_asset(id, &[AssetState::Probing], AssetState::Transcoding, None)
                .await?;
        }
        Ok(())
    }

    async fn enqueue_spec(
        &self,
        asset: &Asset,
        spec: JobSpec,
        priority: u8,
    ) -> MediaResult<String> {
        Ok(self
            .repo
            .enqueue(NewJob {
                asset_id: u64::try_from(asset.id).unwrap_or_default(),
                spec,
                source_key: asset.original_key.clone(),
                source_size: u64::try_from(asset.original_size).unwrap_or_default(),
                source_extension: asset.original_ext.clone(),
                priority,
            })
            .await?)
    }

    async fn enqueue_ladder(
        &self,
        asset: &Asset,
        probe: &ProbeResult,
        settings: &MediaSettings,
        priority: u8,
    ) -> MediaResult<String> {
        let plan = plan_ladder_job(probe, &settings.transcode)
            .map_err(|e| MediaError::Invalid(format!("this video cannot be transcoded: {e}")))?;
        self.enqueue_spec(
            asset,
            JobSpec::Ladder {
                plan: Box::new(plan),
            },
            priority,
        )
        .await
    }

    async fn fail(&self, id: i64, message: &str) {
        if let Err(e) = self
            .repo
            .transition_asset(
                id,
                &[AssetState::Uploaded, AssetState::Probing],
                AssetState::Failed,
                Some(message),
            )
            .await
        {
            tracing::error!(error = %e, asset = id, "could not record the probe failure");
        }
    }

    /// Stream the original into the spool directory (bounded by the size limit).
    async fn spool(&self, asset: &Asset, settings: &MediaSettings) -> MediaResult<PathBuf> {
        tokio::fs::create_dir_all(&settings.spool_dir)
            .await
            .map_err(|e| MediaError::Unavailable(format!("the probe spool directory: {e}")))?;
        let path = settings
            .spool_dir
            .join(format!("{}.{}", asset.id, asset.original_ext));
        let mut stream = self
            .storage
            .get_stream(&asset.original_key, None)
            .await
            .map_err(storage_err)?;
        let mut file = tokio::fs::File::create(&path)
            .await
            .map_err(|e| MediaError::Unavailable(format!("the probe spool file: {e}")))?;
        let mut total = 0u64;
        let outcome: MediaResult<()> = async {
            use futures::StreamExt;
            while let Some(chunk) = stream.body.next().await {
                let chunk = chunk.map_err(storage_err)?;
                total += chunk.len() as u64;
                if total > settings.max_upload_bytes {
                    return Err(MediaError::Invalid(
                        "the original is over the size limit".into(),
                    ));
                }
                file.write_all(&chunk)
                    .await
                    .map_err(|e| MediaError::Unavailable(format!("the probe spool file: {e}")))?;
            }
            file.flush()
                .await
                .map_err(|e| MediaError::Unavailable(format!("the probe spool file: {e}")))
        }
        .await;
        drop(file);
        if let Err(e) = outcome {
            let _ = tokio::fs::remove_file(&path).await;
            return Err(e);
        }
        Ok(path)
    }

    /// Queue the ladder for an asset that waited for approval (review pipeline, staff action).
    pub async fn approve(&self, id: i64) -> MediaResult<Asset> {
        let asset = self.get(id).await?;
        if asset.original_deleted {
            return Err(MediaError::Conflict(
                "the original was deleted; upload it again".into(),
            ));
        }
        let moved = self
            .repo
            .transition_asset(
                id,
                &[
                    AssetState::Probing,
                    AssetState::ProxyReady,
                    AssetState::InReview,
                    AssetState::Approved,
                ],
                AssetState::Transcoding,
                None,
            )
            .await?
            .ok_or_else(|| MediaError::Conflict("this asset is not waiting for approval".into()))?;
        let probe = self.stored_probe(id).await?;
        let settings = self.settings.snapshot().await;
        if let Err(e) = self
            .enqueue_ladder(&moved, &probe, &settings, LADDER_PRIORITY)
            .await
        {
            // Put the state back so the action can be repeated.
            let _ = self
                .repo
                .transition_asset(id, &[AssetState::Transcoding], asset.state, None)
                .await;
            return Err(e);
        }
        Ok(moved)
    }

    async fn stored_probe(&self, id: i64) -> MediaResult<ProbeResult> {
        let v = self
            .repo
            .asset_probe(id)
            .await?
            .ok_or_else(|| MediaError::Conflict("the asset has not been probed yet".into()))?;
        serde_json::from_value(v)
            .map_err(|e| MediaError::Backend(format!("the stored probe is unreadable: {e}")))
    }

    // ---- re-transcode ----

    /// Plan and enqueue a re-transcode (CIN-ING-011).
    pub async fn retranscode(&self, req: RetranscodeRequest) -> MediaResult<RetranscodeOutcome> {
        let (asset, title) = match req.scope {
            RetranscodeScope::Asset(a) => (i64::try_from(a).ok(), None),
            RetranscodeScope::Title(t) => (None, i64::try_from(t).ok()),
            RetranscodeScope::All => (None, None),
        };
        let summaries = self
            .repo
            .retranscode_summaries(asset, title, (crate::retranscode::MAX_BATCH * 4) as u32)
            .await?;
        let plan: RetranscodePlan = crate::retranscode::plan_retranscode(&req, &summaries);
        let settings = self.settings.snapshot().await;
        let priority = priority_value(plan.priority);
        let mut out = RetranscodeOutcome {
            enqueued: Vec::new(),
            skipped: plan
                .skipped
                .iter()
                .map(|(a, r)| RetranscodeSkip {
                    asset_id: i64::try_from(*a).unwrap_or_default(),
                    reason: format!("{r:?}"),
                })
                .collect(),
            estimated_gpu_secs: plan.estimated_gpu_secs,
            truncated: plan.truncated,
            failed: Vec::new(),
        };
        for raw in &plan.enqueue {
            let Ok(id) = i64::try_from(*raw) else {
                continue;
            };
            let attempt = async {
                let asset = self.get(id).await?;
                let probe = self.stored_probe(id).await?;
                let job = self
                    .enqueue_ladder(&asset, &probe, &settings, priority)
                    .await?;
                self.repo
                    .transition_asset(
                        id,
                        &[AssetState::Published, AssetState::Failed],
                        AssetState::Transcoding,
                        None,
                    )
                    .await?;
                Ok::<_, MediaError>(job)
            }
            .await;
            match attempt {
                Ok(job) => out.enqueued.push(job),
                Err(e) => out.failed.push(RetranscodeSkip {
                    asset_id: id,
                    reason: e.to_string(),
                }),
            }
        }
        Ok(out)
    }

    // ---- workers, tokens, queue ----

    pub async fn create_token(&self, by: i64, name: &str) -> MediaResult<CreatedToken> {
        self.create_scoped_token(by, name, TokenScope::Instance)
            .await
    }

    /// A worker token for a member's own machine (BYOS-CNV-003): it claims `mount_transcode` jobs of
    /// `mount_id` and nothing else.
    pub async fn create_mount_token(
        &self,
        by: i64,
        name: &str,
        mount_id: i64,
    ) -> MediaResult<CreatedToken> {
        self.create_scoped_token(by, name, TokenScope::Mount(mount_id))
            .await
    }

    async fn create_scoped_token(
        &self,
        by: i64,
        name: &str,
        scope: TokenScope,
    ) -> MediaResult<CreatedToken> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
            return Err(MediaError::Invalid(
                "the token name must be 1 to 64 characters".into(),
            ));
        }
        let bytes = (self.random)()
            .ok_or_else(|| MediaError::Unavailable("the system random source failed".into()))?;
        let secret = format!("srw_{}", crate::hex(&bytes));
        let token = self
            .repo
            .create_token((self.ids)(), name, &token_hash(&secret), by, scope)
            .await?;
        Ok(CreatedToken { token, secret })
    }

    /// Queue a conversion of one object of a storage mount (BYOS-CNV-003). The caller has proved the right to
    /// queue it and that `key` passed `safe_key`; `size` is the object size from the bucket. Returns the job id.
    pub async fn queue_mount_transcode(
        &self,
        mount_id: i64,
        key: &str,
        size: u64,
        format: crate::mount_job::MountFormat,
        preset: crate::mount_job::MountPreset,
        operator_worker: bool,
    ) -> MediaResult<String> {
        let ext: String = key
            .rsplit('/')
            .next()
            .and_then(|n| n.rsplit_once('.'))
            .map(|(_, e)| e.to_ascii_lowercase())
            .filter(|e| {
                !e.is_empty() && e.len() <= 8 && e.chars().all(|c| c.is_ascii_alphanumeric())
            })
            .unwrap_or_else(|| "bin".to_owned());
        Ok(self
            .repo
            .enqueue(NewJob {
                asset_id: 0,
                spec: JobSpec::MountTranscode {
                    mount_id,
                    key: key.to_owned(),
                    format,
                    preset,
                    operator_worker,
                },
                source_key: key.to_owned(),
                source_size: size,
                source_extension: ext,
                priority: MOUNT_PRIORITY,
            })
            .await?)
    }

    /// The latest jobs of one mount, newest first.
    pub async fn mount_jobs(&self, mount_id: i64, limit: u32) -> MediaResult<Vec<JobSummary>> {
        self.repo
            .list_jobs(&JobQuery {
                mount_id: Some(mount_id),
                limit,
                ..JobQuery::default()
            })
            .await
    }

    pub async fn tokens(&self) -> MediaResult<Vec<WorkerToken>> {
        self.repo.list_tokens().await
    }

    /// The worker tokens of one mount.
    pub async fn mount_tokens(&self, mount_id: i64) -> MediaResult<Vec<WorkerToken>> {
        self.repo.list_mount_tokens(mount_id).await
    }

    /// Revoke a worker token of a mount; `NotFound` when the token belongs to no such mount.
    pub async fn revoke_mount_token(&self, mount_id: i64, id: i64) -> MediaResult<()> {
        if self.repo.revoke_mount_token(mount_id, id).await? {
            Ok(())
        } else {
            Err(MediaError::NotFound("token"))
        }
    }

    pub async fn revoke_token(&self, id: i64) -> MediaResult<()> {
        if self.repo.revoke_token(id).await? {
            Ok(())
        } else {
            Err(MediaError::NotFound("token"))
        }
    }

    pub async fn workers(&self) -> MediaResult<Vec<WorkerView>> {
        let mut list = self.repo.list_workers().await?;
        let now = self.now_ms();
        for w in &mut list {
            w.online = !w.token_revoked && now - w.last_seen_ms < 60_000;
        }
        Ok(list)
    }

    pub async fn set_drain(&self, worker_id: &str, drain: bool) -> MediaResult<()> {
        if self.repo.set_worker_drain(worker_id, drain).await? {
            Ok(())
        } else {
            Err(MediaError::NotFound("worker"))
        }
    }

    pub async fn jobs(&self, q: JobQuery) -> MediaResult<Vec<JobSummary>> {
        self.repo.list_jobs(&q).await
    }

    pub async fn queue_counts(&self) -> MediaResult<QueueCounts> {
        self.repo.queue_counts().await
    }

    pub async fn job(&self, id: i64) -> MediaResult<JobDetail> {
        self.repo
            .job_detail(id)
            .await?
            .ok_or(MediaError::NotFound("job"))
    }

    pub async fn cancel_job(&self, id: i64) -> MediaResult<()> {
        self.repo
            .request_cancel(&id.to_string())
            .await
            .map_err(|e| match e {
                crate::worker_api::StoreError::NotFound => MediaError::NotFound("job"),
                other => other.into(),
            })
    }

    pub async fn retry_job(&self, id: i64) -> MediaResult<()> {
        if self.repo.retry_job(id).await? {
            Ok(())
        } else {
            Err(MediaError::Conflict(
                "only a failed or cancelled job can be retried".into(),
            ))
        }
    }

    // ---- retention ----

    /// The periodic sweep (CIN-ING-009): retention decisions over the assets, expired upload
    /// slots, and the collection of storage prefixes nothing references any more.
    pub async fn sweep(&self) -> MediaResult<SweepReport> {
        let settings = self.settings.snapshot().await;
        let now = self.now();
        let mut report = SweepReport::default();
        let facts = self.repo.retention_facts(5000).await?;
        let plan = retention::plan_cleanup(&settings.retention, &facts, now);
        report.cold_skipped = u32::try_from(plan.move_to_cold.len()).unwrap_or(u32::MAX);
        for (raw, _reason) in &plan.delete_originals {
            let Ok(id) = i64::try_from(*raw) else {
                continue;
            };
            let Some(asset) = self.repo.get_asset(id).await? else {
                continue;
            };
            match self.storage.delete(&asset.original_key).await {
                Ok(()) | Err(sr_storage::StorageError::NotFound(_)) => {
                    self.repo.mark_original_deleted(id).await?;
                    report.originals_deleted += 1;
                    report.bytes_freed += u64::try_from(asset.original_size).unwrap_or(0);
                }
                Err(e) => {
                    tracing::warn!(error = %e, asset = id, "retention could not delete an original");
                    report.failed += 1;
                }
            }
        }
        for (raw, _reason) in &plan.delete_proxies {
            let Ok(id) = i64::try_from(*raw) else {
                continue;
            };
            let Some(asset) = self.repo.get_asset(id).await? else {
                continue;
            };
            if let Some(key) = &asset.proxy_key {
                match self.storage.delete(key).await {
                    Ok(()) | Err(sr_storage::StorageError::NotFound(_)) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, asset = id, "retention could not delete a proxy");
                        report.failed += 1;
                        continue;
                    }
                }
            }
            self.repo.mark_proxy_deleted(id).await?;
            report.proxies_deleted += 1;
        }
        let cutoff = self.now_ms() - SLOT_EXPIRY_MS;
        report.slots_expired =
            u32::try_from(self.repo.expire_slots(cutoff, 200).await?.len()).unwrap_or(u32::MAX);
        report.prefixes_collected = self.collect_garbage().await?;
        Ok(report)
    }

    /// Delete the objects under prefixes queued for collection (after a grace period) unless an
    /// asset references them again.
    pub async fn collect_garbage(&self) -> MediaResult<u32> {
        let grace_ms = 10 * 60 * 1000;
        let due = self.repo.gc_due(self.now_ms() - grace_ms, 50).await?;
        if due.is_empty() {
            return Ok(0);
        }
        let mut done = Vec::new();
        for prefix in due {
            match self.storage.list(&format!("{prefix}/")).await {
                Ok(objects) => {
                    let mut ok = true;
                    for o in objects {
                        if let Err(e) = self.storage.delete(&o.key).await
                            && !matches!(e, sr_storage::StorageError::NotFound(_))
                        {
                            tracing::warn!(error = %e, key = o.key, "media GC could not delete");
                            ok = false;
                            break;
                        }
                    }
                    if ok {
                        done.push(prefix);
                    }
                }
                Err(e) => tracing::warn!(error = %e, prefix, "media GC could not list a prefix"),
            }
        }
        let n = u32::try_from(done.len()).unwrap_or(u32::MAX);
        self.repo.gc_done(&done).await?;
        Ok(n)
    }
}

fn probe_failure(e: &ProbeError, ffprobe: &str) -> String {
    match e {
        ProbeError::Run(m) => format!(
            "ffprobe could not be started ({m}). Install FFmpeg on the server or set \
             cinema.ffprobe_path (currently {ffprobe:?}), then use Probe again."
        ),
        ProbeError::TimedOut => "ffprobe timed out reading the file".to_owned(),
        ProbeError::Unreadable(m) => format!("ffprobe could not read the file: {m}"),
        ProbeError::TooLarge | ProbeError::TooManyStreams(_) | ProbeError::BadJson(_) => {
            format!("the file's metadata is not usable: {e}")
        }
        ProbeError::UnsafePath => "the spool path was refused".to_owned(),
    }
}

fn summarise(p: &ProbeResult) -> ProbeSummary {
    let v = p.primary_video();
    let mut langs: Vec<String> = p.audio.iter().filter_map(|a| a.language.clone()).collect();
    langs.dedup();
    ProbeSummary {
        container: p.format.format_name.clone(),
        duration_secs: p.format.duration_secs,
        width: v.map(|v| v.width),
        height: v.map(|v| v.height),
        video_codec: v.map(|v| v.codec_name.clone()),
        hdr: v.map(|v| format!("{:?}", v.hdr).to_ascii_lowercase()),
        interlaced: v.is_some_and(|v| v.is_interlaced()),
        audio_languages: langs,
        subtitle_count: u32::try_from(p.subtitles.len()).unwrap_or(u32::MAX),
        warnings: p.warnings.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_and_names() {
        assert_eq!(ext_of("Movie (2020).MKV").as_deref(), Some("mkv"));
        assert_eq!(ext_of("C:\\videos\\a.mp4").as_deref(), Some("mp4"));
        assert_eq!(ext_of("noext"), None);
        assert_eq!(ext_of(".mkv"), None);
        assert_eq!(ext_of("a.exe"), None);
        assert_eq!(ext_of("a.mkv.zip"), None);
        assert_eq!(clean_filename("../../etc/pass\nwd.mkv"), "passwd.mkv");
    }

    #[test]
    fn probe_failures_name_the_remedy() {
        let m = probe_failure(&ProbeError::Run("no such file".into()), "ffprobe");
        assert!(m.contains("Install FFmpeg") && m.contains("cinema.ffprobe_path"));
        assert!(probe_failure(&ProbeError::TimedOut, "ffprobe").contains("timed out"));
    }
}
