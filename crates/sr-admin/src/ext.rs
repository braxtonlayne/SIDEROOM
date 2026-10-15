//! Types and ports added in phase 10: appointment offers and delegations
//! (ADM-STF-004), account edits (ADM-PPL-003), private content reads
//! (ADM-COM-004), platform announcements (ADM-PLT-009) and the staff alert
//! store (ADM-ALR-001). Persistence sits behind [`StaffExtrasRepository`]; the
//! Staff Server's system user speaks through a [`StaffNotifier`].

use crate::{
    error::AdminResult,
    model::{AuditEntry, id, opt_id},
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// ===== appointment offers ===============================================

/// State of an appointment offer (the system DM the appointee answers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OfferState {
    Pending,
    Accepted,
    Declined,
    Cancelled,
    Expired,
}

impl OfferState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Declined => "declined",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "pending" => Self::Pending,
            "accepted" => Self::Accepted,
            "declined" => Self::Declined,
            "cancelled" => Self::Cancelled,
            "expired" => Self::Expired,
            _ => return None,
        })
    }
}

/// How long an unanswered offer stays open.
pub const OFFER_TTL_MS: i64 = 7 * 24 * 3600 * 1000;

/// An offer to join the staff, answered by the appointee from a system DM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct StaffOffer {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub account_id: i64,
    pub username: String,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub offered_by: i64,
    pub offered_by_name: String,
    /// Staff Server role ids offered (decimal strings).
    pub role_ids: Vec<String>,
    pub role_names: Vec<String>,
    pub reason: String,
    pub state: OfferState,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
    pub responded_at_ms: Option<i64>,
    /// The system DM carrying the Accept and Decline buttons.
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub dm_channel_id: Option<i64>,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub dm_message_id: Option<i64>,
}

impl StaffOffer {
    pub fn role_ids_i64(&self) -> Vec<i64> {
        self.role_ids
            .iter()
            .filter_map(|r| r.parse().ok())
            .collect()
    }
}

/// Input of [`StaffExtrasRepository::create_offer`].
#[derive(Debug, Clone)]
pub struct NewOffer {
    pub account_id: i64,
    pub offered_by: i64,
    pub role_ids: Vec<i64>,
    pub reason: String,
    pub expires_at_ms: i64,
}

// ===== delegations ======================================================

/// The Owner's delegation of appointment rights (D-132): the delegate may appoint
/// staff, but only with permission bits inside `max_permissions` and that they hold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Delegation {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub account_id: i64,
    pub username: String,
    /// `PlatformPermissions` ceiling as a decimal string.
    pub max_permissions: String,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub granted_by: i64,
    pub granted_at_ms: i64,
    pub reason: String,
}

// ===== announcements ====================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnnouncementSeverity {
    Info,
    Warning,
    Critical,
}

impl AnnouncementSeverity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "info" => Self::Info,
            "warning" => Self::Warning,
            "critical" => Self::Critical,
            _ => return None,
        })
    }
}

/// A platform announcement: a banner every signed-in user sees, optionally also sent
/// as a DM from the system user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Announcement {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub title: String,
    pub body: String,
    pub severity: AnnouncementSeverity,
    /// Whether the system user also DMs it to every active account.
    pub send_dm: bool,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub created_by: i64,
    pub created_at_ms: i64,
    pub starts_at_ms: i64,
    pub ends_at_ms: Option<i64>,
    /// Set when staff end it early.
    pub ended_at_ms: Option<i64>,
}

impl Announcement {
    /// Is the banner showing at `now_ms`?
    pub fn is_active(&self, now_ms: i64) -> bool {
        self.ended_at_ms.is_none()
            && self.starts_at_ms <= now_ms
            && self.ends_at_ms.is_none_or(|e| now_ms < e)
    }
}

/// Input of `POST /admin/announcements`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct AnnouncementInput {
    pub title: String,
    pub body: String,
    pub severity: Option<AnnouncementSeverity>,
    /// Outage or security notice: cannot be dismissed and is always sent as a DM (D-192).
    /// Same as `severity: "critical"`.
    #[serde(default)]
    pub critical: bool,
    /// Also DM it to everyone who has not turned off `notifications.platform_dm`.
    #[serde(default)]
    pub send_dm: bool,
    /// Start showing at this time (default: now).
    pub starts_at_ms: Option<i64>,
    /// Stop showing at this time (default: never).
    pub ends_at_ms: Option<i64>,
}

/// Row for [`StaffExtrasRepository::create_announcement`].
#[derive(Debug, Clone)]
pub struct NewAnnouncement {
    pub title: String,
    pub body: String,
    pub severity: AnnouncementSeverity,
    pub send_dm: bool,
    pub created_by: i64,
    pub starts_at_ms: i64,
    pub ends_at_ms: Option<i64>,
}

// ===== alerts ===========================================================

/// A stored staff alert (what `#alerts` showed), kept for the dashboard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct StaffAlertRow {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub kind: String,
    pub detail: String,
    pub created_at_ms: i64,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub acked_by: Option<i64>,
    pub acked_at_ms: Option<i64>,
}

// ===== private content ==================================================

/// Where a private conversation lives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PrivateChannel {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    /// Discord channel type (1 DM, 3 group DM, 0 text, ...).
    pub kind: i16,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub guild_id: Option<i64>,
    pub name: Option<String>,
    /// Participants of a DM or group DM (empty for guild channels).
    pub recipients: Vec<crate::model::AccountRef>,
    /// Whether the channel is private to some members of its server.
    pub private: bool,
}

/// One earlier version of an edited message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PrivateEdit {
    pub edited_at_ms: i64,
    /// The content before this edit.
    pub previous_content: String,
}

/// A message as staff see it: also removed ones, with the edit history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PrivateMessage {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub author_id: i64,
    pub author_name: String,
    pub content: String,
    pub created_at_ms: i64,
    pub edited_at_ms: Option<i64>,
    /// `visible`, `user_deleted` or `staff_removed`.
    pub visibility: String,
    pub deleted_at_ms: Option<i64>,
    pub attachment_count: i32,
    pub edits: Vec<PrivateEdit>,
}

/// A page of a private conversation, newest first.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PrivateMessagePage {
    pub channel: PrivateChannel,
    pub items: Vec<PrivateMessage>,
    /// Pass as `before` for the next (older) page.
    pub next_before: Option<String>,
    /// Deleted messages older than this many days are gone for good.
    pub deleted_retention_days: i64,
}

// ===== ports ============================================================

/// Persistence of the phase-10 admin features. Like [`crate::AdminRepository`],
/// every state-changing method writes its [`AuditEntry`] in the same transaction.
#[async_trait]
pub trait StaffExtrasRepository: Send + Sync {
    // ---- appointment offers
    /// Fails with `Conflict` when the account already has a pending offer.
    async fn create_offer(&self, o: &NewOffer, audit: &AuditEntry) -> AdminResult<StaffOffer>;
    async fn get_offer(&self, id: i64) -> AdminResult<Option<StaffOffer>>;
    /// Newest first. `pending_only` hides answered and expired offers.
    async fn list_offers(&self, pending_only: bool, limit: i64) -> AdminResult<Vec<StaffOffer>>;
    /// Move a pending offer to `to`; `Conflict` if it is not pending any more.
    async fn close_offer(
        &self,
        id: i64,
        to: OfferState,
        audit: &AuditEntry,
    ) -> AdminResult<StaffOffer>;
    /// Remember the system DM that carries the buttons.
    async fn set_offer_message(&self, id: i64, channel_id: i64, message_id: i64)
    -> AdminResult<()>;
    /// Mark pending offers past their expiry; returns them.
    async fn expire_offers(&self, now_ms: i64) -> AdminResult<Vec<StaffOffer>>;

    // ---- announcements
    async fn create_announcement(
        &self,
        a: &NewAnnouncement,
        audit: &AuditEntry,
    ) -> AdminResult<Announcement>;
    async fn end_announcement(&self, id: i64, audit: &AuditEntry) -> AdminResult<Announcement>;
    async fn list_announcements(&self, limit: i64) -> AdminResult<Vec<Announcement>>;
    /// Announcements showing at `now_ms`.
    async fn active_announcements(&self, now_ms: i64) -> AdminResult<Vec<Announcement>>;

    // ---- alerts
    async fn record_alert(&self, kind: &str, detail: &str) -> AdminResult<StaffAlertRow>;
    async fn recent_alerts(&self, limit: i64) -> AdminResult<Vec<StaffAlertRow>>;
    async fn ack_alert(&self, id: i64, by: i64, audit: &AuditEntry) -> AdminResult<StaffAlertRow>;

    // ---- private content (reads only; the service logs before calling these)
    async fn private_channel(&self, channel_id: i64) -> AdminResult<Option<PrivateChannel>>;
    /// DM and group DM channels `account_id` takes part in.
    async fn dm_channels_of(&self, account_id: i64) -> AdminResult<Vec<PrivateChannel>>;
    /// Channels of a server that `@everyone` cannot see.
    async fn private_channels_of_guild(&self, guild_id: i64) -> AdminResult<Vec<PrivateChannel>>;
    /// Newest first, including deleted messages younger than `deleted_days`.
    async fn private_messages(
        &self,
        channel_id: i64,
        before: Option<i64>,
        limit: i64,
        deleted_days: i64,
    ) -> AdminResult<Vec<PrivateMessage>>;
}

/// What the Staff Server's system user can say. Implemented in the server over the
/// message stack; absent in unit tests.
#[async_trait]
pub trait StaffNotifier: Send + Sync {
    /// DM `offer.account_id` from the system user with Accept and Decline buttons.
    /// Returns the DM channel and message ids.
    async fn appointment_offer(&self, offer: &StaffOffer) -> Option<(i64, i64)>;
    /// A plain system message in `#staff-announcements`.
    async fn staff_notice(&self, text: &str);
    /// DM an announcement from the system user to every active account. Runs in the
    /// background; the implementation must not block the caller.
    fn broadcast_announcement(&self, a: &Announcement);
}
