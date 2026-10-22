//! Publisher endpoints (`/admin/library/*`): series, editions, volumes, chapters and
//! pages. Permission checks live in `sr_library::LibraryService`; every successful
//! change is also written to the staff access log.

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
use sr_library::model::*;
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AdminListParams {
    pub kind: Option<SeriesKind>,
    pub status: Option<SeriesStatus>,
    /// Comma-separated genres the series must have.
    pub genres: Option<String>,
    /// Comma-separated tags the series must have.
    pub tags: Option<String>,
    /// `true` for published, `false` for drafts and unpublished series.
    pub published: Option<bool>,
    /// Title substring.
    pub title: Option<String>,
    /// `include` (default), `hide` or `only`.
    pub age: Option<AgeFilter>,
    pub sort: Option<SortKey>,
    pub order: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// All series, drafts included (`PUBLISH_READING`).
#[utoipa::path(
    get, path = "/admin/library/series", operation_id = "admin_library_list", params(AdminListParams),
    responses((status = 200, body = SeriesPage), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list(
    ctx: PublisherCtx,
    Query(p): Query<AdminListParams>,
) -> ApiResult<Json<SeriesPage>> {
    let q = BrowseParams {
        kind: p.kind,
        status: p.status,
        genres: p.genres,
        tags: p.tags,
        age: p.age,
        sort: p.sort,
        order: p.order.clone(),
        limit: p.limit,
        offset: p.offset,
        ..BrowseParams::default()
    };
    order(&p.order)?;
    let mut q = q.into_query(AgeFilter::Include)?;
    q.published = p.published;
    q.title_contains = p.title;
    ctx.svc()
        .admin_list(ctx.actor(), q)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Create a draft series (`PUBLISH_READING`).
#[utoipa::path(
    post, path = "/admin/library/series", operation_id = "admin_library_create_series", request_body = SeriesInput,
    responses((status = 201, body = Series), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create_series(
    ctx: PublisherCtx,
    Json(b): Json<SeriesInput>,
) -> ApiResult<(StatusCode, Json<Series>)> {
    let s = ctx
        .svc()
        .create_series(ctx.actor(), b)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "library.series.create",
        ("series", s.id),
        json!({ "title": s.title }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(s)))
}

/// A series with its editions, published or not (`PUBLISH_READING`).
#[utoipa::path(
    get, path = "/admin/library/series/{series_id}", operation_id = "admin_library_series",
    params(("series_id" = String, Path, description = "Series snowflake")),
    responses((status = 200, body = SeriesDetail), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn series(
    ctx: PublisherCtx,
    Path(series_id): Path<String>,
) -> ApiResult<Json<SeriesDetail>> {
    ctx.svc()
        .admin_series(ctx.actor(), id(&series_id, "series")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Replace the editable fields of a series (`PUBLISH_READING`).
#[utoipa::path(
    put, path = "/admin/library/series/{series_id}", operation_id = "admin_library_update_series", request_body = SeriesInput,
    params(("series_id" = String, Path, description = "Series snowflake")),
    responses((status = 200, body = Series), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn update_series(
    ctx: PublisherCtx,
    Path(series_id): Path<String>,
    Json(b): Json<SeriesInput>,
) -> ApiResult<Json<Series>> {
    let sid = id(&series_id, "series")?;
    let s = ctx
        .svc()
        .update_series(ctx.actor(), sid, b)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "library.series.update",
        ("series", sid),
        json!({ "title": s.title }),
    )
    .await;
    Ok(Json(s))
}

/// Delete a series with everything under it (`DELETE_CONTENT`).
#[utoipa::path(
    delete, path = "/admin/library/series/{series_id}", operation_id = "admin_library_delete_series",
    params(("series_id" = String, Path, description = "Series snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_series(
    ctx: PublisherCtx,
    Path(series_id): Path<String>,
) -> ApiResult<StatusCode> {
    let sid = id(&series_id, "series")?;
    ctx.svc()
        .delete_series(ctx.actor(), sid)
        .await
        .map_err(map_err)?;
    ctx.audit("library.series.delete", ("series", sid), json!({}))
        .await;
    Ok(StatusCode::NO_CONTENT)
}

/// Publish a series (`PUBLISH_READING`).
#[utoipa::path(
    post, path = "/admin/library/series/{series_id}/publish", operation_id = "admin_library_publish_series",
    params(("series_id" = String, Path, description = "Series snowflake")),
    responses((status = 200, body = Series), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn publish_series(
    ctx: PublisherCtx,
    Path(series_id): Path<String>,
) -> ApiResult<Json<Series>> {
    set_series_published(ctx, series_id, true).await
}

/// Unpublish a series (`PUBLISH_READING` or `DELETE_CONTENT`).
#[utoipa::path(
    post, path = "/admin/library/series/{series_id}/unpublish", operation_id = "admin_library_unpublish_series",
    params(("series_id" = String, Path, description = "Series snowflake")),
    responses((status = 200, body = Series), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn unpublish_series(
    ctx: PublisherCtx,
    Path(series_id): Path<String>,
) -> ApiResult<Json<Series>> {
    set_series_published(ctx, series_id, false).await
}

async fn set_series_published(
    ctx: PublisherCtx,
    series_id: String,
    published: bool,
) -> ApiResult<Json<Series>> {
    let sid = id(&series_id, "series")?;
    let s = ctx
        .svc()
        .set_series_published(ctx.actor(), sid, published)
        .await
        .map_err(map_err)?;
    let action = if published {
        "library.series.publish"
    } else {
        "library.series.unpublish"
    };
    ctx.audit(action, ("series", sid), json!({})).await;
    Ok(Json(s))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MergeBody {
    /// The series that stays; this one is merged into it.
    pub target_id: String,
}

/// Merge this series into another (`PUBLISH_READING` and `DELETE_CONTENT`): editions,
/// chapters, follows and progress move; this series stays as a redirect.
#[utoipa::path(
    post, path = "/admin/library/series/{series_id}/merge", operation_id = "admin_library_merge_series", request_body = MergeBody,
    params(("series_id" = String, Path, description = "Series snowflake to merge away")),
    responses((status = 200, body = Series), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn merge_series(
    ctx: PublisherCtx,
    Path(series_id): Path<String>,
    Json(b): Json<MergeBody>,
) -> ApiResult<Json<Series>> {
    let source = id(&series_id, "series")?;
    let target = id(&b.target_id, "series")?;
    let s = ctx
        .svc()
        .merge_series(ctx.actor(), source, target)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "library.series.merge",
        ("series", source),
        json!({ "target": target.to_string() }),
    )
    .await;
    Ok(Json(s))
}

/// Editions, volumes and every chapter of a series, published or not (`PUBLISH_READING`).
#[utoipa::path(
    get, path = "/admin/library/series/{series_id}/chapters", operation_id = "admin_library_chapters",
    params(("series_id" = String, Path, description = "Series snowflake"), super::member::ChapterParams),
    responses((status = 200, body = ChapterList), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn chapters(
    ctx: PublisherCtx,
    Path(series_id): Path<String>,
    Query(p): Query<super::member::ChapterParams>,
) -> ApiResult<Json<ChapterList>> {
    ctx.svc()
        .admin_chapters(ctx.actor(), id(&series_id, "series")?, p.into_query()?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Add an edition (`PUBLISH_READING`).
#[utoipa::path(
    post, path = "/admin/library/series/{series_id}/editions", operation_id = "admin_library_create_edition", request_body = EditionInput,
    params(("series_id" = String, Path, description = "Series snowflake")),
    responses((status = 201, body = Edition), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create_edition(
    ctx: PublisherCtx,
    Path(series_id): Path<String>,
    Json(b): Json<EditionInput>,
) -> ApiResult<(StatusCode, Json<Edition>)> {
    let e = ctx
        .svc()
        .create_edition(ctx.actor(), id(&series_id, "series")?, b)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "library.edition.create",
        ("edition", e.id),
        json!({ "series": e.series_id.to_string() }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(e)))
}

/// Change an edition (`PUBLISH_READING`).
#[utoipa::path(
    put, path = "/admin/library/editions/{edition_id}", operation_id = "admin_library_update_edition", request_body = EditionInput,
    params(("edition_id" = String, Path, description = "Edition snowflake")),
    responses((status = 200, body = Edition), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn update_edition(
    ctx: PublisherCtx,
    Path(edition_id): Path<String>,
    Json(b): Json<EditionInput>,
) -> ApiResult<Json<Edition>> {
    let eid = id(&edition_id, "edition")?;
    let e = ctx
        .svc()
        .update_edition(ctx.actor(), eid, b)
        .await
        .map_err(map_err)?;
    ctx.audit("library.edition.update", ("edition", eid), json!({}))
        .await;
    Ok(Json(e))
}

/// Delete an edition and its chapters (`DELETE_CONTENT`).
#[utoipa::path(
    delete, path = "/admin/library/editions/{edition_id}", operation_id = "admin_library_delete_edition",
    params(("edition_id" = String, Path, description = "Edition snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_edition(
    ctx: PublisherCtx,
    Path(edition_id): Path<String>,
) -> ApiResult<StatusCode> {
    let eid = id(&edition_id, "edition")?;
    ctx.svc()
        .delete_edition(ctx.actor(), eid)
        .await
        .map_err(map_err)?;
    ctx.audit("library.edition.delete", ("edition", eid), json!({}))
        .await;
    Ok(StatusCode::NO_CONTENT)
}

/// Add a volume to an edition (`PUBLISH_READING`).
#[utoipa::path(
    post, path = "/admin/library/editions/{edition_id}/volumes", operation_id = "admin_library_create_volume", request_body = VolumeInput,
    params(("edition_id" = String, Path, description = "Edition snowflake")),
    responses((status = 201, body = Volume), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create_volume(
    ctx: PublisherCtx,
    Path(edition_id): Path<String>,
    Json(b): Json<VolumeInput>,
) -> ApiResult<(StatusCode, Json<Volume>)> {
    let v = ctx
        .svc()
        .create_volume(ctx.actor(), id(&edition_id, "edition")?, b)
        .await
        .map_err(map_err)?;
    ctx.audit("library.volume.create", ("volume", v.id), json!({}))
        .await;
    Ok((StatusCode::CREATED, Json(v)))
}

/// Delete a volume; its chapters stay, ungrouped (`DELETE_CONTENT`).
#[utoipa::path(
    delete, path = "/admin/library/volumes/{volume_id}", operation_id = "admin_library_delete_volume",
    params(("volume_id" = String, Path, description = "Volume snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_volume(
    ctx: PublisherCtx,
    Path(volume_id): Path<String>,
) -> ApiResult<StatusCode> {
    let vid = id(&volume_id, "volume")?;
    ctx.svc()
        .delete_volume(ctx.actor(), vid)
        .await
        .map_err(map_err)?;
    ctx.audit("library.volume.delete", ("volume", vid), json!({}))
        .await;
    Ok(StatusCode::NO_CONTENT)
}

/// Add a draft chapter to an edition (`PUBLISH_READING`).
#[utoipa::path(
    post, path = "/admin/library/editions/{edition_id}/chapters", operation_id = "admin_library_create_chapter", request_body = ChapterInput,
    params(("edition_id" = String, Path, description = "Edition snowflake")),
    responses((status = 201, body = Chapter), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create_chapter(
    ctx: PublisherCtx,
    Path(edition_id): Path<String>,
    Json(b): Json<ChapterInput>,
) -> ApiResult<(StatusCode, Json<Chapter>)> {
    let c = ctx
        .svc()
        .create_chapter(ctx.actor(), id(&edition_id, "edition")?, b)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "library.chapter.create",
        ("chapter", c.id),
        json!({ "series": c.series_id.to_string() }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(c)))
}

/// Change a chapter's number, title, volume or release group (`PUBLISH_READING`).
#[utoipa::path(
    put, path = "/admin/library/chapters/{chapter_id}", operation_id = "admin_library_update_chapter", request_body = ChapterInput,
    params(("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 200, body = Chapter), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn update_chapter(
    ctx: PublisherCtx,
    Path(chapter_id): Path<String>,
    Json(b): Json<ChapterInput>,
) -> ApiResult<Json<Chapter>> {
    let cid = id(&chapter_id, "chapter")?;
    let c = ctx
        .svc()
        .update_chapter(ctx.actor(), cid, b)
        .await
        .map_err(map_err)?;
    ctx.audit("library.chapter.update", ("chapter", cid), json!({}))
        .await;
    Ok(Json(c))
}

/// Delete a chapter and its pages (`DELETE_CONTENT`).
#[utoipa::path(
    delete, path = "/admin/library/chapters/{chapter_id}", operation_id = "admin_library_delete_chapter",
    params(("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_chapter(
    ctx: PublisherCtx,
    Path(chapter_id): Path<String>,
) -> ApiResult<StatusCode> {
    let cid = id(&chapter_id, "chapter")?;
    ctx.svc()
        .delete_chapter(ctx.actor(), cid)
        .await
        .map_err(map_err)?;
    ctx.audit("library.chapter.delete", ("chapter", cid), json!({}))
        .await;
    Ok(StatusCode::NO_CONTENT)
}

/// Publish a chapter; it needs pages (`PUBLISH_READING`).
#[utoipa::path(
    post, path = "/admin/library/chapters/{chapter_id}/publish", operation_id = "admin_library_publish_chapter",
    params(("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 200, body = Chapter), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn publish_chapter(
    ctx: PublisherCtx,
    Path(chapter_id): Path<String>,
) -> ApiResult<Json<Chapter>> {
    set_chapter_published(ctx, chapter_id, true).await
}

/// Unpublish a chapter (`PUBLISH_READING` or `DELETE_CONTENT`).
#[utoipa::path(
    post, path = "/admin/library/chapters/{chapter_id}/unpublish", operation_id = "admin_library_unpublish_chapter",
    params(("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 200, body = Chapter), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn unpublish_chapter(
    ctx: PublisherCtx,
    Path(chapter_id): Path<String>,
) -> ApiResult<Json<Chapter>> {
    set_chapter_published(ctx, chapter_id, false).await
}

async fn set_chapter_published(
    ctx: PublisherCtx,
    chapter_id: String,
    published: bool,
) -> ApiResult<Json<Chapter>> {
    let cid = id(&chapter_id, "chapter")?;
    let c = ctx
        .svc()
        .set_chapter_published(ctx.actor(), cid, published)
        .await
        .map_err(map_err)?;
    let action = if published {
        "library.chapter.publish"
    } else {
        "library.chapter.unpublish"
    };
    ctx.audit(action, ("chapter", cid), json!({})).await;
    Ok(Json(c))
}

/// Preview a chapter's page manifest, published or not (`PUBLISH_READING`).
#[utoipa::path(
    get, path = "/admin/library/chapters/{chapter_id}/pages", operation_id = "admin_library_pages",
    params(("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 200, body = PageManifest), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn pages(
    ctx: PublisherCtx,
    Path(chapter_id): Path<String>,
) -> ApiResult<Json<PageManifest>> {
    ctx.svc()
        .admin_page_manifest(ctx.actor(), id(&chapter_id, "chapter")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Body of `PUT /admin/library/chapters/{id}/pages/{index}`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ReplacePageBody {
    /// `data:image/<type>;base64,...`: one PNG, JPEG, WebP, GIF or AVIF image of at most 16 MiB.
    pub image: String,
}

/// Largest request body of the single-page route: 16 MiB as base64 plus the JSON around it.
pub const REPLACE_PAGE_BODY_LIMIT: usize = 22 * 1024 * 1024;

/// Replace one page of an image chapter with a new image (`PUBLISH_READING`). The image is converted like
/// an archive page (limits, dimensions, crop data, sized variants); the page keeps its position and
/// double-spread flag, and the page list changes key, so the offline cache and the CBZ export follow.
#[utoipa::path(
    put, path = "/admin/library/chapters/{chapter_id}/pages/{index}", operation_id = "admin_library_replace_page", request_body = ReplacePageBody,
    params(("chapter_id" = String, Path, description = "Chapter snowflake"),
           ("index" = i32, Path, description = "Page index from 0")),
    responses((status = 200, body = Chapter), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn replace_page(
    ctx: PublisherCtx,
    Path((chapter_id, index)): Path<(String, i32)>,
    Json(b): Json<ReplacePageBody>,
) -> ApiResult<Json<Chapter>> {
    let cid = id(&chapter_id, "chapter")?;
    let bytes = crate::routes::settings::expressions::decode_data_uri("image", &b.image)?;
    let size = bytes.len();
    let c = ctx
        .svc()
        .replace_page(ctx.actor(), cid, index, bytes.to_vec())
        .await
        .map_err(map_err)?;
    ctx.audit(
        "library.page.replace",
        ("chapter", cid),
        json!({ "index": index, "bytes": size }),
    )
    .await;
    Ok(Json(c))
}

/// Replace the whole page list of an image chapter: this is how pages are reordered,
/// replaced and removed (`PUBLISH_READING`). Object keys must live under
/// `library/{series_id}/{chapter_id}/`.
#[utoipa::path(
    put, path = "/admin/library/chapters/{chapter_id}/pages", operation_id = "admin_library_replace_pages", request_body = Vec<PageInput>,
    params(("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 200, body = Chapter), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn replace_pages(
    ctx: PublisherCtx,
    Path(chapter_id): Path<String>,
    Json(b): Json<Vec<PageInput>>,
) -> ApiResult<Json<Chapter>> {
    let cid = id(&chapter_id, "chapter")?;
    let n = b.len();
    let c = ctx
        .svc()
        .replace_pages(ctx.actor(), cid, b)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "library.chapter.pages",
        ("chapter", cid),
        json!({ "pages": n }),
    )
    .await;
    Ok(Json(c))
}
