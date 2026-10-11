//! Single-use email tokens (ENT-AUTH-012/013): verification and password
//! reset. Same construction as sessions (256-bit, hashed at rest) with a
//! purpose-specific domain so tokens are not interchangeable.

use crate::{
    error::AuthError,
    session::{TokenHash, random_token, token_shape_ok},
};
use std::fmt;

/// What an email token authorises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EmailTokenPurpose {
    /// Confirm ownership of an email address.
    VerifyEmail,
    /// Reset a forgotten password (30 minutes, doc 06 §4).
    ResetPassword,
    /// One-time Owner login link from `sr-cli bootstrap` (1 hour, open
    /// question 43). Logs in but never resets the password.
    LoginLink,
}

impl EmailTokenPurpose {
    fn domain(self) -> &'static str {
        match self {
            Self::VerifyEmail => "sideroom/email-token/verify/v1",
            Self::ResetPassword => "sideroom/email-token/reset/v1",
            Self::LoginLink => "sideroom/email-token/login-link/v1",
        }
    }

    /// Default lifetime in seconds: reset 30 min, verify 24 h, login link 1 h.
    pub fn default_ttl_secs(self) -> i64 {
        match self {
            Self::VerifyEmail => 24 * 3600,
            Self::ResetPassword => 30 * 60,
            Self::LoginLink => 3600,
        }
    }
}

/// A freshly minted token to embed in an emailed link. Never stored raw.
pub struct EmailToken {
    purpose: EmailTokenPurpose,
    raw: String,
}

impl EmailToken {
    /// Generate a new token for `purpose`.
    pub fn generate(purpose: EmailTokenPurpose) -> Result<Self, AuthError> {
        Ok(Self {
            purpose,
            raw: random_token()?,
        })
    }

    /// Parse a token from a link. `None` if the shape is impossible.
    pub fn parse(purpose: EmailTokenPurpose, s: &str) -> Option<Self> {
        token_shape_ok(s).then(|| Self {
            purpose,
            raw: s.to_owned(),
        })
    }

    /// Raw secret for the emailed link.
    pub fn expose(&self) -> &str {
        &self.raw
    }

    /// The purpose this token was minted for.
    pub fn purpose(&self) -> EmailTokenPurpose {
        self.purpose
    }

    /// At-rest hash to store/look up.
    pub fn hash(&self) -> TokenHash {
        TokenHash::digest(self.purpose.domain(), &self.raw)
    }

    /// Expiry timestamp using the purpose's default TTL.
    pub fn expires_at(&self, now: i64) -> i64 {
        self.expires_at_in(now, self.purpose.default_ttl_secs())
    }

    /// Expiry timestamp for an explicit lifetime (from `AuthConfig`).
    pub fn expires_at_in(&self, now: i64, ttl_secs: i64) -> i64 {
        now.saturating_add(ttl_secs)
    }
}

impl fmt::Debug for EmailToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EmailToken({:?}, ..)", self.purpose)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use EmailTokenPurpose::*;

    #[test]
    fn generate_parse_roundtrip() {
        let t = EmailToken::generate(VerifyEmail).unwrap();
        assert_eq!(t.expose().len(), 43);
        let p = EmailToken::parse(VerifyEmail, t.expose()).unwrap();
        assert_eq!(p.hash(), t.hash());
    }

    #[test]
    fn purposes_do_not_cross() {
        let t = EmailToken::generate(ResetPassword).unwrap();
        let as_verify = EmailToken::parse(VerifyEmail, t.expose()).unwrap();
        assert_ne!(t.hash(), as_verify.hash());
    }

    #[test]
    fn not_a_session_hash() {
        let t = EmailToken::generate(VerifyEmail).unwrap();
        let s = crate::session::SessionToken::parse(t.expose()).unwrap();
        assert_ne!(t.hash(), s.hash());
    }

    #[test]
    fn reset_ttl_is_30_minutes() {
        let t = EmailToken::generate(ResetPassword).unwrap();
        assert_eq!(t.expires_at(1_000), 1_000 + 1800);
        assert_eq!(
            EmailToken::generate(VerifyEmail).unwrap().expires_at(0),
            86_400
        );
    }

    #[test]
    fn rejects_junk_and_redacts() {
        assert!(EmailToken::parse(VerifyEmail, "short").is_none());
        let t = EmailToken::generate(VerifyEmail).unwrap();
        assert!(!format!("{t:?}").contains(t.expose()));
        let a = EmailToken::generate(VerifyEmail).unwrap();
        assert_ne!(a.expose(), t.expose());
    }
}
