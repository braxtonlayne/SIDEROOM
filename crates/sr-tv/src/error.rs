use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TvError {
    #[error("unknown {0}")]
    NotFound(&'static str),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Conflict(String),
    #[error("missing platform permission {0}")]
    MissingPermission(&'static str),
    /// The airing programme is age restricted and the viewer has not confirmed their age (D-184).
    #[error("age confirmation required")]
    AgeConfirmationRequired,
    /// Nothing with a stream is on air.
    #[error("channel is off air")]
    OffAir,
    #[error("storage failure: {0}")]
    Storage(String),
    #[error("catalog failure: {0}")]
    Catalog(String),
}

pub type TvResult<T> = Result<T, TvError>;
