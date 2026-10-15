//! sr-apps: the developer platform (DEV-APP, DEV-BOT, docs/specs/developer-platform.md).
//!
//! * [`service::AppsService`] holds the rules: onboarding, applications and bot accounts,
//!   credentials (only digests are stored), release review, installs, OAuth2 with PKCE,
//!   application commands and the interaction lifecycle.
//! * [`repo::AppsRepository`] is the storage port; `sr-db` implements it over Postgres and
//!   [`memory::MemoryAppsRepo`] over RAM for tests.
//! * [`commands`], [`interactions`], [`perms`] and [`manifest`] are pure rule sets.
//! * [`auth::AppsAuth`] plugs bot tokens and OAuth2 bearer tokens into the REST extractors
//!   (replacing the `NoBots`/`NoBearers` stubs, open question 56).

pub mod auth;
pub mod commands;
pub mod error;
pub mod interactions;
pub mod manifest;
pub mod memory;
pub mod model;
pub mod perms;
pub mod repo;
pub mod service;
pub mod signing;

#[cfg(test)]
mod tests;

pub use error::{AppsError, AppsResult, FieldProblem, OAuthError};
pub use model::*;
pub use repo::AppsRepository;
pub use service::{AppsService, IdSource};
