//! Error type for the auth primitives.

/// Errors from auth primitives. Messages never include secret material.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AuthError {
    /// The operating system RNG failed.
    #[error("secure random source unavailable")]
    Random,
    /// Password hashing failed (invalid parameters or internal error).
    #[error("password hashing failed")]
    Hash,
    /// The stored hash string could not be parsed.
    #[error("stored hash is malformed")]
    MalformedHash,
    /// Invalid argon2 parameters.
    #[error("invalid argon2 parameters")]
    InvalidParams,
    /// TOTP configuration or secret problem.
    #[error("totp error: {0}")]
    Totp(String),
    /// A code or token had the wrong shape.
    #[error("malformed code or token")]
    Malformed,
}
