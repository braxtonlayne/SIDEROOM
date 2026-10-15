//! Following an announcement channel (LNG-CHN-004).
//!
//! `POST /channels/{id}/followers` makes a channel of yours receive everything published in
//! the announcement channel `{id}`. Publishing itself is `POST /channels/{id}/messages/{id}/crosspost`
//! and the channel's auto-publish flag (messages module); this module only manages the
//! subscription, which is a follower webhook (kind 2) in the target channel. Unfollowing is
//! deleting that webhook (`DELETE /webhooks/{id}`).

use super::deps::{SettingsDeps, actor, id, se};
use super::events::SettingsEvent;
use crate::routes::Ext;
use crate::routes::messages::error::ApiJson;
use crate::{HttpError, Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{Json, Router, extract::Path, routing::post};
use serde::{Deserialize, Serialize};
use sr_chat::ChannelType;
use sr_core::{ApiError, ChannelId, ErrorCode};
use utoipa::ToSchema;

pub fn routes() -> Router<AppState> {
    Router::new().route("/channels/{channel_id}/followers", post(follow_channel))
}

/// Body of `POST /channels/{id}/followers`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct FollowBody {
    /// The channel that should receive the announcements (needs `MANAGE_WEBHOOKS` there).
    pub webhook_channel_id: String,
}

/// The new subscription.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FollowedChannelObject {
    /// The announcement channel being followed.
    pub channel_id: String,
    /// The follower webhook in the target channel; delete it to stop following.
    pub webhook_id: String,
}

/// Follow an announcement channel: its published messages are copied into
/// `webhook_channel_id`. Following twice returns the existing subscription. The source must be
/// an announcement channel you can see, the target a text or announcement channel where you
/// hold `MANAGE_WEBHOOKS`.
#[utoipa::path(
    post,
    path = "/channels/{channel_id}/followers",
    params(("channel_id" = String, Path, description = "Announcement channel snowflake")),
    request_body = FollowBody,
    responses(
        (status = 200, body = FollowedChannelObject),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn follow_channel(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(channel_id): Path<String>,
    ApiJson(body): ApiJson<FollowBody>,
) -> ApiResult<Json<FollowedChannelObject>> {
    let user = actor(&p)?;
    let source: ChannelId = id("channel_id", &channel_id)?;
    let target: ChannelId = id("webhook_channel_id", &body.webhook_channel_id)?;
    let src = deps.channel_access(user, source).await?;
    if src.channel.kind != ChannelType::Announcement {
        return Err(ErrorCode::CannotExecuteOnChannelType.into());
    }
    let dst = deps.channel_access(user, target).await?;
    if !matches!(
        dst.channel.kind,
        ChannelType::Text | ChannelType::Announcement
    ) {
        return Err(HttpError(ApiError::invalid_form().field(
            "webhook_channel_id",
            "CHANNEL_TYPE_INVALID",
            "Must be a text or announcement channel.",
        )));
    }
    let label = format!(
        "{} #{}",
        src.guild.guild.name,
        src.channel.name.as_deref().unwrap_or("announcements")
    );
    let webhook = deps
        .service
        .follow_channel(dst.perms, user, dst.guild.guild.id, target, source, &label)
        .await
        .map_err(se)?;
    deps.publish(SettingsEvent::WebhooksUpdated {
        guild: dst.guild.guild.id,
        channel: target,
    })
    .await;
    Ok(Json(FollowedChannelObject {
        channel_id: source.to_string(),
        webhook_id: webhook.id.to_string(),
    }))
}
