//! Discord-shaped guild REST API (LNG-GLD-002/004/005/006/007, LNG-CHN-002/003):
//! guilds, channels and categories, permission overwrites, roles, members, bans.
//!
//! Wiring, in the composition root:
//!
//! ```ignore
//! let svc = ChatService::new(PgChatRepo::new(pool), clock, ids, ChatLimits::default());
//! let guilds = guilds::GuildApi(guilds::backend::backend(svc, bus.clone()));
//! let state = AppState::builder()
//!     .openapi({ let mut d = openapi::base(); d.merge(guilds::openapi()); d })
//!     .extension(Arc::new(guilds))
//!     .build();
//! let app = build_router(state, guilds::router());
//! ```
//!
//! All permission decisions are made inside `sr-chat`; handlers only translate
//! JSON. Every successful mutation publishes a `DomainEvent` that the gateway
//! turns into the matching dispatch.

pub mod backend;
pub mod bans;
pub mod bridge;
pub mod bridge_sync;
pub mod channels;
pub mod dto;
pub mod guild;
pub mod members;
pub mod prune;
pub mod roles;
pub mod safety;
pub mod scrapbook;
pub mod util;
pub mod vanity;

use crate::state::AppState;
use axum::Router;
use utoipa::OpenApi;

pub use backend::GuildBackend;
pub use util::GuildApi;

/// All guild routes (relative to `/api/v1`). Handlers need a [`GuildApi`] registered
/// on the [`AppState`].
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(guild::routes())
        .merge(channels::routes())
        .merge(roles::routes())
        .merge(members::routes())
        .merge(prune::routes())
        .merge(bans::routes())
        .merge(vanity::routes())
        .merge(safety::routes())
        .merge(scrapbook::router())
        .merge(bridge::router())
        .merge(bridge_sync::router())
}

/// OpenAPI fragment for these routes; merge it into [`crate::openapi::base`].
#[derive(OpenApi)]
#[openapi(
    paths(
        guild::create_guild,
        guild::get_guild,
        guild::modify_guild,
        guild::delete_guild,
        guild::modify_sr_profile,
        guild::list_my_guilds,
        guild::leave_guild,
        channels::list_channels,
        channels::create_channel,
        channels::reorder_channels,
        channels::get_channel,
        channels::modify_channel,
        channels::delete_channel,
        channels::put_overwrite,
        channels::delete_overwrite,
        roles::list_roles,
        roles::create_role,
        roles::reorder_roles,
        roles::modify_role,
        roles::delete_role,
        roles::add_member_role,
        roles::remove_member_role,
        members::list_members,
        members::get_member,
        members::modify_member,
        members::kick_member,
        members::get_own_member,
        members::modify_own_member,
        members::search_members,
        roles::list_role_members,
        prune::prune_count,
        prune::prune,
        bans::list_bans,
        bans::get_ban,
        bans::create_ban,
        bans::remove_ban,
        bans::bulk_ban,
        vanity::get_vanity,
        vanity::set_vanity,
        safety::set_mfa_level,
        safety::get_widget,
        safety::set_widget,
        safety::reauth_status,
        safety::reauth,
    ),
    components(schemas(
        dto::UserRef,
        dto::RoleObject,
        dto::OverwriteObject,
        dto::ChannelObject,
        dto::BridgeChannelState,
        dto::GuildObject,
        dto::IncidentsDataObject,
        dto::GuildPreviewObject,
        dto::MemberObject,
        dto::BanObject,
        dto::CreateGuildBody,
        dto::ModifyGuildBody,
        dto::OverwriteBody,
        dto::CreateChannelBody,
        dto::ModifyChannelBody,
        dto::ReorderChannelEntry,
        dto::CreateRoleBody,
        dto::ModifyRoleBody,
        dto::ReorderRoleEntry,
        dto::ModifyMemberBody,
        members::ModifyOwnMemberBody,
        dto::BanBody,
        prune::PruneBody,
        prune::PruneResult,
        bans::BulkBanBody,
        vanity::VanityObject,
        vanity::VanityBody,
        safety::MfaStatus,
        safety::ReauthBody,
        safety::MfaLevelBody,
        safety::MfaLevelObject,
        safety::WidgetSettings,
        safety::ModifyWidgetBody,
        bans::BulkBanResult,
    ))
)]
pub struct GuildsApiDoc;

/// The OpenAPI document fragment for the guild endpoints.
pub fn openapi() -> utoipa::openapi::OpenApi {
    let mut doc = GuildsApiDoc::openapi();
    doc.merge(scrapbook::openapi());
    doc.merge(bridge::openapi());
    doc.merge(bridge_sync::openapi());
    doc
}

#[cfg(test)]
mod scrapbook_tests;
