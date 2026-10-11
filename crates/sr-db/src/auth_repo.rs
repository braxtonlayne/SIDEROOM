//! Postgres implementations of the `sr-auth` repository traits
//! (sessions, TOTP, recovery codes, invites, email tokens).
//!
//! Times at the trait boundary are Unix seconds; columns are `timestamptz`.
//! Row ids that the traits do not carry are minted by the injected id source
//! (a Snowflake generator in production). Every state transition that the
//! traits mark atomic is a single guarded SQL statement.

use std::sync::Arc;

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row, postgres::PgRow};
use sr_auth::{
    email_token::EmailTokenPurpose,
    invite::InviteCode,
    repo::{
        EmailTokenRepository, InviteRecord, InviteRepository, InviteStatus, RecoveryCodeRepository,
        SessionRecord, SessionRepository, TotpRecord, TotpRepository,
    },
    session::TokenHash,
};

const NONCE_LEN: usize = 12;

/// Errors from the Postgres auth repositories.
#[derive(Debug, thiserror::Error)]
pub enum AuthRepoError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    /// Unique violation (e.g. an invite code collision; regenerate and retry).
    #[error("duplicate row")]
    Duplicate,
    /// The targeted row does not exist or is not in the required state.
    #[error("row not found or in the wrong state")]
    NotFound,
    /// The account already has a confirmed TOTP secret.
    #[error("totp already confirmed")]
    AlreadyConfirmed,
    /// Stored data failed to decrypt or decode (wrong key or corruption).
    #[error("stored secret is unreadable")]
    Corrupt,
}

/// Source of new row ids (Snowflakes).
pub type IdSource = Arc<dyn Fn() -> i64 + Send + Sync>;

/// Postgres-backed implementation of all `sr-auth` repository traits.
#[derive(Clone)]
pub struct PgAuthRepo {
    pool: PgPool,
    ids: IdSource,
    key: [u8; 32],
    /// Decrypt-only keys from before the last rotation (`server.secret_key_previous`).
    previous: Vec<[u8; 32]>,
}

/// Key version byte stored in front of every ciphertext: the first byte of a
/// domain-separated SHA-256 of the key. It is a fingerprint, not a secret, so
/// the decryptor can try the keys that match before the others (open question 40).
fn key_version(key: &[u8; 32]) -> u8 {
    let mut h = Sha256::new();
    h.update(b"sideroom/secret-key-version/v1");
    h.update(key);
    h.finalize()[0]
}

/// Parse `server.secret_key` (64 hex characters).
pub fn parse_secret_key(hex: &str) -> Result<[u8; 32], String> {
    if hex.len() != 64 || !hex.is_ascii() {
        return Err("secret key must be 64 hex characters".into());
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16)
            .map_err(|_| "secret key must be hex".to_owned())?;
    }
    Ok(out)
}

impl std::fmt::Debug for PgAuthRepo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PgAuthRepo").finish_non_exhaustive()
    }
}

impl PgAuthRepo {
    /// `secret_key` is the 32-byte key encrypting TOTP secrets at rest.
    pub fn new(pool: PgPool, ids: IdSource, secret_key: [u8; 32]) -> Self {
        Self {
            pool,
            ids,
            key: secret_key,
            previous: Vec::new(),
        }
    }

    /// Add the decrypt-only keys from `server.secret_key_previous`.
    pub fn with_previous_keys(mut self, keys: Vec<[u8; 32]>) -> Self {
        self.previous = keys;
        self
    }

    fn next_id(&self) -> i64 {
        (self.ids)()
    }

    /// Layout: `version(1) || nonce(12) || ciphertext+tag`, AAD = account id.
    fn seal(&self, user_id: i64, plain: &[u8]) -> Result<Vec<u8>, AuthRepoError> {
        let nonce = sr_auth::random::bytes::<NONCE_LEN>().map_err(|_| AuthRepoError::Corrupt)?;
        let cipher = Aes256Gcm::new((&self.key).into());
        let aad = user_id.to_be_bytes();
        let ct = cipher
            .encrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: plain,
                    aad: &aad,
                },
            )
            .map_err(|_| AuthRepoError::Corrupt)?;
        let mut out = Vec::with_capacity(1 + NONCE_LEN + ct.len());
        out.push(key_version(&self.key));
        out.extend_from_slice(&nonce);
        out.extend(ct);
        Ok(out)
    }

    fn open(&self, user_id: i64, blob: &[u8]) -> Result<Vec<u8>, AuthRepoError> {
        let (&version, rest) = blob.split_first().ok_or(AuthRepoError::Corrupt)?;
        if rest.len() <= NONCE_LEN {
            return Err(AuthRepoError::Corrupt);
        }
        let (nonce, ct) = rest.split_at(NONCE_LEN);
        let nonce: [u8; NONCE_LEN] = nonce.try_into().map_err(|_| AuthRepoError::Corrupt)?;
        let aad = user_id.to_be_bytes();
        // Keys whose fingerprint matches the stored version are tried first;
        // the others follow in case of a fingerprint collision.
        let keys = std::iter::once(&self.key).chain(self.previous.iter());
        let (matching, others): (Vec<&[u8; 32]>, Vec<&[u8; 32]>) =
            keys.partition(|k| key_version(k) == version);
        for key in matching.into_iter().chain(others) {
            let cipher = Aes256Gcm::new(key.into());
            if let Ok(plain) = cipher.decrypt(&Nonce::from(nonce), Payload { msg: ct, aad: &aad }) {
                return Ok(plain);
            }
        }
        Err(AuthRepoError::Corrupt)
    }

    /// Re-encrypt every TOTP secret under the current key, in one transaction
    /// (`sr-cli rotate-secret-key`, open question 40). Returns rows rewritten.
    pub async fn rotate_secret_keys(&self) -> Result<u64, AuthRepoError> {
        let rows = sqlx::query("SELECT id, account_id, secret_enc FROM mfa_totp")
            .fetch_all(&self.pool)
            .await?;
        let mut tx = self.pool.begin().await?;
        let mut n = 0;
        for r in &rows {
            let id: i64 = r.try_get("id")?;
            let account: i64 = r.try_get("account_id")?;
            let enc: Vec<u8> = r.try_get("secret_enc")?;
            let plain = self.open(account, &enc)?;
            let sealed = self.seal(account, &plain)?;
            sqlx::query("UPDATE mfa_totp SET secret_enc = $2, updated_at = now() WHERE id = $1")
                .bind(id)
                .bind(sealed)
                .execute(&mut *tx)
                .await?;
            n += 1;
        }
        tx.commit().await?;
        Ok(n)
    }
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .is_some_and(|d| d.code().as_deref() == Some("23505"))
}

fn map_unique(e: sqlx::Error) -> AuthRepoError {
    if is_unique_violation(&e) {
        AuthRepoError::Duplicate
    } else {
        e.into()
    }
}

const SESSION_COLS: &str = "id, account_id, token_hash, \
    EXTRACT(EPOCH FROM created_at)::BIGINT AS created_at, \
    EXTRACT(EPOCH FROM last_seen_at)::BIGINT AS last_seen_at, \
    EXTRACT(EPOCH FROM expires_at)::BIGINT AS expires_at, device_label";

fn session_from_row(r: &PgRow) -> Result<SessionRecord, AuthRepoError> {
    let hash: Vec<u8> = r.try_get("token_hash")?;
    Ok(SessionRecord {
        id: r.try_get("id")?,
        user_id: r.try_get("account_id")?,
        token_hash: TokenHash::from_bytes(&hash).map_err(|_| AuthRepoError::Corrupt)?,
        created_at: r.try_get("created_at")?,
        last_seen_at: r.try_get("last_seen_at")?,
        expires_at: r.try_get("expires_at")?,
        device_name: r.try_get("device_label")?,
    })
}

impl SessionRepository for PgAuthRepo {
    type Error = AuthRepoError;

    async fn create(&self, rec: SessionRecord) -> Result<(), Self::Error> {
        sqlx::query(
            "INSERT INTO sessions (id, account_id, token_hash, device_label, \
             created_at, last_seen_at, expires_at) \
             VALUES ($1, $2, $3, $4, to_timestamp($5::float8), to_timestamp($6::float8), \
             to_timestamp($7::float8))",
        )
        .bind(rec.id)
        .bind(rec.user_id)
        .bind(rec.token_hash.as_bytes().as_slice())
        .bind(rec.device_name)
        .bind(rec.created_at as f64)
        .bind(rec.last_seen_at as f64)
        .bind(rec.expires_at as f64)
        .execute(&self.pool)
        .await
        .map_err(map_unique)?;
        Ok(())
    }

    async fn find_by_hash(&self, hash: &TokenHash) -> Result<Option<SessionRecord>, Self::Error> {
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {SESSION_COLS} FROM sessions WHERE token_hash = $1 AND revoked_at IS NULL"
        )))
        .bind(hash.as_bytes().as_slice())
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(session_from_row).transpose()
    }

    async fn touch(&self, id: i64, last_seen_at: i64, expires_at: i64) -> Result<(), Self::Error> {
        sqlx::query(
            "UPDATE sessions SET last_seen_at = to_timestamp($2::float8), \
             expires_at = to_timestamp($3::float8), updated_at = now() \
             WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(id)
        .bind(last_seen_at as f64)
        .bind(expires_at as f64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_for_user(
        &self,
        user_id: i64,
        now: i64,
    ) -> Result<Vec<SessionRecord>, Self::Error> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {SESSION_COLS} FROM sessions WHERE account_id = $1 \
             AND revoked_at IS NULL AND expires_at > to_timestamp($2::float8) \
             ORDER BY last_seen_at DESC"
        )))
        .bind(user_id)
        .bind(now as f64)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(session_from_row).collect()
    }

    async fn revoke(&self, user_id: i64, session_id: i64) -> Result<bool, Self::Error> {
        let r = sqlx::query(
            "UPDATE sessions SET revoked_at = now(), updated_at = now() \
             WHERE id = $1 AND account_id = $2 AND revoked_at IS NULL",
        )
        .bind(session_id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected() > 0)
    }

    async fn revoke_all_except(&self, user_id: i64, keep: Option<i64>) -> Result<u64, Self::Error> {
        let r = sqlx::query(
            "UPDATE sessions SET revoked_at = now(), updated_at = now() \
             WHERE account_id = $1 AND revoked_at IS NULL AND id IS DISTINCT FROM $2",
        )
        .bind(user_id)
        .bind(keep)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected())
    }
}

impl TotpRepository for PgAuthRepo {
    type Error = AuthRepoError;

    async fn get(&self, user_id: i64) -> Result<Option<TotpRecord>, Self::Error> {
        let row = sqlx::query(
            "SELECT secret_enc, confirmed_at IS NOT NULL AS confirmed, last_used_step \
             FROM mfa_totp WHERE account_id = $1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(r) = row else { return Ok(None) };
        let enc: Vec<u8> = r.try_get("secret_enc")?;
        let step: Option<i64> = r.try_get("last_used_step")?;
        Ok(Some(TotpRecord {
            user_id,
            secret: self.open(user_id, &enc)?,
            confirmed: r.try_get("confirmed")?,
            last_used_step: step.and_then(|s| u64::try_from(s).ok()),
        }))
    }

    async fn put_pending(&self, user_id: i64, secret: &[u8]) -> Result<(), Self::Error> {
        let enc = self.seal(user_id, secret)?;
        // Never overwrite a confirmed secret: disabling is an explicit `delete`.
        let r = sqlx::query(
            "INSERT INTO mfa_totp (id, account_id, secret_enc) VALUES ($1, $2, $3) \
             ON CONFLICT (account_id) DO UPDATE SET secret_enc = EXCLUDED.secret_enc, \
             last_used_step = NULL, updated_at = now() WHERE mfa_totp.confirmed_at IS NULL",
        )
        .bind(self.next_id())
        .bind(user_id)
        .bind(enc)
        .execute(&self.pool)
        .await?;
        if r.rows_affected() == 0 {
            return Err(AuthRepoError::AlreadyConfirmed);
        }
        Ok(())
    }

    async fn confirm(&self, user_id: i64, step: u64) -> Result<(), Self::Error> {
        let r = sqlx::query(
            "UPDATE mfa_totp SET confirmed_at = now(), last_used_step = $2, updated_at = now() \
             WHERE account_id = $1 AND confirmed_at IS NULL",
        )
        .bind(user_id)
        .bind(i64::try_from(step).map_err(|_| AuthRepoError::Corrupt)?)
        .execute(&self.pool)
        .await?;
        if r.rows_affected() == 0 {
            return Err(AuthRepoError::NotFound);
        }
        Ok(())
    }

    async fn consume_step(&self, user_id: i64, step: u64) -> Result<bool, Self::Error> {
        let r = sqlx::query(
            "UPDATE mfa_totp SET last_used_step = $2, updated_at = now() \
             WHERE account_id = $1 AND confirmed_at IS NOT NULL \
             AND (last_used_step IS NULL OR last_used_step < $2)",
        )
        .bind(user_id)
        .bind(i64::try_from(step).map_err(|_| AuthRepoError::Corrupt)?)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected() > 0)
    }

    async fn delete(&self, user_id: i64) -> Result<(), Self::Error> {
        sqlx::query("DELETE FROM mfa_totp WHERE account_id = $1")
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

impl RecoveryCodeRepository for PgAuthRepo {
    type Error = AuthRepoError;

    async fn replace_all(&self, user_id: i64, hashes: &[TokenHash]) -> Result<(), Self::Error> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM recovery_codes WHERE account_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        for h in hashes {
            sqlx::query(
                "INSERT INTO recovery_codes (id, account_id, code_hash) VALUES ($1, $2, $3)",
            )
            .bind(self.next_id())
            .bind(user_id)
            .bind(h.as_bytes().as_slice())
            .execute(&mut *tx)
            .await
            .map_err(map_unique)?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn list_unused(&self, user_id: i64) -> Result<Vec<(i64, TokenHash)>, Self::Error> {
        let rows = sqlx::query(
            "SELECT id, code_hash FROM recovery_codes \
             WHERE account_id = $1 AND used_at IS NULL ORDER BY id",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| -> Result<(i64, TokenHash), AuthRepoError> {
                let hash: Vec<u8> = r.try_get("code_hash")?;
                Ok((
                    r.try_get("id")?,
                    TokenHash::from_bytes(&hash).map_err(|_| AuthRepoError::Corrupt)?,
                ))
            })
            .collect()
    }

    async fn consume(&self, user_id: i64, row_id: i64, now: i64) -> Result<bool, Self::Error> {
        let r = sqlx::query(
            "UPDATE recovery_codes SET used_at = to_timestamp($3::float8), updated_at = now() \
             WHERE id = $1 AND account_id = $2 AND used_at IS NULL",
        )
        .bind(row_id)
        .bind(user_id)
        .bind(now as f64)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected() > 0)
    }
}

impl InviteRepository for PgAuthRepo {
    type Error = AuthRepoError;

    async fn create(&self, rec: InviteRecord) -> Result<(), Self::Error> {
        // New invites are always open; other states arise by transition.
        if rec.status != InviteStatus::Open {
            return Err(AuthRepoError::NotFound);
        }
        sqlx::query(
            "INSERT INTO platform_invites (id, code, issuer_account_id, note, created_at) \
             VALUES ($1, $2, $3, $4, to_timestamp($5::float8))",
        )
        .bind(self.next_id())
        .bind(&rec.code)
        .bind(rec.issuer_id)
        .bind(rec.note.unwrap_or_default())
        .bind(rec.created_at as f64)
        .execute(&self.pool)
        .await
        .map_err(map_unique)?;
        Ok(())
    }

    async fn find(&self, code: &InviteCode) -> Result<Option<InviteRecord>, Self::Error> {
        let row = sqlx::query(
            "SELECT code::text AS code, issuer_account_id, state::text AS state, note, \
             EXTRACT(EPOCH FROM created_at)::BIGINT AS created_at \
             FROM platform_invites WHERE code = $1",
        )
        .bind(code.canonical())
        .fetch_optional(&self.pool)
        .await?;
        let Some(r) = row else { return Ok(None) };
        let state: String = r.try_get("state")?;
        let note: String = r.try_get("note")?;
        Ok(Some(InviteRecord {
            code: r.try_get("code")?,
            issuer_id: r.try_get("issuer_account_id")?,
            note: (!note.is_empty()).then_some(note),
            status: match state.as_str() {
                "open" => InviteStatus::Open,
                "redeemed" => InviteStatus::Redeemed,
                "revoked" => InviteStatus::Revoked,
                _ => return Err(AuthRepoError::Corrupt),
            },
            created_at: r.try_get("created_at")?,
        }))
    }

    async fn count_open(&self, issuer_id: i64) -> Result<u64, Self::Error> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM platform_invites WHERE issuer_account_id = $1 AND state = 'open'",
        )
        .bind(issuer_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(u64::try_from(n).unwrap_or(0))
    }

    async fn revoke(&self, issuer_id: i64, code: &InviteCode) -> Result<bool, Self::Error> {
        let r = sqlx::query(
            "UPDATE platform_invites SET state = 'revoked', revoked_at = now(), updated_at = now() \
             WHERE issuer_account_id = $1 AND code = $2 AND state = 'open'",
        )
        .bind(issuer_id)
        .bind(code.canonical())
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected() > 0)
    }
}

fn purpose_sql(p: EmailTokenPurpose) -> &'static str {
    match p {
        EmailTokenPurpose::VerifyEmail => "verify_email",
        EmailTokenPurpose::ResetPassword => "reset_password",
        EmailTokenPurpose::LoginLink => "login_link",
    }
}

impl EmailTokenRepository for PgAuthRepo {
    type Error = AuthRepoError;

    async fn put(
        &self,
        user_id: i64,
        purpose: EmailTokenPurpose,
        hash: &TokenHash,
        expires_at: i64,
    ) -> Result<(), Self::Error> {
        let mut tx = self.pool.begin().await?;
        // Supersede earlier unconsumed tokens of the same purpose.
        sqlx::query(
            "DELETE FROM email_tokens WHERE account_id = $1 \
             AND purpose = $2::email_token_purpose AND used_at IS NULL",
        )
        .bind(user_id)
        .bind(purpose_sql(purpose))
        .execute(&mut *tx)
        .await?;
        let r = sqlx::query(
            "INSERT INTO email_tokens (id, account_id, purpose, email, token_hash, expires_at) \
             SELECT $1, a.id, $3::email_token_purpose, a.email, $4, to_timestamp($5::float8) \
             FROM accounts a WHERE a.id = $2",
        )
        .bind(self.next_id())
        .bind(user_id)
        .bind(purpose_sql(purpose))
        .bind(hash.as_bytes().as_slice())
        .bind(expires_at as f64)
        .execute(&mut *tx)
        .await?;
        if r.rows_affected() == 0 {
            return Err(AuthRepoError::NotFound);
        }
        tx.commit().await?;
        Ok(())
    }

    async fn consume(
        &self,
        purpose: EmailTokenPurpose,
        hash: &TokenHash,
        now: i64,
    ) -> Result<Option<i64>, Self::Error> {
        let id: Option<i64> = sqlx::query_scalar(
            "UPDATE email_tokens SET used_at = to_timestamp($3::float8), updated_at = now() \
             WHERE token_hash = $1 AND purpose = $2::email_token_purpose AND used_at IS NULL \
             AND expires_at > to_timestamp($3::float8) RETURNING account_id",
        )
        .bind(hash.as_bytes().as_slice())
        .bind(purpose_sql(purpose))
        .bind(now as f64)
        .fetch_optional(&self.pool)
        .await?;
        Ok(id)
    }
}
