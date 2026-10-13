//! Guild templates (LNG-SSET-070, open question 138): one per guild, code = 16
//! base62 characters. A template is a snapshot of roles, channels, overwrites
//! and a few settings; creating a server from one (`instantiate`) lands with the
//! create-server flow, not here.

use super::deps::{GuildAccess, SettingsDeps, actor, id, re, se};
use crate::routes::Ext;
use crate::routes::guilds::dto::{UserRef, iso8601};
use crate::routes::guilds::guild::{count_template_use, created, resolve_template};
use crate::routes::guilds::util::{Api, ce};
use crate::routes::messages::error::ApiJson;
use crate::{Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{
    Json, Router,
    extract::Path,
    routing::{get, put},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sr_chat::ChannelWithOverwrites;
use sr_chat::settings::templates::{
    GuildTemplate, SnapSettings, TemplateSnapshot, builtin_templates, snapshot_guild,
};
use sr_core::GuildId;
use utoipa::ToSchema;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/guilds/templates/builtin", get(list_builtin_templates))
        .route(
            "/guilds/templates/{code}",
            get(get_template).post(create_guild_from_template),
        )
        .route(
            "/guilds/{guild_id}/templates",
            get(list_guild_templates).post(create_template),
        )
        .route(
            "/guilds/{guild_id}/templates/{code}",
            put(sync_template)
                .patch(modify_template)
                .delete(delete_template),
        )
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TemplateObject {
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub usage_count: u32,
    pub creator_id: String,
    pub creator: UserRef,
    pub created_at: String,
    pub updated_at: String,
    pub source_guild_id: String,
    /// The snapshot: `settings`, `roles` and `channels` with local ids.
    #[schema(value_type = Object)]
    pub serialized_source_guild: Value,
    /// Whether the guild changed since the last sync (guild-scoped reads only).
    pub is_dirty: Option<bool>,
}

impl TemplateObject {
    fn of(
        t: &GuildTemplate,
        users: &std::collections::HashMap<sr_core::UserId, sr_chat::repo::UserSummary>,
        is_dirty: Option<bool>,
    ) -> Self {
        Self {
            code: t.code.clone(),
            name: t.name.clone(),
            description: t.description.clone(),
            usage_count: t.usage_count,
            creator_id: t.creator_id.to_string(),
            creator: UserRef::of(t.creator_id, users),
            created_at: iso8601(t.created_at_ms),
            updated_at: iso8601(t.updated_at_ms),
            source_guild_id: t.guild_id.to_string(),
            serialized_source_guild: serde_json::to_value(&t.snapshot).unwrap_or_default(),
            is_dirty,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTemplateBody {
    /// 1-100 characters.
    pub name: String,
    /// At most 120 characters.
    pub description: Option<String>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ModifyTemplateBody {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub description: Option<Option<String>>,
}

fn double_option<'de, D>(d: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(d).map(Some)
}

/// Snapshot the guild as it is now.
pub async fn current_snapshot(
    deps: &SettingsDeps,
    acc: &GuildAccess,
) -> ApiResult<TemplateSnapshot> {
    let gid = acc.guild.id;
    let channels = deps.chat.list_guild_channels(gid).await.map_err(re)?;
    let mut with = Vec::with_capacity(channels.len());
    for channel in channels {
        let overwrites = deps.chat.get_overwrites(channel.id).await.map_err(re)?;
        with.push(ChannelWithOverwrites {
            channel,
            overwrites,
        });
    }
    let mut snap = snapshot_guild(
        SnapSettings {
            preferred_locale: acc.guild.preferred_locale.clone(),
            verification_level: acc.guild.verification_level,
            default_message_notifications: acc.guild.default_message_notifications,
            explicit_content_filter: acc.guild.explicit_content_filter,
            system_channel_local_id: None,
        },
        &acc.roles,
        &with,
    );
    // `snapshot_guild` numbers the kept channels 1..n in input order.
    snap.settings.system_channel_local_id = acc.guild.system_channel_id.and_then(|sys| {
        with.iter()
            .filter(|c| !c.channel.kind.is_dm() && !c.channel.kind.is_thread())
            .position(|c| c.channel.id == sys)
            .and_then(|i| u32::try_from(i + 1).ok())
    });
    Ok(snap)
}

async fn object(
    deps: &SettingsDeps,
    t: &GuildTemplate,
    current: Option<&TemplateSnapshot>,
) -> ApiResult<TemplateObject> {
    let users = deps.users([t.creator_id]).await?;
    Ok(TemplateObject::of(
        t,
        &users,
        current.map(|c| t.is_dirty(c)),
    ))
}

/// A template shipped with the server.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BuiltinTemplateObject {
    /// Pass as `template` to `POST /guilds`, or as the path code of
    /// `POST /guilds/templates/{code}`.
    pub id: String,
    pub name: String,
    pub description: String,
    /// The snapshot: `settings`, `roles` and `channels` with local ids.
    #[schema(value_type = Object)]
    pub serialized_source_guild: Value,
}

/// The built-in templates of the Create a Server wizard (any signed-in user).
#[utoipa::path(
    get,
    path = "/guilds/templates/builtin",
    responses((status = 200, body = Vec<BuiltinTemplateObject>), (status = 401, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_builtin_templates(p: Principal) -> ApiResult<Json<Vec<BuiltinTemplateObject>>> {
    actor(&p)?;
    Ok(Json(
        builtin_templates()
            .iter()
            .map(|t| BuiltinTemplateObject {
                id: t.id.clone(),
                name: t.name.clone(),
                description: t.description.clone(),
                serialized_source_guild: serde_json::to_value(&t.snapshot).unwrap_or_default(),
            })
            .collect(),
    ))
}

/// Body of `POST /guilds/templates/{code}`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct GuildFromTemplateBody {
    /// 2-100 characters.
    pub name: String,
    /// Accepted for compatibility; icon upload arrives with the media pipeline.
    #[serde(default)]
    pub icon: Option<String>,
}

/// Create a server from a template (a built-in id or a template code) in one
/// transaction; the caller owns it. One `GUILD_CREATE`.
#[utoipa::path(
    post,
    path = "/guilds/templates/{code}",
    params(("code" = String, Path, description = "Built-in template id or template code")),
    request_body = GuildFromTemplateBody,
    responses(
        (status = 201, description = "The new guild with its roles and channels", body = crate::routes::guilds::dto::GuildObject),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody)
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn create_guild_from_template(
    p: Principal,
    Api(api): Api,
    Ext(deps): Ext<SettingsDeps>,
    Path(code): Path<String>,
    ApiJson(body): ApiJson<GuildFromTemplateBody>,
) -> ApiResult<(
    axum::http::StatusCode,
    Json<crate::routes::guilds::dto::GuildObject>,
)> {
    let owner = actor(&p)?;
    let (snapshot, used) = resolve_template(Some(&deps), code.trim()).await?;
    let bp = api
        .create_guild_from_snapshot(owner, &body.name, &snapshot)
        .await
        .map_err(ce)?;
    count_template_use(Some(&deps), used).await;
    Ok(created(&bp))
}

/// Preview a template by its code (any signed-in user).
#[utoipa::path(
    get,
    path = "/guilds/templates/{code}",
    params(("code" = String, Path, description = "Template code")),
    responses((status = 200, body = TemplateObject), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_template(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(code): Path<String>,
) -> ApiResult<Json<TemplateObject>> {
    actor(&p)?;
    let t = deps.service.template_by_code(&code).await.map_err(se)?;
    Ok(Json(object(&deps, &t, None).await?))
}

/// The guild's template, if it has one (`MANAGE_GUILD`).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/templates",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = Vec<TemplateObject>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_guild_templates(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<Vec<TemplateObject>>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    let Some(t) = deps
        .service
        .guild_template(acc.perms, gid)
        .await
        .map_err(se)?
    else {
        return Ok(Json(Vec::new()));
    };
    let snap = current_snapshot(&deps, &acc).await?;
    Ok(Json(vec![object(&deps, &t, Some(&snap)).await?]))
}

/// Create the guild's template (`MANAGE_GUILD`); a guild has at most one.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/templates",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = CreateTemplateBody,
    responses((status = 200, body = TemplateObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn create_template(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<CreateTemplateBody>,
) -> ApiResult<Json<TemplateObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    let snap = current_snapshot(&deps, &acc).await?;
    let t = deps
        .service
        .create_template(
            acc.perms,
            user,
            gid,
            &body.name,
            body.description.as_deref(),
            snap.clone(),
        )
        .await
        .map_err(se)?;
    Ok(Json(object(&deps, &t, Some(&snap)).await?))
}

/// Sync the template with the guild's current state (`MANAGE_GUILD`).
#[utoipa::path(
    put,
    path = "/guilds/{guild_id}/templates/{code}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("code" = String, Path, description = "Template code"),
    ),
    responses((status = 200, body = TemplateObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn sync_template(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path((guild_id, code)): Path<(String, String)>,
) -> ApiResult<Json<TemplateObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    let snap = current_snapshot(&deps, &acc).await?;
    let t = deps
        .service
        .sync_template(acc.perms, gid, &code, snap.clone())
        .await
        .map_err(se)?;
    Ok(Json(object(&deps, &t, Some(&snap)).await?))
}

/// Change the template's name or description (`MANAGE_GUILD`).
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/templates/{code}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("code" = String, Path, description = "Template code"),
    ),
    request_body = ModifyTemplateBody,
    responses((status = 200, body = TemplateObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn modify_template(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path((guild_id, code)): Path<(String, String)>,
    ApiJson(body): ApiJson<ModifyTemplateBody>,
) -> ApiResult<Json<TemplateObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    let t = deps
        .service
        .edit_template(
            acc.perms,
            gid,
            &code,
            body.name.as_deref(),
            body.description.as_ref().map(Option::as_deref),
        )
        .await
        .map_err(se)?;
    let snap = current_snapshot(&deps, &acc).await?;
    Ok(Json(object(&deps, &t, Some(&snap)).await?))
}

/// Delete the template (`MANAGE_GUILD`); returns it.
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/templates/{code}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("code" = String, Path, description = "Template code"),
    ),
    responses((status = 200, body = TemplateObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn delete_template(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path((guild_id, code)): Path<(String, String)>,
) -> ApiResult<Json<TemplateObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    let t = deps
        .service
        .delete_template(acc.perms, gid, &code)
        .await
        .map_err(se)?;
    Ok(Json(object(&deps, &t, None).await?))
}
