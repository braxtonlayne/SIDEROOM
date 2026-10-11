//! Guild bans: list, get, create, remove.

use super::dto::{BanBody, BanObject};
use super::util::{Api, actor, ce, id};
use crate::{HttpError, Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{
    Json, Router,
    extract::{Path, Query},
    http::{HeaderMap, StatusCode},
    routing::get,
};
use serde::Deserialize;
use sr_core::{ApiError, GuildId, UserId};

/// Largest `delete_message_seconds` Discord accepts (7 days).
const MAX_DELETE_SECONDS: u32 = 604_800;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/guilds/{guild_id}/bans", get(list_bans))
        .route(
            "/guilds/{guild_id}/bans/{user_id}",
            get(get_ban).put(create_ban).delete(remove_ban),
        )
}

#[derive(Debug, Deserialize)]
pub struct ListBansQuery {
    /// 1 to 1000, default 1000.
    pub limit: Option<usize>,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// List bans (`BAN_MEMBERS`), ordered by user id.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/bans",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("limit" = Option<usize>, Query, description = "1 to 1000, default 1000"),
        ("before" = Option<String>, Query, description = "User snowflake cursor"),
        ("after" = Option<String>, Query, description = "User snowflake cursor"),
    ),
    responses((status = 200, body = Vec<BanObject>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_bans(
    p: Principal,
    Api(api): Api,
    Path(guild_id): Path<String>,
    Query(q): Query<ListBansQuery>,
) -> ApiResult<Json<Vec<BanObject>>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let before = q
        .before
        .as_deref()
        .map(|s| id::<UserId>("before", s))
        .transpose()?;
    let after = q
        .after
        .as_deref()
        .map(|s| id::<UserId>("after", s))
        .transpose()?;
    let bans = api
        .list_bans(user, gid, before, after, q.limit.unwrap_or(1000))
        .await
        .map_err(ce)?;
    Ok(Json(bans.iter().map(BanObject::from).collect()))
}

/// Get one ban (`BAN_MEMBERS`).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/bans/{user_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("user_id" = String, Path, description = "User snowflake"),
    ),
    responses((status = 200, body = BanObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_ban(
    p: Principal,
    Api(api): Api,
    Path((guild_id, user_id)): Path<(String, String)>,
) -> ApiResult<Json<BanObject>> {
    let actor = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let uid: UserId = id("user_id", &user_id)?;
    let ban = api.get_ban(actor, gid, uid).await.map_err(ce)?;
    Ok(Json(BanObject::from(&ban)))
}

/// Ban a user (member or not). The reason comes from `X-Audit-Log-Reason` or the body.
#[utoipa::path(
    put,
    path = "/guilds/{guild_id}/bans/{user_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("user_id" = String, Path, description = "User snowflake"),
        ("X-Audit-Log-Reason" = Option<String>, Header, description = "Stored as the ban reason"),
    ),
    request_body(content = BanBody, description = "Optional"),
    responses((status = 204, description = "Banned"), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn create_ban(
    p: Principal,
    Api(api): Api,
    headers: HeaderMap,
    Path((guild_id, user_id)): Path<(String, String)>,
    body: Option<Json<BanBody>>,
) -> ApiResult<StatusCode> {
    let actor = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let uid: UserId = id("user_id", &user_id)?;
    let body = body.map(|b| b.0).unwrap_or_default();
    let too_big = body
        .delete_message_seconds
        .is_some_and(|s| s > MAX_DELETE_SECONDS)
        || body.delete_message_days.is_some_and(|d| d > 7);
    if too_big {
        return Err(HttpError(ApiError::invalid_form().field(
            "delete_message_seconds",
            "NUMBER_TYPE_MAX",
            "Must be 604800 or less.",
        )));
    }
    let header_reason = headers
        .get("x-audit-log-reason")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    api.ban_member(actor, gid, uid, header_reason.or(body.reason))
        .await
        .map_err(ce)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Remove a ban (`BAN_MEMBERS`).
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/bans/{user_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("user_id" = String, Path, description = "User snowflake"),
    ),
    responses((status = 204, description = "Unbanned"), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn remove_ban(
    p: Principal,
    Api(api): Api,
    Path((guild_id, user_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let actor = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let uid: UserId = id("user_id", &user_id)?;
    api.unban(actor, gid, uid).await.map_err(ce)?;
    Ok(StatusCode::NO_CONTENT)
}
