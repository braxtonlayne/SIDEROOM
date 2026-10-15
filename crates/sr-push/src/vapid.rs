//! VAPID (RFC 8292): the server's application-server key pair and the ES256 JWT that
//! proves to a push service which application server is sending.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};

use crate::PushError;
use crate::ece::random_secret;

/// The application-server key pair. The private half is persisted by the caller (see
/// [`VapidKey::secret_bytes`]); the public half is what browsers subscribe against.
#[derive(Clone)]
pub struct VapidKey {
    key: SigningKey,
}

impl std::fmt::Debug for VapidKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VapidKey")
            .field("public", &self.public_key())
            .finish_non_exhaustive()
    }
}

impl VapidKey {
    /// A fresh random key pair.
    pub fn generate() -> Result<Self, PushError> {
        Ok(Self {
            key: SigningKey::from(random_secret()?),
        })
    }

    /// Restore a key from its 32 secret bytes.
    pub fn from_secret_bytes(bytes: &[u8]) -> Result<Self, PushError> {
        SigningKey::from_slice(bytes)
            .map(|key| Self { key })
            .map_err(|_| PushError::Crypto("stored VAPID key is not a P-256 scalar".into()))
    }

    /// The 32 secret bytes, for persistence. Never log them.
    pub fn secret_bytes(&self) -> Vec<u8> {
        self.key.to_bytes().to_vec()
    }

    /// The uncompressed public key, base64url without padding: the value of
    /// `applicationServerKey` and of the `k=` parameter.
    pub fn public_key(&self) -> String {
        let point = self.key.verifying_key().to_encoded_point(false);
        B64.encode(point.as_bytes())
    }

    /// The `Authorization` header value (`vapid t=<jwt>, k=<public key>`) for a push
    /// service at `audience` (scheme and host, no path). `subject` is a `mailto:` or
    /// `https:` contact; `exp_unix` must be within 24 hours of now.
    pub fn authorization(&self, audience: &str, subject: &str, exp_unix: u64) -> String {
        let header = B64.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
        let claims = serde_json::json!({ "aud": audience, "exp": exp_unix, "sub": subject });
        let signing_input = format!("{header}.{}", B64.encode(claims.to_string()));
        let sig: Signature = self.key.sign(signing_input.as_bytes());
        format!(
            "vapid t={signing_input}.{}, k={}",
            B64.encode(sig.to_bytes()),
            self.public_key()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::signature::Verifier;

    #[test]
    fn jwt_verifies_with_the_published_key_and_carries_the_claims() {
        let k = VapidKey::generate().unwrap();
        let h = k.authorization(
            "https://push.example",
            "mailto:ops@example.test",
            1_900_000_000,
        );
        let rest = h.strip_prefix("vapid t=").unwrap();
        let (jwt, key) = rest.split_once(", k=").unwrap();
        assert_eq!(key, k.public_key());
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);
        let claims: serde_json::Value =
            serde_json::from_slice(&B64.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(claims["aud"], "https://push.example");
        assert_eq!(claims["sub"], "mailto:ops@example.test");
        assert_eq!(claims["exp"], 1_900_000_000);
        let pub_bytes = B64.decode(key).unwrap();
        assert_eq!(pub_bytes.len(), 65);
        let vk = p256::ecdsa::VerifyingKey::from_sec1_bytes(&pub_bytes).unwrap();
        let sig = Signature::from_slice(&B64.decode(parts[2]).unwrap()).unwrap();
        vk.verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &sig)
            .unwrap();
    }

    #[test]
    fn keys_survive_storage() {
        let k = VapidKey::generate().unwrap();
        let again = VapidKey::from_secret_bytes(&k.secret_bytes()).unwrap();
        assert_eq!(k.public_key(), again.public_key());
        assert!(VapidKey::from_secret_bytes(&[0; 32]).is_err());
        assert!(!format!("{k:?}").contains(&B64.encode(k.secret_bytes())));
    }
}
