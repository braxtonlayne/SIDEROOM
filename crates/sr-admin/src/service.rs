//! The admin service: permission checks, reason rules, protection rules and
//! audit entries on top of an [`AdminRepository`]. Every public method takes
//! the resolved [`Caller`]; the HTTP layer and the operator CLI are thin shells.

use crate::{
    error::{AdminError, AdminResult},
    model::{
        AccessKind, AccessLogFilter, AccessLogPage, AccountDetail, AccountPage, AccountQuery,
        AccountState, AccountSummary, AuditEntry, BanCategory, BanOutcome, BanPage, Caller,
        ChainReport, CreditLedger, DeleteRequest, InviteDescendants, InvitePage, InviteQuery,
        InviteTree, JobPage, PersonDetail, Reset2faReport, StaffMember, StaffRole, SuspectPerson,
        SystemCounts, UnbanOutcome,
    },
    repo::AdminRepository,
    rules,
    settings::{self, SettingDef, SettingKind},
};
use serde::Serialize;
use serde_json::{Value, json};
use sr_core::PlatformPermissions as P;
use sr_events::{DomainEvent, EventBus, StaffAlertKind, Topic};
use std::sync::Arc;
use utoipa::ToSchema;

/// The system user "SIDE ROOM" (doc 06 section 8), the actor of CLI operations.
pub const SYSTEM_USER_ID: i64 = 1;
const MAX_REASON_CHARS: usize = 500;
const DEFAULT_PAGE: i64 = 50;
const MAX_PAGE: i64 = 100;
/// Cap on descendants returned by one graph request.
pub const MAX_GRAPH_NODES: i64 = 500;

/// One platform setting with its current value, for Admin -> Limits.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SettingView {
    pub key: String,
    pub group: String,
    pub kind: SettingKind,
    pub description: String,
    pub min: f64,
    pub max: f64,
    pub default: Value,
    pub value: Value,
    pub is_default: bool,
    pub updated_at_ms: Option<i64>,
}

fn page_limit(l: Option<i64>) -> i64 {
    l.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE)
}

fn json_detail(v: Value) -> String {
    v.to_string()
}

#[derive(Clone)]
pub struct AdminService {
    repo: Arc<dyn AdminRepository>,
    bus: Option<Arc<dyn EventBus>>,
}

impl AdminService {
    pub fn new(repo: Arc<dyn AdminRepository>) -> Self {
        Self { repo, bus: None }
    }

    /// Publish staff alerts on this bus.
    pub fn with_bus(mut self, bus: Arc<dyn EventBus>) -> Self {
        self.bus = Some(bus);
        self
    }

    pub fn repo(&self) -> &Arc<dyn AdminRepository> {
        &self.repo
    }

    /// Resolve the platform standing of `user_id`. Fails with `NotStaff` for anyone
    /// without an active appointment and Staff Server membership.
    pub async fn resolve(&self, user_id: i64, ip: Option<String>) -> AdminResult<Caller> {
        let s = self
            .repo
            .standing(user_id)
            .await?
            .ok_or(AdminError::NotStaff)?;
        Ok(Caller {
            user_id,
            permissions: s.permissions,
            is_owner: s.is_owner,
            ip,
        })
    }

    async fn alert(&self, kind: StaffAlertKind, detail: String) {
        if let Some(bus) = &self.bus {
            let _ = bus
                .publish(Topic::Platform, DomainEvent::StaffAlert { kind, detail })
                .await;
        }
    }

    fn entry(
        &self,
        c: &Caller,
        kind: AccessKind,
        action: &str,
        target: Option<(&str, i64)>,
        reason: &str,
        detail: Value,
    ) -> AuditEntry {
        AuditEntry {
            actor_id: c.user_id,
            kind,
            action: action.to_owned(),
            target_kind: target.map(|(k, _)| k.to_owned()),
            target_id: target.map(|(_, id)| id),
            reason: reason.to_owned(),
            ip: c.ip.clone(),
            detail: json_detail(detail),
        }
    }

    /// Require `perm`; a staff member who lacks it leaves a `denied` row in the log.
    async fn require(&self, c: &Caller, perm: P, action: &str) -> AdminResult<()> {
        if c.permissions.has(perm) {
            return Ok(());
        }
        let e = self.entry(
            c,
            AccessKind::Denied,
            action,
            None,
            "",
            json!({ "missing": rules::perm_name(perm) }),
        );
        let _ = self.repo.append_log(&e).await;
        Err(AdminError::MissingPermission(rules::perm_name(perm)))
    }

    fn reason(r: Option<&str>) -> AdminResult<String> {
        let r = r.map(str::trim).unwrap_or("");
        if r.is_empty() {
            return Err(AdminError::ReasonRequired);
        }
        if r.chars().count() > MAX_REASON_CHARS {
            return Err(AdminError::Invalid(format!(
                "reason must be at most {MAX_REASON_CHARS} characters"
            )));
        }
        Ok(r.to_owned())
    }

    fn optional_reason(r: Option<&str>) -> AdminResult<String> {
        match r.map(str::trim) {
            None | Some("") => Ok(String::new()),
            Some(_) => Self::reason(r),
        }
    }

    async fn setting_i64(&self, key: &str) -> AdminResult<i64> {
        let stored = self.repo.settings().await?;
        let def =
            settings::find(key).ok_or_else(|| AdminError::Storage(format!("no setting {key}")))?;
        Ok(stored
            .get(key)
            .and_then(|s| s.value.as_i64())
            .unwrap_or(def.default_int))
    }

    // ===== staff ==========================================================

    pub async fn staff_roles(&self, c: &Caller) -> AdminResult<Vec<StaffRole>> {
        self.require(c, P::APPOINT_STAFF, "staff.roles").await?;
        self.repo.staff_roles().await
    }

    pub async fn list_staff(&self, c: &Caller) -> AdminResult<Vec<StaffMember>> {
        self.require(c, P::APPOINT_STAFF, "staff.list").await?;
        self.repo.list_staff().await
    }

    async fn roles_by_id(&self, ids: &[i64]) -> AdminResult<Vec<StaffRole>> {
        if ids.is_empty() {
            return Err(AdminError::Invalid("at least one role is required".into()));
        }
        let all = self.repo.staff_roles().await?;
        ids.iter()
            .map(|id| {
                all.iter()
                    .find(|r| r.id == *id)
                    .cloned()
                    .ok_or(AdminError::NotFound("staff role"))
            })
            .collect()
    }

    /// Appoint `account_id` with `role_ids`: adds the account to the Staff Server and its roles.
    pub async fn appoint(
        &self,
        c: &Caller,
        account_id: i64,
        role_ids: &[i64],
        reason: Option<&str>,
    ) -> AdminResult<StaffMember> {
        self.require(c, P::APPOINT_STAFF, "staff.appoint").await?;
        let reason = Self::reason(reason)?;
        if account_id == SYSTEM_USER_ID {
            return Err(AdminError::Forbidden(
                "The system user cannot be staff".into(),
            ));
        }
        let roles = self.roles_by_id(role_ids).await?;
        rules::check_assignable(c, &roles)?;
        let account = self
            .repo
            .account_detail(account_id)
            .await?
            .ok_or(AdminError::NotFound("account"))?;
        if account.account.state != AccountState::Active {
            return Err(AdminError::Conflict(
                "Only an active account can be appointed".into(),
            ));
        }
        if self.repo.get_staff(account_id).await?.is_some() {
            return Err(AdminError::Conflict("The account is already staff".into()));
        }
        let names: Vec<_> = roles.iter().map(|r| r.name.clone()).collect();
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "staff.appoint",
            Some(("account", account_id)),
            &reason,
            json!({ "roles": names }),
        );
        self.repo
            .appoint(account_id, role_ids, c.user_id, &reason, &audit)
            .await
    }

    /// Replace the roles of a staff member.
    pub async fn set_staff_roles(
        &self,
        c: &Caller,
        account_id: i64,
        role_ids: &[i64],
        reason: Option<&str>,
    ) -> AdminResult<StaffMember> {
        self.require(c, P::APPOINT_STAFF, "staff.set_roles").await?;
        let reason = Self::reason(reason)?;
        let target = self
            .repo
            .get_staff(account_id)
            .await?
            .ok_or(AdminError::NotFound("staff member"))?;
        rules::check_manageable(c, &target)?;
        if account_id == c.user_id && !c.is_owner {
            return Err(AdminError::Forbidden(
                "You cannot change your own roles".into(),
            ));
        }
        let roles = self.roles_by_id(role_ids).await?;
        rules::check_assignable(c, &roles)?;
        let names: Vec<_> = roles.iter().map(|r| r.name.clone()).collect();
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "staff.set_roles",
            Some(("account", account_id)),
            &reason,
            json!({ "roles": names }),
        );
        self.repo
            .set_staff_roles(account_id, role_ids, &audit)
            .await
    }

    /// Revoke an appointment: the account leaves the Staff Server and loses every
    /// platform permission on its next request.
    pub async fn revoke_staff(
        &self,
        c: &Caller,
        account_id: i64,
        reason: Option<&str>,
    ) -> AdminResult<()> {
        self.require(c, P::APPOINT_STAFF, "staff.revoke").await?;
        let reason = Self::reason(reason)?;
        let target = self
            .repo
            .get_staff(account_id)
            .await?
            .ok_or(AdminError::NotFound("staff member"))?;
        rules::check_manageable(c, &target)?;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "staff.revoke",
            Some(("account", account_id)),
            &reason,
            json!({ "username": target.username }),
        );
        self.repo
            .revoke_staff(account_id, c.user_id, &reason, &audit)
            .await?;
        self.alert(
            StaffAlertKind::StaffRevoked,
            format!("{} was removed from staff", target.username),
        )
        .await;
        Ok(())
    }

    /// Operator only (the CLI): hand the Staff Server and the Owner role to `new_owner`.
    pub async fn transfer_owner(
        &self,
        c: &Caller,
        new_owner: i64,
        reason: Option<&str>,
    ) -> AdminResult<()> {
        if !c.is_owner {
            return Err(AdminError::NotStaff);
        }
        let reason = Self::reason(reason)?;
        let account = self
            .repo
            .account_detail(new_owner)
            .await?
            .ok_or(AdminError::NotFound("account"))?;
        if account.account.state != AccountState::Active || new_owner == SYSTEM_USER_ID {
            return Err(AdminError::Conflict(
                "The new Owner must be an active account".into(),
            ));
        }
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "staff.transfer_owner",
            Some(("account", new_owner)),
            &reason,
            json!({}),
        );
        self.repo.transfer_owner(new_owner, &audit).await
    }

    // ===== accounts =======================================================

    pub async fn search_accounts(
        &self,
        c: &Caller,
        mut q: AccountQuery,
        limit: Option<i64>,
    ) -> AdminResult<AccountPage> {
        self.require(c, P::MANAGE_ACCOUNTS, "accounts.search")
            .await?;
        let limit = page_limit(limit);
        q.limit = limit + 1;
        q.q = q.q.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
        let mut rows = self.repo.search_accounts(&q).await?;
        let more = rows.len() as i64 > limit;
        rows.truncate(limit as usize);
        let next_after = more
            .then(|| rows.last().map(|r| r.id.to_string()))
            .flatten();
        Ok(AccountPage {
            items: rows,
            next_after,
        })
    }

    /// Account drawer; reading it is logged because it shows email and login IPs.
    pub async fn account_detail(&self, c: &Caller, id: i64) -> AdminResult<AccountDetail> {
        self.require(c, P::MANAGE_ACCOUNTS, "accounts.view").await?;
        let d = self
            .repo
            .account_detail(id)
            .await?
            .ok_or(AdminError::NotFound("account"))?;
        let e = self.entry(
            c,
            AccessKind::Read,
            "account.view",
            Some(("account", id)),
            "",
            json!({}),
        );
        self.repo.append_log(&e).await?;
        Ok(d)
    }

    /// Guard shared by actions on a single account.
    async fn guard_account(
        &self,
        c: &Caller,
        id: i64,
        allow_self: bool,
    ) -> AdminResult<AccountDetail> {
        if id == SYSTEM_USER_ID {
            return Err(AdminError::Forbidden(
                "The system user cannot be changed".into(),
            ));
        }
        let d = self
            .repo
            .account_detail(id)
            .await?
            .ok_or(AdminError::NotFound("account"))?;
        if id == c.user_id && !allow_self {
            return Err(AdminError::Forbidden(
                "You cannot do that to your own account".into(),
            ));
        }
        if !c.is_owner && self.repo.get_staff(id).await?.is_some() {
            return Err(AdminError::Forbidden(
                "Only the Owner can act on a staff account".into(),
            ));
        }
        Ok(d)
    }

    /// Suspend (`active` -> `deactivated`, sessions revoked) or unsuspend.
    pub async fn set_suspended(
        &self,
        c: &Caller,
        id: i64,
        suspend: bool,
        reason: Option<&str>,
    ) -> AdminResult<AccountSummary> {
        let action = if suspend {
            "account.suspend"
        } else {
            "account.unsuspend"
        };
        self.require(c, P::MANAGE_ACCOUNTS, action).await?;
        let reason = Self::reason(reason)?;
        let d = self.guard_account(c, id, false).await?;
        let (need, to) = if suspend {
            (AccountState::Active, AccountState::Deactivated)
        } else {
            (AccountState::Deactivated, AccountState::Active)
        };
        if d.account.state != need {
            return Err(AdminError::Conflict(format!(
                "The account is {}, not {}",
                d.account.state.as_str(),
                need.as_str()
            )));
        }
        let cap = self.setting_i64(settings::ACCOUNT_CAP).await?;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            action,
            Some(("account", id)),
            &reason,
            json!({}),
        );
        self.repo.set_account_state(id, to, cap, &audit).await
    }

    pub async fn force_logout(
        &self,
        c: &Caller,
        id: i64,
        reason: Option<&str>,
    ) -> AdminResult<u64> {
        self.require(c, P::MANAGE_ACCOUNTS, "account.force_logout")
            .await?;
        let reason = Self::reason(reason)?;
        self.guard_account(c, id, true).await?;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "account.force_logout",
            Some(("account", id)),
            &reason,
            json!({}),
        );
        self.repo.revoke_sessions(id, &audit).await
    }

    pub async fn reset_2fa(
        &self,
        c: &Caller,
        id: i64,
        reason: Option<&str>,
    ) -> AdminResult<Reset2faReport> {
        self.require(c, P::MANAGE_ACCOUNTS, "account.reset_2fa")
            .await?;
        let reason = Self::reason(reason)?;
        self.guard_account(c, id, false).await?;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "account.reset_2fa",
            Some(("account", id)),
            &reason,
            json!({}),
        );
        self.repo.reset_2fa(id, &audit).await
    }

    /// Stub of the deletion flow (doc 06 section 2): records a pending request only.
    pub async fn request_delete(
        &self,
        c: &Caller,
        id: i64,
        reason: Option<&str>,
    ) -> AdminResult<DeleteRequest> {
        self.require(c, P::MANAGE_ACCOUNTS, "account.delete_request")
            .await?;
        let reason = Self::reason(reason)?;
        let d = self.guard_account(c, id, false).await?;
        if d.account.state == AccountState::Deleted {
            return Err(AdminError::Conflict(
                "The account is already deleted".into(),
            ));
        }
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "account.delete_request",
            Some(("account", id)),
            &reason,
            json!({}),
        );
        self.repo
            .create_delete_request(id, c.user_id, &reason, &audit)
            .await
    }

    // ===== persons and bans ================================================

    pub async fn person_detail(&self, c: &Caller, person_id: i64) -> AdminResult<PersonDetail> {
        self.require(c, P::MANAGE_ACCOUNTS, "person.view").await?;
        let d = self
            .repo
            .person_detail(person_id)
            .await?
            .ok_or(AdminError::NotFound("person"))?;
        let e = self.entry(
            c,
            AccessKind::Read,
            "person.view",
            Some(("person", person_id)),
            "",
            json!({}),
        );
        self.repo.append_log(&e).await?;
        Ok(d)
    }

    /// Ban the Person behind `account_id`; the account is the source for inviter suspicion.
    pub async fn ban_account(
        &self,
        c: &Caller,
        account_id: i64,
        category: BanCategory,
        note: Option<&str>,
    ) -> AdminResult<BanOutcome> {
        self.require(c, P::PLATFORM_BAN, "person.ban").await?;
        let a = self
            .repo
            .account_detail(account_id)
            .await?
            .ok_or(AdminError::NotFound("account"))?;
        self.ban_person(c, a.account.person_id, Some(account_id), category, note)
            .await
    }

    /// Ban the Person (all its live accounts). `source_account` is the account the staff
    /// member acted on; a serious ban raises suspicion on that account's inviter.
    pub async fn ban_person(
        &self,
        c: &Caller,
        person_id: i64,
        source_account: Option<i64>,
        category: BanCategory,
        note: Option<&str>,
    ) -> AdminResult<BanOutcome> {
        self.require(c, P::PLATFORM_BAN, "person.ban").await?;
        let note = Self::reason(note)?;
        let person = self
            .repo
            .person_detail(person_id)
            .await?
            .ok_or(AdminError::NotFound("person"))?;
        if person
            .ban
            .as_ref()
            .is_some_and(|b| b.lifted_at_ms.is_none())
        {
            return Err(AdminError::Conflict("The Person is already banned".into()));
        }
        for a in &person.accounts {
            if a.id == SYSTEM_USER_ID {
                return Err(AdminError::Forbidden(
                    "The system user cannot be banned".into(),
                ));
            }
            if a.id == c.user_id {
                return Err(AdminError::Forbidden("You cannot ban yourself".into()));
            }
            if self.repo.get_staff(a.id).await?.is_some() {
                return Err(AdminError::Conflict(
                    "Revoke the staff appointment before banning this Person".into(),
                ));
            }
        }
        if let Some(src) = source_account
            && !person.accounts.iter().any(|a| a.id == src)
        {
            return Err(AdminError::Invalid(
                "source_account_id does not belong to the Person".into(),
            ));
        }
        let freeze = self.setting_i64(settings::SUSPICION_FREEZE).await?;
        let escalate = self.setting_i64(settings::SUSPICION_ESCALATE).await?;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "person.ban",
            Some(("person", person_id)),
            &note,
            json!({ "category": category.as_str() }),
        );
        let out = self
            .repo
            .ban_person(
                person_id,
                source_account,
                category,
                &note,
                c.user_id,
                freeze,
                escalate,
                &audit,
            )
            .await?;
        if let Some(s) = &out.inviter_suspicion {
            self.alert(
                StaffAlertKind::InviterSuspicion,
                format!(
                    "Serious ban raised suspicion on inviter account {} (level {}{})",
                    s.inviter_account_id,
                    s.level,
                    if s.frozen { ", invites frozen" } else { "" }
                ),
            )
            .await;
            if s.escalated {
                self.alert(
                    StaffAlertKind::InviterEscalated,
                    format!(
                        "Inviter account {} reached suspicion level {}: senior review needed",
                        s.inviter_account_id, s.level
                    ),
                )
                .await;
            }
        }
        Ok(out)
    }

    pub async fn unban_person(
        &self,
        c: &Caller,
        person_id: i64,
        note: Option<&str>,
    ) -> AdminResult<UnbanOutcome> {
        self.require(c, P::PLATFORM_BAN, "person.unban").await?;
        let note = Self::reason(note)?;
        let person = self
            .repo
            .person_detail(person_id)
            .await?
            .ok_or(AdminError::NotFound("person"))?;
        if !person
            .ban
            .as_ref()
            .is_some_and(|b| b.lifted_at_ms.is_none())
        {
            return Err(AdminError::Conflict("The Person is not banned".into()));
        }
        let cap = self.setting_i64(settings::ACCOUNT_CAP).await?;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "person.unban",
            Some(("person", person_id)),
            &note,
            json!({}),
        );
        self.repo
            .unban_person(person_id, cap, c.user_id, &note, &audit)
            .await
    }

    pub async fn list_bans(
        &self,
        c: &Caller,
        active_only: bool,
        after: Option<i64>,
        limit: Option<i64>,
    ) -> AdminResult<BanPage> {
        self.require(c, P::PLATFORM_BAN, "bans.list").await?;
        let limit = page_limit(limit);
        let mut rows = self.repo.list_bans(active_only, after, limit + 1).await?;
        let more = rows.len() as i64 > limit;
        rows.truncate(limit as usize);
        let next_after = more
            .then(|| rows.last().map(|r| r.id.to_string()))
            .flatten();
        Ok(BanPage {
            items: rows,
            next_after,
        })
    }

    // ===== invite graph and credits =======================================

    pub async fn invite_tree(&self, c: &Caller, account_id: i64) -> AdminResult<InviteTree> {
        self.require(c, P::MANAGE_INVITES, "invites.tree").await?;
        let t = self
            .repo
            .invite_tree(account_id)
            .await?
            .ok_or(AdminError::NotFound("account"))?;
        let e = self.entry(
            c,
            AccessKind::Read,
            "invites.tree",
            Some(("account", account_id)),
            "",
            json!({}),
        );
        self.repo.append_log(&e).await?;
        Ok(t)
    }

    pub async fn invite_descendants(
        &self,
        c: &Caller,
        account_id: i64,
        depth: Option<i32>,
    ) -> AdminResult<InviteDescendants> {
        self.require(c, P::MANAGE_INVITES, "invites.descendants")
            .await?;
        let depth = depth.unwrap_or(3).clamp(1, 20);
        let d = self
            .repo
            .invite_descendants(account_id, depth, MAX_GRAPH_NODES)
            .await?
            .ok_or(AdminError::NotFound("account"))?;
        let e = self.entry(
            c,
            AccessKind::Read,
            "invites.descendants",
            Some(("account", account_id)),
            "",
            json!({ "depth": depth }),
        );
        self.repo.append_log(&e).await?;
        Ok(d)
    }

    pub async fn credit_ledger(
        &self,
        c: &Caller,
        account_id: i64,
        limit: Option<i64>,
    ) -> AdminResult<CreditLedger> {
        self.require(c, P::MANAGE_INVITES, "invites.ledger").await?;
        let l = self
            .repo
            .credit_ledger(account_id, page_limit(limit))
            .await?
            .ok_or(AdminError::NotFound("account"))?;
        let e = self.entry(
            c,
            AccessKind::Read,
            "invites.ledger",
            Some(("account", account_id)),
            "",
            json!({}),
        );
        self.repo.append_log(&e).await?;
        Ok(l)
    }

    /// Grant (`delta > 0`) or revoke (`delta < 0`) invite credits; the reason is mandatory.
    pub async fn adjust_credits(
        &self,
        c: &Caller,
        account_id: i64,
        delta: i32,
        reason: Option<&str>,
    ) -> AdminResult<i64> {
        self.require(c, P::MANAGE_INVITES, "invites.adjust_credits")
            .await?;
        let reason = Self::reason(reason)?;
        if delta == 0 || delta.unsigned_abs() > 1000 {
            return Err(AdminError::Invalid(
                "delta must be a non-zero number between -1000 and 1000".into(),
            ));
        }
        if account_id == SYSTEM_USER_ID {
            return Err(AdminError::Forbidden(
                "The system user has no credits".into(),
            ));
        }
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "invites.adjust_credits",
            Some(("account", account_id)),
            &reason,
            json!({ "delta": delta }),
        );
        self.repo
            .adjust_credits(account_id, delta, c.user_id, &audit)
            .await
    }

    pub async fn list_invites(
        &self,
        c: &Caller,
        mut q: InviteQuery,
        limit: Option<i64>,
    ) -> AdminResult<InvitePage> {
        self.require(c, P::MANAGE_INVITES, "invites.list").await?;
        if let Some(s) = &q.state
            && !matches!(s.as_str(), "open" | "revoked" | "redeemed")
        {
            return Err(AdminError::Invalid(
                "state must be open, revoked or redeemed".into(),
            ));
        }
        let limit = page_limit(limit);
        q.limit = limit + 1;
        let mut rows = self.repo.list_invites(&q).await?;
        let more = rows.len() as i64 > limit;
        rows.truncate(limit as usize);
        let next_after = more
            .then(|| rows.last().map(|r| r.id.to_string()))
            .flatten();
        Ok(InvitePage {
            items: rows,
            next_after,
        })
    }

    pub async fn revoke_invite(
        &self,
        c: &Caller,
        invite_id: i64,
        reason: Option<&str>,
    ) -> AdminResult<crate::model::InviteRow> {
        self.require(c, P::MANAGE_INVITES, "invites.revoke").await?;
        let reason = Self::reason(reason)?;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "invites.revoke",
            Some(("invite", invite_id)),
            &reason,
            json!({}),
        );
        self.repo.revoke_invite(invite_id, &audit).await
    }

    pub async fn list_suspects(&self, c: &Caller) -> AdminResult<Vec<SuspectPerson>> {
        self.require(c, P::MANAGE_INVITES, "invites.suspects")
            .await?;
        let freeze = self.setting_i64(settings::SUSPICION_FREEZE).await?;
        self.repo.list_suspects(freeze).await
    }

    pub async fn clear_suspicion(
        &self,
        c: &Caller,
        person_id: i64,
        reason: Option<&str>,
    ) -> AdminResult<()> {
        self.require(c, P::MANAGE_INVITES, "invites.clear_suspicion")
            .await?;
        let reason = Self::reason(reason)?;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "invites.clear_suspicion",
            Some(("person", person_id)),
            &reason,
            json!({}),
        );
        self.repo
            .clear_suspicion(person_id, c.user_id, &audit)
            .await
    }

    // ===== platform settings ==============================================

    pub async fn settings_view(&self, c: &Caller) -> AdminResult<Vec<SettingView>> {
        self.require(c, P::MANAGE_FEATURES, "settings.view").await?;
        let stored = self.repo.settings().await?;
        Ok(settings::CATALOG
            .iter()
            .map(|d| {
                let s = stored.get(d.key);
                SettingView {
                    key: d.key.to_owned(),
                    group: d.group.to_owned(),
                    kind: d.kind,
                    description: d.description.to_owned(),
                    min: d.min,
                    max: d.max,
                    default: d.default_value(),
                    value: s.map_or_else(|| d.default_value(), |s| s.value.clone()),
                    is_default: s.is_none(),
                    updated_at_ms: s.map(|s| s.updated_at_ms),
                }
            })
            .collect())
    }

    /// Validate and store `values` atomically; unknown keys and out-of-range values reject the whole patch.
    pub async fn patch_settings(
        &self,
        c: &Caller,
        values: &[(String, Value)],
        reason: Option<&str>,
    ) -> AdminResult<Vec<SettingView>> {
        self.require(c, P::MANAGE_FEATURES, "settings.patch")
            .await?;
        let reason = Self::optional_reason(reason)?;
        if values.is_empty() {
            return Err(AdminError::Invalid("no settings given".into()));
        }
        let mut clean = Vec::with_capacity(values.len());
        let mut problems = Vec::new();
        for (k, v) in values {
            match settings::find(k) {
                None => problems.push(format!("{k}: unknown setting")),
                Some(def) => match SettingDef::validate(def, v) {
                    Ok(v) => clean.push((k.clone(), v)),
                    Err(e) => problems.push(format!("{k}: {e}")),
                },
            }
        }
        if !problems.is_empty() {
            return Err(AdminError::Invalid(problems.join("; ")));
        }
        let (freeze, escalate) = (
            clean
                .iter()
                .find(|(k, _)| k == settings::SUSPICION_FREEZE)
                .and_then(|(_, v)| v.as_i64()),
            clean
                .iter()
                .find(|(k, _)| k == settings::SUSPICION_ESCALATE)
                .and_then(|(_, v)| v.as_i64()),
        );
        let freeze_now = match freeze {
            Some(v) => v,
            None => self.setting_i64(settings::SUSPICION_FREEZE).await?,
        };
        let escalate_now = match escalate {
            Some(v) => v,
            None => self.setting_i64(settings::SUSPICION_ESCALATE).await?,
        };
        if escalate_now < freeze_now {
            return Err(AdminError::Invalid(
                "the escalation threshold cannot be below the freeze threshold".into(),
            ));
        }
        let keys: Vec<_> = clean
            .iter()
            .map(|(k, v)| json!({ "key": k, "value": v }))
            .collect();
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "settings.patch",
            None,
            &reason,
            json!({ "changes": keys }),
        );
        self.repo.put_settings(&clean, c.user_id, &audit).await?;
        self.settings_view(c).await
    }

    // ===== system ==========================================================

    /// Dashboard counts (`STAFF_BASIC`).
    pub async fn stats(&self, c: &Caller) -> AdminResult<SystemCounts> {
        self.require(c, P::STAFF_BASIC, "stats.view").await?;
        self.repo.counts().await
    }

    /// Counts for the System page (`OPERATOR`).
    pub async fn system_counts(&self, c: &Caller) -> AdminResult<SystemCounts> {
        self.require(c, P::OPERATOR, "system.view").await?;
        self.repo.counts().await
    }

    pub async fn list_jobs(
        &self,
        c: &Caller,
        state: Option<&str>,
        after: Option<i64>,
        limit: Option<i64>,
    ) -> AdminResult<JobPage> {
        self.require(c, P::MANAGE_TRANSCODE, "jobs.list").await?;
        if let Some(s) = state
            && !matches!(s, "queued" | "running" | "succeeded" | "dead")
        {
            return Err(AdminError::Invalid(
                "state must be queued, running, succeeded or dead".into(),
            ));
        }
        let limit = page_limit(limit);
        let mut rows = self.repo.list_jobs(state, after, limit + 1).await?;
        let more = rows.len() as i64 > limit;
        rows.truncate(limit as usize);
        let next_after = more
            .then(|| rows.last().map(|r| r.id.to_string()))
            .flatten();
        Ok(JobPage {
            items: rows,
            next_after,
        })
    }

    pub async fn retry_job(&self, c: &Caller, id: i64, reason: Option<&str>) -> AdminResult<()> {
        self.require(c, P::MANAGE_TRANSCODE, "jobs.retry").await?;
        let reason = Self::optional_reason(reason)?;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "jobs.retry",
            Some(("job", id)),
            &reason,
            json!({}),
        );
        self.repo.retry_job(id, &audit).await
    }

    pub async fn delete_job(&self, c: &Caller, id: i64, reason: Option<&str>) -> AdminResult<()> {
        self.require(c, P::MANAGE_TRANSCODE, "jobs.delete").await?;
        let reason = Self::optional_reason(reason)?;
        let audit = self.entry(
            c,
            AccessKind::AdminAction,
            "jobs.delete",
            Some(("job", id)),
            &reason,
            json!({}),
        );
        self.repo.delete_job(id, &audit).await
    }

    // ===== access log ======================================================

    /// Record an admin action performed outside this service (feature toggles).
    pub async fn record(
        &self,
        c: &Caller,
        action: &str,
        target: Option<(&str, i64)>,
        reason: &str,
        detail: Value,
    ) -> AdminResult<()> {
        let e = self.entry(c, AccessKind::AdminAction, action, target, reason, detail);
        self.repo.append_log(&e).await.map(|_| ())
    }

    /// Require a permission from outside this service (feature toggles).
    pub async fn authorize(&self, c: &Caller, perm: P, action: &str) -> AdminResult<()> {
        self.require(c, perm, action).await
    }

    /// Log a read of private content. High-level staff only; the reason is mandatory.
    /// The caller returns the content only after this succeeded.
    pub async fn log_private_read(
        &self,
        c: &Caller,
        target_kind: &str,
        target_id: i64,
        reason: Option<&str>,
    ) -> AdminResult<()> {
        self.require(c, P::READ_PRIVATE_CONTENT, "private.read")
            .await?;
        let reason = Self::reason(reason)?;
        let e = self.entry(
            c,
            AccessKind::PrivateRead,
            "private.read",
            Some((target_kind, target_id)),
            &reason,
            json!({}),
        );
        self.repo.append_log(&e).await.map(|_| ())
    }

    pub async fn access_log(
        &self,
        c: &Caller,
        mut f: AccessLogFilter,
        limit: Option<i64>,
    ) -> AdminResult<AccessLogPage> {
        self.require(c, P::VIEW_ACCESS_LOG, "access_log.view")
            .await?;
        let limit = page_limit(limit);
        f.limit = limit + 1;
        let mut rows = self.repo.list_log(&f).await?;
        let more = rows.len() as i64 > limit;
        rows.truncate(limit as usize);
        let next_before = more.then(|| rows.last().map(|r| r.seq)).flatten();
        Ok(AccessLogPage {
            items: rows,
            next_before,
        })
    }

    /// Verify the whole chain; a break raises a staff alert.
    pub async fn verify_access_log(&self, c: &Caller) -> AdminResult<ChainReport> {
        self.require(c, P::VIEW_ACCESS_LOG, "access_log.verify")
            .await?;
        self.verify_and_alert().await
    }

    /// Chain verification without a caller (the nightly job and `sr-cli verify-access-log`).
    pub async fn verify_and_alert(&self) -> AdminResult<ChainReport> {
        let report = self.repo.verify_log().await?;
        if !report.ok {
            self.alert(
                StaffAlertKind::AccessLogChainBroken,
                format!(
                    "Access-log chain broken at row {}: {}",
                    report.first_bad_seq.unwrap_or(0),
                    report.problem.clone().unwrap_or_default()
                ),
            )
            .await;
        }
        Ok(report)
    }
}
