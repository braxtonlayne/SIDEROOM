//! Thread and forum endpoints (LNG-THR-001/002, LNG-FRM-001): start a thread from a
//! message or on its own, forum and media posts, membership, active and archived
//! listings. `PATCH`/`DELETE /channels/{thread}` are served by the guild channel routes,
//! which hand thread ids to [`ThreadsDeps::modify`] and [`ThreadsDeps::delete`].
//! Messages inside threads use the ordinary message endpoints.
//!
//! Register [`ThreadsDeps`] with [`crate::AppStateBuilder::extension`]; the start
//! handlers also need [`MessagesDeps`](crate::routes::messages::MessagesDeps).

pub mod deps;
pub mod dto;
pub mod list;
pub mod members;
pub mod start;

#[cfg(test)]
mod tests;

pub use deps::{ThreadOps, ThreadsDeps};

use crate::{openapi::ErrorBody, state::AppState};
use axum::{
    Router,
    routing::{get, post, put},
};
use utoipa::OpenApi;

/// All thread routes (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/channels/{channel_id}/messages/{message_id}/threads",
            post(start::start_from_message),
        )
        .route("/channels/{channel_id}/threads", post(start::start_thread))
        .route(
            "/channels/{channel_id}/thread-members",
            get(members::list_members),
        )
        .route(
            "/channels/{channel_id}/thread-members/{user_id}",
            put(members::add_member)
                .delete(members::remove_member)
                .get(members::get_member),
        )
        .route("/guilds/{guild_id}/threads/active", get(list::list_active))
        .route(
            "/channels/{channel_id}/threads/archived/public",
            get(list::archived_public),
        )
        .route(
            "/channels/{channel_id}/threads/archived/private",
            get(list::archived_private),
        )
        .route(
            "/channels/{channel_id}/users/@me/threads/archived/private",
            get(list::archived_joined_private),
        )
}

/// OpenAPI fragment for these routes; merge into [`crate::openapi::base`].
#[derive(OpenApi)]
#[openapi(
    paths(
        start::start_from_message,
        start::start_thread,
        members::add_member,
        members::remove_member,
        members::get_member,
        members::list_members,
        list::list_active,
        list::archived_public,
        list::archived_private,
        list::archived_joined_private,
    ),
    components(schemas(
        ErrorBody,
        dto::StartFromMessageBody,
        dto::StartThreadBody,
        dto::ThreadResponse,
        dto::ThreadList,
    ))
)]
pub struct ThreadsApi;

/// The OpenAPI document fragment for the thread endpoints.
pub fn openapi() -> utoipa::openapi::OpenApi {
    ThreadsApi::openapi()
}
