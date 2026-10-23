//! Soundboard sounds (VOX-SB-001): the guild sound model, upload checks (type, size,
//! duration), the default sounds and the play cooldown.
//!
//! Bytes live in object storage; only the key is kept in the database. Sounds are played
//! by the *client* (it decodes the file and mixes it into its own microphone stream, so the
//! SFU and end-to-end encryption see ordinary audio), which is why the server only stores
//! the original file and announces the play (`VOICE_CHANNEL_EFFECT_SEND`).
//!
//! # Limits
//!
//! * At most 5 seconds and 512 KiB, MP3, Ogg (Opus or Vorbis) or WAV. The spec's
//!   "-> Opus" transcode is not done on the server (no codec library in the build);
//!   browsers decode all three formats with `decodeAudioData`.
//! * 48 sounds per server for everyone (`ExpressionLimits::max_soundboard_sounds`, D-044).
//! * One play per member every [`PLAY_COOLDOWN_MS`].
//!
//! # Default sounds
//!
//! Eight short sounds that every server and DM can play. They are synthesized here from
//! sine tones ([`render_default`]), so they are original work with no third-party
//! license. Their ids are `1..=8`, below any generated snowflake.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sr_core::{EmojiId, GuildId, SoundId, UserId};

use super::error::{SettingsError, SettingsResult};
use super::expressions::ExpressionLimits;

/// Largest sound file.
pub const MAX_SOUND_BYTES: u64 = 512 * 1024;
/// Longest sound.
pub const MAX_SOUND_MS: u32 = 5_000;
/// A member may play one sound per this many milliseconds.
pub const PLAY_COOLDOWN_MS: i64 = 4_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SoundFormat {
    Mp3,
    Ogg,
    Wav,
}

impl SoundFormat {
    pub const fn code(self) -> i16 {
        match self {
            Self::Mp3 => 1,
            Self::Ogg => 2,
            Self::Wav => 3,
        }
    }

    pub const fn from_code(code: i16) -> Option<Self> {
        Some(match code {
            1 => Self::Mp3,
            2 => Self::Ogg,
            3 => Self::Wav,
            _ => return None,
        })
    }

    pub const fn extension(self) -> &'static str {
        match self {
            Self::Mp3 => "mp3",
            Self::Ogg => "ogg",
            Self::Wav => "wav",
        }
    }

    pub const fn mime(self) -> &'static str {
        match self {
            Self::Mp3 => "audio/mpeg",
            Self::Ogg => "audio/ogg",
            Self::Wav => "audio/wav",
        }
    }
}

/// A guild sound.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sound {
    pub id: SoundId,
    pub guild_id: GuildId,
    pub name: String,
    /// 0.0 to 1.0, the level the sender wants it played at.
    pub volume: f64,
    /// Custom emoji shown with the sound.
    pub emoji_id: Option<EmojiId>,
    /// Unicode emoji shown with the sound.
    pub emoji_name: Option<String>,
    pub available: bool,
    pub creator_id: UserId,
    pub media_key: String,
    pub format: SoundFormat,
    pub duration_ms: u32,
    pub created_at_ms: i64,
}

impl Sound {
    /// Discord's soundboard sound object. `user` is the creator's summary when the caller
    /// has one; the gateway sends `user_id` only.
    pub fn to_json(&self, user: Option<serde_json::Value>) -> serde_json::Value {
        let mut v = serde_json::json!({
            "name": self.name,
            "sound_id": self.id,
            "volume": self.volume,
            "emoji_id": self.emoji_id,
            "emoji_name": self.emoji_name,
            "guild_id": self.guild_id,
            "available": self.available,
        });
        if let Some(o) = v.as_object_mut() {
            match user {
                Some(u) => o.insert("user".into(), u),
                None => o.insert("user_id".into(), serde_json::json!(self.creator_id)),
            };
        }
        v
    }
}

/// Names: 2 to 32 characters once trimmed.
pub fn validate_sound_name(name: &str) -> SettingsResult<()> {
    if (2..=32).contains(&name.trim().chars().count()) {
        Ok(())
    } else {
        Err(SettingsError::invalid(
            "name",
            "BASE_TYPE_BAD_LENGTH",
            "must be between 2 and 32 in length",
        ))
    }
}

/// Volume is a fraction.
pub fn validate_sound_volume(volume: f64) -> SettingsResult<()> {
    if volume.is_finite() && (0.0..=1.0).contains(&volume) {
        Ok(())
    } else {
        Err(SettingsError::invalid(
            "volume",
            "NUMBER_TYPE_MAX",
            "must be between 0 and 1",
        ))
    }
}

/// Check a new sound against size, length and slot limits.
pub fn check_new_sound(
    existing: &[Sound],
    size_bytes: u64,
    duration_ms: u32,
    limits: &ExpressionLimits,
) -> SettingsResult<()> {
    if size_bytes > MAX_SOUND_BYTES {
        return Err(SettingsError::invalid(
            "sound",
            "BINARY_TYPE_MAX_SIZE",
            format!("file cannot be larger than {} KiB", MAX_SOUND_BYTES / 1024),
        ));
    }
    if duration_ms == 0 || duration_ms > MAX_SOUND_MS {
        return Err(SettingsError::invalid(
            "sound",
            "SOUND_DURATION_INVALID",
            format!("must be at most {} seconds long", MAX_SOUND_MS / 1000),
        ));
    }
    if existing.len() >= limits.max_soundboard_sounds {
        return Err(SettingsError::LimitReached("soundboard sounds"));
    }
    Ok(())
}

// ---- reading the file --------------------------------------------------------------------

/// What a sound file turned out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundInfo {
    pub format: SoundFormat,
    pub duration_ms: u32,
}

fn le16(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

fn le32(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

fn le64(d: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(d.get(at..at + 8)?.try_into().ok()?))
}

/// Sniff the type from the magic bytes (the declared MIME type is not trusted) and measure
/// the duration. `None` when the bytes are not a sound we understand.
pub fn inspect_sound(data: &[u8]) -> Option<SoundInfo> {
    if data.len() > 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WAVE" {
        return wav_duration(data).map(|d| SoundInfo {
            format: SoundFormat::Wav,
            duration_ms: d,
        });
    }
    if data.starts_with(b"OggS") {
        return ogg_duration(data).map(|d| SoundInfo {
            format: SoundFormat::Ogg,
            duration_ms: d,
        });
    }
    mp3_duration(data).map(|d| SoundInfo {
        format: SoundFormat::Mp3,
        duration_ms: d,
    })
}

/// PCM WAV: `data` bytes over the byte rate of the `fmt ` chunk.
fn wav_duration(d: &[u8]) -> Option<u32> {
    let mut pos = 12;
    let mut byte_rate = None;
    while pos + 8 <= d.len() {
        let id = &d[pos..pos + 4];
        let size = le32(d, pos + 4)? as usize;
        let body = pos + 8;
        if id == b"fmt " {
            byte_rate = Some(le32(d, body + 8)?);
        } else if id == b"data" {
            let rate = u64::from(byte_rate?);
            if rate == 0 {
                return None;
            }
            // A streaming writer leaves the size at 0 or u32::MAX: use what is there.
            let avail = d.len().saturating_sub(body);
            let bytes = if size == 0 || size > avail {
                avail
            } else {
                size
            };
            return u32::try_from(bytes as u64 * 1000 / rate).ok();
        }
        pos = body.checked_add(size)?.checked_add(size & 1)?;
    }
    None
}

/// Ogg: the last page's granule position over the sample rate (48 kHz for Opus).
fn ogg_duration(d: &[u8]) -> Option<u32> {
    // First packet: the codec header.
    let seg_count = *d.get(26)? as usize;
    let first_packet = 27 + seg_count;
    let head = d.get(first_packet..)?;
    let (rate, pre_skip) = if head.starts_with(b"OpusHead") {
        (48_000u64, u64::from(le16(head, 10)?))
    } else if head.len() > 16 && head[0] == 1 && &head[1..7] == b"vorbis" {
        (u64::from(le32(head, 12)?), 0)
    } else {
        return None;
    };
    if rate == 0 {
        return None;
    }
    // Last page: the final "OggS" in the file.
    let last = d.windows(4).rposition(|w| w == b"OggS")?;
    let granule = le64(d, last + 6)?;
    if granule == u64::MAX {
        return None;
    }
    let samples = granule.saturating_sub(pre_skip);
    // A hostile granule position near 2^64 must not wrap into a short duration (RDY-SEC-008).
    u32::try_from(samples.checked_mul(1000)? / rate).ok()
}

/// MPEG audio: the frame count of a Xing/Info header when there is one, otherwise the
/// audio size over the bitrate of the first frame (exact for constant bitrate).
fn mp3_duration(d: &[u8]) -> Option<u32> {
    let mut start = 0usize;
    if d.starts_with(b"ID3") {
        let s = d.get(6..10)?;
        if s.iter().any(|b| b & 0x80 != 0) {
            return None;
        }
        let size = (usize::from(s[0]) << 21)
            | (usize::from(s[1]) << 14)
            | (usize::from(s[2]) << 7)
            | usize::from(s[3]);
        start = 10 + size;
    }
    // Resync to the first frame header within a small window.
    let at = (start..d.len().min(start + 4096))
        .find(|i| d.get(*i) == Some(&0xFF) && d.get(*i + 1).is_some_and(|b| b & 0xE0 == 0xE0))?;
    let h = d.get(at..at + 4)?;
    let version = (h[1] >> 3) & 3; // 3 = MPEG1, 2 = MPEG2, 0 = MPEG2.5
    let layer = (h[1] >> 1) & 3; // 1 = layer III
    if version == 1 || layer != 1 {
        return None;
    }
    let bitrate_idx = usize::from(h[2] >> 4);
    let rate_idx = usize::from((h[2] >> 2) & 3);
    if bitrate_idx == 0 || bitrate_idx == 15 || rate_idx == 3 {
        return None;
    }
    const BR_V1: [u32; 15] = [
        0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
    ];
    const BR_V2: [u32; 15] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];
    const RATE_V1: [u32; 3] = [44_100, 48_000, 32_000];
    let (kbps, rate, spf) = if version == 3 {
        (BR_V1[bitrate_idx], RATE_V1[rate_idx], 1152u64)
    } else {
        let div = if version == 2 { 1 } else { 2 };
        (BR_V2[bitrate_idx], RATE_V1[rate_idx] / div, 576u64)
    };
    // A VBR file carries its frame count in a Xing or Info header near the start.
    let probe = d.get(at..d.len().min(at + 64))?;
    for tag in [b"Xing", b"Info"] {
        if let Some(p) = probe.windows(4).position(|w| w == tag) {
            let flags = u32::from_be_bytes(probe.get(p + 4..p + 8)?.try_into().ok()?);
            if flags & 1 != 0 {
                let frames = u64::from(u32::from_be_bytes(
                    probe.get(p + 8..p + 12)?.try_into().ok()?,
                ));
                return u32::try_from(frames * spf * 1000 / u64::from(rate)).ok();
            }
        }
    }
    let audio = (d.len() - at) as u64;
    u32::try_from(audio * 8 / u64::from(kbps)).ok()
}

// ---- default sounds ----------------------------------------------------------------------------

/// One default sound: a list of `(frequency Hz, milliseconds)` notes (0 Hz is a rest).
#[derive(Debug, Clone, Copy)]
pub struct DefaultSound {
    pub id: u64,
    pub name: &'static str,
    pub emoji: &'static str,
    pub notes: &'static [(f32, u32)],
}

/// The sounds every server and DM can play.
pub const DEFAULT_SOUNDS: [DefaultSound; 8] = [
    DefaultSound {
        id: 1,
        name: "Chime",
        emoji: "\u{1F514}",
        notes: &[(880.0, 160), (1318.5, 380)],
    },
    DefaultSound {
        id: 2,
        name: "Ding",
        emoji: "\u{2728}",
        notes: &[(1568.0, 420)],
    },
    DefaultSound {
        id: 3,
        name: "Bloop",
        emoji: "\u{1F4A7}",
        notes: &[(330.0, 70), (520.0, 90), (740.0, 140)],
    },
    DefaultSound {
        id: 4,
        name: "Level up",
        emoji: "\u{1F680}",
        notes: &[(523.3, 110), (659.3, 110), (784.0, 110), (1046.5, 320)],
    },
    DefaultSound {
        id: 5,
        name: "Womp womp",
        emoji: "\u{1F61E}",
        notes: &[(392.0, 260), (370.0, 260), (349.2, 260), (261.6, 620)],
    },
    DefaultSound {
        id: 6,
        name: "Alert",
        emoji: "\u{26A0}\u{FE0F}",
        notes: &[
            (988.0, 140),
            (0.0, 60),
            (988.0, 140),
            (0.0, 60),
            (988.0, 140),
        ],
    },
    DefaultSound {
        id: 7,
        name: "Drum roll",
        emoji: "\u{1F941}",
        notes: &[
            (110.0, 60),
            (0.0, 30),
            (110.0, 60),
            (0.0, 30),
            (110.0, 60),
            (0.0, 30),
            (110.0, 60),
            (0.0, 30),
            (82.4, 340),
        ],
    },
    DefaultSound {
        id: 8,
        name: "Sparkle",
        emoji: "\u{1F31F}",
        notes: &[
            (1760.0, 70),
            (2093.0, 70),
            (2637.0, 70),
            (3136.0, 70),
            (2637.0, 200),
        ],
    },
];

/// Look up a default sound by id.
pub fn default_sound(id: u64) -> Option<&'static DefaultSound> {
    DEFAULT_SOUNDS.iter().find(|s| s.id == id)
}

const SYNTH_RATE: u32 = 22_050;

/// Render a default sound as a 16-bit mono PCM WAV file. Deterministic.
pub fn render_default(sound: &DefaultSound) -> Vec<u8> {
    let mut pcm: Vec<i16> = Vec::new();
    for &(freq, ms) in sound.notes {
        let n = (u64::from(SYNTH_RATE) * u64::from(ms) / 1000) as usize;
        let attack = (SYNTH_RATE as usize / 200).max(1); // 5 ms
        for i in 0..n {
            if freq <= 0.0 {
                pcm.push(0);
                continue;
            }
            let t = i as f32 / SYNTH_RATE as f32;
            // Short attack, exponential decay, so notes do not click.
            let env = (i as f32 / attack as f32).min(1.0) * (-3.0 * i as f32 / n as f32).exp();
            let v = (2.0 * std::f32::consts::PI * freq * t).sin() * env * 0.6;
            pcm.push((v * f32::from(i16::MAX)) as i16);
        }
    }
    let data_len = (pcm.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + pcm.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SYNTH_RATE.to_le_bytes());
    out.extend_from_slice(&(SYNTH_RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in pcm {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

impl DefaultSound {
    pub fn duration_ms(&self) -> u32 {
        self.notes.iter().map(|(_, ms)| *ms).sum()
    }

    /// Discord shape; `sound_id` is the small default id.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "sound_id": self.id.to_string(),
            "volume": 1.0,
            "emoji_id": null,
            "emoji_name": self.emoji,
            "available": true,
        })
    }
}

// ---- cooldown ------------------------------------------------------------------------------------

/// Per-member play cooldown. Time is passed in, so it is testable.
#[derive(Debug, Default)]
pub struct PlayCooldown {
    last: HashMap<UserId, i64>,
}

impl PlayCooldown {
    /// Record a play at `now_ms`, or say how many milliseconds remain.
    pub fn try_play(&mut self, user: UserId, now_ms: i64) -> Result<(), i64> {
        if let Some(prev) = self.last.get(&user) {
            let wait = PLAY_COOLDOWN_MS - (now_ms - prev);
            if wait > 0 {
                return Err(wait);
            }
        }
        // Forget members whose cooldown ended long ago, so the map stays small.
        if self.last.len() > 4096 {
            self.last.retain(|_, t| now_ms - *t < PLAY_COOLDOWN_MS);
        }
        self.last.insert(user, now_ms);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(rate: u32, ms: u32) -> Vec<u8> {
        let n = (u64::from(rate) * u64::from(ms) / 1000) as usize;
        let data_len = (n * 2) as u32;
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * 2).to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        out.resize(out.len() + n * 2, 0);
        out
    }

    /// A one-page Ogg Opus file: header packet, then a page whose granule says `ms`.
    fn ogg_opus(ms: u64, pre_skip: u16) -> Vec<u8> {
        let mut head = Vec::new();
        head.extend_from_slice(b"OpusHead");
        head.push(1);
        head.push(1);
        head.extend_from_slice(&pre_skip.to_le_bytes());
        head.extend_from_slice(&48_000u32.to_le_bytes());
        head.extend_from_slice(&0u16.to_le_bytes());
        head.push(0);
        let mut out = Vec::new();
        let page = |granule: u64, payload: &[u8], seq: u32| {
            let mut p = Vec::new();
            p.extend_from_slice(b"OggS");
            p.push(0);
            p.push(if seq == 0 { 2 } else { 4 });
            p.extend_from_slice(&granule.to_le_bytes());
            p.extend_from_slice(&1u32.to_le_bytes());
            p.extend_from_slice(&seq.to_le_bytes());
            p.extend_from_slice(&0u32.to_le_bytes());
            p.push(1);
            p.push(payload.len() as u8);
            p.extend_from_slice(payload);
            p
        };
        out.extend(page(0, &head, 0));
        out.extend(page(ms * 48 + u64::from(pre_skip), &[0u8; 4], 1));
        out
    }

    /// A CBR MPEG1 layer III stream at 128 kbit/s, 44.1 kHz, `ms` long.
    fn mp3(ms: u32, id3: bool) -> Vec<u8> {
        let bytes = (128_000u64 / 8 * u64::from(ms) / 1000) as usize;
        let mut out = Vec::new();
        if id3 {
            out.extend_from_slice(b"ID3\x03\x00\x00\x00\x00\x00\x0a");
            out.extend_from_slice(&[0u8; 10]);
        }
        out.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
        out.resize(out.len() + bytes - 4, 0x55);
        out
    }

    #[test]
    fn wav_durations_are_exact() {
        let i = inspect_sound(&wav(22_050, 1_500)).unwrap();
        assert_eq!(i.format, SoundFormat::Wav);
        assert_eq!(i.duration_ms, 1_500);
        let i = inspect_sound(&wav(48_000, 5_000)).unwrap();
        assert_eq!(i.duration_ms, 5_000);
    }

    #[test]
    fn truncated_wavs_are_rejected() {
        assert!(inspect_sound(b"RIFF\0\0\0\0WAVE").is_none());
        let mut w = wav(22_050, 100);
        w.truncate(30);
        assert!(inspect_sound(&w).is_none());
    }

    #[test]
    fn ogg_opus_duration_comes_from_the_last_granule() {
        let i = inspect_sound(&ogg_opus(2_000, 312)).unwrap();
        assert_eq!(i.format, SoundFormat::Ogg);
        assert_eq!(i.duration_ms, 2_000);
        // A file that is not Opus or Vorbis is not accepted.
        let mut bad = ogg_opus(1_000, 0);
        bad[27 + 1] = b'X';
        assert!(inspect_sound(&bad).is_none());
    }

    #[test]
    fn mp3_duration_follows_the_bitrate() {
        let i = inspect_sound(&mp3(3_000, false)).unwrap();
        assert_eq!(i.format, SoundFormat::Mp3);
        assert!(
            (2_990..=3_010).contains(&i.duration_ms),
            "{}",
            i.duration_ms
        );
        let i = inspect_sound(&mp3(1_000, true)).unwrap();
        assert!((990..=1_010).contains(&i.duration_ms), "{}", i.duration_ms);
    }

    #[test]
    fn a_xing_header_gives_the_exact_vbr_length() {
        // 100 frames of 1152 samples at 44.1 kHz = 2612 ms.
        let mut f = vec![0xFF, 0xFB, 0x90, 0x00];
        f.extend_from_slice(&[0u8; 32]);
        f.extend_from_slice(b"Xing");
        f.extend_from_slice(&1u32.to_be_bytes());
        f.extend_from_slice(&100u32.to_be_bytes());
        f.resize(417, 0);
        let i = inspect_sound(&f).unwrap();
        assert_eq!(i.duration_ms, 2_612);
    }

    #[test]
    fn other_files_are_not_sounds() {
        assert!(inspect_sound(b"").is_none());
        assert!(inspect_sound(b"\x89PNG\r\n\x1a\n").is_none());
        assert!(inspect_sound(b"just some text that is long enough").is_none());
        // MPEG layer II is not accepted.
        assert!(inspect_sound(&[0xFF, 0xFD, 0x90, 0x00, 0, 0, 0, 0]).is_none());
    }

    #[test]
    fn upload_checks_cover_size_length_and_slots() {
        let limits = ExpressionLimits::default();
        assert!(check_new_sound(&[], 100_000, 4_000, &limits).is_ok());
        assert!(check_new_sound(&[], MAX_SOUND_BYTES + 1, 4_000, &limits).is_err());
        assert!(check_new_sound(&[], 100, 5_001, &limits).is_err());
        assert!(check_new_sound(&[], 100, 0, &limits).is_err());
        let full: Vec<Sound> = (0..48)
            .map(|i| Sound {
                id: SoundId::new(1000 + i),
                guild_id: GuildId::new(1),
                name: format!("s{i}"),
                volume: 1.0,
                emoji_id: None,
                emoji_name: None,
                available: true,
                creator_id: UserId::new(1),
                media_key: String::new(),
                format: SoundFormat::Wav,
                duration_ms: 100,
                created_at_ms: 0,
            })
            .collect();
        assert!(matches!(
            check_new_sound(&full, 100, 100, &limits),
            Err(SettingsError::LimitReached("soundboard sounds"))
        ));
    }

    #[test]
    fn names_and_volumes_are_validated() {
        assert!(validate_sound_name("ok").is_ok());
        assert!(validate_sound_name(" a ").is_err());
        assert!(validate_sound_name(&"x".repeat(33)).is_err());
        assert!(validate_sound_volume(0.0).is_ok());
        assert!(validate_sound_volume(1.0).is_ok());
        assert!(validate_sound_volume(1.01).is_err());
        assert!(validate_sound_volume(f64::NAN).is_err());
    }

    #[test]
    fn default_sounds_are_valid_short_and_distinct() {
        let mut ids = std::collections::HashSet::new();
        let mut names = std::collections::HashSet::new();
        for s in &DEFAULT_SOUNDS {
            assert!(ids.insert(s.id) && names.insert(s.name));
            assert!(validate_sound_name(s.name).is_ok());
            let wav = render_default(s);
            let info = inspect_sound(&wav).expect(s.name);
            assert_eq!(info.format, SoundFormat::Wav);
            assert!(info.duration_ms <= MAX_SOUND_MS, "{}", s.name);
            assert!(wav.len() as u64 <= MAX_SOUND_BYTES, "{}", s.name);
            // The measured length matches the notes (within a sample).
            assert!(
                info.duration_ms.abs_diff(s.duration_ms()) <= 2,
                "{}",
                s.name
            );
            // It is actually audible, not silence.
            assert!(wav[44..].chunks(2).any(|c| c != [0, 0]), "{}", s.name);
            assert_eq!(wav, render_default(s), "deterministic");
        }
        assert!(default_sound(3).is_some() && default_sound(99).is_none());
    }

    #[test]
    fn the_cooldown_allows_one_play_per_window() {
        let mut c = PlayCooldown::default();
        let a = UserId::new(1);
        let b = UserId::new(2);
        assert_eq!(c.try_play(a, 10_000), Ok(()));
        assert_eq!(c.try_play(a, 11_000), Err(PLAY_COOLDOWN_MS - 1_000));
        // Other members are unaffected.
        assert_eq!(c.try_play(b, 11_000), Ok(()));
        assert_eq!(c.try_play(a, 10_000 + PLAY_COOLDOWN_MS), Ok(()));
    }

    #[test]
    fn format_codes_round_trip() {
        for f in [SoundFormat::Mp3, SoundFormat::Ogg, SoundFormat::Wav] {
            assert_eq!(SoundFormat::from_code(f.code()), Some(f));
            assert!(!f.extension().is_empty() && f.mime().starts_with("audio/"));
        }
        assert_eq!(SoundFormat::from_code(9), None);
    }
}
