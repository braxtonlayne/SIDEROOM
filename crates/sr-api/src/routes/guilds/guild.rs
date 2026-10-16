//! Guild create/get/modify/delete and the caller's guild list.

use super::dto::{
    CreateGuildBody, GuildObject, GuildPreviewObject, ModifyGuildBody, ModifySrProfileBody,
};
use super::util::{Api, actor, ce, id};
use crate::routes::settings::deps::{SettingsDeps, re, se};
use crate::routes::{MaybeExt, media::MediaDeps};
use crate::{AuthKind, Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{
    Json, Router,
    extract::{Path, Query},
    http::StatusCode,
    routing::{delete, get},
};
use sr_chat::admin_rules::GuildPatch;
use sr_chat::guild_rules::GuildBlueprint;
use sr_chat::guild_traits::GuildTrait;
use sr_chat::settings::error::SettingsError;
use sr_chat::settings::templates::{self, TemplateSnapshot};
use sr_core::{ChannelId, GuildId, UserId};
use sr_storage::media::MediaKind;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/guilds",
            axum::routing::post(create_guild).layer(crate::routes::media::body_limit()),
        )
        .route("/guilds/{guild_id}", get(get_guild).delete(delete_guild))
        .route(
            "/guilds/{guild_id}",
            axum::routing::patch(modify_guild).layer(crate::routes::media::body_limit()),
        )
        .route(
            "/guilds/{guild_id}/sr/profile",
            axum::routing::patch(modify_sr_profile),
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
    MaybeExt(media): MaybeExt<MediaDeps>,
    Json(body): Json<CreateGuildBody>,
) -> ApiResult<(StatusCode, Json<GuildObject>)> {
    let owner = actor(&p)?;
    let icon = body.icon.clone().filter(|i| !i.is_empty());
    if icon.is_some() && media.is_none() {
        tracing::error!("guild icon on create needs MediaDeps registered on AppState");
        return Err(sr_core::ErrorCode::SrInternal.into());
    }
    let key = body
        .template
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    let Some(key) = key else {
        let mut bp = api.create_guild(owner, &body.name).await.map_err(ce)?;
        set_initial_icon(&api, media.as_deref(), owner, &mut bp, icon).await?;
        return Ok(created(&bp));
    };
    let (snapshot, used_code) = resolve_template(settings.as_deref(), key).await?;
    let mut bp = api
        .create_guild_from_snapshot(owner, &body.name, &snapshot)
        .await
        .map_err(ce)?;
    set_initial_icon(&api, media.as_deref(), owner, &mut bp, icon).await?;
    count_template_use(settings.as_deref(), used_code).await;
    Ok(created(&bp))
}

/// J-06: `POST /guilds` takes the icon as a data URI (the `PATCH` validation) so the client
/// needs no second request. A bad image removes the server again and answers the error,
/// so a retry does not leave a duplicate behind.
async fn set_initial_icon(
    api: &std::sync::Arc<dyn super::backend::GuildBackend>,
    media: Option<&MediaDeps>,
    owner: UserId,
    bp: &mut GuildBlueprint,
    icon: Option<String>,
) -> ApiResult<()> {
    let (Some(uri), Some(media)) = (icon, media) else {
        return Ok(());
    };
    let gid = bp.guild.id;
    let hash = match media
        .store(MediaKind::GuildIcon, gid.get(), "icon", uri)
        .await
    {
        Ok(h) => h,
        Err(e) => {
            if let Err(d) = api.delete_guild(owner, gid).await {
                tracing::warn!(error = %d, "could not remove a server whose icon was refused");
            }
            return Err(e);
        }
    };
    let patch = GuildPatch {
        icon_key: Some(Some(hash.clone())),
        ..GuildPatch::default()
    };
    match api.edit_guild(owner, gid, patch).await {
        Ok(g) => {
            bp.guild = g;
            Ok(())
        }
        Err(e) => {
            media.remove(MediaKind::GuildIcon, gid.get(), &hash).await;
            Err(ce(e))
        }
    }
}

/// Edit the Side Room profile extras of a server: today its traits (J-07). `MANAGE_GUILD`;
/// emits `GUILD_UPDATE`.
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/sr/profile",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = ModifySrProfileBody,
    responses(
        (status = 200, body = GuildObject),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn modify_sr_profile(
    p: Principal,
    Api(api): Api,
    gate: super::safety::MfaGate,
    Path(guild_id): Path<String>,
    Json(body): Json<ModifySrProfileBody>,
) -> ApiResult<Json<GuildObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    gate.require_perm(
        &*api,
        user,
        gid,
        sr_core::permissions::Permissions::MANAGE_GUILD,
    )
    .await?;
    let patch = GuildPatch {
        traits: body
            .traits
            .map(|t| t.into_iter().map(GuildTrait::from).collect()),
        ..GuildPatch::default()
    };
    let g = api.edit_guild(user, gid, patch).await.map_err(ce)?;
    let (_, roles) = api.get_guild(user, gid).await.map_err(ce)?;
    Ok(Json(GuildObject::new(&g, Some(&roles), None)))
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

/// Query of `GET /guilds/{id}`.
#[derive(Debug, Default, serde::Deserialize)]
pub struct GetGuildQuery {
    pub with_counts: Option<bool>,
}

/// Get a guild (members only). With `with_counts=true` the object also carries
/// `approximate_member_count` and, for servers of up to 1000 members, `approximate_presence_count`.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("with_counts" = Option<bool>, Query, description = "Add the approximate member and online counts"),
    ),
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
    MaybeExt(settings): MaybeExt<SettingsDeps>,
    MaybeExt(messages): MaybeExt<crate::routes::messages::MessagesDeps>,
    Path(guild_id): Path<String>,
    Query(q): Query<GetGuildQuery>,
) -> ApiResult<Json<GuildObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let (g, roles) = api.get_guild(user, gid).await.map_err(ce)?;
    let mut out = GuildObject::new(&g, Some(&roles), None);
    if let Some(m) = messages {
        out.sr_max_upload_bytes = Some(m.effective_upload_limit(user, Some(gid)).await);
    }
    if q.with_counts == Some(true)
        && let Some(s) = settings
    {
        let members = s.chat.count_members(gid).await.map_err(re)?;
        out.approximate_member_count = Some(members);
        if members <= 1000
            && let Some(p) = s.presence()
        {
            let page = s.chat.page_members(gid, None, 1000).await.map_err(re)?;
            let ids: Vec<_> = page.iter().map(|m| m.user_id).collect();
            out.approximate_presence_count = Some(p.count_online(&ids));
        }
    }
    Ok(Json(out))
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
    gate.require_perm(
        &*api,
        user,
        gid,
        sr_core::permissions::Permissions::MANAGE_GUILD,
    )
    .await?;
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
    gate.require_owner(&*api, user, gid).await?;
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
