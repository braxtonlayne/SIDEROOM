//! CSRF protection for cookie sessions (`X-SR-CSRF`, doc 05 section 2).
//!
//! The token is derived from the secret session token, so a cross-site
//! attacker cannot compute it, and nothing extra is stored. The (HttpOnly)
//! cookie is unreadable by scripts, so the web app fetches the token from
//! `GET /api/v1/sr/csrf`, which cross-origin pages cannot read (CORS), and
//! echoes it in the header on non-safe methods.

use axum::http::{HeaderMap, header};
use data_encoding::BASE64URL_NOPAD;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// Whether a cookie-authenticated request came from an allowed origin. The
/// browser's `Origin` decides; without one, the `Referer`'s origin does. A
/// request carrying neither is allowed (no browser sent it, so no ambient
/// cookie can have been attached cross-site). `Origin: null` is refused.
pub fn origin_allowed(headers: &HeaderMap, allowed: &[String]) -> bool {
    let (raw, from_referer) = match headers.get(header::ORIGIN) {
        Some(v) => (v, false),
        None => match headers.get(header::REFERER) {
            Some(v) => (v, true),
            None => return true,
        },
    };
    let Some(url) = raw.to_str().ok() else {
        return false;
    };
    let origin = if from_referer {
        match url_origin(url) {
            Some(o) => o,
            None => return false,
        }
    } else {
        url
    };
    allowed.iter().any(|a| a == origin.trim_end_matches('/'))
}

/// `scheme://authority` of an absolute URL.
fn url_origin(url: &str) -> Option<&str> {
    let start = url.find("://")? + 3;
    let end = url[start..].find('/').map_or(url.len(), |i| start + i);
    Some(&url[..end])
}

/// Header carrying the token.
pub const CSRF_HEADER: &str = "x-sr-csrf";

/// The CSRF token for a raw session token.
pub fn token_for(raw_session_token: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"sideroom/csrf/v1");
    h.update([0u8]);
    h.update(raw_session_token.as_bytes());
    BASE64URL_NOPAD.encode(&h.finalize())
}

/// Constant-time check of a presented token.
pub fn verify(raw_session_token: &str, presented: &str) -> bool {
    let expected = token_for(raw_session_token);
    expected.as_bytes().ct_eq(presented.as_bytes()).into()
}

/// Methods that do not need CSRF protection.
pub fn is_safe_method(m: &axum::http::Method) -> bool {
    matches!(
        *m,
        axum::http::Method::GET
            | axum::http::Method::HEAD
            | axum::http::Method::OPTIONS
            | axum::http::Method::TRACE
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let t = token_for("secret");
        assert!(verify("secret", &t));
        assert!(!verify("other", &t));
        assert!(!verify("secret", ""));
        assert!(!verify("secret", "x"));
    }

    fn with(name: header::HeaderName, value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(name, value.parse().unwrap());
        h
    }

    #[test]
    fn origin_checks() {
        let allowed = vec!["https://chat.example.org".to_owned()];
        assert!(origin_allowed(
            &with(header::ORIGIN, "https://chat.example.org"),
            &allowed
        ));
        assert!(!origin_allowed(
            &with(header::ORIGIN, "https://evil.example"),
            &allowed
        ));
        assert!(!origin_allowed(&with(header::ORIGIN, "null"), &allowed));
        assert!(origin_allowed(
            &with(header::REFERER, "https://chat.example.org/lounge?x=1"),
            &allowed
        ));
        assert!(!origin_allowed(
            &with(header::REFERER, "https://evil.example/page"),
            &allowed
        ));
        // Neither header: no browser involved, nothing to compare.
        assert!(origin_allowed(&HeaderMap::new(), &allowed));
    }
}
