//! Discord-shaped message endpoints: history, create/edit/delete, bulk delete,
//! reactions, pins, typing and upload slots (LNG-MSG-002..-009, -011, -013, -020;
//! FND-STOR-004).
//!
//! Mount with [`router_with`] (collaborators supplied) under the API version prefix:
//!
//! ```ignore
//! let deps = Arc::new(MessagesDeps::new(repo, store, storage, bus, entitlements, clock, 1, 1)?);
//! let app = build_router(state, messages::router_with(deps));
//! ```
//!
//! [`router`] alone has no collaborators; requests fail with 500 until a
//! `MessagesDeps` extension is layered on. `AppState` has no slot for chat services,
//! hence the extension (open question in `docs/open-questions.md`).

pub mod attachments;
pub mod deps;
pub mod error;
pub mod list;
pub mod memory;
pub mod model;
pub mod pins;
pub mod reactions;
pub mod render;
pub mod send;
pub mod store;
pub mod typing;

#[cfg(test)]
mod tests;

pub use deps::{FixedEntitlements, MessagesDeps, UploadEntitlements};
pub use memory::MemoryBackend;
pub use store::MessageStore;

use crate::{openapi::ErrorBody, state::AppState};
use axum::{
    Extension, Router,
    routing::{delete, get, post, put},
};
use std::sync::Arc;
use utoipa::OpenApi;

/// All message routes (relative to the API version prefix). Requires a
/// [`MessagesDeps`] extension; see [`router_with`].
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/channels/{channel_id}/messages",
            get(list::list_messages).post(send::create_message),
        )
        .route(
            "/channels/{channel_id}/messages/bulk-delete",
            post(send::bulk_delete),
        )
        .route("/channels/{channel_id}/messages/pins", get(pins::list_pins))
        .route(
            "/channels/{channel_id}/messages/{message_id}",
            get(list::get_message)
                .patch(send::edit_message)
                .delete(send::delete_message),
        )
        .route(
            "/channels/{channel_id}/messages/{message_id}/reactions",
            delete(reactions::clear_all),
        )
        .route(
            "/channels/{channel_id}/messages/{message_id}/reactions/{emoji}",
            get(reactions::list_users).delete(reactions::clear_emoji),
        )
        .route(
            "/channels/{channel_id}/messages/{message_id}/reactions/{emoji}/@me",
            put(reactions::add_own).delete(reactions::remove_own),
        )
        .route(
            "/channels/{channel_id}/messages/{message_id}/reactions/{emoji}/{user_id}",
            delete(reactions::remove_user),
        )
        .route("/channels/{channel_id}/pins", get(pins::list_pins_legacy))
        .route(
            "/channels/{channel_id}/pins/{message_id}",
            put(pins::pin).delete(pins::unpin),
        )
        .route(
            "/channels/{channel_id}/typing",
            post(typing::trigger_typing),
        )
        .route(
            "/channels/{channel_id}/attachments",
            post(attachments::request_slots),
        )
        .route(
            "/channels/{channel_id}/attachments/{upload_id}/complete",
            post(attachments::complete_upload),
        )
}

/// [`router`] with its collaborators attached.
pub fn router_with(deps: Arc<MessagesDeps>) -> Router<AppState> {
    router().layer(Extension(deps))
}

/// OpenAPI fragment for these routes; merge into [`crate::openapi::base`].
#[derive(OpenApi)]
#[openapi(
    paths(
        list::list_messages,
        list::get_message,
        send::create_message,
        send::edit_message,
        send::delete_message,
        send::bulk_delete,
        reactions::add_own,
        reactions::remove_own,
        reactions::remove_user,
        reactions::list_users,
        reactions::clear_all,
        reactions::clear_emoji,
        pins::list_pins,
        pins::list_pins_legacy,
        pins::pin,
        pins::unpin,
        typing::trigger_typing,
        attachments::request_slots,
        attachments::complete_upload,
    ),
    components(schemas(
        ErrorBody,
        model::MessageObject,
        model::UserRef,
        model::AttachmentObject,
        model::PartialEmoji,
        model::ReactionObject,
        model::MessageReferenceObject,
        model::CreateMessageBody,
        model::MessageReferenceIn,
        model::AttachmentRef,
        model::EditMessageBody,
        model::BulkDeleteBody,
        model::UploadSlotsBody,
        model::FileSpec,
        model::UploadSlot,
        model::UploadSlotsResponse,
        model::UserListEntry,
        model::PinEntry,
        model::PinsPage,
    ))
)]
pub struct MessagesApi;

/// The OpenAPI document fragment for the message endpoints.
pub fn openapi() -> utoipa::openapi::OpenApi {
    MessagesApi::openapi()
}
