//! Social side of the Library: ratings (LIB-SOC-001), reviews (LIB-SOC-003), reading lists
//! (LIB-SOC-006), rankings (LIB-BRW-002) and unread counts (LIB-SOC-008). The logic lives in
//! `sr-library-social`; this module is the HTTP layer and attaches series cards.

use super::{MemberCtx, PublisherCtx, id, map_err as library_err};
use crate::{
    error::{ApiResult, HttpError},
    openapi::ErrorBody,
};
use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sr_core::{ApiError, ErrorCode};
use sr_library::model::Series;
use sr_library_social::model::*;
use sr_library_social::{LibrarySocial, SocialError};
use std::collections::HashMap;
use utoipa::{IntoParams, ToSchema};

fn social_err(e: SocialError) -> HttpError {
    match e {
        SocialError::NotFound(w) => {
            ApiError::with_message(ErrorCode::SrNotFound, format!("Unknown {w}")).into()
        }
        SocialError::AgeConfirmationRequired => ErrorCode::SrAgeConfirmationRequired.into(),
        SocialError::Invalid(m) => ApiError::with_message(ErrorCode::InvalidFormBody, m).into(),
        SocialError::Conflict(m) => ApiError::with_message(ErrorCode::SrConflict, m).into(),
        SocialError::Storage(m) => {
            tracing::error!(error = %m, "library social failure");
            ErrorCode::SrInternal.into()
        }
    }
}

impl MemberCtx {
    fn social(&self) -> ApiResult<&LibrarySocial> {
        self.deps.social.as_ref().ok_or_else(|| {
            tracing::error!("library social service is not configured");
            ErrorCode::SrInternal.into()
        })
    }

    fn sviewer(&self) -> Viewer {
        Viewer {
            user_id: self.viewer.user_id,
            adult: self.viewer.adult,
            hide_restricted: self.viewer.hide_restricted,
        }
    }
}

/// A count after a vote.
#[derive(Debug, Serialize, ToSchema)]
#[schema(as = LibraryCountBody)]
pub struct CountBody {
    pub count: i32,
}

// ---- ratings -----------------------------------------------------------------

/// The caller's rating of a series and the average per scope (everyone, friends, people you know),
/// with the distribution of scores.
#[utoipa::path(
    get, path = "/library/series/{series_id}/ratings", operation_id = "library_series_ratings",
    params(("series_id" = String, Path, description = "Series snowflake")),
    responses((status = 200, body = SeriesRatings), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn series_ratings(
    ctx: MemberCtx,
    Path(series_id): Path<String>,
) -> ApiResult<Json<SeriesRatings>> {
    ctx.social()?
        .ratings(ctx.sviewer(), id(&series_id, "series")?)
        .await
        .map(Json)
        .map_err(social_err)
}

/// Rate a series 1 to 10 (a restricted series needs the 18+ answer).
#[utoipa::path(
    put, path = "/library/series/{series_id}/rating", operation_id = "library_rate_series", request_body = ScoreInput,
    params(("series_id" = String, Path, description = "Series snowflake")),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody, description = "Age confirmation required (900011)."),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn rate_series(
    ctx: MemberCtx,
    Path(series_id): Path<String>,
    Json(b): Json<ScoreInput>,
) -> ApiResult<StatusCode> {
    ctx.social()?
        .rate(ctx.sviewer(), id(&series_id, "series")?, b.score)
        .await
        .map_err(social_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Remove the caller's rating of a series.
#[utoipa::path(
    delete, path = "/library/series/{series_id}/rating", operation_id = "library_unrate_series",
    params(("series_id" = String, Path, description = "Series snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn unrate_series(ctx: MemberCtx, Path(series_id): Path<String>) -> ApiResult<StatusCode> {
    if ctx
        .social()?
        .unrate(ctx.sviewer(), id(&series_id, "series")?)
        .await
        .map_err(social_err)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(social_err(SocialError::NotFound("rating")))
    }
}

/// The caller's rating of a chapter and its average.
#[utoipa::path(
    get, path = "/library/chapters/{chapter_id}/rating", operation_id = "library_chapter_rating",
    params(("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 200, body = ChapterRating), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn chapter_rating(
    ctx: MemberCtx,
    Path(chapter_id): Path<String>,
) -> ApiResult<Json<ChapterRating>> {
    ctx.social()?
        .chapter_rating(ctx.sviewer(), id(&chapter_id, "chapter")?)
        .await
        .map(Json)
        .map_err(social_err)
}

/// Rate a chapter 1 to 10.
#[utoipa::path(
    put, path = "/library/chapters/{chapter_id}/rating", operation_id = "library_rate_chapter", request_body = ScoreInput,
    params(("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn rate_chapter(
    ctx: MemberCtx,
    Path(chapter_id): Path<String>,
    Json(b): Json<ScoreInput>,
) -> ApiResult<StatusCode> {
    ctx.social()?
        .rate_chapter(ctx.sviewer(), id(&chapter_id, "chapter")?, b.score)
        .await
        .map_err(social_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Remove the caller's rating of a chapter.
#[utoipa::path(
    delete, path = "/library/chapters/{chapter_id}/rating", operation_id = "library_unrate_chapter",
    params(("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn unrate_chapter(
    ctx: MemberCtx,
    Path(chapter_id): Path<String>,
) -> ApiResult<StatusCode> {
    if ctx
        .social()?
        .unrate_chapter(ctx.sviewer(), id(&chapter_id, "chapter")?)
        .await
        .map_err(social_err)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(social_err(SocialError::NotFound("rating")))
    }
}

// ---- reviews -----------------------------------------------------------------

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ReviewParams {
    /// `helpful` (default) or `recent`.
    pub sort: Option<ReviewSort>,
    /// Cursor from the previous page's `next_before`.
    pub before: Option<String>,
    /// 1 to 100, default 24.
    pub limit: Option<u32>,
}

/// Reviews of a series.
#[utoipa::path(
    get, path = "/library/series/{series_id}/reviews", operation_id = "library_reviews",
    params(ReviewParams, ("series_id" = String, Path, description = "Series snowflake")),
    responses((status = 200, body = ReviewPage), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn reviews(
    ctx: MemberCtx,
    Path(series_id): Path<String>,
    Query(p): Query<ReviewParams>,
) -> ApiResult<Json<ReviewPage>> {
    ctx.social()?
        .reviews(
            ctx.sviewer(),
            id(&series_id, "series")?,
            p.sort.unwrap_or_default(),
            p.before.as_deref(),
            p.limit,
        )
        .await
        .map(Json)
        .map_err(social_err)
}

/// Write or replace the caller's review of a series (one per account). A `rating` also sets the score.
#[utoipa::path(
    put, path = "/library/series/{series_id}/review", operation_id = "library_put_review", request_body = ReviewInput,
    params(("series_id" = String, Path, description = "Series snowflake")),
    responses((status = 200, body = Review), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody, description = "Age confirmation required (900011)."),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn put_review(
    ctx: MemberCtx,
    Path(series_id): Path<String>,
    Json(b): Json<ReviewInput>,
) -> ApiResult<Json<Review>> {
    ctx.social()?
        .put_review(ctx.sviewer(), id(&series_id, "series")?, b)
        .await
        .map(Json)
        .map_err(social_err)
}

/// Delete the caller's review of a series.
#[utoipa::path(
    delete, path = "/library/series/{series_id}/review", operation_id = "library_delete_review",
    params(("series_id" = String, Path, description = "Series snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_review(ctx: MemberCtx, Path(series_id): Path<String>) -> ApiResult<StatusCode> {
    if ctx
        .social()?
        .delete_review(ctx.sviewer(), id(&series_id, "series")?)
        .await
        .map_err(social_err)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(social_err(SocialError::NotFound("review")))
    }
}

/// Mark a review helpful (idempotent; not on one's own review).
#[utoipa::path(
    put, path = "/library/reviews/{review_id}/helpful", operation_id = "library_review_helpful",
    params(("review_id" = String, Path, description = "Review snowflake")),
    responses((status = 200, body = CountBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn helpful(ctx: MemberCtx, Path(review_id): Path<String>) -> ApiResult<Json<CountBody>> {
    let count = ctx
        .social()?
        .set_helpful(ctx.sviewer(), id(&review_id, "review")?, true)
        .await
        .map_err(social_err)?;
    Ok(Json(CountBody { count }))
}

/// Take back a helpful mark.
#[utoipa::path(
    delete, path = "/library/reviews/{review_id}/helpful", operation_id = "library_review_unhelpful",
    params(("review_id" = String, Path, description = "Review snowflake")),
    responses((status = 200, body = CountBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn unhelpful(
    ctx: MemberCtx,
    Path(review_id): Path<String>,
) -> ApiResult<Json<CountBody>> {
    let count = ctx
        .social()?
        .set_helpful(ctx.sviewer(), id(&review_id, "review")?, false)
        .await
        .map_err(social_err)?;
    Ok(Json(CountBody { count }))
}

// ---- lists -------------------------------------------------------------------

/// A reading list with signed cover URLs for its first series (the fanned stack).
#[derive(Debug, Serialize, ToSchema)]
pub struct ListCard {
    #[serde(flatten)]
    pub list: ReadingList,
    /// Cover URLs of the `preview` series, same order; series without a cover are skipped.
    pub covers: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListsBody {
    pub items: Vec<ListCard>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListItemView {
    pub series: Series,
    pub position: i64,
    pub note: String,
    pub added_at_ms: i64,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = LibraryListDetail)]
pub struct ListDetail {
    pub list: ListCard,
    pub items: Vec<ListItemView>,
}

async fn cards_for(ctx: &MemberCtx, ids: &[i64]) -> ApiResult<HashMap<i64, Series>> {
    Ok(ctx
        .deps
        .service
        .cards(ctx.viewer, ids)
        .await
        .map_err(library_err)?
        .into_iter()
        .map(|s| (s.id, s))
        .collect())
}

async fn list_cards(ctx: &MemberCtx, lists: Vec<ReadingList>) -> ApiResult<Vec<ListCard>> {
    let ids: Vec<i64> = lists
        .iter()
        .flat_map(|l| l.preview.iter().copied())
        .collect();
    let series = cards_for(ctx, &ids).await?;
    Ok(lists
        .into_iter()
        .map(|list| {
            let covers = list
                .preview
                .iter()
                .filter_map(|id| series.get(id).and_then(|s| s.cover_url.clone()))
                .collect();
            ListCard { list, covers }
        })
        .collect())
}

async fn one_card(ctx: &MemberCtx, list: ReadingList) -> ApiResult<ListCard> {
    Ok(list_cards(ctx, vec![list]).await?.remove(0))
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct MyListsParams {
    /// When given, each list says whether it holds this series (`contains`).
    pub series: Option<String>,
}

/// The caller's reading lists.
#[utoipa::path(
    get, path = "/library/me/lists", operation_id = "library_my_lists", params(MyListsParams),
    responses((status = 200, body = ListsBody), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn my_lists(
    ctx: MemberCtx,
    Query(p): Query<MyListsParams>,
) -> ApiResult<Json<ListsBody>> {
    let series = p.series.as_deref().map(|s| id(s, "series")).transpose()?;
    let lists = ctx
        .social()?
        .my_lists(ctx.sviewer(), series)
        .await
        .map_err(social_err)?;
    Ok(Json(ListsBody {
        items: list_cards(&ctx, lists).await?,
    }))
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct FeaturedParams {
    /// 1 to 100, default 24.
    pub limit: Option<u32>,
}

/// Lists staff featured.
#[utoipa::path(
    get, path = "/library/lists/featured", operation_id = "library_featured_lists", params(FeaturedParams),
    responses((status = 200, body = ListsBody), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn featured_lists(
    ctx: MemberCtx,
    Query(p): Query<FeaturedParams>,
) -> ApiResult<Json<ListsBody>> {
    let lists = ctx
        .social()?
        .featured_lists(ctx.sviewer(), p.limit)
        .await
        .map_err(social_err)?;
    Ok(Json(ListsBody {
        items: list_cards(&ctx, lists).await?,
    }))
}

/// Lists of another member that the caller may see.
#[utoipa::path(
    get, path = "/library/users/{user_id}/lists", operation_id = "library_user_lists",
    params(("user_id" = String, Path, description = "Account snowflake")),
    responses((status = 200, body = ListsBody), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn user_lists(ctx: MemberCtx, Path(user_id): Path<String>) -> ApiResult<Json<ListsBody>> {
    let lists = ctx
        .social()?
        .user_lists(ctx.sviewer(), id(&user_id, "user")?)
        .await
        .map_err(social_err)?;
    Ok(Json(ListsBody {
        items: list_cards(&ctx, lists).await?,
    }))
}

/// Create a reading list.
#[utoipa::path(
    post, path = "/library/lists", operation_id = "library_create_list", request_body = ListInput,
    responses((status = 201, body = ListCard), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create_list(
    ctx: MemberCtx,
    Json(b): Json<ListInput>,
) -> ApiResult<(StatusCode, Json<ListCard>)> {
    let list = ctx
        .social()?
        .create_list(ctx.sviewer(), b)
        .await
        .map_err(social_err)?;
    Ok((StatusCode::CREATED, Json(one_card(&ctx, list).await?)))
}

/// A list with its series in order.
#[utoipa::path(
    get, path = "/library/lists/{list_id}", operation_id = "library_get_list",
    params(("list_id" = String, Path, description = "List snowflake")),
    responses((status = 200, body = ListDetail), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get_list(ctx: MemberCtx, Path(list_id): Path<String>) -> ApiResult<Json<ListDetail>> {
    let (list, entries) = ctx
        .social()?
        .get_list(ctx.sviewer(), id(&list_id, "list")?)
        .await
        .map_err(social_err)?;
    let ids: Vec<i64> = entries.iter().map(|e| e.series_id).collect();
    let mut series = cards_for(&ctx, &ids).await?;
    let items = entries
        .into_iter()
        .filter_map(|e| {
            series.remove(&e.series_id).map(|series| ListItemView {
                series,
                position: e.position,
                note: e.note,
                added_at_ms: e.added_at_ms,
            })
        })
        .collect();
    Ok(Json(ListDetail {
        list: one_card(&ctx, list).await?,
        items,
    }))
}

/// Rename a list, change its description or visibility.
#[utoipa::path(
    patch, path = "/library/lists/{list_id}", operation_id = "library_update_list", request_body = ListPatch,
    params(("list_id" = String, Path, description = "List snowflake")),
    responses((status = 200, body = ListCard), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn update_list(
    ctx: MemberCtx,
    Path(list_id): Path<String>,
    Json(b): Json<ListPatch>,
) -> ApiResult<Json<ListCard>> {
    let list = ctx
        .social()?
        .update_list(ctx.sviewer(), id(&list_id, "list")?, b)
        .await
        .map_err(social_err)?;
    Ok(Json(one_card(&ctx, list).await?))
}

/// Delete a list.
#[utoipa::path(
    delete, path = "/library/lists/{list_id}", operation_id = "library_delete_list",
    params(("list_id" = String, Path, description = "List snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_list(ctx: MemberCtx, Path(list_id): Path<String>) -> ApiResult<StatusCode> {
    if ctx
        .social()?
        .delete_list(ctx.sviewer(), id(&list_id, "list")?)
        .await
        .map_err(social_err)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(social_err(SocialError::NotFound("list")))
    }
}

/// Add a series at the end of a list (or change its note).
#[utoipa::path(
    put, path = "/library/lists/{list_id}/items/{series_id}", operation_id = "library_list_add_item", request_body = ListItemInput,
    params(("list_id" = String, Path, description = "List snowflake"), ("series_id" = String, Path, description = "Series snowflake")),
    responses((status = 200, body = ListCard), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn add_item(
    ctx: MemberCtx,
    Path((list_id, series_id)): Path<(String, String)>,
    body: Option<Json<ListItemInput>>,
) -> ApiResult<Json<ListCard>> {
    let note = body.and_then(|Json(b)| b.note);
    let list = ctx
        .social()?
        .add_item(
            ctx.sviewer(),
            id(&list_id, "list")?,
            id(&series_id, "series")?,
            note,
        )
        .await
        .map_err(social_err)?;
    Ok(Json(one_card(&ctx, list).await?))
}

/// Remove a series from a list.
#[utoipa::path(
    delete, path = "/library/lists/{list_id}/items/{series_id}", operation_id = "library_list_remove_item",
    params(("list_id" = String, Path, description = "List snowflake"), ("series_id" = String, Path, description = "Series snowflake")),
    responses((status = 200, body = ListCard), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn remove_item(
    ctx: MemberCtx,
    Path((list_id, series_id)): Path<(String, String)>,
) -> ApiResult<Json<ListCard>> {
    let list = ctx
        .social()?
        .remove_item(
            ctx.sviewer(),
            id(&list_id, "list")?,
            id(&series_id, "series")?,
        )
        .await
        .map_err(social_err)?;
    Ok(Json(one_card(&ctx, list).await?))
}

/// Reorder a list: `series_ids` names every series of the list exactly once.
#[utoipa::path(
    put, path = "/library/lists/{list_id}/order", operation_id = "library_list_reorder", request_body = ListOrderInput,
    params(("list_id" = String, Path, description = "List snowflake")),
    responses((status = 200, body = ListCard), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn reorder(
    ctx: MemberCtx,
    Path(list_id): Path<String>,
    Json(b): Json<ListOrderInput>,
) -> ApiResult<Json<ListCard>> {
    let list = ctx
        .social()?
        .reorder(ctx.sviewer(), id(&list_id, "list")?, b.series_ids)
        .await
        .map_err(social_err)?;
    Ok(Json(one_card(&ctx, list).await?))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct FeatureBody {
    pub featured: bool,
}

/// Feature or unfeature a public reading list on the Library home (`PUBLISH_READING`).
#[utoipa::path(
    post, path = "/admin/library/lists/{list_id}/feature", operation_id = "admin_library_feature_list", request_body = FeatureBody,
    params(("list_id" = String, Path, description = "List snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn feature_list(
    ctx: PublisherCtx,
    Path(list_id): Path<String>,
    Json(b): Json<FeatureBody>,
) -> ApiResult<StatusCode> {
    ctx.actor().publish().map_err(library_err)?;
    let lid = id(&list_id, "list")?;
    ctx.library
        .social
        .as_ref()
        .ok_or_else(|| HttpError::from(ErrorCode::SrInternal))?
        .set_featured(lid, b.featured)
        .await
        .map_err(social_err)?;
    ctx.audit(
        "library.list.feature",
        ("list", lid),
        json!({ "featured": b.featured }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

// ---- rankings and unread counts ----------------------------------------------

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RankingParams {
    /// `week` (default), `month` or `all`.
    pub period: Option<RankingPeriod>,
    /// Limit to one kind (`manga`, `manhwa`, ...).
    pub kind: Option<String>,
    /// 1 to 100, default 24.
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RankingEntry {
    pub rank: u32,
    pub score: f64,
    pub readers: i64,
    pub rating_average: Option<f64>,
    pub rating_count: i64,
    pub series: Series,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RankingsBody {
    pub period: RankingPeriod,
    pub items: Vec<RankingEntry>,
}

/// Most read series for a period (global; an anonymous aggregate).
#[utoipa::path(
    get, path = "/library/rankings", operation_id = "library_rankings", params(RankingParams),
    responses((status = 200, body = RankingsBody), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn rankings(
    ctx: MemberCtx,
    Query(p): Query<RankingParams>,
) -> ApiResult<Json<RankingsBody>> {
    let r = ctx
        .social()?
        .rankings(
            ctx.sviewer(),
            p.period.unwrap_or_default(),
            p.kind.filter(|k| !k.is_empty()),
            p.limit,
        )
        .await
        .map_err(social_err)?;
    let ids: Vec<i64> = r.items.iter().map(|i| i.series_id).collect();
    let mut series = cards_for(&ctx, &ids).await?;
    let items = r
        .items
        .into_iter()
        .filter_map(|i| {
            series.remove(&i.series_id).map(|series| RankingEntry {
                rank: i.rank,
                score: i.score,
                readers: i.readers,
                rating_average: i.rating_average,
                rating_count: i.rating_count,
                series,
            })
        })
        .collect();
    Ok(Json(RankingsBody {
        period: r.period,
        items,
    }))
}

/// Unread published chapters of every series the caller follows (My library).
#[utoipa::path(
    get, path = "/library/me/unread", operation_id = "library_unread_counts",
    responses((status = 200, body = UnreadCounts), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn unread_counts(ctx: MemberCtx) -> ApiResult<Json<UnreadCounts>> {
    ctx.social()?
        .unread_counts(ctx.sviewer())
        .await
        .map(Json)
        .map_err(social_err)
}
