//! Job planning: a pure function from a [`ProbeResult`] plus settings to a serialisable plan.
//!
//! * [`plan_proxy_job`] is the 480p review proxy (CIN-ING-002).
//! * [`plan_ladder_job`] is the full ladder job (CIN-ING-003): rungs never above the source or
//!   1080p, which audio tracks to keep, subtitle extraction, sprite sheets, poster frames and
//!   closed, aligned GOP settings.
//!
//! The plan carries every output path (relative, forward slashes) so the server that stores
//! renditions and the worker that produces them agree without any further negotiation.

use crate::probe::{
    AttachmentStream, AudioStream, HdrKind, ProbeResult, SubtitleFormat, SubtitleStream,
    TonemapSource, VideoStream,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Bump when the encode profile changes; stored with each rendition so "re-transcode
/// everything older than profile N" (CIN-ING-011) is a query.
pub const PROFILE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RungTemplate {
    pub name: String,
    pub box_width: u32,
    pub box_height: u32,
    pub video_kbps: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscodeSettings {
    /// Hard cap on output height. Never above 1080 (D-086).
    pub max_height: u32,
    pub rungs: Vec<RungTemplate>,
    pub audio_stereo_kbps: u32,
    pub audio_surround_kbps: u32,
    /// Also keep a 5.1 AAC rung for sources with 6 or more channels.
    pub keep_surround: bool,
    pub keep_commentary: bool,
    pub max_audio_tracks: usize,
    /// Preferred languages (ISO 639-2), first match becomes the default track.
    pub preferred_languages: Vec<String>,
    pub segment_secs: u32,
    pub gop_secs: u32,
    pub max_fps: u32,
    pub sprite_interval_secs: u32,
    pub sprite_thumb_width: u32,
    pub sprite_columns: u32,
    pub sprite_rows: u32,
    pub poster_candidates: u32,
    pub max_subtitles: usize,
    pub max_fonts: usize,
    /// Refuse sources longer than this.
    pub max_duration_secs: f64,
    pub proxy_height: u32,
    pub proxy_kbps: u32,
}

impl Default for TranscodeSettings {
    fn default() -> Self {
        let rung = |name: &str, w, h, kbps| RungTemplate {
            name: name.to_string(),
            box_width: w,
            box_height: h,
            video_kbps: kbps,
        };
        Self {
            max_height: 1080,
            rungs: vec![
                rung("1080p", 1920, 1080, 6000),
                rung("720p", 1280, 720, 3000),
                rung("480p", 854, 480, 1500),
            ],
            audio_stereo_kbps: 160,
            audio_surround_kbps: 384,
            keep_surround: false,
            keep_commentary: false,
            max_audio_tracks: 8,
            preferred_languages: vec!["eng".to_string()],
            segment_secs: 6,
            gop_secs: 2,
            max_fps: 60,
            sprite_interval_secs: 10,
            sprite_thumb_width: 160,
            sprite_columns: 10,
            sprite_rows: 10,
            poster_candidates: 5,
            max_subtitles: 32,
            max_fonts: 64,
            max_duration_secs: 24.0 * 3600.0,
            proxy_height: 480,
            proxy_kbps: 900,
        }
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum PlanError {
    #[error("the file has no video stream")]
    NoVideo,
    #[error("the file does not report a duration")]
    NoDuration,
    #[error("the file is {0:.0} s long, over the limit")]
    TooLong(f64),
    #[error("invalid settings: {0}")]
    BadSettings(&'static str),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceSummary {
    pub duration_secs: f64,
    pub width: u32,
    pub height: u32,
    pub display_width: u32,
    pub display_height: u32,
    pub video_codec: String,
    pub hdr: HdrKind,
    pub interlaced: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoPlan {
    /// `-map 0:v:<ordinal>`.
    pub source_ordinal: u32,
    /// Constant output frame rate as a fraction, shared by every rung so GOPs align.
    pub fps_num: u32,
    pub fps_den: u32,
    /// Keyframe interval in frames (`gop_secs * fps`), identical on every rung.
    pub gop_frames: u32,
    pub deinterlace: bool,
    pub tonemap: Option<TonemapSource>,
}

impl VideoPlan {
    pub fn fps(&self) -> f64 {
        f64::from(self.fps_num) / f64::from(self.fps_den.max(1))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RungPlan {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub video_kbps: u32,
    pub maxrate_kbps: u32,
    pub bufsize_kbps: u32,
    /// H.264 level, e.g. `4.0`.
    pub level: String,
    /// RFC 6381 codec string, e.g. `avc1.640028`.
    pub codecs: String,
    pub dir: String,
    pub playlist: String,
    pub init_segment: String,
    pub segment_pattern: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioLayout {
    Stereo,
    Surround51,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioOutput {
    /// `-map 0:a:<ordinal>`.
    pub source_ordinal: u32,
    pub language: Option<String>,
    /// BCP 47 tag for HLS `LANGUAGE` (two-letter where known).
    pub language_tag: Option<String>,
    pub label: String,
    pub layout: AudioLayout,
    pub channels: u32,
    pub bitrate_kbps: u32,
    pub is_default: bool,
    pub commentary: bool,
    pub dir: String,
    pub playlist: String,
    pub init_segment: String,
    pub segment_pattern: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractFormat {
    Ass,
    Srt,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubtitleOutput {
    /// `-map 0:s:<ordinal>`.
    pub source_ordinal: u32,
    pub language: Option<String>,
    pub language_tag: Option<String>,
    pub label: String,
    pub forced: bool,
    pub hearing_impaired: bool,
    pub is_default: bool,
    /// What ffmpeg extracts to; the Rust converter then produces the WebVTT.
    pub extract: ExtractFormat,
    pub raw_path: String,
    /// ASS files are retained for styled rendering (JASSUB); other raw files are discarded.
    pub keep_raw: bool,
    pub vtt_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// Bitmap subtitles (PGS, VobSub, DVB): flagged for OCR later, not converted now.
    ImageBasedOcrLater,
    UnsupportedCodec,
    OverLimit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkippedSubtitle {
    pub source_ordinal: u32,
    pub language: Option<String>,
    pub codec: String,
    pub reason: SkipReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FontOutput {
    /// `-dump_attachment:t:<ordinal>`.
    pub attachment_ordinal: u32,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpritePlan {
    pub interval_secs: u32,
    pub thumb_width: u32,
    pub thumb_height: u32,
    pub columns: u32,
    pub rows: u32,
    pub thumbs_total: u32,
    pub sheets: u32,
    /// printf-style pattern for ffmpeg, e.g. `sprites/sheet-%03d.jpg`.
    pub sheet_pattern: String,
    pub vtt_path: String,
}

impl SpritePlan {
    pub fn sheet_path(&self, n: u32) -> String {
        format!("sprites/sheet-{n:03}.jpg")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PosterPlan {
    pub at_secs: f64,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HlsSettings {
    pub segment_secs: u32,
    pub gop_secs: u32,
    /// Always fMP4 (CMAF).
    pub fmp4: bool,
    pub closed_gop: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobPlan {
    pub profile_version: u32,
    pub source: SourceSummary,
    pub video: VideoPlan,
    pub rungs: Vec<RungPlan>,
    pub audio: Vec<AudioOutput>,
    pub subtitles: Vec<SubtitleOutput>,
    pub skipped_subtitles: Vec<SkippedSubtitle>,
    pub fonts: Vec<FontOutput>,
    pub sprite: SpritePlan,
    pub posters: Vec<PosterPlan>,
    pub hls: HlsSettings,
    pub warnings: Vec<String>,
}

impl JobPlan {
    /// Every relative path the finished job must have produced (playlists, inits, VTTs,
    /// sprite VTT). Segments are enumerated by the worker; the server verifies these exist.
    pub fn required_outputs(&self) -> Vec<String> {
        let mut out = Vec::new();
        for r in &self.rungs {
            out.push(r.playlist.clone());
            out.push(r.init_segment.clone());
        }
        for a in &self.audio {
            out.push(a.playlist.clone());
            out.push(a.init_segment.clone());
        }
        for s in &self.subtitles {
            out.push(s.vtt_path.clone());
            if s.keep_raw {
                out.push(s.raw_path.clone());
            }
        }
        for f in &self.fonts {
            out.push(f.path.clone());
        }
        out.push(self.sprite.vtt_path.clone());
        for n in 0..self.sprite.sheets {
            out.push(self.sprite.sheet_path(n + 1));
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProxyAudio {
    pub source_ordinal: u32,
    pub bitrate_kbps: u32,
}

/// The 480p review proxy: one fast single-file MP4 (CIN-ING-002).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProxyPlan {
    pub source_ordinal: u32,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub deinterlace: bool,
    pub tonemap: Option<TonemapSource>,
    pub crf: u32,
    pub maxrate_kbps: u32,
    pub preset: String,
    pub audio: Option<ProxyAudio>,
    pub duration_secs: f64,
    /// Relative output path of the MP4 (faststart so it plays while it uploads).
    pub output: String,
}

// ---- geometry -----------------------------------------------------------------------------

/// Nearest even number, at least 2.
fn even(x: f64) -> u32 {
    let v = (x / 2.0).round() * 2.0;
    if v < 2.0 { 2 } else { v as u32 }
}

/// Fit `(dw, dh)` into the box without ever upscaling; even dimensions.
pub fn fit_within(dw: u32, dh: u32, box_w: u32, box_h: u32) -> (u32, u32) {
    let f = (f64::from(box_w) / f64::from(dw.max(1)))
        .min(f64::from(box_h) / f64::from(dh.max(1)))
        .min(1.0);
    (even(f64::from(dw) * f), even(f64::from(dh) * f))
}

/// H.264 level table: (label, level_idc, MaxMBPS, MaxFS).
const LEVELS: &[(&str, u8, u32, u32)] = &[
    ("3.0", 30, 40_500, 1_620),
    ("3.1", 31, 108_000, 3_600),
    ("3.2", 32, 216_000, 5_120),
    ("4.0", 40, 245_760, 8_192),
    ("4.2", 42, 522_240, 8_704),
    ("5.1", 51, 983_040, 36_864),
];

/// Smallest H.264 level that fits `w`x`h` at `fps`, and its RFC 6381 string (High profile).
pub fn h264_level(w: u32, h: u32, fps: f64) -> (String, String) {
    let fs = w.div_ceil(16) * h.div_ceil(16);
    let mbps = (f64::from(fs) * fps).ceil() as u32;
    let (label, idc, _, _) = LEVELS
        .iter()
        .find(|(_, _, max_mbps, max_fs)| mbps <= *max_mbps && fs <= *max_fs)
        .copied()
        .unwrap_or(LEVELS[LEVELS.len() - 1]);
    (label.to_string(), format!("avc1.6400{idc:02X}"))
}

/// Constant output frame rate: snaps to the standard fractions, caps at `max_fps` by halving.
pub fn output_fps(source: Option<f64>, max_fps: u32) -> (u32, u32) {
    let Some(mut fps) = source else {
        return (24, 1);
    };
    while fps > f64::from(max_fps) && fps > 1.0 {
        fps /= 2.0;
    }
    let near = |a: f64, b: f64| (a - b).abs() < 0.02;
    for (x, n, d) in [
        (23.976, 24_000, 1001),
        (29.97, 30_000, 1001),
        (59.94, 60_000, 1001),
    ] {
        if near(fps, x) {
            return (n, d);
        }
    }
    (fps.round().max(1.0) as u32, 1)
}

// ---- audio / subtitle helpers -------------------------------------------------------------

/// ISO 639-2 to (name, BCP 47 tag) for the languages that matter most.
fn language_info(code: &str) -> Option<(&'static str, &'static str)> {
    Some(match code {
        "eng" | "en" => ("English", "en"),
        "jpn" | "ja" => ("Japanese", "ja"),
        "spa" | "es" => ("Spanish", "es"),
        "fre" | "fra" | "fr" => ("French", "fr"),
        "ger" | "deu" | "de" => ("German", "de"),
        "ita" | "it" => ("Italian", "it"),
        "por" | "pt" => ("Portuguese", "pt"),
        "rus" | "ru" => ("Russian", "ru"),
        "kor" | "ko" => ("Korean", "ko"),
        "chi" | "zho" | "zh" => ("Chinese", "zh"),
        "ara" | "ar" => ("Arabic", "ar"),
        "hin" | "hi" => ("Hindi", "hi"),
        "dut" | "nld" | "nl" => ("Dutch", "nl"),
        "swe" | "sv" => ("Swedish", "sv"),
        "pol" | "pl" => ("Polish", "pl"),
        "tur" | "tr" => ("Turkish", "tr"),
        "tha" | "th" => ("Thai", "th"),
        "vie" | "vi" => ("Vietnamese", "vi"),
        _ => return None,
    })
}

fn language_label(code: Option<&str>) -> String {
    match code {
        None => "Unknown".to_string(),
        Some(c) => language_info(c)
            .map(|(n, _)| n.to_string())
            .unwrap_or_else(|| c.to_ascii_uppercase()),
    }
}

fn language_tag(code: Option<&str>) -> Option<String> {
    let c = code?;
    Some(language_info(c).map_or_else(|| c.to_string(), |(_, t)| t.to_string()))
}

fn lang_slug(code: Option<&str>) -> String {
    code.unwrap_or("und").to_string()
}

fn select_audio(probe: &ProbeResult, s: &TranscodeSettings) -> (Vec<AudioOutput>, Vec<String>) {
    let mut warnings = Vec::new();
    if probe.audio.is_empty() {
        warnings.push("no audio streams: the rendition will be silent".to_string());
        return (Vec::new(), warnings);
    }
    let any_non_commentary = probe.audio.iter().any(|a| !a.is_commentary);
    // Group by (language, commentary), keep the best track of each group.
    let mut groups: Vec<(Option<&str>, bool, &AudioStream)> = Vec::new();
    for a in &probe.audio {
        if a.is_commentary && any_non_commentary && !s.keep_commentary {
            continue;
        }
        let lang = a.language.as_deref();
        match groups
            .iter_mut()
            .find(|(l, c, _)| *l == lang && *c == a.is_commentary)
        {
            Some((_, _, best)) => {
                let better = (a.channels, a.is_default, std::cmp::Reverse(a.index))
                    > (
                        best.channels,
                        best.is_default,
                        std::cmp::Reverse(best.index),
                    );
                if better {
                    *best = a;
                }
            }
            None => groups.push((lang, a.is_commentary, a)),
        }
    }
    // Preferred languages first (stable), commentary last.
    let rank = |lang: Option<&str>, commentary: bool| {
        let pref = lang
            .and_then(|l| s.preferred_languages.iter().position(|p| p == l))
            .unwrap_or(usize::MAX);
        (commentary, pref)
    };
    groups.sort_by_key(|(l, c, _)| rank(*l, *c));
    if groups.len() > s.max_audio_tracks {
        warnings.push(format!(
            "{} audio tracks dropped (limit {})",
            groups.len() - s.max_audio_tracks,
            s.max_audio_tracks
        ));
        groups.truncate(s.max_audio_tracks);
    }
    let default_idx = groups
        .iter()
        .position(|(l, c, _)| {
            !*c && l.is_some_and(|l| s.preferred_languages.iter().any(|p| p == l))
        })
        .or_else(|| groups.iter().position(|(_, c, a)| !*c && a.is_default))
        .or_else(|| groups.iter().position(|(_, c, _)| !*c))
        .unwrap_or(0);

    let mut out = Vec::new();
    for (i, (lang, commentary, a)) in groups.iter().enumerate() {
        let mut layouts = vec![(AudioLayout::Stereo, 2u32, s.audio_stereo_kbps)];
        if s.keep_surround && a.channels >= 6 && !*commentary {
            layouts.push((AudioLayout::Surround51, 6, s.audio_surround_kbps));
        }
        for (layout, channels, kbps) in layouts {
            let suffix = match layout {
                AudioLayout::Stereo => "stereo",
                AudioLayout::Surround51 => "51",
            };
            let dir = format!("a/{:02}-{}-{suffix}", out.len(), lang_slug(*lang));
            let mut label = language_label(*lang);
            if *commentary {
                label.push_str(" (Commentary)");
            }
            if layout == AudioLayout::Surround51 {
                label.push_str(" 5.1");
            }
            out.push(AudioOutput {
                source_ordinal: a.ordinal,
                language: lang.map(str::to_string),
                language_tag: language_tag(*lang),
                label,
                layout,
                channels,
                bitrate_kbps: kbps,
                // Only the stereo rung of the chosen group is the default.
                is_default: i == default_idx && layout == AudioLayout::Stereo,
                commentary: *commentary,
                playlist: format!("{dir}/index.m3u8"),
                init_segment: format!("{dir}/init.mp4"),
                segment_pattern: format!("{dir}/seg_%05d.m4s"),
                dir,
            });
        }
    }
    (out, warnings)
}

fn select_subtitles(
    probe: &ProbeResult,
    s: &TranscodeSettings,
) -> (Vec<SubtitleOutput>, Vec<SkippedSubtitle>) {
    let mut outputs: Vec<SubtitleOutput> = Vec::new();
    let mut skipped = Vec::new();
    for sub in &probe.subtitles {
        let skip = |reason| SkippedSubtitle {
            source_ordinal: sub.ordinal,
            language: sub.language.clone(),
            codec: sub.codec_name.clone(),
            reason,
        };
        if sub.format.is_image() {
            skipped.push(skip(SkipReason::ImageBasedOcrLater));
            continue;
        }
        if !sub.format.is_text() {
            skipped.push(skip(SkipReason::UnsupportedCodec));
            continue;
        }
        if outputs.len() >= s.max_subtitles {
            skipped.push(skip(SkipReason::OverLimit));
            continue;
        }
        outputs.push(subtitle_output(sub, outputs.len(), s));
    }
    // One default at most: prefer a preferred-language non-forced track.
    let default = outputs.iter().position(|o| {
        !o.forced
            && o.language
                .as_deref()
                .is_some_and(|l| s.preferred_languages.iter().any(|p| p == l))
            && !o.hearing_impaired
    });
    // `is_default` marks the track the player may offer first; it is not auto-enabled.
    for (i, o) in outputs.iter_mut().enumerate() {
        o.is_default = Some(i) == default;
    }
    (outputs, skipped)
}

fn subtitle_output(sub: &SubtitleStream, n: usize, _s: &TranscodeSettings) -> SubtitleOutput {
    let is_ass = sub.format == SubtitleFormat::Ass;
    let slug = lang_slug(sub.language.as_deref());
    let base = format!("subs/{n:02}-{slug}");
    let mut label = language_label(sub.language.as_deref());
    if sub.is_forced {
        label.push_str(" (Forced)");
    }
    if sub.is_hearing_impaired {
        label.push_str(" (SDH)");
    }
    SubtitleOutput {
        source_ordinal: sub.ordinal,
        language: sub.language.clone(),
        language_tag: language_tag(sub.language.as_deref()),
        label,
        forced: sub.is_forced,
        hearing_impaired: sub.is_hearing_impaired,
        is_default: false,
        extract: if is_ass {
            ExtractFormat::Ass
        } else {
            ExtractFormat::Srt
        },
        raw_path: format!("{base}.{}", if is_ass { "ass" } else { "srt" }),
        keep_raw: is_ass,
        vtt_path: format!("{base}.vtt"),
    }
}

fn select_fonts(
    probe: &ProbeResult,
    subs: &[SubtitleOutput],
    s: &TranscodeSettings,
) -> Vec<FontOutput> {
    if !subs.iter().any(|o| o.extract == ExtractFormat::Ass) {
        return Vec::new();
    }
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for a in probe
        .attachments
        .iter()
        .filter(|a| AttachmentStream::is_font(a))
    {
        if out.len() >= s.max_fonts {
            break;
        }
        let name = a.filename.to_ascii_lowercase();
        if seen.contains(&name) {
            continue;
        }
        seen.push(name);
        out.push(FontOutput {
            attachment_ordinal: a.ordinal,
            path: format!("fonts/{}", a.filename),
        });
    }
    out
}

/// A thumbnail is only planned at `t` when the source runs at least this long past `t`. The container's
/// duration includes the audio, which AAC encoder priming makes a few tens of milliseconds longer than the
/// video, and ffmpeg's `fps` filter makes no frame for a time the video never reaches: a 20 s source probed
/// as 20.021 s gets thumbnails at 0 s and 10 s, not a third one at 20 s that would point at an empty tile.
pub const SPRITE_TAIL_SECS: f64 = 0.5;

/// Thumbnails at 0, `interval`, 2 `interval`, ... up to the end of the source (less [`SPRITE_TAIL_SECS`]),
/// at least one. The sprite step (`sr-transcoder` `sprite_step`, `fps=1/interval:eof_action=pass`) makes
/// at least this many frames: one per started interval of the video.
fn plan_sprite(duration: f64, aspect: f64, s: &TranscodeSettings) -> SpritePlan {
    let per_sheet = (s.sprite_columns * s.sprite_rows).max(1);
    let interval = s.sprite_interval_secs.max(1);
    let total =
        (((duration - SPRITE_TAIL_SECS).max(0.0) / f64::from(interval)).ceil() as u32).max(1);
    SpritePlan {
        interval_secs: interval,
        thumb_width: s.sprite_thumb_width,
        thumb_height: even(f64::from(s.sprite_thumb_width) / aspect.max(0.1)),
        columns: s.sprite_columns,
        rows: s.sprite_rows,
        thumbs_total: total,
        sheets: total.div_ceil(per_sheet),
        sheet_pattern: "sprites/sheet-%03d.jpg".to_string(),
        vtt_path: "sprites/sprites.vtt".to_string(),
    }
}

fn plan_posters(duration: f64, s: &TranscodeSettings) -> Vec<PosterPlan> {
    let n = s.poster_candidates.max(1);
    if duration < 10.0 {
        return vec![PosterPlan {
            at_secs: duration * 0.5,
            path: "posters/candidate-1.jpg".to_string(),
        }];
    }
    // Spread over 10%..70% of the runtime, away from opening titles and credits.
    (0..n)
        .map(|i| {
            let frac = if n == 1 {
                0.3
            } else {
                0.1 + 0.6 * f64::from(i) / f64::from(n - 1)
            };
            PosterPlan {
                at_secs: (duration * frac * 10.0).round() / 10.0,
                path: format!("posters/candidate-{}.jpg", i + 1),
            }
        })
        .collect()
}

fn check_settings(s: &TranscodeSettings) -> Result<(), PlanError> {
    if s.max_height == 0 || s.max_height > 1080 {
        return Err(PlanError::BadSettings("max_height must be 1..=1080"));
    }
    if s.segment_secs == 0 || s.gop_secs == 0 || !s.segment_secs.is_multiple_of(s.gop_secs) {
        return Err(PlanError::BadSettings(
            "segment_secs must be a multiple of gop_secs",
        ));
    }
    if s.rungs.is_empty() {
        return Err(PlanError::BadSettings("at least one rung"));
    }
    Ok(())
}

fn source_summary(v: &VideoStream, duration: f64) -> SourceSummary {
    let (dw, dh) = v.display_size();
    SourceSummary {
        duration_secs: duration,
        width: v.width,
        height: v.height,
        display_width: dw,
        display_height: dh,
        video_codec: v.codec_name.clone(),
        hdr: v.hdr,
        interlaced: v.is_interlaced(),
    }
}

fn base_checks<'a>(
    probe: &'a ProbeResult,
    s: &TranscodeSettings,
) -> Result<(&'a VideoStream, f64), PlanError> {
    check_settings(s)?;
    let v = probe.primary_video().ok_or(PlanError::NoVideo)?;
    let duration = probe
        .duration_secs()
        .filter(|d| *d > 0.0)
        .ok_or(PlanError::NoDuration)?;
    if duration > s.max_duration_secs {
        return Err(PlanError::TooLong(duration));
    }
    Ok((v, duration))
}

/// Plan the full ladder job (CIN-ING-003).
pub fn plan_ladder_job(
    probe: &ProbeResult,
    settings: &TranscodeSettings,
) -> Result<JobPlan, PlanError> {
    let (v, duration) = base_checks(probe, settings)?;
    let mut warnings = probe.warnings.clone();
    let (dw, dh) = v.display_size();
    let (fps_num, fps_den) = output_fps(v.frame_rate, settings.max_fps);
    if v.frame_rate.is_none() {
        warnings.push("unknown frame rate: assuming 24 fps".to_string());
    }
    let fps = f64::from(fps_num) / f64::from(fps_den);
    let gop_frames = (fps * f64::from(settings.gop_secs)).round().max(1.0) as u32;

    let mut rungs: Vec<RungPlan> = Vec::new();
    let mut templates = settings.rungs.clone();
    templates.sort_by_key(|t| std::cmp::Reverse(t.box_height));
    for t in &templates {
        if t.box_height > settings.max_height {
            continue;
        }
        if t.box_height > dh && t.box_width > dw {
            continue;
        }
        push_rung(&mut rungs, t, dw, dh, fps);
    }
    if rungs.is_empty() {
        // Source smaller than every rung: one native-size rung, never upscaled.
        if let Some(t) = templates.last() {
            push_rung(&mut rungs, t, dw, dh, fps);
        }
    }
    let tonemap = v.hdr.tonemap_source();
    if tonemap == Some(TonemapSource::DolbyVisionNoFallback) {
        warnings.push(
            "Dolby Vision profile 5 has no HDR10/SDR base layer: needs a libplacebo ffmpeg"
                .to_string(),
        );
    }
    let (audio, audio_warnings) = select_audio(probe, settings);
    warnings.extend(audio_warnings);
    let (subtitles, skipped_subtitles) = select_subtitles(probe, settings);
    let fonts = select_fonts(probe, &subtitles, settings);
    let aspect = f64::from(dw) / f64::from(dh.max(1));

    Ok(JobPlan {
        profile_version: PROFILE_VERSION,
        source: source_summary(v, duration),
        video: VideoPlan {
            source_ordinal: v.ordinal,
            fps_num,
            fps_den,
            gop_frames,
            deinterlace: v.is_interlaced(),
            tonemap,
        },
        rungs,
        audio,
        subtitles,
        skipped_subtitles,
        fonts,
        sprite: plan_sprite(duration, aspect, settings),
        posters: plan_posters(duration, settings),
        hls: HlsSettings {
            segment_secs: settings.segment_secs,
            gop_secs: settings.gop_secs,
            fmp4: true,
            closed_gop: true,
        },
        warnings,
    })
}

fn push_rung(rungs: &mut Vec<RungPlan>, t: &RungTemplate, dw: u32, dh: u32, fps: f64) {
    let (w, h) = fit_within(dw, dh, t.box_width, t.box_height);
    if rungs.iter().any(|r| r.width == w && r.height == h) {
        return;
    }
    let box_px = f64::from(t.box_width) * f64::from(t.box_height);
    let ratio = (f64::from(w) * f64::from(h) / box_px).clamp(0.2, 1.0);
    let kbps = ((f64::from(t.video_kbps) * ratio / 10.0).round() as u32 * 10).max(250);
    let (level, codecs) = h264_level(w, h, fps);
    let dir = format!("v/{}", t.name);
    rungs.push(RungPlan {
        name: t.name.clone(),
        width: w,
        height: h,
        video_kbps: kbps,
        maxrate_kbps: kbps * 3 / 2,
        bufsize_kbps: kbps * 3,
        level,
        codecs,
        playlist: format!("{dir}/index.m3u8"),
        init_segment: format!("{dir}/init.mp4"),
        segment_pattern: format!("{dir}/seg_%05d.m4s"),
        dir,
    });
}

/// Plan the 480p review proxy (CIN-ING-002).
pub fn plan_proxy_job(
    probe: &ProbeResult,
    settings: &TranscodeSettings,
) -> Result<ProxyPlan, PlanError> {
    let (v, duration) = base_checks(probe, settings)?;
    let (dw, dh) = v.display_size();
    let h = settings.proxy_height.min(dh).max(2);
    let box_w = even(f64::from(h) * 16.0 / 9.0).max(2);
    let (width, height) = fit_within(dw, dh, box_w, h);
    let fps = v.frame_rate.map_or(24, |f| (f.round() as u32).clamp(1, 30));
    let (audio, _) = select_audio(probe, settings);
    let audio = audio
        .iter()
        .find(|a| a.is_default && a.layout == AudioLayout::Stereo)
        .or_else(|| audio.first())
        .map(|a| ProxyAudio {
            source_ordinal: a.source_ordinal,
            bitrate_kbps: 96,
        });
    Ok(ProxyPlan {
        source_ordinal: v.ordinal,
        width,
        height,
        fps,
        deinterlace: v.is_interlaced(),
        tonemap: v.hdr.tonemap_source(),
        crf: 30,
        maxrate_kbps: settings.proxy_kbps,
        preset: "veryfast".to_string(),
        audio,
        duration_secs: duration,
        output: "proxy/proxy.mp4".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::*;
    use crate::probe::parse_ffprobe_json;

    fn probe(json: &str) -> ProbeResult {
        parse_ffprobe_json(json.as_bytes()).unwrap()
    }
    fn plan(json: &str) -> JobPlan {
        plan_ladder_job(&probe(json), &TranscodeSettings::default()).unwrap()
    }

    #[test]
    fn full_hd_sdr_gets_three_rungs() {
        let p = plan(H264_SDR);
        let names: Vec<_> = p.rungs.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["1080p", "720p", "480p"]);
        assert_eq!((p.rungs[0].width, p.rungs[0].height), (1920, 1080));
        assert_eq!((p.rungs[1].width, p.rungs[1].height), (1280, 720));
        assert_eq!((p.rungs[2].width, p.rungs[2].height), (854, 480));
        assert_eq!(p.rungs[0].video_kbps, 6000);
        assert_eq!(p.rungs[0].level, "4.0");
        assert_eq!(p.rungs[0].codecs, "avc1.640028");
        assert_eq!(p.rungs[1].codecs, "avc1.64001F");
        assert_eq!(p.rungs[2].codecs, "avc1.64001E");
        assert!(p.video.tonemap.is_none());
        assert!(!p.video.deinterlace);
    }

    #[test]
    fn four_k_hdr_becomes_1080p_sdr_ladder_never_above_1080() {
        let p = plan(HEVC_HDR10);
        assert!(p.rungs.iter().all(|r| r.height <= 1080 && r.width <= 1920));
        assert_eq!(p.rungs[0].height, 1080);
        assert_eq!(p.video.tonemap, Some(TonemapSource::Pq));
        assert_eq!(p.source.height, 2160);
    }

    #[test]
    fn dolby_vision_p5_warns() {
        let p = plan(DV_P5);
        assert_eq!(p.video.tonemap, Some(TonemapSource::DolbyVisionNoFallback));
        assert!(p.warnings.iter().any(|w| w.contains("profile 5")));
        let p = plan(DV_P81);
        assert_eq!(p.video.tonemap, Some(TonemapSource::Pq));
    }

    #[test]
    fn rungs_never_exceed_source() {
        // 720p source: no 1080p rung.
        let json = H264_SDR
            .replace("\"width\":1920", "\"width\":1280")
            .replace("\"height\":1080", "\"height\":720");
        let p = plan(&json);
        let names: Vec<_> = p.rungs.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["720p", "480p"]);
        // 360p source: a single native rung, not upscaled.
        let json = H264_SDR
            .replace("\"width\":1920", "\"width\":640")
            .replace("\"height\":1080", "\"height\":360");
        let p = plan(&json);
        assert_eq!(p.rungs.len(), 1);
        assert_eq!((p.rungs[0].width, p.rungs[0].height), (640, 360));
        assert!(p.rungs[0].video_kbps < 1500);
    }

    #[test]
    fn scope_aspect_keeps_full_width_rung() {
        let json = H264_SDR.replace("\"height\":1080", "\"height\":800");
        let p = plan(&json);
        assert_eq!(p.rungs.len(), 3);
        assert_eq!((p.rungs[0].width, p.rungs[0].height), (1920, 800));
        assert!(p.rungs[0].video_kbps < 6000);
        assert_eq!(p.rungs[1].width, 1280);
    }

    #[test]
    fn interlaced_dvd_is_deinterlaced_with_sar_applied() {
        let p = plan(INTERLACED_DVD);
        assert!(p.video.deinterlace);
        assert!(p.source.interlaced);
        assert_eq!(p.rungs.len(), 1);
        assert_eq!(p.rungs[0].name, "480p");
        assert_eq!((p.rungs[0].width, p.rungs[0].height), (854, 480));
        assert_eq!((p.video.fps_num, p.video.fps_den), (30_000, 1001));
    }

    #[test]
    fn closed_aligned_gop() {
        let p = plan(H264_SDR);
        assert_eq!((p.video.fps_num, p.video.fps_den), (24_000, 1001));
        assert_eq!(p.video.gop_frames, 48);
        assert_eq!(p.hls.segment_secs, 6);
        assert_eq!(p.hls.gop_secs, 2);
        assert!(p.hls.closed_gop && p.hls.fmp4);
        let json = H264_SDR.replace("24000/1001", "60/1");
        let p = plan(&json);
        assert_eq!(p.video.gop_frames, 120);
        assert_eq!(p.rungs[0].level, "4.2");
        // 120 fps is halved to the 60 fps cap.
        assert_eq!(output_fps(Some(120.0), 60), (60, 1));
        assert_eq!(output_fps(Some(59.94), 60), (60_000, 1001));
        assert_eq!(output_fps(None, 60), (24, 1));
    }

    #[test]
    fn audio_one_stereo_track_per_language() {
        let p = plan(MULTI_AUDIO);
        let got: Vec<_> = p
            .audio
            .iter()
            .map(|a| (a.language.as_deref(), a.source_ordinal, a.is_default))
            .collect();
        // English (TrueHD 7.1, ordinal 0) is default; commentary dropped; untagged kept last.
        assert_eq!(
            got,
            [
                (Some("eng"), 0, true),
                (Some("jpn"), 1, false),
                (None, 3, false)
            ]
        );
        assert!(
            p.audio.iter().all(|a| a.layout == AudioLayout::Stereo
                && a.channels == 2
                && a.bitrate_kbps == 160)
        );
        assert_eq!(p.audio[0].label, "English");
        assert_eq!(p.audio[0].language_tag.as_deref(), Some("en"));
        assert_eq!(p.audio[2].label, "Unknown");
        assert_eq!(p.audio[0].dir, "a/00-eng-stereo");
    }

    #[test]
    fn surround_rung_is_optional() {
        let mut s = TranscodeSettings {
            keep_surround: true,
            ..TranscodeSettings::default()
        };
        let p = plan_ladder_job(&probe(MULTI_AUDIO), &s).unwrap();
        let eng: Vec<_> = p
            .audio
            .iter()
            .filter(|a| a.language.as_deref() == Some("eng"))
            .collect();
        assert_eq!(eng.len(), 2);
        assert_eq!(eng[1].layout, AudioLayout::Surround51);
        assert_eq!(eng[1].channels, 6);
        assert_eq!(eng[1].bitrate_kbps, 384);
        assert!(!eng[1].is_default);
        assert_eq!(eng[1].label, "English 5.1");
        // The stereo-only Japanese source gets no surround rung.
        assert_eq!(
            p.audio
                .iter()
                .filter(|a| a.language.as_deref() == Some("jpn"))
                .count(),
            1
        );
        s.keep_commentary = true;
        let p = plan_ladder_job(&probe(MULTI_AUDIO), &s).unwrap();
        assert!(
            p.audio
                .iter()
                .any(|a| a.commentary && a.label.contains("Commentary"))
        );
    }

    #[test]
    fn audio_track_limit() {
        let s = TranscodeSettings {
            max_audio_tracks: 1,
            ..TranscodeSettings::default()
        };
        let p = plan_ladder_job(&probe(MULTI_AUDIO), &s).unwrap();
        assert_eq!(p.audio.len(), 1);
        assert!(p.warnings.iter().any(|w| w.contains("dropped")));
    }

    #[test]
    fn subtitles_text_extracted_image_flagged() {
        let p = plan(SUBS_AND_FONTS);
        assert_eq!(p.subtitles.len(), 3);
        let ass = &p.subtitles[0];
        assert_eq!(ass.extract, ExtractFormat::Ass);
        assert!(ass.keep_raw);
        assert_eq!(ass.raw_path, "subs/00-eng.ass");
        assert_eq!(ass.vtt_path, "subs/00-eng.vtt");
        assert!(ass.is_default);
        let srt = &p.subtitles[1];
        assert_eq!(srt.extract, ExtractFormat::Srt);
        assert!(!srt.keep_raw);
        assert_eq!(srt.label, "Spanish (Forced)");
        assert_eq!(p.subtitles[2].label, "English (SDH)");
        assert_eq!(p.skipped_subtitles.len(), 2);
        assert!(
            p.skipped_subtitles
                .iter()
                .all(|s| s.reason == SkipReason::ImageBasedOcrLater)
        );
        assert_eq!(p.skipped_subtitles[0].codec, "hdmv_pgs_subtitle");
    }

    #[test]
    fn fonts_only_with_ass() {
        let p = plan(SUBS_AND_FONTS);
        assert_eq!(p.fonts.len(), 1);
        assert_eq!(p.fonts[0].path, "fonts/Arial-Bold.ttf");
        // Drop the ASS stream: fonts are not extracted.
        let probe_no_ass = {
            let mut pr = probe(SUBS_AND_FONTS);
            pr.subtitles.retain(|s| s.format != SubtitleFormat::Ass);
            pr
        };
        let p = plan_ladder_job(&probe_no_ass, &TranscodeSettings::default()).unwrap();
        assert!(p.fonts.is_empty());
    }

    #[test]
    fn sprite_thumbnails_stop_where_the_video_does() {
        let s = TranscodeSettings::default();
        let total = |d: f64| plan_sprite(d, 16.0 / 9.0, &s).thumbs_total;
        // The media.yml source: 20 s of video, the container says 20.021 s (AAC priming). ffmpeg makes
        // frames at 0 s and 10 s only; a third thumbnail pointed at an empty tile (CI run of f1e89b77).
        assert_eq!(total(20.021), 2);
        assert_eq!(total(20.0), 2);
        // The 4 s clips of the resilience stage still get their one thumbnail (and so one sheet).
        assert_eq!(total(4.021), 1);
        assert_eq!(total(0.2), 1);
        assert_eq!(plan_sprite(4.021, 16.0 / 9.0, &s).sheets, 1);
        // A real frame past a boundary is still planned.
        assert_eq!(total(20.6), 3);
        assert_eq!(total(25.0), 3);
        // 1001 thumbnails need 11 sheets of 100.
        assert_eq!(total(10_001.0), 1001);
        assert_eq!(plan_sprite(10_001.0, 16.0 / 9.0, &s).sheets, 11);
    }

    #[test]
    fn sprite_and_posters() {
        let p = plan(H264_SDR); // 5432.123 s
        assert_eq!(p.sprite.thumbs_total, 544);
        assert_eq!(p.sprite.sheets, 6);
        assert_eq!(p.sprite.thumb_width, 160);
        assert_eq!(p.sprite.thumb_height, 90);
        assert_eq!(p.sprite.sheet_path(6), "sprites/sheet-006.jpg");
        assert_eq!(p.posters.len(), 5);
        assert!((p.posters[0].at_secs - 543.2).abs() < 0.2);
        assert!(p.posters.windows(2).all(|w| w[0].at_secs < w[1].at_secs));
    }

    #[test]
    fn required_outputs_cover_everything() {
        let p = plan(SUBS_AND_FONTS);
        let req = p.required_outputs();
        assert!(req.contains(&"v/1080p/index.m3u8".to_string()));
        assert!(req.contains(&"a/00-jpn-stereo/init.mp4".to_string()));
        assert!(req.contains(&"subs/00-eng.ass".to_string()));
        assert!(!req.contains(&"subs/01-spa.srt".to_string()));
        assert!(req.contains(&"sprites/sprites.vtt".to_string()));
        assert!(req.contains(&"sprites/sheet-001.jpg".to_string()));
    }

    #[test]
    fn plan_round_trips_through_json() {
        let p = plan(SUBS_AND_FONTS);
        let json = serde_json::to_string(&p).unwrap();
        let back: JobPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn errors() {
        let s = TranscodeSettings::default();
        let mut pr = probe(H264_SDR);
        pr.video.clear();
        assert_eq!(plan_ladder_job(&pr, &s), Err(PlanError::NoVideo));
        let mut pr = probe(H264_SDR);
        pr.format.duration_secs = None;
        assert_eq!(plan_ladder_job(&pr, &s), Err(PlanError::NoDuration));
        let mut pr = probe(H264_SDR);
        pr.format.duration_secs = Some(100.0 * 3600.0);
        assert!(matches!(
            plan_ladder_job(&pr, &s),
            Err(PlanError::TooLong(_))
        ));
        let bad = TranscodeSettings {
            max_height: 2160,
            ..TranscodeSettings::default()
        };
        assert!(matches!(
            plan_ladder_job(&probe(H264_SDR), &bad),
            Err(PlanError::BadSettings(_))
        ));
        let bad = TranscodeSettings {
            segment_secs: 5,
            ..TranscodeSettings::default()
        };
        assert!(plan_ladder_job(&probe(H264_SDR), &bad).is_err());
    }

    #[test]
    fn no_audio_warns() {
        let json = r#"{"streams":[{"index":0,"codec_type":"video","codec_name":"h264",
            "width":1920,"height":1080,"r_frame_rate":"24/1","avg_frame_rate":"24/1"}],
            "format":{"duration":"60"}}"#;
        let p = plan(json);
        assert!(p.audio.is_empty());
        assert!(p.warnings.iter().any(|w| w.contains("silent")));
    }

    #[test]
    fn proxy_plan_480p_fast() {
        let p = plan_proxy_job(&probe(HEVC_HDR10), &TranscodeSettings::default()).unwrap();
        assert_eq!((p.width, p.height), (854, 480));
        assert_eq!(p.tonemap, Some(TonemapSource::Pq));
        assert_eq!(p.preset, "veryfast");
        assert_eq!(p.fps, 24);
        assert_eq!(p.audio.as_ref().unwrap().source_ordinal, 0);
        assert_eq!(p.output, "proxy/proxy.mp4");
        // Tiny source is not upscaled.
        let json = H264_SDR
            .replace("\"width\":1920", "\"width\":426")
            .replace("\"height\":1080", "\"height\":240");
        let p = plan_proxy_job(&probe(&json), &TranscodeSettings::default()).unwrap();
        assert_eq!((p.width, p.height), (426, 240));
    }

    #[test]
    fn level_table() {
        assert_eq!(h264_level(854, 480, 24.0).0, "3.0");
        assert_eq!(h264_level(1280, 720, 24.0).0, "3.1");
        assert_eq!(h264_level(1920, 1080, 24.0).0, "4.0");
        assert_eq!(h264_level(1920, 1080, 60.0).0, "4.2");
    }
}
