//! Postgres implementation of the Entrance [`AuthStore`] (open question 141).
//!
//! Sessions, TOTP, recovery codes and email tokens delegate to
//! [`PgAuthRepo`]. Accounts, invites, registration, trusted devices and the
//! platform audit log are SQL here. Times cross the boundary as Unix seconds.

use std::fmt::Display;

use async_trait::async_trait;
use sqlx::{PgPool, Row};
use sr_auth::{
    email_token::EmailTokenPurpose,
    invite::InviteCode,
    repo::{
        EmailTokenRepository, RecoveryCodeRepository, SessionRecord, SessionRepository, TotpRecord,
        TotpRepository,
    },
    session::TokenHash,
    store::{
        AccountRecord, AccountState, AuthStore, DeleteRefusal, DeletedAccount, InviteState,
        PasskeyRecord, RegisterRejection, RegisterTx, Registered, StoreError, StoreResult,
        TrustedDeviceRecord,
    },
};

use crate::{
    auth_repo::{IdSource, PgAuthRepo},
    registration::{self, NewRegistration, Rejection},
};

fn store_err(e: impl Display) -> StoreError {
    StoreError(e.to_string())
}

/// Postgres-backed [`AuthStore`].
#[derive(Clone)]
pub struct PgAuthStore {
    pool: PgPool,
    repo: PgAuthRepo,
    ids: IdSource,
}

impl std::fmt::Debug for PgAuthStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PgAuthStore").finish_non_exhaustive()
    }
}

impl PgAuthStore {
    /// `repo` must share `pool` (it does the session, TOTP and token work).
    pub fn new(pool: PgPool, repo: PgAuthRepo, ids: IdSource) -> Self {
        Self { pool, repo, ids }
    }

    fn next_id(&self) -> i64 {
        (self.ids)()
    }
}

/// `true` when a write changed a row, `false` on a unique violation (the value
/// is taken), an error otherwise.
fn unique_to_false(r: Result<sqlx::postgres::PgQueryResult, sqlx::Error>) -> StoreResult<bool> {
    match r {
        Ok(done) => Ok(done.rows_affected() > 0),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Ok(false),
        Err(e) => Err(store_err(e)),
    }
}
const ACCOUNT_COLS: &str = "id, person_id, username::text AS username, display_name, \
    email::text AS email, email_verified_at IS NOT NULL AS email_verified, password_hash, \
    state::text AS state, age_confirmed, locale, flags";

fn account_from_row(r: &sqlx::postgres::PgRow) -> StoreResult<AccountRecord> {
    let state: String = r.try_get("state").map_err(store_err)?;
    Ok(AccountRecord {
        id: r.try_get("id").map_err(store_err)?,
        person_id: r.try_get("person_id").map_err(store_err)?,
        username: r.try_get("username").map_err(store_err)?,
        display_name: r.try_get("display_name").map_err(store_err)?,
        email: r.try_get("email").map_err(store_err)?,
        email_verified: r.try_get("email_verified").map_err(store_err)?,
        password_hash: r.try_get("password_hash").map_err(store_err)?,
        state: match state.as_str() {
            "active" => AccountState::Active,
            "deactivated" => AccountState::Deactivated,
            "deleted" => AccountState::Deleted,
            "platform_banned" => AccountState::PlatformBanned,
            other => return Err(StoreError(format!("unknown account state {other}"))),
        },
        age_confirmed: r.try_get("age_confirmed").map_err(store_err)?,
        locale: r.try_get("locale").map_err(store_err)?,
        flags: r.try_get("flags").map_err(store_err)?,
    })
}

macro_rules! passkey_select {
    ($tail:literal) => {
        concat!(
            "SELECT id, account_id, credential_id, cose_key, alg, sign_count, ",
            "backup_eligible, backup_state, aaguid, name, ",
            "EXTRACT(EPOCH FROM created_at)::BIGINT AS created_at, ",
            "EXTRACT(EPOCH FROM last_used_at)::BIGINT AS last_used_at ",
            $tail
        )
    };
}

fn passkey_from_row(r: &sqlx::postgres::PgRow) -> StoreResult<PasskeyRecord> {
    let aaguid: Option<Vec<u8>> = r.try_get("aaguid").map_err(store_err)?;
    Ok(PasskeyRecord {
        id: r.try_get("id").map_err(store_err)?,
        account_id: r.try_get("account_id").map_err(store_err)?,
        credential_id: r.try_get("credential_id").map_err(store_err)?,
        cose_key: r.try_get("cose_key").map_err(store_err)?,
        alg: r.try_get("alg").map_err(store_err)?,
        sign_count: r.try_get("sign_count").map_err(store_err)?,
        backup_eligible: r.try_get("backup_eligible").map_err(store_err)?,
        backup_state: r.try_get("backup_state").map_err(store_err)?,
        aaguid: aaguid.unwrap_or_default(),
        name: r.try_get("name").map_err(store_err)?,
        created_at: r.try_get("created_at").map_err(store_err)?,
        last_used_at: r.try_get("last_used_at").map_err(store_err)?,
    })
}

#[async_trait]
impl AuthStore for PgAuthStore {
    async fn invite_state(&self, code: &InviteCode) -> StoreResult<Option<InviteState>> {
        let state: Option<String> =
            sqlx::query_scalar("SELECT state::text FROM platform_invites WHERE code = $1")
                .bind(code.canonical())
                .fetch_optional(&self.pool)
                .await
                .map_err(store_err)?;
        Ok(match state.as_deref() {
            None => None,
            Some("open") => Some(InviteState::Open),
            Some("redeemed") => Some(InviteState::Redeemed),
            Some("revoked") => Some(InviteState::Revoked),
            Some(other) => return Err(StoreError(format!("unknown invite state {other}"))),
        })
    }

    async fn account_cap_override(&self) -> StoreResult<Option<u64>> {
        let v: Option<i64> = sqlx::query_scalar(
            "SELECT floor((value #>> '{}')::numeric)::bigint FROM platform_settings              WHERE key = 'accounts.cap' AND jsonb_typeof(value) = 'number'",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(store_err)?;
        Ok(v.and_then(|v| u64::try_from(v).ok()).filter(|v| *v > 0))
    }

    async fn counted_accounts(&self) -> StoreResult<u64> {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM accounts WHERE state = 'active' AND NOT is_bot",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(store_err)?;
        Ok(u64::try_from(n).unwrap_or(0))
    }

    async fn register(&self, tx: RegisterTx) -> StoreResult<Result<Registered, RegisterRejection>> {
        let outcome = registration::register_with_invite(
            &self.pool,
            &NewRegistration {
                account_id: tx.account_id,
                person_id: tx.new_person_id,
                ledger_ids: tx.ledger_ids,
                code: tx.code.canonical().to_owned(),
                username: tx.username,
                display_name: tx.display_name,
                email: tx.email,
                password_hash: tx.password_hash,
                discord_id: tx.discord_id,
                identity_id: tx.identity_id,
                account_cap: i64::try_from(tx.account_cap).unwrap_or(i64::MAX),
                initial_credits: tx.initial_credits,
            },
        )
        .await
        .map_err(store_err)?;
        Ok(match outcome {
            Ok(r) => Ok(Registered {
                account_id: r.account_id,
                issuer_id: r.issuer_id,
            }),
            Err(rej) => Err(match rej {
                Rejection::InviteUnavailable => RegisterRejection::InviteUnavailable,
                Rejection::AtCap => RegisterRejection::AtCap,
                Rejection::IssuerOutOfCredits => RegisterRejection::IssuerOutOfCredits,
                Rejection::PersonBanned => RegisterRejection::PersonBanned,
                Rejection::UsernameTaken => RegisterRejection::UsernameTaken,
                Rejection::EmailTaken => RegisterRejection::EmailTaken,
            }),
        })
    }

    async fn username_exists(&self, username: &str) -> StoreResult<bool> {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM accounts WHERE username = $1) \n             OR EXISTS(SELECT 1 FROM deleted_accounts WHERE username = $1 AND username_release_at > now())",
        )
            .bind(username)
            .fetch_one(&self.pool)
            .await
            .map_err(store_err)
    }

    async fn find_by_login(&self, login: &str) -> StoreResult<Option<AccountRecord>> {
        let sql = format!(
            "SELECT {ACCOUNT_COLS} FROM accounts WHERE username = $1 OR email = $1 LIMIT 1"
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(login)
            .fetch_optional(&self.pool)
            .await
            .map_err(store_err)?;
        row.as_ref().map(account_from_row).transpose()
    }

    async fn find_by_email(&self, email: &str) -> StoreResult<Option<AccountRecord>> {
        let sql = format!("SELECT {ACCOUNT_COLS} FROM accounts WHERE email = $1 LIMIT 1");
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(email)
            .fetch_optional(&self.pool)
            .await
            .map_err(store_err)?;
        row.as_ref().map(account_from_row).transpose()
    }

    async fn find_by_id(&self, id: i64) -> StoreResult<Option<AccountRecord>> {
        let sql = format!("SELECT {ACCOUNT_COLS} FROM accounts WHERE id = $1");
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(store_err)?;
        row.as_ref().map(account_from_row).transpose()
    }

    async fn find_by_discord(&self, provider_user_id: &str) -> StoreResult<Option<AccountRecord>> {
        let sql = format!(
            "SELECT {ACCOUNT_COLS} FROM accounts WHERE id = (SELECT account_id FROM \
             external_identities WHERE provider = 'discord' AND provider_user_id = $1)"
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(provider_user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(store_err)?;
        row.as_ref().map(account_from_row).transpose()
    }

    async fn set_password_hash(&self, id: i64, hash: &str) -> StoreResult<()> {
        sqlx::query("UPDATE accounts SET password_hash = $2, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(hash)
            .execute(&self.pool)
            .await
            .map_err(store_err)?;
        Ok(())
    }

    async fn mark_email_verified(&self, id: i64) -> StoreResult<()> {
        sqlx::query(
            "UPDATE accounts SET email_verified_at = COALESCE(email_verified_at, now()), \
             updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(store_err)?;
        Ok(())
    }

    async fn set_age_confirmed(&self, id: i64, confirmed: bool, now: i64) -> StoreResult<()> {
        sqlx::query(
            "UPDATE accounts SET age_confirmed = $2, \
             age_confirmed_at = CASE WHEN $2 THEN COALESCE(age_confirmed_at, to_timestamp($3::float8)) \
             ELSE NULL END, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(confirmed)
        .bind(now as f64)
        .execute(&self.pool)
        .await
        .map_err(store_err)?;
        Ok(())
    }

    async fn set_locale(&self, id: i64, locale: &str) -> StoreResult<()> {
        sqlx::query("UPDATE accounts SET locale = $2, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(locale)
            .execute(&self.pool)
            .await
            .map_err(store_err)?;
        Ok(())
    }

    async fn set_display_name(&self, id: i64, display_name: Option<&str>) -> StoreResult<()> {
        sqlx::query("UPDATE accounts SET display_name = $2, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(display_name)
            .execute(&self.pool)
            .await
            .map_err(store_err)?;
        Ok(())
    }

    async fn set_username(&self, id: i64, username: &str) -> StoreResult<bool> {
        let r = sqlx::query("UPDATE accounts SET username = $2, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(username)
            .execute(&self.pool)
            .await;
        unique_to_false(r)
    }

    async fn email_change_put(
        &self,
        user_id: i64,
        new_email: &str,
        hash: &TokenHash,
        expires_at: i64,
    ) -> StoreResult<()> {
        let mut tx = self.pool.begin().await.map_err(store_err)?;
        sqlx::query(
            "DELETE FROM email_tokens WHERE account_id = $1 \
             AND purpose = 'change_email'::email_token_purpose AND used_at IS NULL",
        )
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(store_err)?;
        sqlx::query(
            "INSERT INTO email_tokens (id, account_id, purpose, email, token_hash, expires_at) \
             VALUES ($1, $2, 'change_email'::email_token_purpose, $3, $4, to_timestamp($5::float8))",
        )
        .bind(self.next_id())
        .bind(user_id)
        .bind(new_email)
        .bind(hash.as_bytes().as_slice())
        .bind(expires_at as f64)
        .execute(&mut *tx)
        .await
        .map_err(store_err)?;
        tx.commit().await.map_err(store_err)
    }

    async fn email_change_consume(
        &self,
        hash: &TokenHash,
        now: i64,
    ) -> StoreResult<Option<(i64, String)>> {
        let row = sqlx::query(
            "UPDATE email_tokens SET used_at = to_timestamp($2::float8), updated_at = now() \
             WHERE token_hash = $1 AND purpose = 'change_email'::email_token_purpose \
             AND used_at IS NULL AND expires_at > to_timestamp($2::float8) \
             RETURNING account_id, email::text AS email",
        )
        .bind(hash.as_bytes().as_slice())
        .bind(now as f64)
        .fetch_optional(&self.pool)
        .await
        .map_err(store_err)?;
        row.map(|r| {
            Ok((
                r.try_get("account_id").map_err(store_err)?,
                r.try_get("email").map_err(store_err)?,
            ))
        })
        .transpose()
    }

    async fn email_change_cancel(&self, user_id: i64) -> StoreResult<bool> {
        let r = sqlx::query(
            "DELETE FROM email_tokens WHERE account_id = $1              AND purpose = 'change_email'::email_token_purpose AND used_at IS NULL",
        )
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(store_err)?;
        Ok(r.rows_affected() > 0)
    }

    async fn session_mfa_verified_at(&self, session_id: i64) -> StoreResult<Option<i64>> {
        sqlx::query_scalar(
            "SELECT EXTRACT(EPOCH FROM mfa_verified_at)::BIGINT FROM sessions              WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await
        .map(Option::flatten)
        .map_err(store_err)
    }

    async fn session_mark_mfa_verified(&self, session_id: i64, at: i64) -> StoreResult<()> {
        sqlx::query("UPDATE sessions SET mfa_verified_at = to_timestamp($2::float8) WHERE id = $1")
            .bind(session_id)
            .bind(at as f64)
            .execute(&self.pool)
            .await
            .map_err(store_err)?;
        Ok(())
    }

    async fn set_email(&self, id: i64, email: &str) -> StoreResult<bool> {
        let r = sqlx::query(
            "UPDATE accounts SET email = $2, email_verified_at = now(), updated_at = now() \
             WHERE id = $1",
        )
        .bind(id)
        .bind(email)
        .execute(&self.pool)
        .await;
        unique_to_false(r)
    }
    async fn create_session(&self, rec: SessionRecord) -> StoreResult<()> {
        SessionRepository::create(&self.repo, rec)
            .await
            .map_err(store_err)
    }

    async fn list_sessions(&self, user_id: i64, now: i64) -> StoreResult<Vec<SessionRecord>> {
        SessionRepository::list_for_user(&self.repo, user_id, now)
            .await
            .map_err(store_err)
    }

    async fn revoke_session(&self, user_id: i64, session_id: i64) -> StoreResult<bool> {
        SessionRepository::revoke(&self.repo, user_id, session_id)
            .await
            .map_err(store_err)
    }

    async fn revoke_sessions_except(&self, user_id: i64, keep: Option<i64>) -> StoreResult<u64> {
        SessionRepository::revoke_all_except(&self.repo, user_id, keep)
            .await
            .map_err(store_err)
    }

    async fn trusted_device_put(
        &self,
        user_id: i64,
        hash: &TokenHash,
        device_name: Option<&str>,
        expires_at: i64,
    ) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO trusted_devices (id, account_id, token_hash, device_name, expires_at) \
             VALUES ($1, $2, $3, $4, to_timestamp($5::float8))",
        )
        .bind(self.next_id())
        .bind(user_id)
        .bind(hash.as_bytes().as_slice())
        .bind(device_name)
        .bind(expires_at as f64)
        .execute(&self.pool)
        .await
        .map_err(store_err)?;
        Ok(())
    }

    async fn trusted_device_matches(
        &self,
        user_id: i64,
        hash: &TokenHash,
        now: i64,
    ) -> StoreResult<bool> {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM trusted_devices WHERE account_id = $1 \
             AND token_hash = $2 AND revoked_at IS NULL AND expires_at > to_timestamp($3::float8))",
        )
        .bind(user_id)
        .bind(hash.as_bytes().as_slice())
        .bind(now as f64)
        .fetch_one(&self.pool)
        .await
        .map_err(store_err)
    }

    async fn trusted_devices_list(
        &self,
        user_id: i64,
        now: i64,
    ) -> StoreResult<Vec<TrustedDeviceRecord>> {
        let rows = sqlx::query(
            "SELECT id, device_name, EXTRACT(EPOCH FROM created_at)::BIGINT AS created_at, \
             EXTRACT(EPOCH FROM expires_at)::BIGINT AS expires_at FROM trusted_devices \
             WHERE account_id = $1 AND revoked_at IS NULL AND expires_at > to_timestamp($2::float8) \
             ORDER BY created_at DESC",
        )
        .bind(user_id)
        .bind(now as f64)
        .fetch_all(&self.pool)
        .await
        .map_err(store_err)?;
        rows.iter()
            .map(|r| {
                Ok(TrustedDeviceRecord {
                    id: r.try_get("id").map_err(store_err)?,
                    device_name: r.try_get("device_name").map_err(store_err)?,
                    created_at: r.try_get("created_at").map_err(store_err)?,
                    expires_at: r.try_get("expires_at").map_err(store_err)?,
                })
            })
            .collect()
    }

    async fn trusted_device_revoke(&self, user_id: i64, id: i64) -> StoreResult<bool> {
        let r = sqlx::query(
            "UPDATE trusted_devices SET revoked_at = now(), updated_at = now() \
             WHERE id = $1 AND account_id = $2 AND revoked_at IS NULL",
        )
        .bind(id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(store_err)?;
        Ok(r.rows_affected() > 0)
    }

    async fn trusted_devices_revoke_all(&self, user_id: i64) -> StoreResult<u64> {
        let r = sqlx::query(
            "UPDATE trusted_devices SET revoked_at = now(), updated_at = now() \
             WHERE account_id = $1 AND revoked_at IS NULL",
        )
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(store_err)?;
        Ok(r.rows_affected())
    }

    async fn totp_get(&self, user_id: i64) -> StoreResult<Option<TotpRecord>> {
        TotpRepository::get(&self.repo, user_id)
            .await
            .map_err(store_err)
    }

    async fn totp_put_pending(&self, user_id: i64, secret: &[u8]) -> StoreResult<()> {
        TotpRepository::put_pending(&self.repo, user_id, secret)
            .await
            .map_err(store_err)
    }

    async fn totp_confirm(&self, user_id: i64, step: u64) -> StoreResult<()> {
        TotpRepository::confirm(&self.repo, user_id, step)
            .await
            .map_err(store_err)
    }

    async fn totp_consume_step(&self, user_id: i64, step: u64) -> StoreResult<bool> {
        TotpRepository::consume_step(&self.repo, user_id, step)
            .await
            .map_err(store_err)
    }

    async fn totp_delete(&self, user_id: i64) -> StoreResult<()> {
        TotpRepository::delete(&self.repo, user_id)
            .await
            .map_err(store_err)
    }

    async fn recovery_replace_all(&self, user_id: i64, hashes: &[TokenHash]) -> StoreResult<()> {
        RecoveryCodeRepository::replace_all(&self.repo, user_id, hashes)
            .await
            .map_err(store_err)
    }

    async fn recovery_list_unused(&self, user_id: i64) -> StoreResult<Vec<(i64, TokenHash)>> {
        RecoveryCodeRepository::list_unused(&self.repo, user_id)
            .await
            .map_err(store_err)
    }

    async fn recovery_consume(&self, user_id: i64, row_id: i64, now: i64) -> StoreResult<bool> {
        RecoveryCodeRepository::consume(&self.repo, user_id, row_id, now)
            .await
            .map_err(store_err)
    }

    async fn email_token_put(
        &self,
        user_id: i64,
        purpose: EmailTokenPurpose,
        hash: &TokenHash,
        expires_at: i64,
    ) -> StoreResult<()> {
        EmailTokenRepository::put(&self.repo, user_id, purpose, hash, expires_at)
            .await
            .map_err(store_err)
    }

    async fn email_token_consume(
        &self,
        purpose: EmailTokenPurpose,
        hash: &TokenHash,
        now: i64,
    ) -> StoreResult<Option<i64>> {
        EmailTokenRepository::consume(&self.repo, purpose, hash, now)
            .await
            .map_err(store_err)
    }

    async fn passkey_user_handle(&self, account_id: i64, candidate: &[u8]) -> StoreResult<Vec<u8>> {
        sqlx::query_scalar(
            "UPDATE accounts SET user_handle = COALESCE(user_handle, $2) WHERE id = $1 \
             RETURNING user_handle",
        )
        .bind(account_id)
        .bind(candidate)
        .fetch_one(&self.pool)
        .await
        .map_err(store_err)
    }

    async fn passkey_insert(&self, rec: PasskeyRecord) -> StoreResult<bool> {
        unique_to_false(
            sqlx::query(
                "INSERT INTO passkey_credentials (id, account_id, credential_id, cose_key, alg, \
                 sign_count, backup_eligible, backup_state, aaguid, name, created_at) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, to_timestamp($11::float8))",
            )
            .bind(rec.id)
            .bind(rec.account_id)
            .bind(&rec.credential_id)
            .bind(&rec.cose_key)
            .bind(rec.alg)
            .bind(rec.sign_count)
            .bind(rec.backup_eligible)
            .bind(rec.backup_state)
            .bind(&rec.aaguid)
            .bind(&rec.name)
            .bind(rec.created_at as f64)
            .execute(&self.pool)
            .await,
        )
    }

    async fn passkey_list(&self, account_id: i64) -> StoreResult<Vec<PasskeyRecord>> {
        let rows = sqlx::query(passkey_select!(
            "FROM passkey_credentials WHERE account_id = $1 ORDER BY created_at, id"
        ))
        .bind(account_id)
        .fetch_all(&self.pool)
        .await
        .map_err(store_err)?;
        rows.iter().map(passkey_from_row).collect()
    }

    async fn passkey_find(&self, credential_id: &[u8]) -> StoreResult<Option<PasskeyRecord>> {
        let row = sqlx::query(passkey_select!(
            "FROM passkey_credentials WHERE credential_id = $1"
        ))
        .bind(credential_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(store_err)?;
        row.as_ref().map(passkey_from_row).transpose()
    }

    async fn passkey_rename(&self, account_id: i64, id: i64, name: &str) -> StoreResult<bool> {
        let r = sqlx::query(
            "UPDATE passkey_credentials SET name = $3, updated_at = now() \
             WHERE id = $1 AND account_id = $2",
        )
        .bind(id)
        .bind(account_id)
        .bind(name)
        .execute(&self.pool)
        .await
        .map_err(store_err)?;
        Ok(r.rows_affected() > 0)
    }

    async fn passkey_delete(&self, account_id: i64, id: i64) -> StoreResult<bool> {
        let r = sqlx::query("DELETE FROM passkey_credentials WHERE id = $1 AND account_id = $2")
            .bind(id)
            .bind(account_id)
            .execute(&self.pool)
            .await
            .map_err(store_err)?;
        Ok(r.rows_affected() > 0)
    }

    async fn passkey_record_use(
        &self,
        id: i64,
        expected_sign_count: i64,
        sign_count: i64,
        backup_state: bool,
        now: i64,
    ) -> StoreResult<bool> {
        let r = sqlx::query(
            "UPDATE passkey_credentials SET sign_count = $3, backup_state = $4, \
             last_used_at = to_timestamp($5::float8), updated_at = now() \
             WHERE id = $1 AND sign_count = $2",
        )
        .bind(id)
        .bind(expected_sign_count)
        .bind(sign_count)
        .bind(backup_state)
        .bind(now as f64)
        .execute(&self.pool)
        .await
        .map_err(store_err)?;
        Ok(r.rows_affected() > 0)
    }

    async fn delete_account(
        &self,
        id: i64,
        ledger_id: i64,
    ) -> StoreResult<Result<DeletedAccount, DeleteRefusal>> {
        let done = crate::account_deletion::delete_account(&self.pool, id, ledger_id)
            .await
            .map_err(store_err)?;
        Ok(match done {
            Ok(r) => Ok(DeletedAccount {
                guilds_left: r.guilds_left,
                friends: r.friends,
                other_relations: r.other_relations,
                credits_forfeited: r.credits_forfeited,
                invites_revoked: r.invites_revoked,
                avatar_key: r.avatar_key,
                banner_key: r.banner_key,
            }),
            Err(crate::account_deletion::Blocked::NotActive) => Err(DeleteRefusal::NotActive),
            Err(crate::account_deletion::Blocked::OwnsGuilds(g)) => {
                Err(DeleteRefusal::OwnsGuilds(g))
            }
        })
    }

    async fn audit(
        &self,
        actor_id: i64,
        action: &str,
        target_id: Option<i64>,
        note: &str,
        now: i64,
    ) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO platform_audit_log (id, actor_id, action, target_id, note, created_at) \
             VALUES ($1, $2, $3, $4, $5, to_timestamp($6::float8))",
        )
        .bind(self.next_id())
        .bind(actor_id)
        .bind(action)
        .bind(target_id)
        .bind(note)
        .bind(now as f64)
        .execute(&self.pool)
        .await
        .map_err(store_err)?;
        Ok(())
    }
}
