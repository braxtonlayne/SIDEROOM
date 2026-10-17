//! The port to the Cinema catalog (`TvCatalog`). Live TV never reads the catalog's tables: the composition
//! root plugs an implementation over the catalog repository in (`sr-server::tv_catalog`), tests use
//! [`MemoryCatalog`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::error::TvResult;
use crate::model::{MediaKind, PoolFilter, Restricted};

/// One thing that can air: a film or one episode, with the asset that carries it.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogItem {
    pub title_id: i64,
    pub episode_id: Option<i64>,
    pub asset_id: i64,
    /// Film or series title.
    pub title: String,
    pub episode_title: Option<String>,
    pub media_kind: MediaKind,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub year: Option<i16>,
    pub duration_ms: i64,
    pub age_restricted: bool,
    /// Lower case.
    pub genres: Vec<String>,
    /// Lower case.
    pub tags: Vec<String>,
}

impl CatalogItem {
    /// Identity for no-repeat windows and cursors: the episode, else the title.
    pub fn key(&self) -> i64 {
        self.episode_id.unwrap_or(self.title_id)
    }
}

impl PoolFilter {
    /// The restriction rule that applies on a channel.
    pub fn restricted_rule(&self, channel_restricted: bool) -> Restricted {
        self.restricted.unwrap_or(if channel_restricted {
            Restricted::Include
        } else {
            Restricted::Exclude
        })
    }

    /// Whether an item passes every filter except the collection (the catalog resolves that one).
    pub fn matches(&self, i: &CatalogItem, channel_restricted: bool) -> bool {
        if !self.kinds.is_empty() && !self.kinds.contains(&i.media_kind) {
            return false;
        }
        if !self.genres.iter().all(|g| i.genres.contains(g)) {
            return false;
        }
        if !self.tags.iter().all(|t| i.tags.contains(t)) {
            return false;
        }
        if !self.title_ids.is_empty() && !self.title_ids.contains(&i.title_id) {
            return false;
        }
        if let Some(y) = self.year_from
            && i.year.is_none_or(|v| v < y)
        {
            return false;
        }
        if let Some(y) = self.year_to
            && i.year.is_none_or(|v| v > y)
        {
            return false;
        }
        let minutes = i.duration_ms / 60_000;
        if self.min_runtime_min.is_some_and(|m| minutes < i64::from(m)) {
            return false;
        }
        if self.max_runtime_min.is_some_and(|m| minutes > i64::from(m)) {
            return false;
        }
        match self.restricted_rule(channel_restricted) {
            Restricted::Exclude => !i.age_restricted,
            Restricted::Include => true,
            Restricted::Only => i.age_restricted,
        }
    }
}

/// A library title as the guide importer sees it (no episodes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitleRef {
    pub title_id: i64,
    pub title: String,
    pub year: Option<i16>,
    pub media_kind: MediaKind,
}

#[async_trait]
pub trait TvCatalog: Send + Sync {
    /// Titles that can be matched by name (TV-ADM-004), each once, at most `limit`. The default derives
    /// them from [`TvCatalog::pool`]; stores may override it with a cheaper listing.
    async fn title_refs(&self, limit: usize) -> TvResult<Vec<TitleRef>> {
        let filter = PoolFilter {
            restricted: Some(Restricted::Include),
            ..PoolFilter::default()
        };
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for i in self.pool(&filter, true, usize::MAX).await? {
            if seen.insert(i.title_id) {
                out.push(TitleRef {
                    title_id: i.title_id,
                    title: i.title,
                    year: i.year,
                    media_kind: i.media_kind,
                });
                if out.len() >= limit {
                    break;
                }
            }
        }
        Ok(out)
    }

    /// Playable items (published, with an asset and a runtime) that pass `filter`, in a stable order:
    /// the order of `filter.title_ids` when given, else by title; episodes of a series by season and
    /// number. At most `limit` items.
    async fn pool(
        &self,
        filter: &PoolFilter,
        channel_restricted: bool,
        limit: usize,
    ) -> TvResult<Vec<CatalogItem>>;

    /// One playable item: a film (`episode_id` None) or an episode of a series.
    async fn item(&self, title_id: i64, episode_id: Option<i64>) -> TvResult<Option<CatalogItem>>;

    /// Short playable items tagged `filler` (at most 10 minutes), for gap filling.
    async fn bumpers(&self, limit: usize) -> TvResult<Vec<CatalogItem>>;
}

/// In-memory catalog for tests and tools.
#[derive(Default, Clone)]
pub struct MemoryCatalog {
    items: Arc<Mutex<Vec<CatalogItem>>>,
    collections: Arc<Mutex<HashMap<i64, Vec<i64>>>>,
}

impl MemoryCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&self, item: CatalogItem) -> &Self {
        self.items
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(item);
        self
    }

    /// Define a collection of titles.
    pub fn collection(&self, id: i64, titles: Vec<i64>) -> &Self {
        self.collections
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id, titles);
        self
    }

    pub fn clear(&self) {
        self.items.lock().unwrap_or_else(|p| p.into_inner()).clear();
    }
}

#[async_trait]
impl TvCatalog for MemoryCatalog {
    async fn pool(
        &self,
        filter: &PoolFilter,
        channel_restricted: bool,
        limit: usize,
    ) -> TvResult<Vec<CatalogItem>> {
        let members = filter.collection_id.map(|c| {
            self.collections
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get(&c)
                .cloned()
                .unwrap_or_default()
        });
        let mut out: Vec<CatalogItem> = self
            .items
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .filter(|i| members.as_ref().is_none_or(|m| m.contains(&i.title_id)))
            .filter(|i| filter.matches(i, channel_restricted))
            .cloned()
            .collect();
        let order = |i: &CatalogItem| {
            filter
                .title_ids
                .iter()
                .position(|t| *t == i.title_id)
                .unwrap_or(usize::MAX)
        };
        out.sort_by(|a, b| {
            (
                order(a),
                &a.title,
                a.title_id,
                a.season_number,
                a.episode_number,
            )
                .cmp(&(
                    order(b),
                    &b.title,
                    b.title_id,
                    b.season_number,
                    b.episode_number,
                ))
        });
        out.truncate(limit);
        Ok(out)
    }

    async fn item(&self, title_id: i64, episode_id: Option<i64>) -> TvResult<Option<CatalogItem>> {
        Ok(self
            .items
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .find(|i| i.title_id == title_id && i.episode_id == episode_id)
            .cloned())
    }

    async fn bumpers(&self, limit: usize) -> TvResult<Vec<CatalogItem>> {
        let mut out: Vec<CatalogItem> = self
            .items
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .filter(|i| i.tags.iter().any(|t| t == "filler") && i.duration_ms <= 600_000)
            .cloned()
            .collect();
        out.sort_by_key(|i| (i.title.clone(), i.title_id));
        out.truncate(limit);
        Ok(out)
    }
}

/// A fixture item for tests: `minutes` long, no genres.
pub fn item(title_id: i64, title: &str, minutes: i64) -> CatalogItem {
    CatalogItem {
        title_id,
        episode_id: None,
        asset_id: title_id * 10,
        title: title.into(),
        episode_title: None,
        media_kind: MediaKind::Movie,
        season_number: None,
        episode_number: None,
        year: Some(2000),
        duration_ms: minutes * 60_000,
        age_restricted: false,
        genres: Vec::new(),
        tags: Vec::new(),
    }
}

/// A fixture episode: series `title_id`, episode id and asset id `title_id * 1000 + season * 100 + number`.
pub fn episode(title_id: i64, title: &str, season: i32, number: i32, minutes: i64) -> CatalogItem {
    let n = title_id * 1000 + i64::from(season * 100 + number);
    CatalogItem {
        episode_id: Some(n),
        asset_id: n,
        episode_title: Some(format!("Episode {number}")),
        media_kind: MediaKind::Series,
        season_number: Some(season),
        episode_number: Some(number),
        ..item(title_id, title, minutes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_memory_catalog_filters_and_orders() {
        let c = MemoryCatalog::new();
        let mut a = item(1, "Alpha", 90);
        a.genres = vec!["action".into()];
        let mut b = item(2, "Bravo", 100);
        b.age_restricted = true;
        c.add(a)
            .add(b)
            .add(episode(3, "Cowboy", 1, 2, 24))
            .add(episode(3, "Cowboy", 1, 1, 24));
        let all = c.pool(&PoolFilter::default(), false, 100).await.unwrap();
        let names: Vec<_> = all.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(names, ["Alpha", "Cowboy", "Cowboy"], "restricted left out");
        assert_eq!(all[1].episode_number, Some(1), "episodes in order");
        let on_restricted = c.pool(&PoolFilter::default(), true, 100).await.unwrap();
        assert_eq!(on_restricted.len(), 4);
        let only = PoolFilter {
            restricted: Some(Restricted::Only),
            ..PoolFilter::default()
        };
        assert_eq!(c.pool(&only, false, 100).await.unwrap().len(), 1);
        let action = PoolFilter {
            genres: vec!["action".into()],
            ..PoolFilter::default()
        };
        assert_eq!(c.pool(&action, false, 100).await.unwrap().len(), 1);
        c.collection(9, vec![3]);
        let coll = PoolFilter {
            collection_id: Some(9),
            ..PoolFilter::default()
        };
        assert_eq!(c.pool(&coll, false, 100).await.unwrap().len(), 2);
        assert!(c.item(1, None).await.unwrap().is_some());
        assert!(c.item(1, Some(5)).await.unwrap().is_none());
    }
}
