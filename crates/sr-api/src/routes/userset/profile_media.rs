//! Profile page pictures (K-17, D-260): a background image and gallery pictures for the customised
//! profile page, next to the avatar and banner.
//!
//! * `POST   /users/@me/profile-media?kind=background|gallery`  the image is the request body
//! * `GET    /users/@me/profile-media`                         my pictures
//! * `DELETE /users/@me/profile-media/{id}`                    remove one (and its references)
//!
//! An upload goes through the banner's pipeline (`sr-media` sniffing, re-encode, metadata stripped, SVG
//! refused, the banner's size and animation caps). At most one background (a new one replaces the old) and
//! twelve gallery pictures. The files are served by the public `/cdn/avatars/<user>/<hash>.<ext>` route; a
//! profile document names a picture by id and `sr-packages` refuses any id that is not the caller's own.

use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query},
    http::StatusCode,
    routing::{delete, get},
};
use serde::Deserialize;
use sr_core::{ApiError, ErrorCode};
use sr_packages::service::media::{MediaView, NewMedia};
use sr_storage::media::MediaKind;
use utoipa::OpenApi;

use crate::{
    auth::SessionOnly,
    openapi::ErrorBody,
    routes::{
        Ext,
        media::MediaDeps,
        messages::error::{MsgError, MsgResult},
        packages::{PackagesDeps, pkg},
    },
    state::AppState,
};

/// Largest upload: the banner's 8 MiB plus a little.
const MAX_BODY: usize = sr_storage::media::MAX_UPLOAD_BYTES + 16 * 1024;

#[derive(Debug, Deserialize)]
pub struct UploadQuery {
    /// `background` or `gallery`.
    pub kind: String,
}

fn id(raw: &str) -> MsgResult<i64> {
    raw.parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| ApiError::new(ErrorCode::SrNotFound).into())
}

/// Upload a profile page picture: the body is the image (PNG, JPEG, GIF or WebP, at most 8 MiB, the same
/// checks as a banner). `kind=background` replaces the current background; `kind=gallery` adds one of at
/// most 12 (409 when full).
#[utoipa::path(
    post, path = "/users/@me/profile-media", tag = "profile-pages",
    params(("kind" = String, Query, description = "background or gallery")),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses((status = 201, body = MediaView), (status = 400, body = ErrorBody),
              (status = 409, body = ErrorBody), (status = 413, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn upload(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<PackagesDeps>,
    Ext(media): Ext<MediaDeps>,
    Query(q): Query<UploadQuery>,
    body: Bytes,
) -> MsgResult<(StatusCode, Json<MediaView>)> {
    let me = p.user_id;
    let owner = u64::try_from(me).map_err(|_| MsgError::from(ErrorCode::SrInternal))?;
    deps.service.media_check(me, &q.kind).await.map_err(pkg)?;
    let media_id = deps.service.media_id();
    let scope = u64::try_from(media_id).map_err(|_| MsgError::from(ErrorCode::SrInternal))?;
    let stored = media
        .store_raw(
            MediaKind::ProfileMedia,
            owner,
            scope,
            "image",
            body.to_vec(),
        )
        .await?;
    let added = deps
        .service
        .media_add(
            me,
            NewMedia {
                id: media_id,
                kind: q.kind,
                object_key: stored.key.clone(),
                content_type: stored.content_type.to_owned(),
                width: i32::try_from(stored.width).unwrap_or(i32::MAX),
                height: i32::try_from(stored.height).unwrap_or(i32::MAX),
                bytes: stored.bytes,
            },
        )
        .await;
    match added {
        Ok(view) => Ok((StatusCode::CREATED, Json(view))),
        Err(e) => {
            // The row was refused (a race past the cap): do not leave the file behind.
            media
                .remove(MediaKind::ProfileMedia, owner, &stored.hash)
                .await;
            Err(pkg(e))
        }
    }
}

/// My profile page pictures, oldest first.
#[utoipa::path(
    get, path = "/users/@me/profile-media", tag = "profile-pages",
    responses((status = 200, body = Vec<MediaView>)),
    security(("session" = []))
)]
pub async fn list(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<PackagesDeps>,
) -> MsgResult<Json<Vec<MediaView>>> {
    Ok(Json(deps.service.media_list(p.user_id).await.map_err(pkg)?))
}

/// Remove one of my pictures. Gallery entries and the background that named it are taken out of my saved
/// profile document.
#[utoipa::path(
    delete, path = "/users/@me/profile-media/{media_id}", tag = "profile-pages",
    params(("media_id" = String, Path, description = "Picture snowflake")),
    responses((status = 204), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn remove(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<PackagesDeps>,
    Path(media): Path<String>,
) -> MsgResult<StatusCode> {
    deps.service
        .media_delete(p.user_id, id(&media)?)
        .await
        .map_err(pkg)?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/users/@me/profile-media",
            get(list)
                .post(upload)
                .layer(DefaultBodyLimit::max(MAX_BODY)),
        )
        .route("/users/@me/profile-media/{media_id}", delete(remove))
}

/// OpenAPI fragment.
#[derive(OpenApi)]
#[openapi(paths(upload, list, remove), components(schemas(MediaView)))]
pub struct ProfileMediaApiDoc;
