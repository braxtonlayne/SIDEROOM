//! Create, edit, delete and bulk-delete (LNG-MSG-002/004/005/006/011, LNG-MSG-020 events).

use super::attachments::{finalize, load_upload};
use super::deps::MessagesDeps;
use super::error::{ApiJson, MsgResult, invalid, parse_id};
use super::model::{
    AllowedMentionsBody, BulkDeleteBody, CreateMessageBody, EditMessageBody, MessageObject,
};
use super::render::{actor, event_payload, render_one};
use super::store::UploadState;
use crate::routes::Ext;
use crate::{Principal, openapi::ErrorBody};
use axum::{Json, extract::Path, http::StatusCode};
use sr_chat::message_flags;
use sr_chat::message_rules::{AllowedMentions, EditMessageInput};
use sr_chat::{ChatError, SendMessage};
use sr_core::{AttachmentId, ChannelId, ErrorCode, MessageId, RoleId, UserId};
use sr_events::DomainEvent;

const MAX_NONCE_LEN: usize = 25;

/// Create a message.
///
/// Mentions are parsed with `sr-markdown` and gated by permissions (`@everyone`,
/// unmentionable roles ping nobody). A repeated `nonce` from the same author in the
/// same channel within five minutes returns the original message instead of
/// creating another.
#[utoipa::path(
    post,
    path = "/channels/{channel_id}/messages",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    request_body = CreateMessageBody,
    responses(
        (status = 200, body = MessageObject),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 429, body = ErrorBody, description = "Slowmode; includes `retry_after` seconds"),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn create_message(
    p: Principal,
    Ext(deps): Ext<MessagesDeps>,
    Path(channel_id): Path<String>,
    ApiJson(body): ApiJson<CreateMessageBody>,
) -> MsgResult<Json<MessageObject>> {
    let author = actor(&p);
    let channel_id: ChannelId = parse_id(&channel_id, ErrorCode::UnknownChannel)?;

    if body.poll.is_some() {
        return Err(invalid(
            "poll",
            "NOT_SUPPORTED",
            "Polls are not available yet.",
        ));
    }
    if body.sticker_ids.as_ref().is_some_and(|s| !s.is_empty()) {
        return Err(invalid(
            "sticker_ids",
            "NOT_SUPPORTED",
            "Stickers are not available yet.",
        ));
    }
    if let Some(n) = &body.nonce
        && (n.is_empty() || n.chars().count() > MAX_NONCE_LEN)
    {
        return Err(invalid(
            "nonce",
            "BASE_TYPE_MAX_LENGTH",
            "Must be 25 or fewer in length.",
        ));
    }

    // Resolve the channel first so unknown/forbidden channels fail before anything else.
    let acc = deps.access(author, channel_id).await?;
    acc.require_view()?;

    // Nonce dedupe (LNG-MSG-002, open question 151): only with `enforce_nonce`
    // does a repeated nonce return the original message; otherwise a new message
    // is created.
    if let Some(nonce) = &body.nonce
        && body.enforce_nonce == Some(true)
        && let Some(existing) = deps.find_nonce(channel_id, author, nonce).await?
        && let Some(msg) = deps
            .repo
            .get_message(channel_id, existing)
            .await?
            .filter(sr_chat::Message::is_visible)
    {
        let mut out = render_one(&deps, author, &msg).await?;
        out.nonce = Some(nonce.clone());
        return Ok(Json(out));
    }

    // Attachments: must be the caller's uploads in this channel, not yet used.
    let mut uploads = Vec::new();
    for r in body.attachments.iter().flatten() {
        let raw = r
            .uploaded_filename
            .as_deref()
            .and_then(|f| f.split('/').next())
            .or(r.id.as_deref())
            .ok_or_else(|| {
                invalid(
                    "attachments",
                    "BASE_TYPE_REQUIRED",
                    "Each attachment needs `id` or `uploaded_filename`.",
                )
            })?;
        let id: AttachmentId = raw
            .parse()
            .map_err(|_| invalid("attachments", "BASE_TYPE_BAD_ID", "Unknown upload."))?;
        if uploads
            .iter()
            .any(|(a, _): &(super::store::Attachment, _)| a.id == id)
        {
            return Err(invalid("attachments", "DUPLICATE", "Duplicate attachment."));
        }
        let mut att = load_upload(&deps, author, channel_id, id)
            .await
            .map_err(|_| invalid("attachments", "BASE_TYPE_BAD_ID", "Unknown upload."))?;
        if att.state == UploadState::Attached {
            return Err(invalid(
                "attachments",
                "UPLOAD_USED",
                "This upload is already attached to a message.",
            ));
        }
        finalize(&deps, &mut att).await?;
        uploads.push((att, r.description.clone()));
    }

    // Replies may tolerate a missing target.
    let mut reply_to = None;
    if let Some(r) = &body.message_reference {
        let id: MessageId = r.message_id.parse().map_err(|_| {
            invalid(
                "message_reference.message_id",
                "NUMBER_TYPE_COERCE",
                "Value is not snowflake.",
            )
        })?;
        let exists = deps
            .repo
            .get_message(channel_id, id)
            .await?
            .is_some_and(|m| m.is_visible());
        if exists || r.fail_if_not_exists.unwrap_or(true) {
            reply_to = Some(id);
        }
    }

    let allowed_mentions = body
        .allowed_mentions
        .as_ref()
        .map(parse_allowed_mentions)
        .transpose()?;
    let mut flags = body.flags.unwrap_or(0);
    if body.tts == Some(true) {
        flags |= message_flags::TTS;
    }
    let embeds = body.embeds.clone().unwrap_or_default();
    if embeds.len() > sr_chat::message::MAX_EMBEDS {
        return Err(invalid(
            "embeds",
            "BASE_TYPE_MAX_LENGTH",
            "Must be 10 or fewer in length.",
        ));
    }
    let req = SendMessage {
        content: body.content.clone().unwrap_or_default(),
        nonce: body.nonce.clone(),
        reply_to,
        attachment_ids: uploads.iter().map(|(a, _)| a.id).collect(),
        embeds,
        components: body.components.clone().unwrap_or_default(),
        sticker_ids: Vec::new(),
        poll: None,
        flags,
        allowed_mentions,
    };
    let msg = deps.send(author, channel_id, req).await?;

    for (mut att, description) in uploads {
        att.state = UploadState::Attached;
        att.message_id = Some(msg.id);
        if description.is_some() {
            att.description = description;
        }
        deps.store.update_attachment(&att).await?;
    }
    if let Some(nonce) = &body.nonce {
        deps.remember_nonce(channel_id, author, nonce, msg.id)
            .await?;
    }

    let data = event_payload(&deps, &msg).await?;
    deps.publish(DomainEvent::MessageSent {
        guild: msg.guild_id,
        channel: channel_id,
        message_id: msg.id.get(),
        author,
        data,
    })
    .await;
    let mut out = render_one(&deps, author, &msg).await?;
    out.nonce.clone_from(&body.nonce);
    Ok(Json(out))
}

/// Edit a message (author only for content; embed suppression also by MANAGE_MESSAGES).
#[utoipa::path(
    patch,
    path = "/channels/{channel_id}/messages/{message_id}",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("message_id" = String, Path, description = "Message snowflake"),
    ),
    request_body = EditMessageBody,
    responses(
        (status = 200, body = MessageObject),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn edit_message(
    p: Principal,
    Ext(deps): Ext<MessagesDeps>,
    Path((channel_id, message_id)): Path<(String, String)>,
    ApiJson(body): ApiJson<EditMessageBody>,
) -> MsgResult<Json<MessageObject>> {
    let editor = actor(&p);
    let channel_id: ChannelId = parse_id(&channel_id, ErrorCode::UnknownChannel)?;
    let message_id: MessageId = parse_id(&message_id, ErrorCode::UnknownMessage)?;
    deps.edit(
        editor,
        channel_id,
        message_id,
        EditMessageInput {
            content: body.content,
            flags: body.flags,
        },
    )
    .await?;
    let msg = deps.visible_message(channel_id, message_id).await?;
    let data = event_payload(&deps, &msg).await?;
    deps.publish(DomainEvent::MessageEdited {
        guild: msg.guild_id,
        channel: channel_id,
        message_id: msg.id.get(),
        data,
    })
    .await;
    Ok(Json(render_one(&deps, editor, &msg).await?))
}

/// Delete a message (author, or MANAGE_MESSAGES). Soft delete: purged after 30 days.
#[utoipa::path(
    delete,
    path = "/channels/{channel_id}/messages/{message_id}",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("message_id" = String, Path, description = "Message snowflake"),
    ),
    responses(
        (status = 204),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn delete_message(
    p: Principal,
    Ext(deps): Ext<MessagesDeps>,
    Path((channel_id, message_id)): Path<(String, String)>,
) -> MsgResult<StatusCode> {
    let user = actor(&p);
    let channel_id: ChannelId = parse_id(&channel_id, ErrorCode::UnknownChannel)?;
    let message_id: MessageId = parse_id(&message_id, ErrorCode::UnknownMessage)?;
    let guild = deps
        .repo
        .get_channel(channel_id)
        .await?
        .and_then(|c| c.guild_id);
    deps.delete(user, channel_id, message_id).await?;
    deps.publish(DomainEvent::MessageDeleted {
        guild,
        channel: channel_id,
        message_id: message_id.get(),
    })
    .await;
    Ok(StatusCode::NO_CONTENT)
}

/// Bulk delete 2-100 messages younger than 14 days (MANAGE_MESSAGES).
#[utoipa::path(
    post,
    path = "/channels/{channel_id}/messages/bulk-delete",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    request_body = BulkDeleteBody,
    responses(
        (status = 204),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn bulk_delete(
    p: Principal,
    Ext(deps): Ext<MessagesDeps>,
    Path(channel_id): Path<String>,
    ApiJson(body): ApiJson<BulkDeleteBody>,
) -> MsgResult<StatusCode> {
    let user = actor(&p);
    let channel_id: ChannelId = parse_id(&channel_id, ErrorCode::UnknownChannel)?;
    // Reject absurd bodies before parsing every id.
    if body.messages.len() > deps.limits.bulk_delete_max * 2 {
        return Err(
            ChatError::invalid("messages", "BASE_TYPE_BAD_LENGTH", "Too many messages.").into(),
        );
    }
    let ids = body
        .messages
        .iter()
        .map(|s| s.parse::<MessageId>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| invalid("messages", "NUMBER_TYPE_COERCE", "Value is not snowflake."))?;
    let guild = deps
        .repo
        .get_channel(channel_id)
        .await?
        .and_then(|c| c.guild_id);
    let plans = deps.bulk_delete(user, channel_id, &ids).await?;
    let message_ids: Vec<u64> = plans.iter().map(|d| d.message_id.get()).collect();
    deps.publish(DomainEvent::MessagesBulkDeleted {
        guild,
        channel: channel_id,
        message_ids,
    })
    .await;
    Ok(StatusCode::NO_CONTENT)
}

/// Parse Discord's `allowed_mentions` object (open question 152).
fn parse_allowed_mentions(body: &AllowedMentionsBody) -> MsgResult<AllowedMentions> {
    let parse_ids = |field: &str, raw: &[String]| -> MsgResult<Vec<u64>> {
        raw.iter()
            .map(|s| {
                s.parse::<u64>()
                    .map_err(|_| invalid(field, "NUMBER_TYPE_COERCE", "Value is not snowflake."))
            })
            .collect()
    };
    for kind in &body.parse {
        if !matches!(kind.as_str(), "users" | "roles" | "everyone") {
            return Err(invalid(
                "allowed_mentions.parse",
                "BASE_TYPE_CHOICES",
                "Must be one of (users, roles, everyone).",
            ));
        }
    }
    let has = |k: &str| body.parse.iter().any(|p| p == k);
    Ok(AllowedMentions {
        parse_users: has("users"),
        parse_roles: has("roles"),
        parse_everyone: has("everyone"),
        users: parse_ids("allowed_mentions.users", &body.users)?
            .into_iter()
            .map(UserId::new)
            .collect(),
        roles: parse_ids("allowed_mentions.roles", &body.roles)?
            .into_iter()
            .map(RoleId::new)
            .collect(),
        replied_user: body.replied_user,
    })
}
