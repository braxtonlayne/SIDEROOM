//! Server-settings REST API (docs/specs/lounge-chat.md section 11, inventory
//! `06-lounge-server-settings.md`): invites, audit log, emoji and stickers,
//! AutoMod rules, webhooks, templates and community/onboarding settings.
//!
//! Wiring, in the composition root:
//!
//! ```ignore
//! let deps = SettingsDeps::new(Arc::new(PgSettingsRepo::new(pool)), chat_repo,
//!     storage, clock, entropy, worker, process)?;
//! let deps = Arc::new(deps);
//! let state = AppState::builder()
//!     .extension(deps.clone())
//!     .extension(Arc::new(AutoModGateHandle(deps.clone())))
//!     .build();
//! let app = build_router(state, settings::router());
//! ```
//!
//! Permissions are decided in `sr-chat` (`SettingsService`); handlers resolve the
//! acting member's guild or channel permissions with [`SettingsDeps`] and
//! translate JSON. Gateway events leave through [`events::SettingsEvents`] and a
//! joined member through [`join::MemberJoiner`] (the composition root wires the
//! bus-backed [`join::InviteMemberJoiner`]).

pub mod audit;
pub mod automod;
pub mod automod_gate;
pub mod community;
pub mod deps;
pub mod events;
pub mod expressions;
pub mod invites;
pub mod join;
pub mod presence;
pub mod repo;
pub mod templates;
pub mod timeout;
pub mod webhooks;

#[cfg(test)]
mod tests;

pub use automod_gate::{
    AutoModGate, AutoModGateHandle, AutoModVerdict, MessageCheck, alert_execution_payload,
};
pub use deps::{Entropy, SettingsDeps};
pub use events::{SettingsEvent, SettingsEvents};
pub use join::{InviteMemberJoiner, MemberJoiner, UnwiredMemberJoiner};
pub use presence::PresenceCounter;
pub use repo::DynRepo;
pub use timeout::{AdminMemberTimeouter, MemberTimeouter, UnwiredMemberTimeouter};

use crate::{openapi::ErrorBody, state::AppState};
use axum::Router;
use utoipa::OpenApi;

/// All settings routes (relative to the API version prefix). Handlers require a
/// [`SettingsDeps`] registered on [`AppState`]; webhook execution also needs the
/// messages module's `MessagesDeps`.
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(invites::routes())
        .merge(audit::routes())
        .merge(expressions::routes())
        .merge(automod::routes())
        .merge(webhooks::routes())
        .merge(templates::routes())
        .merge(community::routes())
}

/// OpenAPI fragment for these routes; merge into [`crate::openapi::base`].
#[derive(OpenApi)]
#[openapi(
    paths(
        invites::create_invite,
        invites::list_channel_invites,
        invites::list_guild_invites,
        invites::get_invite,
        invites::delete_invite,
        invites::accept_invite,
        audit::list_audit_log,
        expressions::list_emojis,
        expressions::get_emoji,
        expressions::emoji_url,
        expressions::sticker_url,
        expressions::create_emoji,
        expressions::modify_emoji,
        expressions::delete_emoji,
        expressions::list_stickers,
        expressions::get_sticker,
        expressions::create_sticker,
        expressions::modify_sticker,
        expressions::delete_sticker,
        automod::list_rules,
        automod::get_rule,
        automod::create_rule,
        automod::modify_rule,
        automod::delete_rule,
        webhooks::create_webhook,
        webhooks::list_channel_webhooks,
        webhooks::list_guild_webhooks,
        webhooks::get_webhook,
        webhooks::modify_webhook,
        webhooks::delete_webhook,
        webhooks::reset_webhook_token,
        webhooks::get_webhook_with_token,
        webhooks::modify_webhook_with_token,
        webhooks::delete_webhook_with_token,
        webhooks::execute_webhook,
        templates::list_builtin_templates,
        templates::create_guild_from_template,
        templates::get_template,
        templates::list_guild_templates,
        templates::create_template,
        templates::sync_template,
        templates::modify_template,
        templates::delete_template,
        community::get_community,
        community::modify_community,
        community::get_onboarding,
        community::put_onboarding,
        community::get_welcome_screen,
        community::modify_welcome_screen,
    ),
    components(schemas(
        ErrorBody,
        invites::InviteObject,
        invites::InviteGuild,
        invites::InviteChannel,
        invites::CreateInviteBody,
        audit::AuditLogObject,
        audit::AuditEntryObject,
        audit::AuditChangeObject,
        expressions::EmojiObject,
        expressions::SignedUrlObject,
        expressions::StickerObject,
        expressions::CreateEmojiBody,
        expressions::ModifyEmojiBody,
        expressions::CreateStickerBody,
        expressions::ModifyStickerBody,
        automod::RuleObject,
        automod::RuleBody,
        automod::TriggerMetadata,
        automod::ActionObject,
        automod::ActionMetadata,
        webhooks::WebhookObject,
        webhooks::CreateWebhookBody,
        webhooks::ModifyWebhookBody,
        webhooks::ExecuteWebhookBody,
        templates::TemplateObject,
        templates::BuiltinTemplateObject,
        templates::GuildFromTemplateBody,
        templates::CreateTemplateBody,
        templates::ModifyTemplateBody,
        community::CommunityObject,
        community::ModifyCommunityBody,
        community::OnboardingObject,
        community::OnboardingDto,
        community::PromptDto,
        community::OptionDto,
        community::WelcomeScreenObject,
        community::WelcomeChannelDto,
        community::ModifyWelcomeScreenBody,
    ))
)]
pub struct SettingsApiDoc;

/// The OpenAPI document fragment for the settings endpoints.
pub fn openapi() -> utoipa::openapi::OpenApi {
    SettingsApiDoc::openapi()
}
