//! Server Insights (LNG-SSET-068): `GET /guilds/{id}/sr/insights`.
//!
//! The numbers come from a daily job (`insights.daily` in `sr-server`): members at the
//! end of each completed UTC day, joins, messages and voice minutes, plus the busiest
//! channels over the window. Voice minutes are 0 until voice sessions are persisted.

use super::deps::{SettingsDeps, actor, id, se};
use crate::routes::Ext;
use crate::routes::messages::error::ApiQuery;
use crate::{Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{Json, Router, extract::Path, routing::get};
use serde::{Deserialize, Serialize};
use sr_core::GuildId;
use utoipa::{IntoParams, ToSchema};

pub fn routes() -> Router<AppState> {
    Router::new().route("/guilds/{guild_id}/sr/insights", get(get_insights))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InsightsDayObject {
    /// `YYYY-MM-DD` (UTC).
    pub date: String,
    /// Members at the end of the day.
    pub members: i64,
    pub joins: i64,
    pub messages: i64,
    pub voice_minutes: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InsightsChannelObject {
    pub channel_id: String,
    pub messages: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InsightsObject {
    /// Oldest first; days the daily job has not computed yet are absent.
    pub days: Vec<InsightsDayObject>,
    pub top_channels: Vec<InsightsChannelObject>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct InsightsQuery {
    /// How many completed days to return, 1 to 90 (default 30).
    pub days: Option<u32>,
}

/// Basic Server Insights (`MANAGE_GUILD` or `VIEW_GUILD_INSIGHTS`).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/sr/insights",
    params(("guild_id" = String, Path, description = "Guild snowflake"), InsightsQuery),
    responses((status = 200, body = InsightsObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_insights(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
    ApiQuery(q): ApiQuery<InsightsQuery>,
) -> ApiResult<Json<InsightsObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    let i = deps
        .service
        .insights(acc.perms, gid, q.days)
        .await
        .map_err(se)?;
    Ok(Json(InsightsObject {
        days: i
            .days
            .into_iter()
            .map(|d| InsightsDayObject {
                date: d.day,
                members: d.members,
                joins: d.joins,
                messages: d.messages,
                voice_minutes: d.voice_minutes,
            })
            .collect(),
        top_channels: i
            .top_channels
            .into_iter()
            .map(|c| InsightsChannelObject {
                channel_id: c.channel_id.to_string(),
                messages: c.messages,
            })
            .collect(),
    }))
}
