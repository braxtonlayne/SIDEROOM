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
use axum::{
    Json,
    body::Bytes,
    extract::Path,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use sr_activities::model::{
    BundleDetail, ConfigInput, ConfigView, InstanceView, PendingView, ShelfItem,
};
use sr_core::{ApiError, ErrorCode, PlatformPermissions};
use utoipa::ToSchema;

/// Largest bundle upload: the 32 MiB limit of a bundle plus headroom for the zip's own framing.
pub const MAX_UPLOAD: usize = sr_activities::bundle::MAX_UNPACKED_BYTES as usize + 64 * 1024;

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
    /// `https://<application-id>.activities.<host>`.
    pub origin: String,
    /// `<origin>/`: accepted as a redirect URI of the application without being stored (J-13), present once
    /// the application has an Activity.
    pub implicit_redirect_uri: Option<String>,
}

fn response(deps: &ActivitiesDeps, app: i64, config: ConfigView) -> Json<ConfigResponse> {
    let origin = deps.activity_origin(app);
    // A settings row exists as soon as anything was saved or uploaded.
    let implicit = (config.updated_at_ms > 0).then(|| format!("{origin}/"));
    Json(ConfigResponse {
        config,
        enabled: deps.enabled,
        origin,
        implicit_redirect_uri: implicit,
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

/// Set the development URL and the URL mappings. A change to the mappings withdraws the approval of the
/// reviewed bundle until a new review.
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

/// Upload a bundle: the request body is a zip of static files (at most 32 MiB unpacked and 2,000 entries,
/// `index.html` at the root, allow-listed file types, no symbolic links, absolute paths or `..`). A refusal
/// names the offending entry. The new bundle is a `draft`; earlier drafts and rejected bundles are
/// superseded. Uploading the same zip again returns the same bundle.
#[utoipa::path(
    post, path = "/applications/{application_id}/activity/bundles", tag = "activities",
    params(("application_id" = String, Path, description = "Application snowflake")),
    request_body(content = Vec<u8>, content_type = "application/zip"),
    responses((status = 201, body = BundleDetail),
              (status = 400, body = ErrorBody, description = "The zip was refused; the error names the entry."),
              (status = 404, body = ErrorBody), (status = 413, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn upload_bundle(
    SessionOnly(p): SessionOnly,
    Ext(dev): Ext<DevDeps>,
    Ext(deps): Ext<ActivitiesDeps>,
    Path(app): Path<String>,
    body: Bytes,
) -> MsgResult<(StatusCode, Json<BundleDetail>)> {
    let app = id(&app, ErrorCode::UnknownApplication)?;
    dev.apps
        .owned(p.user_id, app)
        .await
        .map_err(crate::routes::developer::app_err)?;
    let detail = deps
        .service
        .upload_bundle(app, p.user_id, body.to_vec())
        .await
        .map_err(act)?;
    Ok((StatusCode::CREATED, Json(detail)))
}

/// One uploaded bundle with the list of its files.
#[utoipa::path(
    get, path = "/applications/{application_id}/activity/bundles/{bundle_id}", tag = "activities",
    params(("application_id" = String, Path, description = "Application snowflake"),
           ("bundle_id" = String, Path, description = "Bundle snowflake")),
    responses((status = 200, body = BundleDetail), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get_bundle(
    SessionOnly(p): SessionOnly,
    Ext(dev): Ext<DevDeps>,
    Ext(deps): Ext<ActivitiesDeps>,
    Path((app, bundle)): Path<(String, String)>,
) -> MsgResult<Json<BundleDetail>> {
    let app = id(&app, ErrorCode::UnknownApplication)?;
    let bundle = id(&bundle, sr_not_found())?;
    dev.apps
        .owned(p.user_id, app)
        .await
        .map_err(crate::routes::developer::app_err)?;
    Ok(Json(
        deps.service.bundle_detail(app, bundle).await.map_err(act)?,
    ))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = ActivitySubmitBody)]
pub struct SubmitBody {
    /// The uploaded bundle to put in front of staff.
    pub bundle_id: String,
}

/// Submit an uploaded bundle for review. An earlier pending bundle is replaced; a reviewed bundle keeps
/// being served until staff decide.
#[utoipa::path(
    post, path = "/applications/{application_id}/activity/submit", tag = "activities",
    params(("application_id" = String, Path, description = "Application snowflake")),
    request_body = SubmitBody,
    responses((status = 200, body = ConfigResponse),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn submit(
    SessionOnly(p): SessionOnly,
    Ext(dev): Ext<DevDeps>,
    Ext(deps): Ext<ActivitiesDeps>,
    Path(app): Path<String>,
    ApiJson(body): ApiJson<SubmitBody>,
) -> MsgResult<Json<ConfigResponse>> {
    let app = id(&app, ErrorCode::UnknownApplication)?;
    let bundle = id(&body.bundle_id, sr_not_found())?;
    dev.apps
        .owned(p.user_id, app)
        .await
        .map_err(crate::routes::developer::app_err)?;
    let c = deps.service.submit(app, bundle).await.map_err(act)?;
    Ok(response(&deps, app, c))
}

fn require_review(staff: &StaffCtx) -> MsgResult<()> {
    if staff
        .caller
        .permissions
        .has(PlatformPermissions::REVIEW_APPS)
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

/// Staff: Activities waiting for review, oldest first, each with the bundle's hash and files.
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

/// Staff: the zip of a submitted bundle, as the owner uploaded it, to read before deciding.
#[utoipa::path(
    get, path = "/admin/developers/activities/{application_id}/bundles/{bundle_id}/download", tag = "activities",
    params(("application_id" = String, Path, description = "Application snowflake"),
           ("bundle_id" = String, Path, description = "Bundle snowflake")),
    responses((status = 200, content_type = "application/zip", body = Vec<u8>),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn admin_download(
    staff: StaffCtx,
    Ext(deps): Ext<ActivitiesDeps>,
    Path((app, bundle)): Path<(String, String)>,
) -> MsgResult<Response> {
    require_review(&staff)?;
    let app = id(&app, ErrorCode::UnknownApplication)?;
    let bundle = id(&bundle, sr_not_found())?;
    let (sha, zip) = deps.service.bundle_zip(app, bundle).await.map_err(act)?;
    if let Err(e) = staff
        .svc()
        .record_action(
            &staff.caller,
            "developer.activity_download",
            Some(("application", app)),
            serde_json::json!({"sha256": sha}),
        )
        .await
    {
        tracing::error!(error = %e, "activity download was not written to the access log");
    }
    let mut r = (StatusCode::OK, zip).into_response();
    let h = r.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    if let Ok(v) = HeaderValue::from_str(&format!(
        "attachment; filename=\"activity-{app}-{}.zip\"",
        &sha[..12]
    )) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(r)
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = ActivityDecisionBody)]
pub struct DecisionBody {
    /// `approve` or `reject`.
    pub decision: String,
    /// The pending bundle being decided: a decision about another bundle is refused.
    pub bundle_id: String,
    pub note: Option<String>,
}

/// Staff: approve (the proxy then serves that bundle once the application is released) or reject the
/// pending bundle.
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
    let bundle = id(&body.bundle_id, sr_not_found())?;
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
        .decide(
            app,
            staff.caller.user_id,
            bundle,
            approve,
            body.note.as_deref(),
        )
        .await
        .map_err(act)?;
    let sha = deps
        .service
        .bundle_detail(app, bundle)
        .await
        .ok()
        .map(|d| d.bundle.sha256);
    if let Err(e) = staff
        .svc()
        .record_action(
            &staff.caller,
            "developer.activity_decision",
            Some(("application", app)),
            serde_json::json!({"decision": body.decision, "bundle_id": body.bundle_id, "sha256": sha}),
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

/// The Activities a member can start: released ones with a reviewed bundle, and the development builds
/// they own or test.
#[utoipa::path(
    get, path = "/activities", tag = "activities",
    responses((status = 200, body = ShelfResponse), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn shelf(
    UserOnly(p): UserOnly,
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
        items: deps.service.shelf(p.user_id).await.map_err(act)?,
    }))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ChannelActivity {
    pub enabled: bool,
    pub instance: Option<InstanceView>,
}

/// The Activity running in a voice or text channel, if any.
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

/// Start an Activity in a guild voice channel (needs Connect and Use Activities there) or text channel
/// (needs View Channel and Use Activities). One Activity runs per channel; the starter is its first
/// participant. A direct message channel is refused.
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
