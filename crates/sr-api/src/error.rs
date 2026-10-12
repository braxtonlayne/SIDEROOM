//! Rendering of [`sr_core::ApiError`] as Discord-style JSON error responses.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use sr_core::{ApiError, ErrorCode};

/// Newtype so `ApiError` (foreign) can implement `IntoResponse`.
#[derive(Debug, Clone)]
pub struct HttpError(pub ApiError);

impl From<ApiError> for HttpError {
    fn from(e: ApiError) -> Self {
        Self(e)
    }
}

impl From<ErrorCode> for HttpError {
    fn from(c: ErrorCode) -> Self {
        Self(ApiError::new(c))
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let status =
            StatusCode::from_u16(self.0.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(self.0.body())).into_response()
    }
}

/// Result alias for handlers.
pub type ApiResult<T> = Result<T, HttpError>;
