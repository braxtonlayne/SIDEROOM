//! Donations API: invoices, Patron and server support, leaderboards (member side) and
//! `/admin/donations/*` (staff with `MANAGE_DONATIONS`). Contract: `docs/specs/donations.md`
//! section 6. The logic lives in `sr-payments`; this module is the HTTP layer.
//!
//! Wiring, in the composition root:
//!
//! ```ignore
//! let service = Arc::new(PaymentService::new(repo, chain, rates, Arc::new(BusPaymentEvents::new(bus)), ents, clock, ids));
//! let state = AppState::builder().extension(Arc::new(DonationsDeps::new(service))).build();
//! ```
//! Admin screens additionally need [`AdminDeps`](crate::routes::admin::AdminDeps).

mod admin;
mod dto;
mod member;

#[cfg(test)]
mod tests;

use crate::{
    auth::UserOnly,
    error::HttpError,
    openapi::ErrorBody,
    routes::{
        Ext,
        admin::{StaffCtx, map_err as admin_err},
    },
    state::AppState,
};
use axum::{
    Router,
    extract::FromRequestParts,
    http::request::Parts,
    routing::{delete, get, patch, post, put},
};
use sr_core::{ApiError, ErrorCode, PlatformPermissions as P};
use sr_payments::{PayError, PaymentService};
use std::{sync::Arc, time::Duration};
use utoipa::OpenApi;

/// Leaderboard pages by `(board, window)`: the time they were built and the ranked items.
pub(crate) type BoardCache = moka::future::Cache<(String, String), Arc<(i64, Vec<dto::BoardItem>)>>;
/// Contributor boards by kind.
pub(crate) type ContributorCache =
    moka::future::Cache<String, Arc<(i64, Vec<dto::ContributorItem>)>>;

/// Collaborators of the donation endpoints.
pub struct DonationsDeps {
    pub service: Arc<PaymentService>,
    /// Leaderboard pages, cached hourly (DON-LB-001).
    pub(crate) boards: BoardCache,
    /// Contributor board pages, cached hourly (DON-LB-001).
    pub(crate) contributors: ContributorCache,
    /// The chat module, bound late (member refresh when the badge toggle changes, D-256).
    pub(crate) chat: ChatSlot,
}

impl DonationsDeps {
    pub fn new(service: Arc<PaymentService>) -> Self {
        Self {
            service,
            chat: ChatSlot::default(),
            boards: moka::future::Cache::builder()
                .max_capacity(16)
                .time_to_live(Duration::from_secs(3600))
                .build(),
            contributors: moka::future::Cache::builder()
                .max_capacity(8)
                .time_to_live(Duration::from_secs(3600))
                .build(),
        }
    }

    /// Share the slot the composition root fills with the chat module once it exists.
    #[must_use]
    pub fn with_chat_slot(mut self, chat: ChatSlot) -> Self {
        self.chat = chat;
        self
    }

    /// A cache with a different lifetime (tests).
    pub fn with_board_ttl(mut self, ttl: Duration) -> Self {
        self.boards = moka::future::Cache::builder()
            .max_capacity(16)
            .time_to_live(ttl)
            .build();
        self.contributors = moka::future::Cache::builder()
            .max_capacity(8)
            .time_to_live(ttl)
            .build();
        self
    }
}

/// A signed-in member.
pub(crate) struct DonorCtx {
    pub deps: Arc<DonationsDeps>,
    pub user: i64,
}

impl DonorCtx {
    pub fn svc(&self) -> &PaymentService {
        &self.deps.service
    }
}

impl FromRequestParts<AppState> for DonorCtx {
    type Rejection = HttpError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let UserOnly(p) = UserOnly::from_request_parts(parts, state).await?;
        let Ext(deps) = Ext::<DonationsDeps>::from_request_parts(parts, state).await?;
        Ok(Self {
            deps,
            user: p.user_id,
        })
    }
}

/// Staff with `MANAGE_DONATIONS` (checked per handler so a denial is logged with its action).
pub(crate) struct MoneyCtx {
    pub deps: Arc<DonationsDeps>,
    pub staff: StaffCtx,
}

impl MoneyCtx {
    pub fn svc(&self) -> &PaymentService {
        &self.deps.service
    }

    pub fn user(&self) -> i64 {
        self.staff.caller.user_id
    }

    pub async fn require(&self, action: &str) -> Result<(), HttpError> {
        self.staff
            .svc()
            .authorize(&self.staff.caller, P::MANAGE_DONATIONS, action)
            .await
            .map_err(admin_err)
    }

    /// A trimmed, mandatory reason for a state-changing action.
    pub fn reason(reason: Option<&str>) -> Result<String, HttpError> {
        sr_admin::AdminService::require_reason(reason).map_err(admin_err)
    }

    pub async fn log(
        &self,
        action: &str,
        target: Option<(&str, i64)>,
        reason: &str,
        detail: serde_json::Value,
    ) -> Result<(), HttpError> {
        self.staff
            .svc()
            .record(&self.staff.caller, action, target, reason, detail)
            .await
            .map_err(admin_err)
    }

    pub async fn log_read(
        &self,
        action: &str,
        target: Option<(&str, i64)>,
    ) -> Result<(), HttpError> {
        self.staff
            .svc()
            .record_read(&self.staff.caller, action, target, serde_json::json!({}))
            .await
            .map_err(admin_err)
    }
}

impl FromRequestParts<AppState> for MoneyCtx {
    type Rejection = HttpError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let staff = StaffCtx::from_request_parts(parts, state).await?;
        let Ext(deps) = Ext::<DonationsDeps>::from_request_parts(parts, state).await?;
        Ok(Self { deps, staff })
    }
}

/// Parse a snowflake path or body value.
pub(crate) fn id(raw: &str, what: &'static str) -> Result<i64, HttpError> {
    raw.parse::<i64>().ok().filter(|v| *v > 0).ok_or_else(|| {
        ApiError::with_message(ErrorCode::SrNotFound, format!("Unknown {what}")).into()
    })
}

pub(crate) fn map_pay(e: PayError) -> HttpError {
    match e {
        PayError::Disabled => ErrorCode::SrFeatureDisabled.into(),
        PayError::Invalid(m) => ApiError::with_message(ErrorCode::InvalidFormBody, m).into(),
        PayError::Descriptor(d) => {
            ApiError::with_message(ErrorCode::InvalidFormBody, d.to_string()).into()
        }
        PayError::NotFound => ErrorCode::SrNotFound.into(),
        PayError::Conflict(m) => ApiError::with_message(ErrorCode::SrConflict, m).into(),
        PayError::GapLimit | PayError::RateUnavailable | PayError::Chain(_) => {
            ApiError::with_message(ErrorCode::ApiResourceOverloaded, e.to_string()).into()
        }
        PayError::TooManyOpen => ApiError::with_message(
            ErrorCode::TooManyRequests,
            "You have too many unpaid invoices. Pay or cancel one first.",
        )
        .into(),
        PayError::Storage(m) => {
            tracing::error!(error = %m, "donations storage failure");
            ErrorCode::SrInternal.into()
        }
    }
}

/// Delivers `SR_PAYMENT_UPDATE` and `SR_ENTITLEMENT_UPDATE` over the event bus.
/// Late-bound handle to the chat module (it is built after the payments runtime).
pub type ChatSlot = Arc<std::sync::OnceLock<Arc<crate::routes::messages::MessagesDeps>>>;

pub struct BusPaymentEvents {
    bus: Arc<dyn sr_events::EventBus>,
    chat: ChatSlot,
}

impl BusPaymentEvents {
    pub fn new(bus: Arc<dyn sr_events::EventBus>) -> Self {
        Self {
            bus,
            chat: ChatSlot::default(),
        }
    }

    /// Share the slot the composition root fills with the chat module once it exists.
    #[must_use]
    pub fn with_chat_slot(mut self, chat: ChatSlot) -> Self {
        self.chat = chat;
        self
    }

    async fn to_user(&self, user: i64, name: &str, payload: serde_json::Value) {
        let Ok(uid) = u64::try_from(user) else { return };
        let uid = sr_core::UserId::new(uid);
        let ev = sr_events::DomainEvent::UserScoped {
            users: vec![uid],
            name: name.to_owned(),
            payload,
        };
        if let Err(e) = self.bus.publish(sr_events::Topic::User(uid), ev).await {
            tracing::warn!(error = %e, event = name, "publishing a donation event failed");
        }
    }
}

#[async_trait::async_trait]
impl sr_payments::service::PaymentEvents for BusPaymentEvents {
    async fn payment_update(&self, user_id: i64, payload: serde_json::Value) {
        // A newly credited invoice goes to the Staff Server's `#donations` feed (ADM-ALR-002).
        // The card names no donor: staff with MANAGE_DONATIONS open the invoice to see who paid.
        let state = payload.get("state").and_then(|s| s.as_str()).unwrap_or("");
        if matches!(state, "confirmed" | "overpaid" | "late_paid")
            && let Some(invoice) = payload.get("invoice_id").and_then(|s| s.as_str())
        {
            let sats = payload
                .get("confirmed_sats")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0);
            let ev = sr_events::DomainEvent::StaffFeed {
                feed: sr_events::StaffFeedKind::Donation,
                title: "Donation confirmed".to_owned(),
                detail: format!("{sats} sats credited ({state})."),
                link: Some(format!("/admin/donations/invoices/{invoice}")),
            };
            if let Err(e) = self.bus.publish(sr_events::Topic::Platform, ev).await {
                tracing::warn!(error = %e, "publishing a donation feed card failed");
            }
        }
        self.to_user(user_id, sr_payments::PAYMENT_EVENT, payload)
            .await;
    }

    async fn entitlement_user(&self, user_id: i64, payload: serde_json::Value) {
        self.to_user(user_id, sr_payments::ENTITLEMENT_EVENT, payload)
            .await;
        // The Patron badge and ring ride on the user object (D-256): shared servers get the
        // member again.
        refresh_patron_flag(&self.chat, user_id).await;
    }

    async fn entitlement_guild(&self, guild_id: i64, payload: serde_json::Value) {
        let Ok(g) = u64::try_from(guild_id) else {
            return;
        };
        let guild = sr_core::GuildId::new(g);
        let ev = sr_events::DomainEvent::GuildEntitlementUpdated { guild, payload };
        if let Err(e) = self.bus.publish(sr_events::Topic::Guild(guild), ev).await {
            tracing::warn!(error = %e, "publishing SR_ENTITLEMENT_UPDATE failed");
        }
    }

    async fn server_supported(&self, guild_id: i64, user_id: i64, level: Option<u8>) {
        let (Some(chat), Ok(g), Ok(u)) = (
            self.chat.get(),
            u64::try_from(guild_id),
            u64::try_from(user_id),
        ) else {
            return;
        };
        crate::routes::messages::system::post_server_support(
            chat,
            sr_core::GuildId::new(g),
            sr_core::UserId::new(u),
            level,
        )
        .await;
    }

    async fn staff_alert(&self, detail: String) {
        let ev = sr_events::DomainEvent::StaffAlert {
            kind: sr_events::StaffAlertKind::PaymentAttention,
            detail,
        };
        if let Err(e) = self.bus.publish(sr_events::Topic::Platform, ev).await {
            tracing::warn!(error = %e, "publishing a payment staff alert failed");
        }
    }
}

/// Re-send the member objects of `user` to their servers once the chat module is up.
pub(crate) async fn refresh_patron_flag(chat: &ChatSlot, user: i64) {
    if let (Some(chat), Ok(u)) = (chat.get(), u64::try_from(user)) {
        chat.refresh_member_objects(sr_core::UserId::new(u)).await;
    }
}

/// Unix milliseconds now.
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Donation routes (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/donations/config", get(member::config))
        .route(
            "/donations/invoices",
            get(member::list_invoices).post(member::create_invoice),
        )
        .route("/donations/invoices/{invoice_id}", get(member::get_invoice))
        .route(
            "/donations/invoices/{invoice_id}/check",
            post(member::check_invoice),
        )
        .route(
            "/donations/invoices/{invoice_id}/cancel",
            post(member::cancel_invoice),
        )
        .route("/donations/me", get(member::me))
        .route(
            "/donations/me/preferences",
            patch(member::patch_preferences),
        )
        .route(
            "/guilds/{guild_id}/support",
            get(member::guild_support).patch(member::patch_guild_support),
        )
        .route("/donations/leaderboards/{board}", get(member::leaderboard))
        .route(
            "/donations/leaderboards/contributors/{kind}",
            get(member::contributor_leaderboard),
        )
        // Staff.
        .route("/admin/donations/status", get(admin::status))
        .route("/admin/donations/wallet", put(admin::set_wallet))
        .route("/admin/donations/wallet/test", post(admin::test_wallet))
        .route(
            "/admin/donations/config",
            get(admin::get_config).patch(admin::patch_config),
        )
        .route("/admin/donations/invoices", get(admin::invoices))
        .route(
            "/admin/donations/invoices/{invoice_id}",
            get(admin::invoice),
        )
        .route(
            "/admin/donations/invoices/{invoice_id}/resolve",
            post(admin::resolve),
        )
        .route(
            "/admin/donations/entitlements",
            get(admin::entitlements).post(admin::grant),
        )
        .route(
            "/admin/donations/entitlements/{grant_id}",
            delete(admin::revoke),
        )
        .route("/admin/donations/rates", get(admin::rates))
        .route("/admin/donations/report", get(admin::report))
        .route(
            "/admin/donations/leaderboard/{board}",
            get(admin::leaderboard),
        )
        .route("/admin/donations/leaderboard/hide", post(admin::hide))
}

/// OpenAPI fragment for the donation endpoints.
#[derive(OpenApi)]
#[openapi(
    paths(
        member::config,
        member::create_invoice,
        member::list_invoices,
        member::get_invoice,
        member::check_invoice,
        member::cancel_invoice,
        member::me,
        member::patch_preferences,
        member::guild_support,
        member::patch_guild_support,
        member::leaderboard,
        member::contributor_leaderboard,
        admin::status,
        admin::set_wallet,
        admin::test_wallet,
        admin::get_config,
        admin::patch_config,
        admin::invoices,
        admin::invoice,
        admin::resolve,
        admin::entitlements,
        admin::grant,
        admin::revoke,
        admin::rates,
        admin::report,
        admin::leaderboard,
        admin::hide,
    ),
    components(schemas(
        ErrorBody,
        dto::DonationConfigView,
        dto::PatronOffer,
        dto::LevelOffer,
        dto::RateView,
        dto::HostingView,
        dto::CreateInvoiceBody,
        dto::InvoiceView,
        dto::TxView,
        dto::InvoicePage,
        dto::MeView,
        dto::PatronView,
        dto::PrefsView,
        dto::PrefsBody,
        dto::GuildSupportView,
        dto::ContributorView,
        dto::GuildSupportPatch,
        dto::BoardView,
        dto::BoardItem,
        dto::ContributorBoardView,
        dto::ContributorItem,
        dto::ContributorRank,
        dto::MyRank,
        dto::StatusView,
        dto::WalletStatus,
        dto::WatcherView,
        dto::RateSourceView,
        dto::SetWalletBody,
        dto::WalletResult,
        dto::TestWalletBody,
        dto::TestWalletResult,
        dto::PatchConfigBody,
        dto::AdminInvoiceView,
        dto::AdminInvoicePage,
        dto::AuditView,
        dto::ResolveBody,
        dto::GrantBody,
        dto::GrantView,
        dto::GrantPage,
        dto::RatesView,
        dto::ReportView,
        dto::HideBody,
    ))
)]
struct Doc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    Doc::openapi()
}
