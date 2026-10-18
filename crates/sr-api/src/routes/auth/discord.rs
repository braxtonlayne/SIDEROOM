//! Discord OAuth sign-in and registration (ENT-AUTH-011; open questions 78
//! and 79).
//!
//! The server runs the whole exchange, so the client secret never reaches the
//! browser. Nothing a client sends can create a link on its own:
//! * `sr_oauth_state` (10 min) holds the OAuth `state`; the callback checks it
//!   against the value the server issued (binds the flow to this browser).
//! * `sr_oauth` (10 min) names a pending Discord link for registration. The
//!   Discord user id stays in server memory, keyed by that opaque token, so a
//!   client cannot forge a link. Single node, like the other pending tables.
//!
//! Login: a linked account signs in (behind its second factor, if any).
//! Registration: the pending link is attached to the new account when the
//! registration succeeds (see `register`). An unlinked Discord user is never
//! given an account here.
//! Link and unlink (User Settings > Account): `POST /sr/auth/discord/link` (a session) starts a flow
//! bound to the signed-in account and answers the authorize URL; the callback attaches the Discord
//! identity to that account. `POST /sr/auth/discord/unlink` (the password) detaches it; the record
//! stays, so the identity is not freed for another account (D-030).
//! Every flow uses PKCE (S256) on top of the state cookie.

use super::common::{AuthRouteError, R, Svc, mfa_cookie, user_agent};
use super::mfa::{current_account, reauthenticate};
use super::sessions::session_id;
use crate::{auth::UserOnly, openapi::ErrorBody};
use crate::{auth::parse_cookie, state::AppState};
use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{AppendHeaders, IntoResponse, Response},
};
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointNotSet, EndpointSet,
    PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, TokenResponse, TokenUrl,
    basic::BasicClient,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sr_auth::{
    session::SessionToken,
    store::{AccountState, DiscordLink},
};
use sr_core::{ApiError, ErrorCode};
use std::{collections::HashMap, sync::Mutex};
use utoipa::{OpenApi, ToSchema};

/// Cookie holding the OAuth `state` (open question 79).
pub const STATE_COOKIE: &str = "sr_oauth_state";
/// Cookie naming a pending Discord link for registration.
pub const LINK_COOKIE: &str = "sr_oauth";
/// Lifetime of both cookies and of the server-side entries.
const OAUTH_TTL_SECS: i64 = 600;
/// Where the browser lands after a link attempt (User Settings > Account).
const SETTINGS_PAGE: &str = "/settings/user/account";

const AUTH_URL: &str = "https://discord.com/oauth2/authorize";
const TOKEN_URL: &str = "https://discord.com/api/oauth2/token";
const USER_URL: &str = "https://discord.com/api/users/@me";

/// Discord OAuth settings, from `[discord]` plus the public URL.
#[derive(Debug, Clone)]
pub struct DiscordSettings {
    /// OAuth application id.
    pub client_id: String,
    /// OAuth application secret.
    pub client_secret: String,
    /// Registered redirect URI (`<public_url>/api/v1/sr/auth/discord/callback`).
    pub redirect_uri: String,
}

type DiscordClient =
    BasicClient<EndpointSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointSet>;

/// The configured Discord OAuth client and its HTTP transport.
pub struct DiscordOAuth {
    client: DiscordClient,
    http: oauth2::reqwest::Client,
}

impl DiscordOAuth {
    /// Build the client. Redirects are not followed (no SSRF through them).
    pub fn new(s: &DiscordSettings) -> Result<Self, String> {
        let client = BasicClient::new(ClientId::new(s.client_id.clone()))
            .set_client_secret(ClientSecret::new(s.client_secret.clone()))
            .set_auth_uri(AuthUrl::new(AUTH_URL.to_owned()).map_err(|e| e.to_string())?)
            .set_token_uri(TokenUrl::new(TOKEN_URL.to_owned()).map_err(|e| e.to_string())?)
            .set_redirect_uri(RedirectUrl::new(s.redirect_uri.clone()).map_err(|e| e.to_string())?);
        let http = oauth2::reqwest::ClientBuilder::new()
            .redirect(oauth2::reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self { client, http })
    }
}

/// What a Discord round trip was started for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    /// Sign in with a linked account.
    Login,
    /// Create an account and link this Discord identity.
    Register,
    /// Attach this Discord identity to the signed-in account (its id).
    Link(i64),
}

/// A started flow, keyed by its OAuth `state`.
#[derive(Debug, Clone)]
pub struct OAuthFlow {
    intent: Intent,
    next: Option<String>,
    /// The PKCE code verifier (kept here, never sent to the browser).
    verifier: String,
}

/// A verified Discord identity waiting for registration.
#[derive(Debug, Clone)]
pub struct PendingLink {
    /// Discord user id (snowflake, as a string).
    pub discord_id: String,
    /// Display name shown on the registration form.
    pub username: String,
}

/// Short-lived server-side entries keyed by an opaque token.
pub struct Pending<V> {
    items: Mutex<HashMap<String, (V, i64)>>,
}

impl<V> Default for Pending<V> {
    fn default() -> Self {
        Self {
            items: Mutex::new(HashMap::new()),
        }
    }
}

impl<V: Clone> Pending<V> {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, (V, i64)>> {
        self.items.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Store `value` until `expires_at`; expired entries are dropped here.
    pub fn insert(&self, key: String, value: V, expires_at: i64, now: i64) {
        let mut g = self.lock();
        g.retain(|_, (_, exp)| *exp > now);
        g.insert(key, (value, expires_at));
    }

    /// Read a live entry without consuming it.
    pub fn get(&self, key: &str, now: i64) -> Option<V> {
        self.lock()
            .get(key)
            .filter(|(_, exp)| *exp > now)
            .map(|(v, _)| v.clone())
    }

    /// Consume a live entry (single use).
    pub fn take(&self, key: &str, now: i64) -> Option<V> {
        self.lock()
            .remove(key)
            .filter(|(_, exp)| *exp > now)
            .map(|(v, _)| v)
    }
}

/// Only same-origin relative paths may be used as a post-login destination.
fn safe_next(next: &str) -> Option<String> {
    let ok = next.starts_with('/')
        && !next.starts_with("//")
        && next.len() <= 512
        && !next.contains(['\\', '\r', '\n']);
    ok.then(|| next.to_owned())
}

/// `Set-Cookie` for one of the short-lived OAuth cookies (API-scoped).
fn oauth_cookie(name: &str, value: &str, secure: bool) -> String {
    format!(
        "{name}={value}; Path=/api; HttpOnly; SameSite=Lax; Max-Age={OAUTH_TTL_SECS}{}",
        if secure { "; Secure" } else { "" }
    )
}

/// `Set-Cookie` that expires one of the OAuth cookies.
pub(super) fn clear_oauth_cookie(name: &str, secure: bool) -> String {
    format!(
        "{name}=; Path=/api; HttpOnly; SameSite=Lax; Max-Age=0{}",
        if secure { "; Secure" } else { "" }
    )
}

/// A 302 to `location` that also sets `cookies`.
fn redirect_with(location: &str, cookies: Vec<String>) -> Response {
    let mut headers: Vec<(header::HeaderName, String)> = cookies
        .into_iter()
        .map(|c| (header::SET_COOKIE, c))
        .collect();
    headers.push((header::LOCATION, location.to_owned()));
    (StatusCode::FOUND, AppendHeaders(headers)).into_response()
}

/// `GET /sr/auth/discord/start?intent=login|register&next=`: redirect to Discord.
#[derive(Debug, Deserialize)]
pub struct StartQuery {
    intent: String,
    next: Option<String>,
}

/// Starts a flow: 302 to Discord's authorize page with a fresh `state`.
pub async fn start(
    State(state): State<AppState>,
    Svc { svc, ip }: Svc,
    Query(q): Query<StartQuery>,
) -> R<Response> {
    let Some(oauth) = svc.discord.as_ref() else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let intent = match q.intent.as_str() {
        "login" => Intent::Login,
        "register" => Intent::Register,
        _ => return Ok(StatusCode::BAD_REQUEST.into_response()),
    };
    let now = state.now_secs();
    svc.ip_limits
        .hit(
            "discord-start",
            &ip,
            now,
            &[super::throttle::Rule {
                limit: 20,
                secs: 600,
            }],
        )
        .map_err(|retry_after| AuthRouteError::Throttled { retry_after })?;
    let (url, cookie) = begin(
        &state,
        &svc,
        oauth,
        intent,
        q.next.as_deref().and_then(safe_next),
        now,
    )?;
    Ok(redirect_with(&url, vec![cookie]))
}

/// Records a flow and builds Discord's authorize URL (with a PKCE challenge) and the state cookie.
fn begin(
    state: &AppState,
    svc: &super::common::AuthService,
    oauth: &DiscordOAuth,
    intent: Intent,
    next: Option<String>,
    now: i64,
) -> Result<(String, String), AuthRouteError> {
    let token = SessionToken::generate()?.expose().to_owned();
    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let flow = OAuthFlow {
        intent,
        next,
        verifier: verifier.secret().clone(),
    };
    svc.discord_flows
        .insert(token.clone(), flow, now + OAUTH_TTL_SECS, now);
    let state_value = token.clone();
    let (url, _) = oauth
        .client
        .authorize_url(move || CsrfToken::new(state_value))
        .set_pkce_challenge(challenge)
        .add_scope(Scope::new("identify".to_owned()))
        .url();
    let cookie = oauth_cookie(STATE_COOKIE, &token, state.settings().secure_cookies);
    Ok((url.to_string(), cookie))
}

/// `GET /sr/auth/discord/callback?code&state`: finish the flow.
#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DiscordUser {
    id: String,
    username: String,
    global_name: Option<String>,
}

async fn fetch_user(http: &oauth2::reqwest::Client, access: &str) -> Result<DiscordUser, String> {
    let res = http
        .get(USER_URL)
        .bearer_auth(access)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("Discord answered {}", res.status()));
    }
    let body = res.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&body).map_err(|e| e.to_string())
}

/// Handles the redirect back from Discord.
pub async fn callback(
    State(state): State<AppState>,
    Svc { svc, ip: _ }: Svc,
    headers: HeaderMap,
    Query(q): Query<CallbackQuery>,
) -> R<Response> {
    let now = state.now_secs();
    let secure = state.settings().secure_cookies;
    let clear_state = clear_oauth_cookie(STATE_COOKIE, secure);
    let failed_at = |why: &str, to: &str| {
        tracing::info!(target: "auth", reason = why, "discord sign-in failed");
        redirect_with(to, vec![clear_oauth_cookie(STATE_COOKIE, secure)])
    };
    let failed = |why: &str| failed_at(why, "/login?error=discord_failed");

    let Some(oauth) = svc.discord.as_ref() else {
        return Ok(failed("disabled"));
    };
    // The state must match both the server's record and this browser's cookie.
    let flow = match (q.state.as_deref(), parse_cookie(&headers, STATE_COOKIE)) {
        (Some(s), Some(c)) if s == c => svc.discord_flows.take(s, now),
        _ => None,
    };
    let (Some(flow), Some(code)) = (flow, q.code.as_deref()) else {
        return Ok(failed("state"));
    };
    // A link attempt returns to Settings, a sign-in to the login page.
    let fail_to = match flow.intent {
        Intent::Link(_) => format!("{SETTINGS_PAGE}?error=discord_failed"),
        _ => "/login?error=discord_failed".to_owned(),
    };
    let token = match oauth
        .client
        .exchange_code(AuthorizationCode::new(code.to_owned()))
        .set_pkce_verifier(PkceCodeVerifier::new(flow.verifier.clone()))
        .request_async(&oauth.http)
        .await
    {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(error = %e, "discord token exchange failed");
            return Ok(failed_at("exchange", &fail_to));
        }
    };
    let user = match fetch_user(&oauth.http, token.access_token().secret()).await {
        Ok(u) => u,
        Err(e) => {
            tracing::warn!(error = %e, "discord profile fetch failed");
            return Ok(failed_at("profile", &fail_to));
        }
    };

    match flow.intent {
        Intent::Link(account_id) => {
            let identity_id = svc.next_id();
            let back = match svc
                .store
                .link_discord(account_id, identity_id, &user.id)
                .await?
            {
                DiscordLink::Linked => {
                    let _ = svc
                        .store
                        .audit(account_id, "auth.discord_link", Some(account_id), "", now)
                        .await;
                    SETTINGS_PAGE.to_owned()
                }
                DiscordLink::TakenByOther => format!("{SETTINGS_PAGE}?error=discord_taken"),
                DiscordLink::AlreadyLinked => format!("{SETTINGS_PAGE}?error=discord_already"),
            };
            Ok(redirect_with(&back, vec![clear_state]))
        }
        Intent::Login => {
            let account = svc
                .store
                .find_by_discord(&user.id)
                .await?
                .filter(|a| a.state == AccountState::Active);
            let Some(account) = account else {
                return Ok(redirect_with(
                    "/login?error=discord_unlinked",
                    vec![clear_state],
                ));
            };
            let has_totp = svc
                .store
                .totp_get(account.id)
                .await?
                .is_some_and(|t| t.confirmed);
            if has_totp {
                // The second factor still applies; the web app shows /login/2fa.
                let mfa = SessionToken::generate()?;
                svc.pending
                    .insert(&mfa.hash(), account.id, now + svc.cfg.mfa_ttl_secs, now);
                let cookie = mfa_cookie(&mfa, secure, svc.cfg.mfa_ttl_secs);
                return Ok(redirect_with("/login/2fa", vec![clear_state, cookie]));
            }
            let (_, cookie) = svc
                .start_session(&state, account.id, user_agent(&headers))
                .await?;
            let to = flow.next.as_deref().unwrap_or("/");
            Ok(redirect_with(to, vec![clear_state, cookie]))
        }
        Intent::Register => {
            let link = SessionToken::generate()?.expose().to_owned();
            let display = user.global_name.unwrap_or(user.username);
            svc.discord_links.insert(
                link.clone(),
                PendingLink {
                    discord_id: user.id,
                    username: display,
                },
                now + OAUTH_TTL_SECS,
                now,
            );
            Ok(redirect_with(
                "/register",
                vec![clear_state, oauth_cookie(LINK_COOKIE, &link, secure)],
            ))
        }
    }
}

/// `GET /sr/auth/discord/pending`: the Discord identity waiting for
/// registration, if any (`{pending: false}` otherwise).
pub async fn pending(
    State(state): State<AppState>,
    Svc { svc, .. }: Svc,
    headers: HeaderMap,
) -> R<Json<Value>> {
    let link = parse_cookie(&headers, LINK_COOKIE)
        .and_then(|t| svc.discord_links.get(t, state.now_secs()));
    Ok(Json(match link {
        Some(l) => json!({ "pending": true, "username": l.username }),
        None => json!({ "pending": false }),
    }))
}

/// `DELETE /sr/auth/discord/pending`: drop the pending Discord identity.
pub async fn forget(
    State(state): State<AppState>,
    Svc { svc, .. }: Svc,
    headers: HeaderMap,
) -> R<Response> {
    if let Some(t) = parse_cookie(&headers, LINK_COOKIE) {
        svc.discord_links.take(t, state.now_secs());
    }
    let clear = clear_oauth_cookie(LINK_COOKIE, state.settings().secure_cookies);
    Ok((
        StatusCode::NO_CONTENT,
        AppendHeaders([(header::SET_COOKIE, clear)]),
    )
        .into_response())
}

/// Whether Discord sign-in is configured and whether this account has an identity linked.
#[derive(Debug, Serialize, ToSchema)]
pub struct DiscordStatus {
    /// `[discord]` is configured on this server.
    pub enabled: bool,
    /// The signed-in account has a Discord identity linked.
    pub linked: bool,
}

/// `GET /sr/auth/discord`: link status for User Settings.
#[utoipa::path(
    get,
    path = "/sr/auth/discord",
    responses((status = 200, body = DiscordStatus), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn status(UserOnly(p): UserOnly, Svc { svc, .. }: Svc) -> R<Json<DiscordStatus>> {
    session_id(&p)?;
    Ok(Json(DiscordStatus {
        enabled: svc.discord.is_some(),
        linked: svc.store.discord_linked(p.user_id).await?,
    }))
}

/// The authorize URL to send the browser to.
#[derive(Debug, Serialize, ToSchema)]
pub struct DiscordLinkStart {
    /// Discord's authorize page, with the state and PKCE challenge of this attempt.
    pub url: String,
}

/// `POST /sr/auth/discord/link`: start linking a Discord identity to the signed-in account. The
/// answer sets the state cookie; the browser then goes to `url` and Discord returns to the
/// callback, which sends it back to Settings.
#[utoipa::path(
    post,
    path = "/sr/auth/discord/link",
    responses(
        (status = 200, body = DiscordLinkStart),
        (status = 403, body = ErrorBody),
        (status = 404, description = "Discord sign-in is not configured"),
        (status = 429, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn link_start(
    State(state): State<AppState>,
    UserOnly(p): UserOnly,
    Svc { svc, ip }: Svc,
) -> R<Response> {
    session_id(&p)?;
    let Some(oauth) = svc.discord.as_ref() else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let now = state.now_secs();
    svc.ip_limits
        .hit(
            "discord-start",
            &ip,
            now,
            &[super::throttle::Rule {
                limit: 20,
                secs: 600,
            }],
        )
        .map_err(|retry_after| AuthRouteError::Throttled { retry_after })?;
    if svc.store.discord_linked(p.user_id).await? {
        return Err(ApiError::with_message(
            ErrorCode::General,
            "A Discord account is already linked. Unlink it first.",
        )
        .into());
    }
    let (url, cookie) = begin(&state, &svc, oauth, Intent::Link(p.user_id), None, now)?;
    Ok((
        AppendHeaders([(header::SET_COOKIE, cookie)]),
        Json(DiscordLinkStart { url }),
    )
        .into_response())
}

/// Body of the unlink call.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UnlinkBody {
    /// The account password (re-authentication).
    password: String,
}

/// `POST /sr/auth/discord/unlink {password}`: detach the Discord identity from the account.
#[utoipa::path(
    post,
    path = "/sr/auth/discord/unlink",
    request_body = UnlinkBody,
    responses(
        (status = 204, description = "Unlinked"),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn unlink(
    State(state): State<AppState>,
    UserOnly(p): UserOnly,
    Svc { svc, .. }: Svc,
    Json(body): Json<UnlinkBody>,
) -> R<StatusCode> {
    session_id(&p)?;
    let account = current_account(&svc, p.user_id).await?;
    reauthenticate(&svc, state.now_secs(), &account, &body.password).await?;
    if !svc.store.unlink_discord(account.id).await? {
        return Err(
            ApiError::with_message(ErrorCode::General, "No Discord account is linked.").into(),
        );
    }
    let _ = svc
        .store
        .audit(
            account.id,
            "auth.discord_unlink",
            Some(account.id),
            "",
            state.now_secs(),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

/// OpenAPI fragment for linking Discord from Settings.
#[derive(OpenApi)]
#[openapi(
    paths(status, link_start, unlink),
    components(schemas(DiscordStatus, DiscordLinkStart, UnlinkBody))
)]
pub struct DiscordApiDoc;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_must_be_a_same_origin_path() {
        assert_eq!(safe_next("/lounge").as_deref(), Some("/lounge"));
        assert_eq!(safe_next("//evil.example"), None);
        assert_eq!(safe_next("https://evil.example"), None);
        assert_eq!(safe_next("/\\evil"), None);
        assert_eq!(safe_next("/a\r\nSet-Cookie: x"), None);
    }

    #[test]
    fn pending_entries_expire_and_are_single_use() {
        let p: Pending<u8> = Pending::default();
        p.insert("a".into(), 1, 100, 0);
        assert_eq!(p.get("a", 50), Some(1));
        assert_eq!(p.take("a", 50), Some(1));
        assert_eq!(p.take("a", 50), None);
        p.insert("b".into(), 2, 100, 0);
        assert_eq!(p.get("b", 100), None);
    }

    #[test]
    fn cookies_are_api_scoped_and_httponly() {
        let c = oauth_cookie(STATE_COOKIE, "x", true);
        assert!(c.contains("Path=/api"));
        assert!(c.contains("HttpOnly"));
        assert!(c.contains("Secure"));
        assert!(clear_oauth_cookie(LINK_COOKIE, false).contains("Max-Age=0"));
    }

    #[test]
    fn client_builds_with_valid_settings() {
        let s = DiscordSettings {
            client_id: "123".into(),
            client_secret: "secret".into(),
            redirect_uri: "https://chat.example.org/api/v1/sr/auth/discord/callback".into(),
        };
        assert!(DiscordOAuth::new(&s).is_ok());
    }
}
