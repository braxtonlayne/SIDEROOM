//! sr-voice-proto: the contract between the gateway, the SFU (`sr-voice`) and web
//! clients. Pure data and pure functions, no I/O.
//!
//! * [`wire`]: the voice WebSocket messages (docs/specs/lounge-voice-video.md section 4).
//! * [`token`]: short-lived HMAC join tokens minted by the gateway and checked by the SFU.
//! * [`turn`]: coturn REST-style ephemeral credentials (ADR-0004).
//! * [`control`]: commands the gateway sends to the SFU (server mute, disconnect, ...).
//! * [`select`]: SFU node selection for a channel.
//! * [`video`]: camera and screen tracks, subscriber hints and the governor notice.

pub mod control;
pub mod sdp;
pub mod select;
pub mod token;
pub mod turn;
pub mod video;
pub mod wire;

pub use control::{VoiceCommand, VoiceRelay};
pub use token::{JoinClaims, TokenError, TokenSigner, derive_secret};
pub use turn::{IceServer, TurnConfig, TurnCredentials};
pub use wire::{Message, close, op};
