//! The server's Integrations list (J-05): the applications installed in a server.
//!
//! * `GET /guilds/{id}/integrations` (`MANAGE_GUILD`): one Discord integration object per
//!   installed application. Webhooks and followed channels, which Discord also lists, have
//!   their own routes and are not added here.
//! * `DELETE /guilds/{id}/integrations/{integration_id}` (`MANAGE_GUILD`, and a second factor
//!   when the server asks for one): removes the install the way removing the bot does (the
//!   managed role and the bot's membership go with it) and writes audit entry `INTEGRATION_DELETE`.

use super::{DevDeps, app_err, parse_id};
use crate::{
    auth::Principal,
    openapi::ErrorBody,
    routes::{
        Ext, MaybeExt,
        guilds::{dto::UserRef, safety::MfaGate},
        messages::error::{MsgError, MsgResult},
        settings::SettingsDeps,
    },
};
use axum::{Json, extract::Path, http::StatusCode};
use serde::Serialize;
use serde_json::json;
use sr_chat::settings::audit::{AuditAction, AuditChange};
use sr_core::{ApiError, ErrorCode, GuildId, Permissions, RoleId, UserId};
use sr_events::DomainEvent;
use utoipa::ToSchema;

/// The application part of an integration (Discord shape).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct IntegrationApplicationObject {
    pub id: String,
    pub name: String,
    /// Icon hash, served at `/cdn/avatars/{bot.id}/{hash}.png`.
    pub icon: Option<String>,
    pub description: String,
    pub bot: UserRef,
}

/// `account` of an integration: for an application it is the application itself.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct IntegrationAccountObject {
    pub id: String,
    pub name: String,
}

/// One installed application as a Discord integration object.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct IntegrationObject {
    /// The application id.
    pub id: String,
    pub name: String,
    /// Always `discord`, as for Discord's own application integrations.
    #[serde(rename = "type")]
    pub kind: String,
    pub enabled: bool,
    pub account: IntegrationAccountObject,
    pub application: IntegrationApplicationObject,
    /// Who installed it.
    pub user: UserRef,
    /// The scopes the install was granted.
    pub scopes: Vec<String>,
}

/// The caller must hold `MANAGE_GUILD` (or `ADMINISTRATOR`) in the server; returns the
/// server's second-factor level for the delete.
async fn manager_level(deps: &DevDeps, user: i64, guild: i64) -> MsgResult<u8> {
    let acc = deps
        .guild_access(user, guild)
        .await?
        .ok_or_else(|| MsgError::Api(ApiError::new(ErrorCode::MissingAccess)))?;
    if acc.perms.contains(Permissions::MANAGE_GUILD)
        || acc.perms.contains(Permissions::ADMINISTRATOR)
    {
        Ok(acc.guild.mfa_level)
    } else {
        Err(MsgError::Api(ApiError::new(ErrorCode::MissingPermissions)))
    }
}

/// The applications installed in the server (`MANAGE_GUILD`).
#[utoipa::path(
    get, path = "/guilds/{guild_id}/integrations", tag = "developer",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = Vec<IntegrationObject>), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_integrations(
    p: Principal,
    Ext(deps): Ext<DevDeps>,
    Path(guild): Path<String>,
) -> MsgResult<Json<Vec<IntegrationObject>>> {
    let guild = parse_id(&guild, ErrorCode::UnknownGuild)?;
    manager_level(&deps, p.user_id, guild).await?;
    let installed = deps.apps.guild_integrations(guild).await.map_err(app_err)?;
    let mut ids: Vec<i64> = Vec::new();
    for (install, app) in &installed {
        ids.push(install.installed_by);
        ids.push(app.bot_user_id);
    }
    let users = deps.users(&ids).await?;
    let out = installed
        .iter()
        .map(|(install, app)| IntegrationObject {
            id: app.id.to_string(),
            name: app.name.clone(),
            kind: "discord".to_owned(),
            enabled: true,
            account: IntegrationAccountObject {
                id: app.id.to_string(),
                name: app.name.clone(),
            },
            application: IntegrationApplicationObject {
                id: app.id.to_string(),
                name: app.name.clone(),
                icon: app.icon_key.clone(),
                description: app.description.clone(),
                bot: UserRef::of(UserId::new(app.bot_user_id as u64), &users),
            },
            user: UserRef::of(UserId::new(install.installed_by as u64), &users),
            scopes: install.scopes.clone(),
        })
        .collect();
    Ok(Json(out))
}

/// Remove an installed application from the server (`MANAGE_GUILD`; a second factor when the
/// server requires one). The bot leaves and its managed role is deleted.
#[utoipa::path(
    delete, path = "/guilds/{guild_id}/integrations/{integration_id}", tag = "developer",
    params(("guild_id" = String, Path, description = "Guild snowflake"),
        ("integration_id" = String, Path, description = "The application id")),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn delete_integration(
    p: Principal,
    gate: MfaGate,
    Ext(deps): Ext<DevDeps>,
    MaybeExt(settings): MaybeExt<SettingsDeps>,
    Path((guild, integration)): Path<(String, String)>,
) -> MsgResult<StatusCode> {
    let guild_raw = parse_id(&guild, ErrorCode::UnknownGuild)?;
    let level = manager_level(&deps, p.user_id, guild_raw).await?;
    gate.check_level(UserId::new(p.user_id as u64), level)
        .await
        .map_err(MsgError::from)?;
    let app_id = parse_id(&integration, ErrorCode::UnknownIntegration)?;
    let app = deps
        .apps
        .application(app_id)
        .await
        .map_err(|_| MsgError::Api(ApiError::new(ErrorCode::UnknownIntegration)))?;
    let install = deps
        .apps
        .uninstall(app_id, guild_raw)
        .await
        .map_err(app_err)?;
    let gid = GuildId::new(guild_raw as u64);
    if let Some(role) = install.role_id {
        deps.publish(DomainEvent::RoleDeleted {
            guild: gid,
            role: RoleId::new(role as u64),
            payload: json!({ "guild_id": gid.to_string(), "role_id": role.to_string() }),
        })
        .await;
    }
    deps.publish(DomainEvent::MemberLeft {
        guild: gid,
        user: UserId::new(app.bot_user_id as u64),
    })
    .await;
    if let Some(settings) = settings.as_deref()
        && let Err(e) = settings
            .service
            .record_audit(
                gid,
                UserId::new(p.user_id as u64),
                AuditAction::IntegrationDelete,
                u64::try_from(app_id).ok(),
                vec![AuditChange::new(
                    "name",
                    Some(app.name.clone().into()),
                    None,
                )],
                None,
            )
            .await
    {
        tracing::warn!(error = %e, "integration removal was not written to the audit log");
    }
    Ok(StatusCode::NO_CONTENT)
}
