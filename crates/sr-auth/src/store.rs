//! Persistence boundary for the Entrance/auth endpoints.
//!
//! [`AuthStore`] is a dyn-safe facade over the repository traits in
//! [`crate::repo`] (those use `impl Future`, so they are not object safe) and
//! the account and registration SQL. `sr-db` implements it (`PgAuthStore`);
//! `sr-api` keeps an in-memory implementation for tests.

use crate::{
    email_token::EmailTokenPurpose,
    invite::InviteCode,
    repo::{SessionRecord, TotpRecord},
    session::TokenHash,
};
use async_trait::async_trait;

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
    /// Person the account belongs to (D-030).
    pub person_id: i64,
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
    /// When the age gate was answered "18 or older" (Unix seconds); `None` otherwise.
    pub age_confirmed_at: Option<i64>,
    /// BCP 47 locale (`en-US` by default).
    pub locale: String,
    /// Public account flags (Discord `flags`; 0 for none).
    pub flags: i64,
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
    /// Discord user id proven by OAuth in this flow, if any (ENT-AUTH-011).
    pub discord_id: Option<String>,
    /// Row id for the Discord identity link, if `discord_id` is set.
    pub identity_id: i64,
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

/// A trusted device ("Trust this device for 30 days", open question 144).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedDeviceRecord {
    /// Snowflake id of the trusted-device row.
    pub id: i64,
    /// Device label from the user agent.
    pub device_name: Option<String>,
    /// When the device was trusted.
    pub created_at: i64,
    /// When trust lapses.
    pub expires_at: i64,
}

/// A registered passkey (WebAuthn credential), ENT-AUTH-010.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasskeyRecord {
    /// Snowflake id of the row (what the settings UI addresses).
    pub id: i64,
    /// Owning account.
    pub account_id: i64,
    /// Authenticator-chosen credential id.
    pub credential_id: Vec<u8>,
    /// COSE_Key bytes as produced by the authenticator.
    pub cose_key: Vec<u8>,
    /// COSE algorithm id (-7, -8 or -257).
    pub alg: i16,
    /// Last accepted signature counter.
    pub sign_count: i64,
    /// Backup-eligible flag (synced passkey).
    pub backup_eligible: bool,
    /// Current backup state.
    pub backup_state: bool,
    /// Authenticator AAGUID (informational).
    pub aaguid: Vec<u8>,
    /// User-visible label (1 to 64 characters).
    pub name: String,
    /// Unix seconds.
    pub created_at: i64,
    /// Unix seconds of the last successful use.
    pub last_used_at: Option<i64>,
}

/// What a completed account deletion removed (LNG-USET-007).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeletedAccount {
    /// Servers the account left.
    pub guilds_left: Vec<i64>,
    /// Friends whose relationship rows were removed.
    pub friends: Vec<i64>,
    /// Other users with a pending or blocked relationship that was removed.
    pub other_relations: Vec<i64>,
    /// Invite credits forfeited.
    pub credits_forfeited: i32,
    /// Open platform invites revoked.
    pub invites_revoked: i32,
    /// Avatar content hash before the scrub (media to delete).
    pub avatar_key: Option<String>,
    /// Banner content hash before the scrub.
    pub banner_key: Option<String>,
}

/// Why an account could not be deleted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeleteRefusal {
    /// Unknown or not active.
    NotActive,
    /// The account owns these servers (ids); transfer or delete them first.
    OwnsGuilds(Vec<i64>),
    /// An appointment is active; leave the Staff Server first (D-194).
    Staff,
    /// The account is the platform Owner; transfer ownership first (D-194).
    Owner,
}

/// Why an account cannot create another platform invite right now (doc 06 section 5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InviteBlock {
    /// The email address is not verified yet.
    Unverified,
    /// The account is younger than `invites.min_account_age_days`.
    TooNew { days: i64 },
    /// The account cap is reached (D-014): creating and redeeming are blocked.
    AtCap,
    /// Open codes already equal the remaining credits (D-026).
    NoCredits,
    /// The suspicion freeze applies (doc 06 section 5.4). Shown with generic wording.
    Frozen,
}

/// What `note_login_device` found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceSighting {
    /// The account has no earlier device on record: this is its first sign-in.
    First,
    /// Seen before.
    Known,
    /// A device the account has not signed in from before.
    New,
}

/// One platform invite as the issuer sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberInviteRow {
    /// Canonical code.
    pub code: String,
    /// The issuer's private note.
    pub note: String,
    /// Unix seconds.
    pub created_at: i64,
    /// Who redeemed it (id, username) and when (unix seconds); redeemed rows only.
    pub redeemed_by: Option<(i64, String)>,
    pub redeemed_at: Option<i64>,
}

/// What the Invites section of User Settings shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberInvites {
    /// Remaining credits (ledger balance).
    pub balance: i64,
    /// Open codes, newest first.
    pub open: Vec<MemberInviteRow>,
    /// Redeemed codes, newest first.
    pub redeemed: Vec<MemberInviteRow>,
    /// `None` when the account may create a code now.
    pub blocked: Option<InviteBlock>,
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
    /// The account cap set by staff in `platform_settings` (`accounts.cap`), if any. It wins
    /// over the configured cap so Admin -> Limits applies without a restart.
    async fn account_cap_override(&self) -> StoreResult<Option<u64>> {
        Ok(None)
    }
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
    /// The account linked to a Discord user id (open question 79).
    async fn find_by_discord(&self, provider_user_id: &str) -> StoreResult<Option<AccountRecord>>;
    /// Replace the password hash.
    async fn set_password_hash(&self, id: i64, hash: &str) -> StoreResult<()>;
    /// Mark the email verified (idempotent).
    async fn mark_email_verified(&self, id: i64) -> StoreResult<()>;
    /// Record the age-gate answer (doc 06 section 7).
    async fn set_age_confirmed(&self, id: i64, confirmed: bool, now: i64) -> StoreResult<()>;
    /// Forget the age-gate answer (asked again later); `age_confirmed` and its time go back to unset.
    async fn reset_age_confirmed(&self, id: i64) -> StoreResult<()>;
    /// Set the account's BCP 47 locale (already normalised); mails, push text and system DMs follow it.
    async fn set_locale(&self, id: i64, locale: &str) -> StoreResult<()>;

    // ---- account identity edits (display name, username, email) --------
    /// Set (or clear) the display name.
    async fn set_display_name(&self, id: i64, display_name: Option<&str>) -> StoreResult<()>;
    /// Change the username (already validated and lowercased). `false` when
    /// another account has it.
    async fn set_username(&self, id: i64, username: &str) -> StoreResult<bool>;
    /// Store a pending address change: a token whose row remembers `new_email`.
    /// Supersedes the account's earlier unconsumed change tokens.
    async fn email_change_put(
        &self,
        user_id: i64,
        new_email: &str,
        hash: &TokenHash,
        expires_at: i64,
    ) -> StoreResult<()>;
    /// Atomically consume a valid change token, returning `(account, new_email)`.
    async fn email_change_consume(
        &self,
        hash: &TokenHash,
        now: i64,
    ) -> StoreResult<Option<(i64, String)>>;
    /// Drop the account's pending (unconsumed) address change. `true` when there was one.
    async fn email_change_cancel(&self, user_id: i64) -> StoreResult<bool>;
    /// Replace the account's email and mark it verified (the owner just proved
    /// control of it). `false` when another account already has the address.
    async fn set_email(&self, id: i64, email: &str) -> StoreResult<bool>;

    // ---- sessions (same table the `Principal` extractor reads) ---------
    /// Insert a session.
    async fn create_session(&self, rec: SessionRecord) -> StoreResult<()>;
    /// Live sessions of an account.
    async fn list_sessions(&self, user_id: i64, now: i64) -> StoreResult<Vec<SessionRecord>>;
    /// Revoke one session; false if it was not the user's.
    async fn revoke_session(&self, user_id: i64, session_id: i64) -> StoreResult<bool>;
    /// When this session last passed a second-factor check (login 2FA or the
    /// email-change re-prompt), as Unix seconds. `None` when it never did.
    async fn session_mfa_verified_at(&self, session_id: i64) -> StoreResult<Option<i64>>;
    /// Record a second-factor check on this session.
    async fn session_mark_mfa_verified(&self, session_id: i64, at: i64) -> StoreResult<()>;
    /// Revoke all sessions except `keep`.
    async fn revoke_sessions_except(&self, user_id: i64, keep: Option<i64>) -> StoreResult<u64>;

    // ---- trusted devices (open question 144) ----------------------------
    /// Trust a device: store the digest of its cookie token.
    async fn trusted_device_put(
        &self,
        user_id: i64,
        hash: &TokenHash,
        device_name: Option<&str>,
        expires_at: i64,
    ) -> StoreResult<()>;
    /// Whether `hash` is an unexpired, unrevoked trusted device of `user_id`.
    async fn trusted_device_matches(
        &self,
        user_id: i64,
        hash: &TokenHash,
        now: i64,
    ) -> StoreResult<bool>;
    /// The account's live trusted devices.
    async fn trusted_devices_list(
        &self,
        user_id: i64,
        now: i64,
    ) -> StoreResult<Vec<TrustedDeviceRecord>>;
    /// Revoke one trusted device; false if it was not the user's.
    async fn trusted_device_revoke(&self, user_id: i64, id: i64) -> StoreResult<bool>;
    /// Revoke every trusted device of the account (2FA reset, password reset).
    async fn trusted_devices_revoke_all(&self, user_id: i64) -> StoreResult<u64>;

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
    /// Replace all digests (empty slice removes them) in one transaction.
    async fn recovery_replace_all(&self, user_id: i64, hashes: &[TokenHash]) -> StoreResult<()>;
    /// Unused codes as `(row_id, digest)`.
    async fn recovery_list_unused(&self, user_id: i64) -> StoreResult<Vec<(i64, TokenHash)>>;
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

    // ---- passkeys (ENT-AUTH-010) -------------------------------------------
    /// The account's WebAuthn user handle. Stores `candidate` (32 random bytes)
    /// when the account has none yet; always returns the stored value.
    async fn passkey_user_handle(&self, account_id: i64, candidate: &[u8]) -> StoreResult<Vec<u8>>;
    /// Insert a credential. `false` when the credential id is already registered.
    async fn passkey_insert(&self, rec: PasskeyRecord) -> StoreResult<bool>;
    /// The account's passkeys, oldest first.
    async fn passkey_list(&self, account_id: i64) -> StoreResult<Vec<PasskeyRecord>>;
    /// Find by credential id (any account).
    async fn passkey_find(&self, credential_id: &[u8]) -> StoreResult<Option<PasskeyRecord>>;
    /// Rename; `false` when the passkey is not the account's.
    async fn passkey_rename(&self, account_id: i64, id: i64, name: &str) -> StoreResult<bool>;
    /// Delete; `false` when the passkey is not the account's.
    async fn passkey_delete(&self, account_id: i64, id: i64) -> StoreResult<bool>;
    /// After a verified assertion: compare-and-set the counter. Only applies while the
    /// stored counter still equals `expected_sign_count`, so two concurrent uses of the
    /// same assertion cannot both succeed. Returns whether it applied.
    async fn passkey_record_use(
        &self,
        id: i64,
        expected_sign_count: i64,
        sign_count: i64,
        backup_state: bool,
        now: i64,
    ) -> StoreResult<bool>;

    // ---- new-device sign-in alerts (ENT-AUTH-015) ------------------------------
    /// Record a sign-in from the device labelled `name` (`key` is its normalised form) and say whether the
    /// account has seen that device before. The first device of an account is [`DeviceSighting::First`].
    async fn note_login_device(
        &self,
        _account: i64,
        _key: &str,
        _name: &str,
        _now: i64,
    ) -> StoreResult<DeviceSighting> {
        Ok(DeviceSighting::Known)
    }
    /// Whether the account wants a mail when a new device signs in (default true).
    async fn login_alerts_enabled(&self, _account: i64) -> StoreResult<bool> {
        Ok(true)
    }
    /// Turn new-device mails on or off.
    async fn set_login_alerts(&self, _account: i64, _enabled: bool) -> StoreResult<()> {
        Err(StoreError(
            "login alerts are not supported by this store".into(),
        ))
    }

    // ---- member platform invites (G-57) ---------------------------------------
    /// The account's remaining invite credits (ledger balance), for the
    /// `SR_INVITE_CREDITS_UPDATE` dispatch (ENT-INV-009). `None` when the store cannot say.
    async fn invite_credit_balance(&self, _account: i64) -> StoreResult<Option<i64>> {
        Ok(None)
    }
    /// The account's credits, codes and whether it may create one now.
    async fn member_invites(&self, _account: i64, _now: i64) -> StoreResult<MemberInvites> {
        Err(StoreError(
            "member invites are not supported by this store".into(),
        ))
    }
    /// Create an open code after re-checking every rule in one transaction.
    async fn create_member_invite(
        &self,
        _account: i64,
        _invite_id: i64,
        _code: &InviteCode,
        _note: &str,
        _now: i64,
    ) -> StoreResult<Result<(), InviteBlock>> {
        Err(StoreError(
            "member invites are not supported by this store".into(),
        ))
    }
    /// Revoke an open code the account issued; `false` when it is not open or not theirs.
    async fn revoke_member_invite(&self, _account: i64, _code: &InviteCode) -> StoreResult<bool> {
        Err(StoreError(
            "member invites are not supported by this store".into(),
        ))
    }

    // ---- account deletion (D-029) -------------------------------------------
    /// Delete (scrub) the account in one transaction: see `sr_db::account_deletion`.
    /// `ledger_id` is the id for the credit-forfeit ledger row.
    async fn delete_account(
        &self,
        id: i64,
        ledger_id: i64,
    ) -> StoreResult<Result<DeletedAccount, DeleteRefusal>>;

    // ---- platform audit (doc 04 section 7) --------------------------------
    /// Append a platform-scoped audit row.
    async fn audit(
        &self,
        actor_id: i64,
        action: &str,
        target_id: Option<i64>,
        note: &str,
        now: i64,
    ) -> StoreResult<()>;
}
