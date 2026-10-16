//! sr-e2ee: end-to-end encryption for voice and video calls (D-053, spec
//! docs/specs/lounge-voice-video.md section 5, modeled on Discord's DAVE).
//!
//! What lives here, all pure functions without I/O, clocks or randomness of their own (the
//! caller passes time in milliseconds and random bytes in), so the same code runs natively
//! and in the browser through `sr-e2ee-wasm`:
//!
//! - [`codec`]: which bytes of an encoded Opus, VP8 or H.264 frame stay in the clear so
//!   packetizers, depacketizers and the SFU keep working (VOX-E2E-002).
//! - [`frame`]: the per-frame transform: AES-128-GCM, a 12-byte nonce from a frame counter,
//!   the tag truncated to 8 bytes, and the trailer format (VOX-E2E-002).
//! - [`keys`]: per-sender media keys derived from the secret each sender gets for an epoch,
//!   and the receiver's key ring that keeps the previous epoch for a grace window
//!   (VOX-E2E-006).
//! - [`privacy`]: the per-participant privacy code shown in the shield popover
//!   (VOX-E2E-007).
//! - [`sequencer`]: the delivery service's per-call commit ordering (one commit wins per
//!   epoch), used by the gateway's op 101 relay (VOX-E2E-004). It never sees key material.
//!
//! **Not here yet: MLS (VOX-E2E-001).** The spec's key agreement is MLS (RFC 9420) through
//! `openmls`. The `openmls` crates are not in this machine's offline cargo cache and
//! downloading them needs the owner's approval, so the group logic is not built. The seam is
//! [`keys::SENDER_SECRET_LABEL`]: once a group exists, each sender's base secret is the MLS
//! exporter output `MLS-Exporter(SENDER_SECRET_LABEL, sender_user_id as u64 LE, 16)`, and
//! everything below it (frame transform, key ring, privacy codes) already works from that.
//! Until then E2EE stays off (`voice.e2ee`, default off) and nothing here is fed real keys.

pub mod codec;
pub mod frame;
pub mod keys;
pub mod privacy;
pub mod sequencer;

pub use codec::Codec;
pub use frame::{FrameDecryptor, FrameEncryptor, FrameError};
pub use keys::{KeyRing, SenderSecret};
