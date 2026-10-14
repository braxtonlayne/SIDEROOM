//! Errors of the Cinema social backend. The HTTP layer maps each variant to a status.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SocialError {
    /// Unknown, unpublished, hidden or not-visible object. The cases are not distinguished so
    /// private lists and unpublished titles cannot be probed.
    #[error("unknown {0}")]
    NotFound(&'static str),
    /// The title is age-restricted and the account has not confirmed it is 18 or older (D-032).
    #[error("age confirmation required")]
    AgeConfirmationRequired,
    /// Bad input.
    #[error("{0}")]
    Invalid(String),
    /// The state of the target does not allow the action (limits, duplicates).
    #[error("{0}")]
    Conflict(String),
    #[error("storage failure: {0}")]
    Storage(String),
}

impl From<sqlx::Error> for SocialError {
    fn from(e: sqlx::Error) -> Self {
        Self::Storage(e.to_string())
    }
}

pub type SocialResult<T> = Result<T, SocialError>;
