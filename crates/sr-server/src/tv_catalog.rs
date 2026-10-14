//! The real [`sr_tv::TvCatalog`]: Live TV builds its pools from the Cinema catalog repository.
//!
//! Rules:
//! * Only published, non-merged titles and published episodes are offered (the repository's `Public`
//!   scope), and only when they have an asset and a runtime: a film needs its own `asset_id` and
//!   `runtime_min`, an episode its own asset and `runtime_min` (falling back to the title's).
//! * Age-restricted titles are offered like any other (D-184); the pool filter decides whether a channel
//!   wants them, and opening the stream is what needs age confirmation.
//! * An anime without episodes is a film. Genres and tags are lower case (the catalog lower-cases both on
//!   write), and the item carries both.
//! * Order: the order of `title_ids` when given, else by title; episodes by season and number.

use std::sync::Arc;

use async_trait::async_trait;
use sr_cinema::model::{
    AgeFilter, BrowseQuery, EpisodeRecord, Scope, SortKey, TitleKind, TitleRecord,
};
use sr_cinema::repo::{CinemaRepository, EpisodeQuery};
use sr_tv::model::{MediaKind, PoolFilter, Restricted};
use sr_tv::{CatalogItem, TvCatalog, TvError, TvResult};

/// [`TvCatalog`] over the catalog repository.
pub struct RepoTvCatalog {
    repo: Arc<dyn CinemaRepository>,
}

/// Titles read per pool at most (ten pages of a hundred).
const MAX_TITLES: u32 = 1000;
const PAGE: u32 = 100;

impl RepoTvCatalog {
    pub fn new(repo: Arc<dyn CinemaRepository>) -> Self {
        Self { repo }
    }
}

fn failed(e: sr_cinema::CinemaError) -> TvError {
    TvError::Catalog(e.to_string())
}

fn kind_of(t: &TitleRecord) -> MediaKind {
    match t.kind {
        TitleKind::Movie => MediaKind::Movie,
        TitleKind::Series => MediaKind::Series,
        // An anime with seasons is played by episode; without, it is a film.
        TitleKind::Anime if t.episode_count > 0 || t.season_count > 0 => MediaKind::Anime,
        TitleKind::Anime => MediaKind::Movie,
    }
}

fn minutes_ms(m: Option<i32>) -> Option<i64> {
    m.filter(|m| *m > 0).map(|m| i64::from(m) * 60_000)
}

fn labels(t: &TitleRecord) -> (Vec<String>, Vec<String>) {
    let genres: Vec<String> = t.genres.iter().map(|g| g.to_lowercase()).collect();
    let mut tags: Vec<String> = genres
        .iter()
        .cloned()
        .chain(t.tags.iter().map(|g| g.to_lowercase()))
        .collect();
    tags.sort();
    tags.dedup();
    (genres, tags)
}

fn film_item(t: &TitleRecord) -> Option<CatalogItem> {
    let (genres, tags) = labels(t);
    Some(CatalogItem {
        title_id: t.id,
        episode_id: None,
        asset_id: t.asset_id?,
        title: t.title.clone(),
        episode_title: None,
        media_kind: MediaKind::Movie,
        season_number: None,
        episode_number: None,
        year: t.year,
        duration_ms: minutes_ms(t.runtime_min)?,
        age_restricted: t.age_restricted,
        genres,
        tags,
    })
}

fn episode_item(t: &TitleRecord, e: &EpisodeRecord) -> Option<CatalogItem> {
    if !(e.published && e.title_published && !e.title_merged) {
        return None;
    }
    let (genres, tags) = labels(t);
    Some(CatalogItem {
        title_id: t.id,
        episode_id: Some(e.id),
        asset_id: e.asset_id?,
        title: t.title.clone(),
        episode_title: (!e.title.is_empty()).then(|| e.title.clone()),
        media_kind: kind_of(t),
        season_number: Some(e.season_number),
        episode_number: Some(e.number),
        year: t.year,
        duration_ms: minutes_ms(e.runtime_min).or_else(|| minutes_ms(t.runtime_min))?,
        age_restricted: t.age_restricted,
        genres,
        tags,
    })
}

impl RepoTvCatalog {
    async fn items_of(&self, t: &TitleRecord) -> TvResult<Vec<CatalogItem>> {
        if kind_of(t) == MediaKind::Movie {
            return Ok(film_item(t).into_iter().collect());
        }
        Ok(self
            .repo
            .list_episodes(t.id, &EpisodeQuery::default(), Scope::Public)
            .await
            .map_err(failed)?
            .iter()
            .filter_map(|e| episode_item(t, e))
            .collect())
    }

    async fn titles(&self, f: &PoolFilter, channel_restricted: bool) -> TvResult<Vec<TitleRecord>> {
        if !f.title_ids.is_empty() {
            return self
                .repo
                .titles_by_ids(&f.title_ids, Scope::Public, true)
                .await
                .map_err(failed);
        }
        let mut q = BrowseQuery {
            kind: match f.kinds.as_slice() {
                [MediaKind::Movie] => Some(TitleKind::Movie),
                [MediaKind::Series] => Some(TitleKind::Series),
                _ => None,
            },
            genres: f.genres.clone(),
            tags: f.tags.clone(),
            year_from: f.year_from,
            year_to: f.year_to,
            age: match f.restricted_rule(channel_restricted) {
                Restricted::Exclude => AgeFilter::Hide,
                Restricted::Include => AgeFilter::Include,
                Restricted::Only => AgeFilter::Only,
            },
            collection: f.collection_id,
            sort: SortKey::Title,
            limit: PAGE,
            ..BrowseQuery::default()
        };
        let mut out = Vec::new();
        while q.offset < MAX_TITLES {
            let page = self.repo.browse(&q, Scope::Public).await.map_err(failed)?;
            let n = page.items.len();
            out.extend(page.items);
            if n < PAGE as usize {
                break;
            }
            q.offset += PAGE;
        }
        Ok(out)
    }

    async fn collect(
        &self,
        f: &PoolFilter,
        channel_restricted: bool,
        limit: usize,
    ) -> TvResult<Vec<CatalogItem>> {
        let mut out = Vec::new();
        for t in self.titles(f, channel_restricted).await? {
            for item in self.items_of(&t).await? {
                if f.matches(&item, channel_restricted) {
                    out.push(item);
                }
            }
            if out.len() >= limit {
                break;
            }
        }
        out.truncate(limit);
        Ok(out)
    }
}

#[async_trait]
impl TvCatalog for RepoTvCatalog {
    async fn pool(
        &self,
        filter: &PoolFilter,
        channel_restricted: bool,
        limit: usize,
    ) -> TvResult<Vec<CatalogItem>> {
        self.collect(filter, channel_restricted, limit).await
    }

    async fn item(&self, title_id: i64, episode_id: Option<i64>) -> TvResult<Option<CatalogItem>> {
        let Some(t) = self
            .repo
            .get_title(title_id, Scope::Public)
            .await
            .map_err(failed)?
        else {
            return Ok(None);
        };
        match episode_id {
            None => Ok(if kind_of(&t) == MediaKind::Movie {
                film_item(&t)
            } else {
                None
            }),
            Some(id) => Ok(self
                .repo
                .get_episode(id)
                .await
                .map_err(failed)?
                .filter(|e| e.title_id == title_id)
                .and_then(|e| episode_item(&t, &e))),
        }
    }

    async fn bumpers(&self, limit: usize) -> TvResult<Vec<CatalogItem>> {
        let filter = PoolFilter {
            tags: vec!["filler".into()],
            max_runtime_min: Some(10),
            restricted: Some(Restricted::Exclude),
            ..PoolFilter::default()
        };
        self.collect(&filter, false, limit).await
    }
}
