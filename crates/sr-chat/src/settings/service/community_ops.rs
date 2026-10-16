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
    push(
        "dm_spam_filter",
        old.dm_spam_filter.into(),
        new.dm_spam_filter.into(),
    );
    push(
        "explicit_media_filter_dms",
        old.explicit_media_filter_dms.into(),
        new.explicit_media_filter_dms.into(),
    );
    push(
        "hide_suspicious_links",
        old.hide_suspicious_links.into(),
        new.hide_suspicious_links.into(),
    );
    push(
        "warn_new_account_days",
        old.warn_new_account_days.into(),
        new.warn_new_account_days.into(),
    );
    push(
        "invites_disabled_until",
        old.invites_disabled_until_ms.into(),
        new.invites_disabled_until_ms.into(),
    );
    push(
        "dms_disabled_until",
        old.dms_disabled_until_ms.into(),
        new.dms_disabled_until_ms.into(),
    );
    push(
        "raid_auto_pause_invites",
        old.raid_auto_pause_invites.into(),
        new.raid_auto_pause_invites.into(),
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

    /// `PUT /guilds/{id}/incident-actions` (D-243): set or clear the two timed pauses. Each is
    /// `None` or at most 24 hours ahead. Audit `GUILD_UPDATE` with the changed fields.
    pub async fn set_incident_actions(
        &self,
        perms: Permissions,
        actor: UserId,
        guild: GuildId,
        invites_until_ms: Option<i64>,
        dms_until_ms: Option<i64>,
    ) -> SettingsResult<CommunitySettings> {
        require(perms, Permissions::MANAGE_GUILD)?;
        let now = self.now();
        let invites =
            community::validate_incident_time("invites_disabled_until", invites_until_ms, now)?;
        let dms = community::validate_incident_time("dms_disabled_until", dms_until_ms, now)?;
        let old = self.repo.get_community(guild).await?;
        let mut new = old.clone();
        new.invites_disabled_until_ms = invites;
        new.dms_disabled_until_ms = dms;
        self.repo.put_community(&new).await?;
        let changes = diff(&old, &new);
        if !changes.is_empty() {
            self.record_audit(
                guild,
                actor,
                AuditAction::GuildUpdate,
                Some(guild.get()),
                changes,
                None,
            )
            .await?;
        }
        Ok(new)
    }

    /// The join-rate alert (D-243): called after a member joined with the number of joins in the
    /// last minute. When it exceeds `raid_alert_joins_per_minute` (and no alert fired in the last
    /// ten minutes) the guild records `raid_detected_at`, pauses invites for 24 hours unless
    /// `raid_auto_pause_invites` is off, and an audit row names the system user and the
    /// `community` module. `None` when nothing fired.
    pub async fn register_join_rate(
        &self,
        guild: GuildId,
        joins_last_minute: u64,
    ) -> SettingsResult<Option<community::RaidAlert>> {
        let s = self.repo.get_community(guild).await?;
        if s.raid_alert_joins_per_minute == 0
            || joins_last_minute <= u64::from(s.raid_alert_joins_per_minute)
        {
            return Ok(None);
        }
        let now = self.now();
        let until = now.saturating_add(community::MAX_INCIDENT_PAUSE_MS);
        let Some(paused) = self
            .repo
            .mark_raid(guild, now, community::RAID_ALERT_COOLDOWN_MS, until)
            .await?
        else {
            return Ok(None);
        };
        let mut changes = vec![AuditChange::new("raid_detected_at", None, Some(now.into()))];
        if paused {
            changes.push(AuditChange::new(
                "invites_disabled_until",
                s.invites_disabled_until_ms.map(Into::into),
                Some(until.into()),
            ));
        }
        self.record_audit_as_module(
            guild,
            AuditAction::GuildUpdate,
            Some(guild.get()),
            changes,
            None,
            ("community", "community"),
        )
        .await?;
        Ok(Some(community::RaidAlert {
            detected_at_ms: now,
            paused_until_ms: paused.then_some(until),
        }))
    }

    /// `Resume invites` on the alert message (D-243): clear the timed pause. The caller checked
    /// `MANAGE_GUILD` at press time. Whether a pause was set.
    pub async fn resume_invites(
        &self,
        perms: Permissions,
        actor: UserId,
        guild: GuildId,
    ) -> SettingsResult<bool> {
        require(perms, Permissions::MANAGE_GUILD)?;
        let before = self.repo.get_community(guild).await?;
        if !self.repo.clear_invite_pause(guild).await? {
            return Ok(false);
        }
        self.record_audit(
            guild,
            actor,
            AuditAction::GuildUpdate,
            Some(guild.get()),
            vec![AuditChange::new(
                "invites_disabled_until",
                before.invites_disabled_until_ms.map(Into::into),
                None,
            )],
            None,
        )
        .await?;
        Ok(true)
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
