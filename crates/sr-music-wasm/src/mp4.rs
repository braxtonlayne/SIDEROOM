//! MP4/M4A: `moov/udta/meta/ilst` text atoms, `trkn`/`disk`, `covr`, and the `mvhd` duration.

use crate::tags::*;

/// One box: its type, its body, and the offset just past it.
struct Atom<'a> {
    kind: [u8; 4],
    body: &'a [u8],
    next: usize,
    cut: bool,
}

fn atom_at(b: &[u8], pos: usize) -> Option<Atom<'_>> {
    let h = b.get(pos..pos.checked_add(8)?)?;
    let size = u64::from(u32_be(h)?);
    let kind = [h[4], h[5], h[6], h[7]];
    let (head, total) = match size {
        0 => (8usize, (b.len() - pos) as u64),
        1 => {
            let big = b.get(pos + 8..pos + 16)?;
            (16usize, u64::from_be_bytes(big.try_into().ok()?))
        }
        n => (8usize, n),
    };
    if total < head as u64 {
        return None;
    }
    let start = pos + head;
    let want_end = (pos as u64).checked_add(total)?;
    let cut = want_end > b.len() as u64;
    let end = if cut { b.len() } else { want_end as usize };
    Some(Atom {
        kind,
        body: b.get(start..end)?,
        next: end,
        cut,
    })
}

fn children(b: &[u8]) -> impl Iterator<Item = Atom<'_>> {
    let mut pos = 0usize;
    std::iter::from_fn(move || {
        let a = atom_at(b, pos)?;
        pos = a.next;
        if a.cut {
            pos = b.len();
        }
        Some(a)
    })
}

fn data_payload(atom_body: &[u8]) -> Option<(u32, &[u8])> {
    children(atom_body)
        .find(|a| &a.kind == b"data")
        .and_then(|d| Some((u32_be(d.body)? & 0x00ff_ffff, d.body.get(8..)?)))
}

fn text(atom_body: &[u8]) -> Option<String> {
    let (_, p) = data_payload(atom_body)?;
    clean(&String::from_utf8_lossy(p))
}

fn pair(atom_body: &[u8]) -> Option<u32> {
    let (_, p) = data_payload(atom_body)?;
    let n = u32::from(u16::from_be_bytes([*p.get(2)?, *p.get(3)?]));
    (n > 0).then_some(n)
}

fn apply_ilst(ilst: &[u8], t: &mut Tags) {
    for a in children(ilst) {
        match &a.kind {
            b"\xa9nam" => t.title = t.title.take().or_else(|| text(a.body)),
            b"\xa9ART" => t.artist = t.artist.take().or_else(|| text(a.body)),
            b"\xa9alb" => t.album = t.album.take().or_else(|| text(a.body)),
            b"aART" => t.album_artist = t.album_artist.take().or_else(|| text(a.body)),
            b"\xa9day" => t.year = t.year.or_else(|| text(a.body).and_then(|s| parse_year(&s))),
            b"\xa9gen" => t.genre = t.genre.take().or_else(|| text(a.body)),
            b"trkn" => t.track = t.track.or_else(|| pair(a.body)),
            b"disk" => t.disc = t.disc.or_else(|| pair(a.body)),
            b"covr" if t.cover.is_none() => {
                if let Some((flags, p)) = data_payload(a.body)
                    && !p.is_empty()
                {
                    let mime = match flags {
                        13 => "image/jpeg",
                        14 => "image/png",
                        _ => sniff_mime(p).unwrap_or("application/octet-stream"),
                    };
                    t.cover = Some(Cover {
                        mime: mime.into(),
                        bytes: p.to_vec(),
                    });
                }
            }
            _ => {}
        }
    }
}

fn duration(mvhd: &[u8]) -> Option<u64> {
    let (scale, dur) = if *mvhd.first()? == 1 {
        (
            u64::from(u32_be(mvhd.get(20..)?)?),
            u64::from_be_bytes(mvhd.get(24..32)?.try_into().ok()?),
        )
    } else {
        (
            u64::from(u32_be(mvhd.get(12..)?)?),
            u64::from(u32_be(mvhd.get(16..)?)?),
        )
    };
    (scale > 0 && dur > 0).then(|| dur.saturating_mul(1000) / scale)
}

pub fn read(b: &[u8]) -> Result<Tags, TagError> {
    if b.get(4..8) != Some(b"ftyp") {
        return Err(TagError::Unsupported);
    }
    let mut t = Tags::default();
    let mut pos = 0usize;
    let mut found_moov = false;
    while let Some(a) = atom_at(b, pos) {
        if &a.kind == b"moov" {
            found_moov = true;
            for m in children(a.body) {
                match &m.kind {
                    b"mvhd" => t.duration_hint = duration(m.body),
                    b"udta" => {
                        for u in children(m.body).filter(|u| &u.kind == b"meta") {
                            // `meta` is a full box: 4 bytes of version and flags first.
                            for i in children(u.body.get(4..).unwrap_or_default()) {
                                if &i.kind == b"ilst" {
                                    apply_ilst(i.body, &mut t);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            break;
        }
        if a.cut {
            break;
        }
        pos = a.next;
    }
    if found_moov {
        Ok(t)
    } else {
        // `moov` is often at the end of the file: the caller should retry with the tail.
        Err(TagError::Truncated)
    }
}
