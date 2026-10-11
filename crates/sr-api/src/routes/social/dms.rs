//! Direct messages and group DMs (LNG-DM-*, Q112-Q114).
//!
//! `GET/PATCH/DELETE /channels/{id}` belong to the guilds router (route ownership,
//! open question 156), so the DM-specific edit and close live under
//! `/users/@me/channels/{id}`.

use super::deps::SocialDeps;
use super::error::{ApiJson, MsgResult, invalid};
use super::events::SocialEvent;
use super::model::{CreateDmBody, DmChannelObject, EditDmBody, MessageRequestBody};
use super::util::{actor, channel_id, user_id};
use crate::{Principal, openapi::ErrorBody, routes::Ext};
use axum::{Json, extract::Path, http::StatusCode};
use serde_json::{Value, json};
use sr_chat::social::dm::{DmChannel, DmKind, GroupEdit, RequestState};
use sr_chat::social::repo::DmRepository;
use sr_core::{ChannelId, ErrorCode, UserId};

/// Open conversations of the caller (hidden ones excluded), newest first.
#[utoipa::path(
    get,
    path = "/users/@me/channels",
    responses((status = 200, body = Vec<DmChannelObject>), (status = 401, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
) -> MsgResult<Json<Vec<DmChannelObject>>> {
    let me = actor(&p)?;
    let dms = deps.service.list_dms(me).await?;
    let users = deps
        .summaries(dms.iter().flat_map(|d| d.recipients.iter().copied()))
        .await?;
    let mut out = Vec::with_capacity(dms.len());
    for dm in &dms {
        let mut obj = DmChannelObject::new(dm, me, &users);
        if deps.service.request_state(me, dm.id).await? == Some(RequestState::Pending) {
            obj.is_message_request = Some(true);
        }
        out.push(obj);
    }
    Ok(Json(out))
}

/// Open (or find) a 1:1 DM with `recipient_id`, or create a group DM from
/// `recipients` (the caller's friends).
#[utoipa::path(
    post,
    path = "/users/@me/channels",
    request_body = CreateDmBody,
    responses(
        (status = 200, body = DmChannelObject),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody)
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn create(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    ApiJson(body): ApiJson<CreateDmBody>,
) -> MsgResult<Json<DmChannelObject>> {
    let me = actor(&p)?;
    let recipients = body
        .recipients
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|r| user_id(r))
        .collect::<MsgResult<Vec<UserId>>>()?;
    let single = match (&body.recipient_id, recipients.as_slice()) {
        (Some(id), []) => Some(user_id(id)?),
        (None, [one]) => Some(*one),
        (None, []) => {
            return Err(invalid(
                "recipient_id",
                "BASE_TYPE_REQUIRED",
                "recipient_id or recipients is required.",
            ));
        }
        _ => None,
    };
    if let Some(target) = single {
        let opened = deps.service.open_dm(me, target).await?;
        if opened.created {
            // A request-state DM stays invisible to the recipient until accepted.
            announce(&deps, "CHANNEL_CREATE", &opened.channel, &[me]).await?;
        }
        let users = deps
            .summaries(opened.channel.recipients.iter().copied())
            .await?;
        return Ok(Json(DmChannelObject::new(&opened.channel, me, &users)));
    }
    if body.recipient_id.is_some() {
        return Err(invalid(
            "recipients",
            "BASE_TYPE_BAD_VALUE",
            "Use either recipient_id or recipients.",
        ));
    }
    let dm = deps
        .service
        .create_group_dm(me, &recipients, body.name.as_deref())
        .await?;
    announce(&deps, "CHANNEL_CREATE", &dm, &dm.recipients).await?;
    let users = deps.summaries(dm.recipients.iter().copied()).await?;
    Ok(Json(DmChannelObject::new(&dm, me, &users)))
}

/// Rename a group DM, change its icon, or transfer ownership (owner only).
#[utoipa::path(
    patch,
    path = "/users/@me/channels/{channel_id}",
    params(("channel_id" = String, Path, description = "Group DM snowflake")),
    request_body = EditDmBody,
    responses((status = 200, body = DmChannelObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn edit(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    Path(channel): Path<String>,
    ApiJson(body): ApiJson<EditDmBody>,
) -> MsgResult<Json<DmChannelObject>> {
    let me = actor(&p)?;
    let cid = channel_id(&channel)?;
    let mut dm = None;
    if let Some(owner) = &body.owner_id {
        dm = Some(
            deps.service
                .transfer_owner(me, cid, user_id(owner)?)
                .await?,
        );
    }
    if body.name.is_some() || body.icon.is_some() {
        let edit = GroupEdit {
            name: body.name.clone(),
            icon_key: body.icon.clone(),
        };
        dm = Some(deps.service.edit_group(me, cid, &edit).await?);
    }
    let dm = match dm {
        Some(dm) => dm,
        None => fetch(&deps, me, cid).await?,
    };
    announce(&deps, "CHANNEL_UPDATE", &dm, &dm.recipients).await?;
    let users = deps.summaries(dm.recipients.iter().copied()).await?;
    Ok(Json(DmChannelObject::new(&dm, me, &users)))
}

/// Close a 1:1 DM (it reappears on the next message) or leave a group DM.
#[utoipa::path(
    delete,
    path = "/users/@me/channels/{channel_id}",
    params(("channel_id" = String, Path, description = "DM snowflake")),
    responses((status = 204, description = "Closed or left"), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn close(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    Path(channel): Path<String>,
) -> MsgResult<StatusCode> {
    let me = actor(&p)?;
    let cid = channel_id(&channel)?;
    let dm = fetch(&deps, me, cid).await?;
    if dm.kind == DmKind::Group {
        remove_member(&deps, me, &dm, me).await?;
    } else {
        deps.service.close_dm(me, cid).await?;
        announce(&deps, "CHANNEL_DELETE", &dm, &[me]).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Move a Message Request to `accepted`, `ignored` or `spam`.
#[utoipa::path(
    put,
    path = "/users/@me/channels/{channel_id}/request",
    params(("channel_id" = String, Path, description = "DM snowflake")),
    request_body = MessageRequestBody,
    responses((status = 204, description = "Updated"), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn respond_to_request(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    Path(channel): Path<String>,
    ApiJson(body): ApiJson<MessageRequestBody>,
) -> MsgResult<StatusCode> {
    let me = actor(&p)?;
    let state = RequestState::parse(&body.state).ok_or_else(|| {
        invalid(
            "state",
            "ENUM_TYPE_BASE_TYPE_BAD_VALUE",
            "Must be accepted, ignored or spam.",
        )
    })?;
    deps.service
        .respond_to_request(me, channel_id(&channel)?, state)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Add a friend to a group DM. Adding to a 1:1 DM spawns a new group and returns
/// it (200); adding to a group answers 204.
#[utoipa::path(
    put,
    path = "/channels/{channel_id}/recipients/{user_id}",
    params(
        ("channel_id" = String, Path, description = "DM snowflake"),
        ("user_id" = String, Path, description = "User to add")
    ),
    responses(
        (status = 200, body = DmChannelObject),
        (status = 204, description = "Added"),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody)
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn add_recipient(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    Path((channel, target)): Path<(String, String)>,
) -> MsgResult<axum::response::Response> {
    use axum::response::IntoResponse;
    let me = actor(&p)?;
    let cid = channel_id(&channel)?;
    let target = user_id(&target)?;
    let Some(dm) = deps.service.add_recipients(me, cid, &[target]).await? else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };
    if dm.id == cid {
        let user = deps.user_object(target).await?;
        let payload = json!({ "channel_id": cid.to_string(), "user": user });
        deps.emit(
            SocialEvent::new("CHANNEL_RECIPIENT_ADD", dm.recipients.clone(), payload)
                .for_channel(cid),
        )
        .await;
        // The newcomer needs the channel itself.
        announce(&deps, "CHANNEL_CREATE", &dm, &[target]).await?;
        return Ok(StatusCode::NO_CONTENT.into_response());
    }
    announce(&deps, "CHANNEL_CREATE", &dm, &dm.recipients).await?;
    let users = deps.summaries(dm.recipients.iter().copied()).await?;
    Ok(Json(DmChannelObject::new(&dm, me, &users)).into_response())
}

/// Remove a member from a group DM (owner), or leave it (yourself).
#[utoipa::path(
    delete,
    path = "/channels/{channel_id}/recipients/{user_id}",
    params(
        ("channel_id" = String, Path, description = "Group DM snowflake"),
        ("user_id" = String, Path, description = "User to remove")
    ),
    responses((status = 204, description = "Removed"), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn remove_recipient(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    Path((channel, target)): Path<(String, String)>,
) -> MsgResult<StatusCode> {
    let me = actor(&p)?;
    let dm = fetch(&deps, me, channel_id(&channel)?).await?;
    remove_member(&deps, me, &dm, user_id(&target)?).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn fetch(deps: &SocialDeps, me: UserId, cid: ChannelId) -> MsgResult<DmChannel> {
    let dm = deps
        .service
        .repo()
        .get_dm(cid)
        .await?
        .ok_or(ErrorCode::UnknownChannel)?;
    if dm.has(me) {
        Ok(dm)
    } else {
        Err(ErrorCode::MissingAccess.into())
    }
}

/// Kick or leave, then tell the removed user, the remaining members and (on an
/// ownership hand-over) the new group state.
async fn remove_member(
    deps: &SocialDeps,
    actor: UserId,
    before: &DmChannel,
    target: UserId,
) -> MsgResult<()> {
    let after = deps
        .service
        .remove_recipient(actor, before.id, target)
        .await?;
    announce(deps, "CHANNEL_DELETE", before, &[target]).await?;
    let Some(after) = after else {
        return Ok(());
    };
    let user = deps.user_object(target).await?;
    let payload = json!({ "channel_id": after.id.to_string(), "user": user });
    deps.emit(
        SocialEvent::new(
            "CHANNEL_RECIPIENT_REMOVE",
            after.recipients.clone(),
            payload,
        )
        .for_channel(after.id),
    )
    .await;
    if after.owner != before.owner {
        announce(deps, "CHANNEL_UPDATE", &after, &after.recipients).await?;
    }
    Ok(())
}

/// One event per addressed user: the channel object differs by viewer (the viewer
/// is not listed among the recipients).
async fn announce(
    deps: &SocialDeps,
    name: &'static str,
    dm: &DmChannel,
    to: &[UserId],
) -> MsgResult<()> {
    let users = deps.summaries(dm.recipients.iter().copied()).await?;
    for user in to {
        let payload: Value =
            serde_json::to_value(DmChannelObject::new(dm, *user, &users)).unwrap_or(Value::Null);
        deps.emit(SocialEvent::new(name, vec![*user], payload).for_channel(dm.id))
            .await;
    }
    Ok(())
}
