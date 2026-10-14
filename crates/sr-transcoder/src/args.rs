//! Exact `ffmpeg` argument vectors for every step of a job (CIN-ING-005).
//!
//! Everything returns `Vec<String>` for [`sr_media::runner::CommandSpec`]: no shell, no string
//! interpolation of untrusted text into a command line. Output paths are joined onto the job's
//! sandbox directory after a strict relative-path check.
//!
//! Video is decoded in software and encoded with `libx264` or `h264_nvenc`: keeping the
//! filter graph (deinterlace, scale, tone-map) on the CPU makes both encoders produce the same
//! picture and keeps the GOP structure identical across rungs.

use crate::encoders::{Encoder, FfmpegCaps};
use sr_media::plan::{
    AudioLayout, AudioOutput, ExtractFormat, FontOutput, JobPlan, PosterPlan, ProxyPlan, RungPlan,
    SpritePlan, SubtitleOutput, VideoPlan,
};
use sr_media::probe::TonemapSource;
use sr_media::remux::RemuxPlan;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BuildError {
    #[error("output path {0:?} escapes the job directory")]
    UnsafePath(String),
    #[error("input path is not usable")]
    UnsafeInput,
    #[error("this ffmpeg cannot tone-map HDR (needs the zscale and tonemap filters)")]
    NoTonemap,
    #[error("Dolby Vision profile 5 needs an ffmpeg built with libplacebo")]
    NoLibplacebo,
    #[error("no usable H.264 encoder")]
    NoEncoder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepKind {
    Video,
    Audio,
    Subtitle,
    Font,
    Sprites,
    Poster,
    Proxy,
    Remux,
}

/// One ffmpeg invocation.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub kind: StepKind,
    /// Index into the plan's list for this kind (audio output, subtitle, poster...).
    pub index: usize,
    pub label: String,
    pub args: Vec<String>,
    /// Share of the whole job's progress bar.
    pub weight: f64,
    /// Seconds of input this step walks through, for percent from `out_time`.
    pub expect_secs: f64,
    /// Output directories (relative) to empty and recreate before the step runs.
    pub wipe: Vec<String>,
    /// Output directories (relative) to create if missing.
    pub mkdir: Vec<String>,
}

pub struct BuildCtx<'a> {
    /// Local path of the downloaded original.
    pub input: &'a str,
    /// Job output sandbox (already created).
    pub out_dir: &'a Path,
    pub encoder: Encoder,
    pub caps: &'a FfmpegCaps,
    pub duration_secs: f64,
}

fn strs(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

/// Reject anything that could leave the sandbox, then join with forward slashes.
pub fn sandbox_path(base: &Path, rel: &str) -> Result<String, BuildError> {
    let bad = rel.is_empty()
        || rel.starts_with('/')
        || rel.contains(['\\', ':', '\0'])
        || rel
            .split('/')
            .any(|c| c.is_empty() || c == "." || c == "..");
    if bad {
        return Err(BuildError::UnsafePath(rel.chars().take(80).collect()));
    }
    Ok(format!(
        "{}/{}",
        base.to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/'),
        rel
    ))
}

fn check_input(input: &str) -> Result<(), BuildError> {
    if input.is_empty() || input.starts_with('-') || input.contains('\0') || input.contains("://") {
        return Err(BuildError::UnsafeInput);
    }
    Ok(())
}

/// Arguments every step starts with, `-progress pipe:1` included.
fn base() -> Vec<String> {
    strs(&[
        "-hide_banner",
        "-nostdin",
        "-y",
        "-loglevel",
        "warning",
        "-nostats",
        "-progress",
        "pipe:1",
    ])
}

/// HDR to SDR filter chain (BT.709, 8 bit), by what must be undone.
pub fn tonemap_chain(src: TonemapSource) -> String {
    match src {
        TonemapSource::Pq => {
            "zscale=tin=smpte2084:min=bt2020nc:pin=bt2020:rin=tv:t=linear:npl=100,\
             format=gbrpf32le,zscale=p=bt709,tonemap=tonemap=hable:desat=0,\
             zscale=t=bt709:m=bt709:r=tv,format=yuv420p"
                .to_string()
        }
        TonemapSource::Hlg => {
            "zscale=tin=arib-std-b67:min=bt2020nc:pin=bt2020:rin=tv:t=linear:npl=100,\
             format=gbrpf32le,zscale=p=bt709,tonemap=tonemap=hable:desat=0,\
             zscale=t=bt709:m=bt709:r=tv,format=yuv420p"
                .to_string()
        }
        TonemapSource::DolbyVisionNoFallback => {
            "libplacebo=tonemapping=bt.2390:colorspace=bt709:color_primaries=bt709:\
             color_trc=bt709:range=tv:format=yuv420p"
                .to_string()
        }
    }
}

fn check_tonemap(src: Option<TonemapSource>, caps: &FfmpegCaps) -> Result<(), BuildError> {
    match src {
        None => Ok(()),
        Some(TonemapSource::DolbyVisionNoFallback) if !caps.has_libplacebo() => {
            Err(BuildError::NoLibplacebo)
        }
        Some(TonemapSource::DolbyVisionNoFallback) => Ok(()),
        Some(_) if caps.can_tonemap() => Ok(()),
        Some(_) => Err(BuildError::NoTonemap),
    }
}

/// Deinterlace (when needed) then constant frame rate, before the picture is split per rung.
fn head_filters(v: &VideoPlan) -> String {
    let mut f: Vec<String> = Vec::new();
    if v.deinterlace {
        // One frame per frame (not per field), parity from the stream, all frames.
        f.push("yadif=mode=send_frame:parity=auto:deint=all".to_string());
    }
    f.push(format!("fps={}/{}", v.fps_num, v.fps_den));
    f.join(",")
}

fn rung_filter(r: &RungPlan, tonemap: Option<TonemapSource>) -> String {
    let mut f = vec![format!("scale={}:{}:flags=lanczos", r.width, r.height)];
    if let Some(t) = tonemap {
        f.push(tonemap_chain(t));
    }
    f.push("setsar=1".to_string());
    f.push("format=yuv420p".to_string());
    f.join(",")
}

/// Per-rung encoder options: bitrate-capped H.264 High with closed, fixed-size GOPs and no
/// scene-cut keyframes, so every rung has its keyframes on exactly the same frames.
pub fn video_encoder_args(r: &RungPlan, gop: u32, enc: Encoder, tonemapped: bool) -> Vec<String> {
    let mut a = Vec::new();
    let kbps = |v: u32| format!("{v}k");
    match enc {
        Encoder::Libx264 => {
            a.extend(strs(&[
                "-c:v",
                "libx264",
                "-preset",
                "medium",
                "-profile:v",
                "high",
            ]));
            a.extend(["-level:v".to_string(), r.level.clone()]);
            a.extend(["-b:v".to_string(), kbps(r.video_kbps)]);
            a.extend(["-maxrate".to_string(), kbps(r.maxrate_kbps)]);
            a.extend(["-bufsize".to_string(), kbps(r.bufsize_kbps)]);
            a.extend(["-g".to_string(), gop.to_string()]);
            a.extend(["-keyint_min".to_string(), gop.to_string()]);
            a.extend(strs(&[
                "-sc_threshold",
                "0",
                "-bf",
                "2",
                "-flags",
                "+cgop",
                "-x264-params",
                "open-gop=0:scenecut=0",
            ]));
        }
        Encoder::H264Nvenc => {
            a.extend(strs(&[
                "-c:v",
                "h264_nvenc",
                "-preset",
                "p5",
                "-tune",
                "hq",
                "-profile:v",
                "high",
            ]));
            a.extend(["-level:v".to_string(), r.level.clone()]);
            a.extend(strs(&["-rc", "vbr"]));
            a.extend(["-b:v".to_string(), kbps(r.video_kbps)]);
            a.extend(["-maxrate".to_string(), kbps(r.maxrate_kbps)]);
            a.extend(["-bufsize".to_string(), kbps(r.bufsize_kbps)]);
            a.extend(["-g".to_string(), gop.to_string()]);
            a.extend(strs(&[
                "-bf",
                "2",
                "-forced-idr",
                "1",
                "-no-scenecut",
                "1",
                "-strict_gop",
                "1",
                "-spatial-aq",
                "1",
            ]));
        }
    }
    a.extend(strs(&["-pix_fmt", "yuv420p"]));
    if tonemapped {
        a.extend(strs(&[
            "-color_primaries",
            "bt709",
            "-color_trc",
            "bt709",
            "-colorspace",
            "bt709",
            "-color_range",
            "tv",
        ]));
    }
    a
}

/// fMP4 HLS muxer options for one rendition under `dir` (absolute, forward slashes).
pub fn hls_output_args(dir: &str, segment_secs: u32) -> Vec<String> {
    let mut a = strs(&["-f", "hls", "-hls_time"]);
    a.push(segment_secs.to_string());
    a.extend(strs(&[
        "-hls_playlist_type",
        "vod",
        "-hls_list_size",
        "0",
        "-hls_segment_type",
        "fmp4",
        "-hls_fmp4_init_filename",
        "init.mp4",
        "-hls_flags",
        "independent_segments",
        "-hls_segment_filename",
    ]));
    a.push(format!("{dir}/seg_%05d.m4s"));
    a.push(format!("{dir}/index.m3u8"));
    a
}

/// One ffmpeg for the whole ladder: decode once, split, scale and encode every rung.
pub fn video_step(plan: &JobPlan, ctx: &BuildCtx) -> Result<Step, BuildError> {
    check_input(ctx.input)?;
    check_tonemap(plan.video.tonemap, ctx.caps)?;
    let n = plan.rungs.len();
    let mut graph = format!(
        "[0:v:{}]{}",
        plan.video.source_ordinal,
        head_filters(&plan.video)
    );
    if n > 1 {
        graph.push_str(&format!(",split={n}"));
        for i in 0..n {
            graph.push_str(&format!("[s{i}]"));
        }
        graph.push(';');
    } else {
        graph.push_str("[s0];");
    }
    for (i, r) in plan.rungs.iter().enumerate() {
        graph.push_str(&format!(
            "[s{i}]{}[o{i}]",
            rung_filter(r, plan.video.tonemap)
        ));
        if i + 1 < n {
            graph.push(';');
        }
    }
    let mut args = base();
    args.extend(["-i".to_string(), ctx.input.to_string()]);
    args.extend(["-filter_complex".to_string(), graph]);
    for (i, r) in plan.rungs.iter().enumerate() {
        args.extend(["-map".to_string(), format!("[o{i}]")]);
        args.extend(strs(&["-an", "-sn"]));
        args.extend(video_encoder_args(
            r,
            plan.video.gop_frames,
            ctx.encoder,
            plan.video.tonemap.is_some(),
        ));
        args.extend(hls_output_args(
            &sandbox_path(ctx.out_dir, &r.dir)?,
            plan.hls.segment_secs,
        ));
    }
    Ok(Step {
        kind: StepKind::Video,
        index: 0,
        label: format!("video ladder ({n} rungs, {})", ctx.encoder.name()),
        args,
        weight: 0.80,
        expect_secs: ctx.duration_secs,
        wipe: plan.rungs.iter().map(|r| r.dir.clone()).collect(),
        mkdir: vec![],
    })
}

pub fn audio_step(
    plan: &JobPlan,
    index: usize,
    a: &AudioOutput,
    ctx: &BuildCtx,
) -> Result<Step, BuildError> {
    check_input(ctx.input)?;
    let mut args = base();
    args.extend(["-i".to_string(), ctx.input.to_string()]);
    args.extend(["-map".to_string(), format!("0:a:{}", a.source_ordinal)]);
    args.extend(strs(&["-vn", "-sn", "-c:a", "aac", "-ar", "48000"]));
    args.extend(["-ac".to_string(), a.channels.to_string()]);
    args.extend(["-b:a".to_string(), format!("{}k", a.bitrate_kbps)]);
    if a.layout == AudioLayout::Surround51 {
        args.extend(strs(&["-af", "aformat=channel_layouts=5.1"]));
    } else {
        args.extend(strs(&["-af", "aresample=async=1:first_pts=0"]));
    }
    args.extend(hls_output_args(
        &sandbox_path(ctx.out_dir, &a.dir)?,
        plan.hls.segment_secs,
    ));
    Ok(Step {
        kind: StepKind::Audio,
        index,
        label: format!("audio {} ({})", a.label, a.dir),
        args,
        weight: 0.08 / plan.audio.len().max(1) as f64,
        expect_secs: ctx.duration_secs,
        wipe: vec![a.dir.clone()],
        mkdir: vec![],
    })
}

pub fn subtitle_step(
    plan: &JobPlan,
    index: usize,
    s: &SubtitleOutput,
    ctx: &BuildCtx,
) -> Result<Step, BuildError> {
    check_input(ctx.input)?;
    let (codec, fmt) = match s.extract {
        ExtractFormat::Ass => ("ass", "ass"),
        ExtractFormat::Srt => ("srt", "srt"),
    };
    let mut args = base();
    args.extend(["-i".to_string(), ctx.input.to_string()]);
    args.extend(["-map".to_string(), format!("0:s:{}", s.source_ordinal)]);
    args.extend(strs(&["-vn", "-an", "-c:s", codec, "-f", fmt]));
    args.push(sandbox_path(ctx.out_dir, &s.raw_path)?);
    Ok(Step {
        kind: StepKind::Subtitle,
        index,
        label: format!("subtitle {}", s.label),
        args,
        weight: 0.01 / plan.subtitles.len().max(1) as f64,
        expect_secs: ctx.duration_secs,
        wipe: vec![],
        mkdir: vec!["subs".to_string()],
    })
}

/// Dump one embedded font attachment (ASS styled rendering). The dump happens when the input
/// is opened; one video frame is decoded to a null muxer so ffmpeg has an output to run.
pub fn font_step(index: usize, f: &FontOutput, ctx: &BuildCtx) -> Result<Step, BuildError> {
    check_input(ctx.input)?;
    let mut args = strs(&["-hide_banner", "-nostdin", "-y", "-loglevel", "error"]);
    args.push(format!("-dump_attachment:t:{}", f.attachment_ordinal));
    args.push(sandbox_path(ctx.out_dir, &f.path)?);
    args.extend(["-i".to_string(), ctx.input.to_string()]);
    args.extend(strs(&[
        "-map",
        "0:v:0",
        "-frames:v",
        "1",
        "-f",
        "null",
        "-",
    ]));
    Ok(Step {
        kind: StepKind::Font,
        index,
        label: format!("font {}", f.path),
        args,
        weight: 0.0,
        expect_secs: 0.0,
        wipe: vec![],
        mkdir: vec!["fonts".to_string()],
    })
}

/// Thumbnail sprite sheets: one frame per interval, tiled `columns` x `rows` per JPEG.
pub fn sprite_step(plan: &JobPlan, ctx: &BuildCtx) -> Result<Step, BuildError> {
    check_input(ctx.input)?;
    check_tonemap(plan.video.tonemap, ctx.caps)?;
    let sp: &SpritePlan = &plan.sprite;
    let mut f: Vec<String> = Vec::new();
    if plan.video.deinterlace {
        f.push("yadif=mode=send_frame:parity=auto:deint=all".to_string());
    }
    f.push(format!("fps=1/{}", sp.interval_secs));
    f.push(format!(
        "scale={}:{}:flags=bilinear",
        sp.thumb_width, sp.thumb_height
    ));
    if let Some(t) = plan.video.tonemap {
        f.push(tonemap_chain(t));
    }
    f.push(format!("tile={}x{}", sp.columns, sp.rows));
    let mut args = base();
    args.extend(["-i".to_string(), ctx.input.to_string()]);
    args.extend([
        "-map".to_string(),
        format!("0:v:{}", plan.video.source_ordinal),
    ]);
    args.extend(["-vf".to_string(), f.join(",")]);
    args.extend(strs(&[
        "-an",
        "-sn",
        "-fps_mode",
        "vfr",
        "-pix_fmt",
        "yuvj420p",
        "-q:v",
        "5",
    ]));
    args.push("-f".to_string());
    args.push("image2".to_string());
    args.push(sandbox_path(ctx.out_dir, &sp.sheet_pattern)?);
    Ok(Step {
        kind: StepKind::Sprites,
        index: 0,
        label: "thumbnail sprites".to_string(),
        args,
        weight: 0.07,
        expect_secs: ctx.duration_secs,
        wipe: vec!["sprites".to_string()],
        mkdir: vec![],
    })
}

/// One poster candidate: seek (fast, before `-i`), one frame, native size capped at 1080p.
pub fn poster_step(
    plan: &JobPlan,
    index: usize,
    p: &PosterPlan,
    ctx: &BuildCtx,
) -> Result<Step, BuildError> {
    check_input(ctx.input)?;
    check_tonemap(plan.video.tonemap, ctx.caps)?;
    let mut f: Vec<String> = Vec::new();
    if plan.video.deinterlace {
        f.push("yadif=mode=send_frame:parity=auto:deint=all".to_string());
    }
    f.push(format!(
        "scale={}:{}:force_original_aspect_ratio=decrease:flags=lanczos",
        plan.rungs.first().map_or(1920, |r| r.width),
        plan.rungs.first().map_or(1080, |r| r.height)
    ));
    if let Some(t) = plan.video.tonemap {
        f.push(tonemap_chain(t));
    }
    f.push("setsar=1".to_string());
    let mut args = base();
    args.extend(["-ss".to_string(), format!("{:.3}", p.at_secs)]);
    args.extend(["-i".to_string(), ctx.input.to_string()]);
    args.extend([
        "-map".to_string(),
        format!("0:v:{}", plan.video.source_ordinal),
    ]);
    args.extend(["-vf".to_string(), f.join(",")]);
    args.extend(strs(&[
        "-an",
        "-sn",
        "-frames:v",
        "1",
        "-pix_fmt",
        "yuvj420p",
        "-q:v",
        "2",
    ]));
    args.push(sandbox_path(ctx.out_dir, &p.path)?);
    Ok(Step {
        kind: StepKind::Poster,
        index,
        label: format!("poster {}", index + 1),
        args,
        weight: 0.02 / plan.posters.len().max(1) as f64,
        expect_secs: 0.0,
        wipe: vec![],
        mkdir: vec!["posters".to_string()],
    })
}

/// All steps of a ladder job, in execution order.
pub fn ladder_steps(plan: &JobPlan, ctx: &BuildCtx) -> Result<Vec<Step>, BuildError> {
    let mut steps = vec![video_step(plan, ctx)?];
    for (i, a) in plan.audio.iter().enumerate() {
        steps.push(audio_step(plan, i, a, ctx)?);
    }
    for (i, s) in plan.subtitles.iter().enumerate() {
        steps.push(subtitle_step(plan, i, s, ctx)?);
    }
    for (i, f) in plan.fonts.iter().enumerate() {
        steps.push(font_step(i, f, ctx)?);
    }
    steps.push(sprite_step(plan, ctx)?);
    for (i, p) in plan.posters.iter().enumerate() {
        steps.push(poster_step(plan, i, p, ctx)?);
    }
    Ok(steps)
}

/// The review proxy: one fast MP4, `faststart` so reviewers can watch while it uploads.
pub fn proxy_steps(plan: &ProxyPlan, ctx: &BuildCtx) -> Result<Vec<Step>, BuildError> {
    check_input(ctx.input)?;
    check_tonemap(plan.tonemap, ctx.caps)?;
    let mut f: Vec<String> = Vec::new();
    if plan.deinterlace {
        f.push("yadif=mode=send_frame:parity=auto:deint=all".to_string());
    }
    f.push(format!("fps={}", plan.fps));
    f.push(format!(
        "scale={}:{}:flags=bilinear",
        plan.width, plan.height
    ));
    if let Some(t) = plan.tonemap {
        f.push(tonemap_chain(t));
    }
    f.push("setsar=1".to_string());
    f.push("format=yuv420p".to_string());
    let mut args = base();
    args.extend(["-i".to_string(), ctx.input.to_string()]);
    args.extend(["-map".to_string(), format!("0:v:{}", plan.source_ordinal)]);
    if let Some(a) = &plan.audio {
        args.extend(["-map".to_string(), format!("0:a:{}", a.source_ordinal)]);
    }
    args.extend(["-vf".to_string(), f.join(",")]);
    args.extend(strs(&["-sn"]));
    match ctx.encoder {
        Encoder::Libx264 => {
            args.extend(strs(&[
                "-c:v",
                "libx264",
                "-preset",
                &plan.preset,
                "-profile:v",
                "main",
            ]));
            args.extend(["-crf".to_string(), plan.crf.to_string()]);
        }
        Encoder::H264Nvenc => {
            args.extend(strs(&[
                "-c:v",
                "h264_nvenc",
                "-preset",
                "p1",
                "-profile:v",
                "main",
                "-rc",
                "vbr",
                "-cq",
            ]));
            args.push(plan.crf.to_string());
            args.extend(strs(&["-b:v", "0"]));
        }
    }
    args.extend(["-maxrate".to_string(), format!("{}k", plan.maxrate_kbps)]);
    args.extend([
        "-bufsize".to_string(),
        format!("{}k", plan.maxrate_kbps * 2),
    ]);
    args.extend(strs(&["-pix_fmt", "yuv420p"]));
    match &plan.audio {
        Some(a) => {
            args.extend(strs(&["-c:a", "aac", "-ac", "2", "-ar", "48000"]));
            args.extend(["-b:a".to_string(), format!("{}k", a.bitrate_kbps)]);
        }
        None => args.push("-an".to_string()),
    }
    args.extend(strs(&["-movflags", "+faststart", "-f", "mp4"]));
    args.push(sandbox_path(ctx.out_dir, &plan.output)?);
    Ok(vec![Step {
        kind: StepKind::Proxy,
        index: 0,
        label: format!("review proxy ({})", ctx.encoder.name()),
        args,
        weight: 1.0,
        expect_secs: ctx.duration_secs,
        wipe: vec!["proxy".to_string()],
        mkdir: vec![],
    }])
}

/// The stream-copy remux of a stored rendition into one MP4 (Patron download). `in_dir` holds
/// the downloaded playlists and segments in their original layout; ffmpeg reads the local
/// playlists, so nothing is decoded or re-encoded.
pub fn remux_steps(
    plan: &RemuxPlan,
    in_dir: &Path,
    out_dir: &Path,
) -> Result<Vec<Step>, BuildError> {
    let mut args = base();
    args.extend([
        "-i".to_string(),
        sandbox_path(in_dir, &plan.video_playlist)?,
    ]);
    for a in &plan.audio_playlists {
        args.extend(["-i".to_string(), sandbox_path(in_dir, a)?]);
    }
    args.extend(strs(&["-map", "0:v:0"]));
    for i in 0..plan.audio_playlists.len() {
        args.extend(["-map".to_string(), format!("{}:a:0", i + 1)]);
    }
    args.extend(strs(&[
        "-c",
        "copy",
        "-sn",
        "-dn",
        "-movflags",
        "+faststart",
        "-f",
        "mp4",
    ]));
    args.push(sandbox_path(out_dir, &plan.output)?);
    Ok(vec![Step {
        kind: StepKind::Remux,
        index: 0,
        label: format!("remux {}", plan.rendition),
        args,
        weight: 1.0,
        expect_secs: plan.duration_secs,
        wipe: vec!["remux".to_string()],
        mkdir: vec![],
    }])
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use sr_media::plan::{TranscodeSettings, plan_ladder_job, plan_proxy_job};
    use sr_media::probe::parse_ffprobe_json;
    use std::path::PathBuf;

    pub const SDR_1080: &str = r#"{"streams":[
      {"index":0,"codec_name":"h264","codec_type":"video","width":1920,"height":1080,
       "pix_fmt":"yuv420p","field_order":"progressive","r_frame_rate":"24000/1001",
       "avg_frame_rate":"24000/1001","disposition":{"default":1}},
      {"index":1,"codec_name":"ac3","codec_type":"audio","channels":6,"sample_rate":"48000",
       "disposition":{"default":1},"tags":{"language":"eng"}},
      {"index":2,"codec_name":"ass","codec_type":"subtitle","tags":{"language":"eng"}},
      {"index":3,"codec_name":"subrip","codec_type":"subtitle","tags":{"language":"spa"}},
      {"index":4,"codec_name":"hdmv_pgs_subtitle","codec_type":"subtitle","tags":{"language":"fre"}},
      {"index":5,"codec_name":"ttf","codec_type":"attachment",
       "tags":{"filename":"Font One.ttf","mimetype":"application/x-truetype-font"}}
    ],"format":{"format_name":"matroska,webm","duration":"1200.0"}}"#;

    pub const HDR_4K: &str = r#"{"streams":[
      {"index":0,"codec_name":"hevc","codec_type":"video","width":3840,"height":2160,
       "pix_fmt":"yuv420p10le","color_transfer":"smpte2084","color_primaries":"bt2020",
       "field_order":"progressive","r_frame_rate":"24/1","avg_frame_rate":"24/1"},
      {"index":1,"codec_name":"eac3","codec_type":"audio","channels":6,"tags":{"language":"eng"}}
    ],"format":{"duration":"600.0"}}"#;

    pub const DVD: &str = r#"{"streams":[
      {"index":0,"codec_name":"mpeg2video","codec_type":"video","width":720,"height":480,
       "sample_aspect_ratio":"32:27","pix_fmt":"yuv420p","field_order":"tt",
       "r_frame_rate":"30000/1001","avg_frame_rate":"30000/1001"},
      {"index":1,"codec_name":"ac3","codec_type":"audio","channels":2,"tags":{"language":"eng"}}
    ],"format":{"duration":"300.0"}}"#;

    pub fn caps() -> FfmpegCaps {
        FfmpegCaps {
            version: Some("7.0".into()),
            encoders: ["libx264", "h264_nvenc", "aac"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            hwaccels: ["cuda"].iter().map(|s| s.to_string()).collect(),
            filters: ["zscale", "tonemap", "yadif"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            nvenc_usable: true,
        }
    }

    pub fn ladder(json: &str) -> JobPlan {
        plan_ladder_job(
            &parse_ffprobe_json(json.as_bytes()).unwrap(),
            &TranscodeSettings::default(),
        )
        .unwrap()
    }

    fn ctx<'a>(caps: &'a FfmpegCaps, out: &'a Path, enc: Encoder, dur: f64) -> BuildCtx<'a> {
        BuildCtx {
            input: "/work/j1/in/source.mkv",
            out_dir: out,
            encoder: enc,
            caps,
            duration_secs: dur,
        }
    }

    fn has_seq(args: &[String], seq: &[&str]) -> bool {
        args.windows(seq.len())
            .any(|w| w.iter().zip(seq).all(|(a, b)| a == b))
    }

    #[test]
    fn sandbox_rejects_escapes() {
        let base = PathBuf::from("/work/j1/out");
        assert_eq!(
            sandbox_path(&base, "v/1080p").unwrap(),
            "/work/j1/out/v/1080p"
        );
        for bad in [
            "", "/etc/x", "../x", "a/../b", "a//b", "a\\b", "C:x", "a/./b", "a\0b",
        ] {
            assert!(sandbox_path(&base, bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn libx264_ladder_exact_arguments() {
        let plan = ladder(SDR_1080);
        let caps = caps();
        let out = PathBuf::from("/work/j1/out");
        let step = video_step(&plan, &ctx(&caps, &out, Encoder::Libx264, 1200.0)).unwrap();
        let line = step.args.join(" ");
        let expected_head = "-hide_banner -nostdin -y -loglevel warning -nostats -progress pipe:1 \
            -i /work/j1/in/source.mkv -filter_complex \
            [0:v:0]fps=24000/1001,split=3[s0][s1][s2];\
            [s0]scale=1920:1080:flags=lanczos,setsar=1,format=yuv420p[o0];\
            [s1]scale=1280:720:flags=lanczos,setsar=1,format=yuv420p[o1];\
            [s2]scale=854:480:flags=lanczos,setsar=1,format=yuv420p[o2] ";
        assert!(line.starts_with(expected_head), "{line}");
        let rung0 = "-map [o0] -an -sn -c:v libx264 -preset medium -profile:v high -level:v 4.0 \
            -b:v 6000k -maxrate 9000k -bufsize 18000k -g 48 -keyint_min 48 -sc_threshold 0 -bf 2 \
            -flags +cgop -x264-params open-gop=0:scenecut=0 -pix_fmt yuv420p \
            -f hls -hls_time 6 -hls_playlist_type vod -hls_list_size 0 -hls_segment_type fmp4 \
            -hls_fmp4_init_filename init.mp4 -hls_flags independent_segments \
            -hls_segment_filename /work/j1/out/v/1080p/seg_%05d.m4s /work/j1/out/v/1080p/index.m3u8";
        assert!(line.contains(rung0), "{line}");
        // Three outputs, identical GOP on all of them.
        assert_eq!(step.args.iter().filter(|a| *a == "-g").count(), 3);
        assert_eq!(step.args.iter().filter(|a| *a == "48").count(), 6);
        assert!(line.ends_with("/work/j1/out/v/480p/index.m3u8"));
        assert_eq!(step.kind, StepKind::Video);
        assert_eq!(step.expect_secs, 1200.0);
    }

    #[test]
    fn nvenc_ladder_uses_aligned_closed_gops() {
        let plan = ladder(SDR_1080);
        let caps = caps();
        let out = PathBuf::from("/work/j1/out");
        let step = video_step(&plan, &ctx(&caps, &out, Encoder::H264Nvenc, 1200.0)).unwrap();
        let a = &step.args;
        assert!(has_seq(
            a,
            &[
                "-c:v",
                "h264_nvenc",
                "-preset",
                "p5",
                "-tune",
                "hq",
                "-profile:v",
                "high"
            ]
        ));
        assert!(has_seq(
            a,
            &["-rc", "vbr", "-b:v", "6000k", "-maxrate", "9000k"]
        ));
        assert!(has_seq(
            a,
            &[
                "-g",
                "48",
                "-bf",
                "2",
                "-forced-idr",
                "1",
                "-no-scenecut",
                "1",
                "-strict_gop",
                "1"
            ]
        ));
        assert!(!a.contains(&"libx264".to_string()));
        assert_eq!(a.iter().filter(|x| *x == "-forced-idr").count(), 3);
        assert!(step.label.contains("h264_nvenc"));
    }

    #[test]
    fn hdr_tonemap_per_rung_after_scale() {
        let plan = ladder(HDR_4K);
        assert_eq!(plan.rungs.len(), 3);
        let caps = caps();
        let out = PathBuf::from("/o");
        let step = video_step(&plan, &ctx(&caps, &out, Encoder::Libx264, 600.0)).unwrap();
        let graph = &step.args[step
            .args
            .iter()
            .position(|a| a == "-filter_complex")
            .unwrap()
            + 1];
        assert!(graph.contains(
            "[s0]scale=1920:1080:flags=lanczos,zscale=tin=smpte2084:min=bt2020nc:pin=bt2020:rin=tv:t=linear:npl=100,\
             format=gbrpf32le,zscale=p=bt709,tonemap=tonemap=hable:desat=0,zscale=t=bt709:m=bt709:r=tv,\
             format=yuv420p,setsar=1,format=yuv420p[o0]"
        ), "{graph}");
        assert_eq!(graph.matches("tonemap=hable").count(), 3);
        // Output is tagged BT.709 because we converted.
        assert!(has_seq(
            &step.args,
            &["-color_primaries", "bt709", "-color_trc", "bt709"]
        ));
    }

    #[test]
    fn hdr_without_tonemap_filters_is_a_build_error() {
        let plan = ladder(HDR_4K);
        let mut caps = caps();
        caps.filters.remove("zscale");
        let out = PathBuf::from("/o");
        assert_eq!(
            video_step(&plan, &ctx(&caps, &out, Encoder::Libx264, 1.0)).unwrap_err(),
            BuildError::NoTonemap
        );
        // SDR does not care.
        let sdr = ladder(SDR_1080);
        assert!(video_step(&sdr, &ctx(&caps, &out, Encoder::Libx264, 1.0)).is_ok());
    }

    #[test]
    fn interlaced_anamorphic_single_rung() {
        let plan = ladder(DVD);
        let caps = caps();
        let out = PathBuf::from("/o");
        let step = video_step(&plan, &ctx(&caps, &out, Encoder::Libx264, 300.0)).unwrap();
        let graph = &step.args[step
            .args
            .iter()
            .position(|a| a == "-filter_complex")
            .unwrap()
            + 1];
        assert_eq!(
            graph,
            "[0:v:0]yadif=mode=send_frame:parity=auto:deint=all,fps=30000/1001[s0];\
             [s0]scale=854:480:flags=lanczos,setsar=1,format=yuv420p[o0]"
        );
        assert!(!graph.contains("split"));
    }

    #[test]
    fn audio_steps() {
        let plan = ladder(SDR_1080);
        let caps = caps();
        let out = PathBuf::from("/work/j1/out");
        let c = ctx(&caps, &out, Encoder::Libx264, 1200.0);
        let a = audio_step(&plan, 0, &plan.audio[0], &c).unwrap();
        let line = a.args.join(" ");
        assert!(line.contains("-i /work/j1/in/source.mkv -map 0:a:0 -vn -sn -c:a aac -ar 48000 -ac 2 -b:a 160k -af aresample=async=1:first_pts=0 -f hls -hls_time 6"));
        assert!(line.ends_with("/work/j1/out/a/00-eng-stereo/index.m3u8"));

        let s = TranscodeSettings {
            keep_surround: true,
            ..TranscodeSettings::default()
        };
        let plan51 =
            plan_ladder_job(&parse_ffprobe_json(SDR_1080.as_bytes()).unwrap(), &s).unwrap();
        let sur = plan51
            .audio
            .iter()
            .find(|a| a.layout == AudioLayout::Surround51)
            .unwrap();
        let st = audio_step(&plan51, 1, sur, &c).unwrap();
        assert!(has_seq(
            &st.args,
            &[
                "-ac",
                "6",
                "-b:a",
                "384k",
                "-af",
                "aformat=channel_layouts=5.1"
            ]
        ));
    }

    #[test]
    fn subtitle_and_font_steps() {
        let plan = ladder(SDR_1080);
        assert_eq!(plan.subtitles.len(), 2);
        let caps = caps();
        let out = PathBuf::from("/work/j1/out");
        let c = ctx(&caps, &out, Encoder::Libx264, 1200.0);
        let ass = subtitle_step(&plan, 0, &plan.subtitles[0], &c).unwrap();
        assert!(has_seq(
            &ass.args,
            &["-map", "0:s:0", "-vn", "-an", "-c:s", "ass", "-f", "ass"]
        ));
        assert_eq!(ass.args.last().unwrap(), "/work/j1/out/subs/00-eng.ass");
        let srt = subtitle_step(&plan, 1, &plan.subtitles[1], &c).unwrap();
        assert!(has_seq(
            &srt.args,
            &["-map", "0:s:1", "-vn", "-an", "-c:s", "srt", "-f", "srt"]
        ));
        // PGS is skipped, not extracted.
        assert_eq!(plan.skipped_subtitles.len(), 1);
        let font = font_step(0, &plan.fonts[0], &c).unwrap();
        assert_eq!(
            font.args.join(" "),
            "-hide_banner -nostdin -y -loglevel error -dump_attachment:t:0 \
             /work/j1/out/fonts/Font One.ttf -i /work/j1/in/source.mkv -map 0:v:0 -frames:v 1 -f null -"
        );
    }

    #[test]
    fn sprite_and_poster_steps() {
        let plan = ladder(SDR_1080);
        let caps = caps();
        let out = PathBuf::from("/work/j1/out");
        let c = ctx(&caps, &out, Encoder::Libx264, 1200.0);
        let s = sprite_step(&plan, &c).unwrap();
        assert!(has_seq(
            &s.args,
            &["-vf", "fps=1/10,scale=160:90:flags=bilinear,tile=10x10"]
        ));
        assert_eq!(
            s.args.last().unwrap(),
            "/work/j1/out/sprites/sheet-%03d.jpg"
        );
        let p = poster_step(&plan, 2, &plan.posters[2], &c).unwrap();
        assert!(has_seq(
            &p.args,
            &["-ss", "480.000", "-i", "/work/j1/in/source.mkv"]
        ));
        assert!(has_seq(&p.args, &["-frames:v", "1"]));
        assert_eq!(
            p.args.last().unwrap(),
            "/work/j1/out/posters/candidate-3.jpg"
        );
    }

    #[test]
    fn full_step_order_and_weights() {
        let plan = ladder(SDR_1080);
        let caps = caps();
        let out = PathBuf::from("/o");
        let steps = ladder_steps(&plan, &ctx(&caps, &out, Encoder::Libx264, 1200.0)).unwrap();
        let kinds: Vec<_> = steps.iter().map(|s| s.kind).collect();
        assert_eq!(kinds[0], StepKind::Video);
        assert_eq!(kinds[1], StepKind::Audio);
        assert!(kinds.contains(&StepKind::Subtitle) && kinds.contains(&StepKind::Font));
        assert_eq!(kinds.iter().filter(|k| **k == StepKind::Poster).count(), 5);
        let total: f64 = steps.iter().map(|s| s.weight).sum();
        assert!((total - 0.98).abs() < 0.03, "{total}");
    }

    #[test]
    fn proxy_command() {
        let plan = plan_proxy_job(
            &parse_ffprobe_json(SDR_1080.as_bytes()).unwrap(),
            &TranscodeSettings::default(),
        )
        .unwrap();
        let caps = caps();
        let out = PathBuf::from("/work/j1/out");
        let steps = proxy_steps(&plan, &ctx(&caps, &out, Encoder::Libx264, 1200.0)).unwrap();
        assert_eq!(steps.len(), 1);
        assert_eq!(
            steps[0].args.join(" "),
            "-hide_banner -nostdin -y -loglevel warning -nostats -progress pipe:1 \
             -i /work/j1/in/source.mkv -map 0:v:0 -map 0:a:0 \
             -vf fps=24,scale=854:480:flags=bilinear,setsar=1,format=yuv420p -sn \
             -c:v libx264 -preset veryfast -profile:v main -crf 30 -maxrate 900k -bufsize 1800k \
             -pix_fmt yuv420p -c:a aac -ac 2 -ar 48000 -b:a 96k -movflags +faststart -f mp4 \
             /work/j1/out/proxy/proxy.mp4"
        );
        let nv = proxy_steps(&plan, &ctx(&caps, &out, Encoder::H264Nvenc, 1200.0)).unwrap();
        assert!(has_seq(
            &nv[0].args,
            &["-c:v", "h264_nvenc", "-preset", "p1"]
        ));
    }

    #[test]
    fn hostile_input_paths_are_refused() {
        let plan = ladder(SDR_1080);
        let caps = caps();
        let out = PathBuf::from("/o");
        for bad in ["-i", "http://x/y", "a\0b", ""] {
            let c = BuildCtx {
                input: bad,
                out_dir: &out,
                encoder: Encoder::Libx264,
                caps: &caps,
                duration_secs: 1.0,
            };
            assert_eq!(
                video_step(&plan, &c).unwrap_err(),
                BuildError::UnsafeInput,
                "{bad:?}"
            );
        }
        // A plan with a traversing path never reaches ffmpeg.
        let mut evil = plan.clone();
        evil.rungs[0].dir = "../../etc".into();
        let c = ctx(&caps, &out, Encoder::Libx264, 1.0);
        assert!(matches!(
            video_step(&evil, &c),
            Err(BuildError::UnsafePath(_))
        ));
    }
}
