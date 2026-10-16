//! Codec-aware frame layout (VOX-E2E-002): which bytes of an encoded frame stay in the clear.
//!
//! The browser hands the transform one encoded frame at a time (RTCEncodedAudioFrame or
//! RTCEncodedVideoFrame data, without RTP headers or payload descriptors). Some of those bytes
//! are read again after the transform, by the sender's packetizer, by the SFU and by the
//! receiver's depacketizer before the receive transform runs, so they must stay readable (as
//! in SFrame and DAVE):
//!
//! - **Opus:** nothing. The packetizer does not look inside, the SFU reads audio levels from
//!   the RTP header extension. The whole frame is encrypted.
//! - **VP8:** the uncompressed data chunk at the start of the frame (RFC 6386 section 9.1):
//!   3 bytes on an interframe, 10 on a keyframe (the extra 7 carry the start code and the
//!   dimensions). Its first bit says which one, and the depacketizer and the SFU read it to
//!   find keyframes.
//! - **H.264** (Annex B byte stream): every start code and NAL unit header byte, so the
//!   packetizer can split NAL units and everybody can read NAL types (IDR, SPS). Parameter
//!   sets (SPS 7, PPS 8) and access unit delimiters (9) stay whole: they hold no picture data,
//!   and the receiving packet buffer parses them before the receive transform runs. Slices
//!   (types 1 and 5) keep the start of the slice header through `pic_parameter_set_id`, which
//!   that packet buffer also reads. Everything else (slice data, SEI, ...) is encrypted.
//!
//! The ranges are a function of the clear bytes only, so the receiver, which gets the same
//! clear bytes, derives the same ranges from the encrypted frame. For H.264 that also needs
//! the ciphertext to contain no start code; [`has_start_code_emulation`] checks the output and
//! the encryptor retries with the next counter if it does (as DAVE does).

use std::ops::Range;

/// The codecs the SFU negotiates (Opus, VP8, H.264). Anything else is refused: an unknown
/// codec is never sent in the clear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Codec {
    Opus,
    Vp8,
    H264,
}

impl Codec {
    /// Stable id, bound into the authenticated data of every frame.
    pub fn id(self) -> u8 {
        match self {
            Codec::Opus => 1,
            Codec::Vp8 => 2,
            Codec::H264 => 3,
        }
    }

    pub fn from_id(id: u8) -> Option<Codec> {
        match id {
            1 => Some(Codec::Opus),
            2 => Some(Codec::Vp8),
            3 => Some(Codec::H264),
            _ => None,
        }
    }

    /// From a WebRTC MIME type (`audio/opus`, `video/VP8`, `video/H264`), case-insensitive.
    pub fn from_mime(mime: &str) -> Option<Codec> {
        let m = mime.trim().to_ascii_lowercase();
        match m.as_str() {
            "audio/opus" => Some(Codec::Opus),
            "video/vp8" => Some(Codec::Vp8),
            "video/h264" => Some(Codec::H264),
            _ => None,
        }
    }
}

/// Clear bytes of a VP8 interframe and keyframe.
pub const VP8_DELTA_CLEAR: usize = 3;
pub const VP8_KEY_CLEAR: usize = 10;
/// Most slice-header bytes an H.264 slice keeps in the clear. Three Exp-Golomb values fit in
/// far fewer; a header that needs more is malformed and the parse stops there.
pub const H264_SLICE_PREFIX_MAX: usize = 32;

/// The clear ranges of `frame`: sorted, disjoint, adjacent ranges merged. The complement is
/// what gets encrypted.
pub fn clear_ranges(codec: Codec, frame: &[u8]) -> Vec<Range<usize>> {
    let raw = match codec {
        Codec::Opus => Vec::new(),
        Codec::Vp8 => vp8(frame),
        Codec::H264 => h264(frame),
    };
    normalize(raw)
}

/// The complement of `clear` within `0..len`.
pub fn encrypted_ranges(clear: &[Range<usize>], len: usize) -> Vec<Range<usize>> {
    let mut out = Vec::with_capacity(clear.len() + 1);
    let mut at = 0;
    for r in clear {
        if r.start > at {
            out.push(at..r.start);
        }
        at = r.end;
    }
    if at < len {
        out.push(at..len);
    }
    out
}

fn normalize(mut v: Vec<Range<usize>>) -> Vec<Range<usize>> {
    v.retain(|r| r.start < r.end);
    v.sort_by_key(|r| r.start);
    let mut out: Vec<Range<usize>> = Vec::with_capacity(v.len());
    for r in v {
        match out.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => out.push(r),
        }
    }
    out
}

fn vp8(frame: &[u8]) -> Vec<Range<usize>> {
    let Some(first) = frame.first() else {
        return Vec::new();
    };
    // RFC 6386 9.1: bit 0 of the frame tag is 0 on a keyframe.
    let n = if first & 1 == 0 {
        VP8_KEY_CLEAR
    } else {
        VP8_DELTA_CLEAR
    };
    let end = n.min(frame.len());
    std::iter::once(0..end).collect()
}

/// `(start code start, NAL unit start)` for every Annex B start code. A zero byte right before
/// `00 00 01` counts as the 4-byte form.
pub fn h264_start_codes(buf: &[u8]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 3 <= buf.len() {
        if buf[i] == 0 && buf[i + 1] == 0 && buf[i + 2] == 1 {
            let start = if i > 0 && buf[i - 1] == 0 { i - 1 } else { i };
            out.push((start, i + 3));
            i += 3;
        } else {
            i += 1;
        }
    }
    out
}

fn h264(frame: &[u8]) -> Vec<Range<usize>> {
    let codes = h264_start_codes(frame);
    let mut out = Vec::with_capacity(codes.len());
    for (k, &(sc, nal)) in codes.iter().enumerate() {
        let end = codes.get(k + 1).map_or(frame.len(), |n| n.0);
        if nal >= end {
            out.push(sc..end.max(sc));
            continue;
        }
        let ty = frame[nal] & 0x1f;
        let clear_end = match ty {
            7..=9 => end,
            1 | 5 => nal + 1 + slice_prefix_len(&frame[nal + 1..end]),
            _ => nal + 1,
        };
        out.push(sc..clear_end);
    }
    out
}

/// Bits of an escaped NAL unit payload, skipping emulation prevention bytes (`00 00 03`).
struct BitReader<'a> {
    buf: &'a [u8],
    /// Next byte to load.
    pos: usize,
    cur: u8,
    /// Bits left in `cur`.
    left: u8,
    zeros: u8,
}

impl BitReader<'_> {
    fn bit(&mut self) -> Option<u8> {
        if self.left == 0 {
            if self.pos >= self.buf.len() || self.pos >= H264_SLICE_PREFIX_MAX {
                return None;
            }
            let mut b = self.buf[self.pos];
            self.pos += 1;
            if self.zeros >= 2 && b == 3 {
                // Emulation prevention byte: not part of the RBSP.
                self.zeros = 0;
                if self.pos >= self.buf.len() || self.pos >= H264_SLICE_PREFIX_MAX {
                    return None;
                }
                b = self.buf[self.pos];
                self.pos += 1;
            }
            self.zeros = if b == 0 { self.zeros + 1 } else { 0 };
            self.cur = b;
            self.left = 8;
        }
        self.left -= 1;
        Some((self.cur >> self.left) & 1)
    }

    /// Unsigned Exp-Golomb, at most 32 bits of value.
    fn ue(&mut self) -> Option<u32> {
        let mut zeros = 0u32;
        while self.bit()? == 0 {
            zeros += 1;
            if zeros > 31 {
                return None;
            }
        }
        let mut v: u64 = 1;
        for _ in 0..zeros {
            v = (v << 1) | u64::from(self.bit()?);
        }
        u32::try_from(v - 1).ok()
    }
}

/// How many escaped bytes after the NAL header the slice header needs through
/// `pic_parameter_set_id` (`first_mb_in_slice`, `slice_type`, `pic_parameter_set_id`, all
/// ue(v)). A malformed header keeps exactly the bytes the parse read before giving up, so the
/// receiver, reading the same clear bytes, stops at the same place.
pub fn slice_prefix_len(payload: &[u8]) -> usize {
    let mut r = BitReader {
        buf: payload,
        pos: 0,
        cur: 0,
        left: 0,
        zeros: 0,
    };
    for _ in 0..3 {
        if r.ue().is_none() {
            break;
        }
    }
    r.pos
}

/// Whether `out` (an encrypted H.264 frame with its trailer) contains `00 00 0x` (x <= 3) in
/// any 3-byte window that is not wholly inside one clear range. Such a window would add,
/// move or swallow a start code, or look like an emulation prevention byte.
pub fn has_start_code_emulation(out: &[u8], clear: &[Range<usize>]) -> bool {
    if out.len() < 3 {
        return false;
    }
    let mut ci = 0;
    for i in 0..out.len() - 2 {
        if out[i] != 0 || out[i + 1] != 0 || out[i + 2] > 3 {
            continue;
        }
        while ci < clear.len() && clear[ci].end <= i {
            ci += 1;
        }
        let inside = clear
            .get(ci)
            .is_some_and(|r| r.start <= i && i + 3 <= r.end);
        if !inside {
            return true;
        }
    }
    false
}

#[cfg(test)]
#[allow(clippy::single_range_in_vec_init)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn mime_types() {
        assert_eq!(Codec::from_mime("audio/opus"), Some(Codec::Opus));
        assert_eq!(Codec::from_mime("video/VP8"), Some(Codec::Vp8));
        assert_eq!(Codec::from_mime("video/H264"), Some(Codec::H264));
        assert_eq!(Codec::from_mime("video/AV1"), None);
        for c in [Codec::Opus, Codec::Vp8, Codec::H264] {
            assert_eq!(Codec::from_id(c.id()), Some(c));
        }
        assert_eq!(Codec::from_id(0), None);
    }

    #[test]
    fn opus_is_all_encrypted() {
        assert!(clear_ranges(Codec::Opus, &[1, 2, 3]).is_empty());
        assert_eq!(encrypted_ranges(&[], 3), vec![0..3]);
    }

    #[test]
    fn vp8_key_and_delta() {
        let key = [0x10u8; 40]; // bit 0 clear: keyframe
        assert_eq!(clear_ranges(Codec::Vp8, &key), vec![0..10]);
        let delta = [0x11u8; 40];
        assert_eq!(clear_ranges(Codec::Vp8, &delta), vec![0..3]);
        assert_eq!(clear_ranges(Codec::Vp8, &[0x10, 1]), vec![0..2]);
        assert!(clear_ranges(Codec::Vp8, &[]).is_empty());
        assert_eq!(encrypted_ranges(&[0..3], 40), vec![3..40]);
    }

    /// An IDR access unit as the encoder emits it: SPS, PPS, IDR slice.
    pub(crate) fn idr_frame() -> Vec<u8> {
        let mut f = vec![0, 0, 0, 1, 0x67, 0x42, 0xc0, 0x1f, 0xda, 0x01];
        f.extend_from_slice(&[0, 0, 0, 1, 0x68, 0xce, 0x3c, 0x80]);
        // IDR slice: first_mb=0 ("1"), slice_type=7 ("0001000"), pps_id=0 ("1") -> 1000 1000 1...
        f.extend_from_slice(&[0, 0, 1, 0x65, 0x88, 0x84, 0x21, 0x7f, 0x55, 0xaa, 0x13]);
        f
    }

    #[test]
    fn h264_layout() {
        let f = idr_frame();
        let codes = h264_start_codes(&f);
        assert_eq!(codes, vec![(0, 4), (10, 14), (18, 21)]);
        // SPS and PPS whole, then the slice's start code, header and 2 header bytes
        // (9 bits of ue values span bytes 0x88 and 0x84).
        assert_eq!(clear_ranges(Codec::H264, &f), vec![0..24]);
        assert_eq!(encrypted_ranges(&[0..24], f.len()), vec![24..f.len()]);
    }

    #[test]
    fn h264_non_slice_keeps_header_only() {
        // SEI (6) then a non-IDR slice (1) whose header is "1 1 1" -> one byte.
        let f = [0, 0, 1, 0x06, 5, 4, 3, 0, 0, 1, 0x41, 0xe0, 9, 9];
        assert_eq!(clear_ranges(Codec::H264, &f), vec![0..4, 7..12]);
    }

    #[test]
    fn h264_without_start_code_encrypts_everything() {
        assert!(clear_ranges(Codec::H264, &[9, 9, 9, 9]).is_empty());
    }

    #[test]
    fn slice_prefix_skips_emulation_prevention() {
        // 23 leading zero bits then a 1: the 00 00 03 escape is in the middle.
        let p = [0x00, 0x00, 0x03, 0x01, 0xff, 0xff, 0xff, 0xff];
        let n = slice_prefix_len(&p);
        assert!(n >= 4, "{n}");
        // Truncated: the parse stops at the end.
        assert_eq!(slice_prefix_len(&[0x00]), 1);
        // Malformed (too many zeros): bounded.
        assert!(slice_prefix_len(&[0u8; 64]) <= H264_SLICE_PREFIX_MAX);
    }

    #[test]
    fn emulation_detection() {
        let clear = vec![0..4];
        assert!(!has_start_code_emulation(&[0, 0, 0, 1, 5, 6, 7], &clear));
        assert!(has_start_code_emulation(&[0, 0, 0, 1, 5, 0, 0, 2], &clear));
        // Straddling the clear/encrypted boundary counts.
        assert!(has_start_code_emulation(&[0, 0, 0, 1, 0, 0, 1], &[0..5]));
        assert!(!has_start_code_emulation(&[0, 0, 0, 1, 0, 0, 9], &[0..5]));
        assert!(!has_start_code_emulation(&[0, 0], &[]));
    }
}
