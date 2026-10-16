//! Guild members: list, get, modify, kick.

use super::dto::{MemberObject, ModifyMemberBody, nullable, parse_iso8601};
use super::util::{Api, actor, ce, id, users_for};
use crate::routes::{MaybeExt, media::MediaDeps};
use crate::{HttpError, Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, patch},
};
use serde::Deserialize;
use sr_chat::admin_repo::MemberFilter;
use sr_chat::admin_rules::MemberPatch;
use sr_chat::settings::repo::CommunityRepository;
use sr_core::permissions::Permissions;
use sr_core::{ApiError, ErrorCode, GuildId, RoleId, UserId};
use sr_storage::media::MediaKind;
use utoipa::ToSchema;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/guilds/{guild_id}/members", get(list_members))
        .route("/guilds/{guild_id}/members/search", get(search_members))
        .route(
            "/guilds/{guild_id}/members/@me",
            patch(modify_own_member).layer(crate::routes::media::body_limit()),
        )
        .route("/users/@me/guilds/{guild_id}/member", get(get_own_member))
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
    /// Case-insensitive text that must appear in the nickname, username or display name.
    pub query: Option<String>,
    /// Only members holding this role.
    pub role_id: Option<String>,
    /// `true`: only members with a signal (`new_account`, `rejoined`); needs `MANAGE_GUILD`.
    pub signals: Option<bool>,
}

/// Page through the members of a guild, ordered by user id.
///
/// `query` and `role_id` narrow the page to members matching a search text and/or holding a role.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/members",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("limit" = Option<usize>, Query, description = "1 to 1000, default 1"),
        ("after" = Option<String>, Query, description = "User snowflake cursor"),
        ("query" = Option<String>, Query, description = "Search text (at most 64 characters): matches nickname, username or display name, case-insensitive"),
        ("role_id" = Option<String>, Query, description = "Only members holding this role snowflake"),
        ("signals" = Option<bool>, Query, description = "true: only members with a signal (new_account, rejoined); needs MANAGE_GUILD"),
    ),
    responses((status = 200, body = Vec<MemberObject>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_members(
    State(state): State<AppState>,
    p: Principal,
    Api(api): Api,
    MaybeExt(settings): MaybeExt<crate::routes::settings::SettingsDeps>,
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
    let role = q
        .role_id
        .as_deref()
        .map(|r| id::<RoleId>("role_id", r))
        .transpose()?;
    let filter = MemberFilter {
        query: q.query,
        role,
    };
    let limit = q.limit.unwrap_or(1);
    let ctx = JoinCtx::new(&state, settings.as_deref(), gid).await;
    if q.signals == Some(true) {
        // A filter over a computed fact: read pages and keep the matches until the limit is met.
        let mut kept: Vec<MemberObject> = Vec::new();
        let mut cursor = after;
        for _ in 0..SIGNAL_PAGES {
            let page = api
                .list_members(user, gid, filter.clone(), cursor, SIGNAL_PAGE)
                .await
                .map_err(ce)?;
            let full = page.len() == SIGNAL_PAGE;
            cursor = page.last().map(|m| m.user_id);
            let (objs, permitted) = decorated(&*api, &ctx, user, gid, &page).await?;
            if !permitted {
                return Err(ErrorCode::MissingPermissions.into());
            }
            kept.extend(objs.into_iter().filter(has_signal));
            if kept.len() >= limit || !full {
                break;
            }
        }
        kept.truncate(limit);
        return Ok(Json(kept));
    }
    let members = api
        .list_members(user, gid, filter, after, limit)
        .await
        .map_err(ce)?;
    Ok(Json(decorated(&*api, &ctx, user, gid, &members).await?.0))
}

/// Pages read for a `signals=true` filter, and their size.
const SIGNAL_PAGES: usize = 5;
const SIGNAL_PAGE: usize = 200;

fn has_signal(o: &MemberObject) -> bool {
    o.sr_signals.as_ref().is_some_and(|s| !s.is_empty())
}

/// The clock and the server's `warn_new_account_days` for the member signals (D-240).
struct JoinCtx {
    now_ms: i64,
    warn_days: u32,
}

impl JoinCtx {
    async fn new(
        state: &AppState,
        settings: Option<&crate::routes::settings::SettingsDeps>,
        gid: GuildId,
    ) -> Self {
        let warn_days = match settings {
            Some(s) => s.service.repo().get_community(gid).await.map_or(7, |c| {
                if c.warn_new_account_days == 0 {
                    7
                } else {
                    c.warn_new_account_days
                }
            }),
            None => 7,
        };
        Self {
            now_ms: state.now_secs().saturating_mul(1000),
            warn_days,
        }
    }
}

/// Member objects with `sr_join` and `sr_signals` for a caller who holds `MANAGE_GUILD`, and
/// whether the caller does (`false` leaves the plain objects).
async fn decorated(
    api: &dyn super::backend::GuildBackend,
    ctx: &JoinCtx,
    actor: UserId,
    gid: GuildId,
    members: &[sr_chat::Member],
) -> ApiResult<(Vec<MemberObject>, bool)> {
    let users = users_for(api, members.iter().map(|m| m.user_id)).await?;
    let ids: Vec<UserId> = members.iter().map(|m| m.user_id).collect();
    let joins = api.member_joins(actor, gid, &ids).await.map_err(ce)?;
    let permitted = !joins.is_empty() || members.is_empty();
    let objs = members
        .iter()
        .map(|m| {
            let obj = MemberObject::new(m, false, &users);
            match joins.iter().find(|j| j.user_id == m.user_id) {
                Some(j) => obj.with_join(m.user_id, j, ctx.warn_days, ctx.now_ms),
                None => obj,
            }
        })
        .collect();
    Ok((objs, permitted))
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
    State(state): State<AppState>,
    p: Principal,
    Api(api): Api,
    MaybeExt(settings): MaybeExt<crate::routes::settings::SettingsDeps>,
    Path((guild_id, user_id)): Path<(String, String)>,
) -> ApiResult<Json<MemberObject>> {
    let actor = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let uid: UserId = id("user_id", &user_id)?;
    let m = api.get_member(actor, gid, uid).await.map_err(ce)?;
    let ctx = JoinCtx::new(&state, settings.as_deref(), gid).await;
    let (mut objs, _) = decorated(&*api, &ctx, actor, gid, std::slice::from_ref(&m)).await?;
    Ok(Json(objs.remove(0)))
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
    gate: super::safety::MfaGate,
    Path((guild_id, user_id)): Path<(String, String)>,
    Json(body): Json<ModifyMemberBody>,
) -> ApiResult<Json<MemberObject>> {
    let actor = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    if body.roles.is_some() {
        gate.require_perm(&*api, actor, gid, Permissions::MANAGE_ROLES)
            .await?;
    }
    if body.communication_disabled_until.is_some() {
        gate.require_perm(&*api, actor, gid, Permissions::MODERATE_MEMBERS)
            .await?;
    }
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
        ..MemberPatch::default()
    };
    let m = api.edit_member(actor, gid, uid, patch).await.map_err(ce)?;
    let users = users_for(&*api, [m.user_id]).await?;
    Ok(Json(MemberObject::new(&m, false, &users)))
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
    gate: super::safety::MfaGate,
    Path((guild_id, user_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let actor = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    gate.require_perm(
        &*api,
        actor,
        gid,
        sr_core::permissions::Permissions::KICK_MEMBERS,
    )
    .await?;
    let uid: UserId = id("user_id", &user_id)?;
    api.kick_member(actor, gid, uid).await.map_err(ce)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Query of `GET /guilds/{id}/members/search`.
#[derive(Debug, Deserialize)]
pub struct SearchMembersQuery {
    /// Text to look for in nicknames, usernames and display names (required).
    pub query: String,
    /// 1 to 1000, default 1.
    pub limit: Option<usize>,
    /// `true`: only members with a signal; needs `MANAGE_GUILD`.
    pub signals: Option<bool>,
}

/// Discord's member search: members whose nickname, username or display name contains
/// `query` (case-insensitive), ordered by user id.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/members/search",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("query" = String, Query, description = "Search text, 1 to 64 characters"),
        ("limit" = Option<usize>, Query, description = "1 to 1000, default 1"),
        ("signals" = Option<bool>, Query, description = "true: only members with a signal (new_account, rejoined); needs MANAGE_GUILD"),
    ),
    responses((status = 200, body = Vec<MemberObject>), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn search_members(
    State(state): State<AppState>,
    p: Principal,
    Api(api): Api,
    MaybeExt(settings): MaybeExt<crate::routes::settings::SettingsDeps>,
    Path(guild_id): Path<String>,
    Query(q): Query<SearchMembersQuery>,
) -> ApiResult<Json<Vec<MemberObject>>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    if q.query.trim().is_empty() {
        return Err(HttpError(ApiError::invalid_form().field(
            "query",
            "BASE_TYPE_REQUIRED",
            "This field is required.",
        )));
    }
    let filter = MemberFilter {
        query: Some(q.query),
        role: None,
    };
    let limit = q.limit.unwrap_or(1);
    let ctx = JoinCtx::new(&state, settings.as_deref(), gid).await;
    if q.signals == Some(true) {
        let page = api
            .list_members(user, gid, filter, None, SIGNAL_PAGE.max(limit))
            .await
            .map_err(ce)?;
        let (objs, permitted) = decorated(&*api, &ctx, user, gid, &page).await?;
        if !permitted {
            return Err(ErrorCode::MissingPermissions.into());
        }
        return Ok(Json(
            objs.into_iter().filter(has_signal).take(limit).collect(),
        ));
    }
    let members = api
        .list_members(user, gid, filter, None, limit)
        .await
        .map_err(ce)?;
    Ok(Json(decorated(&*api, &ctx, user, gid, &members).await?.0))
}

/// Body of `PATCH /guilds/{id}/members/@me`: the caller's per-server profile
/// (LNG-GLD-012). Absent fields stay; `null` clears one.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ModifyOwnMemberBody {
    /// Nickname, 1 to 32 characters (needs `CHANGE_NICKNAME`).
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub nick: Option<Option<String>>,
    /// Per-server avatar as an image data URI (PNG, JPEG, GIF or WebP, 8 MiB at most,
    /// cropped to a square).
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub avatar: Option<Option<String>>,
    /// Per-server banner as an image data URI.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub banner: Option<Option<String>>,
    /// Per-server bio, up to 190 characters.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub bio: Option<Option<String>>,
    /// Per-server pronouns, 40 characters at most (G-52).
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub pronouns: Option<Option<String>>,
    /// Per-server profile theme as two 0xRRGGBB colours `[primary, accent]`.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<Vec<u32>>)]
    pub theme_colors: Option<Option<[u32; 2]>>,
    /// Per-server profile effect slug.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub profile_effect: Option<Option<String>>,
    /// Per-server avatar decoration slug.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub avatar_decoration: Option<Option<String>>,
}

/// The caller's own member object in a server (`GET /users/@me/guilds/{id}/member`).
#[utoipa::path(
    get,
    path = "/users/@me/guilds/{guild_id}/member",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = MemberObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_own_member(
    p: Principal,
    Api(api): Api,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<MemberObject>> {
    let me = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let m = api.get_member(me, gid, me).await.map_err(ce)?;
    let users = users_for(&*api, [m.user_id]).await?;
    Ok(Json(MemberObject::new(&m, false, &users)))
}

/// Edit the caller\'s own per-server profile: nickname, avatar, banner and bio
/// (LNG-GLD-012). Images arrive as data URIs and are stored under the user with a hash that
/// is specific to this server, so the same picture in two servers stays independent.
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/members/@me",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = ModifyOwnMemberBody,
    responses((status = 200, body = MemberObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn modify_own_member(
    p: Principal,
    Api(api): Api,
    MaybeExt(media): MaybeExt<MediaDeps>,
    Path(guild_id): Path<String>,
    Json(body): Json<ModifyOwnMemberBody>,
) -> ApiResult<Json<MemberObject>> {
    let me = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let before = api.get_member(me, gid, me).await.map_err(ce)?;
    let wants_images = body.avatar.is_some() || body.banner.is_some();
    if wants_images && media.is_none() {
        tracing::error!("member image edit needs MediaDeps registered on AppState");
        return Err(ErrorCode::SrInternal.into());
    }
    // Images first: a bad upload fails the whole edit before anything is written.
    let mut stored: Vec<(MediaKind, String)> = Vec::new();
    let mut keys: [Option<Option<String>>; 2] = [None, None];
    let inputs = [
        (MediaKind::MemberAvatar, "avatar", body.avatar),
        (MediaKind::MemberBanner, "banner", body.banner),
    ];
    for (n, (kind, field, v)) in inputs.into_iter().enumerate() {
        match (v, &media) {
            (None, _) | (Some(Some(_)), None) => {}
            (Some(None), _) => keys[n] = Some(None),
            (Some(Some(uri)), Some(m)) => {
                match m
                    .store_scoped(kind, me.get(), Some(gid.get()), field, uri)
                    .await
                {
                    Ok(h) => {
                        stored.push((kind, h.clone()));
                        keys[n] = Some(Some(h));
                    }
                    Err(e) => {
                        for (k, h) in &stored {
                            m.remove(*k, me.get(), h).await;
                        }
                        return Err(e);
                    }
                }
            }
        }
    }
    let [avatar_key, banner_key] = keys;
    let patch = MemberPatch {
        nick: body.nick,
        avatar_key,
        banner_key,
        bio: body.bio,
        pronouns: body.pronouns,
        theme_colors: body.theme_colors,
        profile_effect: body.profile_effect,
        avatar_decoration: body.avatar_decoration,
        ..MemberPatch::default()
    };
    let m = match api.edit_member(me, gid, me, patch).await {
        Ok(m) => m,
        Err(e) => {
            if let Some(m) = &media {
                for (k, h) in &stored {
                    m.remove(*k, me.get(), h).await;
                }
            }
            return Err(ce(e));
        }
    };
    // Committed: delete the images this edit replaced or cleared.
    if let Some(mx) = &media {
        for (kind, old, new) in [
            (MediaKind::MemberAvatar, &before.avatar_key, &m.avatar_key),
            (MediaKind::MemberBanner, &before.banner_key, &m.banner_key),
        ] {
            if let Some(old) = old
                && Some(old) != new.as_ref()
            {
                mx.remove(kind, me.get(), old).await;
            }
        }
    }
    let users = users_for(&*api, [m.user_id]).await?;
    Ok(Json(MemberObject::new(&m, false, &users)))
}
