//! sr-gateway: WebSocket gateway protocol (FND-API-006). See docs/02-architecture.md
//! section 4.2 and docs/05-api-and-gateway.md section 6.
//!
//! * [`Gateway`] holds sessions, per-guild subscriber sets and presence/typing,
//!   and fans out `sr-events` domain events with per-recipient permission checks.
//! * [`run_connection`] is the transport-agnostic protocol state machine.
//! * [`gateway_handler`] / [`routes`] adapt it to an axum WebSocket upgrade.
//! * [`Authenticator`] and [`GuildDirectory`] decouple it from auth and the DB.

pub mod compress;
pub mod conn;
pub mod hub;
pub mod protocol;
mod session;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
pub mod traits;
pub mod voice;
mod voice_call;
mod voice_sound;
mod voice_stage;
mod voice_stream;
pub mod ws;

pub use compress::Compression;
pub use conn::{ConnectParams, Frame, run_connection};
pub use hub::{Gateway, GatewayConfig, LARGE_THRESHOLD_DEFAULT, LARGE_THRESHOLD_MAX};
pub use protocol::{Identify, Payload, PresenceUpdate, Resume, close, intents, op};
pub use session::SessionHandle;
pub use traits::{AuthError, AuthedUser, Authenticator, GuildDirectory, OpHandler};
pub use voice::{
    JoinDenied, ModAction, ModError, ModOutcome, ModPatch, VoiceChannelInfo, VoiceChannelKind,
    VoiceConfig, VoiceGuildInfo, VoiceLimits, VoiceState, allowance_ok, is_stage_moderator,
};
pub use voice_call::{CallError, CallHooks, CallSummary, RING_TIMEOUT};
pub use voice_sound::{SoundPlay, SoundPlayError};
pub use voice_stage::{SelfStagePatch, StageError, StageInstance};
pub use voice_stream::{PreviewError, stream_key};
pub use ws::{GatewayQuery, SESSION_COOKIE, gateway_handler, routes};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod voice_tests;
