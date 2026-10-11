//! Metadata stripping for JPEG and PNG (EXIF, XMP, IPTC, text chunks).
//!
//! Images without an orientation tag keep their pixels and ICC profile
//! untouched. When the EXIF orientation asks for a rotation or flip, the pixels
//! are turned upright first (open question 25), so the stored image displays the
//! same after the tag is gone. Those images are re-encoded: JPEG at quality 92
//! and the ICC profile is dropped.

use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use thiserror::Error;

const JPEG_QUALITY: u8 = 92;

/// Stripping failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum StripError {
    /// Not a JPEG or PNG.
    #[error("unsupported image format")]
    Unsupported,
    /// Truncated or corrupt structure.
    #[error("malformed image data")]
    Malformed,
}

/// Strip metadata from a JPEG or PNG, detected by magic bytes. Applies the EXIF
/// orientation to the pixels first when one is present.
pub fn strip_metadata(data: &[u8]) -> Result<Vec<u8>, StripError> {
    let (format, stripped) = if data.starts_with(&[0xFF, 0xD8]) {
        (ImageFormat::Jpeg, strip_jpeg(data)?)
    } else if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        (ImageFormat::Png, strip_png(data)?)
    } else {
        return Err(StripError::Unsupported);
    };
    Ok(upright(data, format)?.unwrap_or(stripped))
}

/// Decode `data`, apply its EXIF orientation and re-encode. Returns `None` when
/// there is nothing to apply, including when the decoder cannot read the
/// orientation (the byte-level stripper has already validated the structure).
fn upright(data: &[u8], format: ImageFormat) -> Result<Option<Vec<u8>>, StripError> {
    let Ok(mut decoder) = ImageReader::with_format(Cursor::new(data), format).into_decoder() else {
        return Ok(None);
    };
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    if orientation == Orientation::NoTransforms {
        return Ok(None);
    }
    let mut img = DynamicImage::from_decoder(decoder).map_err(|_| StripError::Malformed)?;
    img.apply_orientation(orientation);
    let mut out = Vec::new();
    match format {
        ImageFormat::Jpeg => JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
            .encode_image(&img)
            .map_err(|_| StripError::Malformed)?,
        _ => img
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .map_err(|_| StripError::Malformed)?,
    }
    Ok(Some(out))
}

fn strip_jpeg(data: &[u8]) -> Result<Vec<u8>, StripError> {
    let mut out = Vec::with_capacity(data.len());
    out.extend_from_slice(&data[..2]);
    let mut i = 2;
    loop {
        // Skip fill bytes.
        while data.get(i) == Some(&0xFF) && data.get(i + 1) == Some(&0xFF) {
            i += 1;
        }
        if data.get(i) != Some(&0xFF) {
            return Err(StripError::Malformed);
        }
        let marker = *data.get(i + 1).ok_or(StripError::Malformed)?;
        match marker {
            0xD9 => {
                out.extend_from_slice(&data[i..i + 2]);
                return Ok(out);
            }
            0x01 | 0xD0..=0xD7 => {
                out.extend_from_slice(&data[i..i + 2]);
                i += 2;
            }
            _ => {
                let len_bytes = data.get(i + 2..i + 4).ok_or(StripError::Malformed)?;
                let len = usize::from(u16::from_be_bytes([len_bytes[0], len_bytes[1]]));
                if len < 2 {
                    return Err(StripError::Malformed);
                }
                let end = i + 2 + len;
                let seg = data.get(i..end).ok_or(StripError::Malformed)?;
                // APP1 (EXIF/XMP), APP13 (Photoshop/IPTC), COM.
                let drop = matches!(marker, 0xE1 | 0xED | 0xFE);
                if !drop {
                    out.extend_from_slice(seg);
                }
                i = end;
                if marker == 0xDA {
                    // Start of scan: entropy-coded data follows; copy verbatim.
                    out.extend_from_slice(&data[i..]);
                    return Ok(out);
                }
            }
        }
    }
}

fn strip_png(data: &[u8]) -> Result<Vec<u8>, StripError> {
    let mut out = Vec::with_capacity(data.len());
    out.extend_from_slice(&data[..8]);
    let mut i = 8;
    while i < data.len() {
        let hdr = data.get(i..i + 8).ok_or(StripError::Malformed)?;
        let len = u32::from_be_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]) as usize;
        let kind = &hdr[4..8];
        let end = len
            .checked_add(12)
            .and_then(|n| n.checked_add(i))
            .ok_or(StripError::Malformed)?;
        let chunk = data.get(i..end).ok_or(StripError::Malformed)?;
        let drop = matches!(kind, b"eXIf" | b"tEXt" | b"zTXt" | b"iTXt" | b"tIME");
        if !drop {
            out.extend_from_slice(chunk);
        }
        i = end;
        if kind == b"IEND" {
            return Ok(out);
        }
    }
    Err(StripError::Malformed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contains(hay: &[u8], needle: &[u8]) -> bool {
        hay.windows(needle.len()).any(|w| w == needle)
    }

    fn seg(marker: u8, payload: &[u8]) -> Vec<u8> {
        let mut v = vec![0xFF, marker];
        v.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
        v.extend_from_slice(payload);
        v
    }

    #[test]
    fn jpeg_strips_exif_keeps_rest() {
        let mut j = vec![0xFF, 0xD8];
        j.extend(seg(0xE0, b"JFIF\0"));
        j.extend(seg(0xE1, b"Exif\0\0GPS-SECRET"));
        j.extend(seg(0xE2, b"ICC_PROFILE\0"));
        j.extend(seg(0xFE, b"comment"));
        j.extend(seg(0xDA, &[1, 2, 3]));
        j.extend_from_slice(&[0x11, 0xFF, 0x00, 0x22, 0xFF, 0xD9]);
        let out = strip_metadata(&j).unwrap();
        assert!(!contains(&out, b"GPS-SECRET") && !contains(&out, b"comment"));
        assert!(contains(&out, b"JFIF") && contains(&out, b"ICC_PROFILE"));
        assert!(out.ends_with(&[0x11, 0xFF, 0x00, 0x22, 0xFF, 0xD9]));
        assert!(out.len() < j.len());
    }

    fn chunk(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut v = (payload.len() as u32).to_be_bytes().to_vec();
        v.extend_from_slice(kind);
        v.extend_from_slice(payload);
        v.extend_from_slice(&[0, 0, 0, 0]); // CRC not validated
        v
    }

    #[test]
    fn png_strips_text_and_exif() {
        let mut p = b"\x89PNG\r\n\x1a\n".to_vec();
        p.extend(chunk(b"IHDR", &[0; 13]));
        p.extend(chunk(b"eXIf", b"SECRETEXIF"));
        p.extend(chunk(b"tEXt", b"Author\0me"));
        p.extend(chunk(b"IDAT", b"pixels"));
        p.extend(chunk(b"IEND", b""));
        let out = strip_metadata(&p).unwrap();
        assert!(!contains(&out, b"SECRETEXIF") && !contains(&out, b"Author"));
        assert!(contains(&out, b"IHDR") && contains(&out, b"pixels") && contains(&out, b"IEND"));
    }

    /// A 4x2 JPEG whose EXIF orientation tag is `tag`.
    fn jpeg_with_orientation(tag: u16) -> Vec<u8> {
        let img =
            DynamicImage::ImageRgb8(image::RgbImage::from_pixel(4, 2, image::Rgb([200, 10, 10])));
        let mut base = Vec::new();
        JpegEncoder::new(&mut base).encode_image(&img).unwrap();
        // TIFF header, one IFD entry (Orientation, SHORT, count 1), no next IFD.
        let mut tiff = b"II*\0".to_vec();
        tiff.extend_from_slice(&8u32.to_le_bytes());
        tiff.extend_from_slice(&1u16.to_le_bytes());
        tiff.extend_from_slice(&0x0112u16.to_le_bytes());
        tiff.extend_from_slice(&3u16.to_le_bytes());
        tiff.extend_from_slice(&1u32.to_le_bytes());
        tiff.extend_from_slice(&tag.to_le_bytes());
        tiff.extend_from_slice(&[0, 0]);
        tiff.extend_from_slice(&0u32.to_le_bytes());
        let mut payload = b"Exif\0\0".to_vec();
        payload.extend(tiff);
        let mut out = base[..2].to_vec();
        out.extend(seg(0xE1, &payload));
        out.extend_from_slice(&base[2..]);
        out
    }

    #[test]
    fn orientation_is_applied_and_tag_removed() {
        // Tag 6 means "rotate 90 degrees clockwise to display": 4x2 becomes 2x4.
        let out = strip_metadata(&jpeg_with_orientation(6)).unwrap();
        assert!(!contains(&out, b"Exif"));
        let img = image::load_from_memory(&out).unwrap();
        assert_eq!((img.width(), img.height()), (2, 4));
    }

    #[test]
    fn no_orientation_keeps_dimensions() {
        let out = strip_metadata(&jpeg_with_orientation(1)).unwrap();
        assert!(!contains(&out, b"Exif"));
        let img = image::load_from_memory(&out).unwrap();
        assert_eq!((img.width(), img.height()), (4, 2));
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(strip_metadata(b"GIF89a"), Err(StripError::Unsupported));
        assert_eq!(
            strip_metadata(&[0xFF, 0xD8, 0xFF, 0xE1, 0x00]),
            Err(StripError::Malformed)
        );
        let mut p = b"\x89PNG\r\n\x1a\n".to_vec();
        p.extend(chunk(b"IHDR", &[0; 13]));
        assert_eq!(strip_metadata(&p), Err(StripError::Malformed));
        assert_eq!(
            strip_metadata(&[0xFF, 0xD8, 0xFF, 0xE1, 0xFF, 0xFF]),
            Err(StripError::Malformed)
        );
    }
}
