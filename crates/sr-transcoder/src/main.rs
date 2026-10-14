//! `sr-transcoder` binary: parse the command line, detect ffmpeg's capabilities and, with
//! `--check`, print them. The HTTP client for the Worker API is not linked yet (see the
//! handoff note): until it is, the server mode refuses to start rather than pretend.

use sr_media::runner::ProcessRunner;
use sr_transcoder::cli::{self, USAGE};
use sr_transcoder::encoders;
use std::process::ExitCode;

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
    if encoder.is_none() {
        eprintln!("error: no usable H.264 encoder (is ffmpeg installed with libx264?)");
        return ExitCode::from(1);
    }
    eprintln!(
        "error: this build has no Worker API HTTP client linked; run with --check to verify the machine"
    );
    ExitCode::from(3)
}
