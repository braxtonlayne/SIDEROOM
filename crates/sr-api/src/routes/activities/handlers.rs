//! Handlers of the Activities API. Each is a thin translation to `ActivityService`.

use super::{ActivitiesDeps, act};
use crate::{
    auth::{SessionOnly, UserOnly},
    openapi::ErrorBody,
    routes::{
        Ext,
        admin::StaffCtx,
        developer::DevDeps,
        messages::error::{ApiJson, MsgError, MsgResult},
    },
};
use axum::{Json, extract::Path, http::StatusCode};
use data_encoding::HEXLOWER;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sr_activities::model::{ConfigInput, ConfigView, InstanceView, PendingView, ShelfItem};
use sr_core::{ApiError, ErrorCode, PlatformPermissions};
use utoipa::ToSchema;

fn id(raw: &str, unknown: ErrorCode) -> MsgResult<i64> {
    raw.parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| ApiError::new(unknown).into())
}

fn sr_not_found() -> ErrorCode {
    ErrorCode::SrNotFound
}

/// The owner's settings, with the origin the Activity will run on.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConfigResponse {
    pub config: ConfigView,
    /// `[activities] enabled`: when off, nothing here is served.
    pub enabled: bool,
    /// `https://<application-id>.activities.<host>`. Register `<origin>/` as a redirect URI of the
    /// application so the Embedded App SDK's `authorize` can return a code.
    pub origin: String,
}

fn response(deps: &ActivitiesDeps, app: i64, config: ConfigView) -> Json<ConfigResponse> {
    Json(ConfigResponse {
        config,
        enabled: deps.enabled,
        origin: deps.activity_origin(app),
    })
}

/// The Activity settings of my application.
#[utoipa::path(
    get, path = "/applications/{application_id}/activity", tag = "activities",
    params(("application_id" = String, Path, description = "Application snowflake")),
    responses((status = 200, body = ConfigResponse), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get_config(
    SessionOnly(p): SessionOnly,
    Ext(dev): Ext<DevDeps>,
    Ext(deps): Ext<ActivitiesDeps>,
    Path(app): Path<String>,
) -> MsgResult<Json<ConfigResponse>> {
    let app = id(&app, ErrorCode::UnknownApplication)?;
    dev.apps
        .owned(p.user_id, app)
        .await
        .map_err(crate::routes::developer::app_err)?;
    let c = deps.service.config(app).await.map_err(act)?;
    Ok(response(&deps, app, c))
}

/// Set the entry URL and URL mappings. A change withdraws any approval and asks for a new review.
#[utoipa::path(
    put, path = "/applications/{application_id}/activity", tag = "activities",
    params(("application_id" = String, Path, description = "Application snowflake")),
    request_body = ConfigInput,
    responses((status = 200, body = ConfigResponse), (status = 400, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn put_config(
    SessionOnly(p): SessionOnly,
    Ext(dev): Ext<DevDeps>,
    Ext(deps): Ext<ActivitiesDeps>,
    Path(app): Path<String>,
    ApiJson(body): ApiJson<ConfigInput>,
) -> MsgResult<Json<ConfigResponse>> {
    let app = id(&app, ErrorCode::UnknownApplication)?;
    dev.apps
        .owned(p.user_id, app)
        .await
        .map_err(crate::routes::developer::app_err)?;
    let c = deps.service.save_config(app, body).await.map_err(act)?;
    Ok(response(&deps, app, c))
}

/// Submit for review. The server fetches the entry document now and pins its SHA-256: only a document with
/// that hash is served once staff approves.
#[utoipa::path(
    post, path = "/applications/{application_id}/activity/submit", tag = "activities",
    params(("application_id" = String, Path, description = "Application snowflake")),
    responses((status = 200, body = ConfigResponse),
              (status = 400, body = ErrorBody, description = "The entry document could not be fetched."),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn submit(
    SessionOnly(p): SessionOnly,
    Ext(dev): Ext<DevDeps>,
    Ext(deps): Ext<ActivitiesDeps>,
    Path(app): Path<String>,
) -> MsgResult<Json<ConfigResponse>> {
    let app = id(&app, ErrorCode::UnknownApplication)?;
    dev.apps
        .owned(p.user_id, app)
        .await
        .map_err(crate::routes::developer::app_err)?;
    let cfg = deps.service.config(app).await.map_err(act)?;
    let Some(entry) = cfg.entry_url.clone() else {
        return Err(ApiError::with_message(
            ErrorCode::SrConflict,
            "Set an entry URL before submitting.",
        )
        .into());
    };
    let Some(fetcher) = deps.fetcher.clone() else {
        return Err(ApiError::with_message(
            ErrorCode::SrConflict,
            "Fetching entry documents is not available on this server.",
        )
        .into());
    };
    let doc = fetcher
        .get(&entry, super::MAX_PROXIED_BYTES)
        .await
        .map_err(|why| {
            MsgError::Api(ApiError::invalid_form().field(
                "entry_url",
                "INVALID_VALUE",
                format!("The entry URL could not be fetched: {why}."),
            ))
        })?;
    let sha = HEXLOWER.encode(Sha256::digest(&doc.body).as_slice());
    let c = deps.service.submit(app, &sha).await.map_err(act)?;
    Ok(response(&deps, app, c))
}

fn require_review(staff: &StaffCtx) -> MsgResult<()> {
    if staff
        .caller
        .permissions
        .contains(PlatformPermissions::REVIEW_APPS)
    {
        Ok(())
    } else {
        Err(ApiError::with_message(
            ErrorCode::MissingPermissions,
            "Missing platform permission REVIEW_APPS",
        )
        .into())
    }
}

/// Staff: Activities waiting for review, oldest first.
#[utoipa::path(
    get, path = "/admin/developers/activities", tag = "activities",
    responses((status = 200, body = Vec<PendingView>), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn admin_pending(
    staff: StaffCtx,
    Ext(deps): Ext<ActivitiesDeps>,
) -> MsgResult<Json<Vec<PendingView>>> {
    require_review(&staff)?;
    Ok(Json(deps.service.pending(50).await.map_err(act)?))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DecisionBody {
    /// `approve` or `reject`.
    pub decision: String,
    pub note: Option<String>,
}

/// Staff: approve (the proxy then serves the submitted entry document) or reject an Activity.
#[utoipa::path(
    post, path = "/admin/developers/activities/{application_id}/decision", tag = "activities",
    params(("application_id" = String, Path, description = "Application snowflake")),
    request_body = DecisionBody,
    responses((status = 200, body = ConfigView), (status = 403, body = ErrorBody),
              (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn admin_decide(
    staff: StaffCtx,
    Ext(deps): Ext<ActivitiesDeps>,
    Path(app): Path<String>,
    ApiJson(body): ApiJson<DecisionBody>,
) -> MsgResult<Json<ConfigView>> {
    require_review(&staff)?;
    let app = id(&app, ErrorCode::UnknownApplication)?;
    let approve = match body.decision.as_str() {
        "approve" => true,
        "reject" => false,
        _ => {
            return Err(MsgError::Api(ApiError::invalid_form().field(
                "decision",
                "BASE_TYPE_CHOICES",
                "Must be approve or reject.",
            )));
        }
    };
    let c = deps
        .service
        .decide(app, staff.caller.user_id, approve, body.note.as_deref())
        .await
        .map_err(act)?;
    if let Err(e) = staff
        .svc()
        .record_action(
            &staff.caller,
            "developer.activity_decision",
            Some(("application", app)),
            serde_json::json!({"decision": body.decision, "sha256": c.reviewed_sha256}),
        )
        .await
    {
        tracing::error!(error = %e, "activity decision was not written to the access log");
    }
    Ok(Json(c))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ShelfResponse {
    /// `[activities] enabled`.
    pub enabled: bool,
    pub items: Vec<ShelfItem>,
}

/// The approved Activities a member can start in a voice channel.
#[utoipa::path(
    get, path = "/activities", tag = "activities",
    responses((status = 200, body = ShelfResponse), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn shelf(
    UserOnly(_p): UserOnly,
    Ext(deps): Ext<ActivitiesDeps>,
) -> MsgResult<Json<ShelfResponse>> {
    if !deps.enabled {
        return Ok(Json(ShelfResponse {
            enabled: false,
            items: Vec::new(),
        }));
    }
    Ok(Json(ShelfResponse {
        enabled: true,
        items: deps.service.shelf().await.map_err(act)?,
    }))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ChannelActivity {
    pub enabled: bool,
    pub instance: Option<InstanceView>,
}

/// The Activity running in a voice channel, if any.
#[utoipa::path(
    get, path = "/channels/{channel_id}/activity", tag = "activities",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 200, body = ChannelActivity), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn in_channel(
    UserOnly(p): UserOnly,
    Ext(deps): Ext<ActivitiesDeps>,
    Path(channel): Path<String>,
) -> MsgResult<Json<ChannelActivity>> {
    let channel = id(&channel, ErrorCode::UnknownChannel)?;
    if !deps.enabled {
        return Ok(Json(ChannelActivity {
            enabled: false,
            instance: None,
        }));
    }
    let instance = deps
        .service
        .in_channel(p.user_id, channel)
        .await
        .map_err(act)?;
    Ok(Json(ChannelActivity {
        enabled: true,
        instance,
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LaunchBody {
    pub application_id: String,
}

/// Start an approved Activity in a voice channel (needs Connect and Use Activities there). One Activity
/// runs per channel; the starter is its first participant.
#[utoipa::path(
    post, path = "/channels/{channel_id}/activity", tag = "activities",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    request_body = LaunchBody,
    responses((status = 201, body = InstanceView), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody, description = "One is already running.")),
    security(("session" = []))
)]
pub async fn launch(
    UserOnly(p): UserOnly,
    Ext(deps): Ext<ActivitiesDeps>,
    Path(channel): Path<String>,
    ApiJson(body): ApiJson<LaunchBody>,
) -> MsgResult<(StatusCode, Json<InstanceView>)> {
    deps.require_enabled()?;
    let channel = id(&channel, ErrorCode::UnknownChannel)?;
    let app = id(&body.application_id, sr_not_found())?;
    let v = deps
        .service
        .launch(p.user_id, app, channel)
        .await
        .map_err(act)?;
    Ok((StatusCode::CREATED, Json(v)))
}

const INSTANCE: &str = "Activity instance snowflake";

/// A running Activity and who is in it.
#[utoipa::path(
    get, path = "/activity-instances/{instance_id}", tag = "activities",
    params(("instance_id" = String, Path, description = INSTANCE)),
    responses((status = 200, body = InstanceView), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get_instance(
    UserOnly(p): UserOnly,
    Ext(deps): Ext<ActivitiesDeps>,
    Path(instance): Path<String>,
) -> MsgResult<Json<InstanceView>> {
    deps.require_enabled()?;
    let instance = id(&instance, sr_not_found())?;
    Ok(Json(
        deps.service.get(p.user_id, instance).await.map_err(act)?,
    ))
}

/// Join a running Activity.
#[utoipa::path(
    post, path = "/activity-instances/{instance_id}/join", tag = "activities",
    params(("instance_id" = String, Path, description = INSTANCE)),
    responses((status = 200, body = InstanceView), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn join(
    UserOnly(p): UserOnly,
    Ext(deps): Ext<ActivitiesDeps>,
    Path(instance): Path<String>,
) -> MsgResult<Json<InstanceView>> {
    deps.require_enabled()?;
    let instance = id(&instance, sr_not_found())?;
    Ok(Json(
        deps.service.join(p.user_id, instance).await.map_err(act)?,
    ))
}

/// Leave an Activity; the last one out ends it.
#[utoipa::path(
    post, path = "/activity-instances/{instance_id}/leave", tag = "activities",
    params(("instance_id" = String, Path, description = INSTANCE)),
    responses((status = 204), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn leave(
    UserOnly(p): UserOnly,
    Ext(deps): Ext<ActivitiesDeps>,
    Path(instance): Path<String>,
) -> MsgResult<StatusCode> {
    deps.require_enabled()?;
    let instance = id(&instance, sr_not_found())?;
    deps.service.leave(p.user_id, instance).await.map_err(act)?;
    Ok(StatusCode::NO_CONTENT)
}

/// End an Activity for everybody (its starter, or Manage Channels).
#[utoipa::path(
    delete, path = "/activity-instances/{instance_id}", tag = "activities",
    params(("instance_id" = String, Path, description = INSTANCE)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn end(
    UserOnly(p): UserOnly,
    Ext(deps): Ext<ActivitiesDeps>,
    Path(instance): Path<String>,
) -> MsgResult<StatusCode> {
    deps.require_enabled()?;
    let instance = id(&instance, sr_not_found())?;
    deps.service.end(p.user_id, instance).await.map_err(act)?;
    Ok(StatusCode::NO_CONTENT)
}
