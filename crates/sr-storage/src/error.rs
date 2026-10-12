//! Error types for sr-storage.

use thiserror::Error;

/// Convenience alias.
pub type Result<T, E = StorageError> = std::result::Result<T, E>;

/// Errors from the storage layer.
#[derive(Debug, Error)]
pub enum StorageError {
    /// The object does not exist.
    #[error("object not found: {0}")]
    NotFound(String),
    /// Presigning is unavailable for this backend.
    #[error("presigned URLs are not supported by this backend")]
    PresignUnsupported,
    /// Any other backend failure.
    #[error("storage backend error: {0}")]
    Backend(String),
    /// Invalid object path.
    #[error("invalid object path: {0}")]
    InvalidPath(String),
}

impl From<object_store::Error> for StorageError {
    fn from(e: object_store::Error) -> Self {
        match e {
            object_store::Error::NotFound { path, .. } => Self::NotFound(path),
            other => Self::Backend(other.to_string()),
        }
    }
}

impl From<object_store::path::Error> for StorageError {
    fn from(e: object_store::path::Error) -> Self {
        Self::InvalidPath(e.to_string())
    }
}
