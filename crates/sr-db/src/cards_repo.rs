//! Postgres implementation of [`sr_chat::cards::CardStore`] (UTL-CRD-001, migration 1670).
//! Tests: `tests/cards_repo.rs` (needs `SR_TEST_DATABASE_URL`).
//!
//! **Seats are claimed under the card's row lock.** `claim` opens a transaction, locks the `invite_cards` row
//! `FOR UPDATE`, counts the claims and inserts. Any number of concurrent callers queue on that lock, so the count
//! each one sees includes every earlier insert and the last seat goes to exactly one of them (a count-then-insert
//! without the lock would let two callers both see one free seat at READ COMMITTED).

use async_trait::async_trait;
use sqlx::{PgPool, Row, postgres::PgRow};
use sr_chat::cards::reminders::Window;
use sr_chat::cards::{
    CLOSE_AFTER_START_MS, Card, CardError, CardStore, Claim, ClaimOutcome, MAX_OPEN_PER_CHANNEL,
};
use sr_chat::repo::{StoreError, StoreResult};

#[derive(Clone)]
pub struct PgCards {
    pool: PgPool,
}

impl PgCards {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn sdb(e: sqlx::Error) -> StoreError {
    StoreError(e.to_string())
}

fn cerr(e: sqlx::Error) -> CardError {
    CardError::Store(e.to_string())
}

fn ts(ms: i64) -> f64 {
    ms as f64 / 1000.0
}

fn ms(col: &str) -> String {
    format!("(extract(epoch FROM {col}) * 1000)::bigint")
}

/// Card columns, qualified with the alias `c`.
fn card_cols() -> String {
    format!(
        "c.id, c.channel_id, c.guild_id, c.kind, c.title, c.note, {} AS starts_ms, c.duration_min, c.timezone, \
         c.seats, c.title_id, c.link_path, c.created_by, {} AS created_ms, {} AS canceled_ms, \
         {} AS started_ms, {} AS closed_ms",
        ms("c.starts_at"),
        ms("c.created_at"),
        ms("c.canceled_at"),
        ms("c.started_at"),
        ms("c.closed_at"),
    )
}

fn card(r: &PgRow) -> Card {
    Card {
        id: r.get::<i64, _>("id") as u64,
        channel_id: r.get::<i64, _>("channel_id") as u64,
        guild_id: r.get::<Option<i64>, _>("guild_id").map(|g| g as u64),
        kind: r.get("kind"),
        title: r.get("title"),
        note: r.get("note"),
        starts_ms: r.get("starts_ms"),
        duration_min: r.get("duration_min"),
        timezone: r.get("timezone"),
        seats: r.get("seats"),
        title_id: r.get::<Option<i64>, _>("title_id").map(|t| t as u64),
        link_path: r.get("link_path"),
        created_by: r.get::<Option<i64>, _>("created_by").map(|u| u as u64),
        created_ms: r.get("created_ms"),
        canceled_ms: r.get("canceled_ms"),
        started_ms: r.get("started_ms"),
        closed_ms: r.get("closed_ms"),
    }
}

fn claim_cols() -> String {
    format!(
        "k.card_id, k.user_id, {} AS claimed_ms, k.remind, \
         (k.reminded_1h_at IS NOT NULL) AS reminded_hour, (k.reminded_10m_at IS NOT NULL) AS reminded_ten",
        ms("k.claimed_at")
    )
}

fn claim(r: &PgRow) -> Claim {
    Claim {
        card_id: r.get::<i64, _>("card_id") as u64,
        user_id: r.get::<i64, _>("user_id") as u64,
        claimed_ms: r.get("claimed_ms"),
        remind: r.get("remind"),
        reminded_hour: r.get("reminded_hour"),
        reminded_ten: r.get("reminded_ten"),
    }
}

#[async_trait]
impl CardStore for PgCards {
    async fn create(&self, c: &Card) -> Result<(), CardError> {
        let mut tx = self.pool.begin().await.map_err(cerr)?;
        // Serialise creators per channel so the cap holds under concurrency.
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(c.channel_id as i64)
            .execute(&mut *tx)
            .await
            .map_err(cerr)?;
        let open: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM invite_cards \
             WHERE channel_id = $1 AND canceled_at IS NULL AND closed_at IS NULL",
        )
        .bind(c.channel_id as i64)
        .fetch_one(&mut *tx)
        .await
        .map_err(cerr)?;
        if open as usize >= MAX_OPEN_PER_CHANNEL {
            return Err(CardError::TooMany);
        }
        sqlx::query(
            "INSERT INTO invite_cards (id, channel_id, guild_id, kind, title, note, starts_at, duration_min, \
             timezone, seats, title_id, link_path, created_by, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, to_timestamp($7), $8, $9, $10, $11, $12, $13, to_timestamp($14))",
        )
        .bind(c.id as i64)
        .bind(c.channel_id as i64)
        .bind(c.guild_id.map(|g| g as i64))
        .bind(&c.kind)
        .bind(&c.title)
        .bind(&c.note)
        .bind(ts(c.starts_ms))
        .bind(c.duration_min)
        .bind(&c.timezone)
        .bind(c.seats)
        .bind(c.title_id.map(|t| t as i64))
        .bind(&c.link_path)
        .bind(c.created_by.map(|u| u as i64))
        .bind(ts(c.created_ms))
        .execute(&mut *tx)
        .await
        .map_err(cerr)?;
        tx.commit().await.map_err(cerr)?;
        Ok(())
    }

    async fn get(&self, channel: u64, id: u64) -> StoreResult<Option<Card>> {
        let sql = format!(
            "SELECT {} FROM invite_cards c WHERE c.id = $1 AND c.channel_id = $2",
            card_cols()
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id as i64)
            .bind(channel as i64)
            .fetch_optional(&self.pool)
            .await
            .map_err(sdb)?;
        Ok(row.as_ref().map(card))
    }

    async fn get_any(&self, id: u64) -> StoreResult<Option<Card>> {
        let sql = format!("SELECT {} FROM invite_cards c WHERE c.id = $1", card_cols());
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id as i64)
            .fetch_optional(&self.pool)
            .await
            .map_err(sdb)?;
        Ok(row.as_ref().map(card))
    }

    async fn claim(
        &self,
        card_id: u64,
        user: u64,
        remind: bool,
        now_ms: i64,
    ) -> StoreResult<ClaimOutcome> {
        let mut tx = self.pool.begin().await.map_err(sdb)?;
        let row = sqlx::query(
            "SELECT seats, (canceled_at IS NOT NULL OR closed_at IS NOT NULL) AS shut \
             FROM invite_cards WHERE id = $1 FOR UPDATE",
        )
        .bind(card_id as i64)
        .fetch_optional(&mut *tx)
        .await
        .map_err(sdb)?;
        let Some(row) = row else {
            return Ok(ClaimOutcome::Closed);
        };
        if row.get::<bool, _>("shut") {
            return Ok(ClaimOutcome::Closed);
        }
        let seats: i32 = row.get("seats");
        let existing = sqlx::query(
            "UPDATE invite_card_claims SET remind = $3 WHERE card_id = $1 AND user_id = $2",
        )
        .bind(card_id as i64)
        .bind(user as i64)
        .bind(remind)
        .execute(&mut *tx)
        .await
        .map_err(sdb)?;
        if existing.rows_affected() > 0 {
            tx.commit().await.map_err(sdb)?;
            return Ok(ClaimOutcome::AlreadyClaimed);
        }
        let taken: i64 =
            sqlx::query_scalar("SELECT count(*) FROM invite_card_claims WHERE card_id = $1")
                .bind(card_id as i64)
                .fetch_one(&mut *tx)
                .await
                .map_err(sdb)?;
        if taken >= i64::from(seats) {
            return Ok(ClaimOutcome::Full);
        }
        sqlx::query(
            "INSERT INTO invite_card_claims (card_id, user_id, claimed_at, remind) \
             VALUES ($1, $2, to_timestamp($3), $4)",
        )
        .bind(card_id as i64)
        .bind(user as i64)
        .bind(ts(now_ms))
        .bind(remind)
        .execute(&mut *tx)
        .await
        .map_err(sdb)?;
        tx.commit().await.map_err(sdb)?;
        Ok(ClaimOutcome::Claimed)
    }

    async fn unclaim(&self, card_id: u64, user: u64) -> StoreResult<bool> {
        let done =
            sqlx::query("DELETE FROM invite_card_claims WHERE card_id = $1 AND user_id = $2")
                .bind(card_id as i64)
                .bind(user as i64)
                .execute(&self.pool)
                .await
                .map_err(sdb)?;
        Ok(done.rows_affected() > 0)
    }

    async fn claims(&self, card_id: u64) -> StoreResult<Vec<Claim>> {
        let sql = format!(
            "SELECT {} FROM invite_card_claims k WHERE k.card_id = $1 ORDER BY k.claimed_at, k.user_id",
            claim_cols()
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(card_id as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(sdb)?;
        Ok(rows.iter().map(claim).collect())
    }

    async fn cancel(&self, channel: u64, id: u64, now_ms: i64) -> StoreResult<bool> {
        let done = sqlx::query(
            "UPDATE invite_cards SET canceled_at = to_timestamp($3) \
             WHERE id = $1 AND channel_id = $2 AND canceled_at IS NULL AND closed_at IS NULL",
        )
        .bind(id as i64)
        .bind(channel as i64)
        .bind(ts(now_ms))
        .execute(&self.pool)
        .await
        .map_err(sdb)?;
        Ok(done.rows_affected() > 0)
    }

    async fn take_started(&self, now_ms: i64, limit: i64) -> StoreResult<Vec<Card>> {
        let sql = format!(
            "UPDATE invite_cards c SET started_at = to_timestamp($1) WHERE c.id IN (                  SELECT id FROM invite_cards WHERE started_at IS NULL AND canceled_at IS NULL                  AND closed_at IS NULL AND starts_at <= to_timestamp($1)                  ORDER BY starts_at LIMIT $2 FOR UPDATE SKIP LOCKED)              RETURNING {}",
            card_cols()
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(ts(now_ms))
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(sdb)?;
        let mut cards: Vec<Card> = rows.iter().map(card).collect();
        cards.sort_by_key(|c| c.starts_ms);
        Ok(cards)
    }

    async fn take_closed(&self, now_ms: i64, limit: i64) -> StoreResult<Vec<Card>> {
        let sql = format!(
            "UPDATE invite_cards c SET closed_at = to_timestamp($3),                  started_at = COALESCE(c.started_at, to_timestamp($3)) WHERE c.id IN (                  SELECT id FROM invite_cards WHERE closed_at IS NULL AND canceled_at IS NULL                  AND starts_at <= to_timestamp($1)                  ORDER BY starts_at LIMIT $2 FOR UPDATE SKIP LOCKED)              RETURNING {}",
            card_cols()
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(ts(now_ms - CLOSE_AFTER_START_MS))
            .bind(limit)
            .bind(ts(now_ms))
            .fetch_all(&self.pool)
            .await
            .map_err(sdb)?;
        let mut cards: Vec<Card> = rows.iter().map(card).collect();
        cards.sort_by_key(|c| c.starts_ms);
        Ok(cards)
    }

    async fn reminder_candidates(
        &self,
        now_ms: i64,
        limit: i64,
    ) -> StoreResult<Vec<(Card, Claim)>> {
        let sql = format!(
            "SELECT {}, {} FROM invite_card_claims k JOIN invite_cards c ON c.id = k.card_id \
             WHERE k.remind AND (k.reminded_1h_at IS NULL OR k.reminded_10m_at IS NULL) \
               AND c.canceled_at IS NULL AND c.closed_at IS NULL \
               AND c.starts_at > to_timestamp($1) AND c.starts_at <= to_timestamp($1) + interval '1 hour' \
             ORDER BY c.starts_at, k.user_id LIMIT $2",
            card_cols(),
            claim_cols()
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(ts(now_ms))
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(sdb)?;
        Ok(rows.iter().map(|r| (card(r), claim(r))).collect())
    }

    async fn mark_reminded(
        &self,
        card_id: u64,
        user: u64,
        window: Window,
        now_ms: i64,
    ) -> StoreResult<bool> {
        let sql = match window {
            Window::Hour => {
                "UPDATE invite_card_claims SET reminded_1h_at = to_timestamp($3) \
                 WHERE card_id = $1 AND user_id = $2 AND reminded_1h_at IS NULL"
            }
            Window::TenMinutes => {
                "UPDATE invite_card_claims SET reminded_10m_at = to_timestamp($3), \
                 reminded_1h_at = COALESCE(reminded_1h_at, to_timestamp($3)) \
                 WHERE card_id = $1 AND user_id = $2 AND reminded_10m_at IS NULL"
            }
        };
        let done = sqlx::query(sql)
            .bind(card_id as i64)
            .bind(user as i64)
            .bind(ts(now_ms))
            .execute(&self.pool)
            .await
            .map_err(sdb)?;
        Ok(done.rows_affected() > 0)
    }

    async fn unmark_reminded(&self, card_id: u64, user: u64, window: Window) -> StoreResult<()> {
        let sql = match window {
            Window::Hour => {
                "UPDATE invite_card_claims SET reminded_1h_at = NULL WHERE card_id = $1 AND user_id = $2"
            }
            Window::TenMinutes => {
                "UPDATE invite_card_claims SET reminded_10m_at = NULL WHERE card_id = $1 AND user_id = $2"
            }
        };
        sqlx::query(sql)
            .bind(card_id as i64)
            .bind(user as i64)
            .execute(&self.pool)
            .await
            .map_err(sdb)?;
        Ok(())
    }
}
