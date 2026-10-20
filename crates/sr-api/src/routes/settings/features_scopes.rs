//! Channel and user scope of the feature modules (LNG-CHN-017, LNG-USET-036).
//!
//! * `GET|PATCH|DELETE /channels/{id}/sr/features[/{feature_id}]`: the modules that
//!   declare a channel scope, with this channel's own switch and overrides on top of the
//!   server's settings (guild -> channel inheritance). `MANAGE_CHANNELS` in the channel.
//!   A channel can narrow a module a server has on (switch it off here, change a setting
//!   here); it cannot switch on a module the server has off. `DELETE` removes the
//!   channel's overrides so it follows the server again.
//! * `GET|PATCH|DELETE /users/@me/sr/features[/{feature_id}]`: the modules that declare a
//!   user scope, with the person's own preferences. Only the person themself can read or
//!   write them; there is no on/off at this level (a server decides whether a module runs),
//!   only the settings the module marks as personal.
//!
//! Both levels use the same settings schema the server form uses, narrowed to the fields
//! that belong to the level ([`sr_features::SettingsSchema::for_scope`]).

use super::deps::{SettingsDeps, actor, id};
use super::features::{GuildFeatures, check_settings, feature_err};
use crate::routes::Ext;
use crate::routes::guilds::safety::MfaGate;
use crate::routes::messages::error::ApiJson;
use crate::{HttpError, Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{Json, Router, extract::Path, http::StatusCode, routing::get};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sr_core::permissions::Permissions;
use sr_core::{ApiError, ChannelId, ErrorCode, GuildId, UserId};
use sr_features::{Feature, FeatureCtx, FeatureError, Scope, ScopeRef};
use std::sync::Arc;
use utoipa::ToSchema;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/channels/{channel_id}/sr/features",
            get(list_channel_features),
        )
        .route(
            "/channels/{channel_id}/sr/features/{feature_id}",
            axum::routing::patch(patch_channel_feature).delete(reset_channel_feature),
        )
        .route("/users/@me/sr/features", get(list_user_features))
        .route(
            "/users/@me/sr/features/{feature_id}",
            axum::routing::patch(patch_user_feature).delete(reset_user_feature),
        )
}

/// One module as a channel sees it.
#[derive(Debug, Serialize, ToSchema)]
pub struct ChannelFeatureObject {
    pub id: String,
    pub name: String,
    pub description: String,
    pub section: String,
    /// Whether the module is on in the server. A channel cannot switch it on by itself.
    pub guild_enabled: bool,
    /// The channel's own switch: `true` or `false`, or `null` when it follows the server.
    pub override_enabled: Option<bool>,
    /// Whether the module is on in this channel (the server's switch narrowed by the channel's).
    pub enabled: bool,
    /// The platform operator switched the module off everywhere.
    pub platform_disabled: bool,
    /// Effective settings in this channel (defaults, the server's, then the channel's).
    #[schema(value_type = Object)]
    pub settings: Value,
    /// Only what this channel overrides.
    #[schema(value_type = Object)]
    pub overrides: Value,
    /// The settings schema of the channel level, for the auto-rendered form.
    #[schema(value_type = Object)]
    pub schema: Value,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PatchChannelFeatureBody {
    /// This channel's own switch for the module.
    #[serde(default)]
    pub enabled: Option<bool>,
    /// Replace the channel's setting overrides (validated against the channel schema).
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub settings: Option<Map<String, Value>>,
}

/// One module as a person sees it.
#[derive(Debug, Serialize, ToSchema)]
pub struct UserFeatureObject {
    pub id: String,
    pub name: String,
    pub description: String,
    pub section: String,
    /// The person's effective preferences (defaults plus what they chose).
    #[schema(value_type = Object)]
    pub settings: Value,
    /// Only what the person changed.
    #[schema(value_type = Object)]
    pub overrides: Value,
    /// The settings schema of the personal level, for the auto-rendered form.
    #[schema(value_type = Object)]
    pub schema: Value,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PatchUserFeatureBody {
    /// Replace the person's preferences for the module (validated against the personal schema).
    #[schema(value_type = Object)]
    pub settings: Map<String, Value>,
}

fn declared(
    ctx: &FeatureCtx,
    feature_id: &str,
    scope: Scope,
) -> Result<Arc<dyn Feature>, HttpError> {
    let feature = ctx
        .settings
        .registry()
        .get(feature_id)
        .cloned()
        .ok_or_else(|| feature_err(FeatureError::UnknownFeature(feature_id.to_owned())))?;
    if !feature.scopes().contains(&scope) {
        return Err(ApiError::with_message(
            ErrorCode::SrNotFound,
            format!(
                "{feature_id} has no {} settings",
                match scope {
                    Scope::Channel => "per-channel",
                    Scope::User => "personal",
                    _ => "scoped",
                }
            ),
        )
        .into());
    }
    Ok(feature)
}

async fn channel_view(
    ctx: &FeatureCtx,
    f: &Arc<dyn Feature>,
    guild: GuildId,
    channel: ChannelId,
) -> Result<ChannelFeatureObject, FeatureError> {
    let meta = f.meta();
    let fid = f.id();
    let scope = ScopeRef::Channel { guild, channel };
    let own = ctx.settings.overrides(fid, &scope).await?;
    let guild_enabled = ctx
        .settings
        .is_enabled(fid, &ScopeRef::Guild { guild })
        .await?;
    let channel_enabled = ctx.settings.is_enabled(fid, &scope).await?;
    Ok(ChannelFeatureObject {
        id: fid.to_owned(),
        name: meta.name,
        description: meta.description,
        section: meta.section,
        guild_enabled,
        override_enabled: own.enabled,
        enabled: guild_enabled && channel_enabled,
        platform_disabled: ctx.settings.platform_killed(fid).await?,
        settings: Value::Object(ctx.settings.effective_settings(fid, &scope).await?),
        overrides: Value::Object(own.values),
        schema: serde_json::to_value(f.settings_schema().for_scope(Scope::Channel))
            .unwrap_or(Value::Null),
    })
}

async fn user_view(
    ctx: &FeatureCtx,
    f: &Arc<dyn Feature>,
    user: UserId,
) -> Result<UserFeatureObject, FeatureError> {
    let meta = f.meta();
    let fid = f.id();
    let scope = ScopeRef::User { user };
    let own = ctx.settings.overrides(fid, &scope).await?;
    Ok(UserFeatureObject {
        id: fid.to_owned(),
        name: meta.name,
        description: meta.description,
        section: meta.section,
        settings: Value::Object(ctx.settings.effective_settings(fid, &scope).await?),
        overrides: Value::Object(own.values),
        schema: serde_json::to_value(f.settings_schema().for_scope(Scope::User))
            .unwrap_or(Value::Null),
    })
}

/// A guild channel the caller may configure: `MANAGE_CHANNELS` in it.
async fn managed_channel(
    deps: &SettingsDeps,
    user: UserId,
    channel_id: &str,
) -> ApiResult<(ChannelId, super::deps::ChannelAccess)> {
    let cid: ChannelId = id("channel_id", channel_id)?;
    let acc = deps.channel_access(user, cid).await?;
    if !acc.perms.has(Permissions::MANAGE_CHANNELS) {
        return Err(ErrorCode::MissingPermissions.into());
    }
    Ok((cid, acc))
}

/// The modules with a channel scope, with this channel's state (`MANAGE_CHANNELS`).
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/sr/features",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 200, body = Vec<ChannelFeatureObject>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_channel_features(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Ext(gf): Ext<GuildFeatures>,
    Path(channel_id): Path<String>,
) -> ApiResult<Json<Vec<ChannelFeatureObject>>> {
    let user = actor(&p)?;
    let (cid, acc) = managed_channel(&deps, user, &channel_id).await?;
    let gid = acc.guild.guild.id;
    let mut out = Vec::new();
    for f in gf.ctx.settings.registry().iter() {
        if f.scopes().contains(&Scope::Channel) {
            out.push(
                channel_view(&gf.ctx, f, gid, cid)
                    .await
                    .map_err(feature_err)?,
            );
        }
    }
    Ok(Json(out))
}

/// Switch a module on or off in this channel, or change its settings here
/// (`MANAGE_CHANNELS`).
#[utoipa::path(
    patch,
    path = "/channels/{channel_id}/sr/features/{feature_id}",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("feature_id" = String, Path, description = "Feature id, e.g. lounge.message_quotes"),
    ),
    request_body = PatchChannelFeatureBody,
    responses((status = 200, body = ChannelFeatureObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn patch_channel_feature(
    p: Principal,
    gate: MfaGate,
    Ext(deps): Ext<SettingsDeps>,
    Ext(gf): Ext<GuildFeatures>,
    Path((channel_id, feature_id)): Path<(String, String)>,
    ApiJson(body): ApiJson<PatchChannelFeatureBody>,
) -> ApiResult<Json<ChannelFeatureObject>> {
    let user = actor(&p)?;
    let (cid, acc) = managed_channel(&deps, user, &channel_id).await?;
    let gid = acc.guild.guild.id;
    acc.guild
        .require_mfa(&gate, user, Permissions::MANAGE_CHANNELS)
        .await?;
    let feature = declared(&gf.ctx, &feature_id, Scope::Channel)?;
    let scope = ScopeRef::Channel {
        guild: gid,
        channel: cid,
    };
    if let Some(values) = &body.settings {
        check_settings(&deps, &acc.guild, gid, &feature, Scope::Channel, values).await?;
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
    }
    Ok(Json(
        channel_view(&gf.ctx, &feature, gid, cid)
            .await
            .map_err(feature_err)?,
    ))
}

/// Remove this channel's switch and overrides, so it follows the server again
/// (`MANAGE_CHANNELS`).
#[utoipa::path(
    delete,
    path = "/channels/{channel_id}/sr/features/{feature_id}",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("feature_id" = String, Path, description = "Feature id, e.g. lounge.message_quotes"),
    ),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn reset_channel_feature(
    p: Principal,
    gate: MfaGate,
    Ext(deps): Ext<SettingsDeps>,
    Ext(gf): Ext<GuildFeatures>,
    Path((channel_id, feature_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let user = actor(&p)?;
    let (cid, acc) = managed_channel(&deps, user, &channel_id).await?;
    acc.guild
        .require_mfa(&gate, user, Permissions::MANAGE_CHANNELS)
        .await?;
    let feature = declared(&gf.ctx, &feature_id, Scope::Channel)?;
    gf.ctx
        .settings
        .clear(
            feature.id(),
            &ScopeRef::Channel {
                guild: acc.guild.guild.id,
                channel: cid,
            },
        )
        .await
        .map_err(feature_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// The modules with personal settings, with the caller's own preferences. Only the
/// caller can read them.
#[utoipa::path(
    get,
    path = "/users/@me/sr/features",
    responses((status = 200, body = Vec<UserFeatureObject>), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list_user_features(
    p: crate::auth::UserOnly,
    Ext(gf): Ext<GuildFeatures>,
) -> ApiResult<Json<Vec<UserFeatureObject>>> {
    let user = actor(&p.0)?;
    let mut out = Vec::new();
    for f in gf.ctx.settings.registry().iter() {
        if f.scopes().contains(&Scope::User) {
            out.push(user_view(&gf.ctx, f, user).await.map_err(feature_err)?);
        }
    }
    Ok(Json(out))
}

/// Change the caller's own preferences for one module.
#[utoipa::path(
    patch,
    path = "/users/@me/sr/features/{feature_id}",
    params(("feature_id" = String, Path, description = "Feature id, e.g. community.leveling")),
    request_body = PatchUserFeatureBody,
    responses((status = 200, body = UserFeatureObject), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn patch_user_feature(
    p: crate::auth::UserOnly,
    Ext(gf): Ext<GuildFeatures>,
    Path(feature_id): Path<String>,
    ApiJson(body): ApiJson<PatchUserFeatureBody>,
) -> ApiResult<Json<UserFeatureObject>> {
    let user = actor(&p.0)?;
    let feature = declared(&gf.ctx, &feature_id, Scope::User)?;
    gf.ctx
        .settings
        .set_settings(feature.id(), &ScopeRef::User { user }, body.settings)
        .await
        .map_err(feature_err)?;
    Ok(Json(
        user_view(&gf.ctx, &feature, user)
            .await
            .map_err(feature_err)?,
    ))
}

/// Forget the caller's preferences for one module, so the defaults apply again.
#[utoipa::path(
    delete,
    path = "/users/@me/sr/features/{feature_id}",
    params(("feature_id" = String, Path, description = "Feature id, e.g. community.leveling")),
    responses((status = 204), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn reset_user_feature(
    p: crate::auth::UserOnly,
    Ext(gf): Ext<GuildFeatures>,
    Path(feature_id): Path<String>,
) -> ApiResult<StatusCode> {
    let user = actor(&p.0)?;
    let feature = declared(&gf.ctx, &feature_id, Scope::User)?;
    gf.ctx
        .settings
        .clear(feature.id(), &ScopeRef::User { user })
        .await
        .map_err(feature_err)?;
    Ok(StatusCode::NO_CONTENT)
}
