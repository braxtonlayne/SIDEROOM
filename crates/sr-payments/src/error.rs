//! Errors of the payments crate.

use crate::descriptor::DescriptorError;

pub type PayResult<T> = Result<T, PayError>;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PayError {
    /// Donations are switched off or no wallet is configured.
    #[error("donations are not enabled")]
    Disabled,
    #[error("{0}")]
    Invalid(String),
    #[error("not found")]
    NotFound,
    /// The current state does not allow the action.
    #[error("{0}")]
    Conflict(String),
    /// Too many unused addresses in a row; a restore from the xpub would miss later ones.
    #[error("the address gap limit is reached; wait for pending invoices or ask staff to raise it")]
    GapLimit,
    /// No fresh BTC/USD rate.
    #[error("no current exchange rate is available")]
    RateUnavailable,
    #[error("too many open invoices")]
    TooManyOpen,
    #[error("the chain source failed: {0}")]
    Chain(String),
    #[error("descriptor: {0}")]
    Descriptor(#[from] DescriptorError),
    #[error("storage: {0}")]
    Storage(String),
}
