//! Server tags and their adoption (LNG-SSET-011/012).

use sr_core::permissions::Permissions;
use sr_core::{GuildId, UserId};

use super::super::audit::{AuditAction, AuditChange};
use super::super::error::{SettingsError, SettingsResult};
use super::super::repo::SettingsRepository;
use super::super::tags::{GuildTag, PrimaryGuild, validate_tag};
use super::SettingsService;

fn require_manage(perms: Permissions) -> SettingsResult<()> {
    if perms.has(Permissions::MANAGE_GUILD) {
        Ok(())
    } else {
        Err(SettingsError::MissingPermissions)
    }
}

fn tag_changes(old: Option<&GuildTag>, new: Option<&GuildTag>) -> Vec<AuditChange> {
    let mut out = Vec::new();
    let mut push = |key: &str, o: Option<serde_json::Value>, n: Option<serde_json::Value>| {
        if o != n {
            out.push(AuditChange::new(key, o, n));
        }
    };
    push(
        "tag",
        old.map(|t| t.tag.clone().into()),
        new.map(|t| t.tag.clone().into()),
    );
    push(
        "tag_badge",
        old.map(|t| t.badge.into()),
        new.map(|t| t.badge.into()),
    );
    push(
        "tag_color",
        old.map(|t| t.color.into()),
        new.map(|t| t.color.into()),
    );
    push(
        "tag_members_can_display",
        old.map(|t| t.members_can_display.into()),
        new.map(|t| t.members_can_display.into()),
    );
    out
}

impl<R: SettingsRepository> SettingsService<R> {
    /// The guild's tag, if it has one.
    pub async fn guild_tag(&self, guild: GuildId) -> SettingsResult<Option<GuildTag>> {
        Ok(self.repo.get_guild_tag(guild).await?)
    }

    /// Set or replace the tag (`MANAGE_GUILD`). Turning "members can display" off
    /// keeps the tag but hides it everywhere.
    pub async fn save_guild_tag(
        &self,
        perms: Permissions,
        actor: UserId,
        tag: GuildTag,
    ) -> SettingsResult<GuildTag> {
        require_manage(perms)?;
        let tag = validate_tag(tag)?;
        let old = self.repo.get_guild_tag(tag.guild_id).await?;
        self.repo.put_guild_tag(&tag).await?;
        let changes = tag_changes(old.as_ref(), Some(&tag));
        if !changes.is_empty() {
            self.record_audit(
                tag.guild_id,
                actor,
                AuditAction::GuildUpdate,
                Some(tag.guild_id.get()),
                changes,
                None,
            )
            .await?;
        }
        Ok(tag)
    }

    /// Remove the tag; members who adopted it stop displaying it.
    pub async fn clear_guild_tag(
        &self,
        perms: Permissions,
        actor: UserId,
        guild: GuildId,
    ) -> SettingsResult<()> {
        require_manage(perms)?;
        let old = self.repo.get_guild_tag(guild).await?;
        if self.repo.delete_guild_tag(guild).await? {
            self.record_audit(
                guild,
                actor,
                AuditAction::GuildUpdate,
                Some(guild.get()),
                tag_changes(old.as_ref(), None),
                None,
            )
            .await?;
        }
        Ok(())
    }

    /// Adopt `guild`'s tag as the user's displayed tag, or clear it with `None`.
    /// `is_member` is the caller's membership check for `guild`.
    pub async fn adopt_tag(
        &self,
        user: UserId,
        guild: Option<GuildId>,
        is_member: bool,
    ) -> SettingsResult<Option<PrimaryGuild>> {
        let Some(guild) = guild else {
            self.repo.set_primary_guild(user, None).await?;
            return Ok(None);
        };
        let tag = self
            .repo
            .get_guild_tag(guild)
            .await?
            .filter(|t| is_member && t.members_can_display)
            .ok_or_else(|| {
                SettingsError::invalid(
                    "guild_id",
                    "GUILD_TAG_UNAVAILABLE",
                    "That server has no tag you can display.",
                )
            })?;
        self.repo.set_primary_guild(user, Some(guild)).await?;
        Ok(Some(PrimaryGuild::from(&tag)))
    }

    /// The tag the user displays, if it is still valid (tag exists and members may
    /// display it). Membership is checked by the caller.
    pub async fn primary_guild(&self, user: UserId) -> SettingsResult<Option<PrimaryGuild>> {
        let Some(guild) = self.repo.get_primary_guild(user).await? else {
            return Ok(None);
        };
        Ok(self
            .repo
            .get_guild_tag(guild)
            .await?
            .filter(|t| t.members_can_display)
            .map(|t| PrimaryGuild::from(&t)))
    }
}

impl<R: SettingsRepository> SettingsService<R> {
    /// Server Insights for the last `days` completed days (`MANAGE_GUILD` or
    /// `VIEW_GUILD_INSIGHTS`).
    pub async fn insights(
        &self,
        perms: Permissions,
        guild: GuildId,
        days: Option<u32>,
    ) -> SettingsResult<super::super::insights::Insights> {
        if !perms.has(Permissions::MANAGE_GUILD) && !perms.has(Permissions::VIEW_GUILD_INSIGHTS) {
            return Err(SettingsError::MissingPermissions);
        }
        let days = super::super::insights::clamp_days(days);
        Ok(self.repo.insights(guild, days, self.now()).await?)
    }
}

impl<R: SettingsRepository> SettingsService<R> {
    /// The Server Guide content (empty until saved).
    pub async fn guide(&self, guild: GuildId) -> SettingsResult<super::super::guide::Guide> {
        Ok(self.repo.get_guide(guild).await?)
    }

    /// Replace the Server Guide content (`MANAGE_GUILD`).
    pub async fn save_guide(
        &self,
        perms: Permissions,
        actor: UserId,
        guide: super::super::guide::Guide,
    ) -> SettingsResult<super::super::guide::Guide> {
        require_manage(perms)?;
        let guide = super::super::guide::validate_guide(guide)?;
        let old = self.repo.get_guide(guide.guild_id).await?;
        self.repo.put_guide(&guide).await?;
        if old != guide {
            let mut changes = Vec::new();
            if old.welcome_message != guide.welcome_message {
                changes.push(AuditChange::new(
                    "guide_welcome_message",
                    old.welcome_message.map(Into::into),
                    guide.welcome_message.clone().map(Into::into),
                ));
            }
            if old.todos != guide.todos {
                changes.push(AuditChange::new(
                    "guide_todos",
                    Some(old.todos.len().into()),
                    Some(guide.todos.len().into()),
                ));
            }
            if old.resources != guide.resources {
                changes.push(AuditChange::new(
                    "guide_resources",
                    Some(old.resources.len().into()),
                    Some(guide.resources.len().into()),
                ));
            }
            self.record_audit(
                guide.guild_id,
                actor,
                AuditAction::GuildUpdate,
                Some(guide.guild_id.get()),
                changes,
                None,
            )
            .await?;
        }
        Ok(guide)
    }
}
