//! Discord-shaped guild REST API (LNG-GLD-002/004/005/006/007, LNG-CHN-002/003):
//! guilds, channels and categories, permission overwrites, roles, members, bans.
//!
//! Wiring, in the composition root:
//!
//! ```ignore
//! let svc = ChatService::new(PgChatRepo::new(pool), clock, ids, ChatLimits::default());
//! let guilds = guilds::with_backend(guilds::backend::backend(svc, bus.clone()));
//! let state = AppState::builder()
//!     .openapi({ let mut d = openapi::base(); d.merge(guilds::openapi()); d })
//!     .build();
//! let app = build_router(state, guilds);
//! ```
//!
//! All permission decisions are made inside `sr-chat`; handlers only translate
//! JSON. Every successful mutation publishes a `DomainEvent` that the gateway
//! turns into the matching dispatch.

pub mod backend;
pub mod bans;
pub mod channels;
pub mod dto;
pub mod guild;
pub mod members;
pub mod roles;
pub mod util;

use crate::state::AppState;
use axum::{Extension, Router};
use std::sync::Arc;
use utoipa::OpenApi;

pub use backend::GuildBackend;
pub use util::GuildApi;

/// All guild routes (relative to `/api/v1`). Handlers need a [`GuildBackend`]:
/// install it with [`with_backend`] (or `.layer(Extension(GuildApi(..)))`).
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(guild::routes())
        .merge(channels::routes())
        .merge(roles::routes())
        .merge(members::routes())
        .merge(bans::routes())
}

/// [`router`] with the backend installed.
pub fn with_backend(backend: Arc<dyn GuildBackend>) -> Router<AppState> {
    router().layer(Extension(GuildApi(backend)))
}

/// OpenAPI fragment for these routes; merge it into [`crate::openapi::base`].
#[derive(OpenApi)]
#[openapi(
    paths(
        guild::create_guild,
        guild::get_guild,
        guild::modify_guild,
        guild::delete_guild,
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
        bans::list_bans,
        bans::get_ban,
        bans::create_ban,
        bans::remove_ban,
    ),
    components(schemas(
        dto::UserRef,
        dto::RoleObject,
        dto::OverwriteObject,
        dto::ChannelObject,
        dto::GuildObject,
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
        dto::BanBody,
    ))
)]
pub struct GuildsApiDoc;

/// The OpenAPI document fragment for the guild endpoints.
pub fn openapi() -> utoipa::openapi::OpenApi {
    GuildsApiDoc::openapi()
}
