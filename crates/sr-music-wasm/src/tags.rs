//! The result types and shared helpers of every container reader.

use thiserror::Error;

/// Why a file could not be read. Input of any shape returns one of these, never a panic.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TagError {
    #[error("not a recognised audio container")]
    Unsupported,
    #[error("the data ends before the tag does")]
    Truncated,
    /// An MP4 whose `moov` box is not in the head. `offset` is where to fetch next: the `moov` box
    /// when the head shows it, else the next top-level box after the last one the head shows.
    #[error("NEED_MOOV:{offset}")]
    NeedMoov { offset: u64 },
    #[error("malformed tag: {0}")]
    Malformed(&'static str),
}

/// A picture embedded in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cover {
    pub mime: String,
    pub bytes: Vec<u8>,
}

/// The stable result shape (what `lofty` would also fill).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub track: Option<u32>,
    pub disc: Option<u32>,
    pub year: Option<u32>,
    pub genre: Option<String>,
    pub duration_hint: Option<u64>,
    pub cover: Option<Cover>,
}

impl Tags {
    /// JSON for the facade: camelCase keys, the picture reduced to `coverMime` (bytes travel apart).
    pub fn to_json(&self) -> String {
        serde_json::json!({
            "title": self.title,
            "artist": self.artist,
            "album": self.album,
            "albumArtist": self.album_artist,
            "track": self.track,
            "disc": self.disc,
            "year": self.year,
            "genre": self.genre,
            "durationHint": self.duration_hint,
            "coverMime": self.cover.as_ref().map(|c| c.mime.as_str()),
        })
        .to_string()
    }
}

/// Field values are trimmed; empty means absent.
pub(crate) fn clean(s: &str) -> Option<String> {
    let t = s.trim_matches(|c: char| c.is_whitespace() || c == '\0' || c == '\u{feff}');
    (!t.is_empty()).then(|| t.to_string())
}

/// "3/12" -> 3, " 07 " -> 7; 0 and non-numbers are absent.
pub(crate) fn parse_number(s: &str) -> Option<u32> {
    let head = s.split('/').next()?.trim();
    head.parse::<u32>().ok().filter(|n| *n > 0)
}

/// The first four digits of "2019", "2019-03-04", "2019-03-04T10:00".
pub(crate) fn parse_year(s: &str) -> Option<u32> {
    let digits: String = s.trim().chars().take(4).collect();
    if digits.len() == 4 && digits.chars().all(|c| c.is_ascii_digit()) {
        digits.parse().ok().filter(|y| *y > 0)
    } else {
        None
    }
}

pub(crate) fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| c as char).collect()
}

pub(crate) fn u32_be(b: &[u8]) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(..4)?.try_into().ok()?))
}

pub(crate) fn u32_le(b: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(..4)?.try_into().ok()?))
}

/// A picture mime from the tag, repaired when it is a bare format or a legacy value.
pub(crate) fn normalise_mime(raw: &str, data: &[u8]) -> String {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "image/jpeg" | "image/png" | "image/gif" | "image/webp" | "image/bmp" => lower,
        "image/jpg" | "jpg" | "jpeg" => "image/jpeg".into(),
        "png" => "image/png".into(),
        _ => sniff_mime(data)
            .unwrap_or("application/octet-stream")
            .into(),
    }
}

pub(crate) fn sniff_mime(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if data.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if data.starts_with(b"GIF8") {
        Some("image/gif")
    } else if data.len() > 12 && data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WEBP") {
        Some("image/webp")
    } else {
        None
    }
}

/// Standard base64 (RFC 4648), padding optional, whitespace ignored. `None` on a bad byte.
pub(crate) fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Apply a `KEY=value` Vorbis-style comment (first value of a key wins).
pub(crate) fn apply_comment(t: &mut Tags, key: &str, value: &str) {
    match key.to_ascii_uppercase().as_str() {
        "TITLE" => t.title = t.title.take().or_else(|| clean(value)),
        "ARTIST" => t.artist = t.artist.take().or_else(|| clean(value)),
        "ALBUM" => t.album = t.album.take().or_else(|| clean(value)),
        "ALBUMARTIST" | "ALBUM ARTIST" => {
            t.album_artist = t.album_artist.take().or_else(|| clean(value))
        }
        "TRACKNUMBER" | "TRACK" => t.track = t.track.or_else(|| parse_number(value)),
        "DISCNUMBER" | "DISC" => t.disc = t.disc.or_else(|| parse_number(value)),
        "DATE" | "YEAR" | "ORIGINALDATE" => t.year = t.year.or_else(|| parse_year(value)),
        "GENRE" => t.genre = t.genre.take().or_else(|| clean(value)),
        "METADATA_BLOCK_PICTURE" => {
            if t.cover.is_none()
                && let Some(raw) = base64_decode(value)
            {
                t.cover = crate::flac::parse_picture(&raw);
            }
        }
        _ => {}
    }
}

/// Parse the body of a Vorbis comment block (also the tail of Opus `OpusTags`). Stops quietly at
/// a truncated entry, keeping what was read.
pub(crate) fn parse_vorbis_comments(b: &[u8], t: &mut Tags) -> Result<(), TagError> {
    let vendor = u32_le(b).ok_or(TagError::Truncated)? as usize;
    let mut pos = 4usize
        .checked_add(vendor)
        .ok_or(TagError::Malformed("vendor"))?;
    let count = u32_le(b.get(pos..).ok_or(TagError::Truncated)?).ok_or(TagError::Truncated)?;
    pos += 4;
    for _ in 0..count {
        let Some(len) = b.get(pos..).and_then(u32_le) else {
            break;
        };
        pos += 4;
        let Some(end) = pos.checked_add(len as usize) else {
            break;
        };
        let Some(entry) = b.get(pos..end) else {
            break;
        };
        pos = end;
        let s = String::from_utf8_lossy(entry);
        if let Some((k, v)) = s.split_once('=') {
            apply_comment(t, k, v);
        }
    }
    Ok(())
}

impl Tags {
    /// Fill every field that is still absent from `other`.
    pub fn fill_from(&mut self, other: Tags) {
        self.title = self.title.take().or(other.title);
        self.artist = self.artist.take().or(other.artist);
        self.album = self.album.take().or(other.album);
        self.album_artist = self.album_artist.take().or(other.album_artist);
        self.track = self.track.or(other.track);
        self.disc = self.disc.or(other.disc);
        self.year = self.year.or(other.year);
        self.genre = self.genre.take().or(other.genre);
        self.duration_hint = self.duration_hint.or(other.duration_hint);
        self.cover = self.cover.take().or(other.cover);
    }
}
