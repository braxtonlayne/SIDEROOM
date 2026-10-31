//! sr-bridge: the Discord bridge (BRG-COR-001, wave 35; D-381).
//!
//! The bridge is off on every server until its owner switches the module `bridge.enabled` on, pastes the token of
//! their own Discord bot and maps channels. This crate is the half that lives in the server process:
//!
//! * [`model`]: the rows (bridge, channel map, message map) and their queries.
//! * [`service`]: [`BridgeService`], the handle the routes and the server wiring use, and its ports.
//! * [`tokens`]: connecting the owner's bot (a bot token only, sealed).
//! * [`maps`]: mapping and unmapping channels, with the Discord webhook each sending map owns.
//! * [`discord`]: the Discord REST calls the bridge makes, over `sr-sdk`'s transport, with rate-limit state.
//! * [`render`]: SIDE ROOM content to what Discord should show.
//! * [`outbound`]: messages to the queue, and the queue to Discord.
//! * [`teardown`]: [`BridgeService::teardown`], the clean stop.
//! * [`status`]: what the owner's screen and the members' banner read; the data-request side.
//!
//! The inbound half (the gateway shard, `MESSAGE_CREATE` to a SIDE ROOM message) is BRG-GW-001 and lives in
//! `inbound/` next to these modules; it builds on the same model and registers a
//! [`TeardownHook`](service::TeardownHook) so a teardown stops its shard.
//!
//! Real Discord is never contacted by tests: every call goes through [`discord::DiscordApi`], whose base URL is
//! configuration (the fake Discord of `e2e/support/fake-discord.mjs` in the end-to-end suite).

pub mod discord;
pub mod error;
pub mod inbound;
pub mod maps;
pub mod model;
pub mod outbound;
pub mod render;
pub mod service;
pub mod status;
pub mod teardown;
pub mod tokens;

pub use discord::{DiscordApi, redact};
pub use error::{BridgeError, BridgeResult};
pub use model::{Bridge, BridgeStatus, ChannelMap, Direction, MessageMap, MessageMapDirection};
pub use service::{
    BridgeNotices, BridgeService, IdSource, MediaUrls, NoNotices, RelativeUrls, TeardownHook,
    TokenSealer, Tuning,
};
pub use status::{MapView, MemberExport, StatusReport, Totals};
pub use teardown::{Leftover, TeardownReason, TeardownReport};
