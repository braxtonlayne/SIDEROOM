//! In-memory settings repository for the service tests.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use async_trait::async_trait;
use sr_core::{ChannelId, EmojiId, GuildId, StickerId, UserId, WebhookId};

use crate::repo::RepoResult;

use super::audit::{AuditEntry, AuditQuery};
use super::automod::AutoModRule;
use super::community::{CommunitySettings, Onboarding, WelcomeScreen};
use super::expressions::{Emoji, Sticker};
use super::invites::{GuildInvite, InviteStatus};
use super::repo::{
    AuditRepository, AutoModRepository, CommunityRepository, ExpressionRepository,
    InviteRepository, TagRepository, TemplateRepository, WebhookRepository,
};
use super::tags::GuildTag;
use super::templates::GuildTemplate;
use super::webhooks::Webhook;

#[derive(Default)]
pub struct Store {
    pub invites: Vec<GuildInvite>,
    pub bans: HashSet<(GuildId, UserId)>,
    pub audit: Vec<AuditEntry>,
    pub emoji: Vec<Emoji>,
    pub stickers: Vec<Sticker>,
    pub rules: Vec<AutoModRule>,
    pub webhooks: Vec<Webhook>,
    pub templates: Vec<GuildTemplate>,
    pub community: HashMap<GuildId, CommunitySettings>,
    pub onboarding: HashMap<GuildId, Onboarding>,
    pub welcome: HashMap<GuildId, WelcomeScreen>,
    pub tags: HashMap<GuildId, GuildTag>,
    pub primary_guilds: HashMap<UserId, GuildId>,
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
pub struct MemoryInviteJoin {
    pub chat: crate::memory::MemoryBackend,
    pub settings: std::sync::Arc<MemorySettingsRepo>,
}

#[async_trait]
impl super::join::InviteJoinRepository for MemoryInviteJoin {
    async fn join_via_invite(
        &self,
        code: &str,
        user: UserId,
        now_ms: i64,
    ) -> super::SettingsResult<super::join::InviteJoin> {
        use super::SettingsError;
        use super::join::InviteJoin;
        self.settings.with(|s| {
            let inv = s
                .invites
                .iter()
                .find(|i| i.code == code)
                .cloned()
                .ok_or(SettingsError::UnknownInvite)?;
            match inv.status(now_ms) {
                InviteStatus::Revoked => return Err(SettingsError::UnknownInvite),
                InviteStatus::Expired => return Err(SettingsError::InviteExpired),
                InviteStatus::Exhausted => return Err(SettingsError::InviteExhausted),
                InviteStatus::Active => {}
            }
            let mut data = self.chat.data();
            if data
                .members
                .iter()
                .any(|m| m.guild_id == inv.guild_id && m.user_id == user)
            {
                return Ok(InviteJoin::AlreadyMember { invite: inv });
            }
            if s.community
                .get(&inv.guild_id)
                .is_some_and(|c| c.invites_paused)
            {
                return Err(SettingsError::InvitesPaused);
            }
            if s.bans.contains(&(inv.guild_id, user)) {
                return Err(SettingsError::Banned);
            }
            let member = crate::domain::Member {
                guild_id: inv.guild_id,
                user_id: user,
                nick: None,
                role_ids: vec![],
                joined_at_ms: now_ms,
                communication_disabled_until_ms: None,
                pending: false,
            };
            data.members.push(member.clone());
            let stored = s
                .invites
                .iter_mut()
                .find(|i| i.code == code)
                .ok_or(SettingsError::UnknownInvite)?;
            stored.consume(now_ms)?;
            Ok(InviteJoin::Joined {
                invite: stored.clone(),
                member,
            })
        })
    }
}
