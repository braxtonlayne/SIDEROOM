//! sr-events: domain event bus. See docs/02-architecture.md section 5.
//!
//! `EventBus` is the abstraction; `InProcessBus` is the v1 implementation
//! (tokio broadcast channel per topic). `contract` holds reusable tests that
//! any future implementation (e.g. a Postgres LISTEN/NOTIFY bus) must pass.
//!
//! `Topic::Firehose` is a wildcard: every event published to any other topic is
//! also delivered there, for platform-internal consumers (search indexing, cache
//! invalidation). Use `FirehoseDedup` when each event must be seen once.

pub mod bus;
pub mod contract;
pub mod event;
pub mod firehose;
pub mod ids;

pub use bus::{EventBus, EventError, InProcessBus};
pub use event::{DomainEvent, StaffAlertKind, Topic};
pub use firehose::FirehoseDedup;
pub use ids::{ChannelId, GuildId, RoleId, UserId};
