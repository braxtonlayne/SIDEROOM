//! Guild create/get/modify/delete and the caller's guild list.

use super::dto::{CreateGuildBody, GuildObject, GuildPreviewObject, ModifyGuildBody};
use super::util::{Api, actor, ce, id};
use crate::{AuthKind, Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{
    Json, Router,
    extract::Path,
    http::StatusCode,
    routing::{delete, get},
};
use sr_chat::admin_rules::GuildPatch;
use sr_core::{ChannelId, GuildId, UserId};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/guilds", axum::routing::post(create_guild))
        .route(
            "/guilds/{guild_id}",
            get(get_guild).patch(modify_guild).delete(delete_guild),
        )
        .route("/users/@me/guilds", get(list_my_guilds))
        .route("/users/@me/guilds/{guild_id}", delete(leave_guild))
}

/// Create a guild with `@everyone`, two categories, `#general` and a voice channel.
#[utoipa::path(
    post,
    path = "/guilds",
    request_body = CreateGuildBody,
    responses(
        (status = 201, description = "The new guild with its roles and channels", body = GuildObject),
        (status = 400, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn create_guild(
    p: Principal,
    Api(api): Api,
    Json(body): Json<CreateGuildBody>,
) -> ApiResult<(StatusCode, Json<GuildObject>)> {
    let owner = actor(&p)?;
    let bp = api.create_guild(owner, &body.name).await.map_err(ce)?;
    let channels: Vec<_> = bp
        .channels
        .iter()
        .map(|c| super::dto::ChannelObject::new(c, &[]))
        .collect();
    let roles = [bp.everyone_role.clone()];
    Ok((
        StatusCode::CREATED,
        Json(GuildObject::new(&bp.guild, Some(&roles), Some(&channels))),
    ))
}

/// Get a guild (members only).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses(
        (status = 200, body = GuildObject),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_guild(
    p: Principal,
    Api(api): Api,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<GuildObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let (g, roles) = api.get_guild(user, gid).await.map_err(ce)?;
    Ok(Json(GuildObject::new(&g, Some(&roles), None)))
}

/// Modify a guild; `MANAGE_GUILD`, or the owner for `owner_id`.
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = ModifyGuildBody,
    responses(
        (status = 200, body = GuildObject),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn modify_guild(
    p: Principal,
    Api(api): Api,
    Path(guild_id): Path<String>,
    Json(body): Json<ModifyGuildBody>,
) -> ApiResult<Json<GuildObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let system_channel_id = match body.system_channel_id {
        None => None,
        Some(None) => Some(None),
        Some(Some(s)) => Some(Some(id::<ChannelId>("system_channel_id", &s)?)),
    };
    let owner_id = body
        .owner_id
        .as_deref()
        .map(|o| id::<UserId>("owner_id", o))
        .transpose()?;
    let patch = GuildPatch {
        name: body.name,
        description: body.description,
        system_channel_id,
        verification_level: body.verification_level,
        default_message_notifications: body.default_message_notifications,
        explicit_content_filter: body.explicit_content_filter,
        preferred_locale: body.preferred_locale,
        owner_id,
    };
    let g = api.edit_guild(user, gid, patch).await.map_err(ce)?;
    let (_, roles) = api.get_guild(user, gid).await.map_err(ce)?;
    Ok(Json(GuildObject::new(&g, Some(&roles), None)))
}

/// Delete a guild (owner only; the Staff Server cannot be deleted).
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn delete_guild(
    p: Principal,
    Api(api): Api,
    Path(guild_id): Path<String>,
) -> ApiResult<StatusCode> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    api.delete_guild(user, gid).await.map_err(ce)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Guilds the caller belongs to. Bearer tokens need the `guilds` scope.
#[utoipa::path(
    get,
    path = "/users/@me/guilds",
    responses((status = 200, body = Vec<GuildPreviewObject>), (status = 401, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_my_guilds(
    p: Principal,
    Api(api): Api,
) -> ApiResult<Json<Vec<GuildPreviewObject>>> {
    if matches!(p.kind, AuthKind::Bearer { .. }) {
        p.require_scope("guilds")?;
    }
    let user = UserId::new(u64::try_from(p.user_id).unwrap_or(0));
    let list = api.list_my_guilds(user).await.map_err(ce)?;
    Ok(Json(
        list.iter()
            .map(|(g, perms)| GuildPreviewObject {
                id: g.id.to_string(),
                name: g.name.clone(),
                icon: g.icon_key.clone(),
                owner: g.owner_id == user,
                permissions: perms.bits().to_string(),
                features: Vec::new(),
            })
            .collect(),
    ))
}

/// Leave a guild. The owner must transfer ownership first.
#[utoipa::path(
    delete,
    path = "/users/@me/guilds/{guild_id}",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses(
        (status = 204, description = "Left"),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn leave_guild(
    p: Principal,
    Api(api): Api,
    Path(guild_id): Path<String>,
) -> ApiResult<StatusCode> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    api.leave_guild(user, gid).await.map_err(ce)?;
    Ok(StatusCode::NO_CONTENT)
}
