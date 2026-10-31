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
use crate::routes::guilds::safety::MfaGate;
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
use sr_features::modules::slowmode_plus::{MAX_APPLY_MINUTES, SLOWMODE_ID, SlowmodePlus};
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
        .route("/sr/features/flags", get(client_flags))
        .route("/sr/appearance/defaults", get(appearance_defaults))
        .route("/sr/compliance/public", get(compliance_public))
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
        .route(
            "/guilds/{guild_id}/sr/slowmode/presets",
            get(slowmode_presets),
        )
        .route(
            "/guilds/{guild_id}/sr/slowmode/apply",
            axum::routing::post(apply_slowmode),
        )
}

/// Platform flags a client may read (D-216).
#[derive(Debug, Serialize, ToSchema)]
pub struct ClientFlagsObject {
    /// Feature id to on/off, for platform-scope modules that declare themselves client-visible.
    pub flags: std::collections::BTreeMap<String, bool>,
}

/// The platform flags clients read at boot, e.g. `moderation.reports` (any signed-in human; bots
/// are refused).
#[utoipa::path(
    get,
    path = "/sr/features/flags",
    responses((status = 200, body = ClientFlagsObject), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn client_flags(
    _p: crate::auth::UserOnly,
    Ext(gf): Ext<GuildFeatures>,
) -> ApiResult<Json<ClientFlagsObject>> {
    let flags = gf.ctx.settings.client_flags().await.map_err(feature_err)?;
    Ok(Json(ClientFlagsObject { flags }))
}

/// The platform's default themes (ADM-PLT-004).
#[derive(Debug, Serialize, ToSchema)]
pub struct AppearanceDefaultsObject {
    /// Section (`lounge`, `entrance`, `cinema`, `library`, `support`, `tv`) to shipped theme id. A
    /// section without an entry uses the shipped default. A member's own choice wins over these.
    pub default_theme_by_section: std::collections::BTreeMap<String, String>,
}

/// The default theme per section the administrators chose (public: the entrance page is themed
/// before sign-in). Theme ids are not secret.
#[utoipa::path(
    get,
    path = "/sr/appearance/defaults",
    responses((status = 200, body = AppearanceDefaultsObject))
)]
pub async fn appearance_defaults(
    Ext(gf): Ext<GuildFeatures>,
) -> ApiResult<Json<AppearanceDefaultsObject>> {
    let on = gf
        .ctx
        .settings
        .is_enabled(sr_features::APPEARANCE_ID, &ScopeRef::Platform)
        .await
        .unwrap_or(false);
    let values = if on {
        gf.ctx
            .settings
            .effective_settings(sr_features::APPEARANCE_ID, &ScopeRef::Platform)
            .await
            .unwrap_or_default()
    } else {
        Map::new()
    };
    Ok(Json(AppearanceDefaultsObject {
        default_theme_by_section: sr_features::default_themes_from(&values),
    }))
}

/// The compliance settings the server uses, with defaults when none were saved.
pub async fn compliance_settings(gf: &GuildFeatures) -> sr_features::ComplianceSettings {
    let values = gf
        .ctx
        .settings
        .effective_settings(sr_features::COMPLIANCE_ID, &ScopeRef::Platform)
        .await
        .unwrap_or_default();
    sr_features::ComplianceSettings::from_values(&values)
}

/// What visitors may know about the instance's compliance settings (M-06, D-293 to D-295).
#[derive(Debug, Serialize, ToSchema)]
pub struct PublicComplianceObject {
    /// The age a person must be to register (13 unless the operator raised it).
    pub minimum_age: u8,
    /// Years invoices stay linked to a member after the year of payment (D-325).
    pub invoice_retention_years: u8,
    /// The operator's country (ISO 3166-1 alpha-2), when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator_country: Option<String>,
    /// The operator's contact address, also the copyright notice address, when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator_contact: Option<String>,
    /// Invite credits a new account is granted (`invites.default_credits`).
    pub invite_initial_credits: i32,
    /// Lock the Doors is on (`invites.locked`, OPS-INV-001): the entrance says the doors are closed.
    pub doors_closed: bool,
}

/// The minimum registration age and the operator's country and contact (public: the entrance
/// pages and the privacy page are read before sign-in).
#[utoipa::path(
    get,
    path = "/sr/compliance/public",
    responses((status = 200, body = PublicComplianceObject))
)]
pub async fn compliance_public(
    Ext(gf): Ext<GuildFeatures>,
    crate::routes::MaybeExt(auth): crate::routes::MaybeExt<crate::routes::auth::AuthService>,
) -> ApiResult<Json<PublicComplianceObject>> {
    let s = compliance_settings(&gf).await;
    // The same number registration grants (the default, 5, when no auth service is wired), and the lock.
    let (invite_initial_credits, doors_closed) = match auth {
        Some(a) => {
            let controls = a.invite_controls().await;
            (
                controls
                    .default_credits
                    .unwrap_or_else(|| a.initial_credits()),
                controls.locked,
            )
        }
        None => (5, false),
    };
    Ok(Json(PublicComplianceObject {
        minimum_age: s.minimum_age,
        invoice_retention_years: s.invoice_retention_years,
        operator_country: s.operator_country,
        operator_contact: s.operator_contact,
        invite_initial_credits,
        doors_closed,
    }))
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
    /// Only the server owner may switch this module on or off (the Discord bridge, D-381). A manager sees it
    /// greyed.
    pub owner_only: bool,
    /// Present on the answer to switching the Discord bridge off: what the teardown did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub teardown: Option<FeatureTeardown>,
    /// Effective settings in this server (schema defaults plus the server's overrides).
    #[schema(value_type = Object)]
    pub settings: Value,
    /// Settings schema for the auto-rendered form.
    #[schema(value_type = Object)]
    pub schema: Value,
}

/// What switching the Discord bridge off did.
#[derive(Debug, Serialize, ToSchema)]
pub struct FeatureTeardown {
    /// Channel maps removed.
    pub maps_removed: i64,
    /// Webhooks Discord would not delete after three attempts: the owner removes them by hand.
    pub failed_webhooks: Vec<String>,
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

/// Validate `values` against the part of the schema that belongs to `scope`, then check
/// that every channel and role they name is this server's, and that every role is one the
/// acting manager could assign by hand (modules act with the owner's authority).
pub(super) async fn check_settings(
    deps: &SettingsDeps,
    acc: &super::deps::GuildAccess,
    gid: GuildId,
    feature: &Arc<dyn Feature>,
    scope: Scope,
    values: &Map<String, Value>,
) -> ApiResult<()> {
    // Shape first (so the error names the bad field), then ownership of every id.
    let schema = feature.settings_schema();
    schema
        .validate_at(scope, values)
        .map_err(|e| feature_err(e.into()))?;
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
        // Modules grant these roles with the owner's authority, so a manager may only name
        // roles they could assign by hand.
        sr_chat::role_rules::check_assign_role(&actor_perms, role).map_err(ce)?;
    }
    Ok(())
}
pub(super) fn feature_err(e: FeatureError) -> HttpError {
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
        owner_only: f.owner_only(),
        teardown: None,
        settings: Value::Object(ctx.settings.effective_settings(fid, &scope).await?),
        schema: serde_json::to_value(f.settings_schema().for_scope(Scope::Guild))
            .unwrap_or(Value::Null),
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
        // An operator-gated module (the scrapbook) is hidden until the platform opted in.
        if !gf.ctx.settings.offered(f.id()).await.map_err(feature_err)? {
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
    gate: MfaGate,
    Ext(deps): Ext<SettingsDeps>,
    Ext(gf): Ext<GuildFeatures>,
    crate::routes::MaybeExt(bridge): crate::routes::MaybeExt<sr_bridge::BridgeService>,
    Path((guild_id, feature_id)): Path<(String, String)>,
    ApiJson(body): ApiJson<PatchGuildFeatureBody>,
) -> ApiResult<Json<GuildFeatureObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    acc.require_mfa(&gate, user, Permissions::MANAGE_GUILD)
        .await?;
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
    // Hidden means unknown: a module the operator has not offered cannot be configured either.
    if !gf
        .ctx
        .settings
        .offered(feature.id())
        .await
        .map_err(feature_err)?
    {
        return Err(feature_err(FeatureError::UnknownFeature(
            feature.id().to_owned(),
        )));
    }
    // BRG-COR-001 (D-381): a module that declares itself owner-only is switched, configured and removed by the
    // server owner alone; `MANAGE_GUILD` and `ADMINISTRATOR` are 403, a bot is refused, and the server's second
    // factor step applies. This is the one declared property the toggle handler reads, not a second path.
    if feature.owner_only() {
        if !matches!(p.kind, crate::AuthKind::Session { .. }) {
            return Err(ErrorCode::BotsCannotUseThisEndpoint.into());
        }
        if acc.guild.owner_id != user {
            return Err(ErrorCode::MissingPermissions.into());
        }
        gate.check_level(user, acc.guild.mfa_level).await?;
    }
    let scope = ScopeRef::Guild { guild: gid };
    let mut teardown = None;

    if let Some(values) = &body.settings {
        check_settings(&deps, &acc, gid, &feature, Scope::Guild, values).await?;
        gf.ctx
            .settings
            .set_settings(feature.id(), &scope, values.clone())
            .await
            .map_err(feature_err)?;
    }
    if let Some(enabled) = body.enabled {
        gf.ctx
            .settings
            .set_enabled(feature.id(), &scope, Some(enabled))
            .await
            .map_err(feature_err)?;
        // Switching the Discord bridge off is its teardown, before the answer (the shard stops, the webhooks are
        // deleted on Discord, the maps and the token go).
        if !enabled
            && feature.id() == sr_features::BRIDGE_MODULE_ID
            && let Some(b) = &bridge
        {
            let report = b
                .teardown(
                    i64::try_from(gid.get()).unwrap_or(0),
                    i64::try_from(user.get()).unwrap_or(0),
                    sr_bridge::TeardownReason::Owner,
                )
                .await
                .map_err(|e| {
                    tracing::error!(error = %e, "bridge teardown failed");
                    HttpError::from(ErrorCode::SrInternal)
                })?;
            teardown = Some(FeatureTeardown {
                maps_removed: report.maps_removed,
                failed_webhooks: report
                    .leftovers
                    .iter()
                    .map(|l| l.webhook_id.to_string())
                    .collect(),
            });
        }
    }
    let mut out = view(&gf.ctx, &feature, gid).await.map_err(feature_err)?;
    out.teardown = teardown;
    Ok(Json(out))
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

// ---- slowmode presets (G-74) ----------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
pub struct SlowmodePresetObject {
    pub id: String,
    pub name: String,
    pub seconds: u32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SlowmodePresetsObject {
    /// Whether the Slowmode schedules module is on in this server.
    pub enabled: bool,
    pub presets: Vec<SlowmodePresetObject>,
}

async fn require_channel_manager(
    deps: &SettingsDeps,
    user: UserId,
    gid: GuildId,
) -> ApiResult<super::deps::GuildAccess> {
    let acc = deps.guild_access(user, gid).await?;
    if !acc.perms.has(Permissions::MANAGE_CHANNELS) && !acc.perms.has(Permissions::MANAGE_GUILD) {
        return Err(ErrorCode::MissingPermissions.into());
    }
    Ok(acc)
}

/// The slowmode presets of this server and whether the module is on (`MANAGE_CHANNELS`).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/sr/slowmode/presets",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = SlowmodePresetsObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn slowmode_presets(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Ext(gf): Ext<GuildFeatures>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<SlowmodePresetsObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    require_channel_manager(&deps, user, gid).await?;
    let enabled = gf
        .ctx
        .settings
        .is_enabled(SLOWMODE_ID, &ScopeRef::Guild { guild: gid })
        .await
        .map_err(feature_err)?;
    let presets = if enabled {
        SlowmodePlus::presets(&gf.ctx, gid)
            .await
            .map_err(feature_err)?
            .into_iter()
            .map(|p| SlowmodePresetObject {
                id: p.id,
                name: p.name,
                seconds: p.seconds,
            })
            .collect()
    } else {
        Vec::new()
    };
    Ok(Json(SlowmodePresetsObject { enabled, presets }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ApplySlowmodeBody {
    pub preset_id: String,
    /// Channels to set; the caller needs `MANAGE_CHANNELS` in every one.
    pub channel_ids: Vec<String>,
    /// Put the old slowmode back after this many minutes (1 to 10080). Omit to keep it.
    #[serde(default)]
    pub minutes: Option<u32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AppliedSlowmodeObject {
    pub channel_id: String,
    /// Slowmode the channel had before, in seconds.
    pub before: u32,
    /// Slowmode now, in seconds.
    pub seconds: u32,
    /// Unix milliseconds at which it goes back, when a duration was given.
    pub revert_at: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApplySlowmodeResult {
    pub applied: Vec<AppliedSlowmodeObject>,
}

/// Apply a slowmode preset to channels now, optionally for a limited time (`MANAGE_CHANNELS` in
/// every listed channel). The change appears in the audit log as a Slowmode schedules action.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/sr/slowmode/apply",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = ApplySlowmodeBody,
    responses((status = 200, body = ApplySlowmodeResult), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn apply_slowmode(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Ext(gf): Ext<GuildFeatures>,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<ApplySlowmodeBody>,
) -> ApiResult<Json<ApplySlowmodeResult>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    deps.guild_access(user, gid).await?;
    let scope = ScopeRef::Guild { guild: gid };
    if !gf
        .ctx
        .settings
        .is_enabled(SLOWMODE_ID, &scope)
        .await
        .map_err(feature_err)?
    {
        return Err(ApiError::with_message(
            ErrorCode::SrFeatureDisabled,
            "Slowmode schedules are not turned on for this server.",
        )
        .into());
    }
    if body.channel_ids.is_empty() || body.channel_ids.len() > 25 {
        return Err(invalid(
            "channel_ids",
            "BASE_TYPE_BAD_LENGTH",
            "Choose 1 to 25 channels.",
        ));
    }
    if let Some(m) = body.minutes
        && !(1..=MAX_APPLY_MINUTES).contains(&m)
    {
        return Err(invalid(
            "minutes",
            "NUMBER_TYPE_MAX",
            "Minutes must be between 1 and 10080.",
        ));
    }
    let mut channels = Vec::new();
    for raw in &body.channel_ids {
        let ch: sr_core::ChannelId = id("channel_ids", raw)?;
        let access = deps.channel_access(user, ch).await?;
        if access.channel.guild_id != Some(gid) {
            return Err(ErrorCode::UnknownChannel.into());
        }
        if !access.perms.has(Permissions::MANAGE_CHANNELS) {
            return Err(ErrorCode::MissingPermissions.into());
        }
        if !channels.contains(&ch) {
            channels.push(ch);
        }
    }
    let applied = sr_features::scope_module(
        SLOWMODE_ID,
        "Slowmode schedules",
        SlowmodePlus::apply_preset(&gf.ctx, gid, &body.preset_id, &channels, body.minutes),
    )
    .await
    .map_err(feature_err)?;
    Ok(Json(ApplySlowmodeResult {
        applied: applied
            .into_iter()
            .map(|a| AppliedSlowmodeObject {
                channel_id: a.channel.to_string(),
                before: a.before,
                seconds: a.seconds,
                revert_at: a.revert_at_ms,
            })
            .collect(),
    }))
}
