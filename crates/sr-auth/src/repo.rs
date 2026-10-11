//! Repository traits for later wiring to `sr-db` (this crate must not depend
//! on it). Methods are async via return-position `impl Future` (Send) so no
//! `async-trait` dependency is needed. Times are Unix seconds, ids are
//! snowflake `i64`s.
//!
//! Atomicity requirements are documented per method; implementors must
//! honour them in SQL (e.g. `UPDATE .. WHERE consumed_at IS NULL RETURNING`).

use crate::{invite::InviteCode, session::TokenHash};
use std::future::Future;

/// Stored session record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRecord {
    /// Snowflake id of the session row.
    pub id: i64,
    /// Owning account.
    pub user_id: i64,
    /// Hash of the cookie token.
    pub token_hash: TokenHash,
    /// Creation time.
    pub created_at: i64,
    /// Last use time.
    pub last_seen_at: i64,
    /// Current rolling expiry.
    pub expires_at: i64,
    /// Device label derived from the user agent.
    pub device_name: Option<String>,
}

/// Session persistence (ENT-AUTH-005).
pub trait SessionRepository: Send + Sync {
    /// Repository error type.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Insert a new session.
    fn create(&self, record: SessionRecord)
    -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Look up by token hash (callers check expiry with `SessionPolicy`).
    fn find_by_hash(
        &self,
        hash: &TokenHash,
    ) -> impl Future<Output = Result<Option<SessionRecord>, Self::Error>> + Send;
    /// Roll the expiry forward and bump `last_seen_at`.
    fn touch(
        &self,
        id: i64,
        last_seen_at: i64,
        expires_at: i64,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// List an account's live sessions.
    fn list_for_user(
        &self,
        user_id: i64,
        now: i64,
    ) -> impl Future<Output = Result<Vec<SessionRecord>, Self::Error>> + Send;
    /// Revoke one session; returns whether a row was removed.
    fn revoke(
        &self,
        user_id: i64,
        session_id: i64,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// Revoke all of an account's sessions except `keep` (e.g. after password change).
    fn revoke_all_except(
        &self,
        user_id: i64,
        keep: Option<i64>,
    ) -> impl Future<Output = Result<u64, Self::Error>> + Send;
}

/// Stored TOTP state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TotpRecord {
    /// Owning account.
    pub user_id: i64,
    /// Secret bytes, encrypted by the implementation (never plaintext at rest).
    pub secret: Vec<u8>,
    /// False until the first code is confirmed during enrolment.
    pub confirmed: bool,
    /// Highest step already consumed (replay protection).
    pub last_used_step: Option<u64>,
}

/// TOTP persistence (ENT-AUTH-008).
pub trait TotpRepository: Send + Sync {
    /// Repository error type.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Fetch the account's TOTP record, if any.
    fn get(
        &self,
        user_id: i64,
    ) -> impl Future<Output = Result<Option<TotpRecord>, Self::Error>> + Send;
    /// Insert or replace the (unconfirmed) record during enrolment.
    fn put_pending(
        &self,
        user_id: i64,
        secret: &[u8],
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Mark confirmed and record the step used. Must be atomic.
    fn confirm(
        &self,
        user_id: i64,
        step: u64,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Atomically advance `last_used_step` only if `step` is greater; returns
    /// whether it advanced (false means replay).
    fn consume_step(
        &self,
        user_id: i64,
        step: u64,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// Remove TOTP for the account (disable).
    fn delete(&self, user_id: i64) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// Recovery-code persistence (ENT-AUTH-009).
pub trait RecoveryCodeRepository: Send + Sync {
    /// Repository error type.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Replace all of the account's codes with `hashes` in one transaction.
    fn replace_all(
        &self,
        user_id: i64,
        hashes: &[TokenHash],
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Unused digests as `(row_id, digest)`.
    fn list_unused(
        &self,
        user_id: i64,
    ) -> impl Future<Output = Result<Vec<(i64, TokenHash)>, Self::Error>> + Send;
    /// Atomically mark one code used; false if it was already consumed.
    fn consume(
        &self,
        user_id: i64,
        row_id: i64,
        now: i64,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
}

/// Lifecycle of a stored invite (doc 06 §3, §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InviteStatus {
    /// Redeemable.
    Open,
    /// Used to create an account.
    Redeemed,
    /// Revoked by its issuer or staff.
    Revoked,
}

/// Stored platform invite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InviteRecord {
    /// Canonical code.
    pub code: String,
    /// Issuing account.
    pub issuer_id: i64,
    /// Private note visible to issuer and staff only.
    pub note: Option<String>,
    /// Current status.
    pub status: InviteStatus,
    /// Creation time.
    pub created_at: i64,
}

/// Invite persistence (ENT-INV-*). Redemption itself is a multi-table
/// transaction owned by the registration service in `sr-db`, so it is not
/// modelled here.
pub trait InviteRepository: Send + Sync {
    /// Repository error type.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Insert a new open invite. A unique-violation on the code should be
    /// surfaced so the caller can regenerate.
    fn create(&self, record: InviteRecord) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Look up by canonical code.
    fn find(
        &self,
        code: &InviteCode,
    ) -> impl Future<Output = Result<Option<InviteRecord>, Self::Error>> + Send;
    /// Count the issuer's open codes (for the `open < credits` rule).
    fn count_open(&self, issuer_id: i64) -> impl Future<Output = Result<u64, Self::Error>> + Send;
    /// Revoke an open invite owned by `issuer_id`; false if not open (e.g.
    /// a redemption won the race).
    fn revoke(
        &self,
        issuer_id: i64,
        code: &InviteCode,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
}

/// Purpose-scoped single-use email token persistence (ENT-AUTH-012/013).
pub trait EmailTokenRepository: Send + Sync {
    /// Repository error type.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Store a token hash; supersedes earlier unconsumed tokens of the same
    /// purpose for the account.
    fn put(
        &self,
        user_id: i64,
        purpose: crate::email_token::EmailTokenPurpose,
        hash: &TokenHash,
        expires_at: i64,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Atomically consume a valid (unexpired, unconsumed) token and return
    /// its account id. Must be single-use under concurrency.
    fn consume(
        &self,
        purpose: crate::email_token::EmailTokenPurpose,
        hash: &TokenHash,
        now: i64,
    ) -> impl Future<Output = Result<Option<i64>, Self::Error>> + Send;
}
