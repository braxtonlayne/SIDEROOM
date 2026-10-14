//! The developer platform service: onboarding, applications, bot credentials, release
//! review, installs. OAuth2, commands and interactions are in the sibling modules.

mod commands;
mod interactions;
mod oauth;

pub use commands::*;
pub use interactions::*;
pub use oauth::*;

use crate::{
    error::{AppsError, AppsResult, FieldProblem},
    manifest,
    model::*,
    perms,
    repo::{AccountBrief, AppsRepository, InstallOutcome, InstallWrite},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sr_auth::{
    oauth::{self as authoauth, OpaqueKind, OpaqueToken, Scope},
    tokens::BotToken,
};
use sr_core::{Clock, Permissions};
use std::sync::Arc;

/// Snowflake source.
pub type IdSource = Arc<dyn Fn() -> i64 + Send + Sync>;

/// Applications one account may own.
pub const MAX_APPLICATIONS: usize = 25;

/// The service. Cheap to clone.
#[derive(Clone)]
pub struct AppsService {
    pub(crate) repo: Arc<dyn AppsRepository>,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) ids: IdSource,
}

/// Distinguishes an absent JSON field from an explicit `null`.
pub fn nullable<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// `install_params` of an application.
#[derive(Debug, Clone, Deserialize)]
pub struct InstallParamsInput {
    /// Default scopes.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Default permissions bitfield (decimal string).
    #[serde(default)]
    pub permissions: Option<String>,
}

/// Body of `PATCH /applications/{id}`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateInput {
    /// Name.
    pub name: Option<String>,
    /// Description.
    pub description: Option<String>,
    /// Tags.
    pub tags: Option<Vec<String>>,
    /// Public bot.
    pub bot_public: Option<bool>,
    /// Code grant.
    pub bot_require_code_grant: Option<bool>,
    /// Redirect URIs.
    pub redirect_uris: Option<Vec<String>>,
    /// Install defaults.
    pub install_params: Option<InstallParamsInput>,
    /// Terms URL.
    #[serde(default, deserialize_with = "nullable")]
    pub terms_of_service_url: Option<Option<String>>,
    /// Privacy URL.
    #[serde(default, deserialize_with = "nullable")]
    pub privacy_policy_url: Option<Option<String>>,
    /// Requested privileged intents (names).
    pub privileged_intents: Option<Vec<String>>,
    /// Icon hash decided by the API layer after storing the image.
    #[serde(skip)]
    pub icon_key: Option<Option<String>>,
}

/// A created application with its one-time secrets.
#[derive(Debug)]
pub struct Created {
    /// The application.
    pub application: Application,
    /// Client secret, shown once.
    pub client_secret: String,
    /// Bot token, shown once.
    pub bot_token: String,
}

/// What the gateway needs to admit a bot.
#[derive(Debug, Clone)]
pub struct GatewayBot {
    /// Bot account.
    pub user_id: i64,
    /// Application.
    pub application: Application,
    /// Privileged intents the IDENTIFY may use.
    pub allowed_intents: u64,
}

/// A staff decision on a version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionDecision {
    /// Release it.
    Approve,
    /// Refuse it.
    Reject,
    /// Send it back.
    RequestChanges,
}

fn invalid(path: &str, code: &str, msg: &str) -> AppsError {
    AppsError::field(path, code, msg)
}

fn len_between(s: &str, lo: usize, hi: usize) -> bool {
    (lo..=hi).contains(&s.trim().chars().count()) && !s.chars().any(|c| c.is_control())
}

fn https_url(s: &str) -> bool {
    s.len() <= 512
        && s.starts_with("https://")
        && s.len() > 8
        && !s.chars().any(|c| c.is_control() || c == ' ')
}

impl AppsService {
    /// Build the service.
    pub fn new(repo: Arc<dyn AppsRepository>, clock: Arc<dyn Clock>, ids: IdSource) -> Self {
        Self { repo, clock, ids }
    }

    pub(crate) fn now_ms(&self) -> i64 {
        self.clock.now_unix_ms()
    }

    pub(crate) fn next_id(&self) -> i64 {
        (self.ids)()
    }

    pub(crate) fn audit(&self, app: i64, actor: i64, action: &str, details: Value) -> AuditRow {
        AuditRow {
            id: self.next_id(),
            application_id: app,
            actor_id: actor,
            action: action.to_owned(),
            details,
            created_at_ms: self.now_ms(),
        }
    }

    // ------------------------------------------------------------ developer requests

    /// `POST /developers/apply`.
    pub async fn apply(
        &self,
        user: i64,
        pitch: &str,
        prior_work: &str,
        accepted_terms: bool,
    ) -> AppsResult<DeveloperRequest> {
        let mut p = Vec::new();
        if !len_between(pitch, 1, 2000) {
            p.push(FieldProblem::new(
                "pitch",
                "BASE_TYPE_BAD_LENGTH",
                "Must be between 1 and 2000 in length.",
            ));
        }
        if prior_work.chars().count() > 2000 {
            p.push(FieldProblem::new(
                "prior_work",
                "BASE_TYPE_MAX_LENGTH",
                "Must be 2000 or fewer in length.",
            ));
        }
        if !accepted_terms {
            p.push(FieldProblem::new(
                "accepted_terms",
                "BOOLEAN_TYPE_COERCE",
                "You must accept the developer terms.",
            ));
        }
        if !p.is_empty() {
            return Err(AppsError::Invalid(p));
        }
        let flags = self
            .repo
            .account_flags(user)
            .await?
            .ok_or(AppsError::NotFound("account"))?;
        if flags & FLAG_APP_DEVELOPER != 0 {
            return Err(AppsError::Conflict(
                "This account is already an approved developer.".into(),
            ));
        }
        self.repo
            .create_developer_request(
                self.next_id(),
                user,
                pitch.trim(),
                prior_work.trim(),
                self.now_ms(),
            )
            .await
    }

    /// `GET /developers/status`.
    pub async fn developer_status(
        &self,
        user: i64,
    ) -> AppsResult<(bool, Option<DeveloperRequest>)> {
        let flags = self.repo.account_flags(user).await?.unwrap_or(0);
        Ok((
            flags & FLAG_APP_DEVELOPER != 0,
            self.repo.latest_developer_request(user).await?,
        ))
    }

    /// Staff list.
    pub async fn developer_requests(
        &self,
        state: &str,
        limit: i64,
        before: Option<i64>,
    ) -> AppsResult<Vec<(DeveloperRequest, AccountBrief)>> {
        if !["pending", "approved", "rejected", "suspended"].contains(&state) {
            return Err(invalid("state", "BASE_TYPE_CHOICES", "Unknown state."));
        }
        self.repo
            .developer_requests(state, limit.clamp(1, 100), before)
            .await
    }

    /// Staff decision. A rejection needs a note.
    pub async fn decide_developer_request(
        &self,
        reviewer: i64,
        id: i64,
        approve: bool,
        note: Option<&str>,
    ) -> AppsResult<DeveloperRequest> {
        let note = note.map(str::trim).filter(|n| !n.is_empty());
        if !approve && note.is_none() {
            return Err(invalid(
                "note",
                "BASE_TYPE_REQUIRED",
                "A note is required when rejecting.",
            ));
        }
        if note.is_some_and(|n| n.chars().count() > 2000) {
            return Err(invalid(
                "note",
                "BASE_TYPE_MAX_LENGTH",
                "Must be 2000 or fewer in length.",
            ));
        }
        self.repo
            .decide_developer_request(id, reviewer, approve, note, self.now_ms())
            .await
    }

    // ------------------------------------------------------------ applications

    fn check_name(name: &str) -> AppsResult<()> {
        if len_between(name, 2, 32) {
            Ok(())
        } else {
            Err(invalid(
                "name",
                "BASE_TYPE_BAD_LENGTH",
                "Must be between 2 and 32 in length.",
            ))
        }
    }

    /// `POST /applications`.
    pub async fn create_application(
        &self,
        owner: i64,
        name: &str,
        description: &str,
    ) -> AppsResult<Created> {
        Self::check_name(name)?;
        if description.chars().count() > 400 {
            return Err(invalid(
                "description",
                "BASE_TYPE_MAX_LENGTH",
                "Must be 400 or fewer in length.",
            ));
        }
        if self.repo.applications_of(owner).await?.len() >= MAX_APPLICATIONS {
            return Err(invalid(
                "name",
                "MAX_APPLICATIONS",
                "Maximum number of applications reached (25).",
            ));
        }
        let secret = OpaqueToken::generate(OpaqueKind::ClientSecret)
            .map_err(|e| AppsError::Storage(e.to_string()))?;
        let id = self.next_id();
        let bot_user_id = self.next_id();
        let token = BotToken::generate(bot_user_id, self.now_ms().div_euclid(1000))
            .map_err(|e| AppsError::Storage(e.to_string()))?;
        let new = NewApplication {
            id,
            owner_id: owner,
            bot_user_id,
            bot_person_id: self.next_id(),
            bot_username: format!("bot_{bot_user_id}"),
            name: name.trim().to_owned(),
            description: description.trim().to_owned(),
            secret_hash: secret.hash(),
            secret_hint: secret.hint(),
            token_id: self.next_id(),
            token_hash: token.hash(),
            audit_id: self.next_id(),
            now_ms: self.now_ms(),
        };
        let application = self.repo.create_application(new).await?;
        Ok(Created {
            application,
            client_secret: secret.expose().to_owned(),
            bot_token: token.expose().to_owned(),
        })
    }

    /// An application, only for its owner. Anyone else gets `NotFound` (no probing).
    pub async fn owned(&self, owner: i64, id: i64) -> AppsResult<Application> {
        match self.repo.application(id).await? {
            Some(a) if a.owner_id == owner => Ok(a),
            _ => Err(AppsError::NotFound("application")),
        }
    }

    /// Any application (public data; callers decide what to show).
    pub async fn application(&self, id: i64) -> AppsResult<Application> {
        self.repo
            .application(id)
            .await?
            .ok_or(AppsError::NotFound("application"))
    }

    /// The application a bot account belongs to.
    pub async fn application_of_bot(&self, bot_user: i64) -> AppsResult<Option<Application>> {
        self.repo.application_by_bot(bot_user).await
    }

    /// The owner's applications.
    pub async fn applications_of(&self, owner: i64) -> AppsResult<Vec<Application>> {
        self.repo.applications_of(owner).await
    }

    /// `PATCH /applications/{id}`.
    pub async fn update_application(
        &self,
        owner: i64,
        id: i64,
        input: UpdateInput,
    ) -> AppsResult<Application> {
        let app = self.owned(owner, id).await?;
        let mut p = Vec::new();
        let mut patch = AppPatch::default();
        if let Some(n) = &input.name {
            if len_between(n, 2, 32) {
                patch.name = Some(n.trim().to_owned());
            } else {
                p.push(FieldProblem::new(
                    "name",
                    "BASE_TYPE_BAD_LENGTH",
                    "Must be between 2 and 32 in length.",
                ));
            }
        }
        if let Some(d) = &input.description {
            if d.chars().count() <= 400 {
                patch.description = Some(d.trim().to_owned());
            } else {
                p.push(FieldProblem::new(
                    "description",
                    "BASE_TYPE_MAX_LENGTH",
                    "Must be 400 or fewer in length.",
                ));
            }
        }
        if let Some(t) = &input.tags {
            if t.len() <= 5 && t.iter().all(|x| len_between(x, 1, 20)) {
                patch.tags = Some(t.iter().map(|x| x.trim().to_owned()).collect());
            } else {
                p.push(FieldProblem::new(
                    "tags",
                    "BASE_TYPE_MAX_LENGTH",
                    "At most 5 tags of 1 to 20 characters.",
                ));
            }
        }
        patch.bot_public = input.bot_public;
        patch.bot_require_code_grant = input.bot_require_code_grant;
        if let Some(uris) = &input.redirect_uris {
            let mut clean: Vec<String> = Vec::new();
            if uris.len() > 10 {
                p.push(FieldProblem::new(
                    "redirect_uris",
                    "BASE_TYPE_MAX_LENGTH",
                    "Must be 10 or fewer in length.",
                ));
            }
            for (i, u) in uris.iter().enumerate() {
                match authoauth::validate_redirect_uri(u) {
                    Ok(()) if !clean.contains(u) => clean.push(u.clone()),
                    Ok(()) => {}
                    Err(e) => p.push(FieldProblem::new(
                        format!("redirect_uris.{i}"),
                        "INVALID_REDIRECT_URI",
                        e.message(),
                    )),
                }
            }
            patch.redirect_uris = Some(clean);
        }
        if let Some(ip) = &input.install_params {
            let mut scopes = Vec::new();
            for s in &ip.scopes {
                match Scope::parse(s) {
                    Some(Scope::Bot | Scope::ApplicationsCommands) => scopes.push(s.clone()),
                    _ => p.push(FieldProblem::new(
                        "install_params.scopes",
                        "BASE_TYPE_CHOICES",
                        "Only bot and applications.commands are install scopes.",
                    )),
                }
            }
            scopes.dedup();
            patch.install_scopes = Some(scopes);
            match ip.permissions.as_deref().unwrap_or("0").parse::<u64>() {
                Ok(v) => patch.install_permissions = Some(v & Permissions::all().bits()),
                Err(_) => p.push(FieldProblem::new(
                    "install_params.permissions",
                    "NUMBER_TYPE_COERCE",
                    "Not a bitfield.",
                )),
            }
        }
        for (key, val, out) in [
            (
                "terms_of_service_url",
                &input.terms_of_service_url,
                &mut patch.terms_url,
            ),
            (
                "privacy_policy_url",
                &input.privacy_policy_url,
                &mut patch.privacy_url,
            ),
        ] {
            match val {
                Some(Some(u)) if https_url(u) => *out = Some(Some(u.clone())),
                Some(Some(_)) => p.push(FieldProblem::new(
                    key,
                    "URL_TYPE_INVALID_URL",
                    "Must be an https URL of at most 512 characters.",
                )),
                Some(None) => *out = Some(None),
                None => {}
            }
        }
        if let Some(names) = &input.privileged_intents {
            match intents::from_names(names) {
                Ok(bits) => patch.intents_requested = Some(bits),
                Err(bad) => p.push(FieldProblem::new(
                    "privileged_intents",
                    "BASE_TYPE_CHOICES",
                    format!("Unknown intent {bad}."),
                )),
            }
        }
        patch.icon_key = input.icon_key.clone();
        if !p.is_empty() {
            return Err(AppsError::Invalid(p));
        }
        let changed: Vec<&str> = [
            ("name", patch.name.is_some()),
            ("description", patch.description.is_some()),
            ("icon", patch.icon_key.is_some()),
            ("tags", patch.tags.is_some()),
            ("bot_public", patch.bot_public.is_some()),
            ("redirect_uris", patch.redirect_uris.is_some()),
            ("install_params", patch.install_scopes.is_some()),
            (
                "urls",
                patch.terms_url.is_some() || patch.privacy_url.is_some(),
            ),
            ("privileged_intents", patch.intents_requested.is_some()),
        ]
        .into_iter()
        .filter_map(|(k, on)| on.then_some(k))
        .collect();
        let audit = self.audit(app.id, owner, "app.update", json!({"changed": changed}));
        self.repo.update_application(app.id, &patch, audit).await
    }

    /// `PATCH /applications/{id}/bot`: the bot's display name.
    pub async fn rename_bot(&self, owner: i64, id: i64, username: &str) -> AppsResult<()> {
        let app = self.owned(owner, id).await?;
        if !len_between(username, 2, 32) {
            return Err(invalid(
                "username",
                "BASE_TYPE_BAD_LENGTH",
                "Must be between 2 and 32 in length.",
            ));
        }
        let audit = self.audit(app.id, owner, "app.bot_rename", json!({}));
        self.repo.set_bot_name(app.id, username.trim(), audit).await
    }

    /// New client secret (returned once).
    pub async fn reset_secret(&self, owner: i64, id: i64) -> AppsResult<String> {
        let app = self.owned(owner, id).await?;
        let secret = OpaqueToken::generate(OpaqueKind::ClientSecret)
            .map_err(|e| AppsError::Storage(e.to_string()))?;
        let audit = self.audit(app.id, owner, "app.secret_reset", json!({}));
        self.repo
            .rotate_secret(app.id, &secret.hash(), &secret.hint(), self.now_ms(), audit)
            .await?;
        Ok(secret.expose().to_owned())
    }

    /// New bot token (returned once); the old one stops working at once.
    pub async fn reset_bot_token(&self, owner: i64, id: i64) -> AppsResult<String> {
        let app = self.owned(owner, id).await?;
        let token = BotToken::generate(app.bot_user_id, self.now_ms().div_euclid(1000))
            .map_err(|e| AppsError::Storage(e.to_string()))?;
        let audit = self.audit(app.id, owner, "app.token_reset", json!({}));
        self.repo
            .reset_bot_token(app.id, self.next_id(), &token.hash(), self.now_ms(), audit)
            .await?;
        Ok(token.expose().to_owned())
    }

    /// Delete the application. Returns the guilds the bot was removed from.
    pub async fn delete_application(
        &self,
        owner: i64,
        id: i64,
    ) -> AppsResult<(Application, Vec<i64>)> {
        let app = self.owned(owner, id).await?;
        let audit = self.audit(app.id, owner, "app.delete", json!({"name": app.name}));
        let guilds = self.repo.delete_application(app.id, audit).await?;
        Ok((app, guilds))
    }

    /// The audit trail of an owned application.
    pub async fn audit_log(
        &self,
        owner: i64,
        id: i64,
        limit: i64,
        before: Option<i64>,
    ) -> AppsResult<Vec<AuditRow>> {
        let app = self.owned(owner, id).await?;
        self.repo
            .application_audit(app.id, limit.clamp(1, 100), before)
            .await
    }

    /// Installs of an owned application.
    pub async fn installs_of(&self, owner: i64, id: i64) -> AppsResult<Vec<Install>> {
        let app = self.owned(owner, id).await?;
        self.repo.installs(app.id).await
    }

    // ------------------------------------------------------------ bot credentials

    /// Resolve a presented bot token to its record, or `None` for anything wrong: unknown,
    /// revoked, malformed, a user id that does not match, a suspended application or an
    /// inactive bot account.
    pub async fn authenticate_bot(&self, presented: &str) -> AppsResult<Option<BotRecord>> {
        let Some((parts, hash)) = authoauth::hash_bot_token(presented) else {
            return Ok(None);
        };
        let Some(rec) = self.repo.bot_by_token_hash(&hash).await? else {
            return Ok(None);
        };
        // The index lookup already matched; compare again in constant time and bind the
        // token to the account it names.
        if !authoauth::digests_equal(&rec.token_hash, &hash)
            || rec.user_id != parts.user_id
            || !rec.account_active
            || !rec.application.is_live()
        {
            return Ok(None);
        }
        Ok(Some(rec))
    }

    /// What the gateway needs for a bot IDENTIFY.
    pub async fn gateway_bot(&self, presented: &str) -> AppsResult<Option<GatewayBot>> {
        Ok(self.authenticate_bot(presented).await?.map(|r| GatewayBot {
            user_id: r.user_id,
            allowed_intents: r.application.allowed_intents(),
            application: r.application,
        }))
    }

    // ------------------------------------------------------------ release review

    /// Submit a version (the manifest is a snapshot of the current settings).
    pub async fn submit_version(
        &self,
        owner: i64,
        id: i64,
        note: Option<&str>,
    ) -> AppsResult<AppVersion> {
        let app = self.owned(owner, id).await?;
        match app.release_state {
            ReleaseState::Suspended => return Err(AppsError::Suspended),
            ReleaseState::InReview => {
                return Err(AppsError::Conflict(
                    "A version is already waiting for review.".into(),
                ));
            }
            _ => {}
        }
        if app.privacy_url.is_none() {
            return Err(invalid(
                "privacy_policy_url",
                "BASE_TYPE_REQUIRED",
                "A privacy policy URL is required to submit a version.",
            ));
        }
        if note.is_some_and(|n| n.chars().count() > 2000) {
            return Err(invalid(
                "note",
                "BASE_TYPE_MAX_LENGTH",
                "Must be 2000 or fewer in length.",
            ));
        }
        let commands = self.repo.commands(app.id, None).await?;
        let manifest = manifest::build_manifest(&app, &commands);
        let prev = self.repo.last_approved_manifest(app.id).await?;
        let reasons = manifest::review_reasons(prev.as_ref(), &manifest);
        let audit = self.audit(
            app.id,
            owner,
            "app.version_submit",
            json!({"reasons": reasons}),
        );
        self.repo
            .create_version(
                self.next_id(),
                app.id,
                manifest,
                reasons,
                note.map(str::trim).filter(|n| !n.is_empty()),
                owner,
                self.now_ms(),
                audit,
            )
            .await
    }

    /// Versions of an owned application, newest first.
    pub async fn versions_of(
        &self,
        owner: i64,
        id: i64,
    ) -> AppsResult<Vec<(AppVersion, Vec<Value>)>> {
        let app = self.owned(owner, id).await?;
        let list = self.repo.versions(app.id).await?;
        let prev = self.repo.last_approved_manifest(app.id).await?;
        Ok(list
            .into_iter()
            .map(|v| {
                let d = manifest::diff(prev.as_ref(), &v.manifest);
                (v, d)
            })
            .collect())
    }

    /// Withdraw the owner's pending version.
    pub async fn withdraw_version(
        &self,
        owner: i64,
        app_id: i64,
        version_id: i64,
    ) -> AppsResult<AppVersion> {
        let app = self.owned(owner, app_id).await?;
        let (v, _) = self
            .repo
            .version(version_id)
            .await?
            .filter(|(v, _)| v.application_id == app.id)
            .ok_or(AppsError::NotFound("version"))?;
        let audit = self.audit(
            app.id,
            owner,
            "app.version_withdraw",
            json!({"version": v.version}),
        );
        let (v, _) = self
            .repo
            .decide_version(v.id, owner, "withdrawn", None, None, self.now_ms(), audit)
            .await?;
        Ok(v)
    }

    /// Staff queue of versions.
    pub async fn pending_versions(
        &self,
        state: &str,
        limit: i64,
        before: Option<i64>,
    ) -> AppsResult<Vec<(AppVersion, Application, Vec<Value>)>> {
        if ![
            "pending",
            "approved",
            "rejected",
            "changes_requested",
            "withdrawn",
        ]
        .contains(&state)
        {
            return Err(invalid("state", "BASE_TYPE_CHOICES", "Unknown state."));
        }
        let rows = self
            .repo
            .versions_by_state(state, limit.clamp(1, 100), before)
            .await?;
        let mut out = Vec::new();
        for (v, a) in rows {
            let prev = self.repo.last_approved_manifest(a.id).await?;
            let d = manifest::diff(prev.as_ref(), &v.manifest);
            out.push((v, a, d));
        }
        Ok(out)
    }

    /// Staff decision on a pending version.
    pub async fn decide_version(
        &self,
        reviewer: i64,
        id: i64,
        decision: VersionDecision,
        note: Option<&str>,
        approved_intents: &[String],
    ) -> AppsResult<(AppVersion, Application)> {
        let note = note.map(str::trim).filter(|n| !n.is_empty());
        if decision != VersionDecision::Approve && note.is_none() {
            return Err(invalid("note", "BASE_TYPE_REQUIRED", "A note is required."));
        }
        if note.is_some_and(|n| n.chars().count() > 2000) {
            return Err(invalid(
                "note",
                "BASE_TYPE_MAX_LENGTH",
                "Must be 2000 or fewer in length.",
            ));
        }
        let (v, app) = self
            .repo
            .version(id)
            .await?
            .ok_or(AppsError::NotFound("version"))?;
        let bits = intents::from_names(approved_intents).map_err(|b| {
            invalid(
                "approved_intents",
                "BASE_TYPE_CHOICES",
                &format!("Unknown intent {b}."),
            )
        })?;
        let (state, approved) = match decision {
            VersionDecision::Approve => {
                let asked = app.intents_requested & intents::PRIVILEGED;
                if bits & !asked != 0 {
                    return Err(invalid(
                        "approved_intents",
                        "BASE_TYPE_CHOICES",
                        "Cannot approve an intent the application did not request.",
                    ));
                }
                ("approved", Some(bits))
            }
            VersionDecision::Reject => ("rejected", None),
            VersionDecision::RequestChanges => ("changes_requested", None),
        };
        let audit = self.audit(
            app.id,
            reviewer,
            "app.version_decision",
            json!({"version": v.version, "decision": state}),
        );
        self.repo
            .decide_version(id, reviewer, state, note, approved, self.now_ms(), audit)
            .await
    }

    /// Staff: suspend an application.
    pub async fn suspend(&self, staff: i64, id: i64, reason: &str) -> AppsResult<Application> {
        if !len_between(reason, 1, 500) {
            return Err(invalid(
                "reason",
                "BASE_TYPE_BAD_LENGTH",
                "A reason of 1 to 500 characters is required.",
            ));
        }
        let app = self.application(id).await?;
        if app.release_state == ReleaseState::Suspended {
            return Err(AppsError::Conflict("Already suspended.".into()));
        }
        let audit = self.audit(id, staff, "app.suspend", json!({"reason": reason.trim()}));
        self.repo
            .set_suspended(id, true, Some(reason.trim()), audit)
            .await
    }

    /// Staff: lift a suspension.
    pub async fn unsuspend(
        &self,
        staff: i64,
        id: i64,
        reason: Option<&str>,
    ) -> AppsResult<Application> {
        let app = self.application(id).await?;
        if app.release_state != ReleaseState::Suspended {
            return Err(AppsError::Conflict("Not suspended.".into()));
        }
        let audit = self.audit(id, staff, "app.unsuspend", json!({"reason": reason}));
        self.repo.set_suspended(id, false, None, audit).await
    }

    // ------------------------------------------------------------ installs

    /// Install an application into a guild.
    pub async fn install(&self, req: InstallRequest) -> AppsResult<InstallOutcome> {
        let app = self.application(req.application_id).await?;
        let stats = self
            .repo
            .guild_stats(req.guild_id)
            .await?
            .ok_or(AppsError::NotFound("guild"))?;
        perms::check_install(&app, req.installer, req.installer_perms, stats)?;
        let with_bot = req.scopes.contains(&Scope::Bot);
        let granted = if with_bot {
            perms::granted_permissions(
                req.requested_permissions,
                req.installer_perms,
                req.installer == stats.owner_id,
            )
        } else {
            0
        };
        let w = InstallWrite {
            application_id: app.id,
            bot_user_id: app.bot_user_id,
            bot_name: app.name.clone(),
            guild_id: req.guild_id,
            installed_by: req.installer,
            scopes: authoauth::scope_names(&req.scopes),
            permissions: granted,
            with_bot,
            role_id: self.next_id(),
            guild_audit_id: self.next_id(),
            app_audit_id: self.next_id(),
            now_ms: self.now_ms(),
        };
        self.repo.install(w).await
    }

    /// Is the application installed in the guild?
    pub async fn is_installed(&self, app: i64, guild: i64) -> AppsResult<bool> {
        self.repo.is_installed(app, guild).await
    }

    /// Facts about a guild for the consent screen.
    pub async fn user_guilds(&self, user: i64) -> AppsResult<Vec<GuildBrief>> {
        self.repo.user_guilds(user).await
    }

    /// Facts about a guild for the consent screen.
    pub async fn guild_stats(&self, guild: i64) -> AppsResult<Option<GuildStats>> {
        self.repo.guild_stats(guild).await
    }
}

/// An install request after the API layer resolved the installer's standing.
#[derive(Debug, Clone)]
pub struct InstallRequest {
    /// Application.
    pub application_id: i64,
    /// Installer account.
    pub installer: i64,
    /// Installer's guild-level permissions.
    pub installer_perms: Permissions,
    /// Guild.
    pub guild_id: i64,
    /// Scopes authorised.
    pub scopes: Vec<Scope>,
    /// Requested permission bits.
    pub requested_permissions: u64,
}
