//! FTR-SCH-001: scheduled messages. The author composes a message for later in the composer's clock menu, sees
//! the list, edits or cancels. Sending happens in the server's scheduler as the author, with the author's
//! permissions checked at send time.

use super::{UserFeatDeps, body_time, me, raw, row_id, uerr};
use crate::routes::Ext;
use crate::routes::messages::MessagesDeps;
use crate::routes::messages::error::{ApiJson, ApiQuery, MsgResult, parse_id};
use crate::{Principal, openapi::ErrorBody};
use axum::{Json, extract::Path, http::StatusCode};
use serde::Deserialize;
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, ErrorCode};
use sr_userfeat::scheduled::ScheduledMessage;
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
pub struct ScheduleBody {
    /// The message text (up to 2000 characters).
    pub content: String,
    /// ISO 8601 time to send, between a minute and a year from now.
    pub send_at: String,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct EditScheduledBody {
    pub content: Option<String>,
    pub send_at: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    /// Also list the last week's sent, failed and cancelled messages.
    #[serde(default)]
    pub all: bool,
}

/// Schedule a message for a channel. Needs permission to send there now; it is checked again when the message
/// goes out.
#[utoipa::path(
    post,
    path = "/channels/{channel_id}/scheduled-messages",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    request_body = ScheduleBody,
    responses(
        (status = 200, body = ScheduledMessage),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "Too many pending messages"),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn create(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(m): Ext<MessagesDeps>,
    Path(channel): Path<String>,
    ApiJson(body): ApiJson<ScheduleBody>,
) -> MsgResult<Json<ScheduledMessage>> {
    let user = me(&p)?;
    let channel: ChannelId = parse_id(&channel, ErrorCode::UnknownChannel)?;
    let at = body_time("send_at", &body.send_at)?;
    let acc = m.access(user, channel).await?;
    acc.require(if acc.channel.kind.is_thread() {
        Permissions::SEND_MESSAGES_IN_THREADS
    } else {
        Permissions::SEND_MESSAGES
    })?;
    let guild = acc
        .channel
        .guild_id
        .and_then(|g| i64::try_from(g.get()).ok());
    let row =
        d.uf.schedule_message(raw(user), raw_channel(channel), guild, &body.content, at)
            .await
            .map_err(uerr)?;
    Ok(Json(row))
}

fn raw_channel(c: ChannelId) -> i64 {
    i64::try_from(c.get()).unwrap_or(0)
}

/// The caller's scheduled messages, soonest first.
#[utoipa::path(
    get,
    path = "/users/@me/scheduled-messages",
    params(("all" = Option<bool>, Query, description = "Include recently settled messages")),
    responses((status = 200, body = Vec<ScheduledMessage>)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    ApiQuery(q): ApiQuery<ListQuery>,
) -> MsgResult<Json<Vec<ScheduledMessage>>> {
    let user = me(&p)?;
    Ok(Json(
        d.uf.list_scheduled(raw(user), q.all).await.map_err(uerr)?,
    ))
}

/// Change the text or time of a message that has not been sent.
#[utoipa::path(
    patch,
    path = "/users/@me/scheduled-messages/{id}",
    params(("id" = String, Path, description = "Scheduled message snowflake")),
    request_body = EditScheduledBody,
    responses(
        (status = 200, body = ScheduledMessage),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "Already sent or cancelled"),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn edit(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<EditScheduledBody>,
) -> MsgResult<Json<ScheduledMessage>> {
    let user = me(&p)?;
    let id = row_id(&id, "scheduled message")?;
    let at = body
        .send_at
        .as_deref()
        .map(|t| body_time("send_at", t))
        .transpose()?;
    Ok(Json(
        d.uf.edit_scheduled(raw(user), id, body.content.as_deref(), at)
            .await
            .map_err(uerr)?,
    ))
}

/// Cancel a scheduled message.
#[utoipa::path(
    delete,
    path = "/users/@me/scheduled-messages/{id}",
    params(("id" = String, Path, description = "Scheduled message snowflake")),
    responses(
        (status = 204, description = "Cancelled"),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "Already sent"),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn cancel(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Path(id): Path<String>,
) -> MsgResult<StatusCode> {
    let user = me(&p)?;
    let id = row_id(&id, "scheduled message")?;
    d.uf.cancel_scheduled(raw(user), id).await.map_err(uerr)?;
    Ok(StatusCode::NO_CONTENT)
}
