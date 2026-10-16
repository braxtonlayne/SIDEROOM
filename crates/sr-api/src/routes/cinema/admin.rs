//! Publisher endpoints (`/admin/cinema/*`): titles, seasons, episodes, variants, trailers,
//! skip markers and collections. Permission checks live in `sr_cinema::CinemaService`; every
//! successful change is also written to the staff access log.

use super::member::{BrowseParams, order};
use super::{PublisherCtx, id, map_err};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::json;
use sr_cinema::model::*;
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AdminListParams {
    pub kind: Option<TitleKind>,
    /// `true` for published, `false` for drafts and unpublished titles.
    pub published: Option<bool>,
    /// Title substring.
    pub title: Option<String>,
    /// `include` (default), `hide` or `only`.
    pub age: Option<AgeFilter>,
    /// `updated` (default), `newest`, `title`, `year` or `runtime`.
    pub sort: Option<SortKey>,
    pub order: Option<String>,
    /// 1 to 100, default 24.
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// All titles, drafts included (`PUBLISH_MEDIA`).
#[utoipa::path(
    get, path = "/admin/cinema/titles", operation_id = "admin_cinema_list", params(AdminListParams),
    responses((status = 200, body = TitlePage), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list(
    ctx: PublisherCtx,
    Query(p): Query<AdminListParams>,
) -> ApiResult<Json<TitlePage>> {
    order(&p.order)?;
    let base = BrowseParams {
        kind: p.kind,
        age: p.age,
        sort: p.sort,
        order: p.order.clone(),
        limit: p.limit,
        ..BrowseParams::default()
    };
    let mut q = base.into_query(AgeFilter::Include)?;
    q.published = p.published;
    q.title_contains = p.title;
    q.offset = p.offset.unwrap_or(0);
    ctx.svc()
        .admin_list(ctx.actor(), q)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Create a draft title (`PUBLISH_MEDIA`).
#[utoipa::path(
    post, path = "/admin/cinema/titles", operation_id = "admin_cinema_create_title", request_body = TitleInput,
    responses((status = 201, body = Title), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create_title(
    ctx: PublisherCtx,
    Json(b): Json<TitleInput>,
) -> ApiResult<(StatusCode, Json<Title>)> {
    let t = ctx
        .svc()
        .create_title(ctx.actor(), b)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "cinema.title.create",
        ("title", t.id),
        json!({ "title": t.title }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(t)))
}

/// A title with everything under it, published or not (`PUBLISH_MEDIA`).
#[utoipa::path(
    get, path = "/admin/cinema/titles/{title_id}", operation_id = "admin_cinema_title",
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 200, body = AdminTitleDetail), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn title(
    ctx: PublisherCtx,
    Path(title_id): Path<String>,
) -> ApiResult<Json<AdminTitleDetail>> {
    ctx.svc()
        .admin_title(ctx.actor(), id(&title_id, "title")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Replace the editable fields of a title (`PUBLISH_MEDIA`). The asset link is untouched.
#[utoipa::path(
    put, path = "/admin/cinema/titles/{title_id}", operation_id = "admin_cinema_update_title", request_body = TitleInput,
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 200, body = Title), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn update_title(
    ctx: PublisherCtx,
    Path(title_id): Path<String>,
    Json(b): Json<TitleInput>,
) -> ApiResult<Json<Title>> {
    let tid = id(&title_id, "title")?;
    let t = ctx
        .svc()
        .update_title(ctx.actor(), tid, b)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "cinema.title.update",
        ("title", tid),
        json!({ "title": t.title }),
    )
    .await;
    Ok(Json(t))
}

/// Delete a title with everything under it (`DELETE_CONTENT`).
#[utoipa::path(
    delete, path = "/admin/cinema/titles/{title_id}", operation_id = "admin_cinema_delete_title",
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_title(
    ctx: PublisherCtx,
    Path(title_id): Path<String>,
) -> ApiResult<StatusCode> {
    let tid = id(&title_id, "title")?;
    ctx.svc()
        .delete_title(ctx.actor(), tid)
        .await
        .map_err(map_err)?;
    ctx.audit("cinema.title.delete", ("title", tid), json!({}))
        .await;
    Ok(StatusCode::NO_CONTENT)
}

/// Publish a title (`PUBLISH_MEDIA`). A title without a playable asset or a published episode stays a draft (409).
#[utoipa::path(
    post, path = "/admin/cinema/titles/{title_id}/publish", operation_id = "admin_cinema_publish_title",
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 200, body = Title), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn publish_title(
    ctx: PublisherCtx,
    Path(title_id): Path<String>,
) -> ApiResult<Json<Title>> {
    set_title_published(ctx, title_id, true).await
}

/// Unpublish a title (`PUBLISH_MEDIA` or `DELETE_CONTENT`).
#[utoipa::path(
    post, path = "/admin/cinema/titles/{title_id}/unpublish", operation_id = "admin_cinema_unpublish_title",
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 200, body = Title), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn unpublish_title(
    ctx: PublisherCtx,
    Path(title_id): Path<String>,
) -> ApiResult<Json<Title>> {
    set_title_published(ctx, title_id, false).await
}

async fn set_title_published(
    ctx: PublisherCtx,
    title_id: String,
    published: bool,
) -> ApiResult<Json<Title>> {
    let tid = id(&title_id, "title")?;
    let t = ctx
        .svc()
        .set_title_published(ctx.actor(), tid, published)
        .await
        .map_err(map_err)?;
    let action = if published {
        "cinema.title.publish"
    } else {
        "cinema.title.unpublish"
    };
    ctx.audit(action, ("title", tid), json!({})).await;
    Ok(Json(t))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = CinemaMergeBody)]
pub struct MergeBody {
    /// The title that stays; this one is merged into it.
    pub target_id: String,
}

/// Merge this title into another (`PUBLISH_MEDIA` and `DELETE_CONTENT`): seasons, episodes,
/// variants, trailers, markers and collection places move (the target wins a clash and fills
/// its gaps from this title); this title stays as a redirect.
#[utoipa::path(
    post, path = "/admin/cinema/titles/{title_id}/merge", operation_id = "admin_cinema_merge_title", request_body = MergeBody,
    params(("title_id" = String, Path, description = "Title snowflake to merge away")),
    responses((status = 200, body = Title), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn merge_title(
    ctx: PublisherCtx,
    Path(title_id): Path<String>,
    Json(b): Json<MergeBody>,
) -> ApiResult<Json<Title>> {
    let source = id(&title_id, "title")?;
    let target = id(&b.target_id, "title")?;
    let t = ctx
        .svc()
        .merge_titles(ctx.actor(), source, target)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "cinema.title.merge",
        ("title", source),
        json!({ "target": target.to_string() }),
    )
    .await;
    Ok(Json(t))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AssetBody {
    /// The playable asset, or `null` to unlink it. Unlinking the last playable thing of a
    /// published title unpublishes it.
    pub asset_id: Option<String>,
}

impl AssetBody {
    fn asset(&self) -> Result<Option<i64>, crate::error::HttpError> {
        self.asset_id.as_deref().map(|a| id(a, "asset")).transpose()
    }
}

/// Link or unlink the playable asset of a film (`PUBLISH_MEDIA`).
#[utoipa::path(
    put, path = "/admin/cinema/titles/{title_id}/asset", operation_id = "admin_cinema_link_title_asset", request_body = AssetBody,
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 200, body = Title), (status = 404, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn link_title_asset(
    ctx: PublisherCtx,
    Path(title_id): Path<String>,
    Json(b): Json<AssetBody>,
) -> ApiResult<Json<Title>> {
    let tid = id(&title_id, "title")?;
    let asset = b.asset()?;
    let t = ctx
        .svc()
        .link_title_asset(ctx.actor(), tid, asset)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "cinema.title.asset",
        ("title", tid),
        json!({ "asset": asset.map(|a| a.to_string()) }),
    )
    .await;
    Ok(Json(t))
}

/// Add a season (`PUBLISH_MEDIA`).
#[utoipa::path(
    post, path = "/admin/cinema/titles/{title_id}/seasons", operation_id = "admin_cinema_create_season", request_body = SeasonInput,
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 201, body = Season), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create_season(
    ctx: PublisherCtx,
    Path(title_id): Path<String>,
    Json(b): Json<SeasonInput>,
) -> ApiResult<(StatusCode, Json<Season>)> {
    let tid = id(&title_id, "title")?;
    let s = ctx
        .svc()
        .create_season(ctx.actor(), tid, b)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "cinema.season.create",
        ("season", s.id),
        json!({ "title": tid.to_string(), "number": s.number }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(s)))
}

/// Replace the editable fields of a season (`PUBLISH_MEDIA`).
#[utoipa::path(
    put, path = "/admin/cinema/seasons/{season_id}", operation_id = "admin_cinema_update_season", request_body = SeasonInput,
    params(("season_id" = String, Path, description = "Season snowflake")),
    responses((status = 200, body = Season), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn update_season(
    ctx: PublisherCtx,
    Path(season_id): Path<String>,
    Json(b): Json<SeasonInput>,
) -> ApiResult<Json<Season>> {
    let sid = id(&season_id, "season")?;
    let s = ctx
        .svc()
        .update_season(ctx.actor(), sid, b)
        .await
        .map_err(map_err)?;
    ctx.audit("cinema.season.update", ("season", sid), json!({}))
        .await;
    Ok(Json(s))
}

/// Delete a season with its episodes (`DELETE_CONTENT`).
#[utoipa::path(
    delete, path = "/admin/cinema/seasons/{season_id}", operation_id = "admin_cinema_delete_season",
    params(("season_id" = String, Path, description = "Season snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_season(
    ctx: PublisherCtx,
    Path(season_id): Path<String>,
) -> ApiResult<StatusCode> {
    let sid = id(&season_id, "season")?;
    ctx.svc()
        .delete_season(ctx.actor(), sid)
        .await
        .map_err(map_err)?;
    ctx.audit("cinema.season.delete", ("season", sid), json!({}))
        .await;
    Ok(StatusCode::NO_CONTENT)
}

/// Add an episode (`PUBLISH_MEDIA`).
#[utoipa::path(
    post, path = "/admin/cinema/seasons/{season_id}/episodes", operation_id = "admin_cinema_create_episode", request_body = EpisodeInput,
    params(("season_id" = String, Path, description = "Season snowflake")),
    responses((status = 201, body = Episode), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create_episode(
    ctx: PublisherCtx,
    Path(season_id): Path<String>,
    Json(b): Json<EpisodeInput>,
) -> ApiResult<(StatusCode, Json<Episode>)> {
    let sid = id(&season_id, "season")?;
    let e = ctx
        .svc()
        .create_episode(ctx.actor(), sid, b)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "cinema.episode.create",
        ("episode", e.id),
        json!({ "season": sid.to_string(), "number": e.number }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(e)))
}

/// Replace the editable fields of an episode (`PUBLISH_MEDIA`). The asset link is untouched.
#[utoipa::path(
    put, path = "/admin/cinema/episodes/{episode_id}", operation_id = "admin_cinema_update_episode", request_body = EpisodeInput,
    params(("episode_id" = String, Path, description = "Episode snowflake")),
    responses((status = 200, body = Episode), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn update_episode(
    ctx: PublisherCtx,
    Path(episode_id): Path<String>,
    Json(b): Json<EpisodeInput>,
) -> ApiResult<Json<Episode>> {
    let eid = id(&episode_id, "episode")?;
    let e = ctx
        .svc()
        .update_episode(ctx.actor(), eid, b)
        .await
        .map_err(map_err)?;
    ctx.audit("cinema.episode.update", ("episode", eid), json!({}))
        .await;
    Ok(Json(e))
}

/// Delete an episode (`DELETE_CONTENT`). Deleting the last playable thing of a published title unpublishes it.
#[utoipa::path(
    delete, path = "/admin/cinema/episodes/{episode_id}", operation_id = "admin_cinema_delete_episode",
    params(("episode_id" = String, Path, description = "Episode snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_episode(
    ctx: PublisherCtx,
    Path(episode_id): Path<String>,
) -> ApiResult<StatusCode> {
    let eid = id(&episode_id, "episode")?;
    ctx.svc()
        .delete_episode(ctx.actor(), eid)
        .await
        .map_err(map_err)?;
    ctx.audit("cinema.episode.delete", ("episode", eid), json!({}))
        .await;
    Ok(StatusCode::NO_CONTENT)
}

/// Publish an episode (`PUBLISH_MEDIA`). An episode without an asset stays a draft (409).
#[utoipa::path(
    post, path = "/admin/cinema/episodes/{episode_id}/publish", operation_id = "admin_cinema_publish_episode",
    params(("episode_id" = String, Path, description = "Episode snowflake")),
    responses((status = 200, body = Episode), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn publish_episode(
    ctx: PublisherCtx,
    Path(episode_id): Path<String>,
) -> ApiResult<Json<Episode>> {
    set_episode_published(ctx, episode_id, true).await
}

/// Unpublish an episode (`PUBLISH_MEDIA` or `DELETE_CONTENT`).
#[utoipa::path(
    post, path = "/admin/cinema/episodes/{episode_id}/unpublish", operation_id = "admin_cinema_unpublish_episode",
    params(("episode_id" = String, Path, description = "Episode snowflake")),
    responses((status = 200, body = Episode), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn unpublish_episode(
    ctx: PublisherCtx,
    Path(episode_id): Path<String>,
) -> ApiResult<Json<Episode>> {
    set_episode_published(ctx, episode_id, false).await
}

async fn set_episode_published(
    ctx: PublisherCtx,
    episode_id: String,
    published: bool,
) -> ApiResult<Json<Episode>> {
    let eid = id(&episode_id, "episode")?;
    let e = ctx
        .svc()
        .set_episode_published(ctx.actor(), eid, published)
        .await
        .map_err(map_err)?;
    let action = if published {
        "cinema.episode.publish"
    } else {
        "cinema.episode.unpublish"
    };
    ctx.audit(action, ("episode", eid), json!({})).await;
    Ok(Json(e))
}

/// Link or unlink the playable asset of an episode (`PUBLISH_MEDIA`). Unlinking unpublishes the episode.
#[utoipa::path(
    put, path = "/admin/cinema/episodes/{episode_id}/asset", operation_id = "admin_cinema_link_episode_asset", request_body = AssetBody,
    params(("episode_id" = String, Path, description = "Episode snowflake")),
    responses((status = 200, body = Episode), (status = 404, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn link_episode_asset(
    ctx: PublisherCtx,
    Path(episode_id): Path<String>,
    Json(b): Json<AssetBody>,
) -> ApiResult<Json<Episode>> {
    let eid = id(&episode_id, "episode")?;
    let asset = b.asset()?;
    let e = ctx
        .svc()
        .link_episode_asset(ctx.actor(), eid, asset)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "cinema.episode.asset",
        ("episode", eid),
        json!({ "asset": asset.map(|a| a.to_string()) }),
    )
    .await;
    Ok(Json(e))
}

/// Skip windows of an episode, published or not (`PUBLISH_MEDIA`).
#[utoipa::path(
    get, path = "/admin/cinema/episodes/{episode_id}/markers", operation_id = "admin_cinema_episode_markers",
    params(("episode_id" = String, Path, description = "Episode snowflake")),
    responses((status = 200, body = MarkerList), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn episode_markers(
    ctx: PublisherCtx,
    Path(episode_id): Path<String>,
) -> ApiResult<Json<MarkerList>> {
    ctx.svc()
        .admin_episode_markers(ctx.actor(), id(&episode_id, "episode")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Replace the skip windows of a title (`PUBLISH_MEDIA`): at most one of each kind, inside 24 hours.
#[utoipa::path(
    put, path = "/admin/cinema/titles/{title_id}/markers", operation_id = "admin_cinema_replace_title_markers", request_body = MarkersBody,
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 200, body = MarkerList), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn replace_title_markers(
    ctx: PublisherCtx,
    Path(title_id): Path<String>,
    Json(b): Json<MarkersBody>,
) -> ApiResult<Json<MarkerList>> {
    let tid = id(&title_id, "title")?;
    let list = ctx
        .svc()
        .replace_markers(ctx.actor(), Some(tid), None, b.markers)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "cinema.markers.replace",
        ("title", tid),
        json!({ "count": list.markers.len() }),
    )
    .await;
    Ok(Json(list))
}

/// Replace the skip windows of an episode (`PUBLISH_MEDIA`).
#[utoipa::path(
    put, path = "/admin/cinema/episodes/{episode_id}/markers", operation_id = "admin_cinema_replace_episode_markers", request_body = MarkersBody,
    params(("episode_id" = String, Path, description = "Episode snowflake")),
    responses((status = 200, body = MarkerList), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn replace_episode_markers(
    ctx: PublisherCtx,
    Path(episode_id): Path<String>,
    Json(b): Json<MarkersBody>,
) -> ApiResult<Json<MarkerList>> {
    let eid = id(&episode_id, "episode")?;
    let list = ctx
        .svc()
        .replace_markers(ctx.actor(), None, Some(eid), b.markers)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "cinema.markers.replace",
        ("episode", eid),
        json!({ "count": list.markers.len() }),
    )
    .await;
    Ok(Json(list))
}

/// Add a variant (another cut) to a title (`PUBLISH_MEDIA`).
#[utoipa::path(
    post, path = "/admin/cinema/titles/{title_id}/variants", operation_id = "admin_cinema_create_variant", request_body = VariantInput,
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 201, body = Variant), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create_variant(
    ctx: PublisherCtx,
    Path(title_id): Path<String>,
    Json(b): Json<VariantInput>,
) -> ApiResult<(StatusCode, Json<Variant>)> {
    let tid = id(&title_id, "title")?;
    let v = ctx
        .svc()
        .create_variant(ctx.actor(), tid, b)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "cinema.variant.create",
        ("variant", v.id),
        json!({ "title": tid.to_string(), "label": v.label }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(v)))
}

/// Replace a variant (`PUBLISH_MEDIA`).
#[utoipa::path(
    put, path = "/admin/cinema/variants/{variant_id}", operation_id = "admin_cinema_update_variant", request_body = VariantInput,
    params(("variant_id" = String, Path, description = "Variant snowflake")),
    responses((status = 200, body = Variant), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn update_variant(
    ctx: PublisherCtx,
    Path(variant_id): Path<String>,
    Json(b): Json<VariantInput>,
) -> ApiResult<Json<Variant>> {
    let vid = id(&variant_id, "variant")?;
    let v = ctx
        .svc()
        .update_variant(ctx.actor(), vid, b)
        .await
        .map_err(map_err)?;
    ctx.audit("cinema.variant.update", ("variant", vid), json!({}))
        .await;
    Ok(Json(v))
}

/// Delete a variant (`PUBLISH_MEDIA`).
#[utoipa::path(
    delete, path = "/admin/cinema/variants/{variant_id}", operation_id = "admin_cinema_delete_variant",
    params(("variant_id" = String, Path, description = "Variant snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_variant(
    ctx: PublisherCtx,
    Path(variant_id): Path<String>,
) -> ApiResult<StatusCode> {
    let vid = id(&variant_id, "variant")?;
    ctx.svc()
        .delete_variant(ctx.actor(), vid)
        .await
        .map_err(map_err)?;
    ctx.audit("cinema.variant.delete", ("variant", vid), json!({}))
        .await;
    Ok(StatusCode::NO_CONTENT)
}

/// Add a trailer to a title (`PUBLISH_MEDIA`).
#[utoipa::path(
    post, path = "/admin/cinema/titles/{title_id}/trailers", operation_id = "admin_cinema_create_trailer", request_body = TrailerInput,
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 201, body = Trailer), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create_trailer(
    ctx: PublisherCtx,
    Path(title_id): Path<String>,
    Json(b): Json<TrailerInput>,
) -> ApiResult<(StatusCode, Json<Trailer>)> {
    let tid = id(&title_id, "title")?;
    let t = ctx
        .svc()
        .create_trailer(ctx.actor(), tid, b)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "cinema.trailer.create",
        ("trailer", t.id),
        json!({ "title": tid.to_string() }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(t)))
}

/// Replace a trailer (`PUBLISH_MEDIA`).
#[utoipa::path(
    put, path = "/admin/cinema/trailers/{trailer_id}", operation_id = "admin_cinema_update_trailer", request_body = TrailerInput,
    params(("trailer_id" = String, Path, description = "Trailer snowflake")),
    responses((status = 200, body = Trailer), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn update_trailer(
    ctx: PublisherCtx,
    Path(trailer_id): Path<String>,
    Json(b): Json<TrailerInput>,
) -> ApiResult<Json<Trailer>> {
    let tid = id(&trailer_id, "trailer")?;
    let t = ctx
        .svc()
        .update_trailer(ctx.actor(), tid, b)
        .await
        .map_err(map_err)?;
    ctx.audit("cinema.trailer.update", ("trailer", tid), json!({}))
        .await;
    Ok(Json(t))
}

/// Delete a trailer (`PUBLISH_MEDIA`).
#[utoipa::path(
    delete, path = "/admin/cinema/trailers/{trailer_id}", operation_id = "admin_cinema_delete_trailer",
    params(("trailer_id" = String, Path, description = "Trailer snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_trailer(
    ctx: PublisherCtx,
    Path(trailer_id): Path<String>,
) -> ApiResult<StatusCode> {
    let tid = id(&trailer_id, "trailer")?;
    ctx.svc()
        .delete_trailer(ctx.actor(), tid)
        .await
        .map_err(map_err)?;
    ctx.audit("cinema.trailer.delete", ("trailer", tid), json!({}))
        .await;
    Ok(StatusCode::NO_CONTENT)
}

/// Every collection, drafts included (`MANAGE_COLLECTIONS`).
#[utoipa::path(
    get, path = "/admin/cinema/collections", operation_id = "admin_cinema_collections",
    responses((status = 200, body = CollectionList), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn collections(ctx: PublisherCtx) -> ApiResult<Json<CollectionList>> {
    ctx.svc()
        .admin_collections(ctx.actor())
        .await
        .map(Json)
        .map_err(map_err)
}

/// Create a collection (`MANAGE_COLLECTIONS`).
#[utoipa::path(
    post, path = "/admin/cinema/collections", operation_id = "admin_cinema_create_collection", request_body = CollectionInput,
    responses((status = 201, body = Collection), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create_collection(
    ctx: PublisherCtx,
    Json(b): Json<CollectionInput>,
) -> ApiResult<(StatusCode, Json<Collection>)> {
    let c = ctx
        .svc()
        .create_collection(ctx.actor(), b)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "cinema.collection.create",
        ("collection", c.id),
        json!({ "slug": c.slug }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(c)))
}

/// A collection with its titles in order, drafts included (`MANAGE_COLLECTIONS`).
#[utoipa::path(
    get, path = "/admin/cinema/collections/{collection_id}", operation_id = "admin_cinema_collection",
    params(("collection_id" = String, Path, description = "Collection snowflake")),
    responses((status = 200, body = CollectionDetail), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn collection(
    ctx: PublisherCtx,
    Path(collection_id): Path<String>,
) -> ApiResult<Json<CollectionDetail>> {
    ctx.svc()
        .admin_collection(ctx.actor(), id(&collection_id, "collection")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Replace the settings of a collection (`MANAGE_COLLECTIONS`).
#[utoipa::path(
    put, path = "/admin/cinema/collections/{collection_id}", operation_id = "admin_cinema_update_collection", request_body = CollectionInput,
    params(("collection_id" = String, Path, description = "Collection snowflake")),
    responses((status = 200, body = Collection), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn update_collection(
    ctx: PublisherCtx,
    Path(collection_id): Path<String>,
    Json(b): Json<CollectionInput>,
) -> ApiResult<Json<Collection>> {
    let cid = id(&collection_id, "collection")?;
    let c = ctx
        .svc()
        .update_collection(ctx.actor(), cid, b)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "cinema.collection.update",
        ("collection", cid),
        json!({ "slug": c.slug, "published": c.published }),
    )
    .await;
    Ok(Json(c))
}

/// Delete a collection (`MANAGE_COLLECTIONS`).
#[utoipa::path(
    delete, path = "/admin/cinema/collections/{collection_id}", operation_id = "admin_cinema_delete_collection",
    params(("collection_id" = String, Path, description = "Collection snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_collection(
    ctx: PublisherCtx,
    Path(collection_id): Path<String>,
) -> ApiResult<StatusCode> {
    let cid = id(&collection_id, "collection")?;
    ctx.svc()
        .delete_collection(ctx.actor(), cid)
        .await
        .map_err(map_err)?;
    ctx.audit("cinema.collection.delete", ("collection", cid), json!({}))
        .await;
    Ok(StatusCode::NO_CONTENT)
}

/// Replace the titles of a collection, in display order (`MANAGE_COLLECTIONS`).
#[utoipa::path(
    put, path = "/admin/cinema/collections/{collection_id}/items", operation_id = "admin_cinema_set_collection_items", request_body = CollectionItemsBody,
    params(("collection_id" = String, Path, description = "Collection snowflake")),
    responses((status = 200, body = CollectionDetail), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn set_collection_items(
    ctx: PublisherCtx,
    Path(collection_id): Path<String>,
    Json(b): Json<CollectionItemsBody>,
) -> ApiResult<Json<CollectionDetail>> {
    let cid = id(&collection_id, "collection")?;
    let d = ctx
        .svc()
        .set_collection_items(ctx.actor(), cid, b.title_ids)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "cinema.collection.items",
        ("collection", cid),
        json!({ "count": d.items.len() }),
    )
    .await;
    Ok(Json(d))
}

/// Reserve an artwork object for a title and get a presigned `PUT` URL (`PUBLISH_MEDIA`).
/// Upload the image with the same `Content-Type`, then save the returned `key` as the
/// title's `poster_key` / `backdrop_key` / `logo_key`, an episode's `still_key` or a
/// season's `poster_key`.
#[utoipa::path(
    post, path = "/admin/cinema/titles/{title_id}/artwork", operation_id = "admin_cinema_artwork_upload", request_body = ArtworkRequest,
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 200, body = ArtworkUpload), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn artwork_upload(
    ctx: PublisherCtx,
    Path(title_id): Path<String>,
    Json(b): Json<ArtworkRequest>,
) -> ApiResult<Json<ArtworkUpload>> {
    let tid = id(&title_id, "title")?;
    let slot = ctx
        .svc()
        .artwork_upload(ctx.actor(), tid, b)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "cinema.artwork.reserve",
        ("title", tid),
        json!({ "key": slot.key }),
    )
    .await;
    Ok(Json(slot))
}
