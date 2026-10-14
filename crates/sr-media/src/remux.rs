//! Patron download remux plan (CIN-DL-001): a stream-copy `ffmpeg` job that turns one stored
//! HLS rendition (video rung plus its audio tracks) into a single progressive MP4.
//!
//! The worker has no storage credentials and the playlists in storage use relative segment
//! names, so the plan lists every file the job needs (`inputs`, each with the CDN path it is
//! served under); the Worker API signs them when the job is claimed. The worker downloads them
//! into one directory tree, keeping the relative layout, and `ffmpeg -c copy` reads the local
//! playlists. Nothing is re-encoded, so a remux of a 2-hour 1080p rung takes seconds to minutes
//! and needs no GPU.
//!
//! Subtitles are not part of the MP4 (WebVTT in MP4 players is rare); members use the player.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Where the worker writes the finished MP4, relative to the job output directory.
pub const REMUX_OUTPUT: &str = "remux/video.mp4";
/// Most files one remux may list (one rung of a very long film is a few thousand).
pub const MAX_REMUX_INPUTS: usize = 60_000;
/// Most bytes one remux may fetch.
pub const MAX_REMUX_INPUT_BYTES: u64 = 64 * 1024 * 1024 * 1024;
/// Most audio tracks muxed into the MP4.
pub const MAX_REMUX_AUDIO: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemuxFile {
    /// Path relative to the asset prefix, e.g. `v/1080p/seg_00001.m4s`.
    pub path: String,
    pub size_bytes: u64,
}

/// One stored HLS media rendition: its playlist, init segment and media segments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemuxTrack {
    pub playlist: String,
    pub init: String,
    pub segments: Vec<RemuxFile>,
    /// Sizes of the playlist and init segment, which the segment list does not carry.
    pub playlist_size: u64,
    pub init_size: u64,
}

/// What the catalog knows when a member asks for a rendition.
#[derive(Debug, Clone, PartialEq)]
pub struct RemuxRequest {
    pub rendition: String,
    /// Unsigned CDN path of the asset prefix, e.g. `/cdn/hls/123/1` (no trailing slash).
    pub cdn_base: String,
    pub video: RemuxTrack,
    /// Default audio track first.
    pub audio: Vec<RemuxTrack>,
    pub duration_secs: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemuxInput {
    /// Local path under the job's input directory (same as the path under the asset prefix).
    pub path: String,
    /// Unsigned CDN path; the Worker API signs it on claim.
    pub cdn_path: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemuxPlan {
    pub rendition: String,
    /// Local path of the video media playlist.
    pub video_playlist: String,
    /// Local paths of the audio media playlists, default first.
    pub audio_playlists: Vec<String>,
    pub inputs: Vec<RemuxInput>,
    /// Output path, always [`REMUX_OUTPUT`].
    pub output: String,
    pub duration_secs: f64,
}

impl RemuxPlan {
    pub fn total_input_bytes(&self) -> u64 {
        self.inputs.iter().map(|i| i.size_bytes).sum()
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RemuxError {
    #[error("rendition name is not usable")]
    BadRendition,
    #[error("unsafe or unexpected path {0:?}")]
    BadPath(String),
    #[error("the rendition has no segments")]
    Empty,
    #[error("the rendition is too large to remux")]
    TooLarge,
    #[error("duration must be positive")]
    BadDuration,
}

/// A path the job may write or read: relative, at most five components, `[A-Za-z0-9._-]` only
/// (no spaces: the path ends up in a URL), no dot-files.
fn clean_path(p: &str, allowed_dirs: &[&str]) -> Result<(), RemuxError> {
    let bad = || RemuxError::BadPath(p.chars().take(80).collect());
    if p.is_empty() || p.len() > 200 || p.starts_with('/') || p.contains(['\\', '\0']) {
        return Err(bad());
    }
    let parts: Vec<&str> = p.split('/').collect();
    if parts.len() > 5 {
        return Err(bad());
    }
    for c in &parts {
        let ok = !c.is_empty()
            && !c.starts_with('.')
            && c.chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'));
        if !ok {
            return Err(bad());
        }
    }
    if !allowed_dirs.iter().any(|d| p.starts_with(d)) {
        return Err(bad());
    }
    Ok(())
}

fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(d, _)| d)
}

fn add_track(
    t: &RemuxTrack,
    dirs: &[&str],
    cdn_base: &str,
    inputs: &mut Vec<RemuxInput>,
) -> Result<(), RemuxError> {
    if t.segments.is_empty() {
        return Err(RemuxError::Empty);
    }
    clean_path(&t.playlist, dirs)?;
    clean_path(&t.init, dirs)?;
    let dir = dir_of(&t.playlist).to_owned();
    inputs.push(RemuxInput {
        path: t.playlist.clone(),
        cdn_path: format!("{cdn_base}/{}", t.playlist),
        size_bytes: t.playlist_size,
    });
    inputs.push(RemuxInput {
        path: t.init.clone(),
        cdn_path: format!("{cdn_base}/{}", t.init),
        size_bytes: t.init_size,
    });
    for s in &t.segments {
        clean_path(&s.path, dirs)?;
        // ffmpeg's playlists name segments relative to the playlist: they must be siblings.
        if dir_of(&s.path) != dir {
            return Err(RemuxError::BadPath(s.path.chars().take(80).collect()));
        }
        inputs.push(RemuxInput {
            path: s.path.clone(),
            cdn_path: format!("{cdn_base}/{}", s.path),
            size_bytes: s.size_bytes,
        });
    }
    Ok(())
}

/// Plan the remux of one rendition. Pure; every path is validated because the stored segment
/// index came from a worker.
pub fn plan_remux(req: &RemuxRequest) -> Result<RemuxPlan, RemuxError> {
    if req.rendition.is_empty()
        || req.rendition.len() > 32
        || !req
            .rendition
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(RemuxError::BadRendition);
    }
    if !req.duration_secs.is_finite() || req.duration_secs <= 0.0 {
        return Err(RemuxError::BadDuration);
    }
    let base = req.cdn_base.trim_end_matches('/');
    if !base.starts_with("/cdn/") || base.contains("..") || base.contains(['?', '#', '\\']) {
        return Err(RemuxError::BadPath(base.chars().take(80).collect()));
    }
    let mut inputs = Vec::new();
    add_track(&req.video, &["v/"], base, &mut inputs)?;
    for a in req.audio.iter().take(MAX_REMUX_AUDIO) {
        add_track(a, &["a/"], base, &mut inputs)?;
    }
    if inputs.len() > MAX_REMUX_INPUTS {
        return Err(RemuxError::TooLarge);
    }
    let total: u64 = inputs.iter().map(|i| i.size_bytes).sum();
    if total > MAX_REMUX_INPUT_BYTES {
        return Err(RemuxError::TooLarge);
    }
    Ok(RemuxPlan {
        rendition: req.rendition.clone(),
        video_playlist: req.video.playlist.clone(),
        audio_playlists: req
            .audio
            .iter()
            .take(MAX_REMUX_AUDIO)
            .map(|a| a.playlist.clone())
            .collect(),
        inputs,
        output: REMUX_OUTPUT.to_owned(),
        duration_secs: req.duration_secs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(dir: &str, n: usize) -> RemuxTrack {
        RemuxTrack {
            playlist: format!("{dir}/index.m3u8"),
            init: format!("{dir}/init.mp4"),
            segments: (0..n)
                .map(|i| RemuxFile {
                    path: format!("{dir}/seg_{i:05}.m4s"),
                    size_bytes: 1000,
                })
                .collect(),
            playlist_size: 200,
            init_size: 700,
        }
    }

    fn req() -> RemuxRequest {
        RemuxRequest {
            rendition: "1080p".into(),
            cdn_base: "/cdn/hls/9/1/".into(),
            video: track("v/1080p", 3),
            audio: vec![track("a/00-eng-stereo", 3)],
            duration_secs: 18.0,
        }
    }

    #[test]
    fn plans_every_file_under_the_cdn_prefix() {
        let p = plan_remux(&req()).unwrap();
        assert_eq!(p.output, "remux/video.mp4");
        assert_eq!(p.video_playlist, "v/1080p/index.m3u8");
        assert_eq!(p.audio_playlists, vec!["a/00-eng-stereo/index.m3u8"]);
        // playlist + init + 3 segments, for video and for audio
        assert_eq!(p.inputs.len(), 10);
        assert_eq!(p.inputs[0].cdn_path, "/cdn/hls/9/1/v/1080p/index.m3u8");
        assert_eq!(p.inputs[2].path, "v/1080p/seg_00000.m4s");
        assert_eq!(p.total_input_bytes(), 2 * (200 + 700 + 3000));
    }

    #[test]
    fn hostile_paths_and_shapes_are_refused() {
        let mut r = req();
        r.video.segments[1].path = "v/1080p/../../x.m4s".into();
        assert!(matches!(plan_remux(&r), Err(RemuxError::BadPath(_))));
        let mut r = req();
        r.video.segments[0].path = "v/720p/seg_00000.m4s".into();
        assert!(matches!(plan_remux(&r), Err(RemuxError::BadPath(_))));
        let mut r = req();
        r.audio[0].playlist = "v/x/index.m3u8".into();
        assert!(matches!(plan_remux(&r), Err(RemuxError::BadPath(_))));
        let mut r = req();
        r.video.init = "/abs/init.mp4".into();
        assert!(matches!(plan_remux(&r), Err(RemuxError::BadPath(_))));
        let mut r = req();
        r.video.segments.clear();
        assert_eq!(plan_remux(&r), Err(RemuxError::Empty));
        let mut r = req();
        r.rendition = "1080p/../x".into();
        assert_eq!(plan_remux(&r), Err(RemuxError::BadRendition));
        let mut r = req();
        r.cdn_base = "https://evil.test/x".into();
        assert!(matches!(plan_remux(&r), Err(RemuxError::BadPath(_))));
        let mut r = req();
        r.duration_secs = 0.0;
        assert_eq!(plan_remux(&r), Err(RemuxError::BadDuration));
    }

    #[test]
    fn size_caps() {
        let mut r = req();
        r.video.segments[0].size_bytes = MAX_REMUX_INPUT_BYTES + 1;
        assert_eq!(plan_remux(&r), Err(RemuxError::TooLarge));
        let mut r = req();
        r.audio = (0..9).map(|i| track(&format!("a/0{i}-x"), 1)).collect();
        // Only the first four audio tracks are muxed.
        assert_eq!(plan_remux(&r).unwrap().audio_playlists.len(), 4);
    }
}
