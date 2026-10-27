//! Attach the platform bucket (mount 1) to a server as a Guild Shelf (wave-27 follow-up of BYOS-INS-001).
//!
//! * `POST /admin/storage/mounts/{platform_mount_id}/attach` with `{"guild_id"}`: from then on the members of
//!   that server reach the instance bucket through the server's storage grants (`sr_storage::scopes`).
//! * `POST .../detach` with `{"guild_id"}` takes it away again.
//! * `GET .../attachments` lists the servers it is attached to.
//!
//! `OPERATOR` only, audited. Only a mount the platform owns can be attached: a member's or a server's own
//! bucket belongs to its owner. The shelf is read-only for members: `scopes::may_upload` is false for the
//! platform mount, so nothing here lets a member write into the instance bucket.

use super::{ScopeDeps, parse_row};
use crate::error::{ApiResult, HttpError};
use crate::routes::Ext;
use crate::routes::admin::{StaffCtx, map_err};
use crate::routes::messages::error::ApiJson;
use crate::{openapi::ErrorBody, state::AppState};
use axum::{Json, extract::Path, http::StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sr_core::{ApiError, ErrorCode, PlatformPermissions as P};
use sr_storage::mounts::MountOwner;
use utoipa::ToSchema;

/// Body of attach and detach.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AttachBody {
    /// The server (guild snowflake).
    pub guild_id: String,
}

/// A server the platform bucket is attached to.
#[derive(Debug, Serialize, ToSchema)]
pub struct AttachmentView {
    /// The server (guild snowflake).
    pub guild_id: String,
}

/// The routes.
pub fn router() -> axum::Router<AppState> {
    use axum::routing::{get, post};
    axum::Router::new()
        .route(
            "/admin/storage/mounts/{platform_mount_id}/attach",
            post(attach),
        )
        .route(
            "/admin/storage/mounts/{platform_mount_id}/detach",
            post(detach),
        )
        .route(
            "/admin/storage/mounts/{platform_mount_id}/attachments",
            get(list),
        )
}

fn not_found() -> HttpError {
    ErrorCode::SrNotFound.into()
}

fn facts(e: sr_storage::scopes::FactsError) -> HttpError {
    tracing::error!(error = %e, "storage attachment store failed");
    ErrorCode::SrInternal.into()
}

/// The platform mount named in the path; anything else is not attachable.
async fn platform_mount(d: &ScopeDeps, raw: &str) -> ApiResult<i64> {
    let id = parse_row(raw).map_err(|_| not_found())?;
    match d.mounts.by_id(id).await {
        Ok(Some(m)) if m.owner == MountOwner::Platform => Ok(id),
        Ok(Some(_)) => Err(ApiError::invalid_form()
            .field(
                "platform_mount_id",
                "NOT_PLATFORM_MOUNT",
                "Only the instance bucket can be attached to a server.",
            )
            .into()),
        Ok(None) => Err(not_found()),
        Err(e) => {
            tracing::error!(error = %e, "mount lookup failed");
            Err(ErrorCode::SrInternal.into())
        }
    }
}

fn guild(raw: &str) -> ApiResult<i64> {
    raw.parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| ErrorCode::UnknownGuild.into())
}

/// Attach the instance bucket to a server (`OPERATOR`, audited). Attaching twice is not an error.
#[utoipa::path(
    post,
    path = "/admin/storage/mounts/{platform_mount_id}/attach",
    params(("platform_mount_id" = String, Path, description = "The platform mount (1 on a standard install)")),
    request_body = AttachBody,
    responses(
        (status = 200, body = AttachmentView),
        (status = 400, body = ErrorBody),
        (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn attach(
    ctx: StaffCtx,
    Ext(d): Ext<ScopeDeps>,
    Path(mount): Path<String>,
    ApiJson(body): ApiJson<AttachBody>,
) -> ApiResult<Json<AttachmentView>> {
    ctx.svc()
        .authorize(&ctx.caller, P::OPERATOR, "storage.mount_attach")
        .await
        .map_err(map_err)?;
    let mount_id = platform_mount(&d, &mount).await?;
    let guild_id = guild(&body.guild_id)?;
    if !d.store.guild_exists(guild_id).await.map_err(facts)? {
        return Err(ErrorCode::UnknownGuild.into());
    }
    let added = d
        .store
        .attach_mount(mount_id, guild_id, ctx.caller.user_id)
        .await
        .map_err(facts)?;
    if added {
        ctx.svc()
            .record_action(
                &ctx.caller,
                "storage.mount_attach",
                Some(("storage_mount", mount_id)),
                json!({ "guild_id": guild_id.to_string() }),
            )
            .await
            .map_err(map_err)?;
    }
    Ok(Json(AttachmentView {
        guild_id: guild_id.to_string(),
    }))
}

/// Take the instance bucket away from a server (`OPERATOR`, audited). 404 when it was not attached.
#[utoipa::path(
    post,
    path = "/admin/storage/mounts/{platform_mount_id}/detach",
    params(("platform_mount_id" = String, Path, description = "The platform mount")),
    request_body = AttachBody,
    responses(
        (status = 204),
        (status = 400, body = ErrorBody),
        (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn detach(
    ctx: StaffCtx,
    Ext(d): Ext<ScopeDeps>,
    Path(mount): Path<String>,
    ApiJson(body): ApiJson<AttachBody>,
) -> ApiResult<StatusCode> {
    ctx.svc()
        .authorize(&ctx.caller, P::OPERATOR, "storage.mount_detach")
        .await
        .map_err(map_err)?;
    let mount_id = platform_mount(&d, &mount).await?;
    let guild_id = guild(&body.guild_id)?;
    if !d
        .store
        .detach_mount(mount_id, guild_id)
        .await
        .map_err(facts)?
    {
        return Err(not_found());
    }
    ctx.svc()
        .record_action(
            &ctx.caller,
            "storage.mount_detach",
            Some(("storage_mount", mount_id)),
            json!({ "guild_id": guild_id.to_string() }),
        )
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// The servers the instance bucket is attached to (`OPERATOR`).
#[utoipa::path(
    get,
    path = "/admin/storage/mounts/{platform_mount_id}/attachments",
    params(("platform_mount_id" = String, Path, description = "The platform mount")),
    responses(
        (status = 200, body = Vec<AttachmentView>),
        (status = 400, body = ErrorBody),
        (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn list(
    ctx: StaffCtx,
    Ext(d): Ext<ScopeDeps>,
    Path(mount): Path<String>,
) -> ApiResult<Json<Vec<AttachmentView>>> {
    ctx.svc()
        .authorize(&ctx.caller, P::OPERATOR, "storage.mount_attachments")
        .await
        .map_err(map_err)?;
    let mount_id = platform_mount(&d, &mount).await?;
    let guilds = d.store.attached_guilds(mount_id).await.map_err(facts)?;
    Ok(Json(
        guilds
            .into_iter()
            .map(|g| AttachmentView {
                guild_id: g.to_string(),
            })
            .collect(),
    ))
}
