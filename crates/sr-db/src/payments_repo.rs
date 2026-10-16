//! Postgres implementation of [`sr_payments::repo::PaymentRepo`] (tables in migrations
//! 0540..0542). `save_evaluation` and `resolve` lock the invoice row (`FOR UPDATE`) and apply or
//! revert the credit in the same transaction, so crediting is exactly-once under polling,
//! restarts and concurrent workers. the legacy download table is gone (migration 0871).
//!
//! Tests: `tests/payments_repo.rs` (needs `SR_TEST_DATABASE_URL`).

use async_trait::async_trait;
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgRow};
use sr_payments::{
    config::DonationConfig,
    entitlements::{
        DAY_MS, Grant, GrantKind, GrantSource, GuildEntitlement, PatronStatus, next_level,
        notice_level, patron_status, stack_patron,
    },
    error::{PayError, PayResult},
    invoice::{
        Evaluation, Invoice, InvoiceState, Purpose, Resolution, ResolutionKind, TxRecord, is_final,
    },
    repo::*,
};
use std::{collections::HashMap, sync::Arc};

pub type IdSource = Arc<dyn Fn() -> i64 + Send + Sync>;

pub struct PgPaymentRepo {
    pool: PgPool,
    ids: IdSource,
}

impl PgPaymentRepo {
    pub fn new(pool: PgPool, ids: IdSource) -> Self {
        Self { pool, ids }
    }
}

fn pe(e: sqlx::Error) -> PayError {
    if e.as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|c| c == "23503")
    {
        return PayError::NotFound;
    }
    PayError::Storage(e.to_string())
}

fn dynq(
    sql: String,
) -> sqlx::query::Query<'static, Postgres, <Postgres as sqlx::Database>::Arguments> {
    sqlx::query(sqlx::AssertSqlSafe(sql))
}

const INV_COLS: &str = "id, wallet_id, user_id, purpose, guild_id, months, credits, usd_cents, sats, \
    rate_cents_per_btc, rate_source, rate_stale, address, derivation_index, network, state, \
    confirmations_required, confirmations, received_sats, confirmed_sats, shortfall_sats, \
    excess_sats, late, anonymous, created_ms, expires_ms, quote_expires_ms, credited_ms, \
    next_check_ms, resolution_kind, resolution_note, resolved_by, resolved_ms";

fn u32_of(v: Option<i32>) -> Option<u32> {
    v.and_then(|x| u32::try_from(x).ok())
}

fn invoice_from(r: &PgRow) -> PayResult<Invoice> {
    let purpose: String = r.try_get("purpose").map_err(pe)?;
    let state: String = r.try_get("state").map_err(pe)?;
    let kind: Option<String> = r.try_get("resolution_kind").map_err(pe)?;
    let resolution = match kind.as_deref().and_then(ResolutionKind::parse) {
        Some(kind) => Some(Resolution {
            kind,
            note: r
                .try_get::<Option<String>, _>("resolution_note")
                .map_err(pe)?
                .unwrap_or_default(),
            by: r
                .try_get::<Option<i64>, _>("resolved_by")
                .map_err(pe)?
                .unwrap_or(0),
            at_ms: r
                .try_get::<Option<i64>, _>("resolved_ms")
                .map_err(pe)?
                .unwrap_or(0),
        }),
        None => None,
    };
    Ok(Invoice {
        id: r.try_get("id").map_err(pe)?,
        wallet_id: r.try_get("wallet_id").map_err(pe)?,
        user_id: r.try_get("user_id").map_err(pe)?,
        purpose: Purpose::parse(&purpose)
            .ok_or_else(|| PayError::Storage(format!("unknown purpose {purpose}")))?,
        guild_id: r.try_get("guild_id").map_err(pe)?,
        months: u32_of(r.try_get("months").map_err(pe)?),
        credits: u32_of(r.try_get("credits").map_err(pe)?),
        usd_cents: r.try_get("usd_cents").map_err(pe)?,
        sats: r.try_get("sats").map_err(pe)?,
        rate_cents_per_btc: r.try_get("rate_cents_per_btc").map_err(pe)?,
        rate_source: r.try_get("rate_source").map_err(pe)?,
        rate_stale: r.try_get("rate_stale").map_err(pe)?,
        address: r.try_get("address").map_err(pe)?,
        derivation_index: u32_of(r.try_get("derivation_index").map_err(pe)?).unwrap_or(0),
        network: r.try_get("network").map_err(pe)?,
        state: InvoiceState::parse(&state)
            .ok_or_else(|| PayError::Storage(format!("unknown state {state}")))?,
        confirmations_required: u32_of(r.try_get("confirmations_required").map_err(pe)?)
            .unwrap_or(1),
        confirmations: u32_of(r.try_get("confirmations").map_err(pe)?),
        received_sats: r.try_get("received_sats").map_err(pe)?,
        confirmed_sats: r.try_get("confirmed_sats").map_err(pe)?,
        shortfall_sats: r.try_get("shortfall_sats").map_err(pe)?,
        excess_sats: r.try_get("excess_sats").map_err(pe)?,
        late: r.try_get("late").map_err(pe)?,
        anonymous: r.try_get("anonymous").map_err(pe)?,
        created_ms: r.try_get("created_ms").map_err(pe)?,
        expires_ms: r.try_get("expires_ms").map_err(pe)?,
        quote_expires_ms: r.try_get("quote_expires_ms").map_err(pe)?,
        credited_ms: r.try_get("credited_ms").map_err(pe)?,
        next_check_ms: r.try_get("next_check_ms").map_err(pe)?,
        resolution,
    })
}

fn wallet_from(r: &PgRow) -> PayResult<Wallet> {
    Ok(Wallet {
        id: r.try_get("id").map_err(pe)?,
        network: r.try_get("network").map_err(pe)?,
        descriptor: r.try_get("descriptor").map_err(pe)?,
        fingerprint: r.try_get("fingerprint").map_err(pe)?,
        next_index: u32_of(r.try_get("next_index").map_err(pe)?).unwrap_or(0),
        gap_limit: u32_of(r.try_get("gap_limit").map_err(pe)?).unwrap_or(20),
        active: r.try_get("active").map_err(pe)?,
        created_ms: r.try_get("created_ms").map_err(pe)?,
    })
}

const WALLET_COLS: &str = "id, network, descriptor, fingerprint, next_index, gap_limit, active, \
    (extract(epoch FROM created_at) * 1000)::bigint AS created_ms";

fn grant_from(r: &PgRow) -> PayResult<Grant> {
    let kind: String = r.try_get("kind").map_err(pe)?;
    let source: String = r.try_get("source").map_err(pe)?;
    Ok(Grant {
        id: r.try_get("id").map_err(pe)?,
        kind: GrantKind::parse(&kind).ok_or_else(|| PayError::Storage("grant kind".into()))?,
        user_id: r.try_get("user_id").map_err(pe)?,
        guild_id: r.try_get("guild_id").map_err(pe)?,
        source: GrantSource::parse(&source)
            .ok_or_else(|| PayError::Storage("grant source".into()))?,
        source_id: r.try_get("source_id").map_err(pe)?,
        days: r.try_get("days").map_err(pe)?,
        ends_ms: r.try_get("ends_ms").map_err(pe)?,
        permanent: r.try_get("permanent").map_err(pe)?,
        usd_cents: r.try_get("usd_cents").map_err(pe)?,
        starts_ms: r.try_get("starts_ms").map_err(pe)?,
        revoked_ms: r.try_get("revoked_ms").map_err(pe)?,
        reason: r.try_get("reason").map_err(pe)?,
    })
}

const GRANT_COLS: &str = "id, kind, user_id, guild_id, source, source_id, days, ends_ms, permanent, usd_cents, \
     starts_ms, revoked_ms, reason";

fn tx_from(r: &PgRow) -> PayResult<TxRecord> {
    Ok(TxRecord {
        invoice_id: r.try_get("invoice_id").map_err(pe)?,
        txid: r.try_get("txid").map_err(pe)?,
        amount_sats: r.try_get("amount_sats").map_err(pe)?,
        block_height: u32_of(r.try_get("block_height").map_err(pe)?),
        confirmations: u32_of(r.try_get("confirmations").map_err(pe)?).unwrap_or(0),
        first_seen_ms: r.try_get("first_seen_ms").map_err(pe)?,
        last_seen_ms: r.try_get("last_seen_ms").map_err(pe)?,
        dropped_ms: r.try_get("dropped_ms").map_err(pe)?,
    })
}

async fn audit_in(
    tx: &mut Transaction<'_, Postgres>,
    invoice_id: i64,
    kind: &str,
    detail: Value,
    now_ms: i64,
) -> PayResult<()> {
    sqlx::query(
        "INSERT INTO payment_audit (invoice_id, at_ms, kind, detail) VALUES ($1, $2, $3, $4)",
    )
    .bind(invoice_id)
    .bind(now_ms)
    .bind(kind)
    .bind(detail)
    .execute(&mut **tx)
    .await
    .map_err(pe)?;
    Ok(())
}

impl PgPaymentRepo {
    fn id(&self) -> i64 {
        (self.ids)()
    }

    async fn lock_invoice(tx: &mut Transaction<'_, Postgres>, id: i64) -> PayResult<Invoice> {
        let row = dynq(format!(
            "SELECT {INV_COLS} FROM payment_invoices WHERE id = $1 FOR UPDATE"
        ))
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(pe)?
        .ok_or(PayError::NotFound)?;
        invoice_from(&row)
    }

    async fn apply_plan(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        inv_id: i64,
        plan: &CreditPlan,
        now_ms: i64,
    ) -> PayResult<()> {
        for spec in &plan.grants {
            sqlx::query(
                "INSERT INTO entitlement_grants (id, kind, user_id, guild_id, source, source_id, days, \
                 usd_cents, starts_ms) VALUES ($1, $2, $3, $4, 'invoice', $5, $6, $7, $8)",
            )
            .bind(self.id())
            .bind(spec.kind.as_str())
            .bind(spec.user_id)
            .bind(spec.guild_id)
            .bind(inv_id)
            .bind(spec.days)
            .bind(spec.usd_cents)
            .bind(now_ms)
            .execute(&mut **tx)
            .await
            .map_err(pe)?;
        }
        if let (Some(user), n) = (plan.invite_user, plan.invite_credits)
            && n != 0
        {
            sqlx::query(
                "INSERT INTO invite_credits_ledger (id, account_id, delta, reason, ref_id) \
                 VALUES ($1, $2, $3, 'purchase', $4)",
            )
            .bind(self.id())
            .bind(user)
            .bind(i32::try_from(n).unwrap_or(i32::MAX))
            .bind(inv_id)
            .execute(&mut **tx)
            .await
            .map_err(pe)?;
        }
        sqlx::query(
            "INSERT INTO payment_credits (invoice_id, active, credited_ms, reverted_ms, invite_credits, invite_user) \
             VALUES ($1, TRUE, $2, NULL, $3, $4) \
             ON CONFLICT (invoice_id) DO UPDATE SET active = TRUE, credited_ms = $2, reverted_ms = NULL, \
               invite_credits = $3, invite_user = $4",
        )
        .bind(inv_id)
        .bind(now_ms)
        .bind(i32::try_from(plan.invite_credits).unwrap_or(0))
        .bind(plan.invite_user)
        .execute(&mut **tx)
        .await
        .map_err(pe)?;
        Ok(())
    }

    async fn revert_credit(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        inv_id: i64,
        now_ms: i64,
    ) -> PayResult<()> {
        sqlx::query(
            "UPDATE entitlement_grants SET revoked_ms = $2, reason = COALESCE(reason, 'payment reverted') \
             WHERE source = 'invoice' AND source_id = $1 AND revoked_ms IS NULL",
        )
        .bind(inv_id)
        .bind(now_ms)
        .execute(&mut **tx)
        .await
        .map_err(pe)?;
        let row = sqlx::query(
            "UPDATE payment_credits SET active = FALSE, reverted_ms = $2 WHERE invoice_id = $1 \
             RETURNING invite_credits, invite_user",
        )
        .bind(inv_id)
        .bind(now_ms)
        .fetch_one(&mut **tx)
        .await
        .map_err(pe)?;
        let n: i32 = row.try_get("invite_credits").map_err(pe)?;
        let user: Option<i64> = row.try_get("invite_user").map_err(pe)?;
        if let (Some(user), true) = (user, n != 0) {
            sqlx::query(
                "INSERT INTO invite_credits_ledger (id, account_id, delta, reason, ref_id) \
                 VALUES ($1, $2, $3, 'staff_revoke', $4)",
            )
            .bind(self.id())
            .bind(user)
            .bind(-n)
            .bind(inv_id)
            .execute(&mut **tx)
            .await
            .map_err(pe)?;
        }
        Ok(())
    }

    async fn credit_active(tx: &mut Transaction<'_, Postgres>, inv_id: i64) -> PayResult<bool> {
        Ok(sqlx::query_scalar::<_, bool>(
            "SELECT active FROM payment_credits WHERE invoice_id = $1",
        )
        .bind(inv_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(pe)?
        .unwrap_or(false))
    }

    async fn leader_rows(&self, rows: Vec<PgRow>, guild: bool) -> PayResult<Vec<LeaderRow>> {
        rows.iter()
            .map(|r| {
                Ok(LeaderRow {
                    user_id: if guild {
                        None
                    } else {
                        r.try_get("id").map_err(pe)?
                    },
                    guild_id: if guild {
                        r.try_get("id").map_err(pe)?
                    } else {
                        None
                    },
                    cents: r.try_get("cents").map_err(pe)?,
                    hide_name: r.try_get("hide_name").map_err(pe)?,
                    hide_amount: r.try_get("hide_amount").map_err(pe)?,
                })
            })
            .collect()
    }
}

/// The contributor board query. Movies and episodes are the Cinema credits whose item is a title or an
/// episode; chapters are the Library credits. `order` is the ORDER BY clause.
fn contributor_sql(kind: ContributionKind, order: &str, ranked: bool) -> String {
    let (section, exists) = match kind {
        ContributionKind::Movies => (
            "cinema",
            " AND EXISTS (SELECT 1 FROM video_titles t WHERE t.id = c.ref_id)",
        ),
        ContributionKind::Episodes => (
            "cinema",
            " AND EXISTS (SELECT 1 FROM video_episodes e WHERE e.id = c.ref_id)",
        ),
        ContributionKind::Chapters => ("library", ""),
    };
    let rank = if ranked {
        ", ROW_NUMBER() OVER (ORDER BY count(*) DESC, c.user_id) AS rank"
    } else {
        ""
    };
    format!(
        "SELECT * FROM (SELECT c.user_id, count(*)::bigint AS count{rank} \
         FROM contribution_credits c LEFT JOIN donor_preferences p ON p.user_id = c.user_id \
         WHERE c.section = '{section}' AND c.revoked_at IS NULL AND c.user_id IS NOT NULL{exists} \
           AND NOT COALESCE(p.hide_contributions, FALSE) AND NOT COALESCE(p.staff_hidden, FALSE) \
         GROUP BY c.user_id) t ORDER BY {order}"
    )
}

#[async_trait]
impl PaymentRepo for PgPaymentRepo {
    async fn active_wallet(&self) -> PayResult<Option<Wallet>> {
        let row = dynq(format!(
            "SELECT {WALLET_COLS} FROM payment_wallets WHERE active"
        ))
        .fetch_optional(&self.pool)
        .await
        .map_err(pe)?;
        row.as_ref().map(wallet_from).transpose()
    }

    async fn wallet(&self, id: i64) -> PayResult<Option<Wallet>> {
        let row = dynq(format!(
            "SELECT {WALLET_COLS} FROM payment_wallets WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(pe)?;
        row.as_ref().map(wallet_from).transpose()
    }

    async fn set_wallet(
        &self,
        network: &str,
        descriptor: &str,
        fingerprint: &str,
        gap_limit: u32,
        by: Option<i64>,
        _now_ms: i64,
    ) -> PayResult<Wallet> {
        let mut tx = self.pool.begin().await.map_err(pe)?;
        sqlx::query("UPDATE payment_wallets SET active = FALSE WHERE active")
            .execute(&mut *tx)
            .await
            .map_err(pe)?;
        // A known wallet keeps its index counter: addresses are never reused.
        let row = dynq(format!(
            "INSERT INTO payment_wallets (id, network, descriptor, fingerprint, gap_limit, active, created_by) \
             VALUES ($1, $2, $3, $4, $5, TRUE, $6) \
             ON CONFLICT (network, fingerprint) DO UPDATE SET active = TRUE, gap_limit = $5 \
             RETURNING {WALLET_COLS}"
        ))
        .bind(self.id())
        .bind(network)
        .bind(descriptor)
        .bind(fingerprint)
        .bind(i32::try_from(gap_limit).unwrap_or(20))
        .bind(by)
        .fetch_one(&mut *tx)
        .await
        .map_err(pe)?;
        tx.commit().await.map_err(pe)?;
        wallet_from(&row)
    }

    async fn gap_used(&self, wallet_id: i64) -> PayResult<u32> {
        let row = sqlx::query(
            "SELECT w.next_index - COALESCE((SELECT max(i.derivation_index) + 1 FROM payment_invoices i \
             WHERE i.wallet_id = w.id AND EXISTS (SELECT 1 FROM payment_txs t WHERE t.invoice_id = i.id)), 0) AS run \
             FROM payment_wallets w WHERE w.id = $1",
        )
        .bind(wallet_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(pe)?
        .ok_or(PayError::NotFound)?;
        Ok(u32_of(row.try_get("run").map_err(pe)?).unwrap_or(0))
    }

    async fn set_gap_limit(&self, wallet_id: i64, gap_limit: u32) -> PayResult<()> {
        let n = sqlx::query("UPDATE payment_wallets SET gap_limit = $2 WHERE id = $1")
            .bind(wallet_id)
            .bind(i32::try_from(gap_limit).unwrap_or(20))
            .execute(&self.pool)
            .await
            .map_err(pe)?
            .rows_affected();
        if n == 0 {
            Err(PayError::NotFound)
        } else {
            Ok(())
        }
    }

    async fn config(&self) -> PayResult<DonationConfig> {
        let v: Option<Value> = sqlx::query_scalar("SELECT data FROM payment_config WHERE id = 1")
            .fetch_optional(&self.pool)
            .await
            .map_err(pe)?;
        match v {
            Some(v) => serde_json::from_value(v)
                .map_err(|e| PayError::Storage(format!("stored configuration: {e}"))),
            None => Ok(DonationConfig::default()),
        }
    }

    async fn set_config(&self, cfg: &DonationConfig, by: i64, now_ms: i64) -> PayResult<()> {
        let data = serde_json::to_value(cfg).map_err(|e| PayError::Storage(e.to_string()))?;
        sqlx::query(
            "INSERT INTO payment_config (id, data, updated_by, updated_ms) VALUES (1, $1, $2, $3) \
             ON CONFLICT (id) DO UPDATE SET data = $1, updated_by = $2, updated_ms = $3",
        )
        .bind(data)
        .bind(by)
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .map_err(pe)?;
        Ok(())
    }

    async fn create_invoice(
        &self,
        wallet_id: i64,
        new: NewInvoice,
        max_open: u32,
        derive: &(dyn Fn(u32) -> PayResult<String> + Send + Sync),
    ) -> PayResult<Invoice> {
        let mut tx = self.pool.begin().await.map_err(pe)?;
        let w = sqlx::query(
            "SELECT network, next_index, gap_limit FROM payment_wallets WHERE id = $1 FOR UPDATE",
        )
        .bind(wallet_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(pe)?
        .ok_or(PayError::NotFound)?;
        let network: String = w.try_get("network").map_err(pe)?;
        let next_index: i32 = w.try_get("next_index").map_err(pe)?;
        let gap: i32 = w.try_get("gap_limit").map_err(pe)?;
        let open: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM payment_invoices WHERE user_id = $1 \
             AND state IN ('open', 'seen', 'confirming', 'underpaid')",
        )
        .bind(new.user_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(pe)?;
        if open >= i64::from(max_open) {
            return Err(PayError::TooManyOpen);
        }
        let used_max: Option<i32> = sqlx::query_scalar(
            "SELECT max(i.derivation_index) FROM payment_invoices i WHERE i.wallet_id = $1 \
             AND EXISTS (SELECT 1 FROM payment_txs t WHERE t.invoice_id = i.id)",
        )
        .bind(wallet_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(pe)?;
        let run = next_index - used_max.map_or(0, |u| u + 1);
        if run >= gap {
            return Err(PayError::GapLimit);
        }
        let index = u32::try_from(next_index).unwrap_or(0);
        let address = derive(index)?;
        let row = dynq(format!(
            "INSERT INTO payment_invoices (id, wallet_id, user_id, purpose, guild_id, months, credits, \
             usd_cents, sats, rate_cents_per_btc, rate_source, rate_stale, address, derivation_index, network, \
             confirmations_required, shortfall_sats, anonymous, created_ms, expires_ms, quote_expires_ms, next_check_ms) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$9,$17,$18,$19,$20,$18) \
             RETURNING {INV_COLS}"
        ))
        .bind(new.id)
        .bind(wallet_id)
        .bind(new.user_id)
        .bind(new.purpose.as_str())
        .bind(new.guild_id)
        .bind(new.months.map(|m| i32::try_from(m).unwrap_or(1)))
        .bind(new.credits.map(|m| i32::try_from(m).unwrap_or(1)))
        .bind(new.usd_cents)
        .bind(new.sats)
        .bind(new.rate_cents_per_btc)
        .bind(&new.rate_source)
        .bind(new.rate_stale)
        .bind(&address)
        .bind(next_index)
        .bind(&network)
        .bind(i32::try_from(new.confirmations_required).unwrap_or(1))
        .bind(new.anonymous)
        .bind(new.created_ms)
        .bind(new.expires_ms)
        .bind(new.quote_expires_ms)
        .fetch_one(&mut *tx)
        .await
        .map_err(pe)?;
        sqlx::query("UPDATE payment_wallets SET next_index = next_index + 1 WHERE id = $1")
            .bind(wallet_id)
            .execute(&mut *tx)
            .await
            .map_err(pe)?;
        tx.commit().await.map_err(pe)?;
        invoice_from(&row)
    }

    async fn invoice(&self, id: i64) -> PayResult<Option<Invoice>> {
        let row = dynq(format!(
            "SELECT {INV_COLS} FROM payment_invoices WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(pe)?;
        row.as_ref().map(invoice_from).transpose()
    }

    async fn invoice_txs(&self, id: i64) -> PayResult<Vec<TxRecord>> {
        let rows = sqlx::query(
            "SELECT invoice_id, txid, amount_sats, block_height, confirmations, first_seen_ms, \
             last_seen_ms, dropped_ms FROM payment_txs WHERE invoice_id = $1 ORDER BY first_seen_ms, txid",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(pe)?;
        rows.iter().map(tx_from).collect()
    }

    async fn invoice_audit(&self, id: i64) -> PayResult<Vec<AuditRow>> {
        let rows = sqlx::query(
            "SELECT at_ms, kind, detail FROM payment_audit WHERE invoice_id = $1 ORDER BY id",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(pe)?;
        rows.iter()
            .map(|r| {
                Ok(AuditRow {
                    at_ms: r.try_get("at_ms").map_err(pe)?,
                    kind: r.try_get("kind").map_err(pe)?,
                    detail: r.try_get("detail").map_err(pe)?,
                })
            })
            .collect()
    }

    async fn user_invoices(
        &self,
        user_id: i64,
        before_id: Option<i64>,
        limit: u32,
    ) -> PayResult<Vec<Invoice>> {
        let rows = dynq(format!(
            "SELECT {INV_COLS} FROM payment_invoices WHERE user_id = $1 AND ($2::bigint IS NULL OR id < $2) \
             ORDER BY id DESC LIMIT $3"
        ))
        .bind(user_id)
        .bind(before_id)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(pe)?;
        rows.iter().map(invoice_from).collect()
    }

    async fn search_invoices(&self, f: &InvoiceFilter) -> PayResult<Vec<Invoice>> {
        let rows = dynq(format!(
            "SELECT {INV_COLS} FROM payment_invoices i WHERE \
             ($1::text IS NULL OR state = $1) AND ($2::text IS NULL OR purpose = $2) \
             AND ($3::bigint IS NULL OR user_id = $3) \
             AND (NOT $4 OR state IN ('underpaid', 'overpaid', 'late_paid')) \
             AND ($5::bigint IS NULL OR id < $5) \
             AND ($6::text IS NULL OR id::text = $6 OR address = $6 \
                  OR EXISTS (SELECT 1 FROM payment_txs t WHERE t.invoice_id = i.id AND t.txid LIKE $6 || '%')) \
             ORDER BY id DESC LIMIT $7"
        ))
        .bind(f.state.map(InvoiceState::as_str))
        .bind(f.purpose.map(Purpose::as_str))
        .bind(f.user_id)
        .bind(f.needs_attention)
        .bind(f.before_id)
        .bind(f.q.as_deref().map(|q| q.replace(['%', '_'], "")))
        .bind(i64::from(f.limit.clamp(1, 200)))
        .fetch_all(&self.pool)
        .await
        .map_err(pe)?;
        rows.iter().map(invoice_from).collect()
    }

    async fn due_invoices(&self, now_ms: i64, limit: u32) -> PayResult<Vec<Invoice>> {
        let cfg = self.config().await?;
        let finality =
            i32::try_from(cfg.finality_confirmations.max(cfg.confirmations)).unwrap_or(6);
        let late = i64::from(cfg.late_watch_days) * DAY_MS;
        let rows = dynq(format!(
            "SELECT {INV_COLS} FROM payment_invoices WHERE next_check_ms <= $1 \
             AND state NOT IN ('cancelled', 'resolved') \
             AND NOT (state IN ('expired', 'underpaid') AND expires_ms + $2 < $1) \
             AND NOT (state IN ('confirmed', 'overpaid', 'late_paid') AND COALESCE(confirmations, 0) >= $3) \
             ORDER BY next_check_ms LIMIT $4"
        ))
        .bind(now_ms)
        .bind(late)
        .bind(finality)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(pe)?;
        let v: Vec<Invoice> = rows.iter().map(invoice_from).collect::<PayResult<_>>()?;
        debug_assert!(v.iter().all(|i| !is_final(i, &cfg, now_ms)));
        Ok(v)
    }

    async fn record_observations(
        &self,
        invoice_id: i64,
        obs: &[ChainObs],
        now_ms: i64,
    ) -> PayResult<Vec<TxRecord>> {
        let mut tx = self.pool.begin().await.map_err(pe)?;
        for o in obs {
            let inserted: bool = sqlx::query_scalar(
                "INSERT INTO payment_txs (invoice_id, txid, amount_sats, block_height, confirmations, \
                 first_seen_ms, last_seen_ms) VALUES ($1, $2, $3, $4, $5, $6, $6) \
                 ON CONFLICT (invoice_id, txid) DO UPDATE SET amount_sats = $3, block_height = $4, \
                   confirmations = $5, last_seen_ms = $6, dropped_ms = NULL \
                 RETURNING (xmax = 0)",
            )
            .bind(invoice_id)
            .bind(&o.txid)
            .bind(o.amount_sats)
            .bind(o.block_height.map(|h| i32::try_from(h).unwrap_or(i32::MAX)))
            .bind(i32::try_from(o.confirmations).unwrap_or(i32::MAX))
            .bind(now_ms)
            .fetch_one(&mut *tx)
            .await
            .map_err(pe)?;
            if inserted {
                audit_in(
                    &mut tx,
                    invoice_id,
                    "tx_seen",
                    json!({ "txid": o.txid, "sats": o.amount_sats }),
                    now_ms,
                )
                .await?;
            }
        }
        let live: Vec<String> = obs.iter().map(|o| o.txid.clone()).collect();
        let dropped: Vec<String> = sqlx::query_scalar(
            "UPDATE payment_txs SET dropped_ms = $2, confirmations = 0, block_height = NULL \
             WHERE invoice_id = $1 AND dropped_ms IS NULL AND NOT (txid = ANY($3)) RETURNING txid",
        )
        .bind(invoice_id)
        .bind(now_ms)
        .bind(&live)
        .fetch_all(&mut *tx)
        .await
        .map_err(pe)?;
        for txid in dropped {
            audit_in(
                &mut tx,
                invoice_id,
                "tx_dropped",
                json!({ "txid": txid }),
                now_ms,
            )
            .await?;
        }
        let rows = sqlx::query(
            "SELECT invoice_id, txid, amount_sats, block_height, confirmations, first_seen_ms, \
             last_seen_ms, dropped_ms FROM payment_txs WHERE invoice_id = $1 ORDER BY first_seen_ms, txid",
        )
        .bind(invoice_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(pe)?;
        tx.commit().await.map_err(pe)?;
        rows.iter().map(tx_from).collect()
    }

    async fn save_evaluation(
        &self,
        invoice_id: i64,
        eval: &Evaluation,
        plan: &CreditPlan,
        next_check_ms: i64,
        now_ms: i64,
    ) -> PayResult<SaveOutcome> {
        let mut tx = self.pool.begin().await.map_err(pe)?;
        let inv = Self::lock_invoice(&mut tx, invoice_id).await?;
        if inv.state.is_closed() {
            return Ok(SaveOutcome {
                old_state: inv.state,
                invoice: inv,
                credit: CreditChange::None,
                changed: false,
            });
        }
        let active = Self::credit_active(&mut tx, invoice_id).await?;
        let mut credit = CreditChange::None;
        if eval.credited() && !active {
            self.apply_plan(&mut tx, invoice_id, plan, now_ms).await?;
            credit = CreditChange::Applied;
            audit_in(
                &mut tx,
                invoice_id,
                "credited",
                json!({ "state": eval.state.as_str() }),
                now_ms,
            )
            .await?;
        } else if !eval.credited() && active {
            self.revert_credit(&mut tx, invoice_id, now_ms).await?;
            credit = CreditChange::Reverted;
            audit_in(
                &mut tx,
                invoice_id,
                "reverted",
                json!({ "state": eval.state.as_str() }),
                now_ms,
            )
            .await?;
        }
        if inv.state != eval.state {
            audit_in(
                &mut tx,
                invoice_id,
                "state",
                json!({ "from": inv.state.as_str(), "to": eval.state.as_str() }),
                now_ms,
            )
            .await?;
        }
        let credited_ms: Option<i64> = match credit {
            CreditChange::Applied => Some(now_ms),
            CreditChange::Reverted => None,
            CreditChange::None => inv.credited_ms,
        };
        let row = dynq(format!(
            "UPDATE payment_invoices SET state = $2, received_sats = $3, confirmed_sats = $4, \
             confirmations = $5, late = $6, shortfall_sats = $7, excess_sats = $8, next_check_ms = $9, \
             credited_ms = $10 WHERE id = $1 RETURNING {INV_COLS}"
        ))
        .bind(invoice_id)
        .bind(eval.state.as_str())
        .bind(i64::try_from(eval.received_sats).unwrap_or(i64::MAX))
        .bind(i64::try_from(eval.confirmed_sats).unwrap_or(i64::MAX))
        .bind(eval.confirmations.map(|c| i32::try_from(c).unwrap_or(i32::MAX)))
        .bind(eval.late)
        .bind(i64::try_from(eval.shortfall_sats).unwrap_or(i64::MAX))
        .bind(i64::try_from(eval.excess_sats).unwrap_or(i64::MAX))
        .bind(next_check_ms)
        .bind(credited_ms)
        .fetch_one(&mut *tx)
        .await
        .map_err(pe)?;
        tx.commit().await.map_err(pe)?;
        let new = invoice_from(&row)?;
        let changed = new.state != inv.state
            || new.confirmations != inv.confirmations
            || new.received_sats != inv.received_sats
            || credit != CreditChange::None;
        Ok(SaveOutcome {
            old_state: inv.state,
            invoice: new,
            credit,
            changed,
        })
    }

    async fn resolve(
        &self,
        invoice_id: i64,
        kind: ResolutionKind,
        note: &str,
        by: i64,
        plan: &CreditPlan,
        now_ms: i64,
    ) -> PayResult<SaveOutcome> {
        let mut tx = self.pool.begin().await.map_err(pe)?;
        let inv = Self::lock_invoice(&mut tx, invoice_id).await?;
        if inv.state.is_closed() {
            return Err(PayError::Conflict("the invoice is already closed".into()));
        }
        let active = Self::credit_active(&mut tx, invoice_id).await?;
        let mut credit = CreditChange::None;
        match kind {
            ResolutionKind::MarkPaid if !active => {
                self.apply_plan(&mut tx, invoice_id, plan, now_ms).await?;
                credit = CreditChange::Applied;
            }
            ResolutionKind::RefundNoted | ResolutionKind::Dismiss if active => {
                self.revert_credit(&mut tx, invoice_id, now_ms).await?;
                credit = CreditChange::Reverted;
            }
            _ => {}
        }
        audit_in(
            &mut tx,
            invoice_id,
            "resolved",
            json!({ "kind": kind.as_str(), "by": by.to_string(), "note": note }),
            now_ms,
        )
        .await?;
        let credited_ms = match credit {
            CreditChange::Applied => Some(now_ms),
            CreditChange::Reverted => None,
            CreditChange::None => inv.credited_ms,
        };
        let row = dynq(format!(
            "UPDATE payment_invoices SET state = 'resolved', resolution_kind = $2, resolution_note = $3, \
             resolved_by = $4, resolved_ms = $5, credited_ms = $6, next_check_ms = $5 + 3600000 \
             WHERE id = $1 RETURNING {INV_COLS}"
        ))
        .bind(invoice_id)
        .bind(kind.as_str())
        .bind(note)
        .bind(by)
        .bind(now_ms)
        .bind(credited_ms)
        .fetch_one(&mut *tx)
        .await
        .map_err(pe)?;
        tx.commit().await.map_err(pe)?;
        Ok(SaveOutcome {
            old_state: inv.state,
            invoice: invoice_from(&row)?,
            credit,
            changed: true,
        })
    }

    async fn cancel(&self, invoice_id: i64, user_id: i64, now_ms: i64) -> PayResult<Invoice> {
        let mut tx = self.pool.begin().await.map_err(pe)?;
        let inv = Self::lock_invoice(&mut tx, invoice_id).await?;
        if inv.user_id != user_id {
            return Err(PayError::NotFound);
        }
        let has_tx: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM payment_txs WHERE invoice_id = $1 AND dropped_ms IS NULL)",
        )
        .bind(invoice_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(pe)?;
        if inv.state != InvoiceState::Open || has_tx {
            return Err(PayError::Conflict(
                "only an unpaid open invoice can be cancelled".into(),
            ));
        }
        let row = dynq(format!(
            "UPDATE payment_invoices SET state = 'cancelled' WHERE id = $1 RETURNING {INV_COLS}"
        ))
        .bind(invoice_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(pe)?;
        audit_in(&mut tx, invoice_id, "cancelled", json!({}), now_ms).await?;
        tx.commit().await.map_err(pe)?;
        invoice_from(&row)
    }

    async fn schedule_check(&self, invoice_id: i64, next_check_ms: i64) -> PayResult<()> {
        sqlx::query("UPDATE payment_invoices SET next_check_ms = $2 WHERE id = $1")
            .bind(invoice_id)
            .bind(next_check_ms)
            .execute(&self.pool)
            .await
            .map_err(pe)?;
        Ok(())
    }

    async fn audit(
        &self,
        invoice_id: i64,
        kind: &str,
        detail: Value,
        now_ms: i64,
    ) -> PayResult<()> {
        sqlx::query(
            "INSERT INTO payment_audit (invoice_id, at_ms, kind, detail) VALUES ($1, $2, $3, $4)",
        )
        .bind(invoice_id)
        .bind(now_ms)
        .bind(kind)
        .bind(detail)
        .execute(&self.pool)
        .await
        .map_err(pe)?;
        Ok(())
    }

    async fn user_patron(
        &self,
        user_id: i64,
        now_ms: i64,
        grace_days: u32,
    ) -> PayResult<PatronStatus> {
        let rows = dynq(format!(
            "SELECT {GRANT_COLS} FROM entitlement_grants WHERE kind = 'patron' AND user_id = $1 AND revoked_ms IS NULL"
        ))
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(pe)?;
        let grants: Vec<Grant> = rows.iter().map(grant_from).collect::<PayResult<_>>()?;
        Ok(patron_status(stack_patron(&grants), now_ms, grace_days))
    }

    async fn guild_entitlement(&self, guild_id: i64, now_ms: i64) -> PayResult<GuildEntitlement> {
        let cfg = self.config().await?;
        let row = sqlx::query(
            "SELECT level, below_since_ms FROM guild_support_levels WHERE guild_id = $1",
        )
        .bind(guild_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(pe)?;
        let (level, below): (i16, Option<i64>) = match &row {
            Some(r) => (
                r.try_get("level").map_err(pe)?,
                r.try_get("below_since_ms").map_err(pe)?,
            ),
            None => (0, None),
        };
        let pool: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(usd_cents), 0)::bigint FROM entitlement_grants WHERE kind = 'server_pool' \
             AND guild_id = $1 AND revoked_ms IS NULL AND starts_ms > $2",
        )
        .bind(guild_id)
        .bind(now_ms - i64::from(cfg.pool_window_days) * DAY_MS)
        .fetch_one(&self.pool)
        .await
        .map_err(pe)?;
        Ok(GuildEntitlement {
            guild_id,
            level: u8::try_from(level).unwrap_or(0),
            pool_cents: pool,
            grace_until_ms: below.map(|b| b + i64::from(cfg.grace_days) * DAY_MS),
        })
    }

    async fn grant(
        &self,
        spec: &GrantSpec,
        by: i64,
        permanent: bool,
        ends_ms: Option<i64>,
        reason: &str,
        now_ms: i64,
    ) -> PayResult<Grant> {
        let row = dynq(format!(
            "INSERT INTO entitlement_grants (id, kind, user_id, guild_id, source, source_id, days, ends_ms, \
             permanent, usd_cents, starts_ms, reason) VALUES ($1, $2, $3, $4, 'admin', $5, $6, $7, $8, $9, $10, $11) \
             RETURNING {GRANT_COLS}"
        ))
        .bind(self.id())
        .bind(spec.kind.as_str())
        .bind(spec.user_id)
        .bind(spec.guild_id)
        .bind(by)
        .bind(spec.days)
        .bind(ends_ms)
        .bind(permanent)
        .bind(spec.usd_cents)
        .bind(now_ms)
        .bind(reason)
        .fetch_one(&self.pool)
        .await
        .map_err(pe)?;
        grant_from(&row)
    }

    async fn revoke_grant(&self, id: i64, by: i64, reason: &str, now_ms: i64) -> PayResult<Grant> {
        let row = dynq(format!(
            "UPDATE entitlement_grants SET revoked_ms = $2, revoked_by = $3, reason = $4 \
             WHERE id = $1 AND revoked_ms IS NULL RETURNING {GRANT_COLS}"
        ))
        .bind(id)
        .bind(now_ms)
        .bind(by)
        .bind(reason)
        .fetch_optional(&self.pool)
        .await
        .map_err(pe)?;
        match row {
            Some(r) => grant_from(&r),
            None => {
                let exists: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM entitlement_grants WHERE id = $1)",
                )
                .bind(id)
                .fetch_one(&self.pool)
                .await
                .map_err(pe)?;
                Err(if exists {
                    PayError::Conflict("already revoked".into())
                } else {
                    PayError::NotFound
                })
            }
        }
    }

    async fn list_grants(&self, f: &GrantFilter) -> PayResult<Vec<Grant>> {
        let rows = dynq(format!(
            "SELECT {GRANT_COLS} FROM entitlement_grants WHERE ($1::text IS NULL OR kind = $1) \
             AND ($2::bigint IS NULL OR user_id = $2) AND ($3::bigint IS NULL OR guild_id = $3) \
             AND (NOT $4 OR revoked_ms IS NULL) AND ($5::bigint IS NULL OR id < $5) \
             ORDER BY id DESC LIMIT $6"
        ))
        .bind(f.kind.map(GrantKind::as_str))
        .bind(f.user_id)
        .bind(f.guild_id)
        .bind(f.active_only)
        .bind(f.before_id)
        .bind(i64::from(f.limit.clamp(1, 200)))
        .fetch_all(&self.pool)
        .await
        .map_err(pe)?;
        rows.iter().map(grant_from).collect()
    }

    async fn refresh_guild(
        &self,
        guild_id: i64,
        cfg: &DonationConfig,
        now_ms: i64,
    ) -> PayResult<Option<LevelChange>> {
        let mut tx = self.pool.begin().await.map_err(pe)?;
        sqlx::query(
            "INSERT INTO guild_support_levels (guild_id) VALUES ($1) ON CONFLICT DO NOTHING",
        )
        .bind(guild_id)
        .execute(&mut *tx)
        .await
        .map_err(pe)?;
        let row = sqlx::query(
            "SELECT level, below_since_ms FROM guild_support_levels WHERE guild_id = $1 FOR UPDATE",
        )
        .bind(guild_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(pe)?;
        let cur: i16 = row.try_get("level").map_err(pe)?;
        let below: Option<i64> = row.try_get("below_since_ms").map_err(pe)?;
        let pool: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(usd_cents), 0)::bigint FROM entitlement_grants WHERE kind = 'server_pool' \
             AND guild_id = $1 AND revoked_ms IS NULL AND starts_ms > $2",
        )
        .bind(guild_id)
        .bind(now_ms - i64::from(cfg.pool_window_days) * DAY_MS)
        .fetch_one(&mut *tx)
        .await
        .map_err(pe)?;
        let cur_u = u8::try_from(cur).unwrap_or(0);
        let (level, below) = next_level(
            cur_u,
            below,
            pool,
            cfg.level_thresholds_cents,
            cfg.grace_days,
            now_ms,
        );
        sqlx::query(
            "UPDATE guild_support_levels SET level = $2, below_since_ms = $3, updated_ms = $4 WHERE guild_id = $1",
        )
        .bind(guild_id)
        .bind(i16::from(level))
        .bind(below)
        .bind(now_ms)
        .execute(&mut *tx)
        .await
        .map_err(pe)?;
        // Keep the Discord-shaped `premium_tier` source in step.
        sqlx::query("UPDATE guilds SET boost_level = $2 WHERE id = $1 AND boost_level <> $2")
            .bind(guild_id)
            .bind(i16::from(level))
            .execute(&mut *tx)
            .await
            .map_err(pe)?;
        tx.commit().await.map_err(pe)?;
        Ok((level != cur_u).then_some(LevelChange {
            guild_id,
            old: cur_u,
            new: level,
        }))
    }

    async fn guilds_to_refresh(&self, _now_ms: i64) -> PayResult<Vec<i64>> {
        sqlx::query_scalar(
            "SELECT guild_id FROM guild_support_levels WHERE level > 0 OR below_since_ms IS NOT NULL \
             UNION SELECT guild_id FROM entitlement_grants WHERE kind = 'server_pool' AND revoked_ms IS NULL",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(pe)
    }

    async fn patron_transitions(
        &self,
        now_ms: i64,
        grace_days: u32,
    ) -> PayResult<Vec<PatronTransition>> {
        let rows = dynq(format!(
            "SELECT {GRANT_COLS} FROM entitlement_grants WHERE kind = 'patron' AND revoked_ms IS NULL"
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(pe)?;
        let mut by_user: HashMap<i64, Vec<Grant>> = HashMap::new();
        for r in &rows {
            let g = grant_from(r)?;
            if let Some(u) = g.user_id {
                by_user.entry(u).or_default().push(g);
            }
        }
        let notices: Vec<(i64, i16)> =
            sqlx::query("SELECT user_id, level FROM user_patron_notices")
                .fetch_all(&self.pool)
                .await
                .map_err(pe)?
                .iter()
                .map(|r| {
                    Ok((
                        r.try_get("user_id").map_err(pe)?,
                        r.try_get("level").map_err(pe)?,
                    ))
                })
                .collect::<PayResult<_>>()?;
        let have: HashMap<i64, u8> = notices
            .into_iter()
            .map(|(u, l)| (u, u8::try_from(l).unwrap_or(0)))
            .collect();
        let mut out = Vec::new();
        for (u, grants) in by_user {
            let want = notice_level(stack_patron(&grants), now_ms, grace_days);
            let had = have.get(&u).copied().unwrap_or(0);
            if want > had {
                out.push(PatronTransition {
                    user_id: u,
                    kind: match want {
                        1 => PatronTransitionKind::Reminder,
                        2 => PatronTransitionKind::EnteredGrace,
                        _ => PatronTransitionKind::Lapsed,
                    },
                });
            }
            if want != had {
                sqlx::query(
                    "INSERT INTO user_patron_notices (user_id, level) VALUES ($1, $2) \
                     ON CONFLICT (user_id) DO UPDATE SET level = $2",
                )
                .bind(u)
                .bind(i16::from(want))
                .execute(&self.pool)
                .await
                .map_err(pe)?;
            }
        }
        Ok(out)
    }

    async fn guild_access(&self, user_id: i64, guild_id: i64) -> PayResult<GuildAccess> {
        let row = sqlx::query(
            "SELECT (g.owner_id = $1) AS owner, EXISTS (SELECT 1 FROM guild_members m \
             WHERE m.guild_id = g.id AND m.user_id = $1) AS member FROM guilds g WHERE g.id = $2",
        )
        .bind(user_id)
        .bind(guild_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(pe)?;
        Ok(match row {
            Some(r) => GuildAccess {
                member: r.try_get("member").map_err(pe)?,
                owner: r.try_get("owner").map_err(pe)?,
            },
            None => GuildAccess {
                member: false,
                owner: false,
            },
        })
    }

    async fn donor_prefs(&self, user_id: i64) -> PayResult<DonorPrefs> {
        let row = sqlx::query(
            "SELECT hide_name, hide_amount, hide_badge, hide_contributions FROM donor_preferences WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(pe)?;
        Ok(match row {
            Some(r) => DonorPrefs {
                hide_name: r.try_get("hide_name").map_err(pe)?,
                hide_amount: r.try_get("hide_amount").map_err(pe)?,
                hide_badge: r.try_get("hide_badge").map_err(pe)?,
                hide_contributions: r.try_get("hide_contributions").map_err(pe)?,
            },
            None => DonorPrefs::default(),
        })
    }

    async fn set_donor_prefs(&self, user_id: i64, p: DonorPrefs) -> PayResult<()> {
        sqlx::query(
            "INSERT INTO donor_preferences (user_id, hide_name, hide_amount, hide_badge, hide_contributions) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (user_id) DO UPDATE SET hide_name = $2, hide_amount = $3, hide_badge = $4, \
             hide_contributions = $5",
        )
        .bind(user_id)
        .bind(p.hide_name)
        .bind(p.hide_amount)
        .bind(p.hide_badge)
        .bind(p.hide_contributions)
        .execute(&self.pool)
        .await
        .map_err(pe)?;
        Ok(())
    }

    async fn guild_hidden(&self, guild_id: i64) -> PayResult<bool> {
        Ok(sqlx::query_scalar::<_, bool>(
            "SELECT hidden_from_leaderboard FROM guild_support_levels WHERE guild_id = $1",
        )
        .bind(guild_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(pe)?
        .unwrap_or(false))
    }

    async fn set_guild_hidden(&self, guild_id: i64, hidden: bool) -> PayResult<()> {
        sqlx::query(
            "INSERT INTO guild_support_levels (guild_id, hidden_from_leaderboard) VALUES ($1, $2) \
             ON CONFLICT (guild_id) DO UPDATE SET hidden_from_leaderboard = $2",
        )
        .bind(guild_id)
        .bind(hidden)
        .execute(&self.pool)
        .await
        .map_err(pe)?;
        Ok(())
    }

    async fn leaderboard_donors(
        &self,
        since_ms: Option<i64>,
        limit: u32,
    ) -> PayResult<Vec<LeaderRow>> {
        let rows = sqlx::query(
            "SELECT i.user_id AS id, SUM(i.usd_cents)::bigint AS cents, \
                    COALESCE(p.hide_name, FALSE) AS hide_name, COALESCE(p.hide_amount, FALSE) AS hide_amount \
             FROM payment_invoices i JOIN payment_credits c ON c.invoice_id = i.id AND c.active \
             LEFT JOIN donor_preferences p ON p.user_id = i.user_id \
             WHERE i.purpose <> 'invite_credit' AND ($1::bigint IS NULL OR c.credited_ms >= $1) \
               AND NOT COALESCE(p.staff_hidden, FALSE) \
             GROUP BY i.user_id, p.hide_name, p.hide_amount ORDER BY cents DESC, i.user_id LIMIT $2",
        )
        .bind(since_ms)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(pe)?;
        self.leader_rows(rows, false).await
    }

    async fn leaderboard_servers(
        &self,
        since_ms: Option<i64>,
        limit: u32,
    ) -> PayResult<Vec<LeaderRow>> {
        let rows = sqlx::query(
            "SELECT g.guild_id AS id, SUM(g.usd_cents)::bigint AS cents, FALSE AS hide_name, FALSE AS hide_amount \
             FROM entitlement_grants g LEFT JOIN guild_support_levels l ON l.guild_id = g.guild_id \
             WHERE g.kind = 'server_pool' AND g.revoked_ms IS NULL AND ($1::bigint IS NULL OR g.starts_ms >= $1) \
               AND NOT COALESCE(l.hidden_from_leaderboard, FALSE) AND NOT COALESCE(l.staff_hidden, FALSE) \
             GROUP BY g.guild_id ORDER BY cents DESC, g.guild_id LIMIT $2",
        )
        .bind(since_ms)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(pe)?;
        self.leader_rows(rows, true).await
    }

    async fn leaderboard_contributors(
        &self,
        kind: ContributionKind,
        limit: u32,
    ) -> PayResult<Vec<ContributorRow>> {
        let sql = contributor_sql(kind, "count DESC, user_id LIMIT $1", false);
        let rows = dynq(sql)
            .bind(i64::from(limit))
            .fetch_all(&self.pool)
            .await
            .map_err(pe)?;
        rows.iter()
            .map(|r| {
                Ok(ContributorRow {
                    user_id: r.try_get("user_id").map_err(pe)?,
                    count: r.try_get("count").map_err(pe)?,
                })
            })
            .collect()
    }

    async fn contributor_rank(
        &self,
        user_id: i64,
        kind: ContributionKind,
    ) -> PayResult<Option<(u32, i64)>> {
        let sql = contributor_sql(kind, "rank LIMIT 1", true)
            .replace(") t ORDER BY", ") t WHERE user_id = $1 ORDER BY");
        let row = dynq(sql)
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(pe)?;
        match row {
            Some(r) => {
                let rank: i64 = r.try_get("rank").map_err(pe)?;
                Ok(Some((
                    u32::try_from(rank).unwrap_or(u32::MAX),
                    r.try_get("count").map_err(pe)?,
                )))
            }
            None => Ok(None),
        }
    }

    async fn donor_rank(
        &self,
        user_id: i64,
        since_ms: Option<i64>,
    ) -> PayResult<Option<(u32, i64)>> {
        let row = sqlx::query(
            "SELECT rank, cents FROM (SELECT i.user_id, SUM(i.usd_cents)::bigint AS cents, \
                    ROW_NUMBER() OVER (ORDER BY SUM(i.usd_cents) DESC, i.user_id) AS rank \
             FROM payment_invoices i JOIN payment_credits c ON c.invoice_id = i.id AND c.active \
             LEFT JOIN donor_preferences p ON p.user_id = i.user_id \
             WHERE i.purpose <> 'invite_credit' AND ($1::bigint IS NULL OR c.credited_ms >= $1) \
               AND NOT COALESCE(p.staff_hidden, FALSE) \
             GROUP BY i.user_id) t WHERE user_id = $2",
        )
        .bind(since_ms)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(pe)?;
        match row {
            Some(r) => {
                let rank: i64 = r.try_get("rank").map_err(pe)?;
                Ok(Some((
                    u32::try_from(rank).unwrap_or(u32::MAX),
                    r.try_get("cents").map_err(pe)?,
                )))
            }
            None => Ok(None),
        }
    }

    async fn top_contributors(
        &self,
        guild_id: i64,
        since_ms: i64,
        limit: u32,
    ) -> PayResult<Vec<LeaderRow>> {
        let rows = sqlx::query(
            "SELECT g.user_id AS id, SUM(g.usd_cents)::bigint AS cents, \
                    (COALESCE(bool_and(i.anonymous), FALSE) OR COALESCE(p.hide_name, FALSE)) AS hide_name, \
                    COALESCE(p.hide_amount, FALSE) AS hide_amount \
             FROM entitlement_grants g LEFT JOIN payment_invoices i ON g.source = 'invoice' AND i.id = g.source_id \
             LEFT JOIN donor_preferences p ON p.user_id = g.user_id \
             WHERE g.kind = 'server_pool' AND g.guild_id = $1 AND g.revoked_ms IS NULL AND g.starts_ms >= $2 \
               AND g.user_id IS NOT NULL \
             GROUP BY g.user_id, p.hide_name, p.hide_amount ORDER BY cents DESC, g.user_id LIMIT $3",
        )
        .bind(guild_id)
        .bind(since_ms)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(pe)?;
        self.leader_rows(rows, false).await.map(|mut v| {
            for r in &mut v {
                r.guild_id = Some(guild_id);
            }
            v
        })
    }

    async fn profiles(&self, ids: &[i64]) -> PayResult<Vec<Profile>> {
        let rows = sqlx::query(
            "SELECT id, COALESCE(display_name, username::text) AS name, avatar_key FROM accounts WHERE id = ANY($1)",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await
        .map_err(pe)?;
        rows.iter()
            .map(|r| {
                Ok(Profile {
                    id: r.try_get("id").map_err(pe)?,
                    name: r.try_get("name").map_err(pe)?,
                    avatar: r.try_get("avatar_key").map_err(pe)?,
                })
            })
            .collect()
    }

    async fn guild_profiles(&self, ids: &[i64]) -> PayResult<Vec<Profile>> {
        let rows = sqlx::query("SELECT id, name, icon_key FROM guilds WHERE id = ANY($1)")
            .bind(ids)
            .fetch_all(&self.pool)
            .await
            .map_err(pe)?;
        rows.iter()
            .map(|r| {
                Ok(Profile {
                    id: r.try_get("id").map_err(pe)?,
                    name: r.try_get("name").map_err(pe)?,
                    avatar: r.try_get("icon_key").map_err(pe)?,
                })
            })
            .collect()
    }

    async fn set_staff_hidden(
        &self,
        user_id: Option<i64>,
        guild_id: Option<i64>,
        hidden: bool,
    ) -> PayResult<()> {
        if let Some(u) = user_id {
            sqlx::query(
                "INSERT INTO donor_preferences (user_id, staff_hidden) VALUES ($1, $2) \
                 ON CONFLICT (user_id) DO UPDATE SET staff_hidden = $2",
            )
            .bind(u)
            .bind(hidden)
            .execute(&self.pool)
            .await
            .map_err(pe)?;
        }
        if let Some(g) = guild_id {
            sqlx::query(
                "INSERT INTO guild_support_levels (guild_id, staff_hidden) VALUES ($1, $2) \
                 ON CONFLICT (guild_id) DO UPDATE SET staff_hidden = $2",
            )
            .bind(g)
            .bind(hidden)
            .execute(&self.pool)
            .await
            .map_err(pe)?;
        }
        Ok(())
    }

    async fn donated_since(&self, since_ms: i64) -> PayResult<i64> {
        sqlx::query_scalar(
            "SELECT COALESCE(SUM(i.usd_cents), 0)::bigint FROM payment_invoices i \
             JOIN payment_credits c ON c.invoice_id = i.id AND c.active WHERE c.credited_ms >= $1",
        )
        .bind(since_ms)
        .fetch_one(&self.pool)
        .await
        .map_err(pe)
    }

    async fn report(&self, from_ms: i64, to_ms: i64) -> PayResult<Report> {
        let mut r = Report::default();
        let rows = sqlx::query(
            "SELECT i.purpose, SUM(i.usd_cents)::bigint AS cents, count(*)::bigint AS n \
             FROM payment_invoices i JOIN payment_credits c ON c.invoice_id = i.id AND c.active \
             WHERE i.created_ms >= $1 AND i.created_ms < $2 GROUP BY i.purpose ORDER BY i.purpose",
        )
        .bind(from_ms)
        .bind(to_ms)
        .fetch_all(&self.pool)
        .await
        .map_err(pe)?;
        for row in &rows {
            let cents: i64 = row.try_get("cents").map_err(pe)?;
            r.credited_cents += cents;
            r.by_purpose.push((
                row.try_get("purpose").map_err(pe)?,
                cents,
                row.try_get("n").map_err(pe)?,
            ));
        }
        let days = sqlx::query(
            "SELECT to_char(to_timestamp(c.credited_ms / 1000.0) AT TIME ZONE 'UTC', 'YYYY-MM-DD') AS day, \
                    SUM(i.usd_cents)::bigint AS cents \
             FROM payment_invoices i JOIN payment_credits c ON c.invoice_id = i.id AND c.active \
             WHERE i.created_ms >= $1 AND i.created_ms < $2 GROUP BY day ORDER BY day",
        )
        .bind(from_ms)
        .bind(to_ms)
        .fetch_all(&self.pool)
        .await
        .map_err(pe)?;
        for row in &days {
            r.by_day.push((
                row.try_get("day").map_err(pe)?,
                row.try_get("cents").map_err(pe)?,
            ));
        }
        let states = sqlx::query(
            "SELECT state, count(*)::bigint AS n FROM payment_invoices WHERE created_ms >= $1 AND created_ms < $2 \
             GROUP BY state ORDER BY state",
        )
        .bind(from_ms)
        .bind(to_ms)
        .fetch_all(&self.pool)
        .await
        .map_err(pe)?;
        for row in &states {
            let s: String = row.try_get("state").map_err(pe)?;
            if let Some(s) = InvoiceState::parse(&s) {
                r.by_state.push((s, row.try_get("n").map_err(pe)?));
            }
        }
        Ok(r)
    }

    async fn watcher_status(&self) -> PayResult<WatcherStatus> {
        let row = sqlx::query(
            "SELECT last_run_ms, last_ok_ms, last_error, tip_height, polled FROM payment_watcher WHERE id = 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(pe)?;
        Ok(match row {
            Some(r) => WatcherStatus {
                last_run_ms: r.try_get("last_run_ms").map_err(pe)?,
                last_ok_ms: r.try_get("last_ok_ms").map_err(pe)?,
                last_error: r.try_get("last_error").map_err(pe)?,
                tip_height: u32_of(r.try_get("tip_height").map_err(pe)?),
                polled: u32_of(r.try_get("polled").map_err(pe)?).unwrap_or(0),
            },
            None => WatcherStatus::default(),
        })
    }

    async fn set_watcher_status(&self, s: &WatcherStatus) -> PayResult<()> {
        sqlx::query(
            "INSERT INTO payment_watcher (id, last_run_ms, last_ok_ms, last_error, tip_height, polled) \
             VALUES (1, $1, $2, $3, $4, $5) ON CONFLICT (id) DO UPDATE SET last_run_ms = $1, \
               last_ok_ms = $2, last_error = $3, tip_height = $4, polled = $5",
        )
        .bind(s.last_run_ms)
        .bind(s.last_ok_ms)
        .bind(&s.last_error)
        .bind(s.tip_height.map(|t| i32::try_from(t).unwrap_or(i32::MAX)))
        .bind(i32::try_from(s.polled).unwrap_or(i32::MAX))
        .execute(&self.pool)
        .await
        .map_err(pe)?;
        Ok(())
    }
}
