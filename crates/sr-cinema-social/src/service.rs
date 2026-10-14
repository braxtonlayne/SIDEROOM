//! [`CinemaService`]: the entry point of the Cinema social backend. The implementation is split by
//! topic: [`crate::progress`], [`crate::presence`], [`crate::lists`], [`crate::social`] and
//! [`crate::discover`].

use std::sync::Arc;

use serde_json::Value;
use sqlx::PgPool;

use crate::catalog::{CatalogEpisode, CatalogLookup, CatalogTitle, TitleKind};
use crate::error::{SocialError, SocialResult};
use crate::model::Viewer;
use crate::ports::{CinemaEvents, EVENT_CINEMA_UPDATE, ModerationHook, NoEvents, NoModeration};

/// Source of snowflake ids.
pub type IdSource = Arc<dyn Fn() -> i64 + Send + Sync>;

/// A movie or an episode the viewer is allowed to open.
pub(crate) struct Item {
    pub title: CatalogTitle,
    pub episode: Option<CatalogEpisode>,
}

impl Item {
    pub fn episode_id(&self) -> i64 {
        self.episode.as_ref().map_or(0, |e| e.id)
    }
}

#[derive(Clone)]
pub struct CinemaService {
    pub(crate) pool: PgPool,
    pub(crate) catalog: Arc<dyn CatalogLookup>,
    pub(crate) events: Arc<dyn CinemaEvents>,
    pub(crate) moderation: Arc<dyn ModerationHook>,
    pub(crate) ids: IdSource,
}

impl CinemaService {
    pub fn new(pool: PgPool, catalog: Arc<dyn CatalogLookup>, ids: IdSource) -> Self {
        Self {
            pool,
            catalog,
            events: Arc::new(NoEvents),
            moderation: Arc::new(NoModeration),
            ids,
        }
    }

    /// Where `SR_CINEMA_UPDATE` and `SR_ACTIVITY_UPDATE` go.
    pub fn with_events(mut self, events: Arc<dyn CinemaEvents>) -> Self {
        self.events = events;
        self
    }

    pub fn with_moderation(mut self, moderation: Arc<dyn ModerationHook>) -> Self {
        self.moderation = moderation;
        self
    }

    /// `SR_CINEMA_UPDATE` to the account's own sessions.
    pub(crate) async fn emit_self(&self, user: i64, payload: Value) {
        self.events
            .emit(vec![user], EVENT_CINEMA_UPDATE, payload)
            .await;
    }

    pub(crate) async fn published_title(&self, id: i64) -> SocialResult<CatalogTitle> {
        self.catalog
            .title(id)
            .await
            .map_err(|e| SocialError::Storage(e.to_string()))?
            .ok_or(SocialError::NotFound("title"))
    }

    /// A title the viewer may open: published, and for a restricted one after age confirmation.
    pub(crate) async fn openable_title(&self, v: Viewer, id: i64) -> SocialResult<CatalogTitle> {
        let t = self.published_title(id).await?;
        if t.age_restricted && !v.adult {
            return Err(SocialError::AgeConfirmationRequired);
        }
        Ok(t)
    }

    /// The movie (`episode_id` 0) or episode the viewer may open.
    pub(crate) async fn open_item(
        &self,
        v: Viewer,
        title_id: i64,
        episode_id: i64,
    ) -> SocialResult<Item> {
        let title = self.openable_title(v, title_id).await?;
        if episode_id == 0 {
            if title.kind != TitleKind::Movie {
                return Err(SocialError::Invalid("a series is played by episode".into()));
            }
            return Ok(Item {
                title,
                episode: None,
            });
        }
        if title.kind == TitleKind::Movie {
            return Err(SocialError::NotFound("episode"));
        }
        let episode = self
            .catalog
            .episode(title_id, episode_id)
            .await
            .map_err(|e| SocialError::Storage(e.to_string()))?
            .filter(|e| e.title_id == title_id)
            .ok_or(SocialError::NotFound("episode"))?;
        Ok(Item {
            title,
            episode: Some(episode),
        })
    }

    /// Titles by id from the catalog.
    pub(crate) async fn titles_by_id(
        &self,
        ids: &[i64],
    ) -> SocialResult<std::collections::HashMap<i64, CatalogTitle>> {
        if ids.is_empty() {
            return Ok(Default::default());
        }
        let found = self
            .catalog
            .titles(ids)
            .await
            .map_err(|e| SocialError::Storage(e.to_string()))?;
        Ok(found.into_iter().map(|t| (t.id, t)).collect())
    }

    /// `true` when `viewer` may see a title in listings: restricted ones only for confirmed adults.
    pub(crate) fn listable(v: Viewer, t: &CatalogTitle) -> bool {
        !t.age_restricted || v.adult
    }
}
