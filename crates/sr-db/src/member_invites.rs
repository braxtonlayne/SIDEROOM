//! Platform invites an account manages itself (docs/06 section 5, D-024, D-026, D-014).
//!
//! Rules to create a code: the account is active with a verified email and at least
//! `invites.min_account_age_days` old (default 7), the account cap is not reached, the open codes
//! are fewer than the remaining credits, and the issuer's Person is under the suspicion freeze
//! threshold. Creating locks the issuer row so two requests cannot both take the last credit.

use sqlx::{PgPool, Row};

use sr_auth::store::{InviteBlock, MemberInviteRow, MemberInvites};

use crate::{DbResult, lock};

const DEFAULT_MIN_AGE_DAYS: i64 = 7;
const DEFAULT_FREEZE: i64 = 2;
const DEFAULT_CAP: i64 = 20_000;

async fn setting_int(conn: &mut sqlx::PgConnection, key: &str, default: i64) -> DbResult<i64> {
    let v: Option<i64> = sqlx::query_scalar(
        "SELECT floor((value #>> '{}')::numeric)::bigint FROM platform_settings \
         WHERE key = $1 AND jsonb_typeof(value) = 'number'",
    )
    .bind(key)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(v.unwrap_or(default))
}

/// First rule that stops `account` from creating a code, or `None`.
async fn block_for(
    conn: &mut sqlx::PgConnection,
    account: i64,
    now: i64,
    balance: i64,
    open: i64,
) -> DbResult<Option<InviteBlock>> {
    let row = sqlx::query(
        "SELECT a.email_verified_at IS NOT NULL AS verified, \
                (extract(epoch FROM a.created_at))::bigint AS created, \
                p.suspicion_level AS suspicion, \
                a.invites_frozen_at IS NOT NULL AS frozen \
         FROM accounts a JOIN persons p ON p.id = a.person_id WHERE a.id = $1",
    )
    .bind(account)
    .fetch_one(&mut *conn)
    .await?;
    if !row.try_get::<bool, _>("verified")? {
        return Ok(Some(InviteBlock::Unverified));
    }
    let min_days = setting_int(conn, "invites.min_account_age_days", DEFAULT_MIN_AGE_DAYS).await?;
    let created: i64 = row.try_get("created")?;
    if now - created < min_days * 86_400 {
        return Ok(Some(InviteBlock::TooNew { days: min_days }));
    }
    let freeze = setting_int(conn, "invites.suspicion_freeze_threshold", DEFAULT_FREEZE).await?;
    let suspicion: i16 = row.try_get("suspicion")?;
    if row.try_get::<bool, _>("frozen")? || i64::from(suspicion) >= freeze {
        return Ok(Some(InviteBlock::Frozen));
    }
    let cap = setting_int(conn, "accounts.cap", DEFAULT_CAP).await?;
    let active: i64 =
        sqlx::query_scalar("SELECT count(*) FROM accounts WHERE state = 'active' AND NOT is_bot")
            .fetch_one(&mut *conn)
            .await?;
    if active >= cap {
        return Ok(Some(InviteBlock::AtCap));
    }
    if open >= balance {
        return Ok(Some(InviteBlock::NoCredits));
    }
    Ok(None)
}

fn code_row(r: &sqlx::postgres::PgRow) -> DbResult<MemberInviteRow> {
    let by: Option<i64> = r.try_get("redeemer_id")?;
    Ok(MemberInviteRow {
        code: r.try_get("code")?,
        note: r.try_get("note")?,
        created_at: r.try_get("created")?,
        redeemed_by: match by {
            Some(id) => Some((id, r.try_get("redeemer_name")?)),
            None => None,
        },
        redeemed_at: r.try_get("redeemed")?,
    })
}

/// Credits, open and redeemed codes, and the block (if any).
pub async fn overview(pool: &PgPool, account: i64, now: i64) -> DbResult<MemberInvites> {
    let mut conn = pool.acquire().await?;
    let balance: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(delta), 0)::BIGINT FROM invite_credits_ledger WHERE account_id = $1",
    )
    .bind(account)
    .fetch_one(&mut *conn)
    .await?;
    let rows = sqlx::query(
        "SELECT pi.code::text AS code, pi.note, (extract(epoch FROM pi.created_at))::bigint AS created, \
                pi.state::text AS state, r.id AS redeemer_id, r.username::text AS redeemer_name, \
                (extract(epoch FROM pi.redeemed_at))::bigint AS redeemed \
         FROM platform_invites pi LEFT JOIN accounts r ON r.id = pi.redeemed_by_account_id \
         WHERE pi.issuer_account_id = $1 AND pi.state IN ('open', 'redeemed') AND NOT pi.emergency \
         ORDER BY pi.created_at DESC, pi.id DESC LIMIT 500",
    )
    .bind(account)
    .fetch_all(&mut *conn)
    .await?;
    let (mut open, mut redeemed) = (Vec::new(), Vec::new());
    for r in &rows {
        let state: String = r.try_get("state")?;
        let row = code_row(r)?;
        if state == "open" {
            open.push(row);
        } else {
            redeemed.push(row);
        }
    }
    let blocked = block_for(&mut conn, account, now, balance, open.len() as i64).await?;
    Ok(MemberInvites {
        balance,
        open,
        redeemed,
        blocked,
    })
}

/// Create an open code; every rule is re-checked under a lock on the issuer row.
pub async fn create(
    pool: &PgPool,
    account: i64,
    invite_id: i64,
    code: &str,
    note: &str,
    now: i64,
) -> DbResult<Result<(), InviteBlock>> {
    let mut tx = pool.begin().await?;
    // The cap lock first (as registration does), then the issuer row.
    lock::xact_lock(&mut tx, lock::CAP_LOCK).await?;
    sqlx::query("SELECT 1 FROM accounts WHERE id = $1 FOR UPDATE")
        .bind(account)
        .execute(&mut *tx)
        .await?;
    let balance: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(delta), 0)::BIGINT FROM invite_credits_ledger WHERE account_id = $1",
    )
    .bind(account)
    .fetch_one(&mut *tx)
    .await?;
    let open: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM platform_invites WHERE issuer_account_id = $1 AND state = 'open' \
         AND NOT emergency",
    )
    .bind(account)
    .fetch_one(&mut *tx)
    .await?;
    if let Some(b) = block_for(&mut tx, account, now, balance, open).await? {
        return Ok(Err(b));
    }
    sqlx::query(
        "INSERT INTO platform_invites (id, code, issuer_account_id, note) VALUES ($1, $2, $3, $4)",
    )
    .bind(invite_id)
    .bind(code)
    .bind(account)
    .bind(note)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Ok(()))
}

/// Revoke an open code the account issued.
pub async fn revoke(pool: &PgPool, account: i64, code: &str) -> DbResult<bool> {
    let n = sqlx::query(
        "UPDATE platform_invites SET state = 'revoked', revoked_at = now(), updated_at = now() \
         WHERE code = $1 AND issuer_account_id = $2 AND state = 'open' AND NOT emergency",
    )
    .bind(code)
    .bind(account)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(n > 0)
}
