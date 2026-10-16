//! Create, edit, delete and bulk-delete (LNG-MSG-002/004/005/006/011, LNG-MSG-020 events).

use super::attachments::{finalize, load_upload};
use super::deps::MessagesDeps;
use super::error::{ApiJson, MsgResult, invalid, parse_id};
use super::model::{
    AllowedMentionsBody, BulkDeleteBody, CreateMessageBody, EditMessageBody, MessageObject,
};
use super::render::{actor, event_payload, render_one};
use super::store::UploadState;
use crate::routes::guilds::safety::MfaGate;
use crate::routes::settings::{AutoModGateHandle, MessageCheck, alert_execution_payload};
use crate::routes::{Ext, MaybeExt};
use crate::{Principal, openapi::ErrorBody};
use axum::{Json, extract::Path, http::StatusCode};
use sr_chat::message_flags;
use sr_chat::message_rules::{AllowedMentions, EditMessageInput};
use sr_chat::settings::audit::AuditAction;
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
    MaybeExt(gate): MaybeExt<AutoModGateHandle>,
    MaybeExt(settings): MaybeExt<crate::routes::settings::SettingsDeps>,
    Path(channel_id): Path<String>,
    ApiJson(body): ApiJson<CreateMessageBody>,
) -> MsgResult<Json<MessageObject>> {
    let author = actor(&p);
    let channel_id: ChannelId = parse_id(&channel_id, ErrorCode::UnknownChannel)?;

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

    // AutoMod (LNG-SSET-059/060): a blocking rule rejects the message before it is
    // stored (Discord error 200000). A failing check lets the message through and
    // is logged: AutoMod is a filter, not a security boundary, and must not take
    // chat down with it.
    let mut verdict = None;
    if let (Some(gate), Some(guild)) = (&gate, acc.channel.guild_id) {
        let check = MessageCheck {
            guild,
            channel: channel_id,
            author,
            content: body.content.clone().unwrap_or_default(),
            ..MessageCheck::default()
        };
        match gate.0.evaluate(&check).await {
            Ok(v) if v.block.is_some() => {
                publish_alerts(&deps, guild, &v, None).await;
                return Err(ErrorCode::AutoModBlocked.into());
            }
            Ok(v) => verdict = Some(v),
            Err(e) => tracing::warn!(status = e.0.status(), "AutoMod check failed; allowing"),
        }
    }

    let flags_in = body.flags.unwrap_or(0);

    // Attachments: must be the caller's uploads in this channel, not yet used.
    let mut uploads = Vec::new();
    let refs: Vec<&super::model::AttachmentRef> = body.attachments.iter().flatten().collect();
    for r in &refs {
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
    super::rich_create::apply_voice_meta(&mut uploads, &refs);
    if flags_in & message_flags::IS_VOICE_MESSAGE != 0 {
        super::rich_create::validate_voice(&uploads)?;
    }

    // Replies may tolerate a missing target.
    let mut reply_to = None;
    let mut forward = None;
    if let Some(r) = body
        .message_reference
        .as_ref()
        .filter(|r| r.kind == Some(1))
    {
        forward = Some(super::rich_create::plan_forward(&deps, author, r).await?);
    } else if let Some(r) = &body.message_reference {
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

    if let Some(a) = &body.allowed_mentions {
        super::rich_create::validate_allowed_mentions(a)?;
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
    let mut embeds = body.embeds.clone().unwrap_or_default();
    sr_chat::embed_rules::validate_embeds(&mut embeds)?;
    let poll = super::rich_create::plan_poll(&deps, body.poll.as_ref())?;
    let sticker_ids = super::rich_create::resolve_stickers(
        &deps,
        acc.channel.guild_id,
        body.sticker_ids.as_ref(),
    )
    .await?;
    let req = SendMessage {
        content: body.content.clone().unwrap_or_default(),
        nonce: body.nonce.clone(),
        reply_to,
        attachment_ids: uploads.iter().map(|(a, _)| a.id).collect(),
        embeds,
        components: body.components.clone().unwrap_or_default(),
        sticker_ids,
        poll,
        flags,
        allowed_mentions,
        forward,
    };
    // Decided before the send: storing the message reopens closed DMs.
    let dm_announce = deps.plan_dm_announce(&acc.channel, author).await;
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
    // The channel object must reach a recipient before the first message in it.
    deps.announce_dm(channel_id, &dm_announce).await;
    deps.publish(DomainEvent::MessageSent {
        guild: msg.guild_id,
        channel: channel_id,
        message_id: msg.id.get(),
        author,
        data,
    })
    .await;
    if let (Some(v), Some(guild)) = (&verdict, msg.guild_id) {
        publish_alerts(&deps, guild, v, Some(msg.id)).await;
    }
    // Announcement channels set to auto-publish send the message on to their followers
    // (LNG-CHN-004).
    if let Some(s) = &settings {
        super::crosspost::auto_publish(&deps, s, &msg).await;
    }
    if msg.poll.is_some() {
        deps.schedule_poll_finalize(&msg).await;
    }
    if !message_flags_suppress(msg.flags) {
        deps.schedule_unfurl(&msg).await;
    }
    let mut out = render_one(&deps, author, &msg).await?;
    out.nonce.clone_from(&body.nonce);
    Ok(Json(out))
}

fn message_flags_suppress(flags: u32) -> bool {
    MessagesDeps::embeds_suppressed(flags)
}

/// Announce each `send_alert_message` action of `verdict` as an
/// `AUTO_MODERATION_ACTION_EXECUTION` event (blocks are announced by the gate).
async fn publish_alerts(
    deps: &MessagesDeps,
    guild: sr_core::GuildId,
    verdict: &crate::routes::settings::AutoModVerdict,
    message: Option<MessageId>,
) {
    for alert in &verdict.alerts {
        deps.publish(DomainEvent::AutoModActionExecuted {
            guild,
            payload: alert_execution_payload(guild, alert, message),
        })
        .await;
    }
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
    deps.schedule_unfurl(&msg).await;
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
    gate: MfaGate,
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
    let author = deps
        .repo
        .get_message(channel_id, message_id)
        .await?
        .map(|m| m.author_id);
    if author.is_some_and(|a| a != user) {
        // Removing someone else's message needs `MANAGE_MESSAGES` (D-223).
        deps.require_mfa(&gate, user, guild).await?;
    }
    deps.delete(user, channel_id, message_id).await?;
    deps.publish(DomainEvent::MessageDeleted {
        guild,
        channel: channel_id,
        message_id: message_id.get(),
    })
    .await;
    // Discord logs a delete only when someone other than the author removed it.
    if let Some(author) = author.filter(|a| *a != user) {
        let mut options = serde_json::json!({ "channel_id": channel_id.to_string(), "count": 1 });
        // A webhook message is listed under its creator; the entry says which webhook (D-235).
        if let Some(store) = &deps.rich.store
            && let Ok(found) = store.webhook_authors(&[message_id]).await
            && let Some(w) = found.get(&message_id)
        {
            options["webhook_id"] = serde_json::json!(w.webhook_id.to_string());
        }
        deps.audit(
            guild,
            user,
            AuditAction::MessageDelete,
            Some(author.get()),
            options,
        )
        .await;
    }
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
    gate: MfaGate,
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
    deps.require_mfa(&gate, user, guild).await?;
    let plans = deps.bulk_delete(user, channel_id, &ids).await?;
    let message_ids: Vec<u64> = plans.iter().map(|d| d.message_id.get()).collect();
    let deleted = message_ids.len();
    deps.publish(DomainEvent::MessagesBulkDeleted {
        guild,
        channel: channel_id,
        message_ids,
    })
    .await;
    deps.audit(
        guild,
        user,
        AuditAction::MessageBulkDelete,
        Some(channel_id.get()),
        serde_json::json!({ "count": deleted }),
    )
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
