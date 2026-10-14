//! FFmpeg capability detection from the tool's own listings (`-encoders`, `-hwaccels`,
//! `-filters`, `-version`), including whether NVENC actually works on this machine.

use sr_media::runner::{CommandRunner, CommandSpec};
use std::collections::BTreeSet;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FfmpegCaps {
    pub version: Option<String>,
    pub encoders: BTreeSet<String>,
    pub hwaccels: BTreeSet<String>,
    pub filters: BTreeSet<String>,
    /// A one-frame NVENC test encode succeeded (listing alone does not prove a usable GPU).
    pub nvenc_usable: bool,
}

/// Which H.264 encoder a job uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoder {
    Libx264,
    H264Nvenc,
}

impl Encoder {
    pub fn name(self) -> &'static str {
        match self {
            Self::Libx264 => "libx264",
            Self::H264Nvenc => "h264_nvenc",
        }
    }
}

impl FfmpegCaps {
    pub fn has_encoder(&self, name: &str) -> bool {
        self.encoders.contains(name)
    }
    pub fn has_filter(&self, name: &str) -> bool {
        self.filters.contains(name)
    }
    /// `h264_nvenc` is compiled in, the CUDA hwaccel is present and the test encode worked.
    pub fn nvenc_available(&self) -> bool {
        self.has_encoder("h264_nvenc") && self.hwaccels.contains("cuda") && self.nvenc_usable
    }
    /// Pick the encoder: NVENC when usable and not forced off, else libx264.
    pub fn choose(&self, force_cpu: bool) -> Option<Encoder> {
        if !force_cpu && self.nvenc_available() {
            Some(Encoder::H264Nvenc)
        } else if self.has_encoder("libx264") {
            Some(Encoder::Libx264)
        } else {
            None
        }
    }
    /// The zscale + tonemap chain (HDR10/HLG to SDR) can run.
    pub fn can_tonemap(&self) -> bool {
        self.has_filter("zscale") && self.has_filter("tonemap")
    }
    pub fn has_libplacebo(&self) -> bool {
        self.has_filter("libplacebo")
    }
}

/// Parse `ffmpeg -encoders`: entries are ` V....D name   description` after a `------` line.
pub fn parse_encoders(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut in_list = false;
    for line in text.lines() {
        if line.trim_start().starts_with("------") {
            in_list = true;
            continue;
        }
        if !in_list {
            continue;
        }
        let mut it = line.split_whitespace();
        if let (Some(flags), Some(name)) = (it.next(), it.next())
            && flags.len() == 6
            && flags.chars().all(|c| c.is_ascii_alphabetic() || c == '.')
            && is_ident(name)
        {
            out.insert(name.to_string());
        }
    }
    out
}

/// Parse `ffmpeg -hwaccels`: a header line then one method per line.
pub fn parse_hwaccels(text: &str) -> BTreeSet<String> {
    text.lines()
        .skip_while(|l| !l.starts_with("Hardware acceleration methods"))
        .skip(1)
        .map(str::trim)
        .filter(|l| is_ident(l))
        .map(String::from)
        .collect()
}

/// Parse `ffmpeg -filters`: ` T.. name   V->V   description` after a `------` line.
pub fn parse_filters(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut in_list = false;
    for line in text.lines() {
        if line.trim_start().starts_with("------") {
            in_list = true;
            continue;
        }
        if !in_list {
            continue;
        }
        let mut it = line.split_whitespace();
        if let (Some(flags), Some(name)) = (it.next(), it.next())
            && flags.len() == 3
            && is_ident(name)
        {
            out.insert(name.to_string());
        }
    }
    out
}

/// `ffmpeg version 7.0.1-static https://...` to `7.0.1-static`.
pub fn parse_version(text: &str) -> Option<String> {
    let first = text.lines().next()?;
    let rest = first.strip_prefix("ffmpeg version ")?;
    let v: String = rest.split_whitespace().next()?.chars().take(64).collect();
    (!v.is_empty()).then_some(v)
}

fn is_ident(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn listing(runner: &dyn CommandRunner, ffmpeg: &str, arg: &str) -> String {
    let spec = CommandSpec::new(ffmpeg, vec!["-hide_banner".into(), arg.into()])
        .with_timeout(Duration::from_secs(30))
        .with_max_stdout(2 * 1024 * 1024);
    runner
        .run(&spec)
        .ok()
        .filter(|o| o.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// The argument vector of the NVENC smoke test: one black frame through `h264_nvenc`.
pub fn nvenc_test_args() -> Vec<String> {
    [
        "-hide_banner",
        "-nostdin",
        "-loglevel",
        "error",
        "-f",
        "lavfi",
        "-i",
        "color=c=black:s=256x144:r=1:d=1",
        "-frames:v",
        "1",
        "-c:v",
        "h264_nvenc",
        "-f",
        "null",
        "-",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect()
}

/// Probe the local ffmpeg. Missing or broken ffmpeg yields empty capabilities.
pub fn detect(runner: &dyn CommandRunner, ffmpeg: &str) -> FfmpegCaps {
    let mut caps = FfmpegCaps {
        version: parse_version(&listing(runner, ffmpeg, "-version")),
        encoders: parse_encoders(&listing(runner, ffmpeg, "-encoders")),
        hwaccels: parse_hwaccels(&listing(runner, ffmpeg, "-hwaccels")),
        filters: parse_filters(&listing(runner, ffmpeg, "-filters")),
        nvenc_usable: false,
    };
    if caps.has_encoder("h264_nvenc") && caps.hwaccels.contains("cuda") {
        let spec =
            CommandSpec::new(ffmpeg, nvenc_test_args()).with_timeout(Duration::from_secs(30));
        caps.nvenc_usable = runner.run(&spec).is_ok_and(|o| o.success());
    }
    caps
}

/// Does this stderr look like the GPU encoder (not the input) failed? Then a retry on libx264
/// is worthwhile.
pub fn looks_like_gpu_failure(stderr: &str) -> bool {
    const MARKERS: &[&str] = &[
        "OpenEncodeSessionEx failed",
        "No NVENC capable devices found",
        "Cannot load libcuda",
        "Cannot load nvcuda",
        "CUDA_ERROR_",
        "Failed to create CUDA",
        "Driver does not support the required nvenc API version",
        "Cannot init CUDA",
    ];
    let lower = stderr.to_ascii_lowercase();
    MARKERS
        .iter()
        .any(|m| lower.contains(&m.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_media::runner::{FakeResult, FakeRunner};

    pub const ENCODERS: &str = "Encoders:
 V..... = Video
 A..... = Audio
 S..... = Subtitle
 .F.... = Frame-level multithreading
 ..S... = Slice-level multithreading
 ...X.. = Codec is experimental
 ....B. = Supports draw_horiz_band
 .....D = Supports direct rendering method 1
 ------
 V....D libx264              libx264 H.264 / AVC / MPEG-4 AVC / MPEG-4 part 10 (codec h264)
 V....D libx264rgb           libx264 H.264 / AVC / MPEG-4 AVC / MPEG-4 part 10 RGB (codec h264)
 V....D h264_nvenc           NVIDIA NVENC H.264 encoder (codec h264)
 V....D hevc_nvenc           NVIDIA NVENC hevc encoder (codec hevc)
 A....D aac                  AAC (Advanced Audio Coding)
 S..... ass                  ASS (Advanced SSA) subtitle (codec ass)
 S..... webvtt               WebVTT subtitle
";

    pub const HWACCELS: &str = "Hardware acceleration methods:
cuda
vaapi
vdpau
vulkan
";

    pub const FILTERS: &str = "Filters:
  T.. = Timeline support
  .S. = Slice threading
  ..C = Command support
  A = Audio input/output
  V = Video input/output
  N = Dynamic number and/or type of input/output
  | = Source or sink filter
 ------
 ... aresample         A->A       Resample audio data.
 T.. yadif             V->V       Deinterlace the input image.
 ... zscale            V->V       Apply resizing, colorspace and bit depth conversion.
 ... tonemap           V->V       Conversion to/from different dynamic ranges.
 .S. scale             V->V       Scale the input video size and/or convert the image format.
 ... tile              V->V       Tile several successive frames together.
";

    fn runner(encoders: &str, nvenc_ok: bool) -> FakeRunner {
        let encoders = encoders.to_string();
        FakeRunner::new(move |spec| {
            let a = spec.args.join(" ");
            if a.contains("-encoders") {
                FakeResult::ok(encoders.clone())
            } else if a.contains("-hwaccels") {
                FakeResult::ok(HWACCELS)
            } else if a.contains("-filters") {
                FakeResult::ok(FILTERS)
            } else if a.contains("-version") {
                FakeResult::ok(
                    "ffmpeg version 7.0.1-static https://johnvansickle.com/ffmpeg/\nbuilt with gcc 8\n",
                )
            } else if a.contains("h264_nvenc") {
                if nvenc_ok {
                    FakeResult::ok("")
                } else {
                    FakeResult::fail(
                        1,
                        "[h264_nvenc @ 0x1] OpenEncodeSessionEx failed: unsupported device (2)",
                    )
                }
            } else {
                FakeResult::fail(1, "unexpected")
            }
        })
    }

    #[test]
    fn parses_listings() {
        let enc = parse_encoders(ENCODERS);
        assert!(enc.contains("libx264") && enc.contains("h264_nvenc") && enc.contains("aac"));
        assert!(enc.contains("libx264rgb"));
        // Legend lines above the separator are not encoders.
        assert!(!enc.contains("Video") && !enc.contains("="));
        let hw = parse_hwaccels(HWACCELS);
        assert_eq!(
            hw.iter().map(String::as_str).collect::<Vec<_>>(),
            ["cuda", "vaapi", "vdpau", "vulkan"]
        );
        let f = parse_filters(FILTERS);
        assert!(f.contains("zscale") && f.contains("tonemap") && f.contains("yadif"));
        assert!(!f.contains("libplacebo"));
        assert_eq!(
            parse_version("ffmpeg version 7.0.1-static https://x\n"),
            Some("7.0.1-static".into())
        );
        assert_eq!(parse_version("garbage"), None);
    }

    #[test]
    fn nvenc_detected_and_chosen() {
        let r = runner(ENCODERS, true);
        let caps = detect(&r, "ffmpeg");
        assert!(caps.nvenc_available());
        assert!(caps.can_tonemap());
        assert!(!caps.has_libplacebo());
        assert_eq!(caps.version.as_deref(), Some("7.0.1-static"));
        assert_eq!(caps.choose(false), Some(Encoder::H264Nvenc));
        assert_eq!(caps.choose(true), Some(Encoder::Libx264));
        // The smoke test used an argument vector, never a shell string.
        let test = r
            .calls()
            .into_iter()
            .find(|c| c.args.contains(&"h264_nvenc".to_string()))
            .unwrap();
        assert_eq!(test.program, "ffmpeg");
        assert!(test.args.contains(&"lavfi".to_string()));
    }

    #[test]
    fn listed_but_broken_nvenc_falls_back() {
        let caps = detect(&runner(ENCODERS, false), "ffmpeg");
        assert!(caps.has_encoder("h264_nvenc"));
        assert!(!caps.nvenc_available());
        assert_eq!(caps.choose(false), Some(Encoder::Libx264));
    }

    #[test]
    fn no_nvenc_no_test_run() {
        let cpu_only = ENCODERS.replace(
            " V....D h264_nvenc           NVIDIA NVENC H.264 encoder (codec h264)\n",
            "",
        );
        let r = runner(&cpu_only, true);
        let caps = detect(&r, "ffmpeg");
        assert!(!caps.has_encoder("h264_nvenc"));
        assert_eq!(caps.choose(false), Some(Encoder::Libx264));
        assert!(
            r.calls()
                .iter()
                .all(|c| !c.args.contains(&"h264_nvenc".to_string()))
        );
    }

    #[test]
    fn missing_ffmpeg_yields_nothing() {
        let r = FakeRunner::new(|_| FakeResult::fail(127, "not found"));
        let caps = detect(&r, "ffmpeg");
        assert_eq!(caps, FfmpegCaps::default());
        assert_eq!(caps.choose(false), None);
    }

    #[test]
    fn gpu_failure_heuristic() {
        assert!(looks_like_gpu_failure(
            "[h264_nvenc @ 0x1] OpenEncodeSessionEx failed: out of memory (10)"
        ));
        assert!(looks_like_gpu_failure(
            "Cannot load libcuda.so.1\nError initializing output stream"
        ));
        assert!(!looks_like_gpu_failure(
            "Invalid data found when processing input"
        ));
        assert!(!looks_like_gpu_failure("moov atom not found"));
    }
}
