//! sr-events: domain event bus. See docs/02-architecture.md section 5.
//!
//! `EventBus` is the abstraction; `InProcessBus` is the v1 implementation
//! (tokio broadcast channel per topic). `contract` holds reusable tests that
//! any future implementation (e.g. a Postgres LISTEN/NOTIFY bus) must pass.

pub mod bus;
pub mod contract;
pub mod event;
pub mod ids;

pub use bus::{EventBus, EventError, InProcessBus};
pub use event::{DomainEvent, Topic};
pub use ids::{ChannelId, GuildId, UserId};
