//! "Reading" presence (LIB-RDR-017) and the notifications bell count (LIB-SOC-007). The logic lives
//! in `sr_library_social::{presence, notifications}`.

use super::social::social_err;
use super::{MemberCtx, id};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{Json, extract::Path, http::StatusCode};
use sr_library_social::model::*;

/// Reader heartbeat: the caller is reading this chapter. Send it on opening a chapter and about
/// every 30 seconds; the presence lapses after 90 seconds. Nothing is shared when the member turned
/// library sharing off or paused their history (D-190); age-restricted series are masked.
#[utoipa::path(
    put, path = "/library/chapters/{chapter_id}/presence", operation_id = "library_reading_heartbeat",
    params(("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody, description = "Age confirmation required (900011)."),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn heartbeat(ctx: MemberCtx, Path(chapter_id): Path<String>) -> ApiResult<StatusCode> {
    ctx.social()?
        .heartbeat_reading(ctx.sviewer(), id(&chapter_id, "chapter")?)
        .await
        .map_err(social_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// The caller stopped reading: end the presence now.
#[utoipa::path(
    delete, path = "/library/me/presence", operation_id = "library_reading_stop",
    responses((status = 204), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn stop(ctx: MemberCtx) -> ApiResult<StatusCode> {
    ctx.social()?
        .stop_reading(ctx.viewer.user_id)
        .await
        .map_err(social_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Friends who are reading something now and share it.
#[utoipa::path(
    get, path = "/library/friends/reading", operation_id = "library_friends_reading",
    responses((status = 200, body = FriendsReading), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn friends_reading(ctx: MemberCtx) -> ApiResult<Json<FriendsReading>> {
    ctx.social()?
        .friends_reading(ctx.sviewer())
        .await
        .map(Json)
        .map_err(social_err)
}

/// New chapters of followed series (bell on) since the caller last opened the bell, capped at 99.
#[utoipa::path(
    get, path = "/library/me/notifications", operation_id = "library_notification_count",
    responses((status = 200, body = NotificationCount), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn notification_count(ctx: MemberCtx) -> ApiResult<Json<NotificationCount>> {
    ctx.social()?
        .notification_count(ctx.sviewer())
        .await
        .map(Json)
        .map_err(social_err)
}

/// The caller opened the bell: everything so far counts as seen.
#[utoipa::path(
    post, path = "/library/me/notifications/seen", operation_id = "library_notifications_seen",
    responses((status = 204), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn notifications_seen(ctx: MemberCtx) -> ApiResult<StatusCode> {
    ctx.social()?
        .mark_notifications_seen(ctx.sviewer())
        .await
        .map_err(social_err)?;
    Ok(StatusCode::NO_CONTENT)
}
