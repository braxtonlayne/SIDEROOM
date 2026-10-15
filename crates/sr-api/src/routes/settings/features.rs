//! Per-server feature modules (docs/inventory/09-feature-modules.md): list the modules a
//! server can use, switch them on and configure them, plus the temporary-role grants.
//!
//! * `GET   /guilds/{id}/sr/features`: every module with a guild scope, with this
//!   server's flag, effective settings and the settings schema the web app renders.
//! * `PATCH /guilds/{id}/sr/features/{feature_id}`: enable, disable or configure one.
//! * `GET|POST /guilds/{id}/sr/temp-roles`, `DELETE .../{user_id}/{role_id}`: temporary
//!   role grants (module `moderation.temp_roles`).
//!
//! Permissions: features need `MANAGE_GUILD`; temporary roles need `MANAGE_ROLES` and
//! follow the role hierarchy exactly like a manual role change.
//!
//! A module acts with the server owner's authority (see `sr-server`'s
//! `feature_actions`), so what a manager may configure is limited to what they could
//! do by hand: every channel and role a setting names must belong to this server, and
//! every role it names must be one the manager is allowed to assign.

use super::deps::{SettingsDeps, actor, ce, id, invalid};
use crate::routes::Ext;
use crate::routes::guilds::backend::GuildBackend;
use crate::routes::messages::error::ApiJson;
use crate::{HttpError, Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use async_trait::async_trait;
use axum::{
    Json, Router,
    extract::Path,
    http::StatusCode,
    routing::{delete, get},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sr_chat::ChatResult;
use sr_chat::perms::GuildActor;
use sr_core::permissions::Permissions;
use sr_core::{ApiError, ErrorCode, GuildId, RoleId, UserId};
use sr_features::modules::temp_roles::{GrantError, TEMP_ROLES_ID, TempRoles};
use sr_features::{Feature, FeatureCtx, FeatureError, Scope, ScopeRef};
use std::sync::Arc;
use utoipa::ToSchema;

/// Changes a member's roles as the acting member, so permission, role hierarchy,
/// gateway events and the audit log all apply.
#[async_trait]
pub trait RoleAssigner: Send + Sync {
    async fn assign(
        &self,
        actor: UserId,
        guild: GuildId,
        user: UserId,
        role: RoleId,
        add: bool,
    ) -> ChatResult<()>;
}

/// The production assigner: the guild backend.
pub struct BackendRoles(pub Arc<dyn GuildBackend>);

#[async_trait]
impl RoleAssigner for BackendRoles {
    async fn assign(
        &self,
        actor: UserId,
        guild: GuildId,
        user: UserId,
        role: RoleId,
        add: bool,
    ) -> ChatResult<()> {
        self.0.assign_role(actor, guild, user, role, add).await
    }
}

/// Collaborators of these routes, registered on the composition root.
pub struct GuildFeatures {
    pub ctx: FeatureCtx,
    pub roles: Arc<dyn RoleAssigner>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/guilds/{guild_id}/sr/features", get(list_features))
        .route(
            "/guilds/{guild_id}/sr/features/{feature_id}",
            axum::routing::patch(patch_feature),
        )
        .route(
            "/guilds/{guild_id}/sr/temp-roles",
            get(list_temp_roles).post(grant_temp_role),
        )
        .route(
            "/guilds/{guild_id}/sr/temp-roles/{user_id}/{role_id}",
            delete(revoke_temp_role),
        )
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GuildFeatureObject {
    pub id: String,
    pub name: String,
    pub description: String,
    pub section: String,
    pub phase: u8,
    /// Whether the module is on in this server.
    pub enabled: bool,
    /// Whether it starts on in a server that never chose.
    pub default_enabled: bool,
    /// The platform operator switched it off everywhere; the server's choice is kept
    /// but has no effect.
    pub platform_disabled: bool,
    /// Effective settings in this server (schema defaults plus the server's overrides).
    #[schema(value_type = Object)]
    pub settings: Value,
    /// Settings schema for the auto-rendered form.
    #[schema(value_type = Object)]
    pub schema: Value,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PatchGuildFeatureBody {
    /// Turn the module on or off in this server.
    #[serde(default)]
    pub enabled: Option<bool>,
    /// Replace this server's setting overrides (validated against the schema).
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub settings: Option<Map<String, Value>>,
}

fn feature_err(e: FeatureError) -> HttpError {
    match e {
        FeatureError::UnknownFeature(f) => {
            ApiError::with_message(ErrorCode::SrNotFound, format!("Unknown feature {f}")).into()
        }
        FeatureError::UnsupportedScope { feature, .. } => ApiError::with_message(
            ErrorCode::SrNotFound,
            format!("{feature} cannot be configured per server"),
        )
        .into(),
        FeatureError::Validation(v) => invalid(&v.path, "SETTING_INVALID", &v.reason),
        other => {
            tracing::error!(error = %other, "feature failure");
            ErrorCode::SrInternal.into()
        }
    }
}

async fn view(
    ctx: &FeatureCtx,
    f: &Arc<dyn Feature>,
    guild: GuildId,
) -> Result<GuildFeatureObject, FeatureError> {
    let meta = f.meta();
    let fid = f.id();
    let scope = ScopeRef::Guild { guild };
    Ok(GuildFeatureObject {
        id: fid.to_owned(),
        name: meta.name,
        description: meta.description,
        section: meta.section,
        phase: meta.phase,
        enabled: ctx.settings.is_enabled(fid, &scope).await?,
        default_enabled: meta.default_enabled,
        platform_disabled: ctx.settings.platform_killed(fid).await?,
        settings: Value::Object(ctx.settings.effective_settings(fid, &scope).await?),
        schema: serde_json::to_value(f.settings_schema()).unwrap_or(Value::Null),
    })
}

/// The server's feature modules (`MANAGE_GUILD`).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/sr/features",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = Vec<GuildFeatureObject>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_features(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Ext(gf): Ext<GuildFeatures>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<Vec<GuildFeatureObject>>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    if !acc.perms.has(Permissions::MANAGE_GUILD) {
        return Err(ErrorCode::MissingPermissions.into());
    }
    let mut out = Vec::new();
    for f in gf.ctx.settings.registry().iter() {
        if !f.scopes().contains(&Scope::Guild) {
            continue;
        }
        out.push(view(&gf.ctx, f, gid).await.map_err(feature_err)?);
    }
    Ok(Json(out))
}

/// Enable, disable or configure one module in this server (`MANAGE_GUILD`).
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/sr/features/{feature_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("feature_id" = String, Path, description = "Feature id, e.g. lounge.starboard"),
    ),
    request_body = PatchGuildFeatureBody,
    responses((status = 200, body = GuildFeatureObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn patch_feature(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Ext(gf): Ext<GuildFeatures>,
    Path((guild_id, feature_id)): Path<(String, String)>,
    ApiJson(body): ApiJson<PatchGuildFeatureBody>,
) -> ApiResult<Json<GuildFeatureObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    if !acc.perms.has(Permissions::MANAGE_GUILD) {
        return Err(ErrorCode::MissingPermissions.into());
    }
    let feature = gf
        .ctx
        .settings
        .registry()
        .get(&feature_id)
        .cloned()
        .ok_or_else(|| feature_err(FeatureError::UnknownFeature(feature_id.clone())))?;
    if !feature.scopes().contains(&Scope::Guild) {
        return Err(feature_err(FeatureError::UnsupportedScope {
            feature: feature_id,
            scope: Scope::Guild,
        }));
    }
    let scope = ScopeRef::Guild { guild: gid };

    if let Some(values) = &body.settings {
        // Shape first (so the error names the bad field), then ownership of every id.
        let schema = feature.settings_schema();
        schema.validate(values).map_err(|e| feature_err(e.into()))?;
        let picked = schema.picked_ids(values);
        for ch in picked.channels {
            let channel = deps
                .chat
                .get_channel(sr_core::ChannelId::new(ch))
                .await
                .map_err(|e| ce(e.into()))?;
            if channel.and_then(|c| c.guild_id) != Some(gid) {
                return Err(invalid(
                    "settings",
                    "CHANNEL_NOT_IN_GUILD",
                    "A channel in the settings does not belong to this server.",
                ));
            }
        }
        let actor_perms = GuildActor::new(&acc.guild, &acc.member, &acc.roles);
        for r in picked.roles {
            let Some(role) = acc.roles.iter().find(|x| x.id == RoleId::new(r)) else {
                return Err(invalid(
                    "settings",
                    "ROLE_NOT_IN_GUILD",
                    "A role in the settings does not belong to this server.",
                ));
            };
            // Modules grant these roles with the owner's authority, so a manager may
            // only name roles they could assign by hand.
            sr_chat::role_rules::check_assign_role(&actor_perms, role).map_err(ce)?;
        }
        gf.ctx
            .settings
            .set_settings(&feature.id().to_owned(), &scope, values.clone())
            .await
            .map_err(feature_err)?;
    }
    if let Some(enabled) = body.enabled {
        gf.ctx
            .settings
            .set_enabled(feature.id(), &scope, Some(enabled))
            .await
            .map_err(feature_err)?;
    }
    Ok(Json(
        view(&gf.ctx, &feature, gid).await.map_err(feature_err)?,
    ))
}

// ---- temporary roles ------------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
pub struct TempRoleObject {
    pub user_id: String,
    pub role_id: String,
    /// Unix milliseconds at which the role is taken away.
    pub expires_at: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct GrantTempRoleBody {
    pub user_id: String,
    pub role_id: String,
    /// How long the member keeps the role, in seconds.
    pub duration_seconds: u64,
}

async fn require_temp_roles(
    gf: &GuildFeatures,
    deps: &SettingsDeps,
    user: UserId,
    gid: GuildId,
) -> ApiResult<super::deps::GuildAccess> {
    let acc = deps.guild_access(user, gid).await?;
    if !acc.perms.has(Permissions::MANAGE_ROLES) {
        return Err(ErrorCode::MissingPermissions.into());
    }
    let on = gf
        .ctx
        .settings
        .is_enabled(TEMP_ROLES_ID, &ScopeRef::Guild { guild: gid })
        .await
        .map_err(feature_err)?;
    if !on {
        return Err(ApiError::with_message(
            ErrorCode::SrFeatureDisabled,
            "Temporary roles are not turned on for this server.",
        )
        .into());
    }
    Ok(acc)
}

/// Active temporary role grants (`MANAGE_ROLES`).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/sr/temp-roles",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = Vec<TempRoleObject>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_temp_roles(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Ext(gf): Ext<GuildFeatures>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<Vec<TempRoleObject>>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    require_temp_roles(&gf, &deps, user, gid).await?;
    let mut grants = TempRoles::list(&gf.ctx, gid).await.map_err(feature_err)?;
    grants.sort_by_key(|(u, r, at)| (*at, u.get(), *r));
    Ok(Json(
        grants
            .into_iter()
            .map(|(u, r, at)| TempRoleObject {
                user_id: u.to_string(),
                role_id: r.to_string(),
                expires_at: at,
            })
            .collect(),
    ))
}

/// Give a member a role for a limited time (`MANAGE_ROLES`, role hierarchy applies). A
/// second grant of the same role replaces the expiry.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/sr/temp-roles",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = GrantTempRoleBody,
    responses((status = 201, body = TempRoleObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn grant_temp_role(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Ext(gf): Ext<GuildFeatures>,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<GrantTempRoleBody>,
) -> ApiResult<(StatusCode, Json<TempRoleObject>)> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    require_temp_roles(&gf, &deps, user, gid).await?;
    let target: UserId = id("user_id", &body.user_id)?;
    let role: RoleId = id("role_id", &body.role_id)?;
    match TempRoles::check(&gf.ctx, gid, role.get(), body.duration_seconds)
        .await
        .map_err(feature_err)?
    {
        Ok(()) => {}
        Err(GrantError::RoleNotAllowed) => {
            return Err(invalid(
                "role_id",
                "ROLE_NOT_ALLOWED",
                "This role cannot be given temporarily here.",
            ));
        }
        Err(e @ GrantError::BadDuration { .. }) => {
            return Err(invalid(
                "duration_seconds",
                "NUMBER_TYPE_MAX",
                &e.to_string(),
            ));
        }
    }
    // As the acting member: permission, hierarchy, events and the audit log all apply.
    gf.roles
        .assign(user, gid, target, role, true)
        .await
        .map_err(ce)?;
    let expires_at = TempRoles::schedule(&gf.ctx, gid, target, role.get(), body.duration_seconds)
        .await
        .map_err(feature_err)?;
    Ok((
        StatusCode::CREATED,
        Json(TempRoleObject {
            user_id: target.to_string(),
            role_id: role.to_string(),
            expires_at,
        }),
    ))
}

/// End a temporary grant now and take the role away (`MANAGE_ROLES`).
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/sr/temp-roles/{user_id}/{role_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("user_id" = String, Path, description = "User snowflake"),
        ("role_id" = String, Path, description = "Role snowflake"),
    ),
    responses((status = 204, description = "Revoked"), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn revoke_temp_role(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Ext(gf): Ext<GuildFeatures>,
    Path((guild_id, user_id, role_id)): Path<(String, String, String)>,
) -> ApiResult<StatusCode> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    require_temp_roles(&gf, &deps, user, gid).await?;
    let target: UserId = id("user_id", &user_id)?;
    let role: RoleId = id("role_id", &role_id)?;
    let grants = TempRoles::list(&gf.ctx, gid).await.map_err(feature_err)?;
    if !grants
        .iter()
        .any(|(u, r, _)| *u == target && *r == role.get())
    {
        return Err(ErrorCode::SrNotFound.into());
    }
    gf.roles
        .assign(user, gid, target, role, false)
        .await
        .map_err(ce)?;
    TempRoles::forget(&gf.ctx, gid, target, role.get())
        .await
        .map_err(feature_err)?;
    Ok(StatusCode::NO_CONTENT)
}
