//! Container facts from the first bytes of a file (the catalog's one allowed header read, 1 MiB at most).
//!
//! No decoder and no ffprobe: only the container's signature and, for MP4, the video and audio codec tags found
//! in the `moov` box when it sits inside the bytes read. That is enough to say "web native" or "Convert to play".

/// How much of a file the indexer reads for [`sniff`].
pub const HEADER_BYTES: u64 = 1024 * 1024;

/// What the first bytes say.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Container {
    /// `mp4`, `mkv`, `webm`, `mp3`, `flac`, `ogg`, `zip`, `pdf`, ... or empty when unknown.
    pub format: &'static str,
    /// Video codec tag (`avc1`, `hvc1`, `av01`, `vp09`) when found.
    pub video: Option<String>,
    /// Audio codec tag (`mp4a`, `opus`, `ac-3`) when found.
    pub audio: Option<String>,
}

impl Container {
    /// Whether a browser plays it as is: MP4 with H.264, AV1 or VP9 and AAC, WebM, MP3, FLAC, Ogg. An MP4 whose
    /// codec tags were not found at all (the moov is past the bytes read) counts as playable, so a file is greyed
    /// only on evidence.
    pub fn plays_in_browser(&self) -> bool {
        match self.format {
            "webm" | "mp3" | "flac" | "ogg" | "wav" => true,
            "mp4" => {
                let v_ok = self
                    .video
                    .as_deref()
                    .is_none_or(|v| matches!(v, "avc1" | "avc3" | "av01" | "vp09"));
                let a_ok = self
                    .audio
                    .as_deref()
                    .is_none_or(|a| matches!(a, "mp4a" | "opus" | "fLaC"));
                v_ok && a_ok
            }
            _ => false,
        }
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Read the container from the first bytes of a file.
pub fn sniff(head: &[u8]) -> Container {
    let mut c = Container::default();
    if head.len() >= 12 && &head[4..8] == b"ftyp" {
        c.format = "mp4";
        if let Some(moov) = find(head, b"moov") {
            let body = &head[moov..];
            for tag in [
                &b"avc1"[..],
                b"avc3",
                b"hvc1",
                b"hev1",
                b"av01",
                b"vp09",
                b"mp4v",
            ] {
                if find(body, tag).is_some() {
                    c.video = Some(String::from_utf8_lossy(tag).into_owned());
                    break;
                }
            }
            for tag in [&b"mp4a"[..], b"opus", b"fLaC", b"ac-3", b"ec-3"] {
                if find(body, tag).is_some() {
                    c.audio = Some(String::from_utf8_lossy(tag).into_owned());
                    break;
                }
            }
        }
    } else if head.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        // EBML: Matroska or WebM, told apart by the DocType.
        c.format = if find(&head[..head.len().min(64)], b"webm").is_some() {
            "webm"
        } else {
            "mkv"
        };
    } else if head.starts_with(b"fLaC") {
        c.format = "flac";
    } else if head.starts_with(b"OggS") {
        c.format = "ogg";
    } else if head.starts_with(b"ID3")
        || (head.len() > 2 && head[0] == 0xFF && head[1] & 0xE0 == 0xE0)
    {
        c.format = "mp3";
    } else if head.starts_with(b"RIFF") && head.len() > 12 && &head[8..12] == b"WAVE" {
        c.format = "wav";
    } else if head.starts_with(b"PK\x03\x04") {
        c.format = "zip";
    } else if head.starts_with(b"%PDF") {
        c.format = "pdf";
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mp4(codec: &[u8], audio: &[u8]) -> Vec<u8> {
        let mut v = vec![0, 0, 0, 24];
        v.extend_from_slice(b"ftypisom");
        v.extend_from_slice(&[0; 12]);
        v.extend_from_slice(b"moov");
        v.extend_from_slice(codec);
        v.extend_from_slice(audio);
        v
    }

    #[test]
    fn h264_aac_plays_and_hevc_does_not() {
        assert!(sniff(&mp4(b"avc1", b"mp4a")).plays_in_browser());
        assert!(!sniff(&mp4(b"hvc1", b"mp4a")).plays_in_browser());
        assert!(!sniff(&mp4(b"avc1", b"ac-3")).plays_in_browser());
    }

    #[test]
    fn an_mp4_without_a_moov_in_the_bytes_is_not_greyed() {
        let mut v = vec![0, 0, 0, 24];
        v.extend_from_slice(b"ftypisom");
        v.extend_from_slice(&[0; 32]);
        let c = sniff(&v);
        assert_eq!(c.format, "mp4");
        assert!(c.plays_in_browser());
    }

    #[test]
    fn signatures() {
        assert_eq!(sniff(b"fLaC\0\0\0\0\0\0\0\0\0").format, "flac");
        assert_eq!(sniff(b"OggS\0\0\0\0\0\0\0\0\0").format, "ogg");
        assert_eq!(sniff(b"ID3\x04\0\0\0\0\0\0\0\0\0").format, "mp3");
        assert_eq!(sniff(b"%PDF-1.7 ....").format, "pdf");
        assert_eq!(sniff(b"PK\x03\x04 ....").format, "zip");
        let mut mkv = vec![0x1A, 0x45, 0xDF, 0xA3, 0x9F, 0x42, 0x82, 0x84];
        mkv.extend_from_slice(b"webm");
        assert_eq!(sniff(&mkv).format, "webm");
        mkv.truncate(8);
        mkv.extend_from_slice(b"matroska");
        let c = sniff(&mkv);
        assert_eq!(c.format, "mkv");
        assert!(!c.plays_in_browser());
        assert_eq!(sniff(b"").format, "");
    }
}
