//! Server tags and their adoption (LNG-SSET-011/012).
//!
//! * `GET/PUT/DELETE /guilds/{id}/tag`: the guild's tag (`MANAGE_GUILD` to change).
//! * `GET/PUT /users/@me/primary-guild`: the tag the caller displays. Other users see
//!   it as `primary_guild` on the user object (messages, profiles) while the tag exists,
//!   members may display it and the caller is still a member.

use super::deps::{SettingsDeps, actor, id, se};
use crate::routes::Ext;
use crate::routes::guilds::safety::MfaGate;
use crate::routes::messages::error::ApiJson;
use crate::routes::social::model::PrimaryGuildObject;
use crate::{Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{Json, Router, extract::Path, http::StatusCode, routing::get};
use serde::{Deserialize, Serialize};
use sr_chat::settings::tags::GuildTag;
use sr_core::{ErrorCode, GuildId};
use utoipa::ToSchema;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/guilds/{guild_id}/tag",
            get(get_tag).put(put_tag).delete(delete_tag),
        )
        .route(
            "/users/@me/primary-guild",
            get(get_primary_guild).put(put_primary_guild),
        )
}

/// A guild's tag.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GuildTagObject {
    pub tag: String,
    /// Badge icon id (0 to 15).
    pub badge: u8,
    /// Badge colour, 24-bit RGB.
    pub color: u32,
    /// "Members can choose to display".
    pub members_can_display: bool,
}

impl GuildTagObject {
    fn of(t: &GuildTag) -> Self {
        Self {
            tag: t.tag.clone(),
            badge: t.badge,
            color: t.color,
            members_can_display: t.members_can_display,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PutGuildTagBody {
    /// 2 to 4 letters or digits.
    pub tag: String,
    #[serde(default)]
    pub badge: u8,
    #[serde(default)]
    pub color: u32,
    #[serde(default = "yes")]
    pub members_can_display: bool,
}

fn yes() -> bool {
    true
}

/// The guild's tag. Members read it; 404 while the guild has none.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/tag",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = GuildTagObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_tag(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<GuildTagObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    deps.guild_access(user, gid).await?;
    let tag = deps.service.guild_tag(gid).await.map_err(se)?;
    tag.map(|t| Json(GuildTagObject::of(&t)))
        .ok_or_else(|| ErrorCode::SrNotFound.into())
}

/// Set or replace the guild's tag (`MANAGE_GUILD`).
#[utoipa::path(
    put,
    path = "/guilds/{guild_id}/tag",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("X-Audit-Log-Reason" = Option<String>, Header, description = "Stored in the audit log"),
    ),
    request_body = PutGuildTagBody,
    responses((status = 200, body = GuildTagObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn put_tag(
    p: Principal,
    gate: MfaGate,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<PutGuildTagBody>,
) -> ApiResult<Json<GuildTagObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    acc.require_mfa(&gate, user, sr_core::permissions::Permissions::MANAGE_GUILD)
        .await?;
    let saved = deps
        .service
        .save_guild_tag(
            acc.perms,
            user,
            GuildTag {
                guild_id: gid,
                tag: body.tag,
                badge: body.badge,
                color: body.color,
                members_can_display: body.members_can_display,
            },
        )
        .await
        .map_err(se)?;
    Ok(Json(GuildTagObject::of(&saved)))
}

/// Remove the guild's tag (`MANAGE_GUILD`); members stop displaying it.
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/tag",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("X-Audit-Log-Reason" = Option<String>, Header, description = "Stored in the audit log"),
    ),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn delete_tag(
    p: Principal,
    gate: MfaGate,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
) -> ApiResult<StatusCode> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    acc.require_mfa(&gate, user, sr_core::permissions::Permissions::MANAGE_GUILD)
        .await?;
    deps.service
        .clear_guild_tag(acc.perms, user, gid)
        .await
        .map_err(se)?;
    Ok(StatusCode::NO_CONTENT)
}

/// The caller's adopted tag (`null` when none).
#[derive(Debug, Serialize, ToSchema)]
pub struct PrimaryGuildResponse {
    pub primary_guild: Option<PrimaryGuildObject>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PutPrimaryGuildBody {
    /// The server whose tag to display, or `null` to display none.
    pub guild_id: Option<String>,
}

/// The tag the caller displays.
#[utoipa::path(
    get,
    path = "/users/@me/primary-guild",
    responses((status = 200, body = PrimaryGuildResponse), (status = 401, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_primary_guild(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
) -> ApiResult<Json<PrimaryGuildResponse>> {
    let user = actor(&p)?;
    let mut current = deps.service.primary_guild(user).await.map_err(se)?;
    // A member who left no longer displays it.
    if let Some(pg) = &current
        && deps
            .chat
            .get_member(pg.guild_id, user)
            .await
            .map_err(super::deps::re)?
            .is_none()
    {
        current = None;
    }
    Ok(Json(PrimaryGuildResponse {
        primary_guild: current.as_ref().map(PrimaryGuildObject::of),
    }))
}

/// Adopt a server's tag (the caller must be a member and the server must allow it) or
/// clear it with `null`.
#[utoipa::path(
    put,
    path = "/users/@me/primary-guild",
    request_body = PutPrimaryGuildBody,
    responses((status = 200, body = PrimaryGuildResponse), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn put_primary_guild(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    ApiJson(body): ApiJson<PutPrimaryGuildBody>,
) -> ApiResult<Json<PrimaryGuildResponse>> {
    let user = actor(&p)?;
    let guild: Option<GuildId> = body
        .guild_id
        .as_deref()
        .map(|g| id("guild_id", g))
        .transpose()?;
    let is_member = match guild {
        Some(g) => deps
            .chat
            .get_member(g, user)
            .await
            .map_err(super::deps::re)?
            .is_some(),
        None => false,
    };
    let adopted = deps
        .service
        .adopt_tag(user, guild, is_member)
        .await
        .map_err(se)?;
    Ok(Json(PrimaryGuildResponse {
        primary_guild: adopted.as_ref().map(PrimaryGuildObject::of),
    }))
}
