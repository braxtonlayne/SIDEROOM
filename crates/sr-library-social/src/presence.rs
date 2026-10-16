//! "Reading" presence (LIB-RDR-017, D-193): a short-lived row per account refreshed by the reader
//! heartbeat, and `SR_ACTIVITY_UPDATE` (type `reading`, activity type 6) to the member's friends.
//!
//! Privacy rules, enforced here and not by the client:
//! * A member who turned library sharing off or paused their history shares nothing (D-190).
//! * Age-restricted series never show unless the member allows it (`hide_age_restricted` off), and
//!   then go to friends only with the title masked, as in Cinema.
//! * `library.share_reading` in the settings blob (D-229): `off` shares nothing, `friends` (the default)
//!   the member's friends, `everyone` also people who share a server (never strangers; a person who
//!   blocked or ignored the reader is left out). Age-restricted series stay with friends, masked.

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

/// Delete reading presence rows whose heartbeat lapsed (`library.gc`, H-18). The readers of the table
/// already ignore them; this keeps the table small. Returns how many rows went.
pub async fn sweep_expired_presence(pool: &sqlx::PgPool) -> SocialResult<u64> {
    let done = sqlx::query("DELETE FROM reading_presence WHERE expires_at < now()")
        .execute(pool)
        .await?;
    Ok(done.rows_affected())
}

impl LibrarySocial {
    /// Friends, and with `everyone` people who share a server with `user` (minus anyone who blocked
    /// or ignored them), capped. The same audience the Cinema's watching presence uses.
    async fn audience_of(&self, user: i64, everyone: bool) -> SocialResult<Vec<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT target_id FROM relationships WHERE user_id = $1 AND type = 1 \
             UNION \
             SELECT gm2.user_id FROM guild_members gm1 \
               JOIN guild_members gm2 ON gm2.guild_id = gm1.guild_id AND gm2.user_id <> gm1.user_id \
             WHERE $2 AND gm1.user_id = $1 \
               AND NOT EXISTS (SELECT 1 FROM relationships r WHERE r.user_id = gm2.user_id \
                               AND r.target_id = $1 AND r.type IN (2, 6)) \
             LIMIT $3",
        )
        .bind(user)
        .bind(everyone)
        .bind(MAX_AUDIENCE)
        .fetch_all(&self.pool)
        .await?)
    }

    /// `library.share_reading` of `user`: 0 off, 1 friends (default), 2 everyone.
    pub(crate) async fn share_level(&self, user: i64) -> SocialResult<i16> {
        Ok(sqlx::query_scalar(
            "SELECT CASE (SELECT settings #>> '{library,share_reading}' FROM user_settings WHERE user_id = $1) \
                      WHEN 'off' THEN 0 WHEN 'everyone' THEN 2 ELSE 1 END::smallint",
        )
        .bind(user)
        .fetch_one(&self.pool)
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
        let level = self.share_level(v.user_id).await?;
        if !shares || level == 0 {
            return self.stop_reading(v.user_id).await;
        }
        // Age-restricted series go to friends only; `everyone` adds server mates for the rest.
        let audience_flag: i16 = i16::from(level == 2 && !restricted);
        let prev = sqlx::query(
            "SELECT chapter_id, audience, expires_at > now() AS live FROM reading_presence WHERE user_id = $1",
        )
        .bind(v.user_id)
        .fetch_optional(&self.pool)
        .await?;
        let row = sqlx::query(
            "INSERT INTO reading_presence (user_id, series_id, chapter_id, restricted, audience, started_at, expires_at) \
             VALUES ($1, $2, $3, $4, $6, now(), now() + make_interval(secs => $5)) \
             ON CONFLICT (user_id) DO UPDATE SET \
               started_at = CASE WHEN reading_presence.series_id = EXCLUDED.series_id \
                                  AND reading_presence.expires_at > now() \
                            THEN reading_presence.started_at ELSE now() END, \
               series_id = EXCLUDED.series_id, chapter_id = EXCLUDED.chapter_id, \
               restricted = EXCLUDED.restricted, audience = EXCLUDED.audience, \
               expires_at = EXCLUDED.expires_at \
             RETURNING (extract(epoch FROM started_at) * 1000)::bigint AS started_ms, \
                       (extract(epoch FROM expires_at) * 1000)::bigint AS expires_ms",
        )
        .bind(v.user_id)
        .bind(series)
        .bind(chapter)
        .bind(restricted)
        .bind(PRESENCE_TTL_SECS as f64)
        .bind(audience_flag)
        .fetch_one(&self.pool)
        .await?;
        // A heartbeat on the same chapter and audience changes nothing for the audience.
        if prev.as_ref().is_some_and(|p| {
            p.try_get::<bool, _>("live").unwrap_or(false)
                && p.try_get::<i64, _>("chapter_id").unwrap_or(0) == chapter
                && p.try_get::<i16, _>("audience").unwrap_or(0) == audience_flag
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
        let audience = self.audience_of(v.user_id, audience_flag == 1).await?;
        self.emit_activity(audience, payload).await;
        Ok(())
    }

    /// End the account's own "reading" presence and tell friends. Quiet when there is none.
    pub async fn stop_reading(&self, user: i64) -> SocialResult<()> {
        let gone: Option<(bool, i16)> = sqlx::query_as(
            "DELETE FROM reading_presence WHERE user_id = $1 RETURNING expires_at > now(), audience",
        )
        .bind(user)
        .fetch_optional(&self.pool)
        .await?;
        if let Some((true, audience)) = gone {
            let audience = self.audience_of(user, audience == 1).await?;
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
             JOIN reading_series s ON s.id = p.series_id AND s.published AND s.merged_into IS NULL \
             JOIN reading_chapters c ON c.id = p.chapter_id AND c.published \
             WHERE p.expires_at > now() AND activity_shares(p.user_id, 'library') \
               AND (EXISTS (SELECT 1 FROM relationships r WHERE r.user_id = $1 \
                              AND r.target_id = p.user_id AND r.type = 1) \
                    OR (p.audience = 1 AND NOT p.restricted AND p.user_id <> $1 \
                        AND EXISTS (SELECT 1 FROM guild_members g1 JOIN guild_members g2 \
                                      ON g2.guild_id = g1.guild_id \
                                    WHERE g1.user_id = $1 AND g2.user_id = p.user_id) \
                        AND NOT EXISTS (SELECT 1 FROM relationships b \
                                        WHERE ((b.user_id = p.user_id AND b.target_id = $1) \
                                            OR (b.user_id = $1 AND b.target_id = p.user_id)) \
                                          AND b.type IN (2, 6)))) \
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
