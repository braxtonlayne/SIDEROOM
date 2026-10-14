//! Playback data of a ready asset (CIN-ING-007): the pure functions that turn
//! [`AssetPlayback`] into the grant, the subtitle and sprite links and the playlists the CDN
//! renders on request.
//!
//! The HLS playlists ffmpeg wrote to storage name their segments relatively, so they cannot be
//! served as stored (every URL needs its own `ex`/`sig`). The `hls` CDN kind therefore renders
//! the master playlist (which exists only virtually), every media playlist, the subtitle
//! wrapper playlists and the sprite map from the database rows, signing each URL with the same
//! 6 hour expiry. Segments, init files, WebVTT, ASS, fonts and sheet images are served as
//! stored objects.

use crate::assets::AssetPlayback;
use crate::playlist::{
    ManifestGrant, PlaylistConfig, UrlSigner, grant, render_audio_playlist, render_master,
    render_sprite_vtt, render_subtitle_playlist, render_video_playlist,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A rendered file and the type to serve it with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedFile {
    pub content_type: &'static str,
    pub body: String,
}

pub const MPEGURL: &str = "application/vnd.apple.mpegurl";
pub const VTT: &str = "text/vtt; charset=utf-8";

/// The playlist configuration (base path and 6 hour lifetime) of an asset.
pub fn config(pb: &AssetPlayback) -> PlaylistConfig {
    PlaylistConfig::new(pb.base_path.clone())
}

/// Render the file at `rest` (relative to the asset prefix, e.g. `master.m3u8`,
/// `v/1080p/index.m3u8`) if it is one the server renders; `None` for stored objects and for
/// paths the asset does not have.
pub fn render_file(
    pb: &AssetPlayback,
    rest: &str,
    signer: &dyn UrlSigner,
    now: u64,
) -> Option<RenderedFile> {
    let cfg = config(pb);
    let playlist = |body: String| RenderedFile {
        content_type: MPEGURL,
        body,
    };
    if rest == "master.m3u8" {
        if pb.media.videos.is_empty() {
            return None;
        }
        return Some(playlist(render_master(&pb.media, &cfg, signer, now)));
    }
    if let Some(v) = pb.media.videos.iter().find(|v| v.playlist_path == rest) {
        return Some(playlist(render_video_playlist(v, &cfg, signer, now)));
    }
    if let Some(a) = pb.media.audios.iter().find(|a| a.playlist_path == rest) {
        return Some(playlist(render_audio_playlist(a, &cfg, signer, now)));
    }
    if let Some(s) = pb.media.subtitles.iter().find(|s| s.playlist_path == rest) {
        return Some(playlist(render_subtitle_playlist(s, &cfg, signer, now)));
    }
    if let Some(sp) = &pb.sprite
        && sp.vtt_path == rest
    {
        return Some(RenderedFile {
            content_type: VTT,
            body: render_sprite_vtt(sp, pb.duration_secs, &cfg, signer, now),
        });
    }
    None
}

/// Signed URL of an asset file.
pub fn signed(pb: &AssetPlayback, rest: &str, signer: &dyn UrlSigner, expires_at: u64) -> String {
    signer.signed_url(&format!("{}/{rest}", pb.base_path), expires_at)
}

/// One subtitle track as the player wants it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SubtitleLink {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub forced: bool,
    pub is_default: bool,
    pub vtt_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ass_url: Option<String>,
}

/// The playback answer of an asset: manifest grant plus the side files (times in Unix seconds).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PlaybackGrant {
    pub master_url: String,
    pub expires_at: u64,
    pub refresh_at: u64,
    /// Whole seconds.
    pub duration_secs: u64,
    pub subtitles: Vec<SubtitleLink>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sprites_url: Option<String>,
}

pub fn manifest_grant(pb: &AssetPlayback, signer: &dyn UrlSigner, now: u64) -> ManifestGrant {
    grant(&config(pb), signer, now)
}

/// The full playback answer. Call it after the permission and age checks; it mints URLs for
/// whoever asks.
pub fn playback_grant(pb: &AssetPlayback, signer: &dyn UrlSigner, now: u64) -> PlaybackGrant {
    let g = manifest_grant(pb, signer, now);
    let cfg = config(pb);
    let expires = now + cfg.ttl_secs;
    let subtitles = pb
        .media
        .subtitles
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let ass = pb
                .ass_paths
                .iter()
                .find(|(vtt, _)| *vtt == s.vtt_path)
                .map(|(_, ass)| signed(pb, ass, signer, expires));
            SubtitleLink {
                id: i.to_string(),
                language: s.language_tag.clone(),
                label: Some(s.name.clone()),
                forced: s.forced,
                is_default: s.is_default,
                vtt_url: signed(pb, &s.vtt_path, signer, expires),
                ass_url: ass,
            }
        })
        .collect();
    PlaybackGrant {
        master_url: g.master_url,
        expires_at: g.expires_at,
        refresh_at: g.refresh_at,
        duration_secs: pb.duration_secs.max(0.0).round() as u64,
        subtitles,
        sprites_url: pb
            .sprite
            .as_ref()
            .map(|sp| signed(pb, &sp.vtt_path, signer, expires)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playlist::{AudioVariant, MediaSet, Segment, SubtitleVariant, VideoVariant};

    struct S;
    impl UrlSigner for S {
        fn signed_url(&self, path: &str, expires_at: u64) -> String {
            format!("{path}?ex={expires_at}&sig=S")
        }
    }

    pub(crate) fn sample() -> AssetPlayback {
        let seg = |dir: &str, n: u32| Segment {
            path: format!("{dir}/seg_{n:05}.m4s"),
            duration_secs: 6.0,
        };
        AssetPlayback {
            asset_id: 5,
            base_path: "/cdn/hls/9/1".into(),
            duration_secs: 12.0,
            profile_version: 1,
            media: MediaSet {
                videos: vec![VideoVariant {
                    name: "1080p".into(),
                    bandwidth_bps: 6_000_000,
                    average_bandwidth_bps: None,
                    width: 1920,
                    height: 1080,
                    frame_rate: 24.0,
                    codecs: "avc1.640028".into(),
                    playlist_path: "v/1080p/index.m3u8".into(),
                    init_path: "v/1080p/init.mp4".into(),
                    segments: vec![seg("v/1080p", 0), seg("v/1080p", 1)],
                }],
                audios: vec![AudioVariant {
                    name: "English".into(),
                    language_tag: Some("en".into()),
                    channels: 2,
                    is_default: true,
                    playlist_path: "a/00-eng-stereo/index.m3u8".into(),
                    init_path: "a/00-eng-stereo/init.mp4".into(),
                    segments: vec![seg("a/00-eng-stereo", 0)],
                }],
                subtitles: vec![SubtitleVariant {
                    name: "English".into(),
                    language_tag: Some("en".into()),
                    forced: false,
                    is_default: false,
                    vtt_path: "subs/00-eng.vtt".into(),
                    playlist_path: "subs/00-eng.m3u8".into(),
                    duration_secs: 12.0,
                }],
            },
            sprite: None,
            ass_paths: vec![("subs/00-eng.vtt".into(), "subs/00-eng.ass".into())],
        }
    }

    #[test]
    fn renders_the_virtual_and_media_playlists_with_signed_urls() {
        let pb = sample();
        let m = render_file(&pb, "master.m3u8", &S, 1000).unwrap();
        assert_eq!(m.content_type, MPEGURL);
        assert!(
            m.body
                .contains("/cdn/hls/9/1/v/1080p/index.m3u8?ex=22600&sig=S")
        );
        let v = render_file(&pb, "v/1080p/index.m3u8", &S, 1000).unwrap();
        assert!(
            v.body
                .contains("/cdn/hls/9/1/v/1080p/init.mp4?ex=22600&sig=S")
        );
        assert!(
            v.body
                .contains("/cdn/hls/9/1/v/1080p/seg_00001.m4s?ex=22600&sig=S")
        );
        assert!(render_file(&pb, "a/00-eng-stereo/index.m3u8", &S, 1000).is_some());
        assert!(
            render_file(&pb, "subs/00-eng.m3u8", &S, 1000)
                .unwrap()
                .body
                .contains("/cdn/hls/9/1/subs/00-eng.vtt?ex=22600&sig=S")
        );
        // Stored objects and unknown paths are not rendered.
        assert!(render_file(&pb, "v/1080p/seg_00000.m4s", &S, 1000).is_none());
        assert!(render_file(&pb, "v/720p/index.m3u8", &S, 1000).is_none());
        assert!(render_file(&pb, "../master.m3u8", &S, 1000).is_none());
    }

    #[test]
    fn grant_lists_subtitles_and_refresh_time() {
        let pb = sample();
        let g = playback_grant(&pb, &S, 1000);
        assert_eq!(g.master_url, "/cdn/hls/9/1/master.m3u8?ex=22600&sig=S");
        assert_eq!(g.expires_at, 22_600);
        assert_eq!(g.refresh_at, 22_000);
        assert_eq!(g.duration_secs, 12);
        assert_eq!(g.subtitles.len(), 1);
        assert_eq!(
            g.subtitles[0].ass_url.as_deref(),
            Some("/cdn/hls/9/1/subs/00-eng.ass?ex=22600&sig=S")
        );
        assert!(g.sprites_url.is_none());
        let j = serde_json::to_value(&g).unwrap();
        assert!(j.get("sprites_url").is_none());
    }
}
