//! sr-sdk: a small, typed bot SDK for SIDE ROOM (DEV-SDK-001).
//!
//! It speaks the Discord-shaped bot API of docs/specs/developer-platform.md:
//!
//! * [`Rest`] - REST client with the bot token, 429 retries and helpers for messages,
//!   commands and interaction responses;
//! * [`gateway`] - one shard per connection (HELLO, IDENTIFY/RESUME, heartbeats, reconnect),
//!   run as many as `GET /gateway/bot` recommends by [`Client`];
//! * [`Cache`] - the bot user, guilds, channels and roles, kept current from dispatches;
//! * [`CommandRouter`] - build slash and context-menu commands, route interactions to async
//!   handlers, read typed arguments, reply, defer, follow up and autocomplete.
//!
//! See `examples/echo_bot.rs`. Transports are traits, so tests need no network.

pub mod cache;
pub mod client;
pub mod command;
pub mod error;
pub mod gateway;
pub mod http;
pub mod model;
pub mod ops;
#[cfg(feature = "voice-e2ee")]
pub mod voice;
#[cfg(feature = "voice-transport")]
pub mod voice_transport;

pub use cache::Cache;
pub use client::{Client, ClientBuilder, EventHandler, Shards};
pub use command::{
    Args, CommandContext, CommandDef, CommandRouter, Context, OptionDef, OptionKind,
};
pub use error::{Error, Result};
pub use gateway::Event;
pub use http::{MessageBody, Rest};
pub use model::{Interaction, Message, User, intents};
pub use ops::{GatewayHandle, MembersChunk, PresenceUpdate, RequestGuildMembers, Status};
