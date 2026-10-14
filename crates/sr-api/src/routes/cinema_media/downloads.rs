//! Patron downloads (CIN-DL-001): the picker, starting a download, polling it and fetching the
//! file. The shapes match `apps/web/src/cinema/player/api.ts`. Non-Patrons get 403 on all of them;
//! opening an age-restricted title needs the same age confirmation as playback.

use super::{MediaDeps, id, map_err};
use crate::{
    error::{ApiResult, HttpError},
    openapi::ErrorBody,
    routes::{Ext, cinema::MemberCtx},
};
use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
    response::Redirect,
};
use serde::Deserialize;
use sr_cinema::PlayRef;
use sr_media::downloads::{DownloadJob, DownloadOptions};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct EpisodeParam {
    /// For a series: the episode to download.
    pub episode: Option<String>,
}

/// Resolve the asset the member would play (published, age gate), after the Patron check.
async fn asset_of(
    ctx: &MemberCtx,
    media: &MediaDeps,
    title: &str,
    episode: Option<&str>,
) -> Result<(i64, i64, Option<i64>), HttpError> {
    media
        .downloads
        .check_entitled(ctx.viewer.user_id)
        .await
        .map_err(map_err)?;
    let title_id = id(title, "title")?;
    let episode_id = episode.map(|e| id(e, "episode")).transpose()?;
    let what = match episode_id {
        Some(e) => PlayRef::Episode(e),
        None => PlayRef::Title {
            id: title_id,
            variant: None,
        },
    };
    let target = ctx
        .deps
        .service
        .resolve_play(ctx.viewer, what)
        .await
        .map_err(crate::routes::cinema::map_err)?;
    Ok((target.asset_id, title_id, episode_id))
}

/// The renditions a Patron can download for a title or episode, with sizes, which are already
/// prepared, and how many downloads are left today.
#[utoipa::path(
    get, path = "/cinema/titles/{title_id}/downloads", operation_id = "cinema_download_options",
    params(("title_id" = String, Path, description = "Title snowflake"), EpisodeParam),
    responses((status = 200, body = DownloadOptions), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody, description = "Not a Patron, or the title needs age confirmation (code 900011)."),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn options(
    ctx: MemberCtx,
    Ext(media): Ext<MediaDeps>,
    Path(title_id): Path<String>,
    Query(p): Query<EpisodeParam>,
) -> ApiResult<Json<DownloadOptions>> {
    let (asset, _, _) = asset_of(&ctx, &media, &title_id, p.episode.as_deref()).await?;
    media
        .downloads
        .options(ctx.viewer.user_id, asset)
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct StartBody {
    /// A `rendition.id` from the picker (`1080p`, ..., `original`).
    pub rendition_id: String,
    #[serde(default)]
    pub episode_id: Option<String>,
}

/// Start a download (or reuse the prepared file): the MP4 of the chosen rendition is made by a
/// stream-copy job and cached for 7 days. Counts against the daily limit (429 when used up).
#[utoipa::path(
    post, path = "/cinema/titles/{title_id}/downloads", operation_id = "cinema_start_download", request_body = StartBody,
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 202, body = DownloadJob), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody, description = "Not a Patron, the original is not enabled, or age confirmation is needed."),
              (status = 404, body = ErrorBody), (status = 429, body = ErrorBody, description = "The daily limit is used up.")),
    security(("session" = []))
)]
pub async fn start(
    ctx: MemberCtx,
    Ext(media): Ext<MediaDeps>,
    Path(title_id): Path<String>,
    Json(b): Json<StartBody>,
) -> ApiResult<(StatusCode, Json<DownloadJob>)> {
    let (asset, title, episode) =
        asset_of(&ctx, &media, &title_id, b.episode_id.as_deref()).await?;
    let job = media
        .downloads
        .start(ctx.viewer.user_id, title, episode, asset, &b.rendition_id)
        .await
        .map_err(map_err)?;
    Ok((StatusCode::ACCEPTED, Json(job)))
}

/// State of a download you started: `queued`, `preparing`, `ready` (with the link) or `failed`.
#[utoipa::path(
    get, path = "/cinema/downloads/{job_id}", operation_id = "cinema_download",
    params(("job_id" = String, Path, description = "Download id")),
    responses((status = 200, body = DownloadJob), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn job(
    ctx: MemberCtx,
    Ext(media): Ext<MediaDeps>,
    Path(job_id): Path<String>,
) -> ApiResult<Json<DownloadJob>> {
    media
        .downloads
        .job(ctx.viewer.user_id, id(&job_id, "download")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// The file of a ready download: redirects to a signed CDN URL that works for a few minutes.
/// Only the member who started the download can use this link.
#[utoipa::path(
    get, path = "/cinema/downloads/{job_id}/file", operation_id = "cinema_download_file",
    params(("job_id" = String, Path, description = "Download id")),
    responses((status = 307, description = "Redirect to the signed file URL."), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody, description = "Not ready yet, or expired.")),
    security(("session" = []))
)]
pub async fn file(
    ctx: MemberCtx,
    Ext(media): Ext<MediaDeps>,
    Path(job_id): Path<String>,
) -> Result<Redirect, HttpError> {
    let url = media
        .downloads
        .file_url(ctx.viewer.user_id, id(&job_id, "download")?)
        .await
        .map_err(map_err)?;
    let sep = if url.contains('?') { '&' } else { '?' };
    Ok(Redirect::temporary(&format!("{url}{sep}dl=1")))
}
