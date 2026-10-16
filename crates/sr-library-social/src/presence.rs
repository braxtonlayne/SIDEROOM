//! "Reading" presence (LIB-RDR-017, D-193): a short-lived row per account refreshed by the reader
//! heartbeat, and `SR_ACTIVITY_UPDATE` (type `reading`, activity type 6) to the member's friends.
//!
//! Privacy rules, enforced here and not by the client:
//! * A member who turned library sharing off or paused their history shares nothing (D-190).
//! * Age-restricted series never show unless the member allows it (`hide_age_restricted` off), and
//!   then go to friends only with the title masked, as in Cinema.
//! * The audience is the member's friends. (There is no "everyone" level for reading yet.)

use serde_json::{Value, json};
use sqlx::Row;

use crate::error::{SocialError, SocialResult};
use crate::model::*;
use crate::service::LibrarySocial;

/// Seconds a heartbeat keeps the presence alive.
pub const PRESENCE_TTL_SECS: i64 = 90;
const MAX_AUDIENCE: i64 = 500;
const ACTIVITY_NAME: &str = "SIDE ROOM Library";

fn number_text(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{n:.0}")
    } else {
        format!("{n}")
    }
}

/// The Discord-shaped "Reading" activity (type 6; the fields match type 3).
fn activity(title: &str, restricted: bool, chapter: &str, started_ms: i64) -> Value {
    json!({
        "type": 6, "name": ACTIVITY_NAME,
        "details": if restricted { "Age-restricted title" } else { title },
        "state": if restricted { Value::Null } else { json!(format!("Chapter {chapter}")) },
        "timestamps": { "start": started_ms }, "application_id": null,
    })
}

impl LibrarySocial {
    async fn friends_of(&self, user: i64) -> SocialResult<Vec<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT target_id FROM relationships WHERE user_id = $1 AND type = 1 LIMIT $2",
        )
        .bind(user)
        .bind(MAX_AUDIENCE)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn emit_activity(&self, users: Vec<i64>, payload: Value) {
        if users.is_empty() {
            return;
        }
        if let Some(e) = &self.events {
            e.emit_activity(users, payload).await;
        }
    }

    /// A reader heartbeat for `chapter`: refresh the record and announce a change.
    pub async fn heartbeat_reading(&self, v: Viewer, chapter: i64) -> SocialResult<()> {
        let row = sqlx::query(
            "SELECT c.series_id, s.title, s.age_restricted, c.number::float8 AS number \
               FROM reading_chapters c JOIN reading_series s ON s.id = c.series_id \
              WHERE c.id = $1 AND c.published AND s.published AND s.merged_into IS NULL",
        )
        .bind(chapter)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(SocialError::NotFound("chapter"))?;
        let series: i64 = row.try_get("series_id")?;
        let title: String = row.try_get("title")?;
        let restricted: bool = row.try_get("age_restricted")?;
        let number: f64 = row.try_get("number")?;
        if restricted && !v.adult {
            return Err(SocialError::AgeConfirmationRequired);
        }
        let shares: bool = sqlx::query_scalar(
            "SELECT activity_shares($1, 'library') \
                AND NOT ($2 AND activity_hides_restricted($1))",
        )
        .bind(v.user_id)
        .bind(restricted)
        .fetch_one(&self.pool)
        .await?;
        if !shares {
            return self.stop_reading(v.user_id).await;
        }
        let prev = sqlx::query(
            "SELECT chapter_id, expires_at > now() AS live FROM reading_presence WHERE user_id = $1",
        )
        .bind(v.user_id)
        .fetch_optional(&self.pool)
        .await?;
        let row = sqlx::query(
            "INSERT INTO reading_presence (user_id, series_id, chapter_id, restricted, started_at, expires_at) \
             VALUES ($1, $2, $3, $4, now(), now() + make_interval(secs => $5)) \
             ON CONFLICT (user_id) DO UPDATE SET \
               started_at = CASE WHEN reading_presence.series_id = EXCLUDED.series_id \
                                  AND reading_presence.expires_at > now() \
                            THEN reading_presence.started_at ELSE now() END, \
               series_id = EXCLUDED.series_id, chapter_id = EXCLUDED.chapter_id, \
               restricted = EXCLUDED.restricted, expires_at = EXCLUDED.expires_at \
             RETURNING (extract(epoch FROM started_at) * 1000)::bigint AS started_ms, \
                       (extract(epoch FROM expires_at) * 1000)::bigint AS expires_ms",
        )
        .bind(v.user_id)
        .bind(series)
        .bind(chapter)
        .bind(restricted)
        .bind(PRESENCE_TTL_SECS as f64)
        .fetch_one(&self.pool)
        .await?;
        // A heartbeat on the same chapter changes nothing for the audience.
        if prev.as_ref().is_some_and(|p| {
            p.try_get::<bool, _>("live").unwrap_or(false)
                && p.try_get::<i64, _>("chapter_id").unwrap_or(0) == chapter
        }) {
            return Ok(());
        }
        let started: i64 = row.try_get("started_ms")?;
        let expires: i64 = row.try_get("expires_ms")?;
        let mut payload = json!({
            "type": "reading",
            "user_id": v.user_id.to_string(),
            "activity": activity(&title, restricted, &number_text(number), started),
            "expires_at_ms": expires,
        });
        if !restricted {
            payload["series_id"] = json!(series.to_string());
            payload["chapter_id"] = json!(chapter.to_string());
        }
        let audience = self.friends_of(v.user_id).await?;
        self.emit_activity(audience, payload).await;
        Ok(())
    }

    /// End the account's own "reading" presence and tell friends. Quiet when there is none.
    pub async fn stop_reading(&self, user: i64) -> SocialResult<()> {
        let live: Option<bool> = sqlx::query_scalar(
            "DELETE FROM reading_presence WHERE user_id = $1 RETURNING expires_at > now()",
        )
        .bind(user)
        .fetch_optional(&self.pool)
        .await?;
        if live == Some(true) {
            let audience = self.friends_of(user).await?;
            self.emit_activity(
                audience,
                json!({ "type": "stopped", "user_id": user.to_string(), "kind": "reading" }),
            )
            .await;
        }
        Ok(())
    }

    /// Friends who are reading something now and share it, newest first. Age-restricted series
    /// only to a viewer who confirmed 18+.
    pub async fn friends_reading(&self, v: Viewer) -> SocialResult<FriendsReading> {
        let rows = sqlx::query(
            "SELECT p.user_id, p.series_id, p.chapter_id, s.title, c.number::float8 AS number, \
               (extract(epoch FROM p.started_at) * 1000)::bigint AS started_ms, \
               (extract(epoch FROM p.expires_at) * 1000)::bigint AS expires_ms \
             FROM reading_presence p \
             JOIN relationships r ON r.user_id = $1 AND r.target_id = p.user_id AND r.type = 1 \
             JOIN reading_series s ON s.id = p.series_id AND s.published AND s.merged_into IS NULL \
             JOIN reading_chapters c ON c.id = p.chapter_id AND c.published \
             WHERE p.expires_at > now() AND activity_shares(p.user_id, 'library') \
               AND NOT (p.restricted AND activity_hides_restricted(p.user_id)) \
               AND ($2 OR NOT p.restricted) \
             ORDER BY p.started_at DESC LIMIT 60",
        )
        .bind(v.user_id)
        .bind(v.adult)
        .fetch_all(&self.pool)
        .await?;
        let mut items = Vec::with_capacity(rows.len());
        for r in &rows {
            items.push(FriendReading {
                user_id: r.try_get("user_id")?,
                series_id: r.try_get("series_id")?,
                series_title: r.try_get("title")?,
                chapter_id: r.try_get("chapter_id")?,
                chapter_number: r.try_get("number")?,
                started_at_ms: r.try_get("started_ms")?,
                expires_at_ms: r.try_get("expires_ms")?,
            });
        }
        Ok(FriendsReading { items })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restricted_titles_are_masked() {
        let a = activity("Secret", true, "3", 10);
        assert_eq!(a["details"], "Age-restricted title");
        assert!(a["state"].is_null());
        let a = activity("Frieren", false, "36.5", 10);
        assert_eq!(a["type"], json!(6));
        assert_eq!(a["details"], "Frieren");
        assert_eq!(a["state"], "Chapter 36.5");
    }
}
