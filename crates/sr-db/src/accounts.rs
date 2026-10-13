//! Account provisioning helpers shared by `sr-cli` (bootstrap, seed, recovery)
//! and tests. Registration proper (invite redemption, cap check) is a separate
//! service; these helpers deliberately skip invite rules because they are
//! operator tools.

use sqlx::PgPool;

use crate::DbResult;

/// Id of the system user "SIDE ROOM" (doc 06 section 8).
pub const SYSTEM_USER_ID: i64 = 1;

/// `accounts.flags` bit for the system user (Discord `system: true`; open question 44).
pub const FLAG_SYSTEM: i64 = 1 << 0;

/// Record that these accounts were seen now (gateway IDENTIFY/RESUME and its 10-minute
/// refresh; D-166). One statement for the whole batch.
pub async fn touch_last_seen(pool: &PgPool, ids: &[i64]) -> DbResult<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    let r = sqlx::query("UPDATE accounts SET last_seen_at = now() WHERE id = ANY($1)")
        .bind(ids)
        .execute(pool)
        .await?;
    Ok(r.rows_affected())
}

/// Fields for a new account (and its Person).
#[derive(Debug, Clone)]
pub struct NewAccount<'a> {
    pub id: i64,
    pub person_id: i64,
    pub username: &'a str,
    pub display_name: Option<&'a str>,
    pub email: &'a str,
    /// Argon2id PHC string (or an unusable marker for system accounts).
    pub password_hash: &'a str,
    /// One of the `account_state` enum labels.
    pub state: &'a str,
    pub email_verified: bool,
    /// `initial_grant` invite credits to record in the ledger (0 for none).
    pub initial_credits: i32,
    /// Bit set of `FLAG_*` values stored in `accounts.flags`.
    pub flags: i64,
}

/// Create the Person, account and initial credit grant in one transaction.
/// `ledger_id` is used for the credit row when `initial_credits > 0`.
/// Returns `false` (and writes nothing) if the username or email is taken.
pub async fn create_account(pool: &PgPool, a: &NewAccount<'_>, ledger_id: i64) -> DbResult<bool> {
    let mut tx = pool.begin().await?;
    let taken: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM accounts WHERE username = $1 OR email = $2 OR id = $3)",
    )
    .bind(a.username)
    .bind(a.email)
    .bind(a.id)
    .fetch_one(&mut *tx)
    .await?;
    if taken {
        return Ok(false);
    }
    sqlx::query("INSERT INTO persons (id) VALUES ($1) ON CONFLICT (id) DO NOTHING")
        .bind(a.person_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, display_name, email, email_verified_at, \
         password_hash, state, flags) \
         VALUES ($1, $2, $3, $4, $5, CASE WHEN $6 THEN now() END, $7, $8::account_state, $9)",
    )
    .bind(a.id)
    .bind(a.person_id)
    .bind(a.username)
    .bind(a.display_name)
    .bind(a.email)
    .bind(a.email_verified)
    .bind(a.password_hash)
    .bind(a.state)
    .bind(a.flags)
    .execute(&mut *tx)
    .await?;
    if a.initial_credits > 0 {
        sqlx::query(
            "INSERT INTO invite_credits_ledger (id, account_id, delta, reason) \
             VALUES ($1, $2, $3, 'initial_grant')",
        )
        .bind(ledger_id)
        .bind(a.id)
        .bind(a.initial_credits)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(true)
}

/// Mark an invite as an operator emergency invite: redeeming it neither requires nor
/// spends the issuer's credit (open question 46).
pub async fn mark_invite_emergency(pool: &PgPool, code: &str) -> DbResult<bool> {
    let done = sqlx::query("UPDATE platform_invites SET emergency = true WHERE code = $1")
        .bind(code)
        .execute(pool)
        .await?;
    Ok(done.rows_affected() == 1)
}

/// Find an account id by username or email (case-insensitive).
pub async fn find_account_id(pool: &PgPool, username_or_email: &str) -> DbResult<Option<i64>> {
    Ok(
        sqlx::query_scalar("SELECT id FROM accounts WHERE username = $1 OR email = $1 LIMIT 1")
            .bind(username_or_email)
            .fetch_optional(pool)
            .await?,
    )
}

/// Whether any account other than the system user exists (bootstrap guard).
pub async fn has_real_accounts(pool: &PgPool) -> DbResult<bool> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM accounts WHERE id <> $1)")
            .bind(SYSTEM_USER_ID)
            .fetch_one(pool)
            .await?,
    )
}

/// What [`reset_2fa`] removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Reset2fa {
    pub totp: u64,
    pub recovery_codes: u64,
    pub passkeys: u64,
    pub trusted_devices: u64,
    pub sessions_revoked: u64,
}

/// Remove all second factors for the account and revoke its sessions, in one
/// transaction. The password is untouched; the user logs in with it and
/// re-enrols (doc 07: MANAGE_ACCOUNTS "reset 2FA").
pub async fn reset_2fa(pool: &PgPool, account_id: i64) -> DbResult<Reset2fa> {
    let mut tx = pool.begin().await?;
    let out = reset_2fa_in(&mut tx, account_id).await?;
    tx.commit().await?;
    Ok(out)
}

/// The same inside the caller's transaction (admin actions record their audit row in it).
pub async fn reset_2fa_in(tx: &mut sqlx::PgConnection, account_id: i64) -> DbResult<Reset2fa> {
    let totp = sqlx::query("DELETE FROM mfa_totp WHERE account_id = $1")
        .bind(account_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    let recovery_codes = sqlx::query("DELETE FROM recovery_codes WHERE account_id = $1")
        .bind(account_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    let passkeys = sqlx::query("DELETE FROM passkey_credentials WHERE account_id = $1")
        .bind(account_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    let trusted_devices = sqlx::query(
        "UPDATE trusted_devices SET revoked_at = now(), updated_at = now() \
         WHERE account_id = $1 AND revoked_at IS NULL",
    )
    .bind(account_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    let sessions_revoked = sqlx::query(
        "UPDATE sessions SET revoked_at = now(), updated_at = now() \
         WHERE account_id = $1 AND revoked_at IS NULL",
    )
    .bind(account_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    Ok(Reset2fa {
        totp,
        recovery_codes,
        passkeys,
        trusted_devices,
        sessions_revoked,
    })
}

/// The oldest account other than the system user (the Owner after bootstrap).
pub async fn oldest_real_account(pool: &PgPool) -> DbResult<Option<i64>> {
    Ok(
        sqlx::query_scalar("SELECT id FROM accounts WHERE id <> $1 ORDER BY id LIMIT 1")
            .bind(SYSTEM_USER_ID)
            .fetch_optional(pool)
            .await?,
    )
}
