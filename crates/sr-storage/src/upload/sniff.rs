//! Content-type sniffing from magic bytes. Never trust the client's declared type.

/// Detect a MIME type from the leading bytes, or `None` if unrecognised.
pub fn sniff_mime(data: &[u8]) -> Option<&'static str> {
    let starts = |p: &[u8]| data.starts_with(p);
    if starts(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if starts(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if starts(b"GIF87a") || starts(b"GIF89a") {
        Some("image/gif")
    } else if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else if data.len() >= 12 && &data[4..8] == b"ftyp" {
        match &data[8..12] {
            b"avif" | b"avis" => Some("image/avif"),
            b"heic" | b"heix" | b"mif1" => Some("image/heic"),
            _ => Some("video/mp4"),
        }
    } else if starts(b"%PDF-") {
        Some("application/pdf")
    } else if starts(&[0x1A, 0x45, 0xDF, 0xA3]) {
        Some("video/webm")
    } else if starts(b"OggS") {
        Some("audio/ogg")
    } else if starts(b"ID3") || (data.len() >= 2 && data[0] == 0xFF && data[1] & 0xE0 == 0xE0) {
        Some("audio/mpeg")
    } else if starts(b"fLaC") {
        Some("audio/flac")
    } else if starts(b"PK\x03\x04") {
        Some("application/zip")
    } else {
        None
    }
}

/// True if we can strip metadata from this image type.
pub fn is_strippable_image(mime: &str) -> bool {
    matches!(mime, "image/jpeg" | "image/png")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_types() {
        assert_eq!(sniff_mime(b"\x89PNG\r\n\x1a\n...."), Some("image/png"));
        assert_eq!(sniff_mime(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(sniff_mime(b"GIF89a.."), Some("image/gif"));
        assert_eq!(sniff_mime(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(sniff_mime(b"\0\0\0\x18ftypmp42"), Some("video/mp4"));
        assert_eq!(sniff_mime(b"\0\0\0\x18ftypavif"), Some("image/avif"));
        assert_eq!(sniff_mime(b"%PDF-1.7"), Some("application/pdf"));
        assert_eq!(sniff_mime(b"PK\x03\x04"), Some("application/zip"));
    }

    #[test]
    fn unknown_and_short() {
        assert_eq!(sniff_mime(b""), None);
        assert_eq!(sniff_mime(b"<html><script>"), None);
        assert!(is_strippable_image("image/png") && !is_strippable_image("image/gif"));
    }
}
