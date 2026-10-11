//! sr-auth: pure, database-free authentication primitives.
//!
//! See docs/02-architecture.md §8 and docs/06-identity-accounts-invites.md.
//! Nothing here touches a database, network, or clock implicitly; persistence
//! is expressed as traits in [`repo`] to be implemented by `sr-db` later.
//!
//! Secrets (tokens, codes) have redacted `Debug` output and are never stored
//! raw: only their hashes ([`session::TokenHash`], argon2id PHC strings) go
//! to the database. All times are Unix seconds (`i64`) supplied by the caller.

pub mod crockford;
pub mod email_token;
pub mod error;
pub mod invite;
pub mod password;
pub mod random;
pub mod recovery;
pub mod repo;
pub mod session;
pub mod store;
pub mod tokens;
pub mod totp;

pub use error::AuthError;
