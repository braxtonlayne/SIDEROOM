//! Favorites (MUS-LIB-001): the caller's starred tracks. Private to the member.

use super::{MusicLibDeps, StampedListView, check_tracks, lib_failed, parse_row, reachable_only};
use crate::{
    auth::SessionOnly,
    openapi::ErrorBody,
    routes::{Ext, MaybeExt, admin::AdminDeps, messages::error::MsgResult},
};
use axum::{Json, extract::Path, http::StatusCode};

pub(crate) async fn svc_list(deps: &MusicLibDeps, user: i64) -> MsgResult<StampedListView> {
    let rows = deps
        .store
        .favorites(user)
        .await
        .map_err(|e| lib_failed(e, "FAVORITES_FULL"))?;
    let readable = deps.readable(user).await?;
    Ok(reachable_only(rows, &readable))
}

pub(crate) async fn svc_add(
    deps: &MusicLibDeps,
    cap: i64,
    user: i64,
    track: &str,
) -> MsgResult<()> {
    let track = parse_row(track)?;
    check_tracks(deps, user, &[track]).await?;
    deps.store
        .add_favorite(user, track, cap)
        .await
        .map_err(|e| lib_failed(e, "FAVORITES_FULL"))
}

pub(crate) async fn svc_remove(deps: &MusicLibDeps, user: i64, track: &str) -> MsgResult<()> {
    let track = parse_row(track)?;
    deps.store
        .remove_favorite(user, track)
        .await
        .map_err(|e| lib_failed(e, "FAVORITES_FULL"))
}

/// The caller's favorites, newest first, without tracks they can no longer read.
#[utoipa::path(
    get,
    path = "/users/@me/music/favorites",
    responses((status = 200, body = StampedListView)),
    security(("session" = []))
)]
pub async fn list(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<MusicLibDeps>,
) -> MsgResult<Json<StampedListView>> {
    Ok(Json(svc_list(&d, p.user_id).await?))
}

/// Favorite a track (idempotent).
#[utoipa::path(
    put,
    path = "/users/@me/music/favorites/{track_id}",
    params(("track_id" = String, Path, description = "Track snowflake")),
    responses(
        (status = 204, description = "Favorited"),
        (status = 404, body = ErrorBody, description = "No such track, or one the caller cannot read"),
        (status = 409, body = ErrorBody, description = "FAVORITES_FULL: music.max_items reached"),
    ),
    security(("session" = []))
)]
pub async fn add(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<MusicLibDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    Path(track): Path<String>,
) -> MsgResult<StatusCode> {
    let cap = d.item_cap(admin.as_deref()).await;
    svc_add(&d, cap, p.user_id, &track).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Remove a favorite (removing one that is not there is fine).
#[utoipa::path(
    delete,
    path = "/users/@me/music/favorites/{track_id}",
    params(("track_id" = String, Path, description = "Track snowflake")),
    responses((status = 204, description = "Removed")),
    security(("session" = []))
)]
pub async fn remove(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<MusicLibDeps>,
    Path(track): Path<String>,
) -> MsgResult<StatusCode> {
    svc_remove(&d, p.user_id, &track).await?;
    Ok(StatusCode::NO_CONTENT)
}
