//! sr-payments: self-custodial, watch-only Bitcoin donations, entitlements and limits.
//!
//! See docs/specs/donations.md and docs/08-donations-perks-and-cost-model.md. The crate has no
//! spending, signing or broadcasting code: it derives receive addresses from an operator-supplied
//! watch-only descriptor and reads the chain through [`chain::ChainSource`].

pub mod chain;
pub mod config;
pub mod descriptor;
pub mod entitlements;
pub mod error;
pub mod invoice;
pub mod memory;
pub mod rates;
pub mod repo;
pub mod service;

pub use config::{DonationConfig, Limits, PerkSet, VideoProfile, effective_limits};
pub use entitlements::{
    EntitlementService, Entitlements, GuildEntitlement, PatronStatus, UserEntitlement,
};
pub use error::{PayError, PayResult};
pub use invoice::{Invoice, InvoiceState, Purpose};
pub use service::PaymentService;

/// The `SR_PAYMENT_UPDATE` gateway event name (user-scoped, to the payer).
pub const PAYMENT_EVENT: &str = "SR_PAYMENT_UPDATE";
/// The `SR_ENTITLEMENT_UPDATE` gateway event name.
pub const ENTITLEMENT_EVENT: &str = "SR_ENTITLEMENT_UPDATE";

#[cfg(test)]
mod service_tests;
