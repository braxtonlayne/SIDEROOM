//! Membership applications (LNG-SSET-042, backend half).
//!
//! * `GET/PUT /guilds/{id}/sr/applications/form`: the questions form and the "apply to
//!   join" switch. Anyone signed in reads it while applications are open (applicants are
//!   not members); `PUT` needs `MANAGE_GUILD`.
//! * `POST /guilds/{id}/sr/applications`: a non-member submits answers.
//! * `GET /guilds/{id}/sr/applications/@me`: the caller's latest application.
//! * `GET /guilds/{id}/sr/applications`: the review queue (`MANAGE_GUILD`).
//! * `POST /guilds/{id}/sr/applications/{application_id}/decision`: approve (adds the
//!   member and announces `GUILD_MEMBER_ADD`) or reject (`MANAGE_GUILD`).

use super::deps::{SettingsDeps, actor, id, invalid, re, se};
use super::events::SettingsEvent;
use crate::routes::Ext;
use crate::routes::guilds::dto::{MemberObject, iso8601};
use crate::routes::messages::error::{ApiJson, ApiQuery};
use crate::routes::social::model::UserObject;
use crate::{Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{
    Json, Router,
    extract::Path,
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use sr_chat::repo::UserSummary;
use sr_chat::settings::applications::{
    Answer, Application, ApplicationForm, ApplicationStatus, Question, QuestionKind,
};
use sr_core::permissions::Permissions;
use sr_core::{ErrorCode, GuildId, UserId};
use std::collections::HashMap;
use utoipa::{IntoParams, ToSchema};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/guilds/{guild_id}/sr/applications/form",
            get(get_form).put(put_form),
        )
        .route(
            "/guilds/{guild_id}/sr/applications",
            get(list_applications).post(submit_application),
        )
        .route(
            "/guilds/{guild_id}/sr/applications/@me",
            get(get_my_application),
        )
        .route(
            "/guilds/{guild_id}/sr/applications/{application_id}/decision",
            post(decide_application),
        )
}

// ---- form -------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct QuestionDto {
    /// Omit for a new question; the server assigns one.
    #[serde(default)]
    pub id: Option<String>,
    pub label: String,
    /// `short` (one line) or `paragraph`.
    #[serde(default = "short")]
    pub kind: String,
    #[serde(default)]
    pub required: bool,
}

fn short() -> String {
    "short".into()
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PutFormBody {
    /// Whether people may apply.
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub questions: Vec<QuestionDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FormObject {
    pub guild_id: String,
    pub enabled: bool,
    pub description: Option<String>,
    pub questions: Vec<QuestionDto>,
}

impl FormObject {
    fn of(f: &ApplicationForm) -> Self {
        Self {
            guild_id: f.guild_id.to_string(),
            enabled: f.enabled,
            description: f.description.clone(),
            questions: f
                .questions
                .iter()
                .map(|q| QuestionDto {
                    id: Some(q.id.to_string()),
                    label: q.label.clone(),
                    kind: match q.kind {
                        QuestionKind::Short => "short",
                        QuestionKind::Paragraph => "paragraph",
                    }
                    .into(),
                    required: q.required,
                })
                .collect(),
        }
    }
}

/// The applications form. Open to anyone signed in while applications are on;
/// managers always see it.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/sr/applications/form",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = FormObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_form(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<FormObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let form = deps.service.application_form(gid).await.map_err(se)?;
    if !form.enabled {
        // Closed: only managers may look, and everyone else cannot tell the server exists.
        let acc = deps.guild_access(user, gid).await?;
        if !acc.perms.has(Permissions::MANAGE_GUILD) {
            return Err(ErrorCode::MissingPermissions.into());
        }
    } else if deps.chat.get_guild(gid).await.map_err(re)?.is_none() {
        return Err(ErrorCode::UnknownGuild.into());
    }
    Ok(Json(FormObject::of(&form)))
}

/// Save the form (`MANAGE_GUILD`): up to 5 questions.
#[utoipa::path(
    put,
    path = "/guilds/{guild_id}/sr/applications/form",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("X-Audit-Log-Reason" = Option<String>, Header, description = "Stored in the audit log"),
    ),
    request_body = PutFormBody,
    responses((status = 200, body = FormObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn put_form(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<PutFormBody>,
) -> ApiResult<Json<FormObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    let mut questions = Vec::with_capacity(body.questions.len());
    for q in body.questions {
        questions.push(Question {
            id: match &q.id {
                Some(s) => id("questions", s)?,
                None => 0,
            },
            label: q.label,
            kind: match q.kind.as_str() {
                "short" => QuestionKind::Short,
                "paragraph" => QuestionKind::Paragraph,
                _ => {
                    return Err(invalid(
                        "questions",
                        "ENUM_TYPE_INVALID",
                        "Kind must be short or paragraph.",
                    ));
                }
            },
            required: q.required,
        });
    }
    let saved = deps
        .service
        .save_application_form(
            acc.perms,
            user,
            ApplicationForm {
                guild_id: gid,
                enabled: body.enabled,
                description: body.description,
                questions,
            },
        )
        .await
        .map_err(se)?;
    Ok(Json(FormObject::of(&saved)))
}

// ---- applications -----------------------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
pub struct AnswerDto {
    pub question_id: String,
    pub value: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApplicationObject {
    pub id: String,
    pub guild_id: String,
    pub user: UserObject,
    pub answers: Vec<AnswerDto>,
    /// `pending`, `approved` or `rejected`.
    pub status: String,
    pub created_at: String,
    pub reviewed_by: Option<String>,
    pub reviewed_at: Option<String>,
    pub rejection_reason: Option<String>,
}

impl ApplicationObject {
    fn of(a: &Application, users: &HashMap<UserId, UserSummary>) -> Self {
        Self {
            id: a.id.to_string(),
            guild_id: a.guild_id.to_string(),
            user: UserObject::new(a.user_id, users),
            answers: a
                .answers
                .iter()
                .map(|x| AnswerDto {
                    question_id: x.question_id.to_string(),
                    value: x.value.clone(),
                })
                .collect(),
            status: a.status.as_str().into(),
            created_at: iso8601(a.created_at_ms),
            reviewed_by: a.reviewed_by.map(|u| u.to_string()),
            reviewed_at: a.reviewed_at_ms.map(iso8601),
            rejection_reason: a.rejection_reason.clone(),
        }
    }
}

async fn render(deps: &SettingsDeps, apps: &[Application]) -> ApiResult<Vec<ApplicationObject>> {
    let ids: Vec<UserId> = apps.iter().map(|a| a.user_id).collect();
    let users: HashMap<UserId, UserSummary> = deps
        .chat
        .users(&ids)
        .await
        .map_err(re)?
        .into_iter()
        .map(|u| (u.id, u))
        .collect();
    Ok(apps
        .iter()
        .map(|a| ApplicationObject::of(a, &users))
        .collect())
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AnswerBody {
    pub question_id: String,
    pub value: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SubmitBody {
    #[serde(default)]
    pub answers: Vec<AnswerBody>,
}

/// Apply to join a server that has applications open. One application at a time.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/sr/applications",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = SubmitBody,
    responses((status = 201, body = ApplicationObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn submit_application(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<SubmitBody>,
) -> ApiResult<(StatusCode, Json<ApplicationObject>)> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    if deps.chat.get_guild(gid).await.map_err(re)?.is_none() {
        return Err(ErrorCode::UnknownGuild.into());
    }
    let is_member = deps.chat.get_member(gid, user).await.map_err(re)?.is_some();
    let mut answers = Vec::with_capacity(body.answers.len());
    for a in body.answers {
        answers.push(Answer {
            question_id: id("answers", &a.question_id)?,
            value: a.value,
        });
    }
    let app = deps
        .service
        .submit_application(gid, user, &answers, is_member)
        .await
        .map_err(se)?;
    let mut out = render(&deps, std::slice::from_ref(&app)).await?;
    Ok((StatusCode::CREATED, Json(out.remove(0))))
}

/// The caller's latest application to this server (404 when they never applied).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/sr/applications/@me",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = ApplicationObject), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_my_application(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<ApplicationObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let app = deps
        .service
        .my_application(gid, user)
        .await
        .map_err(se)?
        .ok_or(ErrorCode::SrNotFound)?;
    let mut out = render(&deps, std::slice::from_ref(&app)).await?;
    Ok(Json(out.remove(0)))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListQuery {
    /// `pending`, `approved` or `rejected`; all when absent.
    pub status: Option<String>,
    /// Application id to page from (newest first).
    pub before: Option<String>,
    /// 1 to 100 (default 50).
    pub limit: Option<usize>,
}

/// The review queue, newest first (`MANAGE_GUILD`).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/sr/applications",
    params(("guild_id" = String, Path, description = "Guild snowflake"), ListQuery),
    responses((status = 200, body = Vec<ApplicationObject>), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_applications(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
    ApiQuery(q): ApiQuery<ListQuery>,
) -> ApiResult<Json<Vec<ApplicationObject>>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    let status = match q.status.as_deref() {
        None => None,
        Some(s) => Some(ApplicationStatus::parse(s).ok_or_else(|| {
            invalid(
                "status",
                "ENUM_TYPE_INVALID",
                "Status must be pending, approved or rejected.",
            )
        })?),
    };
    let before: Option<u64> = q.before.as_deref().map(|b| id("before", b)).transpose()?;
    let apps = deps
        .service
        .list_applications(acc.perms, gid, status, before, q.limit.unwrap_or(50))
        .await
        .map_err(se)?;
    Ok(Json(render(&deps, &apps).await?))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DecisionBody {
    /// `true` admits the applicant, `false` rejects.
    pub approve: bool,
    /// Why it was rejected, shown to the applicant (512 characters).
    #[serde(default)]
    pub reason: Option<String>,
}

/// Approve or reject a pending application (`MANAGE_GUILD`). Approving adds the member.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/sr/applications/{application_id}/decision",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("application_id" = String, Path, description = "Application snowflake"),
    ),
    request_body = DecisionBody,
    responses((status = 200, body = ApplicationObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn decide_application(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path((guild_id, application_id)): Path<(String, String)>,
    ApiJson(body): ApiJson<DecisionBody>,
) -> ApiResult<Json<ApplicationObject>> {
    let reviewer = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let app_id: u64 = id("application_id", &application_id)?;
    let acc = deps.guild_access(reviewer, gid).await?;
    let app = deps
        .service
        .decide_application(
            acc.perms,
            gid,
            app_id,
            body.approve,
            reviewer,
            body.reason.as_deref(),
        )
        .await
        .map_err(se)?;
    if app.status == ApplicationStatus::Approved {
        announce_join(&deps, &app).await;
    }
    let mut out = render(&deps, std::slice::from_ref(&app)).await?;
    Ok(Json(out.remove(0)))
}

/// `GUILD_MEMBER_ADD` for the new member. The row is committed, so the payload is best
/// effort and a missing member is skipped.
async fn announce_join(deps: &SettingsDeps, app: &Application) {
    let member = match deps.chat.get_member(app.guild_id, app.user_id).await {
        Ok(Some(m)) => m,
        Ok(None) => return,
        Err(e) => {
            tracing::warn!(error = %e, "member lookup for an approved application failed");
            return;
        }
    };
    let users: HashMap<UserId, UserSummary> = match deps.chat.users(&[app.user_id]).await {
        Ok(v) => v.into_iter().map(|u| (u.id, u)).collect(),
        Err(e) => {
            tracing::warn!(error = %e, "user lookup for GUILD_MEMBER_ADD failed");
            HashMap::new()
        }
    };
    let payload =
        serde_json::to_value(MemberObject::new(&member, true, &users)).unwrap_or_default();
    deps.publish(SettingsEvent::MemberJoined {
        guild: app.guild_id,
        user: app.user_id,
        payload,
    })
    .await;
}
