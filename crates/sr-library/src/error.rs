//! Errors of the Library backend. The HTTP layer maps each variant to a status.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LibraryError {
    /// Unknown, unpublished, merged-away or age-hidden object. The cases are not
    /// distinguished so a hidden series cannot be probed.
    #[error("unknown {0}")]
    NotFound(&'static str),
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
