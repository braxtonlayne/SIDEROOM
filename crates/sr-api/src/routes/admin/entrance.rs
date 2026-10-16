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

/// Set one copy block (`MANAGE_FEATURES`, fresh second factor). Audited with the key and length.
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
        .authorize(&ctx.caller, P::MANAGE_FEATURES, "entrance.copy")
        .await
        .map_err(map_err)?;
    ctx.require_reauth().await?;
    ctx.svc()
        .set_entrance_copy(&ctx.caller, &key, b.markdown.as_deref(), r.as_deref())
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Remove an override, so the client's default shows again (`MANAGE_FEATURES`, fresh second factor).
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
        .authorize(&ctx.caller, P::MANAGE_FEATURES, "entrance.copy")
        .await
        .map_err(map_err)?;
    ctx.require_reauth().await?;
    ctx.svc()
        .set_entrance_copy(&ctx.caller, &key, None, r.as_deref())
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}
