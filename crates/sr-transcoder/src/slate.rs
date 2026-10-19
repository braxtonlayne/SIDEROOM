//! The slate source (TV-HLS-003): ffmpeg `lavfi` arguments that synthesise a short colour-bars loop with a
//! tone, no input file and nothing downloaded. The resulting MP4 goes through the ordinary transcode job
//! (short segments, every rung) and is then configured as the TV slate asset, so off-air time plays the
//! bars instead of answering 404 (`sr_tv::TvService::with_slate`).

use crate::args::BuildError;
use std::path::Path;

/// Which test picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlatePicture {
    /// SMPTE colour bars (`smptebars`).
    Bars,
    /// ffmpeg's moving test pattern (`testsrc2`), shows that the stream is alive.
    Moving,
}

impl SlatePicture {
    fn lavfi(self, w: u32, h: u32, fps: u32, secs: u32) -> String {
        let name = match self {
            Self::Bars => "smptebars",
            Self::Moving => "testsrc2",
        };
        format!("{name}=size={w}x{h}:rate={fps}:duration={secs}")
    }
}

/// Arguments for ffmpeg to write the slate source MP4 to `out` (absolute or relative, checked): H.264 High
/// with a keyframe every two seconds and a 440 Hz AAC tone at -20 dB, `secs` long. The size must be even and sane.
pub fn slate_source_args(
    picture: SlatePicture,
    out: &str,
    (w, h): (u32, u32),
    fps: u32,
    secs: u32,
) -> Result<Vec<String>, BuildError> {
    if out.is_empty()
        || out.starts_with('-')
        || Path::new(out)
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(BuildError::UnsafePath(out.to_owned()));
    }
    if w == 0 || h == 0 || w % 2 != 0 || h % 2 != 0 || w > 3840 || h > 2160 {
        return Err(BuildError::UnsafeInput);
    }
    if fps == 0 || fps > 60 || secs == 0 || secs > 600 {
        return Err(BuildError::UnsafeInput);
    }
    let s = |v: &[&str]| v.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>();
    let mut a = s(&["-hide_banner", "-nostdin", "-y", "-f", "lavfi", "-i"]);
    a.push(picture.lavfi(w, h, fps, secs));
    a.extend(s(&["-f", "lavfi", "-i"]));
    a.push(format!(
        "sine=frequency=440:sample_rate=48000:duration={secs}"
    ));
    a.extend(s(&[
        "-c:v",
        "libx264",
        "-profile:v",
        "high",
        "-pix_fmt",
        "yuv420p",
    ]));
    a.extend(["-g".to_owned(), (fps * 2).to_string()]);
    a.extend(s(&[
        "-sc_threshold",
        "0",
        // D-266: the tone sits around -20 dBFS, not at full scale.
        "-af",
        "volume=-20dB",
        "-c:a",
        "aac",
        "-b:a",
        "96k",
        "-ac",
        "2",
        "-shortest",
        "-movflags",
        "+faststart",
    ]));
    a.push(out.to_owned());
    Ok(a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bars_args_are_pure_lavfi_with_no_input_file() {
        let a = slate_source_args(SlatePicture::Bars, "slate.mp4", (1280, 720), 25, 12).unwrap();
        assert!(a.contains(&"smptebars=size=1280x720:rate=25:duration=12".to_owned()));
        assert!(a.contains(&"sine=frequency=440:sample_rate=48000:duration=12".to_owned()));
        assert_eq!(a.iter().filter(|x| *x == "lavfi").count(), 2);
        assert_eq!(a.last().unwrap(), "slate.mp4");
        let g = a.iter().position(|x| x == "-g").unwrap();
        assert_eq!(a[g + 1], "50", "a keyframe every two seconds");
        let af = a.iter().position(|x| x == "-af").unwrap();
        assert_eq!(a[af + 1], "volume=-20dB", "tone at -20 dBFS (D-266)");
    }

    #[test]
    fn the_moving_pattern_uses_testsrc2() {
        let a = slate_source_args(SlatePicture::Moving, "o.mp4", (640, 360), 30, 6).unwrap();
        assert!(a.iter().any(|x| x.starts_with("testsrc2=size=640x360")));
    }

    #[test]
    fn unsafe_output_and_odd_sizes_are_refused() {
        let ok =
            |out: &str, sz, fps, secs| slate_source_args(SlatePicture::Bars, out, sz, fps, secs);
        assert!(matches!(
            ok("../x.mp4", (640, 360), 25, 6),
            Err(BuildError::UnsafePath(_))
        ));
        assert!(matches!(
            ok("-evil", (640, 360), 25, 6),
            Err(BuildError::UnsafePath(_))
        ));
        assert!(ok("a.mp4", (641, 360), 25, 6).is_err());
        assert!(ok("a.mp4", (640, 360), 0, 6).is_err());
        assert!(ok("a.mp4", (640, 360), 25, 0).is_err());
    }
}
