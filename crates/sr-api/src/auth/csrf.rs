//! CSRF protection for cookie sessions (`X-SR-CSRF`, doc 05 section 2).
//!
//! The token is derived from the secret session token, so a cross-site
//! attacker cannot compute it, and nothing extra is stored. The (HttpOnly)
//! cookie is unreadable by scripts, so the web app fetches the token from
//! `GET /api/v1/sr/csrf`, which cross-origin pages cannot read (CORS), and
//! echoes it in the header on non-safe methods.

use data_encoding::BASE64URL_NOPAD;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

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
}
