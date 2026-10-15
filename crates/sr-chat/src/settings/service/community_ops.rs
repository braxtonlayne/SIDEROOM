//! Community, onboarding and welcome-screen operations (LNG-SSET-063..067).

use sr_core::permissions::Permissions;
use sr_core::{ChannelId, GuildId, UserId};

use super::super::audit::{AuditAction, AuditChange};
use super::super::community::{
    self, CommunitySettings, Onboarding, SafetyInputs, WelcomeScreen, validate_onboarding,
    validate_welcome_screen,
};
use super::super::error::{SettingsError, SettingsResult};
use super::super::repo::SettingsRepository;
use super::{SettingsService, require};

fn diff(old: &CommunitySettings, new: &CommunitySettings) -> Vec<AuditChange> {
    let mut out = Vec::new();
    let mut push = |key: &str, a: serde_json::Value, b: serde_json::Value| {
        if a != b {
            out.push(AuditChange::new(key, Some(a), Some(b)));
        }
    };
    let ch = |c: Option<ChannelId>| serde_json::json!(c.map(|c| c.get().to_string()));
    push(
        "community_enabled",
        old.community_enabled.into(),
        new.community_enabled.into(),
    );
    push(
        "rules_channel_id",
        ch(old.rules_channel_id),
        ch(new.rules_channel_id),
    );
    push(
        "public_updates_channel_id",
        ch(old.updates_channel_id),
        ch(new.updates_channel_id),
    );
    push(
        "safety_alerts_channel_id",
        ch(old.safety_alerts_channel_id),
        ch(new.safety_alerts_channel_id),
    );
    push(
        "description",
        old.description.clone().into(),
        new.description.clone().into(),
    );
    push(
        "preferred_locale",
        old.preferred_locale.clone().into(),
        new.preferred_locale.clone().into(),
    );
    push(
        "invites_paused",
        old.invites_paused.into(),
        new.invites_paused.into(),
    );
    push(
        "age_restricted",
        old.age_restricted.into(),
        new.age_restricted.into(),
    );
    push(
        "screening_enabled",
        old.screening_enabled.into(),
        new.screening_enabled.into(),
    );
    push(
        "raid_alert_joins_per_minute",
        old.raid_alert_joins_per_minute.into(),
        new.raid_alert_joins_per_minute.into(),
    );
    out
}

impl<R: SettingsRepository> SettingsService<R> {
    pub async fn community(
        &self,
        perms: Permissions,
        guild: GuildId,
    ) -> SettingsResult<CommunitySettings> {
        require(perms, Permissions::MANAGE_GUILD)?;
        Ok(self.repo.get_community(guild).await?)
    }

    /// Save edited community settings. Turning community on goes through the
    /// safety checks in [`community::enable_community`]; turning it off also
    /// switches off onboarding, the welcome screen and the guide.
    pub async fn save_community(
        &self,
        perms: Permissions,
        actor: UserId,
        mut new: CommunitySettings,
        safety: SafetyInputs,
    ) -> SettingsResult<CommunitySettings> {
        require(perms, Permissions::MANAGE_GUILD)?;
        let old = self.repo.get_community(new.guild_id).await?;
        if new.preferred_locale.trim().is_empty() || new.preferred_locale.len() > 16 {
            return Err(SettingsError::invalid(
                "preferred_locale",
                "BASE_TYPE_BAD_LENGTH",
                "must be a locale tag such as en-US",
            ));
        }
        if new
            .description
            .as_ref()
            .is_some_and(|d| d.chars().count() > 300)
        {
            return Err(SettingsError::invalid(
                "description",
                "BASE_TYPE_MAX_LENGTH",
                "must be 300 or fewer in length",
            ));
        }
        let (screening_description, screening_rules) = community::validate_screening(
            new.screening_enabled,
            new.screening_description.as_deref(),
            &new.screening_rules,
        )?;
        new.screening_description = screening_description;
        new.screening_rules = screening_rules;
        if new.community_enabled && !old.community_enabled {
            let (Some(rules), Some(updates)) = (new.rules_channel_id, new.updates_channel_id)
            else {
                return Err(SettingsError::invalid(
                    "rules_channel_id",
                    "COMMUNITY_REQUIREMENTS",
                    "a rules channel and an updates channel are required",
                ));
            };
            community::enable_community(&mut new, &safety, rules, updates)?;
        } else if !new.community_enabled && old.community_enabled {
            community::disable_community(&mut new);
        }
        self.repo.put_community(&new).await?;
        let changes = diff(&old, &new);
        if !changes.is_empty() {
            self.record_audit(
                new.guild_id,
                actor,
                AuditAction::GuildUpdate,
                Some(new.guild_id.get()),
                changes,
                None,
            )
            .await?;
        }
        Ok(new)
    }

    pub async fn onboarding(
        &self,
        perms: Permissions,
        guild: GuildId,
    ) -> SettingsResult<Onboarding> {
        require(perms, Permissions::MANAGE_GUILD)?;
        Ok(self.repo.get_onboarding(guild).await?)
    }

    /// Save onboarding. `writable` tells whether `@everyone` may send in a channel.
    pub async fn save_onboarding(
        &self,
        perms: Permissions,
        actor: UserId,
        onboarding: Onboarding,
        writable: impl Fn(ChannelId) -> bool,
    ) -> SettingsResult<Onboarding> {
        require(perms, Permissions::MANAGE_GUILD)?;
        validate_onboarding(&onboarding, writable)?;
        let old = self.repo.get_onboarding(onboarding.guild_id).await?;
        self.repo.put_onboarding(&onboarding).await?;
        let guild = onboarding.guild_id;
        // Discord logs each question separately (163 to 165) and the settings once (167).
        for p in &onboarding.prompts {
            let (action, old_title) = match old.prompts.iter().find(|o| o.id == p.id) {
                None => (AuditAction::OnboardingPromptCreate, None),
                Some(o) if o == p => continue,
                Some(o) => (AuditAction::OnboardingPromptUpdate, Some(o.title.clone())),
            };
            let changes = vec![AuditChange::new(
                "title",
                old_title.map(Into::into),
                Some(p.title.clone().into()),
            )];
            self.record_audit(guild, actor, action, Some(p.id), changes, None)
                .await?;
        }
        for o in old
            .prompts
            .iter()
            .filter(|o| !onboarding.prompts.iter().any(|p| p.id == o.id))
        {
            let changes = vec![AuditChange::new(
                "title",
                Some(o.title.clone().into()),
                None,
            )];
            self.record_audit(
                guild,
                actor,
                AuditAction::OnboardingPromptDelete,
                Some(o.id),
                changes,
                None,
            )
            .await?;
        }
        let mut changes = Vec::new();
        if old.enabled != onboarding.enabled {
            changes.push(AuditChange::new(
                "enabled",
                Some(old.enabled.into()),
                Some(onboarding.enabled.into()),
            ));
        }
        if old.default_channel_ids != onboarding.default_channel_ids {
            let ids = |v: &[ChannelId]| {
                serde_json::Value::from(v.iter().map(|c| c.to_string()).collect::<Vec<_>>())
            };
            changes.push(AuditChange::new(
                "default_channel_ids",
                Some(ids(&old.default_channel_ids)),
                Some(ids(&onboarding.default_channel_ids)),
            ));
        }
        if !changes.is_empty() {
            self.record_audit(
                guild,
                actor,
                AuditAction::OnboardingUpdate,
                Some(guild.get()),
                changes,
                None,
            )
            .await?;
        }
        Ok(onboarding)
    }

    pub async fn welcome_screen(&self, guild: GuildId) -> SettingsResult<WelcomeScreen> {
        Ok(self.repo.get_welcome_screen(guild).await?)
    }

    pub async fn save_welcome_screen(
        &self,
        perms: Permissions,
        screen: WelcomeScreen,
    ) -> SettingsResult<WelcomeScreen> {
        require(perms, Permissions::MANAGE_GUILD)?;
        validate_welcome_screen(&screen)?;
        self.repo.put_welcome_screen(&screen).await?;
        Ok(screen)
    }
}
