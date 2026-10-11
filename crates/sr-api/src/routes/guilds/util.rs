//! Shared handler plumbing: backend extractor, id/permission parsing, error mapping.

use super::backend::GuildBackend;
use crate::routes::Ext;
use crate::{AuthKind, HttpError, Principal, state::AppState};
use axum::{extract::FromRequestParts, http::request::Parts};
use sr_chat::ChatError;
use sr_chat::repo::UserSummary;
use sr_core::permissions::Permissions;
use sr_core::{ApiError, ErrorCode, UserId};
use std::collections::HashMap;
use std::{str::FromStr, sync::Arc};

/// Registered on [`AppState`] by the composition root
/// (`.extension(Arc::new(GuildApi(..)))`).
#[derive(Clone)]
pub struct GuildApi(pub Arc<dyn GuildBackend>);

/// Extractor yielding the registered backend; a missing backend is a server
/// misconfiguration (500), never a client error.
pub struct Api(pub Arc<dyn GuildBackend>);

impl FromRequestParts<AppState> for Api {
    type Rejection = HttpError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Ext(g) = Ext::<GuildApi>::from_request_parts(parts, state).await?;
        Ok(Self(g.0.clone()))
    }
}

/// Map a service error to the Discord error body.
pub fn ce(e: ChatError) -> HttpError {
    if matches!(e, ChatError::Repo(_)) {
        tracing::error!(error = %e, "guild repository failure");
    }
    HttpError(e.to_api_error())
}

/// Parse a snowflake-typed id from a path segment or body field.
pub fn id<T: FromStr>(field: &str, raw: &str) -> Result<T, HttpError> {
    raw.parse().map_err(|_| {
        HttpError(ApiError::invalid_form().field(
            field,
            "SNOWFLAKE_TYPE_INVALID",
            "Value is not snowflake.",
        ))
    })
}

/// Parse a decimal permission bitfield.
pub fn permissions(field: &str, raw: &str) -> Result<Permissions, HttpError> {
    raw.parse::<u64>()
        .map(Permissions::from_bits_truncate)
        .map_err(|_| {
            HttpError(ApiError::invalid_form().field(
                field,
                "BASE_TYPE_BAD_INT",
                "Value must be a decimal permission bitfield.",
            ))
        })
}

/// The acting user. OAuth bearer tokens cannot administer guilds (they are
/// limited to `GET /users/@me/guilds` with the `guilds` scope); sessions and
/// bots can.
pub fn actor(p: &Principal) -> Result<UserId, HttpError> {
    match p.kind {
        AuthKind::Bearer { .. } => Err(ErrorCode::MissingAccess.into()),
        _ => Ok(UserId::new(u64::try_from(p.user_id).unwrap_or(0))),
    }
}

/// Display fields for `ids`, keyed by id (open question 157). Unknown ids are absent.
pub async fn users_for(
    api: &dyn GuildBackend,
    ids: impl IntoIterator<Item = UserId>,
) -> Result<HashMap<UserId, UserSummary>, HttpError> {
    let mut ids: Vec<UserId> = ids.into_iter().collect();
    ids.sort();
    ids.dedup();
    let found = api.user_summaries(&ids).await.map_err(ce)?;
    Ok(found.into_iter().map(|u| (u.id, u)).collect())
}
