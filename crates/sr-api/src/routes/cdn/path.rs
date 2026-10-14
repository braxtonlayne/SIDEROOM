//! URL path <-> object key mapping for `/cdn/...`.
//!
//! A CDN path is `/cdn/<kind>/<segment>/...`. Each segment is percent-encoded
//! with a strict alphabet (see [`encode_segment`]); the *canonical* path built
//! from the decoded segments is what the HMAC signs, so a client may send
//! `%20` or a literal space and still verify.
//!
//! | kind          | object key prefix | written by                       |
//! |---------------|-------------------|----------------------------------|
//! | `attachments` | `chat/`           | message attachments              |
//! | `emoji`       | `emoji/`          | custom emoji (`emoji/{guild}/{id}.{ext}`) |
//! | `stickers`    | `sticker/`        | guild stickers                   |
//! | `avatars`     | `avatars/`        | user avatars/banners (`avatars/{user}/{hash}`) |
//! | `icons`       | `icons/`          | guild icon/banner/splash (`icons/{guild}/{hash}`) |
//! | `library`     | `library/`        | Library covers and reader pages (`library/{series}/{chapter}/{page}.webp`), signed |
//! | `cinema`      | `cinema/`         | Cinema artwork (`cinema/{title}/poster.webp`, stills), signed |
//! | `hls`         | `video/`          | Cinema HLS assets (`video/{asset}/v/1080p/seg_00001.m4s`, subtitles, sprites), signed; playlists are rendered on request (`PlaylistRenderer`) |
//!
//! `avatars` and `icons` are *public kinds*: the URL is `/cdn/avatars/{user}/{hash}.{ext}`
//! where `ext` (png, jpg, webp, gif) selects the output format and is not part of the
//! object key; the unguessable content hash is the credential, so no signature is needed.

use thiserror::Error;

/// `(url kind, object key prefix)`.
pub const KINDS: &[(&str, &str)] = &[
    ("attachments", "chat"),
    ("emoji", "emoji"),
    ("stickers", "sticker"),
    ("avatars", "avatars"),
    ("icons", "icons"),
    ("library", "library"),
    ("cinema", "cinema"),
    ("hls", "video"),
];

const MAX_PATH: usize = 1024;
const MAX_SEGMENTS: usize = 8;
const MAX_SEGMENT: usize = 255;

/// Why a path was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum PathError {
    /// Not under an allowed kind.
    #[error("unknown cdn kind")]
    UnknownKind,
    /// Traversal, empty segment, backslash, bad escape, control character, too long.
    #[error("invalid cdn path")]
    Invalid,
}

/// A validated CDN path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdnPath {
    /// The url kind (`attachments`, `hls`, ...).
    pub kind: String,
    /// Canonical wire form that is signed.
    pub canonical: String,
    /// Object storage key.
    pub key: String,
    /// Last decoded segment (the file name). For public kinds this is the
    /// hash without its extension.
    pub filename: String,
    /// `avatars` / `icons`: content-hash addressed, servable without a signature.
    pub public: bool,
    /// Output format named by the extension of a public-kind URL.
    pub format_hint: Option<sr_storage::img::ImgFormat>,
}

/// Kinds addressed by an unguessable content hash (see the module docs).
pub const PUBLIC_KINDS: &[&str] = &["avatars", "icons"];

/// Percent-encode one path segment, leaving only `A-Z a-z 0-9 - . _ ~`.
pub fn encode_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Build the canonical (to-be-signed) path `/cdn/<kind>/<seg>/...`. This is
/// the helper to use when constructing CDN URLs; sign the result with
/// `CdnSigner::signed_url`.
pub fn cdn_path(kind: &str, segments: &[&str]) -> String {
    let mut p = format!("/cdn/{kind}");
    for s in segments {
        p.push('/');
        p.push_str(&encode_segment(s));
    }
    p
}

fn decode_segment(raw: &str) -> Result<String, PathError> {
    let b = raw.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = raw.get(i + 1..i + 3).ok_or(PathError::Invalid)?;
            if !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
                return Err(PathError::Invalid);
            }
            out.push(u8::from_str_radix(hex, 16).map_err(|_| PathError::Invalid)?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| PathError::Invalid)
}

/// Validate a raw request path (as received on the wire, no query) and map it
/// to an object key.
pub fn parse(raw: &str) -> Result<CdnPath, PathError> {
    if raw.len() > MAX_PATH {
        return Err(PathError::Invalid);
    }
    let rest = raw.strip_prefix("/cdn/").ok_or(PathError::Invalid)?;
    if rest.contains('\\') {
        return Err(PathError::Invalid);
    }
    let raw_segs: Vec<&str> = rest.split('/').collect();
    if raw_segs.len() < 2 || raw_segs.len() > MAX_SEGMENTS {
        // A kind alone (or nothing) never names an object.
        return Err(PathError::Invalid);
    }
    let prefix = KINDS
        .iter()
        .find(|(k, _)| *k == raw_segs[0])
        .map(|(_, p)| *p)
        .ok_or(PathError::UnknownKind)?;
    let mut decoded = Vec::with_capacity(raw_segs.len() - 1);
    for seg in &raw_segs[1..] {
        if seg.is_empty() {
            return Err(PathError::Invalid);
        }
        let d = decode_segment(seg)?;
        if d.is_empty()
            || d == "."
            || d == ".."
            || d.len() > MAX_SEGMENT
            || d.contains(['/', '\\'])
            || d.chars().any(char::is_control)
        {
            return Err(PathError::Invalid);
        }
        decoded.push(d);
    }
    let refs: Vec<&str> = decoded.iter().map(String::as_str).collect();
    let canonical = cdn_path(raw_segs[0], &refs);
    let public = PUBLIC_KINDS.contains(&raw_segs[0]);
    let mut stored = decoded;
    let mut format_hint = None;
    if public && let Some(last) = stored.last_mut() {
        let split = last
            .rsplit_once('.')
            .filter(|(stem, _)| !stem.is_empty())
            .and_then(|(stem, ext)| {
                sr_storage::img::ImgFormat::from_name(ext).map(|f| (stem.to_owned(), f))
            });
        if let Some((stem, f)) = split {
            format_hint = Some(f);
            *last = stem;
        }
    }
    Ok(CdnPath {
        kind: raw_segs[0].to_owned(),
        canonical,
        key: format!("{prefix}/{}", stored.join("/")),
        filename: stored.last().cloned().unwrap_or_default(),
        public,
        format_hint,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_kinds_to_keys() {
        let p = parse("/cdn/attachments/1/2/a.png").unwrap();
        assert_eq!(p.key, "chat/1/2/a.png");
        assert_eq!(p.canonical, "/cdn/attachments/1/2/a.png");
        assert_eq!(parse("/cdn/emoji/9/3.gif").unwrap().key, "emoji/9/3.gif");
        assert_eq!(
            parse("/cdn/stickers/9/3.png").unwrap().key,
            "sticker/9/3.png"
        );
    }

    #[test]
    fn public_kinds_strip_the_format_extension() {
        let p = parse("/cdn/avatars/7/abc123.WebP").unwrap();
        assert_eq!(p.key, "avatars/7/abc123");
        assert_eq!(p.canonical, "/cdn/avatars/7/abc123.WebP");
        assert!(p.public);
        assert_eq!(p.format_hint, Some(sr_storage::img::ImgFormat::Webp));
        let p = parse("/cdn/icons/9/a_ff.gif").unwrap();
        assert_eq!(p.key, "icons/9/a_ff");
        let p = parse("/cdn/avatars/7/abc123").unwrap();
        assert_eq!((p.key.as_str(), p.format_hint), ("avatars/7/abc123", None));
        // Other kinds keep the extension and are not public.
        let p = parse("/cdn/attachments/1/2/a.png").unwrap();
        assert!(!p.public);
        assert_eq!(p.key, "chat/1/2/a.png");
        assert_eq!(p.format_hint, None);
    }

    #[test]
    fn canonicalises_encoding() {
        let a = parse("/cdn/attachments/1/2/my%20f%C3%A9.png").unwrap();
        let b = parse("/cdn/attachments/1/2/my%20f%c3%a9.png").unwrap();
        assert_eq!(a.canonical, b.canonical);
        assert_eq!(a.filename, "my fé.png");
        assert_eq!(a.key, "chat/1/2/my fé.png");
        assert_eq!(
            cdn_path("attachments", &["1", "2", "my fé.png"]),
            a.canonical
        );
    }

    #[test]
    fn rejects_bad_paths() {
        for p in [
            "/cdn/attachments/../x",
            "/cdn/attachments/1/./x",
            "/cdn/attachments/%2e%2e/x",
            "/cdn/attachments/%2E%2E/x",
            "/cdn/attachments/1/..%2f..%2fetc",
            "/cdn/attachments/1%5Cx/y",
            "/cdn/attachments/1\\x/y",
            "/cdn/attachments//x",
            "/cdn/attachments/1/x/",
            "/cdn/attachments",
            "/cdn/attachments/",
            "/cdn//attachments/x",
            "/cdn/attachments/%zz/x",
            "/cdn/attachments/%/x",
            "/cdn/attachments/%00/x",
            "/cdn/attachments/%ff/x",
            "/cdn/attachments/%0a/x",
            "//cdn/attachments/1/x",
            "/cdnx/attachments/1/x",
            "/cdn/emoji/%2F/x",
        ] {
            assert!(parse(p).is_err(), "{p} must be refused");
        }
        assert_eq!(parse("/cdn/secrets/1/x"), Err(PathError::UnknownKind));
        assert_eq!(parse("/cdn/chat/1/x"), Err(PathError::UnknownKind));
        assert_eq!(parse("/cdn/avatars%2F1/x"), Err(PathError::UnknownKind));
    }
}
