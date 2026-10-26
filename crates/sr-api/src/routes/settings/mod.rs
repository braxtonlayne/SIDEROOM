//! Server-settings REST API (docs/specs/lounge-chat.md section 11, inventory
//! `06-lounge-server-settings.md`): invites, audit log, emoji and stickers,
//! AutoMod rules, webhooks, templates, community/onboarding settings and server tags.
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

pub mod alerts;
pub mod applications;
pub mod audit;
pub mod automod;
pub mod automod_gate;
pub mod community;
pub mod deps;
pub mod event_world;
pub mod events;
pub mod expressions;
pub mod features;
pub mod features_scopes;
pub mod follow;
pub mod guide;
pub mod insights;
pub mod invites;
pub mod join;
pub mod presence;
pub mod repo;
pub mod scheduled_events;
pub mod soundboard;
pub mod tags;
pub mod templates;
pub mod timeout;
pub mod webhooks;

#[cfg(test)]
mod tests;

pub use automod_gate::{
    AutoModGate, AutoModGateHandle, AutoModVerdict, MessageCheck, alert_execution_payload,
};
pub use deps::{Entropy, SettingsDeps};
pub use event_world::{EventStartNotifier, EventWorld, NoNotifier, NoWorld};
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
        .merge(soundboard::routes())
        .merge(automod::routes())
        .merge(webhooks::routes())
        .merge(follow::routes())
        .merge(templates::routes())
        .merge(community::routes())
        .merge(alerts::routes())
        .merge(tags::routes())
        .merge(insights::routes())
        .merge(guide::routes())
        .merge(features::routes())
        .merge(features_scopes::routes())
        .merge(applications::routes())
        .merge(scheduled_events::routes())
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
        expressions::list_my_expressions,
        expressions::list_emojis,
        expressions::get_emoji,
        expressions::emoji_url,
        expressions::sticker_url,
        expressions::create_emoji,
        expressions::modify_emoji,
        expressions::delete_emoji,
        expressions::list_stickers,
        expressions::get_default_stickers,
        expressions::set_default_stickers,
        expressions::get_sticker,
        expressions::create_sticker,
        expressions::modify_sticker,
        expressions::delete_sticker,
        soundboard::list_default_sounds,
        soundboard::default_sound_audio,
        soundboard::list_sounds,
        soundboard::get_sound,
        soundboard::sound_audio,
        soundboard::create_sound,
        soundboard::modify_sound,
        soundboard::delete_sound,
        soundboard::send_sound,
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
        follow::follow_channel,
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
        community::put_incident_actions,
        alerts::alert_action,
        community::get_onboarding,
        community::put_onboarding,
        community::get_welcome_screen,
        community::modify_welcome_screen,
        community::get_member_verification,
        community::accept_screening,
        community::post_onboarding_responses,
        tags::get_tag,
        tags::put_tag,
        tags::delete_tag,
        tags::get_primary_guild,
        tags::put_primary_guild,
        insights::get_insights,
        guide::get_guide,
        guide::put_guide,
        features::client_flags,
        features::appearance_defaults,
        features::compliance_public,
        features::list_features,
        features::patch_feature,
        features::list_temp_roles,
        features::grant_temp_role,
        features::slowmode_presets,
        features::apply_slowmode,
        features::revoke_temp_role,
        features_scopes::list_channel_features,
        features_scopes::patch_channel_feature,
        features_scopes::reset_channel_feature,
        features_scopes::list_user_features,
        features_scopes::patch_user_feature,
        features_scopes::reset_user_feature,
        applications::get_form,
        applications::put_form,
        applications::submit_application,
        applications::get_my_application,
        applications::list_applications,
        applications::decide_application,
        scheduled_events::list_events,
        scheduled_events::create_event,
        scheduled_events::get_event,
        scheduled_events::modify_event,
        scheduled_events::delete_event,
        scheduled_events::list_interested,
        scheduled_events::mark_interested,
        scheduled_events::clear_interested,
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
        expressions::DefaultStickerPacksBody,
        expressions::ModifyStickerBody,
        soundboard::CreateSoundBody,
        soundboard::ModifySoundBody,
        soundboard::SendSoundBody,
        automod::RuleObject,
        automod::RuleBody,
        automod::TriggerMetadata,
        automod::ActionObject,
        automod::ActionMetadata,
        webhooks::WebhookObject,
        webhooks::CreateWebhookBody,
        webhooks::ModifyWebhookBody,
        webhooks::ExecuteWebhookBody,
        follow::FollowBody,
        follow::FollowedChannelObject,
        templates::TemplateObject,
        templates::BuiltinTemplateObject,
        templates::GuildFromTemplateBody,
        templates::CreateTemplateBody,
        templates::ModifyTemplateBody,
        community::CommunityObject,
        community::ModifyCommunityBody,
        community::IncidentActionsBody,
        alerts::AlertActionBody,
        alerts::AlertActionResult,
        community::OnboardingObject,
        community::OnboardingDto,
        community::PromptDto,
        community::OptionDto,
        community::WelcomeScreenObject,
        community::WelcomeChannelDto,
        community::ModifyWelcomeScreenBody,
        community::MemberVerificationObject,
        community::VerificationField,
        community::AcceptScreeningBody,
        community::OnboardingResponseBody,
        community::OnboardingResultObject,
        tags::GuildTagObject,
        tags::PutGuildTagBody,
        tags::PrimaryGuildResponse,
        tags::PutPrimaryGuildBody,
        insights::InsightsObject,
        guide::GuideObject,
        guide::GuideTodoDto,
        guide::GuideResourceDto,
        guide::PutGuideBody,
        features::GuildFeatureObject,
        features::ClientFlagsObject,
        features::AppearanceDefaultsObject,
        features::PublicComplianceObject,
        features::PatchGuildFeatureBody,
        features::TempRoleObject,
        features::GrantTempRoleBody,
        features::SlowmodePresetObject,
        features::SlowmodePresetsObject,
        features::ApplySlowmodeBody,
        features_scopes::ChannelFeatureObject,
        features_scopes::PatchChannelFeatureBody,
        features_scopes::UserFeatureObject,
        features_scopes::PatchUserFeatureBody,
        features::AppliedSlowmodeObject,
        features::ApplySlowmodeResult,
        applications::QuestionDto,
        applications::PutFormBody,
        applications::FormObject,
        applications::AnswerDto,
        applications::ApplicationObject,
        applications::AnswerBody,
        applications::SubmitBody,
        applications::DecisionBody,
        scheduled_events::EventMetadata,
        scheduled_events::ScheduledEventObject,
        scheduled_events::CreateEventBody,
        scheduled_events::ModifyEventBody,
        scheduled_events::EventUserObject,
        insights::InsightsDayObject,
        insights::InsightsChannelObject,
        crate::routes::social::model::PrimaryGuildObject,
    ))
)]
pub struct SettingsApiDoc;

/// The OpenAPI document fragment for the settings endpoints.
pub fn openapi() -> utoipa::openapi::OpenApi {
    SettingsApiDoc::openapi()
}
