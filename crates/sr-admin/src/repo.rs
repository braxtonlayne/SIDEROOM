//! Storage port of the admin backend. Every state-changing method takes the
//! [`AuditEntry`] describing the action and must write it (to the
//! hash-chained access log and the platform audit log) in the same
//! transaction as the change, so an action and its record stand or fall together.

use crate::{
    error::AdminResult,
    ext::Delegation,
    model::{
        AccessLogEntry, AccessLogFilter, AccountDetail, AccountQuery, AccountState, AccountSummary,
        AuditEntry, BanCategory, BanOutcome, BanRecord, ChainReport, CreditLedger, DeleteRequest,
        InviteDescendants, InviteQuery, InviteRow, InviteTree, JobRow, PersonDetail,
        Reset2faReport, StaffMember, StaffRole, Standing, StoredSetting, SuspectPerson,
        SystemCounts, UnbanOutcome,
    },
};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

#[async_trait]
pub trait AdminRepository: Send + Sync {
    // ---- staff ----------------------------------------------------------
    /// Platform standing of an account: an active appointment, membership of
    /// the Staff Server and the union of its roles' platform bits. The Staff
    /// Server owner is `OWNER`. `None` for everyone else. Evaluated on every
    /// request, so removal from the Staff Server takes effect immediately.
    async fn standing(&self, user_id: i64) -> AdminResult<Option<Standing>>;
    /// Roles of the Staff Server (without `@everyone`).
    async fn staff_roles(&self) -> AdminResult<Vec<StaffRole>>;
    async fn list_staff(&self) -> AdminResult<Vec<StaffMember>>;
    async fn get_staff(&self, account_id: i64) -> AdminResult<Option<StaffMember>>;
    /// Create the appointment, add the account to the Staff Server and give it `role_ids`.
    async fn appoint(
        &self,
        account_id: i64,
        role_ids: &[i64],
        by: i64,
        reason: &str,
        audit: &AuditEntry,
    ) -> AdminResult<StaffMember>;
    /// Replace the Staff Server roles of a staff member.
    async fn set_staff_roles(
        &self,
        account_id: i64,
        role_ids: &[i64],
        audit: &AuditEntry,
    ) -> AdminResult<StaffMember>;
    /// Close the appointment and remove the account from the Staff Server.
    async fn revoke_staff(
        &self,
        account_id: i64,
        by: i64,
        reason: &str,
        audit: &AuditEntry,
    ) -> AdminResult<()>;
    /// Make `new_owner` the Staff Server owner (and hold the `Owner` role);
    /// the previous owner stays staff with the highest non-Owner role.
    async fn transfer_owner(&self, new_owner: i64, audit: &AuditEntry) -> AdminResult<()>;

    /// Record the Owner's delegation of appointment rights to a staff member (or replace its
    /// ceiling). While it is open the delegate's standing includes `APPOINT_STAFF`.
    async fn set_delegation(
        &self,
        account_id: i64,
        max_permissions: u64,
        by: i64,
        reason: &str,
        audit: &AuditEntry,
    ) -> AdminResult<Delegation>;
    /// Close the delegation. `NotFound` when there is none.
    async fn remove_delegation(&self, account_id: i64, audit: &AuditEntry) -> AdminResult<()>;
    async fn list_delegations(&self) -> AdminResult<Vec<Delegation>>;

    // ---- accounts -------------------------------------------------------
    /// Up to `q.limit` rows ordered by id; the service asks for one extra to build the cursor.
    async fn search_accounts(&self, q: &AccountQuery) -> AdminResult<Vec<AccountSummary>>;
    async fn account_detail(&self, id: i64) -> AdminResult<Option<AccountDetail>>;
    /// Move between `active` and `deactivated`. Reactivation fails when `cap` is reached.
    /// Deactivation also revokes the sessions.
    async fn set_account_state(
        &self,
        id: i64,
        to: AccountState,
        cap: i64,
        audit: &AuditEntry,
    ) -> AdminResult<AccountSummary>;
    async fn revoke_sessions(&self, id: i64, audit: &AuditEntry) -> AdminResult<u64>;
    /// Change the username and/or display name (`Some(None)` clears the display name).
    /// `Conflict` when the username is taken.
    async fn rename_account(
        &self,
        id: i64,
        username: Option<&str>,
        display_name: Option<Option<&str>>,
        audit: &AuditEntry,
    ) -> AdminResult<AccountSummary>;
    /// Replace the email (it becomes unverified). `Conflict` when another account uses it.
    async fn set_account_email(
        &self,
        id: i64,
        email: &str,
        audit: &AuditEntry,
    ) -> AdminResult<AccountSummary>;
    /// `flags = (flags | set) & !clear`.
    async fn update_account_flags(
        &self,
        id: i64,
        set: i64,
        clear: i64,
        audit: &AuditEntry,
    ) -> AdminResult<AccountSummary>;
    /// Make the current password unusable and end every session and trusted device, so the
    /// owner must use "Forgot password". Returns the number of sessions ended.
    async fn force_password_reset(&self, id: i64, audit: &AuditEntry) -> AdminResult<u64>;
    async fn reset_2fa(&self, id: i64, audit: &AuditEntry) -> AdminResult<Reset2faReport>;
    async fn create_delete_request(
        &self,
        id: i64,
        requested_by: i64,
        reason: &str,
        audit: &AuditEntry,
    ) -> AdminResult<DeleteRequest>;

    // ---- persons and bans ----------------------------------------------
    async fn person_detail(&self, person_id: i64) -> AdminResult<Option<PersonDetail>>;
    /// Ban the Person: its live accounts become `platform_banned`, sessions die, and for
    /// a serious ban the inviter of `source_account` gains suspicion.
    #[allow(clippy::too_many_arguments)]
    async fn ban_person(
        &self,
        person_id: i64,
        source_account: Option<i64>,
        category: BanCategory,
        note: &str,
        by: i64,
        freeze_at: i64,
        escalate_at: i64,
        audit: &AuditEntry,
    ) -> AdminResult<BanOutcome>;
    async fn unban_person(
        &self,
        person_id: i64,
        cap: i64,
        by: i64,
        note: &str,
        audit: &AuditEntry,
    ) -> AdminResult<UnbanOutcome>;
    async fn list_bans(
        &self,
        active_only: bool,
        after: Option<i64>,
        limit: i64,
    ) -> AdminResult<Vec<BanRecord>>;
    async fn list_suspects(&self, freeze_at: i64) -> AdminResult<Vec<SuspectPerson>>;
    async fn clear_suspicion(&self, person_id: i64, by: i64, audit: &AuditEntry)
    -> AdminResult<()>;

    // ---- invite graph and credits --------------------------------------
    async fn invite_tree(&self, account_id: i64) -> AdminResult<Option<InviteTree>>;
    async fn invite_descendants(
        &self,
        account_id: i64,
        max_depth: i32,
        max_nodes: i64,
    ) -> AdminResult<Option<InviteDescendants>>;
    async fn credit_ledger(&self, account_id: i64, limit: i64)
    -> AdminResult<Option<CreditLedger>>;
    /// Append a `staff_grant` (delta > 0) or `staff_revoke` (delta < 0) ledger row; the balance
    /// may not go below zero. Returns the new balance.
    async fn adjust_credits(
        &self,
        account_id: i64,
        delta: i32,
        by: i64,
        audit: &AuditEntry,
    ) -> AdminResult<i64>;
    async fn list_invites(&self, q: &InviteQuery) -> AdminResult<Vec<InviteRow>>;
    async fn revoke_invite(&self, invite_id: i64, audit: &AuditEntry) -> AdminResult<InviteRow>;

    // ---- platform settings ---------------------------------------------
    async fn settings(&self) -> AdminResult<HashMap<String, StoredSetting>>;
    async fn put_settings(
        &self,
        values: &[(String, Value)],
        by: i64,
        audit: &AuditEntry,
    ) -> AdminResult<()>;

    // ---- system ---------------------------------------------------------
    async fn counts(&self) -> AdminResult<SystemCounts>;
    async fn list_jobs(
        &self,
        state: Option<&str>,
        after: Option<i64>,
        limit: i64,
    ) -> AdminResult<Vec<JobRow>>;
    async fn retry_job(&self, id: i64, audit: &AuditEntry) -> AdminResult<()>;
    async fn delete_job(&self, id: i64, audit: &AuditEntry) -> AdminResult<()>;

    // ---- access log -----------------------------------------------------
    /// Append one row to the chain on its own (reads and denials).
    async fn append_log(&self, entry: &AuditEntry) -> AdminResult<AccessLogEntry>;
    /// Newest first.
    async fn list_log(&self, f: &AccessLogFilter) -> AdminResult<Vec<AccessLogEntry>>;
    async fn verify_log(&self) -> AdminResult<ChainReport>;
}
