//! The storage port of the Cinema catalog. `sr-db` implements it over Postgres.
//!
//! Reads take a [`Scope`], so a caller cannot forget the published filter (CIN-DATA-006 keeps
//! the age rule in the service: everyone sees restricted titles, D-184). Inputs reaching a
//! write method are already validated by the service.

use async_trait::async_trait;

use crate::error::CinemaResult;
use crate::model::*;

/// A page of rows with the unpaged total.
#[derive(Debug, Clone, PartialEq)]
pub struct Rows<T> {
    pub items: Vec<T>,
    pub total: i64,
}

/// A normalised title write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTitle {
    pub id: i64,
    pub input: TitleInput,
    pub sort_title: String,
    pub created_by: i64,
}

/// An episode list request.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EpisodeQuery {
    pub season: Option<i64>,
    /// `(season number, episode number, id)` of the last row of the previous page.
    pub after: Option<(i32, i32, i64)>,
    /// Rows to return; `None` is all.
    pub limit: Option<u32>,
}

#[async_trait]
pub trait CinemaRepository: Send + Sync {
    // ---- reads ---------------------------------------------------------
    /// Filtered, sorted, keyset-paged titles. `total` ignores the cursor and the limit.
    async fn browse(&self, q: &BrowseQuery, scope: Scope) -> CinemaResult<Rows<TitleRecord>>;
    async fn get_title(&self, id: i64, scope: Scope) -> CinemaResult<Option<TitleRecord>>;
    /// Titles for `ids` in the order given; invisible ones are dropped.
    async fn titles_by_ids(
        &self,
        ids: &[i64],
        scope: Scope,
        include_restricted: bool,
    ) -> CinemaResult<Vec<TitleRecord>>;
    async fn facets(
        &self,
        kind: Option<TitleKind>,
        include_restricted: bool,
    ) -> CinemaResult<Facets>;
    /// Seasons of a title. Public: only seasons with a published episode, counting published ones.
    async fn list_seasons(&self, title_id: i64, scope: Scope) -> CinemaResult<Vec<SeasonRecord>>;
    async fn get_season(&self, id: i64) -> CinemaResult<Option<SeasonRecord>>;
    /// Episodes by season number, episode number and id. Public: published ones.
    async fn list_episodes(
        &self,
        title_id: i64,
        q: &EpisodeQuery,
        scope: Scope,
    ) -> CinemaResult<Vec<EpisodeRecord>>;
    /// An episode with its title's facts. Visibility is NOT applied: the service decides.
    async fn get_episode(&self, id: i64) -> CinemaResult<Option<EpisodeRecord>>;
    /// The published, playable episodes before and after `id` in season and episode order.
    async fn episode_neighbours(&self, id: i64) -> CinemaResult<(Option<i64>, Option<i64>)>;
    async fn first_playable_episode(&self, title_id: i64) -> CinemaResult<Option<i64>>;
    async fn list_variants(&self, title_id: i64) -> CinemaResult<Vec<VariantRecord>>;
    async fn get_variant(&self, id: i64) -> CinemaResult<Option<VariantRecord>>;
    async fn list_trailers(&self, title_id: i64) -> CinemaResult<Vec<TrailerRecord>>;
    /// Skip windows of a title (`episode` None) or of one episode.
    async fn list_markers(
        &self,
        title_id: i64,
        episode: Option<i64>,
    ) -> CinemaResult<Vec<MarkerRecord>>;
    /// Collections by position. Public: published ones, counting published titles.
    async fn list_collections(
        &self,
        surface: Option<Surface>,
        scope: Scope,
    ) -> CinemaResult<Vec<CollectionRecord>>;
    async fn get_collection(&self, id: i64, scope: Scope)
    -> CinemaResult<Option<CollectionRecord>>;
    /// Published, non-merged titles with `id > after`, ascending, for (re)indexing.
    async fn index_batch(&self, after: i64, limit: u32) -> CinemaResult<Vec<TitleRecord>>;

    /// Published, non-merged titles sharing at least one tag or genre with `tags` (lower-case),
    /// minus `exclude`, best matches first. Used by the social recommendations.
    async fn titles_sharing_tags(
        &self,
        _tags: &[String],
        _exclude: &[i64],
        _limit: u32,
    ) -> CinemaResult<Vec<TitleRecord>> {
        Ok(Vec::new())
    }
    /// Which catalog row uses `asset_id` (feature, cut or trailer). Visibility is NOT applied:
    /// the service decides. `None`: no row links the asset.
    async fn asset_owner(&self, _asset_id: i64) -> CinemaResult<Option<AssetOwner>> {
        Ok(None)
    }
    /// Members with a live (not revoked) contribution credit for the title or one of its episodes,
    /// best first. Members who opted out of the boards are left out.
    async fn title_contributors(&self, _title_id: i64) -> CinemaResult<Vec<Contributor>> {
        Ok(Vec::new())
    }
    // ---- title writes --------------------------------------------------
    async fn create_title(&self, t: &NewTitle) -> CinemaResult<TitleRecord>;
    async fn update_title(
        &self,
        id: i64,
        input: &TitleInput,
        sort_title: &str,
    ) -> CinemaResult<Option<TitleRecord>>;
    /// Publish or unpublish. Publishing stamps `published_at` once.
    async fn set_title_published(
        &self,
        id: i64,
        published: bool,
    ) -> CinemaResult<Option<TitleRecord>>;
    async fn set_title_asset(
        &self,
        id: i64,
        asset: Option<i64>,
    ) -> CinemaResult<Option<TitleRecord>>;
    /// Delete a title with everything under it; returns the ids of its episodes.
    async fn delete_title(&self, id: i64) -> CinemaResult<Option<Vec<i64>>>;
    /// Move seasons, episodes, variants, trailers, markers and collection places of `source`
    /// into `target` and unpublish `source` (kept as a redirect).
    async fn merge_titles(&self, source: i64, target: i64) -> CinemaResult<MergeOutcome>;

    // ---- seasons and episodes --------------------------------------------
    async fn create_season(
        &self,
        id: i64,
        title: i64,
        input: &SeasonInput,
    ) -> CinemaResult<SeasonRecord>;
    async fn update_season(
        &self,
        id: i64,
        input: &SeasonInput,
    ) -> CinemaResult<Option<SeasonRecord>>;
    /// Delete a season; returns its title and the ids of its episodes.
    async fn delete_season(&self, id: i64) -> CinemaResult<Option<(i64, Vec<i64>)>>;
    async fn create_episode(
        &self,
        id: i64,
        season: i64,
        input: &EpisodeInput,
    ) -> CinemaResult<EpisodeRecord>;
    async fn update_episode(
        &self,
        id: i64,
        input: &EpisodeInput,
    ) -> CinemaResult<Option<EpisodeRecord>>;
    async fn set_episode_published(
        &self,
        id: i64,
        published: bool,
    ) -> CinemaResult<Option<EpisodeRecord>>;
    async fn set_episode_asset(
        &self,
        id: i64,
        asset: Option<i64>,
    ) -> CinemaResult<Option<EpisodeRecord>>;
    /// Delete an episode; returns its title.
    async fn delete_episode(&self, id: i64) -> CinemaResult<Option<i64>>;

    // ---- variants, trailers, markers ----------------------------------------
    async fn create_variant(
        &self,
        id: i64,
        title: i64,
        input: &VariantInput,
    ) -> CinemaResult<VariantRecord>;
    async fn update_variant(
        &self,
        id: i64,
        input: &VariantInput,
    ) -> CinemaResult<Option<VariantRecord>>;
    async fn delete_variant(&self, id: i64) -> CinemaResult<bool>;
    async fn create_trailer(
        &self,
        id: i64,
        title: i64,
        input: &TrailerInput,
    ) -> CinemaResult<TrailerRecord>;
    async fn update_trailer(
        &self,
        id: i64,
        input: &TrailerInput,
    ) -> CinemaResult<Option<TrailerRecord>>;
    async fn delete_trailer(&self, id: i64) -> CinemaResult<bool>;
    /// Replace the whole set of windows of a title or an episode. `ids` supplies one new id per
    /// window. Returns `None` when the title or episode does not exist.
    async fn replace_markers(
        &self,
        title_id: i64,
        episode: Option<i64>,
        markers: &[MarkerInput],
        ids: &[i64],
        created_by: i64,
    ) -> CinemaResult<Option<Vec<MarkerRecord>>>;

    // ---- collections ---------------------------------------------------------
    async fn create_collection(
        &self,
        id: i64,
        input: &CollectionInput,
        created_by: i64,
    ) -> CinemaResult<CollectionRecord>;
    async fn update_collection(
        &self,
        id: i64,
        input: &CollectionInput,
    ) -> CinemaResult<Option<CollectionRecord>>;
    async fn delete_collection(&self, id: i64) -> CinemaResult<bool>;
    /// Replace the titles of a collection, in order. `None` when the collection is unknown;
    /// an unknown title id is `Invalid`.
    async fn set_collection_items(
        &self,
        id: i64,
        titles: &[i64],
    ) -> CinemaResult<Option<CollectionRecord>>;
}

/// How a catalog row uses an asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetUse {
    /// The title's own asset (a film) or an episode's asset.
    Feature,
    /// A cut (variant) of the title; carries the variant id.
    Variant(i64),
    /// A trailer of the title; carries the trailer id.
    Trailer(i64),
}

/// The catalog row that links an asset (see [`CinemaRepository::asset_owner`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetOwner {
    pub title_id: i64,
    pub episode_id: Option<i64>,
    pub usage: AssetUse,
}
