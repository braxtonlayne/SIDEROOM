//! Errors of the packages backend. The HTTP layer maps each variant to a status.

use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum PackageError {
    /// Unknown or not visible; the cases are not told apart.
    #[error("unknown {0}")]
    NotFound(&'static str),
    #[error("missing permissions")]
    Forbidden,
    /// A reviewer tried to decide on their own package (D-258).
    #[error("You cannot review your own package.")]
    SelfReview,
    /// Bad input: `(field, message)`.
    #[error("{1}")]
    Invalid(&'static str, String),
    /// The state does not allow the action.
    #[error("{0}")]
    Conflict(String),
    /// Too many requests from one address (the public guestbook).
    #[error("too many requests")]
    RateLimited,
    /// A package that failed validation: every problem found.
    #[error("the package was refused")]
    Refused(Vec<String>),
    #[error("storage failure: {0}")]
    Storage(String),
}

impl From<sqlx::Error> for PackageError {
    fn from(e: sqlx::Error) -> Self {
        Self::Storage(e.to_string())
    }
}

pub type PackageResult<T> = Result<T, PackageError>;
