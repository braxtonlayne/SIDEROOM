//! In-memory [`AuthStore`] for tests and local bring-up. One mutex guards all
//! state, so [`AuthStore::register`] is trivially atomic.

use crate::auth::memory::MemorySessions;
use async_trait::async_trait;
use sr_auth::{
    email_token::EmailTokenPurpose,
    invite::InviteCode,
    repo::{SessionRecord, SessionRepository, TotpRecord},
    session::TokenHash,
    store::{
        AccountRecord, AccountState, AuthStore, DeleteRefusal, DeletedAccount, InviteState,
        PasskeyRecord, RegisterRejection, RegisterTx, Registered, StoreResult, TrustedDeviceRecord,
    },
};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, MutexGuard},
};

struct MemInvite {
    issuer_id: i64,
    state: InviteState,
}

struct MemToken {
    user_id: i64,
    purpose: EmailTokenPurpose,
    hash: TokenHash,
    expires_at: i64,
    consumed: bool,
    /// Pending address of a `ChangeEmail` token.
    email: Option<String>,
}

struct MemRecovery {
    id: i64,
    hash: TokenHash,
    used: bool,
}

struct MemTrusted {
    id: i64,
    user_id: i64,
    hash: TokenHash,
    name: Option<String>,
    created_at: i64,
    expires_at: i64,
    revoked: bool,
}

#[derive(Default)]
struct Inner {
    accounts: HashMap<i64, AccountRecord>,
    invites: HashMap<String, MemInvite>,
    /// `(account, delta, reason)`.
    ledger: Vec<(i64, i32, &'static str)>,
    totp: HashMap<i64, TotpRecord>,
    recovery: HashMap<i64, Vec<MemRecovery>>,
    tokens: Vec<MemToken>,
    /// Session id to the time of its last second-factor check.
    mfa_verified: HashMap<i64, i64>,
    trusted: Vec<MemTrusted>,
    /// `(actor, action, target, note)`.
    audit: Vec<(i64, String, Option<i64>, String)>,
    /// Discord user id to account id (external identities).
    discord: HashMap<String, i64>,
    banned_emails: HashSet<String>,
    passkeys: Vec<PasskeyRecord>,
    user_handles: HashMap<i64, Vec<u8>>,
    next_row_id: i64,
}

impl Inner {
    fn next_id(&mut self) -> i64 {
        self.next_row_id += 1;
        self.next_row_id
    }
}

/// In-memory [`AuthStore`]. Sessions are delegated to a shared
/// [`MemorySessions`] so the `Principal` extractor (which reads
/// `AppState::sessions`) sees sessions created by login.
pub struct MemoryAuthStore {
    inner: Mutex<Inner>,
    sessions: Arc<MemorySessions>,
}

impl MemoryAuthStore {
    /// New empty store sharing `sessions` with the `AppState`.
    pub fn new(sessions: Arc<MemorySessions>) -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            sessions,
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Insert an account directly (seed/test helper). Grants no credits.
    pub fn add_account(&self, a: AccountRecord) {
        self.lock().accounts.insert(a.id, a);
    }

    /// Insert an open invite issued by `issuer_id`.
    pub fn add_invite(&self, code: &InviteCode, issuer_id: i64) {
        self.lock().invites.insert(
            code.canonical().to_owned(),
            MemInvite {
                issuer_id,
                state: InviteState::Open,
            },
        );
    }

    /// Mark an invite revoked.
    pub fn revoke_invite(&self, code: &InviteCode) {
        if let Some(i) = self.lock().invites.get_mut(code.canonical()) {
            i.state = InviteState::Revoked;
        }
    }

    /// Add a ledger entry.
    pub fn grant(&self, account: i64, delta: i32, reason: &'static str) {
        self.lock().ledger.push((account, delta, reason));
    }

    /// Credit balance of an account.
    pub fn balance(&self, account: i64) -> i32 {
        self.lock()
            .ledger
            .iter()
            .filter(|(a, ..)| *a == account)
            .map(|(_, d, _)| d)
            .sum()
    }

    /// Ledger entries for an account as `(delta, reason)`.
    pub fn ledger_for(&self, account: i64) -> Vec<(i32, &'static str)> {
        self.lock()
            .ledger
            .iter()
            .filter(|(a, ..)| *a == account)
            .map(|(_, d, r)| (*d, *r))
            .collect()
    }

    /// Treat registrations with this email as belonging to a banned Person.
    pub fn ban_email(&self, email: &str) {
        self.lock().banned_emails.insert(email.to_owned());
    }

    /// Fetch an account (test helper).
    pub fn account(&self, id: i64) -> Option<AccountRecord> {
        self.lock().accounts.get(&id).cloned()
    }

    /// Change an account's state.
    pub fn set_state(&self, id: i64, state: AccountState) {
        if let Some(a) = self.lock().accounts.get_mut(&id) {
            a.state = state;
        }
    }

    /// Platform audit rows as `(actor, action, target)` (test helper).
    pub fn audit_rows(&self) -> Vec<(i64, String, Option<i64>)> {
        self.lock()
            .audit
            .iter()
            .map(|(a, act, t, _)| (*a, act.clone(), *t))
            .collect()
    }
}

fn infallible<T>(r: Result<T, std::convert::Infallible>) -> StoreResult<T> {
    match r {
        Ok(v) => Ok(v),
        Err(e) => match e {},
    }
}

#[async_trait]
impl AuthStore for MemoryAuthStore {
    async fn invite_state(&self, code: &InviteCode) -> StoreResult<Option<InviteState>> {
        Ok(self.lock().invites.get(code.canonical()).map(|i| i.state))
    }

    async fn invite_credit_balance(&self, account: i64) -> StoreResult<Option<i64>> {
        Ok(Some(
            self.lock()
                .ledger
                .iter()
                .filter(|(a, ..)| *a == account)
                .map(|(_, d, _)| i64::from(*d))
                .sum(),
        ))
    }

    async fn counted_accounts(&self) -> StoreResult<u64> {
        Ok(self
            .lock()
            .accounts
            .values()
            .filter(|a| a.state == AccountState::Active)
            .count() as u64)
    }

    async fn register(&self, tx: RegisterTx) -> StoreResult<Result<Registered, RegisterRejection>> {
        use RegisterRejection::*;
        let mut g = self.lock();
        let Some(inv) = g.invites.get(tx.code.canonical()) else {
            return Ok(Err(InviteUnavailable));
        };
        if inv.state != InviteState::Open {
            return Ok(Err(InviteUnavailable));
        }
        let issuer_id = inv.issuer_id;
        let active = g
            .accounts
            .values()
            .filter(|a| a.state == AccountState::Active)
            .count() as u64;
        if active >= tx.account_cap {
            return Ok(Err(AtCap));
        }
        let balance: i32 = g
            .ledger
            .iter()
            .filter(|(a, ..)| *a == issuer_id)
            .map(|(_, d, _)| d)
            .sum();
        if balance < 1 {
            return Ok(Err(IssuerOutOfCredits));
        }
        if g.banned_emails.contains(&tx.email) {
            return Ok(Err(PersonBanned));
        }
        if g.accounts.values().any(|a| a.username == tx.username) {
            return Ok(Err(UsernameTaken));
        }
        if g.accounts.values().any(|a| a.email == tx.email) {
            return Ok(Err(EmailTaken));
        }
        if let Some(d) = &tx.discord_id {
            g.discord.entry(d.clone()).or_insert(tx.account_id);
        }
        g.accounts.insert(
            tx.account_id,
            AccountRecord {
                id: tx.account_id,
                person_id: tx.new_person_id,
                username: tx.username,
                display_name: tx.display_name,
                email: tx.email,
                email_verified: false,
                password_hash: tx.password_hash,
                state: AccountState::Active,
                age_confirmed: None,
                locale: "en-US".into(),
                flags: 0,
            },
        );
        if let Some(i) = g.invites.get_mut(tx.code.canonical()) {
            i.state = InviteState::Redeemed;
        }
        g.ledger.push((issuer_id, -1, "redemption"));
        g.ledger
            .push((tx.account_id, tx.initial_credits, "initial_grant"));
        Ok(Ok(Registered {
            account_id: tx.account_id,
            issuer_id,
        }))
    }

    async fn username_exists(&self, username: &str) -> StoreResult<bool> {
        Ok(self
            .lock()
            .accounts
            .values()
            .any(|a| a.username == username))
    }

    async fn find_by_login(&self, login: &str) -> StoreResult<Option<AccountRecord>> {
        Ok(self
            .lock()
            .accounts
            .values()
            .find(|a| a.username == login || a.email == login)
            .cloned())
    }

    async fn find_by_email(&self, email: &str) -> StoreResult<Option<AccountRecord>> {
        Ok(self
            .lock()
            .accounts
            .values()
            .find(|a| a.email == email)
            .cloned())
    }

    async fn find_by_id(&self, id: i64) -> StoreResult<Option<AccountRecord>> {
        Ok(self.lock().accounts.get(&id).cloned())
    }

    async fn find_by_discord(&self, provider_user_id: &str) -> StoreResult<Option<AccountRecord>> {
        let g = self.lock();
        Ok(g.discord
            .get(provider_user_id)
            .and_then(|id| g.accounts.get(id))
            .cloned())
    }

    async fn set_password_hash(&self, id: i64, hash: &str) -> StoreResult<()> {
        if let Some(a) = self.lock().accounts.get_mut(&id) {
            hash.clone_into(&mut a.password_hash);
        }
        Ok(())
    }

    async fn mark_email_verified(&self, id: i64) -> StoreResult<()> {
        if let Some(a) = self.lock().accounts.get_mut(&id) {
            a.email_verified = true;
        }
        Ok(())
    }

    async fn set_age_confirmed(&self, id: i64, confirmed: bool, _now: i64) -> StoreResult<()> {
        if let Some(a) = self.lock().accounts.get_mut(&id) {
            a.age_confirmed = Some(confirmed);
        }
        Ok(())
    }

    async fn set_locale(&self, id: i64, locale: &str) -> StoreResult<()> {
        if let Some(a) = self.lock().accounts.get_mut(&id) {
            a.locale = locale.to_owned();
        }
        Ok(())
    }

    async fn set_display_name(&self, id: i64, display_name: Option<&str>) -> StoreResult<()> {
        if let Some(a) = self.lock().accounts.get_mut(&id) {
            a.display_name = display_name.map(str::to_owned);
        }
        Ok(())
    }

    async fn set_username(&self, id: i64, username: &str) -> StoreResult<bool> {
        let mut g = self.lock();
        if g.accounts
            .values()
            .any(|a| a.id != id && a.username == username)
        {
            return Ok(false);
        }
        if let Some(a) = g.accounts.get_mut(&id) {
            username.clone_into(&mut a.username);
        }
        Ok(true)
    }

    async fn email_change_put(
        &self,
        user_id: i64,
        new_email: &str,
        hash: &TokenHash,
        expires_at: i64,
    ) -> StoreResult<()> {
        let purpose = EmailTokenPurpose::ChangeEmail;
        let mut g = self.lock();
        g.tokens
            .retain(|t| !(t.user_id == user_id && t.purpose == purpose && !t.consumed));
        g.tokens.push(MemToken {
            user_id,
            purpose,
            hash: *hash,
            expires_at,
            consumed: false,
            email: Some(new_email.to_owned()),
        });
        Ok(())
    }

    async fn email_change_consume(
        &self,
        hash: &TokenHash,
        now: i64,
    ) -> StoreResult<Option<(i64, String)>> {
        let mut g = self.lock();
        let Some(t) = g.tokens.iter_mut().find(|t| {
            t.purpose == EmailTokenPurpose::ChangeEmail
                && t.hash.ct_eq(hash)
                && !t.consumed
                && t.expires_at > now
        }) else {
            return Ok(None);
        };
        t.consumed = true;
        Ok(t.email.clone().map(|e| (t.user_id, e)))
    }

    async fn email_change_cancel(&self, user_id: i64) -> StoreResult<bool> {
        let mut g = self.lock();
        let before = g.tokens.len();
        g.tokens.retain(|t| {
            !(t.user_id == user_id && t.purpose == EmailTokenPurpose::ChangeEmail && !t.consumed)
        });
        Ok(g.tokens.len() != before)
    }

    async fn session_mfa_verified_at(&self, session_id: i64) -> StoreResult<Option<i64>> {
        Ok(self.lock().mfa_verified.get(&session_id).copied())
    }

    async fn session_mark_mfa_verified(&self, session_id: i64, at: i64) -> StoreResult<()> {
        self.lock().mfa_verified.insert(session_id, at);
        Ok(())
    }

    async fn set_email(&self, id: i64, email: &str) -> StoreResult<bool> {
        let mut g = self.lock();
        if g.accounts.values().any(|a| a.id != id && a.email == email) {
            return Ok(false);
        }
        if let Some(a) = g.accounts.get_mut(&id) {
            email.clone_into(&mut a.email);
            a.email_verified = true;
        }
        Ok(true)
    }
    async fn create_session(&self, rec: SessionRecord) -> StoreResult<()> {
        infallible(self.sessions.create(rec).await)
    }

    async fn list_sessions(&self, user_id: i64, now: i64) -> StoreResult<Vec<SessionRecord>> {
        infallible(self.sessions.list_for_user(user_id, now).await)
    }

    async fn revoke_session(&self, user_id: i64, session_id: i64) -> StoreResult<bool> {
        infallible(self.sessions.revoke(user_id, session_id).await)
    }

    async fn revoke_sessions_except(&self, user_id: i64, keep: Option<i64>) -> StoreResult<u64> {
        infallible(self.sessions.revoke_all_except(user_id, keep).await)
    }

    async fn trusted_device_put(
        &self,
        user_id: i64,
        hash: &TokenHash,
        device_name: Option<&str>,
        expires_at: i64,
    ) -> StoreResult<()> {
        let mut g = self.lock();
        let id = g.next_id();
        g.trusted.push(MemTrusted {
            id,
            user_id,
            hash: *hash,
            name: device_name.map(str::to_owned),
            created_at: 0,
            expires_at,
            revoked: false,
        });
        Ok(())
    }

    async fn trusted_device_matches(
        &self,
        user_id: i64,
        hash: &TokenHash,
        now: i64,
    ) -> StoreResult<bool> {
        Ok(self.lock().trusted.iter().any(|t| {
            t.user_id == user_id && !t.revoked && t.expires_at > now && t.hash.ct_eq(hash)
        }))
    }

    async fn trusted_devices_list(
        &self,
        user_id: i64,
        now: i64,
    ) -> StoreResult<Vec<TrustedDeviceRecord>> {
        Ok(self
            .lock()
            .trusted
            .iter()
            .filter(|t| t.user_id == user_id && !t.revoked && t.expires_at > now)
            .map(|t| TrustedDeviceRecord {
                id: t.id,
                device_name: t.name.clone(),
                created_at: t.created_at,
                expires_at: t.expires_at,
            })
            .collect())
    }

    async fn trusted_device_revoke(&self, user_id: i64, id: i64) -> StoreResult<bool> {
        let mut g = self.lock();
        match g
            .trusted
            .iter_mut()
            .find(|t| t.id == id && t.user_id == user_id && !t.revoked)
        {
            Some(t) => {
                t.revoked = true;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    async fn trusted_devices_revoke_all(&self, user_id: i64) -> StoreResult<u64> {
        let mut n = 0;
        for t in self
            .lock()
            .trusted
            .iter_mut()
            .filter(|t| t.user_id == user_id && !t.revoked)
        {
            t.revoked = true;
            n += 1;
        }
        Ok(n)
    }

    async fn totp_get(&self, user_id: i64) -> StoreResult<Option<TotpRecord>> {
        Ok(self.lock().totp.get(&user_id).cloned())
    }

    async fn totp_put_pending(&self, user_id: i64, secret: &[u8]) -> StoreResult<()> {
        self.lock().totp.insert(
            user_id,
            TotpRecord {
                user_id,
                secret: secret.to_vec(),
                confirmed: false,
                last_used_step: None,
            },
        );
        Ok(())
    }

    async fn totp_confirm(&self, user_id: i64, step: u64) -> StoreResult<()> {
        if let Some(t) = self.lock().totp.get_mut(&user_id) {
            t.confirmed = true;
            t.last_used_step = Some(step);
        }
        Ok(())
    }

    async fn totp_consume_step(&self, user_id: i64, step: u64) -> StoreResult<bool> {
        let mut g = self.lock();
        let Some(t) = g.totp.get_mut(&user_id) else {
            return Ok(false);
        };
        if sr_auth::totp::step_is_fresh(step, t.last_used_step) {
            t.last_used_step = Some(step);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn totp_delete(&self, user_id: i64) -> StoreResult<()> {
        self.lock().totp.remove(&user_id);
        Ok(())
    }

    async fn recovery_replace_all(&self, user_id: i64, hashes: &[TokenHash]) -> StoreResult<()> {
        let mut g = self.lock();
        let mut rows = Vec::with_capacity(hashes.len());
        for h in hashes {
            rows.push(MemRecovery {
                id: g.next_id(),
                hash: *h,
                used: false,
            });
        }
        g.recovery.insert(user_id, rows);
        Ok(())
    }

    async fn recovery_list_unused(&self, user_id: i64) -> StoreResult<Vec<(i64, TokenHash)>> {
        Ok(self
            .lock()
            .recovery
            .get(&user_id)
            .map(|v| {
                v.iter()
                    .filter(|r| !r.used)
                    .map(|r| (r.id, r.hash))
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn recovery_consume(&self, user_id: i64, row_id: i64, _now: i64) -> StoreResult<bool> {
        let mut g = self.lock();
        let Some(r) = g
            .recovery
            .get_mut(&user_id)
            .and_then(|v| v.iter_mut().find(|r| r.id == row_id))
        else {
            return Ok(false);
        };
        if r.used {
            return Ok(false);
        }
        r.used = true;
        Ok(true)
    }

    async fn email_token_put(
        &self,
        user_id: i64,
        purpose: EmailTokenPurpose,
        hash: &TokenHash,
        expires_at: i64,
    ) -> StoreResult<()> {
        let mut g = self.lock();
        g.tokens
            .retain(|t| !(t.user_id == user_id && t.purpose == purpose && !t.consumed));
        g.tokens.push(MemToken {
            user_id,
            purpose,
            hash: *hash,
            expires_at,
            consumed: false,
            email: None,
        });
        Ok(())
    }

    async fn email_token_consume(
        &self,
        purpose: EmailTokenPurpose,
        hash: &TokenHash,
        now: i64,
    ) -> StoreResult<Option<i64>> {
        let mut g = self.lock();
        let Some(t) = g.tokens.iter_mut().find(|t| {
            t.purpose == purpose && t.hash.ct_eq(hash) && !t.consumed && t.expires_at > now
        }) else {
            return Ok(None);
        };
        t.consumed = true;
        Ok(Some(t.user_id))
    }

    async fn passkey_user_handle(&self, account_id: i64, candidate: &[u8]) -> StoreResult<Vec<u8>> {
        Ok(self
            .lock()
            .user_handles
            .entry(account_id)
            .or_insert_with(|| candidate.to_vec())
            .clone())
    }

    async fn passkey_insert(&self, rec: PasskeyRecord) -> StoreResult<bool> {
        let mut g = self.lock();
        if g.passkeys
            .iter()
            .any(|p| p.credential_id == rec.credential_id)
        {
            return Ok(false);
        }
        g.passkeys.push(rec);
        Ok(true)
    }

    async fn passkey_list(&self, account_id: i64) -> StoreResult<Vec<PasskeyRecord>> {
        Ok(self
            .lock()
            .passkeys
            .iter()
            .filter(|p| p.account_id == account_id)
            .cloned()
            .collect())
    }

    async fn passkey_find(&self, credential_id: &[u8]) -> StoreResult<Option<PasskeyRecord>> {
        Ok(self
            .lock()
            .passkeys
            .iter()
            .find(|p| p.credential_id == credential_id)
            .cloned())
    }

    async fn passkey_rename(&self, account_id: i64, id: i64, name: &str) -> StoreResult<bool> {
        match self
            .lock()
            .passkeys
            .iter_mut()
            .find(|p| p.id == id && p.account_id == account_id)
        {
            Some(p) => {
                name.clone_into(&mut p.name);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    async fn passkey_delete(&self, account_id: i64, id: i64) -> StoreResult<bool> {
        let mut g = self.lock();
        let before = g.passkeys.len();
        g.passkeys
            .retain(|p| !(p.id == id && p.account_id == account_id));
        Ok(g.passkeys.len() != before)
    }

    async fn passkey_record_use(
        &self,
        id: i64,
        expected_sign_count: i64,
        sign_count: i64,
        backup_state: bool,
        now: i64,
    ) -> StoreResult<bool> {
        match self
            .lock()
            .passkeys
            .iter_mut()
            .find(|p| p.id == id && p.sign_count == expected_sign_count)
        {
            Some(p) => {
                p.sign_count = sign_count;
                p.backup_state = backup_state;
                p.last_used_at = Some(now);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    async fn delete_account(
        &self,
        id: i64,
        _ledger_id: i64,
    ) -> StoreResult<Result<DeletedAccount, DeleteRefusal>> {
        {
            let mut g = self.lock();
            let Some(a) = g
                .accounts
                .get_mut(&id)
                .filter(|a| a.state == AccountState::Active)
            else {
                return Ok(Err(DeleteRefusal::NotActive));
            };
            a.state = AccountState::Deleted;
            a.username = format!("deleted.{id}");
            a.display_name = Some("Deleted User".to_owned());
            a.email = format!("deleted.{id}@deleted.invalid");
            a.password_hash = "!".to_owned();
            g.totp.remove(&id);
            g.recovery.remove(&id);
            g.trusted.retain(|t| t.user_id != id);
            g.passkeys.retain(|p| p.account_id != id);
        }
        let _ = self.sessions.revoke_all_except(id, None).await;
        Ok(Ok(DeletedAccount::default()))
    }

    async fn audit(
        &self,
        actor_id: i64,
        action: &str,
        target_id: Option<i64>,
        note: &str,
        _now: i64,
    ) -> StoreResult<()> {
        self.lock()
            .audit
            .push((actor_id, action.to_owned(), target_id, note.to_owned()));
        Ok(())
    }
}
