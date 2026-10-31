//! Errors of the Watch Party backend. The HTTP layer maps each variant to a status.

use thiserror::Error;

use crate::model::PlaybackView;

#[derive(Debug, Error, PartialEq)]
pub enum PartyError {
    /// Unknown or not-visible party, channel or title. The cases are not distinguished so parties
    /// cannot be probed.
    #[error("unknown {0}")]
    NotFound(&'static str),
    /// The caller may not do this (not the host, not a controller, not allowed to start or join).
    #[error("missing permissions")]
    Forbidden,
    /// The title is age-restricted and the account has not confirmed it is 18 or older (D-184).
    #[error("age confirmation required")]
    AgeConfirmationRequired,
    /// Bad input.
    #[error("{0}")]
    Invalid(String),
    /// The state of the party does not allow the action (full, ended, already exists).
    #[error("{0}")]
    Conflict(String),
    /// A playback command carried a stale `expected_version`; `current` is the state to re-sync from.
    #[error("playback state changed")]
    VersionConflict { current: Box<PlaybackView> },
    /// The operator switched off the feature the request needs (`platform.watch_together_embeds`).
    #[error("feature disabled")]
    FeatureDisabled,
    /// Too many commands or reactions in a short time.
    #[error("rate limited")]
    RateLimited,
    #[error("storage failure: {0}")]
    Storage(String),
}

impl From<sqlx::Error> for PartyError {
    fn from(e: sqlx::Error) -> Self {
        Self::Storage(e.to_string())
    }
}

pub type PartyResult<T> = Result<T, PartyError>;
