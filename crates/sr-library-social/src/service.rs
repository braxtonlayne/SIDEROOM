//! [`LibrarySocial`]: the entry point of the Library social backend (ratings, reviews, lists,
//! rankings, unread counts). Split by topic in [`crate::ratings`], [`crate::reviews`],
//! [`crate::lists`] and [`crate::discover`].

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use sqlx::PgPool;
use sr_library::LibraryEvents;

use crate::error::{SocialError, SocialResult};
use crate::model::{Rankings, Viewer};

/// Cached rankings (LIB-SOC-004): the global aggregate is shared and recomputed at most once per
/// `global_ttl` (an hour); the Friends and People you know scopes are cached per member for `scoped_ttl`
/// (ten minutes). Off unless [`LibrarySocial::with_trending_cache`] is called.
#[derive(Clone)]
pub struct TrendCache {
    pub(crate) global: moka::future::Cache<String, Rankings>,
    pub(crate) scoped: moka::future::Cache<String, Rankings>,
}

/// Source of snowflake ids.
pub type IdSource = Arc<dyn Fn() -> i64 + Send + Sync>;

#[derive(Clone)]
pub struct LibrarySocial {
    pub(crate) pool: PgPool,
    pub(crate) events: Option<Arc<dyn LibraryEvents>>,
    pub(crate) ids: IdSource,
    pub(crate) trending: Option<TrendCache>,
}

impl LibrarySocial {
    pub fn new(pool: PgPool, ids: IdSource) -> Self {
        Self {
            pool,
            events: None,
            ids,
            trending: None,
        }
    }

    /// Cache rankings: the global aggregate for `global_ttl`, the scoped ones per member for `scoped_ttl`.
    pub fn with_trending_cache(mut self, global_ttl: Duration, scoped_ttl: Duration) -> Self {
        self.trending = Some(TrendCache {
            global: moka::future::Cache::builder()
                .max_capacity(2_000)
                .time_to_live(global_ttl)
                .build(),
            scoped: moka::future::Cache::builder()
                .max_capacity(50_000)
                .time_to_live(scoped_ttl)
                .build(),
        });
        self
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
