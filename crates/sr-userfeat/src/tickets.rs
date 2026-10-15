//! FTR-TICK-001 ticket threads. Opening a ticket starts a private thread in the server's ticket channel (the API
//! layer does that with the ordinary thread service, as the member) and records it here; staff close it, which
//! archives and locks the thread. This module owns the bookkeeping and the limits.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

use crate::error::{UfError, UfResult, invalid};
use crate::{UserFeatures, features::TICKETS_ID};

pub const MAX_SUBJECT_CHARS: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow, ToSchema)]
pub struct Ticket {
    pub id: String,
    pub opener_id: String,
    /// The private thread, a channel snowflake.
    pub thread_id: String,
    pub subject: String,
    /// `open` or `closed`.
    pub status: String,
    pub closed_by: Option<String>,
    pub closed_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct TicketInput {
    pub subject: String,
}

macro_rules! cols {
    () => {
        concat!(
            "id::text AS id, opener_id::text AS opener_id, thread_id::text AS thread_id, subject, status, \
             closed_by::text AS closed_by, ",
            iso!("closed_at"),
            " AS closed_at, ",
            iso!("created_at"),
            " AS created_at"
        )
    };
}

pub fn clean_subject(subject: &str) -> UfResult<String> {
    let s = subject.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.is_empty() {
        return Err(invalid("subject must not be empty"));
    }
    if s.chars().count() > MAX_SUBJECT_CHARS {
        return Err(invalid(format!(
            "subject must be {MAX_SUBJECT_CHARS} characters or fewer"
        )));
    }
    Ok(s)
}

impl UserFeatures {
    /// Check that `opener` may open another ticket in `guild` and normalise the subject.
    pub async fn check_can_open_ticket(
        &self,
        guild: i64,
        opener: i64,
        subject: &str,
    ) -> UfResult<String> {
        self.require(TICKETS_ID, Some(guild)).await?;
        let cfg = self.settings(TICKETS_ID, Some(guild)).await?;
        if cfg.id("channel").is_none() {
            return Err(UfError::Conflict(
                "No ticket channel has been set up.".into(),
            ));
        }
        let subject = clean_subject(subject)?;
        let open: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM tickets WHERE guild_id = $1 AND opener_id = $2 AND status = 'open'",
        )
        .bind(guild)
        .bind(opener)
        .fetch_one(&self.pool)
        .await?;
        let max = cfg.int("max_open_per_user", 1);
        if open >= max {
            return Err(UfError::Conflict(format!(
                "You already have {open} open ticket(s). Close one before opening another."
            )));
        }
        Ok(subject)
    }

    /// Record a ticket whose thread exists.
    pub async fn record_ticket(
        &self,
        guild: i64,
        opener: i64,
        thread: i64,
        subject: &str,
    ) -> UfResult<Ticket> {
        Ok(sqlx::query_as::<_, Ticket>(concat!(
            "INSERT INTO tickets (id, guild_id, opener_id, thread_id, subject) \
             VALUES ($1, $2, $3, $4, $5) RETURNING ",
            cols!()
        ))
        .bind(self.next_id())
        .bind(guild)
        .bind(opener)
        .bind(thread)
        .bind(subject)
        .fetch_one(&self.pool)
        .await?)
    }

    /// Tickets of a server, newest first. `opener` limits the list to one member's tickets.
    pub async fn list_tickets(
        &self,
        guild: i64,
        opener: Option<i64>,
        status: Option<&str>,
        limit: i64,
    ) -> UfResult<Vec<Ticket>> {
        self.require(TICKETS_ID, Some(guild)).await?;
        Ok(sqlx::query_as::<_, Ticket>(concat!(
            "SELECT ",
            cols!(),
            " FROM tickets WHERE guild_id = $1 AND ($2::bigint IS NULL OR opener_id = $2) \
             AND ($3::text IS NULL OR status = $3) ORDER BY id DESC LIMIT $4"
        ))
        .bind(guild)
        .bind(opener)
        .bind(status)
        .bind(limit.clamp(1, 100))
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_ticket(&self, guild: i64, id: i64) -> UfResult<Ticket> {
        sqlx::query_as::<_, Ticket>(concat!(
            "SELECT ",
            cols!(),
            " FROM tickets WHERE guild_id = $1 AND id = $2"
        ))
        .bind(guild)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(UfError::NotFound("ticket"))
    }

    /// Mark a ticket closed. Closing a closed ticket changes nothing.
    pub async fn close_ticket(&self, guild: i64, id: i64, by: i64) -> UfResult<Ticket> {
        let row = sqlx::query_as::<_, Ticket>(concat!(
            "UPDATE tickets SET status = 'closed', closed_by = COALESCE(closed_by, $3), \
             closed_at = COALESCE(closed_at, now()) WHERE guild_id = $1 AND id = $2 RETURNING ",
            cols!()
        ))
        .bind(guild)
        .bind(id)
        .bind(by)
        .fetch_optional(&self.pool)
        .await?;
        row.ok_or(UfError::NotFound("ticket"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_is_normalised_and_bounded() {
        assert_eq!(
            clean_subject("  need   help\nplease ").unwrap(),
            "need help please"
        );
        assert!(clean_subject("   ").is_err());
        assert!(clean_subject(&"x".repeat(101)).is_err());
        assert!(clean_subject(&"x".repeat(100)).is_ok());
    }
}
