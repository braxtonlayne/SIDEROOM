//! The read side of playback: what the playback endpoints and the playlist renderer need to know about an
//! asset's renditions (docs/specs/cinema-media.md section 10).
//!
//! [`PlaybackSource`] is the port; a Postgres implementation maps the `video_assets` / `renditions` rows to a
//! [`PlaybackAsset`], [`MemoryPlaybackSource`] serves tests and tools.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use thiserror::Error;

use crate::plan::SpritePlan;
use crate::playlist::MediaSet;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PlaybackError {
    #[error("playback storage failure: {0}")]
    Storage(String),
}

/// Extra facts of one subtitle rendition that the playlist model ([`crate::playlist::SubtitleVariant`]) does not
/// carry. Matched to the variant by `vtt_path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtitleExtra {
    pub vtt_path: String,
    /// Stable id of the track (the rendition id, as a decimal string).
    pub id: String,
    /// The styled ASS file kept next to the WebVTT, when the source was ASS.
    pub ass_path: Option<String>,
}

/// A playable asset: its renditions and scrub thumbnails.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackAsset {
    pub id: i64,
    /// The storage mount holding the files (1 is the instance bucket).
    pub mount_id: i64,
    /// Where the asset's files live in storage, without a trailing slash: `transcode/<job>/<attempt>` for
    /// worker outputs (kept where the worker wrote them), `video/<id>` for the default layout.
    pub storage_prefix: String,
    pub duration_ms: Option<i64>,
    /// Video rungs, audio renditions and subtitle files with their segment lists.
    pub media: MediaSet,
    pub subtitle_extras: Vec<SubtitleExtra>,
    /// `None`: no scrub thumbnails.
    pub sprite: Option<SpritePlan>,
}

impl PlaybackAsset {
    pub fn duration_secs(&self) -> Option<f64> {
        self.duration_ms.map(|ms| ms as f64 / 1000.0)
    }

    /// The extras of a subtitle variant.
    pub fn extra(&self, vtt_path: &str) -> Option<&SubtitleExtra> {
        self.subtitle_extras.iter().find(|e| e.vtt_path == vtt_path)
    }
}

#[async_trait]
pub trait PlaybackSource: Send + Sync {
    /// `None`: no such asset, or it is not playable (not published, or no video rendition yet).
    async fn asset(&self, asset_id: i64) -> Result<Option<PlaybackAsset>, PlaybackError>;
}

/// In-memory source for tests and tools.
#[derive(Default, Clone)]
pub struct MemoryPlaybackSource {
    assets: Arc<Mutex<HashMap<i64, PlaybackAsset>>>,
}

impl MemoryPlaybackSource {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or replace an asset.
    pub fn insert(&self, asset: PlaybackAsset) -> &Self {
        self.assets
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(asset.id, asset);
        self
    }

    pub fn remove(&self, id: i64) {
        self.assets
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&id);
    }
}

#[async_trait]
impl PlaybackSource for MemoryPlaybackSource {
    async fn asset(&self, asset_id: i64) -> Result<Option<PlaybackAsset>, PlaybackError> {
        Ok(self
            .assets
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&asset_id)
            .cloned())
    }
}

/// A small fixture asset (two rungs, two audio tracks, two subtitle tracks, sprites) for tests.
pub fn sample_asset(id: i64) -> PlaybackAsset {
    use crate::playlist::{AudioVariant, Segment, SubtitleVariant, VideoVariant};
    let segs = |dir: &str| {
        (0..3)
            .map(|n| Segment {
                path: format!("{dir}/seg_{n:05}.m4s"),
                duration_secs: 6.0,
            })
            .collect::<Vec<_>>()
    };
    let video = |name: &str, w: u32, h: u32, bw: u64| VideoVariant {
        name: name.into(),
        bandwidth_bps: bw,
        average_bandwidth_bps: None,
        width: w,
        height: h,
        frame_rate: 23.976,
        codecs: "avc1.640028".into(),
        playlist_path: format!("v/{name}/index.m3u8"),
        init_path: format!("v/{name}/init.mp4"),
        segments: segs(&format!("v/{name}")),
    };
    let audio = |n: u32, lang: &str, name: &str, default: bool| AudioVariant {
        name: name.into(),
        language_tag: Some(lang.into()),
        channels: 2,
        is_default: default,
        playlist_path: format!("a/{n:02}-{lang}-stereo/index.m3u8"),
        init_path: format!("a/{n:02}-{lang}-stereo/init.mp4"),
        segments: segs(&format!("a/{n:02}-{lang}-stereo")),
    };
    let sub = |n: u32, lang: &str, name: &str, forced: bool, default: bool| SubtitleVariant {
        name: name.into(),
        language_tag: Some(lang.into()),
        forced,
        is_default: default,
        vtt_path: format!("subs/{n:02}-{lang}.vtt"),
        playlist_path: format!("subs/{n:02}-{lang}.m3u8"),
        duration_secs: 18.0,
    };
    PlaybackAsset {
        id,
        mount_id: 1,
        storage_prefix: format!("video/{id}"),
        duration_ms: Some(18_000),
        media: MediaSet {
            videos: vec![
                video("720p", 1280, 720, 3_200_000),
                video("1080p", 1920, 1080, 6_400_000),
            ],
            audios: vec![
                audio(0, "en", "English", true),
                audio(1, "ja", "Japanese", false),
            ],
            subtitles: vec![
                sub(0, "en", "English", false, true),
                sub(1, "fr", "French", false, false),
            ],
        },
        subtitle_extras: vec![
            SubtitleExtra {
                vtt_path: "subs/00-en.vtt".into(),
                id: "9001".into(),
                ass_path: Some("subs/00-en.ass".into()),
            },
            SubtitleExtra {
                vtt_path: "subs/01-fr.vtt".into(),
                id: "9002".into(),
                ass_path: None,
            },
        ],
        sprite: Some(SpritePlan {
            interval_secs: 10,
            thumb_width: 160,
            thumb_height: 90,
            columns: 10,
            rows: 10,
            thumbs_total: 2,
            sheets: 1,
            sheet_pattern: "sprites/sheet-%03d.jpg".into(),
            vtt_path: "sprites/sprites.vtt".into(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_memory_source_serves_what_it_holds() {
        let s = MemoryPlaybackSource::new();
        assert!(s.asset(1).await.unwrap().is_none());
        s.insert(sample_asset(1));
        let a = s.asset(1).await.unwrap().unwrap();
        assert_eq!(a.duration_secs(), Some(18.0));
        assert_eq!(a.extra("subs/00-en.vtt").unwrap().id, "9001");
        s.remove(1);
        assert!(s.asset(1).await.unwrap().is_none());
    }
}
