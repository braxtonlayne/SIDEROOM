//! Guild create/get/modify/delete and the caller's guild list.

use super::dto::{CreateGuildBody, GuildObject, GuildPreviewObject, ModifyGuildBody};
use super::util::{Api, actor, ce, id};
use crate::routes::settings::deps::{SettingsDeps, se};
use crate::routes::{MaybeExt, media::MediaDeps};
use crate::{AuthKind, Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{
    Json, Router,
    extract::Path,
    http::StatusCode,
    routing::{delete, get},
};
use sr_chat::admin_rules::GuildPatch;
use sr_chat::guild_rules::GuildBlueprint;
use sr_chat::settings::error::SettingsError;
use sr_chat::settings::templates::{self, TemplateSnapshot};
use sr_core::{ChannelId, GuildId, UserId};
use sr_storage::media::MediaKind;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/guilds", axum::routing::post(create_guild))
        .route("/guilds/{guild_id}", get(get_guild).delete(delete_guild))
        .route(
            "/guilds/{guild_id}",
            axum::routing::patch(modify_guild).layer(crate::routes::media::body_limit()),
        )
        .route("/users/@me/guilds", get(list_my_guilds))
        .route("/users/@me/guilds/{guild_id}", delete(leave_guild))
}

/// Create a guild with `@everyone`, two categories, `#general` and a voice channel, or
/// from a template (`template`: a built-in id or a template code) in one transaction.
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
    MaybeExt(settings): MaybeExt<SettingsDeps>,
    Json(body): Json<CreateGuildBody>,
) -> ApiResult<(StatusCode, Json<GuildObject>)> {
    let owner = actor(&p)?;
    let key = body
        .template
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    let Some(key) = key else {
        let bp = api.create_guild(owner, &body.name).await.map_err(ce)?;
        return Ok(created(&bp));
    };
    let (snapshot, used_code) = resolve_template(settings.as_deref(), key).await?;
    let bp = api
        .create_guild_from_snapshot(owner, &body.name, &snapshot)
        .await
        .map_err(ce)?;
    count_template_use(settings.as_deref(), used_code).await;
    Ok(created(&bp))
}

/// The snapshot behind `key`: a built-in template id, else a template code. The second
/// value is the code to count a use against (built-ins are not counted).
pub(crate) async fn resolve_template(
    settings: Option<&SettingsDeps>,
    key: &str,
) -> ApiResult<(TemplateSnapshot, Option<String>)> {
    if let Some(b) = templates::builtin(key) {
        return Ok((b.snapshot.clone(), None));
    }
    let Some(settings) = settings else {
        return Err(se(SettingsError::UnknownTemplate));
    };
    let t = settings.service.template_by_code(key).await.map_err(se)?;
    Ok((t.snapshot, Some(t.code)))
}

/// Count a use of a user template; a failing counter never fails the creation.
pub(crate) async fn count_template_use(settings: Option<&SettingsDeps>, code: Option<String>) {
    if let (Some(settings), Some(code)) = (settings, code)
        && let Err(e) = settings.service.use_template(&code).await
    {
        tracing::warn!(error = %e, "could not count a template use");
    }
}

/// The `201` answer for a new guild, with its roles and channels (and their overwrites).
pub(crate) fn created(bp: &GuildBlueprint) -> (StatusCode, Json<GuildObject>) {
    let channels: Vec<_> = bp
        .channels
        .iter()
        .map(|c| {
            let ow: Vec<_> = bp
                .overwrites
                .iter()
                .filter(|(ch, _)| *ch == c.id)
                .map(|(_, o)| *o)
                .collect();
            super::dto::ChannelObject::new(c, &ow)
        })
        .collect();
    let roles: Vec<_> = std::iter::once(bp.everyone_role.clone())
        .chain(bp.extra_roles.iter().cloned())
        .collect();
    (
        StatusCode::CREATED,
        Json(GuildObject::new(&bp.guild, Some(&roles), Some(&channels))),
    )
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
    gate: super::safety::MfaGate,
    MaybeExt(media): MaybeExt<MediaDeps>,
    Path(guild_id): Path<String>,
    Json(body): Json<ModifyGuildBody>,
) -> ApiResult<Json<GuildObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    gate.require(&*api, user, gid).await?;
    let system_channel_id = match body.system_channel_id {
        None => None,
        Some(None) => Some(None),
        Some(Some(s)) => Some(Some(id::<ChannelId>("system_channel_id", &s)?)),
    };
    let afk_channel_id = match body.afk_channel_id {
        None => None,
        Some(None) => Some(None),
        Some(Some(s)) => Some(Some(id::<ChannelId>("afk_channel_id", &s)?)),
    };
    let owner_id = body
        .owner_id
        .as_deref()
        .map(|o| id::<UserId>("owner_id", o))
        .transpose()?;

    // Images first: a bad upload fails the whole edit before anything is written.
    let wants_images = body.icon.is_some() || body.banner.is_some() || body.splash.is_some();
    let media = match (wants_images, media) {
        (true, None) => {
            tracing::error!("guild image edit needs MediaDeps registered on AppState");
            return Err(sr_core::ErrorCode::SrInternal.into());
        }
        (_, m) => m,
    };
    let before = if wants_images {
        Some(api.get_guild(user, gid).await.map_err(ce)?.0)
    } else {
        None
    };
    let mut stored: Vec<(MediaKind, String)> = Vec::new();
    let mut keys: [Option<Option<String>>; 3] = [None, None, None];
    let inputs = [
        (MediaKind::GuildIcon, "icon", body.icon),
        (MediaKind::GuildBanner, "banner", body.banner),
        (MediaKind::GuildSplash, "splash", body.splash),
    ];
    for (n, (kind, field, v)) in inputs.into_iter().enumerate() {
        match (v, &media) {
            (None, _) | (Some(Some(_)), None) => {}
            (Some(None), _) => keys[n] = Some(None),
            (Some(Some(uri)), Some(m)) => match m.store(kind, gid.get(), field, uri).await {
                Ok(h) => {
                    stored.push((kind, h.clone()));
                    keys[n] = Some(Some(h));
                }
                Err(e) => {
                    for (k, h) in &stored {
                        m.remove(*k, gid.get(), h).await;
                    }
                    return Err(e);
                }
            },
        }
    }
    let [icon_key, banner_key, splash_key] = keys;
    let patch = GuildPatch {
        name: body.name,
        description: body.description,
        system_channel_id,
        afk_channel_id,
        afk_timeout: body.afk_timeout,
        verification_level: body.verification_level,
        default_message_notifications: body.default_message_notifications,
        explicit_content_filter: body.explicit_content_filter,
        system_channel_flags: body.system_channel_flags,
        preferred_locale: body.preferred_locale,
        owner_id,
        icon_key,
        banner_key,
        splash_key,
        ..GuildPatch::default()
    };
    let g = match api.edit_guild(user, gid, patch).await {
        Ok(g) => g,
        Err(e) => {
            if let Some(m) = &media {
                for (k, h) in &stored {
                    m.remove(*k, gid.get(), h).await;
                }
            }
            return Err(ce(e));
        }
    };
    // Committed: delete the images this edit replaced or cleared.
    if let (Some(m), Some(b)) = (&media, &before) {
        for (kind, old, new) in [
            (MediaKind::GuildIcon, &b.icon_key, &g.icon_key),
            (MediaKind::GuildBanner, &b.banner_key, &g.banner_key),
            (MediaKind::GuildSplash, &b.splash_key, &g.splash_key),
        ] {
            if let Some(old) = old
                && Some(old) != new.as_ref()
            {
                m.remove(kind, gid.get(), old).await;
            }
        }
    }
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
    gate: super::safety::MfaGate,
    Path(guild_id): Path<String>,
) -> ApiResult<StatusCode> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    gate.require(&*api, user, gid).await?;
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
                is_staff_guild: g.is_staff_guild,
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
