//! Activity data frames (ACT-CH-001): the same transform as media frames ([`crate::frame`]) with
//! [`Codec::Data`], so every byte of a message is AES-128-GCM ciphertext under the sender's key for the
//! epoch, the nonce comes from a message counter, and the trailer carries the tag, the sender's
//! encryptor instance and the counter. The SFU relays these bytes over an SCTP data channel without being
//! able to read or change them.
//!
//! What media does not need but data does: the receiver rejects a replayed or duplicated message. The
//! data channel is ordered and reliable, so counters normally arrive in increasing order; the guard keeps
//! a 64-message window per (sender, epoch, encryptor instance) and drops a counter that was already seen
//! or fell behind the window, which also stops the SFU from replaying a captured message.
//!
//! Every participant holds the room's keys, so this protects against the SFU and outsiders, not against
//! another member (the cards activity says so in its lobby).

use std::collections::HashMap;

use crate::codec::Codec;
use crate::frame::{FrameEncryptor, FrameError, InstanceId, MAX_TRAILER, parse_trailer};
use crate::keys::{KeyRing, SenderSecret};

/// Largest plaintext message (the SDK's envelope limit).
pub const MAX_MESSAGE: usize = 16 * 1024;
/// Largest ciphertext message the SFU relays: a message plus its trailer.
pub const MAX_WIRE: usize = MAX_MESSAGE + MAX_TRAILER;
/// How many counters behind the highest one are still accepted (once each).
pub const REPLAY_WINDOW: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DataError {
    #[error(transparent)]
    Frame(#[from] FrameError),
    /// The counter was already seen, or is older than the replay window.
    #[error("replayed message")]
    Replay,
    #[error("message too large")]
    TooLarge,
}

/// Encrypts one participant's data messages for one epoch. Not `Clone` (a copy would repeat counters).
pub struct DataSender(FrameEncryptor);

impl DataSender {
    pub fn new(secret: &SenderSecret, iid: InstanceId) -> Result<Self, DataError> {
        Ok(DataSender(FrameEncryptor::new(secret, iid)?))
    }

    /// Seal one message.
    pub fn seal(&mut self, message: &[u8]) -> Result<Vec<u8>, DataError> {
        if message.len() > MAX_MESSAGE {
            return Err(DataError::TooLarge);
        }
        Ok(self.0.encrypt(Codec::Data, message)?)
    }

    pub fn counter(&self) -> u32 {
        self.0.counter()
    }
}

#[derive(Default)]
struct Window {
    highest: u32,
    /// Bit i set: counter `highest - i` was seen.
    seen: u64,
}

impl Window {
    /// Record `c`; false when it is a replay or too old.
    fn accept(&mut self, c: u32) -> bool {
        if c > self.highest {
            let shift = c - self.highest;
            self.seen = if shift >= 64 { 0 } else { self.seen << shift };
            self.seen |= 1;
            self.highest = c;
            return true;
        }
        let back = self.highest - c;
        if back >= REPLAY_WINDOW {
            return false;
        }
        let bit = 1u64 << back;
        if self.seen & bit != 0 {
            return false;
        }
        self.seen |= bit;
        true
    }
}

/// Opens the data messages of the other participants.
pub struct DataReceiver {
    ring: KeyRing,
    windows: HashMap<(u64, u64, [u8; 4]), Window>,
}

impl DataReceiver {
    pub fn new(grace_ms: u64) -> Self {
        DataReceiver {
            ring: KeyRing::new(grace_ms),
            windows: HashMap::new(),
        }
    }

    /// Install an epoch's sender secrets (see [`KeyRing::set_epoch`]).
    pub fn set_epoch(
        &mut self,
        epoch: u64,
        senders: impl IntoIterator<Item = (u64, SenderSecret)>,
        now_ms: u64,
    ) -> Result<(), FrameError> {
        self.ring.set_epoch(epoch, senders, now_ms)?;
        // Windows of epochs that are gone never match again; keep the map small.
        self.windows.retain(|(_, e, _), _| *e + 2 >= epoch);
        Ok(())
    }

    /// Open one message from `sender`. An error means: drop the message.
    pub fn open(&mut self, sender: u64, frame: &[u8], now_ms: u64) -> Result<Vec<u8>, DataError> {
        if frame.len() > MAX_WIRE {
            return Err(DataError::TooLarge);
        }
        let t = parse_trailer(frame)?;
        let (epoch, plain) = self.ring.decrypt(sender, Codec::Data, frame, now_ms)?;
        // Only an authentic message may move the window (otherwise a forged counter could lock a sender out).
        let w = self
            .windows
            .entry((sender, epoch, t.instance.0))
            .or_default();
        if !w.accept(t.counter) {
            return Err(DataError::Replay);
        }
        Ok(plain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret(b: u8) -> SenderSecret {
        SenderSecret::new([b; 16])
    }
    const IID: InstanceId = InstanceId([1, 2, 3, 4]);

    fn pair() -> (DataSender, DataReceiver) {
        let tx = DataSender::new(&secret(1), IID).unwrap();
        let mut rx = DataReceiver::new(1000);
        rx.set_epoch(1, [(42, secret(1))], 0).unwrap();
        (tx, rx)
    }

    #[test]
    fn round_trip_and_the_body_is_encrypted() {
        let (mut tx, mut rx) = pair();
        let msg = br#"{"v":1,"t":"state","ops":[{"k":"board","d":"secret","ver":1,"by":"42"}]}"#;
        let wire = tx.seal(msg).unwrap();
        // The SFU sees no plaintext: the body differs everywhere and nothing readable survives.
        assert!(
            wire[..msg.len()]
                .iter()
                .zip(msg)
                .filter(|(a, b)| a == b)
                .count()
                < msg.len() / 4
        );
        assert!(!wire.windows(6).any(|w| w == b"secret"));
        assert_eq!(rx.open(42, &wire, 1).unwrap(), msg);
        assert_eq!(tx.counter(), 1);
        // Empty messages are authenticated too.
        let e = tx.seal(&[]).unwrap();
        assert_eq!(rx.open(42, &e, 2).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn counter_makes_every_ciphertext_different() {
        let (mut tx, _) = pair();
        assert_ne!(tx.seal(b"same").unwrap(), tx.seal(b"same").unwrap());
    }

    #[test]
    fn tampering_wrong_key_wrong_sender_and_media_codec_are_rejected() {
        let (mut tx, mut rx) = pair();
        let wire = tx.seal(b"hello activity").unwrap();
        for i in 0..wire.len() - 3 {
            let mut bad = wire.clone();
            bad[i] ^= 1;
            assert!(rx.open(42, &bad, 1).is_err(), "byte {i}");
        }
        // Not our sender, so no key.
        assert_eq!(
            rx.open(7, &wire, 1),
            Err(DataError::Frame(FrameError::NoKey))
        );
        // Another room's key.
        let mut other = DataReceiver::new(0);
        other.set_epoch(1, [(42, secret(9))], 0).unwrap();
        assert_eq!(
            other.open(42, &wire, 1),
            Err(DataError::Frame(FrameError::Auth))
        );
        // A data frame cannot be passed off as an Opus media frame, nor the reverse.
        let mut ring = KeyRing::new(0);
        ring.set_epoch(1, [(42, secret(1))], 0).unwrap();
        assert!(ring.decrypt(42, Codec::Opus, &wire, 1).is_err());
        let mut media = FrameEncryptor::new(&secret(1), IID).unwrap();
        let opus = media.encrypt(Codec::Opus, b"hello activity").unwrap();
        assert!(rx.open(42, &opus, 1).is_err());
    }

    #[test]
    fn a_replayed_message_is_dropped_but_the_original_still_counts() {
        let (mut tx, mut rx) = pair();
        let a = tx.seal(b"a").unwrap();
        let b = tx.seal(b"b").unwrap();
        assert_eq!(rx.open(42, &a, 1).unwrap(), b"a");
        assert_eq!(rx.open(42, &a, 2), Err(DataError::Replay));
        assert_eq!(rx.open(42, &b, 3).unwrap(), b"b");
        assert_eq!(rx.open(42, &b, 4), Err(DataError::Replay));
    }

    #[test]
    fn out_of_order_inside_the_window_is_accepted_once_and_old_ones_are_not() {
        let (mut tx, mut rx) = pair();
        let msgs: Vec<Vec<u8>> = (0..70).map(|i| tx.seal(&[i as u8]).unwrap()).collect();
        assert!(rx.open(42, &msgs[10], 1).is_ok());
        assert!(rx.open(42, &msgs[8], 1).is_ok()); // late but inside the window
        assert_eq!(rx.open(42, &msgs[8], 1), Err(DataError::Replay));
        assert!(rx.open(42, &msgs[69], 1).is_ok()); // jumps ahead by 59
        assert!(rx.open(42, &msgs[12], 1).is_ok()); // 69 - 12 = 57, inside the window
        assert_eq!(rx.open(42, &msgs[0], 1), Err(DataError::Replay)); // 69 behind: out of the window
    }

    #[test]
    fn a_forged_counter_cannot_advance_the_window() {
        let (mut tx, mut rx) = pair();
        let mut forged = tx.seal(b"x").unwrap();
        let n = forged.len();
        // Bump the counter byte (index n - 5 for a one-byte counter): authentication fails.
        forged[n - 5] = forged[n - 5].wrapping_add(50);
        assert!(rx.open(42, &forged, 1).is_err());
        let real = tx.seal(b"y").unwrap();
        assert_eq!(rx.open(42, &real, 2).unwrap(), b"y");
    }

    #[test]
    fn size_limits() {
        let (mut tx, mut rx) = pair();
        assert_eq!(tx.seal(&vec![0; MAX_MESSAGE + 1]), Err(DataError::TooLarge));
        let big = tx.seal(&vec![7; MAX_MESSAGE]).unwrap();
        assert_eq!(rx.open(42, &big, 1).unwrap().len(), MAX_MESSAGE);
        assert_eq!(
            rx.open(42, &vec![0; MAX_WIRE + 1], 1),
            Err(DataError::TooLarge)
        );
    }

    #[test]
    fn two_senders_have_independent_windows_and_epochs_roll() {
        let mut a = DataSender::new(&secret(1), IID).unwrap();
        let mut b = DataSender::new(&secret(2), IID).unwrap();
        let mut rx = DataReceiver::new(1000);
        rx.set_epoch(1, [(1, secret(1)), (2, secret(2))], 0)
            .unwrap();
        let (ma, mb) = (a.seal(b"a").unwrap(), b.seal(b"b").unwrap());
        assert!(rx.open(1, &ma, 1).is_ok());
        assert!(rx.open(2, &mb, 1).is_ok());
        // A new epoch: new keys work, the old epoch's in-flight message still opens in the grace window.
        let late = a.seal(b"late").unwrap();
        let mut a2 = DataSender::new(&secret(11), IID).unwrap();
        rx.set_epoch(2, [(1, secret(11)), (2, secret(2))], 100)
            .unwrap();
        assert!(rx.open(1, &a2.seal(b"new").unwrap(), 101).is_ok());
        assert!(rx.open(1, &late, 102).is_ok());
    }
}
