//! Channels, categories, bulk reorder and permission overwrites.

use super::backend::GuildBackend;
use super::dto::{
    ChannelObject, CreateChannelBody, ForumSettingsBody, ModifyChannelBody, OverwriteBody,
    ReorderChannelEntry,
};
use super::util::{Api, actor, ce, id, permissions};
use crate::routes::MaybeExt;
use crate::routes::threads::ThreadsDeps;
use crate::routes::watchparty::WatchPartyDeps;
use crate::{HttpError, Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{
    Json, Router,
    extract::Path,
    http::StatusCode,
    routing::{get, put},
};
use sr_chat::ChannelType;
use sr_chat::admin_rules::ChannelMove;
use sr_chat::channel_rules::{ChannelPatch, NewChannel};
use sr_chat::threads::{DefaultReaction, ForumPatch, TagInput, ThreadPatch};
use sr_core::permissions::{Overwrite, OverwriteKind};
use sr_core::{ApiError, ChannelId, GuildId, Snowflake, UserId};
use sr_watchparty::PartyError;
use sr_watchparty::model::{CreateInput, SettingsInput};
use std::sync::Arc;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/guilds/{guild_id}/channels",
            get(list_channels)
                .post(create_channel)
                .patch(reorder_channels),
        )
        .route(
            "/channels/{channel_id}",
            get(get_channel)
                .patch(modify_channel)
                .delete(delete_channel),
        )
        .route(
            "/channels/{channel_id}/permissions/{overwrite_id}",
            put(put_overwrite).delete(delete_overwrite),
        )
}

fn invalid(field: &str, code: &str, msg: &str) -> HttpError {
    HttpError(ApiError::invalid_form().field(field, code, msg))
}

fn overwrite_from(target: Snowflake, body: &OverwriteBody) -> Result<Overwrite, HttpError> {
    let kind = match body.kind {
        0 => OverwriteKind::Role,
        1 => OverwriteKind::Member,
        _ => {
            return Err(invalid(
                "type",
                "ENUM_TYPE_INVALID",
                "Must be 0 (role) or 1 (member).",
            ));
        }
    };
    let allow = body
        .allow
        .as_deref()
        .map(|s| permissions("allow", s))
        .transpose()?
        .unwrap_or_default();
    let deny = body
        .deny
        .as_deref()
        .map(|s| permissions("deny", s))
        .transpose()?
        .unwrap_or_default();
    Ok(Overwrite {
        id: target,
        kind,
        allow,
        deny,
    })
}

fn parent(
    field: &str,
    raw: Option<Option<String>>,
) -> Result<Option<Option<ChannelId>>, HttpError> {
    Ok(match raw {
        None => None,
        Some(None) => Some(None),
        Some(Some(s)) => Some(Some(id::<ChannelId>(field, &s)?)),
    })
}

fn opt_id(field: &str, raw: Option<&str>) -> Result<Option<u64>, HttpError> {
    raw.map(|s| id::<u64>(field, s)).transpose()
}

/// Forum, media and thread-default settings of a create/modify body.
fn forum_patch(f: &ForumSettingsBody, flags: Option<u32>) -> Result<ForumPatch, HttpError> {
    let mut tags = None;
    if let Some(list) = &f.available_tags {
        let mut out = Vec::with_capacity(list.len());
        for (n, t) in list.iter().enumerate() {
            out.push(TagInput {
                id: opt_id(&format!("available_tags.{n}.id"), t.id.as_deref())?,
                name: t.name.clone(),
                moderated: t.moderated,
                emoji_id: opt_id(
                    &format!("available_tags.{n}.emoji_id"),
                    t.emoji_id.as_deref(),
                )?,
                emoji_name: t.emoji_name.clone(),
            });
        }
        tags = Some(out);
    }
    let reaction = match &f.default_reaction_emoji {
        None => None,
        Some(None) => Some(None),
        Some(Some(r)) => Some(Some(DefaultReaction {
            emoji_id: opt_id("default_reaction_emoji.emoji_id", r.emoji_id.as_deref())?,
            emoji_name: r.emoji_name.clone(),
        })),
    };
    Ok(ForumPatch {
        available_tags: tags,
        default_reaction_emoji: reaction,
        default_sort_order: f.default_sort_order,
        default_forum_layout: f.default_forum_layout,
        default_thread_rate_limit: f.default_thread_rate_limit_per_user,
        default_auto_archive_duration: f.default_auto_archive_duration,
        flags,
    })
}

/// A `PATCH` of a thread.
fn thread_patch(b: &ModifyChannelBody) -> Result<ThreadPatch, HttpError> {
    let tags = match &b.applied_tags {
        None => None,
        Some(list) => Some(
            list.iter()
                .enumerate()
                .map(|(n, s)| id::<u64>(&format!("applied_tags.{n}"), s))
                .collect::<Result<Vec<_>, _>>()?,
        ),
    };
    Ok(ThreadPatch {
        name: b.name.clone(),
        archived: b.archived,
        auto_archive_duration: b.auto_archive_duration,
        locked: b.locked,
        invitable: b.invitable,
        rate_limit_per_user: b.rate_limit_per_user,
        applied_tags: tags,
        flags: b.flags,
    })
}

/// Whether `cid` is a thread and the thread service is wired.
async fn thread_service(
    threads: Option<&Arc<ThreadsDeps>>,
    cid: ChannelId,
) -> Result<Option<Arc<ThreadsDeps>>, HttpError> {
    let Some(t) = threads else {
        return Ok(None);
    };
    match t.ops.kind_of(cid).await.map_err(ce)? {
        Some(k) if k.is_thread() => Ok(Some(t.clone())),
        _ => Ok(None),
    }
}

/// A thread edited or deleted by someone other than its creator is a moderator action
/// (`MANAGE_THREADS`, D-223): the server's 2FA requirement applies.
async fn thread_gate(
    gate: &super::safety::MfaGate,
    api: &dyn GuildBackend,
    user: UserId,
    cid: ChannelId,
) -> Result<(), HttpError> {
    let view = api.get_channel(user, cid).await.map_err(ce)?;
    if view.channel.owner_id == Some(user) {
        return Ok(());
    }
    gate.require_channel_perm(
        api,
        user,
        cid,
        sr_core::permissions::Permissions::MANAGE_THREADS,
        true,
    )
    .await
}

/// Channels of the guild that the caller can view.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/channels",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = Vec<ChannelObject>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_channels(
    p: Principal,
    Api(api): Api,
    MaybeExt(bridge): MaybeExt<sr_bridge::BridgeService>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<Vec<ChannelObject>>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let list = api.list_channels(user, gid).await.map_err(ce)?;
    let mut out: Vec<ChannelObject> = list.iter().map(ChannelObject::from).collect();
    // The bridge state rides on the channel payload, so the client draws its banner without another request.
    if let Some(b) = bridge {
        let g = i64::try_from(gid.get()).unwrap_or(0);
        let dirs = b.directions_for_guild(g).await.unwrap_or_default();
        let mode = if dirs.is_empty() {
            None
        } else {
            b.mode_of(g).await.ok().flatten()
        };
        for c in &mut out {
            if let Some(d) = c.id.parse::<i64>().ok().and_then(|i| dirs.get(&i)) {
                c.sr_bridge = Some(super::dto::BridgeChannelState {
                    direction: d.as_str().to_owned(),
                    mode: mode.clone().unwrap_or_else(|| "selective".into()),
                });
            }
        }
    }
    Ok(Json(out))
}

/// Create a channel or category (`type` 4). Needs `MANAGE_CHANNELS`.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/channels",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = CreateChannelBody,
    responses((status = 201, body = ChannelObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn create_channel(
    p: Principal,
    Api(api): Api,
    gate: super::safety::MfaGate,
    MaybeExt(parties): MaybeExt<WatchPartyDeps>,
    Path(guild_id): Path<String>,
    Json(body): Json<CreateChannelBody>,
) -> ApiResult<(StatusCode, Json<ChannelObject>)> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    gate.require_perm(
        &*api,
        user,
        gid,
        sr_core::permissions::Permissions::MANAGE_CHANNELS,
    )
    .await?;
    let kind = ChannelType::from_code(body.kind.unwrap_or(0))
        .ok_or_else(|| invalid("type", "ENUM_TYPE_INVALID", "Unknown channel type."))?;
    let room = if body.sr_watch_party_room == Some(true) {
        Some(room_request(&p, &body, kind, parties)?)
    } else {
        None
    };
    let mut input = NewChannel::new(kind, body.name);
    input.topic = body.topic;
    input.position = body.position;
    input.parent_id = body
        .parent_id
        .as_deref()
        .map(|s| id::<ChannelId>("parent_id", s))
        .transpose()?;
    input.nsfw = body.nsfw.unwrap_or(false);
    input.rate_limit_per_user = body.rate_limit_per_user.unwrap_or(0);
    input.bitrate = body.bitrate;
    input.user_limit = body.user_limit;
    refuse_region(&body.rtc_region)?;
    input.video_quality_mode = body.video_quality_mode;
    input.forum = forum_patch(&body.forum, body.flags)?;
    let mut overwrites = Vec::with_capacity(body.permission_overwrites.len());
    for (n, o) in body.permission_overwrites.iter().enumerate() {
        let raw = o.id.as_deref().ok_or_else(|| {
            invalid(
                &format!("permission_overwrites.{n}.id"),
                "BASE_TYPE_REQUIRED",
                "This field is required",
            )
        })?;
        let target: Snowflake = id(&format!("permission_overwrites.{n}.id"), raw)?;
        overwrites.push(overwrite_from(target, o)?);
    }
    let mut view = api
        .create_channel(user, gid, input, overwrites)
        .await
        .map_err(ce)?;
    if let Some(room) = room {
        // The preset is one request: when the party cannot start (unknown or unplayable title, age gate,
        // the server's party limit), the new channel is taken back so nothing half-made is left behind.
        if let Err(e) = start_room(&room, view.channel.id).await {
            let _ = api.delete_channel(user, view.channel.id).await;
            return Err(e);
        }
        // The CHANNEL_CREATE went out before the party existed: send CHANNEL_UPDATE so live clients
        // see the room flag too (the repo reads it with the channel).
        if let Ok(v) = api
            .edit_channel(
                user,
                view.channel.id,
                ChannelPatch::default(),
                ChannelMove::default(),
            )
            .await
        {
            view = v;
        }
        view.channel.watch_party_room = true;
    }
    Ok((StatusCode::CREATED, Json(ChannelObject::from(&view))))
}

/// A validated "Watch party room" request.
struct RoomRequest {
    deps: Arc<WatchPartyDeps>,
    user: i64,
    /// `None`: an empty room (D-227).
    title_id: Option<i64>,
    episode_id: Option<i64>,
}

fn room_request(
    p: &Principal,
    body: &CreateChannelBody,
    kind: ChannelType,
    parties: Option<Arc<WatchPartyDeps>>,
) -> Result<RoomRequest, HttpError> {
    if kind != ChannelType::Voice {
        return Err(invalid(
            "sr_watch_party_room",
            "ENUM_TYPE_INVALID",
            "A watch party room is a voice channel (type 2).",
        ));
    }
    let snowflake = |field: &str, raw: &str| {
        raw.parse::<i64>()
            .ok()
            .filter(|v| *v > 0)
            .ok_or_else(|| invalid(field, "NUMBER_TYPE_COERCE", "Not a valid snowflake."))
    };
    // D-227: the title is optional; without one the room starts empty and its first host picks the media.
    let title_id = body
        .sr_watch_party_title_id
        .as_deref()
        .map(|raw| snowflake("sr_watch_party_title_id", raw))
        .transpose()?;
    let episode_id = body
        .sr_watch_party_episode_id
        .as_deref()
        .map(|raw| snowflake("sr_watch_party_episode_id", raw))
        .transpose()?;
    if title_id.is_none() && episode_id.is_some() {
        return Err(invalid(
            "sr_watch_party_episode_id",
            "BASE_TYPE_REQUIRED",
            "An episode needs sr_watch_party_title_id.",
        ));
    }
    // People only: a bot has no account to host a party for.
    if matches!(p.kind, crate::AuthKind::Bot { .. }) {
        return Err(sr_core::ErrorCode::BotsCannotUseThisEndpoint.into());
    }
    let Some(deps) = parties else {
        return Err(invalid(
            "sr_watch_party_room",
            "BASE_TYPE_INVALID",
            "Watch parties are not available.",
        ));
    };
    Ok(RoomRequest {
        deps,
        user: p.user_id,
        title_id,
        episode_id,
    })
}

fn party_error(e: PartyError) -> HttpError {
    use sr_core::ErrorCode as C;
    HttpError(match e {
        PartyError::NotFound(w) => ApiError::with_message(C::SrNotFound, format!("Unknown {w}")),
        PartyError::Forbidden => ApiError::new(C::MissingPermissions),
        PartyError::AgeConfirmationRequired => ApiError::new(C::SrAgeConfirmationRequired),
        PartyError::Invalid(m) => ApiError::with_message(C::InvalidFormBody, m),
        PartyError::Conflict(m) => ApiError::with_message(C::SrConflict, m),
        PartyError::RateLimited => ApiError::new(C::TooManyRequests),
        other => {
            tracing::error!(error = %other, "watch party room failed");
            ApiError::new(C::SrInternal)
        }
    })
}

/// Start the persistent party of a new room. The creator is not a participant afterwards: the first member
/// to join an empty persistent party becomes its host.
async fn start_room(room: &RoomRequest, channel: ChannelId) -> Result<(), HttpError> {
    let svc = &room.deps.service;
    let viewer = svc.viewer(room.user).await;
    let party = svc
        .create(
            viewer,
            CreateInput {
                external: None,
                empty: room.title_id.is_none(),
                title_id: room.title_id.unwrap_or(0),
                tv_channel_id: None,
                episode_id: room.episode_id,
                variant_id: None,
                channel_id: i64::try_from(channel.get()).ok(),
                start_ms: None,
                settings: Some(SettingsInput {
                    persistent: Some(true),
                    ..SettingsInput::default()
                }),
            },
        )
        .await
        .map_err(party_error)?;
    svc.leave(viewer, party.id).await.map_err(party_error)?;
    Ok(())
}

/// Bulk update `position` and `parent_id`. Needs `MANAGE_CHANNELS`.
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/channels",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = Vec<ReorderChannelEntry>,
    responses((status = 204, description = "Reordered"), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn reorder_channels(
    p: Principal,
    Api(api): Api,
    gate: super::safety::MfaGate,
    Path(guild_id): Path<String>,
    Json(body): Json<Vec<ReorderChannelEntry>>,
) -> ApiResult<StatusCode> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    gate.require_perm(
        &*api,
        user,
        gid,
        sr_core::permissions::Permissions::MANAGE_CHANNELS,
    )
    .await?;
    let mut moves = Vec::with_capacity(body.len());
    for (n, e) in body.into_iter().enumerate() {
        let cid: ChannelId = id(&format!("{n}.id"), &e.id)?;
        moves.push((
            cid,
            ChannelMove {
                position: e.position,
                parent_id: parent(&format!("{n}.parent_id"), e.parent_id)?,
                lock_permissions: e.lock_permissions,
            },
        ));
    }
    api.reorder_channels(user, gid, moves).await.map_err(ce)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Get a guild channel the caller can view.
#[utoipa::path(
    get,
    path = "/channels/{channel_id}",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 200, body = ChannelObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_channel(
    p: Principal,
    Api(api): Api,
    MaybeExt(bridge): MaybeExt<sr_bridge::BridgeService>,
    Path(channel_id): Path<String>,
) -> ApiResult<Json<ChannelObject>> {
    let user = actor(&p)?;
    let cid: ChannelId = id("channel_id", &channel_id)?;
    let view = api.get_channel(user, cid).await.map_err(ce)?;
    let mut out = ChannelObject::from(&view);
    if let Some(b) = bridge
        && let Ok(Some(d)) = b
            .direction_for_channel(i64::try_from(cid.get()).unwrap_or(0))
            .await
    {
        let mode = b
            .mode_for_channel(i64::try_from(cid.get()).unwrap_or(0))
            .await
            .ok()
            .flatten();
        out.sr_bridge = Some(super::dto::BridgeChannelState {
            direction: d.as_str().to_owned(),
            mode: mode.unwrap_or_else(|| "selective".into()),
        });
    }
    Ok(Json(out))
}

/// Modify a guild channel. Needs `MANAGE_CHANNELS`.
#[utoipa::path(
    patch,
    path = "/channels/{channel_id}",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    request_body = ModifyChannelBody,
    responses((status = 200, body = ChannelObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn modify_channel(
    p: Principal,
    Api(api): Api,
    gate: super::safety::MfaGate,
    MaybeExt(threads): MaybeExt<ThreadsDeps>,
    Path(channel_id): Path<String>,
    Json(body): Json<ModifyChannelBody>,
) -> ApiResult<Json<ChannelObject>> {
    let user = actor(&p)?;
    let cid: ChannelId = id("channel_id", &channel_id)?;
    // Threads are edited by the thread service (archive, lock, tags, pin, ...).
    if let Some(t) = thread_service(threads.as_ref(), cid).await? {
        thread_gate(&gate, &*api, user, cid).await?;
        let patch = thread_patch(&body)?;
        let obj = t.modify(user, cid, patch).await.map_err(ce)?;
        return Ok(Json(obj));
    }
    gate.require_channel_perm(
        &*api,
        user,
        cid,
        sr_core::permissions::Permissions::MANAGE_CHANNELS,
        false,
    )
    .await?;
    let kind = match body.kind {
        None => None,
        Some(code) => Some(ChannelType::from_code(code).ok_or_else(|| {
            HttpError(ApiError::invalid_form().field(
                "type",
                "ENUM_TYPE_INVALID",
                "Not a channel type.",
            ))
        })?),
    };
    refuse_region(&body.rtc_region)?;
    let patch = ChannelPatch {
        kind,
        name: body.name,
        topic: body.topic,
        nsfw: body.nsfw,
        rate_limit_per_user: body.rate_limit_per_user,
        bitrate: body.bitrate,
        user_limit: body.user_limit,
        video_quality_mode: body.video_quality_mode,
        music_allowed: body.sr_music_allowed,
        music_bitrate_kbps: body.sr_music_bitrate_kbps,
        forum: forum_patch(&body.forum, body.flags)?,
    };
    let mv = ChannelMove {
        position: body.position,
        parent_id: parent("parent_id", body.parent_id)?,
        lock_permissions: body.lock_permissions,
    };
    let view = api.edit_channel(user, cid, patch, mv).await.map_err(ce)?;
    Ok(Json(ChannelObject::from(&view)))
}

/// Delete a guild channel; a deleted category orphans its children.
#[utoipa::path(
    delete,
    path = "/channels/{channel_id}",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 200, body = ChannelObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn delete_channel(
    p: Principal,
    Api(api): Api,
    gate: super::safety::MfaGate,
    MaybeExt(threads): MaybeExt<ThreadsDeps>,
    Path(channel_id): Path<String>,
) -> ApiResult<Json<ChannelObject>> {
    let user = actor(&p)?;
    let cid: ChannelId = id("channel_id", &channel_id)?;
    if let Some(t) = thread_service(threads.as_ref(), cid).await? {
        thread_gate(&gate, &*api, user, cid).await?;
        let obj = t.delete(user, cid).await.map_err(ce)?;
        return Ok(Json(obj));
    }
    gate.require_channel_perm(
        &*api,
        user,
        cid,
        sr_core::permissions::Permissions::MANAGE_CHANNELS,
        false,
    )
    .await?;
    let view = api.delete_channel(user, cid).await.map_err(ce)?;
    Ok(Json(ChannelObject::from(&view)))
}

/// Create or replace a permission overwrite (`type` 0 role, 1 member).
#[utoipa::path(
    put,
    path = "/channels/{channel_id}/permissions/{overwrite_id}",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("overwrite_id" = String, Path, description = "Role or user snowflake"),
    ),
    request_body = OverwriteBody,
    responses((status = 204, description = "Saved"), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn put_overwrite(
    p: Principal,
    Api(api): Api,
    gate: super::safety::MfaGate,
    Path((channel_id, overwrite_id)): Path<(String, String)>,
    Json(body): Json<OverwriteBody>,
) -> ApiResult<StatusCode> {
    let user = actor(&p)?;
    let cid: ChannelId = id("channel_id", &channel_id)?;
    gate.require_channel_perm(
        &*api,
        user,
        cid,
        sr_core::permissions::Permissions::MANAGE_ROLES,
        true,
    )
    .await?;
    let target: Snowflake = id("overwrite_id", &overwrite_id)?;
    let ow = overwrite_from(target, &body)?;
    api.set_overwrite(user, cid, ow).await.map_err(ce)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Delete a permission overwrite.
#[utoipa::path(
    delete,
    path = "/channels/{channel_id}/permissions/{overwrite_id}",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("overwrite_id" = String, Path, description = "Role or user snowflake"),
    ),
    responses((status = 204, description = "Removed"), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn delete_overwrite(
    p: Principal,
    Api(api): Api,
    gate: super::safety::MfaGate,
    Path((channel_id, overwrite_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let user = actor(&p)?;
    let cid: ChannelId = id("channel_id", &channel_id)?;
    gate.require_channel_perm(
        &*api,
        user,
        cid,
        sr_core::permissions::Permissions::MANAGE_ROLES,
        true,
    )
    .await?;
    let target: Snowflake = id("overwrite_id", &overwrite_id)?;
    api.delete_overwrite(user, cid, target).await.map_err(ce)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `rtc_region` is read-only here: the installation has one voice region (J-11), so only `null`
/// (no preference) is accepted.
fn refuse_region(region: &Option<Option<String>>) -> ApiResult<()> {
    if matches!(region, Some(Some(_))) {
        return Err(invalid(
            "rtc_region",
            "CHANNEL_REGION_UNSUPPORTED",
            "This installation has one voice region.",
        ));
    }
    Ok(())
}
