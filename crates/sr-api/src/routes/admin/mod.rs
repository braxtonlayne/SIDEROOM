//! Platform admin REST API (`/admin/*`, ADM-* Phase 3).
//!
//! Every handler resolves the caller's platform standing on each request (an
//! open staff appointment, Staff Server membership and the union of their role
//! bits, see `sr_admin`), so removing someone from the Staff Server takes effect
//! on their next call. Permission checks, reason rules and audit logging live in
//! [`sr_admin::AdminService`]; these modules only translate HTTP.
//!
//! Wiring, in the composition root:
//!
//! ```ignore
//! let admin = AdminDeps::new(AdminService::new(Arc::new(PgAdminRepo::new(pool, ids))).with_bus(bus))
//!     .with_features(features.clone())
//!     .with_gateway_sessions(Arc::new(move || gateway.session_count()));
//! let state = AppState::builder().extension(Arc::new(admin)).build();
//! ```

mod accounts;
mod ctx;
mod invites;
mod staff;
mod system;

#[cfg(test)]
mod tests;

pub use ctx::{AdminDeps, StaffCtx};

use crate::{openapi::ErrorBody, state::AppState};
use axum::{
    Router,
    routing::{get, post, put},
};
use utoipa::OpenApi;

/// Admin routes (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/me", get(staff::me))
        .route("/admin/staff", get(staff::list).post(staff::appoint))
        .route("/admin/staff/roles", get(staff::roles))
        .route(
            "/admin/staff/{account_id}",
            axum::routing::delete(staff::revoke),
        )
        .route("/admin/staff/{account_id}/roles", put(staff::set_roles))
        .route("/admin/accounts", get(accounts::search))
        .route("/admin/accounts/{account_id}", get(accounts::detail))
        .route(
            "/admin/accounts/{account_id}/suspend",
            post(accounts::suspend),
        )
        .route(
            "/admin/accounts/{account_id}/unsuspend",
            post(accounts::unsuspend),
        )
        .route(
            "/admin/accounts/{account_id}/force-logout",
            post(accounts::force_logout),
        )
        .route(
            "/admin/accounts/{account_id}/reset-2fa",
            post(accounts::reset_2fa),
        )
        .route(
            "/admin/accounts/{account_id}/delete-request",
            post(accounts::delete_request),
        )
        .route(
            "/admin/accounts/{account_id}/ban",
            post(accounts::ban_account),
        )
        .route("/admin/persons/{person_id}", get(accounts::person))
        .route("/admin/persons/{person_id}/ban", post(accounts::ban_person))
        .route(
            "/admin/persons/{person_id}/unban",
            post(accounts::unban_person),
        )
        .route("/admin/bans", get(accounts::bans))
        .route(
            "/admin/accounts/{account_id}/invite-tree",
            get(invites::tree),
        )
        .route(
            "/admin/accounts/{account_id}/invite-descendants",
            get(invites::descendants),
        )
        .route(
            "/admin/accounts/{account_id}/credits",
            get(invites::ledger).post(invites::adjust),
        )
        .route("/admin/invites", get(invites::list))
        .route(
            "/admin/invites/{invite_id}",
            axum::routing::delete(invites::revoke),
        )
        .route("/admin/suspicion", get(invites::suspects))
        .route(
            "/admin/persons/{person_id}/clear-suspicion",
            post(invites::clear_suspicion),
        )
        .route(
            "/admin/settings",
            get(system::settings).patch(system::patch_settings),
        )
        .route("/admin/features", get(system::features))
        .route(
            "/admin/features/{feature_id}",
            axum::routing::patch(system::patch_feature),
        )
        .route("/admin/stats", get(system::stats))
        .route("/admin/system", get(system::system))
        .route("/admin/jobs", get(system::jobs))
        .route("/admin/jobs/{job_id}/retry", post(system::retry_job))
        .route(
            "/admin/jobs/{job_id}",
            axum::routing::delete(system::delete_job),
        )
        .route("/admin/access-log", get(system::access_log))
        .route("/admin/access-log/verify", get(system::verify_access_log))
}

/// OpenAPI fragment for the admin endpoints.
#[derive(OpenApi)]
#[openapi(
    paths(
        staff::me,
        staff::list,
        staff::roles,
        staff::appoint,
        staff::set_roles,
        staff::revoke,
        accounts::search,
        accounts::detail,
        accounts::suspend,
        accounts::unsuspend,
        accounts::force_logout,
        accounts::reset_2fa,
        accounts::delete_request,
        accounts::ban_account,
        accounts::person,
        accounts::ban_person,
        accounts::unban_person,
        accounts::bans,
        invites::tree,
        invites::descendants,
        invites::ledger,
        invites::adjust,
        invites::list,
        invites::revoke,
        invites::suspects,
        invites::clear_suspicion,
        system::settings,
        system::patch_settings,
        system::features,
        system::patch_feature,
        system::stats,
        system::system,
        system::jobs,
        system::retry_job,
        system::delete_job,
        system::access_log,
        system::verify_access_log,
    ),
    components(schemas(
        ErrorBody,
        ctx::ReasonBody,
        staff::Me,
        staff::AppointBody,
        staff::RolesBody,
        accounts::BanBody,
        accounts::PersonBanBody,
        accounts::ForceLogoutResult,
        invites::AdjustBody,
        invites::AdjustResult,
        system::PatchSettingsBody,
        system::FeatureView,
        system::PatchFeatureBody,
        system::SystemView,
        system::ProbeView,
        sr_admin::SettingView,
        sr_admin::SettingKind,
        sr_admin::AccountState,
        sr_admin::BanCategory,
        sr_admin::AccessKind,
        sr_admin::AccessLogEntry,
        sr_admin::AccessLogPage,
        sr_admin::ChainReport,
        sr_admin::StaffRole,
        sr_admin::StaffMember,
        sr_admin::AccountSummary,
        sr_admin::AccountPage,
        sr_admin::AccountDetail,
        sr_admin::LoginEvent,
        sr_admin::AccountRef,
        sr_admin::Reset2faReport,
        sr_admin::DeleteRequest,
        sr_admin::PersonDetail,
        sr_admin::BanRecord,
        sr_admin::BanPage,
        sr_admin::BanOutcome,
        sr_admin::SuspicionChange,
        sr_admin::UnbanOutcome,
        sr_admin::InviteNode,
        sr_admin::InviteStats,
        sr_admin::InviteTree,
        sr_admin::InviteDescendants,
        sr_admin::LedgerEntry,
        sr_admin::CreditLedger,
        sr_admin::InviteRow,
        sr_admin::InvitePage,
        sr_admin::SuspectPerson,
        sr_admin::SystemCounts,
        sr_admin::JobRow,
        sr_admin::JobPage,
    ))
)]
pub struct AdminApiDoc;

/// The OpenAPI document fragment for the admin endpoints.
pub fn openapi() -> utoipa::openapi::OpenApi {
    AdminApiDoc::openapi()
}
