//! Roles: CRUD, reorder and member assignment.

use super::dto::{CreateRoleBody, MemberObject, ModifyRoleBody, ReorderRoleEntry, RoleObject};
use super::util::{Api, actor, ce, id, permissions, users_for};
use crate::routes::{MaybeExt, media::MediaDeps};
use crate::{Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{
    Json, Router,
    extract::{Path, Query},
    http::StatusCode,
    routing::{get, put},
};
use sr_chat::role_rules::{NewRole, RolePatch};
use sr_core::{GuildId, RoleId, UserId};
use sr_storage::media::MediaKind;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/guilds/{guild_id}/roles",
            get(list_roles).post(create_role).patch(reorder_roles),
        )
        .route(
            "/guilds/{guild_id}/roles/{role_id}",
            axum::routing::patch(modify_role).delete(delete_role),
        )
        .route(
            "/guilds/{guild_id}/roles/{role_id}/members",
            get(list_role_members),
        )
        .route(
            "/guilds/{guild_id}/members/{user_id}/roles/{role_id}",
            put(add_member_role).delete(remove_member_role),
        )
}

/// Roles of the guild (members only), ordered by position.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/roles",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = Vec<RoleObject>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_roles(
    p: Principal,
    Api(api): Api,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<Vec<RoleObject>>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let roles = api.list_roles(user, gid).await.map_err(ce)?;
    Ok(Json(roles.iter().map(RoleObject::from).collect()))
}

/// Create a role directly above `@everyone`. Needs `MANAGE_ROLES`; the caller can only grant bits they hold.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/roles",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = CreateRoleBody,
    responses((status = 200, body = RoleObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn create_role(
    p: Principal,
    Api(api): Api,
    gate: super::safety::MfaGate,
    Path(guild_id): Path<String>,
    Json(body): Json<CreateRoleBody>,
) -> ApiResult<Json<RoleObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    gate.require(&*api, user, gid).await?;
    let input = NewRole {
        name: body.name,
        permissions: body
            .permissions
            .as_deref()
            .map(|s| permissions("permissions", s))
            .transpose()?,
        color: body.color,
        hoist: body.hoist.unwrap_or(false),
        mentionable: body.mentionable.unwrap_or(false),
        unicode_emoji: body.unicode_emoji,
    };
    let role = api.create_role(user, gid, input).await.map_err(ce)?;
    Ok(Json(RoleObject::from(&role)))
}

/// Bulk update role positions. Returns every role of the guild.
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/roles",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = Vec<ReorderRoleEntry>,
    responses((status = 200, body = Vec<RoleObject>), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn reorder_roles(
    p: Principal,
    Api(api): Api,
    gate: super::safety::MfaGate,
    Path(guild_id): Path<String>,
    Json(body): Json<Vec<ReorderRoleEntry>>,
) -> ApiResult<Json<Vec<RoleObject>>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    gate.require(&*api, user, gid).await?;
    let mut moves = Vec::with_capacity(body.len());
    for (n, e) in body.iter().enumerate() {
        moves.push((id::<RoleId>(&format!("{n}.id"), &e.id)?, e.position));
    }
    let roles = api.reorder_roles(user, gid, moves).await.map_err(ce)?;
    Ok(Json(roles.iter().map(RoleObject::from).collect()))
}

/// Modify a role the caller outranks.
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/roles/{role_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("role_id" = String, Path, description = "Role snowflake"),
    ),
    request_body = ModifyRoleBody,
    responses((status = 200, body = RoleObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn modify_role(
    p: Principal,
    Api(api): Api,
    gate: super::safety::MfaGate,
    MaybeExt(media): MaybeExt<MediaDeps>,
    Path((guild_id, role_id)): Path<(String, String)>,
    Json(body): Json<ModifyRoleBody>,
) -> ApiResult<Json<RoleObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    gate.require(&*api, user, gid).await?;
    let rid: RoleId = id("role_id", &role_id)?;
    // The image is stored first (a bad upload fails the edit), the old one is removed after
    // the edit committed, and a refused edit removes the new one again.
    let mut new_icon: Option<Option<String>> = None;
    let mut old_icon: Option<String> = None;
    if body.icon.is_some() || body.unicode_emoji.as_ref().is_some_and(Option::is_some) {
        // An emoji also clears the stored image, so its object must go too.
        old_icon = api
            .get_guild(user, gid)
            .await
            .map_err(ce)?
            .1
            .into_iter()
            .find(|r| r.id == rid)
            .and_then(|r| r.icon_key);
    }
    if let Some(icon) = body.icon {
        if icon.is_some() && media.is_none() {
            tracing::error!("role icon edit needs MediaDeps registered on AppState");
            return Err(sr_core::ErrorCode::SrInternal.into());
        }
        new_icon = Some(match (icon, &media) {
            (Some(uri), Some(m)) => {
                Some(m.store(MediaKind::RoleIcon, gid.get(), "icon", uri).await?)
            }
            _ => None,
        });
    }
    let stored_new = new_icon.clone().flatten();
    let patch = RolePatch {
        name: body.name,
        permissions: body
            .permissions
            .as_deref()
            .map(|s| permissions("permissions", s))
            .transpose()?,
        color: body.color,
        hoist: body.hoist,
        mentionable: body.mentionable,
        unicode_emoji: body.unicode_emoji,
        icon_key: new_icon,
    };
    let role = match api.edit_role(user, gid, rid, patch).await {
        Ok(r) => r,
        Err(e) => {
            if let (Some(m), Some(h)) = (&media, &stored_new) {
                m.remove(MediaKind::RoleIcon, gid.get(), h).await;
            }
            return Err(ce(e));
        }
    };
    if let (Some(m), Some(old)) = (&media, &old_icon)
        && role.icon_key.as_ref() != Some(old)
    {
        m.remove(MediaKind::RoleIcon, gid.get(), old).await;
    }
    Ok(Json(RoleObject::from(&role)))
}

/// Delete a role the caller outranks; members lose it.
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/roles/{role_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("role_id" = String, Path, description = "Role snowflake"),
    ),
    responses((status = 204, description = "Deleted"), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn delete_role(
    p: Principal,
    Api(api): Api,
    gate: super::safety::MfaGate,
    Path((guild_id, role_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    gate.require(&*api, user, gid).await?;
    let rid: RoleId = id("role_id", &role_id)?;
    api.delete_role(user, gid, rid).await.map_err(ce)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_member_role(
    p: Principal,
    api: std::sync::Arc<dyn super::backend::GuildBackend>,
    gate: super::safety::MfaGate,
    (guild_id, user_id, role_id): (String, String, String),
    add: bool,
) -> ApiResult<StatusCode> {
    let actor = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let uid: UserId = id("user_id", &user_id)?;
    let rid: RoleId = id("role_id", &role_id)?;
    gate.require(&*api, actor, gid).await?;
    api.assign_role(actor, gid, uid, rid, add)
        .await
        .map_err(ce)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Add a role to a member.
#[utoipa::path(
    put,
    path = "/guilds/{guild_id}/members/{user_id}/roles/{role_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("user_id" = String, Path, description = "User snowflake"),
        ("role_id" = String, Path, description = "Role snowflake"),
    ),
    responses((status = 204, description = "Added"), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn add_member_role(
    p: Principal,
    Api(api): Api,
    gate: super::safety::MfaGate,
    Path(ids): Path<(String, String, String)>,
) -> ApiResult<StatusCode> {
    set_member_role(p, api, gate, ids, true).await
}

/// Remove a role from a member.
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/members/{user_id}/roles/{role_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("user_id" = String, Path, description = "User snowflake"),
        ("role_id" = String, Path, description = "Role snowflake"),
    ),
    responses((status = 204, description = "Removed"), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn remove_member_role(
    p: Principal,
    Api(api): Api,
    gate: super::safety::MfaGate,
    Path(ids): Path<(String, String, String)>,
) -> ApiResult<StatusCode> {
    set_member_role(p, api, gate, ids, false).await
}

/// Query of `GET /guilds/{id}/roles/{role_id}/members`.
#[derive(Debug, serde::Deserialize)]
pub struct RoleMembersQuery {
    /// 1 to 1000; default 100.
    pub limit: Option<usize>,
    /// Highest user id of the previous page.
    pub after: Option<String>,
}

/// Members holding a role, ordered by user id (any member of the guild may list them).
/// `@everyone` lists every member.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/roles/{role_id}/members",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("role_id" = String, Path, description = "Role snowflake"),
        ("limit" = Option<usize>, Query, description = "1 to 1000, default 100"),
        ("after" = Option<String>, Query, description = "User snowflake cursor"),
    ),
    responses((status = 200, body = Vec<MemberObject>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_role_members(
    p: Principal,
    Api(api): Api,
    Path((guild_id, role_id)): Path<(String, String)>,
    Query(q): Query<RoleMembersQuery>,
) -> ApiResult<Json<Vec<MemberObject>>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let rid: RoleId = id("role_id", &role_id)?;
    let after = q
        .after
        .as_deref()
        .map(|a| id::<UserId>("after", a))
        .transpose()?;
    let members = api
        .list_role_members(user, gid, rid, after, q.limit.unwrap_or(100))
        .await
        .map_err(ce)?;
    let users = users_for(&*api, members.iter().map(|m| m.user_id)).await?;
    Ok(Json(
        members
            .iter()
            .map(|m| MemberObject::new(m, false, &users))
            .collect(),
    ))
}
