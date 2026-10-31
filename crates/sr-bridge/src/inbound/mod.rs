//! The inbound half of the bridge (BRG-GW-001): Discord to SIDE ROOM over the gateway.
//!
//! * [`convert`]: Discord payloads to plain values (names, links with their expiry, mention-safe text).
//! * [`handler`]: [`Inbound`], one dispatch to its effect: create, edit, delete and reactions of mapped
//!   channels, the loop guard, reply resolution through `bridge_message_map`, the abuse limit.
//! * [`manager`]: [`InboundManager`], one `sr-sdk` shard per bridge, at most `bridge.max_bridges`.
//! * [`sink`]: [`ChatSink`], what the chat side provides (the server wiring implements it).
//! * [`store`], [`limit`]: queries and the per-bridge message limit.

pub mod convert;
pub mod handler;
pub mod limit;
pub mod manager;
pub mod members;
pub mod sink;
pub mod store;
pub mod structure;

pub use handler::{Inbound, InboundConfig, Outcome, Why};
pub use manager::{
    INBOUND_INTENTS, InboundHost, InboundManager, Reconciled, ServiceHost, intent_ladder,
    intent_ladder_for_mode,
};
pub use sink::{ChatSink, InboundAuthor, InboundPost};
/// The real gateway transport (WebSocket over rustls), for the server wiring.
pub use sr_sdk::gateway::WsConnector;
pub use structure::{Change, StructureEvent, StructureKind, StructureSink};
