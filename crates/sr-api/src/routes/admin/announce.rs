//! Platform announcements (ADM-PLT-009), the alert list (ADM-ALR-001) and the buttons of
//! staff cards (ADM-STF-007).

use super::ctx::{AdminDeps, ReasonBody, StaffCtx, id, map_err, reason};
use crate::routes::review::ReviewDeps;
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
use sr_admin::{
    Announcement, AnnouncementInput, AnnouncementPatch, CardActionResult, SYSTEM_USER_ID,
    StaffAlertRow,
};
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

/// Edit an announcement that is still showing: its title, message or end time (`MANAGE_FEATURES`,
/// fresh second factor). Severity, `critical` and `send_dm` cannot change and no DM is sent again.
/// 409 once the announcement has ended. Every session hears `SR_ANNOUNCEMENT_UPDATE`.
#[utoipa::path(
    patch, path = "/admin/announcements/{announcement_id}", request_body = EditAnnouncementBody,
    params(("announcement_id" = String, Path, description = "Announcement snowflake")),
    responses((status = 200, body = Announcement), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn edit(
    ctx: StaffCtx,
    Path(announcement_id): Path<String>,
    headers: HeaderMap,
    Json(b): Json<EditAnnouncementBody>,
) -> ApiResult<Json<Announcement>> {
    let r = reason(&headers, b.reason.as_deref());
    ctx.require_reauth().await?;
    ctx.svc()
        .edit_announcement(&ctx.caller, id(&announcement_id)?, b.patch, r.as_deref())
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EditAnnouncementBody {
    #[serde(flatten)]
    pub patch: AnnouncementPatch,
    pub reason: Option<String>,
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

/// What a button on a `#review-queue` card asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewPress {
    Approve,
    /// Opens the review page on the reject dialog (a reason is required).
    Reject,
    /// Opens the review page on the request-changes dialog (a reason is required).
    Changes,
}

/// `sr:review-approve:<id>`, `sr:review-reject:<id>` and `sr:review-changes:<id>`.
fn parse_review_button(custom_id: &str) -> Option<(ReviewPress, i64)> {
    let (action, id) = custom_id.strip_prefix("sr:review-")?.split_once(':')?;
    let id: i64 = id.parse().ok().filter(|v| *v > 0)?;
    match action {
        "approve" => Some((ReviewPress::Approve, id)),
        "reject" => Some((ReviewPress::Reject, id)),
        "changes" => Some((ReviewPress::Changes, id)),
        _ => None,
    }
}

/// Approve runs the review service as the presser (claiming the submission first when it waits in
/// the queue), under the kind's review permission; Reject only points at the review page, because a
/// rejection needs a reason (D-082). A decided submission answers "already decided". The second value
/// is the detail for the staff access log when a decision was made.
pub async fn press_review_button(
    svc: &sr_review::ReviewService,
    caller: sr_review::Caller,
    press: ReviewPress,
    submission: i64,
    now_ms: i64,
) -> Result<(CardActionResult, Option<serde_json::Value>), HttpError> {
    use crate::routes::review::map_err as review_err;
    use sr_review::model::{DecisionInput, SubmissionState};
    let answer = |ok: bool, message: &str, link: Option<String>| CardActionResult {
        ok,
        message: message.into(),
        link,
    };
    let s = svc
        .reviewable(caller, submission)
        .await
        .map_err(review_err)?;
    if !matches!(
        s.state,
        SubmissionState::Submitted | SubmissionState::InReview
    ) {
        return Ok((answer(false, "Already decided.", None), None));
    }
    if press != ReviewPress::Approve {
        let (flag, message) = if press == ReviewPress::Changes {
            (
                "changes",
                "Requesting changes needs a reason. Opening the review page.",
            )
        } else {
            (
                "reject",
                "Rejecting needs a reason. Opening the review page.",
            )
        };
        let link = format!("/admin/review?submission={submission}&{flag}=1");
        return Ok((answer(true, message, Some(link)), None));
    }
    let mine = s.claimed_by == Some(caller.user_id) && s.lease_live(now_ms);
    if !mine {
        match svc.claim(caller, submission, false).await {
            Ok(_) => {}
            Err(sr_review::ReviewError::Conflict(_)) => {
                return Ok((
                    answer(false, "Another reviewer has this claimed.", None),
                    None,
                ));
            }
            Err(e) => return Err(review_err(e)),
        }
    }
    let d = svc
        .approve(caller, submission, DecisionInput::default())
        .await
        .map_err(review_err)?;
    let published = d.submission.state == SubmissionState::Published;
    let detail = serde_json::json!({
        "kind": d.submission.kind,
        "state": d.submission.state.as_str(),
        "published": published,
        "via": "review-queue card",
    });
    let message = if published {
        "Approved and published."
    } else {
        "Approved."
    };
    Ok((answer(true, message, None), Some(detail)))
}

async fn run_review_button(
    ctx: &StaffCtx,
    review: &ReviewDeps,
    press: ReviewPress,
    submission: i64,
) -> Result<CardActionResult, HttpError> {
    let caller = sr_review::Caller {
        user_id: ctx.caller.user_id,
        permissions: ctx.caller.permissions,
    };
    let now = crate::routes::review::now_ms();
    let (result, audit) =
        press_review_button(&review.service, caller, press, submission, now).await?;
    if let Some(detail) = audit
        && let Err(e) = ctx
            .svc()
            .record_action(
                &ctx.caller,
                "review.approve",
                Some(("submission", submission)),
                detail,
            )
            .await
    {
        tracing::error!(error = %e, "review decision was not written to the access log");
    }
    Ok(result)
}

/// `sr:package-approve:<version_id>` (M-11, D-301).
fn parse_package_button(custom_id: &str) -> Option<i64> {
    custom_id
        .strip_prefix("sr:package-approve:")?
        .parse()
        .ok()
        .filter(|v| *v > 0)
}

/// Approve a package version from its `#review-queue` card. Runs `PackageService::decide` as
/// the presser, so the `REVIEW_PACKAGES` permission, the two-person rule (shown as the toast
/// "You cannot review your own package.") and the access log apply; the card itself is closed by
/// the usual outcome event ("Approved by @x", once). A version that is no longer pending answers
/// "Already decided.". Reject stays a link to the review page's dialog.
pub async fn press_package_button(
    service: &sr_packages::service::PackageService,
    permissions: sr_core::PlatformPermissions,
    presser: i64,
    version: i64,
) -> Result<CardActionResult, HttpError> {
    use sr_packages::PackageError;
    use sr_packages::service::Decision;
    let answer = |ok: bool, message: &str| CardActionResult {
        ok,
        message: message.into(),
        link: None,
    };
    if !permissions.contains(sr_core::PlatformPermissions::REVIEW_PACKAGES) {
        return Err(HttpError::from(ErrorCode::MissingPermissions));
    }
    match service
        .decide(version, presser, Decision::Approve, None)
        .await
    {
        Ok(_) => Ok(answer(true, "Approved.")),
        Err(PackageError::SelfReview) => Ok(answer(false, "You cannot review your own package.")),
        Err(PackageError::Conflict(_)) => Ok(answer(false, "Already decided.")),
        Err(PackageError::NotFound(_)) => Err(ErrorCode::UnknownMessage.into()),
        Err(e) => {
            tracing::error!(error = %e, "package approval from a card failed");
            Err(ErrorCode::SrInternal.into())
        }
    }
}

async fn run_package_button(
    ctx: &StaffCtx,
    packages: &crate::routes::packages::PackagesDeps,
    version: i64,
) -> Result<CardActionResult, HttpError> {
    let result = press_package_button(
        &packages.service,
        ctx.caller.permissions,
        ctx.caller.user_id,
        version,
    )
    .await?;
    if result.ok
        && let Err(e) = ctx
            .svc()
            .record_action(
                &ctx.caller,
                "developer.package_approve",
                Some(("package_version", version)),
                serde_json::json!({"via": "review-queue card"}),
            )
            .await
    {
        tracing::error!(error = %e, "package decision was not written to the access log");
    }
    Ok(result)
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
    MaybeExt(review): MaybeExt<ReviewDeps>,
    MaybeExt(packages): MaybeExt<crate::routes::packages::PackagesDeps>,
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
    // A review card that was already decided has lost its buttons; pressing a stale copy of one
    // still gets the answer "already decided" (G-68).
    let review_press = parse_review_button(&b.custom_id);
    let package_press = parse_package_button(&b.custom_id);
    if msg.author_id != UserId::new(SYSTEM_USER_ID as u64)
        || (review_press.is_none()
            && package_press.is_none()
            && !has_custom_id(&msg.components, &b.custom_id))
    {
        return Err(ErrorCode::UnknownMessage.into());
    }
    if let Some(version) = package_press {
        let packages = packages.ok_or_else(|| HttpError::from(ErrorCode::SrInternal))?;
        return run_package_button(&ctx, &packages, version).await.map(Json);
    }
    if let Some((press, submission)) = review_press {
        let review = review.ok_or_else(|| HttpError::from(ErrorCode::SrInternal))?;
        return run_review_button(&ctx, &review, press, submission)
            .await
            .map(Json);
    }
    ctx.svc()
        .card_action(&ctx.caller, &b.custom_id)
        .await
        .map(Json)
        .map_err(map_err)
}
