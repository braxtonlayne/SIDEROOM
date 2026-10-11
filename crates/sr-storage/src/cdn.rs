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
    pub fn verify_query(&self, path: &str, query: &str, now: u64) -> Result<(), VerifyError> {
        let (mut ex, mut sig) = (None, None);
        for pair in query.split('&') {
            match pair.split_once('=') {
                Some(("ex", v)) => ex = Some(v),
                Some(("sig", v)) => sig = Some(v),
                _ => {}
            }
        }
        self.verify(path, ex, sig, now)
    }
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
