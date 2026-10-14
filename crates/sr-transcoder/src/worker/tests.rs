//! State-machine tests with a fake ffmpeg runner that writes the files the real one would.

use super::*;
use crate::args::tests::{SDR_1080, ladder};
use sr_media::plan::{TranscodeSettings, plan_proxy_job};
use sr_media::probe::parse_ffprobe_json;
use sr_media::runner::{FakeResult, FakeRunner};
use sr_media::worker_api::PartUrl;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

pub(crate) struct TempRoot(pub(crate) PathBuf);
impl TempRoot {
    pub(crate) fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!("sr-transcoder-test-{}-{n}", std::process::id()));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

// ---- fake server --------------------------------------------------------------------------

#[derive(Default)]
struct ApiState {
    progress_calls: usize,
    cancel_at: Option<usize>,
    lost_at: Option<usize>,
    uploads: Vec<InitUploadRequest>,
    finished: Vec<String>,
    completes: Vec<CompleteRequest>,
    fails: Vec<FailRequest>,
    claims: usize,
    registered: usize,
    stages: Vec<Stage>,
}

#[derive(Default)]
struct FakeApi {
    state: Mutex<ApiState>,
    shutdown_after_claims: Option<(usize, Arc<AtomicBool>)>,
}

impl FakeApi {
    fn st(&self) -> std::sync::MutexGuard<'_, ApiState> {
        self.state.lock().unwrap()
    }
}

fn status(status: u16, code: &str) -> ApiClientError {
    ApiClientError::Status {
        status,
        code: code.into(),
        message: code.into(),
    }
}

impl WorkerApi for FakeApi {
    fn register(&self, _req: &RegisterRequest) -> ApiResult<RegisterResponse> {
        self.st().registered += 1;
        Ok(RegisterResponse {
            worker_id: "w1".into(),
            heartbeat_secs: 1,
            lease_secs: 120,
            poll_secs: 1,
        })
    }
    fn heartbeat(&self, _w: &str, _req: &HeartbeatRequest) -> ApiResult<HeartbeatResponse> {
        Ok(HeartbeatResponse::default())
    }
    fn claim(&self, _w: &str, _req: &ClaimRequest) -> ApiResult<ClaimResponse> {
        let mut s = self.st();
        s.claims += 1;
        if let Some((n, flag)) = &self.shutdown_after_claims
            && s.claims >= *n
        {
            flag.store(true, Ordering::SeqCst);
        }
        Ok(ClaimResponse::default())
    }
    fn progress(&self, _job: &str, req: &ProgressRequest) -> ApiResult<ProgressResponse> {
        let mut s = self.st();
        s.progress_calls += 1;
        s.stages.push(req.stage);
        let n = s.progress_calls;
        if s.lost_at == Some(n) {
            return Err(status(410, "lease_expired"));
        }
        Ok(ProgressResponse {
            cancel: s.cancel_at.is_some_and(|c| n >= c),
            lease_expires_at: 0,
        })
    }
    fn init_upload(&self, _job: &str, req: &InitUploadRequest) -> ApiResult<InitUploadResponse> {
        self.st().uploads.push(req.clone());
        Ok(InitUploadResponse {
            upload_id: format!("u{}", self.st().uploads.len()),
            part_size: 16 << 20,
            parts: vec![PartUrl {
                part_number: 1,
                url: "https://storage.invalid/part".into(),
            }],
            content_type: "application/octet-stream".into(),
            expires_at: 0,
        })
    }
    fn finish_upload(&self, _j: &str, upload_id: &str, _r: &FinishUploadRequest) -> ApiResult<Ack> {
        self.st().finished.push(upload_id.into());
        Ok(Ack { ok: true })
    }
    fn complete(&self, _j: &str, req: &CompleteRequest) -> ApiResult<Ack> {
        self.st().completes.push(req.clone());
        Ok(Ack { ok: true })
    }
    fn fail(&self, _j: &str, req: &FailRequest) -> ApiResult<Ack> {
        self.st().fails.push(req.clone());
        Ok(Ack { ok: true })
    }
}

struct FakeTransfer {
    short_by: u64,
}

impl Transfer for FakeTransfer {
    fn download(&self, source: &SourceRef, dest: &Path, _max: u64) -> Result<u64, TransferError> {
        let n = source.size_bytes - self.short_by;
        fs::write(dest, vec![7u8; n as usize])
            .map_err(|e| TransferError::Download(e.to_string()))?;
        Ok(n)
    }
    fn upload(
        &self,
        init: &InitUploadResponse,
        file: &Path,
    ) -> Result<Vec<PartDone>, TransferError> {
        assert!(file.is_file(), "{file:?}");
        Ok(init
            .parts
            .iter()
            .map(|p| PartDone {
                part_number: p.part_number,
                etag: "etag".into(),
            })
            .collect())
    }
}

// ---- fake ffmpeg --------------------------------------------------------------------------

fn kind_of(args: &[String]) -> &'static str {
    let has = |s: &str| args.iter().any(|a| a == s);
    if has("-filter_complex") {
        "video"
    } else if args.iter().any(|a| a.starts_with("-dump_attachment")) {
        "font"
    } else if args.iter().any(|a| a.contains("tile=")) {
        "sprites"
    } else if has("-ss") {
        "poster"
    } else if has("-c:s") {
        "subtitle"
    } else if has("aac") && has("hls") {
        "audio"
    } else if args.last().is_some_and(|l| l.ends_with("proxy.mp4")) {
        "proxy"
    } else if args.last().is_some_and(|l| l.ends_with("remux/video.mp4")) {
        "remux"
    } else {
        "unknown"
    }
}

fn write(path: &str, bytes: &[u8]) {
    let p = Path::new(path);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, bytes).unwrap();
}

/// Write what a successful run of this command would leave behind.
fn produce(args: &[String], sheets: u32) {
    let last = args.last().unwrap();
    match kind_of(args) {
        "video" | "audio" => {
            for out in args.iter().filter(|a| a.ends_with("/index.m3u8")) {
                let dir = out.trim_end_matches("/index.m3u8");
                write(&format!("{dir}/init.mp4"), b"init");
                write(&format!("{dir}/seg_00000.m4s"), &[1u8; 100]);
                write(&format!("{dir}/seg_00001.m4s"), &[2u8; 50]);
                write(
                    out,
                    b"#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:7\n#EXT-X-MAP:URI=\"init.mp4\"\n\
                      #EXTINF:6.006000,\nseg_00000.m4s\n#EXTINF:3.5,\nseg_00001.m4s\n#EXT-X-ENDLIST\n",
                );
            }
        }
        "subtitle" => {
            if last.ends_with(".ass") {
                write(
                    last,
                    b"[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n\
                      Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,{\\i1}Hello\\Nworld\n",
                );
            } else {
                write(last, b"1\n00:00:01,000 --> 00:00:02,000\nHola\n\n");
            }
        }
        "font" => {
            let i = args
                .iter()
                .position(|a| a.starts_with("-dump_attachment"))
                .unwrap();
            let dest = &args[i + 1];
            write(dest, b"fontdata");
        }
        "sprites" => {
            for n in 1..=sheets {
                write(&last.replace("%03d", &format!("{n:03}")), b"jpeg");
            }
        }
        "poster" | "proxy" | "remux" => write(last, b"jpeg-or-mp4"),
        _ => {}
    }
}

type Failure = Box<dyn Fn(&str, u32, &[String]) -> Option<String> + Send>;

pub(crate) fn runner(sheets: u32, failure: Failure) -> FakeRunner {
    let counts: Mutex<std::collections::HashMap<String, u32>> = Mutex::new(Default::default());
    FakeRunner::new(move |spec| {
        let kind = kind_of(&spec.args);
        let key = format!("{kind}:{}", spec.args.last().cloned().unwrap_or_default());
        let n = {
            let mut c = counts.lock().unwrap();
            let e = c.entry(key).or_insert(0);
            *e += 1;
            *e
        };
        if let Some(stderr) = failure(kind, n, &spec.args) {
            return FakeResult::fail(1, stderr);
        }
        produce(&spec.args, sheets);
        FakeResult {
            exit_code: Some(0),
            stdout: "out_time_us=1000000\nprogress=continue\nout_time_us=60000000\nprogress=end\n"
                .into(),
            stderr: String::new(),
        }
    })
}

pub(crate) fn never_fail() -> Failure {
    Box::new(|_, _, _| None)
}

// ---- helpers ------------------------------------------------------------------------------

pub(crate) fn caps(nvenc: bool) -> FfmpegCaps {
    let mut c = crate::args::tests::caps();
    c.nvenc_usable = nvenc;
    c
}

pub(crate) fn cfg(root: &Path) -> WorkerConfig {
    let mut c = WorkerConfig::new(root.to_path_buf());
    c.progress_interval = Duration::ZERO;
    c
}

pub(crate) fn ladder_job(id: &str) -> ClaimedJob {
    let plan = ladder(SDR_1080);
    ClaimedJob {
        job_id: id.into(),
        attempt: 1,
        spec: JobSpec::Ladder {
            plan: Box::new(plan),
        },
        source: SourceRef {
            download_url: "https://storage.invalid/orig".into(),
            size_bytes: 1000,
            expires_at: 0,
            extension: "mkv".into(),
        },
        lease_expires_at: 0,
        max_output_bytes: 1 << 30,
        inputs: Vec::new(),
    }
}

pub(crate) fn proxy_job(id: &str) -> ClaimedJob {
    let plan = plan_proxy_job(
        &parse_ffprobe_json(SDR_1080.as_bytes()).unwrap(),
        &TranscodeSettings::default(),
    )
    .unwrap();
    ClaimedJob {
        spec: JobSpec::Proxy {
            plan: Box::new(plan),
        },
        ..ladder_job(id)
    }
}

pub(crate) fn sheets_of(job: &ClaimedJob) -> u32 {
    match &job.spec {
        JobSpec::Ladder { plan } => plan.sprite.sheets,
        JobSpec::Proxy { .. } | JobSpec::Remux { .. } => 0,
    }
}

// ---- tests --------------------------------------------------------------------------------

#[test]
fn ladder_job_happy_path() {
    let root = TempRoot::new();
    let api = FakeApi::default();
    let job = ladder_job("job1");
    let run = runner(sheets_of(&job), never_fail());
    let w = Worker::new(
        &api,
        &FakeTransfer { short_by: 0 },
        &run,
        cfg(&root.0),
        caps(false),
    );

    assert_eq!(w.process_job(&job), JobOutcome::Completed);

    let st = api.st();
    assert!(st.fails.is_empty());
    assert_eq!(st.completes.len(), 1);
    let done = &st.completes[0];
    assert_eq!(done.encoder, "libx264");

    // Steps ran in order: video, audio, subtitles, font, sprites, posters.
    let calls = run.calls();
    let kinds: Vec<_> = calls.iter().map(|c| kind_of(&c.args)).collect();
    assert_eq!(
        kinds,
        [
            "video", "audio", "subtitle", "subtitle", "font", "sprites", "poster", "poster",
            "poster", "poster", "poster"
        ]
    );
    assert!(calls[0].args.contains(&"libx264".to_string()));
    assert_eq!(calls[0].program, "ffmpeg");

    // Every output the plan requires was uploaded, with sizes, and listed in the completion.
    let JobSpec::Ladder { plan } = &job.spec else {
        panic!()
    };
    let uploaded: Vec<&str> = st.uploads.iter().map(|u| u.path.as_str()).collect();
    for required in plan.required_outputs() {
        assert!(uploaded.contains(&required.as_str()), "missing {required}");
    }
    assert_eq!(st.finished.len(), st.uploads.len());
    assert_eq!(done.outputs.len(), st.uploads.len());
    // Playlists are uploaded after their segments.
    let pos = |p: &str| uploaded.iter().position(|u| *u == p).unwrap();
    assert!(pos("v/1080p/seg_00001.m4s") < pos("v/1080p/index.m3u8"));
    // Raw SRT was discarded, the ASS kept next to its WebVTT.
    assert!(!uploaded.contains(&"subs/01-spa.srt"));
    assert!(uploaded.contains(&"subs/00-eng.ass") && uploaded.contains(&"subs/00-eng.vtt"));

    // Segment index per rung and per audio output, with real durations and sizes.
    assert_eq!(
        done.segment_index.len(),
        plan.rungs.len() + plan.audio.len()
    );
    let r0 = &done.segment_index[0];
    assert_eq!(r0.playlist, "v/1080p/index.m3u8");
    assert_eq!(r0.init, "init.mp4");
    assert_eq!(r0.segments.len(), 2);
    assert_eq!(r0.segments[0].size_bytes, 100);
    assert!((r0.segments[1].duration_secs - 3.5).abs() < 1e-9);

    // Progress was reported through every stage and ends before 100.
    assert!(st.stages.contains(&Stage::Download));
    assert!(st.stages.contains(&Stage::Video));
    assert!(st.stages.contains(&Stage::Upload));

    // The work directory is gone.
    assert!(!root.0.join("job1").exists());
}

#[test]
fn converted_subtitles_are_valid_webvtt() {
    let root = TempRoot::new();
    let api = FakeApi::default();
    let job = ladder_job("job1");
    let seen = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let seen2 = Arc::clone(&seen);
    // Capture the VTT files from the work dir at upload time via a transfer that reads them.
    struct Reading(Arc<Mutex<Vec<(String, String)>>>);
    impl Transfer for Reading {
        fn download(&self, s: &SourceRef, d: &Path, _m: u64) -> Result<u64, TransferError> {
            fs::write(d, vec![0u8; s.size_bytes as usize]).unwrap();
            Ok(s.size_bytes)
        }
        fn upload(
            &self,
            init: &InitUploadResponse,
            f: &Path,
        ) -> Result<Vec<PartDone>, TransferError> {
            if f.extension().is_some_and(|e| e == "vtt") {
                self.0.lock().unwrap().push((
                    f.file_name().unwrap().to_string_lossy().into(),
                    fs::read_to_string(f).unwrap(),
                ));
            }
            Ok(vec![PartDone {
                part_number: init.parts[0].part_number,
                etag: "e".into(),
            }])
        }
    }
    let run = runner(sheets_of(&job), never_fail());
    let t = Reading(seen2);
    let w = Worker::new(&api, &t, &run, cfg(&root.0), caps(false));
    assert_eq!(w.process_job(&job), JobOutcome::Completed);
    let files = seen.lock().unwrap();
    let ass = files.iter().find(|(n, _)| n == "00-eng.vtt").unwrap();
    assert_eq!(
        ass.1,
        "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\n<i>Hello\nworld</i>\n\n"
    );
    let srt = files.iter().find(|(n, _)| n == "01-spa.vtt").unwrap();
    assert!(srt.1.contains("Hola"));
    let sprites = files.iter().find(|(n, _)| n == "sprites.vtt").unwrap();
    assert!(
        sprites
            .1
            .starts_with("WEBVTT\n\n00:00:00.000 --> 00:00:10.000\nsheet-001.jpg#xywh=0,0,160,90")
    );
}

#[test]
fn nvenc_failure_falls_back_to_cpu() {
    let root = TempRoot::new();
    let api = FakeApi::default();
    let job = ladder_job("job2");
    let run = runner(
        sheets_of(&job),
        Box::new(|kind, _, args| {
            (kind == "video" && args.contains(&"h264_nvenc".to_string())).then(|| {
                "[h264_nvenc @ 0x1] OpenEncodeSessionEx failed: out of memory (10)".to_string()
            })
        }),
    );
    let w = Worker::new(
        &api,
        &FakeTransfer { short_by: 0 },
        &run,
        cfg(&root.0),
        caps(true),
    );
    assert!(w.capabilities().nvenc);
    assert_eq!(w.process_job(&job), JobOutcome::Completed);

    let calls = run.calls();
    assert!(calls[0].args.contains(&"h264_nvenc".to_string()));
    assert!(calls[1].args.contains(&"libx264".to_string()));
    assert!(!calls[1].args.contains(&"h264_nvenc".to_string()));
    let st = api.st();
    assert_eq!(st.completes[0].encoder, "libx264");
    assert!(
        st.completes[0]
            .log_tail
            .as_deref()
            .unwrap()
            .contains("falling back to libx264")
    );
    assert!(st.fails.is_empty());
}

#[test]
fn nvenc_is_used_when_it_works() {
    let root = TempRoot::new();
    let api = FakeApi::default();
    let job = proxy_job("job3");
    let run = runner(0, never_fail());
    let w = Worker::new(
        &api,
        &FakeTransfer { short_by: 0 },
        &run,
        cfg(&root.0),
        caps(true),
    );
    assert_eq!(w.process_job(&job), JobOutcome::Completed);
    assert!(run.calls()[0].args.contains(&"h264_nvenc".to_string()));
    let st = api.st();
    assert_eq!(st.completes[0].encoder, "h264_nvenc");
    assert_eq!(st.uploads.len(), 1);
    assert_eq!(st.uploads[0].path, "proxy/proxy.mp4");
    // force_cpu wins.
    drop(st);
    let api2 = FakeApi::default();
    let run2 = runner(0, never_fail());
    let mut c = cfg(&root.0);
    c.force_cpu = true;
    let w2 = Worker::new(&api2, &FakeTransfer { short_by: 0 }, &run2, c, caps(true));
    assert_eq!(w2.process_job(&proxy_job("job3b")), JobOutcome::Completed);
    assert!(run2.calls()[0].args.contains(&"libx264".to_string()));
}

#[test]
fn a_failing_step_is_retried_once_then_succeeds() {
    let root = TempRoot::new();
    let api = FakeApi::default();
    let job = ladder_job("job4");
    let run = runner(
        sheets_of(&job),
        Box::new(|kind, n, _| {
            (kind == "audio" && n == 1).then(|| "Conversion failed!".to_string())
        }),
    );
    let w = Worker::new(
        &api,
        &FakeTransfer { short_by: 0 },
        &run,
        cfg(&root.0),
        caps(false),
    );
    assert_eq!(w.process_job(&job), JobOutcome::Completed);
    let audio_runs = run
        .calls()
        .iter()
        .filter(|c| kind_of(&c.args) == "audio")
        .count();
    assert_eq!(audio_runs, 2);
    assert!(
        api.st().completes[0]
            .log_tail
            .as_deref()
            .unwrap()
            .contains("Retrying audio")
    );
}

#[test]
fn persistent_failure_is_reported_with_logs_and_cleans_up() {
    let root = TempRoot::new();
    let api = FakeApi::default();
    let job = ladder_job("job5");
    let run = runner(
        sheets_of(&job),
        Box::new(|kind, _, _| {
            (kind == "video")
                .then(|| "Error while decoding stream #0:0: Invalid argument".to_string())
        }),
    );
    let w = Worker::new(
        &api,
        &FakeTransfer { short_by: 0 },
        &run,
        cfg(&root.0),
        caps(false),
    );
    assert_eq!(
        w.process_job(&job),
        JobOutcome::Failed {
            stage: Stage::Video,
            retryable: true
        }
    );
    let st = api.st();
    assert!(st.completes.is_empty() && st.uploads.is_empty());
    let f = &st.fails[0];
    assert!(f.retryable && !f.cancelled);
    assert!(f.message.contains("video ladder"));
    let log = f.log_tail.as_deref().unwrap();
    assert!(log.contains("Error while decoding stream"));
    assert!(log.contains("$ ffmpeg -hide_banner"));
    assert!(log.contains("exit code 1"));
    // Two attempts, no more.
    assert_eq!(run.calls().len(), 2);
    assert!(!root.0.join("job5").exists());
}

#[test]
fn undecodable_input_is_not_retried() {
    let root = TempRoot::new();
    let api = FakeApi::default();
    let job = ladder_job("job6");
    let run = runner(
        sheets_of(&job),
        Box::new(|_, _, _| Some("in.mkv: Invalid data found when processing input".to_string())),
    );
    let w = Worker::new(
        &api,
        &FakeTransfer { short_by: 0 },
        &run,
        cfg(&root.0),
        caps(false),
    );
    assert_eq!(
        w.process_job(&job),
        JobOutcome::Failed {
            stage: Stage::Video,
            retryable: false
        }
    );
    assert_eq!(run.calls().len(), 1);
    assert!(!api.st().fails[0].retryable);
}

#[test]
fn admin_cancel_kills_ffmpeg_and_reports_cancelled() {
    let root = TempRoot::new();
    let api = FakeApi::default();
    api.st().cancel_at = Some(3);
    let job = ladder_job("job7");
    let run = runner(sheets_of(&job), never_fail());
    let w = Worker::new(
        &api,
        &FakeTransfer { short_by: 0 },
        &run,
        cfg(&root.0),
        caps(false),
    );
    assert_eq!(w.process_job(&job), JobOutcome::Cancelled);
    let st = api.st();
    assert!(st.completes.is_empty() && st.uploads.is_empty());
    assert!(st.fails[0].cancelled);
    // It stopped during the first ffmpeg run: no later step started.
    assert!(run.calls().len() <= 1);
    assert!(!root.0.join("job7").exists());
}

#[test]
fn heartbeat_cancel_list_stops_the_job() {
    let root = TempRoot::new();
    let api = FakeApi::default();
    let job = ladder_job("job8");
    let run = runner(sheets_of(&job), never_fail());
    let w = Worker::new(
        &api,
        &FakeTransfer { short_by: 0 },
        &run,
        cfg(&root.0),
        caps(false),
    );
    w.cancelled.lock().unwrap().insert("job8".into());
    assert_eq!(w.process_job(&job), JobOutcome::Cancelled);
    assert!(run.calls().is_empty());
}

#[test]
fn lost_lease_stops_silently() {
    let root = TempRoot::new();
    let api = FakeApi::default();
    api.st().lost_at = Some(2);
    let job = ladder_job("job9");
    let run = runner(sheets_of(&job), never_fail());
    let w = Worker::new(
        &api,
        &FakeTransfer { short_by: 0 },
        &run,
        cfg(&root.0),
        caps(false),
    );
    assert_eq!(w.process_job(&job), JobOutcome::LeaseLost);
    let st = api.st();
    assert!(st.fails.is_empty() && st.completes.is_empty());
}

#[test]
fn short_download_fails_retryably() {
    let root = TempRoot::new();
    let api = FakeApi::default();
    let job = ladder_job("job10");
    let run = runner(sheets_of(&job), never_fail());
    let w = Worker::new(
        &api,
        &FakeTransfer { short_by: 10 },
        &run,
        cfg(&root.0),
        caps(false),
    );
    assert_eq!(
        w.process_job(&job),
        JobOutcome::Failed {
            stage: Stage::Download,
            retryable: true
        }
    );
    assert!(run.calls().is_empty());
}

#[test]
fn unsafe_job_ids_never_touch_the_disk() {
    let root = TempRoot::new();
    let api = FakeApi::default();
    let run = runner(0, never_fail());
    let w = Worker::new(
        &api,
        &FakeTransfer { short_by: 0 },
        &run,
        cfg(&root.0),
        caps(false),
    );
    for id in ["../escape", "a/b", "", "a b"] {
        let job = ladder_job(id);
        assert_eq!(
            w.process_job(&job),
            JobOutcome::Failed {
                stage: Stage::Download,
                retryable: false
            },
            "{id:?}"
        );
    }
    assert!(run.calls().is_empty());
    assert_eq!(fs::read_dir(&root.0).unwrap().count(), 0);
}

#[test]
fn missing_encoder_and_missing_hdr_filters_fail_permanently() {
    let root = TempRoot::new();
    let api = FakeApi::default();
    let run = runner(0, never_fail());
    let mut no_enc = caps(false);
    no_enc.encoders.clear();
    let w = Worker::new(
        &api,
        &FakeTransfer { short_by: 0 },
        &run,
        cfg(&root.0),
        no_enc,
    );
    assert_eq!(
        w.process_job(&ladder_job("j1")),
        JobOutcome::Failed {
            stage: Stage::Video,
            retryable: false
        }
    );

    // HDR source on an ffmpeg without zscale.
    let hdr = {
        let plan = ladder(crate::args::tests::HDR_4K);
        ClaimedJob {
            spec: JobSpec::Ladder {
                plan: Box::new(plan),
            },
            ..ladder_job("j2")
        }
    };
    let mut no_zscale = caps(false);
    no_zscale.filters.remove("zscale");
    let w = Worker::new(
        &api,
        &FakeTransfer { short_by: 0 },
        &run,
        cfg(&root.0),
        no_zscale,
    );
    assert_eq!(
        w.process_job(&hdr),
        JobOutcome::Failed {
            stage: Stage::Video,
            retryable: false
        }
    );
    assert!(run.calls().is_empty());
}

#[test]
fn oversized_outputs_are_refused_before_upload() {
    let root = TempRoot::new();
    let api = FakeApi::default();
    let mut job = proxy_job("job11");
    job.max_output_bytes = 5;
    let run = runner(0, never_fail());
    let w = Worker::new(
        &api,
        &FakeTransfer { short_by: 0 },
        &run,
        cfg(&root.0),
        caps(false),
    );
    assert_eq!(
        w.process_job(&job),
        JobOutcome::Failed {
            stage: Stage::Upload,
            retryable: false
        }
    );
    assert!(api.st().uploads.is_empty());
}

#[test]
fn run_registers_and_stops_on_shutdown() {
    let root = TempRoot::new();
    let flag = Arc::new(AtomicBool::new(false));
    let api = FakeApi {
        shutdown_after_claims: Some((2, Arc::clone(&flag))),
        ..FakeApi::default()
    };
    let run = runner(0, never_fail());
    let mut c = cfg(&root.0);
    c.concurrency = 2;
    let w = Worker::new(&api, &FakeTransfer { short_by: 0 }, &run, c, caps(false));
    w.run(&flag).unwrap();
    let st = api.st();
    assert_eq!(st.registered, 1);
    assert!(st.claims >= 2);
}

#[test]
fn list_outputs_skips_empty_and_tmp_files() {
    let root = TempRoot::new();
    fs::create_dir_all(root.0.join("a/b")).unwrap();
    fs::write(root.0.join("a/b/x.m4s"), b"1").unwrap();
    fs::write(root.0.join("a/empty.vtt"), b"").unwrap();
    fs::write(root.0.join("a/partial.m4s.tmp"), b"1").unwrap();
    let files = list_outputs(&root.0).unwrap();
    assert_eq!(files, [("a/b/x.m4s".to_string(), 1)]);
}

fn remux_job(id: &str) -> ClaimedJob {
    use sr_media::remux::{RemuxFile, RemuxRequest, RemuxTrack, plan_remux};
    use sr_media::worker_api::ClaimInput;
    let track = |dir: &str| RemuxTrack {
        playlist: format!("{dir}/index.m3u8"),
        init: format!("{dir}/init.mp4"),
        segments: vec![RemuxFile {
            path: format!("{dir}/seg_00000.m4s"),
            size_bytes: 10,
        }],
        playlist_size: 5,
        init_size: 7,
    };
    let plan = plan_remux(&RemuxRequest {
        rendition: "1080p".into(),
        cdn_base: "/cdn/hls/9/1".into(),
        video: track("v/1080p"),
        audio: vec![track("a/00-eng-stereo")],
        duration_secs: 60.0,
    })
    .unwrap();
    let inputs = plan
        .inputs
        .iter()
        .map(|i| ClaimInput {
            path: i.path.clone(),
            url: format!("https://storage.invalid{}?sig=1", i.cdn_path),
            size_bytes: i.size_bytes,
        })
        .collect();
    ClaimedJob {
        spec: JobSpec::Remux {
            plan: Box::new(plan),
        },
        inputs,
        ..ladder_job(id)
    }
}

#[test]
fn remux_job_downloads_the_inputs_copies_streams_and_uploads_the_mp4() {
    let root = TempRoot::new();
    let api = FakeApi::default();
    let job = remux_job("rmx1");
    let run = runner(0, never_fail());
    let w = Worker::new(
        &api,
        &FakeTransfer { short_by: 0 },
        &run,
        cfg(&root.0),
        caps(false),
    );
    assert_eq!(w.process_job(&job), JobOutcome::Completed);
    let st = api.st();
    assert!(st.fails.is_empty(), "{:?}", st.fails);
    let calls = run.calls();
    assert_eq!(calls.len(), 1);
    let args = &calls[0].args;
    // Stream copy, never an encoder; video input first, then the audio, mapped in that order.
    assert!(args.windows(2).any(|w| w[0] == "-c" && w[1] == "copy"));
    assert!(!args.iter().any(|a| a == "libx264" || a == "h264_nvenc"));
    let inputs: Vec<&String> = args
        .iter()
        .zip(args.iter().skip(1))
        .filter(|(a, _)| *a == "-i")
        .map(|(_, b)| b)
        .collect();
    assert_eq!(inputs.len(), 2);
    assert!(inputs[0].ends_with("/in/v/1080p/index.m3u8"));
    assert!(inputs[1].ends_with("/in/a/00-eng-stereo/index.m3u8"));
    assert!(args.windows(2).any(|w| w[0] == "-map" && w[1] == "0:v:0"));
    assert!(args.windows(2).any(|w| w[0] == "-map" && w[1] == "1:a:0"));
    assert!(args.last().unwrap().ends_with("/out/remux/video.mp4"));
    // Only the MP4 is uploaded: the downloaded inputs never go back.
    let uploaded: Vec<&str> = st.uploads.iter().map(|u| u.path.as_str()).collect();
    assert_eq!(uploaded, ["remux/video.mp4"]);
    assert_eq!(st.completes[0].encoder, "copy");
    assert!(st.completes[0].segment_index.is_empty());
    assert!(!root.0.join("rmx1").exists());
}

#[test]
fn remux_job_with_a_short_input_is_a_retryable_download_failure() {
    let root = TempRoot::new();
    let api = FakeApi::default();
    let job = remux_job("rmx2");
    let run = runner(0, never_fail());
    let w = Worker::new(
        &api,
        &FakeTransfer { short_by: 1 },
        &run,
        cfg(&root.0),
        caps(false),
    );
    assert_eq!(
        w.process_job(&job),
        JobOutcome::Failed {
            stage: Stage::Download,
            retryable: true
        }
    );
    assert!(run.calls().is_empty());
}
