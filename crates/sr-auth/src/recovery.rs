//! Recovery codes (ENT-AUTH-009): single-use, generated in a batch, shown once.
//!
//! Format `XXXX-XXXX-XXXX` (12 Crockford symbols, 60 bits). Because that is
//! far less entropy than a session token, each code is stored as an argon2id
//! PHC string rather than a bare digest.

use crate::{crockford, error::AuthError, password::PasswordHasher};
use std::fmt;

/// Codes issued per batch.
pub const BATCH_SIZE: usize = 10;
const SYMBOLS: usize = 12;
const GROUP: usize = 4;

/// A recovery code in canonical form.
#[derive(Clone, PartialEq, Eq)]
pub struct RecoveryCode(String);

impl RecoveryCode {
    /// Generate one random code.
    pub fn generate() -> Result<Self, AuthError> {
        Ok(Self(crockford::random_symbols(SYMBOLS)?))
    }

    /// Parse user input (case, dashes, confusables tolerated).
    pub fn parse(input: &str) -> Result<Self, AuthError> {
        let c = crockford::normalize(input).ok_or(AuthError::Malformed)?;
        if c.len() != SYMBOLS {
            return Err(AuthError::Malformed);
        }
        Ok(Self(c))
    }

    /// Display form `XXXX-XXXX-XXXX`.
    pub fn formatted(&self) -> String {
        crockford::group(&self.0, GROUP)
    }

    /// Hash for storage.
    pub fn hash(&self, hasher: &PasswordHasher) -> Result<String, AuthError> {
        hasher.hash(&self.0)
    }

    /// Check against one stored hash.
    pub fn matches(&self, hasher: &PasswordHasher, stored: &str) -> Result<bool, AuthError> {
        hasher.verify(&self.0, stored)
    }
}

impl fmt::Debug for RecoveryCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RecoveryCode(..)")
    }
}

/// A newly generated batch: show `codes` to the user once, persist `hashes`.
#[derive(Debug)]
pub struct RecoveryBatch {
    /// Plaintext codes (display once).
    pub codes: Vec<RecoveryCode>,
    /// PHC hashes in the same order, for storage.
    pub hashes: Vec<String>,
}

/// Generate [`BATCH_SIZE`] distinct codes and their hashes.
pub fn generate_batch(hasher: &PasswordHasher) -> Result<RecoveryBatch, AuthError> {
    let mut codes: Vec<RecoveryCode> = Vec::with_capacity(BATCH_SIZE);
    while codes.len() < BATCH_SIZE {
        let c = RecoveryCode::generate()?;
        if !codes.contains(&c) {
            codes.push(c);
        }
    }
    let hashes = codes
        .iter()
        .map(|c| c.hash(hasher))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RecoveryBatch { codes, hashes })
}

/// Find which unused stored hash `input` matches. Returns its index so the
/// caller can mark exactly that one consumed. Every stored hash is checked
/// (no early exit) to keep timing independent of position.
pub fn find_match(
    hasher: &PasswordHasher,
    input: &str,
    unused_hashes: &[String],
) -> Result<Option<usize>, AuthError> {
    let Ok(code) = RecoveryCode::parse(input) else {
        return Ok(None);
    };
    let mut found = None;
    for (i, h) in unused_hashes.iter().enumerate() {
        if code.matches(hasher, h)? && found.is_none() {
            found = Some(i);
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::password::PasswordParams;

    fn hasher() -> PasswordHasher {
        PasswordHasher::new(PasswordParams {
            m_cost_kib: 64,
            t_cost: 1,
            p_cost: 1,
        })
        .unwrap()
    }

    #[test]
    fn format_and_parse() {
        let c = RecoveryCode::generate().unwrap();
        let f = c.formatted();
        assert_eq!(f.len(), 14);
        assert_eq!(f.matches('-').count(), 2);
        assert_eq!(RecoveryCode::parse(&f).unwrap(), c);
        assert_eq!(RecoveryCode::parse(&f.to_lowercase()).unwrap(), c);
    }

    #[test]
    fn rejects_bad_shapes() {
        for bad in [
            "",
            "ABCD-EFGH",
            "ABCD-EFGH-JKMN-PQRS",
            "ABCD-EFGH-JKMU",
            "!!!!-!!!!-!!!!",
        ] {
            assert!(RecoveryCode::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn batch_is_distinct_and_hashed() {
        let h = hasher();
        let b = generate_batch(&h).unwrap();
        assert_eq!(b.codes.len(), BATCH_SIZE);
        assert_eq!(b.hashes.len(), BATCH_SIZE);
        for (c, hash) in b.codes.iter().zip(&b.hashes) {
            assert!(hash.starts_with("$argon2id$"));
            assert!(!hash.contains(&c.formatted()));
            assert!(c.matches(&h, hash).unwrap());
        }
        for i in 0..b.codes.len() {
            for j in i + 1..b.codes.len() {
                assert_ne!(b.codes[i], b.codes[j]);
            }
        }
    }

    #[test]
    fn find_match_returns_index_and_consumption_works() {
        let h = hasher();
        let b = generate_batch(&h).unwrap();
        let mut unused = b.hashes.clone();
        let idx = find_match(&h, &b.codes[3].formatted(), &unused)
            .unwrap()
            .unwrap();
        assert_eq!(idx, 3);
        unused.remove(idx);
        assert_eq!(
            find_match(&h, &b.codes[3].formatted(), &unused).unwrap(),
            None
        );
        // Others still work.
        assert!(
            find_match(&h, &b.codes[4].formatted(), &unused)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn find_match_wrong_or_malformed() {
        let h = hasher();
        let b = generate_batch(&h).unwrap();
        assert_eq!(find_match(&h, "garbage", &b.hashes).unwrap(), None);
        let stranger = RecoveryCode::generate().unwrap();
        assert_eq!(
            find_match(&h, &stranger.formatted(), &b.hashes).unwrap(),
            None
        );
        assert_eq!(find_match(&h, &b.codes[0].formatted(), &[]).unwrap(), None);
    }

    #[test]
    fn debug_redacted() {
        let c = RecoveryCode::generate().unwrap();
        assert_eq!(format!("{c:?}"), "RecoveryCode(..)");
    }
}
