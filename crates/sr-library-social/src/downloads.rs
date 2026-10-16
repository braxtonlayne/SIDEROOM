//! The daily allowance of Patron chapter downloads (LIB-RDR-016, D-075).

use crate::error::{SocialError, SocialResult};
use crate::service::LibrarySocial;

impl LibrarySocial {
    /// Count one download against the member's rolling 24 hours. `per_day` comes from the Patron
    /// perk (0 = none). A `Conflict` carries the "used up" message; the API maps it to 429.
    pub async fn take_download_slot(
        &self,
        user: i64,
        chapter: i64,
        format: &str,
        per_day: u32,
    ) -> SocialResult<()> {
        let mut tx = self.pool.begin().await?;
        // Serialise concurrent requests of one member so the count cannot be raced past.
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(user)
            .execute(&mut *tx)
            .await?;
        let used: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM reading_downloads \
             WHERE user_id = $1 AND created_at > now() - interval '24 hours'",
        )
        .bind(user)
        .fetch_one(&mut *tx)
        .await?;
        if used >= i64::from(per_day) {
            return Err(SocialError::Conflict(
                "You have used today's downloads. Try again tomorrow.".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO reading_downloads (id, user_id, chapter_id, format) VALUES ($1, $2, $3, $4)",
        )
        .bind((self.ids)())
        .bind(user)
        .bind(chapter)
        .bind(format)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Downloads left in the rolling 24 hours.
    pub async fn downloads_left(&self, user: i64, per_day: u32) -> SocialResult<u32> {
        let used: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM reading_downloads \
             WHERE user_id = $1 AND created_at > now() - interval '24 hours'",
        )
        .bind(user)
        .fetch_one(&self.pool)
        .await?;
        Ok(per_day.saturating_sub(u32::try_from(used).unwrap_or(u32::MAX)))
    }
}
