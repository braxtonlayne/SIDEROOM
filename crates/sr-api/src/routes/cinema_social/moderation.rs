//! Staff moderation of Cinema reviews and comments: the report queue and hiding content (CIN-SOC-001).
//!
//! All routes need a staff session holding `DELETE_CONTENT` (the platform permission that removes user
//! content). Every change (hide, restore, closing a report) is written to the staff access log as
//! `cinema.<review|comment>.<hide|unhide>` or `cinema.report.resolve`. Hidden content is not listed to
//! members; its author still sees nothing different (no notification).

use super::{CinemaSocialDeps, id, map_err};
use crate::{
    error::{ApiResult, HttpError},
    openapi::ErrorBody,
    routes::{
        Ext,
        admin::{AdminDeps, StaffCtx},
    },
    state::AppState,
};
use axum::{
    Json,
    extract::{FromRequestParts, Path, Query},
    http::{StatusCode, request::Parts},
};
use serde::Deserialize;
use serde_json::json;
use sr_cinema_social::moderation::{ReportPage, ReportStatus};
use sr_cinema_social::{CinemaService, ContentKind, SocialError};
use sr_core::{ApiError, ErrorCode, PlatformPermissions};
use std::sync::Arc;
use utoipa::{IntoParams, ToSchema};

/// A staff session that may moderate Cinema content.
pub(crate) struct ModeratorCtx {
    staff: StaffCtx,
    social: Arc<CinemaSocialDeps>,
}

impl ModeratorCtx {
    fn svc(&self) -> &CinemaService {
        &self.social.service
    }

    async fn audit(&self, action: &str, target: (&str, i64), detail: serde_json::Value) {
        let admin: &AdminDeps = &self.staff.deps;
        if let Err(e) = admin
            .service
            .record_action(&self.staff.caller, action, Some(target), detail)
            .await
        {
            tracing::error!(error = %e, action, "cinema moderation was not written to the access log");
        }
    }
}

impl FromRequestParts<AppState> for ModeratorCtx {
    type Rejection = HttpError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let staff = StaffCtx::from_request_parts(parts, state).await?;
        if !staff
            .caller
            .permissions
            .has(PlatformPermissions::DELETE_CONTENT)
        {
            return Err(ApiError::with_message(
                ErrorCode::MissingPermissions,
                "Missing platform permission DELETE_CONTENT",
            )
            .into());
        }
        let Ext(social) = Ext::<CinemaSocialDeps>::from_request_parts(parts, state).await?;
        Ok(Self { staff, social })
    }
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ReportParams {
    /// `open` (default), `actioned`, `dismissed` or `all`.
    pub status: Option<String>,
    /// `next_before` of the previous page.
    pub before: Option<String>,
    /// 1 to 100, default 24.
    pub limit: Option<u32>,
}

/// The report queue for reviews and comments, newest first, with an excerpt of the reported text.
#[utoipa::path(
    get, path = "/admin/cinema/reports", operation_id = "admin_cinema_reports", params(ReportParams),
    responses((status = 200, body = ReportPage), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn reports(
    ctx: ModeratorCtx,
    Query(p): Query<ReportParams>,
) -> ApiResult<Json<ReportPage>> {
    let status = match p.status.as_deref() {
        None | Some("open") => Some(ReportStatus::Open),
        Some("all") => None,
        Some(s) => Some(ReportStatus::parse(s).ok_or_else(|| {
            map_err(SocialError::Invalid(
                "status must be open, actioned, dismissed or all".into(),
            ))
        })?),
    };
    ctx.svc()
        .reports(status, p.before.as_deref(), p.limit)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Body of a report resolution.
#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = CinemaResolveReport)]
pub struct ResolveBody {
    pub status: ReportStatus,
    /// A note for the access log.
    pub reason: Option<String>,
}

/// Close a report as `actioned` or `dismissed` (or reopen it with `open`).
#[utoipa::path(
    post, path = "/admin/cinema/reports/{report_id}/resolve", operation_id = "admin_cinema_resolve_report",
    params(("report_id" = String, Path, description = "Report snowflake")),
    request_body = ResolveBody,
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn resolve_report(
    ctx: ModeratorCtx,
    Path(report_id): Path<String>,
    Json(b): Json<ResolveBody>,
) -> ApiResult<StatusCode> {
    let rid = id(&report_id, "report")?;
    if !ctx
        .svc()
        .resolve_report(rid, b.status)
        .await
        .map_err(map_err)?
    {
        return Err(map_err(SocialError::NotFound("report")));
    }
    ctx.audit(
        "cinema.report.resolve",
        ("cinema_report", rid),
        json!({ "status": b.status.as_str(), "reason": b.reason }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

/// Body of a hide or restore.
#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = CinemaHideContent)]
pub struct HiddenBody {
    pub hidden: bool,
    /// A note for the access log.
    pub reason: Option<String>,
}

async fn set_hidden(
    ctx: &ModeratorCtx,
    kind: ContentKind,
    target: i64,
    b: HiddenBody,
) -> ApiResult<StatusCode> {
    if !ctx
        .svc()
        .set_hidden(kind, target, b.hidden)
        .await
        .map_err(map_err)?
    {
        return Err(map_err(SocialError::NotFound(kind.as_str())));
    }
    let closed = if b.hidden {
        ctx.svc()
            .close_reports_for(kind, target, ReportStatus::Actioned)
            .await
            .map_err(map_err)?
    } else {
        0
    };
    let action = format!(
        "cinema.{}.{}",
        kind.as_str(),
        if b.hidden { "hide" } else { "unhide" }
    );
    ctx.audit(
        &action,
        (
            if kind == ContentKind::Review {
                "cinema_review"
            } else {
                "cinema_comment"
            },
            target,
        ),
        json!({ "reason": b.reason, "reports_actioned": closed }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

/// Hide a review from every listing, or restore it. Hiding closes its open reports as `actioned`.
#[utoipa::path(
    put, path = "/admin/cinema/reviews/{review_id}/hidden", operation_id = "admin_cinema_hide_review",
    params(("review_id" = String, Path, description = "Review snowflake")),
    request_body = HiddenBody,
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn hide_review(
    ctx: ModeratorCtx,
    Path(review_id): Path<String>,
    Json(b): Json<HiddenBody>,
) -> ApiResult<StatusCode> {
    set_hidden(&ctx, ContentKind::Review, id(&review_id, "review")?, b).await
}

/// Hide a comment from every listing, or restore it. Hiding closes its open reports as `actioned`.
#[utoipa::path(
    put, path = "/admin/cinema/comments/{comment_id}/hidden", operation_id = "admin_cinema_hide_comment",
    params(("comment_id" = String, Path, description = "Comment snowflake")),
    request_body = HiddenBody,
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn hide_comment(
    ctx: ModeratorCtx,
    Path(comment_id): Path<String>,
    Json(b): Json<HiddenBody>,
) -> ApiResult<StatusCode> {
    set_hidden(&ctx, ContentKind::Comment, id(&comment_id, "comment")?, b).await
}
