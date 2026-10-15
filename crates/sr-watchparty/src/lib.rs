//! sr-watchparty: Watch Parties (docs/specs/watch-parties.md). Synchronized playback of Cinema content for
//! a group: only control state is shared, each viewer streams the HLS media themselves.
//!
//! * [`state`]: the pure playback state machine (position arithmetic, versions), unit tested.
//! * [`rules`]: pure rules (who may control, buffering aggregation, host handover, queue order, limits).
//! * [`service::PartyService`] (with [`playback`]): the operations over Postgres. Mutations lock the party
//!   row, so commands are applied in order and `version` orders the states.
//! * [`ports`]: channel permissions, the catalog play check, the age gate, guild limits, gateway events
//!   (`WATCH_PARTY_UPDATE`, `WATCH_PARTY_PLAYBACK`, `WATCH_PARTY_REACTION`, `WATCH_PARTY_MESSAGE`) and the
//!   channel announcement hook.

pub mod error;
pub mod ids;
pub mod model;
pub mod playback;
pub mod ports;
pub mod rules;
pub mod service;
pub mod state;
pub mod store;

pub use error::{PartyError, PartyResult};
pub use model::Viewer;
pub use ports::{
    AgeLookup, Announcement, AnnouncementKind, BusEvents, ChannelAccess, ChannelKind, ChannelPerms,
    ChatHook, FixedLimit, GateError, GuildLimits, NoChannels, NoChat, NoEvents, NoGate, PartyEvents,
    PgAgeLookup, PlayGate, Resolved,
};
pub use service::{IdSource, PartyService};
