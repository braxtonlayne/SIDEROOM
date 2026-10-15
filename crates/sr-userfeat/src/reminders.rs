//! FTR-REM-001 reminders: "remind me" on a message, or `/remind 2h note`. A due reminder is delivered as a DM from
//! the system user (so it reaches a closed browser too); the web client still shows a toast while it is open.

use serde::Serialize;
use sqlx::FromRow;
use utoipa::ToSchema;

use crate::error::{UfError, UfResult, invalid};
use crate::scheduled::now_ms;
use crate::{UserFeatures, features::REMINDERS_ID};

/// Shortest and longest time ahead.
pub const MIN_LEAD_MS: i64 = 5_000;
pub const MAX_LEAD_MS: i64 = 365 * 86_400_000;
pub const MAX_NOTE_CHARS: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow, ToSchema)]
pub struct Reminder {
    pub id: String,
    pub channel_id: Option<String>,
    pub guild_id: Option<String>,
    pub message_id: Option<String>,
    pub note: String,
    /// ISO 8601 time the reminder is due.
    pub remind_at: String,
    /// `pending`, `sent`, `failed` or `cancelled`.
    pub status: String,
    pub created_at: String,
}

macro_rules! cols {
    () => {
        concat!(
            "id::text AS id, channel_id::text AS channel_id, guild_id::text AS guild_id, \
             message_id::text AS message_id, note, ",
            iso!("remind_at"),
            " AS remind_at, status, ",
            iso!("created_at"),
            " AS created_at"
        )
    };
}

/// What a reminder points at.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Target {
    pub channel: Option<i64>,
    pub guild: Option<i64>,
    pub message: Option<i64>,
}

#[derive(Debug, Clone, FromRow)]
pub struct DueReminder {
    pub id: i64,
    pub user_id: i64,
    pub channel_id: Option<i64>,
    pub guild_id: Option<i64>,
    pub message_id: Option<i64>,
    pub note: String,
    pub late_ms: i64,
}

/// Validate the time of a reminder.
pub fn check_remind_at(at_ms: i64, now: i64) -> UfResult<()> {
    if at_ms < now + MIN_LEAD_MS || at_ms > now + MAX_LEAD_MS {
        return Err(invalid(
            "remind_at must be between 5 seconds and a year from now",
        ));
    }
    Ok(())
}

/// "90m", "1h30m", "in 2 hours" at the start of `text` -> milliseconds and the rest of the text. The same
/// grammar as the composer's `/remind`, for bots and the CLI.
pub fn parse_duration(text: &str) -> Option<(i64, String)> {
    let mut s = text.trim();
    if s.len() >= 3 && s[..3].eq_ignore_ascii_case("in ") {
        s = s[3..].trim_start();
    }
    let mut total = 0f64;
    let mut matched = false;
    loop {
        let digits: usize = s
            .bytes()
            .take_while(|b| b.is_ascii_digit() || *b == b'.')
            .count();
        if digits == 0 {
            break;
        }
        let Ok(n) = s[..digits].parse::<f64>() else {
            break;
        };
        let after = s[digits..].trim_start();
        let letters = after.bytes().take_while(u8::is_ascii_alphabetic).count();
        if letters == 0 {
            break;
        }
        let factor = match after[..letters].to_ascii_lowercase().as_str() {
            "s" | "sec" | "secs" | "second" | "seconds" => 1_000.0,
            "m" | "min" | "mins" | "minute" | "minutes" => 60_000.0,
            "h" | "hr" | "hrs" | "hour" | "hours" => 3_600_000.0,
            "d" | "day" | "days" => 86_400_000.0,
            "w" | "week" | "weeks" => 7.0 * 86_400_000.0,
            _ => break,
        };
        total += n * factor;
        matched = true;
        s = after[letters..].trim_start();
    }
    if !matched || total <= 0.0 || total > 1e13 {
        return None;
    }
    Some((total.round() as i64, s.to_owned()))
}

impl UserFeatures {
    pub async fn create_reminder(
        &self,
        user: i64,
        note: &str,
        remind_at_ms: i64,
        target: Target,
    ) -> UfResult<Reminder> {
        self.require(REMINDERS_ID, target.guild).await?;
        let cfg = self.settings(REMINDERS_ID, target.guild).await?;
        let note = note.trim();
        if note.chars().count() > MAX_NOTE_CHARS {
            return Err(invalid(format!(
                "note must be {MAX_NOTE_CHARS} characters or fewer"
            )));
        }
        check_remind_at(remind_at_ms, now_ms())?;
        let max = cfg.int("max_pending", 50);
        let pending: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM reminders WHERE user_id = $1 AND status = 'pending'",
        )
        .bind(user)
        .fetch_one(&self.pool)
        .await?;
        if pending >= max {
            return Err(UfError::Conflict(format!(
                "You can have {max} reminders at a time."
            )));
        }
        let id = self.next_id();
        Ok(sqlx::query_as::<_, Reminder>(concat!(
            "INSERT INTO reminders (id, user_id, channel_id, guild_id, message_id, note, remind_at) \
             VALUES ($1, $2, $3, $4, $5, $6, to_timestamp($7::double precision / 1000.0)) RETURNING ",
            cols!()
        ))
        .bind(id)
        .bind(user)
        .bind(target.channel)
        .bind(target.guild)
        .bind(target.message)
        .bind(note)
        .bind(remind_at_ms)
        .fetch_one(&self.pool)
        .await?)
    }

    /// The user's pending reminders, soonest first; `all` adds the ones from the last week that are settled.
    pub async fn list_reminders(&self, user: i64, all: bool) -> UfResult<Vec<Reminder>> {
        Ok(sqlx::query_as::<_, Reminder>(concat!(
            "SELECT ",
            cols!(),
            " FROM reminders WHERE user_id = $1 AND \
             (status = 'pending' OR ($2 AND remind_at > now() - interval '7 days')) \
             ORDER BY remind_at, id LIMIT 200"
        ))
        .bind(user)
        .bind(all)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn cancel_reminder(&self, user: i64, id: i64) -> UfResult<()> {
        let n = sqlx::query(
            "UPDATE reminders SET status = 'cancelled' \
             WHERE id = $1 AND user_id = $2 AND status = 'pending'",
        )
        .bind(id)
        .bind(user)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if n == 1 {
            return Ok(());
        }
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM reminders WHERE id = $1 AND user_id = $2)",
        )
        .bind(id)
        .bind(user)
        .fetch_one(&self.pool)
        .await?;
        if exists {
            Ok(())
        } else {
            Err(UfError::NotFound("reminder"))
        }
    }

    pub async fn due_reminders(&self, limit: i64) -> UfResult<Vec<DueReminder>> {
        Ok(sqlx::query_as::<_, DueReminder>(
            "SELECT id, user_id, channel_id, guild_id, message_id, note, \
             (extract(epoch from now() - remind_at) * 1000)::bigint AS late_ms \
             FROM reminders WHERE status = 'pending' AND remind_at <= now() \
             ORDER BY remind_at, id LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn settle_reminder(&self, id: i64, delivered: bool) -> UfResult<()> {
        sqlx::query(
            "UPDATE reminders SET status = $2, delivered_at = CASE WHEN $2 = 'sent' THEN now() END \
             WHERE id = $1 AND status = 'pending'",
        )
        .bind(id)
        .bind(if delivered { "sent" } else { "failed" })
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn postpone_reminder(&self, id: i64, delay_ms: i64) -> UfResult<()> {
        sqlx::query(
            "UPDATE reminders SET remind_at = now() + make_interval(secs => $2::double precision / 1000.0) \
             WHERE id = $1 AND status = 'pending'",
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
    fn durations_parse_like_the_composer() {
        assert_eq!(parse_duration("90m").unwrap().0, 90 * 60_000);
        assert_eq!(
            parse_duration("1h30m call mom"),
            Some((90 * 60_000, "call mom".into()))
        );
        assert_eq!(
            parse_duration("in 2 hours drink water").unwrap(),
            (7_200_000, "drink water".into())
        );
        assert_eq!(parse_duration("1.5d").unwrap().0, 129_600_000);
        assert_eq!(parse_duration("2w").unwrap().0, 14 * 86_400_000);
        assert_eq!(parse_duration("30 sec").unwrap().0, 30_000);
        assert_eq!(parse_duration("soon"), None);
        assert_eq!(parse_duration("0m"), None);
        assert_eq!(parse_duration("5 parsecs"), None);
        assert_eq!(parse_duration(""), None);
    }

    #[test]
    fn remind_window() {
        assert!(check_remind_at(1_000_000 + 4_000, 1_000_000).is_err());
        assert!(check_remind_at(1_000_000 + 6_000, 1_000_000).is_ok());
        assert!(check_remind_at(1_000_000 + MAX_LEAD_MS + 1, 1_000_000).is_err());
    }
}
