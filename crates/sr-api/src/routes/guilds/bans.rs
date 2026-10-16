//! Guild bans: list, get, create, remove, bulk ban (LNG-GLD-006).

use super::dto::{BanBody, BanObject};
use super::util::{Api, actor, ce, id, users_for};
use crate::{HttpError, Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{
    Json, Router,
    extract::{Path, Query},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use sr_core::permissions::Permissions;
use sr_core::{ApiError, ErrorCode, GuildId, UserId};
use utoipa::ToSchema;

/// Largest `delete_message_seconds` Discord accepts (7 days).
const MAX_DELETE_SECONDS: u32 = 604_800;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/guilds/{guild_id}/bans", get(list_bans))
        .route("/guilds/{guild_id}/bulk-ban", post(bulk_ban))
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
    let users = users_for(&*api, bans.iter().map(|b| b.user_id)).await?;
    Ok(Json(
        bans.iter().map(|b| BanObject::new(b, &users)).collect(),
    ))
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
    let users = users_for(&*api, [ban.user_id]).await?;
    Ok(Json(BanObject::new(&ban, &users)))
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
    gate: super::safety::MfaGate,
    headers: HeaderMap,
    Path((guild_id, user_id)): Path<(String, String)>,
    body: Option<Json<BanBody>>,
) -> ApiResult<StatusCode> {
    let actor = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    gate.require_perm(
        &*api,
        actor,
        gid,
        sr_core::permissions::Permissions::BAN_MEMBERS,
    )
    .await?;
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
    let delete_seconds = body
        .delete_message_seconds
        .or(body.delete_message_days.map(|d| d * 86_400))
        .unwrap_or(0);
    api.ban_member(
        actor,
        gid,
        uid,
        header_reason.or(body.reason),
        delete_seconds,
    )
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
    gate: super::safety::MfaGate,
    Path((guild_id, user_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let actor = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    gate.require_perm(
        &*api,
        actor,
        gid,
        sr_core::permissions::Permissions::BAN_MEMBERS,
    )
    .await?;
    let uid: UserId = id("user_id", &user_id)?;
    api.unban(actor, gid, uid).await.map_err(ce)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Most users one bulk ban may name (Discord: 200).
pub const MAX_BULK_BAN: usize = 200;

/// Body of `POST /guilds/{id}/bulk-ban`.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct BulkBanBody {
    /// Users to ban, 1 to 200 snowflakes.
    pub user_ids: Vec<String>,
    /// Seconds of message history to delete for every banned user, 0 to 604800.
    pub delete_message_seconds: Option<u32>,
    /// Convenience; Discord carries the reason in `X-Audit-Log-Reason`.
    pub reason: Option<String>,
}

/// Result of a bulk ban.
#[derive(Debug, Serialize, ToSchema)]
pub struct BulkBanResult {
    /// Users that are now banned.
    pub banned_users: Vec<String>,
    /// Users that could not be banned (hierarchy, owner, self or unknown).
    pub failed_users: Vec<String>,
}

/// Ban up to 200 users at once (LNG-GLD-006). Needs `BAN_MEMBERS` and `MANAGE_GUILD`
/// like Discord; each user is subject to the same hierarchy rules as a single ban and
/// the ones that fail are listed rather than failing the request. Every banned user
/// produces the usual `GUILD_BAN_ADD`, audit entry and member removal.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/bulk-ban",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("X-Audit-Log-Reason" = Option<String>, Header, description = "Stored as the ban reason"),
    ),
    request_body = BulkBanBody,
    responses((status = 200, body = BulkBanResult), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn bulk_ban(
    p: Principal,
    Api(api): Api,
    gate: super::safety::MfaGate,
    headers: HeaderMap,
    Path(guild_id): Path<String>,
    Json(body): Json<BulkBanBody>,
) -> ApiResult<Json<BulkBanResult>> {
    let actor = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    gate.require_perm(
        &*api,
        actor,
        gid,
        sr_core::permissions::Permissions::BAN_MEMBERS
            | sr_core::permissions::Permissions::MANAGE_GUILD,
    )
    .await?;
    if body.user_ids.is_empty() || body.user_ids.len() > MAX_BULK_BAN {
        return Err(HttpError(ApiError::invalid_form().field(
            "user_ids",
            "BASE_TYPE_BAD_LENGTH",
            "Must be between 1 and 200 in length.",
        )));
    }
    if body
        .delete_message_seconds
        .is_some_and(|s| s > MAX_DELETE_SECONDS)
    {
        return Err(HttpError(ApiError::invalid_form().field(
            "delete_message_seconds",
            "NUMBER_TYPE_MAX",
            "Must be 604800 or less.",
        )));
    }
    let mut targets: Vec<UserId> = Vec::with_capacity(body.user_ids.len());
    for (n, raw) in body.user_ids.iter().enumerate() {
        let uid: UserId = id(&format!("user_ids.{n}"), raw)?;
        if !targets.contains(&uid) {
            targets.push(uid);
        }
    }
    // Both permissions, checked up front so a mixed batch never half-applies for a
    // moderator who may not bulk ban at all.
    let mine = api.list_my_guilds(actor).await.map_err(ce)?;
    let Some((_, perms)) = mine.iter().find(|(g, _)| g.id == gid) else {
        return Err(HttpError(ApiError::new(ErrorCode::MissingAccess)));
    };
    if !perms.has(Permissions::BAN_MEMBERS) || !perms.has(Permissions::MANAGE_GUILD) {
        return Err(HttpError(ApiError::new(ErrorCode::MissingPermissions)));
    }
    let reason = headers
        .get("x-audit-log-reason")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
        .or(body.reason);
    let seconds = body.delete_message_seconds.unwrap_or(0);
    let (mut banned, mut failed) = (Vec::new(), Vec::new());
    for uid in targets {
        match api
            .ban_member(actor, gid, uid, reason.clone(), seconds)
            .await
        {
            Ok(()) => banned.push(uid.to_string()),
            Err(_) => failed.push(uid.to_string()),
        }
    }
    if banned.is_empty() {
        // Discord answers 500000 when nobody could be banned.
        return Err(HttpError(ApiError::with_message(
            ErrorCode::MissingPermissions,
            "None of these users could be banned.",
        )));
    }
    Ok(Json(BulkBanResult {
        banned_users: banned,
        failed_users: failed,
    }))
}
