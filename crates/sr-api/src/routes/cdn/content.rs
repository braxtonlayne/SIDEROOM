//! Response content-type hardening for user uploads.
//!
//! Only image, video and audio types on the allow-list below are ever served
//! with their real type and `inline`. Everything else (HTML, SVG, XML, PDF,
//! scripts, unknown) is forced to `application/octet-stream` +
//! `Content-Disposition: attachment`, so an upload can never execute in the
//! site's origin. Every response also carries `nosniff` and a sandboxing CSP.

/// How to present an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Presentation {
    /// `Content-Type` value.
    pub content_type: &'static str,
    /// True for safe media shown in the page; false forces a download.
    pub inline: bool,
}

const DOWNLOAD: Presentation = Presentation {
    content_type: "application/octet-stream",
    inline: false,
};

fn media(content_type: &'static str) -> Presentation {
    Presentation {
        content_type,
        inline: true,
    }
}

/// Map a lowercase MIME type to its presentation (allow-list).
pub fn from_mime(mime: &str) -> Presentation {
    match mime {
        "image/png" => media("image/png"),
        "image/jpeg" => media("image/jpeg"),
        "image/gif" => media("image/gif"),
        "image/webp" => media("image/webp"),
        "image/avif" => media("image/avif"),
        "video/mp4" => media("video/mp4"),
        "video/webm" => media("video/webm"),
        "video/quicktime" => media("video/quicktime"),
        "video/iso.segment" => media("video/iso.segment"),
        "text/vtt" => media("text/vtt"),
        "audio/mpeg" => media("audio/mpeg"),
        "audio/ogg" => media("audio/ogg"),
        "audio/flac" => media("audio/flac"),
        "audio/wav" => media("audio/wav"),
        "audio/mp4" => media("audio/mp4"),
        "audio/aac" => media("audio/aac"),
        _ => DOWNLOAD,
    }
}

/// Type implied by the file extension, if it is on the media allow-list.
pub fn from_extension(filename: &str) -> Option<Presentation> {
    let ext = filename.rsplit_once('.')?.1.to_ascii_lowercase();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "m4s" => "video/iso.segment",
        "vtt" => "text/vtt",
        "mp3" => "audio/mpeg",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        _ => return None,
    };
    Some(from_mime(mime))
}

/// Presentation for sniffed leading bytes (fallback for unknown extensions).
pub fn from_sniffed(head: &[u8]) -> Presentation {
    sr_storage::upload::sniff_mime(head).map_or(DOWNLOAD, from_mime)
}

/// `Content-Disposition` value: `inline` or `attachment`, with an ASCII
/// fallback `filename` and an RFC 8187 `filename*`.
pub fn disposition(p: Presentation, filename: &str) -> String {
    let ascii: String = filename
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let kind = if p.inline { "inline" } else { "attachment" };
    format!(
        "{kind}; filename=\"{ascii}\"; filename*=UTF-8''{}",
        super::path::encode_segment(filename)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_and_svg_never_inline() {
        for n in ["a.html", "a.htm", "a.svg", "a.xml", "a.js", "a.pdf", "a"] {
            assert!(from_extension(n).is_none(), "{n}");
        }
        assert_eq!(from_mime("text/html"), DOWNLOAD);
        assert_eq!(from_mime("image/svg+xml"), DOWNLOAD);
        assert_eq!(from_sniffed(b"<html><script>"), DOWNLOAD);
        assert_eq!(from_sniffed(b"%PDF-1.7"), DOWNLOAD);
    }

    #[test]
    fn media_is_inline() {
        assert_eq!(from_extension("X.PNG").unwrap().content_type, "image/png");
        assert!(from_extension("a.mp4").unwrap().inline);
        assert!(from_sniffed(b"\x89PNG\r\n\x1a\nxxxx").inline);
    }

    #[test]
    fn disposition_escapes() {
        let d = disposition(DOWNLOAD, "a\"b é.html");
        assert_eq!(
            d,
            "attachment; filename=\"a_b _.html\"; filename*=UTF-8''a%22b%20%C3%A9.html"
        );
    }
}
