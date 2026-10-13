//! Member prune: preview the count, then remove inactive members (LNG-SSET-029).
//!
//! Discord shape: `GET /guilds/{id}/prune` counts, `POST` removes. A member is pruned when
//! they joined more than `days` ago, wrote no message in the server in that time, are not
//! the owner, have no role (or only roles listed in `include_roles`), and rank below the
//! caller. Needs `KICK_MEMBERS`; each removal dispatches `GUILD_MEMBER_REMOVE` and one
//! `MEMBER_PRUNE` audit entry records the count.

use super::util::{Api, actor, ce, id};
use crate::{HttpError, Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{
    Json, Router,
    extract::{Path, Query},
    routing::get,
};
use serde::{Deserialize, Serialize};
use sr_core::{ApiError, GuildId, RoleId};
use utoipa::ToSchema;

pub fn routes() -> Router<AppState> {
    Router::new().route("/guilds/{guild_id}/prune", get(prune_count).post(prune))
}

/// Default prune window, as Discord.
const DEFAULT_DAYS: u32 = 7;

#[derive(Debug, Serialize, ToSchema)]
pub struct PruneResult {
    /// Members that would be (preview) or were (execute) removed.
    pub pruned: usize,
}

#[derive(Debug, Deserialize)]
pub struct PruneCountQuery {
    /// 1 to 30, default 7.
    pub days: Option<u32>,
    /// Comma-separated role snowflakes whose holders may be pruned.
    pub include_roles: Option<String>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct PruneBody {
    /// 1 to 30, default 7.
    pub days: Option<u32>,
    /// Roles whose holders may be pruned; members with other roles are kept.
    #[serde(default)]
    pub include_roles: Vec<String>,
}

fn roles(raw: impl IntoIterator<Item = String>) -> ApiResult<Vec<RoleId>> {
    let mut out: Vec<RoleId> = raw
        .into_iter()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .enumerate()
        .map(|(n, s)| id::<RoleId>(&format!("include_roles.{n}"), &s))
        .collect::<Result<_, _>>()?;
    if out.len() > 250 {
        return Err(HttpError(ApiError::invalid_form().field(
            "include_roles",
            "BASE_TYPE_MAX_LENGTH",
            "Too many roles.",
        )));
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// How many members a prune would remove right now. Counts at most 10,000.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/prune",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("days" = Option<u32>, Query, description = "1 to 30, default 7"),
        ("include_roles" = Option<String>, Query, description = "Comma-separated role snowflakes"),
    ),
    responses((status = 200, body = PruneResult), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn prune_count(
    p: Principal,
    Api(api): Api,
    Path(guild_id): Path<String>,
    Query(q): Query<PruneCountQuery>,
) -> ApiResult<Json<PruneResult>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let include = roles(
        q.include_roles
            .unwrap_or_default()
            .split(',')
            .map(str::to_owned),
    )?;
    let pruned = api
        .prune_preview(user, gid, q.days.unwrap_or(DEFAULT_DAYS), include)
        .await
        .map_err(ce)?;
    Ok(Json(PruneResult { pruned }))
}

/// Remove inactive members (`KICK_MEMBERS`). Removes at most 10,000 per call; call again
/// for a bigger backlog.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/prune",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = PruneBody,
    responses((status = 200, body = PruneResult), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn prune(
    p: Principal,
    Api(api): Api,
    Path(guild_id): Path<String>,
    Json(body): Json<PruneBody>,
) -> ApiResult<Json<PruneResult>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let include = roles(body.include_roles)?;
    let pruned = api
        .prune(user, gid, body.days.unwrap_or(DEFAULT_DAYS), include)
        .await
        .map_err(ce)?;
    Ok(Json(PruneResult { pruned }))
}
