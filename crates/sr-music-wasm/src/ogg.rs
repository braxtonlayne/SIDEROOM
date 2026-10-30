//! Ogg Vorbis and Opus: the comment packet (second packet of the first logical stream).

use crate::tags::*;

/// Collect the first `want` packets from the pages in `b`. The last packet may be cut off by the
/// end of the buffer; `bool` says so.
fn packets(b: &[u8], want: usize) -> Result<(Vec<Vec<u8>>, bool), TagError> {
    let mut out: Vec<Vec<u8>> = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    let mut pos = 0usize;
    let mut serial: Option<u32> = None;
    while out.len() < want {
        let Some(h) = b.get(pos..pos + 27) else {
            return Ok((finish(out, cur), true));
        };
        if &h[..4] != b"OggS" || h[4] != 0 {
            return Err(TagError::Malformed("Ogg page"));
        }
        let page_serial = u32_le(&h[14..18]).unwrap_or(0);
        let nseg = usize::from(h[26]);
        let Some(table) = b.get(pos + 27..pos + 27 + nseg) else {
            return Ok((finish(out, cur), true));
        };
        let mut data_pos = pos + 27 + nseg;
        let page_len: usize = table.iter().map(|s| usize::from(*s)).sum();
        let same = *serial.get_or_insert(page_serial) == page_serial;
        for &seg in table {
            let seg = usize::from(seg);
            let Some(chunk) = b.get(data_pos..data_pos + seg) else {
                // Page cut off inside its data.
                let rest = b.get(data_pos..).unwrap_or_default();
                if same {
                    cur.extend_from_slice(rest);
                }
                return Ok((finish(out, cur), true));
            };
            data_pos += seg;
            if same {
                cur.extend_from_slice(chunk);
                if seg < 255 {
                    out.push(std::mem::take(&mut cur));
                    if out.len() >= want {
                        break;
                    }
                }
            }
        }
        pos = pos + 27 + nseg + page_len;
    }
    Ok((out, false))
}

fn finish(mut out: Vec<Vec<u8>>, cur: Vec<u8>) -> Vec<Vec<u8>> {
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub fn read(b: &[u8]) -> Result<Tags, TagError> {
    if !b.starts_with(b"OggS") {
        return Err(TagError::Unsupported);
    }
    let (pk, cut) = packets(b, 2)?;
    let ident = pk.first().ok_or(TagError::Truncated)?;
    let comments = pk.get(1).ok_or(TagError::Truncated)?;
    let mut t = Tags::default();
    let body: &[u8] = if ident.starts_with(b"\x01vorbis") {
        comments
            .strip_prefix(b"\x03vorbis".as_slice())
            .ok_or(TagError::Malformed("Vorbis comment header"))?
    } else if ident.starts_with(b"OpusHead") {
        comments
            .strip_prefix(b"OpusTags".as_slice())
            .ok_or(TagError::Malformed("OpusTags header"))?
    } else {
        return Err(TagError::Unsupported);
    };
    if cut {
        // Keep the entries that fit; a cut-off cover is dropped.
        let _ = parse_vorbis_comments(body, &mut t);
    } else {
        parse_vorbis_comments(body, &mut t)?;
    }
    Ok(t)
}
