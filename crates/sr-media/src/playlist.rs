//! Master and media playlist generation with signed segment URLs (CIN-ING-007).
//!
//! Pure functions over a [`UrlSigner`], which has the shape of `sr_storage::cdn::CdnSigner`
//! (`path` + expiry in, `path?ex=..&sig=..` out). Every URL in a rendered playlist expires
//! together, `ttl_secs` (6 h, spec section 3) after `now`; the player asks for a new
//! [`ManifestGrant`] before [`ManifestGrant::refresh_at`] and swaps manifests without a stall.

use serde::{Deserialize, Serialize};
use sr_storage::cdn::CdnSigner;
use thiserror::Error;

/// Anything that can sign a CDN path. [`CdnSigner`] implements it.
pub trait UrlSigner: Send + Sync {
    /// `path?ex=<expires_at>&sig=<..>`.
    fn signed_url(&self, path: &str, expires_at: u64) -> String;
}

impl UrlSigner for CdnSigner {
    fn signed_url(&self, path: &str, expires_at: u64) -> String {
        CdnSigner::signed_url(self, path, expires_at)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistConfig {
    /// URL path prefix of the asset's files, no trailing slash, e.g. `/cdn/hls/123`.
    pub base_path: String,
    /// How long signed URLs stay valid. 6 h.
    pub ttl_secs: u64,
    /// The player refreshes this long before expiry.
    pub refresh_margin_secs: u64,
}

impl PlaylistConfig {
    pub fn new(base_path: impl Into<String>) -> Self {
        Self {
            base_path: base_path.into().trim_end_matches('/').to_string(),
            ttl_secs: 6 * 3600,
            refresh_margin_secs: 600,
        }
    }
    fn url(&self, signer: &dyn UrlSigner, rel: &str, expires_at: u64) -> String {
        signer.signed_url(&format!("{}/{}", self.base_path, rel), expires_at)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    /// Path relative to the asset root, e.g. `v/1080p/seg_00001.m4s`.
    pub path: String,
    pub duration_secs: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoVariant {
    pub name: String,
    /// Peak bits per second including the audio it is usually paired with.
    pub bandwidth_bps: u64,
    pub average_bandwidth_bps: Option<u64>,
    pub width: u32,
    pub height: u32,
    pub frame_rate: f64,
    /// Video codec string, e.g. `avc1.640028`.
    pub codecs: String,
    pub playlist_path: String,
    pub init_path: String,
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioVariant {
    pub name: String,
    pub language_tag: Option<String>,
    pub channels: u32,
    pub is_default: bool,
    pub playlist_path: String,
    pub init_path: String,
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubtitleVariant {
    pub name: String,
    pub language_tag: Option<String>,
    pub forced: bool,
    pub is_default: bool,
    /// The WebVTT file, e.g. `subs/00-eng.vtt`.
    pub vtt_path: String,
    /// The playlist wrapping it, e.g. `subs/00-eng.m3u8`.
    pub playlist_path: String,
    pub duration_secs: f64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MediaSet {
    pub videos: Vec<VideoVariant>,
    pub audios: Vec<AudioVariant>,
    pub subtitles: Vec<SubtitleVariant>,
}

const AUDIO_CODEC: &str = "mp4a.40.2";

/// HLS quoted-string attribute values may not contain quotes or line breaks.
fn attr(s: &str) -> String {
    s.chars()
        .map(|c| if c == '"' { '\'' } else { c })
        .filter(|c| !c.is_control())
        .collect()
}

fn yes_no(b: bool) -> &'static str {
    if b { "YES" } else { "NO" }
}

/// Render the master playlist: audio and subtitle groups plus one variant per video rung,
/// lowest bandwidth first so playback starts quickly.
pub fn render_master(
    set: &MediaSet,
    cfg: &PlaylistConfig,
    signer: &dyn UrlSigner,
    now: u64,
) -> String {
    let expires = now + cfg.ttl_secs;
    let mut out = String::from("#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-INDEPENDENT-SEGMENTS\n");
    for a in &set.audios {
        out.push_str(&format!(
            "#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"{}\",{}DEFAULT={},AUTOSELECT=YES,CHANNELS=\"{}\",URI=\"{}\"\n",
            attr(&a.name),
            a.language_tag
                .as_deref()
                .map(|l| format!("LANGUAGE=\"{}\",", attr(l)))
                .unwrap_or_default(),
            yes_no(a.is_default),
            a.channels,
            cfg.url(signer, &a.playlist_path, expires)
        ));
    }
    for s in &set.subtitles {
        out.push_str(&format!(
            "#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"subs\",NAME=\"{}\",{}DEFAULT={},AUTOSELECT={},FORCED={},URI=\"{}\"\n",
            attr(&s.name),
            s.language_tag
                .as_deref()
                .map(|l| format!("LANGUAGE=\"{}\",", attr(l)))
                .unwrap_or_default(),
            yes_no(s.is_default),
            yes_no(s.is_default || s.forced),
            yes_no(s.forced),
            cfg.url(signer, &s.playlist_path, expires)
        ));
    }
    let mut videos: Vec<&VideoVariant> = set.videos.iter().collect();
    videos.sort_by_key(|v| v.bandwidth_bps);
    for v in videos {
        let mut codecs = v.codecs.clone();
        if !set.audios.is_empty() {
            codecs.push(',');
            codecs.push_str(AUDIO_CODEC);
        }
        let mut line = format!("#EXT-X-STREAM-INF:BANDWIDTH={}", v.bandwidth_bps);
        if let Some(avg) = v.average_bandwidth_bps {
            line.push_str(&format!(",AVERAGE-BANDWIDTH={avg}"));
        }
        line.push_str(&format!(
            ",RESOLUTION={}x{},FRAME-RATE={:.3},CODECS=\"{}\"",
            v.width,
            v.height,
            v.frame_rate,
            attr(&codecs)
        ));
        if !set.audios.is_empty() {
            line.push_str(",AUDIO=\"aud\"");
        }
        if !set.subtitles.is_empty() {
            line.push_str(",SUBTITLES=\"subs\"");
        }
        out.push_str(&line);
        out.push('\n');
        out.push_str(&cfg.url(signer, &v.playlist_path, expires));
        out.push('\n');
    }
    out
}

fn render_segments(
    init: &str,
    segments: &[Segment],
    cfg: &PlaylistConfig,
    signer: &dyn UrlSigner,
    now: u64,
) -> String {
    let expires = now + cfg.ttl_secs;
    let target = segments
        .iter()
        .map(|s| s.duration_secs)
        .fold(0.0_f64, f64::max)
        .ceil()
        .max(1.0) as u64;
    let mut out = format!(
        "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:{target}\n#EXT-X-MEDIA-SEQUENCE:0\n\
         #EXT-X-PLAYLIST-TYPE:VOD\n#EXT-X-INDEPENDENT-SEGMENTS\n#EXT-X-MAP:URI=\"{}\"\n",
        cfg.url(signer, init, expires)
    );
    for s in segments {
        out.push_str(&format!(
            "#EXTINF:{:.6},\n{}\n",
            s.duration_secs,
            cfg.url(signer, &s.path, expires)
        ));
    }
    out.push_str("#EXT-X-ENDLIST\n");
    out
}

/// A video rung's media playlist (fMP4 init + signed segments).
pub fn render_video_playlist(
    v: &VideoVariant,
    cfg: &PlaylistConfig,
    signer: &dyn UrlSigner,
    now: u64,
) -> String {
    render_segments(&v.init_path, &v.segments, cfg, signer, now)
}

/// An audio rendition's media playlist.
pub fn render_audio_playlist(
    a: &AudioVariant,
    cfg: &PlaylistConfig,
    signer: &dyn UrlSigner,
    now: u64,
) -> String {
    render_segments(&a.init_path, &a.segments, cfg, signer, now)
}

/// The one-segment playlist HLS needs in front of a WebVTT file.
pub fn render_subtitle_playlist(
    s: &SubtitleVariant,
    cfg: &PlaylistConfig,
    signer: &dyn UrlSigner,
    now: u64,
) -> String {
    let expires = now + cfg.ttl_secs;
    let target = s.duration_secs.ceil().max(1.0) as u64;
    format!(
        "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:{target}\n#EXT-X-MEDIA-SEQUENCE:0\n\
         #EXT-X-PLAYLIST-TYPE:VOD\n#EXTINF:{:.3},\n{}\n#EXT-X-ENDLIST\n",
        s.duration_secs,
        cfg.url(signer, &s.vtt_path, expires)
    )
}

/// Thumbnail sprite map whose sheet URLs are signed like everything else.
pub fn render_sprite_vtt(
    sprite: &crate::plan::SpritePlan,
    duration_secs: f64,
    cfg: &PlaylistConfig,
    signer: &dyn UrlSigner,
    now: u64,
) -> String {
    let expires = now + cfg.ttl_secs;
    crate::subtitles::sprite_vtt(sprite, duration_secs, &|n| {
        cfg.url(signer, &sprite.sheet_path(n), expires)
    })
}

/// What the playback endpoint returns, and what the refresh endpoint returns again later.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestGrant {
    /// Signed URL of `master.m3u8`.
    pub master_url: String,
    pub expires_at: u64,
    /// Unix time the player should ask for a new grant.
    pub refresh_at: u64,
}

/// Mint a grant for the asset's master playlist. The caller has already checked that the user
/// may watch this title; the refresh endpoint calls this again with the same check.
pub fn grant(cfg: &PlaylistConfig, signer: &dyn UrlSigner, now: u64) -> ManifestGrant {
    let expires_at = now + cfg.ttl_secs;
    ManifestGrant {
        master_url: cfg.url(signer, "master.m3u8", expires_at),
        expires_at,
        refresh_at: expires_at.saturating_sub(cfg.refresh_margin_secs).max(now),
    }
}

/// Client-side helper (also used by tests): is it time to refresh?
pub fn needs_refresh(g: &ManifestGrant, now: u64) -> bool {
    now >= g.refresh_at
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HlsParseError {
    #[error("segment name {0:?} is not a plain file name")]
    UnsafeName(String),
    #[error("playlist has too many segments")]
    TooManySegments,
    #[error("playlist has no segments")]
    Empty,
    #[error("bad duration: {0}")]
    BadDuration(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedMediaPlaylist {
    pub init_name: Option<String>,
    /// `(file name, duration)` in playlist order.
    pub segments: Vec<(String, f64)>,
}

pub const MAX_PARSED_SEGMENTS: usize = 200_000;

fn plain_name(n: &str) -> Result<String, HlsParseError> {
    let ok = !n.is_empty()
        && n.len() <= 128
        && !n.contains(['/', '\\', ':', '\0', '?', '#'])
        && n != ".."
        && n != ".";
    if ok {
        Ok(n.to_string())
    } else {
        Err(HlsParseError::UnsafeName(n.chars().take(64).collect()))
    }
}

/// Read the playlist ffmpeg's HLS muxer wrote, to build the stored segment index. Names must be
/// plain file names (no directories), because the worker joins them onto its output sandbox.
pub fn parse_media_playlist(text: &str) -> Result<ParsedMediaPlaylist, HlsParseError> {
    let mut init = None;
    let mut segments = Vec::new();
    let mut pending: Option<f64> = None;
    for line in text.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("#EXT-X-MAP:") {
            if let Some(uri) = rest
                .split("URI=\"")
                .nth(1)
                .and_then(|r| r.split('"').next())
            {
                init = Some(plain_name(uri)?);
            }
        } else if let Some(rest) = line.strip_prefix("#EXTINF:") {
            let dur = rest.split(',').next().unwrap_or("");
            let d: f64 = dur
                .trim()
                .parse()
                .map_err(|_| HlsParseError::BadDuration(dur.chars().take(32).collect()))?;
            if !d.is_finite() || d < 0.0 {
                return Err(HlsParseError::BadDuration(dur.chars().take(32).collect()));
            }
            pending = Some(d);
        } else if !line.is_empty()
            && !line.starts_with('#')
            && let Some(d) = pending.take()
        {
            if segments.len() >= MAX_PARSED_SEGMENTS {
                return Err(HlsParseError::TooManySegments);
            }
            segments.push((plain_name(line)?, d));
        }
    }
    if segments.is_empty() {
        return Err(HlsParseError::Empty);
    }
    Ok(ParsedMediaPlaylist {
        init_name: init,
        segments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeSigner;
    impl UrlSigner for FakeSigner {
        fn signed_url(&self, path: &str, expires_at: u64) -> String {
            format!("{path}?ex={expires_at}&sig=SIG")
        }
    }

    fn seg(path: &str, d: f64) -> Segment {
        Segment {
            path: path.into(),
            duration_secs: d,
        }
    }

    fn set() -> MediaSet {
        let video = |name: &str, bw, w, h, codecs: &str| VideoVariant {
            name: name.into(),
            bandwidth_bps: bw,
            average_bandwidth_bps: Some(bw * 9 / 10),
            width: w,
            height: h,
            frame_rate: 23.976,
            codecs: codecs.into(),
            playlist_path: format!("v/{name}/index.m3u8"),
            init_path: format!("v/{name}/init.mp4"),
            segments: vec![
                seg(&format!("v/{name}/seg_00000.m4s"), 6.006),
                seg(&format!("v/{name}/seg_00001.m4s"), 6.006),
                seg(&format!("v/{name}/seg_00002.m4s"), 2.5),
            ],
        };
        MediaSet {
            videos: vec![
                video("1080p", 6_300_000, 1920, 1080, "avc1.640028"),
                video("480p", 1_700_000, 854, 480, "avc1.64001E"),
                video("720p", 3_200_000, 1280, 720, "avc1.64001F"),
            ],
            audios: vec![
                AudioVariant {
                    name: "English".into(),
                    language_tag: Some("en".into()),
                    channels: 2,
                    is_default: true,
                    playlist_path: "a/00-eng-stereo/index.m3u8".into(),
                    init_path: "a/00-eng-stereo/init.mp4".into(),
                    segments: vec![seg("a/00-eng-stereo/seg_00000.m4s", 6.0)],
                },
                AudioVariant {
                    name: "Jap\"anese\n".into(),
                    language_tag: Some("ja".into()),
                    channels: 2,
                    is_default: false,
                    playlist_path: "a/01-jpn-stereo/index.m3u8".into(),
                    init_path: "a/01-jpn-stereo/init.mp4".into(),
                    segments: vec![seg("a/01-jpn-stereo/seg_00000.m4s", 6.0)],
                },
            ],
            subtitles: vec![SubtitleVariant {
                name: "English (Forced)".into(),
                language_tag: Some("en".into()),
                forced: true,
                is_default: false,
                vtt_path: "subs/00-eng.vtt".into(),
                playlist_path: "subs/00-eng.m3u8".into(),
                duration_secs: 5432.123,
            }],
        }
    }

    fn cfg() -> PlaylistConfig {
        PlaylistConfig::new("/cdn/hls/42/")
    }

    #[test]
    fn master_playlist_exact() {
        let m = render_master(&set(), &cfg(), &FakeSigner, 1000);
        let exp = 1000 + 21_600;
        let expected = format!(
            "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-INDEPENDENT-SEGMENTS\n\
             #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"English\",LANGUAGE=\"en\",DEFAULT=YES,AUTOSELECT=YES,CHANNELS=\"2\",URI=\"/cdn/hls/42/a/00-eng-stereo/index.m3u8?ex={exp}&sig=SIG\"\n\
             #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"Jap'anese\",LANGUAGE=\"ja\",DEFAULT=NO,AUTOSELECT=YES,CHANNELS=\"2\",URI=\"/cdn/hls/42/a/01-jpn-stereo/index.m3u8?ex={exp}&sig=SIG\"\n\
             #EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"subs\",NAME=\"English (Forced)\",LANGUAGE=\"en\",DEFAULT=NO,AUTOSELECT=YES,FORCED=YES,URI=\"/cdn/hls/42/subs/00-eng.m3u8?ex={exp}&sig=SIG\"\n\
             #EXT-X-STREAM-INF:BANDWIDTH=1700000,AVERAGE-BANDWIDTH=1530000,RESOLUTION=854x480,FRAME-RATE=23.976,CODECS=\"avc1.64001E,mp4a.40.2\",AUDIO=\"aud\",SUBTITLES=\"subs\"\n\
             /cdn/hls/42/v/480p/index.m3u8?ex={exp}&sig=SIG\n\
             #EXT-X-STREAM-INF:BANDWIDTH=3200000,AVERAGE-BANDWIDTH=2880000,RESOLUTION=1280x720,FRAME-RATE=23.976,CODECS=\"avc1.64001F,mp4a.40.2\",AUDIO=\"aud\",SUBTITLES=\"subs\"\n\
             /cdn/hls/42/v/720p/index.m3u8?ex={exp}&sig=SIG\n\
             #EXT-X-STREAM-INF:BANDWIDTH=6300000,AVERAGE-BANDWIDTH=5670000,RESOLUTION=1920x1080,FRAME-RATE=23.976,CODECS=\"avc1.640028,mp4a.40.2\",AUDIO=\"aud\",SUBTITLES=\"subs\"\n\
             /cdn/hls/42/v/1080p/index.m3u8?ex={exp}&sig=SIG\n"
        );
        assert_eq!(m, expected);
    }

    #[test]
    fn master_without_audio_or_subs() {
        let mut s = set();
        s.audios.clear();
        s.subtitles.clear();
        let m = render_master(&s, &cfg(), &FakeSigner, 0);
        assert!(!m.contains("AUDIO="));
        assert!(!m.contains("SUBTITLES"));
        assert!(m.contains("CODECS=\"avc1.64001E\""));
    }

    #[test]
    fn media_playlist_signs_every_url_with_one_expiry() {
        let s = set();
        let p = render_video_playlist(&s.videos[0], &cfg(), &FakeSigner, 100);
        let exp = 100 + 21_600;
        assert!(p.contains("#EXT-X-TARGETDURATION:7\n"));
        assert!(p.contains("#EXT-X-PLAYLIST-TYPE:VOD\n"));
        assert!(p.contains(&format!(
            "#EXT-X-MAP:URI=\"/cdn/hls/42/v/1080p/init.mp4?ex={exp}&sig=SIG\"\n"
        )));
        assert!(p.contains(&format!(
            "#EXTINF:6.006000,\n/cdn/hls/42/v/1080p/seg_00000.m4s?ex={exp}&sig=SIG\n"
        )));
        assert!(p.contains("#EXTINF:2.500000,"));
        assert!(p.ends_with("#EXT-X-ENDLIST\n"));
        assert_eq!(p.matches("sig=SIG").count(), 4);
        assert_eq!(p.matches(&format!("ex={exp}")).count(), 4);
    }

    #[test]
    fn subtitle_playlist_wraps_the_vtt() {
        let s = set();
        let p = render_subtitle_playlist(&s.subtitles[0], &cfg(), &FakeSigner, 0);
        assert!(p.contains("#EXT-X-TARGETDURATION:5433\n"));
        assert!(p.contains("#EXTINF:5432.123,\n/cdn/hls/42/subs/00-eng.vtt?ex=21600&sig=SIG\n"));
    }

    #[test]
    fn grant_and_refresh_window() {
        let g = grant(&cfg(), &FakeSigner, 1_000_000);
        assert_eq!(g.expires_at, 1_021_600);
        assert_eq!(g.refresh_at, 1_021_000);
        assert_eq!(g.master_url, "/cdn/hls/42/master.m3u8?ex=1021600&sig=SIG");
        assert!(!needs_refresh(&g, 1_000_001));
        assert!(!needs_refresh(&g, 1_020_999));
        assert!(needs_refresh(&g, 1_021_000));
        // A refreshed grant moves the window forward.
        let g2 = grant(&cfg(), &FakeSigner, 1_021_000);
        assert!(g2.expires_at > g.expires_at);
    }

    #[test]
    fn works_with_the_real_cdn_signer() {
        let signer = CdnSigner::new(vec![7u8; 32]);
        let g = grant(&cfg(), &signer, 5000);
        let (path, query) = g.master_url.split_once('?').unwrap();
        assert_eq!(path, "/cdn/hls/42/master.m3u8");
        assert_eq!(signer.verify_query(path, query, 5001), Ok(()));
        assert!(signer.verify_query(path, query, g.expires_at).is_err());
        assert!(
            signer
                .verify_query("/cdn/hls/43/master.m3u8", query, 5001)
                .is_err()
        );
    }

    #[test]
    fn sprite_urls_are_signed() {
        let plan = crate::plan::SpritePlan {
            interval_secs: 10,
            thumb_width: 160,
            thumb_height: 90,
            columns: 10,
            rows: 10,
            thumbs_total: 2,
            sheets: 1,
            sheet_pattern: "sprites/sheet-%03d.jpg".into(),
            vtt_path: "sprites/sprites.vtt".into(),
        };
        let v = render_sprite_vtt(&plan, 15.0, &cfg(), &FakeSigner, 0);
        assert!(v.contains("/cdn/hls/42/sprites/sheet-001.jpg?ex=21600&sig=SIG#xywh=0,0,160,90"));
    }

    #[test]
    fn parse_ffmpeg_playlist() {
        let text = "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:7\n#EXT-X-MEDIA-SEQUENCE:0\n\
                    #EXT-X-PLAYLIST-TYPE:VOD\n#EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:6.006000,\n\
                    seg_00000.m4s\n#EXTINF:6.006000,\nseg_00001.m4s\n#EXTINF:1.5,\nseg_00002.m4s\n#EXT-X-ENDLIST\n";
        let p = parse_media_playlist(text).unwrap();
        assert_eq!(p.init_name.as_deref(), Some("init.mp4"));
        assert_eq!(p.segments.len(), 3);
        assert_eq!(p.segments[2], ("seg_00002.m4s".to_string(), 1.5));
    }

    #[test]
    fn parse_rejects_traversal_and_junk() {
        for bad in [
            "#EXTINF:6,\n../../etc/passwd\n",
            "#EXTINF:6,\nsub/dir.m4s\n",
            "#EXTINF:6,\nC:evil.m4s\n",
            "#EXT-X-MAP:URI=\"/abs/init.mp4\"\n#EXTINF:6,\na.m4s\n",
        ] {
            assert!(
                matches!(parse_media_playlist(bad), Err(HlsParseError::UnsafeName(_))),
                "{bad}"
            );
        }
        assert_eq!(parse_media_playlist("#EXTM3U\n"), Err(HlsParseError::Empty));
        assert!(matches!(
            parse_media_playlist("#EXTINF:abc,\na.m4s\n"),
            Err(HlsParseError::BadDuration(_))
        ));
        assert!(matches!(
            parse_media_playlist("#EXTINF:-1,\na.m4s\n"),
            Err(HlsParseError::BadDuration(_))
        ));
    }
}
