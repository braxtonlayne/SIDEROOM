//! OAuth2 authorization code flow with PKCE (DEV-APP-007).

use super::AppsService;
use crate::{
    error::{AppsError, AppsResult, FieldProblem, OAuthError},
    model::*,
    repo::{CodeConsume, RefreshLookup},
};
use sr_auth::oauth::{
    self as authoauth, OpaqueKind, OpaqueToken, Scope, hash_presented, parse_scopes, scope_names,
};

/// Access token lifetime.
pub const ACCESS_TTL_SECS: i64 = 7 * 24 * 3600;
/// Refresh token lifetime.
pub const REFRESH_TTL_SECS: i64 = 90 * 24 * 3600;
/// Authorization code lifetime.
pub const CODE_TTL_SECS: i64 = 300;

/// Query/body of the authorize endpoints.
#[derive(Debug, Clone, Default)]
pub struct AuthorizeParams {
    /// Client (application) id.
    pub client_id: String,
    /// Space-separated scopes.
    pub scope: String,
    /// Redirect URI.
    pub redirect_uri: Option<String>,
    /// Must be `code` when present.
    pub response_type: Option<String>,
    /// Opaque state echoed back.
    pub state: Option<String>,
    /// PKCE challenge.
    pub code_challenge: Option<String>,
    /// PKCE method (`S256`).
    pub code_challenge_method: Option<String>,
    /// Requested bot permissions.
    pub permissions: Option<String>,
    /// Pre-selected guild.
    pub guild_id: Option<String>,
}

/// An authorize request that passed validation.
#[derive(Debug, Clone)]
pub struct ValidatedAuthorize {
    /// The application.
    pub app: Application,
    /// Requested scopes.
    pub scopes: Vec<Scope>,
    /// Where to send the browser (registered, exact).
    pub redirect_uri: Option<String>,
    /// State to echo.
    pub state: Option<String>,
    /// PKCE challenge.
    pub code_challenge: Option<String>,
    /// Requested permission bits.
    pub permissions: u64,
    /// Pre-selected guild.
    pub guild_id: Option<i64>,
}

impl ValidatedAuthorize {
    /// Does this request add a bot?
    pub fn wants_bot(&self) -> bool {
        self.scopes.contains(&Scope::Bot)
    }

    /// Does it need a guild choice (bot or commands install)?
    pub fn wants_install(&self) -> bool {
        self.wants_bot() || self.scopes.contains(&Scope::ApplicationsCommands)
    }

    /// Scopes that make the flow produce a code (account data or permission editing).
    pub fn wants_code(&self) -> bool {
        self.scopes.iter().any(|s| {
            matches!(
                s,
                Scope::Identify | Scope::Email | Scope::Guilds | Scope::CommandsPermissionsUpdate
            )
        })
    }
}

/// How a client authenticated at the token endpoints.
#[derive(Debug, Clone)]
pub struct ClientCredentials {
    /// `client_id` as presented.
    pub client_id: String,
    /// `client_secret` as presented, if any.
    pub secret: Option<String>,
}

/// Success body of `/oauth2/token`.
#[derive(Debug, Clone)]
pub struct TokenResponse {
    /// Access token (shown once).
    pub access_token: String,
    /// Refresh token (shown once).
    pub refresh_token: String,
    /// Seconds until the access token expires.
    pub expires_in: i64,
    /// Granted scopes.
    pub scope: Vec<String>,
}

/// RFC 7662 answer.
#[derive(Debug, Clone)]
pub struct Introspection {
    /// Live token?
    pub active: bool,
    /// The token details when active.
    pub details: Option<IntrospectionDetails>,
}

/// Fields of an active introspection.
#[derive(Debug, Clone)]
pub struct IntrospectionDetails {
    /// Scopes.
    pub scope: Vec<String>,
    /// Client id.
    pub client_id: i64,
    /// Username (with `identify`).
    pub username: Option<String>,
    /// User id.
    pub sub: i64,
    /// Expiry, Unix seconds.
    pub exp: i64,
    /// Issued at, Unix seconds.
    pub iat: i64,
}

fn oerr(error: &'static str, d: &str) -> OAuthError {
    OAuthError::new(error, d)
}

fn server(e: AppsError) -> OAuthError {
    tracing::error!(error = %e, "oauth storage failure");
    OAuthError::new("server_error", "Temporary failure.")
}

impl AppsService {
    /// Validate the parameters of an authorize request. Nothing here redirects: an invalid
    /// client or redirect URI must be shown as an error page, never followed.
    pub async fn validate_authorize(&self, p: &AuthorizeParams) -> AppsResult<ValidatedAuthorize> {
        let client = p
            .client_id
            .parse::<i64>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| {
                AppsError::field("client_id", "INVALID_CLIENT", "Unknown application.")
            })?;
        let app = self.repo.application(client).await?.ok_or_else(|| {
            AppsError::field("client_id", "INVALID_CLIENT", "Unknown application.")
        })?;
        if !app.is_live() {
            return Err(AppsError::Suspended);
        }
        let mut problems = Vec::new();
        if p.response_type.as_deref().is_some_and(|r| r != "code") {
            problems.push(FieldProblem::new(
                "response_type",
                "UNSUPPORTED_RESPONSE_TYPE",
                "Only response_type=code is supported.",
            ));
        }
        let scopes = match parse_scopes(&p.scope) {
            Ok(s) if !s.is_empty() => s,
            Ok(_) => {
                problems.push(FieldProblem::new(
                    "scope",
                    "BASE_TYPE_REQUIRED",
                    "At least one scope is required.",
                ));
                Vec::new()
            }
            Err(bad) => {
                problems.push(FieldProblem::new(
                    "scope",
                    "INVALID_SCOPE",
                    format!("Unknown or unsupported scope: {bad}."),
                ));
                Vec::new()
            }
        };
        if scopes.contains(&Scope::Email) && !scopes.contains(&Scope::Identify) {
            problems.push(FieldProblem::new(
                "scope",
                "INVALID_SCOPE",
                "The email scope needs identify.",
            ));
        }
        let redirect_uri = match p.redirect_uri.as_deref().filter(|r| !r.is_empty()) {
            Some(r) if authoauth::redirect_registered(&app.redirect_uris, r) => Some(r.to_owned()),
            Some(_) => {
                problems.push(FieldProblem::new(
                    "redirect_uri",
                    "INVALID_REDIRECT_URI",
                    "Invalid OAuth2 redirect_uri.",
                ));
                None
            }
            None => None,
        };
        let wants_code = scopes.iter().any(|s| {
            matches!(
                s,
                Scope::Identify | Scope::Email | Scope::Guilds | Scope::CommandsPermissionsUpdate
            )
        });
        if wants_code
            && redirect_uri.is_none()
            && p.redirect_uri.as_deref().is_none_or(str::is_empty)
        {
            problems.push(FieldProblem::new(
                "redirect_uri",
                "BASE_TYPE_REQUIRED",
                "A redirect_uri is required for these scopes.",
            ));
        }
        let code_challenge = match p.code_challenge.as_deref().filter(|c| !c.is_empty()) {
            Some(c)
                if authoauth::pkce_challenge_ok(c)
                    && authoauth::pkce_method_ok(p.code_challenge_method.as_deref()) =>
            {
                Some(c.to_owned())
            }
            Some(_) => {
                problems.push(FieldProblem::new(
                    "code_challenge",
                    "INVALID_CODE_CHALLENGE",
                    "Only S256 challenges (43 base64url characters) are supported.",
                ));
                None
            }
            None => None,
        };
        if p.state.as_deref().is_some_and(|s| s.len() > 512) {
            problems.push(FieldProblem::new(
                "state",
                "BASE_TYPE_MAX_LENGTH",
                "Must be 512 or fewer in length.",
            ));
        }
        let permissions = match p.permissions.as_deref().filter(|s| !s.is_empty()) {
            Some(s) => match s.parse::<u64>() {
                Ok(v) => v,
                Err(_) => {
                    problems.push(FieldProblem::new(
                        "permissions",
                        "NUMBER_TYPE_COERCE",
                        "Not a bitfield.",
                    ));
                    0
                }
            },
            None => app.install_permissions,
        };
        let guild_id = match p.guild_id.as_deref().filter(|s| !s.is_empty()) {
            Some(s) => match s.parse::<i64>() {
                Ok(v) if v > 0 => Some(v),
                _ => {
                    problems.push(FieldProblem::new(
                        "guild_id",
                        "SNOWFLAKE_TYPE_INVALID",
                        "Not a valid id.",
                    ));
                    None
                }
            },
            None => None,
        };
        if !problems.is_empty() {
            return Err(AppsError::Invalid(problems));
        }
        Ok(ValidatedAuthorize {
            app,
            scopes,
            redirect_uri,
            state: p.state.clone(),
            code_challenge,
            permissions,
            guild_id,
        })
    }

    /// Record the grant and mint an authorization code for the redirect.
    pub async fn issue_code(&self, user: i64, v: &ValidatedAuthorize) -> AppsResult<String> {
        let redirect = v.redirect_uri.clone().ok_or_else(|| {
            AppsError::field(
                "redirect_uri",
                "BASE_TYPE_REQUIRED",
                "A redirect_uri is required.",
            )
        })?;
        let names = scope_names(&v.scopes);
        let now = self.now_ms();
        let grant = self
            .repo
            .upsert_grant(self.next_id(), v.app.id, user, &names, now)
            .await?;
        let code = OpaqueToken::generate(OpaqueKind::AuthCode)
            .map_err(|e| AppsError::Storage(e.to_string()))?;
        self.repo
            .store_code(
                &code.hash(),
                grant.id,
                &redirect,
                &names,
                v.code_challenge.as_deref(),
                now + CODE_TTL_SECS * 1000,
            )
            .await?;
        Ok(code.expose().to_owned())
    }

    /// Authenticate the client of a token-endpoint call. With `require_secret` a missing or
    /// wrong secret is `invalid_client`; without, a presented secret must still be right.
    pub async fn authenticate_client(
        &self,
        c: &ClientCredentials,
        require_secret: bool,
    ) -> Result<(Application, bool), OAuthError> {
        let bad = || oerr("invalid_client", "Client authentication failed.");
        let id = c
            .client_id
            .parse::<i64>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(bad)?;
        let app = self
            .repo
            .application(id)
            .await
            .map_err(server)?
            .ok_or_else(bad)?;
        if !app.is_live() {
            return Err(bad());
        }
        match c.secret.as_deref() {
            Some(s) => {
                let presented = hash_presented(OpaqueKind::ClientSecret, s).ok_or_else(bad)?;
                if !authoauth::digests_equal(&presented, &app.client_secret_hash) {
                    return Err(bad());
                }
                Ok((app, true))
            }
            None if require_secret => Err(bad()),
            None => Ok((app, false)),
        }
    }

    fn mint(&self, grant: &Grant, scopes: &[String]) -> AppsResult<(TokenRecord, TokenResponse)> {
        let access = OpaqueToken::generate(OpaqueKind::AccessToken)
            .map_err(|e| AppsError::Storage(e.to_string()))?;
        let refresh = OpaqueToken::generate(OpaqueKind::RefreshToken)
            .map_err(|e| AppsError::Storage(e.to_string()))?;
        let now = self.now_ms();
        let rec = TokenRecord {
            id: self.next_id(),
            grant_id: grant.id,
            application_id: grant.application_id,
            user_id: grant.user_id,
            scopes: scopes.to_vec(),
            access_hash: access.hash(),
            refresh_hash: refresh.hash(),
            access_expires_at_ms: now + ACCESS_TTL_SECS * 1000,
            refresh_expires_at_ms: now + REFRESH_TTL_SECS * 1000,
            refresh_used: false,
            revoked: false,
        };
        let resp = TokenResponse {
            access_token: access.expose().to_owned(),
            refresh_token: refresh.expose().to_owned(),
            expires_in: ACCESS_TTL_SECS,
            scope: scopes.to_vec(),
        };
        Ok((rec, resp))
    }

    /// `grant_type=authorization_code`.
    pub async fn exchange_code(
        &self,
        client: &ClientCredentials,
        code: &str,
        redirect_uri: &str,
        verifier: Option<&str>,
    ) -> Result<TokenResponse, OAuthError> {
        let (app, authenticated) = self.authenticate_client(client, false).await?;
        let grant_err = |d: &str| oerr("invalid_grant", d);
        let hash = hash_presented(OpaqueKind::AuthCode, code)
            .ok_or_else(|| grant_err("Invalid authorization code."))?;
        let rec = match self.repo.consume_code(&hash).await.map_err(server)? {
            CodeConsume::Unknown => return Err(grant_err("Invalid authorization code.")),
            CodeConsume::Replayed(grant_id) => {
                // A replayed code means someone else may hold the first tokens.
                self.repo
                    .revoke_code_tokens(grant_id, &hash)
                    .await
                    .map_err(server)?;
                return Err(grant_err("Authorization code already used."));
            }
            CodeConsume::Fresh(r) => r,
        };
        if rec.grant.application_id != app.id {
            return Err(grant_err(
                "Authorization code was issued to another client.",
            ));
        }
        if rec.expires_at_ms <= self.now_ms() {
            return Err(grant_err("Authorization code expired."));
        }
        if rec.redirect_uri != redirect_uri {
            return Err(grant_err(
                "redirect_uri does not match the authorization request.",
            ));
        }
        match (&rec.code_challenge, verifier) {
            (Some(ch), Some(v)) if authoauth::pkce_verify(v, ch) => {}
            (Some(_), _) => return Err(grant_err("Invalid code_verifier.")),
            (None, _) if !authenticated => {
                return Err(oerr(
                    "invalid_client",
                    "A client without a secret must use PKCE.",
                ));
            }
            (None, _) => {}
        }
        let (token, resp) = self.mint(&rec.grant, &rec.scopes).map_err(server)?;
        self.repo
            .insert_token(&token, Some(&hash), self.now_ms())
            .await
            .map_err(server)?;
        Ok(resp)
    }

    /// `grant_type=refresh_token`. Refresh tokens rotate; presenting a spent one revokes the
    /// whole grant.
    pub async fn refresh(
        &self,
        client: &ClientCredentials,
        refresh_token: &str,
    ) -> Result<TokenResponse, OAuthError> {
        let (app, _) = self.authenticate_client(client, true).await?;
        let grant_err = |d: &str| oerr("invalid_grant", d);
        let hash = hash_presented(OpaqueKind::RefreshToken, refresh_token)
            .ok_or_else(|| grant_err("Invalid refresh token."))?;
        let rec = match self.repo.token_by_refresh(&hash).await.map_err(server)? {
            RefreshLookup::Unknown => return Err(grant_err("Invalid refresh token.")),
            RefreshLookup::Spent(r) => {
                if r.application_id == app.id {
                    self.repo.revoke_grant(r.grant_id).await.map_err(server)?;
                }
                return Err(grant_err("Invalid refresh token."));
            }
            RefreshLookup::Live(r) => r,
        };
        if rec.application_id != app.id {
            return Err(grant_err("Invalid refresh token."));
        }
        if rec.refresh_expires_at_ms <= self.now_ms() {
            return Err(grant_err("Refresh token expired."));
        }
        if !self.repo.spend_refresh(rec.id).await.map_err(server)? {
            // Lost a race with another use of the same token: treat as reuse.
            self.repo.revoke_grant(rec.grant_id).await.map_err(server)?;
            return Err(grant_err("Invalid refresh token."));
        }
        self.repo.revoke_token(rec.id).await.map_err(server)?;
        let grant = Grant {
            id: rec.grant_id,
            application_id: rec.application_id,
            user_id: rec.user_id,
            scopes: rec.scopes.clone(),
            created_at_ms: 0,
        };
        let (token, resp) = self.mint(&grant, &rec.scopes).map_err(server)?;
        self.repo
            .insert_token(&token, None, self.now_ms())
            .await
            .map_err(server)?;
        Ok(resp)
    }

    async fn lookup_any(
        &self,
        app: &Application,
        token: &str,
        hint: Option<&str>,
    ) -> Result<Option<(TokenRecord, bool)>, OAuthError> {
        // (record, presented_was_refresh)
        let order: [bool; 2] = if hint == Some("refresh_token") {
            [true, false]
        } else {
            [false, true]
        };
        for refresh in order {
            if refresh {
                if let Some(h) = hash_presented(OpaqueKind::RefreshToken, token) {
                    match self.repo.token_by_refresh(&h).await.map_err(server)? {
                        RefreshLookup::Live(r) | RefreshLookup::Spent(r)
                            if r.application_id == app.id =>
                        {
                            return Ok(Some((r, true)));
                        }
                        _ => {}
                    }
                }
            } else if let Some(h) = hash_presented(OpaqueKind::AccessToken, token)
                && let Some(r) = self.repo.token_by_access(&h).await.map_err(server)?
                && r.application_id == app.id
            {
                return Ok(Some((r, false)));
            }
        }
        Ok(None)
    }

    /// RFC 7009. Always succeeds for a client that authenticates; unknown tokens and tokens of
    /// other clients are silently ignored.
    pub async fn revoke_token(
        &self,
        client: &ClientCredentials,
        token: &str,
        hint: Option<&str>,
    ) -> Result<(), OAuthError> {
        let (app, _) = self.authenticate_client(client, false).await?;
        if let Some((rec, was_refresh)) = self.lookup_any(&app, token, hint).await? {
            if was_refresh {
                self.repo.revoke_grant(rec.grant_id).await.map_err(server)?;
            } else {
                self.repo.revoke_token(rec.id).await.map_err(server)?;
            }
        }
        Ok(())
    }

    /// RFC 7662. Only the client the token was issued to learns anything.
    pub async fn introspect(
        &self,
        client: &ClientCredentials,
        token: &str,
        hint: Option<&str>,
    ) -> Result<Introspection, OAuthError> {
        let (app, _) = self.authenticate_client(client, true).await?;
        let inactive = Introspection {
            active: false,
            details: None,
        };
        let Some((rec, _)) = self.lookup_any(&app, token, hint).await? else {
            return Ok(inactive);
        };
        let now = self.now_ms();
        if rec.revoked || rec.access_expires_at_ms <= now {
            return Ok(inactive);
        }
        let username = if rec.scopes.iter().any(|s| s == "identify") {
            self.repo
                .account_brief(rec.user_id)
                .await
                .map_err(server)?
                .map(|a| a.username)
        } else {
            None
        };
        Ok(Introspection {
            active: true,
            details: Some(IntrospectionDetails {
                scope: rec.scopes.clone(),
                client_id: rec.application_id,
                username,
                sub: rec.user_id,
                exp: rec.access_expires_at_ms.div_euclid(1000),
                iat: (rec.access_expires_at_ms - ACCESS_TTL_SECS * 1000).div_euclid(1000),
            }),
        })
    }

    /// Resolve a bearer access token for the REST extractor.
    pub async fn bearer(&self, presented: &str) -> AppsResult<Option<(TokenRecord, Application)>> {
        let Some(hash) = hash_presented(OpaqueKind::AccessToken, presented) else {
            return Ok(None);
        };
        let Some(rec) = self.repo.token_by_access(&hash).await? else {
            return Ok(None);
        };
        if rec.revoked
            || rec.access_expires_at_ms <= self.now_ms()
            || !authoauth::digests_equal(&rec.access_hash, &hash)
        {
            return Ok(None);
        }
        let Some(app) = self.repo.application(rec.application_id).await? else {
            return Ok(None);
        };
        if !app.is_live() {
            return Ok(None);
        }
        Ok(Some((rec, app)))
    }

    /// The apps a user authorized.
    pub async fn grants_of(&self, user: i64) -> AppsResult<Vec<(Grant, Application)>> {
        self.repo.grants_of(user).await
    }

    /// Revoke one of the user's grants.
    pub async fn revoke_grant(&self, user: i64, grant_id: i64) -> AppsResult<()> {
        let g = self
            .repo
            .grant_of(user, grant_id)
            .await?
            .ok_or(AppsError::NotFound("grant"))?;
        self.repo.revoke_grant(g.id).await
    }
}
