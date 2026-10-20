//! The data-subject request queue (M-25 step c, MOD-DSR; D-295, D-315, spec section 6).
//!
//! A request is an export or an erasure for one account of a Person. `due_at` is computed from
//! `compliance.data_request_deadline_days` when the row is created and never recomputed; one
//! documented extension of at most [`MAX_EXTENSION_DAYS`] days may be recorded (GDPR article
//! 12(3); 60 days is the conservative count of "two further months": days, not calendar months,
//! D-316). The member is told of an extension by system DM when reachable (D-317). Every state change writes the staff access log in the same transaction. Rows are kept
//! forever (D-124, D-212): there is no delete path.
//!
//! Lifecycle: `open` (received) -> `verified` (staff checked who is asking) -> `running` (an
//! export is being built) -> `completed`; `declined` ends a request staff refuse with a reason.
//! The export run reuses the self-service `user.data_export` job and its README; the erasure
//! run reuses account deletion and names the records it keeps ([`retained`]).
//!
//! Permission: `MANAGE_TAKEDOWNS` (D-210). Not legal advice; the retained-record bases are the
//! project defaults of D-295 for a lawyer to check.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sr_core::PlatformPermissions as P;
use utoipa::ToSchema;

use crate::{
    error::{AdminError, AdminResult},
    model::{AccessKind, AuditEntry, Caller, id, opt_id},
    repo::AdminRepository,
    service::AdminService,
};

/// Milliseconds in a day.
pub const DAY_MS: i64 = 86_400_000;
/// Longest single extension, in days: 60 days (the conservative count of article 12(3)'s two further
/// months; days, not calendar months, D-316).
pub const MAX_EXTENSION_DAYS: i64 = 60;
/// Requests older than this when entered are refused as a typo (days).
const MAX_BACKDATE_DAYS: i64 = 365;
/// Cap on the queue page.
const MAX_LIST: i64 = 200;

/// What the request asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DataRequestKind {
    Export,
    Erasure,
}

impl DataRequestKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Export => "export",
            Self::Erasure => "erasure",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "export" => Some(Self::Export),
            "erasure" => Some(Self::Erasure),
            _ => None,
        }
    }
}

/// Where the request stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DataRequestState {
    Open,
    Verified,
    Running,
    Completed,
    Declined,
}

impl DataRequestState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Verified => "verified",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Declined => "declined",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "open" => Self::Open,
            "verified" => Self::Verified,
            "running" => Self::Running,
            "completed" => Self::Completed,
            "declined" => Self::Declined,
            _ => return None,
        })
    }
    /// Still needs staff (counts against the deadline).
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Open | Self::Verified | Self::Running)
    }
}

/// One record kept after an erasure, with the reason (G-66, D-124, D-295).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct RetainedRecord {
    pub id: String,
    pub label: String,
    pub reason: String,
}

/// Number of records an erasure keeps (D-318, D-326).
pub const RETAINED_COUNT: usize = 9;

/// The records an erasure keeps, in the order the export README and the deletion screen list them.
/// `invoice_years` is `compliance.invoice_retention_years` (D-325). The same list is written to the
/// request's outcome and shown in the queue; a test guards that they really survive the run.
pub fn retained(invoice_years: u8) -> Vec<RetainedRecord> {
    let r = |id: &str, label: &str, reason: String| RetainedRecord {
        id: id.to_owned(),
        label: label.to_owned(),
        reason,
    };
    vec![
        r(
            "person_record",
            "Person record",
            "Links the accounts of one person so a ban cannot be evaded (D-124).".into(),
        ),
        r(
            "invite_graph",
            "Invite graph",
            "Who invited whom, for abuse tracing and inviter accountability (D-124).".into(),
        ),
        r(
            "invoices",
            "Invoices",
            format!("Donation and purchase records, kept for {invoice_years} years after the year of payment, as accounting law requires, then no longer linked to you (D-124, D-325)."),
        ),
        r(
            "moderation_history",
            "Staff-side moderation history",
            "Bans, cases and the staff access log keep staff decisions accountable (D-124, D-212).".into(),
        ),
        r(
            "messages",
            "Messages and uploads you did not delete",
            "They stay in the rooms they were posted in, shown as Deleted User, unless you deleted them first (D-029, D-123).".into(),
        ),
        r(
            "held_content",
            "Content under a moderation hold",
            "A held row is kept while its hold lasts, then it follows the normal purge; staff decide on release (D-122).".into(),
        ),
        r(
            "external_identities",
            "Linked Discord identity",
            "Kept so a person who comes back is linked to the same Person record (D-030); it does not stop them from joining again with a new invite.".into(),
        ),
        r(
            "deleted_account_archive",
            "Your original username, email address and display name, in a staff-only archive",
            "So staff can recognise your account if you write in later (a data request, a dispute) and link a returning person to the same Person record (D-030); kept as long as the Person record (D-124); every staff view of it is logged (D-326).".into(),
        ),
        r(
            "request_log",
            "This request and its audit trail",
            "The request log is kept to show the deadline was met (D-295).".into(),
        ),
    ]
}

/// A queue row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DataRequest {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub kind: DataRequestKind,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub person_id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub account_id: i64,
    pub username: String,
    pub state: DataRequestState,
    pub note: String,
    pub received_at_ms: i64,
    /// The deadline computed when the request was entered.
    pub due_at_ms: i64,
    /// The new deadline once the one extension was recorded.
    pub extended_until_ms: Option<i64>,
    pub extension_reason: Option<String>,
    /// `extended_until_ms` when extended, else `due_at_ms`.
    pub effective_due_ms: i64,
    /// Active and past the effective deadline.
    pub overdue: bool,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub verified_by: Option<i64>,
    pub verified_at_ms: Option<i64>,
    /// The `data_exports` row an export run queued.
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub export_id: Option<i64>,
    pub outcome: Option<String>,
    /// Ids of the [`retained`] records an erasure named.
    pub retained: Vec<String>,
    pub completed_at_ms: Option<i64>,
    /// When the extension was recorded.
    pub extended_at_ms: Option<i64>,
    /// The extension was granted after the original `due_at` (it does not cure a missed deadline).
    pub extended_late: bool,
    /// When the member was told of the extension by system DM; `None` when no DM could be sent.
    pub member_notified_at_ms: Option<i64>,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub created_by: i64,
}

impl DataRequest {
    /// Fill the derived fields.
    pub fn with_clock(mut self, now_ms: i64) -> Self {
        self.effective_due_ms = self.extended_until_ms.unwrap_or(self.due_at_ms);
        self.extended_late = self.extended_at_ms.is_some_and(|a| a > self.due_at_ms);
        self.overdue = self.state.is_active() && self.effective_due_ms < now_ms;
        self
    }
}

/// The staff-only archive of a deleted account's original identifiers (D-326).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DeletedArchive {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub account_id: i64,
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
    pub deleted_at_ms: i64,
    /// When the username can be taken again.
    pub username_release_at_ms: i64,
    pub credits_forfeited: i32,
    pub invites_revoked: i32,
}

/// Input of [`DataRequestRepository::create`].
#[derive(Debug, Clone)]
pub struct NewDataRequest {
    pub kind: DataRequestKind,
    pub account_id: i64,
    pub received_at_ms: i64,
    pub due_at_ms: i64,
    pub note: String,
    pub created_by: i64,
}

/// What an erasure run did to the account (media keys and relations the caller cleans up).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ErasureReport {
    pub guilds_left: Vec<i64>,
    pub friends: Vec<i64>,
    pub other_relations: Vec<i64>,
    pub credits_forfeited: i32,
    pub invites_revoked: i32,
    pub avatar_key: Option<String>,
    pub banner_key: Option<String>,
    pub profile_media_keys: Vec<String>,
    /// Messages and attachments kept because a moderation hold covers them.
    pub held_rows: i64,
    /// The account was already deleted when the run started.
    pub already_deleted: bool,
    /// `Some(n)` when the run queued the member's message self-delete at their request (N-06): `n`
    /// messages were visible when it was queued. `None`: the messages were left in place (D-029).
    pub messages_queued: Option<i64>,
}

/// Options of an erasure run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErasureOptions {
    /// The member also asked for their messages to be deleted (N-06, D-324).
    pub delete_messages: bool,
    /// `compliance.invoice_retention_years`, for the outcome text.
    pub invoice_years: u8,
}

/// Result of an erasure run.
#[derive(Debug, Clone)]
pub struct ErasureRun {
    pub request: DataRequest,
    pub report: ErasureReport,
}

/// Storage port of the queue. Each write takes the access-log entry and stores both atomically.
#[async_trait]
pub trait DataRequestRepository: Send + Sync {
    /// Fails with `Conflict` when the account already has an active request of that kind.
    async fn create(&self, n: &NewDataRequest, audit: &AuditEntry) -> AdminResult<DataRequest>;
    async fn get(&self, id: i64) -> AdminResult<Option<DataRequest>>;
    /// Active requests first by deadline, then closed ones newest first.
    async fn list(&self, active_only: bool, limit: i64) -> AdminResult<Vec<DataRequest>>;
    /// The archive row of a deleted account, if there is one (D-326).
    async fn deleted_archive(&self, account_id: i64) -> AdminResult<Option<DeletedArchive>>;
    /// `open` -> `verified`. `Conflict` from any other state.
    async fn verify(
        &self,
        id: i64,
        by: i64,
        now_ms: i64,
        audit: &AuditEntry,
    ) -> AdminResult<DataRequest>;
    /// `open` or `verified` -> `declined`.
    async fn decline(
        &self,
        id: i64,
        outcome: &str,
        now_ms: i64,
        audit: &AuditEntry,
    ) -> AdminResult<DataRequest>;
    /// Record the one extension: `Conflict` when one exists or the request is closed.
    async fn extend(
        &self,
        id: i64,
        days: i64,
        reason: &str,
        by: i64,
        now_ms: i64,
        audit: &AuditEntry,
    ) -> AdminResult<DataRequest>;
    /// Record the outcome of the extension notice: sets `member_notified_at` when `delivered` and
    /// writes the access-log entry `audit` (which names the outcome).
    async fn member_notice(
        &self,
        id: i64,
        delivered: bool,
        now_ms: i64,
        audit: &AuditEntry,
    ) -> AdminResult<DataRequest>;
    /// Queue the export build for a `verified` export request (no 24-hour cooldown: staff act on
    /// the subject's behalf). Moves it to `running`.
    async fn run_export(
        &self,
        id: i64,
        now_ms: i64,
        audit: &AuditEntry,
    ) -> AdminResult<DataRequest>;
    /// Delete the account for a `verified` erasure request and complete it, naming [`retained`].
    /// `Conflict` (with the reason) when the account cannot be deleted yet, for example it owns servers.
    async fn run_erasure(
        &self,
        id: i64,
        now_ms: i64,
        opts: ErasureOptions,
        audit: &AuditEntry,
    ) -> AdminResult<ErasureRun>;
}

/// The outcome text of an erasure: what went and what stays.
pub fn erasure_outcome(report: &ErasureReport, invoice_years: u8) -> String {
    let mut s = String::from(
        "Account erased: it now reads \"Deleted User\", sign-in, sessions, memberships, friendships and \
         open invites are gone. Retained: ",
    );
    s.push_str(
        &retained(invoice_years)
            .iter()
            .map(|r| r.label.as_str())
            .collect::<Vec<_>>()
            .join("; "),
    );
    s.push('.');
    if report.held_rows > 0 {
        s.push_str(&format!(
            " {} held row(s) stay until their case releases them; the case needs a decision.",
            report.held_rows
        ));
    }
    match report.messages_queued {
        Some(n) => s.push_str(&format!(
            " Messages deleted at the member's request ({n} rows, 30-day purge)."
        )),
        None => s.push_str(" Messages left in place (D-029)."),
    }
    if report.already_deleted {
        s.push_str(" The account had already been deleted by its owner.");
    }
    s
}

/// The ids of [`retained`], as stored on the row.
pub fn retained_ids() -> Vec<String> {
    retained(10).into_iter().map(|r| r.id).collect()
}

/// Outcome text of a finished export (set by the export job when the package is ready).
pub const EXPORT_DONE: &str =
    "Export built. The person downloads it from Settings, Your data, for 7 days.";

impl AdminService {
    fn requests(&self) -> AdminResult<&Arc<dyn DataRequestRepository>> {
        self.data_requests
            .as_ref()
            .ok_or_else(|| AdminError::Storage("the data-request store is not configured".into()))
    }

    /// Enter a request that arrived by email or form. `deadline_days` is
    /// `compliance.data_request_deadline_days`; `received_at_ms` defaults to now.
    pub async fn create_data_request(
        &self,
        c: &Caller,
        kind: DataRequestKind,
        account_id: i64,
        received_at_ms: Option<i64>,
        note: Option<&str>,
        deadline_days: i64,
    ) -> AdminResult<DataRequest> {
        self.require(c, P::MANAGE_TAKEDOWNS, "data_request.create")
            .await?;
        let now = self.now_ms();
        let received = received_at_ms.unwrap_or(now);
        if received > now + DAY_MS {
            return Err(AdminError::Invalid(
                "the request cannot have arrived in the future".into(),
            ));
        }
        if received < now - MAX_BACKDATE_DAYS * DAY_MS {
            return Err(AdminError::Invalid(
                "that arrival date is more than a year ago".into(),
            ));
        }
        let note = note.map(str::trim).unwrap_or("");
        if note.chars().count() > 1000 {
            return Err(AdminError::Invalid(
                "the note is at most 1000 characters".into(),
            ));
        }
        if !(1..=365).contains(&deadline_days) {
            return Err(AdminError::Invalid("the deadline is out of range".into()));
        }
        if account_id == crate::service::SYSTEM_USER_ID {
            return Err(AdminError::Forbidden(
                "The system user cannot be changed".into(),
            ));
        }
        let due = received + deadline_days * DAY_MS;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "data_request.create",
            Some(("account", account_id)),
            note,
            json!({ "kind": kind.as_str(), "received_at_ms": received, "due_at_ms": due, "deadline_days": deadline_days }),
        );
        let n = NewDataRequest {
            kind,
            account_id,
            received_at_ms: received,
            due_at_ms: due,
            note: note.to_owned(),
            created_by: c.user_id,
        };
        Ok(self.requests()?.create(&n, &audit).await?.with_clock(now))
    }

    /// The queue (`MANAGE_TAKEDOWNS`).
    pub async fn list_data_requests(
        &self,
        c: &Caller,
        active_only: bool,
        limit: Option<i64>,
    ) -> AdminResult<Vec<DataRequest>> {
        self.require(c, P::MANAGE_TAKEDOWNS, "data_request.list")
            .await?;
        let now = self.now_ms();
        let rows = self
            .requests()?
            .list(active_only, limit.unwrap_or(100).clamp(1, MAX_LIST))
            .await?;
        Ok(rows.into_iter().map(|r| r.with_clock(now)).collect())
    }

    /// One request; the read is logged.
    pub async fn get_data_request(&self, c: &Caller, req_id: i64) -> AdminResult<DataRequest> {
        self.require(c, P::MANAGE_TAKEDOWNS, "data_request.view")
            .await?;
        let r = self
            .requests()?
            .get(req_id)
            .await?
            .ok_or(AdminError::NotFound("data request"))?;
        let e = self.entry(
            c,
            AccessKind::Read,
            "data_request.view",
            Some(("data_request", req_id)),
            "",
            json!({}),
        );
        self.repo().append_log(&e).await?;
        Ok(r.with_clock(self.now_ms()))
    }

    /// Staff checked who is asking; `how` says how (a reply from the account email, a signed form).
    pub async fn verify_data_request(
        &self,
        c: &Caller,
        req_id: i64,
        how: Option<&str>,
    ) -> AdminResult<DataRequest> {
        self.require(c, P::MANAGE_TAKEDOWNS, "data_request.verify")
            .await?;
        let how = Self::reason(how)?;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "data_request.verify",
            Some(("data_request", req_id)),
            &how,
            json!({}),
        );
        let now = self.now_ms();
        Ok(self
            .requests()?
            .verify(req_id, c.user_id, now, &audit)
            .await?
            .with_clock(now))
    }

    /// Refuse a request with a stated reason (it stays in the log).
    pub async fn decline_data_request(
        &self,
        c: &Caller,
        req_id: i64,
        reason: Option<&str>,
    ) -> AdminResult<DataRequest> {
        self.require(c, P::MANAGE_TAKEDOWNS, "data_request.decline")
            .await?;
        let reason = Self::reason(reason)?;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "data_request.decline",
            Some(("data_request", req_id)),
            &reason,
            json!({}),
        );
        let now = self.now_ms();
        let outcome = format!("Declined: {reason}");
        Ok(self
            .requests()?
            .decline(req_id, &outcome, now, &audit)
            .await?
            .with_clock(now))
    }

    /// Add `days` to the deadline, once, with a reason (`POST /admin/data-requests/{id}/extend`).
    ///
    /// The access log holds **two rows** for an extension (D-327): `data_request.extend`, committed
    /// with the row update, and `data_request.extend_notice`, written after the member DM was tried,
    /// carrying `member_notified` and `late`. The log is a hash chain and append-only, so the outcome of
    /// the DM cannot be added to the first row, and the DM must not run inside the extension's
    /// transaction (it would hold the transaction open across a network call, or announce an extension
    /// that then rolled back). The notice row is written also when the DM fails, so a missing notice
    /// row means a crash between the two writes, never a failed DM.
    pub async fn extend_data_request(
        &self,
        c: &Caller,
        req_id: i64,
        days: i64,
        reason: Option<&str>,
    ) -> AdminResult<DataRequest> {
        self.require(c, P::MANAGE_TAKEDOWNS, "data_request.extend")
            .await?;
        let reason = Self::reason(reason)?;
        if !(1..=MAX_EXTENSION_DAYS).contains(&days) {
            return Err(AdminError::Invalid(format!(
                "an extension is 1 to {MAX_EXTENSION_DAYS} days (counted in days, not months)"
            )));
        }
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "data_request.extend",
            Some(("data_request", req_id)),
            &reason,
            json!({ "days": days }),
        );
        let now = self.now_ms();
        let r = self
            .requests()?
            .extend(req_id, days, &reason, c.user_id, now, &audit)
            .await?;
        // D-317: article 12(3) makes telling the person part of the extension. Best effort.
        let text = format!(
            "Staff need more time for the data request you made. The new due date is {}. Reason from staff: {}",
            date_utc(r.extended_until_ms.unwrap_or(r.due_at_ms)),
            reason
        );
        let delivered = self.system_dm(r.account_id, &text).await;
        let notice = self.entry(
            c,
            AccessKind::AdminAction,
            "data_request.extend_notice",
            Some(("data_request", req_id)),
            "",
            json!({ "member_notified": delivered, "late": r.extended_at_ms.is_some_and(|a| a > r.due_at_ms) }),
        );
        Ok(self
            .requests()?
            .member_notice(req_id, delivered, now, &notice)
            .await?
            .with_clock(now))
    }

    /// The original username, email and display name of a deleted account, from the staff-only
    /// archive (D-326). `MANAGE_ACCOUNTS`. Every view is written to the staff access log with
    /// `account.deleted_archive.view`, written before the data is returned.
    pub async fn view_deleted_archive(
        &self,
        c: &Caller,
        account_id: i64,
    ) -> AdminResult<DeletedArchive> {
        self.require(c, P::MANAGE_ACCOUNTS, "account.deleted_archive.view")
            .await?;
        let a = self
            .requests()?
            .deleted_archive(account_id)
            .await?
            .ok_or(AdminError::NotFound("deleted account archive"))?;
        let e = self.entry(
            c,
            AccessKind::Read,
            "account.deleted_archive.view",
            Some(("account", account_id)),
            "",
            json!({}),
        );
        self.repo().append_log(&e).await?;
        Ok(a)
    }

    /// Run an export request: queue the self-service build job for the subject.
    pub async fn run_data_export(&self, c: &Caller, req_id: i64) -> AdminResult<DataRequest> {
        self.require(c, P::MANAGE_TAKEDOWNS, "data_request.run")
            .await?;
        let r = self.loaded_request(req_id).await?;
        if r.kind != DataRequestKind::Export {
            return Err(AdminError::Conflict("This is an erasure request".into()));
        }
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "data_request.run_export",
            Some(("data_request", req_id)),
            "",
            json!({ "account_id": r.account_id.to_string() }),
        );
        let now = self.now_ms();
        Ok(self
            .requests()?
            .run_export(req_id, now, &audit)
            .await?
            .with_clock(now))
    }

    /// Run an erasure request through account deletion. The caller finishes the media clean-up
    /// and the live notices from the returned report.
    pub async fn run_data_erasure(
        &self,
        c: &Caller,
        req_id: i64,
        opts: ErasureOptions,
    ) -> AdminResult<ErasureRun> {
        self.require(c, P::MANAGE_TAKEDOWNS, "data_request.run")
            .await?;
        let r = self.loaded_request(req_id).await?;
        if r.kind != DataRequestKind::Erasure {
            return Err(AdminError::Conflict("This is an export request".into()));
        }
        if r.account_id == c.user_id {
            return Err(AdminError::Forbidden(
                "You cannot erase your own account from the queue".into(),
            ));
        }
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "data_request.run_erasure",
            Some(("data_request", req_id)),
            "",
            json!({ "account_id": r.account_id.to_string(), "retained": retained_ids(), "delete_messages": opts.delete_messages }),
        );
        let now = self.now_ms();
        let mut run = self
            .requests()?
            .run_erasure(req_id, now, opts, &audit)
            .await?;
        run.request = run.request.with_clock(now);
        Ok(run)
    }

    async fn loaded_request(&self, id: i64) -> AdminResult<DataRequest> {
        self.requests()?
            .get(id)
            .await?
            .ok_or(AdminError::NotFound("data request"))
    }
}

// ===== memory store (tests) ===============================================

/// In-memory queue over a [`crate::memory::MemoryAdminRepo`] (accounts and the log).
pub struct MemoryDataRequests {
    accounts: Arc<crate::memory::MemoryAdminRepo>,
    rows: Mutex<Vec<DataRequest>>,
    next: Mutex<i64>,
    /// Accounts whose erasure is blocked, with the reason (tests).
    blocked: Mutex<std::collections::HashMap<i64, String>>,
    archives: Mutex<std::collections::HashMap<i64, DeletedArchive>>,
}

impl MemoryDataRequests {
    pub fn new(accounts: Arc<crate::memory::MemoryAdminRepo>) -> Self {
        Self {
            accounts,
            rows: Mutex::new(Vec::new()),
            next: Mutex::new(9_000),
            blocked: Mutex::new(std::collections::HashMap::new()),
            archives: Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Seed an archive row (tests).
    pub fn add_archive(&self, a: DeletedArchive) {
        self.archives
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(a.account_id, a);
    }

    /// Make the erasure of `account` fail with a conflict (it owns servers, for example).
    pub fn block_erasure(&self, account: i64, why: &str) {
        self.blocked
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(account, why.to_owned());
    }

    fn rows(&self) -> std::sync::MutexGuard<'_, Vec<DataRequest>> {
        self.rows.lock().unwrap_or_else(|p| p.into_inner())
    }

    async fn log(&self, audit: &AuditEntry) -> AdminResult<()> {
        self.accounts.append_log(audit).await.map(|_| ())
    }
}

fn not_found() -> AdminError {
    AdminError::NotFound("data request")
}

#[async_trait]
impl DataRequestRepository for MemoryDataRequests {
    async fn create(&self, n: &NewDataRequest, audit: &AuditEntry) -> AdminResult<DataRequest> {
        let a = self
            .accounts
            .account_detail(n.account_id)
            .await?
            .ok_or(AdminError::NotFound("account"))?;
        let id = {
            let mut next = self.next.lock().unwrap_or_else(|p| p.into_inner());
            *next += 1;
            *next
        };
        let row = {
            let mut rows = self.rows();
            if rows
                .iter()
                .any(|r| r.account_id == n.account_id && r.kind == n.kind && r.state.is_active())
            {
                return Err(AdminError::Conflict(
                    "That account already has an open request of this kind".into(),
                ));
            }
            let row = DataRequest {
                id,
                kind: n.kind,
                person_id: a.account.person_id,
                account_id: n.account_id,
                username: a.account.username.clone(),
                state: DataRequestState::Open,
                note: n.note.clone(),
                received_at_ms: n.received_at_ms,
                due_at_ms: n.due_at_ms,
                extended_until_ms: None,
                extension_reason: None,
                effective_due_ms: n.due_at_ms,
                overdue: false,
                verified_by: None,
                verified_at_ms: None,
                export_id: None,
                outcome: None,
                retained: Vec::new(),
                completed_at_ms: None,
                extended_at_ms: None,
                extended_late: false,
                member_notified_at_ms: None,
                created_by: n.created_by,
            };
            rows.push(row.clone());
            row
        };
        self.log(audit).await?;
        Ok(row)
    }

    async fn get(&self, id: i64) -> AdminResult<Option<DataRequest>> {
        Ok(self.rows().iter().find(|r| r.id == id).cloned())
    }

    async fn list(&self, active_only: bool, limit: i64) -> AdminResult<Vec<DataRequest>> {
        let mut v: Vec<_> = self
            .rows()
            .iter()
            .filter(|r| !active_only || r.state.is_active())
            .cloned()
            .collect();
        v.sort_by_key(|r| {
            (
                !r.state.is_active(),
                if r.state.is_active() {
                    r.extended_until_ms.unwrap_or(r.due_at_ms)
                } else {
                    -r.id
                },
            )
        });
        v.truncate(usize::try_from(limit).unwrap_or(0));
        Ok(v)
    }

    async fn verify(
        &self,
        id: i64,
        by: i64,
        now_ms: i64,
        audit: &AuditEntry,
    ) -> AdminResult<DataRequest> {
        let row = {
            let mut rows = self.rows();
            let r = rows.iter_mut().find(|r| r.id == id).ok_or_else(not_found)?;
            if r.state != DataRequestState::Open {
                return Err(AdminError::Conflict(
                    "Only an open request can be verified".into(),
                ));
            }
            r.state = DataRequestState::Verified;
            r.verified_by = Some(by);
            r.verified_at_ms = Some(now_ms);
            r.clone()
        };
        self.log(audit).await?;
        Ok(row)
    }

    async fn decline(
        &self,
        id: i64,
        outcome: &str,
        now_ms: i64,
        audit: &AuditEntry,
    ) -> AdminResult<DataRequest> {
        let row = {
            let mut rows = self.rows();
            let r = rows.iter_mut().find(|r| r.id == id).ok_or_else(not_found)?;
            if !matches!(r.state, DataRequestState::Open | DataRequestState::Verified) {
                return Err(AdminError::Conflict(
                    "That request can no longer be declined".into(),
                ));
            }
            r.state = DataRequestState::Declined;
            r.outcome = Some(outcome.to_owned());
            r.completed_at_ms = Some(now_ms);
            r.clone()
        };
        self.log(audit).await?;
        Ok(row)
    }

    async fn extend(
        &self,
        id: i64,
        days: i64,
        reason: &str,
        by: i64,
        now_ms: i64,
        audit: &AuditEntry,
    ) -> AdminResult<DataRequest> {
        let row = {
            let mut rows = self.rows();
            let r = rows.iter_mut().find(|r| r.id == id).ok_or_else(not_found)?;
            if !r.state.is_active() {
                return Err(AdminError::Conflict("The request is closed".into()));
            }
            if r.extended_until_ms.is_some() {
                return Err(AdminError::Conflict(
                    "The deadline was already extended once".into(),
                ));
            }
            r.extended_until_ms = Some(r.due_at_ms + days * DAY_MS);
            r.extension_reason = Some(reason.to_owned());
            r.extended_at_ms = Some(now_ms);
            let _ = by;
            r.clone()
        };
        self.log(audit).await?;
        Ok(row)
    }

    async fn member_notice(
        &self,
        id: i64,
        delivered: bool,
        now_ms: i64,
        audit: &AuditEntry,
    ) -> AdminResult<DataRequest> {
        let row = {
            let mut rows = self.rows();
            let r = rows.iter_mut().find(|r| r.id == id).ok_or_else(not_found)?;
            if delivered {
                r.member_notified_at_ms = Some(now_ms);
            }
            r.clone()
        };
        self.log(audit).await?;
        Ok(row)
    }

    async fn deleted_archive(&self, account_id: i64) -> AdminResult<Option<DeletedArchive>> {
        Ok(self
            .archives
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&account_id)
            .cloned())
    }

    async fn run_export(
        &self,
        id: i64,
        _now_ms: i64,
        audit: &AuditEntry,
    ) -> AdminResult<DataRequest> {
        let row = {
            let mut rows = self.rows();
            let r = rows.iter_mut().find(|r| r.id == id).ok_or_else(not_found)?;
            if r.state != DataRequestState::Verified {
                return Err(AdminError::Conflict(
                    "Verify who is asking before running the export".into(),
                ));
            }
            r.state = DataRequestState::Running;
            r.export_id = Some(id + 1);
            r.clone()
        };
        self.log(audit).await?;
        Ok(row)
    }

    async fn run_erasure(
        &self,
        id: i64,
        now_ms: i64,
        opts: ErasureOptions,
        audit: &AuditEntry,
    ) -> AdminResult<ErasureRun> {
        let report = ErasureReport {
            messages_queued: opts.delete_messages.then_some(0),
            ..ErasureReport::default()
        };
        let row = {
            let mut rows = self.rows();
            let r = rows.iter_mut().find(|r| r.id == id).ok_or_else(not_found)?;
            if r.state != DataRequestState::Verified {
                return Err(AdminError::Conflict(
                    "Verify who is asking before running the erasure".into(),
                ));
            }
            if let Some(why) = self
                .blocked
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get(&r.account_id)
            {
                return Err(AdminError::Conflict(why.clone()));
            }
            r.state = DataRequestState::Completed;
            r.outcome = Some(erasure_outcome(&report, opts.invoice_years));
            r.retained = retained_ids();
            r.completed_at_ms = Some(now_ms);
            r.clone()
        };
        self.log(audit).await?;
        Ok(ErasureRun {
            request: row,
            report,
        })
    }
}

/// `YYYY-MM-DD` (UTC) of a Unix-millisecond time.
pub fn date_utc(ms: i64) -> String {
    let z = ms.div_euclid(DAY_MS) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}-{:02}", if m <= 2 { y + 1 } else { y }, m, d)
}
