//! Feature route modules. `sr-server` mounts everything through [`all`] and
//! documents it through [`openapi`], so adding a module is one `.merge(...)`
//! line in each. Each module exposes `pub fn router() -> Router<AppState>`.
//!
//! Services a handler needs are registered on the composition root with
//! [`crate::AppStateBuilder::extension`] and read with the [`Ext`] extractor.
//!
//! Route ownership (open question 156). A path+method may be registered by one
//! module only; `Router::merge` panics at startup on a clash, and the
//! `all_routes_build_without_overlap` test catches it in CI.
//!
//! * `guilds`: `GET/PATCH/DELETE /channels/{id}` for every channel type (the DM
//!   branch delegates to the DM service) and `/users/@me/guilds*`.
//! * `messages`: `/channels/{id}/{messages,pins,typing,attachments}*`.
//! * `auth`: `/sr/*` entrance routes.
//! * `admin`: `/admin/*` platform admin (staff sessions only; permissions checked per call).
//! * `cdn`: `GET|HEAD /cdn/{*path}` signed media delivery. Not part of [`all`]: it lives
//!   outside `/api/v1`; the composition root merges [`cdn::layered`] at the top level.
//! * `library`: `/library/*` catalog for members, `/admin/library/*` for publishers.
//! * `cinema`: `/cinema/*` catalog for members, `/admin/cinema/*` for publishers.
//! * `voice`: `GET /sr/voice/turn` and `PATCH /guilds/{id}/voice-states/{user_id}`.
//! * `social`: relationships, ignores, notes, DMs (`/users/@me/channels*`, `/channels/{id}/recipients/*`),
//!   acks and read states, notification settings, profiles (`PATCH /users/@me`, `/users/{id}/profile`)
//!   and `/users/@me/presence`. Only `GET /users/@me` stays with the base router.

use crate::{error::HttpError, openapi, state::AppState};
use axum::{Router, extract::FromRequestParts, http::request::Parts};
use sr_core::ErrorCode;
use std::{any::type_name, sync::Arc};

pub mod admin;
pub mod auth;
pub mod cdn;
pub mod cinema;
pub mod cinema_media;
pub mod cinema_social;
pub mod guilds;
pub mod library;
pub mod media;
pub mod messages;
pub mod review;
pub mod search;
pub mod settings;
pub mod social;
pub mod threads;
pub mod voice;

/// Extractor for a service registered on [`AppState`]. A missing service is a
/// server misconfiguration: it logs once per request and answers 500
/// `SrInternal`, never a client error.
pub struct Ext<T: Send + Sync + 'static>(pub Arc<T>);

impl<T: Send + Sync + 'static> FromRequestParts<AppState> for Ext<T> {
    type Rejection = HttpError;

    async fn from_request_parts(
        _parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        match state.extension::<T>() {
            Some(v) => Ok(Self(v)),
            None => {
                tracing::error!(
                    service = type_name::<T>(),
                    "route needs a service that was not registered on AppState"
                );
                Err(ErrorCode::SrInternal.into())
            }
        }
    }
}

/// Like [`Ext`], for a service that is allowed to be absent (optional hooks such
/// as the AutoMod gate): `None` instead of a 500.
pub struct MaybeExt<T: Send + Sync + 'static>(pub Option<Arc<T>>);

impl<T: Send + Sync + 'static> FromRequestParts<AppState> for MaybeExt<T> {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        _parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self(state.extension::<T>()))
    }
}

/// Every feature router, relative to the `/api/v1` prefix. Merging panics on
/// an overlapping method and path, so this doubles as the startup check.
pub fn all() -> Router<AppState> {
    Router::new()
        .merge(admin::router())
        .merge(auth::router())
        .merge(messages::router())
        .merge(guilds::router())
        .merge(social::router())
        .merge(settings::router())
        .merge(search::router())
        .merge(threads::router())
        .merge(library::router())
        .merge(cinema::router())
        .merge(cinema_media::router())
        .merge(review::router())
        .merge(voice::router())
        .merge(cinema_social::router())
}

/// Feature-owned routers (`Feature::routes()`), nested under
/// `/sr/features/{id}` relative to `/api/v1`.
pub fn features<I>(list: I) -> Router<AppState>
where
    I: IntoIterator<Item = (&'static str, Router)>,
{
    list.into_iter().fold(Router::new(), |r, (id, feature)| {
        r.nest_service(&format!("/sr/features/{id}"), feature)
    })
}

/// The full OpenAPI document: [`crate::openapi::base`] plus each module's paths.
pub fn openapi() -> utoipa::openapi::OpenApi {
    let mut doc = openapi::base();
    doc.merge(admin::openapi());
    doc.merge(auth::openapi());
    doc.merge(messages::openapi());
    doc.merge(guilds::openapi());
    doc.merge(social::openapi());
    doc.merge(settings::openapi());
    doc.merge(search::openapi());
    doc.merge(threads::openapi());
    doc.merge(library::openapi());
    doc.merge(cinema::openapi());
    doc.merge(cinema_media::openapi());
    doc.merge(review::openapi());
    doc.merge(voice::openapi());
    doc.merge(cinema_social::openapi());
    doc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::build_router;

    /// Building the full router panics on any duplicate method+path, so a
    /// clash between the guilds, messages and auth routers fails here.
    #[test]
    fn all_routes_build_without_overlap() {
        let state = AppState::builder().build();
        let _app = build_router(state, all());
    }

    #[test]
    fn openapi_lists_guild_and_message_paths() {
        let doc = serde_json::to_value(openapi()).unwrap();
        let paths = doc["paths"].as_object().unwrap();
        assert!(paths.contains_key("/channels/{channel_id}/messages"));
        assert!(paths.keys().any(|p| p.starts_with("/guilds")));
        assert!(paths.contains_key("/users/@me"));
        for p in [
            "/sr/auth/password",
            "/sr/auth/username",
            "/sr/auth/email/change",
            "/users/@me/settings",
            "/users/@me/notification-defaults",
            "/users/@me/privacy",
            "/guilds/{guild_id}/members/search",
            "/guilds/{guild_id}/roles/{role_id}/members",
            "/guilds/{guild_id}/prune",
            "/webhooks/{webhook_id}/reset-token",
        ] {
            assert!(paths.contains_key(p), "missing {p}");
        }
    }

    #[tokio::test]
    async fn ext_reads_registered_service_and_500s_when_missing() {
        use axum::{
            body::Body,
            http::{Request, StatusCode},
            routing::get,
        };
        use tower::ServiceExt;

        async fn read(Ext(v): Ext<String>) -> String {
            (*v).clone()
        }

        let state = AppState::builder()
            .extension(Arc::new("svc".to_owned()))
            .build();
        let app = Router::new()
            .route("/", get(read))
            .with_state(state.clone());
        let resp = app
            .clone()
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let bare = Router::new()
            .route("/", get(read))
            .with_state(AppState::builder().build());
        let resp = bare
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
