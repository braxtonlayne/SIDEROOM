//! Errors of the developer platform services. The HTTP layer maps them to Discord error
//! codes (`sr-api::routes::developer`).

use thiserror::Error;

/// One field-level validation problem: dotted path, code, message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldProblem {
    /// Dotted path (`options.0.name`).
    pub path: String,
    /// Discord-style code (`BASE_TYPE_REQUIRED`).
    pub code: String,
    /// Message.
    pub message: String,
}

impl FieldProblem {
    /// New problem.
    pub fn new(path: impl Into<String>, code: &str, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            code: code.to_owned(),
            message: message.into(),
        }
    }
}

/// What went wrong.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AppsError {
    /// The named thing does not exist or is not the caller's to see.
    #[error("unknown {0}")]
    NotFound(&'static str),
    /// Refused: the caller lacks standing (maps to 50001).
    #[error("{0}")]
    Forbidden(String),
    /// Refused: a guild permission is missing (maps to 50013).
    #[error("missing permissions")]
    MissingPermissions,
    /// Invalid input (maps to 50035 with the field tree).
    #[error("invalid form body")]
    Invalid(Vec<FieldProblem>),
    /// The current state does not allow the action (maps to 900010).
    #[error("{0}")]
    Conflict(String),
    /// A development-state application outside its test servers (900120).
    #[error("app not released")]
    NotReleased,
    /// The application is suspended (900121).
    #[error("application suspended")]
    Suspended,
    /// Unknown or expired interaction (10062).
    #[error("unknown interaction")]
    UnknownInteraction,
    /// A second acknowledgement (40060).
    #[error("interaction already acknowledged")]
    AlreadyAcknowledged,
    /// Wrong interaction token (50027).
    #[error("invalid webhook token")]
    InvalidToken,
    /// Unknown command (10063).
    #[error("unknown application command")]
    UnknownCommand,
    /// Storage failure; logged, answered as 500.
    #[error("storage: {0}")]
    Storage(String),
}

impl AppsError {
    /// A single-field validation error.
    pub fn field(path: &str, code: &str, message: impl Into<String>) -> Self {
        Self::Invalid(vec![FieldProblem::new(path, code, message)])
    }
}

/// Result alias.
pub type AppsResult<T> = Result<T, AppsError>;

/// An RFC 6749 error from the OAuth2 token endpoints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthError {
    /// `invalid_request`, `invalid_client`, `invalid_grant`, ...
    pub error: &'static str,
    /// Human-readable detail (never contains a secret).
    pub description: String,
}

impl OAuthError {
    /// New error.
    pub fn new(error: &'static str, description: impl Into<String>) -> Self {
        Self {
            error,
            description: description.into(),
        }
    }

    /// HTTP status: 401 for `invalid_client`, 500 for `server_error`, else 400.
    pub fn status(&self) -> u16 {
        match self.error {
            "invalid_client" => 401,
            "server_error" => 500,
            _ => 400,
        }
    }
}
