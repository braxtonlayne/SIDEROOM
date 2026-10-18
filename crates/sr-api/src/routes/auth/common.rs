//! Shared pieces: config, the service object, error rendering, extractors,
//! cookies.

use super::{
    discord::{DiscordOAuth, DiscordSettings, OAuthFlow, Pending, PendingLink},
    mail::Mailer,
    store::{AuthStore, StoreError},
    throttle::{Lockout, PendingMfaTable, WindowLimiter},
};
use crate::{
    HttpError,
    auth::{SESSION_COOKIE, session_cookie},
    routes::Ext,
    state::AppState,
};
use axum::{
    Json,
    extract::{ConnectInfo, FromRequestParts},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
};
use serde_json::json;
use sr_auth::{
    AuthError,
    password::{PasswordHasher, PasswordParams},
    repo::SessionRecord,
    session::{DeviceToken, SessionPolicy, SessionToken},
};
use sr_core::{ApiError, ErrorCode};
use std::{net::SocketAddr, sync::Arc};

/// Snowflake id source (`sr_core::SnowflakeGenerator::generate` wrapped).
pub type IdSource = Arc<dyn Fn() -> i64 + Send + Sync>;

/// Name of the pending-2FA cookie.
pub const MFA_COOKIE: &str = "sr_mfa";

/// Tunables for the auth routes.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// TOTP issuer label shown in authenticator apps.
    pub issuer: String,
    /// Public base URL used in emailed links (no trailing slash).
    pub public_url: String,
    /// Maximum counted accounts (D-013).
    pub account_cap: u64,
    /// Credits granted on registration (`invites.initial_credits`).
    pub initial_credits: i32,
    /// Take the client IP from the last `X-Forwarded-For` entry (set only
    /// behind a trusted reverse proxy that appends it).
    pub trust_forwarded_for: bool,
    /// Failed logins allowed before a lock starts.
    pub lockout_free_attempts: u32,
    /// First lock duration in seconds; doubles per further failure.
    pub lockout_base_secs: i64,
    /// Longest lock in seconds.
    pub lockout_max_secs: i64,
    /// Lifetime of the password-verified/2FA-pending state in seconds.
    pub mfa_ttl_secs: i64,
    /// Wrong second-factor codes before the pending login is discarded.
    pub mfa_max_attempts: u32,
    /// Minimum seconds between verification emails per account.
    pub verify_resend_cooldown_secs: i64,
    /// argon2id cost parameters for new password hashes (doc 06 section 4).
    pub argon2: PasswordParams,
    /// Session idle timeout in seconds (rolls on use).
    pub session_idle_secs: i64,
    /// Session absolute lifetime in seconds (cookie `Max-Age`).
    pub session_absolute_secs: i64,
    /// Password-reset link lifetime in seconds.
    pub reset_ttl_secs: i64,
    /// Email verification link lifetime in seconds.
    pub verify_ttl_secs: i64,
    /// Owner login link lifetime in seconds (open question 43).
    pub login_link_ttl_secs: i64,
    /// "Trust this device" lifetime in seconds (open question 144).
    pub trusted_device_secs: i64,
    /// WebAuthn RP id; default: the host of `public_url`.
    pub webauthn_rp_id: Option<String>,
    /// Extra exact origins allowed for passkey ceremonies (besides `public_url`),
    /// for example a dev server on another port.
    pub webauthn_origins: Vec<String>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            issuer: "SIDE ROOM".to_owned(),
            public_url: "http://localhost:8080".to_owned(),
            account_cap: 20_000,
            initial_credits: 5,
            trust_forwarded_for: false,
            lockout_free_attempts: 5,
            lockout_base_secs: 30,
            lockout_max_secs: 15 * 60,
            mfa_ttl_secs: 5 * 60,
            mfa_max_attempts: 5,
            verify_resend_cooldown_secs: 60,
            argon2: PasswordParams::default(),
            session_idle_secs: 30 * 86_400,
            session_absolute_secs: 180 * 86_400,
            reset_ttl_secs: 30 * 60,
            verify_ttl_secs: 24 * 3600,
            login_link_ttl_secs: 3600,
            trusted_device_secs: 30 * 86_400,
            webauthn_rp_id: None,
            webauthn_origins: Vec::new(),
        }
    }
}

impl AuthConfig {
    /// The session expiry policy these numbers describe. Install it on
    /// `AppState` with `AppStateBuilder::session_policy` so the `Principal`
    /// extractor and the login path agree.
    pub fn session_policy(&self) -> SessionPolicy {
        SessionPolicy {
            idle_ttl_secs: self.session_idle_secs,
            absolute_ttl_secs: self.session_absolute_secs,
        }
    }
}

/// Everything the handlers need. Build once and share via [`Arc`].
pub struct AuthService {
    pub(super) store: Arc<dyn AuthStore>,
    pub(super) mailer: Arc<dyn Mailer>,
    pub(super) hasher: PasswordHasher,
    pub(super) ids: IdSource,
    pub(super) cfg: AuthConfig,
    pub(super) ip_limits: WindowLimiter,
    pub(super) account_lock: Lockout,
    pub(super) ip_lock: Lockout,
    pub(super) pending: PendingMfaTable,
    /// Pending passkey ceremonies (single node, like `pending`).
    pub(super) ceremonies: super::passkeys::Ceremonies,
    /// Discord OAuth client; `None` when `[discord]` has no client id.
    pub(super) discord: Option<DiscordOAuth>,
    /// Started Discord flows, keyed by OAuth `state`.
    pub(super) discord_flows: Pending<OAuthFlow>,
    /// Verified Discord identities waiting for registration.
    pub(super) discord_links: Pending<PendingLink>,
    /// Event bus for staff alerts (open question 142); `None` in tests that do not care.
    pub(super) bus: Option<Arc<dyn sr_events::EventBus>>,
}

impl AuthService {
    /// Send a plain test message to the account's own address (the operator's "send test mail" button).
    /// Returns the address with its local part masked. Fails when the account is unknown or the mailer
    /// refuses; the error never carries the mail body.
    pub async fn send_operator_test_mail(&self, id: i64) -> Result<String, AuthRouteError> {
        let account = self
            .store
            .find_by_id(id)
            .await?
            .ok_or_else(|| AuthRouteError::internal("account missing"))?;
        let to = account.email.clone();
        let mail = super::mail::OutgoingMail {
            to: to.clone(),
            subject: "SIDE ROOM mail test".to_owned(),
            text: "This is a test message from the SIDE ROOM admin Setup page. If you can read it, outgoing mail works.".to_owned(),
            html: "<p>This is a test message from the SIDE ROOM admin Setup page. If you can read it, outgoing mail works.</p>".to_owned(),
        };
        self.mailer.send(mail).await.map_err(|e| {
            tracing::warn!("operator mail test failed: {e}");
            AuthRouteError::internal("mail delivery")
        })?;
        Ok(mask_email(&to))
    }

    /// Email a plain notice to the account's own address when that address is verified (a staff action
    /// that touched the account, K-20). Returns whether a message went out; a mailer failure is logged and
    /// reported as `false`, never as an error, because the action it reports already happened.
    pub async fn send_notice_mail(&self, id: i64, subject: &str, text: &str) -> bool {
        let Ok(Some(account)) = self.store.find_by_id(id).await else {
            return false;
        };
        if !account.email_verified {
            return false;
        }
        let html = format!("<p>{}</p>", super::mail::html_escape(text));
        let mail = super::mail::OutgoingMail {
            to: account.email,
            subject: subject.to_owned(),
            text: text.to_owned(),
            html,
        };
        match self.mailer.send(mail).await {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!("account notice mail failed: {e}");
                false
            }
        }
    }

    /// Set the account's locale (`PATCH /users/@me {locale}`).
    pub async fn set_locale(
        &self,
        id: i64,
        locale: &str,
    ) -> Result<(), sr_auth::store::StoreError> {
        self.store.set_locale(id, locale).await
    }

    /// The account row of `id` and whether TOTP is confirmed on it (for `GET /users/@me`).
    pub async fn account_overview(
        &self,
        id: i64,
    ) -> Result<Option<(super::store::AccountRecord, bool)>, sr_auth::store::StoreError> {
        let Some(account) = self.store.find_by_id(id).await? else {
            return Ok(None);
        };
        let mfa = self.store.totp_get(id).await?.is_some_and(|t| t.confirmed);
        Ok(Some((account, mfa)))
    }

    /// Service with default config and password hashing parameters.
    pub fn new(store: Arc<dyn AuthStore>, mailer: Arc<dyn Mailer>, ids: IdSource) -> Self {
        let cfg = AuthConfig::default();
        Self {
            hasher: PasswordHasher::new(cfg.argon2).unwrap_or_default(),
            account_lock: Lockout::new(
                cfg.lockout_free_attempts,
                cfg.lockout_base_secs,
                cfg.lockout_max_secs,
            ),
            // Per-IP: generous free budget so shared NATs are not punished.
            ip_lock: Lockout::new(30, cfg.lockout_base_secs, cfg.lockout_max_secs),
            store,
            mailer,
            ids,
            cfg,
            ip_limits: WindowLimiter::default(),
            pending: PendingMfaTable::default(),
            ceremonies: super::passkeys::Ceremonies::default(),
            discord: None,
            discord_flows: Pending::default(),
            discord_links: Pending::default(),
            bus: None,
        }
    }

    /// Publish staff alerts (banned-Person registration attempts) on this bus.
    pub fn with_bus(mut self, bus: Arc<dyn sr_events::EventBus>) -> Self {
        self.bus = Some(bus);
        self
    }

    /// Publish a `StaffAlert` on the platform topic. Best effort: the request has
    /// already been decided, so a bus failure is only logged.
    pub(super) async fn staff_alert(&self, kind: sr_events::StaffAlertKind, detail: &str) {
        tracing::warn!(target: "staff_alert", kind = kind.as_str(), "{detail}");
        if let Some(bus) = &self.bus
            && let Err(e) = bus
                .publish(
                    sr_events::Topic::Platform,
                    sr_events::DomainEvent::StaffAlert {
                        kind,
                        detail: detail.to_owned(),
                    },
                )
                .await
        {
            tracing::warn!(error = %e, "failed to publish staff alert");
        }
    }

    /// `SR_USER_UPDATE` to the user own sessions (account facts the gateway caches, such as
    /// the age-gate answer). Best effort.
    pub(super) async fn notify_user_update(&self, user_id: i64, payload: serde_json::Value) {
        let (Some(bus), Ok(uid)) = (&self.bus, u64::try_from(user_id)) else {
            return;
        };
        let user = sr_core::UserId::new(uid);
        if let Err(e) = bus
            .publish(
                sr_events::Topic::User(user),
                sr_events::DomainEvent::UserScoped {
                    users: vec![user],
                    name: "SR_USER_UPDATE".to_owned(),
                    payload,
                },
            )
            .await
        {
            tracing::warn!(error = %e, "failed to publish SR_USER_UPDATE");
        }
    }

    /// `SR_INVITE_CREDITS_UPDATE` to the issuer's own sessions after one of their codes was
    /// redeemed: the remaining credits (ENT-INV-009). Best effort.
    pub(super) async fn notify_invite_credits(&self, issuer_id: i64) {
        let (Some(bus), Ok(uid)) = (&self.bus, u64::try_from(issuer_id)) else {
            return;
        };
        let Ok(Some(balance)) = self.store.invite_credit_balance(issuer_id).await else {
            return;
        };
        let user = sr_core::UserId::new(uid);
        if let Err(e) = bus
            .publish(
                sr_events::Topic::User(user),
                sr_events::DomainEvent::UserScoped {
                    users: vec![user],
                    name: "SR_INVITE_CREDITS_UPDATE".to_owned(),
                    payload: serde_json::json!({ "credits": balance, "reason": "redemption" }),
                },
            )
            .await
        {
            tracing::warn!(error = %e, "failed to publish SR_INVITE_CREDITS_UPDATE");
        }
    }

    /// Post a card to a Staff Server feed (`#new-members`). Best effort, like alerts.
    pub(super) async fn staff_feed(
        &self,
        feed: sr_events::StaffFeedKind,
        title: &str,
        detail: &str,
        link: Option<String>,
    ) {
        if let Some(bus) = &self.bus
            && let Err(e) = bus
                .publish(
                    sr_events::Topic::Platform,
                    sr_events::DomainEvent::StaffFeed {
                        feed,
                        title: title.to_owned(),
                        detail: detail.to_owned(),
                        link,
                    },
                )
                .await
        {
            tracing::warn!(error = %e, "failed to publish staff feed card");
        }
    }

    /// Enable Discord sign-in and registration (open questions 78 and 79).
    pub fn with_discord(mut self, settings: &DiscordSettings) -> Result<Self, String> {
        self.discord = Some(DiscordOAuth::new(settings)?);
        Ok(self)
    }

    /// Replace the config (rebuilds the lockout policies).
    pub fn with_config(mut self, cfg: AuthConfig) -> Self {
        self.account_lock = Lockout::new(
            cfg.lockout_free_attempts,
            cfg.lockout_base_secs,
            cfg.lockout_max_secs,
        );
        self.ip_lock = Lockout::new(30, cfg.lockout_base_secs, cfg.lockout_max_secs);
        self.cfg = cfg;
        self
    }

    /// Replace the password hasher (tests use cheap parameters).
    pub fn with_hasher(mut self, hasher: PasswordHasher) -> Self {
        self.hasher = hasher;
        self
    }

    /// The account cap in force: the staff-set `accounts.cap` setting, else the configured one.
    pub(super) async fn account_cap(&self) -> Result<u64, sr_auth::store::StoreError> {
        Ok(self
            .store
            .account_cap_override()
            .await?
            .unwrap_or(self.cfg.account_cap))
    }

    pub(super) fn next_id(&self) -> i64 {
        (self.ids)()
    }

    /// Argon2 hashing off the async threads.
    pub(super) async fn hash_password(&self, pw: &str) -> Result<String, AuthRouteError> {
        let (h, pw) = (self.hasher.clone(), pw.to_owned());
        tokio::task::spawn_blocking(move || h.hash(&pw))
            .await
            .map_err(|_| AuthRouteError::internal("hash task"))?
            .map_err(AuthRouteError::from)
    }

    /// Verify against a stored hash off the async threads. A malformed stored
    /// hash counts as a mismatch (and is logged).
    pub(super) async fn verify_password(&self, pw: &str, stored: &str) -> bool {
        let (h, pw, stored) = (self.hasher.clone(), pw.to_owned(), stored.to_owned());
        match tokio::task::spawn_blocking(move || h.verify(&pw, &stored)).await {
            Ok(Ok(ok)) => ok,
            Ok(Err(e)) => {
                tracing::error!(error = %e, "stored password hash unusable");
                false
            }
            Err(_) => false,
        }
    }

    /// Equalise timing for an unknown account.
    pub(super) async fn dummy_verify(&self, pw: &str) {
        let (h, pw) = (self.hasher.clone(), pw.to_owned());
        let _ = tokio::task::spawn_blocking(move || h.dummy_verify(&pw)).await;
    }

    /// Create a session row and the `Set-Cookie` value for it.
    pub(super) async fn start_session(
        &self,
        state: &AppState,
        user_id: i64,
        user_agent: Option<&str>,
    ) -> Result<(SessionToken, String), AuthRouteError> {
        self.start_session_inner(state, user_id, user_agent, false)
            .await
    }

    /// Like [`Self::start_session`] for a login that just passed a second-factor
    /// check, so the session counts as freshly verified (F-03, 15 minutes).
    pub(super) async fn start_session_mfa(
        &self,
        state: &AppState,
        user_id: i64,
        user_agent: Option<&str>,
    ) -> Result<(SessionToken, String), AuthRouteError> {
        self.start_session_inner(state, user_id, user_agent, true)
            .await
    }

    async fn start_session_inner(
        &self,
        state: &AppState,
        user_id: i64,
        user_agent: Option<&str>,
        mfa_verified: bool,
    ) -> Result<(SessionToken, String), AuthRouteError> {
        let now = state.now_secs();
        let token = SessionToken::generate()?;
        let policy: &SessionPolicy = state.session_policy();
        let rec = SessionRecord {
            id: self.next_id(),
            user_id,
            token_hash: token.hash(),
            created_at: now,
            last_seen_at: now,
            expires_at: policy.expiry(now, now),
            device_name: super::validate::device_name(user_agent),
        };
        let session_id = rec.id;
        self.store.create_session(rec).await?;
        self.note_login_device(user_id, user_agent, now).await;
        if mfa_verified {
            self.store
                .session_mark_mfa_verified(session_id, now)
                .await?;
        }
        let cookie = session_cookie(
            &token,
            state.settings().secure_cookies,
            policy.absolute_ttl_secs,
        );
        Ok((token, cookie))
    }

    /// New-device sign-in alert (ENT-AUTH-015). Every session start comes through here, so the device label
    /// (`validate::device_name`, "Firefox on Windows") is recorded for each sign-in, including the one
    /// that registers the account (which never alerts). A label the account has not used before, on an
    /// account that wants alerts and has a verified address, sends one notice in the background. Best
    /// effort: a failure here never blocks the sign-in.
    async fn note_login_device(&self, user_id: i64, user_agent: Option<&str>, now: i64) {
        let Some(name) = super::validate::device_name(user_agent) else {
            return;
        };
        let key = name.to_lowercase();
        let sighting = match self
            .store
            .note_login_device(user_id, &key, &name, now)
            .await
        {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "could not record the sign-in device");
                return;
            }
        };
        if sighting != sr_auth::store::DeviceSighting::New {
            return;
        }
        let account = match self.store.find_by_id(user_id).await {
            Ok(Some(a)) if a.email_verified => a,
            _ => return,
        };
        if !self
            .store
            .login_alerts_enabled(user_id)
            .await
            .unwrap_or(true)
        {
            return;
        }
        // "2026-10-15T09:30:00Z" -> "2026-10-15 09:30 UTC"
        let iso = super::validate::iso8601(now);
        let when = format!("{} {} UTC", &iso[..10], &iso[11..16]);
        let mail = super::mail::new_login(&account.email, &name, &when, &account.locale);
        let mailer = self.mailer.clone();
        tokio::spawn(async move {
            if let Err(e) = mailer.send(mail).await {
                tracing::warn!(error = %e, "could not send the new-device alert");
            }
        });
    }

    /// Tell the account holder, by mail, that their own two-factor setup changed (ENT-AUTH-014). `change`
    /// is a sentence start such as "A passkey was added". Sent in the background to a verified address;
    /// a failure is only logged.
    pub(super) async fn notify_security(&self, user_id: i64, change: &str) {
        let account = match self.store.find_by_id(user_id).await {
            Ok(Some(a)) if a.email_verified => a,
            _ => return,
        };
        let mail = super::mail::two_factor_changed(&account.email, change, &account.locale);
        let mailer = self.mailer.clone();
        tokio::spawn(async move {
            if let Err(e) = mailer.send(mail).await {
                tracing::warn!(error = %e, "could not send the security notice");
            }
        });
    }

    /// Render the "This wasn't me" link of an email-change notice (the web page
    /// `/email-change/cancel?token=`, F-24).
    pub(super) fn cancel_email_link(&self, token: &str) -> String {
        format!(
            "{}/email-change/cancel?token={token}",
            self.cfg.public_url.trim_end_matches('/')
        )
    }

    /// Render a verification/reset link.
    pub(super) fn link(&self, path: &str, token: &str) -> String {
        format!(
            "{}/{path}/{token}",
            self.cfg.public_url.trim_end_matches('/')
        )
    }
}

/// `Set-Cookie` value that clears the session cookie.
pub(super) fn clear_session_cookie(secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{}",
        if secure { "; Secure" } else { "" }
    )
}

/// `Set-Cookie` value for the pending-2FA token (scoped to the API).
pub(super) fn mfa_cookie(token: &SessionToken, secure: bool, max_age: i64) -> String {
    format!(
        "{MFA_COOKIE}={}; Path=/api; HttpOnly; SameSite=Strict; Max-Age={max_age}{}",
        token.expose(),
        if secure { "; Secure" } else { "" }
    )
}

/// Name of the "trust this device" cookie (open question 144).
pub const TRUSTED_COOKIE: &str = "sr_trusted";

/// `Set-Cookie` value for a trusted device (scoped to the API).
pub(super) fn trusted_cookie(token: &DeviceToken, secure: bool, max_age: i64) -> String {
    format!(
        "{TRUSTED_COOKIE}={}; Path=/api; HttpOnly; SameSite=Strict; Max-Age={max_age}{}",
        token.expose(),
        if secure { "; Secure" } else { "" }
    )
}

/// `Set-Cookie` value that clears the pending-2FA cookie.
pub(super) fn clear_mfa_cookie(secure: bool) -> String {
    format!(
        "{MFA_COOKIE}=; Path=/api; HttpOnly; SameSite=Strict; Max-Age=0{}",
        if secure { "; Secure" } else { "" }
    )
}

/// Handler error: a normal API error, or a throttle with `Retry-After`.
#[derive(Debug)]
pub enum AuthRouteError {
    /// Rendered as the Discord-style JSON error.
    Api(HttpError),
    /// 429 with `retry_after` in the body and `Retry-After` header.
    Throttled {
        /// Seconds until the caller may retry.
        retry_after: i64,
    },
}

impl AuthRouteError {
    pub(super) fn internal(what: &str) -> Self {
        tracing::error!(what, "auth internal error");
        Self::Api(ErrorCode::SrInternal.into())
    }
}

impl From<HttpError> for AuthRouteError {
    fn from(e: HttpError) -> Self {
        Self::Api(e)
    }
}
impl From<ApiError> for AuthRouteError {
    fn from(e: ApiError) -> Self {
        Self::Api(e.into())
    }
}
impl From<ErrorCode> for AuthRouteError {
    fn from(e: ErrorCode) -> Self {
        Self::Api(e.into())
    }
}
impl From<StoreError> for AuthRouteError {
    fn from(e: StoreError) -> Self {
        tracing::error!(error = %e, "auth store failure");
        Self::Api(ErrorCode::SrInternal.into())
    }
}
impl From<AuthError> for AuthRouteError {
    fn from(e: AuthError) -> Self {
        tracing::error!(error = %e, "auth primitive failure");
        Self::Api(ErrorCode::SrInternal.into())
    }
}

impl IntoResponse for AuthRouteError {
    fn into_response(self) -> Response {
        match self {
            Self::Api(e) => e.into_response(),
            Self::Throttled { retry_after } => {
                let retry_after = retry_after.max(1);
                let body = json!({
                    "code": ErrorCode::TooManyRequests.code(),
                    "message": ErrorCode::TooManyRequests.message(),
                    "retry_after": retry_after,
                });
                let mut res = (StatusCode::TOO_MANY_REQUESTS, Json(body)).into_response();
                if let Ok(v) = HeaderValue::from_str(&retry_after.to_string()) {
                    res.headers_mut().insert(header::RETRY_AFTER, v);
                }
                res
            }
        }
    }
}

/// Handler result.
pub type R<T> = Result<T, AuthRouteError>;

/// The shared service plus request metadata. Extracting it also applies the
/// browser `Origin` check on unsafe methods.
pub struct Svc {
    /// The service.
    pub svc: Arc<AuthService>,
    /// Client IP (see [`AuthConfig::trust_forwarded_for`]).
    pub ip: String,
}

impl FromRequestParts<AppState> for Svc {
    type Rejection = AuthRouteError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Registered on AppState by sr-server (AppStateBuilder::extension).
        let Ext(svc) = Ext::<AuthService>::from_request_parts(parts, state).await?;
        if !matches!(
            parts.method,
            Method::GET | Method::HEAD | Method::OPTIONS | Method::TRACE
        ) && let Some(origin) = parts.headers.get(header::ORIGIN)
        {
            let allowed = origin.to_str().is_ok_and(|o| {
                state
                    .settings()
                    .allowed_origins
                    .iter()
                    .any(|a| a.eq_ignore_ascii_case(o))
            });
            if !allowed {
                return Err(
                    ApiError::with_message(ErrorCode::MissingAccess, "Origin not allowed").into(),
                );
            }
        }
        let ip = client_ip(parts, svc.cfg.trust_forwarded_for);
        Ok(Self { svc, ip })
    }
}

fn client_ip(parts: &Parts, trust_xff: bool) -> String {
    if trust_xff
        && let Some(ip) = parts
            .headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit(',').next())
            .map(str::trim)
            .filter(|s| !s.is_empty())
    {
        return ip.to_owned();
    }
    parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map_or_else(|| "unknown".to_owned(), |c| c.0.ip().to_string())
}

/// The `User-Agent` header, if valid UTF-8.
pub(super) fn user_agent(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
}

/// `j***@example.test`: the first character and the domain.
fn mask_email(email: &str) -> String {
    match email.split_once('@') {
        Some((local, domain)) => {
            let first: String = local.chars().take(1).collect();
            format!("{first}***@{domain}")
        }
        None => "***".to_owned(),
    }
}
