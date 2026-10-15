//! Ed25519 signing of HTTP interactions (DEV-BOT-005).
//!
//! Every application owns a key pair. The platform signs `timestamp || body` of each
//! request it makes to the application's interactions endpoint with the private half; the
//! developer verifies with the public half (`verify_key`, hex), exactly like Discord's
//! `X-Signature-Ed25519` / `X-Signature-Timestamp` scheme, so Discord libraries' verify
//! helpers work unchanged.

use data_encoding::HEXLOWER;
use ed25519_dalek::{Signature, Signer as _, SigningKey, VerifyingKey};
use sr_auth::AuthError;

/// The 32-byte private seed of an application's signing key. Never printed.
#[derive(Clone, PartialEq, Eq)]
pub struct SigningSeed([u8; 32]);

impl std::fmt::Debug for SigningSeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SigningSeed(..)")
    }
}

impl SigningSeed {
    /// A fresh random seed from the operating system.
    pub fn generate() -> Result<Self, AuthError> {
        Ok(Self(sr_auth::random::bytes::<32>()?))
    }

    /// From stored bytes; `None` unless exactly 32 bytes.
    pub fn from_slice(b: &[u8]) -> Option<Self> {
        <[u8; 32]>::try_from(b).ok().map(Self)
    }

    /// The raw bytes, for storage only.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// The public key, lower-case hex (64 characters): the application's `verify_key`.
    pub fn verify_key_hex(&self) -> String {
        HEXLOWER.encode(SigningKey::from_bytes(&self.0).verifying_key().as_bytes())
    }

    /// Signature over `timestamp || body`, lower-case hex (128 characters).
    pub fn sign_hex(&self, timestamp: &str, body: &[u8]) -> String {
        let mut msg = Vec::with_capacity(timestamp.len() + body.len());
        msg.extend_from_slice(timestamp.as_bytes());
        msg.extend_from_slice(body);
        HEXLOWER.encode(&SigningKey::from_bytes(&self.0).sign(&msg).to_bytes())
    }
}

/// Check a signature the way a developer's endpoint must. `false` for anything malformed.
pub fn verify_hex(verify_key_hex: &str, signature_hex: &str, timestamp: &str, body: &[u8]) -> bool {
    let (Ok(key), Ok(sig)) = (
        HEXLOWER.decode(verify_key_hex.to_ascii_lowercase().as_bytes()),
        HEXLOWER.decode(signature_hex.to_ascii_lowercase().as_bytes()),
    ) else {
        return false;
    };
    let (Ok(key), Ok(sig)) = (<[u8; 32]>::try_from(key), <[u8; 64]>::try_from(sig)) else {
        return false;
    };
    let Ok(key) = VerifyingKey::from_bytes(&key) else {
        return false;
    };
    let mut msg = Vec::with_capacity(timestamp.len() + body.len());
    msg.extend_from_slice(timestamp.as_bytes());
    msg.extend_from_slice(body);
    key.verify_strict(&msg, &Signature::from_bytes(&sig))
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc8032_vector_1_matches() {
        // RFC 8032 section 7.1, test 1 (empty message): seed, public key, signature.
        let seed = HEXLOWER
            .decode(b"9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60")
            .unwrap();
        let s = SigningSeed::from_slice(&seed).unwrap();
        assert_eq!(
            s.verify_key_hex(),
            "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
        );
        assert_eq!(
            s.sign_hex("", b""),
            "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"
        );
    }

    #[test]
    fn sign_and_verify_round_trip_and_reject_tampering() {
        let s = SigningSeed::generate().unwrap();
        let vk = s.verify_key_hex();
        assert_eq!(vk.len(), 64);
        let sig = s.sign_hex("1700000000", br#"{"type":1}"#);
        assert_eq!(sig.len(), 128);
        assert!(verify_hex(&vk, &sig, "1700000000", br#"{"type":1}"#));
        assert!(verify_hex(
            &vk.to_uppercase(),
            &sig.to_uppercase(),
            "1700000000",
            br#"{"type":1}"#
        ));
        assert!(!verify_hex(&vk, &sig, "1700000001", br#"{"type":1}"#));
        assert!(!verify_hex(&vk, &sig, "1700000000", br#"{"type":2}"#));
        let other = SigningSeed::generate().unwrap();
        assert!(!verify_hex(
            &other.verify_key_hex(),
            &sig,
            "1700000000",
            br#"{"type":1}"#
        ));
        for junk in ["", "zz", &"0".repeat(128), &sig[..126]] {
            assert!(!verify_hex(&vk, junk, "1700000000", br#"{"type":1}"#));
        }
    }

    #[test]
    fn debug_hides_the_seed() {
        let s = SigningSeed::from_slice(&[7u8; 32]).unwrap();
        assert_eq!(format!("{s:?}"), "SigningSeed(..)");
        assert!(SigningSeed::from_slice(&[0u8; 31]).is_none());
    }
}
