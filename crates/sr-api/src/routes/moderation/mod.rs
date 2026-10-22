//! Moderation API (Phase 7 step 1, `MOD-*`; docs/specs/moderation-compliance.md; D-209 to D-215):
//!
//! * members file reports (`POST /moderation/reports`, behind the `moderation.reports` flag) and read the status
//!   of their own (`GET /moderation/reports/mine`);
//! * a server's moderators (`MANAGE_MESSAGES`) work the cases of their server, platform staff
//!   (`MANAGE_CASES`) work the platform's: the case routes under `/moderation/cases/{id}` work for both, and
//!   [`sr_moderation::rules::access`] decides what each may see and do;
//! * holds and appeals are platform staff only (D-213, D-215);
//! * a banned person files an appeal from the sign-in screen with their credentials
//!   (`POST /moderation/appeals`), since the account has no session.
//!
//! The rules and the SQL live in `sr-moderation`; this module checks the caller, applies the channel
//! permissions to what a report points at, and maps errors.

mod age;
mod cases;
mod dashboard;
mod holds;
mod reports;
mod suspicion;
mod takedowns;
#[cfg(test)]
mod tests;

use crate::{
    auth::UserOnly,
    error::{ApiResult, HttpError},
    openapi::ErrorBody,
    routes::{Ext, MaybeExt, admin::StaffCtx, settings::deps::SettingsDeps},
    state::AppState,
};
use axum::{
    Router,
    extract::FromRequestParts,
    http::request::Parts,
    routing::{get, post, put},
};
use serde::Serialize;
use sr_core::{ApiError, ErrorCode, GuildId, UserId, permissions::Permissions};
use sr_features::{FeatureService, ScopeRef};
use sr_moderation::{
    ModError, Moderation,
    model::Who,
    rules::{self, CaseHead},
};
use std::sync::Arc;
use utoipa::{OpenApi, ToSchema};

/// The id of the flag that turns member reporting on (`sr_features::moderation`).
pub use sr_features::moderation::REPORTS_FEATURE_ID;

/// Collaborators of the moderation endpoints.
pub struct ModerationDeps {
    pub svc: Moderation,
    features: Option<FeatureService>,
}

impl ModerationDeps {
    pub fn new(svc: Moderation) -> Self {
        Self {
            svc,
            features: None,
        }
    }

    /// The feature service the `moderation.reports` flag is read from. Without it reporting is off.
    pub fn with_features(mut self, f: FeatureService) -> Self {
        self.features = Some(f);
        self
    }

    pub(crate) async fn reports_enabled(&self) -> bool {
        match &self.features {
            Some(f) => f
                .is_enabled(REPORTS_FEATURE_ID, &ScopeRef::Platform)
                .await
                .unwrap_or(false),
            None => false,
        }
    }
}

/// A moderation error as an API error.
pub(crate) fn map_err(e: ModError) -> HttpError {
    match e {
        ModError::NotFound(w) => {
            ApiError::with_message(ErrorCode::SrNotFound, format!("Unknown {w}")).into()
        }
        ModError::Forbidden => ErrorCode::MissingPermissions.into(),
        ModError::MissingPermission(p) => ApiError::with_message(
            ErrorCode::MissingPermissions,
            format!("Missing platform permission {p}"),
        )
        .into(),
        ModError::ReasonRequired => ErrorCode::SrReasonRequired.into(),
        ModError::Invalid(m) => ApiError::with_message(ErrorCode::InvalidFormBody, m).into(),
        ModError::Conflict(m) => ApiError::with_message(ErrorCode::SrConflict, m).into(),
        ModError::RateLimited => ApiError::with_message(
            ErrorCode::TooManyRequests,
            "You have filed too many reports today. Try again tomorrow.",
        )
        .into(),
        ModError::Storage(m) => {
            tracing::error!(error = %m, "moderation storage failure");
            ErrorCode::SrInternal.into()
        }
    }
}

/// A snowflake path segment; a malformed one names nothing.
pub(crate) fn id(raw: &str, what: &'static str) -> Result<i64, HttpError> {
    raw.parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| map_err(ModError::NotFound(what)))
}

/// Run a staff action; a missing permission is written to the access log as a denial (MOD-PERM-002).
pub(crate) async fn staff_result<T>(
    ctx: &StaffCtx,
    action: &str,
    target: Option<(&str, i64)>,
    r: Result<T, ModError>,
) -> ApiResult<T> {
    if let Err(ModError::MissingPermission(p)) = &r {
        let _ = ctx
            .svc()
            .record_denied(&ctx.caller, action, target, &format!("missing {p}"))
            .await;
    }
    r.map_err(map_err)
}

/// A signed-in person who may be platform staff, a server's moderator, or neither. The case routes decide per
/// case which hat applies ([`rules::pick_who`]).
pub(crate) struct Actor {
    pub user_id: i64,
    pub staff: Option<StaffCtx>,
    pub deps: Arc<ModerationDeps>,
    settings: Option<Arc<SettingsDeps>>,
}

impl Actor {
    pub fn svc(&self) -> &Moderation {
        &self.deps.svc
    }

    /// Whether the caller moderates `guild` (`MANAGE_MESSAGES`, owner and administrator included).
    async fn moderates(&self, guild: i64) -> bool {
        let (Ok(u), Ok(g)) = (u64::try_from(self.user_id), u64::try_from(guild)) else {
            return false;
        };
        let Some(settings) = &self.settings else {
            return false;
        };
        settings
            .guild_access(UserId::new(u), GuildId::new(g))
            .await
            .is_ok_and(|a| a.perms.has(Permissions::MANAGE_MESSAGES))
    }

    /// The hat to wear for case `case_id`, or 404 when neither reaches it.
    pub async fn who_for(&self, case_id: i64) -> ApiResult<Who<'_>> {
        let head: CaseHead = self.svc().case_head(case_id).await.map_err(map_err)?;
        let moderates = match head.guild_id {
            Some(g) if self.moderates(g).await => Some((self.user_id, g)),
            _ => None,
        };
        rules::pick_who(&head, self.staff.as_ref().map(|s| &s.caller), moderates)
            .ok_or_else(|| map_err(ModError::NotFound("case")))
    }

    /// The staff context for the actions only platform staff take.
    pub fn staff_only(&self) -> ApiResult<&StaffCtx> {
        self.staff
            .as_ref()
            .ok_or_else(|| ErrorCode::SrStaffOnly.into())
    }
}

impl FromRequestParts<AppState> for Actor {
    type Rejection = HttpError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let UserOnly(p) = UserOnly::from_request_parts(parts, state).await?;
        let Ext(deps) = Ext::<ModerationDeps>::from_request_parts(parts, state).await?;
        let MaybeExt(settings) = MaybeExt::<SettingsDeps>::from_request_parts(parts, state)
            .await
            .unwrap_or(MaybeExt(None));
        // A person who is not staff is simply a member here; only the staff routes demand the standing.
        let staff = StaffCtx::from_request_parts(parts, state).await.ok();
        Ok(Self {
            user_id: p.user_id,
            staff,
            deps,
            settings,
        })
    }
}

/// `{ "delivered": bool }`.
#[derive(Debug, Serialize, ToSchema)]
pub struct Delivered {
    /// Whether the system DM went out.
    pub delivered: bool,
}

/// Moderation routes (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/moderation/reports", post(reports::file_report))
        .route("/moderation/reports/mine", get(reports::my_reports))
        .route(
            "/moderation/cases",
            get(cases::list_cases).post(cases::open_case),
        )
        .route(
            "/moderation/guilds/{guild_id}/cases",
            get(cases::guild_cases),
        )
        .route("/moderation/cases/{case_id}", get(cases::get_case))
        .route("/moderation/cases/{case_id}/state", post(cases::set_state))
        .route("/moderation/cases/{case_id}/reopen", post(cases::reopen))
        .route("/moderation/cases/{case_id}/notes", post(cases::add_note))
        .route(
            "/moderation/cases/{case_id}/assignee",
            put(cases::claim).delete(cases::release),
        )
        .route(
            "/moderation/cases/{case_id}/escalate",
            post(cases::escalate),
        )
        .route(
            "/moderation/cases/{case_id}/actions",
            post(cases::record_action),
        )
        .route("/moderation/cases/{case_id}/warn", post(cases::warn))
        .route("/moderation/cases/{case_id}/ban", post(cases::ban))
        .route("/moderation/cases/{case_id}/holds", post(holds::place))
        .route("/moderation/holds", get(holds::list))
        .route("/moderation/holds/{hold_id}/release", post(holds::release))
        .route(
            "/moderation/holds/{hold_id}/reviewed",
            post(holds::reviewed),
        )
        .route("/moderation/holds/{hold_id}/retain", post(holds::retain))
        .route("/moderation/age/restrictions", post(age::restrict))
        .route("/moderation/age/underage", post(age::underage))
        .route(
            "/moderation/age/underage/{account_id}/lift",
            post(age::lift),
        )
        .route(
            "/moderation/appeals",
            get(holds::list_appeals).post(holds::file_appeal),
        )
        .route(
            "/moderation/takedowns",
            get(takedowns::list).post(takedowns::create),
        )
        .route("/moderation/takedowns/export", get(takedowns::export))
        .route("/moderation/takedowns/{notice_id}", get(takedowns::get))
        .route(
            "/moderation/takedowns/{notice_id}/items",
            post(takedowns::add_item),
        )
        .route(
            "/moderation/takedowns/{notice_id}/items/{item_id}",
            axum::routing::delete(takedowns::remove_item),
        )
        .route(
            "/moderation/takedowns/{notice_id}/hold",
            post(takedowns::hold),
        )
        .route(
            "/moderation/takedowns/{notice_id}/reject",
            post(takedowns::reject),
        )
        .route(
            "/moderation/takedowns/{notice_id}/uphold",
            post(takedowns::uphold),
        )
        .route(
            "/moderation/takedowns/{notice_id}/counter-notice",
            post(takedowns::counter_notice),
        )
        .route(
            "/moderation/takedowns/{notice_id}/court-filing",
            post(takedowns::court_filing),
        )
        .route(
            "/moderation/takedowns/{notice_id}/restore",
            post(takedowns::restore),
        )
        .route(
            "/moderation/takedowns/{notice_id}/withdraw",
            post(takedowns::withdraw),
        )
        .route("/moderation/item-status", get(takedowns::item_status))
        .route("/moderation/dashboard", get(dashboard::get))
        .route("/moderation/dashboard/inbox", get(dashboard::inbox))
        .route(
            "/moderation/dashboard/inbox/dismiss",
            post(dashboard::dismiss),
        )
        .route(
            "/moderation/persons/{person_id}/suspicion",
            get(suspicion::panel),
        )
        .route(
            "/moderation/persons/{person_id}/evasion-scan",
            post(suspicion::scan),
        )
        .route("/moderation/evasion-signals", get(suspicion::list))
        .route("/moderation/evasion-signals/count", get(suspicion::count))
        .route(
            "/moderation/evasion-signals/{signal_id}",
            get(suspicion::detail),
        )
        .route(
            "/moderation/evasion-signals/{signal_id}/link-case",
            post(suspicion::link_case),
        )
        .route(
            "/moderation/evasion-signals/{signal_id}/review",
            post(suspicion::review),
        )
        .route(
            "/moderation/appeals/{appeal_id}/decision",
            post(holds::decide_appeal),
        )
}

/// OpenAPI fragment for the moderation endpoints.
#[derive(OpenApi)]
#[openapi(
    paths(
        reports::file_report,
        reports::my_reports,
        cases::list_cases,
        cases::open_case,
        cases::guild_cases,
        cases::get_case,
        cases::set_state,
        cases::reopen,
        cases::add_note,
        cases::claim,
        cases::release,
        cases::escalate,
        cases::record_action,
        cases::warn,
        cases::ban,
        holds::place,
        holds::list,
        holds::release,
        holds::reviewed,
        holds::retain,
        age::restrict,
        age::underage,
        age::lift,
        holds::list_appeals,
        holds::file_appeal,
        holds::decide_appeal,
        takedowns::list,
        takedowns::create,
        takedowns::get,
        takedowns::add_item,
        takedowns::remove_item,
        takedowns::hold,
        takedowns::reject,
        takedowns::uphold,
        takedowns::counter_notice,
        takedowns::court_filing,
        takedowns::restore,
        takedowns::withdraw,
        takedowns::export,
        takedowns::item_status,
        dashboard::get,
        dashboard::inbox,
        dashboard::dismiss,
        suspicion::panel,
        suspicion::scan,
        suspicion::list,
        suspicion::detail,
        suspicion::review,
        suspicion::link_case,
        suspicion::count,
    ),
    components(schemas(
        sr_moderation::model::NewReport,
        sr_moderation::model::ReportReceipt,
        sr_moderation::model::MyReport,
        sr_moderation::model::CaseSummary,
        sr_moderation::model::CaseDetail,
        sr_moderation::model::CasePage,
        sr_moderation::model::CaseReport,
        sr_moderation::model::CaseEvidence,
        sr_moderation::model::CaseNote,
        sr_moderation::model::CaseEvent,
        sr_moderation::model::HoldView,
        sr_moderation::model::AppealView,
        sr_moderation::model::NewCase,
        sr_moderation::model::ReporterView,
        sr_moderation::model::ReportCategory,
        sr_moderation::model::TargetKind,
        sr_moderation::model::CaseState,
        sr_moderation::model::Routing,
        sr_moderation::model::CaseSeverity,
        sr_moderation::model::ReportState,
        sr_moderation::model::AppealDecision,
        cases::StateBody,
        cases::CaseNoteBody,
        cases::OpenCaseBody,
        cases::EscalateBody,
        cases::ReasonOnly,
        cases::ActionBody,
        cases::WarnBody,
        cases::CaseBanBody,
        cases::BanDone,
        holds::HoldBody,
        holds::HoldPlaced,
        holds::HoldReleased,
        holds::AppealBody,
        holds::AppealDecisionBody,
        holds::HoldRetainBody,
        age::RestrictionBody,
        age::UnderageBody,
        age::LiftBody,
        sr_moderation::AgeChange,
        sr_moderation::AgeTargetKind,
        sr_moderation::AgeBand,
        sr_moderation::UnderageOutcome,
        sr_moderation::model::NewNotice,
        sr_moderation::model::NewItem,
        sr_moderation::model::NoticeSummary,
        sr_moderation::model::NoticeDetail,
        sr_moderation::model::NoticeEvent,
        sr_moderation::model::TakedownItem,
        sr_moderation::model::StrikeCount,
        sr_moderation::model::UpholdOutcome,
        sr_moderation::model::RejectOutcome,
        sr_moderation::model::TakedownKind,
        sr_moderation::model::CaseTakedown,
        sr_moderation::model::CountedNotice,
        sr_moderation::model::NoticeState,
        sr_moderation::model::ItemKind,
        sr_moderation::model::ItemState,
        takedowns::TakedownReasonBody,
        takedowns::CounterNoticeBody,
        takedowns::CourtFilingBody,
        takedowns::TakedownItemStatus,
        sr_moderation::dashboard::DashboardView,
        sr_moderation::dashboard::DashInbox,
        sr_moderation::dashboard::DashHealth,
        sr_moderation::dashboard::DashDay,
        sr_moderation::dashboard::DashCount,
        sr_moderation::dashboard::DashRetention,
        sr_moderation::dashboard::DashRetentionRow,
        sr_moderation::dashboard::DashAge,
        sr_moderation::dashboard::DashBans,
        sr_moderation::dashboard::DashTakedowns,
        sr_moderation::dashboard::DashDataRequests,
        sr_moderation::dashboard::InboxGroup,
        sr_moderation::dashboard::InboxPage,
        sr_moderation::dashboard::InboxSkipped,
        sr_moderation::dashboard::InboxDismissOutcome,
        dashboard::InboxDismissBody,
        suspicion::SignalReviewBody,
        suspicion::LinkCaseBody,
        suspicion::SignalCount,
        sr_moderation::model::SuspicionPanel,
        sr_moderation::model::PanelEvent,
        sr_moderation::model::PersonAccountRef,
        sr_moderation::model::BannedDescendant,
        sr_moderation::model::InvitePath,
        sr_moderation::model::SuspicionCaseRef,
        sr_moderation::model::SignalView,
        sr_moderation::model::SignalDetail,
        sr_moderation::model::SharedAddress,
        sr_moderation::model::ScanOutcome,
        sr_moderation::model::SignalKind,
        sr_moderation::model::SignalState,
        Delivered,
        ErrorBody,
    ))
)]
struct ModerationApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ModerationApiDoc::openapi()
}
