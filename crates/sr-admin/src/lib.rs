//! sr-admin: the platform admin backend (Phase 3, `ADM-*`).
//!
//! * [`service::AdminService`] enforces platform permissions (doc 07 section 3),
//!   reason rules and protection rules, and writes every action to the
//!   hash-chained staff access log (doc 07 section 5).
//! * [`repo::AdminRepository`] is the storage port; `sr-db` implements it over
//!   Postgres and [`memory::MemoryAdminRepo`] over RAM for tests.
//! * [`chain`] is the hash chain, [`settings`] the catalog of editable limits,
//!   [`rules`] the appointment rules and the default staff roles.

pub mod chain;
pub mod error;
pub mod memory;
pub mod model;
pub mod repo;
pub mod rules;
pub mod service;
pub mod settings;

#[cfg(test)]
mod tests;

pub use error::{AdminError, AdminResult};
pub use model::*;
pub use repo::AdminRepository;
pub use service::{AdminService, SYSTEM_USER_ID};
