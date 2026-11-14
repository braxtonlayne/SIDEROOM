//! Staff subtitle management (`/admin/cinema/assets/{asset_id}/subtitles`, `PUBLISH_MEDIA`, D-685): list the
//! tracks of a video, upload a subtitle file, edit a track, delete an uploaded one. Every change is written to
//! the staff access log (`cinema.subtitle.upload`, `cinema.subtitle.update`, `cinema.subtitle.delete`).
//!
//! An upload is untrusted input: the format is read from the content, the file is parsed and written out again
//! as WebVTT, and a file with no readable cue is refused (400). A track belongs to one asset: a track id under
//! another asset's path is 404.

use super::staff::{NEED, refuse_slate};
use super::{MediaCtx, id, map_err};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sr_core::{ApiError, ErrorCode};
use sr_media::assets::{SubtitlePatch, SubtitleTrack};
use sr_media::service::SubtitleUploadMeta;
use sr_media::subtitles::MAX_SUBTITLE_BYTES;
use utoipa::{IntoParams, ToSchema};

/// The request body limit of the upload route: the file limit and a little room.
pub const UPLOAD_BODY_LIMIT: usize = MAX_SUBTITLE_BYTES + 64 * 1024;

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = CinemaSubtitleList)]
pub struct SubtitleList {
    pub tracks: Vec<SubtitleTrack>,
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct UploadParams {
    /// A BCP 47 tag such as `en` or `pt-BR` (optional).
    pub language: Option<String>,
    /// The name in the player menu, 1 to 80 characters; the language, or "Subtitles", when empty.
    pub label: Option<String>,
    pub forced: Option<bool>,
    pub hearing_impaired: Option<bool>,
}

/// Changes to a track; every field is optional.
#[derive(Debug, Default, Deserialize, ToSchema)]
#[schema(as = CinemaSubtitlePatch)]
pub struct PatchBody {
    /// 1 to 80 characters.
    pub label: Option<String>,
    /// A BCP 47 tag such as `en` or `pt-BR`.
    pub language: Option<String>,
    pub forced: Option<bool>,
    pub hearing_impaired: Option<bool>,
    /// `true` makes this the default track and clears the default of the others.
    pub is_default: Option<bool>,
    /// Hide the track from viewers (or show it again).
    pub hidden: Option<bool>,
}

/// A language as the title editor validates it (`pt-br` becomes `pt-BR`).
fn language(raw: &str) -> Result<String, crate::error::HttpError> {
    sr_cinema::rules::language(raw)
        .map_err(|e| ApiError::with_message(ErrorCode::InvalidFormBody, e.to_string()).into())
}

/// The tracks of a video, hidden ones included (`PUBLISH_MEDIA`): the ones extracted from the file first, then
/// the uploaded ones.
#[utoipa::path(
    get, path = "/admin/cinema/assets/{asset_id}/subtitles", operation_id = "admin_cinema_subtitles",
    params(("asset_id" = String, Path, description = "Asset snowflake")),
    responses((status = 200, body = SubtitleList), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list(ctx: MediaCtx, Path(asset_id): Path<String>) -> ApiResult<Json<SubtitleList>> {
    ctx.require(NEED.0, NEED.1)?;
    let asset = id(&asset_id, "asset")?;
    refuse_slate(&ctx, asset).await?;
    let tracks = ctx.media().subtitle_tracks(asset).await.map_err(map_err)?;
    Ok(Json(SubtitleList { tracks }))
}

/// Upload a subtitle file for a video (`PUBLISH_MEDIA`): the body is the file (SubRip, WebVTT or ASS/SSA, at
/// most 20 MiB; the format is read from the content, not the name). It is parsed and stored again as WebVTT (an
/// ASS file is also kept for styled rendering). A file with no readable cue, over the limit or with too many
/// cues is 400 with the reason; a video whose storage does not take uploads is 409. The track is kept when the
/// video is re-encoded.
#[utoipa::path(
    post, path = "/admin/cinema/assets/{asset_id}/subtitles", operation_id = "admin_cinema_upload_subtitle",
    params(("asset_id" = String, Path, description = "Asset snowflake"), UploadParams),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses((status = 201, body = SubtitleTrack), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody),
              (status = 413, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn upload(
    ctx: MediaCtx,
    Path(asset_id): Path<String>,
    Query(q): Query<UploadParams>,
    body: Bytes,
) -> ApiResult<(StatusCode, Json<SubtitleTrack>)> {
    ctx.require(NEED.0, NEED.1)?;
    let asset = id(&asset_id, "asset")?;
    refuse_slate(&ctx, asset).await?;
    let language = q
        .language
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(language)
        .transpose()?;
    let track = ctx
        .media()
        .upload_subtitle(
            asset,
            SubtitleUploadMeta {
                label: q.label,
                language,
                forced: q.forced.unwrap_or(false),
                hearing_impaired: q.hearing_impaired.unwrap_or(false),
            },
            &body,
        )
        .await
        .map_err(map_err)?;
    ctx.audit(
        "cinema.subtitle.upload",
        ("asset", asset),
        json!({ "track": track.id.to_string(), "language": track.language, "label": track.label,
                "bytes": body.len(), "styled": track.styled }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(track)))
}

/// Change a track (`PUBLISH_MEDIA`): label, language, forced, hearing impaired, default (at most one per
/// video: the others are cleared) or hidden. The settings of an embedded track are kept when the video is
/// re-encoded.
#[utoipa::path(
    patch, path = "/admin/cinema/assets/{asset_id}/subtitles/{track_id}", operation_id = "admin_cinema_update_subtitle",
    params(("asset_id" = String, Path, description = "Asset snowflake"),
           ("track_id" = String, Path, description = "Track snowflake")),
    request_body = PatchBody,
    responses((status = 200, body = SubtitleTrack), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn update(
    ctx: MediaCtx,
    Path((asset_id, track_id)): Path<(String, String)>,
    Json(b): Json<PatchBody>,
) -> ApiResult<Json<SubtitleTrack>> {
    ctx.require(NEED.0, NEED.1)?;
    let asset = id(&asset_id, "asset")?;
    let track_id = id(&track_id, "subtitle track")?;
    refuse_slate(&ctx, asset).await?;
    let patch = SubtitlePatch {
        label: b.label,
        language: b
            .language
            .as_deref()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(language)
            .transpose()?,
        forced: b.forced,
        hearing_impaired: b.hearing_impaired,
        is_default: b.is_default,
        hidden: b.hidden,
    };
    let detail = json!({ "track": track_id.to_string(), "label": patch.label, "language": patch.language,
        "forced": patch.forced, "hearing_impaired": patch.hearing_impaired,
        "is_default": patch.is_default, "hidden": patch.hidden });
    let track = ctx
        .media()
        .update_subtitle(asset, track_id, patch)
        .await
        .map_err(map_err)?;
    ctx.audit("cinema.subtitle.update", ("asset", asset), detail)
        .await;
    Ok(Json(track))
}

/// Delete an uploaded track with its files (`PUBLISH_MEDIA`). A track that came from the video file answers
/// 400: hide it instead.
#[utoipa::path(
    delete, path = "/admin/cinema/assets/{asset_id}/subtitles/{track_id}", operation_id = "admin_cinema_delete_subtitle",
    params(("asset_id" = String, Path, description = "Asset snowflake"),
           ("track_id" = String, Path, description = "Track snowflake")),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete(
    ctx: MediaCtx,
    Path((asset_id, track_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    ctx.require(NEED.0, NEED.1)?;
    let asset = id(&asset_id, "asset")?;
    let track = id(&track_id, "subtitle track")?;
    refuse_slate(&ctx, asset).await?;
    ctx.media()
        .delete_subtitle(asset, track)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "cinema.subtitle.delete",
        ("asset", asset),
        json!({ "track": track.to_string() }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}
