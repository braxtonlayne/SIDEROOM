//! The persistence boundary now lives in `sr_auth::store` (open question 141),
//! so `sr-db` can implement it without depending on `sr-api`. This module
//! re-exports it for the route modules.

pub use sr_auth::store::{
    AccountRecord, AccountState, AuthStore, InviteState, RegisterRejection, RegisterTx, Registered,
    StoreError, StoreResult,
};
