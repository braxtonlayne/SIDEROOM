//! Watch progress, watched marks, history, continue watching and "watching" presence
//! (CIN-PLY-007, CIN-PLY-009). Every change is also dispatched as `SR_CINEMA_UPDATE` to the
//! caller's other sessions.

use super::{CinemaCtx, id, map_err};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use serde::Deserialize;
use sr_cinema_social::SocialError;
use sr_cinema_social::model::*;
use utoipa::IntoParams;

const NO_PROGRESS: &str = "No progress saved yet, or unknown title or episode.";

async fn get_progress(ctx: &CinemaCtx, title: &str, episode: &str) -> ApiResult<Json<Progress>> {
    let (t, e) = ids(title, episode)?;
    ctx.svc()
        .progress(ctx.viewer, t, e)
        .await
        .map(Json)
        .map_err(map_err)
}

fn ids(title: &str, episode: &str) -> ApiResult<(i64, i64)> {
    let ep = if episode == "0" {
        0
    } else {
        id(episode, "episode")?
    };
    Ok((id(title, "title")?, ep))
}

async fn put_progress(
    ctx: &CinemaCtx,
    title: &str,
    episode: &str,
    body: ProgressInput,
) -> ApiResult<Json<ProgressSaved>> {
    let (t, e) = ids(title, episode)?;
    ctx.svc()
        .save_progress(ctx.viewer, t, e, body)
        .await
        .map(Json)
        .map_err(map_err)
}

async fn delete_progress(ctx: &CinemaCtx, title: &str, episode: &str) -> ApiResult<StatusCode> {
    let (t, e) = ids(title, episode)?;
    let removed = ctx
        .svc()
        .clear_progress(ctx.viewer, t, e)
        .await
        .map_err(map_err)?;
    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(map_err(SocialError::NotFound("progress")))
    }
}

/// The caller's progress in a movie.
#[utoipa::path(
    get, path = "/cinema/titles/{title_id}/progress", operation_id = "cinema_get_movie_progress",
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 200, body = Progress), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody, description = "Age confirmation required (900011)."),
              (status = 404, body = ErrorBody, description = NO_PROGRESS)),
    security(("session" = []))
)]
pub async fn get_movie_progress(
    ctx: CinemaCtx,
    Path(title_id): Path<String>,
) -> ApiResult<Json<Progress>> {
    get_progress(&ctx, &title_id, "0").await
}

/// Save movie progress (the player heartbeat, every 10 s while playing). With `base_updated_at_ms` a
/// newer stored position is kept and returned with `applied: false`; repeat with `force` to overwrite.
/// `completed` is automatic at 90%. `playing: false` also ends the "watching" presence.
#[utoipa::path(
    put, path = "/cinema/titles/{title_id}/progress", operation_id = "cinema_put_movie_progress",
    request_body = ProgressInput,
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 200, body = ProgressSaved), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody, description = "Age confirmation required (900011)."),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn put_movie_progress(
    ctx: CinemaCtx,
    Path(title_id): Path<String>,
    Json(b): Json<ProgressInput>,
) -> ApiResult<Json<ProgressSaved>> {
    put_progress(&ctx, &title_id, "0", b).await
}

/// Forget a movie's progress.
#[utoipa::path(
    delete, path = "/cinema/titles/{title_id}/progress", operation_id = "cinema_delete_movie_progress",
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_movie_progress(
    ctx: CinemaCtx,
    Path(title_id): Path<String>,
) -> ApiResult<StatusCode> {
    delete_progress(&ctx, &title_id, "0").await
}

/// The caller's progress in an episode.
#[utoipa::path(
    get, path = "/cinema/titles/{title_id}/episodes/{episode_id}/progress", operation_id = "cinema_get_episode_progress",
    params(("title_id" = String, Path, description = "Title snowflake"), ("episode_id" = String, Path, description = "Episode snowflake")),
    responses((status = 200, body = Progress), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody, description = "Age confirmation required (900011)."),
              (status = 404, body = ErrorBody, description = NO_PROGRESS)),
    security(("session" = []))
)]
pub async fn get_episode_progress(
    ctx: CinemaCtx,
    Path((title_id, episode_id)): Path<(String, String)>,
) -> ApiResult<Json<Progress>> {
    get_progress(&ctx, &title_id, &episode_id).await
}

/// Save episode progress; see the movie variant for the rules.
#[utoipa::path(
    put, path = "/cinema/titles/{title_id}/episodes/{episode_id}/progress", operation_id = "cinema_put_episode_progress",
    request_body = ProgressInput,
    params(("title_id" = String, Path, description = "Title snowflake"), ("episode_id" = String, Path, description = "Episode snowflake")),
    responses((status = 200, body = ProgressSaved), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody, description = "Age confirmation required (900011)."),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn put_episode_progress(
    ctx: CinemaCtx,
    Path((title_id, episode_id)): Path<(String, String)>,
    Json(b): Json<ProgressInput>,
) -> ApiResult<Json<ProgressSaved>> {
    put_progress(&ctx, &title_id, &episode_id, b).await
}

/// Forget an episode's progress.
#[utoipa::path(
    delete, path = "/cinema/titles/{title_id}/episodes/{episode_id}/progress", operation_id = "cinema_delete_episode_progress",
    params(("title_id" = String, Path, description = "Title snowflake"), ("episode_id" = String, Path, description = "Episode snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_episode_progress(
    ctx: CinemaCtx,
    Path((title_id, episode_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    delete_progress(&ctx, &title_id, &episode_id).await
}

/// Every progress row of a title (episode ticks and the season/title watched state).
#[utoipa::path(
    get, path = "/cinema/titles/{title_id}/watch-state", operation_id = "cinema_watch_state",
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 200, body = WatchState), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn watch_state(
    ctx: CinemaCtx,
    Path(title_id): Path<String>,
) -> ApiResult<Json<WatchState>> {
    ctx.svc()
        .watch_state(ctx.viewer, id(&title_id, "title")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Mark an episode, a season (`season`) or a whole title watched or unwatched.
#[utoipa::path(
    post, path = "/cinema/titles/{title_id}/watched", operation_id = "cinema_mark_watched",
    request_body = WatchedInput,
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 200, body = WatchedResult), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn watched(
    ctx: CinemaCtx,
    Path(title_id): Path<String>,
    Json(b): Json<WatchedInput>,
) -> ApiResult<Json<WatchedResult>> {
    ctx.svc()
        .mark_watched(ctx.viewer, id(&title_id, "title")?, b)
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PageParams {
    /// Cursor from the previous page's `next_before`.
    pub before: Option<String>,
    /// 1 to 100, default 24.
    pub limit: Option<u32>,
}

/// Watch history, newest first (every movie and episode separately).
#[utoipa::path(
    get, path = "/cinema/me/history", operation_id = "cinema_history", params(PageParams),
    responses((status = 200, body = HistoryPage), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn history(ctx: CinemaCtx, Query(p): Query<PageParams>) -> ApiResult<Json<HistoryPage>> {
    ctx.svc()
        .history(ctx.viewer, p.before.as_deref(), p.limit)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Clear all watch history, progress included.
#[utoipa::path(
    delete, path = "/cinema/me/history", operation_id = "cinema_clear_history",
    responses((status = 204), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn clear_history(ctx: CinemaCtx) -> ApiResult<StatusCode> {
    ctx.svc().clear_history(ctx.viewer).await.map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Whether recording of history is paused.
#[utoipa::path(
    get, path = "/cinema/me/history/settings", operation_id = "cinema_get_history_settings",
    responses((status = 200, body = HistorySettings), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get_history_settings(ctx: CinemaCtx) -> ApiResult<Json<HistorySettings>> {
    ctx.svc()
        .history_settings(ctx.viewer)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Pause or resume recording of history. While paused nothing is stored and no presence is shown.
#[utoipa::path(
    put, path = "/cinema/me/history/settings", operation_id = "cinema_put_history_settings",
    request_body = HistorySettings,
    responses((status = 200, body = HistorySettings), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn put_history_settings(
    ctx: CinemaCtx,
    Json(b): Json<HistorySettings>,
) -> ApiResult<Json<HistorySettings>> {
    ctx.svc()
        .set_history_paused(ctx.viewer, b.paused)
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct LimitParams {
    /// 1 to 100, default 24.
    pub limit: Option<u32>,
}

/// Unfinished items to resume, and the next episode after a finished one, latest first.
#[utoipa::path(
    get, path = "/cinema/me/continue-watching", operation_id = "cinema_continue_watching", params(LimitParams),
    responses((status = 200, body = ContinueWatching), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn continue_watching(
    ctx: CinemaCtx,
    Query(p): Query<LimitParams>,
) -> ApiResult<Json<ContinueWatching>> {
    ctx.svc()
        .continue_watching(ctx.viewer, p.limit)
        .await
        .map(Json)
        .map_err(map_err)
}

/// End the caller's "watching" presence now.
#[utoipa::path(
    delete, path = "/cinema/me/watching", operation_id = "cinema_stop_watching",
    responses((status = 204), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn stop_watching(ctx: CinemaCtx) -> ApiResult<StatusCode> {
    ctx.svc()
        .stop_watching(ctx.viewer.user_id)
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Friends who are watching something now and share it (`cinema.share_watching`).
#[utoipa::path(
    get, path = "/cinema/me/friends-watching", operation_id = "cinema_friends_watching",
    responses((status = 200, body = FriendsWatching), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn friends_watching(ctx: CinemaCtx) -> ApiResult<Json<FriendsWatching>> {
    ctx.svc()
        .friends_watching(ctx.viewer)
        .await
        .map(Json)
        .map_err(map_err)
}
