//! Member-facing Cinema endpoints: discover rows, browse, title detail, seasons and episodes,
//! skip markers, play targets, facets and instant search.

use super::{MemberCtx, csv, id, map_err};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{
    Json,
    extract::{Path, Query},
};
use serde::Deserialize;
use sr_cinema::PlayRef;
use sr_cinema::model::*;
use std::collections::BTreeMap;
use utoipa::IntoParams;

/// The home page: tonight's feature plus the rows in page order.
#[utoipa::path(
    get, path = "/cinema/discover", operation_id = "cinema_discover",
    responses((status = 200, body = Discover), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn discover(ctx: MemberCtx) -> ApiResult<Json<Discover>> {
    ctx.deps
        .service
        .discover(ctx.viewer)
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[allow(dead_code)] // documents `limit`; the handler reads the raw query so provider parameters pass through
pub struct RowParams {
    /// 1 to 50, default 20.
    pub limit: Option<u32>,
}

/// One row by key: `new-this-week`, `recently-added`, `anime-room`, `one-more-episode`,
/// `films`, `animation-banner`, `genre:<name>`, `collection:<id>` or a provided row such as
/// `continue-watching` or `trending` (other query parameters, for example `scope`, go to the
/// provider). An unknown key, or a provided row nobody provides, is 404.
#[utoipa::path(
    get, path = "/cinema/rows/{key}", operation_id = "cinema_row",
    params(("key" = String, Path, description = "Row key"), RowParams),
    responses((status = 200, body = Row), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn row(
    ctx: MemberCtx,
    Path(key): Path<String>,
    Query(mut params): Query<BTreeMap<String, String>>,
) -> ApiResult<Json<Row>> {
    let limit = params.remove("limit").and_then(|l| l.parse::<u32>().ok());
    ctx.deps
        .service
        .row(ctx.viewer, &key, limit, params)
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CollectionParams {
    /// `discover`, `movies`, `series` or `anime`.
    pub surface: Option<Surface>,
}

/// Published curated collections (the "Room selection" dropdown), by position.
#[utoipa::path(
    get, path = "/cinema/collections", operation_id = "cinema_collections", params(CollectionParams),
    responses((status = 200, body = CollectionList), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn collections(
    ctx: MemberCtx,
    Query(p): Query<CollectionParams>,
) -> ApiResult<Json<CollectionList>> {
    ctx.deps
        .service
        .collections(p.surface)
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct BrowseParams {
    pub kind: Option<TitleKind>,
    /// Comma-separated; a title must have every one.
    pub genres: Option<String>,
    /// Comma-separated; a title with any of them is left out.
    pub exclude_genres: Option<String>,
    /// Comma-separated; a title must have every one.
    pub tags: Option<String>,
    pub year_from: Option<i16>,
    pub year_to: Option<i16>,
    /// Comma-separated content ratings (`PG-13,R`); a title with any of them.
    pub rating: Option<String>,
    /// Minutes.
    pub runtime_min: Option<i32>,
    /// Minutes.
    pub runtime_max: Option<i32>,
    /// Audio language (BCP 47).
    pub language: Option<String>,
    /// Subtitle language (BCP 47).
    pub subtitle_language: Option<String>,
    /// `include` (default), `hide` or `only`. Everyone may see restricted titles; opening them needs age confirmation. A member who set `cinema.hide_age_restricted` gets `hide` as the default.
    pub age: Option<AgeFilter>,
    /// Only the titles of this collection ("Room selection").
    pub collection: Option<String>,
    /// `updated` (default), `newest`, `title`, `year`, `runtime`, or `curated` (needs `collection`).
    pub sort: Option<SortKey>,
    /// `asc` or `desc`; default depends on the sort.
    pub order: Option<String>,
    /// 1 to 100, default 24.
    pub limit: Option<u32>,
    /// `next_cursor` of the previous page.
    pub cursor: Option<String>,
}

pub(super) fn order(raw: &Option<String>) -> ApiResult<Option<bool>> {
    match raw.as_deref() {
        None => Ok(None),
        Some("asc") => Ok(Some(false)),
        Some("desc") => Ok(Some(true)),
        Some(_) => Err(map_err(sr_cinema::CinemaError::Invalid(
            "order must be asc or desc".into(),
        ))),
    }
}

impl BrowseParams {
    pub(super) fn into_query(self, default_age: AgeFilter) -> ApiResult<BrowseQuery> {
        Ok(BrowseQuery {
            kind: self.kind,
            genres: csv(&self.genres),
            exclude_genres: csv(&self.exclude_genres),
            tags: csv(&self.tags),
            year_from: self.year_from,
            year_to: self.year_to,
            ratings: csv(&self.rating),
            runtime_min: self.runtime_min,
            runtime_max: self.runtime_max,
            language: self.language,
            subtitle_language: self.subtitle_language,
            age: self.age.unwrap_or(default_age),
            collection: self
                .collection
                .as_deref()
                .map(|c| id(c, "collection"))
                .transpose()?,
            sort: self.sort.unwrap_or(SortKey::Updated),
            descending: order(&self.order)?,
            limit: self.limit.unwrap_or(24),
            ..BrowseQuery::default()
        })
    }
}

/// Browse the catalog with filters, sort and keyset pagination.
#[utoipa::path(
    get, path = "/cinema/titles", operation_id = "cinema_browse", params(BrowseParams),
    responses((status = 200, body = TitlePage), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn browse(ctx: MemberCtx, Query(p): Query<BrowseParams>) -> ApiResult<Json<TitlePage>> {
    let cursor = p.cursor.clone();
    let q = p.into_query(ctx.viewer.default_age())?;
    ctx.deps
        .service
        .browse(ctx.viewer, q, cursor.as_deref())
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct FacetParams {
    pub kind: Option<TitleKind>,
}

/// Genre, rating and language counts plus year and runtime bounds for the filters.
#[utoipa::path(
    get, path = "/cinema/facets", operation_id = "cinema_facets", params(FacetParams),
    responses((status = 200, body = Facets), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn facets(ctx: MemberCtx, Query(p): Query<FacetParams>) -> ApiResult<Json<Facets>> {
    ctx.deps
        .service
        .facets(ctx.viewer, p.kind)
        .await
        .map(Json)
        .map_err(map_err)
}

/// A title with its seasons, variants, trailers and skip markers. A merged title answers with its target.
#[utoipa::path(
    get, path = "/cinema/titles/{title_id}", operation_id = "cinema_title",
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 200, body = TitleDetail), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn title(ctx: MemberCtx, Path(title_id): Path<String>) -> ApiResult<Json<TitleDetail>> {
    ctx.deps
        .service
        .title_detail(ctx.viewer, id(&title_id, "title")?)
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct EpisodeParams {
    /// Only this season's episodes.
    pub season: Option<String>,
    /// Page size, 1 to 500. Without it every episode is returned.
    pub limit: Option<u32>,
    /// Cursor from the previous page's `next_cursor`.
    pub after: Option<String>,
}

/// Seasons and episodes of a title, in season and episode order.
#[utoipa::path(
    get, path = "/cinema/titles/{title_id}/episodes", operation_id = "cinema_episodes",
    params(("title_id" = String, Path, description = "Title snowflake"), EpisodeParams),
    responses((status = 200, body = EpisodeList), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn episodes(
    ctx: MemberCtx,
    Path(title_id): Path<String>,
    Query(p): Query<EpisodeParams>,
) -> ApiResult<Json<EpisodeList>> {
    let season = p.season.as_deref().map(|s| id(s, "season")).transpose()?;
    ctx.deps
        .service
        .episodes(id(&title_id, "title")?, season, p.limit, p.after.as_deref())
        .await
        .map(Json)
        .map_err(map_err)
}

/// One episode.
#[utoipa::path(
    get, path = "/cinema/episodes/{episode_id}", operation_id = "cinema_episode",
    params(("episode_id" = String, Path, description = "Episode snowflake")),
    responses((status = 200, body = Episode), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn episode(ctx: MemberCtx, Path(episode_id): Path<String>) -> ApiResult<Json<Episode>> {
    ctx.deps
        .service
        .episode(id(&episode_id, "episode")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Skip intro / credits / recap windows of a title (films).
#[utoipa::path(
    get, path = "/cinema/titles/{title_id}/markers", operation_id = "cinema_title_markers",
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 200, body = MarkerList), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn title_markers(
    ctx: MemberCtx,
    Path(title_id): Path<String>,
) -> ApiResult<Json<MarkerList>> {
    ctx.deps
        .service
        .title_markers(id(&title_id, "title")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Skip intro / credits / recap windows of an episode.
#[utoipa::path(
    get, path = "/cinema/episodes/{episode_id}/markers", operation_id = "cinema_episode_markers",
    params(("episode_id" = String, Path, description = "Episode snowflake")),
    responses((status = 200, body = MarkerList), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn episode_markers(
    ctx: MemberCtx,
    Path(episode_id): Path<String>,
) -> ApiResult<Json<MarkerList>> {
    ctx.deps
        .service
        .episode_markers(id(&episode_id, "episode")?)
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PlayParams {
    /// A variant (cut) of the title, for example the director's cut.
    pub variant: Option<String>,
}

/// What the player needs to open a title: the asset, markers and (for an episodic title) the
/// first playable episode. A restricted title needs age confirmation (403, code 900011).
#[utoipa::path(
    get, path = "/cinema/titles/{title_id}/play", operation_id = "cinema_play_title",
    params(("title_id" = String, Path, description = "Title snowflake"), PlayParams),
    responses((status = 200, body = PlayTarget), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody, description = "Age-restricted title and the account has not confirmed it is 18 or older: error code 900011 (AGE_CONFIRMATION_REQUIRED). Show the 'Are you 18 or older?' dialog, POST /sr/age-gate, then retry."), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn play_title(
    ctx: MemberCtx,
    Path(title_id): Path<String>,
    Query(p): Query<PlayParams>,
) -> ApiResult<Json<PlayTarget>> {
    let variant = p.variant.as_deref().map(|v| id(v, "variant")).transpose()?;
    ctx.deps
        .service
        .resolve_play(
            ctx.viewer,
            PlayRef::Title {
                id: id(&title_id, "title")?,
                variant,
            },
        )
        .await
        .map(Json)
        .map_err(map_err)
}

/// What the player needs to open an episode, with the next and previous playable episodes.
/// A restricted title needs age confirmation (403, code 900011).
#[utoipa::path(
    get, path = "/cinema/episodes/{episode_id}/play", operation_id = "cinema_play_episode",
    params(("episode_id" = String, Path, description = "Episode snowflake")),
    responses((status = 200, body = PlayTarget), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody, description = "Age-restricted title and the account has not confirmed it is 18 or older: error code 900011 (AGE_CONFIRMATION_REQUIRED). Show the 'Are you 18 or older?' dialog, POST /sr/age-gate, then retry."), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn play_episode(
    ctx: MemberCtx,
    Path(episode_id): Path<String>,
) -> ApiResult<Json<PlayTarget>> {
    ctx.deps
        .service
        .resolve_play(ctx.viewer, PlayRef::Episode(id(&episode_id, "episode")?))
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SearchParams {
    /// Search-box text; every word is matched as a prefix.
    pub q: String,
    /// 1 to 50, default 8.
    pub limit: Option<usize>,
    pub kind: Option<TitleKind>,
}

/// Instant search: matching titles plus people, genre and tag suggestions.
#[utoipa::path(
    get, path = "/cinema/search", operation_id = "cinema_search", params(SearchParams),
    responses((status = 200, body = InstantSearch), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn search(
    ctx: MemberCtx,
    Query(p): Query<SearchParams>,
) -> ApiResult<Json<InstantSearch>> {
    ctx.deps
        .service
        .search(ctx.viewer, &p.q, p.limit.unwrap_or(8), p.kind)
        .await
        .map(Json)
        .map_err(map_err)
}
