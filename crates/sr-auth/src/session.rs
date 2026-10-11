//! Opaque session tokens (doc 02 §8): 256-bit random, stored hashed.
//!
//! The raw token goes only to the client (cookie). The database stores
//! [`TokenHash`] (SHA-256). SHA-256 without a salt is appropriate because the
//! input already has 256 bits of entropy.

use crate::{error::AuthError, random};
use data_encoding::{BASE64URL_NOPAD, HEXLOWER};
use sha2::{Digest, Sha256};
use std::fmt;
use subtle::ConstantTimeEq;

/// Length of a raw token in bytes.
pub const TOKEN_BYTES: usize = 32;

/// SHA-256 digest of a token, the only form persisted.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TokenHash([u8; 32]);

impl TokenHash {
    /// Hash `token` under a domain-separation label so a token minted for one
    /// purpose can never match a hash stored for another.
    pub(crate) fn digest(domain: &str, token: &str) -> Self {
        let mut h = Sha256::new();
        h.update(domain.as_bytes());
        h.update([0u8]);
        h.update(token.as_bytes());
        Self(h.finalize().into())
    }

    /// Raw digest bytes (e.g. for a `bytea` column).
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Rebuild from stored bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, AuthError> {
        <[u8; 32]>::try_from(bytes)
            .map(Self)
            .map_err(|_| AuthError::Malformed)
    }

    /// Lowercase hex form (64 chars).
    pub fn to_hex(&self) -> String {
        HEXLOWER.encode(&self.0)
    }

    /// Constant-time equality.
    pub fn ct_eq(&self, other: &Self) -> bool {
        self.0.ct_eq(&other.0).into()
    }
}

impl fmt::Debug for TokenHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TokenHash(..)")
    }
}

/// Generate a fresh 256-bit token string (base64url, 43 chars).
pub(crate) fn random_token() -> Result<String, AuthError> {
    Ok(BASE64URL_NOPAD.encode(&random::bytes::<TOKEN_BYTES>()?))
}

/// True if `s` has the exact shape of a generated token. Used to reject junk
/// cheaply before hitting the database.
pub(crate) fn token_shape_ok(s: &str) -> bool {
    s.len() == 43
        && BASE64URL_NOPAD
            .decode(s.as_bytes())
            .is_ok_and(|b| b.len() == TOKEN_BYTES)
}

const DOMAIN: &str = "sideroom/session/v1";

/// A freshly minted session token. Shown once (set as cookie), never stored.
pub struct SessionToken(String);

impl SessionToken {
    /// Generate a new 256-bit session token.
    pub fn generate() -> Result<Self, AuthError> {
        Ok(Self(random_token()?))
    }

    /// Parse a token presented by a client. `None` if it cannot possibly be
    /// one of ours.
    pub fn parse(s: &str) -> Option<Self> {
        token_shape_ok(s).then(|| Self(s.to_owned()))
    }

    /// The raw secret, for the cookie value.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// The at-rest representation to look up or persist.
    pub fn hash(&self) -> TokenHash {
        TokenHash::digest(DOMAIN, &self.0)
    }
}

impl fmt::Debug for SessionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionToken(..)")
    }
}

/// Rolling-expiry policy for sessions (ENT-AUTH-005).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionPolicy {
    /// Seconds of inactivity before a session expires (rolls on use).
    pub idle_ttl_secs: i64,
    /// Hard cap in seconds from creation regardless of activity.
    pub absolute_ttl_secs: i64,
}

impl Default for SessionPolicy {
    /// 30 days rolling, 180 days absolute.
    fn default() -> Self {
        Self {
            idle_ttl_secs: 30 * 86_400,
            absolute_ttl_secs: 180 * 86_400,
        }
    }
}

impl SessionPolicy {
    /// Expiry for a session created at `created_at` and used at `now`:
    /// `min(now + idle, created_at + absolute)`.
    pub fn expiry(&self, created_at: i64, now: i64) -> i64 {
        now.saturating_add(self.idle_ttl_secs)
            .min(created_at.saturating_add(self.absolute_ttl_secs))
    }

    /// Whether a session with `expires_at` is still valid at `now`.
    pub fn is_valid(expires_at: i64, now: i64) -> bool {
        now < expires_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_shape() {
        let t = SessionToken::generate().unwrap();
        assert_eq!(t.expose().len(), 43);
        assert!(token_shape_ok(t.expose()));
        assert!(
            t.expose()
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
    }

    #[test]
    fn tokens_are_unique() {
        let a = SessionToken::generate().unwrap();
        let b = SessionToken::generate().unwrap();
        assert_ne!(a.expose(), b.expose());
        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn hash_is_deterministic_and_not_the_token() {
        let t = SessionToken::generate().unwrap();
        let again = SessionToken::parse(t.expose()).unwrap();
        assert_eq!(t.hash(), again.hash());
        assert!(t.hash().ct_eq(&again.hash()));
        assert!(!t.hash().to_hex().contains(t.expose()));
        assert_eq!(t.hash().to_hex().len(), 64);
    }

    #[test]
    fn parse_rejects_junk() {
        for bad in ["", "abc", &"a".repeat(44), &"!".repeat(43), &"a".repeat(42)] {
            assert!(SessionToken::parse(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn domain_separation() {
        let t = SessionToken::generate().unwrap();
        assert_ne!(t.hash(), TokenHash::digest("other", t.expose()));
    }

    #[test]
    fn hash_bytes_roundtrip() {
        let h = SessionToken::generate().unwrap().hash();
        assert_eq!(TokenHash::from_bytes(h.as_bytes()).unwrap(), h);
        assert!(TokenHash::from_bytes(&[0; 31]).is_err());
    }

    #[test]
    fn debug_is_redacted() {
        let t = SessionToken::generate().unwrap();
        assert!(!format!("{t:?}").contains(t.expose()));
        assert!(!format!("{:?}", t.hash()).contains(&t.hash().to_hex()));
    }

    #[test]
    fn rolling_expiry_is_capped() {
        let p = SessionPolicy {
            idle_ttl_secs: 100,
            absolute_ttl_secs: 250,
        };
        assert_eq!(p.expiry(0, 0), 100);
        assert_eq!(p.expiry(0, 100), 200);
        assert_eq!(p.expiry(0, 200), 250);
        assert!(SessionPolicy::is_valid(250, 249));
        assert!(!SessionPolicy::is_valid(250, 250));
    }
}
