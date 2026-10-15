//! Discord-shaped social endpoints: relationships, DMs and group DMs, private notes,
//! read states, notification settings, profiles and presence preference
//! (docs/specs/lounge-chat.md sections 2-4 and 10; open questions 110-119, 128).
//!
//! The handlers read a [`SocialDeps`] registered on the composition root; the ack
//! and notification-override routes also read [`crate::routes::messages::MessagesDeps`]
//! for channel visibility.
//!
//! ```ignore
//! let deps = SocialDeps::new(DynRepo::new(repo), users, bus, clock, limits, 1, 3)?;
//! let state = AppState::builder().extension(Arc::new(deps)).build();
//! let app = build_router(state, social::router());
//! ```

pub mod deps;
pub mod dms;
pub mod dynrepo;
pub mod error;
pub mod events;
pub mod group_invites;
pub mod model;
pub mod notes;
pub mod notifications;
pub mod presence;
pub mod profile;
pub mod read_state;
pub mod relationships;
pub mod user_settings;
pub mod util;

#[cfg(test)]
mod tests;

pub use deps::SocialDeps;
pub use dynrepo::DynRepo;

use crate::{openapi::ErrorBody, state::AppState};
use axum::{
    Router,
    routing::{get, post, put},
};
use utoipa::OpenApi;

/// All social routes (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/users/@me",
            axum::routing::patch(profile::edit_self).layer(crate::routes::media::body_limit()),
        )
        .route("/users/@me/profile", get(profile::get_own_profile))
        .route("/sr/profile-catalog", get(profile::get_catalog))
        .route("/users/{user_id}/profile", get(profile::get_profile))
        .route(
            "/users/@me/presence",
            get(presence::get_presence).patch(presence::patch_presence),
        )
        .route(
            "/users/@me/relationships",
            get(relationships::list).post(relationships::add),
        )
        .route(
            "/users/@me/relationships/{user_id}",
            get(relationships::get_one)
                .put(relationships::put)
                .patch(relationships::set_nickname)
                .delete(relationships::delete),
        )
        .route("/users/@me/ignores", get(relationships::list_ignores))
        .route(
            "/users/@me/ignores/{user_id}",
            put(relationships::ignore).delete(relationships::unignore),
        )
        .route(
            "/users/@me/notes/{user_id}",
            get(notes::get_note).put(notes::put_note),
        )
        .route("/users/@me/channels", get(dms::list).post(dms::create))
        .route(
            "/users/@me/channels/{channel_id}",
            axum::routing::patch(dms::edit).delete(dms::close),
        )
        .route(
            "/users/@me/channels/{channel_id}/request",
            put(dms::respond_to_request),
        )
        .route(
            "/channels/{channel_id}/recipients/{user_id}",
            put(dms::add_recipient).delete(dms::remove_recipient),
        )
        .route(
            "/channels/{channel_id}/messages/{message_id}/ack",
            post(read_state::ack_message),
        )
        .route(
            "/channels/{channel_id}/pins/ack",
            post(read_state::ack_pins),
        )
        .route("/read-states/ack-bulk", post(read_state::ack_bulk))
        .route("/guilds/{guild_id}/ack", post(read_state::ack_guild))
        .route("/users/@me/read-states", get(read_state::list))
        .route(
            "/users/@me/settings",
            get(user_settings::get_settings).patch(user_settings::patch_settings),
        )
        .route(
            "/users/@me/notification-defaults",
            get(user_settings::get_notification_defaults)
                .patch(user_settings::patch_notification_defaults),
        )
        .route(
            "/users/@me/privacy",
            get(user_settings::get_privacy).patch(user_settings::patch_privacy),
        )
        .route(
            "/users/@me/notification-keywords",
            get(user_settings::get_notification_keywords).put(user_settings::put_notification_keywords),
        )
        .route(
            "/users/@me/guilds/{guild_id}/settings",
            get(notifications::get_settings).patch(notifications::patch_settings),
        )
}

/// OpenAPI fragment for these routes; merge into [`crate::openapi::base`].
#[derive(OpenApi)]
#[openapi(
    paths(
        profile::edit_self,
        profile::get_own_profile,
        profile::get_catalog,
        profile::get_profile,
        presence::get_presence,
        presence::patch_presence,
        relationships::list,
        relationships::add,
        relationships::get_one,
        relationships::put,
        relationships::set_nickname,
        relationships::delete,
        relationships::list_ignores,
        relationships::ignore,
        relationships::unignore,
        notes::get_note,
        notes::put_note,
        dms::list,
        dms::create,
        dms::edit,
        dms::close,
        dms::respond_to_request,
        dms::add_recipient,
        dms::remove_recipient,
        read_state::ack_message,
        read_state::ack_pins,
        read_state::ack_bulk,
        read_state::ack_guild,
        read_state::list,
        notifications::get_settings,
        notifications::patch_settings,
        user_settings::get_settings,
        user_settings::patch_settings,
        user_settings::get_notification_defaults,
        user_settings::patch_notification_defaults,
        user_settings::get_privacy,
        user_settings::patch_privacy,
        user_settings::get_notification_keywords,
        user_settings::put_notification_keywords,
    ),
    components(schemas(
        ErrorBody,
        model::UserObject,
        profile::CatalogEntryObject,
        profile::ProfileCatalogObject,
        user_settings::UserSettingsObject,
        user_settings::PatchUserSettingsBody,
        user_settings::NotificationDefaultsObject,
        user_settings::PatchNotificationDefaultsBody,
        user_settings::PrivacyObject,
        user_settings::PatchPrivacyBody,
        user_settings::NotificationKeywordsObject,
        model::RelationshipObject,
        model::PutRelationshipBody,
        model::AddRelationshipBody,
        model::MutualGuildObject,
        model::NicknameBody,
        model::NoteObject,
        model::NoteBody,
        model::DmChannelObject,
        model::CreateDmBody,
        model::EditDmBody,
        model::MessageRequestBody,
        model::AckBody,
        model::AckResponse,
        model::BulkAckEntry,
        model::BulkAckBody,
        model::ReadStateObject,
        model::ReadStatesResponse,
        model::MuteConfigObject,
        model::ChannelOverrideObject,
        model::UserGuildSettingsObject,
        model::MuteConfigIn,
        model::ChannelOverrideBody,
        model::GuildSettingsBody,
        model::CustomStatusObject,
        model::ConnectionObject,
        model::UserProfileFields,
        model::ProfileObject,
        model::CustomStatusBody,
        model::EditSelfBody,
        model::SelfObject,
        model::PresenceObject,
        model::PresenceBody,
    ))
)]
pub struct SocialApi;

/// The OpenAPI document fragment for the social endpoints.
pub fn openapi() -> utoipa::openapi::OpenApi {
    SocialApi::openapi()
}

/// Tell everyone who knew the account that it is gone (LNG-USET-007): friends get
/// `RELATIONSHIP_REMOVE`, every server it was in gets the member leave, and the account's
/// own open sessions get `SR_ACCOUNT_DELETED` so other tabs sign out. Best effort.
pub async fn announce_account_deleted(
    deps: &SocialDeps,
    account: i64,
    done: &sr_auth::store::DeletedAccount,
) {
    use sr_core::{GuildId, UserId};
    let to_user = |raw: i64| u64::try_from(raw).ok().map(UserId::new);
    let Some(me) = to_user(account) else { return };
    for friend in done.friends.iter().filter_map(|f| to_user(*f)) {
        deps.emit(events::SocialEvent::new(
            "RELATIONSHIP_REMOVE",
            vec![friend],
            serde_json::json!({ "id": me.to_string(), "type": 1, "nickname": null }),
        ))
        .await;
    }
    for guild in done
        .guilds_left
        .iter()
        .filter_map(|g| u64::try_from(*g).ok())
    {
        deps.emit_domain(sr_events::DomainEvent::MemberLeft {
            guild: GuildId::new(guild),
            user: me,
        })
        .await;
    }
    deps.emit(events::SocialEvent::new(
        "SR_ACCOUNT_DELETED",
        vec![me],
        serde_json::json!({ "user_id": me.to_string() }),
    ))
    .await;
}
