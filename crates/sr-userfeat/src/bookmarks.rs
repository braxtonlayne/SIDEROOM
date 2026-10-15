//! FTR-BKM-001 bookmarks: private saved messages with a note. Only the owner ever sees a bookmark; the API layer
//! decides at read time whether the owner can still see the message it points at.

use serde::Serialize;
use sqlx::FromRow;
use utoipa::ToSchema;

use crate::error::{UfError, UfResult, invalid};
use crate::{UserFeatures, features::BOOKMARKS_ID};

pub const MAX_NOTE_CHARS: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow, ToSchema)]
pub struct Bookmark {
    pub message_id: String,
    pub channel_id: String,
    pub guild_id: Option<String>,
    pub note: String,
    pub created_at: String,
}

macro_rules! cols {
    () => {
        concat!(
            "message_id::text AS message_id, channel_id::text AS channel_id, guild_id::text AS guild_id, note, ",
            iso!("created_at"),
            " AS created_at"
        )
    };
}

impl UserFeatures {
    /// Save (or update the note of) a bookmark.
    pub async fn save_bookmark(
        &self,
        user: i64,
        message: i64,
        channel: i64,
        guild: Option<i64>,
        note: &str,
    ) -> UfResult<Bookmark> {
        self.require(BOOKMARKS_ID, guild).await?;
        let cfg = self.settings(BOOKMARKS_ID, guild).await?;
        let note = note.trim();
        if note.chars().count() > MAX_NOTE_CHARS {
            return Err(invalid(format!(
                "note must be {MAX_NOTE_CHARS} characters or fewer"
            )));
        }
        let existing: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM message_bookmarks WHERE user_id = $1 AND message_id = $2)",
        )
        .bind(user)
        .bind(message)
        .fetch_one(&self.pool)
        .await?;
        if !existing {
            let count: i64 =
                sqlx::query_scalar("SELECT count(*) FROM message_bookmarks WHERE user_id = $1")
                    .bind(user)
                    .fetch_one(&self.pool)
                    .await?;
            let max = cfg.int("max_per_user", 500);
            if count >= max {
                return Err(UfError::Conflict(format!(
                    "You can keep {max} bookmarks. Remove one to save another."
                )));
            }
        }
        Ok(sqlx::query_as::<_, Bookmark>(concat!(
            "INSERT INTO message_bookmarks (user_id, message_id, channel_id, guild_id, note) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (user_id, message_id) DO UPDATE SET note = EXCLUDED.note \
             RETURNING ",
            cols!()
        ))
        .bind(user)
        .bind(message)
        .bind(channel)
        .bind(guild)
        .bind(note)
        .fetch_one(&self.pool)
        .await?)
    }

    /// Newest first; `before` is the message id of the last row of the previous page.
    pub async fn list_bookmarks(
        &self,
        user: i64,
        before: Option<i64>,
        limit: i64,
    ) -> UfResult<Vec<Bookmark>> {
        Ok(sqlx::query_as::<_, Bookmark>(concat!(
            "SELECT ",
            cols!(),
            " FROM message_bookmarks WHERE user_id = $1 AND ($2::bigint IS NULL OR message_id < $2) \
             ORDER BY message_id DESC LIMIT $3"
        ))
        .bind(user)
        .bind(before)
        .bind(limit.clamp(1, 100))
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_bookmark(&self, user: i64, message: i64) -> UfResult<Bookmark> {
        sqlx::query_as::<_, Bookmark>(concat!(
            "SELECT ",
            cols!(),
            " FROM message_bookmarks WHERE user_id = $1 AND message_id = $2"
        ))
        .bind(user)
        .bind(message)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(UfError::NotFound("bookmark"))
    }

    /// Remove a bookmark. Removing one that is not there is not an error.
    pub async fn remove_bookmark(&self, user: i64, message: i64) -> UfResult<()> {
        sqlx::query("DELETE FROM message_bookmarks WHERE user_id = $1 AND message_id = $2")
            .bind(user)
            .bind(message)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
