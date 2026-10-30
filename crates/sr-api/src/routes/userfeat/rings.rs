//! PRF-RNG-001: 24-hour status rings.
//!
//! * `GET  /rings` the rings the caller may see, grouped by author (the avatar ring and the tray)
//! * `GET  /rings/{ring_id}` one ring
//! * `GET  /rings/{ring_id}/media` the photo or clip
//! * `POST /rings/{ring_id}/view` note that the caller opened it
//! * `GET  /users/@me/rings`, `POST /users/@me/rings` (text and track), `POST /users/@me/rings/media`
//!   (photo and clip, the file is the body), `DELETE /users/@me/rings/{ring_id}`
//! * `GET  /users/@me/rings/{ring_id}/viewers` the author's viewer list
//! * `GET  /moderation/rings/{ring_id}/media` staff read of a held ring (logged, needs a reason)
//!
//! **One rule for the audience:** every reader goes through the SQL function `status_ring_visible_to`, so a ring the
//! caller may not see answers `404 Unknown ring`, exactly like a ring that does not exist. Reports use the generic
//! `POST /moderation/reports` with `target_kind = "status_ring"`.
//!
//! The file of a photo or clip is stored on the author's own storage mount when they have a working one, else in
//! the instance bucket under `rings/`. Photos pass the banner's image pipeline (type sniffed, re-encoded, metadata
//! stripped, SVG refused). A clip is an MP4 or WebM of at most 60 seconds and 50 MiB; the container is sniffed, the
//! length is the client's claim (clamped to 60 s) because the server does not decode video.

use super::{UserFeatDeps, body_id, row_id, uerr};
use crate::auth::SessionOnly;
use crate::error::ApiResult;
use crate::openapi::ErrorBody;
use crate::routes::Ext;
use crate::routes::messages::error::{ApiJson, ApiQuery, MsgError, MsgResult, invalid};
use crate::routes::moderation::{Actor, map_err};
use axum::{
    Json,
    body::Bytes,
    extract::Path,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::Response,
};
use serde::Deserialize;
use sr_core::ErrorCode;
use sr_moderation::model::Who;
use sr_storage::media::{self, MediaKind};
use sr_storage::mount_storage::MountStorages;
use sr_storage::mounts::{INSTANCE_MOUNT_ID, MountOwner, MountResolver, MountStatus};
use sr_userfeat::rings::{
    INSTANCE_PREFIX, MAX_OBJECT_BYTES, NewRing, Removed, Ring, RingAudienceKind, RingFeedEntry,
    RingKind, RingObject, RingTrack, RingViewer,
};
use std::sync::Arc;
use utoipa::ToSchema;

/// What the ring routes need beyond the database: the mounts and a handle to each bucket.
pub struct RingDeps {
    pub storages: Arc<MountStorages>,
    pub mounts: Arc<dyn MountResolver>,
}

/// Request body limit of the upload route: the largest clip plus a little.
pub const UPLOAD_LIMIT: usize = 50 * 1024 * 1024 + 64 * 1024;

/// Body of `POST /users/@me/rings` (a text or track ring).
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateRingBody {
    /// `text` or `track`. Photos and clips use `POST /users/@me/rings/media`.
    pub kind: RingKind,
    /// The text of a text ring (up to 280 characters).
    #[serde(default)]
    pub text: String,
    /// The track of a track ring.
    #[serde(default)]
    pub track: Option<RingTrack>,
    /// `friends` or `guilds`.
    pub audience: RingAudienceKind,
    /// The chosen servers (`guilds` only; 1 to 10, ones the caller is in).
    #[serde(default)]
    pub guild_ids: Vec<String>,
}

/// Query of `POST /users/@me/rings/media`.
#[derive(Debug, Deserialize)]
pub struct MediaQuery {
    /// `photo` or `clip`.
    pub kind: RingKind,
    /// `friends` or `guilds`.
    pub audience: RingAudienceKind,
    /// Comma-separated server ids (`guilds` only).
    pub guild_ids: Option<String>,
    /// An optional caption (up to 280 characters).
    pub text: Option<String>,
    /// A clip's length in milliseconds (1 to 60 000).
    pub duration_ms: Option<i64>,
}

fn guild_ids(list: &[String]) -> MsgResult<Vec<i64>> {
    list.iter().map(|g| body_id("guild_ids", g)).collect()
}

/// The unique "unknown ring" answer: a hidden ring and a missing one are the same.
fn unknown_ring() -> MsgError {
    uerr(sr_userfeat::UfError::NotFound("ring"))
}

/// Mount to store a member's file on: their own active mount, else the instance bucket.
async fn mount_for(deps: &RingDeps, user: i64) -> i64 {
    match deps.mounts.for_owner(MountOwner::User(user)).await {
        Ok(list) => list
            .into_iter()
            .find(|m| m.status == MountStatus::Active)
            .map_or(INSTANCE_MOUNT_ID, |m| m.id),
        Err(e) => {
            tracing::warn!(error = %e, "could not look up the author's mounts for a ring");
            INSTANCE_MOUNT_ID
        }
    }
}

/// The container of a clip, from its first bytes: `("video/mp4" | "video/webm")`.
fn sniff_clip(b: &[u8]) -> Option<&'static str> {
    if b.len() >= 12 && &b[4..8] == b"ftyp" {
        return Some("video/mp4");
    }
    if b.len() >= 4 && b[..4] == [0x1A, 0x45, 0xDF, 0xA3] {
        return Some("video/webm");
    }
    None
}

/// Post a text or track ring.
#[utoipa::path(
    post,
    path = "/users/@me/rings",
    request_body = CreateRingBody,
    responses(
        (status = 201, body = Ring),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody, description = "A chosen server the caller is not in"),
        (status = 409, body = ErrorBody, description = "Daily limit reached"),
    ),
    security(("session" = []))
)]
pub async fn create(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<UserFeatDeps>,
    ApiJson(b): ApiJson<CreateRingBody>,
) -> MsgResult<(StatusCode, Json<Ring>)> {
    if b.kind.has_object() {
        return Err(invalid(
            "kind",
            "RING_KIND",
            "Photos and clips are uploaded to /users/@me/rings/media.",
        ));
    }
    let ring =
        d.uf.create_ring(
            p.user_id,
            NewRing {
                kind: b.kind,
                body: b.text,
                track: b.track,
                audience: b.audience,
                guild_ids: guild_ids(&b.guild_ids)?,
                object: None,
            },
        )
        .await
        .map_err(uerr)?;
    Ok((StatusCode::CREATED, Json(ring)))
}

/// Post a photo or clip: the file is the request body (`Content-Type` is ignored; the bytes are sniffed).
#[utoipa::path(
    post,
    path = "/users/@me/rings/media",
    params(
        ("kind" = String, Query, description = "photo or clip"),
        ("audience" = String, Query, description = "friends or guilds"),
        ("guild_ids" = Option<String>, Query, description = "Comma-separated server ids (guilds only)"),
        ("text" = Option<String>, Query, description = "Caption, up to 280 characters"),
        ("duration_ms" = Option<i64>, Query, description = "A clip's length, 1 to 60000"),
    ),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses(
        (status = 201, body = Ring),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "Daily limit reached"),
        (status = 413, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn upload(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<UserFeatDeps>,
    Ext(deps): Ext<RingDeps>,
    ApiQuery(q): ApiQuery<MediaQuery>,
    body: Bytes,
) -> MsgResult<(StatusCode, Json<Ring>)> {
    let user = p.user_id;
    if !q.kind.has_object() {
        return Err(invalid(
            "kind",
            "RING_KIND",
            "A text or track ring has no file.",
        ));
    }
    // Refuse before any byte is stored: the switch and the daily limit.
    d.uf.ring_precheck(user).await.map_err(uerr)?;
    let (_, max_clip_ms, max_bytes) = d.uf.ring_limits().await.map_err(uerr)?;
    if body.is_empty() {
        return Err(invalid("body", "RING_FILE", "Send the file as the body."));
    }
    if i64::try_from(body.len()).unwrap_or(i64::MAX) > max_bytes.min(MAX_OBJECT_BYTES) {
        return Err(ErrorCode::RequestEntityTooLarge.into());
    }
    let guilds = match (&q.audience, &q.guild_ids) {
        (RingAudienceKind::Guilds, Some(list)) => {
            let parts: Vec<String> = list
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect();
            guild_ids(&parts)?
        }
        _ => Vec::new(),
    };

    let (bytes, content_type, duration_ms): (Vec<u8>, &'static str, Option<i64>) = match q.kind {
        RingKind::Photo => {
            if q.duration_ms.is_some() {
                return Err(invalid(
                    "duration_ms",
                    "RING_DURATION",
                    "Only a clip has a length.",
                ));
            }
            let raw = body.to_vec();
            let prepared = tokio::task::spawn_blocking(move || {
                // Same checks as a profile picture: sniffed type, re-encoded (metadata gone), SVG refused.
                media::prepare_bytes_scoped(
                    MediaKind::ProfileMedia,
                    &raw,
                    &sr_storage::img::Limits::default(),
                    None,
                )
            })
            .await
            .map_err(|_| MsgError::from(ErrorCode::SrInternal))?
            .map_err(|_| {
                invalid(
                    "body",
                    "RING_PHOTO",
                    "That is not a PNG, JPEG, GIF or WebP picture we can use.",
                )
            })?;
            (prepared.bytes, prepared.format.mime(), None)
        }
        _ => {
            let Some(mime) = sniff_clip(&body) else {
                return Err(invalid(
                    "body",
                    "RING_CLIP",
                    "A clip must be an MP4 or WebM video.",
                ));
            };
            let ms = q.duration_ms.unwrap_or(0);
            if !(1..=max_clip_ms).contains(&ms) {
                return Err(invalid(
                    "duration_ms",
                    "RING_DURATION",
                    "A clip is at most 60 seconds long.",
                ));
            }
            (body.to_vec(), mime, Some(ms))
        }
    };

    let id = d.uf.new_ring_id();
    let mount_id = mount_for(&deps, user).await;
    let key = format!("{INSTANCE_PREFIX}{id}");
    let storage = deps.storages.for_mount(mount_id).await.map_err(|e| {
        tracing::error!(error = %e, mount_id, "no storage for a ring");
        MsgError::from(ErrorCode::SrInternal)
    })?;
    let size = i64::try_from(bytes.len()).unwrap_or(i64::MAX);
    storage.put(&key, bytes.into()).await.map_err(|e| {
        tracing::error!(error = %e, mount_id, "storing a ring failed");
        MsgError::from(ErrorCode::SrInternal)
    })?;
    let made =
        d.uf.create_ring_with_id(
            user,
            id,
            NewRing {
                kind: q.kind,
                body: q.text.unwrap_or_default(),
                track: None,
                audience: q.audience,
                guild_ids: guilds,
                object: Some(RingObject {
                    mount_id,
                    object_key: key.clone(),
                    content_type: content_type.to_owned(),
                    bytes: size,
                    duration_ms,
                }),
            },
        )
        .await;
    match made {
        Ok(ring) => Ok((StatusCode::CREATED, Json(ring))),
        Err(e) => {
            // The row was refused: do not leave the file behind.
            if let Err(del) = storage.delete(&key).await {
                tracing::warn!(error = %del, key, "could not remove the file of a refused ring");
            }
            Err(uerr(e))
        }
    }
}

/// The caller's own live rings.
#[utoipa::path(
    get,
    path = "/users/@me/rings",
    responses((status = 200, body = Vec<Ring>)),
    security(("session" = []))
)]
pub async fn mine(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<UserFeatDeps>,
) -> MsgResult<Json<Vec<Ring>>> {
    Ok(Json(d.uf.my_rings(p.user_id).await.map_err(uerr)?))
}

/// Delete one of the caller's rings now. A ring under a moderation hold is hidden and stays until the hold ends.
#[utoipa::path(
    delete,
    path = "/users/@me/rings/{ring_id}",
    params(("ring_id" = String, Path, description = "Ring snowflake")),
    responses((status = 204, description = "Deleted"), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn remove(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<UserFeatDeps>,
    Path(id): Path<String>,
) -> MsgResult<StatusCode> {
    let id = row_id(&id, "ring")?;
    let _: Removed = d.uf.delete_ring(p.user_id, id).await.map_err(uerr)?;
    Ok(StatusCode::NO_CONTENT)
}

/// The people who opened the caller's ring. Only the author can read this; everybody else gets 404.
#[utoipa::path(
    get,
    path = "/users/@me/rings/{ring_id}/viewers",
    params(("ring_id" = String, Path, description = "Ring snowflake")),
    responses((status = 200, body = Vec<RingViewer>), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn viewers(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<UserFeatDeps>,
    Path(id): Path<String>,
) -> MsgResult<Json<Vec<RingViewer>>> {
    let id = row_id(&id, "ring")?;
    Ok(Json(d.uf.ring_viewers(p.user_id, id).await.map_err(uerr)?))
}

/// Every ring the caller may see, grouped by author: mutual friends' rings and rings shared with a server they
/// are in. Their own come first.
#[utoipa::path(
    get,
    path = "/rings",
    responses((status = 200, body = Vec<RingFeedEntry>)),
    security(("session" = []))
)]
pub async fn feed(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<UserFeatDeps>,
) -> MsgResult<Json<Vec<RingFeedEntry>>> {
    Ok(Json(d.uf.ring_feed(p.user_id).await.map_err(uerr)?))
}

/// One ring. 404 when it does not exist, has expired, or the caller is not in its audience (the same answer).
#[utoipa::path(
    get,
    path = "/rings/{ring_id}",
    params(("ring_id" = String, Path, description = "Ring snowflake")),
    responses((status = 200, body = Ring), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get_one(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<UserFeatDeps>,
    Path(id): Path<String>,
) -> MsgResult<Json<Ring>> {
    let id = row_id(&id, "ring").map_err(|_| unknown_ring())?;
    Ok(Json(d.uf.ring_for(p.user_id, id).await.map_err(uerr)?))
}

/// Note that the caller opened a ring (the author's viewer list). 404 as for `GET /rings/{ring_id}`.
#[utoipa::path(
    post,
    path = "/rings/{ring_id}/view",
    params(("ring_id" = String, Path, description = "Ring snowflake")),
    responses((status = 204, description = "Noted"), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn view(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<UserFeatDeps>,
    Path(id): Path<String>,
) -> MsgResult<StatusCode> {
    let id = row_id(&id, "ring")?;
    d.uf.record_ring_view(p.user_id, id).await.map_err(uerr)?;
    Ok(StatusCode::NO_CONTENT)
}

/// The photo or clip of a ring. 404 as for `GET /rings/{ring_id}`. Supports one `Range: bytes=a-b`.
#[utoipa::path(
    get,
    path = "/rings/{ring_id}/media",
    params(("ring_id" = String, Path, description = "Ring snowflake")),
    responses(
        (status = 200, description = "The file", content_type = "application/octet-stream"),
        (status = 206, description = "A part of the file"),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn media_file(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<UserFeatDeps>,
    Ext(deps): Ext<RingDeps>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> MsgResult<Response> {
    let id = row_id(&id, "ring")?;
    let m = d.uf.ring_media(p.user_id, id).await.map_err(uerr)?;
    serve(&deps, m.mount_id, &m.object_key, &m.content_type, &headers).await
}

/// Staff read of a held ring's file (platform staff with `MANAGE_CASES`). A reason is required and the read is
/// written to the access log before the file leaves. Only a ring a report froze can be read this way.
#[utoipa::path(
    get,
    path = "/moderation/rings/{ring_id}/media",
    params(
        ("ring_id" = String, Path, description = "Ring snowflake"),
        ("reason" = String, Query, description = "Why the file is read"),
    ),
    responses(
        (status = 200, description = "The file", content_type = "application/octet-stream"),
        (status = 400, body = ErrorBody),
        (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn held_media(
    a: Actor,
    Ext(deps): Ext<RingDeps>,
    Path(id): Path<String>,
    ApiQuery(q): ApiQuery<ReasonQuery>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let id = crate::routes::moderation::id(&id, "ring")?;
    let ctx = a.staff_only()?;
    let held = a
        .svc()
        .read_held_ring(&Who::Staff(&ctx.caller), id, &q.reason)
        .await
        .map_err(map_err)?;
    serve(
        &deps,
        held.mount_id,
        &held.object_key,
        &held.content_type,
        &headers,
    )
    .await
    .map_err(|e| match e {
        MsgError::Api(a) => a.into(),
        _ => ErrorCode::SrInternal.into(),
    })
}

/// Query of the staff read.
#[derive(Debug, Deserialize)]
pub struct ReasonQuery {
    #[serde(default)]
    pub reason: String,
}

/// Stream a stored file with the headers that keep it from being cached, sniffed or run.
async fn serve(
    deps: &RingDeps,
    mount_id: i64,
    key: &str,
    content_type: &str,
    headers: &HeaderMap,
) -> MsgResult<Response> {
    let storage = deps.storages.for_mount(mount_id).await.map_err(|e| {
        tracing::warn!(error = %e, mount_id, "a ring's storage is not reachable");
        unknown_ring()
    })?;
    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_range);
    let (status, body, content_range) = match range {
        Some((start, end)) => {
            let info = storage.head(key).await.map_err(|_| unknown_ring())?;
            let size = info.size;
            if start >= size {
                return Err(unknown_ring());
            }
            let end = end.map_or(size, |e| (e + 1).min(size));
            let part = storage
                .get_range(key, start, end)
                .await
                .map_err(|_| unknown_ring())?;
            (
                StatusCode::PARTIAL_CONTENT,
                part,
                Some(format!("bytes {start}-{}/{size}", end - 1)),
            )
        }
        None => (
            StatusCode::OK,
            storage.get(key).await.map_err(|_| unknown_ring())?,
            None,
        ),
    };
    let mut resp = Response::new(axum::body::Body::from(body));
    *resp.status_mut() = status;
    let h = resp.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("sandbox; default-src 'none'"),
    );
    h.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    if let Some(cr) = content_range
        && let Ok(v) = HeaderValue::from_str(&cr)
    {
        h.insert(header::CONTENT_RANGE, v);
    }
    Ok(resp)
}

/// `bytes=a-b` or `bytes=a-` (one range only).
fn parse_range(v: &str) -> Option<(u64, Option<u64>)> {
    let spec = v.strip_prefix("bytes=")?;
    if spec.contains(',') {
        return None;
    }
    let (a, b) = spec.split_once('-')?;
    let start = a.trim().parse::<u64>().ok()?;
    let end = match b.trim() {
        "" => None,
        e => Some(e.parse::<u64>().ok()?),
    };
    match end {
        Some(e) if e < start => None,
        _ => Some((start, end)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_are_recognised_by_their_container() {
        assert_eq!(sniff_clip(b"\0\0\0\x18ftypmp42\0\0\0\0"), Some("video/mp4"));
        assert_eq!(
            sniff_clip(&[0x1A, 0x45, 0xDF, 0xA3, 1, 2]),
            Some("video/webm")
        );
        assert_eq!(
            sniff_clip(b"<svg xmlns='http://www.w3.org/2000/svg'/>"),
            None
        );
        assert_eq!(sniff_clip(b"GIF89a......"), None);
        assert_eq!(sniff_clip(b""), None);
    }

    #[test]
    fn ranges_parse_as_a_single_span() {
        assert_eq!(parse_range("bytes=0-99"), Some((0, Some(99))));
        assert_eq!(parse_range("bytes=100-"), Some((100, None)));
        assert_eq!(parse_range("bytes=5-1"), None);
        assert_eq!(parse_range("bytes=0-1,4-5"), None);
        assert_eq!(parse_range("items=0-1"), None);
        assert_eq!(parse_range("bytes=-5"), None);
    }
}
