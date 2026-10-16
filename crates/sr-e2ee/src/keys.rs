//! Media keys (VOX-E2E-002) and the receiver's epoch key ring (VOX-E2E-006).
//!
//! Every sender gets a 16-byte base secret per epoch from the group: the MLS exporter with
//! [`SENDER_SECRET_LABEL`] and the sender's user id (u64, little-endian) as context. Nobody
//! outside the group can compute it, and it changes with every epoch (every join and leave).
//!
//! A sender's encryptor adds a random 4-byte *instance id* and derives its AES-128 key with
//! HKDF-SHA256 from the base secret and that id. Each encryptor instance has its own frame
//! counter, so two instances in the same epoch (a restarted media worker, say) use different
//! keys rather than repeating nonces under one key. The id travels in every frame's trailer.

use std::collections::HashMap;

use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroize;

use crate::codec::Codec;
use crate::frame::{FrameDecryptor, FrameError, InstanceId};

/// MLS exporter label for a sender's per-epoch base secret (context: user id as u64 LE,
/// length 16). Used once the MLS group exists (VOX-E2E-001).
pub const SENDER_SECRET_LABEL: &str = "SideRoom Secure Frames v1";
/// Length of a sender base secret and of a media key.
pub const SECRET_LEN: usize = 16;
/// How long frames under the previous epoch's keys are still accepted after a new epoch
/// starts (spec section 5.4, "a short transition window"; DAVE keeps 10 seconds).
pub const DEFAULT_GRACE_MS: u64 = 10_000;
/// Derived decryptors kept per epoch before the cache is reset.
const MAX_DECRYPTORS: usize = 256;

const HKDF_SALT: &[u8] = b"sr-e2ee/v1";
const HKDF_INFO: &[u8] = b"sr-e2ee/v1 media key";

/// A sender's base secret for one epoch. Zeroed on drop, never printed.
#[derive(Clone, PartialEq, Eq)]
pub struct SenderSecret([u8; SECRET_LEN]);

impl SenderSecret {
    pub fn new(bytes: [u8; SECRET_LEN]) -> Self {
        SenderSecret(bytes)
    }

    pub fn from_slice(b: &[u8]) -> Option<Self> {
        Some(SenderSecret(b.try_into().ok()?))
    }

    /// The raw bytes (for handing a secret to the media worker).
    pub fn as_bytes(&self) -> &[u8; SECRET_LEN] {
        &self.0
    }

    /// The AES-128 key for one encryptor instance.
    pub fn media_key(&self, iid: InstanceId) -> MediaKey {
        let hk = Hkdf::<Sha256>::new(Some(HKDF_SALT), &self.0);
        let mut info = Vec::with_capacity(HKDF_INFO.len() + 4);
        info.extend_from_slice(HKDF_INFO);
        info.extend_from_slice(&iid.0);
        let mut k = [0u8; SECRET_LEN];
        // 16 bytes is far below HKDF's 255 * 32 limit.
        hk.expand(&info, &mut k).expect("16-byte HKDF output");
        MediaKey(k)
    }
}

impl std::fmt::Debug for SenderSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SenderSecret(..)")
    }
}

/// An AES-128-GCM key. Zeroed on drop, never printed.
#[derive(Clone)]
pub struct MediaKey(pub(crate) [u8; SECRET_LEN]);

impl Drop for SenderSecret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl Drop for MediaKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl std::fmt::Debug for MediaKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MediaKey(..)")
    }
}

struct EpochKeys {
    epoch: u64,
    senders: HashMap<u64, SenderSecret>,
    decryptors: HashMap<(u64, InstanceId), FrameDecryptor>,
    /// Past this the epoch is dropped; `None` for the current epoch.
    expires_at: Option<u64>,
}

impl EpochKeys {
    fn decrypt(&mut self, sender: u64, codec: Codec, frame: &[u8]) -> Result<Vec<u8>, FrameError> {
        let iid = crate::frame::peek_instance(frame)?;
        if !self.decryptors.contains_key(&(sender, iid)) {
            let secret = self.senders.get(&sender).ok_or(FrameError::NoKey)?;
            if self.decryptors.len() >= MAX_DECRYPTORS {
                self.decryptors.clear();
            }
            let d = FrameDecryptor::new(secret, iid);
            self.decryptors.insert((sender, iid), d);
        }
        self.decryptors[&(sender, iid)].decrypt(codec, frame)
    }
}

/// What a receiver holds: the current epoch's sender secrets and, for a grace window, the
/// previous epoch's, so frames encrypted just before a sender switched keys still play.
pub struct KeyRing {
    grace_ms: u64,
    current: Option<EpochKeys>,
    previous: Vec<EpochKeys>,
}

impl Default for KeyRing {
    fn default() -> Self {
        KeyRing::new(DEFAULT_GRACE_MS)
    }
}

impl KeyRing {
    pub fn new(grace_ms: u64) -> Self {
        KeyRing {
            grace_ms,
            current: None,
            previous: Vec::new(),
        }
    }

    pub fn epoch(&self) -> Option<u64> {
        self.current.as_ref().map(|c| c.epoch)
    }

    /// Install a new epoch. Epochs only go forward; an older or equal one is refused.
    pub fn set_epoch(
        &mut self,
        epoch: u64,
        senders: impl IntoIterator<Item = (u64, SenderSecret)>,
        now_ms: u64,
    ) -> Result<(), FrameError> {
        if self.current.as_ref().is_some_and(|c| c.epoch >= epoch) {
            return Err(FrameError::StaleEpoch);
        }
        self.prune(now_ms);
        if let Some(mut old) = self.current.take() {
            old.expires_at = Some(now_ms.saturating_add(self.grace_ms));
            self.previous.push(old);
        }
        self.current = Some(EpochKeys {
            epoch,
            senders: senders.into_iter().collect(),
            decryptors: HashMap::new(),
            expires_at: None,
        });
        Ok(())
    }

    /// Forget epochs whose grace window has passed.
    pub fn prune(&mut self, now_ms: u64) {
        self.previous
            .retain(|e| e.expires_at.is_some_and(|t| now_ms < t));
    }

    /// Forget everything (left the call).
    pub fn clear(&mut self) {
        self.current = None;
        self.previous.clear();
    }

    /// Decrypt a frame from `sender`: the current epoch first, then earlier epochs still in
    /// their grace window, newest first. Returns the epoch that worked.
    pub fn decrypt(
        &mut self,
        sender: u64,
        codec: Codec,
        frame: &[u8],
        now_ms: u64,
    ) -> Result<(u64, Vec<u8>), FrameError> {
        self.prune(now_ms);
        let mut last = FrameError::NoKey;
        let cur = self.current.iter_mut();
        let prev = self.previous.iter_mut().rev();
        for e in cur.chain(prev) {
            match e.decrypt(sender, codec, frame) {
                Ok(p) => return Ok((e.epoch, p)),
                Err(err @ (FrameError::Malformed | FrameError::Unsupported)) => return Err(err),
                Err(err) => last = err,
            }
        }
        Err(last)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::FrameEncryptor;

    fn secret(b: u8) -> SenderSecret {
        SenderSecret::new([b; 16])
    }

    #[test]
    fn media_key_depends_on_secret_and_instance() {
        let a = secret(1).media_key(InstanceId([1, 2, 3, 4]));
        let b = secret(1).media_key(InstanceId([1, 2, 3, 5]));
        let c = secret(2).media_key(InstanceId([1, 2, 3, 4]));
        assert_ne!(a.0, b.0);
        assert_ne!(a.0, c.0);
        assert_eq!(a.0, secret(1).media_key(InstanceId([1, 2, 3, 4])).0);
        assert_eq!(format!("{a:?}"), "MediaKey(..)");
        assert_eq!(format!("{:?}", secret(1)), "SenderSecret(..)");
    }

    #[test]
    fn grace_window_keeps_previous_epoch() {
        let iid = InstanceId([7, 7, 7, 7]);
        let mut old_tx = FrameEncryptor::new(&secret(1), iid).unwrap();
        let mut new_tx = FrameEncryptor::new(&secret(2), iid).unwrap();
        let mut ring = KeyRing::new(1000);
        ring.set_epoch(1, [(42, secret(1))], 0).unwrap();
        let f1 = old_tx.encrypt(Codec::Opus, b"hello").unwrap();
        assert_eq!(
            ring.decrypt(42, Codec::Opus, &f1, 10).unwrap(),
            (1, b"hello".to_vec())
        );

        ring.set_epoch(2, [(42, secret(2))], 100).unwrap();
        let f2 = new_tx.encrypt(Codec::Opus, b"world").unwrap();
        assert_eq!(ring.decrypt(42, Codec::Opus, &f2, 200).unwrap().0, 2);
        // A late frame under epoch 1 still plays inside the window...
        let late = old_tx.encrypt(Codec::Opus, b"late").unwrap();
        assert_eq!(ring.decrypt(42, Codec::Opus, &late, 1099).unwrap().0, 1);
        // ...and not after it.
        let later = old_tx.encrypt(Codec::Opus, b"later").unwrap();
        assert_eq!(
            ring.decrypt(42, Codec::Opus, &later, 1100),
            Err(FrameError::Auth)
        );
        // Unknown sender, stale epoch.
        assert_eq!(
            ring.decrypt(9, Codec::Opus, &f2, 1200),
            Err(FrameError::NoKey)
        );
        assert_eq!(ring.set_epoch(2, [], 1300), Err(FrameError::StaleEpoch));
        ring.clear();
        assert_eq!(ring.epoch(), None);
    }
}
