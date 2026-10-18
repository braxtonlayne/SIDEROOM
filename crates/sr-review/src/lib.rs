//! sr-review: the shared submissions and review pipeline (docs/specs/submissions-and-review.md,
//! inventory 12).
//!
//! * [`machine`]: the pure state machine (draft, submitted, in_review, changes_requested,
//!   approved, rejected, published, withdrawn).
//! * [`kind`]: the [`SubmissionKind`] trait and registry; `sr-library` implements the first
//!   kinds (`library.series`, `library.items`).
//! * [`service`]: submitter and reviewer actions, leases, decisions, requests, notifications.
//! * [`repo`]: the storage port; `sr-db` implements it over Postgres, [`memory`] for tests.

pub mod error;
pub mod kind;
pub mod machine;
pub mod memory;
pub mod model;
pub mod repo;
pub mod service;
pub mod similar;

#[cfg(test)]
mod tests;

pub use error::{ReviewError, ReviewResult};
pub use kind::{KindDraft, KindRegistry, SubmissionKind};
pub use service::{Caller, EditAs, IdSource, ReviewEvents, ReviewService};

/// Name of the gateway event delivered to the submitter (doc 05).
pub const EVENT_NAME: &str = "SR_SUBMISSION_UPDATE";
