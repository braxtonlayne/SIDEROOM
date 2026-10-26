//! Secrets sealed at rest with `server.secret_key` (open question 40, D-185): TOTP secrets
//! and the interactions signing seed use the same layout and key rotation.
//!
//! Layout: `version(1) || nonce(12) || AES-256-GCM ciphertext+tag`. The caller supplies the
//! associated data (the owning row id) so a ciphertext cannot be moved to another row.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use sha2::{Digest, Sha256};

const NONCE_LEN: usize = 12;

/// Sealing failed, or a stored blob is unreadable (wrong key, wrong row, corruption).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("sealed secret is unreadable")]
pub struct SealError;

/// Key version byte stored in front of every ciphertext: the first byte of a
/// domain-separated SHA-256 of the key. It is a fingerprint, not a secret, so the
/// decryptor can try the keys that match before the others.
pub fn key_version(key: &[u8; 32]) -> u8 {
    let mut h = Sha256::new();
    h.update(b"sideroom/secret-key-version/v1");
    h.update(key);
    h.finalize()[0]
}

/// The server secret plus the decrypt-only keys from before the last rotation.
#[derive(Clone)]
pub struct SecretBox {
    key: [u8; 32],
    previous: Vec<[u8; 32]>,
}

impl std::fmt::Debug for SecretBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretBox(..)")
    }
}

impl SecretBox {
    /// Seal under `key`.
    pub fn new(key: [u8; 32]) -> Self {
        Self {
            key,
            previous: Vec::new(),
        }
    }

    /// Add the decrypt-only keys from `server.secret_key_previous`.
    pub fn with_previous_keys(mut self, keys: Vec<[u8; 32]>) -> Self {
        self.previous = keys;
        self
    }

    /// Encrypt `plain` bound to `aad`.
    pub fn seal(&self, aad: &[u8], plain: &[u8]) -> Result<Vec<u8>, SealError> {
        let nonce = sr_auth::random::bytes::<NONCE_LEN>().map_err(|_| SealError)?;
        let ct = Aes256Gcm::new((&self.key).into())
            .encrypt(&Nonce::from(nonce), Payload { msg: plain, aad })
            .map_err(|_| SealError)?;
        let mut out = Vec::with_capacity(1 + NONCE_LEN + ct.len());
        out.push(key_version(&self.key));
        out.extend_from_slice(&nonce);
        out.extend(ct);
        Ok(out)
    }

    /// Decrypt a blob made by [`Self::seal`] with the same `aad`.
    pub fn open(&self, aad: &[u8], blob: &[u8]) -> Result<Vec<u8>, SealError> {
        let (&version, rest) = blob.split_first().ok_or(SealError)?;
        if rest.len() <= NONCE_LEN {
            return Err(SealError);
        }
        let (nonce, ct) = rest.split_at(NONCE_LEN);
        let nonce: [u8; NONCE_LEN] = nonce.try_into().map_err(|_| SealError)?;
        // Keys whose fingerprint matches the stored version are tried first; the others
        // follow in case of a fingerprint collision.
        let keys = std::iter::once(&self.key).chain(self.previous.iter());
        let (matching, others): (Vec<&[u8; 32]>, Vec<&[u8; 32]>) =
            keys.partition(|k| key_version(k) == version);
        for key in matching.into_iter().chain(others) {
            if let Ok(plain) =
                Aes256Gcm::new(key.into()).decrypt(&Nonce::from(nonce), Payload { msg: ct, aad })
            {
                return Ok(plain);
            }
        }
        Err(SealError)
    }
}

/// Mount credentials (BYOS-MNT-001) seal through the same box; the row id is the associated data
/// (`sr_storage::mounts::mount_aad`).
impl sr_storage::mounts::Sealer for SecretBox {
    fn seal(&self, aad: &[u8], plain: &[u8]) -> Result<Vec<u8>, sr_storage::mounts::MountError> {
        SecretBox::seal(self, aad, plain).map_err(|_| sr_storage::mounts::MountError::Unseal)
    }
    fn open(&self, aad: &[u8], blob: &[u8]) -> Result<Vec<u8>, sr_storage::mounts::MountError> {
        SecretBox::open(self, aad, blob).map_err(|_| sr_storage::mounts::MountError::Unseal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_binds_the_aad_and_survives_rotation() {
        let old = SecretBox::new([1; 32]);
        let blob = old.seal(b"row-1", b"seed").unwrap();
        assert_eq!(blob[0], key_version(&[1; 32]));
        assert_eq!(old.open(b"row-1", &blob).unwrap(), b"seed");
        assert!(old.open(b"row-2", &blob).is_err());
        assert!(SecretBox::new([2; 32]).open(b"row-1", &blob).is_err());
        let rotated = SecretBox::new([2; 32]).with_previous_keys(vec![[1; 32]]);
        assert_eq!(rotated.open(b"row-1", &blob).unwrap(), b"seed");
        assert!(old.open(b"row-1", &blob[..5]).is_err());
        assert_eq!(format!("{old:?}"), "SecretBox(..)");
    }

    #[test]
    fn mount_secrets_seal_with_the_row_id_as_aad() {
        use sr_storage::mounts::{MountError, SealedSecret, Secret};
        let old = SecretBox::new([1; 32]);
        let sealed = SealedSecret::seal(&old, 77, &Secret::new("wJalrXUtnFEMI/K7MDENG")).unwrap();
        assert!(
            !sealed.as_bytes().windows(8).any(|w| w == b"wJalrXUt"),
            "ciphertext must not contain the plaintext"
        );
        assert_eq!(sealed.as_bytes()[0], key_version(&[1; 32]));
        assert_eq!(
            sealed.open(&old, 77).unwrap().expose(),
            "wJalrXUtnFEMI/K7MDENG"
        );
        assert!(matches!(sealed.open(&old, 78), Err(MountError::Unseal)));
        // Rotation: the new box still opens the old blob and re-seals it under the new key.
        let rotated = SecretBox::new([2; 32]).with_previous_keys(vec![[1; 32]]);
        let resealed = sealed.reseal(&rotated, 77).unwrap();
        assert_eq!(resealed.as_bytes()[0], key_version(&[2; 32]));
        let new_only = SecretBox::new([2; 32]);
        assert_eq!(
            resealed.open(&new_only, 77).unwrap().expose(),
            "wJalrXUtnFEMI/K7MDENG"
        );
        assert!(sealed.open(&new_only, 77).is_err());
    }
}
