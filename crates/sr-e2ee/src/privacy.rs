//! Privacy codes (VOX-E2E-007, spec section 5.5): a per-participant fingerprint of the MLS
//! epoch authenticator and that participant's identity key, shown in the shield popover as
//! six groups of five digits. Two people who see the same code for a participant agree on
//! the group state and on that participant's key; comparing them out of band detects a
//! server that inserted a member or swapped a key.
//!
//! The digits are an encoding of a SHA-256 hash of length-prefixed, domain-separated
//! inputs (each 5-byte chunk of the hash, as a big-endian integer, modulo 100000, as DAVE
//! displays its codes). The code changes with every epoch, by design: it vouches for the
//! current call state.

use sha2::{Digest, Sha256};

const DOMAIN: &[u8] = b"sr-e2ee/v1 privacy code";
pub const GROUPS: usize = 6;
pub const GROUP_DIGITS: usize = 5;

fn put(h: &mut Sha256, field: &[u8]) {
    h.update((field.len() as u32).to_be_bytes());
    h.update(field);
}

/// The groups of a participant's code.
pub fn privacy_code_groups(
    epoch_authenticator: &[u8],
    user_id: u64,
    identity_public_key: &[u8],
) -> [u32; GROUPS] {
    let mut h = Sha256::new();
    put(&mut h, DOMAIN);
    put(&mut h, epoch_authenticator);
    put(&mut h, &user_id.to_be_bytes());
    put(&mut h, identity_public_key);
    let d = h.finalize();
    let mut out = [0u32; GROUPS];
    for (i, g) in out.iter_mut().enumerate() {
        let mut v: u64 = 0;
        for b in &d[i * 5..i * 5 + 5] {
            v = (v << 8) | u64::from(*b);
        }
        *g = (v % 100_000) as u32;
    }
    out
}

/// `"01234 56789 ..."`.
pub fn privacy_code(
    epoch_authenticator: &[u8],
    user_id: u64,
    identity_public_key: &[u8],
) -> String {
    privacy_code_groups(epoch_authenticator, user_id, identity_public_key)
        .iter()
        .map(|g| format!("{g:0width$}", width = GROUP_DIGITS))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_and_sensitivity() {
        let c = privacy_code(b"auth", 42, b"key");
        assert_eq!(c.len(), GROUPS * GROUP_DIGITS + GROUPS - 1);
        assert!(
            c.split(' ')
                .all(|g| g.len() == 5 && g.bytes().all(|b| b.is_ascii_digit()))
        );
        assert_eq!(c, privacy_code(b"auth", 42, b"key"));
        assert_ne!(c, privacy_code(b"auth2", 42, b"key"));
        assert_ne!(c, privacy_code(b"auth", 43, b"key"));
        assert_ne!(c, privacy_code(b"auth", 42, b"key2"));
        // Length prefixes keep field boundaries apart.
        assert_ne!(privacy_code(b"ab", 1, b"c"), privacy_code(b"a", 1, b"bc"));
    }
}
