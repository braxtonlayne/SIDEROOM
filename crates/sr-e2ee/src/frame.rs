//! The per-frame transform (VOX-E2E-002): AES-128-GCM over the non-clear bytes of an encoded
//! frame, a 12-byte nonce built from a frame counter, the tag truncated to 8 bytes.
//!
//! Wire format of an encrypted frame:
//!
//! ```text
//! body (same length as the input; clear ranges as they were, the rest ciphertext)
//! tag        8 bytes   first 8 bytes of the GCM tag
//! instance   4 bytes   encryptor instance id (no zero byte)
//! counter    1-5 bytes ULEB128, 1..=u32::MAX
//! version    1 byte    1
//! length     1 byte    length of this trailer, tag through magic
//! magic      2 bytes   0x53 0x52 ("SR")
//! ```
//!
//! Nonce: 8 zero bytes then the counter (u32 big-endian). The key is unique per sender,
//! epoch and encryptor instance (see [`crate::keys`]) and the counter only goes up, so a
//! nonce is never used twice under one key; at `u32::MAX` the encryptor stops and the client
//! must wait for the next epoch.
//!
//! Authenticated data: `"sr-e2ee/v1"`, the codec id, the body length (u32 BE), then for every
//! clear range its start and length (u32 BE each) and its bytes. So the clear bytes cannot be
//! changed, and a frame cannot be replayed as another codec.
//!
//! The 8-byte tag (as in DAVE) is checked by computing the full tag over the received
//! ciphertext and comparing its first 8 bytes in constant time. RustCrypto's GCM only takes
//! 12 to 16-byte tags, so the receiver decrypts with the same CTR keystream GCM uses
//! (`ctr::Ctr32BE`, starting at counter block 2), re-encrypts the plaintext with
//! `aes-gcm` to get the full tag, and releases the plaintext only if the tags match. Both are
//! stock RustCrypto primitives; the tests check the keystream equals `aes-gcm`'s.
//!
//! Nothing is ever passed through in the clear: an unknown codec, a frame without a valid
//! trailer, or a tag mismatch is an error, and the caller drops the frame.

use std::ops::Range;

use aes::Aes128;
use aes_gcm::aead::AeadInPlace;
use aes_gcm::{Aes128Gcm, KeyInit};
use ctr::cipher::{KeyIvInit, StreamCipher};
use subtle::ConstantTimeEq;
use zeroize::Zeroize;

use crate::codec::{self, Codec};
use crate::keys::{MediaKey, SenderSecret};

pub const TAG_LEN: usize = 8;
pub const INSTANCE_LEN: usize = 4;
pub const VERSION: u8 = 1;
pub const MAGIC: [u8; 2] = [0x53, 0x52];
/// Smallest and largest trailers (1 and 5-byte counters).
pub const MIN_TRAILER: usize = TAG_LEN + INSTANCE_LEN + 1 + 4;
pub const MAX_TRAILER: usize = TAG_LEN + INSTANCE_LEN + 5 + 4;
/// Largest frame accepted (a 4K keyframe is well below this).
pub const MAX_FRAME: usize = 4 << 20;
/// H.264 frames whose ciphertext looks like a start code are re-encrypted with the next
/// counter; this many tries in a row failing is treated as an error (it is astronomically
/// unlikely with honest input).
pub const MAX_ATTEMPTS: u32 = 16;
const AAD_LABEL: &[u8] = b"sr-e2ee/v1";

type Aes128Ctr = ctr::Ctr32BE<Aes128>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    /// No trailer, a bad trailer, or a frame too large.
    #[error("malformed frame")]
    Malformed,
    /// A codec this transform does not know.
    #[error("unsupported codec")]
    Unsupported,
    /// The tag did not match: wrong key, or the frame was changed.
    #[error("authentication failed")]
    Auth,
    /// No key for this sender.
    #[error("no key for sender")]
    NoKey,
    /// An epoch older than or equal to the current one.
    #[error("stale epoch")]
    StaleEpoch,
    /// The frame counter ran out; wait for the next epoch.
    #[error("frame counter exhausted")]
    Exhausted,
    /// Could not avoid start-code emulation in an H.264 frame.
    #[error("start code emulation")]
    Emulation,
    /// An instance id with a zero byte.
    #[error("bad instance id")]
    BadInstance,
}

/// A random per-encryptor id (4 bytes, none of them zero; see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InstanceId(pub [u8; INSTANCE_LEN]);

impl InstanceId {
    /// Build from random bytes, replacing zero bytes by 1 (a negligible loss of entropy that
    /// keeps the trailer free of `00 00` for H.264).
    pub fn from_random(mut b: [u8; INSTANCE_LEN]) -> Self {
        for x in &mut b {
            if *x == 0 {
                *x = 1;
            }
        }
        InstanceId(b)
    }
}

fn nonce(counter: u32) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[8..].copy_from_slice(&counter.to_be_bytes());
    n
}

fn aad(codec: Codec, body: &[u8], clear: &[Range<usize>]) -> Vec<u8> {
    let clear_bytes: usize = clear.iter().map(|r| r.len() + 8).sum();
    let mut a = Vec::with_capacity(AAD_LABEL.len() + 5 + clear_bytes);
    a.extend_from_slice(AAD_LABEL);
    a.push(codec.id());
    a.extend_from_slice(&(body.len() as u32).to_be_bytes());
    for r in clear {
        a.extend_from_slice(&(r.start as u32).to_be_bytes());
        a.extend_from_slice(&(r.len() as u32).to_be_bytes());
        a.extend_from_slice(&body[r.clone()]);
    }
    a
}

fn gather(body: &[u8], ranges: &[Range<usize>]) -> Vec<u8> {
    let mut v = Vec::with_capacity(ranges.iter().map(|r| r.len()).sum());
    for r in ranges {
        v.extend_from_slice(&body[r.clone()]);
    }
    v
}

fn scatter(body: &mut [u8], ranges: &[Range<usize>], src: &[u8]) {
    let mut at = 0;
    for r in ranges {
        body[r.clone()].copy_from_slice(&src[at..at + r.len()]);
        at += r.len();
    }
}

fn uleb128(mut v: u32, out: &mut Vec<u8>) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

/// Minimal ULEB128 that exactly fills `b`.
fn read_uleb128(b: &[u8]) -> Option<u32> {
    if b.is_empty() || b.len() > 5 {
        return None;
    }
    let mut v: u64 = 0;
    for (i, &x) in b.iter().enumerate() {
        let last = i + 1 == b.len();
        if last == (x & 0x80 != 0) {
            return None;
        }
        if last && x == 0 && i > 0 {
            return None; // not minimal
        }
        v |= u64::from(x & 0x7f) << (7 * i);
    }
    u32::try_from(v).ok()
}

/// A parsed trailer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trailer {
    pub body_len: usize,
    pub tag: [u8; TAG_LEN],
    pub instance: InstanceId,
    pub counter: u32,
}

pub fn parse_trailer(frame: &[u8]) -> Result<Trailer, FrameError> {
    let n = frame.len();
    if !(MIN_TRAILER..=MAX_FRAME + MAX_TRAILER).contains(&n) || frame[n - 2..] != MAGIC {
        return Err(FrameError::Malformed);
    }
    let len = usize::from(frame[n - 3]);
    if frame[n - 4] != VERSION || !(MIN_TRAILER..=MAX_TRAILER).contains(&len) || len > n {
        return Err(FrameError::Malformed);
    }
    let t = n - len;
    let tag: [u8; TAG_LEN] = frame[t..t + TAG_LEN].try_into().expect("tag length");
    let iid: [u8; INSTANCE_LEN] = frame[t + TAG_LEN..t + TAG_LEN + INSTANCE_LEN]
        .try_into()
        .expect("instance length");
    let counter = read_uleb128(&frame[t + TAG_LEN + INSTANCE_LEN..n - 4])
        .filter(|&c| c >= 1)
        .ok_or(FrameError::Malformed)?;
    if iid.contains(&0) {
        return Err(FrameError::Malformed);
    }
    Ok(Trailer {
        body_len: t,
        tag,
        instance: InstanceId(iid),
        counter,
    })
}

/// The instance id of an encrypted frame (to pick the key).
pub fn peek_instance(frame: &[u8]) -> Result<InstanceId, FrameError> {
    parse_trailer(frame).map(|t| t.instance)
}

/// Encrypts one sender's frames for one epoch. Not `Clone`: a copy would repeat counters.
pub struct FrameEncryptor {
    gcm: Aes128Gcm,
    iid: InstanceId,
    counter: u32,
}

impl FrameEncryptor {
    pub fn new(secret: &SenderSecret, iid: InstanceId) -> Result<Self, FrameError> {
        if iid.0.contains(&0) {
            return Err(FrameError::BadInstance);
        }
        let key = secret.media_key(iid);
        Ok(FrameEncryptor {
            gcm: Aes128Gcm::new((&key.0).into()),
            iid,
            counter: 0,
        })
    }

    /// The last counter used (0 before the first frame).
    pub fn counter(&self) -> u32 {
        self.counter
    }

    pub fn instance(&self) -> InstanceId {
        self.iid
    }

    pub fn encrypt(&mut self, codec: Codec, frame: &[u8]) -> Result<Vec<u8>, FrameError> {
        if frame.len() > MAX_FRAME {
            return Err(FrameError::Malformed);
        }
        let clear = codec::clear_ranges(codec, frame);
        let enc = codec::encrypted_ranges(&clear, frame.len());
        let aad = aad(codec, frame, &clear);
        for _ in 0..MAX_ATTEMPTS {
            if self.counter == u32::MAX {
                return Err(FrameError::Exhausted);
            }
            self.counter += 1;
            let mut buf = gather(frame, &enc);
            let tag = self
                .gcm
                .encrypt_in_place_detached(&nonce(self.counter).into(), &aad, &mut buf)
                .map_err(|_| FrameError::Malformed)?;
            let mut out = Vec::with_capacity(frame.len() + MAX_TRAILER);
            out.extend_from_slice(frame);
            scatter(&mut out, &enc, &buf);
            let start = out.len();
            out.extend_from_slice(&tag[..TAG_LEN]);
            out.extend_from_slice(&self.iid.0);
            uleb128(self.counter, &mut out);
            out.push(VERSION);
            let len = out.len() - start + 3;
            out.push(len as u8);
            out.extend_from_slice(&MAGIC);
            if codec == Codec::H264 && codec::has_start_code_emulation(&out, &clear) {
                continue;
            }
            return Ok(out);
        }
        Err(FrameError::Emulation)
    }
}

/// Decrypts the frames of one sender's encryptor instance in one epoch.
pub struct FrameDecryptor {
    key: MediaKey,
    gcm: Aes128Gcm,
    iid: InstanceId,
}

impl FrameDecryptor {
    pub fn new(secret: &SenderSecret, iid: InstanceId) -> Self {
        let key = secret.media_key(iid);
        FrameDecryptor {
            gcm: Aes128Gcm::new((&key.0).into()),
            key,
            iid,
        }
    }

    pub fn decrypt(&self, codec: Codec, frame: &[u8]) -> Result<Vec<u8>, FrameError> {
        let t = parse_trailer(frame)?;
        if t.instance != self.iid {
            return Err(FrameError::NoKey);
        }
        let body = &frame[..t.body_len];
        let clear = codec::clear_ranges(codec, body);
        let enc = codec::encrypted_ranges(&clear, body.len());
        let aad = aad(codec, body, &clear);
        let n = nonce(t.counter);
        let ct = gather(body, &enc);

        // Plaintext from GCM's own keystream: counter block 2 onwards (block 1 masks the tag).
        let mut pt = ct.clone();
        let mut iv = [0u8; 16];
        iv[..12].copy_from_slice(&n);
        iv[15] = 2;
        Aes128Ctr::new((&self.key.0).into(), (&iv).into()).apply_keystream(&mut pt);

        // The full tag over the received ciphertext: re-encrypting the plaintext gives back
        // the same ciphertext and its tag.
        let mut check = pt.clone();
        let full = self
            .gcm
            .encrypt_in_place_detached(&n.into(), &aad, &mut check)
            .map_err(|_| FrameError::Malformed)?;
        let ok = full[..TAG_LEN].ct_eq(&t.tag) & check.as_slice().ct_eq(&ct);
        if !bool::from(ok) {
            pt.zeroize();
            return Err(FrameError::Auth);
        }
        let mut out = body.to_vec();
        scatter(&mut out, &enc, &pt);
        pt.zeroize();
        Ok(out)
    }
}

#[cfg(test)]
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use super::*;
    use crate::codec::tests::idr_frame;

    fn secret() -> SenderSecret {
        SenderSecret::new(*b"0123456789abcdef")
    }
    const IID: InstanceId = InstanceId([0xa1, 0xb2, 0xc3, 0xd4]);

    fn pair() -> (FrameEncryptor, FrameDecryptor) {
        let s = secret();
        (
            FrameEncryptor::new(&s, IID).unwrap(),
            FrameDecryptor::new(&s, IID),
        )
    }

    #[test]
    fn round_trip_every_codec() {
        let (mut tx, rx) = pair();
        let opus = b"\xfc\x01\x02 opus payload".to_vec();
        let vp8k = [
            &[0x10u8, 0x02, 0x00, 0x9d, 0x01, 0x2a, 0x80, 0x02, 0xe0, 0x01][..],
            &[7u8; 50],
        ]
        .concat();
        let vp8d = [&[0x31u8, 0x02, 0x00][..], &[9u8; 30]].concat();
        for (codec, f) in [
            (Codec::Opus, opus),
            (Codec::Vp8, vp8k),
            (Codec::Vp8, vp8d),
            (Codec::H264, idr_frame()),
        ] {
            let e = tx.encrypt(codec, &f).unwrap();
            assert_eq!(rx.decrypt(codec, &e).unwrap(), f, "{codec:?}");
            // The clear prefix is untouched, the rest is not.
            let clear = codec::clear_ranges(codec, &f);
            for r in &clear {
                assert_eq!(e[r.clone()], f[r.clone()]);
            }
            let enc = codec::encrypted_ranges(&clear, f.len());
            assert!(
                enc.iter().all(|r| e[r.clone()] != f[r.clone()]),
                "{codec:?}"
            );
        }
        assert_eq!(tx.counter(), 4);
    }

    #[test]
    fn empty_frame_still_authenticated() {
        let (mut tx, rx) = pair();
        let e = tx.encrypt(Codec::Opus, &[]).unwrap();
        assert_eq!(e.len(), MIN_TRAILER);
        assert_eq!(rx.decrypt(Codec::Opus, &e).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn tampering_is_rejected() {
        let (mut tx, rx) = pair();
        let f = idr_frame();
        let e = tx.encrypt(Codec::H264, &f).unwrap();
        // Every single byte of the frame matters (body, tag, instance, counter).
        let t = parse_trailer(&e).unwrap();
        for i in 0..t.body_len + TAG_LEN + INSTANCE_LEN + 1 {
            let mut bad = e.clone();
            bad[i] ^= 0x01;
            assert!(rx.decrypt(Codec::H264, &bad).is_err(), "byte {i}");
        }
        // Wrong codec, wrong key, truncated, plaintext.
        assert_eq!(rx.decrypt(Codec::Vp8, &e), Err(FrameError::Auth));
        let other = FrameDecryptor::new(&SenderSecret::new([9; 16]), IID);
        assert_eq!(other.decrypt(Codec::H264, &e), Err(FrameError::Auth));
        assert_eq!(
            rx.decrypt(Codec::H264, &e[..e.len() - 1]),
            Err(FrameError::Malformed)
        );
        assert_eq!(rx.decrypt(Codec::H264, &f), Err(FrameError::Malformed));
    }

    #[test]
    fn counters_never_repeat_and_stop_at_max() {
        let (mut tx, _) = pair();
        let a = tx.encrypt(Codec::Opus, b"x").unwrap();
        let b = tx.encrypt(Codec::Opus, b"x").unwrap();
        assert_ne!(a, b);
        assert_eq!(parse_trailer(&b).unwrap().counter, 2);
        tx.counter = u32::MAX - 1;
        let last = tx.encrypt(Codec::Opus, b"x").unwrap();
        assert_eq!(parse_trailer(&last).unwrap().counter, u32::MAX);
        assert_eq!(last.len(), 1 + MAX_TRAILER);
        assert_eq!(tx.encrypt(Codec::Opus, b"x"), Err(FrameError::Exhausted));
    }

    #[test]
    fn instance_ids_have_no_zero() {
        assert_eq!(InstanceId::from_random([0, 5, 0, 7]).0, [1, 5, 1, 7]);
        assert!(matches!(
            FrameEncryptor::new(&secret(), InstanceId([0, 1, 2, 3])),
            Err(FrameError::BadInstance)
        ));
    }

    #[test]
    fn uleb_is_minimal() {
        for v in [1u32, 127, 128, 16_383, 16_384, u32::MAX] {
            let mut b = Vec::new();
            uleb128(v, &mut b);
            assert_eq!(read_uleb128(&b), Some(v));
            assert!(!b.contains(&0));
        }
        assert_eq!(read_uleb128(&[0x80, 0x00]), None);
        assert_eq!(read_uleb128(&[0x81]), None);
        assert_eq!(read_uleb128(&[0xff, 0xff, 0xff, 0xff, 0x7f]), None); // > u32
    }

    #[test]
    fn ctr_keystream_matches_aes_gcm() {
        // The decrypt path's CTR must equal aes-gcm's encryption keystream.
        let key = secret().media_key(IID);
        let gcm = Aes128Gcm::new((&key.0).into());
        let n = nonce(77);
        let pt = [0x5au8; 100];
        let mut ct = pt;
        gcm.encrypt_in_place_detached(&n.into(), b"", &mut ct)
            .unwrap();
        let mut iv = [0u8; 16];
        iv[..12].copy_from_slice(&n);
        iv[15] = 2;
        let mut ks = pt;
        Aes128Ctr::new((&key.0).into(), (&iv).into()).apply_keystream(&mut ks);
        assert_eq!(&ct[..100], &ks[..]);
    }

    #[test]
    fn h264_output_has_no_emulation_and_same_layout() {
        let (mut tx, rx) = pair();
        let f = idr_frame();
        for _ in 0..2000 {
            let e = tx.encrypt(Codec::H264, &f).unwrap();
            let t = parse_trailer(&e).unwrap();
            assert_eq!(
                codec::h264_start_codes(&e[..t.body_len]),
                codec::h264_start_codes(&f)
            );
            assert_eq!(rx.decrypt(Codec::H264, &e).unwrap(), f);
        }
    }
}
