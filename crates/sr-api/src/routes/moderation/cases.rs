//! Case routes (MOD-CASE-004 to -013, MOD-RPT-009).

use super::{Actor, Delivered, id, map_err, staff_result};
use crate::{
    auth::UserOnly,
    error::ApiResult,
    openapi::ErrorBody,
    routes::{Ext, settings::deps::SettingsDeps},
};
use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sr_admin::BanCategory;
use sr_core::{GuildId, UserId, permissions::Permissions};
use sr_moderation::{
    ModError, Transition,
    model::{
        CaseDetail, CaseFilter, CaseNote, CasePage, CaseSeverity, CaseState, CaseSummary, NewCase,
        ReportCategory, Who,
    },
};
use utoipa::{IntoParams, ToSchema};

use super::ModerationDeps;

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CaseParams {
    /// `open`, `triage`, `investigating`, `actioned`, `dismissed` or `closed`.
    pub state: Option<String>,
    /// Only cases still being worked (open, triage, investigating).
    pub open_only: Option<bool>,
    pub category: Option<String>,
    pub severity: Option<String>,
    /// Assigned to this account.
    pub assignee: Option<String>,
    /// Platform staff only: cases of this server.
    pub guild_id: Option<String>,
    /// Cases about this account.
    pub subject_account_id: Option<String>,
    /// `next_cursor` of the previous page.
    pub cursor: Option<String>,
    /// Oldest first (the work queue); default newest first.
    pub oldest_first: Option<bool>,
    /// 1 to 100, default 25.
    pub limit: Option<i64>,
}

fn filter(p: &CaseParams) -> ApiResult<CaseFilter> {
    let bad = |m: &str| map_err(ModError::Invalid(m.to_owned()));
    Ok(CaseFilter {
        state: p
            .state
            .as_deref()
            .map(|s| CaseState::parse(s).ok_or_else(|| bad("unknown state")))
            .transpose()?,
        open_only: p.open_only.unwrap_or(false),
        category: p
            .category
            .as_deref()
            .map(|s| ReportCategory::parse(s).ok_or_else(|| bad("unknown category")))
            .transpose()?,
        severity: p
            .severity
            .as_deref()
            .map(|s| CaseSeverity::parse(s).ok_or_else(|| bad("unknown severity")))
            .transpose()?,
        assignee: p
            .assignee
            .as_deref()
            .map(|s| id(s, "account"))
            .transpose()?,
        guild_id: p.guild_id.as_deref().map(|s| id(s, "server")).transpose()?,
        subject_account: p
            .subject_account_id
            .as_deref()
            .map(|s| id(s, "account"))
            .transpose()?,
        cursor: p.cursor.as_deref().map(|s| id(s, "cursor")).transpose()?,
        oldest_first: p.oldest_first.unwrap_or(false),
        limit: p.limit.unwrap_or(25),
    })
}

/// The case queue of platform staff: what is routed to platform staff or to reviewers (`MANAGE_CASES`, or a
/// review bit for the cases of that kind). Server cases appear once their moderators escalate (D-209).
#[utoipa::path(
    get, path = "/moderation/cases", params(CaseParams),
    responses((status = 200, body = CasePage), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list_cases(a: Actor, Query(p): Query<CaseParams>) -> ApiResult<Json<CasePage>> {
    let ctx = a.staff_only()?;
    let f = filter(&p)?;
    let r = a.svc().list_cases(&Who::Staff(&ctx.caller), &f).await;
    staff_result(ctx, "moderation.case.list", None, r)
        .await
        .map(Json)
}

/// The cases of one server for its moderators (`MANAGE_MESSAGES`): the ones routed to the server, and rows
/// for those they escalated.
#[utoipa::path(
    get, path = "/moderation/guilds/{guild_id}/cases", params(CaseParams,
        ("guild_id" = String, Path, description = "Server snowflake")),
    responses((status = 200, body = CasePage), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn guild_cases(
    UserOnly(p): UserOnly,
    Ext(d): Ext<ModerationDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
    Query(q): Query<CaseParams>,
) -> ApiResult<Json<CasePage>> {
    let g = id(&guild_id, "server")?;
    let (Ok(u), Ok(gu)) = (u64::try_from(p.user_id), u64::try_from(g)) else {
        return Err(map_err(ModError::NotFound("server")));
    };
    let access = s.guild_access(UserId::new(u), GuildId::new(gu)).await?;
    if !access.perms.has(Permissions::MANAGE_MESSAGES) {
        return Err(map_err(ModError::Forbidden));
    }
    let who = Who::GuildMod {
        user_id: p.user_id,
        guild_id: g,
    };
    d.svc
        .list_cases(&who, &filter(&q)?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Body of a staff-opened case: the case plus an optional server and severity.
#[derive(Debug, Deserialize, ToSchema)]
pub struct OpenCaseBody {
    #[serde(flatten)]
    pub case: NewCase,
    /// The server the case is about.
    #[serde(default, with = "sr_admin::model::opt_id")]
    #[schema(value_type = Option<String>)]
    pub guild_id: Option<i64>,
    /// Defaults to the preset of the category.
    #[serde(default)]
    pub severity: Option<CaseSeverity>,
}

/// Open a case without a report (`MANAGE_CASES`).
#[utoipa::path(
    post, path = "/moderation/cases", request_body = OpenCaseBody,
    responses((status = 201, body = CaseSummary), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn open_case(
    a: Actor,
    Json(b): Json<OpenCaseBody>,
) -> ApiResult<(StatusCode, Json<CaseSummary>)> {
    let ctx = a.staff_only()?;
    let extras = sr_moderation::model::CaseExtras {
        guild_id: b.guild_id,
        severity: b.severity,
    };
    let r = a
        .svc()
        .open_case_with(&Who::Staff(&ctx.caller), &b.case, extras)
        .await;
    staff_result(ctx, "moderation.case.open", None, r)
        .await
        .map(|c| (StatusCode::CREATED, Json(c)))
}

/// One case as far as the caller may see it. Reading the reporters as platform staff and opening a DM
/// snapshot are written to the access log first (D-125, D-214).
#[utoipa::path(
    get, path = "/moderation/cases/{case_id}",
    params(("case_id" = String, Path, description = "Case snowflake")),
    responses((status = 200, body = CaseDetail), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_case(a: Actor, Path(case_id): Path<String>) -> ApiResult<Json<CaseDetail>> {
    let cid = id(&case_id, "case")?;
    let who = a.who_for(cid).await?;
    a.svc().get_case(&who, cid).await.map(Json).map_err(map_err)
}

/// Body of a state change.
#[derive(Debug, Deserialize, ToSchema)]
pub struct StateBody {
    pub to: CaseState,
    /// What was decided, in a few words. Required when closing.
    pub outcome: Option<String>,
    /// The line reporters read when the case closes. Never names the other party.
    pub reporter_line: Option<String>,
}

/// Move a case along its lifecycle.
#[utoipa::path(
    post, path = "/moderation/cases/{case_id}/state", request_body = StateBody,
    params(("case_id" = String, Path, description = "Case snowflake")),
    responses((status = 200, body = CaseSummary), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn set_state(
    a: Actor,
    Path(case_id): Path<String>,
    Json(b): Json<StateBody>,
) -> ApiResult<Json<CaseSummary>> {
    let cid = id(&case_id, "case")?;
    let who = a.who_for(cid).await?;
    a.svc()
        .set_state(
            &who,
            cid,
            &Transition {
                to: b.to,
                outcome: b.outcome,
                reporter_line: b.reporter_line,
            },
        )
        .await
        .map(Json)
        .map_err(map_err)
}

/// Body of actions that only need a reason.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ReasonOnly {
    pub reason: Option<String>,
}

/// Reopen a closed case inside the reopen window; later, a new case links back to it. Answers the case to work on.
#[utoipa::path(
    post, path = "/moderation/cases/{case_id}/reopen", request_body = ReasonOnly,
    params(("case_id" = String, Path, description = "Case snowflake")),
    responses((status = 200, body = CaseSummary), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn reopen(
    a: Actor,
    Path(case_id): Path<String>,
    Json(b): Json<ReasonOnly>,
) -> ApiResult<Json<CaseSummary>> {
    let cid = id(&case_id, "case")?;
    let who = a.who_for(cid).await?;
    a.svc()
        .reopen_case(&who, cid, b.reason.as_deref())
        .await
        .map(Json)
        .map_err(map_err)
}

/// Body of an internal note.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CaseNoteBody {
    /// Up to 4000 characters.
    pub body: String,
}

/// Add an internal note (append-only).
#[utoipa::path(
    post, path = "/moderation/cases/{case_id}/notes", request_body = CaseNoteBody,
    params(("case_id" = String, Path, description = "Case snowflake")),
    responses((status = 201, body = CaseNote), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn add_note(
    a: Actor,
    Path(case_id): Path<String>,
    Json(b): Json<CaseNoteBody>,
) -> ApiResult<(StatusCode, Json<CaseNote>)> {
    let cid = id(&case_id, "case")?;
    let who = a.who_for(cid).await?;
    a.svc()
        .add_note(&who, cid, &b.body)
        .await
        .map(|n| (StatusCode::CREATED, Json(n)))
        .map_err(map_err)
}

/// Claim a case.
#[utoipa::path(
    put, path = "/moderation/cases/{case_id}/assignee",
    params(("case_id" = String, Path, description = "Case snowflake")),
    responses((status = 200, body = CaseSummary), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn claim(a: Actor, Path(case_id): Path<String>) -> ApiResult<Json<CaseSummary>> {
    let cid = id(&case_id, "case")?;
    let who = a.who_for(cid).await?;
    a.svc()
        .assign(&who, cid, true)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Release a case (the assignee, or somebody with `MANAGE_CASES`).
#[utoipa::path(
    delete, path = "/moderation/cases/{case_id}/assignee",
    params(("case_id" = String, Path, description = "Case snowflake")),
    responses((status = 200, body = CaseSummary), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn release(a: Actor, Path(case_id): Path<String>) -> ApiResult<Json<CaseSummary>> {
    let cid = id(&case_id, "case")?;
    let who = a.who_for(cid).await?;
    a.svc()
        .assign(&who, cid, false)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Body of an escalation.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct EscalateBody {
    /// Why the server's moderators hand this over.
    pub reason: Option<String>,
}

/// Hand a server's case to platform staff (the server's moderators only).
#[utoipa::path(
    post, path = "/moderation/cases/{case_id}/escalate", request_body = EscalateBody,
    params(("case_id" = String, Path, description = "Case snowflake")),
    responses((status = 200, body = CaseSummary), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn escalate(
    a: Actor,
    Path(case_id): Path<String>,
    Json(b): Json<EscalateBody>,
) -> ApiResult<Json<CaseSummary>> {
    let cid = id(&case_id, "case")?;
    let who = a.who_for(cid).await?;
    a.svc()
        .escalate(&who, cid, b.reason.as_deref())
        .await
        .map(Json)
        .map_err(map_err)
}

/// Body of a recorded action.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ActionBody {
    /// `delete_message`, `restore_message`, `remove_attachment`, `timeout`, `kick`, `guild_ban`, `mark_nsfw`
    /// or `other`. Recording does not perform it.
    pub action: String,
    pub note: Option<String>,
    /// `true` performs the action through the server's services (`delete_message`, `restore_message`,
    /// `remove_attachment`, `timeout`, `kick`, `guild_ban`, `suspend_account`) on the case's own target;
    /// otherwise it only records work done elsewhere.
    #[serde(default)]
    pub perform: bool,
    /// `timeout` when performing: minutes, 1 to 40320.
    pub timeout_minutes: Option<i64>,
}

/// Record that an action was taken with the server's own tools, or (`perform`) do it through the server's services
/// on the case's own target; either way the case moves to `actioned`.
#[utoipa::path(
    post, path = "/moderation/cases/{case_id}/actions", request_body = ActionBody,
    params(("case_id" = String, Path, description = "Case snowflake")),
    responses((status = 200, body = CaseSummary), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn record_action(
    a: Actor,
    Path(case_id): Path<String>,
    Json(b): Json<ActionBody>,
) -> ApiResult<Json<CaseSummary>> {
    let cid = id(&case_id, "case")?;
    let who = a.who_for(cid).await?;
    if b.perform {
        let action = sr_moderation::PerformedAction::parse(&b.action)
            .ok_or_else(|| map_err(ModError::Invalid("unknown action".into())))?;
        let r = a
            .svc()
            .perform_action(
                &who,
                cid,
                &sr_moderation::PerformRequest {
                    action,
                    note: b.note.clone().unwrap_or_default(),
                    timeout_minutes: b.timeout_minutes,
                },
            )
            .await;
        return match a.staff.as_ref() {
            Some(ctx) => staff_result(ctx, "moderation.case.perform", Some(("case", cid)), r)
                .await
                .map(Json),
            None => r.map(Json).map_err(map_err),
        };
    }
    a.svc()
        .record_action(&who, cid, &b.action, b.note.as_deref().unwrap_or(""))
        .await
        .map(Json)
        .map_err(map_err)
}

/// Body of a warning.
#[derive(Debug, Deserialize, ToSchema)]
pub struct WarnBody {
    /// Up to 1000 characters; sent to the person as a system DM that never names a reporter.
    pub text: String,
}

/// Warn the person a case is about (`MANAGE_CASES`).
#[utoipa::path(
    post, path = "/moderation/cases/{case_id}/warn", request_body = WarnBody,
    params(("case_id" = String, Path, description = "Case snowflake")),
    responses((status = 200, body = Delivered), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn warn(
    a: Actor,
    Path(case_id): Path<String>,
    Json(b): Json<WarnBody>,
) -> ApiResult<Json<Delivered>> {
    let cid = id(&case_id, "case")?;
    let ctx = a.staff_only()?;
    let r = a.svc().warn(&Who::Staff(&ctx.caller), cid, &b.text).await;
    staff_result(ctx, "moderation.case.warn", Some(("case", cid)), r)
        .await
        .map(|delivered| Json(Delivered { delivered }))
}

/// Body of a ban from a case.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CaseBanBody {
    /// The why, from the section 8 vocabulary.
    pub reason_category: ReportCategory,
    /// Omit it to take the default of `reason_category` (D-211); staff can raise or lower it.
    pub severity: Option<CaseSeverity>,
    /// Why, for the audit log and the case. Required.
    pub note: Option<String>,
}

/// What a ban from a case did.
#[derive(Debug, Serialize, ToSchema)]
pub struct BanDone {
    #[serde(with = "sr_admin::model::id")]
    #[schema(value_type = String)]
    pub ban_id: i64,
}

/// Platform-ban the Person a case is about (`MANAGE_CASES` and `PLATFORM_BAN`). The ban goes through the admin
/// service (suspicion, sessions, its own log row); the case records it with the `reason_category`.
#[utoipa::path(
    post, path = "/moderation/cases/{case_id}/ban", request_body = CaseBanBody,
    params(("case_id" = String, Path, description = "Case snowflake")),
    responses((status = 200, body = BanDone), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn ban(
    a: Actor,
    Path(case_id): Path<String>,
    Json(b): Json<CaseBanBody>,
) -> ApiResult<Json<BanDone>> {
    let cid = id(&case_id, "case")?;
    let ctx = a.staff_only()?;
    let who = Who::Staff(&ctx.caller);
    let subject = a.svc().case_subject(&who, cid).await;
    let (person, account, _, _) =
        staff_result(ctx, "moderation.case.ban", Some(("case", cid)), subject).await?;
    let person = person
        .ok_or_else(|| map_err(ModError::Conflict("This case has no person to ban".into())))?;
    let severity = b.severity.unwrap_or(b.reason_category.default_severity());
    let category = match severity {
        CaseSeverity::Serious => BanCategory::Serious,
        CaseSeverity::Standard => BanCategory::Standard,
    };
    ctx.svc()
        .ban_person(&ctx.caller, person, account, category, b.note.as_deref())
        .await
        .map_err(crate::routes::admin::map_err)?;
    let r = a
        .svc()
        .record_platform_ban(
            &who,
            cid,
            b.reason_category,
            b.note.as_deref().unwrap_or(""),
        )
        .await;
    staff_result(ctx, "moderation.case.ban", Some(("case", cid)), r)
        .await
        .map(|ban_id| Json(BanDone { ban_id }))
}
