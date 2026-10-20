//! Member bulk self-delete of messages and uploads (N-05, D-324, doc 09 section 1).
//!
//! * `GET /sr/users/@me/messages/delete-all?servers=&dms=&before_ms=` is the number of messages a
//!   run with that scope would hide now, and the last run.
//! * `POST /sr/users/@me/messages/delete-all` queues the run (job `user.messages_self_delete`). It
//!   needs the typed username, the password and, with two-factor on, a second factor, like account
//!   deletion. 409 while a run is queued or ran in the last 24 hours; 202 with the run.
//!
//! The rows go to `user_deleted` with the 30-day purge of D-121; rows under a moderation hold are
//! hidden but never purged (D-122). The store behind the routes is [`SelfDeleteStore`] (Postgres in
//! `sr-server`, which also runs the job, writes the access-log row and DMs the count).

use super::{
    common::{R, Svc},
    identity::{MfaProof, require_second_factor},
    mfa::{current_account, reauthenticate},
    sessions::session_id,
};
use crate::routes::{Ext, messages::model::iso};
use crate::{auth::UserOnly, openapi::ErrorBody, state::AppState};
use async_trait::async_trait;
use axum::{Json, extract::Query, http::StatusCode};
use serde::{Deserialize, Serialize};
use sr_core::{ApiError, ErrorCode};
use std::sync::Arc;
use utoipa::{OpenApi, ToSchema};

/// What a run covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelfDeleteScope {
    pub servers: bool,
    pub dms: bool,
    pub before_ms: Option<i64>,
}

/// One run as the store keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfDeleteRun {
    pub id: i64,
    pub state: String,
    pub scope: SelfDeleteScope,
    pub requested_ms: i64,
    pub finished_ms: Option<i64>,
    pub hidden_count: Option<i64>,
}

/// Why a request was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelfDeleteRefusal {
    InProgress,
    TooSoon { retry_after_secs: i64 },
}

/// Persistence behind the routes.
#[async_trait]
pub trait SelfDeleteStore: Send + Sync {
    async fn count(&self, user: i64, scope: SelfDeleteScope) -> Result<i64, String>;
    async fn latest(&self, user: i64) -> Result<Option<SelfDeleteRun>, String>;
    async fn request(
        &self,
        user: i64,
        scope: SelfDeleteScope,
    ) -> Result<Result<SelfDeleteRun, SelfDeleteRefusal>, String>;
}

/// Collaborators of the routes, registered on the composition root.
pub struct SelfDeleteDeps {
    pub store: Arc<dyn SelfDeleteStore>,
}

/// A run.
#[derive(Debug, Serialize, ToSchema)]
#[schema(as = MessagesSelfDeleteRun)]
pub struct RunObject {
    pub id: String,
    /// `queued`, `running`, `done` or `failed`.
    pub state: String,
    pub servers: bool,
    pub dms: bool,
    pub before_ms: Option<i64>,
    pub requested_at: String,
    pub finished_at: Option<String>,
    /// Messages hidden, once done.
    pub hidden_count: Option<i64>,
}

impl From<&SelfDeleteRun> for RunObject {
    fn from(r: &SelfDeleteRun) -> Self {
        Self {
            id: r.id.to_string(),
            state: r.state.clone(),
            servers: r.scope.servers,
            dms: r.scope.dms,
            before_ms: r.scope.before_ms,
            requested_at: iso(r.requested_ms),
            finished_at: r.finished_ms.map(iso),
            hidden_count: r.hidden_count,
        }
    }
}

/// What a run would hide, and the last run.
#[derive(Debug, Serialize, ToSchema)]
#[schema(as = MessagesSelfDeletePreview)]
pub struct PreviewObject {
    /// Visible messages inside the scope right now (messages under a hold are included: they are
    /// hidden from everyone but never purged).
    pub count: i64,
    pub last: Option<RunObject>,
}

#[derive(Debug, Deserialize)]
pub struct PreviewQuery {
    pub servers: Option<bool>,
    pub dms: Option<bool>,
    pub before_ms: Option<i64>,
}

/// Body of `POST /sr/users/@me/messages/delete-all`.
#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = MessagesSelfDeleteBody)]
pub struct DeleteAllBody {
    /// The account's username, typed to confirm (case-insensitive).
    confirm_username: String,
    /// Current password.
    password: String,
    /// Second factor, when the account has one and this session has not passed a check in the last 15 minutes.
    #[serde(default)]
    mfa: Option<MfaProof>,
    /// Hide messages in servers (including servers you left).
    servers: bool,
    /// Hide messages in direct messages and group DMs.
    dms: bool,
    /// Only messages older than this (Unix milliseconds).
    #[serde(default)]
    before_ms: Option<i64>,
}

fn internal(e: String) -> crate::error::HttpError {
    tracing::error!(error = %e, "message self-delete store failure");
    ErrorCode::SrInternal.into()
}

/// How many messages a run would hide, and the last run.
#[utoipa::path(
    get,
    path = "/sr/users/@me/messages/delete-all",
    params(
        ("servers" = Option<bool>, Query, description = "Count messages in servers (default true)"),
        ("dms" = Option<bool>, Query, description = "Count messages in direct messages (default true)"),
        ("before_ms" = Option<i64>, Query, description = "Only messages older than this"),
    ),
    responses((status = 200, body = PreviewObject), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn preview(
    UserOnly(p): UserOnly,
    Ext(deps): Ext<SelfDeleteDeps>,
    Query(q): Query<PreviewQuery>,
) -> R<Json<PreviewObject>> {
    let scope = SelfDeleteScope {
        servers: q.servers.unwrap_or(true),
        dms: q.dms.unwrap_or(true),
        before_ms: q.before_ms,
    };
    let count = if scope.servers || scope.dms {
        deps.store.count(p.user_id, scope).await.map_err(internal)?
    } else {
        0
    };
    let last = deps.store.latest(p.user_id).await.map_err(internal)?;
    Ok(Json(PreviewObject {
        count,
        last: last.as_ref().map(RunObject::from),
    }))
}

/// Hide every message and upload of yours in the chosen places. Runs in the background.
#[utoipa::path(
    post,
    path = "/sr/users/@me/messages/delete-all",
    request_body = DeleteAllBody,
    responses(
        (status = 202, body = RunObject),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "A run is open, or one ran in the last 24 hours"),
    ),
    security(("session" = []))
)]
pub async fn delete_all(
    axum::extract::State(state): axum::extract::State<AppState>,
    UserOnly(p): UserOnly,
    Svc { svc, .. }: Svc,
    Ext(deps): Ext<SelfDeleteDeps>,
    Json(body): Json<DeleteAllBody>,
) -> R<(StatusCode, Json<RunObject>)> {
    let sid = session_id(&p)?;
    let now = state.now_secs();
    let account = current_account(&svc, p.user_id).await?;
    if !body
        .confirm_username
        .trim()
        .eq_ignore_ascii_case(&account.username)
    {
        return Err(ApiError::invalid_form()
            .field(
                "confirm_username",
                "mismatch",
                "Type your username exactly to confirm.",
            )
            .into());
    }
    if !body.servers && !body.dms {
        return Err(ApiError::invalid_form()
            .field(
                "servers",
                "required",
                "Choose servers, direct messages or both.",
            )
            .into());
    }
    reauthenticate(&svc, now, &account, &body.password).await?;
    require_second_factor(&svc, now, &account, sid, body.mfa.as_ref()).await?;
    let scope = SelfDeleteScope {
        servers: body.servers,
        dms: body.dms,
        before_ms: body.before_ms,
    };
    match deps
        .store
        .request(account.id, scope)
        .await
        .map_err(internal)?
    {
        Ok(run) => Ok((StatusCode::ACCEPTED, Json(RunObject::from(&run)))),
        Err(SelfDeleteRefusal::InProgress) => Err(ApiError::with_message(
            ErrorCode::SrConflict,
            "A bulk delete is already running for your account.",
        )
        .into()),
        Err(SelfDeleteRefusal::TooSoon { retry_after_secs }) => Err(ApiError::with_message(
            ErrorCode::SrConflict,
            format!(
                "You can run this once every 24 hours. Try again in {} hours.",
                (retry_after_secs + 3599) / 3600
            ),
        )
        .into()),
    }
}

/// OpenAPI fragment.
#[derive(OpenApi)]
#[openapi(
    paths(preview, delete_all),
    components(schemas(RunObject, PreviewObject, DeleteAllBody))
)]
pub struct SelfDeleteApiDoc;
