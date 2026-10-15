//! Starting threads and forum posts.

use super::deps::ThreadsDeps;
use super::dto::{StartFromMessageBody, StartThreadBody, ThreadResponse};
use crate::routes::Ext;
use crate::routes::guilds::dto::ChannelObject;
use crate::routes::messages::MessagesDeps;
use crate::routes::messages::error::{ApiJson, MsgResult, invalid, parse_id};
use crate::routes::messages::render::{actor, event_payload, render_one};
use crate::{Principal, openapi::ErrorBody};
use axum::{Json, extract::Path, http::StatusCode};
use sr_chat::threads::{ForumPost, StartThread, StartedThread};
use sr_chat::{ChannelType, Message, SendMessage, message_flags};
use sr_core::{ChannelId, ErrorCode, MessageId};
use sr_events::DomainEvent;

/// Announce a stored system message as `MESSAGE_CREATE`.
async fn announce_message(m: &MessagesDeps, msg: &Message) -> MsgResult<()> {
    let data = event_payload(m, msg).await?;
    m.publish(DomainEvent::MessageSent {
        guild: msg.guild_id,
        channel: msg.channel_id,
        message_id: msg.id.get(),
        author: msg.author_id,
        data,
    })
    .await;
    Ok(())
}

/// `THREAD_CREATE`, the creator's own membership, then the system messages.
pub(crate) async fn announce_started(
    t: &ThreadsDeps,
    m: &MessagesDeps,
    s: &StartedThread,
) -> MsgResult<()> {
    t.publish_created(&s.view.channel).await;
    if let Some(member) = &s.view.member {
        t.publish_self_member(&s.view.channel, member).await;
    }
    for msg in &s.messages {
        announce_message(m, msg).await?;
    }
    Ok(())
}

/// Start a thread from a message. The thread's id is the message's id; a message can
/// have one thread. Needs `CREATE_PUBLIC_THREADS`.
#[utoipa::path(
    post,
    path = "/channels/{channel_id}/messages/{message_id}/threads",
    params(
        ("channel_id" = String, Path, description = "Parent channel snowflake"),
        ("message_id" = String, Path, description = "Message snowflake"),
    ),
    request_body = StartFromMessageBody,
    responses(
        (status = 201, body = ThreadResponse),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn start_from_message(
    p: Principal,
    Ext(t): Ext<ThreadsDeps>,
    Ext(m): Ext<MessagesDeps>,
    Path((channel_id, message_id)): Path<(String, String)>,
    ApiJson(body): ApiJson<StartFromMessageBody>,
) -> MsgResult<(StatusCode, Json<ThreadResponse>)> {
    let user = actor(&p);
    let parent: ChannelId = parse_id(&channel_id, ErrorCode::UnknownChannel)?;
    let mid: MessageId = parse_id(&message_id, ErrorCode::UnknownMessage)?;
    let started = t
        .ops
        .start_from_message(
            user,
            parent,
            mid,
            StartThread {
                name: body.name,
                auto_archive_duration: body.auto_archive_duration,
                rate_limit_per_user: body.rate_limit_per_user,
                ..StartThread::default()
            },
        )
        .await?;
    announce_started(&t, &m, &started).await?;
    let channel =
        ChannelObject::new(&started.view.channel, &[]).with_member(started.view.member.as_ref());
    Ok((
        StatusCode::CREATED,
        Json(ThreadResponse {
            channel,
            message: None,
        }),
    ))
}

fn thread_type(code: Option<i16>) -> MsgResult<Option<ChannelType>> {
    match code {
        None => Ok(None),
        Some(c) => match ChannelType::from_code(c) {
            Some(k) if k.is_thread() => Ok(Some(k)),
            _ => Err(invalid(
                "type",
                "ENUM_TYPE_INVALID",
                "Must be 10, 11 or 12.",
            )),
        },
    }
}

/// Start a thread without a message, or a post in a forum or media channel (then
/// `message` is required and becomes the first message).
#[utoipa::path(
    post,
    path = "/channels/{channel_id}/threads",
    params(("channel_id" = String, Path, description = "Parent channel snowflake")),
    request_body = StartThreadBody,
    responses(
        (status = 201, body = ThreadResponse),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn start_thread(
    p: Principal,
    Ext(t): Ext<ThreadsDeps>,
    Ext(m): Ext<MessagesDeps>,
    Path(channel_id): Path<String>,
    ApiJson(body): ApiJson<StartThreadBody>,
) -> MsgResult<(StatusCode, Json<ThreadResponse>)> {
    let user = actor(&p);
    let parent: ChannelId = parse_id(&channel_id, ErrorCode::UnknownChannel)?;
    let kind = thread_type(body.kind)?;
    let parent_kind = t
        .ops
        .kind_of(parent)
        .await?
        .ok_or(ErrorCode::UnknownChannel)?;

    if !parent_kind.is_forum_like() {
        let started = t
            .ops
            .start_standalone(
                user,
                parent,
                StartThread {
                    name: body.name,
                    auto_archive_duration: body.auto_archive_duration,
                    rate_limit_per_user: body.rate_limit_per_user,
                    kind,
                    invitable: body.invitable,
                },
            )
            .await?;
        announce_started(&t, &m, &started).await?;
        let channel = ChannelObject::new(&started.view.channel, &[])
            .with_member(started.view.member.as_ref());
        return Ok((
            StatusCode::CREATED,
            Json(ThreadResponse {
                channel,
                message: None,
            }),
        ));
    }

    // Forum and media posts: a thread whose first message is the starter.
    let msg_body = body.message.ok_or_else(|| {
        invalid(
            "message",
            "BASE_TYPE_REQUIRED",
            "A forum post needs a message.",
        )
    })?;
    if msg_body.attachments.as_ref().is_some_and(|a| !a.is_empty()) {
        return Err(invalid(
            "message.attachments",
            "NOT_SUPPORTED",
            "Attachments on forum posts are not available yet.",
        ));
    }
    if msg_body.poll.is_some() || msg_body.sticker_ids.as_ref().is_some_and(|s| !s.is_empty()) {
        return Err(invalid(
            "message",
            "NOT_SUPPORTED",
            "Polls and stickers are not available yet.",
        ));
    }
    let mut tags = Vec::new();
    for (n, raw) in body.applied_tags.iter().flatten().enumerate() {
        tags.push(raw.parse::<u64>().map_err(|_| {
            invalid(
                &format!("applied_tags.{n}"),
                "SNOWFLAKE_TYPE_INVALID",
                "Value is not snowflake.",
            )
        })?);
    }
    let view = t
        .ops
        .start_forum_post(
            user,
            parent,
            ForumPost {
                name: body.name,
                auto_archive_duration: body.auto_archive_duration,
                rate_limit_per_user: body.rate_limit_per_user,
                applied_tags: tags,
            },
        )
        .await?;
    let tid = view.channel.id;
    let mut flags = msg_body.flags.unwrap_or(0);
    if msg_body.tts == Some(true) {
        flags |= message_flags::TTS;
    }
    let req = SendMessage {
        content: msg_body.content.unwrap_or_default(),
        embeds: msg_body.embeds.unwrap_or_default(),
        components: msg_body.components.unwrap_or_default(),
        flags,
        ..SendMessage::default()
    };
    let msg = match m.send(user, tid, req).await {
        Ok(msg) => msg,
        Err(e) => {
            // The post never became visible: remove it again.
            if let Err(d) = t.ops.discard(tid).await {
                tracing::warn!(error = %d, "discarding a failed forum post failed");
            }
            return Err(e.into());
        }
    };
    let fresh = t.ops.view(user, tid).await?;
    t.publish_created(&fresh.channel).await;
    if let Some(member) = &fresh.member {
        t.publish_self_member(&fresh.channel, member).await;
    }
    announce_message(&m, &msg).await?;
    let message = render_one(&m, user, &msg).await?;
    Ok((
        StatusCode::CREATED,
        Json(ThreadResponse {
            channel: ChannelObject::new(&fresh.channel, &[]).with_member(fresh.member.as_ref()),
            message: Some(message),
        }),
    ))
}
