//! sr-activities: Activities (docs/11 section 3, DEV-ACT-001/002). An application ships a web app that runs
//! in a sandboxed iframe on its own origin (`https://<application-id>.activities.<host>/`), inside a voice
//! channel. This crate holds
//!
//! * [`rules`]: pure validation of the entry URL and URL mappings, and the proxy's request-to-target mapping;
//! * [`service::ActivityService`]: the per-application settings with their review state (the proxy serves
//!   only an entry document whose SHA-256 equals the reviewed one), activity instances (one running per voice
//!   channel) and their participants;
//! * [`ports`]: channel permissions and gateway events.
//!
//! The HTTP layer (owner and staff routes, instance routes, the proxy) is `sr_api::routes::activities`.

pub mod error;
pub mod model;
pub mod ports;
pub mod rules;
pub mod service;

pub use error::{ActivityError, ActivityResult};
pub use ports::{ActivityEvents, BusEvents, ChannelAccess, ChannelPerms, NoChannels, NoEvents};
pub use service::{ActivityService, IdSource};
