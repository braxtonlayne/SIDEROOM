//! FLAC: STREAMINFO duration, Vorbis comments and PICTURE blocks.

use crate::tags::*;

/// A FLAC PICTURE block body (also the payload of Ogg `METADATA_BLOCK_PICTURE`).
pub(crate) fn parse_picture(b: &[u8]) -> Option<Cover> {
    let mut pos = 4usize; // picture type
    let mime_len = u32_be(b.get(pos..)?)? as usize;
    pos += 4;
    let mime = latin1(b.get(pos..pos.checked_add(mime_len)?)?);
    pos += mime_len;
    let desc_len = u32_be(b.get(pos..)?)? as usize;
    pos = pos.checked_add(4)?.checked_add(desc_len)?;
    pos = pos.checked_add(16)?; // width, height, depth, colours
    let data_len = u32_be(b.get(pos..)?)? as usize;
    pos += 4;
    let data = b.get(pos..pos.checked_add(data_len)?)?;
    if data.is_empty() {
        return None;
    }
    Some(Cover {
        mime: normalise_mime(&mime, data),
        bytes: data.to_vec(),
    })
}

pub fn read(b: &[u8]) -> Result<Tags, TagError> {
    if !b.starts_with(b"fLaC") {
        return Err(TagError::Unsupported);
    }
    let mut t = Tags::default();
    let mut pos = 4usize;
    let mut blocks = 0u32;
    loop {
        let (Some(&head), Some(l)) = (b.get(pos), b.get(pos + 1..pos + 4)) else {
            if blocks > 0 {
                break;
            }
            return Err(TagError::Truncated);
        };
        blocks += 1;
        let len = (usize::from(l[0]) << 16) | (usize::from(l[1]) << 8) | usize::from(l[2]);
        let kind = head & 0x7f;
        let last = head & 0x80 != 0;
        let start = pos + 4;
        let end = start + len;
        let body = b.get(start..end);
        match (kind, body) {
            (0, Some(si)) if si.len() >= 18 => {
                // 20 bits sample rate, 3 bits channels, 5 bits depth, 36 bits total samples.
                let rate =
                    (u64::from(si[10]) << 12) | (u64::from(si[11]) << 4) | u64::from(si[12] >> 4);
                let total = (u64::from(si[13] & 0x0f) << 32)
                    | (u64::from(si[14]) << 24)
                    | (u64::from(si[15]) << 16)
                    | (u64::from(si[16]) << 8)
                    | u64::from(si[17]);
                if rate > 0 && total > 0 {
                    t.duration_hint = Some(total * 1000 / rate);
                }
            }
            (4, Some(vc)) => parse_vorbis_comments(vc, &mut t)?,
            (4, None) => {
                // Truncated comment block: keep the entries that fit.
                if let Some(partial) = b.get(start..) {
                    let _ = parse_vorbis_comments(partial, &mut t);
                }
                break;
            }
            (6, Some(pic)) if t.cover.is_none() => t.cover = parse_picture(pic),
            _ => {}
        }
        if body.is_none() || last {
            break;
        }
        pos = end;
    }
    Ok(t)
}
