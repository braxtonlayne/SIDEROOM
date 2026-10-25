//! The platform first run (RDY-ONB-001): whether the person closed or finished the short welcome that follows
//! the first sign-in. `GET /users/@me` reports it as `first_run_done`; `PUT /sr/auth/first-run {done}` sets it
//! (`false` re-opens the welcome from `/help`). The flag only decides whether the welcome shows by itself;
//! nothing else in the product reads it, so no step can lock anybody out.

use super::{
    common::{R, Svc},
    sessions::session_id,
};
use crate::{auth::UserOnly, openapi::ErrorBody};
use axum::{Json, http::StatusCode};
use serde::Deserialize;
use utoipa::{OpenApi, ToSchema};

/// Body of `PUT /sr/auth/first-run`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct FirstRunBody {
    /// `true` once the welcome is closed or finished, `false` to show it again.
    pub done: bool,
}

/// Mark the first run done, or re-open it.
#[utoipa::path(
    put,
    path = "/sr/auth/first-run",
    request_body = FirstRunBody,
    responses(
        (status = 204, description = "Saved"),
        (status = 401, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn put(
    UserOnly(p): UserOnly,
    Svc { svc, .. }: Svc,
    Json(body): Json<FirstRunBody>,
) -> R<StatusCode> {
    session_id(&p)?;
    svc.store.set_first_run_done(p.user_id, body.done).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// OpenAPI fragment of this module.
#[derive(OpenApi)]
#[openapi(paths(put), components(schemas(FirstRunBody)))]
pub struct FirstRunApiDoc;
