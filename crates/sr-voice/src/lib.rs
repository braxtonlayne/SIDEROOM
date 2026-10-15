//! sr-voice: the SFU (ADR-0003). [`core`] is the sans-IO engine on str0m; the rest of the
//! crate is the tokio shell around it ([`server`]: UDP/TCP sockets and the actor,
//! [`ws`]: the voice WebSocket, [`metrics`]). The wire contract is
//! `docs/specs/lounge-voice-video.md` section 4.

pub mod config;
pub mod core;
pub mod governor;
pub mod keyframe;
pub mod layers;
pub mod loadtest;
pub mod loss;
pub mod metrics;
pub mod server;
pub mod speaking;
pub mod ws;

pub use config::SfuConfig;
pub use server::{SfuEvent, SfuEventKind, SfuHandle, StartError, start};
pub use ws::router;
