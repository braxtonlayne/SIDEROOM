//! `sr-core` — domain types shared by every SIDE ROOM crate.
//!
//! See docs/02-architecture.md §3 and §7. This crate must stay free of I/O
//! so it can also be compiled to WASM for the client.

pub mod clock;
pub mod id;
pub mod snowflake;

pub use clock::{Clock, FixedClock, SystemClock};
pub use snowflake::{Snowflake, SnowflakeGenerator};
