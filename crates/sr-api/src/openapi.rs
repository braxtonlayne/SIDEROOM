//! OpenAPI generation (FND-API-004). The document is generated from Rust via
//! `utoipa`; feature crates define their own `#[derive(OpenApi)]` types and
//! merge them into [`base`] (`OpenApi::merge`) before building `AppState`.

use crate::{auth::SESSION_COOKIE, state::AppState};
use axum::{Json, extract::State};
use serde::Serialize;
use utoipa::{
    Modify, OpenApi, ToSchema,
    openapi::security::{ApiKey, ApiKeyValue, SecurityScheme},
};

/// Discord-style error body (doc 05 section 3).
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    /// Numeric error code (Discord's where one exists, `>= 900000` for SIDE ROOM).
    pub code: u32,
    /// Human-readable message.
    pub message: String,
    /// Nested field error tree (`{field: {_errors: [{code, message}]}}`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<serde_json::Value>,
}

/// Body of `GET /users/@me`: a Discord-shaped user object. Session callers get
/// the full object (including their own `email`); bot and bearer callers get the
/// id, `bot` and the placeholder fields only, since no account row backs them.
#[derive(Debug, Serialize, ToSchema)]
pub struct CurrentUser {
    /// Snowflake id as a string.
    pub id: String,
    /// `session`, `bot` or `bearer` (SIDE ROOM addition).
    pub auth: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// The display name, if the account set one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_name: Option<String>,
    /// Same as `global_name` (SIDE ROOM name for the field).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Avatar hash; the image is at `/cdn/avatars/{id}/{hash}.png`.
    pub avatar: Option<String>,
    /// Profile banner hash, served like the avatar.
    pub banner: Option<String>,
    /// Always `"0"` (no discriminators).
    pub discriminator: String,
    /// True for bot callers.
    pub bot: bool,
    /// Whether the email address was verified (session callers).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified: Option<bool>,
    /// The caller's own email (session callers only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Whether TOTP is confirmed on the account (session callers).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mfa_enabled: Option<bool>,
    /// Public account flags (session callers).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<i64>,
    /// BCP 47 locale (session callers).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    /// Whether the person closed or finished the platform first run; `false` for a new account
    /// (session callers).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_run_done: Option<bool>,
}

/// Body of `GET /sr/csrf`.
#[derive(Debug, Serialize, ToSchema)]
pub struct CsrfToken {
    /// Value for the `X-SR-CSRF` header.
    pub token: String,
}

struct Security;

impl Modify for Security {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let c = openapi.components.get_or_insert_with(Default::default);
        c.add_security_scheme(
            "authorization",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::with_description(
                "Authorization",
                "`Bot <token>` or `Bearer <token>`",
            ))),
        );
        c.add_security_scheme(
            "session",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::with_description(
                SESSION_COOKIE,
                "Browser session; non-GET requests also need `X-SR-CSRF`",
            ))),
        );
    }
}

/// The SIDE ROOM API document.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "SIDE ROOM API",
        description = "Discord-shaped REST API. `/api/v10` is an alias of `/api/v1`.",
        version = "1"
    ),
    servers((url = "/api/v1", description = "Current")),
    paths(current_user, csrf_token),
    components(schemas(ErrorBody, CurrentUser, CsrfToken)),
    modifiers(&Security)
)]
pub struct ApiDoc;

/// Base document; merge feature documents into it.
pub fn base() -> utoipa::openapi::OpenApi {
    ApiDoc::openapi()
}

/// `GET /openapi.json`
pub async fn openapi_json(State(state): State<AppState>) -> Json<utoipa::openapi::OpenApi> {
    Json(state.openapi().clone())
}

/// The authenticated caller (as a Discord-shaped user object).
#[utoipa::path(
    get,
    path = "/users/@me",
    responses(
        (status = 200, body = CurrentUser),
        (status = 401, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn current_user(
    p: crate::auth::Principal,
    crate::routes::MaybeExt(auth): crate::routes::MaybeExt<crate::routes::auth::AuthService>,
    crate::routes::MaybeExt(social): crate::routes::MaybeExt<crate::routes::social::SocialDeps>,
) -> Result<Json<CurrentUser>, crate::HttpError> {
    let mut me = CurrentUser {
        id: p.user_id.to_string(),
        auth: match p.kind {
            crate::AuthKind::Session { .. } => "session",
            crate::AuthKind::Bot { .. } => "bot",
            crate::AuthKind::Bearer { .. } => "bearer",
        }
        .to_owned(),
        username: None,
        global_name: None,
        display_name: None,
        avatar: None,
        banner: None,
        discriminator: "0".to_owned(),
        bot: matches!(p.kind, crate::AuthKind::Bot { .. }),
        verified: None,
        email: None,
        mfa_enabled: None,
        flags: None,
        locale: None,
        first_run_done: None,
    };
    if let Some(social) = social
        && let Ok(u) = social
            .user_object(sr_core::UserId::new(u64::try_from(p.user_id).unwrap_or(0)))
            .await
    {
        me.avatar = u.avatar;
        me.banner = u.banner;
        // Bots and OAuth2 callers have no account overview below; the public user fields are
        // enough (a bearer token reaches this route only with `identify`).
        if !matches!(p.kind, crate::AuthKind::Session { .. }) {
            me.username = u.username;
            me.global_name.clone_from(&u.global_name);
            me.display_name = u.global_name;
            me.bot = me.bot || u.bot;
        }
    }
    // The `email` scope also reveals the address the user signed up with.
    if let crate::AuthKind::Bearer { scopes, .. } = &p.kind
        && scopes.iter().any(|s| s == "email")
        && let Some(auth) = &auth
        && let Ok(Some((account, _))) = auth.account_overview(p.user_id).await
    {
        me.verified = Some(account.email_verified);
        me.email = Some(account.email);
    }
    // Only a session is backed by an account row; the email is the caller's own.
    if matches!(p.kind, crate::AuthKind::Session { .. })
        && let Some(auth) = auth
    {
        let found = auth.account_overview(p.user_id).await.map_err(|e| {
            tracing::error!(error = %e, "could not load the current account");
            crate::HttpError(sr_core::ErrorCode::SrInternal.into())
        })?;
        if let Some((account, mfa)) = found {
            me.username = Some(account.username);
            me.global_name.clone_from(&account.display_name);
            me.display_name = account.display_name;
            me.verified = Some(account.email_verified);
            me.email = Some(account.email);
            me.mfa_enabled = Some(mfa);
            me.flags = Some(account.flags);
            me.locale = Some(account.locale);
            me.first_run_done = Some(auth.first_run_done(p.user_id).await.unwrap_or(true));
        }
    }
    Ok(Json(me))
}

/// The CSRF token for the current session.
#[utoipa::path(
    get,
    path = "/sr/csrf",
    responses(
        (status = 200, body = CsrfToken),
        (status = 401, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn csrf_token(
    headers: axum::http::HeaderMap,
    crate::auth::UserOnly(_): crate::auth::UserOnly,
) -> Result<Json<CsrfToken>, crate::HttpError> {
    // Only cookie sessions have a CSRF token; the principal was verified above.
    let raw = crate::auth::parse_cookie(&headers, SESSION_COOKIE)
        .ok_or(sr_core::ErrorCode::Unauthorized)?;
    Ok(Json(CsrfToken {
        token: crate::auth::csrf::token_for(raw),
    }))
}
