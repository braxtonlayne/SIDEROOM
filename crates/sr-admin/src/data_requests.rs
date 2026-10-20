//! The data-subject request queue (M-25 step c, MOD-DSR; D-295, D-315, spec section 6).
//!
//! A request is an export or an erasure for one account of a Person. `due_at` is computed from
//! `compliance.data_request_deadline_days` when the row is created and never recomputed; one
//! documented extension of at most [`MAX_EXTENSION_DAYS`] days may be recorded (GDPR article
//! 12(3)). Every state change writes the staff access log in the same transaction. Rows are kept
//! forever (D-124, D-212): there is no delete path.
//!
//! Lifecycle: `open` (received) -> `verified` (staff checked who is asking) -> `running` (an
//! export is being built) -> `completed`; `declined` ends a request staff refuse with a reason.
//! The export run reuses the self-service `user.data_export` job and its README; the erasure
//! run reuses account deletion and names the records it keeps ([`RETAINED`]).
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
/// Longest single extension: two further months (GDPR article 12(3)).
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
pub struct RetainedRecord {
    pub id: &'static str,
    pub label: &'static str,
    pub reason: &'static str,
}

/// The records an erasure keeps. The same list is written to the request's outcome and
/// shown in the queue; a test guards that they really survive the run.
pub const RETAINED: [RetainedRecord; 7] = [
    RetainedRecord {
        id: "person_record",
        label: "Person record",
        reason: "Links the accounts of one person so a ban cannot be evaded (D-124).",
    },
    RetainedRecord {
        id: "invite_graph",
        label: "Invite graph",
        reason: "Who invited whom, for abuse tracing and inviter accountability (D-124).",
    },
    RetainedRecord {
        id: "invoices",
        label: "Invoices",
        reason: "Donation and purchase records kept for accounting duties (D-124).",
    },
    RetainedRecord {
        id: "moderation_history",
        label: "Staff-side moderation history",
        reason: "Bans, cases and the staff access log keep staff decisions accountable (D-124, D-212).",
    },
    RetainedRecord {
        id: "held_content",
        label: "Content under a moderation hold",
        reason: "A held row is kept while its hold lasts; staff decide on release (D-122).",
    },
    RetainedRecord {
        id: "external_identities",
        label: "Linked Discord identity",
        reason: "Stays so the same identity cannot register again (D-030).",
    },
    RetainedRecord {
        id: "request_log",
        label: "This request and its audit trail",
        reason: "The request log is kept to show the deadline was met (D-295).",
    },
];

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
    /// Ids of the [`RETAINED`] records an erasure named.
    pub retained: Vec<String>,
    pub completed_at_ms: Option<i64>,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub created_by: i64,
}

impl DataRequest {
    /// Fill the derived fields.
    pub fn with_clock(mut self, now_ms: i64) -> Self {
        self.effective_due_ms = self.extended_until_ms.unwrap_or(self.due_at_ms);
        self.overdue = self.state.is_active() && self.effective_due_ms < now_ms;
        self
    }
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
    /// Queue the export build for a `verified` export request (no 24-hour cooldown: staff act on
    /// the subject's behalf). Moves it to `running`.
    async fn run_export(
        &self,
        id: i64,
        now_ms: i64,
        audit: &AuditEntry,
    ) -> AdminResult<DataRequest>;
    /// Delete the account for a `verified` erasure request and complete it, naming [`RETAINED`].
    /// `Conflict` (with the reason) when the account cannot be deleted yet, for example it owns servers.
    async fn run_erasure(
        &self,
        id: i64,
        now_ms: i64,
        audit: &AuditEntry,
    ) -> AdminResult<ErasureRun>;
}

/// The outcome text of an erasure: what went and what stays.
pub fn erasure_outcome(report: &ErasureReport) -> String {
    let mut s = String::from(
        "Account erased: it now reads \"Deleted User\", sign-in, sessions, memberships, friendships and \
         open invites are gone. Retained: ",
    );
    s.push_str(
        &RETAINED
            .iter()
            .map(|r| r.label)
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
    if report.already_deleted {
        s.push_str(" The account had already been deleted by its owner.");
    }
    s
}

/// The ids of [`RETAINED`], as stored on the row.
pub fn retained_ids() -> Vec<String> {
    RETAINED.iter().map(|r| r.id.to_owned()).collect()
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
                "an extension is 1 to {MAX_EXTENSION_DAYS} days"
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
        Ok(self
            .requests()?
            .extend(req_id, days, &reason, c.user_id, now, &audit)
            .await?
            .with_clock(now))
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
    pub async fn run_data_erasure(&self, c: &Caller, req_id: i64) -> AdminResult<ErasureRun> {
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
            json!({ "account_id": r.account_id.to_string(), "retained": retained_ids() }),
        );
        let now = self.now_ms();
        let mut run = self.requests()?.run_erasure(req_id, now, &audit).await?;
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
}

impl MemoryDataRequests {
    pub fn new(accounts: Arc<crate::memory::MemoryAdminRepo>) -> Self {
        Self {
            accounts,
            rows: Mutex::new(Vec::new()),
            next: Mutex::new(9_000),
            blocked: Mutex::new(std::collections::HashMap::new()),
        }
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
        _now_ms: i64,
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
            let _ = by;
            r.clone()
        };
        self.log(audit).await?;
        Ok(row)
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
        audit: &AuditEntry,
    ) -> AdminResult<ErasureRun> {
        let report = ErasureReport::default();
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
            r.outcome = Some(erasure_outcome(&report));
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
