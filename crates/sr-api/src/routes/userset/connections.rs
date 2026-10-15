//! Account connections (LNG-USET-023/024): link a GitHub, Discord, Spotify, Twitch, YouTube,
//! Reddit or Steam account to the profile, or add an unverified link for anything else.
//!
//! * `GET    /users/@me/connections`                          connections and the providers on offer
//! * `POST   /users/@me/connections/{provider}/authorize`     start a verified link: `{ "url" }`
//! * `GET    /connections/{provider}/callback`                the provider sends the browser back here
//! * `PUT    /users/@me/connections/link`                     add an unverified link
//! * `PATCH  /users/@me/connections/{provider}/{id}`          visibility, stats toggle, name of a link
//! * `DELETE /users/@me/connections/{provider}/{id}`          remove
//! * `POST   /users/@me/connections/{provider}/{id}/refresh`  re-read the public stats now
//!
//! The server runs the whole exchange, so client secrets never reach the browser. Every
//! outgoing request goes through the SSRF-safe [`sr_net::SafeClient`] (resolved addresses
//! checked and pinned, no redirects). The OAuth `state` is random, single use, ten minutes
//! old at most, and bound to the signed-in account; PKCE protects the code where the
//! provider supports it. Only the public identity (id, name, link) and a few public numbers
//! are kept: **no access or refresh token is stored**, so the periodic refresh only covers
//! providers with a token-free public endpoint (GitHub, Reddit).
//!
//! Providers that need an OAuth application stay behind operator config
//! (`[connections.<provider>]`); a provider without one is not offered. The flow state lives
//! in memory, like the Discord sign-in: single node.

use super::providers::{self, Flow, Identity, Spec};
use crate::routes::messages::error::{ApiJson, MsgError, MsgResult};
use crate::routes::social::model::ConnectionObject;
use crate::routes::social::{SocialDeps, util::actor};
use crate::routes::{Ext, MaybeExt};
use crate::{HttpError, Principal, auth::UserOnly, openapi::ErrorBody, state::AppState};
use axum::{
    Json, Router,
    extract::{Path, Query},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use data_encoding::BASE64URL_NOPAD;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sr_chat::social::SocialError;
use sr_chat::social::profile::Connection;
use sr_core::{ApiError, Clock, ErrorCode, config::ConnectionsConfig};
use sr_net::{ApiCall, ApiMethod, SafeClient};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use url::Url;
use utoipa::{OpenApi, ToSchema};

/// How long a started flow can be completed.
const FLOW_TTL_MS: i64 = 10 * 60 * 1000;
/// Most flows one account may have open.
const MAX_OPEN_FLOWS: usize = 5;
/// Most flows open at once, whoever started them.
const MAX_FLOWS: usize = 2_000;
/// Smallest gap between two manual stats refreshes of one account.
const REFRESH_GAP_MS: i64 = 60_000;
/// Response bodies from providers are small documents.
const MAX_BODY: usize = 256 * 1024;
/// Where the callback sends the browser.
const SETTINGS_PAGE: &str = "/settings/user/connections";

struct Provider {
    spec: &'static Spec,
    client_id: String,
    client_secret: String,
}

struct PendingFlow {
    user: i64,
    provider: &'static str,
    verifier: Option<String>,
    expires_ms: i64,
}

/// Collaborators of the connection routes, registered on the composition root.
pub struct ConnectionsDeps {
    providers: HashMap<&'static str, Provider>,
    public_url: String,
    client: SafeClient,
    clock: Arc<dyn Clock>,
    flows: Mutex<HashMap<String, PendingFlow>>,
    last_refresh: Mutex<HashMap<i64, i64>>,
}

impl ConnectionsDeps {
    /// `discord` is the `[discord]` sign-in application, reused for linking when
    /// `[connections.discord]` has none of its own (its redirect URI must be registered too).
    pub fn new(
        public_url: &str,
        cfg: &ConnectionsConfig,
        discord: (&str, &str),
        client: SafeClient,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let mut providers = HashMap::new();
        for spec in providers::OAUTH {
            let c = match spec.id {
                "github" => &cfg.github,
                "discord" => &cfg.discord,
                "spotify" => &cfg.spotify,
                "twitch" => &cfg.twitch,
                "youtube" => &cfg.youtube,
                "reddit" => &cfg.reddit,
                _ => &cfg.steam,
            };
            if c.enabled == Some(false) {
                continue;
            }
            let (mut id, mut secret) = (c.client_id.clone(), c.client_secret.expose().to_owned());
            if spec.id == "discord" && (id.is_empty() || secret.is_empty()) {
                id = discord.0.to_owned();
                secret = discord.1.to_owned();
            }
            let usable = match spec.flow {
                Flow::SteamOpenId => true,
                Flow::OAuth2 { .. } => !id.is_empty() && !secret.is_empty(),
            };
            if usable {
                providers.insert(
                    spec.id,
                    Provider {
                        spec,
                        client_id: id,
                        client_secret: secret,
                    },
                );
            }
        }
        Self {
            providers,
            public_url: public_url.trim_end_matches('/').to_owned(),
            client,
            clock,
            flows: Mutex::default(),
            last_refresh: Mutex::default(),
        }
    }

    fn callback_url(&self, provider: &str) -> String {
        format!("{}/api/v1/connections/{provider}/callback", self.public_url)
    }

    fn flows(&self) -> std::sync::MutexGuard<'_, HashMap<String, PendingFlow>> {
        self.flows.lock().unwrap_or_else(|p| p.into_inner())
    }
}

fn random_token() -> MsgResult<String> {
    let b = sr_auth::random::bytes::<32>().map_err(|e| {
        tracing::error!(error = %e, "no randomness for a connection flow");
        MsgError::from(ErrorCode::SrInternal)
    })?;
    Ok(BASE64URL_NOPAD.encode(&b))
}

fn unknown_provider() -> MsgError {
    ApiError::with_message(
        ErrorCode::UnknownApplication,
        "Unknown connection provider.",
    )
    .into()
}

fn unknown_connection() -> MsgError {
    ApiError::with_message(ErrorCode::UnknownApplication, "Unknown connection.").into()
}

/// The owner's view of a connection: stats are shown even when the toggle hides them from
/// others.
fn owner_object(c: &Connection) -> ConnectionObject {
    let mut o = ConnectionObject::from(c);
    o.stats = crate::routes::social::model::clean_stats(&c.stats);
    o
}

// ---- list -----------------------------------------------------------------------------------

/// A provider on offer.
#[derive(Debug, Serialize, ToSchema)]
pub struct ProviderObject {
    pub id: String,
    pub name: String,
    /// `true` when linking proves the account (OAuth / OpenID); `false` for a typed link.
    pub verified: bool,
    /// `false` when the operator has not set the provider up. Link-only providers are
    /// always available.
    pub available: bool,
    /// Link-only providers: the member must give a URL (websites).
    pub requires_url: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ConnectionsResponse {
    pub connections: Vec<ConnectionObject>,
    pub providers: Vec<ProviderObject>,
}

/// The caller's connections (hidden ones included) and the providers that can be added.
#[utoipa::path(
    get,
    path = "/users/@me/connections",
    responses((status = 200, body = ConnectionsResponse)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list(
    UserOnly(p): UserOnly,
    Ext(social): Ext<SocialDeps>,
    Ext(deps): Ext<ConnectionsDeps>,
) -> MsgResult<Json<ConnectionsResponse>> {
    let me = actor(&p)?;
    let connections = social
        .service
        .own_connections(me)
        .await?
        .iter()
        .map(owner_object)
        .collect();
    let mut providers: Vec<ProviderObject> = providers::OAUTH
        .iter()
        .map(|s| ProviderObject {
            id: s.id.to_owned(),
            name: s.name.to_owned(),
            verified: true,
            available: deps.providers.contains_key(s.id),
            requires_url: false,
        })
        .collect();
    providers.extend(
        providers::LINK_ONLY
            .iter()
            .map(|(id, name, needs_url)| ProviderObject {
                id: (*id).to_owned(),
                name: (*name).to_owned(),
                verified: false,
                available: true,
                requires_url: *needs_url,
            }),
    );
    Ok(Json(ConnectionsResponse {
        connections,
        providers,
    }))
}

// ---- start a verified link ------------------------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthorizeResponse {
    /// Send the browser here; the provider returns it to the callback, which ends in a redirect to
    /// `/settings/user/connections?connected=<provider>` or `?error=<code>`.
    pub url: String,
}

/// Start linking `provider`. Error codes on the way back: `denied`, `state`, `exchange`,
/// `profile`, `taken` (the account belongs to someone else), `limit`, `signed_out`, `failed`.
#[utoipa::path(
    post,
    path = "/users/@me/connections/{provider}/authorize",
    params(("provider" = String, Path, description = "github, discord, spotify, twitch, youtube, reddit or steam")),
    responses((status = 200, body = AuthorizeResponse), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn authorize(
    UserOnly(p): UserOnly,
    Ext(deps): Ext<ConnectionsDeps>,
    Path(provider): Path<String>,
) -> MsgResult<Json<AuthorizeResponse>> {
    actor(&p)?;
    let prov = deps
        .providers
        .get(provider.as_str())
        .ok_or_else(unknown_provider)?;
    let now = deps.clock.now_unix_ms();
    let state = random_token()?;
    let redirect_uri = deps.callback_url(prov.spec.id);

    let (url, verifier) = match prov.spec.flow {
        Flow::OAuth2 {
            auth_url,
            scopes,
            pkce,
            extra,
            ..
        } => {
            let mut u = Url::parse(auth_url).map_err(|_| MsgError::from(ErrorCode::SrInternal))?;
            let verifier = if pkce { Some(random_token()?) } else { None };
            {
                let mut q = u.query_pairs_mut();
                q.append_pair("response_type", "code")
                    .append_pair("client_id", &prov.client_id)
                    .append_pair("redirect_uri", &redirect_uri)
                    .append_pair("state", &state);
                if !scopes.is_empty() {
                    q.append_pair("scope", &scopes.join(" "));
                }
                if let Some(v) = &verifier {
                    q.append_pair(
                        "code_challenge",
                        &BASE64URL_NOPAD.encode(&Sha256::digest(v)),
                    )
                    .append_pair("code_challenge_method", "S256");
                }
                for (k, v) in extra {
                    q.append_pair(k, v);
                }
            }
            (u.to_string(), verifier)
        }
        Flow::SteamOpenId => {
            let return_to = format!("{redirect_uri}?state={state}");
            let mut u = Url::parse(providers::STEAM_OPENID)
                .map_err(|_| MsgError::from(ErrorCode::SrInternal))?;
            u.query_pairs_mut()
                .extend_pairs(providers::steam_redirect_params(
                    &deps.public_url,
                    &return_to,
                ));
            (u.to_string(), None)
        }
    };

    let mut flows = deps.flows();
    flows.retain(|_, f| f.expires_ms > now);
    let open = flows.values().filter(|f| f.user == p.user_id).count();
    if open >= MAX_OPEN_FLOWS || flows.len() >= MAX_FLOWS {
        return Err(MsgError::RetryAfter {
            api: ApiError::with_message(
                ErrorCode::TooManyRequests,
                "Too many connection attempts. Try again in a few minutes.",
            ),
            retry_after_ms: FLOW_TTL_MS,
        });
    }
    flows.insert(
        state,
        PendingFlow {
            user: p.user_id,
            provider: prov.spec.id,
            verifier,
            expires_ms: now + FLOW_TTL_MS,
        },
    );
    drop(flows);
    Ok(Json(AuthorizeResponse { url }))
}

// ---- the callback ---------------------------------------------------------------------------

fn redirect(to: &str) -> Response {
    let mut res = StatusCode::FOUND.into_response();
    if let Ok(v) = HeaderValue::from_str(to) {
        res.headers_mut().insert(header::LOCATION, v);
    }
    res.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    // Do not leak the one-time code or state through the Referer of the next page.
    res.headers_mut()
        .insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    res
}

async fn json_call(client: &SafeClient, call: ApiCall) -> Result<(u16, Value), ()> {
    let (status, body) = client.call(call).await.map_err(|e| {
        tracing::warn!(error = %e, "connection provider call failed");
    })?;
    Ok((status, serde_json::from_slice(&body).unwrap_or(Value::Null)))
}

fn call(url: &str, method: ApiMethod) -> Option<ApiCall> {
    Some(ApiCall {
        url: Url::parse(url).ok()?,
        method,
        headers: Vec::new(),
        basic: None,
        accept: "application/json",
        max_bytes: MAX_BODY,
    })
}

/// Trade the authorization code for an access token and read the account with it.
async fn oauth_identity(
    deps: &ConnectionsDeps,
    prov: &Provider,
    code: &str,
    verifier: Option<&str>,
) -> Result<Identity, &'static str> {
    let Flow::OAuth2 {
        token_url,
        basic_auth,
        ..
    } = prov.spec.flow
    else {
        return Err("failed");
    };
    let mut form = vec![
        ("grant_type".to_owned(), "authorization_code".to_owned()),
        ("code".to_owned(), code.to_owned()),
        ("redirect_uri".to_owned(), deps.callback_url(prov.spec.id)),
    ];
    if let Some(v) = verifier {
        form.push(("code_verifier".to_owned(), v.to_owned()));
    }
    let mut token_call = call(token_url, ApiMethod::PostForm(Vec::new())).ok_or("failed")?;
    if basic_auth {
        token_call.basic = Some((prov.client_id.clone(), prov.client_secret.clone()));
    } else {
        form.push(("client_id".to_owned(), prov.client_id.clone()));
        form.push(("client_secret".to_owned(), prov.client_secret.clone()));
    }
    token_call.method = ApiMethod::PostForm(form);
    let (status, body) = json_call(&deps.client, token_call)
        .await
        .map_err(|()| "exchange")?;
    let token = body["access_token"].as_str().filter(|t| !t.is_empty());
    let (true, Some(token)) = (status == 200 && body.get("error").is_none(), token) else {
        return Err("exchange");
    };

    let mut api = call(prov.spec.api_url, ApiMethod::Get).ok_or("failed")?;
    api.headers
        .push(("Authorization".into(), format!("Bearer {token}")));
    if prov.spec.client_id_header {
        api.headers
            .push(("Client-Id".into(), prov.client_id.clone()));
    }
    let (status, doc) = json_call(&deps.client, api).await.map_err(|()| "profile")?;
    if status != 200 {
        return Err("profile");
    }
    providers::parse_identity(prov.spec.id, &doc).ok_or("profile")
}

/// Check a Steam OpenID answer with Steam and look the account up.
async fn steam_identity(
    deps: &ConnectionsDeps,
    prov: &Provider,
    params: &[(String, String)],
    state: &str,
) -> Result<Identity, &'static str> {
    let return_to = format!("{}?state={state}", deps.callback_url("steam"));
    let (steam_id, form) = providers::steam_check(params, &return_to).ok_or("exchange")?;
    let verify = call(providers::STEAM_OPENID, ApiMethod::PostForm(form)).ok_or("failed")?;
    let (status, body) = deps.client.call(verify).await.map_err(|e| {
        tracing::warn!(error = %e, "Steam verification failed");
        "exchange"
    })?;
    if status != 200 || !providers::steam_is_valid(&String::from_utf8_lossy(&body)) {
        return Err("exchange");
    }
    // Without a Web API key there is no persona name; the Steam id stands in.
    if prov.client_secret.is_empty() {
        return Ok(providers::parse_steam_summary(&steam_id, &Value::Null));
    }
    let mut u = Url::parse(prov.spec.api_url).map_err(|_| "failed")?;
    u.query_pairs_mut()
        .append_pair("key", &prov.client_secret)
        .append_pair("steamids", &steam_id);
    let Some(api) = call(u.as_str(), ApiMethod::Get) else {
        return Err("failed");
    };
    let summary = json_call(&deps.client, api)
        .await
        .ok()
        .filter(|(s, _)| *s == 200)
        .map_or(Value::Null, |(_, d)| d);
    Ok(providers::parse_steam_summary(&steam_id, &summary))
}

async fn finish(
    deps: &ConnectionsDeps,
    social: &SocialDeps,
    principal: Result<Principal, HttpError>,
    provider: &str,
    q: HashMap<String, String>,
) -> Result<&'static str, &'static str> {
    let p = principal.map_err(|_| "signed_out")?;
    let state = q.get("state").ok_or("state")?;
    let now = deps.clock.now_unix_ms();
    // Single use: the state is gone whatever happens next.
    let flow = deps.flows().remove(state).ok_or("state")?;
    if flow.expires_ms <= now || flow.user != p.user_id || flow.provider != provider {
        return Err("state");
    }
    let prov = deps.providers.get(flow.provider).ok_or("state")?;
    if q.contains_key("error") {
        return Err("denied");
    }
    let identity = match prov.spec.flow {
        Flow::OAuth2 { .. } => {
            let code = q.get("code").filter(|c| !c.is_empty()).ok_or("exchange")?;
            oauth_identity(deps, prov, code, flow.verifier.as_deref()).await?
        }
        Flow::SteamOpenId => {
            let params: Vec<(String, String)> =
                q.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            steam_identity(deps, prov, &params, state).await?
        }
    };
    let me = sr_core::UserId::new(u64::try_from(p.user_id).map_err(|_| "failed")?);

    // One account per provider identity.
    match social
        .service
        .connection_owner(flow.provider, &identity.external_id)
        .await
    {
        Ok(Some(owner)) if owner != me => return Err("taken"),
        Ok(_) => {}
        Err(_) => return Err("failed"),
    }
    // Linking again keeps what the member chose for the card.
    let existing = social
        .service
        .own_connections(me)
        .await
        .map_err(|_| "failed")?
        .into_iter()
        .find(|c| c.provider == flow.provider && c.external_id == identity.external_id);
    let connection = Connection {
        user: me,
        provider: flow.provider.to_owned(),
        external_id: identity.external_id,
        display_name: identity.name,
        verified: true,
        visible: existing.as_ref().is_none_or(|c| c.visible),
        url: identity.url,
        show_activity: existing.as_ref().is_none_or(|c| c.show_activity),
        stats: identity.stats,
        created_ms: 0,
    };
    let stored = if existing.is_some() {
        social.service.update_connection(connection).await
    } else {
        social.service.upsert_verified_connection(connection).await
    };
    match stored {
        Ok(()) => Ok("connected"),
        Err(SocialError::Invalid { field, .. }) if field == "connections" => Err("limit"),
        Err(_) => Err("failed"),
    }
}

/// The provider sends the browser back here with `code` and `state` (Steam: `openid.*`).
/// Always answers with a redirect to the settings page.
pub async fn callback(
    Path(provider): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    principal: Result<Principal, HttpError>,
    Ext(deps): Ext<ConnectionsDeps>,
    MaybeExt(social): MaybeExt<SocialDeps>,
) -> Response {
    let Some(social) = social else {
        return redirect(&format!("{SETTINGS_PAGE}?error=failed"));
    };
    match finish(&deps, &social, principal, &provider, q).await {
        Ok(_) => redirect(&format!("{SETTINGS_PAGE}?connected={provider}")),
        Err(code) => {
            let provider = if providers::oauth_spec(&provider).is_some() {
                provider.as_str()
            } else {
                "unknown"
            };
            redirect(&format!("{SETTINGS_PAGE}?error={code}&provider={provider}"))
        }
    }
}

// ---- unverified links, edit, remove, refresh -------------------------------------------------

/// Body of `PUT /users/@me/connections/link`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct LinkBody {
    /// A link-only provider such as `website`, `xbox`, `roblox` or `tiktok`.
    pub provider: String,
    /// The account name shown on the profile (1 to 100 characters). For `website` it
    /// defaults to the host.
    #[serde(default)]
    pub name: Option<String>,
    /// `https://` address of the profile; required for `website`.
    #[serde(default)]
    pub url: Option<String>,
}

fn clean_https_url(raw: &str) -> Option<String> {
    let u = Url::parse(raw.trim()).ok()?;
    (u.scheme() == "https"
        && u.host_str().is_some()
        && u.username().is_empty()
        && u.password().is_none()
        && raw.len() <= 300)
        .then(|| u.to_string())
}

fn bad_field(field: &str, message: &str) -> MsgError {
    ApiError::invalid_form()
        .field(field, "invalid", message)
        .into()
}

/// Add (or update) an unverified link. It shows on the profile without a check mark.
#[utoipa::path(
    put,
    path = "/users/@me/connections/link",
    request_body = LinkBody,
    responses((status = 200, body = ConnectionObject), (status = 400, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn put_link(
    UserOnly(p): UserOnly,
    Ext(social): Ext<SocialDeps>,
    ApiJson(body): ApiJson<LinkBody>,
) -> MsgResult<Json<ConnectionObject>> {
    let me = actor(&p)?;
    let (_, _, needs_url) = providers::link_spec(&body.provider)
        .ok_or_else(|| bad_field("provider", "This provider cannot be added by hand."))?;
    let url = match body.url.as_deref().filter(|u| !u.trim().is_empty()) {
        Some(u) => Some(clean_https_url(u).ok_or_else(|| {
            bad_field(
                "url",
                "Use a full https:// address (300 characters at most).",
            )
        })?),
        None if *needs_url => return Err(bad_field("url", "A website needs an address.")),
        None => None,
    };
    let name = match body
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        Some(n) => n.to_owned(),
        None => url
            .as_deref()
            .and_then(|u| Url::parse(u).ok())
            .and_then(|u| u.host_str().map(str::to_owned))
            .ok_or_else(|| bad_field("name", "Give the account name."))?,
    };
    if name.chars().count() > 100 || name.chars().any(char::is_control) {
        return Err(bad_field(
            "name",
            "1 to 100 characters, no control characters.",
        ));
    }
    let connection = Connection {
        user: me,
        provider: body.provider,
        external_id: name.chars().take(128).collect(),
        display_name: name,
        verified: false,
        visible: true,
        url,
        show_activity: false,
        stats: json!({}),
        created_ms: 0,
    };
    social.service.upsert_connection(connection.clone()).await?;
    let stored = social
        .service
        .own_connections(me)
        .await?
        .into_iter()
        .find(|c| c.provider == connection.provider && c.external_id == connection.external_id)
        .ok_or_else(unknown_connection)?;
    Ok(Json(owner_object(&stored)))
}

/// Body of `PATCH /users/@me/connections/{provider}/{id}`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PatchBody {
    /// Show the connection on the profile.
    #[serde(default)]
    pub visible: Option<bool>,
    /// Show the stats line (follower counts and the like).
    #[serde(default)]
    pub show_activity: Option<bool>,
    /// New display name; only for unverified links.
    #[serde(default)]
    pub name: Option<String>,
}

async fn find(
    social: &SocialDeps,
    me: sr_core::UserId,
    provider: &str,
    id: &str,
) -> MsgResult<Connection> {
    social
        .service
        .own_connections(me)
        .await?
        .into_iter()
        .find(|c| c.provider == provider && c.external_id == id)
        .ok_or_else(unknown_connection)
}

/// Change how a connection shows on the profile.
#[utoipa::path(
    patch,
    path = "/users/@me/connections/{provider}/{id}",
    params(("provider" = String, Path, description = "Provider"), ("id" = String, Path, description = "Account id at the provider")),
    request_body = PatchBody,
    responses((status = 200, body = ConnectionObject), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn patch(
    UserOnly(p): UserOnly,
    Ext(social): Ext<SocialDeps>,
    Path((provider, id)): Path<(String, String)>,
    ApiJson(body): ApiJson<PatchBody>,
) -> MsgResult<Json<ConnectionObject>> {
    let me = actor(&p)?;
    let mut c = find(&social, me, &provider, &id).await?;
    if let Some(v) = body.visible {
        c.visible = v;
    }
    if let Some(v) = body.show_activity {
        c.show_activity = v;
    }
    if let Some(name) = body.name.as_deref().map(str::trim) {
        if c.verified {
            return Err(bad_field(
                "name",
                "The name of a verified account comes from the provider.",
            ));
        }
        if name.is_empty() || name.chars().count() > 100 || name.chars().any(char::is_control) {
            return Err(bad_field(
                "name",
                "1 to 100 characters, no control characters.",
            ));
        }
        name.clone_into(&mut c.display_name);
    }
    social.service.update_connection(c.clone()).await?;
    Ok(Json(owner_object(&c)))
}

/// Remove a connection.
#[utoipa::path(
    delete,
    path = "/users/@me/connections/{provider}/{id}",
    params(("provider" = String, Path, description = "Provider"), ("id" = String, Path, description = "Account id at the provider")),
    responses((status = 204, description = "Removed"), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn remove(
    UserOnly(p): UserOnly,
    Ext(social): Ext<SocialDeps>,
    Path((provider, id)): Path<(String, String)>,
) -> MsgResult<StatusCode> {
    let me = actor(&p)?;
    find(&social, me, &provider, &id).await?;
    social.service.remove_connection(me, &provider, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Read the public stats of a stored connection again, without a token. `None` when the
/// provider has no token-free endpoint or the answer is unusable.
pub async fn refresh_stats(client: &SafeClient, c: &Connection) -> Option<Value> {
    let url = providers::refresh_url(&c.provider, &c.external_id, &c.display_name)?;
    let (status, doc) = json_call(client, call(&url, ApiMethod::Get)?).await.ok()?;
    (status == 200)
        .then(|| providers::parse_refresh(&c.provider, &doc))
        .flatten()
}

/// Providers whose stats can be refreshed without a token.
pub const REFRESHABLE: &[&str] = &["github", "reddit"];

/// Re-read the stats now (GitHub and Reddit; others keep what was read when linking).
/// At most once a minute per account.
#[utoipa::path(
    post,
    path = "/users/@me/connections/{provider}/{id}/refresh",
    params(("provider" = String, Path, description = "Provider"), ("id" = String, Path, description = "Account id at the provider")),
    responses((status = 200, body = ConnectionObject), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody), (status = 429, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn refresh(
    UserOnly(p): UserOnly,
    Ext(social): Ext<SocialDeps>,
    Ext(deps): Ext<ConnectionsDeps>,
    Path((provider, id)): Path<(String, String)>,
) -> MsgResult<Json<ConnectionObject>> {
    let me = actor(&p)?;
    let mut c = find(&social, me, &provider, &id).await?;
    if !c.verified || !REFRESHABLE.contains(&provider.as_str()) {
        return Err(bad_field(
            "provider",
            "This connection's stats are read when you link the account.",
        ));
    }
    let now = deps.clock.now_unix_ms();
    {
        let mut last = deps.last_refresh.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(at) = last
            .get(&p.user_id)
            .filter(|at| now - **at < REFRESH_GAP_MS)
        {
            return Err(MsgError::RetryAfter {
                api: ApiError::with_message(
                    ErrorCode::TooManyRequests,
                    "Wait a minute before refreshing again.",
                ),
                retry_after_ms: REFRESH_GAP_MS - (now - at),
            });
        }
        last.retain(|_, at| now - *at < REFRESH_GAP_MS);
        last.insert(p.user_id, now);
    }
    let stats = refresh_stats(&deps.client, &c).await.ok_or_else(|| {
        MsgError::from(ApiError::with_message(
            ErrorCode::ApiResourceOverloaded,
            "The provider did not answer. Try again later.",
        ))
    })?;
    social.service.refresh_connection_stats(&c, &stats).await?;
    c.stats = stats;
    Ok(Json(owner_object(&c)))
}

/// The connection routes (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/users/@me/connections", get(list))
        .route("/users/@me/connections/link", put(put_link))
        .route(
            "/users/@me/connections/{provider}/authorize",
            post(authorize),
        )
        .route(
            "/users/@me/connections/{provider}/{id}",
            axum::routing::patch(patch).delete(remove),
        )
        .route(
            "/users/@me/connections/{provider}/{id}/refresh",
            post(refresh),
        )
        .route("/connections/{provider}/callback", get(callback))
}

/// OpenAPI fragment.
#[derive(OpenApi)]
#[openapi(
    paths(list, authorize, put_link, patch, remove, refresh),
    components(schemas(
        ConnectionsResponse,
        ProviderObject,
        AuthorizeResponse,
        LinkBody,
        PatchBody,
        ConnectionObject
    ))
)]
pub struct ConnectionsApiDoc;
