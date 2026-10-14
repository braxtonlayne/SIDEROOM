//! The MIME allowlist per upload surface (XC-SEC-002).
//!
//! Every place that accepts bytes from a user names its [`Surface`] here, so the answer to
//! "what may be uploaded where" lives in one table. The declared type is never trusted:
//! the bytes are sniffed ([`sniff_mime`]) and checked against the surface.
//!
//! **SVG decision.** SVG (and HTML or XML, which can carry script) is rejected on every
//! surface that is rendered inline (profile images, expressions). There is no sanitizer in
//! the workspace and writing one is a larger attack surface than refusing the format. On
//! the attachment surface the bytes are kept as a download only: the stored type becomes
//! `application/octet-stream`, which the CDN serves as an attachment with `nosniff` and a
//! sandboxing CSP, so it can never execute on the site's origin.

use super::sniff::sniff_mime;

/// Where an upload is going.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// Chat attachments: any file, but only ever served as sniffed media or a download.
    ChatAttachment,
    /// Avatars, banners, guild icons and splashes (re-encoded on upload).
    ProfileImage,
    /// Custom emoji and sticker images.
    Expression,
}

/// Why a file was refused for a surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SurfaceError {
    /// SVG, HTML or XML: can carry script, never accepted for inline images.
    #[error("SVG, HTML and XML files are not accepted here")]
    ActiveContent,
    /// The bytes are not a type this surface accepts.
    #[error("this file type is not accepted here")]
    NotAllowed,
}

const RASTER: &[&str] = &["image/png", "image/jpeg", "image/gif", "image/webp"];

impl Surface {
    /// Sniffed types accepted as images on this surface. `None` means "any file".
    pub fn allowed_images(self) -> Option<&'static [&'static str]> {
        match self {
            Surface::ChatAttachment => None,
            Surface::ProfileImage | Surface::Expression => Some(RASTER),
        }
    }

    /// Check `head` (the leading bytes of the file) for this surface.
    ///
    /// Returns the sniffed MIME type, or `None` for an opaque file that is only valid
    /// where [`Self::allowed_images`] is `None`.
    pub fn check(self, head: &[u8]) -> Result<Option<&'static str>, SurfaceError> {
        if is_active_content(head) {
            return match self.allowed_images() {
                None => Ok(None),
                Some(_) => Err(SurfaceError::ActiveContent),
            };
        }
        let sniffed = sniff_mime(head);
        match (self.allowed_images(), sniffed) {
            (None, s) => Ok(s),
            (Some(allowed), Some(s)) if allowed.contains(&s) => Ok(Some(s)),
            (Some(_), _) => Err(SurfaceError::NotAllowed),
        }
    }
}

/// True when `head` looks like SVG, HTML or XML (what a browser might execute): after an
/// optional BOM and whitespace, a `<` opening a known tag, declaration or comment. This is
/// deliberately broad; a false positive only turns an attachment into a download.
pub fn is_active_content(head: &[u8]) -> bool {
    let head = head.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(head);
    let rest = match head.iter().position(|b| !b.is_ascii_whitespace()) {
        Some(i) => &head[i..],
        None => return false,
    };
    if rest.first() != Some(&b'<') {
        return false;
    }
    let lower: Vec<u8> = rest
        .iter()
        .take(64)
        .map(|b| b.to_ascii_lowercase())
        .collect();
    [
        &b"<svg"[..],
        b"<?xml",
        b"<!doctype",
        b"<html",
        b"<script",
        b"<!--",
        b"<head",
        b"<body",
        b"<iframe",
        b"<math",
    ]
    .iter()
    .any(|p| lower.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n....";

    #[test]
    fn active_content_detection_table() {
        for yes in [
            &b"<svg xmlns='http://www.w3.org/2000/svg'/>"[..],
            b"  \n<SVG onload=alert(1)>",
            b"\xEF\xBB\xBF<?xml version='1.0'?><svg/>",
            b"<!DOCTYPE html><html>",
            b"<html><script>",
            b"<script>alert(1)</script>",
            b"<!-- x --><svg/>",
        ] {
            assert!(is_active_content(yes), "{:?}", String::from_utf8_lossy(yes));
        }
        for no in [PNG, b"hello <svg>", b"plain text", b"", b"   ", b"%PDF-1.7"] {
            assert!(!is_active_content(no), "{:?}", String::from_utf8_lossy(no));
        }
    }

    #[test]
    fn inline_surfaces_accept_only_raster_and_never_markup() {
        for s in [Surface::ProfileImage, Surface::Expression] {
            assert_eq!(s.check(PNG), Ok(Some("image/png")));
            assert_eq!(s.check(&[0xFF, 0xD8, 0xFF, 0xE0]), Ok(Some("image/jpeg")));
            assert_eq!(s.check(b"GIF89a.."), Ok(Some("image/gif")));
            assert_eq!(s.check(b"<svg/>"), Err(SurfaceError::ActiveContent));
            assert_eq!(
                s.check(b"<html><script>alert(1)</script>"),
                Err(SurfaceError::ActiveContent)
            );
            // Real but non-raster types, and unknown bytes, are not images.
            assert_eq!(s.check(b"%PDF-1.7"), Err(SurfaceError::NotAllowed));
            assert_eq!(
                s.check(b"\0\0\0\x18ftypmp42"),
                Err(SurfaceError::NotAllowed)
            );
            assert_eq!(s.check(b"random"), Err(SurfaceError::NotAllowed));
        }
    }

    #[test]
    fn attachments_take_anything_but_markup_has_no_type() {
        let a = Surface::ChatAttachment;
        assert_eq!(a.check(PNG), Ok(Some("image/png")));
        assert_eq!(a.check(b"%PDF-1.7"), Ok(Some("application/pdf")));
        assert_eq!(a.check(b"random"), Ok(None));
        // Markup is accepted as an opaque download: no sniffed type, so it is stored as
        // application/octet-stream and never rendered inline.
        assert_eq!(a.check(b"<svg onload=alert(1)>"), Ok(None));
    }
}
