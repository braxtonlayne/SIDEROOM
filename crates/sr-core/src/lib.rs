//! sr-core: domain types, Snowflake IDs, permission bitfields, errors, time, config types.
//! See docs/02-architecture.md section 3.
//!
//! Everything except [`config`] is wasm-compatible (no I/O, no threads required).

#[cfg(feature = "config")]
pub mod config;
pub mod error;
pub mod ids;
pub mod member_flags;
pub mod permissions;
pub mod platform_permissions;
pub mod snowflake;
pub mod time;

pub use error::{ApiError, ErrorCode};
pub use ids::*;
pub use permissions::Permissions;
pub use platform_permissions::PlatformPermissions;
pub use snowflake::{Snowflake, SnowflakeGenerator};
#[cfg(not(target_arch = "wasm32"))]
pub use time::SystemClock;
pub use time::{Clock, ManualClock};
