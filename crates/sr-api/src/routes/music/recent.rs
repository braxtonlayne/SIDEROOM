//! Recently played (MUS-LIB-001): the caller's last 200 tracks, newest first. The player records a play when a
//! track has really started (not on every seek). Private to the member.

use super::{MusicLibDeps, StampedListView, check_tracks, lib_failed, parse_row, reachable_only};
use crate::{
    auth::SessionOnly,
    openapi::ErrorBody,
    routes::{
        Ext,
        messages::error::{ApiJson, ApiQuery, MsgResult},
    },
};
use axum::{Json, http::StatusCode};
use serde::Deserialize;
use sr_storage::music_lib::RECENT_KEEP;
use utoipa::ToSchema;

/// Body of the record call.
#[derive(Debug, Deserialize, ToSchema)]
pub struct RecordBody {
    /// The track that started playing.
    pub track_id: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    /// 1 to 200, default 50.
    pub limit: Option<i64>,
}

pub(crate) async fn svc_list(
    deps: &MusicLibDeps,
    user: i64,
    limit: Option<i64>,
) -> MsgResult<StampedListView> {
    let rows = deps
        .store
        .recent(user, limit.unwrap_or(50).clamp(1, RECENT_KEEP))
        .await
        .map_err(|e| lib_failed(e, "LIST_FULL"))?;
    let readable = deps.readable(user).await?;
    Ok(reachable_only(rows, &readable))
}

pub(crate) async fn svc_record(deps: &MusicLibDeps, user: i64, track: &str) -> MsgResult<()> {
    let track = parse_row(track)?;
    check_tracks(deps, user, &[track]).await?;
    deps.store
        .record_recent(user, track)
        .await
        .map_err(|e| lib_failed(e, "LIST_FULL"))
}

/// The caller's recently played tracks, newest first, without tracks they can no longer read.
#[utoipa::path(
    get,
    path = "/users/@me/music/recent",
    params(("limit" = Option<i64>, Query, description = "1 to 200, default 50")),
    responses((status = 200, body = StampedListView)),
    security(("session" = []))
)]
pub async fn list(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<MusicLibDeps>,
    ApiQuery(q): ApiQuery<ListQuery>,
) -> MsgResult<Json<StampedListView>> {
    Ok(Json(svc_list(&d, p.user_id, q.limit).await?))
}

/// Record that a track started playing: it moves to the top and the list is pruned to 200.
#[utoipa::path(
    post,
    path = "/users/@me/music/recent",
    request_body = RecordBody,
    responses(
        (status = 204, description = "Recorded"),
        (status = 404, body = ErrorBody, description = "No such track, or one the caller cannot read"),
    ),
    security(("session" = []))
)]
pub async fn record(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<MusicLibDeps>,
    ApiJson(body): ApiJson<RecordBody>,
) -> MsgResult<StatusCode> {
    svc_record(&d, p.user_id, &body.track_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Forget the history.
#[utoipa::path(
    delete,
    path = "/users/@me/music/recent",
    responses((status = 204, description = "Cleared")),
    security(("session" = []))
)]
pub async fn clear(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<MusicLibDeps>,
) -> MsgResult<StatusCode> {
    d.store
        .clear_recent(p.user_id)
        .await
        .map_err(|e| lib_failed(e, "LIST_FULL"))?;
    Ok(StatusCode::NO_CONTENT)
}
