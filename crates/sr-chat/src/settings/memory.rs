//! In-memory settings repository for the service tests.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use async_trait::async_trait;
use sr_core::{ChannelId, EmojiId, GuildId, SoundId, StickerId, UserId, WebhookId};

use crate::repo::RepoResult;

use super::applications::{Application, ApplicationForm, ApplicationStatus};
use super::audit::{AuditEntry, AuditQuery};
use super::automod::AutoModRule;
use super::community::{CommunitySettings, Onboarding, WelcomeScreen};
use super::expressions::{Emoji, Sticker};
use super::invites::{GuildInvite, InviteStatus};
use super::repo::{
    ApplicationRepository, AuditRepository, AutoModRepository, CommunityRepository,
    ExpressionRepository, GuideRepository, InsightsRepository, InviteRepository,
    ScheduledEventRepository, TagRepository, TemplateRepository, WebhookRepository,
};
use super::scheduled_events::{EventStatus, ScheduledEvent};
use super::soundboard::Sound;
use super::tags::GuildTag;
use super::templates::GuildTemplate;
use super::webhooks::Webhook;

#[derive(Default)]
pub struct Store {
    pub invites: Vec<GuildInvite>,
    /// Joins through each guild's vanity code (LNG-INV-004).
    pub vanity_uses: HashMap<GuildId, u32>,
    /// Memberships made through a temporary invite (LNG-INV-005).
    pub temporary: HashSet<(GuildId, UserId)>,
    pub bans: HashSet<(GuildId, UserId)>,
    pub audit: Vec<AuditEntry>,
    pub emoji: Vec<Emoji>,
    pub stickers: Vec<Sticker>,
    pub sounds: Vec<Sound>,
    pub rules: Vec<AutoModRule>,
    pub webhooks: Vec<Webhook>,
    pub templates: Vec<GuildTemplate>,
    pub community: HashMap<GuildId, CommunitySettings>,
    pub onboarding: HashMap<GuildId, Onboarding>,
    pub welcome: HashMap<GuildId, WelcomeScreen>,
    pub tags: HashMap<GuildId, GuildTag>,
    pub primary_guilds: HashMap<UserId, GuildId>,
    pub insights: HashMap<GuildId, super::insights::Insights>,
    pub guides: HashMap<GuildId, super::guide::Guide>,
    pub events: Vec<ScheduledEvent>,
    /// `(event, user, created_ms)`.
    pub event_interest: Vec<(u64, UserId, i64)>,
    pub app_forms: HashMap<GuildId, ApplicationForm>,
    pub applications: Vec<Application>,
}

#[derive(Default)]
pub struct MemorySettingsRepo {
    pub store: Mutex<Store>,
}

impl MemorySettingsRepo {
    pub fn with<R>(&self, f: impl FnOnce(&mut Store) -> R) -> R {
        f(&mut self.store.lock().expect("store lock"))
    }
}

#[async_trait]
impl InviteRepository for MemorySettingsRepo {
    async fn insert_invite(&self, invite: &GuildInvite) -> RepoResult<()> {
        self.with(|s| s.invites.push(invite.clone()));
        Ok(())
    }

    async fn get_invite(&self, code: &str) -> RepoResult<Option<GuildInvite>> {
        Ok(self.with(|s| s.invites.iter().find(|i| i.code == code).cloned()))
    }

    async fn list_guild_invites(&self, guild: GuildId) -> RepoResult<Vec<GuildInvite>> {
        Ok(self.with(|s| {
            let mut v: Vec<_> = s
                .invites
                .iter()
                .filter(|i| i.guild_id == guild)
                .cloned()
                .collect();
            v.sort_by_key(|i| std::cmp::Reverse(i.created_at_ms));
            v
        }))
    }

    async fn count_active_invites(&self, guild: GuildId, now_ms: i64) -> RepoResult<usize> {
        Ok(self.with(|s| {
            s.invites
                .iter()
                .filter(|i| i.guild_id == guild && i.status(now_ms) == InviteStatus::Active)
                .count()
        }))
    }

    async fn revoke_invite(&self, code: &str, now_ms: i64) -> RepoResult<bool> {
        Ok(self.with(|s| {
            match s
                .invites
                .iter_mut()
                .find(|i| i.code == code && i.revoked_at_ms.is_none())
            {
                Some(i) => {
                    i.revoked_at_ms = Some(now_ms);
                    true
                }
                None => false,
            }
        }))
    }

    async fn consume_invite(&self, code: &str, now_ms: i64) -> RepoResult<Option<GuildInvite>> {
        Ok(self.with(|s| {
            let inv = s.invites.iter_mut().find(|i| i.code == code)?;
            inv.consume(now_ms).ok()?;
            Some(inv.clone())
        }))
    }

    async fn purge_dead_invites(&self, before_ms: i64) -> RepoResult<u64> {
        Ok(self.with(|s| {
            let n = s.invites.len();
            s.invites.retain(|i| {
                let died = i.revoked_at_ms.into_iter().chain(i.expires_at_ms()).min();
                !died.is_some_and(|d| d < before_ms)
            });
            (n - s.invites.len()) as u64
        }))
    }

    async fn is_banned(&self, guild: GuildId, user: UserId) -> RepoResult<bool> {
        Ok(self.with(|s| s.bans.contains(&(guild, user))))
    }

    async fn vanity_uses(&self, guild: GuildId) -> RepoResult<u32> {
        Ok(self.with(|s| s.vanity_uses.get(&guild).copied().unwrap_or(0)))
    }
}

#[async_trait]
impl AuditRepository for MemorySettingsRepo {
    async fn append_audit(&self, entry: &AuditEntry) -> RepoResult<()> {
        self.with(|s| s.audit.push(entry.clone()));
        Ok(())
    }

    async fn list_audit(&self, query: &AuditQuery) -> RepoResult<Vec<AuditEntry>> {
        Ok(self.with(|s| {
            let mut v: Vec<_> = s
                .audit
                .iter()
                .filter(|e| query.matches(e))
                .cloned()
                .collect();
            v.sort_by_key(|e| std::cmp::Reverse(e.id));
            v.truncate(query.clamped_limit());
            v
        }))
    }

    async fn purge_audit_before(&self, cutoff_ms: i64) -> RepoResult<u64> {
        Ok(self.with(|s| {
            let n = s.audit.len();
            s.audit.retain(|e| e.created_at_ms >= cutoff_ms);
            (n - s.audit.len()) as u64
        }))
    }
}

#[async_trait]
impl ExpressionRepository for MemorySettingsRepo {
    async fn list_emoji(&self, guild: GuildId) -> RepoResult<Vec<Emoji>> {
        Ok(self.with(|s| {
            s.emoji
                .iter()
                .filter(|e| e.guild_id == guild)
                .cloned()
                .collect()
        }))
    }

    async fn get_emoji(&self, guild: GuildId, id: EmojiId) -> RepoResult<Option<Emoji>> {
        Ok(self.with(|s| {
            s.emoji
                .iter()
                .find(|e| e.guild_id == guild && e.id == id)
                .cloned()
        }))
    }

    async fn find_emoji(&self, id: EmojiId) -> RepoResult<Option<Emoji>> {
        Ok(self.with(|s| s.emoji.iter().find(|e| e.id == id).cloned()))
    }

    async fn insert_emoji(&self, emoji: &Emoji) -> RepoResult<()> {
        self.with(|s| s.emoji.push(emoji.clone()));
        Ok(())
    }

    async fn update_emoji(&self, emoji: &Emoji) -> RepoResult<()> {
        self.with(|s| {
            if let Some(e) = s.emoji.iter_mut().find(|e| e.id == emoji.id) {
                *e = emoji.clone();
            }
        });
        Ok(())
    }

    async fn delete_emoji(&self, guild: GuildId, id: EmojiId) -> RepoResult<bool> {
        Ok(self.with(|s| {
            let n = s.emoji.len();
            s.emoji.retain(|e| !(e.guild_id == guild && e.id == id));
            n != s.emoji.len()
        }))
    }

    async fn list_stickers(&self, guild: GuildId) -> RepoResult<Vec<Sticker>> {
        Ok(self.with(|s| {
            s.stickers
                .iter()
                .filter(|e| e.guild_id == guild)
                .cloned()
                .collect()
        }))
    }

    async fn get_sticker(&self, guild: GuildId, id: StickerId) -> RepoResult<Option<Sticker>> {
        Ok(self.with(|s| {
            s.stickers
                .iter()
                .find(|e| e.guild_id == guild && e.id == id)
                .cloned()
        }))
    }

    async fn insert_sticker(&self, sticker: &Sticker) -> RepoResult<()> {
        self.with(|s| s.stickers.push(sticker.clone()));
        Ok(())
    }

    async fn update_sticker(&self, sticker: &Sticker) -> RepoResult<()> {
        self.with(|s| {
            if let Some(e) = s.stickers.iter_mut().find(|e| e.id == sticker.id) {
                *e = sticker.clone();
            }
        });
        Ok(())
    }

    async fn delete_sticker(&self, guild: GuildId, id: StickerId) -> RepoResult<bool> {
        Ok(self.with(|s| {
            let n = s.stickers.len();
            s.stickers.retain(|e| !(e.guild_id == guild && e.id == id));
            n != s.stickers.len()
        }))
    }
    async fn list_sounds(&self, guild: GuildId) -> RepoResult<Vec<Sound>> {
        Ok(self.with(|s| {
            s.sounds
                .iter()
                .filter(|e| e.guild_id == guild)
                .cloned()
                .collect()
        }))
    }

    async fn get_sound(&self, guild: GuildId, id: SoundId) -> RepoResult<Option<Sound>> {
        Ok(self.with(|s| {
            s.sounds
                .iter()
                .find(|e| e.guild_id == guild && e.id == id)
                .cloned()
        }))
    }

    async fn insert_sound(&self, sound: &Sound) -> RepoResult<()> {
        self.with(|s| s.sounds.push(sound.clone()));
        Ok(())
    }

    async fn update_sound(&self, sound: &Sound) -> RepoResult<()> {
        self.with(|s| {
            if let Some(e) = s.sounds.iter_mut().find(|e| e.id == sound.id) {
                *e = sound.clone();
            }
        });
        Ok(())
    }

    async fn delete_sound(&self, guild: GuildId, id: SoundId) -> RepoResult<bool> {
        Ok(self.with(|s| {
            let n = s.sounds.len();
            s.sounds.retain(|e| !(e.guild_id == guild && e.id == id));
            n != s.sounds.len()
        }))
    }
}

#[async_trait]
impl AutoModRepository for MemorySettingsRepo {
    async fn list_rules(&self, guild: GuildId) -> RepoResult<Vec<AutoModRule>> {
        Ok(self.with(|s| {
            s.rules
                .iter()
                .filter(|r| r.guild_id == guild)
                .cloned()
                .collect()
        }))
    }

    async fn get_rule(&self, guild: GuildId, id: u64) -> RepoResult<Option<AutoModRule>> {
        Ok(self.with(|s| {
            s.rules
                .iter()
                .find(|r| r.guild_id == guild && r.id == id)
                .cloned()
        }))
    }

    async fn put_rule(&self, rule: &AutoModRule) -> RepoResult<()> {
        self.with(|s| match s.rules.iter_mut().find(|r| r.id == rule.id) {
            Some(r) => *r = rule.clone(),
            None => s.rules.push(rule.clone()),
        });
        Ok(())
    }

    async fn delete_rule(&self, guild: GuildId, id: u64) -> RepoResult<bool> {
        Ok(self.with(|s| {
            let n = s.rules.len();
            s.rules.retain(|r| !(r.guild_id == guild && r.id == id));
            n != s.rules.len()
        }))
    }
}

#[async_trait]
impl WebhookRepository for MemorySettingsRepo {
    async fn insert_webhook(&self, webhook: &Webhook) -> RepoResult<()> {
        self.with(|s| s.webhooks.push(webhook.clone()));
        Ok(())
    }

    async fn get_webhook(&self, id: WebhookId) -> RepoResult<Option<Webhook>> {
        Ok(self.with(|s| s.webhooks.iter().find(|w| w.id == id).cloned()))
    }

    async fn list_guild_webhooks(&self, guild: GuildId) -> RepoResult<Vec<Webhook>> {
        Ok(self.with(|s| {
            s.webhooks
                .iter()
                .filter(|w| w.guild_id == guild)
                .cloned()
                .collect()
        }))
    }

    async fn list_channel_followers(&self, source: ChannelId) -> RepoResult<Vec<Webhook>> {
        Ok(self.with(|s| {
            s.webhooks
                .iter()
                .filter(|w| {
                    w.kind == super::webhooks::WebhookKind::ChannelFollower
                        && w.source_channel_id == Some(source)
                })
                .cloned()
                .collect()
        }))
    }

    async fn count_channel_webhooks(&self, channel: ChannelId) -> RepoResult<usize> {
        Ok(self.with(|s| {
            s.webhooks
                .iter()
                .filter(|w| w.channel_id == channel)
                .count()
        }))
    }

    async fn update_webhook(&self, webhook: &Webhook) -> RepoResult<()> {
        self.with(|s| {
            if let Some(w) = s.webhooks.iter_mut().find(|w| w.id == webhook.id) {
                *w = webhook.clone();
            }
        });
        Ok(())
    }

    async fn delete_webhook(&self, id: WebhookId) -> RepoResult<bool> {
        Ok(self.with(|s| {
            let n = s.webhooks.len();
            s.webhooks.retain(|w| w.id != id);
            n != s.webhooks.len()
        }))
    }
}

#[async_trait]
impl TemplateRepository for MemorySettingsRepo {
    async fn get_template(&self, code: &str) -> RepoResult<Option<GuildTemplate>> {
        Ok(self.with(|s| s.templates.iter().find(|t| t.code == code).cloned()))
    }

    async fn get_guild_template(&self, guild: GuildId) -> RepoResult<Option<GuildTemplate>> {
        Ok(self.with(|s| s.templates.iter().find(|t| t.guild_id == guild).cloned()))
    }

    async fn put_template(&self, template: &GuildTemplate) -> RepoResult<()> {
        self.with(|s| {
            s.templates.retain(|t| t.guild_id != template.guild_id);
            s.templates.push(template.clone());
        });
        Ok(())
    }

    async fn delete_template(&self, guild: GuildId) -> RepoResult<bool> {
        Ok(self.with(|s| {
            let n = s.templates.len();
            s.templates.retain(|t| t.guild_id != guild);
            n != s.templates.len()
        }))
    }

    async fn bump_template_usage(&self, code: &str) -> RepoResult<()> {
        self.with(|s| {
            if let Some(t) = s.templates.iter_mut().find(|t| t.code == code) {
                t.usage_count += 1;
            }
        });
        Ok(())
    }
}

#[async_trait]
impl CommunityRepository for MemorySettingsRepo {
    async fn get_community(&self, guild: GuildId) -> RepoResult<CommunitySettings> {
        Ok(self.with(|s| {
            s.community
                .get(&guild)
                .cloned()
                .unwrap_or_else(|| CommunitySettings::new(guild))
        }))
    }

    async fn put_community(&self, settings: &CommunitySettings) -> RepoResult<()> {
        self.with(|s| s.community.insert(settings.guild_id, settings.clone()));
        Ok(())
    }

    async fn get_onboarding(&self, guild: GuildId) -> RepoResult<Onboarding> {
        Ok(self.with(|s| {
            s.onboarding.get(&guild).cloned().unwrap_or(Onboarding {
                guild_id: guild,
                ..Onboarding::default()
            })
        }))
    }

    async fn put_onboarding(&self, onboarding: &Onboarding) -> RepoResult<()> {
        self.with(|s| s.onboarding.insert(onboarding.guild_id, onboarding.clone()));
        Ok(())
    }

    async fn get_welcome_screen(&self, guild: GuildId) -> RepoResult<WelcomeScreen> {
        Ok(self.with(|s| {
            s.welcome.get(&guild).cloned().unwrap_or(WelcomeScreen {
                guild_id: guild,
                ..WelcomeScreen::default()
            })
        }))
    }

    async fn put_welcome_screen(&self, screen: &WelcomeScreen) -> RepoResult<()> {
        self.with(|s| s.welcome.insert(screen.guild_id, screen.clone()));
        Ok(())
    }
}

/// The parts of an in-memory chat store the invite join touches: members, guilds and
/// channels. Implemented by both in-memory chat backends so tests can use either.
pub trait ChatMemory: Send + Sync {
    fn with_state<R>(
        &self,
        f: impl FnOnce(
            &mut Vec<crate::domain::Member>,
            &HashMap<GuildId, crate::domain::Guild>,
            &HashMap<ChannelId, crate::domain::Channel>,
        ) -> R,
    ) -> R;
}

impl ChatMemory for crate::memory::MemoryBackend {
    fn with_state<R>(
        &self,
        f: impl FnOnce(
            &mut Vec<crate::domain::Member>,
            &HashMap<GuildId, crate::domain::Guild>,
            &HashMap<ChannelId, crate::domain::Channel>,
        ) -> R,
    ) -> R {
        let mut d = self.data();
        let d = &mut *d;
        f(&mut d.members, &d.guilds, &d.channels)
    }
}

impl ChatMemory for crate::memory::MemoryRepo {
    fn with_state<R>(
        &self,
        f: impl FnOnce(
            &mut Vec<crate::domain::Member>,
            &HashMap<GuildId, crate::domain::Guild>,
            &HashMap<ChannelId, crate::domain::Channel>,
        ) -> R,
    ) -> R {
        let mut s = self.store.lock().unwrap_or_else(|p| p.into_inner());
        let s = &mut *s;
        f(&mut s.members, &s.guilds, &s.channels)
    }
}

#[async_trait]
impl ApplicationRepository for MemorySettingsRepo {
    async fn get_application_form(&self, guild: GuildId) -> RepoResult<ApplicationForm> {
        Ok(self.with(|s| {
            s.app_forms.get(&guild).cloned().unwrap_or(ApplicationForm {
                guild_id: guild,
                ..Default::default()
            })
        }))
    }

    async fn put_application_form(&self, form: &ApplicationForm) -> RepoResult<()> {
        self.with(|s| s.app_forms.insert(form.guild_id, form.clone()));
        Ok(())
    }

    async fn insert_application(&self, app: &Application) -> RepoResult<bool> {
        Ok(self.with(|s| {
            let open = s.applications.iter().any(|a| {
                a.guild_id == app.guild_id
                    && a.user_id == app.user_id
                    && a.status == ApplicationStatus::Pending
            });
            if !open {
                s.applications.push(app.clone());
            }
            !open
        }))
    }

    async fn get_application(&self, guild: GuildId, id: u64) -> RepoResult<Option<Application>> {
        Ok(self.with(|s| {
            s.applications
                .iter()
                .find(|a| a.guild_id == guild && a.id == id)
                .cloned()
        }))
    }

    async fn latest_application(
        &self,
        guild: GuildId,
        user: UserId,
    ) -> RepoResult<Option<Application>> {
        Ok(self.with(|s| {
            s.applications
                .iter()
                .filter(|a| a.guild_id == guild && a.user_id == user)
                .max_by_key(|a| a.id)
                .cloned()
        }))
    }

    async fn list_applications(
        &self,
        guild: GuildId,
        status: Option<ApplicationStatus>,
        before: Option<u64>,
        limit: usize,
    ) -> RepoResult<Vec<Application>> {
        Ok(self.with(|s| {
            let mut v: Vec<Application> = s
                .applications
                .iter()
                .filter(|a| a.guild_id == guild)
                .filter(|a| status.is_none_or(|st| a.status == st))
                .filter(|a| before.is_none_or(|b| a.id < b))
                .cloned()
                .collect();
            v.sort_by_key(|a| std::cmp::Reverse(a.id));
            v.truncate(limit);
            v
        }))
    }

    /// Records the decision only: the member rows live in the chat store, which this
    /// repository cannot see.
    async fn decide_application(
        &self,
        guild: GuildId,
        id: u64,
        approve: bool,
        reviewer: UserId,
        now_ms: i64,
        reason: Option<&str>,
    ) -> RepoResult<Option<Application>> {
        Ok(self.with(|s| {
            let a = s.applications.iter_mut().find(|a| {
                a.guild_id == guild && a.id == id && a.status == ApplicationStatus::Pending
            })?;
            a.status = if approve {
                ApplicationStatus::Approved
            } else {
                ApplicationStatus::Rejected
            };
            a.reviewed_by = Some(reviewer);
            a.reviewed_at_ms = Some(now_ms);
            a.rejection_reason = reason.filter(|_| !approve).map(str::to_owned);
            Some(a.clone())
        }))
    }
}

#[async_trait]
impl GuideRepository for MemorySettingsRepo {
    async fn get_guide(&self, guild: GuildId) -> RepoResult<super::guide::Guide> {
        Ok(self.with(|s| {
            s.guides
                .get(&guild)
                .cloned()
                .unwrap_or(super::guide::Guide {
                    guild_id: guild,
                    ..Default::default()
                })
        }))
    }

    async fn put_guide(&self, guide: &super::guide::Guide) -> RepoResult<()> {
        self.with(|s| s.guides.insert(guide.guild_id, guide.clone()));
        Ok(())
    }
}

#[async_trait]
impl InsightsRepository for MemorySettingsRepo {
    async fn insights(
        &self,
        guild: GuildId,
        days: u32,
        _now_ms: i64,
    ) -> RepoResult<super::insights::Insights> {
        Ok(self.with(|s| {
            let mut i = s.insights.get(&guild).cloned().unwrap_or_default();
            let keep = days as usize;
            if i.days.len() > keep {
                i.days.drain(..i.days.len() - keep);
            }
            i
        }))
    }
}

#[async_trait]
impl TagRepository for MemorySettingsRepo {
    async fn get_guild_tag(&self, guild: GuildId) -> RepoResult<Option<GuildTag>> {
        Ok(self.with(|s| s.tags.get(&guild).cloned()))
    }

    async fn put_guild_tag(&self, tag: &GuildTag) -> RepoResult<()> {
        self.with(|s| s.tags.insert(tag.guild_id, tag.clone()));
        Ok(())
    }

    async fn delete_guild_tag(&self, guild: GuildId) -> RepoResult<bool> {
        Ok(self.with(|s| {
            s.primary_guilds.retain(|_, g| *g != guild);
            s.tags.remove(&guild).is_some()
        }))
    }

    async fn get_primary_guild(&self, user: UserId) -> RepoResult<Option<GuildId>> {
        Ok(self.with(|s| s.primary_guilds.get(&user).copied()))
    }

    async fn set_primary_guild(&self, user: UserId, guild: Option<GuildId>) -> RepoResult<()> {
        self.with(|s| match guild {
            Some(g) => s.primary_guilds.insert(user, g).map(|_| ()),
            None => s.primary_guilds.remove(&user).map(|_| ()),
        });
        Ok(())
    }
}

/// In-memory [`InviteJoinRepository`] over a chat backend and a settings store.
/// Each join runs inside one pair of locks with no await, so it is atomic.
pub struct MemoryInviteJoin<C = crate::memory::MemoryBackend> {
    pub chat: C,
    pub settings: std::sync::Arc<MemorySettingsRepo>,
}

#[async_trait]
impl<C: ChatMemory> super::join::InviteJoinRepository for MemoryInviteJoin<C> {
    async fn join_via_invite(
        &self,
        code: &str,
        user: UserId,
        now_ms: i64,
    ) -> super::SettingsResult<super::join::InviteJoin> {
        use super::SettingsError;
        use super::join::InviteJoin;
        self.settings.with(|s| {
            self.chat.with_state(|members, guilds, channels| {
                // A real invite wins over a vanity code with the same text.
                let (inv, vanity) = match s.invites.iter().find(|i| i.code == code).cloned() {
                    Some(inv) => (inv, false),
                    None => {
                        let g = guilds
                            .values()
                            .find(|g| {
                                g.vanity_code
                                    .as_deref()
                                    .is_some_and(|v| v.eq_ignore_ascii_case(code))
                            })
                            .ok_or(SettingsError::UnknownInvite)?;
                        let channel = g
                            .system_channel_id
                            .or_else(|| {
                                let mut text: Vec<_> = channels
                                    .values()
                                    .filter(|c| {
                                        c.guild_id == Some(g.id)
                                            && matches!(
                                                c.kind,
                                                crate::domain::ChannelType::Text
                                                    | crate::domain::ChannelType::Announcement
                                            )
                                    })
                                    .collect();
                                text.sort_by_key(|c| (c.position, c.id));
                                text.first().map(|c| c.id)
                            })
                            .ok_or(SettingsError::UnknownInvite)?;
                        let uses = s.vanity_uses.get(&g.id).copied().unwrap_or(0);
                        (
                            GuildInvite::vanity(
                                g.vanity_code.clone().unwrap_or_default(),
                                g.id,
                                channel,
                                g.owner_id,
                                uses,
                                now_ms,
                            ),
                            true,
                        )
                    }
                };
                match inv.status(now_ms) {
                    InviteStatus::Revoked => return Err(SettingsError::UnknownInvite),
                    InviteStatus::Expired => return Err(SettingsError::InviteExpired),
                    InviteStatus::Exhausted => return Err(SettingsError::InviteExhausted),
                    InviteStatus::Active => {}
                }
                // The Staff Server is joined through staff tooling only (admin-be).
                if guilds.get(&inv.guild_id).is_some_and(|g| g.is_staff_guild) {
                    return Err(SettingsError::StaffServerInvites);
                }
                if members
                    .iter()
                    .any(|m| m.guild_id == inv.guild_id && m.user_id == user)
                {
                    return Ok(InviteJoin::AlreadyMember { invite: inv });
                }
                let community = s.community.get(&inv.guild_id);
                if community.is_some_and(|c| c.invites_paused) {
                    return Err(SettingsError::InvitesPaused);
                }
                if s.bans.contains(&(inv.guild_id, user)) {
                    return Err(SettingsError::Banned);
                }
                let pending = community.is_some_and(|c| c.community_enabled && c.screening_enabled);
                let member = crate::domain::Member {
                    guild_id: inv.guild_id,
                    user_id: user,
                    nick: None,
                    role_ids: vec![],
                    joined_at_ms: now_ms,
                    communication_disabled_until_ms: None,
                    pending,
                    avatar_key: None,
                    banner_key: None,
                    bio: None,
                    extras: Default::default(),
                };
                members.push(member.clone());
                if inv.temporary {
                    s.temporary.insert((inv.guild_id, user));
                }
                let invite = if vanity {
                    let uses = s.vanity_uses.entry(inv.guild_id).or_insert(0);
                    *uses += 1;
                    GuildInvite { uses: *uses, ..inv }
                } else {
                    let stored = s
                        .invites
                        .iter_mut()
                        .find(|i| i.code == code)
                        .ok_or(SettingsError::UnknownInvite)?;
                    stored.consume(now_ms)?;
                    stored.clone()
                };
                Ok(InviteJoin::Joined { invite, member })
            })
        })
    }

    async fn join_listed(
        &self,
        guild: sr_core::GuildId,
        user: UserId,
        now_ms: i64,
    ) -> super::SettingsResult<Option<crate::domain::Member>> {
        use super::SettingsError;
        self.settings.with(|s| {
            self.chat.with_state(|members, _, _| {
                if members
                    .iter()
                    .any(|m| m.guild_id == guild && m.user_id == user)
                {
                    return Ok(None);
                }
                let community = s.community.get(&guild);
                if community.is_some_and(|c| c.invites_paused) {
                    return Err(SettingsError::InvitesPaused);
                }
                if s.bans.contains(&(guild, user)) {
                    return Err(SettingsError::Banned);
                }
                let member = crate::domain::Member {
                    guild_id: guild,
                    user_id: user,
                    nick: None,
                    role_ids: vec![],
                    joined_at_ms: now_ms,
                    communication_disabled_until_ms: None,
                    pending: community.is_some_and(|c| c.community_enabled && c.screening_enabled),
                    avatar_key: None,
                    banner_key: None,
                    bio: None,
                    extras: Default::default(),
                };
                members.push(member.clone());
                Ok(Some(member))
            })
        })
    }

    async fn take_temporary_memberships(
        &self,
        user: UserId,
    ) -> super::SettingsResult<Vec<sr_core::GuildId>> {
        let mut gone = Vec::new();
        self.settings.with(|s| {
            self.chat.with_state(|members, _, _| {
                let held: Vec<_> = s
                    .temporary
                    .iter()
                    .filter(|(_, u)| *u == user)
                    .map(|(g, _)| *g)
                    .collect();
                for g in held {
                    let has_role = members
                        .iter()
                        .any(|m| m.guild_id == g && m.user_id == user && !m.role_ids.is_empty());
                    if has_role {
                        continue;
                    }
                    members.retain(|m| !(m.guild_id == g && m.user_id == user));
                    s.temporary.remove(&(g, user));
                    gone.push(g);
                }
            });
        });
        Ok(gone)
    }
}

#[async_trait]
impl ScheduledEventRepository for MemorySettingsRepo {
    async fn insert_event(&self, event: &ScheduledEvent) -> RepoResult<()> {
        self.with(|s| s.events.push(event.clone()));
        Ok(())
    }

    async fn get_event(&self, guild: GuildId, id: u64) -> RepoResult<Option<ScheduledEvent>> {
        Ok(self.with(|s| {
            s.events
                .iter()
                .find(|e| e.guild_id == guild && e.id == id)
                .cloned()
        }))
    }

    async fn list_open_events(&self, guild: GuildId) -> RepoResult<Vec<ScheduledEvent>> {
        Ok(self.with(|s| {
            let mut v: Vec<_> = s
                .events
                .iter()
                .filter(|e| e.guild_id == guild && e.status.is_open())
                .cloned()
                .collect();
            v.sort_by_key(|e| (e.start_ms, e.id));
            v
        }))
    }

    async fn count_open_events(&self, guild: GuildId) -> RepoResult<usize> {
        Ok(self.with(|s| {
            s.events
                .iter()
                .filter(|e| e.guild_id == guild && e.status.is_open())
                .count()
        }))
    }

    async fn update_event(&self, event: &ScheduledEvent) -> RepoResult<()> {
        self.with(|s| {
            if let Some(e) = s.events.iter_mut().find(|e| e.id == event.id) {
                *e = event.clone();
            }
        });
        Ok(())
    }

    async fn delete_event(&self, guild: GuildId, id: u64) -> RepoResult<bool> {
        Ok(self.with(|s| {
            let before = s.events.len();
            s.events.retain(|e| !(e.guild_id == guild && e.id == id));
            s.event_interest.retain(|(e, _, _)| *e != id);
            s.events.len() != before
        }))
    }

    async fn add_interest(&self, event: u64, user: UserId, now_ms: i64) -> RepoResult<bool> {
        Ok(self.with(|s| {
            let dup = s
                .event_interest
                .iter()
                .any(|(e, u, _)| *e == event && *u == user);
            if !dup {
                s.event_interest.push((event, user, now_ms));
            }
            !dup
        }))
    }

    async fn remove_interest(&self, event: u64, user: UserId) -> RepoResult<bool> {
        Ok(self.with(|s| {
            let before = s.event_interest.len();
            s.event_interest
                .retain(|(e, u, _)| !(*e == event && *u == user));
            s.event_interest.len() != before
        }))
    }

    async fn interest_counts(&self, events: &[u64]) -> RepoResult<HashMap<u64, u32>> {
        Ok(self.with(|s| {
            let mut out = HashMap::new();
            for (e, _, _) in &s.event_interest {
                if events.contains(e) {
                    *out.entry(*e).or_insert(0) += 1;
                }
            }
            out
        }))
    }

    async fn interested_in(&self, user: UserId, events: &[u64]) -> RepoResult<HashSet<u64>> {
        Ok(self.with(|s| {
            s.event_interest
                .iter()
                .filter(|(e, u, _)| *u == user && events.contains(e))
                .map(|(e, _, _)| *e)
                .collect()
        }))
    }

    async fn list_interested(
        &self,
        event: u64,
        before: Option<UserId>,
        after: Option<UserId>,
        limit: usize,
    ) -> RepoResult<Vec<UserId>> {
        Ok(self.with(|s| {
            let mut v: Vec<UserId> = s
                .event_interest
                .iter()
                .filter(|(e, _, _)| *e == event)
                .map(|(_, u, _)| *u)
                .filter(|u| after.is_none_or(|a| *u > a) && before.is_none_or(|b| *u < b))
                .collect();
            v.sort();
            if before.is_some() && after.is_none() {
                let skip = v.len().saturating_sub(limit);
                v.drain(..skip);
            } else {
                v.truncate(limit);
            }
            v
        }))
    }

    async fn due_events(&self, now_ms: i64) -> RepoResult<Vec<ScheduledEvent>> {
        Ok(self.with(|s| {
            s.events
                .iter()
                .filter(|e| match e.status {
                    EventStatus::Scheduled => e.start_ms <= now_ms,
                    EventStatus::Active => e.end_ms.is_some_and(|end| end <= now_ms),
                    _ => false,
                })
                .cloned()
                .collect()
        }))
    }
}
