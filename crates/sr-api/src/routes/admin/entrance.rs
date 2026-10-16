//! Operator-editable Entrance copy (ENT-UI-010/025): the `/no-invitation` and `/house-notes` pages
//! and the landing note. The client keeps built-in defaults; a block exists here only after staff
//! overrode it.

use super::ctx::{AdminDeps, StaffCtx, map_err, reason};
use crate::{error::ApiResult, openapi::ErrorBody, routes::Ext};
use axum::{
    Json,
    extract::Path,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sr_core::PlatformPermissions as P;
use utoipa::ToSchema;

/// The copy blocks staff overrode, key to markdown. Public (the Entrance is pre-login); blocks
/// without an override are missing and the client shows its default. Keys: `no-invitation`,
/// `house-notes`, `landing-note`. Answers with an `ETag` and `Cache-Control: max-age=60`;
/// `If-None-Match` gets 304.
#[utoipa::path(
    get, path = "/sr/entrance/copy",
    responses((status = 200, body = std::collections::BTreeMap<String, String>), (status = 304, description = "Unchanged"))
)]
pub async fn get_copy(headers: HeaderMap, Ext(deps): Ext<AdminDeps>) -> ApiResult<Response> {
    let copy = deps.service.entrance_copy().await.map_err(map_err)?;
    let body = serde_json::to_vec(&copy).unwrap_or_default();
    let etag = format!("\"{}\"", hex8(&Sha256::digest(&body)));
    let unchanged = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == etag);
    let mut resp = if unchanged {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        Json(copy).into_response()
    };
    let h = resp.headers_mut();
    if let Ok(v) = HeaderValue::from_str(&etag) {
        h.insert(header::ETAG, v);
    }
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=60"),
    );
    Ok(resp)
}

fn hex8(bytes: &[u8]) -> String {
    bytes.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// Body of `PUT /admin/entrance/copy/{key}`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct EntranceCopyBody {
    /// Markdown, at most 16 KiB. Blank removes the override.
    pub markdown: Option<String>,
    pub reason: Option<String>,
}

/// Set one copy block (`MANAGE_ENTRANCE`, fresh second factor). Audited with the key and length.
/// The text is stored as written; the Entrance renders it with the restricted entrance profile of
/// the shared markdown renderer (no mentions, no custom emoji).
#[utoipa::path(
    put, path = "/admin/entrance/copy/{key}", request_body = EntranceCopyBody,
    params(("key" = String, Path, description = "no-invitation, house-notes or landing-note")),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn put_copy(
    ctx: StaffCtx,
    Path(key): Path<String>,
    headers: HeaderMap,
    Json(b): Json<EntranceCopyBody>,
) -> ApiResult<StatusCode> {
    let r = reason(&headers, b.reason.as_deref());
    ctx.svc()
        .authorize(&ctx.caller, P::MANAGE_ENTRANCE, "entrance.copy")
        .await
        .map_err(map_err)?;
    ctx.require_reauth().await?;
    ctx.svc()
        .set_entrance_copy(&ctx.caller, &key, b.markdown.as_deref(), r.as_deref())
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Remove an override, so the client's default shows again (`MANAGE_ENTRANCE`, fresh second factor).
#[utoipa::path(
    delete, path = "/admin/entrance/copy/{key}",
    params(("key" = String, Path, description = "no-invitation, house-notes or landing-note")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_copy(
    ctx: StaffCtx,
    Path(key): Path<String>,
    headers: HeaderMap,
) -> ApiResult<StatusCode> {
    let r = reason(&headers, None);
    ctx.svc()
        .authorize(&ctx.caller, P::MANAGE_ENTRANCE, "entrance.copy")
        .await
        .map_err(map_err)?;
    ctx.require_reauth().await?;
    ctx.svc()
        .set_entrance_copy(&ctx.caller, &key, None, r.as_deref())
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

// ===== hero image (H-41, D-239) ===================================================

/// Object key of the operator's hero image.
const HERO_KEY: &str = "entrance/hero";
/// Longest side after the shrink.
const HERO_PX: u32 = 1600;
/// Largest stored image.
const HERO_MAX_BYTES: usize = 4 * 1024 * 1024;
/// Largest upload before the shrink.
const HERO_MAX_UPLOAD: usize = 8 * 1024 * 1024;

fn hero_invalid(code: &str, message: &str) -> crate::error::HttpError {
    sr_core::ApiError::invalid_form()
        .field("image", code, message)
        .into()
}

/// Body of `PUT /admin/entrance/hero`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct EntranceHeroBody {
    /// `data:image/png;base64,...`: PNG, JPEG or WebP, at most 8 MiB; the longest side is shrunk
    /// to 1600 px and the stored image may be at most 4 MiB.
    pub image: String,
    pub reason: Option<String>,
}

/// Replace the Entrance hero image (`MANAGE_ENTRANCE`, fresh second factor). The image is sniffed
/// (never trusted by its declared type), shrunk, re-encoded without metadata and stored at
/// `entrance/hero`. Audited.
#[utoipa::path(
    put, path = "/admin/entrance/hero", request_body = EntranceHeroBody,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn put_hero(
    ctx: StaffCtx,
    headers: HeaderMap,
    Json(b): Json<EntranceHeroBody>,
) -> ApiResult<StatusCode> {
    let r = reason(&headers, b.reason.as_deref());
    ctx.svc()
        .authorize(&ctx.caller, P::MANAGE_ENTRANCE, "entrance.hero")
        .await
        .map_err(map_err)?;
    ctx.require_reauth().await?;
    let storage = ctx
        .deps
        .storage
        .clone()
        .ok_or_else(|| crate::error::HttpError::from(sr_core::ErrorCode::SrInternal))?;
    let raw = crate::routes::settings::expressions::decode_data_uri("image", &b.image)?;
    if raw.len() > HERO_MAX_UPLOAD {
        return Err(hero_invalid(
            "IMAGE_TOO_LARGE",
            "The image may be at most 8 MiB.",
        ));
    }
    use crate::routes::settings::expressions::{Image, sniff};
    if !matches!(
        sniff(&raw),
        Some(Image::Png { .. } | Image::Jpeg | Image::Webp { .. })
    ) {
        return Err(hero_invalid(
            "IMAGE_INVALID",
            "Must be a PNG, JPEG or WebP image.",
        ));
    }
    let stored = tokio::task::spawn_blocking(move || {
        sr_storage::img::normalize(
            &raw,
            sr_storage::img::Fit::Bound {
                w: HERO_PX,
                h: HERO_PX,
            },
            &sr_storage::img::Limits::default(),
        )
    })
    .await
    .map_err(|_| crate::error::HttpError::from(sr_core::ErrorCode::SrInternal))?
    .map_err(|_| hero_invalid("IMAGE_INVALID", "That image could not be read."))?;
    if stored.bytes.len() > HERO_MAX_BYTES {
        return Err(hero_invalid(
            "IMAGE_TOO_LARGE",
            "The image is over 4 MiB after it was shrunk. Try a JPEG.",
        ));
    }
    let bytes = stored.bytes.len();
    storage
        .put(HERO_KEY, stored.bytes.into())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "storing the entrance hero failed");
            crate::error::HttpError::from(sr_core::ErrorCode::SrInternal)
        })?;
    ctx.svc()
        .record(
            &ctx.caller,
            "entrance.hero",
            None,
            r.as_deref().unwrap_or(""),
            serde_json::json!({ "set": true, "bytes": bytes, "width": stored.width, "height": stored.height }),
        )
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Remove the hero image, so the shipped art shows again (`MANAGE_ENTRANCE`, fresh second factor).
#[utoipa::path(
    delete, path = "/admin/entrance/hero",
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_hero(ctx: StaffCtx, headers: HeaderMap) -> ApiResult<StatusCode> {
    let r = reason(&headers, None);
    ctx.svc()
        .authorize(&ctx.caller, P::MANAGE_ENTRANCE, "entrance.hero")
        .await
        .map_err(map_err)?;
    ctx.require_reauth().await?;
    let storage = ctx
        .deps
        .storage
        .clone()
        .ok_or_else(|| crate::error::HttpError::from(sr_core::ErrorCode::SrInternal))?;
    if storage.head(HERO_KEY).await.is_ok() {
        storage.delete(HERO_KEY).await.map_err(|e| {
            tracing::error!(error = %e, "deleting the entrance hero failed");
            crate::error::HttpError::from(sr_core::ErrorCode::SrInternal)
        })?;
    }
    ctx.svc()
        .record(
            &ctx.caller,
            "entrance.hero",
            None,
            r.as_deref().unwrap_or(""),
            serde_json::json!({ "set": false }),
        )
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// The operator's Entrance hero image. Public (the Entrance is pre-login); 404 when none is set.
/// `ETag` from the content, `Cache-Control: public, max-age=3600`; `If-None-Match` gets 304.
#[utoipa::path(
    get, path = "/sr/entrance/hero",
    responses((status = 200, description = "The image", content_type = "image/*"),
              (status = 304, description = "Unchanged"), (status = 404, body = ErrorBody))
)]
pub async fn get_hero(headers: HeaderMap, Ext(deps): Ext<AdminDeps>) -> ApiResult<Response> {
    let not_found = || crate::error::HttpError::from(sr_core::ErrorCode::SrNotFound);
    let storage = deps.storage.clone().ok_or_else(not_found)?;
    let data = storage.get(HERO_KEY).await.map_err(|_| not_found())?;
    let etag = format!("\"{}\"", hex8(&Sha256::digest(&data)));
    let mime =
        sr_storage::img::ImgFormat::sniff(&data).map_or("application/octet-stream", |f| f.mime());
    let unchanged = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == etag);
    let mut resp = if unchanged {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        (StatusCode::OK, data).into_response()
    };
    let h = resp.headers_mut();
    if !unchanged {
        h.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    }
    if let Ok(v) = HeaderValue::from_str(&etag) {
        h.insert(header::ETAG, v);
    }
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=3600"),
    );
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    Ok(resp)
}
