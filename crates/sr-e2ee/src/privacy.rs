//! Privacy codes (VOX-E2E-007, D-250, as in DAVE), six groups of five digits each:
//!
//! - **Pairwise verification code** ([`pairwise_code`]): from the two users' ids and identity
//!   public keys only, the same on both sides. It stays the same across calls until one of the
//!   identity keys changes (a new device, cleared site data), so a "verified" mark can be kept
//!   per identity key. Comparing it out of band (in person, another app) proves no one swapped
//!   either person's key.
//! - **Call code** ([`call_code`]): from the current MLS epoch authenticator. Everyone in the
//!   same group state sees the same code; it changes with every join and leave and is only
//!   compared live, never stored.
//!
//! The digits encode a SHA-256 hash of length-prefixed, domain-separated inputs: each 5-byte
//! chunk as a big-endian integer modulo 100000 (as DAVE displays its codes).

use sha2::{Digest, Sha256};

pub const GROUPS: usize = 6;
pub const GROUP_DIGITS: usize = 5;

fn put(h: &mut Sha256, field: &[u8]) {
    h.update((field.len() as u32).to_be_bytes());
    h.update(field);
}

fn digits(d: &[u8]) -> String {
    (0..GROUPS)
        .map(|i| {
            let v = d[i * 5..i * 5 + 5]
                .iter()
                .fold(0u64, |v, b| (v << 8) | u64::from(*b));
            format!("{:0width$}", v % 100_000, width = GROUP_DIGITS)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn fingerprint(user_id: u64, identity_key: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(8 + identity_key.len());
    f.extend_from_slice(&user_id.to_be_bytes());
    f.extend_from_slice(identity_key);
    f
}

/// The verification code between two users (order does not matter).
pub fn pairwise_code(user_a: u64, key_a: &[u8], user_b: u64, key_b: &[u8]) -> String {
    let mut fs = [fingerprint(user_a, key_a), fingerprint(user_b, key_b)];
    fs.sort();
    let mut h = Sha256::new();
    put(&mut h, b"sr-e2ee/v1 pairwise code");
    put(&mut h, &fs[0]);
    put(&mut h, &fs[1]);
    digits(&h.finalize())
}

/// The code of the call's current epoch.
pub fn call_code(epoch_authenticator: &[u8]) -> String {
    let mut h = Sha256::new();
    put(&mut h, b"sr-e2ee/v1 call code");
    put(&mut h, epoch_authenticator);
    digits(&h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shaped(c: &str) -> bool {
        c.len() == GROUPS * GROUP_DIGITS + GROUPS - 1
            && c.split(' ')
                .all(|g| g.len() == 5 && g.bytes().all(|b| b.is_ascii_digit()))
    }

    #[test]
    fn pairwise_is_symmetric_and_key_bound() {
        let c = pairwise_code(1, b"ka", 2, b"kb");
        assert!(shaped(&c));
        assert_eq!(c, pairwise_code(2, b"kb", 1, b"ka"));
        assert_ne!(c, pairwise_code(1, b"ka", 2, b"kc"));
        assert_ne!(c, pairwise_code(1, b"ka", 3, b"kb"));
        // Length prefixes keep fields apart.
        assert_ne!(
            pairwise_code(1, b"ab", 2, b"c"),
            pairwise_code(1, b"a", 2, b"bc")
        );
    }

    #[test]
    fn call_code_follows_the_epoch() {
        let c = call_code(b"auth-1");
        assert!(shaped(&c));
        assert_eq!(c, call_code(b"auth-1"));
        assert_ne!(c, call_code(b"auth-2"));
    }
}
