//! Search errors.

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("index error: {0}")]
    Index(String),
    #[error(transparent)]
    Query(#[from] crate::query::QueryError),
}

impl From<tantivy::TantivyError> for SearchError {
    fn from(e: tantivy::TantivyError) -> Self {
        Self::Index(e.to_string())
    }
}

impl From<std::io::Error> for SearchError {
    fn from(e: std::io::Error) -> Self {
        Self::Index(e.to_string())
    }
}

impl From<tantivy::directory::error::OpenDirectoryError> for SearchError {
    fn from(e: tantivy::directory::error::OpenDirectoryError) -> Self {
        Self::Index(e.to_string())
    }
}

pub type SearchResult<T> = Result<T, SearchError>;
