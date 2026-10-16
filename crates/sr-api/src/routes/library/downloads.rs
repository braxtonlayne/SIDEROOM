//! Patron chapter downloads (LIB-RDR-016, D-075): `POST /library/chapters/{id}/download` hands out a
//! signed link to the chapter as CBZ (image chapters) or as the kept PDF/EPUB, counted against the
//! Patron's daily allowance. Everybody else gets 403, server side.

use super::social::social_err;
use super::{MemberCtx, id, map_err};
use crate::{
    error::{ApiResult, HttpError},
    openapi::ErrorBody,
    routes::{
        MaybeExt,
        cinema_media::{MediaDeps, map_err as media_err},
    },
};
use axum::{Json, extract::Path};
use serde::Serialize;
use sr_core::{ApiError, ErrorCode};
use sr_library::exports::ChapterExport;
use utoipa::ToSchema;

fn not_patron() -> HttpError {
    ApiError::with_message(
        ErrorCode::MissingPermissions,
        "Downloads are for Patrons.".to_owned(),
    )
    .into()
}

/// What the member may download.
#[derive(Debug, Serialize, ToSchema)]
#[schema(as = LibraryDownloadAllowance)]
pub struct Allowance {
    /// Holds the Patron download perk.
    pub entitled: bool,
    pub per_day: u32,
    /// Downloads left in the rolling 24 hours.
    pub left: u32,
}

async fn per_day(media: &Option<std::sync::Arc<MediaDeps>>, user: i64) -> ApiResult<Option<u32>> {
    match media {
        Some(m) => m.downloads.per_day(user).await.map_err(media_err),
        None => Ok(None),
    }
}

/// Whether the caller may download chapters, and how many downloads are left today.
#[utoipa::path(
    get, path = "/library/me/downloads", operation_id = "library_download_allowance",
    responses((status = 200, body = Allowance), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn allowance(
    ctx: MemberCtx,
    MaybeExt(media): MaybeExt<MediaDeps>,
) -> ApiResult<Json<Allowance>> {
    let Some(per_day) = per_day(&media, ctx.viewer.user_id).await? else {
        return Ok(Json(Allowance {
            entitled: false,
            per_day: 0,
            left: 0,
        }));
    };
    let left = ctx
        .social()?
        .downloads_left(ctx.viewer.user_id, per_day)
        .await
        .map_err(social_err)?;
    Ok(Json(Allowance {
        entitled: true,
        per_day,
        left,
    }))
}

/// Download a chapter: a signed link to the CBZ, PDF or EPUB. Counts against the daily limit.
#[utoipa::path(
    post, path = "/library/chapters/{chapter_id}/download", operation_id = "library_download_chapter",
    params(("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 200, body = ChapterExport), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody, description = "Not a Patron, or the series needs age confirmation (900011)."),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody, description = "Too large to export, or no file."),
              (status = 429, body = ErrorBody, description = "The daily limit is used up.")),
    security(("session" = []))
)]
pub async fn download(
    ctx: MemberCtx,
    MaybeExt(media): MaybeExt<MediaDeps>,
    Path(chapter_id): Path<String>,
) -> ApiResult<Json<ChapterExport>> {
    let user = ctx.viewer.user_id;
    let per_day = per_day(&media, user).await?.ok_or_else(not_patron)?;
    let chapter = id(&chapter_id, "chapter")?;
    // Resolve (and age-check) the file first so a failed export does not cost a download.
    let file = ctx
        .deps
        .service
        .export_chapter(ctx.viewer, chapter)
        .await
        .map_err(map_err)?;
    ctx.social()?
        .take_download_slot(user, chapter, &file.format, per_day)
        .await
        .map_err(|e| match e {
            sr_library_social::SocialError::Conflict(m) => {
                ApiError::with_message(ErrorCode::TooManyRequests, m).into()
            }
            other => social_err(other),
        })?;
    Ok(Json(file))
}
