//! Routes bots call: gateway discovery, the current application, application commands and
//! their permissions.

use super::{
    ApplicationObject, DevDeps, GatewayBotInfo, GatewayInfo, SessionStartLimit, app_err, bot_app,
    parse_id,
};
use crate::{
    auth::{AuthKind, BotOnly, Principal},
    openapi::ErrorBody,
    routes::{
        Ext,
        messages::error::{ApiJson, MsgError, MsgResult},
    },
};
use axum::{Json, extract::Path, http::StatusCode};
use serde_json::{Value, json};
use sr_apps::{commands::command_json, model::CommandPermission, service::PermissionsView};
use sr_core::{ApiError, ErrorCode, Permissions};

const APP: ErrorCode = ErrorCode::UnknownApplication;

/// Where to connect (Discord's unauthenticated `GET /gateway`).
#[utoipa::path(
    get, path = "/gateway", tag = "bot",
    responses((status = 200, body = GatewayInfo))
)]
pub async fn gateway(Ext(deps): Ext<DevDeps>) -> Json<GatewayInfo> {
    Json(GatewayInfo {
        url: deps.gateway_url(),
    })
}

/// Gateway URL and session limits for a bot. There is no sharding: `shards` is always 1.
#[utoipa::path(
    get, path = "/gateway/bot", tag = "bot",
    responses((status = 200, body = GatewayBotInfo), (status = 401, body = ErrorBody)),
    security(("authorization" = []))
)]
pub async fn gateway_bot(BotOnly(_): BotOnly, Ext(deps): Ext<DevDeps>) -> Json<GatewayBotInfo> {
    Json(GatewayBotInfo {
        url: deps.gateway_url(),
        shards: 1,
        session_start_limit: SessionStartLimit {
            total: 1000,
            remaining: 1000,
            reset_after: 86_400_000,
            max_concurrency: 1,
        },
    })
}

/// The bot's own application (public fields).
#[utoipa::path(
    get, path = "/applications/@me", tag = "bot",
    responses((status = 200, body = ApplicationObject), (status = 401, body = ErrorBody)),
    security(("authorization" = []))
)]
pub async fn current_application(
    BotOnly(p): BotOnly,
    Ext(deps): Ext<DevDeps>,
) -> MsgResult<Json<ApplicationObject>> {
    let AuthKind::Bot { application_id } = p.kind else {
        return Err(ErrorCode::OnlyBotsCanUseThisEndpoint.into());
    };
    let app = deps
        .apps
        .application(application_id)
        .await
        .map_err(app_err)?;
    let users = deps.users(&[app.owner_id, app.bot_user_id]).await?;
    Ok(Json(ApplicationObject::new(&app, &users, false, false)))
}

fn cmd_json(c: &sr_apps::model::Command) -> Value {
    command_json(c, false)
}

fn guild_of(raw: &str) -> MsgResult<i64> {
    parse_id(raw, ErrorCode::UnknownGuild)
}

fn command_of(raw: &str) -> MsgResult<i64> {
    parse_id(raw, ErrorCode::UnknownApplicationCommand)
}

async fn list(deps: &DevDeps, app: i64, guild: Option<i64>) -> MsgResult<Json<Vec<Value>>> {
    let cmds = deps.apps.list_commands(app, guild).await.map_err(app_err)?;
    Ok(Json(cmds.iter().map(cmd_json).collect()))
}

async fn create(
    deps: &DevDeps,
    app: i64,
    guild: Option<i64>,
    body: &Value,
) -> MsgResult<(StatusCode, Json<Value>)> {
    let (c, created) = deps
        .apps
        .create_command(app, guild, body)
        .await
        .map_err(app_err)?;
    Ok((
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(cmd_json(&c)),
    ))
}

/// Global commands of the application.
#[utoipa::path(
    get, path = "/applications/{application_id}/commands", tag = "bot",
    params(("application_id" = String, Path, description = "Application snowflake")),
    responses((status = 200, body = Vec<Object>), (status = 403, body = ErrorBody)),
    security(("authorization" = []))
)]
pub async fn list_global(
    BotOnly(p): BotOnly,
    Ext(deps): Ext<DevDeps>,
    Path(app): Path<String>,
) -> MsgResult<Json<Vec<Value>>> {
    list(&deps, bot_app(&p, &app)?, None).await
}

/// Create (or update by name) a global command.
#[utoipa::path(
    post, path = "/applications/{application_id}/commands", tag = "bot",
    params(("application_id" = String, Path, description = "Application snowflake")),
    request_body = Object,
    responses((status = 201, body = Object), (status = 200, body = Object), (status = 400, body = ErrorBody)),
    security(("authorization" = []))
)]
pub async fn create_global(
    BotOnly(p): BotOnly,
    Ext(deps): Ext<DevDeps>,
    Path(app): Path<String>,
    ApiJson(body): ApiJson<Value>,
) -> MsgResult<(StatusCode, Json<Value>)> {
    create(&deps, bot_app(&p, &app)?, None, &body).await
}

/// Replace all global commands.
#[utoipa::path(
    put, path = "/applications/{application_id}/commands", tag = "bot",
    params(("application_id" = String, Path, description = "Application snowflake")),
    request_body = Vec<Object>,
    responses((status = 200, body = Vec<Object>), (status = 400, body = ErrorBody)),
    security(("authorization" = []))
)]
pub async fn overwrite_global(
    BotOnly(p): BotOnly,
    Ext(deps): Ext<DevDeps>,
    Path(app): Path<String>,
    ApiJson(body): ApiJson<Value>,
) -> MsgResult<Json<Vec<Value>>> {
    let cmds = deps
        .apps
        .bulk_overwrite(bot_app(&p, &app)?, None, &body)
        .await
        .map_err(app_err)?;
    Ok(Json(cmds.iter().map(cmd_json).collect()))
}

/// One global command.
#[utoipa::path(
    get, path = "/applications/{application_id}/commands/{command_id}", tag = "bot",
    params(("application_id" = String, Path, description = "Application snowflake"),
        ("command_id" = String, Path, description = "Command snowflake")),
    responses((status = 200, body = Object), (status = 404, body = ErrorBody)),
    security(("authorization" = []))
)]
pub async fn get_global(
    BotOnly(p): BotOnly,
    Ext(deps): Ext<DevDeps>,
    Path((app, cmd)): Path<(String, String)>,
) -> MsgResult<Json<Value>> {
    let c = deps
        .apps
        .get_command(bot_app(&p, &app)?, None, command_of(&cmd)?)
        .await
        .map_err(app_err)?;
    Ok(Json(cmd_json(&c)))
}

/// Modify a global command.
#[utoipa::path(
    patch, path = "/applications/{application_id}/commands/{command_id}", tag = "bot",
    params(("application_id" = String, Path, description = "Application snowflake"),
        ("command_id" = String, Path, description = "Command snowflake")),
    request_body = Object,
    responses((status = 200, body = Object), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []))
)]
pub async fn patch_global(
    BotOnly(p): BotOnly,
    Ext(deps): Ext<DevDeps>,
    Path((app, cmd)): Path<(String, String)>,
    ApiJson(body): ApiJson<Value>,
) -> MsgResult<Json<Value>> {
    let c = deps
        .apps
        .patch_command(bot_app(&p, &app)?, None, command_of(&cmd)?, &body)
        .await
        .map_err(app_err)?;
    Ok(Json(cmd_json(&c)))
}

/// Delete a global command.
#[utoipa::path(
    delete, path = "/applications/{application_id}/commands/{command_id}", tag = "bot",
    params(("application_id" = String, Path, description = "Application snowflake"),
        ("command_id" = String, Path, description = "Command snowflake")),
    responses((status = 204), (status = 404, body = ErrorBody)),
    security(("authorization" = []))
)]
pub async fn delete_global(
    BotOnly(p): BotOnly,
    Ext(deps): Ext<DevDeps>,
    Path((app, cmd)): Path<(String, String)>,
) -> MsgResult<StatusCode> {
    deps.apps
        .delete_command(bot_app(&p, &app)?, None, command_of(&cmd)?)
        .await
        .map_err(app_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Commands of the application in one server.
#[utoipa::path(
    get, path = "/applications/{application_id}/guilds/{guild_id}/commands", tag = "bot",
    params(("application_id" = String, Path, description = "Application snowflake"),
        ("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = Vec<Object>), (status = 403, body = ErrorBody)),
    security(("authorization" = []))
)]
pub async fn list_guild(
    BotOnly(p): BotOnly,
    Ext(deps): Ext<DevDeps>,
    Path((app, guild)): Path<(String, String)>,
) -> MsgResult<Json<Vec<Value>>> {
    list(&deps, bot_app(&p, &app)?, Some(guild_of(&guild)?)).await
}

/// Create (or update by name) a server command.
#[utoipa::path(
    post, path = "/applications/{application_id}/guilds/{guild_id}/commands", tag = "bot",
    params(("application_id" = String, Path, description = "Application snowflake"),
        ("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = Object,
    responses((status = 201, body = Object), (status = 200, body = Object), (status = 400, body = ErrorBody)),
    security(("authorization" = []))
)]
pub async fn create_guild(
    BotOnly(p): BotOnly,
    Ext(deps): Ext<DevDeps>,
    Path((app, guild)): Path<(String, String)>,
    ApiJson(body): ApiJson<Value>,
) -> MsgResult<(StatusCode, Json<Value>)> {
    create(&deps, bot_app(&p, &app)?, Some(guild_of(&guild)?), &body).await
}

/// Replace all commands of one server.
#[utoipa::path(
    put, path = "/applications/{application_id}/guilds/{guild_id}/commands", tag = "bot",
    params(("application_id" = String, Path, description = "Application snowflake"),
        ("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = Vec<Object>,
    responses((status = 200, body = Vec<Object>), (status = 400, body = ErrorBody)),
    security(("authorization" = []))
)]
pub async fn overwrite_guild(
    BotOnly(p): BotOnly,
    Ext(deps): Ext<DevDeps>,
    Path((app, guild)): Path<(String, String)>,
    ApiJson(body): ApiJson<Value>,
) -> MsgResult<Json<Vec<Value>>> {
    let cmds = deps
        .apps
        .bulk_overwrite(bot_app(&p, &app)?, Some(guild_of(&guild)?), &body)
        .await
        .map_err(app_err)?;
    Ok(Json(cmds.iter().map(cmd_json).collect()))
}

/// One server command.
#[utoipa::path(
    get, path = "/applications/{application_id}/guilds/{guild_id}/commands/{command_id}", tag = "bot",
    params(("application_id" = String, Path, description = "Application snowflake"),
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("command_id" = String, Path, description = "Command snowflake")),
    responses((status = 200, body = Object), (status = 404, body = ErrorBody)),
    security(("authorization" = []))
)]
pub async fn get_guild(
    BotOnly(p): BotOnly,
    Ext(deps): Ext<DevDeps>,
    Path((app, guild, cmd)): Path<(String, String, String)>,
) -> MsgResult<Json<Value>> {
    let c = deps
        .apps
        .get_command(
            bot_app(&p, &app)?,
            Some(guild_of(&guild)?),
            command_of(&cmd)?,
        )
        .await
        .map_err(app_err)?;
    Ok(Json(cmd_json(&c)))
}

/// Modify a server command.
#[utoipa::path(
    patch, path = "/applications/{application_id}/guilds/{guild_id}/commands/{command_id}", tag = "bot",
    params(("application_id" = String, Path, description = "Application snowflake"),
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("command_id" = String, Path, description = "Command snowflake")),
    request_body = Object,
    responses((status = 200, body = Object), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []))
)]
pub async fn patch_guild(
    BotOnly(p): BotOnly,
    Ext(deps): Ext<DevDeps>,
    Path((app, guild, cmd)): Path<(String, String, String)>,
    ApiJson(body): ApiJson<Value>,
) -> MsgResult<Json<Value>> {
    let c = deps
        .apps
        .patch_command(
            bot_app(&p, &app)?,
            Some(guild_of(&guild)?),
            command_of(&cmd)?,
            &body,
        )
        .await
        .map_err(app_err)?;
    Ok(Json(cmd_json(&c)))
}

/// Delete a server command.
#[utoipa::path(
    delete, path = "/applications/{application_id}/guilds/{guild_id}/commands/{command_id}", tag = "bot",
    params(("application_id" = String, Path, description = "Application snowflake"),
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("command_id" = String, Path, description = "Command snowflake")),
    responses((status = 204), (status = 404, body = ErrorBody)),
    security(("authorization" = []))
)]
pub async fn delete_guild(
    BotOnly(p): BotOnly,
    Ext(deps): Ext<DevDeps>,
    Path((app, guild, cmd)): Path<(String, String, String)>,
) -> MsgResult<StatusCode> {
    deps.apps
        .delete_command(
            bot_app(&p, &app)?,
            Some(guild_of(&guild)?),
            command_of(&cmd)?,
        )
        .await
        .map_err(app_err)?;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------------ permissions v2

fn perm_entry(p: &CommandPermission) -> Value {
    json!({"id": p.target_id.to_string(), "type": p.target_type, "permission": p.allowed})
}

fn view_json(v: &PermissionsView) -> Value {
    json!({
        "id": v.id.to_string(),
        "application_id": v.application_id.to_string(),
        "guild_id": v.guild_id.to_string(),
        "permissions": v.permissions.iter().map(perm_entry).collect::<Vec<_>>(),
    })
}

/// Overwrites of every command in a server.
#[utoipa::path(
    get, path = "/applications/{application_id}/guilds/{guild_id}/commands/permissions", tag = "bot",
    params(("application_id" = String, Path, description = "Application snowflake"),
        ("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = Vec<Object>)),
    security(("authorization" = []))
)]
pub async fn guild_permissions(
    BotOnly(p): BotOnly,
    Ext(deps): Ext<DevDeps>,
    Path((app, guild)): Path<(String, String)>,
) -> MsgResult<Json<Vec<Value>>> {
    let views = deps
        .apps
        .guild_permissions(bot_app(&p, &app)?, guild_of(&guild)?)
        .await
        .map_err(app_err)?;
    Ok(Json(views.iter().map(view_json).collect()))
}

/// Overwrites of one command.
#[utoipa::path(
    get, path = "/applications/{application_id}/guilds/{guild_id}/commands/{command_id}/permissions",
    tag = "bot",
    params(("application_id" = String, Path, description = "Application snowflake"),
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("command_id" = String, Path, description = "Command snowflake")),
    responses((status = 200, body = Object), (status = 404, body = ErrorBody)),
    security(("authorization" = []))
)]
pub async fn command_permissions(
    BotOnly(p): BotOnly,
    Ext(deps): Ext<DevDeps>,
    Path((app, guild, cmd)): Path<(String, String, String)>,
) -> MsgResult<Json<Value>> {
    let app = bot_app(&p, &app)?;
    let cmd = command_of(&cmd)?;
    let v = deps
        .apps
        .command_permissions(app, guild_of(&guild)?, (cmd != app).then_some(cmd))
        .await
        .map_err(app_err)?;
    Ok(Json(view_json(&v)))
}

/// Replace the overwrites of one command. Needs a bearer token with
/// `applications.commands.permissions.update`, issued to this application, of a user who
/// manages the server. Bot tokens and sessions are refused (as on Discord).
#[utoipa::path(
    put, path = "/applications/{application_id}/guilds/{guild_id}/commands/{command_id}/permissions",
    tag = "bot",
    params(("application_id" = String, Path, description = "Application snowflake"),
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("command_id" = String, Path, description = "Command snowflake")),
    request_body = Object,
    responses((status = 200, body = Object), (status = 403, body = ErrorBody)),
    security(("authorization" = []))
)]
pub async fn set_command_permissions(
    p: Principal,
    Ext(deps): Ext<DevDeps>,
    Path((app, guild, cmd)): Path<(String, String, String)>,
    ApiJson(body): ApiJson<Value>,
) -> MsgResult<Json<Value>> {
    let deny = |m: &str| {
        MsgError::Api(ApiError::with_message(
            ErrorCode::MissingAccess,
            m.to_owned(),
        ))
    };
    let AuthKind::Bearer {
        scopes,
        application_id,
    } = &p.kind
    else {
        return Err(deny("This endpoint needs an OAuth2 bearer token."));
    };
    if !scopes
        .iter()
        .any(|s| s == "applications.commands.permissions.update")
    {
        return Err(deny(
            "Missing scope: applications.commands.permissions.update",
        ));
    }
    let app = parse_id(&app, APP)?;
    if *application_id != app {
        return Err(deny("The token was issued to another application."));
    }
    let guild = guild_of(&guild)?;
    let acc = deps
        .guild_access(p.user_id, guild)
        .await?
        .ok_or_else(|| deny("Missing Access"))?;
    if !acc.perms.contains(Permissions::MANAGE_GUILD)
        && !acc.perms.contains(Permissions::ADMINISTRATOR)
    {
        return Err(MsgError::Api(ApiError::new(ErrorCode::MissingPermissions)));
    }
    let cmd = command_of(&cmd)?;
    let v = deps
        .apps
        .set_command_permissions(app, guild, (cmd != app).then_some(cmd), &body)
        .await
        .map_err(app_err)?;
    Ok(Json(view_json(&v)))
}
