//! Errors of the moderation backend. The HTTP layer maps each variant to a status.

use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ModError {
    /// Unknown or not visible to the caller. The two are not told apart, so rows cannot be probed.
    #[error("unknown {0}")]
    NotFound(&'static str),
    /// The caller may not do this.
    #[error("missing permissions")]
    Forbidden,
    /// A platform permission is missing (named, so the log and the client can say which).
    #[error("missing platform permission {0}")]
    MissingPermission(&'static str),
    /// A reason is required for this action.
    #[error("a reason is required")]
    ReasonRequired,
    /// Bad input (the message names the field).
    #[error("{0}")]
    Invalid(String),
    /// The state of the row does not allow the action.
    #[error("{0}")]
    Conflict(String),
    /// Too many reports today.
    #[error("too many reports; try again tomorrow")]
    RateLimited,
    #[error("storage failure: {0}")]
    Storage(String),
}

impl From<sqlx::Error> for ModError {
    fn from(e: sqlx::Error) -> Self {
        Self::Storage(e.to_string())
    }
}

impl From<sr_admin::AdminError> for ModError {
    fn from(e: sr_admin::AdminError) -> Self {
        use sr_admin::AdminError as A;
        match e {
            A::NotStaff => Self::Forbidden,
            A::MissingPermission(p) => Self::MissingPermission(p),
            A::Forbidden(_) => Self::Forbidden,
            A::ReasonRequired => Self::ReasonRequired,
            A::NotFound(w) => Self::NotFound(w),
            A::Invalid(m) => Self::Invalid(m),
            A::Conflict(m) => Self::Conflict(m),
            A::Storage(m) => Self::Storage(m),
        }
    }
}

pub type ModResult<T> = Result<T, ModError>;

pub fn invalid(m: impl Into<String>) -> ModError {
    ModError::Invalid(m.into())
}

pub fn conflict(m: impl Into<String>) -> ModError {
    ModError::Conflict(m.into())
}
