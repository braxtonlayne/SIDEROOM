//! Repository traits for the settings domain. Postgres implementations are in
//! `sr-db::settings_repo`; an in-memory one backs the tests.

use async_trait::async_trait;
use sr_core::{EmojiId, GuildId, StickerId, UserId, WebhookId};

use crate::repo::RepoResult;

use super::audit::{AuditEntry, AuditQuery};
use super::automod::AutoModRule;
use super::community::{CommunitySettings, Onboarding, WelcomeScreen};
use super::expressions::{Emoji, Sticker};
use super::invites::GuildInvite;
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

/// Everything the settings service needs.
pub trait SettingsRepository:
    InviteRepository
    + AuditRepository
    + ExpressionRepository
    + AutoModRepository
    + WebhookRepository
    + TemplateRepository
    + CommunityRepository
{
}
impl<
    T: InviteRepository
        + AuditRepository
        + ExpressionRepository
        + AutoModRepository
        + WebhookRepository
        + TemplateRepository
        + CommunityRepository,
> SettingsRepository for T
{
}
