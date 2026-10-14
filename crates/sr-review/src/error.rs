//! Errors of the review pipeline. The HTTP layer maps each variant to a status.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReviewError {
    /// Unknown submission, item or request (also for one the caller may not see).
    #[error("unknown {0}")]
    NotFound(&'static str),
    /// The caller lacks the named platform permission.
    #[error("missing platform permission {0}")]
    MissingPermission(&'static str),
    /// The caller may not act on this submission (not the submitter, own submission, no lease).
    #[error("{0}")]
    Forbidden(String),
    /// Bad input (empty reason, unknown kind, invalid payload).
    #[error("{0}")]
    Invalid(String),
    /// The state of the submission does not allow the action (illegal transition, lease held
    /// by someone else, rate limit).
    #[error("{0}")]
    Conflict(String),
    /// The submission failed the kind's checks at submit time.
    #[error("the submission is not ready: {}", .0.join("; "))]
    NotReady(Vec<String>),
    #[error("storage failure: {0}")]
    Storage(String),
}

pub type ReviewResult<T> = Result<T, ReviewError>;
