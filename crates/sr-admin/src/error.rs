//! Errors of the admin backend. The HTTP layer maps each variant to a status.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AdminError {
    /// The caller is not platform staff (no appointment, not in the Staff Server, or no role).
    #[error("this action requires platform staff privileges")]
    NotStaff,
    /// Staff, but the roles lack the named platform permission.
    #[error("missing platform permission {0}")]
    MissingPermission(&'static str),
    /// A rule forbids the action (Owner immutable, high-level grants need the Owner, ...).
    #[error("{0}")]
    Forbidden(String),
    /// A reason is required for this action.
    #[error("a reason is required for this action")]
    ReasonRequired,
    #[error("unknown {0}")]
    NotFound(&'static str),
    /// Bad input.
    #[error("{0}")]
    Invalid(String),
    /// The state of the target does not allow the action.
    #[error("{0}")]
    Conflict(String),
    #[error("storage failure: {0}")]
    Storage(String),
}

pub type AdminResult<T> = Result<T, AdminError>;
