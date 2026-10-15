//! FTR-REM-001: reminders. `/remind 2h note` in the composer and "Remind me" in a message menu both create one
//! here; the scheduler delivers it as a DM from the system user when it is due.

use super::{UserFeatDeps, body_id, body_time, me, raw, row_id, uerr};
use crate::routes::Ext;
use crate::routes::messages::MessagesDeps;
use crate::routes::messages::error::{ApiJson, ApiQuery, MsgResult, invalid};
use crate::{Principal, openapi::ErrorBody};
use axum::{Json, extract::Path, http::StatusCode};
use serde::Deserialize;
use sr_core::{ChannelId, MessageId};
use sr_userfeat::reminders::{Reminder, Target};
use utoipa::ToSchema;

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ReminderBody {
    /// What to be reminded of (up to 1000 characters).
    #[serde(default)]
    pub note: Option<String>,
    /// ISO 8601 time, 5 seconds to a year from now. Give this or `in_seconds`.
    #[serde(default)]
    pub remind_at: Option<String>,
    /// Seconds from now.
    #[serde(default)]
    pub in_seconds: Option<i64>,
    /// With `message_id`: the message to be reminded about.
    #[serde(default)]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub message_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub all: bool,
}

/// Create a reminder.
#[utoipa::path(
    post,
    path = "/users/@me/reminders",
    request_body = ReminderBody,
    responses(
        (status = 200, body = Reminder),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "Too many pending reminders"),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn create(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(m): Ext<MessagesDeps>,
    ApiJson(body): ApiJson<ReminderBody>,
) -> MsgResult<Json<Reminder>> {
    let user = me(&p)?;
    let now = sr_userfeat::scheduled::now_ms();
    let at = match (&body.remind_at, body.in_seconds) {
        (Some(t), None) => body_time("remind_at", t)?,
        (None, Some(s)) => now.saturating_add(s.saturating_mul(1000)),
        _ => {
            return Err(invalid(
                "remind_at",
                "BASE_TYPE_REQUIRED",
                "Give exactly one of remind_at and in_seconds.",
            ));
        }
    };
    let mut target = Target::default();
    match (&body.channel_id, &body.message_id) {
        (Some(c), Some(msg)) => {
            let c = body_id("channel_id", c)?;
            let mid = body_id("message_id", msg)?;
            let channel = ChannelId::new(c as u64);
            let acc = m.access(user, channel).await?;
            acc.require_view()?;
            m.visible_message(channel, MessageId::new(mid as u64))
                .await?;
            target = Target {
                channel: Some(c),
                guild: acc
                    .channel
                    .guild_id
                    .and_then(|g| i64::try_from(g.get()).ok()),
                message: Some(mid),
            };
        }
        (None, None) => {}
        _ => {
            return Err(invalid(
                "message_id",
                "BASE_TYPE_REQUIRED",
                "channel_id and message_id go together.",
            ));
        }
    }
    Ok(Json(
        d.uf.create_reminder(raw(user), body.note.as_deref().unwrap_or(""), at, target)
            .await
            .map_err(uerr)?,
    ))
}

/// The caller's reminders, soonest first.
#[utoipa::path(
    get,
    path = "/users/@me/reminders",
    params(("all" = Option<bool>, Query, description = "Include recently delivered reminders")),
    responses((status = 200, body = Vec<Reminder>)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    ApiQuery(q): ApiQuery<ListQuery>,
) -> MsgResult<Json<Vec<Reminder>>> {
    let user = me(&p)?;
    Ok(Json(
        d.uf.list_reminders(raw(user), q.all).await.map_err(uerr)?,
    ))
}

/// Cancel a reminder.
#[utoipa::path(
    delete,
    path = "/users/@me/reminders/{id}",
    params(("id" = String, Path, description = "Reminder snowflake")),
    responses((status = 204, description = "Cancelled"), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn cancel(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Path(id): Path<String>,
) -> MsgResult<StatusCode> {
    let user = me(&p)?;
    let id = row_id(&id, "reminder")?;
    d.uf.cancel_reminder(raw(user), id).await.map_err(uerr)?;
    Ok(StatusCode::NO_CONTENT)
}
