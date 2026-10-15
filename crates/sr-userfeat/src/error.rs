//! Errors of the feature modules. The HTTP layer maps each variant to a status.

use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum UfError {
    /// Unknown or not-visible row. Cases are not distinguished so rows cannot be probed.
    #[error("unknown {0}")]
    NotFound(&'static str),
    /// The caller may not do this.
    #[error("missing permissions")]
    Forbidden,
    /// Bad input (the message names the field).
    #[error("{0}")]
    Invalid(String),
    /// The state of the row does not allow the action (already sent, limit reached).
    #[error("{0}")]
    Conflict(String),
    /// The module is switched off for this server or platform.
    #[error("the {0} feature is turned off")]
    Disabled(&'static str),
    #[error("storage failure: {0}")]
    Storage(String),
}

impl From<sqlx::Error> for UfError {
    fn from(e: sqlx::Error) -> Self {
        Self::Storage(e.to_string())
    }
}

impl From<sr_features::FeatureError> for UfError {
    fn from(e: sr_features::FeatureError) -> Self {
        Self::Storage(e.to_string())
    }
}

pub type UfResult<T> = Result<T, UfError>;

/// `Invalid` from anything displayable.
pub fn invalid(m: impl Into<String>) -> UfError {
    UfError::Invalid(m.into())
}
