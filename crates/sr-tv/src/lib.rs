//! sr-tv: Live TV, the schedule engine and virtual-linear HLS (docs/specs/cinema-live-tv.md, inventory 15).
//!
//! * [`engine`]: the pure, deterministic schedule generator (rules, catalog pools, time zone, cursor).
//! * [`schedule`]: pure edits of a channel's blocks (placing programmes, fitting generated blocks around them).
//! * [`live`]: live-style HLS playlists cut from the wall clock out of stored VOD renditions.
//! * [`service`]: channels, the rolling schedule, now/next, the guide, staff controls.
//! * Ports: [`catalog::TvCatalog`] (the Cinema catalog), [`repo::TvRepository`] (storage; [`pg::PgTvRepo`]),
//!   `sr_media::playback::PlaybackSource` (renditions), `sr_core::Clock` (time), [`presence::Presence`].
//!
//! Nothing here has played a real stream; see the handoff for what is verified.

pub mod access;
pub mod catalog;
pub mod engine;
pub mod error;
pub mod gate;
pub mod guide;
pub mod live;
pub mod model;
pub mod pg;
pub mod pool_source;
pub mod presence;
pub mod repo;
pub mod schedule;
pub mod service;
pub mod tz;

pub use access::ChannelAccess;
pub use catalog::{CatalogItem, MemoryCatalog, TitleRef, TvCatalog};
pub use error::{TvError, TvResult};
pub use presence::{MemoryPresence, Presence};
pub use repo::{MemoryTvRepo, TvRepository};
pub use service::{
    Actor, LiveDeps, ReminderNotice, ReminderSink, ReminderView, SlateSource, TvService, Viewer,
};

#[cfg(test)]
mod resolve_tests;
#[cfg(test)]
mod service_tests;
#[cfg(test)]
mod shelf_tests;
