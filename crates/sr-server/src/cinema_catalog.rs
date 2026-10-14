//! The real [`sr_cinema_social::CatalogLookup`]: the social features (progress, ratings, comments, lists)
//! resolve titles and episodes through the Cinema catalog repository.
//!
//! Rules:
//! * Only published, non-merged titles and published episodes are returned (the repository's `Public`
//!   scope). Age-restricted titles are returned like any other (D-184): the social service gates play-like
//!   calls with the viewer's age confirmation (`900011`), it never needs the catalog to hide them.
//! * An anime title without episodes is a film in the catalog's own words, so it is reported as a movie
//!   (`episode_id` 0); an episodic title is `series` or `anime`.
//! * Runtimes are minutes in the catalog and seconds here.
//! * `tags` are the title's genres plus its tags, lower-case (the catalog lower-cases both on write).

use std::sync::Arc;

use async_trait::async_trait;
use sr_cinema::model::{EpisodeRecord, Scope, TitleKind as CatalogKind, TitleRecord};
use sr_cinema::repo::{CinemaRepository, EpisodeQuery};
use sr_cinema_social::{
    CatalogEpisode, CatalogError, CatalogLookup, CatalogResult, CatalogTitle, TitleKind,
};

/// [`CatalogLookup`] over the catalog repository.
pub struct RepoCatalog {
    repo: Arc<dyn CinemaRepository>,
}

impl RepoCatalog {
    pub fn new(repo: Arc<dyn CinemaRepository>) -> Self {
        Self { repo }
    }
}

fn failed(e: sr_cinema::CinemaError) -> CatalogError {
    CatalogError(e.to_string())
}

fn title_of(r: &TitleRecord) -> CatalogTitle {
    let kind = match r.kind {
        CatalogKind::Movie => TitleKind::Movie,
        CatalogKind::Series => TitleKind::Series,
        // An anime with seasons is played by episode; without, it is a film.
        CatalogKind::Anime if r.episode_count > 0 || r.season_count > 0 => TitleKind::Anime,
        CatalogKind::Anime => TitleKind::Movie,
    };
    let mut tags: Vec<String> = r.genres.iter().chain(r.tags.iter()).cloned().collect();
    tags.sort();
    tags.dedup();
    CatalogTitle {
        id: r.id,
        kind,
        title: r.title.clone(),
        year: r.year.map(i32::from),
        age_restricted: r.age_restricted,
        runtime_secs: r.runtime_min.map(|m| m.saturating_mul(60)),
        poster_key: r.poster_key.clone(),
        tags,
    }
}

fn episode_of(e: &EpisodeRecord) -> CatalogEpisode {
    CatalogEpisode {
        id: e.id,
        title_id: e.title_id,
        season: e.season_number,
        number: e.number,
        title: (!e.title.is_empty()).then(|| e.title.clone()),
        runtime_secs: e.runtime_min.map(|m| m.saturating_mul(60)),
    }
}

/// A published episode of a published, non-merged title.
fn visible(e: &EpisodeRecord) -> bool {
    e.published && e.title_published && !e.title_merged
}

#[async_trait]
impl CatalogLookup for RepoCatalog {
    async fn title(&self, id: i64) -> CatalogResult<Option<CatalogTitle>> {
        Ok(self
            .repo
            .get_title(id, Scope::Public)
            .await
            .map_err(failed)?
            .as_ref()
            .map(title_of))
    }

    async fn titles(&self, ids: &[i64]) -> CatalogResult<Vec<CatalogTitle>> {
        Ok(self
            .repo
            .titles_by_ids(ids, Scope::Public, true)
            .await
            .map_err(failed)?
            .iter()
            .map(title_of)
            .collect())
    }

    async fn episode(
        &self,
        title_id: i64,
        episode_id: i64,
    ) -> CatalogResult<Option<CatalogEpisode>> {
        Ok(self
            .repo
            .get_episode(episode_id)
            .await
            .map_err(failed)?
            .filter(|e| e.title_id == title_id && visible(e))
            .as_ref()
            .map(episode_of))
    }

    async fn episodes(
        &self,
        title_id: i64,
        season: Option<i32>,
    ) -> CatalogResult<Vec<CatalogEpisode>> {
        let q = EpisodeQuery::default();
        Ok(self
            .repo
            .list_episodes(title_id, &q, Scope::Public)
            .await
            .map_err(failed)?
            .iter()
            .filter(|e| visible(e) && season.is_none_or(|s| e.season_number == s))
            .map(episode_of)
            .collect())
    }

    async fn next_episode(
        &self,
        title_id: i64,
        after_episode_id: i64,
    ) -> CatalogResult<Option<CatalogEpisode>> {
        // The repository's neighbour query only counts episodes that can be played (have an asset).
        let (_, next) = self
            .repo
            .episode_neighbours(after_episode_id)
            .await
            .map_err(failed)?;
        match next {
            Some(id) => self.episode(title_id, id).await,
            None => Ok(None),
        }
    }

    async fn titles_with_tags(
        &self,
        tags: &[String],
        exclude: &[i64],
        limit: u32,
    ) -> CatalogResult<Vec<CatalogTitle>> {
        Ok(self
            .repo
            .titles_sharing_tags(tags, exclude, limit)
            .await
            .map_err(failed)?
            .iter()
            .map(title_of)
            .collect())
    }
}
