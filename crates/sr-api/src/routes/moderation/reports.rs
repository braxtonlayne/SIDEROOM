//! Reports from members (MOD-RPT-002, MOD-RPT-007).

use super::{ModerationDeps, map_err};
use crate::{
    auth::UserOnly,
    error::ApiResult,
    openapi::ErrorBody,
    routes::{Ext, MaybeExt, messages::MessagesDeps},
};
use axum::{Json, http::StatusCode};
use sr_core::{ChannelId, ErrorCode, permissions::Permissions};
use sr_moderation::model::{MyReport, NewReport, ReportReceipt, TargetKind};

/// File a report (any signed-in member that can see the target). The server snapshots the evidence, routes
/// the case (D-209), freezes a reported message, and never shows the reporter to the reported person.
/// 403 `SrFeatureDisabled` while the `moderation.reports` flag is off.
#[utoipa::path(
    post, path = "/moderation/reports", request_body = NewReport,
    responses((status = 201, body = ReportReceipt), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody),
              (status = 429, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn file_report(
    UserOnly(p): UserOnly,
    Ext(d): Ext<ModerationDeps>,
    MaybeExt(m): MaybeExt<MessagesDeps>,
    Json(body): Json<NewReport>,
) -> ApiResult<(StatusCode, Json<ReportReceipt>)> {
    if !d.reports_enabled().await {
        return Err(ErrorCode::SrFeatureDisabled.into());
    }
    // The channel permissions decide whether the reporter can see a message or a channel at all.
    let channel = match body.target_kind {
        TargetKind::Message => body.channel_id,
        TargetKind::Channel => Some(body.target_id),
        _ => None,
    };
    if let Some(c) = channel {
        let (Ok(u), Ok(c)) = (u64::try_from(p.user_id), u64::try_from(c)) else {
            return Err(ErrorCode::UnknownChannel.into());
        };
        let Some(m) = m else {
            return Err(ErrorCode::SrInternal.into());
        };
        let access = m
            .access(sr_core::UserId::new(u), ChannelId::new(c))
            .await
            .map_err(|_| ErrorCode::UnknownChannel)?;
        let needed = if body.target_kind == TargetKind::Message {
            Permissions::READ_MESSAGE_HISTORY
        } else {
            Permissions::VIEW_CHANNEL
        };
        access
            .require(needed)
            .map_err(|_| ErrorCode::UnknownChannel)?;
    }
    let receipt = d.svc.file_report(p.user_id, &body).await.map_err(map_err)?;
    Ok((StatusCode::CREATED, Json(receipt)))
}

/// The caller's own reports with their status and outcome line (never the other party), newest first.
#[utoipa::path(
    get, path = "/moderation/reports/mine",
    responses((status = 200, body = Vec<MyReport>), (status = 401, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn my_reports(
    UserOnly(p): UserOnly,
    Ext(d): Ext<ModerationDeps>,
) -> ApiResult<Json<Vec<MyReport>>> {
    d.svc
        .my_reports(p.user_id, 50)
        .await
        .map(Json)
        .map_err(map_err)
}
