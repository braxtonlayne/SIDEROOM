//! Worker API contract and server-side service (CIN-ING-004).
//!
//! Workers authenticate with a bearer token, register, heartbeat, claim jobs, download the
//! original through a signed URL, upload outputs with presigned multipart PUTs, report
//! progress and finish with complete or fail (plus logs). No inbound port is needed on the
//! worker: everything is worker-initiated HTTPS.
//!
//! The service is written against two ports: [`JobStore`] (jobs, workers, uploads; the
//! Postgres tables are the catalog's job, proposed in docs/specs/cinema-media.md) and
//! [`OutputStorage`] (presigned multipart). In-memory implementations are provided for tests
//! and for single-process development.

use crate::plan::{JobPlan, ProxyPlan};
use crate::playlist::UrlSigner;
use crate::remux::RemuxPlan;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use thiserror::Error;
use utoipa::ToSchema;

/// Route prefix; see [`crate::http::router`].
pub const API_PREFIX: &str = "/api/v1/transcode/worker";

pub const DEFAULT_PART_SIZE: u64 = 16 * 1024 * 1024;
pub const MAX_PARTS: u64 = 10_000;
pub const MAX_LOG_BYTES: usize = 32 * 1024;
pub const MAX_OUTPUT_FILES: usize = 200_000;

// ---- wire types ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, ToSchema)]
pub struct WorkerCapabilities {
    pub nvenc: bool,
    pub encoders: Vec<String>,
    pub hwaccels: Vec<String>,
    pub concurrency: u32,
    pub ffmpeg_version: Option<String>,
    /// ffmpeg was built with libplacebo (needed for Dolby Vision profile 5).
    pub libplacebo: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub name: String,
    pub version: String,
    pub capabilities: WorkerCapabilities,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegisterResponse {
    pub worker_id: String,
    pub heartbeat_secs: u32,
    pub lease_secs: u32,
    /// Poll for work this often when `claim` returns nothing.
    pub poll_secs: u32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HeartbeatRequest {
    pub active_jobs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct HeartbeatResponse {
    /// Jobs the worker must stop (an admin cancelled them).
    pub cancel_jobs: Vec<String>,
    /// Finish current jobs and claim no more (the token was revoked or the admin drained it).
    pub drain: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Proxy,
    Ladder,
    /// Stream-copy remux of a stored rendition into an MP4 (Patron download, CIN-DL-001).
    Remux,
}

impl JobKind {
    pub const ALL: [Self; 3] = [Self::Proxy, Self::Ladder, Self::Remux];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Proxy => "proxy",
            Self::Ladder => "ladder",
            Self::Remux => "remux",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ClaimRequest {
    /// Kinds this worker accepts; empty means all.
    pub kinds: Vec<JobKind>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JobSpec {
    Proxy { plan: Box<ProxyPlan> },
    Ladder { plan: Box<JobPlan> },
    Remux { plan: Box<RemuxPlan> },
}

impl JobSpec {
    pub fn kind(&self) -> JobKind {
        match self {
            Self::Proxy { .. } => JobKind::Proxy,
            Self::Ladder { .. } => JobKind::Ladder,
            Self::Remux { .. } => JobKind::Remux,
        }
    }
    /// Paths that must exist when the job completes.
    pub fn required_outputs(&self) -> Vec<String> {
        match self {
            Self::Proxy { plan } => vec![plan.output.clone()],
            Self::Ladder { plan } => plan.required_outputs(),
            Self::Remux { plan } => vec![plan.output.clone()],
        }
    }
    /// Directories (prefixes) outputs may be written under.
    pub fn allowed_prefixes(&self) -> Vec<String> {
        match self {
            Self::Proxy { .. } => vec!["proxy/".into()],
            Self::Remux { .. } => vec!["remux/".into()],
            Self::Ladder { plan } => {
                let mut p: Vec<String> = plan.rungs.iter().map(|r| format!("{}/", r.dir)).collect();
                p.extend(plan.audio.iter().map(|a| format!("{}/", a.dir)));
                p.extend(["subs/", "sprites/", "posters/", "fonts/"].map(String::from));
                p
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceRef {
    /// Signed GET URL of the original.
    pub download_url: String,
    pub size_bytes: u64,
    pub expires_at: u64,
    /// Lowercase extension of the original (`mkv`), used for the local file name.
    pub extension: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClaimedJob {
    pub job_id: String,
    pub attempt: u32,
    pub spec: JobSpec,
    pub source: SourceRef,
    pub lease_expires_at: u64,
    /// The worker must not produce more than this many bytes in total.
    pub max_output_bytes: u64,
    /// Remux jobs: every file to fetch, with a signed URL (empty for the other kinds).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<ClaimInput>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClaimInput {
    /// Local path under the job input directory.
    pub path: String,
    pub url: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ClaimResponse {
    pub job: Option<ClaimedJob>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Download,
    Video,
    Audio,
    Subtitles,
    Sprites,
    Upload,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ProgressRequest {
    pub stage: Stage,
    /// Overall percent, 0..=100.
    pub percent: f32,
    pub fps: Option<f32>,
    pub speed: Option<f32>,
    pub out_time_secs: Option<f64>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgressResponse {
    pub cancel: bool,
    pub lease_expires_at: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InitUploadRequest {
    /// Relative output path, e.g. `v/1080p/seg_00001.m4s`.
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PartUrl {
    pub part_number: u32,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InitUploadResponse {
    pub upload_id: String,
    pub part_size: u64,
    /// One presigned PUT per part; a file no larger than `part_size` has exactly one.
    pub parts: Vec<PartUrl>,
    pub content_type: String,
    pub expires_at: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PartDone {
    pub part_number: u32,
    pub etag: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinishUploadRequest {
    pub parts: Vec<PartDone>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputRef {
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SegmentEntry {
    pub name: String,
    pub duration_secs: f64,
    pub size_bytes: u64,
}

/// The per-rendition segment index stored in `renditions.segment_index` (Live TV stitcher).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SegmentIndex {
    /// The rendition's playlist path, e.g. `v/1080p/index.m3u8`.
    pub playlist: String,
    pub init: String,
    pub segments: Vec<SegmentEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompleteRequest {
    pub outputs: Vec<OutputRef>,
    pub segment_index: Vec<SegmentIndex>,
    /// `h264_nvenc` or `libx264`.
    pub encoder: String,
    pub wall_secs: f64,
    pub log_tail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FailRequest {
    pub stage: Stage,
    pub message: String,
    pub log_tail: Option<String>,
    /// A retry might succeed (transient I/O, encoder crash); false for bad input.
    pub retryable: bool,
    /// The worker stopped because it was told to cancel.
    pub cancelled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ack {
    pub ok: bool,
}

// ---- errors -------------------------------------------------------------------------------

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ApiError {
    #[error("missing or invalid worker token")]
    Unauthorized,
    #[error("this job belongs to another worker")]
    Forbidden,
    #[error("not found")]
    NotFound,
    #[error("job is not in a state that allows this: {0}")]
    Conflict(&'static str),
    #[error("lease expired: stop work on this job")]
    Gone,
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("internal error")]
    Internal(String),
}

impl ApiError {
    pub fn status(&self) -> u16 {
        match self {
            Self::Unauthorized => 401,
            Self::Forbidden => 403,
            Self::NotFound => 404,
            Self::Conflict(_) => 409,
            Self::Gone => 410,
            Self::BadRequest(_) => 400,
            Self::Internal(_) => 500,
        }
    }
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unauthorized => "unauthorized",
            Self::Forbidden => "forbidden",
            Self::NotFound => "not_found",
            Self::Conflict(_) => "conflict",
            Self::Gone => "lease_expired",
            Self::BadRequest(_) => "bad_request",
            Self::Internal(_) => "internal",
        }
    }
}

// ---- ports --------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenInfo {
    pub token_id: u64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkerRecord {
    pub worker_id: String,
    pub token_id: u64,
    pub name: String,
    pub version: String,
    pub capabilities: WorkerCapabilities,
    pub last_seen: u64,
    pub drain: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewJob {
    pub asset_id: u64,
    pub spec: JobSpec,
    /// Object key of the original in storage.
    pub source_key: String,
    pub source_size: u64,
    pub source_extension: String,
    pub priority: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JobRecord {
    pub job_id: String,
    pub asset_id: u64,
    pub spec: JobSpec,
    pub source_key: String,
    pub source_size: u64,
    pub source_extension: String,
    pub priority: u8,
    pub state: JobState,
    pub attempt: u32,
    pub worker_id: Option<String>,
    pub lease_expires_at: Option<u64>,
    pub cancel_requested: bool,
    pub progress: Option<ProgressRequest>,
    pub error: Option<String>,
    pub log_tail: Option<String>,
    pub outputs: Vec<OutputRef>,
    pub segment_index: Vec<SegmentIndex>,
    pub encoder: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UploadRecord {
    pub upload_id: String,
    pub path: String,
    pub key: String,
    pub size: u64,
    pub finished: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailOutcome {
    Requeued,
    Failed,
    Cancelled,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StoreError {
    #[error("job not found")]
    NotFound,
    #[error("{0}")]
    Conflict(&'static str),
    #[error("backend error: {0}")]
    Backend(String),
}

impl From<StoreError> for ApiError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::NotFound => Self::NotFound,
            StoreError::Conflict(m) => Self::Conflict(m),
            StoreError::Backend(m) => Self::Internal(m),
        }
    }
}

/// Persistence port for jobs, workers and uploads.
#[async_trait]
pub trait JobStore: Send + Sync {
    /// Look up a worker token by the SHA-256 (lowercase hex) of the secret.
    async fn authenticate(&self, token_sha256: &str) -> Result<Option<TokenInfo>, StoreError>;
    async fn upsert_worker(
        &self,
        token: &TokenInfo,
        reg: &RegisterRequest,
        now: u64,
    ) -> Result<WorkerRecord, StoreError>;
    async fn get_worker(&self, worker_id: &str) -> Result<Option<WorkerRecord>, StoreError>;
    async fn touch_worker(&self, worker_id: &str, now: u64) -> Result<(), StoreError>;
    /// Jobs of this worker with a cancel request pending.
    async fn cancel_requests_for(&self, worker_id: &str) -> Result<Vec<String>, StoreError>;

    async fn enqueue(&self, job: NewJob) -> Result<String, StoreError>;
    /// Atomically claim the best queued job of an accepted kind, if any.
    async fn claim_next(
        &self,
        worker_id: &str,
        kinds: &[JobKind],
        now: u64,
        lease_secs: u64,
    ) -> Result<Option<JobRecord>, StoreError>;
    async fn get_job(&self, job_id: &str) -> Result<Option<JobRecord>, StoreError>;
    /// Record progress and extend the lease. Returns the updated record.
    async fn report_progress(
        &self,
        job_id: &str,
        worker_id: &str,
        progress: &ProgressRequest,
        now: u64,
        lease_secs: u64,
    ) -> Result<JobRecord, StoreError>;
    async fn add_upload(&self, job_id: &str, upload: UploadRecord) -> Result<(), StoreError>;
    async fn finish_upload(
        &self,
        job_id: &str,
        upload_id: &str,
    ) -> Result<UploadRecord, StoreError>;
    async fn uploads(&self, job_id: &str) -> Result<Vec<UploadRecord>, StoreError>;
    async fn complete_job(
        &self,
        job_id: &str,
        worker_id: &str,
        req: &CompleteRequest,
    ) -> Result<(), StoreError>;
    async fn fail_job(
        &self,
        job_id: &str,
        worker_id: &str,
        req: &FailRequest,
        max_attempts: u32,
    ) -> Result<FailOutcome, StoreError>;
    /// Admin: ask a running job to stop, or cancel a queued one outright.
    async fn request_cancel(&self, job_id: &str) -> Result<(), StoreError>;
    /// Requeue or fail running jobs whose lease ran out. Returns the affected job ids.
    async fn reap_expired(&self, now: u64, max_attempts: u32) -> Result<Vec<String>, StoreError>;
}

/// Presigned multipart port (S3/Garage).
#[async_trait]
pub trait OutputStorage: Send + Sync {
    async fn init_multipart(&self, key: &str, content_type: &str) -> Result<String, String>;
    async fn presign_part(
        &self,
        key: &str,
        upload_id: &str,
        part_number: u32,
        ttl_secs: u64,
    ) -> Result<String, String>;
    async fn complete_multipart(
        &self,
        key: &str,
        upload_id: &str,
        parts: &[PartDone],
    ) -> Result<(), String>;
}

// ---- output path sandbox ------------------------------------------------------------------

fn content_type_for(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "m3u8" => "application/vnd.apple.mpegurl",
        "mp4" => "video/mp4",
        "m4s" => "video/iso.segment",
        "vtt" => "text/vtt",
        "ass" => "text/x-ssa",
        "jpg" | "jpeg" => "image/jpeg",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "ttc" => "font/collection",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        _ => return None,
    })
}

/// Validate a worker-supplied output path and return its content type.
///
/// Relative, at most five components, characters `[A-Za-z0-9._ -]` only (no `..`, no leading
/// dot, no separators other than `/`), a known extension, and under a directory the job's
/// plan allows. Anything else is a bad request: the worker is untrusted too.
pub fn validate_output_path(
    path: &str,
    allowed_prefixes: &[String],
) -> Result<&'static str, ApiError> {
    let bad = |m: &str| Err(ApiError::BadRequest(format!("output path: {m}")));
    if path.is_empty() || path.len() > 200 {
        return bad("empty or too long");
    }
    if path.starts_with('/') || path.contains('\\') || path.contains('\0') {
        return bad("must be relative");
    }
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() > 5 {
        return bad("too deep");
    }
    for p in &parts {
        let ok = !p.is_empty()
            && !p.starts_with('.')
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ' '));
        if !ok {
            return bad("illegal component");
        }
    }
    if !allowed_prefixes
        .iter()
        .any(|pre| path.starts_with(pre.as_str()))
    {
        return bad("directory not part of this job");
    }
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    content_type_for(&ext)
        .ok_or_else(|| ApiError::BadRequest("output path: unsupported file type".into()))
}

/// The CDN path an original is served under: object keys start with `originals/`, which the
/// `originals` CDN kind adds back (`originals/12/source.mkv` is `/cdn/originals/12/source.mkv`).
pub fn original_cdn_path(key: &str) -> String {
    format!(
        "/cdn/originals/{}",
        key.strip_prefix("originals/").unwrap_or(key)
    )
}

/// Storage key for a job output: staged per job and attempt; the catalog promotes on complete.
pub fn output_key(job: &JobRecord, path: &str) -> String {
    format!("transcode/{}/{}/{}", job.job_id, job.attempt, path)
}

// ---- service ------------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ServiceConfig {
    pub heartbeat_secs: u32,
    pub lease_secs: u32,
    pub poll_secs: u32,
    pub max_attempts: u32,
    pub download_ttl_secs: u64,
    pub upload_ttl_secs: u64,
    pub part_size: u64,
    pub max_file_bytes: u64,
    /// Multiple of the original's size a job may produce, with a floor.
    pub output_budget_factor: u64,
    pub output_budget_floor: u64,
}

impl ServiceConfig {
    /// Every output is one part, so one presigned `PUT` (for storage that cannot presign parts).
    pub fn single_part(mut self) -> Self {
        self.part_size = self.max_file_bytes.saturating_add(1);
        self
    }
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            heartbeat_secs: 20,
            lease_secs: 120,
            poll_secs: 10,
            max_attempts: 3,
            download_ttl_secs: 8 * 3600,
            upload_ttl_secs: 3600,
            part_size: DEFAULT_PART_SIZE,
            max_file_bytes: 4 * 1024 * 1024 * 1024,
            output_budget_factor: 2,
            output_budget_floor: 20 * 1024 * 1024 * 1024,
        }
    }
}

/// Authenticated caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller(pub TokenInfo);

pub struct WorkerService {
    pub store: Arc<dyn JobStore>,
    pub outputs: Arc<dyn OutputStorage>,
    /// Signs `GET` URLs of originals (`/cdn/originals/<key>`).
    pub downloads: Arc<dyn UrlSigner>,
    pub clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    pub config: ServiceConfig,
}

pub fn token_hash(token: &str) -> String {
    crate::hex(&Sha256::digest(token.as_bytes()))
}

fn clip_log(log: Option<&str>) -> Option<String> {
    let log = log?;
    let mut start = log.len().saturating_sub(MAX_LOG_BYTES);
    while !log.is_char_boundary(start) {
        start += 1;
    }
    Some(
        log[start..]
            .chars()
            .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
            .collect(),
    )
}

impl WorkerService {
    fn now(&self) -> u64 {
        (self.clock)()
    }

    /// Authenticate a bearer token.
    pub async fn authenticate(&self, bearer: Option<&str>) -> Result<Caller, ApiError> {
        let token = bearer
            .filter(|t| !t.is_empty())
            .ok_or(ApiError::Unauthorized)?;
        self.store
            .authenticate(&token_hash(token))
            .await?
            .map(Caller)
            .ok_or(ApiError::Unauthorized)
    }

    async fn own_worker(&self, caller: &Caller, worker_id: &str) -> Result<WorkerRecord, ApiError> {
        let w = self
            .store
            .get_worker(worker_id)
            .await?
            .ok_or(ApiError::NotFound)?;
        if w.token_id != caller.0.token_id {
            return Err(ApiError::Forbidden);
        }
        Ok(w)
    }

    /// The caller's running job, or the right error.
    async fn own_job(&self, caller: &Caller, job_id: &str) -> Result<JobRecord, ApiError> {
        let job = self
            .store
            .get_job(job_id)
            .await?
            .ok_or(ApiError::NotFound)?;
        let worker_id = job.worker_id.as_deref().ok_or(ApiError::Forbidden)?;
        self.own_worker(caller, worker_id).await?;
        if job.state != JobState::Running {
            return Err(ApiError::Conflict("job is not running"));
        }
        if job.lease_expires_at.is_some_and(|l| l <= self.now()) {
            return Err(ApiError::Gone);
        }
        Ok(job)
    }

    pub async fn register(
        &self,
        caller: &Caller,
        req: RegisterRequest,
    ) -> Result<RegisterResponse, ApiError> {
        if req.name.is_empty() || req.name.len() > 64 || req.capabilities.concurrency > 64 {
            return Err(ApiError::BadRequest("invalid registration".into()));
        }
        let w = self
            .store
            .upsert_worker(&caller.0, &req, self.now())
            .await?;
        Ok(RegisterResponse {
            worker_id: w.worker_id,
            heartbeat_secs: self.config.heartbeat_secs,
            lease_secs: self.config.lease_secs,
            poll_secs: self.config.poll_secs,
        })
    }

    pub async fn heartbeat(
        &self,
        caller: &Caller,
        worker_id: &str,
        _req: HeartbeatRequest,
    ) -> Result<HeartbeatResponse, ApiError> {
        let w = self.own_worker(caller, worker_id).await?;
        self.store.touch_worker(worker_id, self.now()).await?;
        // Opportunistic: every heartbeat also recycles jobs of dead workers.
        self.store
            .reap_expired(self.now(), self.config.max_attempts)
            .await?;
        Ok(HeartbeatResponse {
            cancel_jobs: self.store.cancel_requests_for(worker_id).await?,
            drain: w.drain,
        })
    }

    pub async fn claim(
        &self,
        caller: &Caller,
        worker_id: &str,
        req: ClaimRequest,
    ) -> Result<ClaimResponse, ApiError> {
        let w = self.own_worker(caller, worker_id).await?;
        if w.drain {
            return Ok(ClaimResponse { job: None });
        }
        let now = self.now();
        let kinds = if req.kinds.is_empty() {
            JobKind::ALL.to_vec()
        } else {
            req.kinds
        };
        let Some(job) = self
            .store
            .claim_next(worker_id, &kinds, now, u64::from(self.config.lease_secs))
            .await?
        else {
            return Ok(ClaimResponse { job: None });
        };
        let expires_at = now + self.config.download_ttl_secs;
        let (url, inputs) = match &job.spec {
            JobSpec::Remux { plan } => {
                let inputs: Vec<ClaimInput> = plan
                    .inputs
                    .iter()
                    .map(|i| ClaimInput {
                        path: i.path.clone(),
                        url: self.downloads.signed_url(&i.cdn_path, expires_at),
                        size_bytes: i.size_bytes,
                    })
                    .collect();
                let first = inputs.first().map(|i| i.url.clone()).unwrap_or_default();
                (first, inputs)
            }
            _ => (
                self.downloads
                    .signed_url(&original_cdn_path(&job.source_key), expires_at),
                Vec::new(),
            ),
        };
        let budget = job
            .source_size
            .saturating_mul(self.config.output_budget_factor)
            .max(self.config.output_budget_floor);
        Ok(ClaimResponse {
            job: Some(ClaimedJob {
                job_id: job.job_id.clone(),
                attempt: job.attempt,
                spec: job.spec.clone(),
                source: SourceRef {
                    download_url: url,
                    size_bytes: job.source_size,
                    expires_at,
                    extension: job.source_extension.clone(),
                },
                lease_expires_at: job.lease_expires_at.unwrap_or(now),
                max_output_bytes: budget,
                inputs,
            }),
        })
    }

    pub async fn progress(
        &self,
        caller: &Caller,
        job_id: &str,
        mut req: ProgressRequest,
    ) -> Result<ProgressResponse, ApiError> {
        let job = self.own_job(caller, job_id).await?;
        if !req.percent.is_finite() {
            return Err(ApiError::BadRequest("percent is not a number".into()));
        }
        req.percent = req.percent.clamp(0.0, 100.0);
        req.message = req
            .message
            .map(|m| m.chars().filter(|c| !c.is_control()).take(200).collect());
        let updated = self
            .store
            .report_progress(
                job_id,
                job.worker_id.as_deref().unwrap_or_default(),
                &req,
                self.now(),
                u64::from(self.config.lease_secs),
            )
            .await?;
        Ok(ProgressResponse {
            cancel: updated.cancel_requested,
            lease_expires_at: updated.lease_expires_at.unwrap_or(0),
        })
    }

    pub async fn init_upload(
        &self,
        caller: &Caller,
        job_id: &str,
        req: InitUploadRequest,
    ) -> Result<InitUploadResponse, ApiError> {
        let job = self.own_job(caller, job_id).await?;
        let content_type = validate_output_path(&req.path, &job.spec.allowed_prefixes())?;
        if req.size == 0 || req.size > self.config.max_file_bytes {
            return Err(ApiError::BadRequest("file size out of range".into()));
        }
        let existing = self.store.uploads(job_id).await?;
        if existing.len() >= MAX_OUTPUT_FILES {
            return Err(ApiError::BadRequest("too many output files".into()));
        }
        let budget = job
            .source_size
            .saturating_mul(self.config.output_budget_factor)
            .max(self.config.output_budget_floor);
        let used: u64 = existing
            .iter()
            .filter(|u| u.path != req.path)
            .map(|u| u.size)
            .sum();
        if used.saturating_add(req.size) > budget {
            return Err(ApiError::BadRequest("output budget exceeded".into()));
        }
        let part_size = self.config.part_size.max(1);
        let n_parts = req.size.div_ceil(part_size);
        if n_parts > MAX_PARTS {
            return Err(ApiError::BadRequest(
                "file too large for the part size".into(),
            ));
        }
        let key = output_key(&job, &req.path);
        let upload_id = self
            .outputs
            .init_multipart(&key, content_type)
            .await
            .map_err(ApiError::Internal)?;
        let mut parts = Vec::new();
        for n in 1..=n_parts {
            let part_number = u32::try_from(n).map_err(|_| ApiError::Internal("part".into()))?;
            let url = self
                .outputs
                .presign_part(&key, &upload_id, part_number, self.config.upload_ttl_secs)
                .await
                .map_err(ApiError::Internal)?;
            parts.push(PartUrl { part_number, url });
        }
        self.store
            .add_upload(
                job_id,
                UploadRecord {
                    upload_id: upload_id.clone(),
                    path: req.path,
                    key,
                    size: req.size,
                    finished: false,
                },
            )
            .await?;
        Ok(InitUploadResponse {
            upload_id,
            part_size,
            parts,
            content_type: content_type.to_string(),
            expires_at: self.now() + self.config.upload_ttl_secs,
        })
    }

    pub async fn finish_upload(
        &self,
        caller: &Caller,
        job_id: &str,
        upload_id: &str,
        req: FinishUploadRequest,
    ) -> Result<Ack, ApiError> {
        self.own_job(caller, job_id).await?;
        let record = self
            .store
            .uploads(job_id)
            .await?
            .into_iter()
            .find(|u| u.upload_id == upload_id)
            .ok_or(ApiError::NotFound)?;
        let expected = record.size.div_ceil(self.config.part_size.max(1));
        let mut numbers: Vec<u32> = req.parts.iter().map(|p| p.part_number).collect();
        numbers.sort_unstable();
        let contiguous = numbers
            .iter()
            .enumerate()
            .all(|(i, n)| u64::from(*n) == i as u64 + 1);
        if !contiguous
            || numbers.len() as u64 != expected
            || req.parts.iter().any(|p| p.etag.is_empty())
        {
            return Err(ApiError::BadRequest("parts do not match the upload".into()));
        }
        self.outputs
            .complete_multipart(&record.key, upload_id, &req.parts)
            .await
            .map_err(ApiError::Internal)?;
        self.store.finish_upload(job_id, upload_id).await?;
        Ok(Ack { ok: true })
    }

    pub async fn complete(
        &self,
        caller: &Caller,
        job_id: &str,
        req: CompleteRequest,
    ) -> Result<Ack, ApiError> {
        let job = self.own_job(caller, job_id).await?;
        let finished: BTreeMap<String, u64> = self
            .store
            .uploads(job_id)
            .await?
            .into_iter()
            .filter(|u| u.finished)
            .map(|u| (u.path, u.size))
            .collect();
        for required in job.spec.required_outputs() {
            if !finished.contains_key(&required) {
                return Err(ApiError::BadRequest(format!("missing output {required}")));
            }
        }
        for o in &req.outputs {
            match finished.get(&o.path) {
                Some(size) if *size == o.size => {}
                _ => {
                    return Err(ApiError::BadRequest(format!(
                        "output {} was not uploaded with that size",
                        o.path
                    )));
                }
            }
        }
        if let JobSpec::Ladder { plan } = &job.spec {
            for rung in &plan.rungs {
                let idx = req
                    .segment_index
                    .iter()
                    .find(|s| s.playlist == rung.playlist)
                    .ok_or_else(|| {
                        ApiError::BadRequest(format!("missing segment index for {}", rung.name))
                    })?;
                if idx.segments.is_empty() {
                    return Err(ApiError::BadRequest("empty segment index".into()));
                }
                for s in &idx.segments {
                    let path = format!("{}/{}", rung.dir, s.name);
                    if !finished.contains_key(&path)
                        || !s.duration_secs.is_finite()
                        || s.duration_secs <= 0.0
                    {
                        return Err(ApiError::BadRequest(format!("segment {path} not uploaded")));
                    }
                }
            }
        }
        let mut req = req;
        req.log_tail = clip_log(req.log_tail.as_deref());
        self.store
            .complete_job(job_id, job.worker_id.as_deref().unwrap_or_default(), &req)
            .await?;
        Ok(Ack { ok: true })
    }

    pub async fn fail(
        &self,
        caller: &Caller,
        job_id: &str,
        mut req: FailRequest,
    ) -> Result<Ack, ApiError> {
        let job = self.own_job(caller, job_id).await?;
        req.log_tail = clip_log(req.log_tail.as_deref());
        req.message = req
            .message
            .chars()
            .filter(|c| !c.is_control())
            .take(500)
            .collect();
        self.store
            .fail_job(
                job_id,
                job.worker_id.as_deref().unwrap_or_default(),
                &req,
                self.config.max_attempts,
            )
            .await?;
        Ok(Ack { ok: true })
    }
}

// ---- in-memory implementations ------------------------------------------------------------

#[derive(Default)]
struct MemInner {
    tokens: HashMap<String, TokenInfo>,
    workers: BTreeMap<String, WorkerRecord>,
    jobs: BTreeMap<String, JobRecord>,
    created: HashMap<String, u64>,
    uploads: HashMap<String, Vec<UploadRecord>>,
    next_job: u64,
    next_worker: u64,
}

/// In-memory [`JobStore`] for tests and single-process development.
#[derive(Default)]
pub struct MemoryJobStore {
    inner: Mutex<MemInner>,
}

impl MemoryJobStore {
    pub fn new() -> Self {
        Self::default()
    }
    /// Register a worker token (the admin UI does this; the secret is shown once).
    pub fn add_token(&self, secret: &str, info: TokenInfo) {
        if let Ok(mut g) = self.inner.lock() {
            g.tokens.insert(token_hash(secret), info);
        }
    }
    pub fn set_drain(&self, worker_id: &str, drain: bool) {
        if let Ok(mut g) = self.inner.lock()
            && let Some(w) = g.workers.get_mut(worker_id)
        {
            w.drain = drain;
        }
    }
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, MemInner>, StoreError> {
        self.inner
            .lock()
            .map_err(|_| StoreError::Backend("poisoned".into()))
    }
}

fn requeue_or_fail(
    job: &mut JobRecord,
    retryable: bool,
    error: &str,
    max_attempts: u32,
) -> FailOutcome {
    job.worker_id = None;
    job.lease_expires_at = None;
    job.error = Some(error.to_string());
    if retryable && job.attempt < max_attempts && !job.cancel_requested {
        job.state = JobState::Queued;
        FailOutcome::Requeued
    } else if job.cancel_requested {
        job.state = JobState::Cancelled;
        FailOutcome::Cancelled
    } else {
        job.state = JobState::Failed;
        FailOutcome::Failed
    }
}

#[async_trait]
impl JobStore for MemoryJobStore {
    async fn authenticate(&self, token_sha256: &str) -> Result<Option<TokenInfo>, StoreError> {
        Ok(self.lock()?.tokens.get(token_sha256).cloned())
    }

    async fn upsert_worker(
        &self,
        token: &TokenInfo,
        reg: &RegisterRequest,
        now: u64,
    ) -> Result<WorkerRecord, StoreError> {
        let mut g = self.lock()?;
        let existing = g
            .workers
            .values()
            .find(|w| w.token_id == token.token_id && w.name == reg.name)
            .map(|w| w.worker_id.clone());
        let id = match existing {
            Some(id) => id,
            None => {
                g.next_worker += 1;
                format!("w{}", g.next_worker)
            }
        };
        let drain = g.workers.get(&id).is_some_and(|w| w.drain);
        let rec = WorkerRecord {
            worker_id: id.clone(),
            token_id: token.token_id,
            name: reg.name.clone(),
            version: reg.version.clone(),
            capabilities: reg.capabilities.clone(),
            last_seen: now,
            drain,
        };
        g.workers.insert(id, rec.clone());
        Ok(rec)
    }

    async fn get_worker(&self, worker_id: &str) -> Result<Option<WorkerRecord>, StoreError> {
        Ok(self.lock()?.workers.get(worker_id).cloned())
    }

    async fn touch_worker(&self, worker_id: &str, now: u64) -> Result<(), StoreError> {
        let mut g = self.lock()?;
        let w = g.workers.get_mut(worker_id).ok_or(StoreError::NotFound)?;
        w.last_seen = now;
        Ok(())
    }

    async fn cancel_requests_for(&self, worker_id: &str) -> Result<Vec<String>, StoreError> {
        Ok(self
            .lock()?
            .jobs
            .values()
            .filter(|j| {
                j.state == JobState::Running
                    && j.cancel_requested
                    && j.worker_id.as_deref() == Some(worker_id)
            })
            .map(|j| j.job_id.clone())
            .collect())
    }

    async fn enqueue(&self, job: NewJob) -> Result<String, StoreError> {
        let mut g = self.lock()?;
        g.next_job += 1;
        let seq = g.next_job;
        let id = format!("j{seq}");
        g.created.insert(id.clone(), seq);
        g.jobs.insert(
            id.clone(),
            JobRecord {
                job_id: id.clone(),
                asset_id: job.asset_id,
                spec: job.spec,
                source_key: job.source_key,
                source_size: job.source_size,
                source_extension: job.source_extension,
                priority: job.priority,
                state: JobState::Queued,
                attempt: 0,
                worker_id: None,
                lease_expires_at: None,
                cancel_requested: false,
                progress: None,
                error: None,
                log_tail: None,
                outputs: Vec::new(),
                segment_index: Vec::new(),
                encoder: None,
            },
        );
        Ok(id)
    }

    async fn claim_next(
        &self,
        worker_id: &str,
        kinds: &[JobKind],
        now: u64,
        lease_secs: u64,
    ) -> Result<Option<JobRecord>, StoreError> {
        let mut g = self.lock()?;
        let best = g
            .jobs
            .values()
            .filter(|j| j.state == JobState::Queued && kinds.contains(&j.spec.kind()))
            .min_by_key(|j| {
                (
                    std::cmp::Reverse(j.priority),
                    g.created.get(&j.job_id).copied().unwrap_or(u64::MAX),
                )
            })
            .map(|j| j.job_id.clone());
        let Some(id) = best else { return Ok(None) };
        let job = g.jobs.get_mut(&id).ok_or(StoreError::NotFound)?;
        job.state = JobState::Running;
        job.attempt += 1;
        job.worker_id = Some(worker_id.to_string());
        job.lease_expires_at = Some(now + lease_secs);
        job.progress = None;
        job.error = None;
        let out = job.clone();
        g.uploads.remove(&id);
        Ok(Some(out))
    }

    async fn get_job(&self, job_id: &str) -> Result<Option<JobRecord>, StoreError> {
        Ok(self.lock()?.jobs.get(job_id).cloned())
    }

    async fn report_progress(
        &self,
        job_id: &str,
        worker_id: &str,
        progress: &ProgressRequest,
        now: u64,
        lease_secs: u64,
    ) -> Result<JobRecord, StoreError> {
        let mut g = self.lock()?;
        let job = g.jobs.get_mut(job_id).ok_or(StoreError::NotFound)?;
        if job.worker_id.as_deref() != Some(worker_id) || job.state != JobState::Running {
            return Err(StoreError::Conflict("job is not running on this worker"));
        }
        job.progress = Some(progress.clone());
        job.lease_expires_at = Some(now + lease_secs);
        Ok(job.clone())
    }

    async fn add_upload(&self, job_id: &str, upload: UploadRecord) -> Result<(), StoreError> {
        let mut g = self.lock()?;
        if !g.jobs.contains_key(job_id) {
            return Err(StoreError::NotFound);
        }
        let list = g.uploads.entry(job_id.to_string()).or_default();
        list.retain(|u| u.path != upload.path);
        list.push(upload);
        Ok(())
    }

    async fn finish_upload(
        &self,
        job_id: &str,
        upload_id: &str,
    ) -> Result<UploadRecord, StoreError> {
        let mut g = self.lock()?;
        let u = g
            .uploads
            .get_mut(job_id)
            .and_then(|l| l.iter_mut().find(|u| u.upload_id == upload_id))
            .ok_or(StoreError::NotFound)?;
        u.finished = true;
        Ok(u.clone())
    }

    async fn uploads(&self, job_id: &str) -> Result<Vec<UploadRecord>, StoreError> {
        Ok(self
            .lock()?
            .uploads
            .get(job_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn complete_job(
        &self,
        job_id: &str,
        worker_id: &str,
        req: &CompleteRequest,
    ) -> Result<(), StoreError> {
        let mut g = self.lock()?;
        let job = g.jobs.get_mut(job_id).ok_or(StoreError::NotFound)?;
        if job.worker_id.as_deref() != Some(worker_id) || job.state != JobState::Running {
            return Err(StoreError::Conflict("job is not running on this worker"));
        }
        job.state = JobState::Succeeded;
        job.outputs = req.outputs.clone();
        job.segment_index = req.segment_index.clone();
        job.encoder = Some(req.encoder.clone());
        job.log_tail = req.log_tail.clone();
        job.lease_expires_at = None;
        Ok(())
    }

    async fn fail_job(
        &self,
        job_id: &str,
        worker_id: &str,
        req: &FailRequest,
        max_attempts: u32,
    ) -> Result<FailOutcome, StoreError> {
        let mut g = self.lock()?;
        let job = g.jobs.get_mut(job_id).ok_or(StoreError::NotFound)?;
        if job.worker_id.as_deref() != Some(worker_id) || job.state != JobState::Running {
            return Err(StoreError::Conflict("job is not running on this worker"));
        }
        job.log_tail = req.log_tail.clone();
        if req.cancelled {
            job.cancel_requested = true;
        }
        Ok(requeue_or_fail(
            job,
            req.retryable,
            &req.message,
            max_attempts,
        ))
    }

    async fn request_cancel(&self, job_id: &str) -> Result<(), StoreError> {
        let mut g = self.lock()?;
        let job = g.jobs.get_mut(job_id).ok_or(StoreError::NotFound)?;
        match job.state {
            JobState::Queued => job.state = JobState::Cancelled,
            JobState::Running => job.cancel_requested = true,
            _ => return Err(StoreError::Conflict("job already finished")),
        }
        Ok(())
    }

    async fn reap_expired(&self, now: u64, max_attempts: u32) -> Result<Vec<String>, StoreError> {
        let mut g = self.lock()?;
        let mut hit = Vec::new();
        for job in g.jobs.values_mut() {
            if job.state == JobState::Running && job.lease_expires_at.is_some_and(|l| l <= now) {
                requeue_or_fail(job, true, "worker lease expired", max_attempts);
                hit.push(job.job_id.clone());
            }
        }
        Ok(hit)
    }
}

/// In-memory [`OutputStorage`] that records what was asked of it.
#[derive(Default)]
pub struct MemoryOutputStorage {
    pub completed: Mutex<Vec<(String, Vec<PartDone>)>>,
    counter: Mutex<u64>,
}

#[async_trait]
impl OutputStorage for MemoryOutputStorage {
    async fn init_multipart(&self, key: &str, _content_type: &str) -> Result<String, String> {
        let mut c = self.counter.lock().map_err(|_| "poisoned".to_string())?;
        *c += 1;
        Ok(format!("mp-{}-{}", *c, key.len()))
    }
    async fn presign_part(
        &self,
        key: &str,
        upload_id: &str,
        part_number: u32,
        ttl_secs: u64,
    ) -> Result<String, String> {
        Ok(format!(
            "https://storage.invalid/{key}?uploadId={upload_id}&partNumber={part_number}&ttl={ttl_secs}"
        ))
    }
    async fn complete_multipart(
        &self,
        key: &str,
        _upload_id: &str,
        parts: &[PartDone],
    ) -> Result<(), String> {
        self.completed
            .lock()
            .map_err(|_| "poisoned".to_string())?
            .push((key.to_string(), parts.to_vec()));
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use crate::fixtures::*;
    use crate::plan::{TranscodeSettings, plan_ladder_job, plan_proxy_job};
    use crate::probe::parse_ffprobe_json;
    use std::sync::atomic::{AtomicU64, Ordering};

    pub struct Harness {
        pub svc: WorkerService,
        pub store: Arc<MemoryJobStore>,
        pub outputs: Arc<MemoryOutputStorage>,
        pub time: Arc<AtomicU64>,
    }

    struct Signer;
    impl UrlSigner for Signer {
        fn signed_url(&self, path: &str, expires_at: u64) -> String {
            format!("{path}?ex={expires_at}&sig=S")
        }
    }

    pub const TOKEN: &str = "worker-secret-1";

    pub fn harness() -> Harness {
        let store = Arc::new(MemoryJobStore::new());
        store.add_token(
            TOKEN,
            TokenInfo {
                token_id: 1,
                name: "gpu-box".into(),
            },
        );
        store.add_token(
            "other-secret",
            TokenInfo {
                token_id: 2,
                name: "other".into(),
            },
        );
        let outputs = Arc::new(MemoryOutputStorage::default());
        let time = Arc::new(AtomicU64::new(1_000));
        let t = Arc::clone(&time);
        let svc = WorkerService {
            store: store.clone(),
            outputs: outputs.clone(),
            downloads: Arc::new(Signer),
            clock: Arc::new(move || t.load(Ordering::SeqCst)),
            config: ServiceConfig::default(),
        };
        Harness {
            svc,
            store,
            outputs,
            time,
        }
    }

    pub fn ladder_spec() -> JobSpec {
        let probe = parse_ffprobe_json(H264_SDR.as_bytes()).unwrap();
        JobSpec::Ladder {
            plan: Box::new(plan_ladder_job(&probe, &TranscodeSettings::default()).unwrap()),
        }
    }

    pub fn proxy_spec() -> JobSpec {
        let probe = parse_ffprobe_json(H264_SDR.as_bytes()).unwrap();
        JobSpec::Proxy {
            plan: Box::new(plan_proxy_job(&probe, &TranscodeSettings::default()).unwrap()),
        }
    }

    pub fn new_job(spec: JobSpec, priority: u8) -> NewJob {
        NewJob {
            asset_id: 77,
            spec,
            source_key: "originals/77/source.mkv".into(),
            source_size: 4_100_000_000,
            source_extension: "mkv".into(),
            priority,
        }
    }

    pub fn register_req() -> RegisterRequest {
        RegisterRequest {
            name: "gpu-box".into(),
            version: "0.1.0".into(),
            capabilities: WorkerCapabilities {
                nvenc: true,
                encoders: vec!["h264_nvenc".into(), "libx264".into()],
                hwaccels: vec!["cuda".into()],
                concurrency: 2,
                ffmpeg_version: Some("7.0".into()),
                libplacebo: false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;
    use std::sync::atomic::Ordering;

    async fn caller(h: &Harness) -> Caller {
        h.svc.authenticate(Some(TOKEN)).await.unwrap()
    }

    #[tokio::test]
    async fn auth_register_heartbeat() {
        let h = harness();
        assert_eq!(h.svc.authenticate(None).await, Err(ApiError::Unauthorized));
        assert_eq!(
            h.svc.authenticate(Some("")).await,
            Err(ApiError::Unauthorized)
        );
        assert_eq!(
            h.svc.authenticate(Some("nope")).await,
            Err(ApiError::Unauthorized)
        );
        let c = caller(&h).await;
        let r = h.svc.register(&c, register_req()).await.unwrap();
        assert_eq!(r.heartbeat_secs, 20);
        // Re-registering the same name keeps the id.
        let r2 = h.svc.register(&c, register_req()).await.unwrap();
        assert_eq!(r.worker_id, r2.worker_id);
        let hb = h
            .svc
            .heartbeat(&c, &r.worker_id, HeartbeatRequest::default())
            .await
            .unwrap();
        assert!(hb.cancel_jobs.is_empty() && !hb.drain);
        // Another token cannot use this worker.
        let other = h.svc.authenticate(Some("other-secret")).await.unwrap();
        assert_eq!(
            h.svc
                .heartbeat(&other, &r.worker_id, HeartbeatRequest::default())
                .await,
            Err(ApiError::Forbidden)
        );
    }

    #[tokio::test]
    async fn claim_order_priority_then_age_and_signed_download() {
        let h = harness();
        let c = caller(&h).await;
        let w = h.svc.register(&c, register_req()).await.unwrap().worker_id;
        let low = h.store.enqueue(new_job(ladder_spec(), 1)).await.unwrap();
        let proxy = h.store.enqueue(new_job(proxy_spec(), 9)).await.unwrap();
        let low2 = h.store.enqueue(new_job(ladder_spec(), 1)).await.unwrap();
        let first = h
            .svc
            .claim(&c, &w, ClaimRequest::default())
            .await
            .unwrap()
            .job
            .unwrap();
        assert_eq!(first.job_id, proxy);
        assert_eq!(first.attempt, 1);
        assert_eq!(
            first.source.download_url,
            format!("/cdn/originals/77/source.mkv?ex={}&sig=S", 1_000 + 8 * 3600)
        );
        assert_eq!(first.lease_expires_at, 1_120);
        assert_eq!(first.max_output_bytes, 20 * 1024 * 1024 * 1024);
        let second = h
            .svc
            .claim(
                &c,
                &w,
                ClaimRequest {
                    kinds: vec![JobKind::Ladder],
                },
            )
            .await
            .unwrap()
            .job
            .unwrap();
        assert_eq!(second.job_id, low);
        let third = h
            .svc
            .claim(&c, &w, ClaimRequest::default())
            .await
            .unwrap()
            .job
            .unwrap();
        assert_eq!(third.job_id, low2);
        assert!(
            h.svc
                .claim(&c, &w, ClaimRequest::default())
                .await
                .unwrap()
                .job
                .is_none()
        );
    }

    #[tokio::test]
    async fn remux_claim_signs_every_input_and_sandboxes_the_output() {
        use crate::remux::{RemuxFile, RemuxRequest, RemuxTrack, plan_remux};
        let h = harness();
        let c = caller(&h).await;
        let w = h.svc.register(&c, register_req()).await.unwrap().worker_id;
        let track = |dir: &str| RemuxTrack {
            playlist: format!("{dir}/index.m3u8"),
            init: format!("{dir}/init.mp4"),
            segments: vec![RemuxFile {
                path: format!("{dir}/seg_00000.m4s"),
                size_bytes: 10,
            }],
            playlist_size: 1,
            init_size: 2,
        };
        let plan = plan_remux(&RemuxRequest {
            rendition: "1080p".into(),
            cdn_base: "/cdn/hls/9/1".into(),
            video: track("v/1080p"),
            audio: vec![track("a/00-eng-stereo")],
            duration_secs: 6.0,
        })
        .unwrap();
        let spec = JobSpec::Remux {
            plan: Box::new(plan),
        };
        assert_eq!(spec.kind(), JobKind::Remux);
        assert_eq!(spec.required_outputs(), vec!["remux/video.mp4".to_string()]);
        h.store
            .enqueue(NewJob {
                asset_id: 77,
                spec: spec.clone(),
                source_key: String::new(),
                source_size: 26,
                source_extension: "m3u8".into(),
                priority: 3,
            })
            .await
            .unwrap();
        let job = h
            .svc
            .claim(&c, &w, ClaimRequest::default())
            .await
            .unwrap()
            .job
            .unwrap();
        assert_eq!(job.inputs.len(), 6);
        assert_eq!(
            job.inputs[0].url,
            format!(
                "/cdn/hls/9/1/v/1080p/index.m3u8?ex={}&sig=S",
                1_000 + 8 * 3600
            )
        );
        assert_eq!(job.inputs[0].path, "v/1080p/index.m3u8");
        assert!(
            h.svc
                .init_upload(
                    &c,
                    &job.job_id,
                    InitUploadRequest {
                        path: "remux/video.mp4".into(),
                        size: 100
                    }
                )
                .await
                .is_ok()
        );
        // The remux job may write nothing but its own directory.
        assert!(
            h.svc
                .init_upload(
                    &c,
                    &job.job_id,
                    InitUploadRequest {
                        path: "v/1080p/index.m3u8".into(),
                        size: 100
                    }
                )
                .await
                .is_err()
        );
        let wire = serde_json::to_string(&job).unwrap();
        assert!(wire.contains("\"inputs\""));
        let back: ClaimedJob = serde_json::from_str(&wire).unwrap();
        assert_eq!(back, job);
    }

    #[tokio::test]
    async fn drained_worker_gets_no_jobs() {
        let h = harness();
        let c = caller(&h).await;
        let w = h.svc.register(&c, register_req()).await.unwrap().worker_id;
        h.store.enqueue(new_job(ladder_spec(), 1)).await.unwrap();
        h.store.set_drain(&w, true);
        assert!(
            h.svc
                .claim(&c, &w, ClaimRequest::default())
                .await
                .unwrap()
                .job
                .is_none()
        );
        let hb = h
            .svc
            .heartbeat(&c, &w, HeartbeatRequest::default())
            .await
            .unwrap();
        assert!(hb.drain);
    }

    async fn claimed(h: &Harness, spec: JobSpec) -> (Caller, String, ClaimedJob) {
        let c = caller(h).await;
        let w = h.svc.register(&c, register_req()).await.unwrap().worker_id;
        h.store.enqueue(new_job(spec, 5)).await.unwrap();
        let job = h
            .svc
            .claim(&c, &w, ClaimRequest::default())
            .await
            .unwrap()
            .job
            .unwrap();
        (c, w, job)
    }

    #[tokio::test]
    async fn progress_extends_lease_and_surfaces_cancel() {
        let h = harness();
        let (c, w, job) = claimed(&h, ladder_spec()).await;
        h.time.store(1_100, Ordering::SeqCst);
        let req = ProgressRequest {
            stage: Stage::Video,
            percent: 250.0,
            fps: Some(120.0),
            speed: Some(4.0),
            out_time_secs: Some(60.0),
            message: Some("hello\u{7}".into()),
        };
        let r = h.svc.progress(&c, &job.job_id, req.clone()).await.unwrap();
        assert!(!r.cancel);
        assert_eq!(r.lease_expires_at, 1_220);
        let rec = h.store.get_job(&job.job_id).await.unwrap().unwrap();
        assert_eq!(rec.progress.as_ref().unwrap().percent, 100.0);
        assert_eq!(rec.progress.unwrap().message.as_deref(), Some("hello"));
        h.store.request_cancel(&job.job_id).await.unwrap();
        assert!(
            h.svc
                .progress(&c, &job.job_id, req.clone())
                .await
                .unwrap()
                .cancel
        );
        let hb = h
            .svc
            .heartbeat(&c, &w, HeartbeatRequest::default())
            .await
            .unwrap();
        assert_eq!(hb.cancel_jobs, std::slice::from_ref(&job.job_id));
        let bad = ProgressRequest {
            percent: f32::NAN,
            ..req
        };
        assert!(matches!(
            h.svc.progress(&c, &job.job_id, bad).await,
            Err(ApiError::BadRequest(_))
        ));
    }

    #[tokio::test]
    async fn expired_lease_is_gone_and_reaped() {
        let h = harness();
        let (c, _w, job) = claimed(&h, ladder_spec()).await;
        h.time.store(2_000, Ordering::SeqCst);
        let req = ProgressRequest {
            stage: Stage::Video,
            percent: 1.0,
            fps: None,
            speed: None,
            out_time_secs: None,
            message: None,
        };
        assert_eq!(
            h.svc.progress(&c, &job.job_id, req).await,
            Err(ApiError::Gone)
        );
        let reaped = h.store.reap_expired(2_000, 3).await.unwrap();
        assert_eq!(reaped, std::slice::from_ref(&job.job_id));
        let rec = h.store.get_job(&job.job_id).await.unwrap().unwrap();
        assert_eq!(rec.state, JobState::Queued);
        assert_eq!(rec.attempt, 1);
    }

    #[tokio::test]
    async fn another_worker_cannot_touch_the_job() {
        let h = harness();
        let (_c, _w, job) = claimed(&h, ladder_spec()).await;
        let other = h.svc.authenticate(Some("other-secret")).await.unwrap();
        let req = InitUploadRequest {
            path: "subs/00-eng.vtt".into(),
            size: 10,
        };
        assert_eq!(
            h.svc.init_upload(&other, &job.job_id, req).await,
            Err(ApiError::Forbidden)
        );
        assert_eq!(
            h.svc
                .init_upload(
                    &other,
                    "nope",
                    InitUploadRequest {
                        path: "x".into(),
                        size: 1
                    }
                )
                .await,
            Err(ApiError::NotFound)
        );
    }

    #[test]
    fn output_path_sandbox() {
        let allowed = vec!["v/1080p/".to_string(), "subs/".to_string()];
        assert_eq!(
            validate_output_path("v/1080p/seg_00001.m4s", &allowed),
            Ok("video/iso.segment")
        );
        assert_eq!(
            validate_output_path("subs/00-eng.vtt", &allowed),
            Ok("text/vtt")
        );
        for bad in [
            "",
            "/etc/passwd",
            "../x.vtt",
            "subs/../../x.vtt",
            "subs/.hidden.vtt",
            "subs\\x.vtt",
            "subs/a b/../c.vtt",
            "subs/x.exe",
            "subs/x",
            "other/x.vtt",
            "v/1080p/a/b/c/d.m4s",
            "subs//x.vtt",
            "subs/x\0.vtt",
            "subs/x:y.vtt",
        ] {
            assert!(
                matches!(
                    validate_output_path(bad, &allowed),
                    Err(ApiError::BadRequest(_))
                ),
                "{bad:?}"
            );
        }
        assert!(validate_output_path(&format!("subs/{}.vtt", "a".repeat(300)), &allowed).is_err());
    }

    #[tokio::test]
    async fn upload_complete_flow() {
        let h = harness();
        let (c, _w, job) = claimed(&h, proxy_spec()).await;
        let JobSpec::Proxy { plan } = &job.spec else {
            panic!()
        };
        let size = 40 * 1024 * 1024u64; // three 16 MiB parts
        let init = h
            .svc
            .init_upload(
                &c,
                &job.job_id,
                InitUploadRequest {
                    path: plan.output.clone(),
                    size,
                },
            )
            .await
            .unwrap();
        assert_eq!(init.parts.len(), 3);
        assert_eq!(init.content_type, "video/mp4");
        assert!(init.parts[1].url.contains("partNumber=2"));
        assert!(
            init.parts[0]
                .url
                .contains(&format!("transcode/{}/1/proxy/proxy.mp4", job.job_id))
        );

        // Completing before the upload is finished is refused.
        let complete = CompleteRequest {
            outputs: vec![OutputRef {
                path: plan.output.clone(),
                size,
            }],
            segment_index: vec![],
            encoder: "libx264".into(),
            wall_secs: 12.0,
            log_tail: Some("ok".into()),
        };
        assert!(matches!(
            h.svc.complete(&c, &job.job_id, complete.clone()).await,
            Err(ApiError::BadRequest(_))
        ));
        // Wrong part set is refused, the right one accepted.
        let two = FinishUploadRequest {
            parts: vec![
                PartDone {
                    part_number: 1,
                    etag: "a".into(),
                },
                PartDone {
                    part_number: 2,
                    etag: "b".into(),
                },
            ],
        };
        assert!(matches!(
            h.svc
                .finish_upload(&c, &job.job_id, &init.upload_id, two)
                .await,
            Err(ApiError::BadRequest(_))
        ));
        let three = FinishUploadRequest {
            parts: (1..=3)
                .map(|n| PartDone {
                    part_number: n,
                    etag: format!("e{n}"),
                })
                .collect(),
        };
        h.svc
            .finish_upload(&c, &job.job_id, &init.upload_id, three)
            .await
            .unwrap();
        assert_eq!(h.outputs.completed.lock().unwrap().len(), 1);
        // Size mismatch in the completion is refused.
        let mut wrong = complete.clone();
        wrong.outputs[0].size = 1;
        assert!(h.svc.complete(&c, &job.job_id, wrong).await.is_err());
        h.svc.complete(&c, &job.job_id, complete).await.unwrap();
        let rec = h.store.get_job(&job.job_id).await.unwrap().unwrap();
        assert_eq!(rec.state, JobState::Succeeded);
        assert_eq!(rec.encoder.as_deref(), Some("libx264"));
        // A finished job accepts nothing more.
        assert!(matches!(
            h.svc
                .progress(
                    &c,
                    &job.job_id,
                    ProgressRequest {
                        stage: Stage::Upload,
                        percent: 1.0,
                        fps: None,
                        speed: None,
                        out_time_secs: None,
                        message: None
                    }
                )
                .await,
            Err(ApiError::Conflict(_))
        ));
    }

    #[tokio::test]
    async fn upload_limits() {
        let h = harness();
        let (c, _w, job) = claimed(&h, ladder_spec()).await;
        let init = |path: &str, size: u64| {
            h.svc.init_upload(
                &c,
                &job.job_id,
                InitUploadRequest {
                    path: path.into(),
                    size,
                },
            )
        };
        assert!(matches!(
            init("subs/x.vtt", 0).await,
            Err(ApiError::BadRequest(_))
        ));
        assert!(matches!(
            init("subs/x.vtt", 5 * 1024 * 1024 * 1024).await,
            Err(ApiError::BadRequest(_))
        ));
        assert!(matches!(
            init("proxy/proxy.mp4", 10).await,
            Err(ApiError::BadRequest(_))
        ));
        assert!(init("v/1080p/index.m3u8", 500).await.is_ok());
    }

    #[tokio::test]
    async fn ladder_complete_needs_segment_index_and_all_outputs() {
        let h = harness();
        let (c, _w, job) = claimed(&h, ladder_spec()).await;
        let JobSpec::Ladder { plan } = &job.spec else {
            panic!()
        };
        // Upload every required output plus one segment per rung.
        let mut outputs = Vec::new();
        let mut paths = plan.required_outputs();
        for r in &plan.rungs {
            paths.push(format!("{}/seg_00000.m4s", r.dir));
        }
        for p in &paths {
            let init = h
                .svc
                .init_upload(
                    &c,
                    &job.job_id,
                    InitUploadRequest {
                        path: p.clone(),
                        size: 1000,
                    },
                )
                .await
                .unwrap();
            h.svc
                .finish_upload(
                    &c,
                    &job.job_id,
                    &init.upload_id,
                    FinishUploadRequest {
                        parts: vec![PartDone {
                            part_number: 1,
                            etag: "e".into(),
                        }],
                    },
                )
                .await
                .unwrap();
            outputs.push(OutputRef {
                path: p.clone(),
                size: 1000,
            });
        }
        let index: Vec<SegmentIndex> = plan
            .rungs
            .iter()
            .map(|r| SegmentIndex {
                playlist: r.playlist.clone(),
                init: "init.mp4".into(),
                segments: vec![SegmentEntry {
                    name: "seg_00000.m4s".into(),
                    duration_secs: 6.006,
                    size_bytes: 1000,
                }],
            })
            .collect();
        let req = CompleteRequest {
            outputs,
            segment_index: index.clone(),
            encoder: "h264_nvenc".into(),
            wall_secs: 600.0,
            log_tail: Some("x".repeat(100_000)),
        };
        let mut missing_index = req.clone();
        missing_index.segment_index.pop();
        assert!(
            h.svc
                .complete(&c, &job.job_id, missing_index)
                .await
                .is_err()
        );
        let mut ghost = req.clone();
        ghost.segment_index[0].segments[0].name = "seg_99999.m4s".into();
        assert!(h.svc.complete(&c, &job.job_id, ghost).await.is_err());
        h.svc.complete(&c, &job.job_id, req).await.unwrap();
        let rec = h.store.get_job(&job.job_id).await.unwrap().unwrap();
        assert_eq!(rec.log_tail.unwrap().len(), MAX_LOG_BYTES);
        assert_eq!(rec.segment_index, index);
    }

    #[tokio::test]
    async fn fail_retry_then_permanent() {
        let h = harness();
        let c = caller(&h).await;
        let w = h.svc.register(&c, register_req()).await.unwrap().worker_id;
        let id = h.store.enqueue(new_job(ladder_spec(), 5)).await.unwrap();
        let fail = |retryable| FailRequest {
            stage: Stage::Video,
            message: "ffmpeg exited 1\u{0}".into(),
            log_tail: Some("Error while decoding\u{1b}[0m".into()),
            retryable,
            cancelled: false,
        };
        for attempt in 1..=2 {
            let job = h
                .svc
                .claim(&c, &w, ClaimRequest::default())
                .await
                .unwrap()
                .job
                .unwrap();
            assert_eq!(job.attempt, attempt);
            h.svc.fail(&c, &id, fail(true)).await.unwrap();
            assert_eq!(
                h.store.get_job(&id).await.unwrap().unwrap().state,
                JobState::Queued
            );
        }
        h.svc
            .claim(&c, &w, ClaimRequest::default())
            .await
            .unwrap()
            .job
            .unwrap();
        h.svc.fail(&c, &id, fail(true)).await.unwrap();
        let rec = h.store.get_job(&id).await.unwrap().unwrap();
        assert_eq!(rec.state, JobState::Failed);
        assert_eq!(rec.error.as_deref(), Some("ffmpeg exited 1"));
        assert_eq!(rec.log_tail.as_deref(), Some("Error while decoding[0m"));
        // Non-retryable fails at once.
        let id2 = h.store.enqueue(new_job(ladder_spec(), 5)).await.unwrap();
        h.svc
            .claim(&c, &w, ClaimRequest::default())
            .await
            .unwrap()
            .job
            .unwrap();
        h.svc.fail(&c, &id2, fail(false)).await.unwrap();
        assert_eq!(
            h.store.get_job(&id2).await.unwrap().unwrap().state,
            JobState::Failed
        );
    }

    #[tokio::test]
    async fn cancel_flow() {
        let h = harness();
        let c = caller(&h).await;
        let w = h.svc.register(&c, register_req()).await.unwrap().worker_id;
        // Queued jobs cancel at once.
        let q = h.store.enqueue(new_job(ladder_spec(), 5)).await.unwrap();
        h.store.request_cancel(&q).await.unwrap();
        assert_eq!(
            h.store.get_job(&q).await.unwrap().unwrap().state,
            JobState::Cancelled
        );
        // Running jobs finish as cancelled when the worker reports it.
        let r = h.store.enqueue(new_job(ladder_spec(), 5)).await.unwrap();
        h.svc
            .claim(&c, &w, ClaimRequest::default())
            .await
            .unwrap()
            .job
            .unwrap();
        h.store.request_cancel(&r).await.unwrap();
        h.svc
            .fail(
                &c,
                &r,
                FailRequest {
                    stage: Stage::Video,
                    message: "cancelled".into(),
                    log_tail: None,
                    retryable: true,
                    cancelled: true,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            h.store.get_job(&r).await.unwrap().unwrap().state,
            JobState::Cancelled
        );
        assert_eq!(
            h.store.request_cancel(&r).await,
            Err(StoreError::Conflict("job already finished"))
        );
    }

    #[test]
    fn wire_format_is_stable() {
        let json = serde_json::to_value(JobKind::Ladder).unwrap();
        assert_eq!(json, "ladder");
        let spec = proxy_spec();
        let v = serde_json::to_value(&spec).unwrap();
        assert_eq!(v["kind"], "proxy");
        let back: JobSpec = serde_json::from_value(v).unwrap();
        assert_eq!(back, spec);
        assert_eq!(ApiError::Gone.status(), 410);
        assert_eq!(ApiError::Unauthorized.code(), "unauthorized");
    }
}
