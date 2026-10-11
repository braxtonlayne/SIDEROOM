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
pub mod traits;
pub mod ws;

pub use compress::Compression;
pub use conn::{ConnectParams, Frame, run_connection};
pub use hub::{Gateway, GatewayConfig};
pub use protocol::{Identify, Payload, PresenceUpdate, Resume, close, intents, op};
pub use traits::{AuthError, AuthedUser, Authenticator, GuildDirectory};
pub use ws::{GatewayQuery, gateway_handler, routes};

#[cfg(test)]
mod tests;
