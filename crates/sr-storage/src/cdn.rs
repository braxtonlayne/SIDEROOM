//! Signed CDN URLs (`?ex=<unix>&sig=<b64url>`) and HTTP `Range` parsing.
//!
//! Framework-agnostic: sr-api wraps [`CdnSigner::verify`] in middleware.
//! The signature is `HMAC-SHA256(secret, "{path}\n{ex}")`, base64url without padding.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use thiserror::Error;

type HmacSha256 = Hmac<Sha256>;

/// Why a signed URL was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum VerifyError {
    /// `ex` or `sig` missing or malformed.
    #[error("missing or malformed signature parameters")]
    Malformed,
    /// Expiry is in the past.
    #[error("signed URL expired")]
    Expired,
    /// Signature mismatch.
    #[error("invalid signature")]
    BadSignature,
}

/// Generates and verifies signed CDN URLs.
#[derive(Clone)]
pub struct CdnSigner {
    secret: Vec<u8>,
}

impl std::fmt::Debug for CdnSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CdnSigner(..)")
    }
}

impl CdnSigner {
    /// Create a signer from a secret key (use at least 32 random bytes).
    pub fn new(secret: impl Into<Vec<u8>>) -> Self {
        Self {
            secret: secret.into(),
        }
    }

    fn tag(&self, path: &str, expires_at: u64) -> Vec<u8> {
        let mut mac = match HmacSha256::new_from_slice(&self.secret) {
            Ok(m) => m,
            // HMAC accepts any key length; this branch is unreachable.
            Err(_) => return Vec::new(),
        };
        mac.update(path.as_bytes());
        mac.update(b"\n");
        mac.update(expires_at.to_string().as_bytes());
        mac.finalize().into_bytes().to_vec()
    }

    /// Signature for `path` (e.g. `/cdn/attachments/1/2/a.png`) expiring at
    /// `expires_at` (unix seconds).
    pub fn sign(&self, path: &str, expires_at: u64) -> String {
        URL_SAFE_NO_PAD.encode(self.tag(path, expires_at))
    }

    /// Return `path?ex=..&sig=..`.
    pub fn signed_url(&self, path: &str, expires_at: u64) -> String {
        format!("{path}?ex={expires_at}&sig={}", self.sign(path, expires_at))
    }

    /// Verify `ex`/`sig` for `path` at time `now` (unix seconds), comparing in constant time.
    pub fn verify(
        &self,
        path: &str,
        ex: Option<&str>,
        sig: Option<&str>,
        now: u64,
    ) -> Result<(), VerifyError> {
        let expires_at: u64 = ex
            .and_then(|e| e.parse().ok())
            .ok_or(VerifyError::Malformed)?;
        let provided = sig
            .and_then(|s| URL_SAFE_NO_PAD.decode(s).ok())
            .ok_or(VerifyError::Malformed)?;
        let expected = self.tag(path, expires_at);
        let ok: bool = expected.as_slice().ct_eq(&provided).into();
        if !ok {
            return Err(VerifyError::BadSignature);
        }
        if now >= expires_at {
            return Err(VerifyError::Expired);
        }
        Ok(())
    }

    /// Convenience: verify from a raw query string (`ex=1&sig=abc&size=64`).
    /// `size` and `format` in the query are part of the signed string.
    pub fn verify_query(&self, path: &str, query: &str, now: u64) -> Result<(), VerifyError> {
        let (mut ex, mut sig, mut size, mut format) = (None, None, None, None);
        for pair in query.split('&') {
            match pair.split_once('=') {
                Some(("ex", v)) => ex = Some(v),
                Some(("sig", v)) => sig = Some(v),
                Some(("size", v)) => size = Some(v),
                Some(("format", v)) => format = Some(v),
                _ => {}
            }
        }
        let size = match size {
            Some(v) => Some(v.parse::<u32>().map_err(|_| VerifyError::Malformed)?),
            None => None,
        };
        let canonical = variant_canonical(path, size, format);
        self.verify(&canonical, ex, sig, now)
    }

    /// `path?size=..&format=..&ex=..&sig=..`, the signature covering the variant.
    pub fn signed_variant_url(
        &self,
        path: &str,
        size: Option<u32>,
        format: Option<&str>,
        expires_at: u64,
    ) -> String {
        let canonical = variant_canonical(path, size, format);
        let mut q = String::new();
        if let Some(s) = size {
            q.push_str(&format!("size={s}&"));
        }
        if let Some(f) = format {
            q.push_str(&format!("format={}&", f.to_ascii_lowercase()));
        }
        format!(
            "{path}?{q}ex={expires_at}&sig={}",
            self.sign(&canonical, expires_at)
        )
    }
}

/// Longest third-party URL the image proxy signs (matches the unfurler's limit).
pub const MAX_EXTERNAL_URL_LEN: usize = 2048;

impl CdnSigner {
    /// Key for the external image proxy, derived from the server secret so a
    /// signature minted for one purpose can never verify for another.
    fn external_key(&self) -> Vec<u8> {
        match HmacSha256::new_from_slice(&self.secret) {
            Ok(mut m) => {
                m.update(b"sideroom/external-image/v1");
                m.finalize().into_bytes().to_vec()
            }
            Err(_) => Vec::new(),
        }
    }

    fn external_tag(&self, url: &str) -> Vec<u8> {
        match HmacSha256::new_from_slice(&self.external_key()) {
            Ok(mut m) => {
                m.update(url.as_bytes());
                m.finalize().into_bytes().to_vec()
            }
            Err(_) => Vec::new(),
        }
    }

    /// Path of the proxied form of a third-party image URL:
    /// `/cdn/external/{hmac}/{url_b64}`, both parts base64url without padding.
    /// The link has no expiry, so browsers can cache it. `None` for a URL that is
    /// not absolute http(s) or is too long.
    pub fn external_path(&self, url: &str) -> Option<String> {
        let lower = url.get(..8).map(str::to_ascii_lowercase);
        let http = lower
            .as_deref()
            .is_some_and(|l| l.starts_with("https://") || l.starts_with("http://"));
        if !http || url.len() > MAX_EXTERNAL_URL_LEN || url.chars().any(char::is_control) {
            return None;
        }
        Some(format!(
            "/cdn/external/{}/{}",
            URL_SAFE_NO_PAD.encode(self.external_tag(url)),
            URL_SAFE_NO_PAD.encode(url.as_bytes())
        ))
    }

    /// Check `hmac` against `url_b64` from a proxy path and return the URL it
    /// names. Constant-time comparison; nothing is fetched before this passes.
    pub fn verify_external(&self, hmac: &str, url_b64: &str) -> Result<String, VerifyError> {
        if url_b64.len() > MAX_EXTERNAL_URL_LEN * 2 {
            return Err(VerifyError::Malformed);
        }
        let provided = URL_SAFE_NO_PAD
            .decode(hmac)
            .map_err(|_| VerifyError::Malformed)?;
        let raw = URL_SAFE_NO_PAD
            .decode(url_b64)
            .map_err(|_| VerifyError::Malformed)?;
        let url = String::from_utf8(raw).map_err(|_| VerifyError::Malformed)?;
        let ok: bool = self.external_tag(&url).as_slice().ct_eq(&provided).into();
        if ok {
            Ok(url)
        } else {
            Err(VerifyError::BadSignature)
        }
    }
}

/// The string that is signed for an image variant: `path`, then `?size=N` and
/// `&format=F` for the parameters present (format lower-cased). With neither it
/// is `path` itself, so URLs signed before resizing existed stay valid.
pub fn variant_canonical(path: &str, size: Option<u32>, format: Option<&str>) -> String {
    let mut out = path.to_owned();
    let mut sep = '?';
    if let Some(s) = size {
        out.push(sep);
        out.push_str(&format!("size={s}"));
        sep = '&';
    }
    if let Some(f) = format {
        out.push(sep);
        out.push_str(&format!("format={}", f.to_ascii_lowercase()));
    }
    out
}

/// A parsed single byte range from a `Range` header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteRange {
    /// `bytes=a-b` (inclusive end).
    FromTo(u64, u64),
    /// `bytes=a-`
    From(u64),
    /// `bytes=-n` (last n bytes)
    Suffix(u64),
}

/// Why a Range header could not be served as a range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum RangeError {
    /// Unparseable or unsupported (e.g. multi-range); serve the full body (200).
    #[error("malformed or unsupported range header")]
    Malformed,
    /// Valid but outside the object; respond 416.
    #[error("range not satisfiable")]
    Unsatisfiable,
}

/// A resolved range: `start..end` (end exclusive) of an object of known length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedRange {
    /// First byte.
    pub start: u64,
    /// One past the last byte.
    pub end: u64,
    /// Total object length.
    pub total: u64,
}

impl ResolvedRange {
    /// Value for the `Content-Range` response header.
    pub fn content_range(&self) -> String {
        format!("bytes {}-{}/{}", self.start, self.end - 1, self.total)
    }
}

impl ByteRange {
    /// Parse a `Range` header value. Only a single `bytes` range is supported.
    pub fn parse(header: &str) -> Result<Self, RangeError> {
        let spec = header
            .trim()
            .strip_prefix("bytes=")
            .ok_or(RangeError::Malformed)?
            .trim();
        if spec.contains(',') {
            return Err(RangeError::Malformed);
        }
        let (a, b) = spec.split_once('-').ok_or(RangeError::Malformed)?;
        let (a, b) = (a.trim(), b.trim());
        let num = |s: &str| s.parse::<u64>().map_err(|_| RangeError::Malformed);
        match (a.is_empty(), b.is_empty()) {
            (true, true) => Err(RangeError::Malformed),
            (true, false) => Ok(Self::Suffix(num(b)?)),
            (false, true) => Ok(Self::From(num(a)?)),
            (false, false) => {
                let (s, e) = (num(a)?, num(b)?);
                if e < s {
                    return Err(RangeError::Malformed);
                }
                Ok(Self::FromTo(s, e))
            }
        }
    }

    /// Resolve against an object of `total` bytes.
    pub fn resolve(self, total: u64) -> Result<ResolvedRange, RangeError> {
        let (start, end) = match self {
            Self::FromTo(s, e) => (s, e.saturating_add(1).min(total)),
            Self::From(s) => (s, total),
            Self::Suffix(0) => return Err(RangeError::Unsatisfiable),
            Self::Suffix(n) => (total.saturating_sub(n), total),
        };
        if start >= total || start >= end {
            return Err(RangeError::Unsatisfiable);
        }
        Ok(ResolvedRange { start, end, total })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: &str = "/cdn/attachments/1/2/a.png";

    #[test]
    fn sign_verify_roundtrip() {
        let s = CdnSigner::new(b"k".repeat(32));
        let url = s.signed_url(P, 1000);
        let q = url.split_once('?').unwrap().1;
        assert_eq!(s.verify_query(P, q, 999), Ok(()));
    }

    #[test]
    fn rejects_expired_tampered_and_wrong_key() {
        let s = CdnSigner::new(b"k".repeat(32));
        let sig = s.sign(P, 1000);
        assert_eq!(
            s.verify(P, Some("1000"), Some(&sig), 1000),
            Err(VerifyError::Expired)
        );
        assert_eq!(
            s.verify(P, Some("2000"), Some(&sig), 10),
            Err(VerifyError::BadSignature)
        );
        assert_eq!(
            s.verify("/cdn/other", Some("1000"), Some(&sig), 10),
            Err(VerifyError::BadSignature)
        );
        let other = CdnSigner::new(b"z".repeat(32));
        assert_eq!(
            other.verify(P, Some("1000"), Some(&sig), 10),
            Err(VerifyError::BadSignature)
        );
    }

    #[test]
    fn rejects_malformed() {
        let s = CdnSigner::new(b"k".to_vec());
        assert_eq!(s.verify(P, None, Some("x"), 0), Err(VerifyError::Malformed));
        assert_eq!(
            s.verify(P, Some("abc"), Some("x"), 0),
            Err(VerifyError::Malformed)
        );
        assert_eq!(
            s.verify(P, Some("1"), Some("!!!"), 0),
            Err(VerifyError::Malformed)
        );
        assert_eq!(s.verify_query(P, "size=64", 0), Err(VerifyError::Malformed));
    }

    #[test]
    fn variant_signature_covers_size_and_format() {
        let s = CdnSigner::new(b"k".repeat(32));
        let url = s.signed_variant_url(P, Some(128), Some("WebP"), 1000);
        let q = url.split_once('?').unwrap().1;
        assert!(q.starts_with("size=128&format=webp&ex=1000&sig="));
        assert_eq!(s.verify_query(P, q, 10), Ok(()));
        let other_size = q.replace("size=128", "size=256");
        assert_eq!(
            s.verify_query(P, &other_size, 10),
            Err(VerifyError::BadSignature)
        );
        let other_fmt = q.replace("format=webp", "format=png");
        assert_eq!(
            s.verify_query(P, &other_fmt, 10),
            Err(VerifyError::BadSignature)
        );
        let stripped = q.replace("size=128&format=webp&", "");
        assert_eq!(
            s.verify_query(P, &stripped, 10),
            Err(VerifyError::BadSignature)
        );
        // A bare-path signature (old URL) still verifies, but cannot be turned into a variant.
        let bare = s.signed_url(P, 1000);
        let bq = bare.split_once('?').unwrap().1;
        assert_eq!(s.verify_query(P, bq, 10), Ok(()));
        assert_eq!(
            s.verify_query(P, &format!("size=64&{bq}"), 10),
            Err(VerifyError::BadSignature)
        );
        assert_eq!(
            s.verify_query(P, "size=abc&ex=1&sig=x", 0),
            Err(VerifyError::Malformed)
        );
        assert_eq!(variant_canonical(P, Some(64), None), format!("{P}?size=64"));
        assert_eq!(
            variant_canonical(P, None, Some("PNG")),
            format!("{P}?format=png")
        );
    }

    #[test]
    fn external_links_roundtrip_and_resist_tampering() {
        let s = CdnSigner::new(b"k".repeat(32));
        let url = "https://img.example.org/a b/c.png?x=1&y=é";
        let path = s.external_path(url).unwrap();
        let rest = path.strip_prefix("/cdn/external/").unwrap();
        let (sig, b64) = rest.split_once('/').unwrap();
        assert_eq!(s.verify_external(sig, b64).as_deref(), Ok(url));
        // Another URL under the same signature, another key, a mangled signature.
        let other = URL_SAFE_NO_PAD.encode(b"https://evil.example/x.png");
        assert_eq!(
            s.verify_external(sig, &other),
            Err(VerifyError::BadSignature)
        );
        let k2 = CdnSigner::new(b"z".repeat(32));
        assert_eq!(k2.verify_external(sig, b64), Err(VerifyError::BadSignature));
        assert_eq!(s.verify_external("!!", b64), Err(VerifyError::Malformed));
        assert_eq!(s.verify_external(sig, "@@"), Err(VerifyError::Malformed));
        // A CDN signature for the same text does not open the proxy.
        let cdn_sig = s.sign(url, 0);
        assert_eq!(
            s.verify_external(&cdn_sig, b64),
            Err(VerifyError::BadSignature)
        );
        // Stable (cacheable) and restricted to http(s).
        assert_eq!(s.external_path(url).as_deref(), Some(path.as_str()));
        for bad in [
            "ftp://x.test/a.png",
            "/cdn/attachments/1/2/a.png",
            "javascript:alert(1)",
            "data:image/png;base64,AAAA",
            "https://x.test/
",
        ] {
            assert_eq!(s.external_path(bad), None, "{bad}");
        }
        assert_eq!(
            s.external_path(&format!("https://x.test/{}", "a".repeat(2100))),
            None
        );
    }

    #[test]
    fn range_parse_and_resolve() {
        assert_eq!(ByteRange::parse("bytes=0-99"), Ok(ByteRange::FromTo(0, 99)));
        assert_eq!(ByteRange::parse("bytes=5-"), Ok(ByteRange::From(5)));
        assert_eq!(ByteRange::parse("bytes=-10"), Ok(ByteRange::Suffix(10)));
        assert_eq!(
            ByteRange::parse("bytes=0-1,4-5"),
            Err(RangeError::Malformed)
        );
        assert_eq!(ByteRange::parse("items=0-1"), Err(RangeError::Malformed));
        assert_eq!(ByteRange::parse("bytes=9-2"), Err(RangeError::Malformed));
        assert_eq!(ByteRange::parse("bytes=-"), Err(RangeError::Malformed));

        let r = ByteRange::FromTo(0, 99).resolve(50).unwrap();
        assert_eq!((r.start, r.end), (0, 50));
        assert_eq!(r.content_range(), "bytes 0-49/50");
        let r = ByteRange::Suffix(10).resolve(100).unwrap();
        assert_eq!((r.start, r.end), (90, 100));
        assert_eq!(ByteRange::Suffix(500).resolve(100).unwrap().start, 0);
        assert_eq!(
            ByteRange::From(100).resolve(100),
            Err(RangeError::Unsatisfiable)
        );
        assert_eq!(
            ByteRange::Suffix(0).resolve(100),
            Err(RangeError::Unsatisfiable)
        );
        assert_eq!(
            ByteRange::From(0).resolve(0),
            Err(RangeError::Unsatisfiable)
        );
    }
}

/// Percent-encode one path segment, leaving only `A-Z a-z 0-9 - . _ ~` (the alphabet of the CDN paths).
fn encode_segment(s: &str) -> String {
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

/// The CDN path of an object on a storage mount other than the instance bucket (BYOS): `/cdn/mount/<id>/<key
/// segments>`, to be signed with [`CdnSigner::signed_url`]. The server answers it with a redirect to a short
/// presigned URL of the member's bucket and never streams the bytes. `key` is mount-relative and has passed
/// `path::safe_key`.
pub fn mount_path(mount_id: i64, key: &str) -> String {
    let mut p = format!("/cdn/mount/{mount_id}");
    for seg in key.split('/').filter(|s| !s.is_empty()) {
        p.push('/');
        p.push_str(&encode_segment(seg));
    }
    p
}

#[cfg(test)]
mod mount_path_tests {
    use super::*;

    #[test]
    fn a_mount_key_becomes_encoded_segments() {
        assert_eq!(
            mount_path(77, "Movies/Alien (1979)/poster.jpg"),
            "/cdn/mount/77/Movies/Alien%20%281979%29/poster.jpg"
        );
        assert_eq!(mount_path(7, "a//b"), "/cdn/mount/7/a/b");
    }
}
