//! Errors of the Library backend. The HTTP layer maps each variant to a status.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LibraryError {
    /// Unknown, unpublished or merged-away object. The cases are not distinguished
    /// so an unpublished series cannot be probed.
    #[error("unknown {0}")]
    NotFound(&'static str),
    /// The series is age-restricted and the account has not confirmed it is 18 or
    /// older (D-032, D-184). The object is visible; only opening its pages is gated.
    #[error("age confirmation required")]
    AgeConfirmationRequired,
    /// The caller lacks the named platform permission.
    #[error("missing platform permission {0}")]
    MissingPermission(&'static str),
    /// Bad input.
    #[error("{0}")]
    Invalid(String),
    /// The state of the target does not allow the action.
    #[error("{0}")]
    Conflict(String),
    #[error("storage failure: {0}")]
    Storage(String),
    #[error("search index failure: {0}")]
    Search(String),
}

impl From<tantivy::TantivyError> for LibraryError {
    fn from(e: tantivy::TantivyError) -> Self {
        Self::Search(e.to_string())
    }
}

pub type LibraryResult<T> = Result<T, LibraryError>;
