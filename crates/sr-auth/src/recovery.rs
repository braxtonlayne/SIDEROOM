//! Recovery codes (ENT-AUTH-009): single-use, generated in a batch, shown once.
//!
//! Format `XXXX-XXXX-XXXX-XXXX` (16 Crockford symbols, 80 bits). Each code is
//! stored as a SHA-256 digest ([`TokenHash`], domain-separated), not an
//! argon2 hash: 80 random bits need no slow hash, and a digest lets the
//! database enforce uniqueness per account (open question 1 and 39).

use crate::{crockford, error::AuthError, session::TokenHash};
use std::fmt;

/// Codes issued per batch.
pub const BATCH_SIZE: usize = 10;
const SYMBOLS: usize = 16;
const GROUP: usize = 4;
const DOMAIN: &str = "sideroom/recovery-code/v1";

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

    /// Display form `XXXX-XXXX-XXXX-XXXX`.
    pub fn formatted(&self) -> String {
        crockford::group(&self.0, GROUP)
    }

    /// SHA-256 digest for storage (the only persisted form).
    pub fn hash(&self) -> TokenHash {
        TokenHash::digest(DOMAIN, &self.0)
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
    /// Digests in the same order, for storage.
    pub hashes: Vec<TokenHash>,
}

/// Generate [`BATCH_SIZE`] distinct codes and their digests.
pub fn generate_batch() -> Result<RecoveryBatch, AuthError> {
    let mut codes: Vec<RecoveryCode> = Vec::with_capacity(BATCH_SIZE);
    while codes.len() < BATCH_SIZE {
        let c = RecoveryCode::generate()?;
        if !codes.contains(&c) {
            codes.push(c);
        }
    }
    let hashes = codes.iter().map(RecoveryCode::hash).collect();
    Ok(RecoveryBatch { codes, hashes })
}

/// Find which unused stored digest `input` matches. Returns its index so the
/// caller can mark exactly that one consumed. Every digest is compared in
/// constant time and there is no early exit, so timing does not reveal the
/// position of a match.
pub fn find_match(input: &str, unused: &[TokenHash]) -> Option<usize> {
    let code = RecoveryCode::parse(input).ok()?;
    let digest = code.hash();
    let mut found = None;
    for (i, h) in unused.iter().enumerate() {
        if digest.ct_eq(h) && found.is_none() {
            found = Some(i);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_and_parse() {
        let c = RecoveryCode::generate().unwrap();
        let f = c.formatted();
        assert_eq!(f.len(), 19);
        assert_eq!(f.matches('-').count(), 3);
        assert_eq!(RecoveryCode::parse(&f).unwrap(), c);
        assert_eq!(RecoveryCode::parse(&f.to_lowercase()).unwrap(), c);
    }

    #[test]
    fn rejects_bad_shapes() {
        for bad in [
            "",
            "ABCD-EFGH",
            "ABCD-EFGH-JKMN",
            "ABCD-EFGH-JKMN-PQRS-TVWX",
            "ABCD-EFGH-JKMN-PQRU",
            "!!!!-!!!!-!!!!-!!!!",
        ] {
            assert!(RecoveryCode::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn batch_is_distinct_and_hashed() {
        let b = generate_batch().unwrap();
        assert_eq!(b.codes.len(), BATCH_SIZE);
        assert_eq!(b.hashes.len(), BATCH_SIZE);
        for (c, hash) in b.codes.iter().zip(&b.hashes) {
            assert_eq!(c.hash(), *hash);
            assert!(!hash.to_hex().contains(&c.formatted()));
        }
        for i in 0..b.codes.len() {
            for j in i + 1..b.codes.len() {
                assert_ne!(b.codes[i], b.codes[j]);
                assert!(!b.hashes[i].ct_eq(&b.hashes[j]));
            }
        }
    }

    #[test]
    fn find_match_returns_index_and_consumption_works() {
        let b = generate_batch().unwrap();
        let mut unused = b.hashes.clone();
        let idx = find_match(&b.codes[3].formatted(), &unused).unwrap();
        assert_eq!(idx, 3);
        unused.remove(idx);
        assert_eq!(find_match(&b.codes[3].formatted(), &unused), None);
        // Others still work.
        assert!(find_match(&b.codes[4].formatted(), &unused).is_some());
    }

    #[test]
    fn find_match_wrong_or_malformed() {
        let b = generate_batch().unwrap();
        assert_eq!(find_match("garbage", &b.hashes), None);
        let stranger = RecoveryCode::generate().unwrap();
        assert_eq!(find_match(&stranger.formatted(), &b.hashes), None);
        assert_eq!(find_match(&b.codes[0].formatted(), &[]), None);
    }

    #[test]
    fn debug_redacted() {
        let c = RecoveryCode::generate().unwrap();
        assert_eq!(format!("{c:?}"), "RecoveryCode(..)");
    }
}
