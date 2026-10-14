//! Staff upload path for cinema originals (`/admin/cinema/assets/*`, `PUBLISH_MEDIA`).
//!
//! Reserve a slot (`POST`), `PUT` each part to its presigned URL, complete, then poll the asset:
//! `uploaded`, `probing`, `transcoding`, `published` (or `failed` with the reason). Begin and
//! complete are written to the staff access log.

use super::{MediaCtx, id, map_err};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sr_cinema::Actor;
use sr_core::PlatformPermissions as P;
use sr_media::assets::{Asset, AssetQuery, AssetState, MediaError};
use sr_media::service::{AssetStatus, CompleteUploadRequest, UploadRequest, UploadSlot};
use utoipa::{IntoParams, ToSchema};

const NEED: (P, &str) = (P::PUBLISH_MEDIA, "PUBLISH_MEDIA");

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AssetListParams {
    pub state: Option<AssetState>,
    /// Only assets uploaded for this title.
    pub title_id: Option<String>,
    /// 1 to 200, default 50.
    pub limit: Option<u32>,
    /// `next_before` of the previous page.
    pub before: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AssetList {
    pub items: Vec<Asset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_before: Option<String>,
}

/// Uploaded videos, newest first (`PUBLISH_MEDIA`).
#[utoipa::path(
    get, path = "/admin/cinema/assets", operation_id = "admin_cinema_assets", params(AssetListParams),
    responses((status = 200, body = AssetList), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list(ctx: MediaCtx, Query(p): Query<AssetListParams>) -> ApiResult<Json<AssetList>> {
    ctx.require(NEED.0, NEED.1)?;
    let limit = p.limit.unwrap_or(50).clamp(1, 200);
    let items = ctx
        .media()
        .list_assets(AssetQuery {
            state: p.state,
            title_id: p.title_id.as_deref().map(|t| id(t, "title")).transpose()?,
            uploaded_by: None,
            limit,
            before: p.before.as_deref().map(|b| id(b, "cursor")).transpose()?,
        })
        .await
        .map_err(map_err)?;
    let next_before = (items.len() as u32 == limit)
        .then(|| items.last().map(|a| a.id.to_string()))
        .flatten();
    Ok(Json(AssetList { items, next_before }))
}

/// Reserve an upload slot for an original and get the presigned multipart `PUT` URLs
/// (`PUBLISH_MEDIA`).
///
/// Send each part of the file (`part_size` bytes, the last one shorter) to its URL with `PUT`
/// and keep the `ETag` response header of every part, then call the `complete` endpoint with the
/// parts. A slot that is never completed expires after a day. Files over the configured size
/// limit (`cinema.upload.max_gb`) and unknown containers are refused with 400.
#[utoipa::path(
    post, path = "/admin/cinema/assets", operation_id = "admin_cinema_begin_upload", request_body = UploadRequest,
    responses((status = 201, body = UploadSlot), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 503, body = ErrorBody, description = "Object storage did not start the upload.")),
    security(("session" = []))
)]
pub async fn begin_upload(
    ctx: MediaCtx,
    Json(b): Json<UploadRequest>,
) -> ApiResult<(StatusCode, Json<UploadSlot>)> {
    ctx.require(NEED.0, NEED.1)?;
    let slot = ctx
        .media()
        .begin_upload(ctx.user(), b)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "cinema.asset.upload.begin",
        ("asset", slot.asset.id),
        json!({
            "filename": slot.asset.filename,
            "size_bytes": slot.asset.original_size,
            "title": slot.asset.title_id.map(|t| t.to_string()),
        }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(slot)))
}

/// The file is uploaded: finish the multipart upload, check the object and queue the probe
/// (`PUBLISH_MEDIA`). Repeating the call for an asset past the upload returns it unchanged.
#[utoipa::path(
    post, path = "/admin/cinema/assets/{asset_id}/complete", operation_id = "admin_cinema_complete_upload",
    request_body = CompleteUploadRequest,
    params(("asset_id" = String, Path, description = "Asset snowflake")),
    responses((status = 202, body = Asset), (status = 400, body = ErrorBody, description = "The storage did not accept the parts, or the file is empty, over the limit or not the announced size."),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn complete_upload(
    ctx: MediaCtx,
    Path(asset_id): Path<String>,
    Json(b): Json<CompleteUploadRequest>,
) -> ApiResult<(StatusCode, Json<Asset>)> {
    ctx.require(NEED.0, NEED.1)?;
    let asset = ctx
        .media()
        .complete_upload(id(&asset_id, "asset")?, b)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "cinema.asset.upload.complete",
        ("asset", asset.id),
        json!({ "state": asset.state, "size_bytes": asset.original_size }),
    )
    .await;
    Ok((StatusCode::ACCEPTED, Json(asset)))
}

/// An asset with its jobs and a summary of its probe: poll this after completing the upload
/// (`PUBLISH_MEDIA`).
#[utoipa::path(
    get, path = "/admin/cinema/assets/{asset_id}", operation_id = "admin_cinema_asset",
    params(("asset_id" = String, Path, description = "Asset snowflake")),
    responses((status = 200, body = AssetStatus), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn status(ctx: MediaCtx, Path(asset_id): Path<String>) -> ApiResult<Json<AssetStatus>> {
    ctx.require(NEED.0, NEED.1)?;
    ctx.media()
        .status(id(&asset_id, "asset")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Probe the original again, for example after FFmpeg was installed on the server
/// (`PUBLISH_MEDIA`).
#[utoipa::path(
    post, path = "/admin/cinema/assets/{asset_id}/probe", operation_id = "admin_cinema_reprobe",
    params(("asset_id" = String, Path, description = "Asset snowflake")),
    responses((status = 202, body = Asset), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn reprobe(
    ctx: MediaCtx,
    Path(asset_id): Path<String>,
) -> ApiResult<(StatusCode, Json<Asset>)> {
    ctx.require(NEED.0, NEED.1)?;
    let a = ctx
        .media()
        .reprobe(id(&asset_id, "asset")?)
        .await
        .map_err(map_err)?;
    ctx.audit("cinema.asset.reprobe", ("asset", a.id), json!({}))
        .await;
    Ok((StatusCode::ACCEPTED, Json(a)))
}

/// Queue the ladder transcode of an asset that waited for approval (`PUBLISH_MEDIA`).
#[utoipa::path(
    post, path = "/admin/cinema/assets/{asset_id}/transcode", operation_id = "admin_cinema_transcode",
    params(("asset_id" = String, Path, description = "Asset snowflake")),
    responses((status = 202, body = Asset), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn transcode(
    ctx: MediaCtx,
    Path(asset_id): Path<String>,
) -> ApiResult<(StatusCode, Json<Asset>)> {
    ctx.require(NEED.0, NEED.1)?;
    let a = ctx
        .media()
        .approve(id(&asset_id, "asset")?)
        .await
        .map_err(map_err)?;
    ctx.audit("cinema.asset.transcode", ("asset", a.id), json!({}))
        .await;
    Ok((StatusCode::ACCEPTED, Json(a)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LinkBody {
    /// The title the asset belongs to (a film, or the series for bookkeeping).
    #[serde(default)]
    pub title_id: Option<String>,
    /// The episode the asset is the video of. With an episode, the episode is linked.
    #[serde(default)]
    pub episode_id: Option<String>,
    /// Also make it the playable asset of the title or episode in the catalog (default true).
    /// Needs a finished ladder.
    #[serde(default)]
    pub set_playable: Option<bool>,
}

/// Record which title or episode an asset is for and, once it is playable, link it in the
/// catalog as the video of that title or episode (`PUBLISH_MEDIA`).
#[utoipa::path(
    put, path = "/admin/cinema/assets/{asset_id}/link", operation_id = "admin_cinema_link_asset", request_body = LinkBody,
    params(("asset_id" = String, Path, description = "Asset snowflake")),
    responses((status = 200, body = Asset), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody, description = "The asset has no playable renditions yet.")),
    security(("session" = []))
)]
pub async fn link(
    ctx: MediaCtx,
    Path(asset_id): Path<String>,
    Json(b): Json<LinkBody>,
) -> ApiResult<Json<Asset>> {
    ctx.require(NEED.0, NEED.1)?;
    let asset_id = id(&asset_id, "asset")?;
    let title = b.title_id.as_deref().map(|t| id(t, "title")).transpose()?;
    let episode = b
        .episode_id
        .as_deref()
        .map(|e| id(e, "episode"))
        .transpose()?;
    if title.is_none() && episode.is_none() {
        return Err(map_err(MediaError::Invalid(
            "give a title_id or an episode_id".into(),
        )));
    }
    let asset = ctx.media().get(asset_id).await.map_err(map_err)?;
    let playable = b.set_playable.unwrap_or(true);
    if playable && !asset.playable {
        return Err(map_err(MediaError::Conflict(
            "the asset has no playable renditions yet".into(),
        )));
    }
    if playable {
        let actor = Actor {
            user_id: ctx.user(),
            permissions: ctx.staff.caller.permissions,
        };
        match episode {
            Some(e) => {
                ctx.cinema
                    .service
                    .link_episode_asset(actor, e, Some(asset_id))
                    .await
                    .map_err(crate::routes::cinema::map_err)?;
            }
            None => {
                if let Some(t) = title {
                    ctx.cinema
                        .service
                        .link_title_asset(actor, t, Some(asset_id))
                        .await
                        .map_err(crate::routes::cinema::map_err)?;
                }
            }
        }
    }
    let updated = ctx
        .media()
        .link(asset_id, title, episode)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "cinema.asset.link",
        ("asset", asset_id),
        json!({ "title": title.map(|t| t.to_string()), "episode": episode.map(|e| e.to_string()), "playable": playable }),
    )
    .await;
    Ok(Json(updated))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct OriginalDownloadsBody {
    pub enabled: bool,
}

/// Allow (or stop allowing) members with the download perk to fetch the original file
/// (`PUBLISH_MEDIA`). An asset with this on keeps its original on the hot tier.
#[utoipa::path(
    put, path = "/admin/cinema/assets/{asset_id}/original-downloads", operation_id = "admin_cinema_original_downloads",
    request_body = OriginalDownloadsBody,
    params(("asset_id" = String, Path, description = "Asset snowflake")),
    responses((status = 200, body = Asset), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn original_downloads(
    ctx: MediaCtx,
    Path(asset_id): Path<String>,
    Json(b): Json<OriginalDownloadsBody>,
) -> ApiResult<Json<Asset>> {
    ctx.require(NEED.0, NEED.1)?;
    let a = ctx
        .media()
        .set_original_downloads(id(&asset_id, "asset")?, b.enabled)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "cinema.asset.original_downloads",
        ("asset", a.id),
        json!({ "enabled": b.enabled }),
    )
    .await;
    Ok(Json(a))
}
