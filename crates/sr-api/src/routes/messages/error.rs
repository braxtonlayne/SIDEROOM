//! Route error type, request extractors with Discord-shaped rejections, id parsing.

use super::store::StoreError;
use crate::error::HttpError;
use axum::{
    Json,
    extract::{FromRequest, FromRequestParts, Request},
    http::{HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
};
use serde::de::DeserializeOwned;
use serde_json::json;
use sr_chat::{ChatError, RepoError};
use sr_core::{ApiError, ErrorCode};
use sr_storage::StorageError;
use std::str::FromStr;

/// Everything a handler can fail with.
#[derive(Debug)]
pub enum MsgError {
    Api(ApiError),
    /// Slowmode: 429 with Discord's `retry_after` (seconds) and a `Retry-After` header.
    RetryAfter {
        api: ApiError,
        retry_after_ms: i64,
    },
}

pub type MsgResult<T> = Result<T, MsgError>;

impl From<ApiError> for MsgError {
    fn from(e: ApiError) -> Self {
        Self::Api(e)
    }
}

impl From<ErrorCode> for MsgError {
    fn from(c: ErrorCode) -> Self {
        Self::Api(ApiError::new(c))
    }
}

impl From<HttpError> for MsgError {
    fn from(e: HttpError) -> Self {
        Self::Api(e.0)
    }
}

fn internal(what: &str, e: &dyn std::fmt::Display) -> MsgError {
    tracing::error!(error = %e, "{what} failure");
    ErrorCode::SrInternal.into()
}

impl From<ChatError> for MsgError {
    fn from(e: ChatError) -> Self {
        match e {
            ChatError::Slowmode { retry_after_ms } => Self::RetryAfter {
                api: e.to_api_error(),
                retry_after_ms,
            },
            ChatError::Repo(ref r) => internal("repository", r),
            other => Self::Api(other.to_api_error()),
        }
    }
}

impl From<RepoError> for MsgError {
    fn from(e: RepoError) -> Self {
        internal("repository", &e)
    }
}

impl From<StoreError> for MsgError {
    fn from(e: StoreError) -> Self {
        internal("message store", &e)
    }
}

impl From<StorageError> for MsgError {
    fn from(e: StorageError) -> Self {
        internal("object storage", &e)
    }
}

impl IntoResponse for MsgError {
    fn into_response(self) -> Response {
        match self {
            Self::Api(e) => HttpError(e).into_response(),
            Self::RetryAfter {
                api,
                retry_after_ms,
            } => {
                let mut body = api.body();
                body["retry_after"] = json!(retry_after_ms as f64 / 1000.0);
                body["global"] = json!(false);
                let secs = (retry_after_ms.max(0) + 999) / 1000;
                let mut resp = (StatusCode::TOO_MANY_REQUESTS, Json(body)).into_response();
                if let Ok(v) = HeaderValue::from_str(&secs.to_string()) {
                    resp.headers_mut().insert(header::RETRY_AFTER, v);
                }
                resp
            }
        }
    }
}

/// 50035 for a single bad field.
pub fn invalid(field: &str, code: &str, message: &str) -> MsgError {
    MsgError::Api(ApiError::invalid_form().field(field, code, message))
}

/// Parse a snowflake path segment; `unknown` is the 404 code when it is malformed.
pub fn parse_id<T: FromStr>(raw: &str, unknown: ErrorCode) -> MsgResult<T> {
    raw.parse::<T>().map_err(|_| unknown.into())
}

/// JSON body extractor whose rejection is a Discord-style 50035.
pub struct ApiJson<T>(pub T);

impl<T: DeserializeOwned, S: Send + Sync> FromRequest<S> for ApiJson<T> {
    type Rejection = MsgError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(v)) => Ok(Self(v)),
            Err(e) if e.status() == StatusCode::PAYLOAD_TOO_LARGE => {
                Err(ErrorCode::RequestEntityTooLarge.into())
            }
            Err(e) => Err(invalid("body", "INVALID_JSON", &e.body_text())),
        }
    }
}

/// Query-string extractor whose rejection is a Discord-style 50035.
pub struct ApiQuery<T>(pub T);

impl<T: DeserializeOwned, S: Send + Sync> FromRequestParts<S> for ApiQuery<T> {
    type Rejection = MsgError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match axum::extract::Query::<T>::from_request_parts(parts, state).await {
            Ok(axum::extract::Query(v)) => Ok(Self(v)),
            Err(e) => Err(invalid("query", "INVALID_QUERY", &e.body_text())),
        }
    }
}
