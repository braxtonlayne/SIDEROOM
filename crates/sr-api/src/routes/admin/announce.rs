//! Platform announcements (ADM-PLT-009), the alert list (ADM-ALR-001) and the buttons of
//! staff cards (ADM-STF-007).

use super::ctx::{AdminDeps, ReasonBody, StaffCtx, id, map_err, reason};
use crate::{
    auth::Principal,
    error::{ApiResult, HttpError},
    openapi::ErrorBody,
    routes::{Ext, MaybeExt, messages::MessagesDeps},
};
use axum::{
    Json,
    extract::{Path, Query},
    http::HeaderMap,
};
use serde::Deserialize;
use sr_admin::{Announcement, AnnouncementInput, CardActionResult, SYSTEM_USER_ID, StaffAlertRow};
use sr_chat::message::Component;
use sr_core::{ChannelId, ErrorCode, MessageId, UserId};
use utoipa::ToSchema;

/// Announcements, newest first (`MANAGE_FEATURES`).
#[utoipa::path(
    get, path = "/admin/announcements",
    responses((status = 200, body = Vec<Announcement>), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list(ctx: StaffCtx) -> ApiResult<Json<Vec<Announcement>>> {
    ctx.svc()
        .list_announcements(&ctx.caller)
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AnnounceBody {
    #[serde(flatten)]
    pub announcement: AnnouncementInput,
    pub reason: Option<String>,
}

/// Publish an announcement: a banner every signed-in user sees, and with `send_dm` also a
/// DM from the SIDE ROOM system user to every active account (`MANAGE_FEATURES`). Needs a
/// fresh second factor because it reaches everyone.
#[utoipa::path(
    post, path = "/admin/announcements", request_body = AnnounceBody,
    responses((status = 200, body = Announcement), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create(
    ctx: StaffCtx,
    headers: HeaderMap,
    Json(b): Json<AnnounceBody>,
) -> ApiResult<Json<Announcement>> {
    let r = reason(&headers, b.reason.as_deref());
    ctx.require_reauth().await?;
    ctx.svc()
        .announce(&ctx.caller, b.announcement, r.as_deref())
        .await
        .map(Json)
        .map_err(map_err)
}

/// End an announcement early (`MANAGE_FEATURES`).
#[utoipa::path(
    delete, path = "/admin/announcements/{announcement_id}",
    params(("announcement_id" = String, Path, description = "Announcement snowflake")),
    responses((status = 200, body = Announcement), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn end(
    ctx: StaffCtx,
    Path(announcement_id): Path<String>,
    headers: HeaderMap,
    body: Option<Json<ReasonBody>>,
) -> ApiResult<Json<Announcement>> {
    let r = reason(&headers, body.and_then(|Json(b)| b.reason).as_deref());
    ctx.svc()
        .end_announcement(&ctx.caller, id(&announcement_id)?, r.as_deref())
        .await
        .map(Json)
        .map_err(map_err)
}

/// The banners showing right now. Any signed-in user.
#[utoipa::path(
    get, path = "/announcements/active",
    responses((status = 200, body = Vec<Announcement>), (status = 401, body = ErrorBody)),
    security(("session" = []), ("authorization" = []))
)]
pub async fn active(
    _p: Principal,
    Ext(deps): Ext<AdminDeps>,
) -> ApiResult<Json<Vec<Announcement>>> {
    deps.service
        .active_announcements()
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Default, Deserialize)]
pub struct AlertParams {
    pub limit: Option<i64>,
}

/// Recent staff alerts, newest first (`STAFF_BASIC`).
#[utoipa::path(
    get, path = "/admin/alerts",
    params(("limit" = Option<i64>, Query, description = "1-100, default 20")),
    responses((status = 200, body = Vec<StaffAlertRow>), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn alerts(
    ctx: StaffCtx,
    Query(p): Query<AlertParams>,
) -> ApiResult<Json<Vec<StaffAlertRow>>> {
    ctx.svc()
        .recent_alerts(&ctx.caller, p.limit)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Acknowledge an alert (`STAFF_BASIC`).
#[utoipa::path(
    post, path = "/admin/alerts/{alert_id}/ack",
    params(("alert_id" = String, Path, description = "Alert snowflake")),
    responses((status = 200, body = StaffAlertRow), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn ack(ctx: StaffCtx, Path(alert_id): Path<String>) -> ApiResult<Json<StaffAlertRow>> {
    ctx.svc()
        .ack_alert(&ctx.caller, id(&alert_id)?)
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CardActionBody {
    pub channel_id: String,
    pub message_id: String,
    /// The `custom_id` of the pressed button (`sr:<action>:<id>`).
    pub custom_id: String,
}

fn has_custom_id(components: &[Component], wanted: &str) -> bool {
    components
        .iter()
        .any(|c| c.custom_id.as_deref() == Some(wanted) || has_custom_id(&c.components, wanted))
}

/// Press a button on a Staff Server card (`#alerts`, `#review-queue`, ...). The message must
/// be a system-user card in a channel of the Staff Server that the caller can see; the
/// action runs with the caller's own platform permissions (the card grants nothing).
#[utoipa::path(
    post, path = "/admin/card-actions", request_body = CardActionBody,
    responses((status = 200, body = CardActionResult), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn card_action(
    ctx: StaffCtx,
    MaybeExt(messages): MaybeExt<MessagesDeps>,
    Json(b): Json<CardActionBody>,
) -> ApiResult<Json<CardActionResult>> {
    let channel = ChannelId::new(id(&b.channel_id)? as u64);
    let message = MessageId::new(id(&b.message_id)? as u64);
    let messages = messages.ok_or_else(|| HttpError::from(ErrorCode::SrInternal))?;
    let user = UserId::new(ctx.caller.user_id as u64);
    // The caller must be able to see the channel, and it must belong to the Staff Server.
    let access = messages
        .access(user, channel)
        .await
        .map_err(|_| HttpError::from(ErrorCode::UnknownChannel))?;
    let guild = access
        .channel
        .guild_id
        .ok_or_else(|| HttpError::from(ErrorCode::UnknownChannel))?;
    let in_staff_guild = messages
        .repo
        .get_guild(guild)
        .await
        .ok()
        .flatten()
        .is_some_and(|g| g.is_staff_guild);
    if !in_staff_guild || access.require_view().is_err() {
        return Err(ErrorCode::UnknownChannel.into());
    }
    let msg = messages
        .visible_message(channel, message)
        .await
        .map_err(|_| HttpError::from(ErrorCode::UnknownMessage))?;
    if msg.author_id != UserId::new(SYSTEM_USER_ID as u64)
        || !has_custom_id(&msg.components, &b.custom_id)
    {
        return Err(ErrorCode::UnknownMessage.into());
    }
    ctx.svc()
        .card_action(&ctx.caller, &b.custom_id)
        .await
        .map(Json)
        .map_err(map_err)
}
