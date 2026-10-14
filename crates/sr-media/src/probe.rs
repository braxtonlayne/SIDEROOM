//! Probe service (CIN-ING-001): `ffprobe -print_format json` into typed structs.
//!
//! The JSON is untrusted (a hostile file controls every tag). It is size-capped before
//! parsing, the number of streams is bounded, strings are truncated and sanitised, and
//! numbers are read leniently (ffprobe prints many of them as JSON strings).

use crate::runner::{CommandRunner, CommandSpec, RunError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use thiserror::Error;

/// Largest ffprobe JSON document accepted.
pub const MAX_PROBE_JSON_BYTES: usize = 4 * 1024 * 1024;
/// Most streams a file may declare before it is rejected.
pub const MAX_STREAMS: usize = 256;
/// Longest tag/title string kept.
pub const MAX_TEXT_LEN: usize = 256;
/// Longest chapter list kept.
pub const MAX_CHAPTERS: usize = 1000;
const MAX_DIMENSION: u64 = 16_384;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProbeError {
    #[error("ffprobe output is larger than {MAX_PROBE_JSON_BYTES} bytes")]
    TooLarge,
    #[error("ffprobe output is not valid JSON: {0}")]
    BadJson(String),
    #[error("file declares {0} streams (limit {MAX_STREAMS})")]
    TooManyStreams(usize),
    #[error("ffprobe could not read the file: {0}")]
    Unreadable(String),
    #[error("ffprobe timed out")]
    TimedOut,
    #[error("could not run ffprobe: {0}")]
    Run(String),
    #[error("unsafe input path")]
    UnsafePath,
}

impl From<RunError> for ProbeError {
    fn from(e: RunError) -> Self {
        Self::Run(e.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldOrder {
    Progressive,
    Interlaced,
    Unknown,
}

/// Dynamic range of a video stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HdrKind {
    Sdr,
    /// PQ (SMPTE ST 2084), static metadata.
    Hdr10,
    Hdr10Plus,
    /// Hybrid log-gamma.
    Hlg,
    DolbyVision {
        profile: u8,
        /// `dv_bl_signal_compatibility_id`: 0 none (profile 5), 1 HDR10, 2 SDR, 4 HLG.
        compat_id: Option<u8>,
    },
}

/// Transfer function the tone-mapper must undo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TonemapSource {
    Pq,
    Hlg,
    /// Dolby Vision without an HDR10/HLG/SDR base layer (profile 5, IPT-PQ-C2). The
    /// zscale chain cannot decode this; it needs a libplacebo build.
    DolbyVisionNoFallback,
}

impl HdrKind {
    pub fn is_hdr(&self) -> bool {
        !matches!(self, Self::Sdr)
    }

    /// What must be undone to reach SDR, or `None` when the stream already is SDR (including
    /// Dolby Vision profiles whose base layer is SDR).
    pub fn tonemap_source(&self) -> Option<TonemapSource> {
        match self {
            Self::Sdr => None,
            Self::Hdr10 | Self::Hdr10Plus => Some(TonemapSource::Pq),
            Self::Hlg => Some(TonemapSource::Hlg),
            Self::DolbyVision { compat_id, .. } => match compat_id {
                Some(2) => None,
                Some(4) => Some(TonemapSource::Hlg),
                Some(0) => Some(TonemapSource::DolbyVisionNoFallback),
                _ => Some(TonemapSource::Pq),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormatInfo {
    pub format_name: String,
    pub duration_secs: Option<f64>,
    pub size_bytes: Option<u64>,
    pub bit_rate: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoStream {
    /// Absolute stream index in the file (`-map 0:<index>`).
    pub index: u32,
    /// Position among video streams (`-map 0:v:<ordinal>`).
    pub ordinal: u32,
    pub codec_name: String,
    pub profile: Option<String>,
    pub width: u32,
    pub height: u32,
    /// Sample aspect ratio as (num, den); (1, 1) when absent.
    pub sar: (u32, u32),
    pub pix_fmt: Option<String>,
    pub bit_depth: Option<u8>,
    pub frame_rate: Option<f64>,
    pub field_order: FieldOrder,
    pub color_transfer: Option<String>,
    pub color_primaries: Option<String>,
    pub color_space: Option<String>,
    pub hdr: HdrKind,
    pub bit_rate: Option<u64>,
    pub is_default: bool,
    /// Cover art or a thumbnail stored as a video stream.
    pub is_attached_pic: bool,
    pub language: Option<String>,
}

impl VideoStream {
    pub fn is_interlaced(&self) -> bool {
        self.field_order == FieldOrder::Interlaced
    }

    /// Display size after applying the sample aspect ratio (anamorphic DVD material).
    pub fn display_size(&self) -> (u32, u32) {
        let (n, d) = self.sar;
        if n == 0 || d == 0 || n == d {
            return (self.width, self.height);
        }
        let w = (u64::from(self.width) * u64::from(n) / u64::from(d)).max(1);
        (u32::try_from(w).unwrap_or(self.width), self.height)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioStream {
    pub index: u32,
    /// Position among audio streams (`-map 0:a:<ordinal>`).
    pub ordinal: u32,
    pub codec_name: String,
    pub profile: Option<String>,
    pub channels: u32,
    pub channel_layout: Option<String>,
    pub sample_rate: Option<u32>,
    pub bit_rate: Option<u64>,
    pub language: Option<String>,
    pub title: Option<String>,
    pub is_default: bool,
    pub is_commentary: bool,
    pub is_visual_impaired: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "name", rename_all = "snake_case")]
pub enum SubtitleFormat {
    Srt,
    Ass,
    WebVtt,
    MovText,
    /// Blu-ray bitmap subtitles.
    Pgs,
    /// DVD bitmap subtitles.
    VobSub,
    DvbSub,
    Other(String),
}

impl SubtitleFormat {
    pub fn from_codec(name: &str) -> Self {
        match name {
            "subrip" | "srt" | "text" => Self::Srt,
            "ass" | "ssa" => Self::Ass,
            "webvtt" => Self::WebVtt,
            "mov_text" => Self::MovText,
            "hdmv_pgs_subtitle" | "pgssub" => Self::Pgs,
            "dvd_subtitle" | "dvdsub" => Self::VobSub,
            "dvb_subtitle" | "dvbsub" => Self::DvbSub,
            other => Self::Other(other.to_string()),
        }
    }
    /// Text formats can be converted to WebVTT; bitmap formats cannot without OCR.
    pub fn is_text(&self) -> bool {
        matches!(self, Self::Srt | Self::Ass | Self::WebVtt | Self::MovText)
    }
    pub fn is_image(&self) -> bool {
        matches!(self, Self::Pgs | Self::VobSub | Self::DvbSub)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubtitleStream {
    pub index: u32,
    /// Position among subtitle streams (`-map 0:s:<ordinal>`).
    pub ordinal: u32,
    pub codec_name: String,
    pub format: SubtitleFormat,
    pub language: Option<String>,
    pub title: Option<String>,
    pub is_default: bool,
    pub is_forced: bool,
    pub is_hearing_impaired: bool,
}

/// An attachment stream (usually a font embedded for ASS subtitles).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentStream {
    pub index: u32,
    /// Position among attachment streams (`-dump_attachment:t:<ordinal>`).
    pub ordinal: u32,
    /// Base name only; any directory part in the file is stripped.
    pub filename: String,
    pub mime_type: Option<String>,
}

impl AttachmentStream {
    pub fn is_font(&self) -> bool {
        let mime_font = self.mime_type.as_deref().is_some_and(|m| {
            m.starts_with("font/")
                || m.starts_with("application/x-font")
                || m == "application/x-truetype-font"
                || m == "application/vnd.ms-opentype"
                || m == "application/font-sfnt"
        });
        let lower = self.filename.to_ascii_lowercase();
        mime_font
            || [".ttf", ".otf", ".ttc", ".woff", ".woff2"]
                .iter()
                .any(|e| lower.ends_with(e))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chapter {
    pub start_secs: f64,
    pub end_secs: f64,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProbeResult {
    pub format: FormatInfo,
    pub video: Vec<VideoStream>,
    pub audio: Vec<AudioStream>,
    pub subtitles: Vec<SubtitleStream>,
    pub attachments: Vec<AttachmentStream>,
    pub chapters: Vec<Chapter>,
    /// Non-fatal oddities worth surfacing to reviewers.
    pub warnings: Vec<String>,
}

impl ProbeResult {
    /// The stream the ladder is built from: not cover art, default flag first, then pixels.
    pub fn primary_video(&self) -> Option<&VideoStream> {
        self.video
            .iter()
            .filter(|v| !v.is_attached_pic)
            .max_by_key(|v| {
                (
                    v.is_default,
                    u64::from(v.width) * u64::from(v.height),
                    std::cmp::Reverse(v.index),
                )
            })
    }
    pub fn duration_secs(&self) -> Option<f64> {
        self.format.duration_secs
    }
}

// ---- lenient JSON accessors ---------------------------------------------------------------

fn clean_text(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .take(MAX_TEXT_LEN)
        .collect::<String>()
        .trim()
        .to_string()
}

fn text(v: &Value, key: &str) -> Option<String> {
    let s = clean_text(v.get(key)?.as_str()?);
    (!s.is_empty()).then_some(s)
}

fn number(v: &Value, key: &str) -> Option<f64> {
    let n = match v.get(key)? {
        Value::Number(n) => n.as_f64()?,
        Value::String(s) => s.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    n.is_finite().then_some(n)
}

fn unsigned(v: &Value, key: &str) -> Option<u64> {
    match v.get(key)? {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.trim().parse::<u64>().ok(),
        _ => None,
    }
}

fn tag(v: &Value, name: &str) -> Option<String> {
    let tags = v.get("tags")?.as_object()?;
    tags.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .and_then(|(_, val)| val.as_str())
        .map(clean_text)
        .filter(|s| !s.is_empty())
}

fn disposition(v: &Value, name: &str) -> bool {
    v.get("disposition")
        .and_then(|d| d.get(name))
        .and_then(Value::as_i64)
        == Some(1)
}

/// ISO 639 code from the tags, lowercased; `und`/`unk`/`zxx` and junk become `None`.
fn language(v: &Value) -> Option<String> {
    let l = tag(v, "language")?.to_ascii_lowercase();
    let ok = l.len() <= 12 && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    (ok && !matches!(l.as_str(), "und" | "unk" | "zxx" | "mis" | "mul")).then_some(l)
}

fn parse_ratio(s: &str) -> Option<(u32, u32)> {
    let (a, b) = s.split_once([':', '/'])?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

fn frame_rate(v: &Value) -> Option<f64> {
    for key in ["avg_frame_rate", "r_frame_rate"] {
        if let Some((n, d)) = v.get(key).and_then(Value::as_str).and_then(parse_ratio)
            && d != 0
        {
            let fps = f64::from(n) / f64::from(d);
            if (1.0..=240.0).contains(&fps) {
                return Some(fps);
            }
        }
    }
    None
}

fn field_order(v: &Value) -> FieldOrder {
    match v.get("field_order").and_then(Value::as_str) {
        Some("progressive") => FieldOrder::Progressive,
        Some("tt" | "bb" | "tb" | "bt") => FieldOrder::Interlaced,
        _ => FieldOrder::Unknown,
    }
}

fn detect_hdr(v: &Value, transfer: Option<&str>) -> HdrKind {
    let mut dovi: Option<(u8, Option<u8>)> = None;
    let mut hdr10plus = false;
    if let Some(list) = v.get("side_data_list").and_then(Value::as_array) {
        for sd in list {
            let kind = sd
                .get("side_data_type")
                .and_then(Value::as_str)
                .unwrap_or("");
            if kind.contains("DOVI") {
                let profile = unsigned(sd, "dv_profile")
                    .and_then(|p| u8::try_from(p).ok())
                    .unwrap_or(0);
                let compat = unsigned(sd, "dv_bl_signal_compatibility_id")
                    .and_then(|p| u8::try_from(p).ok());
                dovi = Some((profile, compat));
            } else if kind.contains("2094-40") || kind.contains("HDR10+") {
                hdr10plus = true;
            }
        }
    }
    if let Some((profile, compat_id)) = dovi {
        return HdrKind::DolbyVision { profile, compat_id };
    }
    match transfer {
        Some("smpte2084") if hdr10plus => HdrKind::Hdr10Plus,
        Some("smpte2084") => HdrKind::Hdr10,
        Some("arib-std-b67") => HdrKind::Hlg,
        _ => HdrKind::Sdr,
    }
}

fn bit_depth(v: &Value, pix_fmt: Option<&str>) -> Option<u8> {
    if let Some(b) = unsigned(v, "bits_per_raw_sample").and_then(|b| u8::try_from(b).ok())
        && b > 0
    {
        return Some(b);
    }
    let p = pix_fmt?;
    if p.contains("p10") || p.contains("10le") || p.contains("10be") {
        Some(10)
    } else if p.contains("p12") || p.contains("12le") {
        Some(12)
    } else {
        Some(8)
    }
}

fn dimension(v: &Value, key: &str) -> Option<u32> {
    unsigned(v, key)
        .filter(|d| (1..=MAX_DIMENSION).contains(d))
        .and_then(|d| u32::try_from(d).ok())
}

/// Base name of an attachment filename: no directories, no control characters.
fn safe_filename(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = clean_text(base)
        .chars()
        .map(|c| {
            if matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches('.').to_string();
    if cleaned.is_empty() {
        "attachment".to_string()
    } else {
        cleaned
    }
}

/// Parse an `ffprobe -print_format json -show_format -show_streams -show_chapters` document.
pub fn parse_ffprobe_json(json: &[u8]) -> Result<ProbeResult, ProbeError> {
    if json.len() > MAX_PROBE_JSON_BYTES {
        return Err(ProbeError::TooLarge);
    }
    let root: Value =
        serde_json::from_slice(json).map_err(|e| ProbeError::BadJson(e.to_string()))?;
    if let Some(err) = root.get("error") {
        let msg = err
            .get("string")
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        return Err(ProbeError::Unreadable(clean_text(msg)));
    }
    let streams = root
        .get("streams")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    if streams.len() > MAX_STREAMS {
        return Err(ProbeError::TooManyStreams(streams.len()));
    }
    let fmt = root.get("format").cloned().unwrap_or(Value::Null);
    let mut result = ProbeResult {
        format: FormatInfo {
            format_name: text(&fmt, "format_name").unwrap_or_default(),
            duration_secs: number(&fmt, "duration").filter(|d| *d >= 0.0),
            size_bytes: unsigned(&fmt, "size"),
            bit_rate: unsigned(&fmt, "bit_rate"),
        },
        video: Vec::new(),
        audio: Vec::new(),
        subtitles: Vec::new(),
        attachments: Vec::new(),
        chapters: Vec::new(),
        warnings: Vec::new(),
    };
    let (mut vo, mut ao, mut so, mut to) = (0u32, 0u32, 0u32, 0u32);
    for (position, s) in streams.iter().enumerate() {
        let index = unsigned(s, "index")
            .and_then(|i| u32::try_from(i).ok())
            .unwrap_or_else(|| u32::try_from(position).unwrap_or(u32::MAX));
        let codec_name = text(s, "codec_name").unwrap_or_else(|| "unknown".to_string());
        match s.get("codec_type").and_then(Value::as_str) {
            Some("video") => {
                let ordinal = vo;
                vo += 1;
                let (Some(width), Some(height)) = (dimension(s, "width"), dimension(s, "height"))
                else {
                    result
                        .warnings
                        .push(format!("video stream {index} has no usable dimensions"));
                    continue;
                };
                let transfer = text(s, "color_transfer");
                let pix_fmt = text(s, "pix_fmt");
                let sar = text(s, "sample_aspect_ratio")
                    .as_deref()
                    .and_then(parse_ratio)
                    .filter(|(n, d)| *n > 0 && *d > 0)
                    .unwrap_or((1, 1));
                let hdr = detect_hdr(s, transfer.as_deref());
                result.video.push(VideoStream {
                    index,
                    ordinal,
                    codec_name,
                    profile: text(s, "profile"),
                    width,
                    height,
                    sar,
                    bit_depth: bit_depth(s, pix_fmt.as_deref()),
                    pix_fmt,
                    frame_rate: frame_rate(s),
                    field_order: field_order(s),
                    color_transfer: transfer,
                    color_primaries: text(s, "color_primaries"),
                    color_space: text(s, "color_space"),
                    hdr,
                    bit_rate: unsigned(s, "bit_rate"),
                    is_default: disposition(s, "default"),
                    is_attached_pic: disposition(s, "attached_pic"),
                    language: language(s),
                });
            }
            Some("audio") => {
                let ordinal = ao;
                ao += 1;
                let channels = unsigned(s, "channels")
                    .and_then(|c| u32::try_from(c).ok())
                    .filter(|c| (1..=32).contains(c))
                    .unwrap_or(2);
                let title = tag(s, "title");
                let commentary = disposition(s, "comment")
                    || title
                        .as_deref()
                        .is_some_and(|t| t.to_ascii_lowercase().contains("commentary"));
                result.audio.push(AudioStream {
                    index,
                    ordinal,
                    codec_name,
                    profile: text(s, "profile"),
                    channels,
                    channel_layout: text(s, "channel_layout"),
                    sample_rate: unsigned(s, "sample_rate").and_then(|r| u32::try_from(r).ok()),
                    bit_rate: unsigned(s, "bit_rate"),
                    language: language(s),
                    title,
                    is_default: disposition(s, "default"),
                    is_commentary: commentary,
                    is_visual_impaired: disposition(s, "visual_impaired"),
                });
            }
            Some("subtitle") => {
                let ordinal = so;
                so += 1;
                result.subtitles.push(SubtitleStream {
                    index,
                    ordinal,
                    format: SubtitleFormat::from_codec(&codec_name),
                    codec_name,
                    language: language(s),
                    title: tag(s, "title"),
                    is_default: disposition(s, "default"),
                    is_forced: disposition(s, "forced"),
                    is_hearing_impaired: disposition(s, "hearing_impaired"),
                });
            }
            Some("attachment") => {
                let ordinal = to;
                to += 1;
                result.attachments.push(AttachmentStream {
                    index,
                    ordinal,
                    filename: safe_filename(&tag(s, "filename").unwrap_or_default()),
                    mime_type: tag(s, "mimetype").map(|m| m.to_ascii_lowercase()),
                });
            }
            _ => {}
        }
    }
    if let Some(chapters) = root.get("chapters").and_then(Value::as_array) {
        for c in chapters.iter().take(MAX_CHAPTERS) {
            if let (Some(start), Some(end)) = (number(c, "start_time"), number(c, "end_time"))
                && end >= start
            {
                result.chapters.push(Chapter {
                    start_secs: start,
                    end_secs: end,
                    title: tag(c, "title"),
                });
            }
        }
    }
    if result.format.duration_secs.is_none() {
        result.warnings.push("container reports no duration".into());
    }
    Ok(result)
}

/// Reject paths that ffprobe/ffmpeg could read as options or protocols.
pub fn check_input_path(path: &str) -> Result<(), ProbeError> {
    let bad = path.is_empty()
        || path.starts_with('-')
        || path.contains('\0')
        || path.contains('\n')
        || path.contains("://")
        || path.starts_with("pipe:");
    if bad {
        Err(ProbeError::UnsafePath)
    } else {
        Ok(())
    }
}

/// The exact `ffprobe` argument vector for `input`.
pub fn ffprobe_args(input: &str) -> Vec<String> {
    [
        "-v",
        "error",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
        "-show_chapters",
        "-show_error",
        "-i",
        input,
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect()
}

/// Run ffprobe on a local file and parse the result.
pub fn probe_file(
    runner: &dyn CommandRunner,
    ffprobe: &str,
    input: &str,
) -> Result<ProbeResult, ProbeError> {
    check_input_path(input)?;
    let spec = CommandSpec::new(ffprobe, ffprobe_args(input))
        .with_timeout(Duration::from_secs(120))
        .with_max_stdout(MAX_PROBE_JSON_BYTES);
    let out = runner.run(&spec)?;
    if out.timed_out {
        return Err(ProbeError::TimedOut);
    }
    if out.stdout_truncated {
        return Err(ProbeError::TooLarge);
    }
    if out.exit_code != Some(0) && out.stdout.is_empty() {
        return Err(ProbeError::Unreadable(clean_text(&out.stderr_tail)));
    }
    parse_ffprobe_json(&out.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::*;
    use crate::runner::{FakeResult, FakeRunner};

    #[test]
    fn h264_sdr_basic() {
        let p = parse_ffprobe_json(H264_SDR.as_bytes()).unwrap();
        assert_eq!(p.format.format_name, "matroska,webm");
        assert_eq!(p.duration_secs(), Some(5432.123));
        assert_eq!(p.format.size_bytes, Some(4_100_000_000));
        let v = p.primary_video().unwrap();
        assert_eq!((v.width, v.height), (1920, 1080));
        assert_eq!(v.codec_name, "h264");
        assert_eq!(v.hdr, HdrKind::Sdr);
        assert!(!v.is_interlaced());
        assert_eq!(v.field_order, FieldOrder::Progressive);
        assert!((v.frame_rate.unwrap() - 23.976).abs() < 0.001);
        assert_eq!(v.bit_depth, Some(8));
        assert_eq!(p.audio.len(), 1);
        assert_eq!(p.audio[0].language.as_deref(), Some("eng"));
        assert_eq!(p.audio[0].channels, 6);
        assert!(p.warnings.is_empty());
    }

    #[test]
    fn hevc_hdr10() {
        let p = parse_ffprobe_json(HEVC_HDR10.as_bytes()).unwrap();
        let v = p.primary_video().unwrap();
        assert_eq!((v.width, v.height), (3840, 2160));
        assert_eq!(v.hdr, HdrKind::Hdr10);
        assert!(v.hdr.is_hdr());
        assert_eq!(v.hdr.tonemap_source(), Some(TonemapSource::Pq));
        assert_eq!(v.bit_depth, Some(10));
        assert_eq!(v.color_primaries.as_deref(), Some("bt2020"));
    }

    #[test]
    fn dolby_vision_profiles() {
        let p = parse_ffprobe_json(DV_P81.as_bytes()).unwrap();
        let v = p.primary_video().unwrap();
        assert_eq!(
            v.hdr,
            HdrKind::DolbyVision {
                profile: 8,
                compat_id: Some(1)
            }
        );
        assert_eq!(v.hdr.tonemap_source(), Some(TonemapSource::Pq));

        let p5 = parse_ffprobe_json(DV_P5.as_bytes()).unwrap();
        let v5 = p5.primary_video().unwrap();
        assert_eq!(
            v5.hdr.tonemap_source(),
            Some(TonemapSource::DolbyVisionNoFallback)
        );
        // Profile 8.4 is an HLG base layer.
        let hlg_dv = HdrKind::DolbyVision {
            profile: 8,
            compat_id: Some(4),
        };
        assert_eq!(hlg_dv.tonemap_source(), Some(TonemapSource::Hlg));
        // An SDR base layer needs no tone-map.
        let sdr_dv = HdrKind::DolbyVision {
            profile: 8,
            compat_id: Some(2),
        };
        assert_eq!(sdr_dv.tonemap_source(), None);
    }

    #[test]
    fn hlg_and_hdr10plus() {
        let p = parse_ffprobe_json(HLG.as_bytes()).unwrap();
        assert_eq!(p.video[0].hdr, HdrKind::Hlg);
        let p = parse_ffprobe_json(HDR10PLUS.as_bytes()).unwrap();
        assert_eq!(p.video[0].hdr, HdrKind::Hdr10Plus);
    }

    #[test]
    fn interlaced_anamorphic_dvd() {
        let p = parse_ffprobe_json(INTERLACED_DVD.as_bytes()).unwrap();
        let v = p.primary_video().unwrap();
        assert!(v.is_interlaced());
        assert_eq!(v.codec_name, "mpeg2video");
        assert_eq!((v.width, v.height), (720, 480));
        assert_eq!(v.sar, (32, 27));
        assert_eq!(v.display_size(), (853, 480));
        assert!((v.frame_rate.unwrap() - 29.97).abs() < 0.01);
    }

    #[test]
    fn multi_audio_languages_and_commentary() {
        let p = parse_ffprobe_json(MULTI_AUDIO.as_bytes()).unwrap();
        assert_eq!(p.audio.len(), 4);
        let langs: Vec<_> = p.audio.iter().map(|a| a.language.as_deref()).collect();
        assert_eq!(langs, [Some("eng"), Some("jpn"), Some("eng"), None]);
        assert_eq!(p.audio[0].ordinal, 0);
        assert_eq!(p.audio[3].ordinal, 3);
        assert!(p.audio[0].is_default);
        assert!(p.audio[2].is_commentary);
        assert!(!p.audio[1].is_commentary);
        assert_eq!(p.audio[1].channels, 2);
        assert_eq!(p.audio[0].codec_name, "truehd");
    }

    #[test]
    fn subtitles_and_attachments() {
        let p = parse_ffprobe_json(SUBS_AND_FONTS.as_bytes()).unwrap();
        assert_eq!(p.subtitles.len(), 5);
        let fmts: Vec<_> = p.subtitles.iter().map(|s| s.format.clone()).collect();
        assert_eq!(
            fmts,
            [
                SubtitleFormat::Ass,
                SubtitleFormat::Srt,
                SubtitleFormat::Pgs,
                SubtitleFormat::VobSub,
                SubtitleFormat::MovText
            ]
        );
        assert!(p.subtitles[0].format.is_text());
        assert!(p.subtitles[2].format.is_image());
        assert!(p.subtitles[1].is_forced);
        assert!(p.subtitles[4].is_hearing_impaired);
        assert_eq!(p.subtitles[2].ordinal, 2);
        assert_eq!(p.attachments.len(), 2);
        assert!(p.attachments[0].is_font());
        // A hostile path in the tag collapses to its base name.
        assert_eq!(p.attachments[1].filename, "passwd");
        assert!(!p.attachments[1].is_font());
        assert_eq!(p.chapters.len(), 2);
        assert_eq!(p.chapters[0].title.as_deref(), Some("Opening"));
    }

    #[test]
    fn cover_art_is_not_primary() {
        let p = parse_ffprobe_json(COVER_ART.as_bytes()).unwrap();
        assert_eq!(p.video.len(), 2);
        let v = p.primary_video().unwrap();
        assert_eq!(v.codec_name, "h264");
        assert!(p.video[1].is_attached_pic);
    }

    #[test]
    fn rejects_hostile_input() {
        assert_eq!(
            parse_ffprobe_json(&vec![b' '; MAX_PROBE_JSON_BYTES + 1]),
            Err(ProbeError::TooLarge)
        );
        assert!(matches!(
            parse_ffprobe_json(b"{not json"),
            Err(ProbeError::BadJson(_))
        ));
        let many = format!(
            "{{\"streams\":[{}]}}",
            vec!["{\"codec_type\":\"data\"}"; MAX_STREAMS + 1].join(",")
        );
        assert_eq!(
            parse_ffprobe_json(many.as_bytes()),
            Err(ProbeError::TooManyStreams(MAX_STREAMS + 1))
        );
        let err = parse_ffprobe_json(br#"{"error":{"code":-1,"string":"Invalid data"}}"#);
        assert_eq!(err, Err(ProbeError::Unreadable("Invalid data".into())));
    }

    #[test]
    fn long_and_control_text_is_sanitised() {
        let title = "x".repeat(1000);
        let json = format!(
            r#"{{"streams":[{{"index":0,"codec_type":"audio","codec_name":"aac","channels":2,
            "tags":{{"title":"{title}\u0007","language":"EN\u0000G"}}}}]}}"#
        );
        let p = parse_ffprobe_json(json.as_bytes()).unwrap();
        assert_eq!(p.audio[0].title.as_ref().unwrap().len(), MAX_TEXT_LEN);
        assert_eq!(p.audio[0].language.as_deref(), Some("eng"));
    }

    #[test]
    fn absurd_dimensions_are_dropped() {
        let json = r#"{"streams":[{"index":0,"codec_type":"video","codec_name":"h264",
            "width":999999,"height":1080}]}"#;
        let p = parse_ffprobe_json(json.as_bytes()).unwrap();
        assert!(p.video.is_empty());
        assert!(p.warnings.iter().any(|w| w.contains("dimensions")));
    }

    #[test]
    fn probe_file_uses_runner_with_argument_vector() {
        let runner = FakeRunner::fixed(H264_SDR);
        let p = probe_file(&runner, "ffprobe", "/work/in.mkv").unwrap();
        assert_eq!(p.video.len(), 1);
        let calls = runner.calls();
        assert_eq!(calls[0].program, "ffprobe");
        assert_eq!(calls[0].args.last().unwrap(), "/work/in.mkv");
        assert!(
            calls[0]
                .args
                .windows(2)
                .any(|w| w == ["-print_format", "json"])
        );
    }

    #[test]
    fn probe_file_rejects_option_like_and_protocol_paths() {
        let runner = FakeRunner::fixed("{}");
        for bad in ["-i", "http://evil/x", "pipe:0", "a\0b", ""] {
            assert_eq!(
                probe_file(&runner, "ffprobe", bad),
                Err(ProbeError::UnsafePath),
                "{bad}"
            );
        }
        assert!(runner.calls().is_empty());
    }

    #[test]
    fn probe_file_reports_ffprobe_failure() {
        let runner = FakeRunner::new(|_| FakeResult::fail(1, "x.mkv: Invalid data found"));
        match probe_file(&runner, "ffprobe", "x.mkv") {
            Err(ProbeError::Unreadable(m)) => assert!(m.contains("Invalid data")),
            other => panic!("{other:?}"),
        }
    }
}
