//! sr-media: the Cinema media pipeline as pure, testable logic (CIN-ING-*).
//!
//! Nothing here links FFmpeg. External tools run through the [`runner::CommandRunner`] port
//! (argument vectors only, never a shell), so every module is unit-tested with canned tool
//! output. Media files and `ffprobe` output are untrusted: JSON size, stream counts, string
//! lengths and output paths are all bounded or sandboxed.
//!
//! * [`probe`]: `ffprobe` JSON into typed [`probe::ProbeResult`] (CIN-ING-001).
//! * [`plan`]: [`plan::ProxyPlan`] (CIN-ING-002) and [`plan::JobPlan`] (CIN-ING-003).
//! * [`worker_api`] and [`http`]: the worker contract, service and routes (CIN-ING-004).
//! * [`playlist`]: master/media playlists with signed URLs (CIN-ING-007).
//! * [`subtitles`]: ASS/SRT to WebVTT and the sprite map (CIN-ING-008).
//! * [`retention`]: originals and proxy retention (CIN-ING-009).
//! * [`retranscode`]: re-transcode planning (CIN-ING-011).

pub mod http;
pub mod plan;
pub mod playback;
pub mod playlist;
pub mod probe;
pub mod retention;
pub mod retranscode;
pub mod runner;
pub mod subtitles;
pub mod worker_api;

#[cfg(test)]
mod fixtures;

/// Lowercase hex of `bytes`.
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(char::from(DIGITS[usize::from(b >> 4)]));
        out.push(char::from(DIGITS[usize::from(b & 0x0f)]));
    }
    out
}
