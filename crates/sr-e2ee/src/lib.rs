//! sr-e2ee: end-to-end encryption for voice and video calls (D-053, spec
//! docs/specs/lounge-voice-video.md section 5, modeled on Discord's DAVE).
//!
//! What lives here; apart from MLS (which draws its own randomness through openmls), pure
//! functions without I/O, clocks or randomness of their own, so the same code runs natively
//! and in the browser through `sr-e2ee-wasm`:
//!
//! - [`codec`]: which bytes of an encoded Opus, VP8 or H.264 frame stay in the clear so
//!   packetizers, depacketizers and the SFU keep working (VOX-E2E-002).
//! - [`frame`]: the per-frame transform: AES-128-GCM, a 12-byte nonce from a frame counter,
//!   the tag truncated to 8 bytes, and the trailer format (VOX-E2E-002).
//! - [`keys`]: per-sender media keys derived from the secret each sender gets for an epoch,
//!   and the receiver's key ring that keeps the previous epoch for a grace window
//!   (VOX-E2E-006).
//! - [`privacy`]: the D-250 privacy codes (pairwise and per call) shown in the shield popover
//!   (VOX-E2E-007).
//! - [`sequencer`]: the delivery service's per-call commit ordering (one commit wins per
//!   epoch), used by the gateway's op 101 relay (VOX-E2E-004). It never sees key material.
//! - [`mls`]: the MLS key agreement on `openmls` (VOX-E2E-001, D-249): one group per call,
//!   identity-certified session credentials, roster-checked commits, welcomes, the delivery
//!   service's external-sender key, and each sender's per-epoch base secret from the exporter
//!   ([`keys::SENDER_SECRET_LABEL`]).
//!
//! On by default (`voice.e2ee` on the server, `VITE_SR_VOICE_E2EE` in the web build, D-249) since the
//! media path passed in Chromium and Firefox (VOX-E2E-003).

pub mod codec;
pub mod frame;
pub mod keys;
pub mod mls;
pub mod privacy;
pub mod sequencer;

pub use codec::Codec;
pub use frame::{FrameDecryptor, FrameEncryptor, FrameError};
pub use keys::{KeyRing, SenderSecret};
