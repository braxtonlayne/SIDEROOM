//! Operator-editable Entrance copy (ENT-UI-010/025): the `/no-invitation` and `/house-notes` pages
//! and a few landing strings. The client keeps built-in defaults; a block exists here only after
//! staff overrode it.

use super::ctx::{AdminDeps, StaffCtx, map_err, reason};
use crate::{error::ApiResult, openapi::ErrorBody, routes::Ext};
use axum::{Json, extract::Path, http::HeaderMap, http::StatusCode};
use serde::Deserialize;
use std::collections::BTreeMap;
use utoipa::ToSchema;

/// The copy blocks staff overrode, key to markdown. Public (the Entrance is pre-login); blocks
/// without an override are missing and the client shows its default. Keys: `no-invitation`,
/// `house-notes`, `landing-headline`, `landing-subhead`, `login-notice`.
#[utoipa::path(
    get, path = "/sr/entrance/copy",
    responses((status = 200, body = BTreeMap<String, String>))
)]
pub async fn get_copy(Ext(deps): Ext<AdminDeps>) -> ApiResult<Json<BTreeMap<String, String>>> {
    deps.service
        .entrance_copy()
        .await
        .map(Json)
        .map_err(map_err)
}

/// Body of `PUT /admin/entrance/copy/{key}`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct EntranceCopyBody {
    /// Markdown, at most 20 000 characters. `null` or blank removes the override.
    pub markdown: Option<String>,
    pub reason: Option<String>,
}

/// Set or reset one copy block (`MANAGE_FEATURES`). Audited with the key and length.
#[utoipa::path(
    put, path = "/admin/entrance/copy/{key}", request_body = EntranceCopyBody,
    params(("key" = String, Path, description = "no-invitation, house-notes, landing-headline, landing-subhead or login-notice")),
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
        .set_entrance_copy(&ctx.caller, &key, b.markdown.as_deref(), r.as_deref())
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}
