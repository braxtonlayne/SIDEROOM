//! Server-settings domain (docs/specs/lounge-chat.md section 11, inventory
//! `06-lounge-server-settings.md`): guild invites, audit log, custom emoji and
//! stickers, AutoMod, webhooks, templates and community/onboarding flags.
//!
//! Everything here is pure: validation and rules take the current time as an
//! argument, persistence goes through the traits in [`repo`], and the Postgres
//! implementation lives in `sr-db::settings_repo`. The REST routes are in
//! `sr-api::routes::settings`.

pub mod applications;
pub mod audit;
pub mod automod;
pub mod community;
pub mod directory;
pub mod error;
pub mod expressions;
pub mod guide;
pub mod insights;
pub mod invites;
pub mod join;
pub mod repo;
pub mod service;
pub mod tags;
pub mod templates;
pub mod webhooks;

#[cfg(any(test, feature = "memory"))]
pub mod memory;
#[cfg(test)]
mod service_tests;

pub use error::{SettingsError, SettingsResult};
pub use service::{AuditObserver, SettingsService};
