//! Channels, categories, bulk reorder and permission overwrites.

use super::dto::{
    ChannelObject, CreateChannelBody, ForumSettingsBody, ModifyChannelBody, OverwriteBody,
    ReorderChannelEntry,
};
use super::util::{Api, actor, ce, id, permissions};
use crate::routes::MaybeExt;
use crate::routes::threads::ThreadsDeps;
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
use sr_core::{ApiError, ChannelId, GuildId, Snowflake};
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
    Path(guild_id): Path<String>,
) -> ApiResult<Json<Vec<ChannelObject>>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let list = api.list_channels(user, gid).await.map_err(ce)?;
    Ok(Json(list.iter().map(ChannelObject::from).collect()))
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
    Path(guild_id): Path<String>,
    Json(body): Json<CreateChannelBody>,
) -> ApiResult<(StatusCode, Json<ChannelObject>)> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let kind = ChannelType::from_code(body.kind.unwrap_or(0))
        .ok_or_else(|| invalid("type", "ENUM_TYPE_INVALID", "Unknown channel type."))?;
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
    let view = api
        .create_channel(user, gid, input, overwrites)
        .await
        .map_err(ce)?;
    Ok((StatusCode::CREATED, Json(ChannelObject::from(&view))))
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
    Path(guild_id): Path<String>,
    Json(body): Json<Vec<ReorderChannelEntry>>,
) -> ApiResult<StatusCode> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
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
    Path(channel_id): Path<String>,
) -> ApiResult<Json<ChannelObject>> {
    let user = actor(&p)?;
    let cid: ChannelId = id("channel_id", &channel_id)?;
    let view = api.get_channel(user, cid).await.map_err(ce)?;
    Ok(Json(ChannelObject::from(&view)))
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
    MaybeExt(threads): MaybeExt<ThreadsDeps>,
    Path(channel_id): Path<String>,
    Json(body): Json<ModifyChannelBody>,
) -> ApiResult<Json<ChannelObject>> {
    let user = actor(&p)?;
    let cid: ChannelId = id("channel_id", &channel_id)?;
    // Threads are edited by the thread service (archive, lock, tags, pin, ...).
    if let Some(t) = thread_service(threads.as_ref(), cid).await? {
        let patch = thread_patch(&body)?;
        let obj = t.modify(user, cid, patch).await.map_err(ce)?;
        return Ok(Json(obj));
    }
    let patch = ChannelPatch {
        name: body.name,
        topic: body.topic,
        nsfw: body.nsfw,
        rate_limit_per_user: body.rate_limit_per_user,
        bitrate: body.bitrate,
        user_limit: body.user_limit,
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
    MaybeExt(threads): MaybeExt<ThreadsDeps>,
    Path(channel_id): Path<String>,
) -> ApiResult<Json<ChannelObject>> {
    let user = actor(&p)?;
    let cid: ChannelId = id("channel_id", &channel_id)?;
    if let Some(t) = thread_service(threads.as_ref(), cid).await? {
        let obj = t.delete(user, cid).await.map_err(ce)?;
        return Ok(Json(obj));
    }
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
    Path((channel_id, overwrite_id)): Path<(String, String)>,
    Json(body): Json<OverwriteBody>,
) -> ApiResult<StatusCode> {
    let user = actor(&p)?;
    let cid: ChannelId = id("channel_id", &channel_id)?;
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
    Path((channel_id, overwrite_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let user = actor(&p)?;
    let cid: ChannelId = id("channel_id", &channel_id)?;
    let target: Snowflake = id("overwrite_id", &overwrite_id)?;
    api.delete_overwrite(user, cid, target).await.map_err(ce)?;
    Ok(StatusCode::NO_CONTENT)
}
