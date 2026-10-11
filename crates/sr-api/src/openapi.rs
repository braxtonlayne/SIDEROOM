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

/// Body of `GET /users/@me` (minimal until the users feature lands).
#[derive(Debug, Serialize, ToSchema)]
pub struct CurrentUser {
    /// Snowflake id as a string.
    pub id: String,
    /// `session`, `bot` or `bearer`.
    pub auth: String,
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

/// The authenticated caller (placeholder until the users feature lands).
#[utoipa::path(
    get,
    path = "/users/@me",
    responses(
        (status = 200, body = CurrentUser),
        (status = 401, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn current_user(p: crate::auth::Principal) -> Json<CurrentUser> {
    Json(CurrentUser {
        id: p.user_id.to_string(),
        auth: match p.kind {
            crate::AuthKind::Session { .. } => "session",
            crate::AuthKind::Bot { .. } => "bot",
            crate::AuthKind::Bearer { .. } => "bearer",
        }
        .to_owned(),
    })
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
