//! FTR-SCH-001 scheduled messages: compose now, send later as the author.
//!
//! A row is `pending` until the tick (see [`crate::tick`]) sends it through [`crate::Poster::send_as`], which
//! applies the author's permissions at send time (a member who lost access meanwhile gets a failed row, not a
//! post). The author lists, edits and cancels their own rows; nobody else can see them.

use serde::Serialize;
use sqlx::FromRow;
use utoipa::ToSchema;

use crate::error::{UfError, UfResult, invalid};
use crate::{UserFeatures, features::SCHEDULED_ID};

/// Longest scheduled message (Discord's limit for ordinary messages).
pub const MAX_CONTENT_CHARS: usize = 2000;
/// A message must be at least this far in the future (a scheduled message is not an instant send).
pub const MIN_LEAD_MS: i64 = 60_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow, ToSchema)]
pub struct ScheduledMessage {
    pub id: String,
    pub channel_id: String,
    pub guild_id: Option<String>,
    pub content: String,
    /// ISO 8601 time the message goes out.
    pub send_at: String,
    /// `pending`, `sent`, `failed` or `cancelled`.
    pub status: String,
    /// Why it failed, for a failed row.
    pub failure: Option<String>,
    /// The sent message, for a sent row.
    pub message_id: Option<String>,
    pub created_at: String,
}

macro_rules! cols {
    () => {
        concat!(
            "id::text AS id, channel_id::text AS channel_id, guild_id::text AS guild_id, content, ",
            iso!("send_at"),
            " AS send_at, status, failure, message_id::text AS message_id, ",
            iso!("created_at"),
            " AS created_at"
        )
    };
}

/// A pending row that is due.
#[derive(Debug, Clone, FromRow)]
pub struct DueScheduled {
    pub id: i64,
    pub author_id: i64,
    pub channel_id: i64,
    pub guild_id: Option<i64>,
    pub content: String,
    /// Milliseconds past the due time (to give up on rows that cannot be delivered for an hour).
    pub late_ms: i64,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Validate and normalise the content of a scheduled message.
pub fn clean_content(content: &str) -> UfResult<String> {
    let c = content.trim();
    if c.is_empty() {
        return Err(invalid("content must not be empty"));
    }
    if c.chars().count() > MAX_CONTENT_CHARS {
        return Err(invalid(format!(
            "content must be {MAX_CONTENT_CHARS} characters or fewer"
        )));
    }
    Ok(c.to_owned())
}

/// Check a send time against the allowed window.
pub fn check_send_at(send_at_ms: i64, now: i64, max_days: i64) -> UfResult<()> {
    if send_at_ms < now + MIN_LEAD_MS {
        return Err(invalid("send_at must be at least a minute from now"));
    }
    if send_at_ms > now + max_days.max(1) * 86_400_000 {
        return Err(invalid(format!(
            "send_at must be within {} days",
            max_days.max(1)
        )));
    }
    Ok(())
}

impl UserFeatures {
    /// Schedule `content` for `channel` at `send_at_ms`. `guild` decides which server's switch and limits apply.
    pub async fn schedule_message(
        &self,
        author: i64,
        channel: i64,
        guild: Option<i64>,
        content: &str,
        send_at_ms: i64,
    ) -> UfResult<ScheduledMessage> {
        self.require(SCHEDULED_ID, guild).await?;
        let cfg = self.settings(SCHEDULED_ID, guild).await?;
        let content = clean_content(content)?;
        check_send_at(send_at_ms, now_ms(), cfg.int("max_days_ahead", 365))?;
        let max = cfg.int("max_pending", 25);
        let pending: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM scheduled_messages WHERE author_id = $1 AND status = 'pending'",
        )
        .bind(author)
        .fetch_one(&self.pool)
        .await?;
        if pending >= max {
            return Err(UfError::Conflict(format!(
                "You can have {max} scheduled messages at a time."
            )));
        }
        let id = self.next_id();
        let row = sqlx::query_as::<_, ScheduledMessage>(concat!(
            "INSERT INTO scheduled_messages (id, author_id, channel_id, guild_id, content, send_at) \
             VALUES ($1, $2, $3, $4, $5, to_timestamp($6::double precision / 1000.0)) RETURNING ",
            cols!()
        ))
        .bind(id)
        .bind(author)
        .bind(channel)
        .bind(guild)
        .bind(&content)
        .bind(send_at_ms)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// The author's scheduled messages, soonest first; `all` includes sent, failed and cancelled rows from the
    /// last week.
    pub async fn list_scheduled(&self, author: i64, all: bool) -> UfResult<Vec<ScheduledMessage>> {
        let rows = sqlx::query_as::<_, ScheduledMessage>(concat!(
            "SELECT ",
            cols!(),
            " FROM scheduled_messages WHERE author_id = $1 AND \
             (status = 'pending' OR ($2 AND updated_at > now() - interval '7 days')) \
             ORDER BY send_at, id LIMIT 200"
        ))
        .bind(author)
        .bind(all)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn get_scheduled(&self, author: i64, id: i64) -> UfResult<ScheduledMessage> {
        sqlx::query_as::<_, ScheduledMessage>(concat!(
            "SELECT ",
            cols!(),
            " FROM scheduled_messages WHERE id = $1 AND author_id = $2"
        ))
        .bind(id)
        .bind(author)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(UfError::NotFound("scheduled message"))
    }

    /// Change the text and/or time of a pending message.
    pub async fn edit_scheduled(
        &self,
        author: i64,
        id: i64,
        content: Option<&str>,
        send_at_ms: Option<i64>,
    ) -> UfResult<ScheduledMessage> {
        let current = self.get_scheduled(author, id).await?;
        if current.status != "pending" {
            return Err(UfError::Conflict(
                "Only a message that has not been sent yet can be changed.".into(),
            ));
        }
        let guild = current.guild_id.as_deref().and_then(|g| g.parse().ok());
        let cfg = self.settings(SCHEDULED_ID, guild).await?;
        let content = content.map(clean_content).transpose()?;
        if let Some(at) = send_at_ms {
            check_send_at(at, now_ms(), cfg.int("max_days_ahead", 365))?;
        }
        sqlx::query_as::<_, ScheduledMessage>(concat!(
            "UPDATE scheduled_messages SET content = COALESCE($3, content), \
             send_at = COALESCE(to_timestamp($4::double precision / 1000.0), send_at), updated_at = now() \
             WHERE id = $1 AND author_id = $2 AND status = 'pending' RETURNING ",
            cols!()
        ))
        .bind(id)
        .bind(author)
        .bind(content)
        .bind(send_at_ms)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| UfError::Conflict("The message was sent before the change landed.".into()))
    }

    /// Cancel a pending message. A message that has already gone out cannot be cancelled.
    pub async fn cancel_scheduled(&self, author: i64, id: i64) -> UfResult<()> {
        let n = sqlx::query(
            "UPDATE scheduled_messages SET status = 'cancelled', updated_at = now() \
             WHERE id = $1 AND author_id = $2 AND status = 'pending'",
        )
        .bind(id)
        .bind(author)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if n == 1 {
            return Ok(());
        }
        // Not pending: unknown, or already settled.
        let current = self.get_scheduled(author, id).await?;
        if current.status == "cancelled" {
            Ok(())
        } else {
            Err(UfError::Conflict(
                "That message has already been handled.".into(),
            ))
        }
    }

    /// Pending messages whose time has come, oldest first.
    pub async fn due_scheduled(&self, limit: i64) -> UfResult<Vec<DueScheduled>> {
        Ok(sqlx::query_as::<_, DueScheduled>(
            "SELECT id, author_id, channel_id, guild_id, content, \
             (extract(epoch from now() - send_at) * 1000)::bigint AS late_ms \
             FROM scheduled_messages WHERE status = 'pending' AND send_at <= now() \
             ORDER BY send_at, id LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Record the outcome of a send. Only a still-pending row changes, so a cancel that raced the send wins
    /// only if it landed first.
    pub async fn settle_scheduled(
        &self,
        id: i64,
        message_id: Option<i64>,
        failure: Option<&str>,
    ) -> UfResult<bool> {
        let status = if message_id.is_some() {
            "sent"
        } else {
            "failed"
        };
        Ok(sqlx::query(
            "UPDATE scheduled_messages SET status = $2, message_id = $3, failure = $4, \
             sent_at = CASE WHEN $2 = 'sent' THEN now() END, updated_at = now() \
             WHERE id = $1 AND status = 'pending'",
        )
        .bind(id)
        .bind(status)
        .bind(message_id)
        .bind(failure)
        .execute(&self.pool)
        .await?
        .rows_affected()
            == 1)
    }

    /// Push a pending row back by `delay_ms` (a transient delivery failure).
    pub async fn postpone_scheduled(&self, id: i64, delay_ms: i64) -> UfResult<()> {
        sqlx::query(
            "UPDATE scheduled_messages SET send_at = now() + make_interval(secs => $2::double precision / 1000.0), \
             updated_at = now() WHERE id = $1 AND status = 'pending'",
        )
        .bind(id)
        .bind(delay_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_is_trimmed_and_bounded() {
        assert_eq!(clean_content("  hi  ").unwrap(), "hi");
        assert!(clean_content("   ").is_err());
        assert!(clean_content(&"x".repeat(2001)).is_err());
        assert!(clean_content(&"x".repeat(2000)).is_ok());
    }

    #[test]
    fn send_time_window() {
        let now = 1_000_000;
        assert!(check_send_at(now + 30_000, now, 365).is_err());
        assert!(check_send_at(now + 61_000, now, 365).is_ok());
        assert!(check_send_at(now + 366 * 86_400_000, now, 365).is_err());
        assert!(check_send_at(now + 8 * 86_400_000, now, 7).is_err());
    }
}
