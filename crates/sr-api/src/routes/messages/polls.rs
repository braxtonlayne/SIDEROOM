//! Poll endpoints (LNG-MSG-010): vote, withdraw, list voters, end early. Creating a
//! poll is part of "create message"; finalisation after expiry is a job
//! ([`super::rich_jobs`]).

use super::deps::MessagesDeps;
use super::error::{ApiQuery, MsgResult, invalid, parse_id};
use super::model::{MessageObject, ReactionUsersQuery, UserRef};
use super::render::{actor, render_one};
use crate::routes::Ext;
use crate::{Principal, openapi::ErrorBody};
use axum::{Json, extract::Path, http::StatusCode};
use serde::Serialize;
use sr_chat::rich_repo::{RetractResult, VoteResult};
use sr_chat::{ChatError, Message, poll_rules};
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, ErrorCode, MessageId, UserId};
use sr_events::DomainEvent;
use utoipa::ToSchema;

/// Users who picked one answer.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PollVoters {
    pub users: Vec<UserRef>,
}

struct Target {
    user: UserId,
    channel: ChannelId,
    answer: u32,
    message: Message,
}

async fn target(
    deps: &MessagesDeps,
    p: &Principal,
    channel: &str,
    message: &str,
    answer: Option<&str>,
) -> MsgResult<Target> {
    let user = actor(p);
    let channel: ChannelId = parse_id(channel, ErrorCode::UnknownChannel)?;
    let message_id: MessageId = parse_id(message, ErrorCode::UnknownMessage)?;
    let acc = deps.access(user, channel).await?;
    acc.require(Permissions::READ_MESSAGE_HISTORY)?;
    let message = deps.visible_message(channel, message_id).await?;
    if message.poll.is_none() {
        return Err(invalid(
            "message_id",
            "NO_POLL",
            "This message has no poll.",
        ));
    }
    let answer = match answer {
        Some(a) => a.parse::<u32>().ok().filter(|n| *n > 0).ok_or_else(|| {
            invalid(
                "answer_id",
                "NUMBER_TYPE_COERCE",
                "Value is not an answer id.",
            )
        })?,
        None => 0,
    };
    Ok(Target {
        user,
        channel,
        answer,
        message,
    })
}

fn closed() -> super::error::MsgError {
    invalid("poll", "POLL_CLOSED", "This poll has ended.")
}

/// Vote for an answer. A single-choice poll moves the caller's earlier vote.
#[utoipa::path(
    put,
    path = "/channels/{channel_id}/polls/{message_id}/answers/{answer_id}/voters/@me",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("message_id" = String, Path, description = "Poll message snowflake"),
        ("answer_id" = u32, Path, description = "Answer id"),
    ),
    responses(
        (status = 204),
        (status = 400, body = ErrorBody, description = "Unknown answer or the poll has ended"),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn vote(
    p: Principal,
    Ext(deps): Ext<MessagesDeps>,
    Path((channel, message, answer)): Path<(String, String, String)>,
) -> MsgResult<StatusCode> {
    let t = target(&deps, &p, &channel, &message, Some(&answer)).await?;
    let store = deps.rich_store("poll")?;
    match store
        .cast_vote(t.message.id, t.answer, t.user, deps.now_ms())
        .await?
    {
        VoteResult::Added { replaced } => {
            for old in replaced {
                deps.publish(DomainEvent::PollVoteRemoved {
                    guild: t.message.guild_id,
                    channel: t.channel,
                    message_id: t.message.id.get(),
                    user: t.user,
                    answer_id: old,
                })
                .await;
            }
            deps.publish(DomainEvent::PollVoteAdded {
                guild: t.message.guild_id,
                channel: t.channel,
                message_id: t.message.id.get(),
                user: t.user,
                answer_id: t.answer,
            })
            .await;
        }
        VoteResult::AlreadyVoted => {}
        VoteResult::Closed => return Err(closed()),
        VoteResult::UnknownAnswer => {
            return Err(invalid(
                "answer_id",
                "UNKNOWN_ANSWER",
                "Unknown poll answer.",
            ));
        }
        VoteResult::NoPoll => return Err(ChatError::UnknownMessage.into()),
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Withdraw the caller's vote for an answer.
#[utoipa::path(
    delete,
    path = "/channels/{channel_id}/polls/{message_id}/answers/{answer_id}/voters/@me",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("message_id" = String, Path, description = "Poll message snowflake"),
        ("answer_id" = u32, Path, description = "Answer id"),
    ),
    responses(
        (status = 204),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn unvote(
    p: Principal,
    Ext(deps): Ext<MessagesDeps>,
    Path((channel, message, answer)): Path<(String, String, String)>,
) -> MsgResult<StatusCode> {
    let t = target(&deps, &p, &channel, &message, Some(&answer)).await?;
    let store = deps.rich_store("poll")?;
    match store
        .retract_vote(t.message.id, t.answer, t.user, deps.now_ms())
        .await?
    {
        RetractResult::Removed => {
            deps.publish(DomainEvent::PollVoteRemoved {
                guild: t.message.guild_id,
                channel: t.channel,
                message_id: t.message.id.get(),
                user: t.user,
                answer_id: t.answer,
            })
            .await;
        }
        RetractResult::NotVoted => {}
        RetractResult::Closed => return Err(closed()),
        RetractResult::NoPoll => return Err(ChatError::UnknownMessage.into()),
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Users who voted for an answer, ascending by id.
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/polls/{message_id}/answers/{answer_id}",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("message_id" = String, Path, description = "Poll message snowflake"),
        ("answer_id" = u32, Path, description = "Answer id"),
        ("limit" = Option<u32>, Query, description = "1-100, default 25"),
        ("after" = Option<String>, Query, description = "User id to start after"),
    ),
    responses((status = 200, body = PollVoters), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn voters(
    p: Principal,
    Ext(deps): Ext<MessagesDeps>,
    Path((channel, message, answer)): Path<(String, String, String)>,
    ApiQuery(q): ApiQuery<ReactionUsersQuery>,
) -> MsgResult<Json<PollVoters>> {
    let t = target(&deps, &p, &channel, &message, Some(&answer)).await?;
    let limit = match q.limit {
        None => 25,
        Some(l) if (1..=100).contains(&l) => l as usize,
        Some(_) => {
            return Err(invalid(
                "limit",
                "NUMBER_TYPE_MAX",
                "Must be between 1 and 100.",
            ));
        }
    };
    let after = q
        .after
        .as_deref()
        .map(|s| s.parse::<UserId>())
        .transpose()
        .map_err(|_| invalid("after", "NUMBER_TYPE_COERCE", "Value is not snowflake."))?;
    let ids = deps
        .rich_store("poll")?
        .poll_voters(t.message.id, t.answer, after, limit)
        .await?;
    let found = deps.users.users(&ids).await?;
    let users = found.into_iter().map(|u| (u.id, u)).collect();
    Ok(Json(PollVoters {
        users: ids.iter().map(|id| UserRef::new(*id, &users)).collect(),
    }))
}

/// End a poll now. Only the author can; the final results go out as `MESSAGE_UPDATE`.
#[utoipa::path(
    post,
    path = "/channels/{channel_id}/polls/{message_id}/expire",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("message_id" = String, Path, description = "Poll message snowflake"),
    ),
    responses(
        (status = 200, body = MessageObject),
        (status = 400, body = ErrorBody, description = "The poll has already ended"),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn expire(
    p: Principal,
    Ext(deps): Ext<MessagesDeps>,
    Path((channel, message)): Path<(String, String)>,
) -> MsgResult<Json<MessageObject>> {
    let t = target(&deps, &p, &channel, &message, None).await?;
    if t.message.author_id != t.user {
        return Err(ChatError::MissingPermissions.into());
    }
    let store = deps.rich_store("poll")?;
    let now = deps.now_ms();
    if !t
        .message
        .poll
        .as_ref()
        .is_some_and(|poll| poll_rules::is_open(poll, now))
    {
        return Err(closed());
    }
    store.expire_poll_now(t.message.id, now).await?;
    super::rich_jobs::finalize_poll(&deps, t.channel, t.message.id).await?;
    let fresh = deps.visible_message(t.channel, t.message.id).await?;
    Ok(Json(render_one(&deps, t.user, &fresh).await?))
}
