//! ID3v2.2/2.3/2.4 at the head of a file and ID3v1 in the last 128 bytes.

use crate::tags::*;

const GENRES: [&str; 80] = [
    "Blues",
    "Classic Rock",
    "Country",
    "Dance",
    "Disco",
    "Funk",
    "Grunge",
    "Hip-Hop",
    "Jazz",
    "Metal",
    "New Age",
    "Oldies",
    "Other",
    "Pop",
    "R&B",
    "Rap",
    "Reggae",
    "Rock",
    "Techno",
    "Industrial",
    "Alternative",
    "Ska",
    "Death Metal",
    "Pranks",
    "Soundtrack",
    "Euro-Techno",
    "Ambient",
    "Trip-Hop",
    "Vocal",
    "Jazz+Funk",
    "Fusion",
    "Trance",
    "Classical",
    "Instrumental",
    "Acid",
    "House",
    "Game",
    "Sound Clip",
    "Gospel",
    "Noise",
    "AlternRock",
    "Bass",
    "Soul",
    "Punk",
    "Space",
    "Meditative",
    "Instrumental Pop",
    "Instrumental Rock",
    "Ethnic",
    "Gothic",
    "Darkwave",
    "Techno-Industrial",
    "Electronic",
    "Pop-Folk",
    "Eurodance",
    "Dream",
    "Southern Rock",
    "Comedy",
    "Cult",
    "Gangsta",
    "Top 40",
    "Christian Rap",
    "Pop/Funk",
    "Jungle",
    "Native American",
    "Cabaret",
    "New Wave",
    "Psychadelic",
    "Rave",
    "Showtunes",
    "Trailer",
    "Lo-Fi",
    "Tribal",
    "Acid Punk",
    "Acid Jazz",
    "Polka",
    "Retro",
    "Musical",
    "Rock & Roll",
    "Hard Rock",
];

fn genre_name(n: usize) -> Option<String> {
    GENRES.get(n).map(|g| (*g).to_string())
}

/// "(17)", "(17)Rock", "17" and plain names.
fn resolve_genre(raw: &str) -> Option<String> {
    let t = raw.trim();
    if let Some(rest) = t.strip_prefix('(')
        && let Some((num, tail)) = rest.split_once(')')
        && let Ok(n) = num.parse::<usize>()
    {
        return clean(tail).or_else(|| genre_name(n));
    }
    if let Ok(n) = t.parse::<usize>() {
        return genre_name(n).or_else(|| clean(t));
    }
    clean(t)
}

fn syncsafe(b: &[u8]) -> Option<u32> {
    let s = b.get(..4)?;
    if s.iter().any(|x| x & 0x80 != 0) {
        return None;
    }
    Some(
        (u32::from(s[0]) << 21)
            | (u32::from(s[1]) << 14)
            | (u32::from(s[2]) << 7)
            | u32::from(s[3]),
    )
}

/// Undo unsynchronisation: `FF 00` becomes `FF`.
pub(crate) fn de_unsync(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        out.push(b[i]);
        if b[i] == 0xff && b.get(i + 1) == Some(&0) {
            i += 1;
        }
        i += 1;
    }
    out
}

/// Split text into its NUL-separated values for the given ID3 encoding byte.
fn decode_text(enc: u8, b: &[u8]) -> Vec<String> {
    let strings: Vec<String> = match enc {
        1 | 2 => {
            // UTF-16: BOM decides the order for 1, big-endian for 2. Values end at an aligned 00 00.
            let (mut le, mut body) = (false, b);
            if enc == 1 {
                if b.starts_with(&[0xff, 0xfe]) {
                    le = true;
                    body = &b[2..];
                } else if b.starts_with(&[0xfe, 0xff]) {
                    body = &b[2..];
                }
            }
            let units: Vec<u16> = body
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| {
                    if le {
                        u16::from_le_bytes(*c)
                    } else {
                        u16::from_be_bytes(*c)
                    }
                })
                .collect();
            units
                .split(|u| *u == 0)
                .map(String::from_utf16_lossy)
                .collect()
        }
        3 => b
            .split(|c| *c == 0)
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect(),
        _ => b.split(|c| *c == 0).map(latin1).collect(),
    };
    strings.into_iter().filter_map(|s| clean(&s)).collect()
}

/// Length of a terminator-delimited string at the start of `b` (and the terminator's width).
fn find_terminator(enc: u8, b: &[u8]) -> Option<(usize, usize)> {
    if enc == 1 || enc == 2 {
        let mut i = 0;
        while i + 1 < b.len() {
            if b[i] == 0 && b[i + 1] == 0 {
                return Some((i, 2));
            }
            i += 2;
        }
        None
    } else {
        b.iter().position(|c| *c == 0).map(|i| (i, 1))
    }
}

struct Picture {
    kind: u8,
    cover: Cover,
}

fn parse_apic(body: &[u8], v22: bool) -> Option<Picture> {
    let enc = *body.first()?;
    let rest = body.get(1..)?;
    let (mime, after_mime) = if v22 {
        (latin1(rest.get(..3)?), rest.get(3..)?)
    } else {
        let (n, w) = find_terminator(0, rest)?;
        (latin1(&rest[..n]), rest.get(n + w..)?)
    };
    if mime == "-->" {
        return None; // a URL, not data
    }
    let kind = *after_mime.first()?;
    let after_kind = after_mime.get(1..)?;
    let (n, w) = find_terminator(enc, after_kind)?;
    let data = after_kind.get(n + w..)?;
    if data.is_empty() {
        return None;
    }
    Some(Picture {
        kind,
        cover: Cover {
            mime: normalise_mime(&mime, data),
            bytes: data.to_vec(),
        },
    })
}

fn apply_frame(t: &mut Tags, id: &str, body: &[u8], pic: &mut Option<Picture>, v22: bool) {
    let text = || {
        let enc = *body.first()?;
        Some(decode_text(enc, body.get(1..)?))
    };
    let first = || text().and_then(|v| v.into_iter().next());
    match id {
        "TIT2" | "TT2" => t.title = t.title.take().or_else(first),
        "TPE1" | "TP1" => {
            if t.artist.is_none() {
                let all = text().unwrap_or_default();
                t.artist = (!all.is_empty()).then(|| all.join(", "));
            }
        }
        "TALB" | "TAL" => t.album = t.album.take().or_else(first),
        "TPE2" | "TP2" => t.album_artist = t.album_artist.take().or_else(first),
        "TRCK" | "TRK" => t.track = t.track.or_else(|| first().and_then(|s| parse_number(&s))),
        "TPOS" | "TPA" => t.disc = t.disc.or_else(|| first().and_then(|s| parse_number(&s))),
        "TDRC" | "TYER" | "TYE" | "TDRL" => {
            t.year = t.year.or_else(|| first().and_then(|s| parse_year(&s)))
        }
        "TCON" | "TCO" => {
            if t.genre.is_none() {
                t.genre = first().and_then(|s| resolve_genre(&s));
            }
        }
        "TLEN" | "TLE" => {
            t.duration_hint = t.duration_hint.or_else(|| {
                first()
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .filter(|n| *n > 0)
            });
        }
        "APIC" | "PIC" => {
            if let Some(p) = parse_apic(body, v22) {
                // Prefer the front cover (type 3), else the first picture.
                let better = match pic {
                    None => true,
                    Some(cur) => cur.kind != 3 && p.kind == 3,
                };
                if better {
                    *pic = Some(p);
                }
            }
        }
        _ => {}
    }
}

/// Read an ID3v2 tag at the start of `b`. A tag longer than `b` yields the frames that fit.
pub fn read_v2(b: &[u8]) -> Result<Tags, TagError> {
    if !b.starts_with(b"ID3") {
        return Err(TagError::Unsupported);
    }
    let header = b.get(..10).ok_or(TagError::Truncated)?;
    let (major, flags) = (header[3], header[5]);
    if !(2..=4).contains(&major) {
        return Err(TagError::Malformed("ID3v2 version"));
    }
    let size = syncsafe(&header[6..10]).ok_or(TagError::Malformed("tag size"))? as usize;
    let avail = b.get(10..).unwrap_or_default();
    let raw = avail.get(..size.min(avail.len())).unwrap_or_default();
    let unsync_all = flags & 0x80 != 0;
    // v2.2/2.3 unsynchronise the whole body; v2.4 does it per frame (sizes count the encoded bytes).
    let owned;
    let mut body: &[u8] = if unsync_all && major < 4 {
        owned = de_unsync(raw);
        &owned
    } else {
        raw
    };
    if flags & 0x40 != 0 && major >= 3 {
        // Extended header: skip it.
        let skip = if major == 4 {
            syncsafe(body).map(|s| s as usize)
        } else {
            u32_be(body).map(|s| s as usize + 4)
        }
        .ok_or(TagError::Malformed("extended header"))?;
        body = body.get(skip..).unwrap_or_default();
    }

    let v22 = major == 2;
    let id_len = if v22 { 3 } else { 4 };
    let head_len = if v22 { 6 } else { 10 };
    let mut t = Tags::default();
    let mut pic = None;
    let mut pos = 0usize;
    while let Some(fh) = body.get(pos..pos + head_len) {
        if fh[0] == 0 {
            break; // padding
        }
        let id = &fh[..id_len];
        if !id
            .iter()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        {
            break;
        }
        let id = String::from_utf8_lossy(id).into_owned();
        let (fsize, fflags) = if v22 {
            (
                (usize::from(fh[3]) << 16) | (usize::from(fh[4]) << 8) | usize::from(fh[5]),
                0u16,
            )
        } else {
            let sz = if major == 4 {
                match syncsafe(&fh[4..8]) {
                    Some(s) => s as usize,
                    None => break,
                }
            } else {
                u32_be(&fh[4..8]).unwrap_or(0) as usize
            };
            (sz, u16::from_be_bytes([fh[8], fh[9]]))
        };
        let start = pos + head_len;
        let Some(end) = start.checked_add(fsize) else {
            break;
        };
        let Some(mut data) = body.get(start..end) else {
            break; // frame cut off by the end of the buffer (often a big APIC): stop here
        };
        pos = end;
        let owned_frame;
        if major == 4 {
            // 0x01 data length indicator (4 syncsafe bytes), 0x02 unsynchronisation.
            if fflags & 0x0001 != 0 {
                data = data.get(4..).unwrap_or_default();
            }
            if fflags & 0x0002 != 0 || unsync_all {
                owned_frame = de_unsync(data);
                data = &owned_frame;
            }
            if fflags & 0x000c != 0 {
                continue; // compressed or encrypted: not readable
            }
        } else if major == 3 && fflags & 0x00c0 != 0 {
            continue; // compression / encryption bits
        }
        apply_frame(&mut t, &id, data, &mut pic, v22);
    }
    t.cover = pic.map(|p| p.cover);
    Ok(t)
}

/// Read an ID3v1 tag from the last 128 bytes of `b` (v1.1 track byte supported).
pub fn read_v1(b: &[u8]) -> Result<Tags, TagError> {
    let tail = b
        .len()
        .checked_sub(128)
        .and_then(|s| b.get(s..))
        .ok_or(TagError::Truncated)?;
    if !tail.starts_with(b"TAG") {
        return Err(TagError::Unsupported);
    }
    let f = |r: std::ops::Range<usize>| tail.get(r).and_then(|s| clean(&latin1(s)));
    let mut t = Tags {
        title: f(3..33),
        artist: f(33..63),
        album: f(63..93),
        year: f(93..97).and_then(|s| parse_year(&s)),
        ..Tags::default()
    };
    if tail[125] == 0 && tail[126] != 0 {
        t.track = Some(u32::from(tail[126]));
    }
    t.genre = genre_name(usize::from(tail[127]));
    Ok(t)
}
