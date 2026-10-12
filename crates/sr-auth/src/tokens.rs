//! Bot tokens and OAuth2 bearer tokens (open question 56; docs 02 section 3,
//! 04 section 6, 09 section 2).
//!
//! * Bot token: three base64url parts, Discord style,
//!   `base64url(bot user id).base64url(issued at, unix seconds).base64url(32 random bytes)`.
//!   Only its SHA-256 digest ([`TokenHash`]) is ever stored (`bot_tokens`,
//!   Phase 6, with `applications`).
//! * Bearer tokens are opaque and stored hashed like session tokens.
//!
//! The lookup traits here are the boundary the `sr-api` extractors call. Their
//! Postgres implementations land with the applications tables; until then
//! `sr-api` answers with no bots and no bearers.

use crate::{error::AuthError, random, session::TokenHash};
use async_trait::async_trait;
use data_encoding::BASE64URL_NOPAD;
use std::fmt;

/// Opaque backend failure (maps to a 500).
#[derive(Debug, thiserror::Error)]
#[error("auth backend error: {0}")]
pub struct BackendError(pub String);

/// A bot identified by its token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotIdentity {
    /// The bot's user id.
    pub user_id: i64,
    /// Owning application id.
    pub application_id: i64,
}

/// Bot token verification.
#[async_trait]
pub trait BotBackend: Send + Sync {
    /// `None` when the token is unknown or revoked.
    async fn authenticate(&self, token: &str) -> Result<Option<BotIdentity>, BackendError>;
}

/// An OAuth2 access token's owner and scopes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BearerIdentity {
    /// The authorising user.
    pub user_id: i64,
    /// Granted scopes (`identify`, `guilds`, ...).
    pub scopes: Vec<String>,
}

/// OAuth2 bearer token verification.
#[async_trait]
pub trait BearerBackend: Send + Sync {
    /// `None` when the token is unknown, expired or revoked.
    async fn authenticate(&self, token: &str) -> Result<Option<BearerIdentity>, BackendError>;
}

const BOT_DOMAIN: &str = "sideroom/bot-token/v1";

/// The fields a bot token encodes in its first two parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BotTokenParts {
    /// Bot user id.
    pub user_id: i64,
    /// Issue time, Unix seconds.
    pub issued_at: i64,
}

/// A freshly minted bot token. Shown once; store [`BotToken::hash`] only.
pub struct BotToken(String);

impl BotToken {
    /// Mint a token for `user_id` issued at `issued_at` (Unix seconds).
    pub fn generate(user_id: i64, issued_at: i64) -> Result<Self, AuthError> {
        let secret = BASE64URL_NOPAD.encode(&random::bytes::<32>()?);
        Ok(Self(format!(
            "{}.{}.{secret}",
            BASE64URL_NOPAD.encode(user_id.to_string().as_bytes()),
            BASE64URL_NOPAD.encode(issued_at.to_string().as_bytes()),
        )))
    }

    /// Parse a presented token's shape. `None` if it cannot be one of ours.
    pub fn parse_parts(s: &str) -> Option<BotTokenParts> {
        let mut it = s.split('.');
        let (id, issued, secret) = (it.next()?, it.next()?, it.next()?);
        if it.next().is_some() || BASE64URL_NOPAD.decode(secret.as_bytes()).ok()?.len() != 32 {
            return None;
        }
        let decode_int = |p: &str| -> Option<i64> {
            let bytes = BASE64URL_NOPAD.decode(p.as_bytes()).ok()?;
            std::str::from_utf8(&bytes).ok()?.parse().ok()
        };
        Some(BotTokenParts {
            user_id: decode_int(id)?,
            issued_at: decode_int(issued)?,
        })
    }

    /// The raw token, for the one-time display.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// The at-rest digest.
    pub fn hash(&self) -> TokenHash {
        TokenHash::digest(BOT_DOMAIN, &self.0)
    }
}

impl fmt::Debug for BotToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BotToken(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bot_token_round_trips_its_parts() {
        let t = BotToken::generate(4_242, 1_800_000_000).unwrap();
        assert_eq!(t.expose().split('.').count(), 3);
        assert_eq!(
            BotToken::parse_parts(t.expose()),
            Some(BotTokenParts {
                user_id: 4_242,
                issued_at: 1_800_000_000,
            })
        );
    }

    #[test]
    fn bot_token_shape_is_checked() {
        let t = BotToken::generate(1, 2).unwrap();
        let s = t.expose().to_owned();
        assert!(BotToken::parse_parts("a.b").is_none());
        assert!(BotToken::parse_parts(&format!("{s}.extra")).is_none());
        assert!(BotToken::parse_parts(&format!("!!.{}", s.split('.').nth(1).unwrap())).is_none());
    }

    #[test]
    fn bot_token_hash_is_stable_and_redacted() {
        let t = BotToken::generate(7, 8).unwrap();
        assert_eq!(t.hash(), t.hash());
        assert_ne!(t.hash(), TokenHash::digest("other", t.expose()));
        assert_eq!(format!("{t:?}"), "BotToken(..)");
    }
}
