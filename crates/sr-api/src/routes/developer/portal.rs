//! Developer onboarding, application management and staff review.

use super::{
    AppVersionAdmin, AppVersionObject, ApplicationObject, ApplyBody, AuditEntryObject,
    CreateApplicationBody, CreatedApplication, DecideRequestBody, DecideVersionBody, DevDeps,
    DeveloperRequestAdmin, DeveloperRequestObject, DeveloperStatus, EndpointStatusObject,
    InstallObject, InteractionsKeyObject, PasswordBody, PatchApplicationBody, PatchBotBody,
    ResetSecretResult, ResetTokenResult, SubmitVersionBody, SuspendBody, app_err,
    dto::VersionApplication, invalid, parse_id,
};
use crate::{
    auth::SessionOnly,
    openapi::ErrorBody,
    routes::{
        Ext, MaybeExt,
        admin::StaffCtx,
        media::MediaDeps,
        messages::error::{ApiJson, ApiQuery, MsgError, MsgResult},
    },
};
use axum::{Json, extract::Path, http::StatusCode};
use serde::Deserialize;
use sr_apps::{
    AppsError,
    model::Application,
    service::{InstallParamsInput, UpdateInput, VersionDecision},
};
use sr_core::{ApiError, ErrorCode, PlatformPermissions};
use sr_storage::media::MediaKind;

const APP: ErrorCode = ErrorCode::UnknownApplication;

/// Responses that carry a secret are never cached.
type NoStore = [(axum::http::HeaderName, &'static str); 1];
const NO_STORE: NoStore = [(axum::http::header::CACHE_CONTROL, "no-store")];

/// Staff routes need `REVIEW_APPS` on top of a staff session.
fn require_review(staff: &StaffCtx) -> MsgResult<()> {
    if staff
        .caller
        .permissions
        .contains(PlatformPermissions::REVIEW_APPS)
    {
        Ok(())
    } else {
        Err(MsgError::Api(ApiError::with_message(
            ErrorCode::MissingPermissions,
            "Missing platform permission REVIEW_APPS",
        )))
    }
}

async fn staff_log(staff: &StaffCtx, action: &str, target: (&str, i64), detail: serde_json::Value) {
    if let Err(e) = staff
        .svc()
        .record_action(&staff.caller, action, Some(target), detail)
        .await
    {
        tracing::error!(error = %e, action, "developer decision was not written to the access log");
    }
}

/// Listing parameters of the staff queues.
#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct QueueQuery {
    /// Default `pending`.
    pub state: Option<String>,
    /// 1 to 100, default 50.
    pub limit: Option<i64>,
    /// Return rows older than this id.
    pub before: Option<String>,
}

impl QueueQuery {
    fn parts(&self) -> MsgResult<(String, i64, Option<i64>)> {
        let before = match &self.before {
            Some(b) => Some(
                parse_id(b, ErrorCode::SrNotFound)
                    .map_err(|_| invalid("before", "SNOWFLAKE_TYPE_INVALID", "Not a valid id."))?,
            ),
            None => None,
        };
        Ok((
            self.state.clone().unwrap_or_else(|| "pending".into()),
            self.limit.unwrap_or(50),
            before,
        ))
    }
}

// ------------------------------------------------------------------ onboarding

/// Ask to build apps.
#[utoipa::path(
    post, path = "/developers/apply", tag = "developer", request_body = ApplyBody,
    responses((status = 201, body = DeveloperRequestObject), (status = 400, body = ErrorBody),
        (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn apply(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
    ApiJson(body): ApiJson<ApplyBody>,
) -> MsgResult<(StatusCode, Json<DeveloperRequestObject>)> {
    let r = deps
        .apps
        .apply(
            p.user_id,
            &body.pitch,
            body.prior_work.as_deref().unwrap_or(""),
            body.accepted_terms,
        )
        .await
        .map_err(app_err)?;
    Ok((StatusCode::CREATED, Json((&r).into())))
}

/// Am I a developer, and where is my request?
#[utoipa::path(
    get, path = "/developers/status", tag = "developer",
    responses((status = 200, body = DeveloperStatus)),
    security(("session" = []))
)]
pub async fn status(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
) -> MsgResult<Json<DeveloperStatus>> {
    let (is_developer, request) = deps
        .apps
        .developer_status(p.user_id)
        .await
        .map_err(app_err)?;
    Ok(Json(DeveloperStatus {
        is_developer,
        request: request.as_ref().map(Into::into),
    }))
}

/// Staff: developer requests.
#[utoipa::path(
    get, path = "/admin/developers/requests", tag = "developer", params(QueueQuery),
    responses((status = 200, body = Vec<DeveloperRequestAdmin>), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn admin_requests(
    staff: StaffCtx,
    Ext(deps): Ext<DevDeps>,
    ApiQuery(q): ApiQuery<QueueQuery>,
) -> MsgResult<Json<Vec<DeveloperRequestAdmin>>> {
    require_review(&staff)?;
    let (state, limit, before) = q.parts()?;
    let rows = deps
        .apps
        .developer_requests(&state, limit, before)
        .await
        .map_err(app_err)?;
    Ok(Json(
        rows.iter()
            .map(|(r, a)| DeveloperRequestAdmin::new(r, a))
            .collect(),
    ))
}

/// Staff: approve or reject a developer request.
#[utoipa::path(
    post, path = "/admin/developers/requests/{request_id}/decision", tag = "developer",
    params(("request_id" = String, Path, description = "Request snowflake")),
    request_body = DecideRequestBody,
    responses((status = 200, body = DeveloperRequestObject), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn admin_decide_request(
    staff: StaffCtx,
    Ext(deps): Ext<DevDeps>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<DecideRequestBody>,
) -> MsgResult<Json<DeveloperRequestObject>> {
    require_review(&staff)?;
    let id = parse_id(&id, ErrorCode::SrNotFound)?;
    let r = deps
        .apps
        .decide_developer_request(staff.caller.user_id, id, body.approve, body.note.as_deref())
        .await
        .map_err(app_err)?;
    staff_log(
        &staff,
        "developer.request_decision",
        ("developer_request", id),
        serde_json::json!({"approved": body.approve, "account": r.account_id.to_string()}),
    )
    .await;
    Ok(Json((&r).into()))
}

// ------------------------------------------------------------------ applications

async fn render(
    deps: &DevDeps,
    apps: &[Application],
    owner_view: bool,
    staff: bool,
) -> MsgResult<Vec<ApplicationObject>> {
    let ids: Vec<i64> = apps
        .iter()
        .flat_map(|a| [a.owner_id, a.bot_user_id])
        .collect();
    let users = deps.users(&ids).await?;
    let mut out = Vec::with_capacity(apps.len());
    for a in apps {
        let mut o = ApplicationObject::new(a, &users, owner_view, staff);
        if owner_view {
            let info = deps.apps.endpoint_info(a.id).await.map_err(app_err)?;
            o.interactions_endpoint_url = info.url;
            o.verify_key = info.verify_key;
            o.interactions_endpoint_state = o
                .interactions_endpoint_url
                .is_some()
                .then(|| info.state.as_str().to_owned());
        }
        out.push(o);
    }
    Ok(out)
}

/// Create the application's interactions signing key when it has none (older applications)
/// and return the public key. Idempotent; the key is never rotated here.
#[utoipa::path(
    post, path = "/applications/{application_id}/interactions-key", tag = "developer",
    params(("application_id" = String, Path, description = "Application snowflake")),
    responses((status = 200, body = InteractionsKeyObject), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn ensure_interactions_key(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
    Path(id): Path<String>,
) -> MsgResult<Json<InteractionsKeyObject>> {
    let id = parse_id(&id, APP)?;
    let info = deps
        .apps
        .ensure_signing_key(p.user_id, id)
        .await
        .map_err(app_err)?;
    Ok(Json(InteractionsKeyObject {
        verify_key: info.verify_key.unwrap_or_default(),
    }))
}

/// Run the PING check against the saved endpoint URL and, when it passes, switch a disabled
/// endpoint back on (D-186). The failure counter restarts.
#[utoipa::path(
    post, path = "/applications/{application_id}/endpoint/verify", tag = "developer",
    params(("application_id" = String, Path, description = "Application snowflake")),
    responses((status = 200, body = EndpointStatusObject), (status = 400, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn verify_endpoint(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
    Path(id): Path<String>,
) -> MsgResult<Json<EndpointStatusObject>> {
    let id = parse_id(&id, APP)?;
    deps.apps.owned(p.user_id, id).await.map_err(app_err)?;
    let info = deps.apps.endpoint_info(id).await.map_err(app_err)?;
    let Some(url) = info.url else {
        return Err(invalid(
            "interactions_endpoint_url",
            "BASE_TYPE_REQUIRED",
            "Save an interactions endpoint URL first.",
        ));
    };
    if !deps.endpoints_available() {
        return Err(invalid(
            "interactions_endpoint_url",
            "NOT_AVAILABLE",
            "HTTP interactions endpoints are not available on this server.",
        ));
    }
    let seed = deps.apps.signing_seed(id).await.map_err(app_err)?;
    deps.verify_endpoint(id, &url, &seed).await.map_err(|m| {
        invalid(
            "interactions_endpoint_url",
            "INTERACTIONS_ENDPOINT_VERIFY",
            &m,
        )
    })?;
    let info = deps
        .apps
        .reenable_endpoint(p.user_id, id)
        .await
        .map_err(app_err)?;
    Ok(Json(EndpointStatusObject {
        interactions_endpoint_url: info.url,
        interactions_endpoint_state: info.state.as_str().to_owned(),
        verify_key: info.verify_key,
    }))
}

/// Check a new endpoint URL (format, then a signed PING round trip) and store it.
async fn apply_endpoint(deps: &DevDeps, owner: i64, id: i64, url: Option<String>) -> MsgResult<()> {
    let Some(url) = url else {
        deps.apps
            .set_endpoint(owner, id, None)
            .await
            .map_err(app_err)?;
        return Ok(());
    };
    sr_apps::AppsService::check_endpoint_url(&url).map_err(app_err)?;
    if !deps.endpoints_available() {
        return Err(invalid(
            "interactions_endpoint_url",
            "NOT_AVAILABLE",
            "HTTP interactions endpoints are not available on this server.",
        ));
    }
    deps.apps
        .ensure_signing_key(owner, id)
        .await
        .map_err(app_err)?;
    let seed = deps.apps.signing_seed(id).await.map_err(app_err)?;
    deps.verify_endpoint(id, &url, &seed).await.map_err(|m| {
        invalid(
            "interactions_endpoint_url",
            "INTERACTIONS_ENDPOINT_VERIFY",
            &m,
        )
    })?;
    deps.apps
        .set_endpoint(owner, id, Some(url))
        .await
        .map_err(app_err)?;
    Ok(())
}

/// Create an application and its bot.
#[utoipa::path(
    post, path = "/applications", tag = "developer", request_body = CreateApplicationBody,
    responses((status = 201, body = CreatedApplication), (status = 400, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create_app(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
    ApiJson(body): ApiJson<CreateApplicationBody>,
) -> MsgResult<(StatusCode, NoStore, Json<CreatedApplication>)> {
    let c = deps
        .apps
        .create_application(
            p.user_id,
            &body.name,
            body.description.as_deref().unwrap_or(""),
        )
        .await
        .map_err(app_err)?;
    let application = render(&deps, std::slice::from_ref(&c.application), true, false)
        .await?
        .remove(0);
    Ok((
        StatusCode::CREATED,
        NO_STORE,
        Json(CreatedApplication {
            application,
            client_secret: c.client_secret,
            bot_token: c.bot_token,
        }),
    ))
}

/// My applications.
#[utoipa::path(
    get, path = "/applications", tag = "developer",
    responses((status = 200, body = Vec<ApplicationObject>)),
    security(("session" = []))
)]
pub async fn list_apps(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
) -> MsgResult<Json<Vec<ApplicationObject>>> {
    let apps = deps
        .apps
        .applications_of(p.user_id)
        .await
        .map_err(app_err)?;
    Ok(Json(render(&deps, &apps, true, false).await?))
}

/// One of my applications.
#[utoipa::path(
    get, path = "/applications/{application_id}", tag = "developer",
    params(("application_id" = String, Path, description = "Application snowflake")),
    responses((status = 200, body = ApplicationObject), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get_app(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
    Path(id): Path<String>,
) -> MsgResult<Json<ApplicationObject>> {
    let id = parse_id(&id, APP)?;
    let app = deps.apps.owned(p.user_id, id).await.map_err(app_err)?;
    Ok(Json(
        render(&deps, std::slice::from_ref(&app), true, false)
            .await?
            .remove(0),
    ))
}

/// Change my application's settings.
#[utoipa::path(
    patch, path = "/applications/{application_id}", tag = "developer",
    params(("application_id" = String, Path, description = "Application snowflake")),
    request_body = PatchApplicationBody,
    responses((status = 200, body = ApplicationObject), (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn patch_app(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
    MaybeExt(media): MaybeExt<MediaDeps>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<PatchApplicationBody>,
) -> MsgResult<Json<ApplicationObject>> {
    let id = parse_id(&id, APP)?;
    let current = deps.apps.owned(p.user_id, id).await.map_err(app_err)?;
    // The endpoint is checked first: a refused URL leaves the rest of the patch unapplied.
    let only_endpoint = body.interactions_endpoint_url.is_some()
        && body.name.is_none()
        && body.description.is_none()
        && body.icon.is_none()
        && body.tags.is_none()
        && body.bot_public.is_none()
        && body.bot_require_code_grant.is_none()
        && body.redirect_uris.is_none()
        && body.install_params.is_none()
        && body.terms_of_service_url.is_none()
        && body.privacy_policy_url.is_none()
        && body.privileged_intents.is_none();
    if let Some(ep) = body.interactions_endpoint_url.clone() {
        apply_endpoint(&deps, p.user_id, id, ep).await?;
    }
    if only_endpoint {
        let app = deps.apps.owned(p.user_id, id).await.map_err(app_err)?;
        return Ok(Json(
            render(&deps, std::slice::from_ref(&app), true, false)
                .await?
                .remove(0),
        ));
    }
    let install_params: Option<InstallParamsInput> = match body.install_params {
        Some(v) => Some(serde_json::from_value(v).map_err(|_| {
            invalid(
                "install_params",
                "MODEL_TYPE_CONVERT",
                "Expected {scopes, permissions}.",
            )
        })?),
        None => None,
    };
    let mut input = UpdateInput {
        name: body.name,
        description: body.description,
        tags: body.tags,
        bot_public: body.bot_public,
        bot_require_code_grant: body.bot_require_code_grant,
        redirect_uris: body.redirect_uris,
        install_params,
        terms_of_service_url: body.terms_of_service_url,
        privacy_policy_url: body.privacy_policy_url,
        privileged_intents: body.privileged_intents,
        icon_key: None,
    };
    let mut new_icon: Option<Option<String>> = None;
    if let Some(icon) = body.icon {
        match icon {
            Some(uri) => {
                let media = media.as_ref().ok_or(ErrorCode::SrInternal)?;
                let hash = media
                    .store(
                        MediaKind::UserAvatar,
                        current.bot_user_id as u64,
                        "icon",
                        uri,
                    )
                    .await
                    .map_err(|e| MsgError::Api(e.0))?;
                new_icon = Some(Some(hash.clone()));
                input.icon_key = Some(Some(hash));
            }
            None => {
                new_icon = Some(None);
                input.icon_key = Some(None);
            }
        }
    }
    let updated = match deps.apps.update_application(p.user_id, id, input).await {
        Ok(a) => a,
        Err(e) => {
            // Do not leave an orphaned upload behind.
            if let (Some(Some(h)), Some(m)) = (&new_icon, media.as_deref()) {
                m.remove(MediaKind::UserAvatar, current.bot_user_id as u64, h)
                    .await;
            }
            return Err(app_err(e));
        }
    };
    if let (Some(_), Some(old), Some(m)) = (&new_icon, &current.icon_key, media.as_deref())
        && updated.icon_key.as_ref() != Some(old)
    {
        m.remove(MediaKind::UserAvatar, current.bot_user_id as u64, old)
            .await;
    }
    Ok(Json(
        render(&deps, std::slice::from_ref(&updated), true, false)
            .await?
            .remove(0),
    ))
}

/// Delete my application and remove its bot everywhere.
#[utoipa::path(
    delete, path = "/applications/{application_id}", tag = "developer",
    params(("application_id" = String, Path, description = "Application snowflake")),
    request_body = PasswordBody,
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_app(
    SessionOnly(p): SessionOnly,
    state: axum::extract::State<crate::state::AppState>,
    Ext(deps): Ext<DevDeps>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<PasswordBody>,
) -> MsgResult<StatusCode> {
    let id = parse_id(&id, APP)?;
    deps.apps.owned(p.user_id, id).await.map_err(app_err)?;
    deps.confirm_password(state.now_secs(), p.user_id, &body.password)
        .await?;
    let (app, guilds) = deps
        .apps
        .delete_application(p.user_id, id)
        .await
        .map_err(app_err)?;
    // The bot left every server it was in: tell the gateway.
    for g in guilds {
        deps.publish(sr_events::DomainEvent::MemberLeft {
            guild: sr_core::GuildId::new(g as u64),
            user: sr_core::UserId::new(app.bot_user_id as u64),
        })
        .await;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Rename the bot account.
#[utoipa::path(
    patch, path = "/applications/{application_id}/bot", tag = "developer",
    params(("application_id" = String, Path, description = "Application snowflake")),
    request_body = PatchBotBody,
    responses((status = 200, body = ApplicationObject), (status = 400, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn patch_bot(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<PatchBotBody>,
) -> MsgResult<Json<ApplicationObject>> {
    let id = parse_id(&id, APP)?;
    deps.apps
        .rename_bot(p.user_id, id, &body.username)
        .await
        .map_err(app_err)?;
    let app = deps.apps.owned(p.user_id, id).await.map_err(app_err)?;
    Ok(Json(
        render(&deps, std::slice::from_ref(&app), true, false)
            .await?
            .remove(0),
    ))
}

/// Replace the client secret (shown once).
#[utoipa::path(
    post, path = "/applications/{application_id}/reset-secret", tag = "developer",
    params(("application_id" = String, Path, description = "Application snowflake")),
    request_body = PasswordBody,
    responses((status = 200, body = ResetSecretResult), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn reset_secret(
    SessionOnly(p): SessionOnly,
    state: axum::extract::State<crate::state::AppState>,
    Ext(deps): Ext<DevDeps>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<PasswordBody>,
) -> MsgResult<(NoStore, Json<ResetSecretResult>)> {
    let id = parse_id(&id, APP)?;
    deps.apps.owned(p.user_id, id).await.map_err(app_err)?;
    deps.confirm_password(state.now_secs(), p.user_id, &body.password)
        .await?;
    let client_secret = deps
        .apps
        .reset_secret(p.user_id, id)
        .await
        .map_err(app_err)?;
    Ok((NO_STORE, Json(ResetSecretResult { client_secret })))
}

/// Replace the bot token (shown once).
#[utoipa::path(
    post, path = "/applications/{application_id}/bot/reset-token", tag = "developer",
    params(("application_id" = String, Path, description = "Application snowflake")),
    request_body = PasswordBody,
    responses((status = 200, body = ResetTokenResult), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn reset_token(
    SessionOnly(p): SessionOnly,
    state: axum::extract::State<crate::state::AppState>,
    Ext(deps): Ext<DevDeps>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<PasswordBody>,
) -> MsgResult<(NoStore, Json<ResetTokenResult>)> {
    let id = parse_id(&id, APP)?;
    deps.apps.owned(p.user_id, id).await.map_err(app_err)?;
    deps.confirm_password(state.now_secs(), p.user_id, &body.password)
        .await?;
    let token = deps
        .apps
        .reset_bot_token(p.user_id, id)
        .await
        .map_err(app_err)?;
    Ok((NO_STORE, Json(ResetTokenResult { token })))
}

/// Paging of the audit trail.
#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AuditQuery {
    pub limit: Option<i64>,
    pub before: Option<String>,
}

/// The application's audit trail.
#[utoipa::path(
    get, path = "/applications/{application_id}/audit-log", tag = "developer",
    params(("application_id" = String, Path, description = "Application snowflake"), AuditQuery),
    responses((status = 200, body = Vec<AuditEntryObject>)),
    security(("session" = []))
)]
pub async fn audit_log(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
    Path(id): Path<String>,
    ApiQuery(q): ApiQuery<AuditQuery>,
) -> MsgResult<Json<Vec<AuditEntryObject>>> {
    let id = parse_id(&id, APP)?;
    let before = match &q.before {
        Some(b) => Some(
            parse_id(b, ErrorCode::SrNotFound)
                .map_err(|_| invalid("before", "SNOWFLAKE_TYPE_INVALID", "Not a valid id."))?,
        ),
        None => None,
    };
    let rows = deps
        .apps
        .audit_log(p.user_id, id, q.limit.unwrap_or(50), before)
        .await
        .map_err(app_err)?;
    Ok(Json(rows.iter().map(Into::into).collect()))
}

/// Servers the application is installed in.
#[utoipa::path(
    get, path = "/applications/{application_id}/installs", tag = "developer",
    params(("application_id" = String, Path, description = "Application snowflake")),
    responses((status = 200, body = Vec<InstallObject>)),
    security(("session" = []))
)]
pub async fn installs(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
    Path(id): Path<String>,
) -> MsgResult<Json<Vec<InstallObject>>> {
    let id = parse_id(&id, APP)?;
    let rows = deps
        .apps
        .installs_of(p.user_id, id)
        .await
        .map_err(app_err)?;
    Ok(Json(rows.iter().map(Into::into).collect()))
}

// ------------------------------------------------------------------ versions

/// Release versions of an application.
#[utoipa::path(
    get, path = "/applications/{application_id}/versions", tag = "developer",
    params(("application_id" = String, Path, description = "Application snowflake")),
    responses((status = 200, body = Vec<AppVersionObject>)),
    security(("session" = []))
)]
pub async fn list_versions(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
    Path(id): Path<String>,
) -> MsgResult<Json<Vec<AppVersionObject>>> {
    let id = parse_id(&id, APP)?;
    let rows = deps
        .apps
        .versions_of(p.user_id, id)
        .await
        .map_err(app_err)?;
    Ok(Json(
        rows.into_iter()
            .map(|(v, d)| AppVersionObject::new(&v, d))
            .collect(),
    ))
}

/// Submit the current settings for release review.
#[utoipa::path(
    post, path = "/applications/{application_id}/versions", tag = "developer",
    params(("application_id" = String, Path, description = "Application snowflake")),
    request_body = SubmitVersionBody,
    responses((status = 201, body = AppVersionObject), (status = 400, body = ErrorBody),
        (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn submit_version(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<SubmitVersionBody>,
) -> MsgResult<(StatusCode, Json<AppVersionObject>)> {
    let id = parse_id(&id, APP)?;
    let note = body.note;
    let v = deps
        .apps
        .submit_version(p.user_id, id, note.as_deref())
        .await
        .map_err(app_err)?;
    let versions = deps
        .apps
        .versions_of(p.user_id, id)
        .await
        .map_err(app_err)?;
    let diff = versions
        .into_iter()
        .find(|(x, _)| x.id == v.id)
        .map(|(_, d)| d)
        .unwrap_or_default();
    Ok((StatusCode::CREATED, Json(AppVersionObject::new(&v, diff))))
}

/// Withdraw a pending version.
#[utoipa::path(
    post, path = "/applications/{application_id}/versions/{version_id}/withdraw", tag = "developer",
    params(("application_id" = String, Path, description = "Application snowflake"),
        ("version_id" = String, Path, description = "Version snowflake")),
    responses((status = 200, body = AppVersionObject), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn withdraw_version(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
    Path((id, vid)): Path<(String, String)>,
) -> MsgResult<Json<AppVersionObject>> {
    let id = parse_id(&id, APP)?;
    let vid = parse_id(&vid, ErrorCode::SrNotFound)?;
    let v = deps
        .apps
        .withdraw_version(p.user_id, id, vid)
        .await
        .map_err(app_err)?;
    Ok(Json(AppVersionObject::new(&v, vec![])))
}

/// Staff: version queue.
#[utoipa::path(
    get, path = "/admin/developers/versions", tag = "developer", params(QueueQuery),
    responses((status = 200, body = Vec<AppVersionAdmin>), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn admin_versions(
    staff: StaffCtx,
    Ext(deps): Ext<DevDeps>,
    ApiQuery(q): ApiQuery<QueueQuery>,
) -> MsgResult<Json<Vec<AppVersionAdmin>>> {
    require_review(&staff)?;
    let (state, limit, before) = q.parts()?;
    let rows = deps
        .apps
        .pending_versions(&state, limit, before)
        .await
        .map_err(app_err)?;
    Ok(Json(
        rows.into_iter()
            .map(|(v, a, d)| AppVersionAdmin {
                version: AppVersionObject::new(&v, d),
                application: VersionApplication {
                    id: a.id.to_string(),
                    name: a.name.clone(),
                    owner_id: a.owner_id.to_string(),
                },
            })
            .collect(),
    ))
}

/// Staff: approve, reject or send back a version.
#[utoipa::path(
    post, path = "/admin/developers/versions/{version_id}/decision", tag = "developer",
    params(("version_id" = String, Path, description = "Version snowflake")),
    request_body = DecideVersionBody,
    responses((status = 200, body = AppVersionObject), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn admin_decide_version(
    staff: StaffCtx,
    Ext(deps): Ext<DevDeps>,
    Path(vid): Path<String>,
    ApiJson(body): ApiJson<DecideVersionBody>,
) -> MsgResult<Json<AppVersionObject>> {
    require_review(&staff)?;
    let vid = parse_id(&vid, ErrorCode::SrNotFound)?;
    let decision = match body.decision.as_str() {
        "approve" => VersionDecision::Approve,
        "reject" => VersionDecision::Reject,
        "request_changes" => VersionDecision::RequestChanges,
        _ => {
            return Err(invalid(
                "decision",
                "BASE_TYPE_CHOICES",
                "Must be approve, reject or request_changes.",
            ));
        }
    };
    let (v, app) = deps
        .apps
        .decide_version(
            staff.caller.user_id,
            vid,
            decision,
            body.note.as_deref(),
            &body.approved_intents,
        )
        .await
        .map_err(app_err)?;
    staff_log(
        &staff,
        "developer.version_decision",
        ("application", app.id),
        serde_json::json!({"version": v.version, "decision": body.decision}),
    )
    .await;
    Ok(Json(AppVersionObject::new(&v, vec![])))
}

/// Staff: switch an application off.
#[utoipa::path(
    post, path = "/admin/developers/apps/{application_id}/suspend", tag = "developer",
    params(("application_id" = String, Path, description = "Application snowflake")),
    request_body = SuspendBody,
    responses((status = 200, body = ApplicationObject), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn admin_suspend(
    staff: StaffCtx,
    Ext(deps): Ext<DevDeps>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<SuspendBody>,
) -> MsgResult<Json<ApplicationObject>> {
    require_review(&staff)?;
    let id = parse_id(&id, APP)?;
    let reason = body.reason.unwrap_or_default();
    let app = deps
        .apps
        .suspend(staff.caller.user_id, id, &reason)
        .await
        .map_err(app_err)?;
    staff_log(
        &staff,
        "developer.suspend",
        ("application", id),
        serde_json::json!({"reason": reason}),
    )
    .await;
    Ok(Json(
        render(&deps, std::slice::from_ref(&app), true, true)
            .await?
            .remove(0),
    ))
}

/// Staff: lift a suspension.
#[utoipa::path(
    post, path = "/admin/developers/apps/{application_id}/unsuspend", tag = "developer",
    params(("application_id" = String, Path, description = "Application snowflake")),
    request_body = SuspendBody,
    responses((status = 200, body = ApplicationObject), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn admin_unsuspend(
    staff: StaffCtx,
    Ext(deps): Ext<DevDeps>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<SuspendBody>,
) -> MsgResult<Json<ApplicationObject>> {
    require_review(&staff)?;
    let id = parse_id(&id, APP)?;
    let app = deps
        .apps
        .unsuspend(staff.caller.user_id, id, body.reason.as_deref())
        .await
        .map_err(app_err)?;
    staff_log(
        &staff,
        "developer.unsuspend",
        ("application", id),
        serde_json::json!({"reason": body.reason}),
    )
    .await;
    Ok(Json(
        render(&deps, std::slice::from_ref(&app), true, true)
            .await?
            .remove(0),
    ))
}

#[allow(dead_code)]
fn _unused(_: AppsError) {}
