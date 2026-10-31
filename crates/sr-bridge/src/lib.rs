//! sr-bridge: the Discord bridge (BRG-COR-001, wave 35; D-381).
//!
//! The bridge is off on every server until its owner switches the module `bridge.enabled` on, pastes the token of
//! their own Discord bot and maps channels. This crate is the half that lives in the server process:
//!
//! * [`model`]: the rows (bridge, channel map, message map, opt-outs, outbox) and their queries.
//! * [`service`]: [`BridgeService`], the handle the routes and the wiring use: connect and revoke the token,
//!   map and unmap channels, the opt-out, the status, the audit row.
//! * [`discord`]: the Discord REST calls the bridge makes (`GET /users/@me`, webhooks), over `sr-sdk`'s
//!   transport, with the per-webhook rate-limit state.
//! * [`outbound`]: SIDE ROOM messages to Discord: rendering to Discord markdown, the queue and its worker.
//! * [`teardown`]: [`BridgeService::teardown`], the clean stop.
//!
//! The inbound half (the gateway shard, `MESSAGE_CREATE` to a SIDE ROOM message) is BRG-GW-001 and lives in
//! `inbound/` next to these modules; it builds on the same model.
//!
//! Real Discord is never contacted by tests: every call goes through [`discord::DiscordApi`], whose base URL is
//! configuration (the fake Discord of `e2e/support/fake-discord.mjs` in the end-to-end suite).

pub mod error;
pub mod model;
pub mod service;

pub use error::{BridgeError, BridgeResult};
pub use model::{Bridge, BridgeStatus, ChannelMap, Direction, MessageMap, MessageMapDirection};
pub use service::{BridgeService, IdSource, TokenSealer};
