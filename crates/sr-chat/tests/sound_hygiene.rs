//! RDY-SEC-008: the soundboard file reader over generated corpora. The sniffing and the duration measure are the
//! only checks on an uploaded sound (the bytes are never decoded), so they must never panic, never read past the
//! data, and never let arithmetic on a hostile header wrap into a short duration.
//!
//! Limits asserted are those of doc 12 section 3 "Upload hygiene" (512 KiB, 5 seconds).

use sr_chat::settings::{
    expressions::ExpressionLimits,
    soundboard::{MAX_SOUND_BYTES, MAX_SOUND_MS, SoundFormat, check_new_sound, inspect_sound},
};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

const SEED: u64 = 0x5352_5032_3153_4E44;

/// PCM WAV, mono, 16-bit, 8000 Hz: 16000 bytes per second.
fn wav(data_bytes: usize) -> Vec<u8> {
    let mut v = b"RIFF".to_vec();
    v.extend_from_slice(&((36 + data_bytes) as u32).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&[1, 0, 1, 0]);
    v.extend_from_slice(&8000u32.to_le_bytes());
    v.extend_from_slice(&16000u32.to_le_bytes());
    v.extend_from_slice(&[2, 0, 16, 0]);
    v.extend_from_slice(b"data");
    v.extend_from_slice(&(data_bytes as u32).to_le_bytes());
    v.resize(v.len() + data_bytes, 0);
    v
}

fn ogg_page(granule: u64, header_type: u8, payload: &[u8]) -> Vec<u8> {
    let mut v = b"OggS".to_vec();
    v.push(0);
    v.push(header_type);
    v.extend_from_slice(&granule.to_le_bytes());
    v.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    let mut left = payload.len();
    let mut lacing = Vec::new();
    while left >= 255 {
        lacing.push(255);
        left -= 255;
    }
    lacing.push(left as u8);
    v.push(lacing.len() as u8);
    v.extend_from_slice(&lacing);
    v.extend_from_slice(payload);
    v
}

/// An Ogg Opus stream whose last page claims `granule` samples at 48 kHz, with `pre_skip` samples of padding.
fn opus(granule: u64, pre_skip: u16) -> Vec<u8> {
    let mut head = b"OpusHead\x01\x01".to_vec();
    head.extend_from_slice(&pre_skip.to_le_bytes());
    head.extend_from_slice(&48_000u32.to_le_bytes());
    head.extend_from_slice(&[0, 0, 0]);
    let mut v = ogg_page(0, 0x02, &head);
    v.extend(ogg_page(0, 0x00, b"OpusTags\0\0\0\0\0\0\0\0"));
    v.extend(ogg_page(granule, 0x04, &[0xF8, 0xFF, 0xFE]));
    v
}

/// MPEG-1 Layer III, 128 kbps, 44.1 kHz frames of 417 bytes, `n` of them (a constant bitrate, no Xing header).
fn mp3(frames: usize) -> Vec<u8> {
    let mut v = Vec::new();
    for _ in 0..frames {
        v.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
        v.resize(v.len() + 413, 0);
    }
    v
}

fn ms(data: &[u8]) -> Option<u32> {
    inspect_sound(data).map(|i| i.duration_ms)
}

fn lim() -> ExpressionLimits {
    ExpressionLimits::default()
}

#[test]
fn size_and_duration_limits_hold_at_the_limit_and_one_over() {
    assert_eq!((MAX_SOUND_BYTES, MAX_SOUND_MS), (512 * 1024, 5_000));
    assert!(check_new_sound(&[], MAX_SOUND_BYTES, MAX_SOUND_MS, &lim()).is_ok());
    assert!(check_new_sound(&[], MAX_SOUND_BYTES + 1, MAX_SOUND_MS, &lim()).is_err());
    assert!(check_new_sound(&[], 100, MAX_SOUND_MS + 1, &lim()).is_err());
    assert!(check_new_sound(&[], 100, 0, &lim()).is_err());
}

#[test]
fn a_five_second_sound_is_accepted_and_one_more_millisecond_is_not_in_every_format() {
    // WAV: 16000 bytes a second.
    let w = wav(80_000);
    assert_eq!(
        inspect_sound(&w).map(|i| (i.format, i.duration_ms)),
        Some((SoundFormat::Wav, 5000))
    );
    assert!(check_new_sound(&[], w.len() as u64, 5000, &lim()).is_ok());
    assert_eq!(ms(&wav(80_016)), Some(5001));
    // Opus: 48000 samples a second after the pre-skip.
    assert_eq!(ms(&opus(5 * 48_000 + 312, 312)), Some(5000));
    assert_eq!(ms(&opus(5 * 48_000 + 312 + 48, 312)), Some(5001));
    // MP3 CBR: 128 kbps is 16000 bytes a second, so 80000 bytes (a whole number of frames is not needed).
    let m = mp3(192); // 80064 bytes
    assert_eq!(inspect_sound(&m).map(|i| i.format), Some(SoundFormat::Mp3));
    assert_eq!(ms(&m), Some(5004));
    assert_eq!(ms(&mp3(191)), Some(4977));
}

#[test]
fn headers_that_overflow_the_arithmetic_are_refused_never_wrapped_into_a_short_sound() {
    // samples * 1000 wraps past 2^64: this granule makes the product land on 48_000_384, which a wrapping
    // multiply would divide into a one-second sound.
    let wrap = (u128::from(u64::MAX) + 1 + 48_000_384) / 1000;
    assert_eq!((wrap * 1000) % (u128::from(u64::MAX) + 1), 48_000_384);
    let hostile = [u64::MAX - 1, u64::MAX / 2, u64::MAX / 1000 + 1, wrap as u64];
    for g in hostile {
        let r = ms(&opus(g, 0));
        assert!(
            r.is_none() || r > Some(MAX_SOUND_MS),
            "granule {g} measured as {r:?}"
        );
    }
    // Same for the other formats: a data size or frame count at the top of its range.
    let mut w = wav(16);
    let at = w.len() - 16 - 4;
    w[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(
        ms(&w).is_none_or(|d| d <= 1),
        "a streaming writer's u32::MAX size reads what is there"
    );
    let mut xing = mp3(4);
    xing[4..4 + 12].copy_from_slice(b"Xing\0\0\0\x01\xFF\xFF\xFF\xFF");
    let r = ms(&xing);
    assert!(r.is_none() || r > Some(MAX_SOUND_MS), "{r:?}");
}

#[test]
fn every_truncation_and_random_corruption_is_measured_or_refused_without_a_panic() {
    let mut rng = Rng(SEED);
    let corpus = [wav(2000), opus(48_000 + 312, 312), mp3(8)];
    for full in &corpus {
        for cut in 0..full.len() {
            let _ = inspect_sound(&full[..cut]);
        }
        for _ in 0..1500 {
            let mut b = full.clone();
            for _ in 0..=rng.below(6) {
                let at = rng.below(b.len().max(1));
                match rng.below(4) {
                    0 => {
                        if let Some(x) = b.get_mut(at) {
                            *x ^= 1 << rng.below(8);
                        }
                    }
                    1 => {
                        if let Some(x) = b.get_mut(at) {
                            *x = rng.next() as u8;
                        }
                    }
                    2 => b.insert(at.min(b.len()), rng.next() as u8),
                    _ => {
                        if !b.is_empty() {
                            b.remove(at.min(b.len() - 1));
                        }
                    }
                }
            }
            let _ = inspect_sound(&b);
        }
    }
    // Pure noise behind each magic number.
    for magic in [
        &b"RIFF\0\0\0\0WAVE"[..],
        b"OggS",
        b"ID3\x04\0\0\0\0\0\x10",
        b"\xFF\xFB\x90\x00",
    ] {
        for _ in 0..500 {
            let mut b = magic.to_vec();
            b.extend((0..rng.below(300)).map(|_| rng.next() as u8));
            let _ = inspect_sound(&b);
        }
    }
}

#[test]
fn polyglots_and_markup_are_not_sounds() {
    for bytes in [
        &b"<html><script>alert(1)</script>"[..],
        b"<svg xmlns='http://www.w3.org/2000/svg'/>",
        b"\x89PNG\r\n\x1a\n",
        b"PK\x03\x04",
        b"%PDF-1.7",
        b"",
    ] {
        assert!(
            inspect_sound(bytes).is_none(),
            "{:?}",
            String::from_utf8_lossy(bytes)
        );
    }
    // A real WAV with a page of HTML after its data chunk still measures by its header, and stays a WAV:
    // sounds are served as audio/wav under nosniff, never as markup.
    let mut w = wav(16_000);
    w.extend_from_slice(b"<html><script>alert(1)</script>");
    assert_eq!(inspect_sound(&w).map(|i| i.format), Some(SoundFormat::Wav));
}

#[test]
fn the_doc_table_matches_the_code() {
    let doc = include_str!("../../../docs/12-deployment-and-operations.md");
    let l = ExpressionLimits::default();
    for row in [
        format!("| Emoji | file size | {} KiB |", l.max_emoji_bytes / 1024),
        format!(
            "| Sticker | file size | {} KiB |",
            l.max_sticker_bytes / 1024
        ),
        format!(
            "| Soundboard sound | file size and length | {} KiB and {} seconds |",
            MAX_SOUND_BYTES / 1024,
            MAX_SOUND_MS / 1000
        ),
    ] {
        assert!(
            doc.contains(&row),
            "docs/12 section 3 lacks the row {row:?}"
        );
    }
}
