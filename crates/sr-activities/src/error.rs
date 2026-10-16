//! Errors of the Activities backend. The HTTP layer maps each variant to a status.

use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ActivityError {
    /// Unknown, not visible or not approved. The cases are not told apart so activities cannot be probed.
    #[error("unknown {0}")]
    NotFound(&'static str),
    #[error("missing permissions")]
    Forbidden,
    /// Bad input: `(field, message)`.
    #[error("{1}")]
    Invalid(&'static str, String),
    /// The state does not allow the action (already running, not pending).
    #[error("{0}")]
    Conflict(String),
    #[error("storage failure: {0}")]
    Storage(String),
}

impl From<sqlx::Error> for ActivityError {
    fn from(e: sqlx::Error) -> Self {
        Self::Storage(e.to_string())
    }
}

pub type ActivityResult<T> = Result<T, ActivityError>;
