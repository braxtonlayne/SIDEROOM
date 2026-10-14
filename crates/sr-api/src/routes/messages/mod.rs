//! Discord-shaped message endpoints: history, create/edit/delete, bulk delete,
//! reactions, pins, typing and upload slots (LNG-MSG-002..-009, -011, -013, -020;
//! FND-STOR-004).
//!
//! Mount with [`router`] under the API version prefix. The services the handlers
//! need are registered on the composition root as [`MessagesDeps`] through
//! [`crate::AppStateBuilder::extension`] and read with [`crate::routes::Ext`]:
//!
//! ```ignore
//! let deps = MessagesDeps::new(repo, store, storage, bus, entitlements, clock, 1, 1)?;
//! let state = AppState::builder().extension(Arc::new(deps)).build();
//! let app = build_router(state, messages::router());
//! ```

pub mod attachments;
pub mod deps;
pub mod error;
pub mod list;

pub mod call_record;
pub mod model;
pub mod pins;
pub mod polls;
pub mod reactions;
pub mod render;
pub mod rich;
pub mod rich_create;
pub mod rich_jobs;
pub mod rich_model;
pub mod send;
pub mod store;
pub mod typing;

#[cfg(test)]
mod tests;

pub use deps::{DmAnnouncer, FixedEntitlements, MessagesDeps, UploadEntitlements};
pub use rich::{JobScheduler, RichDeps, UnfurlGate};

use crate::{openapi::ErrorBody, state::AppState};
use axum::{
    Router,
    routing::{delete, get, post, put},
};
use utoipa::OpenApi;

/// All message routes (relative to the API version prefix). Handlers require a
/// [`MessagesDeps`] registered on [`AppState`].
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
            "/channels/{channel_id}/polls/{message_id}/answers/{answer_id}",
            get(polls::voters),
        )
        .route(
            "/channels/{channel_id}/polls/{message_id}/answers/{answer_id}/voters/@me",
            put(polls::vote).delete(polls::unvote),
        )
        .route(
            "/channels/{channel_id}/polls/{message_id}/expire",
            post(polls::expire),
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
        polls::vote,
        polls::unvote,
        polls::voters,
        polls::expire,
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
        polls::PollVoters,
    ))
)]
pub struct MessagesApi;

/// The OpenAPI document fragment for the message endpoints.
pub fn openapi() -> utoipa::openapi::OpenApi {
    MessagesApi::openapi()
}
