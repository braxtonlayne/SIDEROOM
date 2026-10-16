//! In-memory [`AdminRepository`] for service and route tests. It mirrors the
//! Postgres behaviour closely enough for the rules in [`crate::service`]: a
//! hash-chained log, appointments plus Staff Server membership, Person bans
//! with inviter suspicion, and the credit ledger.

use crate::{
    chain,
    error::{AdminError, AdminResult},
    ext::{
        AdminGuild, AdminGuildRef, AdminSession, Announcement, Delegation, NewAnnouncement,
        NewOffer, OfferState, PrivateChannel, PrivateEdit, PrivateMessage, StaffAlertRow,
        StaffExtrasRepository, StaffOffer, SuspicionEvent,
    },
    model::{
        AccessLogEntry, AccessLogFilter, AccountDetail, AccountQuery, AccountRef, AccountState,
        AccountSummary, AuditEntry, BanCategory, BanOutcome, BanRecord, ChainReport, CreditLedger,
        DeleteRequest, InviteDescendants, InviteNode, InviteQuery, InviteRow, InviteStats,
        InviteTree, JobRow, LedgerEntry, PersonDetail, Reset2faReport, StaffMember, StaffRole,
        Standing, StoredSetting, SuspectPerson, SystemCounts, UnbanOutcome,
    },
    repo::AdminRepository,
};
use async_trait::async_trait;
use serde_json::Value;
use sr_core::{Clock, PlatformPermissions as P, SystemClock};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, MutexGuard},
};

#[derive(Debug, Clone)]
struct Acct {
    s: AccountSummary,
    invited_by: Option<i64>,
    sessions: u64,
    mfa: bool,
    password_reset_forced: bool,
    invites_frozen: bool,
}

#[derive(Debug, Clone)]
struct Appt {
    account_id: i64,
    by: i64,
    at_ms: i64,
    reason: String,
    revoked: bool,
}

#[derive(Debug, Default)]
struct State {
    next_id: i64,
    accounts: Vec<Acct>,
    persons: HashMap<i64, (String, i32)>,
    roles: Vec<StaffRole>,
    members: HashSet<i64>,
    member_roles: HashMap<i64, Vec<i64>>,
    owner: Option<i64>,
    appts: Vec<Appt>,
    bans: Vec<BanRecord>,
    invites: Vec<InviteRow>,
    ledger: Vec<(i64, LedgerEntry)>,
    settings: HashMap<String, StoredSetting>,
    jobs: Vec<JobRow>,
    log: Vec<AccessLogEntry>,
    delete_requests: Vec<DeleteRequest>,
    deleted_jobs: Vec<i64>,
    delegations: Vec<Delegation>,
    offers: Vec<StaffOffer>,
    announcements: Vec<Announcement>,
    suspicion_events: Vec<(i64, SuspicionEvent)>,
    guilds: Vec<AdminGuild>,
    entrance_copy: std::collections::BTreeMap<String, String>,
    alerts: Vec<StaffAlertRow>,
    private_channels: Vec<PrivateChannel>,
    private_messages: Vec<(i64, PrivateMessage)>,
}

impl State {
    fn id(&mut self) -> i64 {
        self.next_id += 1;
        self.next_id
    }

    fn acct(&self, id: i64) -> Option<&Acct> {
        self.accounts.iter().find(|a| a.s.id == id)
    }

    fn staff_member(&self, account_id: i64) -> Option<StaffMember> {
        let a = self.acct(account_id)?;
        let is_owner = self.owner == Some(account_id);
        let appt = self
            .appts
            .iter()
            .find(|p| p.account_id == account_id && !p.revoked);
        if !is_owner && (appt.is_none() || !self.members.contains(&account_id)) {
            return None;
        }
        let roles: Vec<StaffRole> = self
            .member_roles
            .get(&account_id)
            .into_iter()
            .flatten()
            .filter_map(|rid| self.roles.iter().find(|r| r.id == *rid).cloned())
            .collect();
        let mut bits = P::from_roles(roles.iter().map(StaffRole::permissions));
        if is_owner {
            bits |= P::OWNER;
        }
        Some(StaffMember {
            account_id,
            username: a.s.username.clone(),
            display_name: a.s.display_name.clone(),
            is_owner,
            roles,
            platform_permissions: bits.bits().to_string(),
            appointed_by: appt.map(|p| p.by),
            appointed_at_ms: appt.map_or(0, |p| p.at_ms),
            reason: appt.map(|p| p.reason.clone()).unwrap_or_default(),
        })
    }

    fn balance(&self, account: i64) -> i64 {
        self.ledger
            .iter()
            .filter(|(a, _)| *a == account)
            .map(|(_, e)| i64::from(e.delta))
            .sum()
    }

    fn suspicion(&self, person: i64) -> i32 {
        self.persons.get(&person).map_or(0, |p| p.1)
    }

    fn node(&self, a: &Acct, depth: i32) -> InviteNode {
        InviteNode {
            account_id: a.s.id,
            username: a.s.username.clone(),
            state: a.s.state,
            person_id: a.s.person_id,
            suspicion_level: self.suspicion(a.s.person_id),
            parent_id: a.invited_by,
            depth,
        }
    }

    fn children(&self, id: i64) -> Vec<i64> {
        self.accounts
            .iter()
            .filter(|a| a.invited_by == Some(id))
            .map(|a| a.s.id)
            .collect()
    }

    fn active_count(&self) -> i64 {
        self.accounts
            .iter()
            .filter(|a| a.s.state == AccountState::Active)
            .count() as i64
    }

    fn append(&mut self, now_ms: i64, e: &AuditEntry) -> AccessLogEntry {
        let (prev, seq) = match self.log.last() {
            Some(l) => (
                chain::from_hex(&l.hash).unwrap_or(chain::GENESIS),
                l.seq + 1,
            ),
            None => (chain::GENESIS, 1),
        };
        let row = chain::seal(&prev, seq, now_ms * 1000 + seq % 1000, e);
        self.log.push(row.clone());
        row
    }
}

/// Test double with builder-style setup helpers.
pub struct MemoryAdminRepo {
    state: Mutex<State>,
    clock: Arc<dyn Clock>,
}

impl Default for MemoryAdminRepo {
    fn default() -> Self {
        Self::new(Arc::new(SystemClock))
    }
}

impl MemoryAdminRepo {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            state: Mutex::new(State {
                next_id: 1_000,
                ..State::default()
            }),
            clock,
        }
    }

    fn s(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn now(&self) -> i64 {
        self.clock.now_unix_ms()
    }

    /// Add an account (and its Person); returns the account id. `person` reuses a Person.
    /// Seed a server for the platform-wide list.
    pub fn add_guild(&self, id: i64, name: &str, owner_id: i64, members: i64) {
        let now = self.now();
        let mut s = self.s();
        let owner_username = s.acct(owner_id).map(|a| a.s.username.clone());
        s.guilds.push(AdminGuild {
            id,
            name: name.to_owned(),
            owner_id,
            owner_username,
            member_count: members,
            channel_count: 3,
            created_at_ms: now,
            is_staff_guild: false,
            deleted: false,
            community_enabled: false,
        });
    }

    /// Point a seeded server at another owner (tests whose chat store moved the ownership).
    pub fn set_guild_owner(&self, id: i64, owner_id: i64) {
        let mut s = self.s();
        let name = s.acct(owner_id).map(|a| a.s.username.clone());
        if let Some(g) = s.guilds.iter_mut().find(|g| g.id == id) {
            g.owner_id = owner_id;
            g.owner_username = name;
        }
    }

    pub fn add_account(
        &self,
        id: i64,
        username: &str,
        state: AccountState,
        person: Option<i64>,
        invited_by: Option<i64>,
    ) -> i64 {
        let now = self.now();
        let mut s = self.s();
        let person_id = person.unwrap_or(id + 500_000);
        s.persons.entry(person_id).or_insert((String::new(), 0));
        s.accounts.push(Acct {
            s: AccountSummary {
                id,
                person_id,
                username: username.to_owned(),
                display_name: None,
                email: format!("{username}@example.test"),
                email_verified: true,
                state,
                flags: 0,
                created_at_ms: now,
                state_changed_at_ms: now,
            },
            invited_by,
            sessions: 2,
            mfa: true,
            password_reset_forced: false,
            invites_frozen: false,
        });
        id
    }

    /// Add a Staff Server role.
    pub fn add_role(&self, id: i64, name: &str, position: i32, perms: P) -> StaffRole {
        let r = StaffRole {
            id,
            name: name.to_owned(),
            position,
            platform_permissions: perms.bits().to_string(),
        };
        self.s().roles.push(r.clone());
        r
    }

    /// Make `id` the Staff Server owner (and a member).
    pub fn set_owner(&self, id: i64) {
        let mut s = self.s();
        s.owner = Some(id);
        s.members.insert(id);
    }

    /// Put an account into the Staff Server with these roles without an appointment
    /// (the "role without appointment has no effect" case).
    pub fn add_member_without_appointment(&self, id: i64, roles: &[i64]) {
        let mut s = self.s();
        s.members.insert(id);
        s.member_roles.insert(id, roles.to_vec());
    }

    /// Remove an account from the Staff Server only (leave/kick), keeping the appointment row.
    pub fn kick_from_staff_guild(&self, id: i64) {
        let mut s = self.s();
        s.members.remove(&id);
        s.member_roles.remove(&id);
        // The database closes the appointment when the member row goes (migration 0650).
        for p in s.appts.iter_mut().filter(|p| p.account_id == id) {
            p.revoked = true;
        }
        s.delegations.retain(|d| d.account_id != id);
    }

    /// Add a private channel for the private-content tests.
    pub fn add_private_channel(&self, ch: PrivateChannel) {
        self.s().private_channels.push(ch);
    }

    /// Add a message (also a deleted one) to a private channel.
    pub fn add_private_message(&self, channel: i64, m: PrivateMessage) {
        self.s().private_messages.push((channel, m));
    }

    pub fn add_invite(&self, issuer: i64, code: &str, redeemed_by: Option<i64>) -> i64 {
        let now = self.now();
        let mut s = self.s();
        let id = s.id();
        s.invites.push(InviteRow {
            id,
            code: code.to_owned(),
            issuer_account_id: issuer,
            state: if redeemed_by.is_some() {
                "redeemed"
            } else {
                "open"
            }
            .to_owned(),
            redeemed_by_account_id: redeemed_by,
            note: String::new(),
            emergency: false,
            created_at_ms: now,
        });
        id
    }

    pub fn add_credits(&self, account: i64, delta: i32) {
        let now = self.now();
        let mut s = self.s();
        let id = s.id();
        s.ledger.push((
            account,
            LedgerEntry {
                id,
                delta,
                reason: "initial_grant".into(),
                ref_id: None,
                created_at_ms: now,
            },
        ));
    }

    pub fn add_job(&self, kind: &str, state: &str) -> i64 {
        let now = self.now();
        let mut s = self.s();
        let id = s.id();
        s.jobs.push(JobRow {
            id,
            kind: kind.to_owned(),
            state: state.to_owned(),
            attempts: 5,
            max_attempts: 5,
            last_error: (state == "dead").then(|| "boom".to_owned()),
            run_at_ms: now,
            finished_at_ms: (state == "dead").then_some(now),
        });
        id
    }

    /// Session count of an account (for suspend / force-logout tests).
    pub fn sessions_of(&self, id: i64) -> u64 {
        self.s().acct(id).map_or(0, |a| a.sessions)
    }

    /// Whether staff forced a password reset on the account.
    pub fn password_reset_forced(&self, id: i64) -> bool {
        self.s().acct(id).is_some_and(|a| a.password_reset_forced)
    }

    pub fn state_of(&self, id: i64) -> Option<AccountState> {
        self.s().acct(id).map(|a| a.s.state)
    }

    /// All log rows (oldest first).
    pub fn log_rows(&self) -> Vec<AccessLogEntry> {
        self.s().log.clone()
    }

    /// Edit a stored row without re-hashing it, to simulate tampering.
    pub fn tamper_log(&self, seq: i64, reason: &str) {
        if let Some(r) = self.s().log.iter_mut().find(|r| r.seq == seq) {
            r.reason = reason.to_owned();
        }
    }
}

fn audit_in(s: &mut State, now: i64, a: &AuditEntry) {
    s.append(now, a);
}

#[async_trait]
impl AdminRepository for MemoryAdminRepo {
    async fn standing(&self, user_id: i64) -> AdminResult<Option<Standing>> {
        let s = self.s();
        let deleg = s
            .delegations
            .iter()
            .find(|d| d.account_id == user_id)
            .map(|d| P::from_bits_truncate(d.max_permissions.parse().unwrap_or(0)));
        Ok(s.staff_member(user_id).map(|m| {
            let mut permissions =
                P::from_bits_truncate(m.platform_permissions.parse().unwrap_or(0));
            if deleg.is_some() && !m.is_owner {
                permissions |= P::APPOINT_STAFF;
            }
            Standing {
                permissions,
                is_owner: m.is_owner,
                grant_ceiling: if m.is_owner { None } else { deleg },
            }
        }))
    }

    async fn staff_roles(&self) -> AdminResult<Vec<StaffRole>> {
        let mut r = self.s().roles.clone();
        r.sort_by_key(|r| r.position);
        Ok(r)
    }

    async fn list_staff(&self) -> AdminResult<Vec<StaffMember>> {
        let s = self.s();
        Ok(s.accounts
            .iter()
            .filter_map(|a| s.staff_member(a.s.id))
            .collect())
    }

    async fn get_staff(&self, account_id: i64) -> AdminResult<Option<StaffMember>> {
        Ok(self.s().staff_member(account_id))
    }

    async fn appoint(
        &self,
        account_id: i64,
        role_ids: &[i64],
        by: i64,
        reason: &str,
        audit: &AuditEntry,
    ) -> AdminResult<StaffMember> {
        let now = self.now();
        let mut s = self.s();
        if s.acct(account_id).is_none() {
            return Err(AdminError::NotFound("account"));
        }
        if s.staff_member(account_id).is_some() {
            return Err(AdminError::Conflict("The account is already staff".into()));
        }
        s.appts
            .retain(|p| !(p.account_id == account_id && p.revoked));
        s.appts.push(Appt {
            account_id,
            by,
            at_ms: now,
            reason: reason.to_owned(),
            revoked: false,
        });
        s.members.insert(account_id);
        s.member_roles.insert(account_id, role_ids.to_vec());
        audit_in(&mut s, now, audit);
        s.staff_member(account_id)
            .ok_or_else(|| AdminError::Storage("appointment vanished".into()))
    }

    async fn set_staff_roles(
        &self,
        account_id: i64,
        role_ids: &[i64],
        audit: &AuditEntry,
    ) -> AdminResult<StaffMember> {
        let now = self.now();
        let mut s = self.s();
        if s.staff_member(account_id).is_none() {
            return Err(AdminError::NotFound("staff member"));
        }
        s.member_roles.insert(account_id, role_ids.to_vec());
        audit_in(&mut s, now, audit);
        s.staff_member(account_id)
            .ok_or(AdminError::NotFound("staff member"))
    }

    async fn revoke_staff(
        &self,
        account_id: i64,
        _by: i64,
        _reason: &str,
        audit: &AuditEntry,
    ) -> AdminResult<()> {
        let now = self.now();
        let mut s = self.s();
        if s.owner == Some(account_id) {
            return Err(AdminError::Forbidden("The Owner cannot be revoked".into()));
        }
        let Some(p) = s
            .appts
            .iter_mut()
            .find(|p| p.account_id == account_id && !p.revoked)
        else {
            return Err(AdminError::NotFound("staff member"));
        };
        p.revoked = true;
        s.members.remove(&account_id);
        s.member_roles.remove(&account_id);
        s.delegations.retain(|d| d.account_id != account_id);
        audit_in(&mut s, now, audit);
        Ok(())
    }

    async fn transfer_owner(&self, new_owner: i64, audit: &AuditEntry) -> AdminResult<()> {
        let now = self.now();
        let mut s = self.s();
        if s.acct(new_owner).is_none() {
            return Err(AdminError::NotFound("account"));
        }
        let old = s.owner;
        s.owner = Some(new_owner);
        s.members.insert(new_owner);
        let owner_role = s.roles.iter().find(|r| r.name == "Owner").map(|r| r.id);
        if let Some(rid) = owner_role {
            s.member_roles.entry(new_owner).or_default().push(rid);
            if let Some(old) = old
                && let Some(v) = s.member_roles.get_mut(&old)
            {
                v.retain(|r| *r != rid);
            }
        }
        if !s
            .appts
            .iter()
            .any(|p| p.account_id == new_owner && !p.revoked)
        {
            s.appts.push(Appt {
                account_id: new_owner,
                by: audit.actor_id,
                at_ms: now,
                reason: "ownership transfer".into(),
                revoked: false,
            });
        }
        audit_in(&mut s, now, audit);
        Ok(())
    }

    async fn search_accounts(&self, q: &AccountQuery) -> AdminResult<Vec<AccountSummary>> {
        let s = self.s();
        let needle = q.q.as_ref().map(|n| n.to_lowercase());
        let mut rows: Vec<_> = s
            .accounts
            .iter()
            .map(|a| &a.s)
            .filter(|a| q.after.is_none_or(|x| a.id > x))
            .filter(|a| q.state.is_none_or(|st| a.state == st))
            .filter(|a| q.person_id.is_none_or(|p| a.person_id == p))
            .filter(|a| {
                needle.as_ref().is_none_or(|n| {
                    a.username.to_lowercase().contains(n)
                        || a.email.to_lowercase().contains(n)
                        || a.display_name
                            .as_ref()
                            .is_some_and(|d| d.to_lowercase().contains(n))
                        || a.id.to_string() == *n
                })
            })
            .cloned()
            .collect();
        rows.sort_by_key(|a| a.id);
        rows.truncate(q.limit.max(0) as usize);
        Ok(rows)
    }

    async fn account_detail(&self, id: i64) -> AdminResult<Option<AccountDetail>> {
        let s = self.s();
        let Some(a) = s.acct(id) else { return Ok(None) };
        let issued = s
            .invites
            .iter()
            .filter(|i| i.issuer_account_id == id)
            .count() as i64;
        let redeemed = s
            .invites
            .iter()
            .filter(|i| i.issuer_account_id == id && i.state == "redeemed")
            .count() as i64;
        Ok(Some(AccountDetail {
            account: a.s.clone(),
            invited_by: a.invited_by.and_then(|p| s.acct(p)).map(|p| AccountRef {
                id: p.s.id,
                username: p.s.username.clone(),
            }),
            sessions_active: a.sessions as i64,
            mfa_enabled: a.mfa,
            credit_balance: s.balance(id),
            invites_issued: issued,
            invites_redeemed: redeemed,
            suspicion_level: s.suspicion(a.s.person_id),
            person_banned: s
                .bans
                .iter()
                .any(|b| b.person_id == a.s.person_id && b.lifted_at_ms.is_none()),
            recent_logins: vec![],
            invites_frozen: a.invites_frozen,
        }))
    }

    async fn set_account_state(
        &self,
        id: i64,
        to: AccountState,
        cap: i64,
        audit: &AuditEntry,
    ) -> AdminResult<AccountSummary> {
        let now = self.now();
        let mut s = self.s();
        let active = s.active_count();
        let Some(a) = s.accounts.iter_mut().find(|a| a.s.id == id) else {
            return Err(AdminError::NotFound("account"));
        };
        match (a.s.state, to) {
            (AccountState::Active, AccountState::Deactivated) => a.sessions = 0,
            (AccountState::Deactivated, AccountState::Active) => {
                if active >= cap {
                    return Err(AdminError::Conflict("The account cap is reached".into()));
                }
            }
            _ => {
                return Err(AdminError::Conflict(
                    "That state change is not allowed".into(),
                ));
            }
        }
        a.s.state = to;
        a.s.state_changed_at_ms = now;
        let out = a.s.clone();
        audit_in(&mut s, now, audit);
        Ok(out)
    }

    async fn revoke_sessions(&self, id: i64, audit: &AuditEntry) -> AdminResult<u64> {
        let now = self.now();
        let mut s = self.s();
        let Some(a) = s.accounts.iter_mut().find(|a| a.s.id == id) else {
            return Err(AdminError::NotFound("account"));
        };
        let n = a.sessions;
        a.sessions = 0;
        audit_in(&mut s, now, audit);
        Ok(n)
    }

    async fn reset_2fa(&self, id: i64, audit: &AuditEntry) -> AdminResult<Reset2faReport> {
        let now = self.now();
        let mut s = self.s();
        let Some(a) = s.accounts.iter_mut().find(|a| a.s.id == id) else {
            return Err(AdminError::NotFound("account"));
        };
        let r = Reset2faReport {
            totp: u64::from(a.mfa),
            sessions_revoked: a.sessions,
            ..Reset2faReport::default()
        };
        a.mfa = false;
        a.sessions = 0;
        audit_in(&mut s, now, audit);
        Ok(r)
    }

    async fn create_delete_request(
        &self,
        id: i64,
        _requested_by: i64,
        _reason: &str,
        audit: &AuditEntry,
    ) -> AdminResult<DeleteRequest> {
        let now = self.now();
        let mut s = self.s();
        if s.acct(id).is_none() {
            return Err(AdminError::NotFound("account"));
        }
        let rid = s.id();
        let r = DeleteRequest {
            id: rid,
            account_id: id,
            state: "pending".into(),
            created_at_ms: now,
        };
        s.delete_requests.push(r.clone());
        audit_in(&mut s, now, audit);
        Ok(r)
    }

    async fn person_detail(&self, person_id: i64) -> AdminResult<Option<PersonDetail>> {
        let s = self.s();
        let Some((notes, level)) = s.persons.get(&person_id) else {
            return Ok(None);
        };
        Ok(Some(PersonDetail {
            id: person_id,
            notes: notes.clone(),
            suspicion_level: *level,
            accounts: s
                .accounts
                .iter()
                .filter(|a| a.s.person_id == person_id)
                .map(|a| a.s.clone())
                .collect(),
            ban: s
                .bans
                .iter()
                .rev()
                .find(|b| b.person_id == person_id)
                .cloned(),
        }))
    }

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
    ) -> AdminResult<BanOutcome> {
        let now = self.now();
        let mut s = self.s();
        if s.bans
            .iter()
            .any(|b| b.person_id == person_id && b.lifted_at_ms.is_none())
        {
            return Err(AdminError::Conflict("The Person is already banned".into()));
        }
        let (mut banned, mut sessions) = (0, 0);
        for a in s.accounts.iter_mut().filter(|a| a.s.person_id == person_id) {
            if matches!(a.s.state, AccountState::Active | AccountState::Deactivated) {
                a.s.state = AccountState::PlatformBanned;
                a.s.state_changed_at_ms = now;
                banned += 1;
            }
            sessions += a.sessions;
            a.sessions = 0;
        }
        let id = s.id();
        let ban = BanRecord {
            id,
            person_id,
            category,
            note: note.to_owned(),
            banned_by: by,
            created_at_ms: now,
            lifted_at_ms: None,
        };
        s.bans.push(ban.clone());
        let mut inviter_suspicion = None;
        if category == BanCategory::Serious
            && let Some(src) = source_account
            && let Some(inviter) = s.acct(src).and_then(|a| a.invited_by)
            && let Some(ip) = s.acct(inviter).map(|a| a.s.person_id)
        {
            let e = s.persons.entry(ip).or_insert((String::new(), 0));
            e.1 += 1;
            let level = e.1;
            inviter_suspicion = Some(crate::model::SuspicionChange {
                person_id: ip,
                inviter_account_id: inviter,
                level,
                frozen: i64::from(level) >= freeze_at,
                escalated: i64::from(level) >= escalate_at,
            });
        }
        audit_in(&mut s, now, audit);
        Ok(BanOutcome {
            ban,
            accounts_banned: banned,
            sessions_revoked: sessions,
            inviter_suspicion,
        })
    }

    async fn unban_person(
        &self,
        person_id: i64,
        cap: i64,
        _by: i64,
        _note: &str,
        audit: &AuditEntry,
    ) -> AdminResult<UnbanOutcome> {
        let now = self.now();
        let mut s = self.s();
        let Some(b) = s
            .bans
            .iter_mut()
            .find(|b| b.person_id == person_id && b.lifted_at_ms.is_none())
        else {
            return Err(AdminError::Conflict("The Person is not banned".into()));
        };
        b.lifted_at_ms = Some(now);
        let mut active = s.active_count();
        let (mut ok, mut left) = (0, 0);
        for a in s
            .accounts
            .iter_mut()
            .filter(|a| a.s.person_id == person_id && a.s.state == AccountState::PlatformBanned)
        {
            if active < cap {
                a.s.state = AccountState::Active;
                active += 1;
                ok += 1;
            } else {
                a.s.state = AccountState::Deactivated;
                left += 1;
            }
        }
        audit_in(&mut s, now, audit);
        Ok(UnbanOutcome {
            accounts_reactivated: ok,
            accounts_left_deactivated: left,
        })
    }

    async fn list_bans(
        &self,
        active_only: bool,
        after: Option<i64>,
        limit: i64,
    ) -> AdminResult<Vec<BanRecord>> {
        let s = self.s();
        let mut v: Vec<_> = s
            .bans
            .iter()
            .filter(|b| !active_only || b.lifted_at_ms.is_none())
            .filter(|b| after.is_none_or(|x| b.id < x))
            .cloned()
            .collect();
        v.sort_by_key(|b| std::cmp::Reverse(b.id));
        v.truncate(limit.max(0) as usize);
        Ok(v)
    }

    async fn list_suspects(&self, freeze_at: i64) -> AdminResult<Vec<SuspectPerson>> {
        let s = self.s();
        let mut v: Vec<_> = s
            .persons
            .iter()
            .filter(|(_, (_, l))| *l > 0)
            .map(|(id, (_, l))| SuspectPerson {
                person_id: *id,
                suspicion_level: *l,
                frozen: i64::from(*l) >= freeze_at,
                accounts: s
                    .accounts
                    .iter()
                    .filter(|a| a.s.person_id == *id)
                    .map(|a| AccountRef {
                        id: a.s.id,
                        username: a.s.username.clone(),
                    })
                    .collect(),
            })
            .collect();
        v.sort_by_key(|p| std::cmp::Reverse(p.suspicion_level));
        Ok(v)
    }

    async fn clear_suspicion(
        &self,
        person_id: i64,
        _by: i64,
        audit: &AuditEntry,
    ) -> AdminResult<()> {
        let now = self.now();
        let mut s = self.s();
        let Some(p) = s.persons.get_mut(&person_id) else {
            return Err(AdminError::NotFound("person"));
        };
        p.1 = 0;
        audit_in(&mut s, now, audit);
        Ok(())
    }

    async fn invite_tree(&self, account_id: i64) -> AdminResult<Option<InviteTree>> {
        let s = self.s();
        let Some(a) = s.acct(account_id) else {
            return Ok(None);
        };
        let mut ancestors = Vec::new();
        let (mut cur, mut depth) = (a.invited_by, -1);
        while let Some(p) = cur.and_then(|id| s.acct(id)) {
            ancestors.push(s.node(p, depth));
            depth -= 1;
            cur = p.invited_by;
            if ancestors.len() > 1000 {
                break;
            }
        }
        ancestors.reverse();
        let mut stats = InviteStats::default();
        for i in s
            .invites
            .iter()
            .filter(|i| i.issuer_account_id == account_id)
        {
            stats.issued += 1;
            match i.state.as_str() {
                "open" => stats.open += 1,
                "redeemed" => stats.redeemed += 1,
                _ => stats.revoked += 1,
            }
        }
        let mut stack = s.children(account_id);
        while let Some(id) = stack.pop() {
            stats.descendants += 1;
            if s.acct(id)
                .is_some_and(|d| d.s.state == AccountState::PlatformBanned)
            {
                stats.banned_descendants += 1;
            }
            stack.extend(s.children(id));
        }
        Ok(Some(InviteTree {
            account: s.node(a, 0),
            ancestors,
            stats,
        }))
    }

    async fn invite_descendants(
        &self,
        account_id: i64,
        max_depth: i32,
        max_nodes: i64,
    ) -> AdminResult<Option<InviteDescendants>> {
        let s = self.s();
        if s.acct(account_id).is_none() {
            return Ok(None);
        }
        let mut nodes = Vec::new();
        let mut frontier = vec![account_id];
        let mut truncated = false;
        'outer: for depth in 1..=max_depth {
            let mut next = Vec::new();
            for p in frontier {
                for c in s.children(p) {
                    if nodes.len() as i64 >= max_nodes {
                        truncated = true;
                        break 'outer;
                    }
                    if let Some(a) = s.acct(c) {
                        nodes.push(s.node(a, depth));
                        next.push(c);
                    }
                }
            }
            frontier = next;
        }
        Ok(Some(InviteDescendants { nodes, truncated }))
    }

    async fn credit_ledger(
        &self,
        account_id: i64,
        limit: i64,
    ) -> AdminResult<Option<CreditLedger>> {
        let s = self.s();
        if s.acct(account_id).is_none() {
            return Ok(None);
        }
        let mut entries: Vec<_> = s
            .ledger
            .iter()
            .filter(|(a, _)| *a == account_id)
            .map(|(_, e)| e.clone())
            .collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.id));
        entries.truncate(limit.max(0) as usize);
        Ok(Some(CreditLedger {
            balance: s.balance(account_id),
            entries,
        }))
    }

    async fn adjust_credits(
        &self,
        account_id: i64,
        delta: i32,
        by: i64,
        audit: &AuditEntry,
    ) -> AdminResult<i64> {
        let now = self.now();
        let mut s = self.s();
        if s.acct(account_id).is_none() {
            return Err(AdminError::NotFound("account"));
        }
        let balance = s.balance(account_id);
        if balance + i64::from(delta) < 0 {
            return Err(AdminError::Conflict(
                "The balance cannot go below zero".into(),
            ));
        }
        let id = s.id();
        s.ledger.push((
            account_id,
            LedgerEntry {
                id,
                delta,
                reason: if delta > 0 {
                    "staff_grant"
                } else {
                    "staff_revoke"
                }
                .into(),
                ref_id: Some(by),
                created_at_ms: now,
            },
        ));
        audit_in(&mut s, now, audit);
        Ok(balance + i64::from(delta))
    }

    async fn list_invites(&self, q: &InviteQuery) -> AdminResult<Vec<InviteRow>> {
        let s = self.s();
        let mut v: Vec<_> = s
            .invites
            .iter()
            .filter(|i| q.issuer_id.is_none_or(|x| i.issuer_account_id == x))
            .filter(|i| q.state.as_ref().is_none_or(|x| &i.state == x))
            .filter(|i| q.after.is_none_or(|x| i.id < x))
            .cloned()
            .collect();
        v.sort_by_key(|i| std::cmp::Reverse(i.id));
        v.truncate(q.limit.max(0) as usize);
        Ok(v)
    }

    async fn revoke_invite(&self, invite_id: i64, audit: &AuditEntry) -> AdminResult<InviteRow> {
        let now = self.now();
        let mut s = self.s();
        let Some(i) = s.invites.iter_mut().find(|i| i.id == invite_id) else {
            return Err(AdminError::NotFound("invite"));
        };
        if i.state != "open" {
            return Err(AdminError::Conflict(
                "Only an open invite can be revoked".into(),
            ));
        }
        i.state = "revoked".into();
        let out = i.clone();
        audit_in(&mut s, now, audit);
        Ok(out)
    }

    async fn settings(&self) -> AdminResult<HashMap<String, StoredSetting>> {
        Ok(self.s().settings.clone())
    }

    async fn put_settings(
        &self,
        values: &[(String, Value)],
        by: i64,
        audit: &AuditEntry,
    ) -> AdminResult<()> {
        let now = self.now();
        let mut s = self.s();
        for (k, v) in values {
            s.settings.insert(
                k.clone(),
                StoredSetting {
                    value: v.clone(),
                    updated_by: Some(by),
                    updated_at_ms: now,
                },
            );
        }
        audit_in(&mut s, now, audit);
        Ok(())
    }

    async fn set_delegation(
        &self,
        account_id: i64,
        max_permissions: u64,
        by: i64,
        reason: &str,
        audit: &AuditEntry,
    ) -> AdminResult<Delegation> {
        let now = self.now();
        let mut s = self.s();
        let Some(m) = s.staff_member(account_id) else {
            return Err(AdminError::NotFound("staff member"));
        };
        s.delegations.retain(|d| d.account_id != account_id);
        let d = Delegation {
            account_id,
            username: m.username,
            max_permissions: max_permissions.to_string(),
            granted_by: by,
            granted_at_ms: now,
            reason: reason.to_owned(),
        };
        s.delegations.push(d.clone());
        audit_in(&mut s, now, audit);
        Ok(d)
    }

    async fn remove_delegation(&self, account_id: i64, audit: &AuditEntry) -> AdminResult<()> {
        let now = self.now();
        let mut s = self.s();
        let before = s.delegations.len();
        s.delegations.retain(|d| d.account_id != account_id);
        if s.delegations.len() == before {
            return Err(AdminError::NotFound("delegation"));
        }
        audit_in(&mut s, now, audit);
        Ok(())
    }

    async fn list_delegations(&self) -> AdminResult<Vec<Delegation>> {
        Ok(self.s().delegations.clone())
    }

    async fn rename_account(
        &self,
        id: i64,
        username: Option<&str>,
        display_name: Option<Option<&str>>,
        audit: &AuditEntry,
    ) -> AdminResult<AccountSummary> {
        let now = self.now();
        let mut s = self.s();
        if let Some(u) = username
            && s.accounts
                .iter()
                .any(|a| a.s.id != id && a.s.username.eq_ignore_ascii_case(u))
        {
            return Err(AdminError::Conflict("That username is taken".into()));
        }
        let a = s
            .accounts
            .iter_mut()
            .find(|a| a.s.id == id)
            .ok_or(AdminError::NotFound("account"))?;
        if let Some(u) = username {
            a.s.username = u.to_owned();
        }
        if let Some(d) = display_name {
            a.s.display_name = d.map(str::to_owned);
        }
        let out = a.s.clone();
        audit_in(&mut s, now, audit);
        Ok(out)
    }

    async fn set_account_email(
        &self,
        id: i64,
        email: &str,
        audit: &AuditEntry,
    ) -> AdminResult<AccountSummary> {
        let now = self.now();
        let mut s = self.s();
        if s.accounts
            .iter()
            .any(|a| a.s.id != id && a.s.email.eq_ignore_ascii_case(email))
        {
            return Err(AdminError::Conflict(
                "That email address is used by another account".into(),
            ));
        }
        let a = s
            .accounts
            .iter_mut()
            .find(|a| a.s.id == id)
            .ok_or(AdminError::NotFound("account"))?;
        a.s.email = email.to_owned();
        a.s.email_verified = false;
        let out = a.s.clone();
        audit_in(&mut s, now, audit);
        Ok(out)
    }

    async fn update_account_flags(
        &self,
        id: i64,
        set: i64,
        clear: i64,
        audit: &AuditEntry,
    ) -> AdminResult<AccountSummary> {
        let now = self.now();
        let mut s = self.s();
        let a = s
            .accounts
            .iter_mut()
            .find(|a| a.s.id == id)
            .ok_or(AdminError::NotFound("account"))?;
        a.s.flags = (a.s.flags | set) & !clear;
        let out = a.s.clone();
        audit_in(&mut s, now, audit);
        Ok(out)
    }

    async fn force_password_reset(&self, id: i64, audit: &AuditEntry) -> AdminResult<u64> {
        let now = self.now();
        let mut s = self.s();
        let a = s
            .accounts
            .iter_mut()
            .find(|a| a.s.id == id)
            .ok_or(AdminError::NotFound("account"))?;
        let n = a.sessions;
        a.sessions = 0;
        a.password_reset_forced = true;
        audit_in(&mut s, now, audit);
        Ok(n)
    }

    async fn counts(&self) -> AdminResult<SystemCounts> {
        let s = self.s();
        let n = |st: AccountState| s.accounts.iter().filter(|a| a.s.state == st).count() as i64;
        let j = |st: &str| s.jobs.iter().filter(|j| j.state == st).count() as i64;
        Ok(SystemCounts {
            accounts_active: n(AccountState::Active),
            accounts_deactivated: n(AccountState::Deactivated),
            accounts_banned: n(AccountState::PlatformBanned),
            accounts_deleted: n(AccountState::Deleted),
            sessions_active: s.accounts.iter().map(|a| a.sessions as i64).sum(),
            jobs_queued: j("queued"),
            jobs_running: j("running"),
            jobs_dead: j("dead"),
            staff: s
                .accounts
                .iter()
                .filter(|a| s.staff_member(a.s.id).is_some())
                .count() as i64,
            alerts_unacked: s.alerts.iter().filter(|a| a.acked_at_ms.is_none()).count() as i64,
            ..SystemCounts::default()
        })
    }

    async fn list_jobs(
        &self,
        state: Option<&str>,
        after: Option<i64>,
        limit: i64,
    ) -> AdminResult<Vec<JobRow>> {
        let s = self.s();
        let mut v: Vec<_> = s
            .jobs
            .iter()
            .filter(|j| state.is_none_or(|x| j.state == x))
            .filter(|j| after.is_none_or(|x| j.id < x))
            .cloned()
            .collect();
        v.sort_by_key(|j| std::cmp::Reverse(j.id));
        v.truncate(limit.max(0) as usize);
        Ok(v)
    }

    async fn retry_job(&self, id: i64, audit: &AuditEntry) -> AdminResult<()> {
        let now = self.now();
        let mut s = self.s();
        let Some(j) = s.jobs.iter_mut().find(|j| j.id == id && j.state == "dead") else {
            return Err(AdminError::NotFound("dead job"));
        };
        j.state = "queued".into();
        j.attempts = 0;
        audit_in(&mut s, now, audit);
        Ok(())
    }

    async fn delete_job(&self, id: i64, audit: &AuditEntry) -> AdminResult<()> {
        let now = self.now();
        let mut s = self.s();
        let before = s.jobs.len();
        s.jobs.retain(|j| !(j.id == id && j.state == "dead"));
        if s.jobs.len() == before {
            return Err(AdminError::NotFound("dead job"));
        }
        s.deleted_jobs.push(id);
        audit_in(&mut s, now, audit);
        Ok(())
    }

    async fn append_log(&self, entry: &AuditEntry) -> AdminResult<AccessLogEntry> {
        let now = self.now();
        Ok(self.s().append(now, entry))
    }

    async fn list_log(&self, f: &AccessLogFilter) -> AdminResult<Vec<AccessLogEntry>> {
        let s = self.s();
        let mut v: Vec<_> = s
            .log
            .iter()
            .filter(|r| f.actor_id.is_none_or(|x| r.actor_id == x))
            .filter(|r| f.target_id.is_none_or(|x| r.target_id == Some(x)))
            .filter(|r| f.action.as_ref().is_none_or(|x| &r.action == x))
            .filter(|r| f.kind.is_none_or(|x| r.kind == x))
            .filter(|r| f.before.is_none_or(|x| r.seq < x))
            .cloned()
            .collect();
        v.sort_by_key(|r| std::cmp::Reverse(r.seq));
        v.truncate(f.limit.max(0) as usize);
        Ok(v)
    }

    async fn verify_log(&self) -> AdminResult<ChainReport> {
        Ok(chain::verify_rows(&self.s().log))
    }
}

#[async_trait]
impl StaffExtrasRepository for MemoryAdminRepo {
    async fn create_offer(&self, o: &NewOffer, audit: &AuditEntry) -> AdminResult<StaffOffer> {
        let now = self.now();
        let mut s = self.s();
        if s.offers
            .iter()
            .any(|x| x.account_id == o.account_id && x.state == OfferState::Pending)
        {
            return Err(AdminError::Conflict(
                "The account already has a pending offer".into(),
            ));
        }
        let username = s
            .acct(o.account_id)
            .map(|a| a.s.username.clone())
            .ok_or(AdminError::NotFound("account"))?;
        let by_name = s
            .acct(o.offered_by)
            .map(|a| a.s.username.clone())
            .unwrap_or_default();
        let role_names = o
            .role_ids
            .iter()
            .filter_map(|id| s.roles.iter().find(|r| r.id == *id).map(|r| r.name.clone()))
            .collect();
        let offer = StaffOffer {
            id: s.id(),
            account_id: o.account_id,
            username,
            offered_by: o.offered_by,
            offered_by_name: by_name,
            role_ids: o.role_ids.iter().map(i64::to_string).collect(),
            role_names,
            reason: o.reason.clone(),
            state: OfferState::Pending,
            created_at_ms: now,
            expires_at_ms: o.expires_at_ms,
            responded_at_ms: None,
            dm_channel_id: None,
            dm_message_id: None,
        };
        s.offers.push(offer.clone());
        audit_in(&mut s, now, audit);
        Ok(offer)
    }

    async fn get_offer(&self, id: i64) -> AdminResult<Option<StaffOffer>> {
        Ok(self.s().offers.iter().find(|o| o.id == id).cloned())
    }

    async fn list_offers(&self, pending_only: bool, limit: i64) -> AdminResult<Vec<StaffOffer>> {
        let s = self.s();
        let mut v: Vec<_> = s
            .offers
            .iter()
            .filter(|o| !pending_only || o.state == OfferState::Pending)
            .cloned()
            .collect();
        v.sort_by_key(|o| std::cmp::Reverse(o.id));
        v.truncate(limit.max(0) as usize);
        Ok(v)
    }

    async fn close_offer(
        &self,
        id: i64,
        to: OfferState,
        audit: &AuditEntry,
    ) -> AdminResult<StaffOffer> {
        let now = self.now();
        let mut s = self.s();
        let o = s
            .offers
            .iter_mut()
            .find(|o| o.id == id)
            .ok_or(AdminError::NotFound("appointment offer"))?;
        if o.state != OfferState::Pending {
            return Err(AdminError::Conflict(format!(
                "This offer was already {}",
                o.state.as_str()
            )));
        }
        o.state = to;
        o.responded_at_ms = Some(now);
        let out = o.clone();
        audit_in(&mut s, now, audit);
        Ok(out)
    }

    async fn set_offer_message(
        &self,
        id: i64,
        channel_id: i64,
        message_id: i64,
    ) -> AdminResult<()> {
        let mut s = self.s();
        let o = s
            .offers
            .iter_mut()
            .find(|o| o.id == id)
            .ok_or(AdminError::NotFound("appointment offer"))?;
        o.dm_channel_id = Some(channel_id);
        o.dm_message_id = Some(message_id);
        Ok(())
    }

    async fn expire_offers(&self, now_ms: i64) -> AdminResult<Vec<StaffOffer>> {
        let mut s = self.s();
        let mut out = Vec::new();
        for o in s
            .offers
            .iter_mut()
            .filter(|o| o.state == OfferState::Pending && o.expires_at_ms <= now_ms)
        {
            o.state = OfferState::Expired;
            o.responded_at_ms = Some(now_ms);
            out.push(o.clone());
        }
        Ok(out)
    }

    async fn create_announcement(
        &self,
        a: &NewAnnouncement,
        audit: &AuditEntry,
    ) -> AdminResult<Announcement> {
        let now = self.now();
        let mut s = self.s();
        let row = Announcement {
            id: s.id(),
            title: a.title.clone(),
            body: a.body.clone(),
            severity: a.severity,
            send_dm: a.send_dm,
            created_by: a.created_by,
            created_at_ms: now,
            starts_at_ms: a.starts_at_ms,
            ends_at_ms: a.ends_at_ms,
            ended_at_ms: None,
        };
        s.announcements.push(row.clone());
        audit_in(&mut s, now, audit);
        Ok(row)
    }

    async fn end_announcement(&self, id: i64, audit: &AuditEntry) -> AdminResult<Announcement> {
        let now = self.now();
        let mut s = self.s();
        let a = s
            .announcements
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or(AdminError::NotFound("announcement"))?;
        if a.ended_at_ms.is_none() {
            a.ended_at_ms = Some(now);
        }
        let out = a.clone();
        audit_in(&mut s, now, audit);
        Ok(out)
    }

    async fn account_sessions(&self, _account_id: i64) -> AdminResult<Vec<AdminSession>> {
        Ok(Vec::new())
    }

    async fn account_guilds(&self, _account_id: i64) -> AdminResult<Vec<AdminGuildRef>> {
        Ok(Vec::new())
    }

    async fn set_invites_frozen(
        &self,
        account_id: i64,
        frozen: bool,
        audit: &AuditEntry,
    ) -> AdminResult<()> {
        let now = self.now();
        let mut s = self.s();
        let a = s
            .accounts
            .iter_mut()
            .find(|a| a.s.id == account_id)
            .ok_or(AdminError::NotFound("account"))?;
        a.invites_frozen = frozen;
        audit_in(&mut s, now, audit);
        Ok(())
    }

    async fn set_person_notes(
        &self,
        person_id: i64,
        notes: &str,
        audit: &AuditEntry,
    ) -> AdminResult<()> {
        let now = self.now();
        let mut s = self.s();
        s.persons
            .get_mut(&person_id)
            .ok_or(AdminError::NotFound("person"))?
            .0 = notes.to_owned();
        audit_in(&mut s, now, audit);
        Ok(())
    }

    async fn link_account(
        &self,
        person_id: i64,
        account_id: i64,
        audit: &AuditEntry,
    ) -> AdminResult<i64> {
        let now = self.now();
        let mut s = self.s();
        if !s.persons.contains_key(&person_id) {
            return Err(AdminError::NotFound("person"));
        }
        let old = s
            .acct(account_id)
            .ok_or(AdminError::NotFound("account"))?
            .s
            .person_id;
        if old == person_id {
            return Err(AdminError::Conflict(
                "the account already belongs to this Person".into(),
            ));
        }
        let banned = |s: &State, p: i64| {
            s.bans
                .iter()
                .any(|b| b.person_id == p && b.lifted_at_ms.is_none())
        };
        if banned(&s, old) || banned(&s, person_id) {
            return Err(AdminError::Conflict("lift the ban first".into()));
        }
        if let Some(a) = s.accounts.iter_mut().find(|a| a.s.id == account_id) {
            a.s.person_id = person_id;
        }
        if !s.accounts.iter().any(|a| a.s.person_id == old) {
            let (old_notes, old_level) = s.persons.remove(&old).unwrap_or_default();
            if let Some(t) = s.persons.get_mut(&person_id) {
                t.1 = t.1.max(old_level);
                if !old_notes.is_empty() {
                    if !t.0.is_empty() {
                        t.0.push_str(
                            "

",
                        );
                    }
                    t.0.push_str(&old_notes);
                }
            }
        }
        audit_in(&mut s, now, audit);
        Ok(old)
    }

    async fn unlink_account(&self, account_id: i64, audit: &AuditEntry) -> AdminResult<i64> {
        let now = self.now();
        let mut s = self.s();
        let old = s
            .acct(account_id)
            .ok_or(AdminError::NotFound("account"))?
            .s
            .person_id;
        if s.accounts.iter().filter(|a| a.s.person_id == old).count() < 2 {
            return Err(AdminError::Conflict(
                "this is the only account of its Person".into(),
            ));
        }
        if s.bans
            .iter()
            .any(|b| b.person_id == old && b.lifted_at_ms.is_none())
        {
            return Err(AdminError::Conflict("lift the ban first".into()));
        }
        let new_person_id = s.id();
        s.persons.insert(new_person_id, (String::new(), 0));
        if let Some(a) = s.accounts.iter_mut().find(|a| a.s.id == account_id) {
            a.s.person_id = new_person_id;
        }
        audit_in(&mut s, now, audit);
        Ok(new_person_id)
    }

    async fn suspicion_log(&self, person_id: i64, limit: i64) -> AdminResult<Vec<SuspicionEvent>> {
        let s = self.s();
        Ok(s.suspicion_events
            .iter()
            .rev()
            .filter(|(p, _)| *p == person_id)
            .take(usize::try_from(limit).unwrap_or(0))
            .map(|(_, e)| e.clone())
            .collect())
    }

    async fn search_guilds(
        &self,
        q: Option<&str>,
        after: Option<i64>,
        limit: i64,
    ) -> AdminResult<Vec<AdminGuild>> {
        let s = self.s();
        let needle = q.map(str::to_lowercase);
        let mut v: Vec<AdminGuild> = s
            .guilds
            .iter()
            .filter(|g| after.is_none_or(|a| g.id > a))
            .filter(|g| {
                needle.as_ref().is_none_or(|n| {
                    g.name.to_lowercase().contains(n.as_str()) || g.id.to_string() == *n
                })
            })
            .cloned()
            .collect();
        v.sort_by_key(|g| g.id);
        v.truncate(usize::try_from(limit).unwrap_or(0));
        Ok(v)
    }

    async fn get_guild(&self, id: i64) -> AdminResult<Option<AdminGuild>> {
        Ok(self.s().guilds.iter().find(|g| g.id == id).cloned())
    }

    async fn record_guild_action(&self, audit: &AuditEntry) -> AdminResult<()> {
        let now = self.now();
        audit_in(&mut self.s(), now, audit);
        Ok(())
    }

    async fn entrance_copy(&self) -> AdminResult<std::collections::BTreeMap<String, String>> {
        Ok(self.s().entrance_copy.clone())
    }

    async fn set_entrance_copy(
        &self,
        key: &str,
        markdown: Option<&str>,
        _by: i64,
        audit: &AuditEntry,
    ) -> AdminResult<()> {
        let now = self.now();
        let mut s = self.s();
        match markdown {
            Some(m) => {
                s.entrance_copy.insert(key.to_owned(), m.to_owned());
            }
            None => {
                s.entrance_copy.remove(key);
            }
        }
        audit_in(&mut s, now, audit);
        Ok(())
    }

    async fn get_announcement(&self, id: i64) -> AdminResult<Announcement> {
        self.s()
            .announcements
            .iter()
            .find(|a| a.id == id)
            .cloned()
            .ok_or(AdminError::NotFound("announcement"))
    }

    async fn edit_announcement(
        &self,
        id: i64,
        patch: &crate::ext::AnnouncementPatch,
        now_ms: i64,
        audit: &AuditEntry,
    ) -> AdminResult<Announcement> {
        let now = self.now();
        let mut s = self.s();
        let a = s
            .announcements
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or(AdminError::NotFound("announcement"))?;
        if a.ended_at_ms.is_some() || a.ends_at_ms.is_some_and(|e| e <= now_ms) {
            return Err(AdminError::Conflict("this announcement has ended".into()));
        }
        if let Some(t) = &patch.title {
            a.title.clone_from(t);
        }
        if let Some(b) = &patch.body {
            a.body.clone_from(b);
        }
        if let Some(e) = patch.ends_at_ms {
            a.ends_at_ms = Some(e);
        }
        let out = a.clone();
        audit_in(&mut s, now, audit);
        Ok(out)
    }

    async fn list_announcements(&self, limit: i64) -> AdminResult<Vec<Announcement>> {
        let mut v = self.s().announcements.clone();
        v.sort_by_key(|a| std::cmp::Reverse(a.id));
        v.truncate(limit.max(0) as usize);
        Ok(v)
    }

    async fn active_announcements(&self, now_ms: i64) -> AdminResult<Vec<Announcement>> {
        let mut v: Vec<_> = self
            .s()
            .announcements
            .iter()
            .filter(|a| a.is_active(now_ms))
            .cloned()
            .collect();
        v.sort_by_key(|a| std::cmp::Reverse(a.id));
        Ok(v)
    }

    async fn record_alert(&self, kind: &str, detail: &str) -> AdminResult<StaffAlertRow> {
        let now = self.now();
        let mut s = self.s();
        let row = StaffAlertRow {
            id: s.id(),
            kind: kind.to_owned(),
            detail: detail.to_owned(),
            created_at_ms: now,
            acked_by: None,
            acked_at_ms: None,
        };
        s.alerts.push(row.clone());
        Ok(row)
    }

    async fn recent_alerts(&self, limit: i64) -> AdminResult<Vec<StaffAlertRow>> {
        let mut v = self.s().alerts.clone();
        v.sort_by_key(|a| std::cmp::Reverse(a.id));
        v.truncate(limit.max(0) as usize);
        Ok(v)
    }

    async fn ack_alert(&self, id: i64, by: i64, audit: &AuditEntry) -> AdminResult<StaffAlertRow> {
        let now = self.now();
        let mut s = self.s();
        let a = s
            .alerts
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or(AdminError::NotFound("alert"))?;
        if a.acked_at_ms.is_none() {
            a.acked_by = Some(by);
            a.acked_at_ms = Some(now);
        }
        let out = a.clone();
        audit_in(&mut s, now, audit);
        Ok(out)
    }

    async fn private_channel(&self, channel_id: i64) -> AdminResult<Option<PrivateChannel>> {
        Ok(self
            .s()
            .private_channels
            .iter()
            .find(|c| c.id == channel_id)
            .cloned())
    }

    async fn dm_channels_of(&self, account_id: i64) -> AdminResult<Vec<PrivateChannel>> {
        Ok(self
            .s()
            .private_channels
            .iter()
            .filter(|c| c.recipients.iter().any(|r| r.id == account_id))
            .cloned()
            .collect())
    }

    async fn private_channels_of_guild(&self, guild_id: i64) -> AdminResult<Vec<PrivateChannel>> {
        Ok(self
            .s()
            .private_channels
            .iter()
            .filter(|c| c.guild_id == Some(guild_id))
            .cloned()
            .collect())
    }

    async fn private_messages(
        &self,
        channel_id: i64,
        before: Option<i64>,
        limit: i64,
        deleted_days: i64,
    ) -> AdminResult<Vec<PrivateMessage>> {
        let cutoff = self.now() - deleted_days * 24 * 3600 * 1000;
        let mut v: Vec<_> = self
            .s()
            .private_messages
            .iter()
            .filter(|(c, _)| *c == channel_id)
            .map(|(_, m)| m.clone())
            .filter(|m| before.is_none_or(|b| m.id < b))
            .filter(|m| m.deleted_at_ms.is_none_or(|d| d >= cutoff))
            .collect();
        v.sort_by_key(|m| std::cmp::Reverse(m.id));
        v.truncate(limit.max(0) as usize);
        Ok(v)
    }
}

/// An edit for tests.
pub fn edit(at_ms: i64, previous: &str) -> PrivateEdit {
    PrivateEdit {
        edited_at_ms: at_ms,
        previous_content: previous.to_owned(),
    }
}
