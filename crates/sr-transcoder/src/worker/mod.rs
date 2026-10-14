//! The worker loop and per-job state machine (CIN-ING-005), written against ports:
//! [`WorkerApi`] (the server), [`Transfer`] (presigned downloads and uploads) and
//! `sr_media::runner::CommandRunner` (ffmpeg).
//!
//! Job states as seen by the worker: `claimed -> downloading -> encoding (step by step, each
//! retried) -> uploading -> completed`, with `failed` (reported with logs), `cancelled`
//! (admin cancel) and `lease lost` (the server gave the job to someone else: stop silently)
//! as the other exits.

use crate::args::{BuildCtx, BuildError, Step, StepKind, ladder_steps, proxy_steps, sandbox_path};
use crate::encoders::{Encoder, FfmpegCaps, looks_like_gpu_failure};
use crate::progress::{ProgressParser, overall_percent};
use sr_media::plan::ExtractFormat;
use sr_media::plan::JobPlan;
use sr_media::playlist::parse_media_playlist;
use sr_media::runner::{CommandRunner, CommandSpec, Control};
use sr_media::subtitles::{SubtitleError, ass_to_webvtt, sprite_vtt, srt_to_webvtt};
use sr_media::worker_api::{
    Ack, ClaimRequest, ClaimResponse, ClaimedJob, CompleteRequest, FailRequest,
    FinishUploadRequest, HeartbeatRequest, HeartbeatResponse, InitUploadRequest,
    InitUploadResponse, JobSpec, OutputRef, PartDone, ProgressRequest, ProgressResponse,
    RegisterRequest, RegisterResponse, SegmentEntry, SegmentIndex, SourceRef, Stage,
    WorkerCapabilities,
};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use thiserror::Error;

// ---- ports --------------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum ApiClientError {
    #[error("server answered {status} {code}: {message}")]
    Status {
        status: u16,
        code: String,
        message: String,
    },
    #[error("transport error: {0}")]
    Transport(String),
}

impl ApiClientError {
    /// The server no longer wants this worker working on the job (unknown, taken away, lease
    /// expired, already finished).
    pub fn is_job_lost(&self) -> bool {
        matches!(
            self,
            Self::Status {
                status: 403 | 404 | 409 | 410,
                ..
            }
        )
    }
}

type ApiResult<T> = Result<T, ApiClientError>;

/// The server side of the Worker API, as a blocking client.
pub trait WorkerApi: Send + Sync {
    fn register(&self, req: &RegisterRequest) -> ApiResult<RegisterResponse>;
    fn heartbeat(&self, worker_id: &str, req: &HeartbeatRequest) -> ApiResult<HeartbeatResponse>;
    fn claim(&self, worker_id: &str, req: &ClaimRequest) -> ApiResult<ClaimResponse>;
    fn progress(&self, job_id: &str, req: &ProgressRequest) -> ApiResult<ProgressResponse>;
    fn init_upload(&self, job_id: &str, req: &InitUploadRequest) -> ApiResult<InitUploadResponse>;
    fn finish_upload(
        &self,
        job_id: &str,
        upload_id: &str,
        req: &FinishUploadRequest,
    ) -> ApiResult<Ack>;
    fn complete(&self, job_id: &str, req: &CompleteRequest) -> ApiResult<Ack>;
    fn fail(&self, job_id: &str, req: &FailRequest) -> ApiResult<Ack>;
}

#[derive(Debug, Error)]
pub enum TransferError {
    #[error("download failed: {0}")]
    Download(String),
    #[error("upload failed: {0}")]
    Upload(String),
}

/// Moves bytes to and from object storage through presigned URLs.
pub trait Transfer: Send + Sync {
    /// Download the original to `dest`. Implementations must stop at `max_bytes`.
    fn download(
        &self,
        source: &SourceRef,
        dest: &Path,
        max_bytes: u64,
    ) -> Result<u64, TransferError>;
    /// PUT every part of `file` to the presigned part URLs; return the ETags.
    fn upload(
        &self,
        init: &InitUploadResponse,
        file: &Path,
    ) -> Result<Vec<PartDone>, TransferError>;
}

// ---- configuration and results ------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub name: String,
    pub version: String,
    pub work_root: PathBuf,
    pub ffmpeg: String,
    pub force_cpu: bool,
    pub concurrency: u32,
    /// Attempts per ffmpeg step before the job is failed.
    pub step_attempts: u32,
    /// Minimum time between progress reports.
    pub progress_interval: Duration,
    /// Step timeout = expected duration x this + 5 minutes.
    pub timeout_factor: f64,
    /// Largest original accepted.
    pub max_source_bytes: u64,
}

impl WorkerConfig {
    pub fn new(work_root: PathBuf) -> Self {
        Self {
            name: "worker".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            work_root,
            ffmpeg: "ffmpeg".to_string(),
            force_cpu: false,
            concurrency: 1,
            step_attempts: 2,
            progress_interval: Duration::from_secs(5),
            timeout_factor: 8.0,
            max_source_bytes: 200 * 1024 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum JobOutcome {
    Completed,
    /// Reported to the server as failed.
    Failed {
        stage: Stage,
        retryable: bool,
    },
    /// An admin cancelled the job; the server was told.
    Cancelled,
    /// The server took the job away; nothing was reported.
    LeaseLost,
}

enum JobError {
    Cancelled,
    LeaseLost,
    Failed {
        stage: Stage,
        message: String,
        log: String,
        retryable: bool,
    },
}

impl JobError {
    fn fail(stage: Stage, message: impl Into<String>, retryable: bool) -> Self {
        Self::Failed {
            stage,
            message: message.into(),
            log: String::new(),
            retryable,
        }
    }
}

fn stage_of(kind: StepKind) -> Stage {
    match kind {
        StepKind::Video | StepKind::Proxy => Stage::Video,
        StepKind::Audio => Stage::Audio,
        StepKind::Subtitle | StepKind::Font => Stage::Subtitles,
        StepKind::Sprites | StepKind::Poster => Stage::Sprites,
    }
}

/// A per-job working directory that is removed on drop.
struct JobDir {
    root: PathBuf,
}

impl JobDir {
    fn create(work_root: &Path, job_id: &str) -> Result<Self, JobError> {
        let ok = !job_id.is_empty()
            && job_id.len() <= 64
            && job_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        if !ok {
            return Err(JobError::fail(Stage::Download, "unsafe job id", false));
        }
        let root = work_root.join(job_id);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("in"))
            .and_then(|()| fs::create_dir_all(root.join("out")))
            .map_err(|e| JobError::fail(Stage::Download, format!("work dir: {e}"), true))?;
        Ok(Self { root })
    }
    fn input(&self, ext: &str) -> PathBuf {
        self.root.join("in").join(format!("source.{ext}"))
    }
    fn out(&self) -> PathBuf {
        self.root.join("out")
    }
}

impl Drop for JobDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn clean_ext(ext: &str) -> String {
    let e: String = ext
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(8)
        .collect::<String>()
        .to_ascii_lowercase();
    if e.is_empty() { "bin".to_string() } else { e }
}

/// Stderr that means the input is bad (retrying cannot help).
fn is_bad_input(stderr: &str) -> bool {
    [
        "Invalid data found when processing input",
        "moov atom not found",
        "Unsupported codec",
        "does not contain any stream",
        "Output file does not contain any stream",
    ]
    .iter()
    .any(|m| stderr.contains(m))
}

// ---- the worker ---------------------------------------------------------------------------

pub struct Worker<'a> {
    pub api: &'a dyn WorkerApi,
    pub transfer: &'a dyn Transfer,
    pub runner: &'a dyn CommandRunner,
    pub cfg: WorkerConfig,
    pub caps: FfmpegCaps,
    /// Job ids the server asked us to cancel (filled by the heartbeat).
    cancelled: Mutex<HashSet<String>>,
    draining: AtomicBool,
}

struct Reporter<'r> {
    job_id: &'r str,
    last: Instant,
    interval: Duration,
}

impl<'a> Worker<'a> {
    pub fn new(
        api: &'a dyn WorkerApi,
        transfer: &'a dyn Transfer,
        runner: &'a dyn CommandRunner,
        cfg: WorkerConfig,
        caps: FfmpegCaps,
    ) -> Self {
        Self {
            api,
            transfer,
            runner,
            cfg,
            caps,
            cancelled: Mutex::new(HashSet::new()),
            draining: AtomicBool::new(false),
        }
    }

    pub fn capabilities(&self) -> WorkerCapabilities {
        WorkerCapabilities {
            nvenc: self.caps.nvenc_available() && !self.cfg.force_cpu,
            encoders: self.caps.encoders.iter().cloned().collect(),
            hwaccels: self.caps.hwaccels.iter().cloned().collect(),
            concurrency: self.cfg.concurrency,
            ffmpeg_version: self.caps.version.clone(),
            libplacebo: self.caps.has_libplacebo(),
        }
    }

    fn is_cancelled(&self, job_id: &str) -> bool {
        self.cancelled.lock().is_ok_and(|s| s.contains(job_id))
    }

    /// Send a progress report. `Err` when the job must stop.
    fn report(
        &self,
        r: &mut Reporter,
        stage: Stage,
        percent: f32,
        sample: Option<&crate::progress::ProgressSample>,
        force: bool,
    ) -> Result<(), JobError> {
        if self.is_cancelled(r.job_id) {
            return Err(JobError::Cancelled);
        }
        if !force && r.last.elapsed() < r.interval {
            return Ok(());
        }
        r.last = Instant::now();
        let req = ProgressRequest {
            stage,
            percent,
            fps: sample.and_then(|s| s.fps),
            speed: sample.and_then(|s| s.speed),
            out_time_secs: sample.and_then(|s| s.out_time_secs),
            message: None,
        };
        match self.api.progress(r.job_id, &req) {
            Ok(resp) if resp.cancel => Err(JobError::Cancelled),
            Ok(_) => Ok(()),
            Err(e) if e.is_job_lost() => Err(JobError::LeaseLost),
            // A flaky network must not kill a long encode; the lease has slack.
            Err(_) => Ok(()),
        }
    }

    /// Claim and process at most one job. Returns `None` when the queue was empty.
    pub fn poll_once(&self, worker_id: &str) -> Result<Option<JobOutcome>, ApiClientError> {
        let resp = self.api.claim(worker_id, &ClaimRequest::default())?;
        Ok(resp.job.map(|job| self.process_job(&job)))
    }

    /// Run one claimed job to a terminal state, reporting to the server.
    pub fn process_job(&self, job: &ClaimedJob) -> JobOutcome {
        let started = Instant::now();
        let mut notes = String::new();
        let result = self.run_job(job, started, &mut notes);
        if let Ok(mut set) = self.cancelled.lock() {
            set.remove(&job.job_id);
        }
        match result {
            Ok(req) => match self.api.complete(&job.job_id, &req) {
                Ok(_) => JobOutcome::Completed,
                Err(e) if e.is_job_lost() => JobOutcome::LeaseLost,
                Err(e) => {
                    // The job is done locally but the server did not learn it: report a
                    // retryable failure so it is requeued rather than left leased.
                    let _ = self.api.fail(
                        &job.job_id,
                        &FailRequest {
                            stage: Stage::Upload,
                            message: format!("could not report completion: {e}"),
                            log_tail: Some(notes),
                            retryable: true,
                            cancelled: false,
                        },
                    );
                    JobOutcome::Failed {
                        stage: Stage::Upload,
                        retryable: true,
                    }
                }
            },
            Err(JobError::LeaseLost) => JobOutcome::LeaseLost,
            Err(JobError::Cancelled) => {
                let _ = self.api.fail(
                    &job.job_id,
                    &FailRequest {
                        stage: Stage::Video,
                        message: "cancelled".to_string(),
                        log_tail: None,
                        retryable: false,
                        cancelled: true,
                    },
                );
                JobOutcome::Cancelled
            }
            Err(JobError::Failed {
                stage,
                message,
                log,
                retryable,
            }) => {
                let _ = self.api.fail(
                    &job.job_id,
                    &FailRequest {
                        stage,
                        message,
                        log_tail: Some(format!("{notes}{log}")),
                        retryable,
                        cancelled: false,
                    },
                );
                JobOutcome::Failed { stage, retryable }
            }
        }
    }

    fn run_job(
        &self,
        job: &ClaimedJob,
        started: Instant,
        notes: &mut String,
    ) -> Result<CompleteRequest, JobError> {
        let dir = JobDir::create(&self.cfg.work_root, &job.job_id)?;
        let mut rep = Reporter {
            job_id: &job.job_id,
            last: Instant::now()
                .checked_sub(self.cfg.progress_interval)
                .unwrap_or_else(Instant::now),
            interval: self.cfg.progress_interval,
        };
        self.report(&mut rep, Stage::Download, 0.0, None, true)?;

        // 1. download
        if job.source.size_bytes > self.cfg.max_source_bytes {
            return Err(JobError::fail(
                Stage::Download,
                "original is too large",
                false,
            ));
        }
        let input = dir.input(&clean_ext(&job.source.extension));
        let got = self
            .transfer
            .download(&job.source, &input, job.source.size_bytes)
            .map_err(|e| JobError::fail(Stage::Download, e.to_string(), true))?;
        if got != job.source.size_bytes {
            return Err(JobError::fail(
                Stage::Download,
                format!("downloaded {got} bytes, expected {}", job.source.size_bytes),
                true,
            ));
        }
        self.report(&mut rep, Stage::Download, 5.0, None, true)?;

        // 2. encode
        let (duration, plan_ladder) = match &job.spec {
            JobSpec::Ladder { plan } => (plan.source.duration_secs, Some(plan.as_ref())),
            JobSpec::Proxy { plan } => (plan.duration_secs, None),
        };
        let mut encoder = self.caps.choose(self.cfg.force_cpu).ok_or_else(|| {
            JobError::fail(Stage::Video, BuildError::NoEncoder.to_string(), false)
        })?;
        let input_str = input.to_string_lossy().replace('\\', "/");
        let out = dir.out();
        let build = |enc: Encoder| -> Result<Vec<Step>, JobError> {
            let ctx = BuildCtx {
                input: &input_str,
                out_dir: &out,
                encoder: enc,
                caps: &self.caps,
                duration_secs: duration,
            };
            match &job.spec {
                JobSpec::Ladder { plan } => ladder_steps(plan, &ctx),
                JobSpec::Proxy { plan } => proxy_steps(plan, &ctx),
            }
            .map_err(|e| JobError::fail(Stage::Video, e.to_string(), false))
        };
        let mut steps = build(encoder)?;
        let mut segment_index: Vec<SegmentIndex> = Vec::new();
        let total_weight: f64 = steps.iter().map(|s| s.weight).sum();
        let mut done_weight = 0.0;
        let mut i = 0;
        while i < steps.len() {
            let mut step = steps[i].clone();
            let mut attempt = 1;
            loop {
                match self.run_step(&step, &out, &mut rep, done_weight, total_weight) {
                    Ok(()) => break,
                    Err(StepFailure::Job(e)) => return Err(e),
                    Err(StepFailure::Exit { log, timed_out }) => {
                        let gpu_step = matches!(step.kind, StepKind::Video | StepKind::Proxy);
                        if encoder == Encoder::H264Nvenc && gpu_step && looks_like_gpu_failure(&log)
                        {
                            notes.push_str("NVENC failed; falling back to libx264.\n");
                            encoder = Encoder::Libx264;
                            steps = build(encoder)?;
                            step = steps[i].clone();
                            attempt = 1;
                            continue;
                        }
                        if is_bad_input(&log) {
                            return Err(JobError::Failed {
                                stage: stage_of(step.kind),
                                message: format!(
                                    "{} failed: the file could not be decoded",
                                    step.label
                                ),
                                log,
                                retryable: false,
                            });
                        }
                        if attempt < self.cfg.step_attempts {
                            attempt += 1;
                            notes.push_str(&format!(
                                "Retrying {} (attempt {attempt}).\n",
                                step.label
                            ));
                            continue;
                        }
                        return Err(JobError::Failed {
                            stage: stage_of(step.kind),
                            message: format!(
                                "{} {}",
                                step.label,
                                if timed_out { "timed out" } else { "failed" }
                            ),
                            log,
                            retryable: true,
                        });
                    }
                }
            }
            if let Some(plan) = plan_ladder {
                self.post_process(plan, &step, &out, &mut segment_index)?;
            }
            done_weight += step.weight;
            i += 1;
        }

        // 3. upload
        let outputs = self.upload_all(job, &out, &mut rep)?;
        Ok(CompleteRequest {
            outputs,
            segment_index,
            encoder: encoder.name().to_string(),
            wall_secs: started.elapsed().as_secs_f64(),
            log_tail: (!notes.is_empty()).then(|| notes.clone()),
        })
    }

    fn prepare_dirs(&self, step: &Step, out: &Path) -> Result<(), JobError> {
        let io =
            |e: std::io::Error| JobError::fail(stage_of(step.kind), format!("work dir: {e}"), true);
        for rel in &step.wipe {
            let p = sandbox_path(out, rel)
                .map_err(|e| JobError::fail(stage_of(step.kind), e.to_string(), false))?;
            match fs::remove_dir_all(&p) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(io(e)),
            }
            fs::create_dir_all(&p).map_err(io)?;
        }
        for rel in &step.mkdir {
            let p = sandbox_path(out, rel)
                .map_err(|e| JobError::fail(stage_of(step.kind), e.to_string(), false))?;
            fs::create_dir_all(&p).map_err(io)?;
        }
        Ok(())
    }

    fn run_step(
        &self,
        step: &Step,
        out: &Path,
        rep: &mut Reporter,
        done_weight: f64,
        total_weight: f64,
    ) -> Result<(), StepFailure> {
        self.prepare_dirs(step, out).map_err(StepFailure::Job)?;
        let timeout = Duration::from_secs_f64(
            (step.expect_secs * self.cfg.timeout_factor + 300.0).min(24.0 * 3600.0),
        );
        let spec =
            CommandSpec::new(self.cfg.ffmpeg.clone(), step.args.clone()).with_timeout(timeout);
        let stage = stage_of(step.kind);
        let mut parser = ProgressParser::new();
        let mut abort: Option<JobError> = None;
        let result = self.runner.run_streaming(&spec, &mut |line| {
            if let Some(sample) = parser.feed(line) {
                let pct = overall_percent(
                    done_weight,
                    total_weight,
                    step.weight,
                    sample.out_time_secs,
                    step.expect_secs,
                );
                // Encoding spans 5%..90% of the whole job.
                let mapped = 5.0 + pct * 0.85;
                if let Err(e) = self.report(rep, stage, mapped, Some(&sample), false) {
                    abort = Some(e);
                    return Control::Kill;
                }
            }
            Control::Continue
        });
        if let Some(e) = abort {
            return Err(StepFailure::Job(e));
        }
        if self.is_cancelled(rep.job_id) {
            return Err(StepFailure::Job(JobError::Cancelled));
        }
        let info = result.map_err(|e| {
            StepFailure::Job(JobError::fail(
                stage,
                format!("could not run ffmpeg: {e}"),
                false,
            ))
        })?;
        if info.success() {
            Ok(())
        } else {
            let mut log = format!("$ ffmpeg {}\n", step.args.join(" "));
            if let Some(c) = info.exit_code {
                log.push_str(&format!("exit code {c}\n"));
            }
            log.push_str(&info.stderr_tail);
            Err(StepFailure::Exit {
                log,
                timed_out: info.timed_out,
            })
        }
    }

    /// Work that follows a successful ffmpeg step: segment index, WebVTT, sprite map.
    fn post_process(
        &self,
        plan: &JobPlan,
        step: &Step,
        out: &Path,
        index: &mut Vec<SegmentIndex>,
    ) -> Result<(), JobError> {
        let stage = stage_of(step.kind);
        let bad = |m: String| JobError::fail(stage, m, true);
        match step.kind {
            StepKind::Video => {
                for r in &plan.rungs {
                    index.push(read_segment_index(out, &r.dir, &r.playlist).map_err(bad)?);
                }
            }
            StepKind::Audio => {
                let a = &plan.audio[step.index];
                index.push(read_segment_index(out, &a.dir, &a.playlist).map_err(bad)?);
            }
            StepKind::Subtitle => {
                let s = &plan.subtitles[step.index];
                let raw_path = sandbox_path(out, &s.raw_path).map_err(|e| bad(e.to_string()))?;
                let raw = fs::read(&raw_path).map_err(|e| bad(format!("subtitle output: {e}")))?;
                let vtt = match s.extract {
                    ExtractFormat::Ass => ass_to_webvtt(&raw),
                    ExtractFormat::Srt => srt_to_webvtt(&raw),
                };
                let text = match vtt {
                    Ok(t) => t,
                    // An empty track is still a track: keep the required output present.
                    Err(SubtitleError::NoCues) => "WEBVTT\n\n".to_string(),
                    Err(e) => {
                        return Err(JobError::fail(
                            stage,
                            format!("subtitle conversion: {e}"),
                            false,
                        ));
                    }
                };
                let vtt_path = sandbox_path(out, &s.vtt_path).map_err(|e| bad(e.to_string()))?;
                fs::write(vtt_path, text).map_err(|e| bad(e.to_string()))?;
                if !s.keep_raw {
                    let _ = fs::remove_file(raw_path);
                }
            }
            StepKind::Font | StepKind::Poster => {
                let rel = match step.kind {
                    StepKind::Font => &plan.fonts[step.index].path,
                    _ => &plan.posters[step.index].path,
                };
                let p = sandbox_path(out, rel).map_err(|e| bad(e.to_string()))?;
                match fs::metadata(&p) {
                    Ok(m) if m.len() > 0 => {}
                    _ => return Err(bad(format!("{rel} was not produced"))),
                }
            }
            StepKind::Sprites => {
                let produced = (1..=plan.sprite.sheets.max(1))
                    .take_while(|n| {
                        sandbox_path(out, &plan.sprite.sheet_path(*n))
                            .ok()
                            .is_some_and(|p| fs::metadata(p).is_ok_and(|m| m.len() > 0))
                    })
                    .count() as u32;
                if produced < plan.sprite.sheets {
                    return Err(bad(format!(
                        "expected {} sprite sheets, found {produced}",
                        plan.sprite.sheets
                    )));
                }
                let vtt = sprite_vtt(&plan.sprite, plan.source.duration_secs, &|n| {
                    format!("sheet-{n:03}.jpg")
                });
                let p = sandbox_path(out, &plan.sprite.vtt_path).map_err(|e| bad(e.to_string()))?;
                fs::write(p, vtt).map_err(|e| bad(e.to_string()))?;
            }
            StepKind::Proxy => {}
        }
        Ok(())
    }

    fn upload_all(
        &self,
        job: &ClaimedJob,
        out: &Path,
        rep: &mut Reporter,
    ) -> Result<Vec<OutputRef>, JobError> {
        let mut files = list_outputs(out).map_err(|e| JobError::fail(Stage::Upload, e, true))?;
        // Segments and media first, playlists last: a half-uploaded rendition never looks done.
        files.sort_by_key(|(rel, _)| (rel.ends_with(".m3u8"), rel.clone()));
        let total: u64 = files.iter().map(|(_, s)| *s).sum();
        if total > job.max_output_bytes {
            return Err(JobError::fail(
                Stage::Upload,
                "outputs exceed the job's size budget",
                false,
            ));
        }
        let mut refs = Vec::new();
        let n = files.len().max(1);
        for (i, (rel, size)) in files.into_iter().enumerate() {
            let path = sandbox_path(out, &rel)
                .map_err(|e| JobError::fail(Stage::Upload, e.to_string(), false))?;
            let init = self
                .api
                .init_upload(
                    &job.job_id,
                    &InitUploadRequest {
                        path: rel.clone(),
                        size,
                    },
                )
                .map_err(|e| {
                    if e.is_job_lost() {
                        JobError::LeaseLost
                    } else {
                        JobError::fail(Stage::Upload, format!("{rel}: {e}"), true)
                    }
                })?;
            let parts = self
                .transfer
                .upload(&init, Path::new(&path))
                .map_err(|e| JobError::fail(Stage::Upload, format!("{rel}: {e}"), true))?;
            self.api
                .finish_upload(&job.job_id, &init.upload_id, &FinishUploadRequest { parts })
                .map_err(|e| {
                    if e.is_job_lost() {
                        JobError::LeaseLost
                    } else {
                        JobError::fail(Stage::Upload, format!("{rel}: {e}"), true)
                    }
                })?;
            refs.push(OutputRef { path: rel, size });
            let pct = 90.0 + 9.0 * (i + 1) as f32 / n as f32;
            self.report(rep, Stage::Upload, pct, None, false)?;
        }
        Ok(refs)
    }

    /// Register, then heartbeat and process jobs on `concurrency` threads until `shutdown`.
    pub fn run(&self, shutdown: &AtomicBool) -> Result<(), ApiClientError> {
        let reg = self.api.register(&RegisterRequest {
            name: self.cfg.name.clone(),
            version: self.cfg.version.clone(),
            capabilities: self.capabilities(),
        })?;
        let sleep = |secs: u32| {
            let mut left = u64::from(secs) * 10;
            while left > 0 && !shutdown.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(100));
                left -= 1;
            }
        };
        std::thread::scope(|scope| {
            scope.spawn(|| {
                while !shutdown.load(Ordering::SeqCst) {
                    if let Ok(hb) = self
                        .api
                        .heartbeat(&reg.worker_id, &HeartbeatRequest::default())
                    {
                        self.draining.store(hb.drain, Ordering::SeqCst);
                        if let Ok(mut set) = self.cancelled.lock() {
                            set.extend(hb.cancel_jobs);
                        }
                    }
                    sleep(reg.heartbeat_secs);
                }
            });
            for _ in 0..self.cfg.concurrency.max(1) {
                scope.spawn(|| {
                    while !shutdown.load(Ordering::SeqCst) && !self.draining.load(Ordering::SeqCst)
                    {
                        match self.poll_once(&reg.worker_id) {
                            Ok(Some(_)) => {}
                            Ok(None) | Err(_) => sleep(reg.poll_secs),
                        }
                    }
                });
            }
        });
        Ok(())
    }
}

enum StepFailure {
    Job(JobError),
    Exit { log: String, timed_out: bool },
}

/// Parse a rendition's ffmpeg-written playlist into the stored segment index, checking that
/// every named file exists inside the sandbox.
fn read_segment_index(out: &Path, dir: &str, playlist_rel: &str) -> Result<SegmentIndex, String> {
    let pl = sandbox_path(out, playlist_rel).map_err(|e| e.to_string())?;
    let meta = fs::metadata(&pl).map_err(|e| format!("{playlist_rel}: {e}"))?;
    if meta.len() > 16 * 1024 * 1024 {
        return Err(format!("{playlist_rel} is implausibly large"));
    }
    let text = fs::read_to_string(&pl).map_err(|e| format!("{playlist_rel}: {e}"))?;
    let parsed = parse_media_playlist(&text).map_err(|e| format!("{playlist_rel}: {e}"))?;
    let init = parsed.init_name.unwrap_or_else(|| "init.mp4".to_string());
    let init_path = sandbox_path(out, &format!("{dir}/{init}")).map_err(|e| e.to_string())?;
    if fs::metadata(&init_path).map_or(true, |m| m.len() == 0) {
        return Err(format!("{dir}/{init} was not produced"));
    }
    let mut segments = Vec::with_capacity(parsed.segments.len());
    for (name, duration) in parsed.segments {
        let p = sandbox_path(out, &format!("{dir}/{name}")).map_err(|e| e.to_string())?;
        let size = fs::metadata(&p)
            .map_err(|_| format!("{dir}/{name} is listed but missing"))?
            .len();
        segments.push(SegmentEntry {
            name,
            duration_secs: duration,
            size_bytes: size,
        });
    }
    Ok(SegmentIndex {
        playlist: playlist_rel.to_string(),
        init,
        segments,
    })
}

/// Regular files under `out` (no symlinks), as `(relative path, size)`.
pub fn list_outputs(out: &Path) -> Result<Vec<(String, u64)>, String> {
    fn walk(dir: &Path, rel: &str, depth: u32, acc: &mut Vec<(String, u64)>) -> Result<(), String> {
        if depth > 5 {
            return Err("output tree is too deep".into());
        }
        for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let meta = fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
            let child = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            if meta.is_symlink() {
                return Err(format!("{child} is a symbolic link"));
            }
            if meta.is_dir() {
                walk(&entry.path(), &child, depth + 1, acc)?;
            } else if meta.is_file() && meta.len() > 0 && !name.ends_with(".tmp") {
                acc.push((child, meta.len()));
            }
            if acc.len() > sr_media::worker_api::MAX_OUTPUT_FILES {
                return Err("too many output files".into());
            }
        }
        Ok(())
    }
    let mut acc = Vec::new();
    walk(out, "", 0, &mut acc)?;
    Ok(acc)
}

#[cfg(test)]
pub(crate) mod tests;
