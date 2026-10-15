//! Platform settings, feature toggles, system status, the job queue and the access log.

use super::ctx::{ReasonBody, StaffCtx, id, map_err, reason};
use crate::{error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sr_admin::{
    AccessKind, AccessLogFilter, AccessLogPage, ChainReport, JobPage, SettingView, SystemCounts,
};
use sr_core::{ApiError, ErrorCode, PlatformPermissions as P};
use sr_features::{FeatureError, ScopeRef};
use utoipa::ToSchema;

// ---- platform settings ---------------------------------------------------

/// Every editable platform setting with its current value (`MANAGE_FEATURES`).
#[utoipa::path(
    get, path = "/admin/settings",
    responses((status = 200, body = Vec<SettingView>), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn settings(ctx: StaffCtx) -> ApiResult<Json<Vec<SettingView>>> {
    ctx.svc()
        .settings_view(&ctx.caller)
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PatchSettingsBody {
    /// Setting key to new value; unknown keys or out-of-range values reject the whole patch.
    #[schema(value_type = Object)]
    pub values: Map<String, Value>,
    pub reason: Option<String>,
}

/// Change platform settings and limits without a restart (`MANAGE_FEATURES`).
#[utoipa::path(
    patch, path = "/admin/settings", request_body = PatchSettingsBody,
    responses((status = 200, body = Vec<SettingView>), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn patch_settings(
    ctx: StaffCtx,
    headers: HeaderMap,
    Json(b): Json<PatchSettingsBody>,
) -> ApiResult<Json<Vec<SettingView>>> {
    let r = reason(&headers, b.reason.as_deref());
    let values: Vec<(String, Value)> = b.values.into_iter().collect();
    ctx.svc()
        .patch_settings(&ctx.caller, &values, r.as_deref())
        .await
        .map(Json)
        .map_err(map_err)
}

// ---- feature toggles -----------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
pub struct FeatureView {
    pub id: String,
    pub name: String,
    pub description: String,
    pub section: String,
    pub phase: u8,
    pub default_enabled: bool,
    /// Whether the feature is on at the platform scope (a platform-off is a kill switch).
    pub enabled: bool,
    pub scopes: Vec<String>,
    /// Effective settings at the platform scope.
    #[schema(value_type = Object)]
    pub settings: Value,
    /// Settings schema for auto-rendered forms.
    #[schema(value_type = Object)]
    pub schema: Value,
}

fn feature_err(e: FeatureError) -> crate::HttpError {
    match e {
        FeatureError::UnknownFeature(f) => {
            ApiError::with_message(ErrorCode::SrNotFound, format!("Unknown feature {f}")).into()
        }
        FeatureError::Store(e) => {
            tracing::error!(error = %e, "feature store failure");
            ErrorCode::SrInternal.into()
        }
        other => ApiError::with_message(ErrorCode::InvalidFormBody, other.to_string()).into(),
    }
}

async fn feature_view(
    svc: &sr_features::FeatureService,
    f: &std::sync::Arc<dyn sr_features::Feature>,
) -> Result<FeatureView, FeatureError> {
    let meta = f.meta();
    let id = f.id();
    Ok(FeatureView {
        id: id.to_owned(),
        name: meta.name,
        description: meta.description,
        section: meta.section,
        phase: meta.phase,
        default_enabled: meta.default_enabled,
        enabled: svc.is_enabled(id, &ScopeRef::Platform).await?,
        scopes: f
            .scopes()
            .iter()
            .map(|s| json!(s).as_str().unwrap_or_default().to_owned())
            .collect(),
        settings: Value::Object(
            svc.effective_settings(id, &ScopeRef::Platform)
                .await
                .unwrap_or_default(),
        ),
        schema: serde_json::to_value(f.settings_schema()).unwrap_or(Value::Null),
    })
}

/// List the registered feature modules with their platform state (`MANAGE_FEATURES`).
/// The registry may be sparse: only registered features appear.
#[utoipa::path(
    get, path = "/admin/features",
    responses((status = 200, body = Vec<FeatureView>), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn features(ctx: StaffCtx) -> ApiResult<Json<Vec<FeatureView>>> {
    ctx.svc()
        .authorize(&ctx.caller, P::MANAGE_FEATURES, "features.list")
        .await
        .map_err(map_err)?;
    let mut out = Vec::new();
    if let Some(svc) = &ctx.deps.features {
        for f in svc.registry().iter() {
            out.push(feature_view(svc, f).await.map_err(feature_err)?);
        }
    }
    Ok(Json(out))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PatchFeatureBody {
    /// Turn the feature on or off at the platform scope.
    pub enabled: Option<bool>,
    /// Drop the explicit platform flag so the feature default applies again.
    #[serde(default)]
    pub reset_enabled: bool,
    /// Replace the platform-scope setting overrides (validated against the feature schema).
    #[schema(value_type = Option<Object>)]
    pub settings: Option<Map<String, Value>>,
    pub reason: Option<String>,
}

/// Enable, disable or configure a feature at the platform scope (`MANAGE_FEATURES`).
#[utoipa::path(
    patch, path = "/admin/features/{feature_id}", request_body = PatchFeatureBody,
    params(("feature_id" = String, Path, description = "Feature id, e.g. lounge.slowmode_plus")),
    responses((status = 200, body = FeatureView), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn patch_feature(
    ctx: StaffCtx,
    Path(feature_id): Path<String>,
    headers: HeaderMap,
    Json(b): Json<PatchFeatureBody>,
) -> ApiResult<Json<FeatureView>> {
    ctx.svc()
        .authorize(&ctx.caller, P::MANAGE_FEATURES, "features.patch")
        .await
        .map_err(map_err)?;
    let reason = reason(&headers, b.reason.as_deref()).unwrap_or_default();
    let Some(svc) = &ctx.deps.features else {
        return Err(feature_err(FeatureError::UnknownFeature(feature_id)));
    };
    let feature = svc
        .registry()
        .get(&feature_id)
        .cloned()
        .ok_or_else(|| feature_err(FeatureError::UnknownFeature(feature_id.clone())))?;
    if let Some(e) = b.enabled {
        svc.set_enabled(&feature_id, &ScopeRef::Platform, Some(e))
            .await
            .map_err(feature_err)?;
    } else if b.reset_enabled {
        svc.set_enabled(&feature_id, &ScopeRef::Platform, None)
            .await
            .map_err(feature_err)?;
    }
    if let Some(values) = &b.settings {
        svc.set_settings(&feature_id, &ScopeRef::Platform, values.clone())
            .await
            .map_err(feature_err)?;
    }
    ctx.svc()
        .record(
            &ctx.caller,
            "features.patch",
            None,
            &reason,
            json!({
                "feature": feature_id,
                "enabled": b.enabled,
                "reset_enabled": b.reset_enabled,
                "settings_changed": b.settings.is_some(),
            }),
        )
        .await
        .map_err(map_err)?;
    Ok(Json(
        feature_view(svc, &feature).await.map_err(feature_err)?,
    ))
}

// ---- status --------------------------------------------------------------

/// Dashboard counts (`STAFF_BASIC`).
#[utoipa::path(
    get, path = "/admin/stats",
    responses((status = 200, body = StatsView), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn stats(ctx: StaffCtx) -> ApiResult<Json<StatsView>> {
    let counts = ctx.svc().stats(&ctx.caller).await.map_err(map_err)?;
    let voice = ctx.deps.voice_stats.as_ref().map(|f| f());
    Ok(Json(StatsView {
        counts,
        voice_rooms: voice.map(|v| v.0),
        egress_bytes: voice.map(|v| v.1),
    }))
}

/// The dashboard counts, plus the voice figures when the SFU runs in this process.
#[derive(Debug, Serialize, ToSchema)]
pub struct StatsView {
    #[serde(flatten)]
    pub counts: SystemCounts,
    /// Voice rooms with at least one participant (absent without an embedded SFU).
    pub voice_rooms: Option<u64>,
    /// Bytes the SFU has sent since it started (absent without an embedded SFU).
    pub egress_bytes: Option<u64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ProbeView {
    pub name: String,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SystemView {
    pub version: String,
    pub counts: SystemCounts,
    /// Readiness probes (database, object storage).
    pub probes: Vec<ProbeView>,
    /// Open gateway sessions, when the gateway is attached.
    pub gateway_sessions: Option<usize>,
}

/// System status: version, readiness probes, gateway sessions, queue depth (`OPERATOR`).
#[utoipa::path(
    get, path = "/admin/system",
    responses((status = 200, body = SystemView), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn system(State(state): State<AppState>, ctx: StaffCtx) -> ApiResult<Json<SystemView>> {
    let counts = ctx
        .svc()
        .system_counts(&ctx.caller)
        .await
        .map_err(map_err)?;
    let mut probes = Vec::new();
    for c in state.readiness() {
        let r = c.check().await;
        probes.push(ProbeView {
            name: c.name().to_owned(),
            ok: r.is_ok(),
            error: r.err(),
        });
    }
    Ok(Json(SystemView {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        counts,
        probes,
        gateway_sessions: ctx.deps.gateway_sessions.as_ref().map(|f| f()),
    }))
}

// ---- job queue -----------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
pub struct JobParams {
    /// `queued`, `running`, `succeeded` or `dead`.
    pub state: Option<String>,
    pub after: Option<String>,
    pub limit: Option<i64>,
}

/// List jobs, newest first (`MANAGE_TRANSCODE`).
#[utoipa::path(
    get, path = "/admin/jobs",
    params(
        ("state" = Option<String>, Query, description = "queued, running, succeeded, dead"),
        ("after" = Option<String>, Query, description = "Cursor"),
        ("limit" = Option<i64>, Query, description = "1-100, default 50"),
    ),
    responses((status = 200, body = JobPage), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn jobs(ctx: StaffCtx, Query(p): Query<JobParams>) -> ApiResult<Json<JobPage>> {
    ctx.svc()
        .list_jobs(
            &ctx.caller,
            p.state.as_deref(),
            p.after.as_deref().map(id).transpose()?,
            p.limit,
        )
        .await
        .map(Json)
        .map_err(map_err)
}

/// Re-queue a dead job (`MANAGE_TRANSCODE`).
#[utoipa::path(
    post, path = "/admin/jobs/{job_id}/retry",
    params(("job_id" = String, Path, description = "Job snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn retry_job(
    ctx: StaffCtx,
    Path(job_id): Path<String>,
    headers: HeaderMap,
    body: Option<Json<ReasonBody>>,
) -> ApiResult<StatusCode> {
    let r = reason(&headers, body.and_then(|Json(b)| b.reason).as_deref());
    ctx.svc()
        .retry_job(&ctx.caller, id(&job_id)?, r.as_deref())
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Delete a dead job (`MANAGE_TRANSCODE`).
#[utoipa::path(
    delete, path = "/admin/jobs/{job_id}",
    params(("job_id" = String, Path, description = "Job snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_job(
    ctx: StaffCtx,
    Path(job_id): Path<String>,
    headers: HeaderMap,
    body: Option<Json<ReasonBody>>,
) -> ApiResult<StatusCode> {
    let r = reason(&headers, body.and_then(|Json(b)| b.reason).as_deref());
    ctx.svc()
        .delete_job(&ctx.caller, id(&job_id)?, r.as_deref())
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- access log ----------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
pub struct LogParams {
    pub actor_id: Option<String>,
    pub target_id: Option<String>,
    pub action: Option<String>,
    /// `admin_action`, `read`, `private_read` or `denied`.
    pub kind: Option<String>,
    /// Cursor: the `next_before` of the previous page.
    pub before: Option<i64>,
    pub limit: Option<i64>,
}

/// Page through the staff access log, newest first (`VIEW_ACCESS_LOG`).
#[utoipa::path(
    get, path = "/admin/access-log",
    params(
        ("actor_id" = Option<String>, Query, description = "Only this actor"),
        ("target_id" = Option<String>, Query, description = "Only this target"),
        ("action" = Option<String>, Query, description = "Exact action, e.g. account.suspend"),
        ("kind" = Option<String>, Query, description = "admin_action, read, private_read, denied"),
        ("before" = Option<i64>, Query, description = "Cursor"),
        ("limit" = Option<i64>, Query, description = "1-100, default 50"),
    ),
    responses((status = 200, body = AccessLogPage), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn access_log(
    ctx: StaffCtx,
    Query(p): Query<LogParams>,
) -> ApiResult<Json<AccessLogPage>> {
    let kind = match p.kind.as_deref() {
        None => None,
        Some(k) => Some(AccessKind::parse(k).ok_or_else(|| {
            ApiError::with_message(ErrorCode::InvalidFormBody, "Unknown access-log kind")
        })?),
    };
    let f = AccessLogFilter {
        actor_id: p.actor_id.as_deref().map(id).transpose()?,
        target_id: p.target_id.as_deref().map(id).transpose()?,
        action: p.action,
        kind,
        before: p.before,
        limit: 0,
    };
    ctx.svc()
        .access_log(&ctx.caller, f, p.limit)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Verify the whole hash chain (`VIEW_ACCESS_LOG`). A break raises a staff alert.
#[utoipa::path(
    get, path = "/admin/access-log/verify",
    responses((status = 200, body = ChainReport), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn verify_access_log(ctx: StaffCtx) -> ApiResult<Json<ChainReport>> {
    ctx.svc()
        .verify_access_log(&ctx.caller)
        .await
        .map(Json)
        .map_err(map_err)
}
