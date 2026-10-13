//! Member-facing Library endpoints: browse, series, chapters, page manifest,
//! instant search, latest updates and follows.

use super::{MemberCtx, csv, id, map_err};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use serde::Deserialize;
use sr_library::model::*;
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct BrowseParams {
    pub kind: Option<SeriesKind>,
    pub status: Option<SeriesStatus>,
    /// Comma-separated; a series must have every one.
    pub genres: Option<String>,
    /// Comma-separated; a series with any of them is left out.
    pub exclude_genres: Option<String>,
    /// Comma-separated; a series must have every one.
    pub tags: Option<String>,
    /// BCP 47 tag: series with a published chapter in this language.
    pub language: Option<String>,
    pub year_from: Option<i16>,
    pub year_to: Option<i16>,
    /// `include` (default), `hide` or `only`. Everyone may see restricted series; opening their pages needs age confirmation. A member who set `library.hide_age_restricted` gets `hide` as the default.
    pub age: Option<AgeFilter>,
    /// `updated` (default), `newest`, `title` or `followers`.
    pub sort: Option<SortKey>,
    /// `asc` or `desc`; default depends on the sort.
    pub order: Option<String>,
    /// 1 to 100, default 24.
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

pub(super) fn order(raw: &Option<String>) -> ApiResult<Option<bool>> {
    match raw.as_deref() {
        None => Ok(None),
        Some("asc") => Ok(Some(false)),
        Some("desc") => Ok(Some(true)),
        Some(_) => Err(map_err(sr_library::LibraryError::Invalid(
            "order must be asc or desc".into(),
        ))),
    }
}

impl BrowseParams {
    pub(super) fn into_query(self, default_age: AgeFilter) -> ApiResult<BrowseQuery> {
        Ok(BrowseQuery {
            kind: self.kind,
            status: self.status,
            genres: csv(&self.genres),
            exclude_genres: csv(&self.exclude_genres),
            tags: csv(&self.tags),
            language: self.language,
            year_from: self.year_from,
            year_to: self.year_to,
            age: self.age.unwrap_or(default_age),
            sort: self.sort.unwrap_or(SortKey::Updated),
            descending: order(&self.order)?,
            published: None,
            title_contains: None,
            limit: self.limit.unwrap_or(24),
            offset: self.offset.unwrap_or(0),
        })
    }
}

/// Browse the catalog with filters, sort and pagination.
#[utoipa::path(
    get, path = "/library/series", operation_id = "library_browse", params(BrowseParams),
    responses((status = 200, body = SeriesPage), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn browse(ctx: MemberCtx, Query(p): Query<BrowseParams>) -> ApiResult<Json<SeriesPage>> {
    let default_age = if ctx.viewer.hide_restricted {
        AgeFilter::Hide
    } else {
        AgeFilter::Include
    };
    let q = p.into_query(default_age)?;
    ctx.deps
        .service
        .browse(ctx.viewer, q)
        .await
        .map(Json)
        .map_err(map_err)
}

/// A series with its editions and the caller's follow. A merged series answers with its target.
#[utoipa::path(
    get, path = "/library/series/{series_id}", operation_id = "library_series",
    params(("series_id" = String, Path, description = "Series snowflake")),
    responses((status = 200, body = SeriesDetail), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn series(
    ctx: MemberCtx,
    Path(series_id): Path<String>,
) -> ApiResult<Json<SeriesDetail>> {
    ctx.deps
        .service
        .series_detail(ctx.viewer, id(&series_id, "series")?)
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ChapterParams {
    /// Only this edition's chapters.
    pub edition: Option<String>,
    /// Page size, 1 to 500. Without it every chapter is returned.
    pub limit: Option<u32>,
    /// Cursor from the previous page's `next_cursor`.
    pub after: Option<String>,
    /// `asc` (default) or `desc`: edition, then chapter number, then id.
    pub order: Option<String>,
}

impl ChapterParams {
    pub(super) fn into_query(self) -> ApiResult<ChapterQuery> {
        Ok(ChapterQuery {
            edition: self
                .edition
                .as_deref()
                .map(|e| id(e, "edition"))
                .transpose()?,
            after: self.after,
            limit: self.limit,
            descending: order(&self.order)?.unwrap_or(false),
        })
    }
}

/// Chapters of a series with volumes, editions and the caller's read marks.
#[utoipa::path(
    get, path = "/library/series/{series_id}/chapters", operation_id = "library_chapters",
    params(("series_id" = String, Path, description = "Series snowflake"), ChapterParams),
    responses((status = 200, body = ChapterList), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn chapters(
    ctx: MemberCtx,
    Path(series_id): Path<String>,
    Query(p): Query<ChapterParams>,
) -> ApiResult<Json<ChapterList>> {
    ctx.deps
        .service
        .chapters(ctx.viewer, id(&series_id, "series")?, p.into_query()?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// The reader's page manifest: sized, signed image URLs plus neighbouring chapters.
///
/// For an age-restricted series the account must have confirmed it is 18 or older
/// (D-032); otherwise the answer is 403 with code 900011 (`AGE_CONFIRMATION_REQUIRED`)
/// and the client shows the same dialog as age-restricted Lounge channels.
#[utoipa::path(
    get, path = "/library/chapters/{chapter_id}/pages", operation_id = "library_pages",
    params(("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 200, body = PageManifest), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody, description = "Age-restricted series and the account has not confirmed it is 18 or older: error code 900011 (AGE_CONFIRMATION_REQUIRED). Show the 'Are you 18 or older?' dialog, POST /sr/age-gate, then retry."), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn pages(
    ctx: MemberCtx,
    Path(chapter_id): Path<String>,
) -> ApiResult<Json<PageManifest>> {
    ctx.deps
        .service
        .page_manifest(ctx.viewer, id(&chapter_id, "chapter")?)
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SearchParams {
    /// Search-box text; every word is matched as a prefix.
    pub q: String,
    /// 1 to 20, default 8.
    pub limit: Option<usize>,
}

/// Instant search: matching series plus author, genre and tag suggestions.
#[utoipa::path(
    get, path = "/library/search", operation_id = "library_search", params(SearchParams),
    responses((status = 200, body = InstantSearch), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn search(
    ctx: MemberCtx,
    Query(p): Query<SearchParams>,
) -> ApiResult<Json<InstantSearch>> {
    ctx.deps
        .service
        .search(ctx.viewer, &p.q, p.limit.unwrap_or(8))
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct UpdatesParams {
    /// Only followed series.
    pub following: Option<bool>,
    /// Cursor from the previous page's `next_before`.
    pub before: Option<String>,
    /// 1 to 100, default 30.
    pub limit: Option<u32>,
}

/// Latest published chapters, newest first.
#[utoipa::path(
    get, path = "/library/updates", operation_id = "library_updates", params(UpdatesParams),
    responses((status = 200, body = UpdatesPage), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn updates(
    ctx: MemberCtx,
    Query(p): Query<UpdatesParams>,
) -> ApiResult<Json<UpdatesPage>> {
    ctx.deps
        .service
        .updates(
            ctx.viewer,
            p.following.unwrap_or(false),
            p.before.as_deref(),
            p.limit.unwrap_or(30),
        )
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct FollowBody {
    /// The "My library" shelf; default `reading`.
    pub shelf: Option<Shelf>,
    /// New-chapter notifications; default on.
    pub notify: Option<bool>,
}

/// Follow a series, or change its shelf and notification setting.
#[utoipa::path(
    put, path = "/library/series/{series_id}/follow", operation_id = "library_follow", request_body = FollowBody,
    params(("series_id" = String, Path, description = "Series snowflake")),
    responses((status = 200, body = Follow), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn follow(
    ctx: MemberCtx,
    Path(series_id): Path<String>,
    Json(b): Json<FollowBody>,
) -> ApiResult<Json<Follow>> {
    ctx.deps
        .service
        .follow(
            ctx.viewer,
            id(&series_id, "series")?,
            b.shelf.unwrap_or(Shelf::Reading),
            b.notify.unwrap_or(true),
        )
        .await
        .map(Json)
        .map_err(map_err)
}

/// Stop following a series.
#[utoipa::path(
    delete, path = "/library/series/{series_id}/follow", operation_id = "library_unfollow",
    params(("series_id" = String, Path, description = "Series snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn unfollow(ctx: MemberCtx, Path(series_id): Path<String>) -> ApiResult<StatusCode> {
    let removed = ctx
        .deps
        .service
        .unfollow(ctx.viewer, id(&series_id, "series")?)
        .await
        .map_err(map_err)?;
    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(map_err(sr_library::LibraryError::NotFound("follow")))
    }
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct FollowsParams {
    pub shelf: Option<Shelf>,
    /// 1 to 100, default 24.
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// The caller's followed series ("My library"), newest follow first.
#[utoipa::path(
    get, path = "/library/me/follows", operation_id = "library_my_follows", params(FollowsParams),
    responses((status = 200, body = FollowPage), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn my_follows(
    ctx: MemberCtx,
    Query(p): Query<FollowsParams>,
) -> ApiResult<Json<FollowPage>> {
    ctx.deps
        .service
        .my_follows(ctx.viewer, p.shelf, p.limit, p.offset)
        .await
        .map(Json)
        .map_err(map_err)
}
