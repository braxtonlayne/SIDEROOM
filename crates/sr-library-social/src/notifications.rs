//! The bell's unread count (LIB-SOC-007, LIB-UI-004): published chapters of series the member
//! follows with the bell on, newer than the later of the follow and the last time they opened the
//! bell. Opening the bell calls [`LibrarySocial::mark_notifications_seen`].

use crate::error::SocialResult;
use crate::model::*;
use crate::service::LibrarySocial;

/// The count stops here ("99+").
pub const UNREAD_CAP: i64 = 99;

impl LibrarySocial {
    pub async fn notification_count(&self, v: Viewer) -> SocialResult<NotificationCount> {
        let unread: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM ( \
               SELECT 1 FROM reading_follows f \
                 JOIN reading_series s ON s.id = f.series_id AND s.published AND s.merged_into IS NULL \
                 JOIN reading_chapters c ON c.series_id = f.series_id AND c.published \
                 LEFT JOIN reading_notification_state n ON n.user_id = f.user_id \
                WHERE f.user_id = $1 AND f.notify AND (NOT $2 OR NOT s.age_restricted) \
                  AND c.published_at > GREATEST(f.created_at, COALESCE(n.seen_at, f.created_at)) \
                LIMIT $3) x",
        )
        .bind(v.user_id)
        .bind(v.hide_restricted)
        .bind(UNREAD_CAP)
        .fetch_one(&self.pool)
        .await?;
        Ok(NotificationCount { unread })
    }

    pub async fn mark_notifications_seen(&self, v: Viewer) -> SocialResult<()> {
        sqlx::query(
            "INSERT INTO reading_notification_state (user_id, seen_at) VALUES ($1, now()) \
             ON CONFLICT (user_id) DO UPDATE SET seen_at = now()",
        )
        .bind(v.user_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
