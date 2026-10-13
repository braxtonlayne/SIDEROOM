//! Join tokens: the gateway mints one per voice join (`VOICE_SERVER_UPDATE.token`),
//! the SFU verifies it on IDENTIFY. Format: `v1.<claims>.<mac>`, both parts base64url
//! without padding; the MAC is HMAC-SHA256 over `sr-voice-token-v1.<claims>`.

use data_encoding::BASE64URL_NOPAD;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sr_core::{ChannelId, GuildId, UserId};
use thiserror::Error;

type HmacSha256 = Hmac<Sha256>;

/// How long a freshly minted token is valid (VOX-SIG-004).
pub const TOKEN_TTL_SECS: u64 = 60;

const PREFIX: &str = "v1";
const DOMAIN: &str = "sr-voice-token-v1.";

/// What the token asserts. The flags are the state at mint time; later changes
/// (server mute, ...) reach the SFU as [`crate::VoiceCommand`]s.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinClaims {
    pub user_id: UserId,
    /// `None` for DM calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<GuildId>,
    pub channel_id: ChannelId,
    /// The gateway session that joined; used to tell sessions of one user apart.
    pub session_id: String,
    /// Unix seconds.
    pub exp: u64,
    /// May send audio (SPEAK permission and not server muted).
    pub speak: bool,
    /// Server deafened: receives nothing.
    #[serde(default)]
    pub deaf: bool,
    /// Voice activity is denied (no USE_VAD): audio is forwarded only while the client reports push-to-talk.
    #[serde(default)]
    pub ptt_required: bool,
    /// PRIORITY_SPEAKER: speaking events carry the priority bit.
    #[serde(default)]
    pub priority: bool,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TokenError {
    #[error("malformed token")]
    Malformed,
    #[error("bad signature")]
    BadSignature,
    #[error("token expired")]
    Expired,
}

/// Signs and verifies join tokens with one shared secret.
#[derive(Clone)]
pub struct TokenSigner {
    secret: Vec<u8>,
}

impl std::fmt::Debug for TokenSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TokenSigner(***)")
    }
}

impl TokenSigner {
    pub fn new(secret: impl Into<Vec<u8>>) -> Self {
        Self {
            secret: secret.into(),
        }
    }

    fn mac(&self, claims_b64: &str) -> Option<HmacSha256> {
        let mut mac = HmacSha256::new_from_slice(&self.secret).ok()?;
        mac.update(DOMAIN.as_bytes());
        mac.update(claims_b64.as_bytes());
        Some(mac)
    }

    /// Mint a token. `None` only if the secret cannot key an HMAC (never for non-empty keys).
    pub fn sign(&self, claims: &JoinClaims) -> Option<String> {
        let json = serde_json::to_vec(claims).ok()?;
        let body = BASE64URL_NOPAD.encode(&json);
        let tag = self.mac(&body)?.finalize().into_bytes();
        Some(format!("{PREFIX}.{body}.{}", BASE64URL_NOPAD.encode(&tag)))
    }

    /// Check the signature (constant time) and expiry against `now_secs`.
    pub fn verify(&self, token: &str, now_secs: u64) -> Result<JoinClaims, TokenError> {
        let mut parts = token.split('.');
        let (Some(PREFIX), Some(body), Some(tag), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(TokenError::Malformed);
        };
        let tag = BASE64URL_NOPAD
            .decode(tag.as_bytes())
            .map_err(|_| TokenError::Malformed)?;
        self.mac(body)
            .ok_or(TokenError::BadSignature)?
            .verify_slice(&tag)
            .map_err(|_| TokenError::BadSignature)?;
        let json = BASE64URL_NOPAD
            .decode(body.as_bytes())
            .map_err(|_| TokenError::Malformed)?;
        let claims: JoinClaims =
            serde_json::from_slice(&json).map_err(|_| TokenError::Malformed)?;
        if claims.exp <= now_secs {
            return Err(TokenError::Expired);
        }
        Ok(claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claims(exp: u64) -> JoinClaims {
        JoinClaims {
            user_id: UserId::new(1),
            guild_id: Some(GuildId::new(2)),
            channel_id: ChannelId::new(3),
            session_id: "abc".into(),
            exp,
            speak: true,
            deaf: false,
            ptt_required: false,
            priority: false,
        }
    }

    #[test]
    fn round_trip_and_expiry() {
        let s = TokenSigner::new(b"secret".to_vec());
        let t = s.sign(&claims(100)).unwrap();
        assert_eq!(s.verify(&t, 99).unwrap(), claims(100));
        assert_eq!(s.verify(&t, 100), Err(TokenError::Expired));
    }

    #[test]
    fn forged_and_malformed_tokens_are_rejected() {
        let s = TokenSigner::new(b"secret".to_vec());
        let other = TokenSigner::new(b"other".to_vec());
        let t = s.sign(&claims(100)).unwrap();
        assert_eq!(other.verify(&t, 1), Err(TokenError::BadSignature));
        // Swap in different claims under the old MAC.
        let forged_body = BASE64URL_NOPAD.encode(&serde_json::to_vec(&claims(9999)).unwrap());
        let tag = t.rsplit('.').next().unwrap();
        assert_eq!(
            s.verify(&format!("v1.{forged_body}.{tag}"), 1),
            Err(TokenError::BadSignature)
        );
        for bad in ["", "v1", "v1.a", "v2.a.b", "v1.a.b.c", "v1.!!.??"] {
            assert_eq!(s.verify(bad, 1), Err(TokenError::Malformed), "{bad}");
        }
    }
}
