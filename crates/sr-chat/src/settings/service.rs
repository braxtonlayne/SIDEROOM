//! Orchestration for the settings domain. Callers pass the actor guild-level
//! `Permissions` (already computed from roles); the service enforces them and
//! writes audit entries for mutations.

use std::sync::Arc;

use sr_core::permissions::Permissions;
use sr_core::{ChannelId, Clock, EmojiId, GuildId, RoleId, SnowflakeGenerator, StickerId, UserId};

use super::audit::{
    AuditAction, AuditChange, AuditEntry, AuditQuery, AuditRetention, SYSTEM_USER_ID,
};
use super::automod::{
    self, AutoModRule, CompiledRule, MessageContext, PresetLists, ProfileContext, RuleCache,
    RuleMatch, SpamWindow, Trigger,
};
use super::error::{SettingsError, SettingsResult};
use super::expressions::{
    self, Emoji, ExpressionLimits, Sticker, StickerFormat, check_new_emoji, check_new_sticker,
};
use super::invites::{self, GuildInvite, InviteOptions, MAX_INVITES_PER_GUILD};
use super::repo::SettingsRepository;

/// Called with every audit entry right after it is stored (best effort, must not
/// block: spawn for async work). Used to push `GUILD_AUDIT_LOG_ENTRY_CREATE`.
pub type AuditObserver = Arc<dyn Fn(&AuditEntry) + Send + Sync>;

/// Source of invite-code entropy.
pub type Entropy = Arc<dyn Fn() -> u64 + Send + Sync>;

mod application_ops;
mod community_ops;
mod event_ops;
mod expression_ops;
mod sound_ops;
mod tag_ops;
mod template_ops;
mod webhook_ops;

pub use event_ops::EventStats;
pub use expression_ops::StickerPatch;
pub use sound_ops::{NewSound, SoundPatch};
pub use webhook_ops::WebhookPatch;

pub struct SettingsService<R> {
    repo: R,
    clock: Arc<dyn Clock>,
    ids: SnowflakeGenerator<Arc<dyn Clock>>,
    entropy: Entropy,
    limits: ExpressionLimits,
    retention: AuditRetention,
    presets: PresetLists,
    /// Compiled rules per guild (open question 136).
    rule_cache: RuleCache,
    /// Per-member message history for the rate and duplicate checks.
    spam: SpamWindow,
    audit_observer: Option<AuditObserver>,
}

pub(super) fn require(perms: Permissions, needed: Permissions) -> SettingsResult<()> {
    if perms.has(needed) {
        Ok(())
    } else {
        Err(SettingsError::MissingPermissions)
    }
}

/// Fields of a new emoji, already uploaded to storage.
#[derive(Debug, Clone)]
pub struct NewEmoji {
    pub name: String,
    pub animated: bool,
    pub size_bytes: u64,
    pub media_key: String,
    pub role_ids: Vec<RoleId>,
}

#[derive(Debug, Clone)]
pub struct NewSticker {
    pub name: String,
    pub description: Option<String>,
    pub tags: String,
    pub format: StickerFormat,
    pub size_bytes: u64,
    pub media_key: String,
}

impl<R: SettingsRepository> SettingsService<R> {
    pub fn new(
        repo: R,
        clock: Arc<dyn Clock>,
        ids: SnowflakeGenerator<Arc<dyn Clock>>,
        entropy: Entropy,
    ) -> Self {
        Self {
            repo,
            clock,
            ids,
            entropy,
            limits: ExpressionLimits::default(),
            retention: AuditRetention::default(),
            presets: PresetLists::new(),
            rule_cache: RuleCache::default(),
            spam: SpamWindow::default(),
            audit_observer: None,
        }
    }

    /// Observe every stored audit entry.
    pub fn with_audit_observer(mut self, observer: AuditObserver) -> Self {
        self.audit_observer = Some(observer);
        self
    }

    pub fn with_spam_window(mut self, spam: SpamWindow) -> Self {
        self.spam = spam;
        self
    }

    pub fn with_rule_cache(mut self, cache: RuleCache) -> Self {
        self.rule_cache = cache;
        self
    }

    /// A fresh snowflake from the service's generator.
    pub fn new_id(&self) -> u64 {
        self.ids.generate().get()
    }

    /// A random word from the injected entropy source (tokens, codes).
    pub fn random_u64(&self) -> u64 {
        (self.entropy)()
    }

    /// Current time from the injected clock.
    pub fn now_ms(&self) -> i64 {
        self.now()
    }

    /// Drop the guild's compiled AutoMod rules (call after any rule change made
    /// outside this service, e.g. by another node's invalidation event).
    pub fn invalidate_rules(&self, guild: GuildId) {
        self.rule_cache.invalidate(guild);
    }

    pub fn with_limits(mut self, limits: ExpressionLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn with_retention(mut self, retention: AuditRetention) -> Self {
        self.retention = retention;
        self
    }

    pub fn with_presets(mut self, presets: PresetLists) -> Self {
        self.presets = presets;
        self
    }

    pub fn repo(&self) -> &R {
        &self.repo
    }

    fn now(&self) -> i64 {
        self.clock.now_unix_ms()
    }

    // ---- audit log ----------------------------------------------------------

    /// Append an audit entry (LNG-SSET-056). `reason` is `X-Audit-Log-Reason`.
    pub async fn record_audit(
        &self,
        guild: GuildId,
        actor: UserId,
        action: AuditAction,
        target_id: Option<u64>,
        changes: Vec<AuditChange>,
        reason: Option<&str>,
    ) -> SettingsResult<AuditEntry> {
        let entry = AuditEntry::new(
            self.ids.generate().get(),
            guild,
            actor,
            action,
            target_id,
            reason,
            self.now(),
        )?
        .with_changes(changes);
        self.store_audit(&entry).await?;
        Ok(entry)
    }

    /// Append an entry written by a built-in feature module (D-187): the actor is the system
    /// user and `options.module` names the module, so the viewer shows "SIDE ROOM · <module>".
    pub async fn record_audit_as_module(
        &self,
        guild: GuildId,
        action: AuditAction,
        target_id: Option<u64>,
        changes: Vec<AuditChange>,
        reason: Option<&str>,
        module: (&str, &str),
    ) -> SettingsResult<AuditEntry> {
        let entry = AuditEntry::new(
            self.ids.generate().get(),
            guild,
            UserId::new(SYSTEM_USER_ID),
            action,
            target_id,
            reason,
            self.now(),
        )?
        .with_changes(changes)
        .with_options(serde_json::json!({ "module": module.1, "module_id": module.0 }));
        self.store_audit(&entry).await?;
        Ok(entry)
    }

    /// Append an audit entry that carries Discord's per-action `options`
    /// (`channel_id`, `count`, `message_id`, ...).
    pub async fn record_audit_with_options(
        &self,
        guild: GuildId,
        actor: UserId,
        action: AuditAction,
        target_id: Option<u64>,
        options: serde_json::Value,
    ) -> SettingsResult<AuditEntry> {
        let entry = AuditEntry::new(
            self.ids.generate().get(),
            guild,
            actor,
            action,
            target_id,
            None,
            self.now(),
        )?
        .with_options(options);
        self.store_audit(&entry).await?;
        Ok(entry)
    }

    /// Store an already built entry (one carrying `options`, say) and notify the
    /// audit observer.
    pub async fn store_audit(&self, entry: &AuditEntry) -> SettingsResult<()> {
        self.repo.append_audit(entry).await?;
        if let Some(observe) = &self.audit_observer {
            observe(entry);
        }
        Ok(())
    }

    pub async fn list_audit(
        &self,
        perms: Permissions,
        query: &AuditQuery,
    ) -> SettingsResult<Vec<AuditEntry>> {
        require(perms, Permissions::VIEW_AUDIT_LOG)?;
        Ok(self.repo.list_audit(query).await?)
    }

    /// Retention job body; returns the number of purged entries.
    pub async fn purge_audit(&self) -> SettingsResult<u64> {
        match self.retention.cutoff_ms(self.now()) {
            Some(cutoff) => Ok(self.repo.purge_audit_before(cutoff).await?),
            None => Ok(0),
        }
    }

    // ---- invites ------------------------------------------------------------

    pub async fn create_invite(
        &self,
        perms: Permissions,
        guild: GuildId,
        channel: ChannelId,
        inviter: UserId,
        opts: InviteOptions,
    ) -> SettingsResult<GuildInvite> {
        require(perms, Permissions::CREATE_INSTANT_INVITE)?;
        let now = self.now();
        if self.repo.count_active_invites(guild, now).await? >= MAX_INVITES_PER_GUILD {
            return Err(SettingsError::MaxInvites);
        }
        // Codes are short; retry on the rare collision.
        for _ in 0..5 {
            let code = invites::generate_code(|| (self.entropy)());
            if self.repo.get_invite(&code).await?.is_some() {
                continue;
            }
            let invite = GuildInvite::new(code, guild, channel, inviter, opts, now)?;
            self.repo.insert_invite(&invite).await?;
            self.record_audit(
                guild,
                inviter,
                AuditAction::InviteCreate,
                None,
                vec![AuditChange::new(
                    "code",
                    None,
                    Some(invite.code.clone().into()),
                )],
                None,
            )
            .await?;
            return Ok(invite);
        }
        Err(SettingsError::Repo(crate::error::RepoError(
            "could not allocate an invite code".into(),
        )))
    }

    /// Resolve and consume an invite for `user` (joining is the caller job).
    /// Honors paused invites, bans, expiry and max uses; the use count is bumped
    /// atomically by the repository.
    pub async fn use_invite(&self, code: &str, user: UserId) -> SettingsResult<GuildInvite> {
        let now = self.now();
        let invite = self
            .repo
            .get_invite(code)
            .await?
            .ok_or(SettingsError::UnknownInvite)?;
        invite.check_usable(now)?;
        if self
            .repo
            .get_community(invite.guild_id)
            .await?
            .invites_paused
        {
            return Err(SettingsError::InvitesPaused);
        }
        if self.repo.is_banned(invite.guild_id, user).await? {
            return Err(SettingsError::Banned);
        }
        match self.repo.consume_invite(code, now).await? {
            Some(updated) => Ok(updated),
            // Lost a race for the last use (or it expired in between): classify.
            None => {
                let current = self
                    .repo
                    .get_invite(code)
                    .await?
                    .ok_or(SettingsError::UnknownInvite)?;
                current.check_usable(now)?;
                Err(SettingsError::InviteExhausted)
            }
        }
    }

    pub async fn revoke_invite(
        &self,
        perms: Permissions,
        actor: UserId,
        guild: GuildId,
        code: &str,
        reason: Option<&str>,
    ) -> SettingsResult<()> {
        require(perms, Permissions::MANAGE_GUILD)?;
        let invite = self
            .repo
            .get_invite(code)
            .await?
            .filter(|i| i.guild_id == guild)
            .ok_or(SettingsError::UnknownInvite)?;
        if !self.repo.revoke_invite(code, self.now()).await? {
            return Err(SettingsError::UnknownInvite);
        }
        self.record_audit(
            guild,
            actor,
            AuditAction::InviteDelete,
            None,
            vec![AuditChange::new("code", Some(invite.code.into()), None)],
            reason,
        )
        .await?;
        Ok(())
    }

    /// Security Actions: pause or resume all invites (LNG-SSET-038).
    pub async fn set_invites_paused(
        &self,
        perms: Permissions,
        actor: UserId,
        guild: GuildId,
        paused: bool,
    ) -> SettingsResult<()> {
        require(perms, Permissions::MANAGE_GUILD)?;
        let mut s = self.repo.get_community(guild).await?;
        if s.invites_paused == paused {
            return Ok(());
        }
        s.invites_paused = paused;
        self.repo.put_community(&s).await?;
        self.record_audit(
            guild,
            actor,
            AuditAction::GuildUpdate,
            Some(guild.get()),
            vec![AuditChange::new(
                "invites_paused",
                Some((!paused).into()),
                Some(paused.into()),
            )],
            None,
        )
        .await?;
        Ok(())
    }

    // ---- emoji and stickers ---------------------------------------------------

    pub async fn create_emoji(
        &self,
        perms: Permissions,
        actor: UserId,
        guild: GuildId,
        new: NewEmoji,
    ) -> SettingsResult<Emoji> {
        require(
            perms,
            Permissions::MANAGE_GUILD_EXPRESSIONS | Permissions::CREATE_GUILD_EXPRESSIONS,
        )
        .or_else(|_| require(perms, Permissions::MANAGE_GUILD_EXPRESSIONS))
        .or_else(|_| require(perms, Permissions::CREATE_GUILD_EXPRESSIONS))?;
        let existing = self.repo.list_emoji(guild).await?;
        check_new_emoji(
            &existing,
            &new.name,
            new.animated,
            new.size_bytes,
            &new.role_ids,
            &self.limits,
        )?;
        let emoji = Emoji {
            id: EmojiId::new(self.ids.generate().get()),
            guild_id: guild,
            name: new.name,
            animated: new.animated,
            available: true,
            role_ids: new.role_ids,
            creator_id: actor,
            media_key: new.media_key,
            created_at_ms: self.now(),
        };
        self.repo.insert_emoji(&emoji).await?;
        self.record_audit(
            guild,
            actor,
            AuditAction::EmojiCreate,
            Some(emoji.id.get()),
            vec![AuditChange::new(
                "name",
                None,
                Some(emoji.name.clone().into()),
            )],
            None,
        )
        .await?;
        Ok(emoji)
    }

    pub async fn rename_emoji(
        &self,
        perms: Permissions,
        actor: UserId,
        guild: GuildId,
        id: EmojiId,
        name: &str,
    ) -> SettingsResult<Emoji> {
        require(perms, Permissions::MANAGE_GUILD_EXPRESSIONS)?;
        expressions::validate_emoji_name(name)?;
        let mut emoji = self
            .repo
            .get_emoji(guild, id)
            .await?
            .ok_or(SettingsError::UnknownEmoji)?;
        let old = std::mem::replace(&mut emoji.name, name.to_owned());
        self.repo.update_emoji(&emoji).await?;
        self.record_audit(
            guild,
            actor,
            AuditAction::EmojiUpdate,
            Some(id.get()),
            vec![AuditChange::new(
                "name",
                Some(old.into()),
                Some(name.to_owned().into()),
            )],
            None,
        )
        .await?;
        Ok(emoji)
    }

    pub async fn delete_emoji(
        &self,
        perms: Permissions,
        actor: UserId,
        guild: GuildId,
        id: EmojiId,
    ) -> SettingsResult<()> {
        require(perms, Permissions::MANAGE_GUILD_EXPRESSIONS)?;
        if !self.repo.delete_emoji(guild, id).await? {
            return Err(SettingsError::UnknownEmoji);
        }
        self.record_audit(
            guild,
            actor,
            AuditAction::EmojiDelete,
            Some(id.get()),
            vec![],
            None,
        )
        .await?;
        Ok(())
    }

    pub async fn create_sticker(
        &self,
        perms: Permissions,
        actor: UserId,
        guild: GuildId,
        new: NewSticker,
    ) -> SettingsResult<Sticker> {
        require(perms, Permissions::MANAGE_GUILD_EXPRESSIONS)?;
        expressions::validate_sticker(&new.name, new.description.as_deref(), &new.tags)?;
        let existing = self.repo.list_stickers(guild).await?;
        check_new_sticker(&existing, new.size_bytes, &self.limits)?;
        let sticker = Sticker {
            id: StickerId::new(self.ids.generate().get()),
            guild_id: guild,
            name: new.name.trim().to_owned(),
            description: new.description.filter(|d| !d.is_empty()),
            tags: new.tags,
            format: new.format,
            available: true,
            creator_id: actor,
            media_key: new.media_key,
            created_at_ms: self.now(),
        };
        self.repo.insert_sticker(&sticker).await?;
        self.record_audit(
            guild,
            actor,
            AuditAction::StickerCreate,
            Some(sticker.id.get()),
            vec![AuditChange::new(
                "name",
                None,
                Some(sticker.name.clone().into()),
            )],
            None,
        )
        .await?;
        Ok(sticker)
    }

    pub async fn delete_sticker(
        &self,
        perms: Permissions,
        actor: UserId,
        guild: GuildId,
        id: StickerId,
    ) -> SettingsResult<()> {
        require(perms, Permissions::MANAGE_GUILD_EXPRESSIONS)?;
        if !self.repo.delete_sticker(guild, id).await? {
            return Err(SettingsError::UnknownSticker);
        }
        self.record_audit(
            guild,
            actor,
            AuditAction::StickerDelete,
            Some(id.get()),
            vec![],
            None,
        )
        .await?;
        Ok(())
    }

    // ---- AutoMod --------------------------------------------------------------

    /// Create or replace a rule after validation, compilation and quota checks.
    pub async fn put_automod_rule(
        &self,
        perms: Permissions,
        actor: UserId,
        mut rule: AutoModRule,
    ) -> SettingsResult<AutoModRule> {
        require(perms, Permissions::MANAGE_GUILD)?;
        let is_new = rule.id == 0;
        if is_new {
            rule.id = self.ids.generate().get();
            rule.creator_id = actor;
        }
        // Compile to surface regex errors before saving.
        CompiledRule::compile(rule.clone(), &self.presets)?;
        let existing = self.repo.list_rules(rule.guild_id).await?;
        automod::check_rule_quota(&existing, &rule)?;
        if !is_new && !existing.iter().any(|r| r.id == rule.id) {
            return Err(SettingsError::UnknownRule);
        }
        self.repo.put_rule(&rule).await?;
        self.rule_cache.invalidate(rule.guild_id);
        self.record_audit(
            rule.guild_id,
            actor,
            if is_new {
                AuditAction::AutoModerationRuleCreate
            } else {
                AuditAction::AutoModerationRuleUpdate
            },
            Some(rule.id),
            vec![AuditChange::new(
                "name",
                None,
                Some(rule.name.clone().into()),
            )],
            None,
        )
        .await?;
        Ok(rule)
    }

    pub async fn delete_automod_rule(
        &self,
        perms: Permissions,
        actor: UserId,
        guild: GuildId,
        id: u64,
    ) -> SettingsResult<()> {
        require(perms, Permissions::MANAGE_GUILD)?;
        self.rule_cache.invalidate(guild);
        if !self.repo.delete_rule(guild, id).await? {
            return Err(SettingsError::UnknownRule);
        }
        self.record_audit(
            guild,
            actor,
            AuditAction::AutoModerationRuleDelete,
            Some(id),
            vec![],
            None,
        )
        .await?;
        Ok(())
    }

    async fn compiled_rules(&self, guild: GuildId) -> SettingsResult<Arc<Vec<CompiledRule>>> {
        if let Some(hit) = self.rule_cache.get(guild) {
            return Ok(hit);
        }
        let rules = self.repo.list_rules(guild).await?;
        // A rule that no longer compiles (e.g. a preset list was removed) is skipped,
        // never allowed to break message delivery.
        let compiled = rules
            .into_iter()
            .filter(|r| r.enabled)
            .filter_map(|r| CompiledRule::compile(r, &self.presets).ok())
            .collect();
        Ok(self.rule_cache.insert(guild, compiled))
    }

    /// Run message AutoMod. The caller executes the returned actions.
    pub async fn check_message(
        &self,
        guild: GuildId,
        ctx: &MessageContext,
    ) -> SettingsResult<Vec<RuleMatch>> {
        let rules = self.compiled_rules(guild).await?;
        // The sliding window only feeds spam rules, so skip it when none apply.
        let burst = if !ctx.bypass && rules.iter().any(|r| r.rule.trigger == Trigger::Spam) {
            self.spam
                .observe(guild, ctx.author_id, &ctx.content, self.now())
        } else {
            None
        };
        Ok(automod::evaluate_message_with(&rules, ctx, burst))
    }

    pub async fn check_profile(
        &self,
        guild: GuildId,
        ctx: &ProfileContext,
    ) -> SettingsResult<Vec<RuleMatch>> {
        let rules = self.compiled_rules(guild).await?;
        Ok(automod::evaluate_profile(&rules, ctx))
    }
}
