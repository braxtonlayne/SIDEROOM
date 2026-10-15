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
mod accounts_ext;
mod announce;
mod ctx;
mod invites;
mod private;
mod staff;
mod staff_ext;
mod system;

#[cfg(test)]
mod tests;

pub use ctx::{AdminDeps, ReasonBody, StaffCtx, map_err, reason as reason_from};

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
            "/admin/staff/offers",
            get(staff_ext::list_offers).post(staff_ext::create_offer),
        )
        .route(
            "/admin/staff/offers/{offer_id}",
            axum::routing::delete(staff_ext::cancel_offer),
        )
        .route("/staff/offers/{offer_id}/respond", post(staff_ext::respond))
        .route("/admin/staff/delegations", get(staff_ext::list_delegations))
        .route(
            "/admin/staff/{account_id}/delegation",
            put(staff_ext::delegate).delete(staff_ext::undelegate),
        )
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
        .route(
            "/admin/accounts/{account_id}/rename",
            post(accounts_ext::rename),
        )
        .route(
            "/admin/accounts/{account_id}/change-email",
            post(accounts_ext::change_email),
        )
        .route(
            "/admin/accounts/{account_id}/badges",
            post(accounts_ext::badges),
        )
        .route(
            "/admin/accounts/{account_id}/force-password-reset",
            post(accounts_ext::force_password_reset),
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
        .route(
            "/admin/reauth",
            get(private::reauth_status).post(private::reauth),
        )
        .route(
            "/admin/private/channels/{channel_id}/messages",
            get(private::messages),
        )
        .route(
            "/admin/private/accounts/{account_id}/dms",
            get(private::account_dms),
        )
        .route(
            "/admin/private/guilds/{guild_id}/channels",
            get(private::guild_channels),
        )
        .route(
            "/admin/announcements",
            get(announce::list).post(announce::create),
        )
        .route(
            "/admin/announcements/{announcement_id}",
            axum::routing::delete(announce::end),
        )
        .route("/announcements/active", get(announce::active))
        .route("/admin/alerts", get(announce::alerts))
        .route("/admin/alerts/{alert_id}/ack", post(announce::ack))
        .route("/admin/card-actions", post(announce::card_action))
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
        staff_ext::list_offers,
        staff_ext::create_offer,
        staff_ext::cancel_offer,
        staff_ext::respond,
        staff_ext::list_delegations,
        staff_ext::delegate,
        staff_ext::undelegate,
        accounts_ext::rename,
        accounts_ext::change_email,
        accounts_ext::badges,
        accounts_ext::force_password_reset,
        private::reauth_status,
        private::reauth,
        private::messages,
        private::account_dms,
        private::guild_channels,
        announce::list,
        announce::create,
        announce::end,
        announce::active,
        announce::alerts,
        announce::ack,
        announce::card_action,
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
        staff_ext::OfferBody,
        staff_ext::RespondBody,
        staff_ext::DelegateBody,
        accounts_ext::RenameBody,
        accounts_ext::ChangeEmailBody,
        accounts_ext::BadgesBody,
        accounts_ext::ForcePasswordResetResult,
        private::ReauthStatus,
        private::ReauthBody,
        announce::AnnounceBody,
        announce::CardActionBody,
        sr_admin::StaffOffer,
        sr_admin::OfferState,
        sr_admin::Delegation,
        sr_admin::Announcement,
        sr_admin::AnnouncementSeverity,
        sr_admin::AnnouncementInput,
        sr_admin::StaffAlertRow,
        sr_admin::PrivateChannel,
        sr_admin::PrivateEdit,
        sr_admin::PrivateMessage,
        sr_admin::PrivateMessagePage,
        sr_admin::CardActionResult,
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
