//! LNG-MSG-019: duration and waveform of a voice message, computed server-side.
//!
//! A voice message is one `audio/ogg` file carrying Opus. symphonia has no Opus decoder, so
//! nothing is decoded: the Ogg container is walked in pure Rust (pages, then packets by
//! lacing). The duration is the last page's granule position minus the `OpusHead` pre-skip,
//! at the fixed 48 kHz Opus clock (RFC 7845). The waveform is an envelope taken from the
//! Opus packet sizes: encoders spend more bytes on louder frames and almost none on silence,
//! so the largest packet per bucket, rescaled to 0..=255, is a faithful picture of loudness
//! without decoding a sample.

/// Upper bound on the waveform length (bytes), as in Discord's voice-message payload.
pub const MAX_WAVEFORM_BYTES: usize = 256;

/// What the server learned about a voice file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceMeta {
    /// Length in milliseconds.
    pub duration_ms: u32,
    /// Amplitude envelope, 1 to [`MAX_WAVEFORM_BYTES`] bytes.
    pub waveform: Vec<u8>,
}

/// K-01: whether the client's own recording can stand in for the packet-size envelope.
///
/// The measured duration stays authoritative. A declared waveform of 1 to
/// [`MAX_WAVEFORM_BYTES`] decoded bytes is kept when the declared duration is within 2 percent
/// or 100 ms (the larger) of the measured one: the client then saw the same audio, and its
/// envelope is taken from real samples, so it is truer than the packet-size estimate.
#[must_use]
pub fn declared_waveform_is_consistent(
    measured_ms: u32,
    declared_secs: Option<f64>,
    declared_len: usize,
) -> bool {
    if declared_len == 0 || declared_len > MAX_WAVEFORM_BYTES {
        return false;
    }
    let Some(d) = declared_secs.filter(|d| d.is_finite() && *d > 0.0) else {
        return false;
    };
    let measured = f64::from(measured_ms);
    let tolerance = (measured * 0.02).max(100.0);
    (d * 1000.0 - measured).abs() <= tolerance
}

/// Opus always counts granule positions at 48 kHz.
const OPUS_RATE: u64 = 48_000;

/// Analyze an Ogg Opus file. `None` when it is not a well-formed Ogg Opus stream with audio.
#[must_use]
pub fn analyze_ogg_opus(bytes: &[u8]) -> Option<VoiceMeta> {
    let mut pos = 0usize;
    let mut serial: Option<u32> = None;
    let mut pre_skip: u64 = 0;
    let mut last_granule: Option<u64> = None;
    let mut sizes: Vec<u32> = Vec::new();
    // Bytes of the packet being assembled (kept only for the OpusHead packet).
    let mut head_buf: Vec<u8> = Vec::new();
    let mut partial: u32 = 0;
    let mut packet_no = 0usize;
    while pos + 27 <= bytes.len() {
        if &bytes[pos..pos + 4] != b"OggS" || bytes[pos + 4] != 0 {
            return None;
        }
        let granule = i64::from_le_bytes(bytes[pos + 6..pos + 14].try_into().ok()?);
        let page_serial = u32::from_le_bytes(bytes[pos + 14..pos + 18].try_into().ok()?);
        let nsegs = usize::from(bytes[pos + 26]);
        let table = bytes.get(pos + 27..pos + 27 + nsegs)?;
        let data_len: usize = table.iter().map(|&s| usize::from(s)).sum();
        let body_start = pos + 27 + nsegs;
        let body = bytes.get(body_start..body_start + data_len)?;
        pos = body_start + data_len;
        if *serial.get_or_insert(page_serial) != page_serial {
            continue; // another logical stream: ignore it
        }
        let mut at = 0usize;
        for &seg in table {
            let seg_len = usize::from(seg);
            if packet_no == 0 {
                head_buf.extend_from_slice(&body[at..at + seg_len]);
            }
            at += seg_len;
            partial += u32::from(seg);
            if seg == 255 {
                continue;
            }
            // A packet ends here.
            if packet_no == 0 {
                if head_buf.len() < 19 || &head_buf[..8] != b"OpusHead" {
                    return None;
                }
                pre_skip = u64::from(u16::from_le_bytes([head_buf[10], head_buf[11]]));
            } else if packet_no >= 2 {
                sizes.push(partial);
            }
            packet_no += 1;
            partial = 0;
        }
        if granule >= 0 && !table.is_empty() {
            last_granule = Some(granule.unsigned_abs());
        }
    }
    if pos != bytes.len() || sizes.is_empty() {
        return None;
    }
    let samples = last_granule?.saturating_sub(pre_skip);
    if samples == 0 {
        return None;
    }
    let duration_ms = u32::try_from(samples * 1000 / OPUS_RATE).ok()?;
    Some(VoiceMeta {
        duration_ms,
        waveform: envelope(&sizes),
    })
}

/// Bucket packet sizes into at most [`MAX_WAVEFORM_BYTES`] bytes (the largest packet of each
/// bucket) and rescale so the smallest maps to 0 and the largest to 255.
fn envelope(sizes: &[u32]) -> Vec<u8> {
    let buckets = sizes.len().min(MAX_WAVEFORM_BYTES);
    let peaks: Vec<u32> = (0..buckets)
        .map(|b| {
            let lo = b * sizes.len() / buckets;
            let hi = ((b + 1) * sizes.len() / buckets).max(lo + 1);
            sizes[lo..hi].iter().copied().max().unwrap_or(0)
        })
        .collect();
    let min = peaks.iter().copied().min().unwrap_or(0);
    let max = peaks.iter().copied().max().unwrap_or(0);
    if max == min {
        return vec![128; buckets];
    }
    peaks
        .iter()
        .map(|&p| u8::try_from(u64::from(p - min) * 255 / u64::from(max - min)).unwrap_or(255))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One Ogg page holding complete packets.
    fn page(serial: u32, granule: i64, packets: &[Vec<u8>]) -> Vec<u8> {
        let mut table = Vec::new();
        let mut body = Vec::new();
        for p in packets {
            let mut n = p.len();
            while n >= 255 {
                table.push(255);
                n -= 255;
            }
            table.push(u8::try_from(n).unwrap());
            body.extend_from_slice(p);
        }
        let mut out = b"OggS".to_vec();
        out.push(0);
        out.push(0);
        out.extend_from_slice(&granule.to_le_bytes());
        out.extend_from_slice(&serial.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.push(u8::try_from(table.len()).unwrap());
        out.extend(table);
        out.extend(body);
        out
    }

    fn head(pre_skip: u16) -> Vec<u8> {
        let mut h = b"OpusHead".to_vec();
        h.extend_from_slice(&[1, 1]);
        h.extend_from_slice(&pre_skip.to_le_bytes());
        h.extend_from_slice(&48_000u32.to_le_bytes());
        h.extend_from_slice(&[0, 0, 0]);
        h
    }

    fn file(sizes: &[usize], granule: i64) -> Vec<u8> {
        let mut f = page(7, 0, &[head(312)]);
        f.extend(page(7, 0, &[b"OpusTags....".to_vec()]));
        let audio: Vec<Vec<u8>> = sizes.iter().map(|&n| vec![1u8; n]).collect();
        let chunks: Vec<&[Vec<u8>]> = audio.chunks(50).collect();
        for (i, c) in chunks.iter().enumerate() {
            let g = if i + 1 == chunks.len() { granule } else { 1 };
            f.extend(page(7, g, c));
        }
        f
    }

    #[test]
    fn duration_comes_from_the_last_granule_minus_pre_skip() {
        // 2.0 s of audio plus the 312-sample pre-skip.
        let m = analyze_ogg_opus(&file(&[10, 80, 300, 5], 96_000 + 312)).unwrap();
        assert_eq!(m.duration_ms, 2000);
        assert_eq!(m.waveform.len(), 4);
    }

    #[test]
    fn waveform_scales_smallest_to_zero_and_largest_to_max() {
        let m = analyze_ogg_opus(&file(&[10, 60, 110], 48_312)).unwrap();
        assert_eq!(m.waveform, vec![0, 127, 255]);
    }

    #[test]
    fn long_files_are_bucketed_to_the_cap() {
        let sizes: Vec<usize> = (0..1000).map(|i| 5 + i % 40).collect();
        let m = analyze_ogg_opus(&file(&sizes, 48_000 * 20 + 312)).unwrap();
        assert_eq!(m.waveform.len(), MAX_WAVEFORM_BYTES);
        assert_eq!(m.duration_ms, 20_000);
    }

    #[test]
    fn packets_longer_than_a_lacing_run_are_counted_whole() {
        let m = analyze_ogg_opus(&file(&[600, 20], 48_312)).unwrap();
        assert_eq!(m.waveform, vec![255, 0]);
    }

    #[test]
    fn garbage_and_non_opus_streams_are_rejected() {
        assert!(analyze_ogg_opus(b"").is_none());
        assert!(analyze_ogg_opus(b"not an ogg file at all, just text").is_none());
        let vorbis = page(1, 0, &[b"\x01vorbis-identification-header".to_vec()]);
        assert!(analyze_ogg_opus(&vorbis).is_none());
        let mut cut = file(&[10, 20], 48_312);
        cut.truncate(cut.len() - 3);
        assert!(analyze_ogg_opus(&cut).is_none());
        // Headers only: no audio.
        let mut headers = page(7, 0, &[head(0)]);
        headers.extend(page(7, 0, &[b"OpusTags".to_vec()]));
        assert!(analyze_ogg_opus(&headers).is_none());
    }

    #[test]
    fn declared_waveform_is_kept_only_when_consistent() {
        // Within 2 percent of 60 s (1.2 s) is kept; beyond it is not.
        assert!(declared_waveform_is_consistent(60_000, Some(61.0), 64));
        assert!(!declared_waveform_is_consistent(60_000, Some(62.0), 64));
        // Short clips get the 100 ms floor.
        assert!(declared_waveform_is_consistent(2_000, Some(2.09), 10));
        assert!(!declared_waveform_is_consistent(2_000, Some(2.2), 10));
        // Malformed lengths and missing or odd durations are replaced.
        assert!(!declared_waveform_is_consistent(2_000, Some(2.0), 0));
        assert!(!declared_waveform_is_consistent(2_000, Some(2.0), 257));
        assert!(declared_waveform_is_consistent(2_000, Some(2.0), 256));
        assert!(!declared_waveform_is_consistent(2_000, None, 10));
        assert!(!declared_waveform_is_consistent(2_000, Some(f64::NAN), 10));
    }
}
