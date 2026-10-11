//! argon2id password hashing (ENT-AUTH-003).
//!
//! Hashes are PHC strings embedding their own parameters, so verification
//! works across parameter changes and [`PasswordHasher::needs_rehash`] tells
//! the login path to transparently upgrade a stored hash.

use crate::{error::AuthError, random};
use argon2::{
    Algorithm, Argon2, Params, PasswordHasher as _, PasswordVerifier as _, Version,
    password_hash::phc::PasswordHash,
};

/// Tunable argon2id cost parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasswordParams {
    /// Memory cost in KiB.
    pub m_cost_kib: u32,
    /// Iterations.
    pub t_cost: u32,
    /// Parallelism lanes.
    pub p_cost: u32,
}

impl Default for PasswordParams {
    /// 64 MiB, 3 passes, 1 lane (above the OWASP minimum of 19 MiB / 2 / 1).
    fn default() -> Self {
        Self {
            m_cost_kib: 64 * 1024,
            t_cost: 3,
            p_cost: 1,
        }
    }
}

impl PasswordParams {
    fn to_argon(self) -> Result<Params, AuthError> {
        Params::new(self.m_cost_kib, self.t_cost, self.p_cost, None)
            .map_err(|_| AuthError::InvalidParams)
    }
}

/// Hashes and verifies passwords with a fixed target parameter set.
#[derive(Debug, Clone, Default)]
pub struct PasswordHasher {
    params: PasswordParams,
}

impl PasswordHasher {
    /// Create a hasher with explicit target parameters.
    pub fn new(params: PasswordParams) -> Result<Self, AuthError> {
        params.to_argon()?;
        Ok(Self { params })
    }

    fn argon(&self) -> Result<Argon2<'static>, AuthError> {
        Ok(Argon2::new(
            Algorithm::Argon2id,
            Version::V0x13,
            self.params.to_argon()?,
        ))
    }

    /// Hash a password with a fresh random 128-bit salt. Returns a PHC string.
    pub fn hash(&self, password: &str) -> Result<String, AuthError> {
        let salt = random::bytes::<16>()?;
        let hash = self
            .argon()?
            .hash_password_with_salt(password.as_bytes(), &salt)
            .map_err(|_| AuthError::Hash)?;
        Ok(hash.to_string())
    }

    /// Verify a password against a stored PHC string. Returns `Ok(false)` on
    /// mismatch and `Err` only when the stored hash is unusable.
    pub fn verify(&self, password: &str, stored: &str) -> Result<bool, AuthError> {
        let parsed: PasswordHash = stored.parse().map_err(|_| AuthError::MalformedHash)?;
        // Verification uses the parameters embedded in the stored hash.
        match Argon2::default().verify_password(password.as_bytes(), &parsed) {
            Ok(()) => Ok(true),
            Err(argon2::password_hash::Error::PasswordInvalid) => Ok(false),
            Err(_) => Err(AuthError::MalformedHash),
        }
    }

    /// True if `stored` is not argon2id v19 with exactly the target params
    /// (or is unparseable), meaning the caller should re-hash after a
    /// successful login.
    pub fn needs_rehash(&self, stored: &str) -> bool {
        let Ok(parsed) = stored.parse::<PasswordHash>() else {
            return true;
        };
        if parsed.algorithm.as_str() != argon2::ARGON2ID_IDENT.as_str() {
            return true;
        }
        if parsed.version != Some(0x13) {
            return true;
        }
        let Ok(p) = Params::try_from(&parsed) else {
            return true;
        };
        p.m_cost() != self.params.m_cost_kib
            || p.t_cost() != self.params.t_cost
            || p.p_cost() != self.params.p_cost
    }

    /// Burn the same CPU as a real verification so unknown-account logins do
    /// not leak existence via timing. Always returns `false`.
    pub fn dummy_verify(&self, password: &str) -> bool {
        let _ = self.hash(password);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cheap(m: u32, t: u32) -> PasswordHasher {
        PasswordHasher::new(PasswordParams {
            m_cost_kib: m,
            t_cost: t,
            p_cost: 1,
        })
        .unwrap()
    }

    #[test]
    fn hash_and_verify() {
        let h = cheap(64, 1);
        let s = h.hash("correct horse battery staple").unwrap();
        assert!(s.starts_with("$argon2id$v=19$"));
        assert!(h.verify("correct horse battery staple", &s).unwrap());
        assert!(!h.verify("wrong", &s).unwrap());
    }

    #[test]
    fn salts_differ() {
        let h = cheap(64, 1);
        assert_ne!(h.hash("same").unwrap(), h.hash("same").unwrap());
    }

    #[test]
    fn unicode_and_empty() {
        let h = cheap(64, 1);
        let s = h.hash("pässwörd🔑").unwrap();
        assert!(h.verify("pässwörd🔑", &s).unwrap());
        assert!(!h.verify("passwörd🔑", &s).unwrap());
        let e = h.hash("").unwrap();
        assert!(h.verify("", &e).unwrap());
    }

    #[test]
    fn malformed_stored_hash() {
        let h = cheap(64, 1);
        assert_eq!(h.verify("x", "not a hash"), Err(AuthError::MalformedHash));
        assert!(h.needs_rehash("not a hash"));
    }

    #[test]
    fn verify_works_across_param_changes() {
        let s = cheap(64, 1).hash("pw").unwrap();
        assert!(cheap(128, 2).verify("pw", &s).unwrap());
    }

    #[test]
    fn rehash_detection() {
        let old = cheap(64, 1);
        let new = cheap(128, 2);
        let s = old.hash("pw").unwrap();
        assert!(!old.needs_rehash(&s));
        assert!(new.needs_rehash(&s));
        assert!(!new.needs_rehash(&new.hash("pw").unwrap()));
    }

    #[test]
    fn rehash_detects_other_variants() {
        let h = cheap(64, 1);
        let argon2i = "$argon2i$v=19$m=64,t=1,p=1$c29tZXNhbHRzb21lc2FsdA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        assert!(h.needs_rehash(argon2i));
    }

    #[test]
    fn invalid_params_rejected() {
        assert!(
            PasswordHasher::new(PasswordParams {
                m_cost_kib: 0,
                t_cost: 0,
                p_cost: 0
            })
            .is_err()
        );
    }

    #[test]
    fn defaults_meet_owasp_floor() {
        let d = PasswordParams::default();
        assert!(d.m_cost_kib >= 19 * 1024 && d.t_cost >= 2 && d.p_cost >= 1);
    }

    #[test]
    fn dummy_verify_is_false() {
        assert!(!cheap(64, 1).dummy_verify("anything"));
    }
}
