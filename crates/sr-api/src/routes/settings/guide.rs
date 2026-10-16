//! Server Guide (LNG-SSET-066): `GET/PUT /guilds/{id}/sr/guide`.
//!
//! Members read the guide while the community setting `guide_enabled` is on; managers
//! always. `PUT` replaces the content (`MANAGE_GUILD`); items without an `id` get one.

use super::community::require_guild_channels;
use super::deps::{SettingsDeps, actor, id, se};
use crate::routes::Ext;
use crate::routes::guilds::safety::MfaGate;
use crate::routes::messages::error::ApiJson;
use crate::{Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{Json, Router, extract::Path, routing::get};
use serde::{Deserialize, Serialize};
use sr_chat::settings::guide::{Guide, GuideResource, GuideTodo};
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, ErrorCode, GuildId};
use utoipa::ToSchema;

pub fn routes() -> Router<AppState> {
    Router::new().route("/guilds/{guild_id}/sr/guide", get(get_guide).put(put_guide))
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GuideTodoDto {
    #[serde(default)]
    pub id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub channel_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GuideResourceDto {
    #[serde(default)]
    pub id: Option<String>,
    pub title: String,
    /// Markdown.
    pub content: String,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct PutGuideBody {
    #[serde(default)]
    pub welcome_message: Option<String>,
    #[serde(default)]
    pub todos: Vec<GuideTodoDto>,
    #[serde(default)]
    pub resources: Vec<GuideResourceDto>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GuideObject {
    pub guild_id: String,
    /// Whether members see the guide (the community setting).
    pub enabled: bool,
    pub welcome_message: Option<String>,
    pub todos: Vec<GuideTodoDto>,
    pub resources: Vec<GuideResourceDto>,
}

impl GuideObject {
    fn of(g: &Guide, enabled: bool) -> Self {
        Self {
            guild_id: g.guild_id.to_string(),
            enabled,
            welcome_message: g.welcome_message.clone(),
            todos: g
                .todos
                .iter()
                .map(|t| GuideTodoDto {
                    id: Some(t.id.to_string()),
                    title: t.title.clone(),
                    description: t.description.clone(),
                    channel_id: t.channel_id.map(|c| c.to_string()),
                })
                .collect(),
            resources: g
                .resources
                .iter()
                .map(|r| GuideResourceDto {
                    id: Some(r.id.to_string()),
                    title: r.title.clone(),
                    content: r.content.clone(),
                })
                .collect(),
        }
    }
}

/// The Server Guide. Members see it while it is enabled; managers always.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/sr/guide",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = GuideObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_guide(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<GuideObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    let manager = acc.perms.has(Permissions::MANAGE_GUILD);
    let enabled = deps
        .service
        .community(Permissions::MANAGE_GUILD, gid)
        .await
        .map_err(se)?
        .guide_enabled;
    if !enabled && !manager {
        return Err(ErrorCode::MissingPermissions.into());
    }
    let g = deps.service.guide(gid).await.map_err(se)?;
    Ok(Json(GuideObject::of(&g, enabled)))
}

/// Replace the Server Guide content (`MANAGE_GUILD`): a welcome message (300), up to
/// 5 to-dos and up to 10 resource pages.
#[utoipa::path(
    put,
    path = "/guilds/{guild_id}/sr/guide",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("X-Audit-Log-Reason" = Option<String>, Header, description = "Stored in the audit log"),
    ),
    request_body = PutGuideBody,
    responses((status = 200, body = GuideObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn put_guide(
    p: Principal,
    gate: MfaGate,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<PutGuideBody>,
) -> ApiResult<Json<GuideObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    acc.require_mfa(&gate, user, Permissions::MANAGE_GUILD)
        .await?;
    let mut todos = Vec::with_capacity(body.todos.len());
    for t in body.todos {
        todos.push(GuideTodo {
            id: match &t.id {
                Some(s) => id("todos", s)?,
                None => deps.service.new_id(),
            },
            title: t.title,
            description: t.description,
            channel_id: t
                .channel_id
                .as_deref()
                .map(|c| id::<ChannelId>("todos", c))
                .transpose()?,
        });
    }
    let channels: Vec<ChannelId> = todos.iter().filter_map(|t| t.channel_id).collect();
    require_guild_channels(&deps, gid, "todos", channels).await?;
    let mut resources = Vec::with_capacity(body.resources.len());
    for r in body.resources {
        resources.push(GuideResource {
            id: match &r.id {
                Some(s) => id("resources", s)?,
                None => deps.service.new_id(),
            },
            title: r.title,
            content: r.content,
        });
    }
    let saved = deps
        .service
        .save_guide(
            acc.perms,
            user,
            Guide {
                guild_id: gid,
                welcome_message: body.welcome_message,
                todos,
                resources,
            },
        )
        .await
        .map_err(se)?;
    let enabled = deps
        .service
        .community(acc.perms, gid)
        .await
        .map_err(se)?
        .guide_enabled;
    Ok(Json(GuideObject::of(&saved, enabled)))
}
