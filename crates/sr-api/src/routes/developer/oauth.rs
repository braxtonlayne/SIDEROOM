//! OAuth2: the authorize screen API, token, revoke, introspect, `@me`, authorized apps.

use super::{
    ApplicationObject, AuthorizeBody, AuthorizeInfo, AuthorizeResult, AuthorizedApp,
    AuthorizedAppInfo, DevDeps, GuildChoice, IntrospectionBody, OAuthMe, OwnerObject, ScopeObject,
    TokenRequestBody, TokenResponseBody, app_err, dto::AuthorizeQuery, invalid, iso, parse_id,
};
use crate::{
    auth::{AuthKind, Principal, SessionOnly},
    openapi::ErrorBody,
    rate_limit::Decision,
    routes::{
        Ext, MaybeExt,
        activities::ActivitiesDeps,
        guilds::dto::{MemberObject, RoleObject},
        messages::error::{ApiJson, ApiQuery, MsgError, MsgResult},
    },
    state::AppState,
};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use sr_apps::{
    AppsError, OAuthError,
    model::GuildStats,
    perms,
    service::{AuthorizeParams, ClientCredentials, InstallRequest, ValidatedAuthorize},
};
use sr_auth::oauth::{Scope, scope_names};
use sr_core::{ApiError, ErrorCode, GuildId, Permissions, RoleId, UserId};
use sr_events::DomainEvent;
use std::collections::HashMap;

fn params_of(q: &AuthorizeQuery) -> AuthorizeParams {
    AuthorizeParams {
        client_id: q.client_id.clone(),
        scope: q.scope.clone(),
        redirect_uri: q.redirect_uri.clone(),
        response_type: q.response_type.clone(),
        state: q.state.clone(),
        code_challenge: q.code_challenge.clone(),
        code_challenge_method: q.code_challenge_method.clone(),
        permissions: q.permissions.clone(),
        guild_id: q.guild_id.clone(),
    }
}

/// The redirect URI an Activity gets without registering it (J-13, D-248): `<activity origin>/` for an
/// application that has an Activity, while Activities are enabled on this server.
async fn implicit_redirects(activities: Option<&ActivitiesDeps>, client_id: &str) -> Vec<String> {
    let (Some(a), Ok(app)) = (activities, client_id.parse::<i64>()) else {
        return Vec::new();
    };
    if !a.enabled || app <= 0 {
        return Vec::new();
    }
    match a.service.has_activity(app).await {
        Ok(true) => vec![format!("{}/", a.activity_origin(app))],
        _ => Vec::new(),
    }
}

fn reason_for(e: &AppsError) -> &'static str {
    match e {
        AppsError::MissingPermissions => "missing_permission",
        AppsError::NotReleased => "app_not_released",
        AppsError::Forbidden(_) => "bot_not_public",
        AppsError::Suspended => "suspended",
        _ => "unavailable",
    }
}

/// What the consent screen shows. An invalid client, redirect URI or scope is a 400 with a
/// field error, never a redirect.
#[utoipa::path(
    get, path = "/oauth2/authorize", tag = "oauth2", params(AuthorizeQuery),
    responses((status = 200, body = AuthorizeInfo), (status = 400, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn authorize_info(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
    MaybeExt(activities): MaybeExt<ActivitiesDeps>,
    ApiQuery(q): ApiQuery<AuthorizeQuery>,
) -> MsgResult<Json<AuthorizeInfo>> {
    let implicit = implicit_redirects(activities.as_deref(), &q.client_id).await;
    let v = deps
        .apps
        .validate_authorize_with(&params_of(&q), &implicit)
        .await
        .map_err(app_err)?;
    let ids = [v.app.owner_id, v.app.bot_user_id, p.user_id];
    let users = deps.users(&ids).await?;
    let app_obj = ApplicationObject::new(&v.app, &users, false, false);
    let me = users.get(&UserId::new(p.user_id as u64));
    let mut guilds = Vec::new();
    if v.wants_install() {
        let is_tester = deps
            .apps
            .is_tester(v.app.id, p.user_id)
            .await
            .map_err(app_err)?;
        for g in deps.apps.user_guilds(p.user_id).await.map_err(app_err)? {
            let Some(acc) = deps.guild_access(p.user_id, g.id).await? else {
                continue;
            };
            let stats = GuildStats {
                owner_id: g.owner_id,
                member_count: g.member_count,
            };
            let verdict = perms::check_install(&v.app, p.user_id, acc.perms, is_tester, stats);
            guilds.push(GuildChoice {
                id: g.id.to_string(),
                name: g.name,
                icon: g.icon_key,
                owner: g.owner_id == p.user_id,
                permissions: acc.perms.bits().to_string(),
                can_install: verdict.is_ok(),
                reason: verdict.err().as_ref().map(|e| reason_for(e).to_owned()),
            });
        }
    }
    let already_authorized = deps
        .apps
        .grants_of(p.user_id)
        .await
        .map_err(app_err)?
        .iter()
        .any(|(_, a)| a.id == v.app.id);
    Ok(Json(AuthorizeInfo {
        bot: v.wants_bot().then(|| app_obj.bot.clone()),
        application: app_obj,
        scopes: v
            .scopes
            .iter()
            .map(|s| ScopeObject {
                name: s.name().to_owned(),
                description: s.description().to_owned(),
            })
            .collect(),
        redirect_uri: v.redirect_uri.clone(),
        state: v.state.clone(),
        user: OwnerObject {
            id: p.user_id.to_string(),
            username: me.map(|u| u.username.clone()),
            global_name: me.and_then(|u| u.display_name.clone()),
            bot: false,
        },
        guilds,
        permissions: v.permissions.to_string(),
        already_authorized,
    }))
}

/// Append query parameters to a validated redirect URI.
fn with_query(base: &str, pairs: &[(&str, &str)]) -> String {
    match url::Url::parse(base) {
        Ok(mut u) => {
            {
                let mut q = u.query_pairs_mut();
                for (k, v) in pairs {
                    q.append_pair(k, v);
                }
            }
            u.to_string()
        }
        Err(_) => base.to_owned(),
    }
}

/// Install the application and announce the new member and role.
async fn install(deps: &DevDeps, user: i64, v: &ValidatedAuthorize) -> MsgResult<bool> {
    let guild = v.guild_id.ok_or_else(|| {
        invalid(
            "guild_id",
            "BASE_TYPE_REQUIRED",
            "A server is required to add this application.",
        )
    })?;
    // The installer must be a member who holds MANAGE_GUILD (or owns the server): their own
    // permissions also cap what the bot's role may receive.
    let acc = deps
        .guild_access(user, guild)
        .await?
        .ok_or_else(|| MsgError::Api(ApiError::new(ErrorCode::MissingAccess)))?;
    let out = deps
        .apps
        .install(InstallRequest {
            application_id: v.app.id,
            installer: user,
            installer_perms: acc.perms,
            guild_id: guild,
            scopes: v.scopes.clone(),
            requested_permissions: v.permissions,
        })
        .await
        .map_err(app_err)?;
    let gid = GuildId::new(guild as u64);
    if let Some((role_id, _, _)) = &out.role {
        let roles = deps.chat().list_roles(gid).await.map_err(super::re)?;
        if let Some(role) = roles.iter().find(|r| r.id.get() as i64 == *role_id) {
            let payload = json!({"guild_id": gid, "role": RoleObject::from(role)});
            let rid = RoleId::new(*role_id as u64);
            deps.publish(if out.role_created {
                DomainEvent::RoleCreated {
                    guild: gid,
                    role: rid,
                    payload,
                }
            } else {
                DomainEvent::RoleUpdated {
                    guild: gid,
                    role: rid,
                    payload,
                }
            })
            .await;
        }
    }
    if out.member_added {
        let bot = UserId::new(v.app.bot_user_id as u64);
        if let Some(member) = deps.chat().get_member(gid, bot).await.map_err(super::re)? {
            let users: HashMap<_, _> = deps.users(&[v.app.bot_user_id]).await?;
            let payload =
                serde_json::to_value(MemberObject::new(&member, true, &users)).unwrap_or_default();
            deps.publish(DomainEvent::MemberJoined {
                guild: gid,
                user: bot,
                payload,
            })
            .await;
        }
        if let Some(mut entry) = out.audit_entry.clone() {
            entry["guild_id"] = json!(gid);
            deps.publish(DomainEvent::AuditLogEntryCreated {
                guild: gid,
                payload: entry,
            })
            .await;
        }
    }
    Ok(v.wants_bot() && !out.member_added)
}

/// Approve or deny an authorization request. Returns where to send the browser.
#[utoipa::path(
    post, path = "/oauth2/authorize", tag = "oauth2", request_body = AuthorizeBody,
    responses((status = 200, body = AuthorizeResult), (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn authorize(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
    MaybeExt(activities): MaybeExt<ActivitiesDeps>,
    ApiJson(body): ApiJson<AuthorizeBody>,
) -> MsgResult<Json<AuthorizeResult>> {
    let params = AuthorizeParams {
        client_id: body.client_id,
        scope: body.scope,
        redirect_uri: body.redirect_uri,
        response_type: body.response_type,
        state: body.state,
        code_challenge: body.code_challenge,
        code_challenge_method: body.code_challenge_method,
        permissions: body.permissions,
        guild_id: body.guild_id,
    };
    let implicit = implicit_redirects(activities.as_deref(), &params.client_id).await;
    let v = deps
        .apps
        .validate_authorize_with(&params, &implicit)
        .await
        .map_err(app_err)?;
    let state_pair = v.state.clone();
    let mut extra: Vec<(&str, String)> = Vec::new();
    if let Some(s) = &state_pair {
        extra.push(("state", s.clone()));
    }
    let done = format!("{}/oauth2/authorized", deps.public_url());
    if !body.authorize {
        let location = match &v.redirect_uri {
            Some(r) => {
                let mut pairs: Vec<(&str, &str)> = vec![("error", "access_denied")];
                pairs.extend(extra.iter().map(|(k, v)| (*k, v.as_str())));
                with_query(r, &pairs)
            }
            None => format!("{done}?error=access_denied"),
        };
        return Ok(Json(AuthorizeResult {
            location,
            already_installed: None,
        }));
    }
    let mut already = None;
    if v.wants_install() {
        let was_member = install(&deps, p.user_id, &v).await?;
        already = v.wants_bot().then_some(was_member);
    }
    let guild = v.guild_id.map(|g| g.to_string());
    let location = match (&v.redirect_uri, v.wants_code()) {
        (Some(r), true) => {
            let code = deps.apps.issue_code(p.user_id, &v).await.map_err(app_err)?;
            let mut pairs: Vec<(&str, &str)> = vec![("code", code.as_str())];
            pairs.extend(extra.iter().map(|(k, v)| (*k, v.as_str())));
            if let Some(g) = &guild {
                pairs.push(("guild_id", g));
            }
            with_query(r, &pairs)
        }
        (Some(r), false) => {
            let mut pairs: Vec<(&str, &str)> = Vec::new();
            pairs.extend(extra.iter().map(|(k, v)| (*k, v.as_str())));
            if let Some(g) = &guild {
                pairs.push(("guild_id", g));
            }
            with_query(r, &pairs)
        }
        (None, _) => done,
    };
    Ok(Json(AuthorizeResult {
        location,
        already_installed: already,
    }))
}

// ------------------------------------------------------------------ token endpoints

/// An RFC 6749 error response.
pub struct OAuthFail(pub OAuthError, pub bool);

impl IntoResponse for OAuthFail {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.0.status()).unwrap_or(StatusCode::BAD_REQUEST);
        let mut resp = (
            status,
            Json(json!({"error": self.0.error, "error_description": self.0.description})),
        )
            .into_response();
        let h = resp.headers_mut();
        h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        h.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
        if status == StatusCode::UNAUTHORIZED && self.1 {
            h.insert(
                header::WWW_AUTHENTICATE,
                HeaderValue::from_static("Basic realm=\"oauth2\""),
            );
        }
        resp
    }
}

fn no_store(mut r: Response) -> Response {
    let h = r.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        HeaderName::from_static("pragma"),
        HeaderValue::from_static("no-cache"),
    );
    r
}

fn form_decode(s: &str) -> String {
    url::form_urlencoded::parse(format!("v={s}").as_bytes())
        .next()
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default()
}

/// The request body (JSON or `application/x-www-form-urlencoded`).
fn parse_body(headers: &HeaderMap, body: &Bytes) -> Result<TokenRequestBody, OAuthError> {
    let is_json = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|c| c.starts_with("application/json"));
    if is_json {
        return serde_json::from_slice(body).map_err(|_| {
            OAuthError::new("invalid_request", "The request body is not valid JSON.")
        });
    }
    let mut out = TokenRequestBody::default();
    for (k, v) in url::form_urlencoded::parse(body) {
        let v = v.into_owned();
        match k.as_ref() {
            "grant_type" => out.grant_type = Some(v),
            "code" => out.code = Some(v),
            "redirect_uri" => out.redirect_uri = Some(v),
            "code_verifier" => out.code_verifier = Some(v),
            "refresh_token" => out.refresh_token = Some(v),
            "client_id" => out.client_id = Some(v),
            "client_secret" => out.client_secret = Some(v),
            "token" => out.token = Some(v),
            "token_type_hint" => out.token_type_hint = Some(v),
            _ => {}
        }
    }
    Ok(out)
}

/// Client credentials: HTTP Basic, or the body, never both. `true` when Basic was used.
fn credentials(
    headers: &HeaderMap,
    b: &TokenRequestBody,
) -> Result<(ClientCredentials, bool), OAuthError> {
    let basic = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("basic"))
        .map(|(_, rest)| rest.trim());
    match basic {
        Some(raw) => {
            if b.client_id.is_some() || b.client_secret.is_some() {
                return Err(OAuthError::new(
                    "invalid_request",
                    "Use one client authentication method.",
                ));
            }
            let bad = || OAuthError::new("invalid_client", "Malformed Basic credentials.");
            let decoded = data_encoding::BASE64
                .decode(raw.as_bytes())
                .map_err(|_| bad())?;
            let text = String::from_utf8(decoded).map_err(|_| bad())?;
            let (id, secret) = text.split_once(':').ok_or_else(bad)?;
            Ok((
                ClientCredentials {
                    client_id: form_decode(id),
                    secret: Some(form_decode(secret)).filter(|s| !s.is_empty()),
                },
                true,
            ))
        }
        None => {
            let id = b
                .client_id
                .clone()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| OAuthError::new("invalid_request", "client_id is required."))?;
            Ok((
                ClientCredentials {
                    client_id: id,
                    secret: b.client_secret.clone().filter(|s| !s.is_empty()),
                },
                false,
            ))
        }
    }
}

/// Per-client and per-address limits of the token endpoints.
fn limited(state: &AppState, route: &str, client: &str, ip: &str) -> Option<Response> {
    for (key, resource, who) in [
        (
            format!("{route} (client)"),
            client.to_owned(),
            "oauth-client".to_owned(),
        ),
        (
            format!("{route} (address)"),
            String::new(),
            format!("ip:{ip}"),
        ),
    ] {
        if let Decision::Limited { retry_after, .. } =
            state.rate_limiter().check(&key, &resource, &who)
        {
            let secs = retry_after.max(0.0);
            let mut resp = (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({
                    "code": ErrorCode::TooManyRequests.code(),
                    "message": "You are being rate limited.",
                    "retry_after": secs,
                    "global": false,
                })),
            )
                .into_response();
            if let Ok(v) = HeaderValue::from_str(&(secs.ceil() as u64).to_string()) {
                resp.headers_mut().insert(header::RETRY_AFTER, v);
            }
            return Some(no_store(resp));
        }
    }
    None
}

/// Read the request: headers, the (small) body and the caller address the limiters use.
#[allow(clippy::result_large_err)]
async fn read_request(
    state: &AppState,
    req: axum::extract::Request,
) -> Result<(HeaderMap, Bytes, String), Response> {
    let ip = crate::rate_limit::client_ip(&req, state.settings().trust_forwarded_for)
        .map_or_else(|| "unknown".to_owned(), |i| i.to_string());
    let (parts, body) = req.into_parts();
    let bytes = axum::body::to_bytes(body, 64 * 1024).await.map_err(|_| {
        OAuthFail(
            OAuthError::new("invalid_request", "The request body is too large."),
            false,
        )
        .into_response()
    })?;
    Ok((parts.headers, bytes, ip))
}

/// Exchange a code or a refresh token for tokens.
#[utoipa::path(
    post, path = "/oauth2/token", tag = "oauth2", request_body = TokenRequestBody,
    responses((status = 200, body = TokenResponseBody), (status = 400, body = super::OAuthErrorBody),
        (status = 401, body = super::OAuthErrorBody))
)]
pub async fn token(
    State(state): State<AppState>,
    Ext(deps): Ext<DevDeps>,
    req: axum::extract::Request,
) -> Response {
    let (headers, body, ip) = match read_request(&state, req).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let form = match parse_body(&headers, &body) {
        Ok(f) => f,
        Err(e) => return OAuthFail(e, false).into_response(),
    };
    let (creds, basic) = match credentials(&headers, &form) {
        Ok(c) => c,
        Err(e) => return OAuthFail(e, headers.contains_key(header::AUTHORIZATION)).into_response(),
    };
    if let Some(r) = limited(&state, "POST /oauth2/token", &creds.client_id, &ip) {
        return r;
    }
    let result = match form.grant_type.as_deref() {
        Some("authorization_code") => match (&form.code, &form.redirect_uri) {
            (Some(code), Some(uri)) => {
                deps.apps
                    .exchange_code(&creds, code, uri, form.code_verifier.as_deref())
                    .await
            }
            _ => Err(OAuthError::new(
                "invalid_request",
                "code and redirect_uri are required.",
            )),
        },
        Some("refresh_token") => match &form.refresh_token {
            Some(t) => deps.apps.refresh(&creds, t).await,
            None => Err(OAuthError::new(
                "invalid_request",
                "refresh_token is required.",
            )),
        },
        Some(_) => Err(OAuthError::new(
            "unsupported_grant_type",
            "Supported grant types: authorization_code, refresh_token.",
        )),
        None => Err(OAuthError::new(
            "invalid_request",
            "grant_type is required.",
        )),
    };
    match result {
        Ok(t) => no_store(
            Json(TokenResponseBody {
                access_token: t.access_token,
                token_type: "Bearer".into(),
                expires_in: t.expires_in,
                refresh_token: t.refresh_token,
                scope: t.scope.join(" "),
            })
            .into_response(),
        ),
        Err(e) => OAuthFail(e, basic).into_response(),
    }
}

/// Revoke an access or refresh token (RFC 7009). Always 200 for an authenticated client.
#[utoipa::path(
    post, path = "/oauth2/token/revoke", tag = "oauth2", request_body = TokenRequestBody,
    responses((status = 200), (status = 401, body = super::OAuthErrorBody))
)]
pub async fn revoke(
    State(state): State<AppState>,
    Ext(deps): Ext<DevDeps>,
    req: axum::extract::Request,
) -> Response {
    let (headers, body, ip) = match read_request(&state, req).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let form = match parse_body(&headers, &body) {
        Ok(f) => f,
        Err(e) => return OAuthFail(e, false).into_response(),
    };
    let (creds, basic) = match credentials(&headers, &form) {
        Ok(c) => c,
        Err(e) => return OAuthFail(e, headers.contains_key(header::AUTHORIZATION)).into_response(),
    };
    if let Some(r) = limited(&state, "POST /oauth2/token/revoke", &creds.client_id, &ip) {
        return r;
    }
    let Some(token) = form.token.as_deref() else {
        return OAuthFail(
            OAuthError::new("invalid_request", "token is required."),
            basic,
        )
        .into_response();
    };
    match deps
        .apps
        .revoke_token(&creds, token, form.token_type_hint.as_deref())
        .await
    {
        Ok(()) => no_store(Json(json!({})).into_response()),
        Err(e) => OAuthFail(e, basic).into_response(),
    }
}

/// Token introspection (RFC 7662) for the client the token was issued to.
#[utoipa::path(
    post, path = "/oauth2/introspect", tag = "oauth2", request_body = TokenRequestBody,
    responses((status = 200, body = IntrospectionBody), (status = 401, body = super::OAuthErrorBody))
)]
pub async fn introspect(
    State(state): State<AppState>,
    Ext(deps): Ext<DevDeps>,
    req: axum::extract::Request,
) -> Response {
    let (headers, body, ip) = match read_request(&state, req).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let form = match parse_body(&headers, &body) {
        Ok(f) => f,
        Err(e) => return OAuthFail(e, false).into_response(),
    };
    let (creds, basic) = match credentials(&headers, &form) {
        Ok(c) => c,
        Err(e) => return OAuthFail(e, headers.contains_key(header::AUTHORIZATION)).into_response(),
    };
    if let Some(r) = limited(&state, "POST /oauth2/introspect", &creds.client_id, &ip) {
        return r;
    }
    let Some(token) = form.token.as_deref() else {
        return OAuthFail(
            OAuthError::new("invalid_request", "token is required."),
            basic,
        )
        .into_response();
    };
    match deps
        .apps
        .introspect(&creds, token, form.token_type_hint.as_deref())
        .await
    {
        Ok(i) => {
            let out = match i.details {
                Some(d) if i.active => IntrospectionBody {
                    active: true,
                    scope: Some(d.scope.join(" ")),
                    client_id: Some(d.client_id.to_string()),
                    username: d.username,
                    sub: Some(d.sub.to_string()),
                    exp: Some(d.exp),
                    iat: Some(d.iat),
                    token_type: Some("Bearer".into()),
                },
                _ => IntrospectionBody {
                    active: false,
                    scope: None,
                    client_id: None,
                    username: None,
                    sub: None,
                    exp: None,
                    iat: None,
                    token_type: None,
                },
            };
            no_store(Json(out).into_response())
        }
        Err(e) => OAuthFail(e, basic).into_response(),
    }
}

/// The token's application, scopes, expiry and (with `identify`) user.
#[utoipa::path(
    get, path = "/oauth2/@me", tag = "oauth2",
    responses((status = 200, body = OAuthMe), (status = 401, body = ErrorBody)),
    security(("authorization" = []))
)]
pub async fn me(
    p: Principal,
    headers: HeaderMap,
    Ext(deps): Ext<DevDeps>,
    MaybeExt(auth): MaybeExt<crate::routes::auth::AuthService>,
) -> MsgResult<Json<OAuthMe>> {
    let AuthKind::Bearer { scopes, .. } = &p.kind else {
        return Err(MsgError::Api(ApiError::with_message(
            ErrorCode::MissingAccess,
            "This endpoint needs an OAuth2 access token.",
        )));
    };
    let raw = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split_once(' '))
        .map(|(_, t)| t.trim().to_owned())
        .unwrap_or_default();
    let (rec, app) = deps
        .apps
        .bearer(&raw)
        .await
        .map_err(app_err)?
        .ok_or_else(|| MsgError::Api(ApiError::new(ErrorCode::InvalidAuthenticationToken)))?;
    let users = deps
        .users(&[app.owner_id, app.bot_user_id, p.user_id])
        .await?;
    let mut user = None;
    if scopes.iter().any(|s| s == "identify") {
        let u = users.get(&UserId::new(p.user_id as u64));
        let mut obj = json!({
            "id": p.user_id.to_string(),
            "username": u.map(|u| u.username.clone()),
            "global_name": u.and_then(|u| u.display_name.clone()),
            "avatar": u.and_then(|u| u.avatar_key.clone()),
            "bot": false,
        });
        if scopes.iter().any(|s| s == "email")
            && let Some(auth) = auth
            && let Ok(Some((account, _))) = auth.account_overview(p.user_id).await
        {
            obj["email"] = json!(account.email);
            obj["verified"] = json!(account.email_verified);
        }
        user = Some(obj);
    }
    Ok(Json(OAuthMe {
        application: ApplicationObject::new(&app, &users, false, false),
        scopes: scopes.clone(),
        expires: iso(rec.access_expires_at_ms),
        user,
    }))
}

/// Applications the user authorized.
#[utoipa::path(
    get, path = "/oauth2/tokens", tag = "oauth2",
    responses((status = 200, body = Vec<AuthorizedApp>)),
    security(("session" = []))
)]
pub async fn list_grants(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
) -> MsgResult<Json<Vec<AuthorizedApp>>> {
    let rows = deps.apps.grants_of(p.user_id).await.map_err(app_err)?;
    Ok(Json(
        rows.iter()
            .map(|(g, a)| AuthorizedApp {
                id: g.id.to_string(),
                application: AuthorizedAppInfo {
                    id: a.id.to_string(),
                    name: a.name.clone(),
                    icon: a.icon_key.clone(),
                },
                scopes: g.scopes.clone(),
                created_at: iso(g.created_at_ms),
            })
            .collect(),
    ))
}

/// Revoke a grant and every token of it.
#[utoipa::path(
    delete, path = "/oauth2/tokens/{grant_id}", tag = "oauth2",
    params(("grant_id" = String, Path, description = "Grant snowflake")),
    responses((status = 204), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn revoke_grant(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
    Path(id): Path<String>,
) -> MsgResult<StatusCode> {
    let id = parse_id(&id, ErrorCode::SrNotFound)?;
    deps.apps
        .revoke_grant(p.user_id, id)
        .await
        .map_err(app_err)?;
    Ok(StatusCode::NO_CONTENT)
}

#[allow(dead_code)]
fn _keep(_: Permissions, _: Value, _: &[Scope]) {
    let _ = scope_names;
}
