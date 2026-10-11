//! Persistence boundary for the Entrance/auth endpoints.
//!
//! [`AuthStore`] is a dyn-safe facade over what `sr-auth`'s repository traits
//! (those use `impl Future`, so they are not object safe) and the account /
//! registration SQL provide. A Postgres adapter delegates to
//! `sr_db::auth_repo::PgAuthRepo` for sessions, TOTP, recovery codes and email
//! tokens, and to `sr_db::registration` for [`AuthStore::register`].
//! [`super::memory::MemoryAuthStore`] is the in-memory implementation used by
//! tests and local bring-up.

use async_trait::async_trait;
use sr_auth::{
    email_token::EmailTokenPurpose,
    invite::InviteCode,
    repo::{SessionRecord, TotpRecord},
    session::TokenHash,
};

/// Opaque storage failure (maps to a 500).
#[derive(Debug, thiserror::Error)]
#[error("auth store error: {0}")]
pub struct StoreError(pub String);

/// Result alias for store calls.
pub type StoreResult<T> = Result<T, StoreError>;

/// Account lifecycle state (doc 06 section 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountState {
    /// Normal; counts toward the cap.
    Active,
    /// Staff-deactivated.
    Deactivated,
    /// User-deleted; can never log in.
    Deleted,
    /// Banned from the platform.
    PlatformBanned,
}

/// What the auth endpoints need to know about an account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountRecord {
    /// Snowflake id.
    pub id: i64,
    /// Lowercase username.
    pub username: String,
    /// Optional display name.
    pub display_name: Option<String>,
    /// Lowercase email.
    pub email: String,
    /// Whether the email was verified.
    pub email_verified: bool,
    /// Argon2id PHC string.
    pub password_hash: String,
    /// Lifecycle state.
    pub state: AccountState,
    /// Answer to the age gate, if asked (doc 06 section 7).
    pub age_confirmed: Option<bool>,
}

/// Lifecycle of an invite as seen by the public check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InviteState {
    /// Redeemable.
    Open,
    /// Already used.
    Redeemed,
    /// Revoked.
    Revoked,
}

/// Everything the registration transaction needs (doc 06 section 3.3).
#[derive(Debug, Clone)]
pub struct RegisterTx {
    /// New account id.
    pub account_id: i64,
    /// Person id to use if no existing Person matches (D-030).
    pub new_person_id: i64,
    /// Ledger row ids: `[issuer -1 redemption, new account +N initial_grant]`.
    pub ledger_ids: [i64; 2],
    /// Canonical invite code.
    pub code: InviteCode,
    /// Validated lowercase username.
    pub username: String,
    /// Validated display name.
    pub display_name: Option<String>,
    /// Validated lowercase email.
    pub email: String,
    /// Argon2id PHC string.
    pub password_hash: String,
    /// Maximum counted (active) accounts.
    pub account_cap: u64,
    /// Credits granted to the new account (`invites.initial_credits`).
    pub initial_credits: i32,
    /// Unix seconds.
    pub now: i64,
}

/// Why a registration was refused. Everything except the two taken
/// variants is reported to the client as one generic error (no oracle).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisterRejection {
    /// Unknown, used or revoked code.
    InviteUnavailable,
    /// The account cap is reached.
    AtCap,
    /// The issuer has no credit left.
    IssuerOutOfCredits,
    /// The matched Person is platform-banned (staff must be alerted).
    PersonBanned,
    /// Username already used (including by deleted accounts within 30 days).
    UsernameTaken,
    /// Email already used.
    EmailTaken,
}

/// A created account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registered {
    /// New account id.
    pub account_id: i64,
    /// Issuing account (for `SR_INVITE_CREDITS_UPDATE`).
    pub issuer_id: i64,
}

/// Storage used by the auth routes. Implementations must honour the atomicity
/// notes on each method.
#[async_trait]
pub trait AuthStore: Send + Sync {
    // ---- invites and registration -------------------------------------
    /// State of an invite, `None` if the code does not exist.
    async fn invite_state(&self, code: &InviteCode) -> StoreResult<Option<InviteState>>;
    /// Number of counted (`active`) accounts.
    async fn counted_accounts(&self) -> StoreResult<u64>;
    /// One transaction: lock the invite row and require `open`; take the cap
    /// advisory lock and reject at the cap; require issuer credit >= 1;
    /// match or create the Person and reject banned Persons; create the
    /// account; mark the invite redeemed; write ledger rows (issuer -1
    /// `redemption`, new account +`initial_credits` `initial_grant`).
    /// Nothing is written on rejection.
    async fn register(&self, tx: RegisterTx) -> StoreResult<Result<Registered, RegisterRejection>>;

    // ---- accounts ------------------------------------------------------
    /// Whether a username is taken by any account.
    async fn username_exists(&self, username: &str) -> StoreResult<bool>;
    /// Find by lowercase username or email.
    async fn find_by_login(&self, login: &str) -> StoreResult<Option<AccountRecord>>;
    /// Find by lowercase email.
    async fn find_by_email(&self, email: &str) -> StoreResult<Option<AccountRecord>>;
    /// Find by id.
    async fn find_by_id(&self, id: i64) -> StoreResult<Option<AccountRecord>>;
    /// Replace the password hash.
    async fn set_password_hash(&self, id: i64, hash: &str) -> StoreResult<()>;
    /// Mark the email verified (idempotent).
    async fn mark_email_verified(&self, id: i64) -> StoreResult<()>;
    /// Record the age-gate answer (doc 06 section 7).
    async fn set_age_confirmed(&self, id: i64, confirmed: bool, now: i64) -> StoreResult<()>;

    // ---- sessions (same table the `Principal` extractor reads) ---------
    /// Insert a session.
    async fn create_session(&self, rec: SessionRecord) -> StoreResult<()>;
    /// Live sessions of an account.
    async fn list_sessions(&self, user_id: i64, now: i64) -> StoreResult<Vec<SessionRecord>>;
    /// Revoke one session; false if it was not the user's.
    async fn revoke_session(&self, user_id: i64, session_id: i64) -> StoreResult<bool>;
    /// Revoke all sessions except `keep`.
    async fn revoke_sessions_except(&self, user_id: i64, keep: Option<i64>) -> StoreResult<u64>;

    // ---- TOTP ------------------------------------------------------------
    /// The account's TOTP record (secret decrypted).
    async fn totp_get(&self, user_id: i64) -> StoreResult<Option<TotpRecord>>;
    /// Insert/replace the pending (unconfirmed) secret.
    async fn totp_put_pending(&self, user_id: i64, secret: &[u8]) -> StoreResult<()>;
    /// Mark confirmed, recording the step.
    async fn totp_confirm(&self, user_id: i64, step: u64) -> StoreResult<()>;
    /// Atomically advance the last used step; false means replay.
    async fn totp_consume_step(&self, user_id: i64, step: u64) -> StoreResult<bool>;
    /// Remove TOTP.
    async fn totp_delete(&self, user_id: i64) -> StoreResult<()>;

    // ---- recovery codes --------------------------------------------------
    /// Replace all codes (empty slice removes them) in one transaction.
    async fn recovery_replace_all(&self, user_id: i64, hashes: &[String]) -> StoreResult<()>;
    /// Unused codes as `(row_id, phc_hash)`.
    async fn recovery_list_unused(&self, user_id: i64) -> StoreResult<Vec<(i64, String)>>;
    /// Atomically consume one code; false if already used.
    async fn recovery_consume(&self, user_id: i64, row_id: i64, now: i64) -> StoreResult<bool>;

    // ---- email tokens ----------------------------------------------------
    /// Store a token hash, superseding earlier unconsumed tokens of the same
    /// purpose for the account.
    async fn email_token_put(
        &self,
        user_id: i64,
        purpose: EmailTokenPurpose,
        hash: &TokenHash,
        expires_at: i64,
    ) -> StoreResult<()>;
    /// Atomically consume a valid token, returning its account.
    async fn email_token_consume(
        &self,
        purpose: EmailTokenPurpose,
        hash: &TokenHash,
        now: i64,
    ) -> StoreResult<Option<i64>>;
}
