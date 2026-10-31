//! Errors of the bridge. The HTTP layer maps each variant to a status.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum BridgeError {
    /// Unknown or not-visible row.
    #[error("unknown {0}")]
    NotFound(&'static str),
    /// Bad input (the message names the field).
    #[error("{0}")]
    Invalid(String),
    /// The state does not allow the action (already mapped, no token yet).
    #[error("{0}")]
    Conflict(String),
    /// The token was refused: not a bot token, or Discord rejected it.
    #[error("{0}")]
    BadToken(String),
    /// Discord could not be reached, or answered with an error.
    #[error("discord: {0}")]
    Discord(String),
    /// A whole_guild plan no longer matches either side (409 BRIDGE_PLAN_STALE): rebuild it.
    #[error("the plan is stale")]
    PlanStale,
    /// A sealed value could not be written or read.
    #[error("sealed value unreadable")]
    Seal,
    #[error("storage failure: {0}")]
    Storage(String),
}

impl From<sqlx::Error> for BridgeError {
    fn from(e: sqlx::Error) -> Self {
        Self::Storage(e.to_string())
    }
}

impl From<sr_features::FeatureError> for BridgeError {
    fn from(e: sr_features::FeatureError) -> Self {
        Self::Storage(e.to_string())
    }
}

impl From<sr_sdk::Error> for BridgeError {
    fn from(e: sr_sdk::Error) -> Self {
        Self::Discord(e.to_string())
    }
}

pub type BridgeResult<T> = Result<T, BridgeError>;
