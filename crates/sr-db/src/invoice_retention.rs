//! Detaching old invoices from the member (N-08, D-325, doc 09). An invoice is kept for
//! `compliance.invoice_retention_years` after the end of the year it was settled in; then its
//! `user_id` is set to NULL (the row, amounts, dates and chain address stay for the books).
//! Invoices still in flight (`open`, `seen`, `confirming`, `underpaid`) are never detached.

use sqlx::PgPool;

use crate::DbResult;

/// States that are final, so the retention clock can run.
pub const FINAL_STATES: [&str; 6] = [
    "confirmed",
    "overpaid",
    "late_paid",
    "resolved",
    "expired",
    "cancelled",
];

/// Detach every final invoice whose reference time (credited, else resolved, else created) lies before
/// 1 January of `now.year - years`. Returns how many rows changed.
pub async fn detach_expired(pool: &PgPool, years: u8, now_ms: i64) -> DbResult<u64> {
    let r = sqlx::query(
        "UPDATE payment_invoices SET user_id = NULL \
         WHERE user_id IS NOT NULL AND state = ANY($1) \
           AND COALESCE(credited_ms, resolved_ms, created_ms) < \
               (EXTRACT(EPOCH FROM date_trunc('year', to_timestamp($2::double precision / 1000.0)) \
                                   - make_interval(years => $3)) * 1000)::bigint",
    )
    .bind(
        FINAL_STATES
            .iter()
            .map(|s| (*s).to_owned())
            .collect::<Vec<_>>(),
    )
    .bind(now_ms)
    .bind(i32::from(years))
    .execute(pool)
    .await?;
    Ok(r.rows_affected())
}
