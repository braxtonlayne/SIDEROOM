//! TOTP 2FA (ENT-AUTH-008) on top of `totp-rs`: SHA-1, 6 digits, 30 s step,
//! +/-1 step skew (the settings every authenticator app supports).
//!
//! The caller must encrypt the secret at rest and enforce replay protection
//! with [`step_is_fresh`]: `verify` returns the matched step so the
//! repository can persist `last_used_step`.

use crate::{error::AuthError, random};
use std::fmt;
use totp_rs::{Algorithm, Builder, Totp};

/// 160-bit secret, the RFC 4226 recommendation.
const SECRET_BYTES: usize = 20;

/// A TOTP shared secret. Redacted in `Debug`.
#[derive(Clone, PartialEq, Eq)]
pub struct TotpSecret(Vec<u8>);

impl TotpSecret {
    /// Generate a new random secret.
    pub fn generate() -> Result<Self, AuthError> {
        Ok(Self(random::bytes::<SECRET_BYTES>()?.to_vec()))
    }

    /// Rebuild from stored raw bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, AuthError> {
        if bytes.len() < 16 {
            return Err(AuthError::Totp("secret too short".into()));
        }
        Ok(Self(bytes.to_vec()))
    }

    /// Raw bytes (to be encrypted before persisting).
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Base32 form for manual entry (no padding, uppercase).
    pub fn to_base32(&self) -> String {
        data_encoding::BASE32_NOPAD.encode(&self.0)
    }
}

impl fmt::Debug for TotpSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TotpSecret(..)")
    }
}

/// Everything the enrolment screen needs.
#[derive(Debug, Clone)]
pub struct TotpEnrollment {
    /// The secret to persist (pending until the first code is confirmed).
    pub secret: TotpSecret,
    /// `otpauth://` provisioning URI (render as QR on the client).
    pub otpauth_uri: String,
    /// Base32 secret for manual entry.
    pub manual_entry_key: String,
}

fn build(secret: &TotpSecret, issuer: &str, account: &str) -> Result<Totp, AuthError> {
    Builder::new()
        .with_algorithm(Algorithm::SHA1)
        .with_digits(6)
        .with_skew(1)
        .with_step_duration(30)
        .with_secret(secret.0.clone())
        .with_account_name(account)
        .with_issuer(Some(issuer))
        .build()
        .map_err(|e| AuthError::Totp(e.to_string()))
}

/// Start enrolment: generate a secret and its provisioning URI.
/// `issuer` and `account` must not contain `:`.
pub fn enroll(issuer: &str, account: &str) -> Result<TotpEnrollment, AuthError> {
    let secret = TotpSecret::generate()?;
    let totp = build(&secret, issuer, account)?;
    let otpauth_uri = totp.to_url().map_err(|e| AuthError::Totp(e.to_string()))?;
    Ok(TotpEnrollment {
        manual_entry_key: secret.to_base32(),
        secret,
        otpauth_uri,
    })
}

/// Check `code` at unix time `now`. Returns the matched time-step on
/// success; `None` for a wrong or malformed code.
pub fn verify(
    secret: &TotpSecret,
    issuer: &str,
    account: &str,
    code: &str,
    now: u64,
) -> Result<Option<u64>, AuthError> {
    let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    Ok(build(secret, issuer, account)?.check(&code, now))
}

/// Compute the current code (used by tests and by enrolment self-checks).
pub fn current_code(
    secret: &TotpSecret,
    issuer: &str,
    account: &str,
    now: u64,
) -> Result<String, AuthError> {
    Ok(build(secret, issuer, account)?.generate(now).to_string())
}

/// Replay protection: a step is acceptable only if it is newer than the last
/// one consumed for this account.
pub fn step_is_fresh(step: u64, last_used_step: Option<u64>) -> bool {
    last_used_step.is_none_or(|last| step > last)
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: u64 = 1_700_000_000;

    fn fixture() -> TotpSecret {
        TotpSecret::from_bytes(b"12345678901234567890").unwrap()
    }

    #[test]
    fn enroll_produces_uri_and_key() {
        let e = enroll("SIDE ROOM", "sam@example.com").unwrap();
        assert!(e.otpauth_uri.starts_with("otpauth://totp/"));
        assert!(
            e.otpauth_uri.contains("issuer=SIDE%20ROOM")
                || e.otpauth_uri.contains("issuer=SIDE+ROOM")
        );
        assert!(e.otpauth_uri.contains("secret="));
        assert!(e.otpauth_uri.contains(&e.manual_entry_key));
        assert_eq!(e.secret.as_bytes().len(), 20);
        assert_eq!(e.manual_entry_key.len(), 32);
    }

    #[test]
    fn secrets_are_unique() {
        assert_ne!(
            TotpSecret::generate().unwrap(),
            TotpSecret::generate().unwrap()
        );
    }

    #[test]
    fn rfc6238_sha1_vector() {
        // RFC 6238 appendix B: T=59 -> 94287082 (8 digits); 6-digit is 287082.
        let code = current_code(&fixture(), "SR", "a@b.c", 59).unwrap();
        assert_eq!(code, "287082");
    }

    #[test]
    fn verify_accepts_current_and_adjacent_steps() {
        let s = fixture();
        let now = T;
        for t in [now - 30, now, now + 30] {
            let code = current_code(&s, "SR", "a@b.c", t).unwrap();
            assert!(verify(&s, "SR", "a@b.c", &code, now).unwrap().is_some());
        }
    }

    #[test]
    fn verify_rejects_outside_skew() {
        let s = fixture();
        for t in [T - 90, T + 90] {
            let code = current_code(&s, "SR", "a@b.c", t).unwrap();
            assert!(verify(&s, "SR", "a@b.c", &code, T).unwrap().is_none());
        }
    }

    #[test]
    fn verify_rejects_garbage() {
        let s = fixture();
        for bad in ["", "abcdef", "12345", "1234567", "000000x"] {
            assert!(
                verify(&s, "SR", "a@b.c", bad, T).unwrap().is_none(),
                "{bad}"
            );
        }
    }

    #[test]
    fn verify_tolerates_spaces() {
        let s = fixture();
        let code = current_code(&s, "SR", "a@b.c", T).unwrap();
        let spaced = format!("{} {}", &code[..3], &code[3..]);
        assert!(verify(&s, "SR", "a@b.c", &spaced, T).unwrap().is_some());
    }

    #[test]
    fn wrong_secret_fails() {
        let code = current_code(&fixture(), "SR", "a@b.c", T).unwrap();
        let other = TotpSecret::from_bytes(b"ABCDEFGHIJKLMNOPQRST").unwrap();
        assert!(verify(&other, "SR", "a@b.c", &code, T).unwrap().is_none());
    }

    #[test]
    fn replay_protection() {
        let s = fixture();
        let code = current_code(&s, "SR", "a@b.c", T).unwrap();
        let step = verify(&s, "SR", "a@b.c", &code, T).unwrap().unwrap();
        assert_eq!(step, T / 30);
        assert!(step_is_fresh(step, None));
        assert!(!step_is_fresh(step, Some(step)));
        assert!(!step_is_fresh(step, Some(step + 1)));
        assert!(step_is_fresh(step + 1, Some(step)));
    }

    #[test]
    fn short_secret_and_debug() {
        assert!(TotpSecret::from_bytes(&[1; 8]).is_err());
        assert_eq!(format!("{:?}", fixture()), "TotpSecret(..)");
    }

    #[test]
    fn bad_account_rejected() {
        assert!(enroll("SR", "a:b").is_err());
    }
}
