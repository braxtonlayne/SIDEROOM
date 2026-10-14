//! The port to the Cinema catalog (titles, seasons, episodes), which lives in `sr-cinema`.
//!
//! This crate stores only per-account state keyed by plain `title_id` / `episode_id`. Everything it needs to
//! know about the catalog (does the title exist and is it published, is it age-restricted, which episode is
//! next, which tags does it carry) comes through [`CatalogLookup`]. [`MemoryCatalog`] is the in-memory fake.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::ids::id;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("catalog failure: {0}")]
pub struct CatalogError(pub String);

pub type CatalogResult<T> = Result<T, CatalogError>;

/// What a title is. Movies are played directly (`episode_id` 0); series and anime by episode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = CinemaTitleKind)]
pub enum TitleKind {
    Movie,
    Series,
    Anime,
}

impl TitleKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Movie => "movie",
            Self::Series => "series",
            Self::Anime => "anime",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "movie" => Self::Movie,
            "series" => Self::Series,
            "anime" => Self::Anime,
            _ => return None,
        })
    }
}

/// A published title as the catalog describes it. Unpublished and merged-away titles are not returned.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogTitle {
    pub id: i64,
    pub kind: TitleKind,
    pub title: String,
    pub year: Option<i32>,
    pub age_restricted: bool,
    /// Length of a movie, when known.
    pub runtime_secs: Option<i32>,
    pub poster_key: Option<String>,
    /// Genres and tags, lower-case.
    pub tags: Vec<String>,
}

/// A published episode.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogEpisode {
    pub id: i64,
    pub title_id: i64,
    pub season: i32,
    pub number: i32,
    pub title: Option<String>,
    pub runtime_secs: Option<i32>,
}

/// Catalog lookups the social features depend on. Implemented by `sr-cinema`.
#[async_trait]
pub trait CatalogLookup: Send + Sync {
    /// `None`: unknown, unpublished or merged away.
    async fn title(&self, id: i64) -> CatalogResult<Option<CatalogTitle>>;
    /// Several titles at once; missing ids are skipped.
    async fn titles(&self, ids: &[i64]) -> CatalogResult<Vec<CatalogTitle>>;
    async fn episode(
        &self,
        title_id: i64,
        episode_id: i64,
    ) -> CatalogResult<Option<CatalogEpisode>>;
    /// Published episodes of a title (of one season when given), in play order.
    async fn episodes(
        &self,
        title_id: i64,
        season: Option<i32>,
    ) -> CatalogResult<Vec<CatalogEpisode>>;
    /// The published episode that plays after `after_episode_id`, crossing seasons.
    async fn next_episode(
        &self,
        title_id: i64,
        after_episode_id: i64,
    ) -> CatalogResult<Option<CatalogEpisode>>;
    /// Published titles sharing at least one tag with `tags`, minus `exclude`, at most `limit`.
    async fn titles_with_tags(
        &self,
        tags: &[String],
        exclude: &[i64],
        limit: u32,
    ) -> CatalogResult<Vec<CatalogTitle>>;
}

/// A title for display, resolved from the catalog at read time.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaTitleRef)]
pub struct TitleRef {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub kind: TitleKind,
    pub title: String,
    pub year: Option<i32>,
    pub age_restricted: bool,
    pub poster_key: Option<String>,
}

impl From<&CatalogTitle> for TitleRef {
    fn from(t: &CatalogTitle) -> Self {
        Self {
            id: t.id,
            kind: t.kind,
            title: t.title.clone(),
            year: t.year,
            age_restricted: t.age_restricted,
            poster_key: t.poster_key.clone(),
        }
    }
}

/// An episode for display.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaEpisodeRef)]
pub struct EpisodeRef {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub title_id: i64,
    pub season: i32,
    pub number: i32,
    pub title: Option<String>,
    pub runtime_secs: Option<i32>,
}

impl From<&CatalogEpisode> for EpisodeRef {
    fn from(e: &CatalogEpisode) -> Self {
        Self {
            id: e.id,
            title_id: e.title_id,
            season: e.season,
            number: e.number,
            title: e.title.clone(),
            runtime_secs: e.runtime_secs,
        }
    }
}

/// In-memory catalog for tests and tools. Episodes play in `(season, number, id)` order.
#[derive(Default, Clone)]
pub struct MemoryCatalog {
    inner: Arc<Mutex<MemoryInner>>,
}

#[derive(Default)]
struct MemoryInner {
    titles: HashMap<i64, CatalogTitle>,
    episodes: Vec<CatalogEpisode>,
}

impl MemoryCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, MemoryInner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Add (or replace) a published title.
    pub fn add_title(&self, t: CatalogTitle) -> &Self {
        self.lock().titles.insert(t.id, t);
        self
    }

    /// Add a published episode.
    pub fn add_episode(&self, e: CatalogEpisode) -> &Self {
        self.lock().episodes.push(e);
        self
    }

    /// Unpublish a title (it vanishes from every lookup).
    pub fn remove_title(&self, id: i64) {
        self.lock().titles.remove(&id);
    }

    /// Quick constructor for a title with tags.
    pub fn title_with(id: i64, kind: TitleKind, name: &str, tags: &[&str]) -> CatalogTitle {
        CatalogTitle {
            id,
            kind,
            title: name.to_owned(),
            year: Some(2020),
            age_restricted: false,
            runtime_secs: (kind == TitleKind::Movie).then_some(7200),
            poster_key: None,
            tags: tags.iter().map(|t| (*t).to_owned()).collect(),
        }
    }

    /// Quick constructor for an episode.
    pub fn episode_with(id: i64, title_id: i64, season: i32, number: i32) -> CatalogEpisode {
        CatalogEpisode {
            id,
            title_id,
            season,
            number,
            title: Some(format!("Episode {number}")),
            runtime_secs: Some(1500),
        }
    }
}

fn sorted(mut v: Vec<CatalogEpisode>) -> Vec<CatalogEpisode> {
    v.sort_by_key(|e| (e.season, e.number, e.id));
    v
}

#[async_trait]
impl CatalogLookup for MemoryCatalog {
    async fn title(&self, id: i64) -> CatalogResult<Option<CatalogTitle>> {
        Ok(self.lock().titles.get(&id).cloned())
    }

    async fn titles(&self, ids: &[i64]) -> CatalogResult<Vec<CatalogTitle>> {
        let g = self.lock();
        Ok(ids
            .iter()
            .filter_map(|i| g.titles.get(i).cloned())
            .collect())
    }

    async fn episode(
        &self,
        title_id: i64,
        episode_id: i64,
    ) -> CatalogResult<Option<CatalogEpisode>> {
        let g = self.lock();
        if !g.titles.contains_key(&title_id) {
            return Ok(None);
        }
        Ok(g.episodes
            .iter()
            .find(|e| e.id == episode_id && e.title_id == title_id)
            .cloned())
    }

    async fn episodes(
        &self,
        title_id: i64,
        season: Option<i32>,
    ) -> CatalogResult<Vec<CatalogEpisode>> {
        let g = self.lock();
        if !g.titles.contains_key(&title_id) {
            return Ok(vec![]);
        }
        Ok(sorted(
            g.episodes
                .iter()
                .filter(|e| e.title_id == title_id && season.is_none_or(|s| e.season == s))
                .cloned()
                .collect(),
        ))
    }

    async fn next_episode(
        &self,
        title_id: i64,
        after_episode_id: i64,
    ) -> CatalogResult<Option<CatalogEpisode>> {
        let all = self.episodes(title_id, None).await?;
        let at = all.iter().position(|e| e.id == after_episode_id);
        Ok(at.and_then(|i| all.get(i + 1).cloned()))
    }

    async fn titles_with_tags(
        &self,
        tags: &[String],
        exclude: &[i64],
        limit: u32,
    ) -> CatalogResult<Vec<CatalogTitle>> {
        let g = self.lock();
        let mut out: Vec<CatalogTitle> = g
            .titles
            .values()
            .filter(|t| !exclude.contains(&t.id) && t.tags.iter().any(|x| tags.contains(x)))
            .cloned()
            .collect();
        out.sort_by_key(|t| t.id);
        out.truncate(limit as usize);
        Ok(out)
    }
}

/// A catalog without titles: every lookup finds nothing, so every item answers 404. The composition
/// root uses it until `sr-cinema` provides the real [`CatalogLookup`].
pub struct NoCatalog;

#[async_trait]
impl CatalogLookup for NoCatalog {
    async fn title(&self, _id: i64) -> CatalogResult<Option<CatalogTitle>> {
        Ok(None)
    }

    async fn titles(&self, _ids: &[i64]) -> CatalogResult<Vec<CatalogTitle>> {
        Ok(vec![])
    }

    async fn episode(&self, _t: i64, _e: i64) -> CatalogResult<Option<CatalogEpisode>> {
        Ok(None)
    }

    async fn episodes(&self, _t: i64, _s: Option<i32>) -> CatalogResult<Vec<CatalogEpisode>> {
        Ok(vec![])
    }

    async fn next_episode(&self, _t: i64, _after: i64) -> CatalogResult<Option<CatalogEpisode>> {
        Ok(None)
    }

    async fn titles_with_tags(
        &self,
        _tags: &[String],
        _exclude: &[i64],
        _limit: u32,
    ) -> CatalogResult<Vec<CatalogTitle>> {
        Ok(vec![])
    }
}
