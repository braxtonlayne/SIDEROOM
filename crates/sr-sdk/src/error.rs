//! Errors returned by the SDK.

use serde_json::Value;

/// Result alias used across the SDK.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Everything that can go wrong talking to a SIDE ROOM server.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The server answered with an error body `{code, message, errors?}`.
    #[error("api error {status} (code {code}): {message}")]
    Api {
        status: u16,
        code: u32,
        message: String,
        errors: Option<Value>,
    },
    /// The server kept answering 429 after the retry budget was spent.
    #[error("rate limited (retry after {retry_after_ms} ms)")]
    RateLimited { retry_after_ms: u64 },
    /// The connection or request failed before a usable answer arrived.
    #[error("transport error: {0}")]
    Transport(String),
    /// A body could not be decoded into the expected type.
    #[error("decode error: {0}")]
    Decode(String),
    /// The gateway closed with a code a reconnect cannot fix (bad token, bad intents...).
    #[error("gateway closed with fatal code {code}: {reason}")]
    GatewayFatal { code: u16, reason: &'static str },
    /// The caller used the SDK wrongly (for example a reply to a non-interaction).
    #[error("invalid use: {0}")]
    Invalid(String),
}

impl Error {
    /// The API error code, if this is an [`Error::Api`].
    pub fn api_code(&self) -> Option<u32> {
        match self {
            Error::Api { code, .. } => Some(*code),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Decode(e.to_string())
    }
}
