//! Type-erased settings repository. The routes hold one concrete
//! `SettingsService<DynRepo>`, so the composition root decides the backend
//! (Postgres in production, the in-memory repository in tests).

use async_trait::async_trait;
use sr_chat::repo::RepoResult;
use sr_chat::settings::applications::{Application, ApplicationForm, ApplicationStatus};
use sr_chat::settings::audit::{AuditEntry, AuditQuery};
use sr_chat::settings::automod::{AutoModAlert, AutoModRule};
use sr_chat::settings::community::{CommunitySettings, Onboarding, WelcomeScreen};
use sr_chat::settings::expressions::{Emoji, Sticker};
use sr_chat::settings::guide::Guide;
use sr_chat::settings::insights::Insights;
use sr_chat::settings::invites::GuildInvite;
use sr_chat::settings::repo::{
    ApplicationRepository, AuditRepository, AutoModRepository, CommunityRepository,
    ExpressionRepository, GuideRepository, InsightsRepository, InviteRepository,
    ScheduledEventRepository, SettingsRepository, TagRepository, TemplateRepository,
    WebhookRepository,
};
use sr_chat::settings::scheduled_events::ScheduledEvent;
use sr_chat::settings::soundboard::Sound;
use sr_chat::settings::tags::GuildTag;
use sr_chat::settings::templates::GuildTemplate;
use sr_chat::settings::webhooks::Webhook;
use sr_core::{ChannelId, EmojiId, GuildId, SoundId, StickerId, UserId, WebhookId};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// A shared handle to any [`SettingsRepository`].
#[derive(Clone)]
pub struct DynRepo(pub Arc<dyn SettingsRepository>);

impl DynRepo {
    pub fn new<R: SettingsRepository + 'static>(repo: R) -> Self {
        Self(Arc::new(repo))
    }
}

#[async_trait]
impl InviteRepository for DynRepo {
    async fn insert_invite(&self, invite: &GuildInvite) -> RepoResult<()> {
        self.0.insert_invite(invite).await
    }
    async fn get_invite(&self, code: &str) -> RepoResult<Option<GuildInvite>> {
        self.0.get_invite(code).await
    }
    async fn list_guild_invites(&self, guild: GuildId) -> RepoResult<Vec<GuildInvite>> {
        self.0.list_guild_invites(guild).await
    }
    async fn count_active_invites(&self, guild: GuildId, now_ms: i64) -> RepoResult<usize> {
        self.0.count_active_invites(guild, now_ms).await
    }
    async fn revoke_invite(&self, code: &str, now_ms: i64) -> RepoResult<bool> {
        self.0.revoke_invite(code, now_ms).await
    }
    async fn consume_invite(&self, code: &str, now_ms: i64) -> RepoResult<Option<GuildInvite>> {
        self.0.consume_invite(code, now_ms).await
    }
    async fn purge_dead_invites(&self, before_ms: i64) -> RepoResult<u64> {
        self.0.purge_dead_invites(before_ms).await
    }
    async fn is_banned(&self, guild: GuildId, user: UserId) -> RepoResult<bool> {
        self.0.is_banned(guild, user).await
    }
    async fn vanity_uses(&self, guild: GuildId) -> RepoResult<u32> {
        self.0.vanity_uses(guild).await
    }
}

#[async_trait]
impl AuditRepository for DynRepo {
    async fn append_audit(&self, entry: &AuditEntry) -> RepoResult<()> {
        self.0.append_audit(entry).await
    }
    async fn list_audit(&self, query: &AuditQuery) -> RepoResult<Vec<AuditEntry>> {
        self.0.list_audit(query).await
    }
    async fn purge_audit_before(&self, cutoff_ms: i64) -> RepoResult<u64> {
        self.0.purge_audit_before(cutoff_ms).await
    }
}

#[async_trait]
impl ExpressionRepository for DynRepo {
    async fn list_emoji(&self, guild: GuildId) -> RepoResult<Vec<Emoji>> {
        self.0.list_emoji(guild).await
    }
    async fn get_emoji(&self, guild: GuildId, id: EmojiId) -> RepoResult<Option<Emoji>> {
        self.0.get_emoji(guild, id).await
    }
    async fn find_emoji(&self, id: EmojiId) -> RepoResult<Option<Emoji>> {
        self.0.find_emoji(id).await
    }
    async fn insert_emoji(&self, emoji: &Emoji) -> RepoResult<()> {
        self.0.insert_emoji(emoji).await
    }
    async fn update_emoji(&self, emoji: &Emoji) -> RepoResult<()> {
        self.0.update_emoji(emoji).await
    }
    async fn delete_emoji(&self, guild: GuildId, id: EmojiId) -> RepoResult<bool> {
        self.0.delete_emoji(guild, id).await
    }
    async fn list_stickers(&self, guild: GuildId) -> RepoResult<Vec<Sticker>> {
        self.0.list_stickers(guild).await
    }
    async fn get_sticker(&self, guild: GuildId, id: StickerId) -> RepoResult<Option<Sticker>> {
        self.0.get_sticker(guild, id).await
    }
    async fn insert_sticker(&self, sticker: &Sticker) -> RepoResult<()> {
        self.0.insert_sticker(sticker).await
    }
    async fn update_sticker(&self, sticker: &Sticker) -> RepoResult<()> {
        self.0.update_sticker(sticker).await
    }
    async fn delete_sticker(&self, guild: GuildId, id: StickerId) -> RepoResult<bool> {
        self.0.delete_sticker(guild, id).await
    }
    async fn list_sounds(&self, guild: GuildId) -> RepoResult<Vec<Sound>> {
        self.0.list_sounds(guild).await
    }
    async fn get_sound(&self, guild: GuildId, id: SoundId) -> RepoResult<Option<Sound>> {
        self.0.get_sound(guild, id).await
    }
    async fn insert_sound(&self, sound: &Sound) -> RepoResult<()> {
        self.0.insert_sound(sound).await
    }
    async fn update_sound(&self, sound: &Sound) -> RepoResult<()> {
        self.0.update_sound(sound).await
    }
    async fn delete_sound(&self, guild: GuildId, id: SoundId) -> RepoResult<bool> {
        self.0.delete_sound(guild, id).await
    }
}

#[async_trait]
impl AutoModRepository for DynRepo {
    async fn list_rules(&self, guild: GuildId) -> RepoResult<Vec<AutoModRule>> {
        self.0.list_rules(guild).await
    }
    async fn get_rule(&self, guild: GuildId, id: u64) -> RepoResult<Option<AutoModRule>> {
        self.0.get_rule(guild, id).await
    }
    async fn put_rule(&self, rule: &AutoModRule) -> RepoResult<()> {
        self.0.put_rule(rule).await
    }
    async fn delete_rule(&self, guild: GuildId, id: u64) -> RepoResult<bool> {
        self.0.delete_rule(guild, id).await
    }
    async fn insert_alert(&self, alert: &AutoModAlert) -> RepoResult<()> {
        self.0.insert_alert(alert).await
    }
    async fn get_alert(&self, id: u64) -> RepoResult<Option<AutoModAlert>> {
        self.0.get_alert(id).await
    }
    async fn resolve_alert(&self, id: u64, user: UserId, now_ms: i64) -> RepoResult<bool> {
        self.0.resolve_alert(id, user, now_ms).await
    }
}

#[async_trait]
impl WebhookRepository for DynRepo {
    async fn insert_webhook(&self, webhook: &Webhook) -> RepoResult<()> {
        self.0.insert_webhook(webhook).await
    }
    async fn get_webhook(&self, id: WebhookId) -> RepoResult<Option<Webhook>> {
        self.0.get_webhook(id).await
    }
    async fn list_guild_webhooks(&self, guild: GuildId) -> RepoResult<Vec<Webhook>> {
        self.0.list_guild_webhooks(guild).await
    }
    async fn list_channel_followers(&self, source: ChannelId) -> RepoResult<Vec<Webhook>> {
        self.0.list_channel_followers(source).await
    }
    async fn count_channel_webhooks(&self, channel: ChannelId) -> RepoResult<usize> {
        self.0.count_channel_webhooks(channel).await
    }
    async fn update_webhook(&self, webhook: &Webhook) -> RepoResult<()> {
        self.0.update_webhook(webhook).await
    }
    async fn delete_webhook(&self, id: WebhookId) -> RepoResult<bool> {
        self.0.delete_webhook(id).await
    }
}

#[async_trait]
impl TemplateRepository for DynRepo {
    async fn get_template(&self, code: &str) -> RepoResult<Option<GuildTemplate>> {
        self.0.get_template(code).await
    }
    async fn get_guild_template(&self, guild: GuildId) -> RepoResult<Option<GuildTemplate>> {
        self.0.get_guild_template(guild).await
    }
    async fn put_template(&self, template: &GuildTemplate) -> RepoResult<()> {
        self.0.put_template(template).await
    }
    async fn delete_template(&self, guild: GuildId) -> RepoResult<bool> {
        self.0.delete_template(guild).await
    }
    async fn bump_template_usage(&self, code: &str) -> RepoResult<()> {
        self.0.bump_template_usage(code).await
    }
}

#[async_trait]
impl CommunityRepository for DynRepo {
    async fn get_community(&self, guild: GuildId) -> RepoResult<CommunitySettings> {
        self.0.get_community(guild).await
    }
    async fn put_community(&self, settings: &CommunitySettings) -> RepoResult<()> {
        self.0.put_community(settings).await
    }
    async fn mark_raid(
        &self,
        guild: GuildId,
        now_ms: i64,
        cooldown_ms: i64,
        pause_until_ms: i64,
    ) -> RepoResult<Option<bool>> {
        self.0
            .mark_raid(guild, now_ms, cooldown_ms, pause_until_ms)
            .await
    }
    async fn clear_invite_pause(&self, guild: GuildId) -> RepoResult<bool> {
        self.0.clear_invite_pause(guild).await
    }
    async fn get_onboarding(&self, guild: GuildId) -> RepoResult<Onboarding> {
        self.0.get_onboarding(guild).await
    }
    async fn put_onboarding(&self, onboarding: &Onboarding) -> RepoResult<()> {
        self.0.put_onboarding(onboarding).await
    }
    async fn get_welcome_screen(&self, guild: GuildId) -> RepoResult<WelcomeScreen> {
        self.0.get_welcome_screen(guild).await
    }
    async fn put_welcome_screen(&self, screen: &WelcomeScreen) -> RepoResult<()> {
        self.0.put_welcome_screen(screen).await
    }
}

#[async_trait]
impl TagRepository for DynRepo {
    async fn get_guild_tag(&self, guild: GuildId) -> RepoResult<Option<GuildTag>> {
        self.0.get_guild_tag(guild).await
    }
    async fn put_guild_tag(&self, tag: &GuildTag) -> RepoResult<()> {
        self.0.put_guild_tag(tag).await
    }
    async fn delete_guild_tag(&self, guild: GuildId) -> RepoResult<bool> {
        self.0.delete_guild_tag(guild).await
    }
    async fn get_primary_guild(&self, user: UserId) -> RepoResult<Option<GuildId>> {
        self.0.get_primary_guild(user).await
    }
    async fn set_primary_guild(&self, user: UserId, guild: Option<GuildId>) -> RepoResult<()> {
        self.0.set_primary_guild(user, guild).await
    }
}

#[async_trait]
impl InsightsRepository for DynRepo {
    async fn insights(&self, guild: GuildId, days: u32, now_ms: i64) -> RepoResult<Insights> {
        self.0.insights(guild, days, now_ms).await
    }
}

#[async_trait]
impl GuideRepository for DynRepo {
    async fn get_guide(&self, guild: GuildId) -> RepoResult<Guide> {
        self.0.get_guide(guild).await
    }
    async fn put_guide(&self, guide: &Guide) -> RepoResult<()> {
        self.0.put_guide(guide).await
    }
}

#[async_trait]
impl ApplicationRepository for DynRepo {
    async fn get_application_form(&self, guild: GuildId) -> RepoResult<ApplicationForm> {
        self.0.get_application_form(guild).await
    }
    async fn put_application_form(&self, form: &ApplicationForm) -> RepoResult<()> {
        self.0.put_application_form(form).await
    }
    async fn insert_application(&self, app: &Application) -> RepoResult<bool> {
        self.0.insert_application(app).await
    }
    async fn get_application(&self, guild: GuildId, id: u64) -> RepoResult<Option<Application>> {
        self.0.get_application(guild, id).await
    }
    async fn latest_application(
        &self,
        guild: GuildId,
        user: UserId,
    ) -> RepoResult<Option<Application>> {
        self.0.latest_application(guild, user).await
    }
    async fn list_applications(
        &self,
        guild: GuildId,
        status: Option<ApplicationStatus>,
        before: Option<u64>,
        limit: usize,
    ) -> RepoResult<Vec<Application>> {
        self.0.list_applications(guild, status, before, limit).await
    }
    async fn decide_application(
        &self,
        guild: GuildId,
        id: u64,
        approve: bool,
        reviewer: UserId,
        now_ms: i64,
        reason: Option<&str>,
    ) -> RepoResult<Option<Application>> {
        self.0
            .decide_application(guild, id, approve, reviewer, now_ms, reason)
            .await
    }
}

#[async_trait]
impl ScheduledEventRepository for DynRepo {
    async fn insert_event(&self, event: &ScheduledEvent) -> RepoResult<()> {
        self.0.insert_event(event).await
    }
    async fn get_event(&self, guild: GuildId, id: u64) -> RepoResult<Option<ScheduledEvent>> {
        self.0.get_event(guild, id).await
    }
    async fn list_open_events(&self, guild: GuildId) -> RepoResult<Vec<ScheduledEvent>> {
        self.0.list_open_events(guild).await
    }
    async fn count_open_events(&self, guild: GuildId) -> RepoResult<usize> {
        self.0.count_open_events(guild).await
    }
    async fn update_event(&self, event: &ScheduledEvent) -> RepoResult<()> {
        self.0.update_event(event).await
    }
    async fn delete_event(&self, guild: GuildId, id: u64) -> RepoResult<bool> {
        self.0.delete_event(guild, id).await
    }
    async fn add_interest(&self, event: u64, user: UserId, now_ms: i64) -> RepoResult<bool> {
        self.0.add_interest(event, user, now_ms).await
    }
    async fn remove_interest(&self, event: u64, user: UserId) -> RepoResult<bool> {
        self.0.remove_interest(event, user).await
    }
    async fn interest_counts(&self, events: &[u64]) -> RepoResult<HashMap<u64, u32>> {
        self.0.interest_counts(events).await
    }
    async fn interested_in(&self, user: UserId, events: &[u64]) -> RepoResult<HashSet<u64>> {
        self.0.interested_in(user, events).await
    }
    async fn list_interested(
        &self,
        event: u64,
        before: Option<UserId>,
        after: Option<UserId>,
        limit: usize,
    ) -> RepoResult<Vec<UserId>> {
        self.0.list_interested(event, before, after, limit).await
    }
    async fn due_events(&self, now_ms: i64) -> RepoResult<Vec<ScheduledEvent>> {
        self.0.due_events(now_ms).await
    }
}
