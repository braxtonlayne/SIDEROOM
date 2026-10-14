//! Repository traits for the settings domain. Postgres implementations are in
//! `sr-db::settings_repo`; an in-memory one backs the tests.

use async_trait::async_trait;
use sr_core::{EmojiId, GuildId, StickerId, UserId, WebhookId};

use crate::repo::RepoResult;

use super::applications::{Application, ApplicationForm, ApplicationStatus};
use super::audit::{AuditEntry, AuditQuery};
use super::automod::AutoModRule;
use super::community::{CommunitySettings, Onboarding, WelcomeScreen};
use super::expressions::{Emoji, Sticker};
use super::guide::Guide;
use super::insights::Insights;
use super::invites::GuildInvite;
use super::tags::GuildTag;
use super::templates::GuildTemplate;
use super::webhooks::Webhook;

#[async_trait]
pub trait InviteRepository: Send + Sync {
    async fn insert_invite(&self, invite: &GuildInvite) -> RepoResult<()>;
    async fn get_invite(&self, code: &str) -> RepoResult<Option<GuildInvite>>;
    /// Every invite of the guild including expired and revoked ones, newest first.
    async fn list_guild_invites(&self, guild: GuildId) -> RepoResult<Vec<GuildInvite>>;
    /// Invites that are neither revoked, expired nor exhausted.
    async fn count_active_invites(&self, guild: GuildId, now_ms: i64) -> RepoResult<usize>;
    /// Returns whether an open invite was revoked.
    async fn revoke_invite(&self, code: &str, now_ms: i64) -> RepoResult<bool>;
    /// Atomically record one use if the invite is still usable at `now_ms`
    /// (not revoked, not expired, uses < max_uses). Returns the updated invite,
    /// or `None` when it was not usable.
    async fn consume_invite(&self, code: &str, now_ms: i64) -> RepoResult<Option<GuildInvite>>;
    /// Delete invites that expired or were revoked before `before_ms`.
    async fn purge_dead_invites(&self, before_ms: i64) -> RepoResult<u64>;
    async fn is_banned(&self, guild: GuildId, user: UserId) -> RepoResult<bool>;
    /// How many members joined through the guild's vanity code (LNG-INV-004).
    async fn vanity_uses(&self, guild: GuildId) -> RepoResult<u32>;
}

#[async_trait]
pub trait AuditRepository: Send + Sync {
    /// Append-only: there is no update. Entries leave only through the retention purge.
    async fn append_audit(&self, entry: &AuditEntry) -> RepoResult<()>;
    /// Newest first.
    async fn list_audit(&self, query: &AuditQuery) -> RepoResult<Vec<AuditEntry>>;
    /// Retention job: delete entries created before `cutoff_ms`.
    async fn purge_audit_before(&self, cutoff_ms: i64) -> RepoResult<u64>;
}

#[async_trait]
pub trait ExpressionRepository: Send + Sync {
    async fn list_emoji(&self, guild: GuildId) -> RepoResult<Vec<Emoji>>;
    async fn get_emoji(&self, guild: GuildId, id: EmojiId) -> RepoResult<Option<Emoji>>;
    async fn insert_emoji(&self, emoji: &Emoji) -> RepoResult<()>;
    async fn update_emoji(&self, emoji: &Emoji) -> RepoResult<()>;
    async fn delete_emoji(&self, guild: GuildId, id: EmojiId) -> RepoResult<bool>;
    async fn list_stickers(&self, guild: GuildId) -> RepoResult<Vec<Sticker>>;
    async fn get_sticker(&self, guild: GuildId, id: StickerId) -> RepoResult<Option<Sticker>>;
    async fn insert_sticker(&self, sticker: &Sticker) -> RepoResult<()>;
    async fn update_sticker(&self, sticker: &Sticker) -> RepoResult<()>;
    async fn delete_sticker(&self, guild: GuildId, id: StickerId) -> RepoResult<bool>;
}

#[async_trait]
pub trait AutoModRepository: Send + Sync {
    async fn list_rules(&self, guild: GuildId) -> RepoResult<Vec<AutoModRule>>;
    async fn get_rule(&self, guild: GuildId, id: u64) -> RepoResult<Option<AutoModRule>>;
    /// Insert or replace by id.
    async fn put_rule(&self, rule: &AutoModRule) -> RepoResult<()>;
    async fn delete_rule(&self, guild: GuildId, id: u64) -> RepoResult<bool>;
}

#[async_trait]
pub trait WebhookRepository: Send + Sync {
    async fn insert_webhook(&self, webhook: &Webhook) -> RepoResult<()>;
    async fn get_webhook(&self, id: WebhookId) -> RepoResult<Option<Webhook>>;
    async fn list_guild_webhooks(&self, guild: GuildId) -> RepoResult<Vec<Webhook>>;
    async fn count_channel_webhooks(&self, channel: sr_core::ChannelId) -> RepoResult<usize>;
    /// The follower webhooks (kind 2) subscribed to announcements published in `source`.
    async fn list_channel_followers(&self, source: sr_core::ChannelId) -> RepoResult<Vec<Webhook>>;
    async fn update_webhook(&self, webhook: &Webhook) -> RepoResult<()>;
    async fn delete_webhook(&self, id: WebhookId) -> RepoResult<bool>;
}

#[async_trait]
pub trait TemplateRepository: Send + Sync {
    async fn get_template(&self, code: &str) -> RepoResult<Option<GuildTemplate>>;
    async fn get_guild_template(&self, guild: GuildId) -> RepoResult<Option<GuildTemplate>>;
    /// Insert or replace (one template per guild).
    async fn put_template(&self, template: &GuildTemplate) -> RepoResult<()>;
    async fn delete_template(&self, guild: GuildId) -> RepoResult<bool>;
    async fn bump_template_usage(&self, code: &str) -> RepoResult<()>;
}

#[async_trait]
pub trait CommunityRepository: Send + Sync {
    /// Defaults when nothing was saved yet.
    async fn get_community(&self, guild: GuildId) -> RepoResult<CommunitySettings>;
    async fn put_community(&self, settings: &CommunitySettings) -> RepoResult<()>;
    async fn get_onboarding(&self, guild: GuildId) -> RepoResult<Onboarding>;
    async fn put_onboarding(&self, onboarding: &Onboarding) -> RepoResult<()>;
    async fn get_welcome_screen(&self, guild: GuildId) -> RepoResult<WelcomeScreen>;
    async fn put_welcome_screen(&self, screen: &WelcomeScreen) -> RepoResult<()>;
}

#[async_trait]
pub trait TagRepository: Send + Sync {
    async fn get_guild_tag(&self, guild: GuildId) -> RepoResult<Option<GuildTag>>;
    /// Insert or replace (one tag per guild).
    async fn put_guild_tag(&self, tag: &GuildTag) -> RepoResult<()>;
    /// Remove the guild's tag and every member's adoption of it.
    async fn delete_guild_tag(&self, guild: GuildId) -> RepoResult<bool>;
    /// The guild whose tag `user` displays, if they adopted one.
    async fn get_primary_guild(&self, user: UserId) -> RepoResult<Option<GuildId>>;
    async fn set_primary_guild(&self, user: UserId, guild: Option<GuildId>) -> RepoResult<()>;
}

#[async_trait]
pub trait ApplicationRepository: Send + Sync {
    /// A disabled, empty form when nothing was saved yet.
    async fn get_application_form(&self, guild: GuildId) -> RepoResult<ApplicationForm>;
    async fn put_application_form(&self, form: &ApplicationForm) -> RepoResult<()>;
    /// Store a new pending application. `false` when the user already has one pending.
    async fn insert_application(&self, app: &Application) -> RepoResult<bool>;
    async fn get_application(&self, guild: GuildId, id: u64) -> RepoResult<Option<Application>>;
    /// The user's most recent application to the guild.
    async fn latest_application(
        &self,
        guild: GuildId,
        user: UserId,
    ) -> RepoResult<Option<Application>>;
    /// Newest first; `before` is an application id to page from.
    async fn list_applications(
        &self,
        guild: GuildId,
        status: Option<ApplicationStatus>,
        before: Option<u64>,
        limit: usize,
    ) -> RepoResult<Vec<Application>>;
    /// Decide a pending application. Approving also adds the member (the Postgres
    /// implementation does both in one transaction). `None` when it is unknown or no
    /// longer pending.
    async fn decide_application(
        &self,
        guild: GuildId,
        id: u64,
        approve: bool,
        reviewer: UserId,
        now_ms: i64,
        reason: Option<&str>,
    ) -> RepoResult<Option<Application>>;
}

#[async_trait]
pub trait GuideRepository: Send + Sync {
    /// An empty guide when nothing was saved yet.
    async fn get_guide(&self, guild: GuildId) -> RepoResult<Guide>;
    async fn put_guide(&self, guide: &Guide) -> RepoResult<()>;
}

#[async_trait]
pub trait InsightsRepository: Send + Sync {
    /// The last `days` completed UTC days before `now_ms` and the busiest channels in
    /// them.
    async fn insights(&self, guild: GuildId, days: u32, now_ms: i64) -> RepoResult<Insights>;
}

/// Everything the settings service needs.
pub trait SettingsRepository:
    InviteRepository
    + AuditRepository
    + ExpressionRepository
    + AutoModRepository
    + WebhookRepository
    + TemplateRepository
    + CommunityRepository
    + TagRepository
    + InsightsRepository
    + GuideRepository
    + ApplicationRepository
{
}
impl<
    T: InviteRepository
        + AuditRepository
        + ExpressionRepository
        + AutoModRepository
        + WebhookRepository
        + TemplateRepository
        + CommunityRepository
        + TagRepository
        + InsightsRepository
        + GuideRepository
        + ApplicationRepository,
> SettingsRepository for T
{
}
