//! Guild members: list, get, modify, kick.

use super::dto::{MemberObject, ModifyMemberBody, parse_iso8601};
use super::util::{Api, actor, ce, id};
use crate::{HttpError, Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{
    Json, Router,
    extract::{Path, Query},
    http::StatusCode,
    routing::get,
};
use serde::Deserialize;
use sr_chat::admin_rules::MemberPatch;
use sr_core::{ApiError, GuildId, RoleId, UserId};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/guilds/{guild_id}/members", get(list_members))
        .route(
            "/guilds/{guild_id}/members/{user_id}",
            get(get_member).patch(modify_member).delete(kick_member),
        )
}

#[derive(Debug, Deserialize)]
pub struct ListMembersQuery {
    /// 1 to 1000; Discord's default is 1.
    pub limit: Option<usize>,
    /// Highest user id of the previous page.
    pub after: Option<String>,
}

/// Page through the members of a guild, ordered by user id.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/members",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("limit" = Option<usize>, Query, description = "1 to 1000, default 1"),
        ("after" = Option<String>, Query, description = "User snowflake cursor"),
    ),
    responses((status = 200, body = Vec<MemberObject>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_members(
    p: Principal,
    Api(api): Api,
    Path(guild_id): Path<String>,
    Query(q): Query<ListMembersQuery>,
) -> ApiResult<Json<Vec<MemberObject>>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let after = q
        .after
        .as_deref()
        .map(|a| id::<UserId>("after", a))
        .transpose()?;
    let members = api
        .list_members(user, gid, after, q.limit.unwrap_or(1))
        .await
        .map_err(ce)?;
    Ok(Json(
        members
            .iter()
            .map(|m| MemberObject::new(m, false))
            .collect(),
    ))
}

/// Get one member.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/members/{user_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("user_id" = String, Path, description = "User snowflake"),
    ),
    responses((status = 200, body = MemberObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_member(
    p: Principal,
    Api(api): Api,
    Path((guild_id, user_id)): Path<(String, String)>,
) -> ApiResult<Json<MemberObject>> {
    let actor = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let uid: UserId = id("user_id", &user_id)?;
    let m = api.get_member(actor, gid, uid).await.map_err(ce)?;
    Ok(Json(MemberObject::new(&m, false)))
}

/// Modify nickname, roles or timeout of a member, subject to the role hierarchy.
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/members/{user_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("user_id" = String, Path, description = "User snowflake"),
    ),
    request_body = ModifyMemberBody,
    responses((status = 200, body = MemberObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn modify_member(
    p: Principal,
    Api(api): Api,
    Path((guild_id, user_id)): Path<(String, String)>,
    Json(body): Json<ModifyMemberBody>,
) -> ApiResult<Json<MemberObject>> {
    let actor = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let uid: UserId = id("user_id", &user_id)?;
    let roles = match body.roles {
        None => None,
        Some(list) => Some(
            list.iter()
                .enumerate()
                .map(|(n, r)| id::<RoleId>(&format!("roles.{n}"), r))
                .collect::<Result<Vec<_>, _>>()?,
        ),
    };
    let timeout = match body.communication_disabled_until {
        None => None,
        Some(None) => Some(None),
        Some(Some(s)) => Some(Some(parse_iso8601(&s).ok_or_else(|| {
            HttpError(ApiError::invalid_form().field(
                "communication_disabled_until",
                "DATE_TIME_TYPE_PARSE",
                "Could not parse the timestamp.",
            ))
        })?)),
    };
    let patch = MemberPatch {
        nick: body.nick,
        roles,
        communication_disabled_until_ms: timeout,
    };
    let m = api.edit_member(actor, gid, uid, patch).await.map_err(ce)?;
    Ok(Json(MemberObject::new(&m, false)))
}

/// Remove a member from the guild (`KICK_MEMBERS`, hierarchy applies).
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/members/{user_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("user_id" = String, Path, description = "User snowflake"),
    ),
    responses((status = 204, description = "Kicked"), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn kick_member(
    p: Principal,
    Api(api): Api,
    Path((guild_id, user_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let actor = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let uid: UserId = id("user_id", &user_id)?;
    api.kick_member(actor, gid, uid).await.map_err(ce)?;
    Ok(StatusCode::NO_CONTENT)
}
