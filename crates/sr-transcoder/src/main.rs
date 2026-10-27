//! `sr-transcoder` binary: parse the command line, detect ffmpeg's capabilities and either
//! print them (`--check`) or register with the server and process jobs.
//!
//! Shutdown: SIGTERM/Ctrl-C end the process immediately. A job in flight keeps its lease
//! until it expires and is then requeued by the server, so nothing is lost.

use sr_media::runner::ProcessRunner;
use sr_transcoder::cli::{self, USAGE};
use sr_transcoder::encoders;
use sr_transcoder::http_client::{HttpTransfer, HttpWorkerApi};
use sr_transcoder::worker::{Worker, WorkerConfig, ffprobe_beside};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

fn main() -> ExitCode {
    let parsed = cli::parse(std::env::args().skip(1), &|k| std::env::var(k).ok(), &|p| {
        std::fs::read_to_string(p).map_err(|e| e.to_string())
    });
    let cli = match parsed {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    if cli.help {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let caps = encoders::detect(&ProcessRunner, &cli.ffmpeg);
    let encoder = caps.choose(cli.force_cpu);
    if cli.check {
        let report = serde_json::json!({
            "ffmpeg_version": caps.version,
            "encoder": encoder.map(encoders::Encoder::name),
            "nvenc_available": caps.nvenc_available(),
            "tonemap": caps.can_tonemap(),
            "libplacebo": caps.has_libplacebo(),
            "hwaccels": caps.hwaccels,
        });
        println!("{report:#}");
        return if encoder.is_some() {
            ExitCode::SUCCESS
        } else {
            eprintln!("error: no usable H.264 encoder (is ffmpeg installed with libx264?)");
            ExitCode::from(1)
        };
    }
    let Some(encoder) = encoder else {
        eprintln!("error: no usable H.264 encoder (is ffmpeg installed with libx264?)");
        return ExitCode::from(1);
    };
    eprintln!(
        "sr-transcoder: ffmpeg {} using {}",
        caps.version.as_deref().unwrap_or("?"),
        encoder.name()
    );
    // `cli::parse` guarantees both of these in server mode.
    let (Some(server), Some(token)) = (cli.server.as_deref(), cli.token.as_deref()) else {
        return ExitCode::from(2);
    };
    let api = match HttpWorkerApi::new(server, token) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(1);
        }
    };
    let transfer = match HttpTransfer::new(server) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(1);
        }
    };
    if let Err(e) = std::fs::create_dir_all(&cli.work_dir) {
        eprintln!("error: cannot create the work directory: {e}");
        return ExitCode::from(1);
    }
    let mut cfg = WorkerConfig::new(cli.work_dir.clone());
    cfg.name = cli.name.clone();
    cfg.ffmpeg = cli.ffmpeg.clone();
    cfg.ffprobe = ffprobe_beside(&cli.ffmpeg);
    cfg.force_cpu = cli.force_cpu;
    cfg.concurrency = cli.concurrency;
    let worker = Worker::new(&api, &transfer, &ProcessRunner, cfg, caps);
    let shutdown = AtomicBool::new(false);
    match worker.run(&shutdown) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}
