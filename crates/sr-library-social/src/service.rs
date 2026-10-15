//! [`LibrarySocial`]: the entry point of the Library social backend (ratings, reviews, lists,
//! rankings, unread counts). Split by topic in [`crate::ratings`], [`crate::reviews`],
//! [`crate::lists`] and [`crate::discover`].

use std::sync::Arc;

use serde_json::Value;
use sqlx::PgPool;
use sr_library::LibraryEvents;

use crate::error::{SocialError, SocialResult};
use crate::model::Viewer;

/// Source of snowflake ids.
pub type IdSource = Arc<dyn Fn() -> i64 + Send + Sync>;

#[derive(Clone)]
pub struct LibrarySocial {
    pub(crate) pool: PgPool,
    pub(crate) events: Option<Arc<dyn LibraryEvents>>,
    pub(crate) ids: IdSource,
}

impl LibrarySocial {
    pub fn new(pool: PgPool, ids: IdSource) -> Self {
        Self {
            pool,
            events: None,
            ids,
        }
    }

    /// Where `SR_LIBRARY_UPDATE` goes (the same sink as the reading state).
    pub fn with_events(mut self, events: Arc<dyn LibraryEvents>) -> Self {
        self.events = Some(events);
        self
    }

    /// `SR_LIBRARY_UPDATE` to the account's own sessions.
    pub(crate) async fn emit_self(&self, user: i64, payload: Value) {
        if let Some(e) = &self.events {
            e.emit(vec![user], payload).await;
        }
    }

    /// A series anyone may see (published, not merged away). Reads use this.
    pub(crate) async fn visible_series(&self, id: i64) -> SocialResult<()> {
        let found: Option<bool> = sqlx::query_scalar(
            "SELECT age_restricted FROM reading_series \
             WHERE id = $1 AND published AND merged_into IS NULL",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        found.map(|_| ()).ok_or(SocialError::NotFound("series"))
    }

    /// A series the viewer may act on: visible, and a restricted one only after the 18+ answer.
    pub(crate) async fn actionable_series(&self, v: Viewer, id: i64) -> SocialResult<()> {
        let restricted: bool = sqlx::query_scalar(
            "SELECT age_restricted FROM reading_series \
             WHERE id = $1 AND published AND merged_into IS NULL",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(SocialError::NotFound("series"))?;
        if restricted && !v.adult {
            return Err(SocialError::AgeConfirmationRequired);
        }
        Ok(())
    }
}
