//! Transactional invite redemption (ENT-INV-007, ENT-INV-008, docs/06 section 3.3).
//!
//! Lock order (identical for every registration, so no deadlocks):
//! invite row `FOR UPDATE` -> `CAP_LOCK` advisory lock -> issuer account row
//! `FOR UPDATE`. The cap lock serialises all registrations, which also makes
//! the username/email pre-checks race-free against other registrations.
//!
//! Person matching (open question 142): a Discord identity already linked to
//! an account, or a verified email of an existing account, reuses that
//! account's Person (D-030). If that Person has a platform-banned account the
//! registration is refused with [`Rejection::PersonBanned`] and nothing is
//! written.

use sqlx::PgPool;

use crate::{DbResult, lock};

/// Input of [`register_with_invite`].
#[derive(Debug, Clone)]
pub struct NewRegistration {
    pub account_id: i64,
    /// Person to create when no existing Person matches.
    pub person_id: i64,
    /// `[issuer -1 redemption, new account +N initial_grant]` ledger row ids.
    pub ledger_ids: [i64; 2],
    /// Canonical invite code (10 Crockford characters).
    pub code: String,
    /// Lowercase username.
    pub username: String,
    pub display_name: Option<String>,
    /// Lowercase email.
    pub email: String,
    /// Argon2id PHC string.
    pub password_hash: String,
    /// Discord user id proven by OAuth in this flow, linked on success.
    pub discord_id: Option<String>,
    /// Row id for the `external_identities` link (used with `discord_id`).
    pub identity_id: i64,
    /// Maximum number of `active` accounts.
    pub account_cap: i64,
    pub initial_credits: i32,
}

/// Why nothing was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    InviteUnavailable,
    AtCap,
    IssuerOutOfCredits,
    PersonBanned,
    UsernameTaken,
    EmailTaken,
}

/// A created account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Registered {
    pub account_id: i64,
    pub issuer_id: i64,
}

/// Redeem an invite and create the account in one transaction.
pub async fn register_with_invite(
    pool: &PgPool,
    r: &NewRegistration,
) -> DbResult<Result<Registered, Rejection>> {
    let mut tx = pool.begin().await?;

    let invite: Option<(i64, i64, String, bool)> = sqlx::query_as(
        "SELECT id, issuer_account_id, state::text, emergency FROM platform_invites \
         WHERE code = $1 FOR UPDATE",
    )
    .bind(&r.code)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((invite_id, issuer_id, state, emergency)) = invite else {
        return Ok(Err(Rejection::InviteUnavailable));
    };
    if state != "open" {
        return Ok(Err(Rejection::InviteUnavailable));
    }

    lock::xact_lock(&mut tx, lock::CAP_LOCK).await?;
    let active: i64 = sqlx::query_scalar("SELECT count(*) FROM accounts WHERE state = 'active'")
        .fetch_one(&mut *tx)
        .await?;
    if active >= r.account_cap {
        return Ok(Err(Rejection::AtCap));
    }

    // Serialise credit checks per issuer, then read the balance.
    sqlx::query("SELECT 1 FROM accounts WHERE id = $1 FOR UPDATE")
        .bind(issuer_id)
        .execute(&mut *tx)
        .await?;
    let balance: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(delta), 0)::BIGINT FROM invite_credits_ledger WHERE account_id = $1",
    )
    .bind(issuer_id)
    .fetch_one(&mut *tx)
    .await?;
    // Emergency (operator) invites are outside the credit economy (open question 46).
    if !emergency && balance < 1 {
        return Ok(Err(Rejection::IssuerOutOfCredits));
    }

    let username_taken: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM accounts WHERE username = $1)")
            .bind(&r.username)
            .fetch_one(&mut *tx)
            .await?;
    if username_taken {
        return Ok(Err(Rejection::UsernameTaken));
    }
    let email_taken: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM accounts WHERE email = $1)")
            .bind(&r.email)
            .fetch_one(&mut *tx)
            .await?;
    if email_taken {
        return Ok(Err(Rejection::EmailTaken));
    }

    // Person matching (open question 142): Discord identity first, then a
    // verified email of an existing account, across every account state.
    let discord_known: Option<i64> = match &r.discord_id {
        Some(did) => {
            sqlx::query_scalar::<_, i64>(
                "SELECT a.person_id FROM external_identities e \
                 JOIN accounts a ON a.id = e.account_id \
                 WHERE e.provider = 'discord' AND e.provider_user_id = $1 LIMIT 1",
            )
            .bind(did)
            .fetch_optional(&mut *tx)
            .await?
        }
        None => None,
    };
    let matched_person = match discord_known {
        Some(p) => Some(p),
        None => {
            sqlx::query_scalar::<_, i64>(
                "SELECT person_id FROM accounts \
                 WHERE email = $1 AND email_verified_at IS NOT NULL LIMIT 1",
            )
            .bind(&r.email)
            .fetch_optional(&mut *tx)
            .await?
        }
    };
    let person_id = match matched_person {
        Some(p) => {
            let banned: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM accounts \
                 WHERE person_id = $1 AND state = 'platform_banned') \
                 OR EXISTS(SELECT 1 FROM person_bans \
                 WHERE person_id = $1 AND lifted_at IS NULL)",
            )
            .bind(p)
            .fetch_one(&mut *tx)
            .await?;
            if banned {
                return Ok(Err(Rejection::PersonBanned));
            }
            p
        }
        None => {
            sqlx::query("INSERT INTO persons (id) VALUES ($1)")
                .bind(r.person_id)
                .execute(&mut *tx)
                .await?;
            r.person_id
        }
    };

    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, display_name, email, password_hash, \
         state, invited_by_invite_id) \
         VALUES ($1, $2, $3, $4, $5, $6, 'active', $7)",
    )
    .bind(r.account_id)
    .bind(person_id)
    .bind(&r.username)
    .bind(&r.display_name)
    .bind(&r.email)
    .bind(&r.password_hash)
    .bind(invite_id)
    .execute(&mut *tx)
    .await?;
    // A Discord identity already linked to a Person is not linked twice.
    if let Some(did) = r.discord_id.as_ref().filter(|_| discord_known.is_none()) {
        sqlx::query(
            "INSERT INTO external_identities (id, account_id, provider, provider_user_id) \
             VALUES ($1, $2, 'discord', $3)",
        )
        .bind(r.identity_id)
        .bind(r.account_id)
        .bind(did)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "UPDATE platform_invites SET state = 'redeemed', redeemed_by_account_id = $2, \
         redeemed_at = now(), updated_at = now() WHERE id = $1",
    )
    .bind(invite_id)
    .bind(r.account_id)
    .execute(&mut *tx)
    .await?;
    // Operator emergency invites never touch the issuer's credit (open question 46).
    if !emergency {
        sqlx::query(
            "INSERT INTO invite_credits_ledger (id, account_id, delta, reason, ref_id) \
             VALUES ($1, $2, -1, 'redemption', $3)",
        )
        .bind(r.ledger_ids[0])
        .bind(issuer_id)
        .bind(invite_id)
        .execute(&mut *tx)
        .await?;
    }
    if r.initial_credits > 0 {
        sqlx::query(
            "INSERT INTO invite_credits_ledger (id, account_id, delta, reason, ref_id) \
             VALUES ($1, $2, $3, 'initial_grant', $4)",
        )
        .bind(r.ledger_ids[1])
        .bind(r.account_id)
        .bind(r.initial_credits)
        .bind(invite_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(Ok(Registered {
        account_id: r.account_id,
        issuer_id,
    }))
}
