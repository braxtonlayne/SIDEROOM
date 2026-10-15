//! In-memory [`AppsRepository`] for tests. It mirrors the Postgres rules that matter to the
//! service layer (uniqueness, compare-and-set transitions, cascades) but is not a model of
//! the whole schema: accounts and guilds are registered by the test.

use crate::{
    error::{AppsError, AppsResult},
    model::*,
    repo::*,
};
use async_trait::async_trait;
use serde_json::Value;
use sr_auth::session::TokenHash;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{Mutex, MutexGuard},
};

#[derive(Default)]
struct Account {
    username: String,
    display_name: Option<String>,
    flags: i64,
    is_bot: bool,
    active: bool,
}

#[derive(Default)]
struct Guild {
    owner: i64,
    members: HashSet<i64>,
}

struct TokenRow {
    id: i64,
    app: i64,
    user: i64,
    hash: TokenHash,
    revoked: bool,
}

struct CodeRow {
    rec: CodeRecord,
}

struct OAuthToken {
    rec: TokenRecord,
    code: Option<TokenHash>,
}

#[derive(Default)]
struct State {
    accounts: HashMap<i64, Account>,
    guilds: HashMap<i64, Guild>,
    dev_requests: Vec<DeveloperRequest>,
    apps: BTreeMap<i64, Application>,
    state_before_suspend: HashMap<i64, ReleaseState>,
    bot_tokens: Vec<TokenRow>,
    audit: Vec<AuditRow>,
    versions: Vec<AppVersion>,
    installs: Vec<Install>,
    commands: Vec<Command>,
    perms: Vec<(i64, i64, i64, CommandPermission)>,
    grants: Vec<(Grant, bool)>,
    codes: Vec<CodeRow>,
    used_codes: HashMap<Vec<u8>, i64>,
    tokens: Vec<OAuthToken>,
    interactions: Vec<InteractionRow>,
    imsgs: Vec<InteractionMessage>,
    interactions_cfg: HashMap<i64, InteractionsConfig>,
    testers: Vec<Tester>,
    declines: std::collections::HashMap<(i64, i64), i64>,
}

/// RAM-backed repository.
#[derive(Default)]
pub struct MemoryAppsRepo(Mutex<State>);

fn bytes(h: &TokenHash) -> Vec<u8> {
    h.as_bytes().to_vec()
}

impl MemoryAppsRepo {
    /// Empty repository.
    pub fn new() -> Self {
        Self::default()
    }

    fn st(&self) -> MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Register an active account.
    pub fn add_account(&self, id: i64, username: &str) {
        self.st().accounts.insert(
            id,
            Account {
                username: username.to_owned(),
                active: true,
                ..Account::default()
            },
        );
    }

    /// Register a guild with its owner (the owner is a member).
    pub fn add_guild(&self, id: i64, owner: i64, extra_members: &[i64]) {
        let mut g = Guild {
            owner,
            members: extra_members.iter().copied().collect(),
        };
        g.members.insert(owner);
        self.st().guilds.insert(id, g);
    }

    /// Is the user a member (tests check the bot joined)?
    pub fn is_member(&self, guild: i64, user: i64) -> bool {
        self.st()
            .guilds
            .get(&guild)
            .is_some_and(|g| g.members.contains(&user))
    }

    /// Remove a member (a kick).
    pub fn remove_member(&self, guild: i64, user: i64) {
        if let Some(g) = self.st().guilds.get_mut(&guild) {
            g.members.remove(&user);
        }
    }

    /// Is the account active?
    pub fn account_active(&self, id: i64) -> Option<bool> {
        self.st().accounts.get(&id).map(|a| a.active)
    }

    /// Deactivate an account (a banned bot owner, a closed bot).
    pub fn set_account_active(&self, id: i64, active: bool) {
        if let Some(a) = self.st().accounts.get_mut(&id) {
            a.active = active;
        }
    }
}

fn brief(id: i64, a: &Account) -> AccountBrief {
    AccountBrief {
        id,
        username: a.username.clone(),
        display_name: a.display_name.clone(),
        is_bot: a.is_bot,
    }
}

fn conflict(m: &str) -> AppsError {
    AppsError::Conflict(m.to_owned())
}

#[async_trait]
impl AppsRepository for MemoryAppsRepo {
    async fn account_brief(&self, account: i64) -> AppsResult<Option<AccountBrief>> {
        Ok(self.st().accounts.get(&account).map(|a| brief(account, a)))
    }

    async fn account_flags(&self, account: i64) -> AppsResult<Option<i64>> {
        Ok(self.st().accounts.get(&account).map(|a| a.flags))
    }

    async fn create_developer_request(
        &self,
        id: i64,
        account: i64,
        pitch: &str,
        prior_work: &str,
        now_ms: i64,
    ) -> AppsResult<DeveloperRequest> {
        let mut s = self.st();
        if s.dev_requests
            .iter()
            .any(|r| r.account_id == account && r.state == "pending")
        {
            return Err(conflict("A request is already waiting for review."));
        }
        let r = DeveloperRequest {
            id,
            account_id: account,
            pitch: pitch.to_owned(),
            prior_work: prior_work.to_owned(),
            state: "pending".into(),
            reviewer_note: None,
            created_at_ms: now_ms,
            reviewed_at_ms: None,
        };
        s.dev_requests.push(r.clone());
        Ok(r)
    }

    async fn latest_developer_request(&self, account: i64) -> AppsResult<Option<DeveloperRequest>> {
        Ok(self
            .st()
            .dev_requests
            .iter()
            .filter(|r| r.account_id == account)
            .max_by_key(|r| r.id)
            .cloned())
    }

    async fn developer_requests(
        &self,
        state: &str,
        limit: i64,
        before: Option<i64>,
    ) -> AppsResult<Vec<(DeveloperRequest, AccountBrief)>> {
        let s = self.st();
        let mut v: Vec<_> = s
            .dev_requests
            .iter()
            .filter(|r| r.state == state && before.is_none_or(|b| r.id < b))
            .cloned()
            .collect();
        v.sort_by_key(|r| std::cmp::Reverse(r.id));
        v.truncate(limit.max(0) as usize);
        Ok(v.into_iter()
            .filter_map(|r| {
                let b = s
                    .accounts
                    .get(&r.account_id)
                    .map(|a| brief(r.account_id, a))?;
                Some((r, b))
            })
            .collect())
    }

    async fn decide_developer_request(
        &self,
        id: i64,
        reviewer: i64,
        approve: bool,
        note: Option<&str>,
        now_ms: i64,
    ) -> AppsResult<DeveloperRequest> {
        let _ = reviewer;
        let mut s = self.st();
        let r = s
            .dev_requests
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or(AppsError::NotFound("request"))?;
        if r.state != "pending" {
            return Err(conflict("Already decided."));
        }
        r.state = if approve { "approved" } else { "rejected" }.into();
        r.reviewer_note = note.map(str::to_owned);
        r.reviewed_at_ms = Some(now_ms);
        let (out, acct) = (r.clone(), r.account_id);
        if approve && let Some(a) = s.accounts.get_mut(&acct) {
            a.flags |= FLAG_APP_DEVELOPER;
        }
        Ok(out)
    }

    async fn create_application(&self, new: NewApplication) -> AppsResult<Application> {
        let mut s = self.st();
        if !s
            .accounts
            .get(&new.owner_id)
            .is_some_and(|a| a.active && !a.is_bot)
        {
            return Err(AppsError::NotFound("account"));
        }
        let app = Application {
            id: new.id,
            owner_id: new.owner_id,
            bot_user_id: new.bot_user_id,
            name: new.name.clone(),
            description: new.description.clone(),
            icon_key: None,
            tags: vec![],
            bot_public: false,
            bot_require_code_grant: false,
            redirect_uris: vec![],
            install_scopes: vec!["bot".into(), "applications.commands".into()],
            install_permissions: 0,
            terms_url: None,
            privacy_url: None,
            release_state: ReleaseState::Development,
            suspended_reason: None,
            intents_requested: 0,
            intents_approved: 0,
            client_secret_hash: new.secret_hash,
            client_secret_hint: new.secret_hint.clone(),
            created_at_ms: new.now_ms,
        };
        s.accounts.insert(
            new.bot_user_id,
            Account {
                username: new.bot_username.clone(),
                display_name: Some(new.name.clone()),
                is_bot: true,
                active: true,
                ..Account::default()
            },
        );
        s.apps.insert(app.id, app.clone());
        s.bot_tokens.push(TokenRow {
            id: new.token_id,
            app: app.id,
            user: new.bot_user_id,
            hash: new.token_hash,
            revoked: false,
        });
        s.audit.push(AuditRow {
            id: new.audit_id,
            application_id: app.id,
            actor_id: new.owner_id,
            action: "app.create".into(),
            details: serde_json::json!({"name": new.name}),
            created_at_ms: new.now_ms,
        });
        Ok(app)
    }

    async fn application(&self, id: i64) -> AppsResult<Option<Application>> {
        Ok(self.st().apps.get(&id).cloned())
    }

    async fn application_by_bot(&self, bot_user: i64) -> AppsResult<Option<Application>> {
        Ok(self
            .st()
            .apps
            .values()
            .find(|a| a.bot_user_id == bot_user)
            .cloned())
    }

    async fn applications_of(&self, owner: i64) -> AppsResult<Vec<Application>> {
        Ok(self
            .st()
            .apps
            .values()
            .filter(|a| a.owner_id == owner)
            .cloned()
            .collect())
    }

    async fn update_application(
        &self,
        id: i64,
        p: &AppPatch,
        audit: AuditRow,
    ) -> AppsResult<Application> {
        let mut s = self.st();
        let a = s
            .apps
            .get_mut(&id)
            .ok_or(AppsError::NotFound("application"))?;
        macro_rules! set {
            ($field:ident) => {
                if let Some(v) = &p.$field {
                    a.$field = v.clone();
                }
            };
        }
        set!(name);
        set!(description);
        set!(icon_key);
        set!(tags);
        set!(bot_public);
        set!(bot_require_code_grant);
        set!(redirect_uris);
        set!(install_scopes);
        set!(install_permissions);
        set!(intents_requested);
        if let Some(v) = &p.terms_url {
            a.terms_url = v.clone();
        }
        if let Some(v) = &p.privacy_url {
            a.privacy_url = v.clone();
        }
        let (out, bot, name) = (a.clone(), a.bot_user_id, p.name.clone());
        if let (Some(n), Some(acc)) = (name, s.accounts.get_mut(&bot)) {
            acc.display_name = Some(n);
        }
        s.audit.push(audit);
        Ok(out)
    }

    async fn interactions_config(&self, app: i64) -> AppsResult<InteractionsConfig> {
        let s = self.st();
        if !s.apps.contains_key(&app) {
            return Err(AppsError::NotFound("application"));
        }
        Ok(s.interactions_cfg.get(&app).cloned().unwrap_or_default())
    }

    async fn set_interactions_config(
        &self,
        app: i64,
        endpoint_url: Option<Option<String>>,
        seed_if_absent: Option<crate::signing::SigningSeed>,
        audit: Option<AuditRow>,
    ) -> AppsResult<()> {
        let mut s = self.st();
        if !s.apps.contains_key(&app) {
            return Err(AppsError::NotFound("application"));
        }
        let cfg = s.interactions_cfg.entry(app).or_default();
        if let Some(u) = endpoint_url {
            // A newly saved URL passed the PING check: start healthy.
            if u.is_some() {
                cfg.state = EndpointState::Enabled;
                cfg.failures = 0;
                cfg.failure_window_start_ms = None;
            }
            cfg.endpoint_url = u;
        }
        if cfg.seed.is_none() {
            cfg.seed = seed_if_absent;
        }
        if let Some(a) = audit {
            s.audit.push(a);
        }
        Ok(())
    }

    async fn record_endpoint_outcome(&self, app: i64, ok: bool, now_ms: i64) -> AppsResult<bool> {
        let mut s = self.st();
        if !s.apps.contains_key(&app) {
            return Err(AppsError::NotFound("application"));
        }
        let cfg = s.interactions_cfg.entry(app).or_default();
        Ok(crate::service::endpoint_health_step(cfg, ok, now_ms))
    }

    async fn set_endpoint_state(
        &self,
        app: i64,
        state: EndpointState,
        audit: AuditRow,
    ) -> AppsResult<()> {
        let mut s = self.st();
        if !s.apps.contains_key(&app) {
            return Err(AppsError::NotFound("application"));
        }
        let cfg = s.interactions_cfg.entry(app).or_default();
        cfg.state = state;
        cfg.failures = 0;
        cfg.failure_window_start_ms = None;
        s.audit.push(audit);
        Ok(())
    }

    async fn account_by_username(&self, username: &str) -> AppsResult<Option<AccountBrief>> {
        Ok(self
            .st()
            .accounts
            .iter()
            .find(|(_, a)| a.active && !a.is_bot && a.username.eq_ignore_ascii_case(username))
            .map(|(id, a)| brief(*id, a)))
    }

    async fn invite_tester(&self, t: &Tester, audit: AuditRow) -> AppsResult<Tester> {
        let mut s = self.st();
        let n = s
            .testers
            .iter()
            .filter(|x| x.application_id == t.application_id)
            .count() as i64;
        if s.testers
            .iter()
            .any(|x| x.application_id == t.application_id && x.account_id == t.account_id)
        {
            return Err(AppsError::Conflict("Already invited.".into()));
        }
        if n >= MAX_TESTERS {
            return Err(AppsError::Conflict(format!(
                "An application can have at most {MAX_TESTERS} testers."
            )));
        }
        s.testers.push(t.clone());
        s.audit.push(audit);
        Ok(t.clone())
    }

    async fn testers(&self, app: i64) -> AppsResult<Vec<Tester>> {
        Ok(self
            .st()
            .testers
            .iter()
            .filter(|t| t.application_id == app)
            .cloned()
            .collect())
    }

    async fn tester_by_id(&self, id: i64) -> AppsResult<Option<Tester>> {
        Ok(self.st().testers.iter().find(|t| t.id == id).cloned())
    }

    async fn tester_of(&self, app: i64, account: i64) -> AppsResult<Option<Tester>> {
        Ok(self
            .st()
            .testers
            .iter()
            .find(|t| t.application_id == app && t.account_id == account)
            .cloned())
    }

    async fn accept_tester(&self, id: i64, audit: AuditRow) -> AppsResult<Tester> {
        let mut s = self.st();
        let t = s
            .testers
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or(AppsError::NotFound("invitation"))?;
        if t.state != TesterState::Invited {
            return Err(AppsError::Conflict("Already answered.".into()));
        }
        t.state = TesterState::Accepted;
        let out = t.clone();
        s.audit.push(audit);
        Ok(out)
    }

    async fn decline_tester(
        &self,
        app: i64,
        account: i64,
        now_ms: i64,
        audit: AuditRow,
    ) -> AppsResult<bool> {
        let removed = self.remove_tester(app, account, audit).await?;
        if removed {
            self.st().declines.insert((app, account), now_ms);
        }
        Ok(removed)
    }

    async fn declined_since(&self, app: i64, account: i64, since_ms: i64) -> AppsResult<bool> {
        Ok(self
            .st()
            .declines
            .get(&(app, account))
            .is_some_and(|at| *at >= since_ms))
    }

    async fn purge_tester_declines(&self, before_ms: i64) -> AppsResult<u64> {
        let mut s = self.st();
        let n = s.declines.len();
        s.declines.retain(|_, at| *at >= before_ms);
        Ok((n - s.declines.len()) as u64)
    }

    async fn remove_tester(&self, app: i64, account: i64, audit: AuditRow) -> AppsResult<bool> {
        let mut s = self.st();
        let before = s.testers.len();
        s.testers
            .retain(|t| !(t.application_id == app && t.account_id == account));
        let removed = s.testers.len() != before;
        if removed {
            s.audit.push(audit);
        }
        Ok(removed)
    }

    async fn set_bot_name(&self, app: i64, name: &str, audit: AuditRow) -> AppsResult<()> {
        let mut s = self.st();
        let bot = s
            .apps
            .get(&app)
            .ok_or(AppsError::NotFound("application"))?
            .bot_user_id;
        if let Some(a) = s.accounts.get_mut(&bot) {
            a.display_name = Some(name.to_owned());
        }
        s.audit.push(audit);
        Ok(())
    }

    async fn rotate_secret(
        &self,
        app: i64,
        hash: &TokenHash,
        hint: &str,
        _now_ms: i64,
        audit: AuditRow,
    ) -> AppsResult<()> {
        let mut s = self.st();
        let a = s
            .apps
            .get_mut(&app)
            .ok_or(AppsError::NotFound("application"))?;
        a.client_secret_hash = *hash;
        a.client_secret_hint = hint.to_owned();
        s.audit.push(audit);
        Ok(())
    }

    async fn reset_bot_token(
        &self,
        app: i64,
        token_id: i64,
        hash: &TokenHash,
        _now_ms: i64,
        audit: AuditRow,
    ) -> AppsResult<()> {
        let mut s = self.st();
        let bot = s
            .apps
            .get(&app)
            .ok_or(AppsError::NotFound("application"))?
            .bot_user_id;
        for t in s.bot_tokens.iter_mut().filter(|t| t.app == app) {
            t.revoked = true;
        }
        s.bot_tokens.push(TokenRow {
            id: token_id,
            app,
            user: bot,
            hash: *hash,
            revoked: false,
        });
        s.audit.push(audit);
        Ok(())
    }

    async fn delete_application(&self, app: i64, audit: AuditRow) -> AppsResult<Vec<i64>> {
        let mut s = self.st();
        let a = s
            .apps
            .remove(&app)
            .ok_or(AppsError::NotFound("application"))?;
        let mut left = Vec::new();
        for (gid, g) in s.guilds.iter_mut() {
            if g.members.remove(&a.bot_user_id) {
                left.push(*gid);
            }
        }
        if let Some(acc) = s.accounts.get_mut(&a.bot_user_id) {
            acc.active = false;
        }
        s.bot_tokens.retain(|t| t.app != app);
        s.installs.retain(|i| i.application_id != app);
        s.commands.retain(|c| c.application_id != app);
        s.perms.retain(|p| p.0 != app);
        s.versions.retain(|v| v.application_id != app);
        let gids: Vec<i64> = s
            .grants
            .iter()
            .filter(|g| g.0.application_id == app)
            .map(|g| g.0.id)
            .collect();
        s.tokens.retain(|t| !gids.contains(&t.rec.grant_id));
        s.codes.retain(|c| !gids.contains(&c.rec.grant.id));
        s.grants.retain(|g| g.0.application_id != app);
        let ints: Vec<i64> = s
            .interactions
            .iter()
            .filter(|i| i.application_id == app)
            .map(|i| i.id)
            .collect();
        s.imsgs.retain(|m| !ints.contains(&m.interaction_id));
        s.interactions.retain(|i| i.application_id != app);
        s.audit.push(audit);
        Ok(left)
    }

    async fn bot_by_token_hash(&self, hash: &TokenHash) -> AppsResult<Option<BotRecord>> {
        let s = self.st();
        let Some(t) = s.bot_tokens.iter().find(|t| &t.hash == hash && !t.revoked) else {
            return Ok(None);
        };
        let Some(app) = s.apps.get(&t.app).cloned() else {
            return Ok(None);
        };
        Ok(Some(BotRecord {
            token_id: t.id,
            user_id: t.user,
            application: app,
            token_hash: t.hash,
            account_active: s.accounts.get(&t.user).is_some_and(|a| a.active),
        }))
    }

    async fn application_audit(
        &self,
        app: i64,
        limit: i64,
        before: Option<i64>,
    ) -> AppsResult<Vec<AuditRow>> {
        let mut v: Vec<_> = self
            .st()
            .audit
            .iter()
            .filter(|a| a.application_id == app && before.is_none_or(|b| a.id < b))
            .cloned()
            .collect();
        v.sort_by_key(|a| std::cmp::Reverse(a.id));
        v.truncate(limit.max(0) as usize);
        Ok(v)
    }

    async fn record_audit(&self, row: AuditRow) -> AppsResult<()> {
        self.st().audit.push(row);
        Ok(())
    }

    async fn create_version(
        &self,
        id: i64,
        app: i64,
        manifest: Value,
        reasons: Vec<String>,
        note: Option<&str>,
        by: i64,
        now_ms: i64,
        audit: AuditRow,
    ) -> AppsResult<AppVersion> {
        let _ = by;
        let mut s = self.st();
        if s.versions
            .iter()
            .any(|v| v.application_id == app && v.state == "pending")
        {
            return Err(conflict("A version is already waiting for review."));
        }
        let n = s
            .versions
            .iter()
            .filter(|v| v.application_id == app)
            .count() as i32
            + 1;
        let v = AppVersion {
            id,
            application_id: app,
            version: n,
            manifest,
            needs_review_reasons: reasons,
            note: note.map(str::to_owned),
            state: "pending".into(),
            reviewer_note: None,
            created_at_ms: now_ms,
            decided_at_ms: None,
        };
        s.versions.push(v.clone());
        if let Some(a) = s.apps.get_mut(&app) {
            a.release_state = ReleaseState::InReview;
        }
        s.audit.push(audit);
        Ok(v)
    }

    async fn versions(&self, app: i64) -> AppsResult<Vec<AppVersion>> {
        let mut v: Vec<_> = self
            .st()
            .versions
            .iter()
            .filter(|v| v.application_id == app)
            .cloned()
            .collect();
        v.sort_by_key(|v| std::cmp::Reverse(v.id));
        Ok(v)
    }

    async fn version(&self, id: i64) -> AppsResult<Option<(AppVersion, Application)>> {
        let s = self.st();
        Ok(s.versions.iter().find(|v| v.id == id).and_then(|v| {
            s.apps
                .get(&v.application_id)
                .map(|a| (v.clone(), a.clone()))
        }))
    }

    async fn versions_by_state(
        &self,
        state: &str,
        limit: i64,
        before: Option<i64>,
    ) -> AppsResult<Vec<(AppVersion, Application)>> {
        let s = self.st();
        let mut v: Vec<_> = s
            .versions
            .iter()
            .filter(|v| v.state == state && before.is_none_or(|b| v.id < b))
            .filter_map(|v| {
                s.apps
                    .get(&v.application_id)
                    .map(|a| (v.clone(), a.clone()))
            })
            .collect();
        v.sort_by_key(|(v, _)| std::cmp::Reverse(v.id));
        v.truncate(limit.max(0) as usize);
        Ok(v)
    }

    async fn last_approved_manifest(&self, app: i64) -> AppsResult<Option<Value>> {
        Ok(self
            .st()
            .versions
            .iter()
            .filter(|v| v.application_id == app && v.state == "approved")
            .max_by_key(|v| v.id)
            .map(|v| v.manifest.clone()))
    }

    async fn decide_version(
        &self,
        id: i64,
        _reviewer: i64,
        state: &str,
        note: Option<&str>,
        approved_intents: Option<u64>,
        now_ms: i64,
        audit: AuditRow,
    ) -> AppsResult<(AppVersion, Application)> {
        let mut s = self.st();
        let v = s
            .versions
            .iter_mut()
            .find(|v| v.id == id)
            .ok_or(AppsError::NotFound("version"))?;
        if v.state != "pending" {
            return Err(conflict("Already decided."));
        }
        v.state = state.to_owned();
        v.reviewer_note = note.map(str::to_owned);
        v.decided_at_ms = Some(now_ms);
        let (out, app_id) = (v.clone(), v.application_id);
        let had_release = s
            .versions
            .iter()
            .any(|x| x.application_id == app_id && x.state == "approved");
        let a = s
            .apps
            .get_mut(&app_id)
            .ok_or(AppsError::NotFound("application"))?;
        if a.release_state != ReleaseState::Suspended {
            a.release_state = if state == "approved" || had_release {
                ReleaseState::Released
            } else {
                ReleaseState::Development
            };
        }
        if let Some(bits) = approved_intents {
            a.intents_approved = bits;
        }
        let app = a.clone();
        s.audit.push(audit);
        Ok((out, app))
    }

    async fn set_suspended(
        &self,
        app: i64,
        suspended: bool,
        reason: Option<&str>,
        audit: AuditRow,
    ) -> AppsResult<Application> {
        let mut s = self.st();
        let cur = s
            .apps
            .get(&app)
            .ok_or(AppsError::NotFound("application"))?
            .release_state;
        let restore = if suspended {
            s.state_before_suspend.insert(app, cur);
            ReleaseState::Suspended
        } else {
            s.state_before_suspend
                .remove(&app)
                .unwrap_or(ReleaseState::Development)
        };
        let a = s
            .apps
            .get_mut(&app)
            .ok_or(AppsError::NotFound("application"))?;
        a.release_state = restore;
        a.suspended_reason = if suspended {
            reason.map(str::to_owned)
        } else {
            None
        };
        let out = a.clone();
        s.audit.push(audit);
        Ok(out)
    }

    async fn guild_stats(&self, guild: i64) -> AppsResult<Option<GuildStats>> {
        Ok(self.st().guilds.get(&guild).map(|g| GuildStats {
            owner_id: g.owner,
            member_count: g.members.len() as i64,
        }))
    }

    async fn user_guilds(&self, user: i64) -> AppsResult<Vec<GuildBrief>> {
        let s = self.st();
        let mut v: Vec<GuildBrief> = s
            .guilds
            .iter()
            .filter(|(_, g)| g.members.contains(&user))
            .map(|(id, g)| GuildBrief {
                id: *id,
                name: format!("guild {id}"),
                icon_key: None,
                owner_id: g.owner,
                member_count: g.members.len() as i64,
            })
            .collect();
        v.sort_by_key(|g| g.id);
        Ok(v)
    }

    async fn install(&self, req: InstallWrite) -> AppsResult<InstallOutcome> {
        let mut s = self.st();
        let g = s
            .guilds
            .get_mut(&req.guild_id)
            .ok_or(AppsError::NotFound("guild"))?;
        let member_added = req.with_bot && g.members.insert(req.bot_user_id);
        let install = Install {
            application_id: req.application_id,
            guild_id: req.guild_id,
            installed_by: req.installed_by,
            scopes: req.scopes.clone(),
            permissions: req.permissions,
            role_id: req.with_bot.then_some(req.role_id),
            created_at_ms: req.now_ms,
        };
        s.installs
            .retain(|i| !(i.application_id == req.application_id && i.guild_id == req.guild_id));
        s.installs.push(install.clone());
        Ok(InstallOutcome {
            install,
            member_added,
            role: (req.with_bot).then(|| (req.role_id, req.bot_name.clone(), req.permissions)),
            role_created: member_added,
            audit_entry: member_added.then(
                || serde_json::json!({"id": req.guild_audit_id.to_string(), "action_type": 28}),
            ),
        })
    }

    async fn installs(&self, app: i64) -> AppsResult<Vec<Install>> {
        Ok(self
            .st()
            .installs
            .iter()
            .filter(|i| i.application_id == app)
            .cloned()
            .collect())
    }

    async fn guild_installs(&self, guild: i64) -> AppsResult<Vec<Install>> {
        Ok(self
            .st()
            .installs
            .iter()
            .filter(|i| i.guild_id == guild)
            .cloned()
            .collect())
    }

    async fn is_installed(&self, app: i64, guild: i64) -> AppsResult<bool> {
        let s = self.st();
        let Some(i) = s
            .installs
            .iter()
            .find(|i| i.application_id == app && i.guild_id == guild)
        else {
            return Ok(false);
        };
        let needs_bot = i.scopes.iter().any(|x| x == "bot");
        let bot = s.apps.get(&app).map(|a| a.bot_user_id);
        Ok(!needs_bot
            || bot.is_some_and(|b| s.guilds.get(&guild).is_some_and(|g| g.members.contains(&b))))
    }

    async fn commands(&self, app: i64, guild: Option<i64>) -> AppsResult<Vec<Command>> {
        let mut v: Vec<_> = self
            .st()
            .commands
            .iter()
            .filter(|c| c.application_id == app && c.guild_id == guild)
            .cloned()
            .collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(v)
    }

    async fn command(&self, app: i64, id: i64) -> AppsResult<Option<Command>> {
        Ok(self
            .st()
            .commands
            .iter()
            .find(|c| c.application_id == app && c.id == id)
            .cloned())
    }

    async fn command_by_id(&self, id: i64) -> AppsResult<Option<Command>> {
        Ok(self.st().commands.iter().find(|c| c.id == id).cloned())
    }

    async fn upsert_command(
        &self,
        app: i64,
        guild: Option<i64>,
        new_id: i64,
        version: i64,
        c: &NewCommand,
    ) -> AppsResult<(Command, bool)> {
        let mut s = self.st();
        let idx = s.commands.iter().position(|x| {
            x.application_id == app && x.guild_id == guild && x.kind == c.kind && x.name == c.name
        });
        let id = idx.map_or(new_id, |i| s.commands[i].id);
        let cmd = to_command(id, app, guild, version, c);
        match idx {
            Some(i) => s.commands[i] = cmd.clone(),
            None => s.commands.push(cmd.clone()),
        }
        Ok((cmd, idx.is_none()))
    }

    async fn replace_command(
        &self,
        app: i64,
        id: i64,
        version: i64,
        c: &NewCommand,
    ) -> AppsResult<Command> {
        let mut s = self.st();
        let slot = s
            .commands
            .iter_mut()
            .find(|x| x.application_id == app && x.id == id)
            .ok_or(AppsError::UnknownCommand)?;
        let cmd = to_command(id, app, slot.guild_id, version, c);
        *slot = cmd.clone();
        Ok(cmd)
    }

    async fn delete_command(&self, app: i64, id: i64) -> AppsResult<bool> {
        let mut s = self.st();
        let before = s.commands.len();
        s.commands
            .retain(|c| !(c.application_id == app && c.id == id));
        s.perms.retain(|p| !(p.0 == app && p.2 == id));
        Ok(s.commands.len() != before)
    }

    async fn bulk_overwrite(
        &self,
        app: i64,
        guild: Option<i64>,
        ids: Vec<i64>,
        version: i64,
        cmds: &[NewCommand],
    ) -> AppsResult<Vec<Command>> {
        let mut s = self.st();
        let old: Vec<Command> = s
            .commands
            .iter()
            .filter(|c| c.application_id == app && c.guild_id == guild)
            .cloned()
            .collect();
        s.commands
            .retain(|c| !(c.application_id == app && c.guild_id == guild));
        let mut out = Vec::new();
        for (i, c) in cmds.iter().enumerate() {
            let prior = old.iter().find(|o| o.kind == c.kind && o.name == c.name);
            let id = prior.map_or(ids[i], |p| p.id);
            let ver = match prior {
                Some(p) if same_definition(p, c) => p.version,
                _ => version,
            };
            let cmd = to_command(id, app, guild, ver, c);
            s.commands.push(cmd.clone());
            out.push(cmd);
        }
        let kept: Vec<i64> = out.iter().map(|c| c.id).collect();
        s.perms.retain(|p| {
            p.0 != app || guild.is_none_or(|g| p.1 != g) || p.2 == app || kept.contains(&p.2)
        });
        Ok(out)
    }

    async fn command_permissions(
        &self,
        app: i64,
        guild: i64,
        command_id: i64,
    ) -> AppsResult<Vec<CommandPermission>> {
        Ok(self
            .st()
            .perms
            .iter()
            .filter(|p| p.0 == app && p.1 == guild && p.2 == command_id)
            .map(|p| p.3)
            .collect())
    }

    async fn guild_command_permissions(
        &self,
        app: i64,
        guild: i64,
    ) -> AppsResult<Vec<(i64, Vec<CommandPermission>)>> {
        let s = self.st();
        let mut m: BTreeMap<i64, Vec<CommandPermission>> = BTreeMap::new();
        for p in s.perms.iter().filter(|p| p.0 == app && p.1 == guild) {
            m.entry(p.2).or_default().push(p.3);
        }
        Ok(m.into_iter().collect())
    }

    async fn set_command_permissions(
        &self,
        app: i64,
        guild: i64,
        command_id: i64,
        perms: &[CommandPermission],
    ) -> AppsResult<()> {
        let mut s = self.st();
        s.perms
            .retain(|p| !(p.0 == app && p.1 == guild && p.2 == command_id));
        for p in perms {
            s.perms.push((app, guild, command_id, *p));
        }
        Ok(())
    }

    async fn upsert_grant(
        &self,
        new_id: i64,
        app: i64,
        user: i64,
        scopes: &[String],
        now_ms: i64,
    ) -> AppsResult<Grant> {
        let mut s = self.st();
        if let Some((g, _)) = s
            .grants
            .iter_mut()
            .find(|(g, revoked)| !*revoked && g.application_id == app && g.user_id == user)
        {
            for sc in scopes {
                if !g.scopes.contains(sc) {
                    g.scopes.push(sc.clone());
                }
            }
            return Ok(g.clone());
        }
        let g = Grant {
            id: new_id,
            application_id: app,
            user_id: user,
            scopes: scopes.to_vec(),
            created_at_ms: now_ms,
        };
        s.grants.push((g.clone(), false));
        Ok(g)
    }

    async fn store_code(
        &self,
        hash: &TokenHash,
        grant_id: i64,
        redirect_uri: &str,
        scopes: &[String],
        challenge: Option<&str>,
        expires_ms: i64,
    ) -> AppsResult<()> {
        let mut s = self.st();
        let grant = s
            .grants
            .iter()
            .find(|(g, _)| g.id == grant_id)
            .map(|(g, _)| g.clone())
            .ok_or(AppsError::NotFound("grant"))?;
        s.codes.push(CodeRow {
            rec: CodeRecord {
                code_hash: *hash,
                grant,
                redirect_uri: redirect_uri.to_owned(),
                scopes: scopes.to_vec(),
                code_challenge: challenge.map(str::to_owned),
                expires_at_ms: expires_ms,
                used: false,
            },
        });
        Ok(())
    }

    async fn consume_code(&self, hash: &TokenHash) -> AppsResult<CodeConsume> {
        let mut s = self.st();
        let Some(c) = s.codes.iter_mut().find(|c| &c.rec.code_hash == hash) else {
            return Ok(CodeConsume::Unknown);
        };
        if c.rec.used {
            return Ok(CodeConsume::Replayed(c.rec.grant.id));
        }
        c.rec.used = true;
        let rec = c.rec.clone();
        s.used_codes.insert(bytes(hash), rec.grant.id);
        Ok(CodeConsume::Fresh(rec))
    }

    async fn insert_token(
        &self,
        rec: &TokenRecord,
        code: Option<&TokenHash>,
        _now_ms: i64,
    ) -> AppsResult<()> {
        self.st().tokens.push(OAuthToken {
            rec: rec.clone(),
            code: code.copied(),
        });
        Ok(())
    }

    async fn token_by_access(&self, hash: &TokenHash) -> AppsResult<Option<TokenRecord>> {
        let s = self.st();
        Ok(s.tokens
            .iter()
            .find(|t| &t.rec.access_hash == hash && !t.rec.revoked)
            .filter(|t| s.grants.iter().any(|(g, r)| g.id == t.rec.grant_id && !*r))
            .map(|t| t.rec.clone()))
    }

    async fn token_by_refresh(&self, hash: &TokenHash) -> AppsResult<RefreshLookup> {
        let s = self.st();
        let Some(t) = s.tokens.iter().find(|t| &t.rec.refresh_hash == hash) else {
            return Ok(RefreshLookup::Unknown);
        };
        let grant_live = s.grants.iter().any(|(g, r)| g.id == t.rec.grant_id && !*r);
        Ok(if t.rec.refresh_used || t.rec.revoked || !grant_live {
            RefreshLookup::Spent(t.rec.clone())
        } else {
            RefreshLookup::Live(t.rec.clone())
        })
    }

    async fn spend_refresh(&self, token_id: i64) -> AppsResult<bool> {
        let mut s = self.st();
        match s.tokens.iter_mut().find(|t| t.rec.id == token_id) {
            Some(t) if !t.rec.refresh_used => {
                t.rec.refresh_used = true;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn revoke_token(&self, token_id: i64) -> AppsResult<()> {
        if let Some(t) = self.st().tokens.iter_mut().find(|t| t.rec.id == token_id) {
            t.rec.revoked = true;
        }
        Ok(())
    }

    async fn revoke_grant(&self, grant_id: i64) -> AppsResult<()> {
        let mut s = self.st();
        for (g, r) in s.grants.iter_mut() {
            if g.id == grant_id {
                *r = true;
            }
        }
        for t in s.tokens.iter_mut().filter(|t| t.rec.grant_id == grant_id) {
            t.rec.revoked = true;
        }
        Ok(())
    }

    async fn revoke_code_tokens(&self, grant_id: i64, code: &TokenHash) -> AppsResult<()> {
        let mut s = self.st();
        for t in s
            .tokens
            .iter_mut()
            .filter(|t| t.rec.grant_id == grant_id && t.code.as_ref() == Some(code))
        {
            t.rec.revoked = true;
        }
        Ok(())
    }

    async fn grants_of(&self, user: i64) -> AppsResult<Vec<(Grant, Application)>> {
        let s = self.st();
        Ok(s.grants
            .iter()
            .filter(|(g, r)| !*r && g.user_id == user)
            .filter_map(|(g, _)| {
                s.apps
                    .get(&g.application_id)
                    .map(|a| (g.clone(), a.clone()))
            })
            .collect())
    }

    async fn grant_of(&self, user: i64, grant_id: i64) -> AppsResult<Option<Grant>> {
        Ok(self
            .st()
            .grants
            .iter()
            .find(|(g, r)| !*r && g.user_id == user && g.id == grant_id)
            .map(|(g, _)| g.clone()))
    }

    async fn insert_interaction(&self, row: &InteractionRow) -> AppsResult<()> {
        self.st().interactions.push(row.clone());
        Ok(())
    }

    async fn interaction(&self, id: i64) -> AppsResult<Option<InteractionRow>> {
        Ok(self.st().interactions.iter().find(|i| i.id == id).cloned())
    }

    async fn interaction_by_hash(&self, hash: &TokenHash) -> AppsResult<Option<InteractionRow>> {
        Ok(self
            .st()
            .interactions
            .iter()
            .find(|i| i.token_hash == *hash.as_bytes())
            .cloned())
    }

    async fn ack_interaction(
        &self,
        id: i64,
        state: &str,
        response_type: i16,
        ephemeral: bool,
        _now_ms: i64,
    ) -> AppsResult<bool> {
        let mut s = self.st();
        match s.interactions.iter_mut().find(|i| i.id == id) {
            Some(i) if i.state == "created" => {
                i.state = state.to_owned();
                i.response_type = Some(response_type);
                i.ephemeral = ephemeral;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn set_original_message(&self, id: i64, message_id: i64) -> AppsResult<()> {
        if let Some(i) = self.st().interactions.iter_mut().find(|i| i.id == id) {
            i.original_message_id = Some(message_id);
        }
        Ok(())
    }

    async fn add_interaction_message(&self, m: &InteractionMessage) -> AppsResult<()> {
        self.st().imsgs.push(m.clone());
        Ok(())
    }

    async fn interaction_message(&self, message_id: i64) -> AppsResult<Option<InteractionMessage>> {
        Ok(self
            .st()
            .imsgs
            .iter()
            .find(|m| m.message_id == message_id)
            .cloned())
    }

    async fn set_ephemeral_payload(&self, message_id: i64, payload: &Value) -> AppsResult<()> {
        if let Some(m) = self
            .st()
            .imsgs
            .iter_mut()
            .find(|m| m.message_id == message_id)
        {
            m.payload = Some(payload.clone());
        }
        Ok(())
    }

    async fn delete_interaction_message(&self, message_id: i64) -> AppsResult<()> {
        self.st().imsgs.retain(|m| m.message_id != message_id);
        Ok(())
    }

    async fn purge_interactions(&self, before_ms: i64) -> AppsResult<u64> {
        let mut s = self.st();
        let dead: Vec<i64> = s
            .interactions
            .iter()
            .filter(|i| i.expires_at_ms < before_ms)
            .map(|i| i.id)
            .collect();
        s.imsgs.retain(|m| !dead.contains(&m.interaction_id));
        s.interactions.retain(|i| !dead.contains(&i.id));
        Ok(dead.len() as u64)
    }
}

fn to_command(id: i64, app: i64, guild: Option<i64>, version: i64, c: &NewCommand) -> Command {
    Command {
        id,
        application_id: app,
        guild_id: guild,
        kind: c.kind,
        name: c.name.clone(),
        description: c.description.clone(),
        options: c.options.clone(),
        name_localizations: c.name_localizations.clone(),
        description_localizations: c.description_localizations.clone(),
        default_member_permissions: c.default_member_permissions,
        dm_permission: c.dm_permission,
        nsfw: c.nsfw,
        contexts: c.contexts.clone(),
        integration_types: c.integration_types.clone(),
        version,
    }
}

/// Does an existing command already say what the new definition says? (Bulk overwrite keeps
/// the version of unchanged commands.)
pub fn same_definition(old: &Command, new: &NewCommand) -> bool {
    old.description == new.description
        && old.options == new.options
        && old.name_localizations == new.name_localizations
        && old.description_localizations == new.description_localizations
        && old.default_member_permissions == new.default_member_permissions
        && old.dm_permission == new.dm_permission
        && old.nsfw == new.nsfw
        && old.contexts == new.contexts
        && old.integration_types == new.integration_types
}
