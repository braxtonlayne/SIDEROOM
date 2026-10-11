//! Guild template operations (LNG-SSET-070, open question 138): one template per
//! guild, code = 16 base62 characters.

use sr_core::permissions::Permissions;
use sr_core::{GuildId, UserId};

use super::super::error::{SettingsError, SettingsResult};
use super::super::repo::SettingsRepository;
use super::super::templates::{self, GuildTemplate, TemplateSnapshot};
use super::{SettingsService, require};

impl<R: SettingsRepository> SettingsService<R> {
    /// Create the guild's template from `snapshot` (built by the caller from the
    /// live roles, channels and settings).
    pub async fn create_template(
        &self,
        perms: Permissions,
        actor: UserId,
        guild: GuildId,
        name: &str,
        description: Option<&str>,
        snapshot: TemplateSnapshot,
    ) -> SettingsResult<GuildTemplate> {
        require(perms, Permissions::MANAGE_GUILD)?;
        if self.repo.get_guild_template(guild).await?.is_some() {
            return Err(SettingsError::invalid(
                "guild_id",
                "TEMPLATE_ALREADY_EXISTS",
                "this server already has a template",
            ));
        }
        for _ in 0..5 {
            let code = templates::generate_code(|| (self.entropy)());
            if self.repo.get_template(&code).await?.is_some() {
                continue;
            }
            let t =
                GuildTemplate::new(code, guild, actor, name, description, snapshot, self.now())?;
            self.repo.put_template(&t).await?;
            return Ok(t);
        }
        Err(SettingsError::Repo(crate::error::RepoError(
            "could not allocate a template code".into(),
        )))
    }

    /// Public lookup by code (anyone with the link may preview a template).
    pub async fn template_by_code(&self, code: &str) -> SettingsResult<GuildTemplate> {
        self.repo
            .get_template(code)
            .await?
            .ok_or(SettingsError::UnknownTemplate)
    }

    pub async fn guild_template(
        &self,
        perms: Permissions,
        guild: GuildId,
    ) -> SettingsResult<Option<GuildTemplate>> {
        require(perms, Permissions::MANAGE_GUILD)?;
        Ok(self.repo.get_guild_template(guild).await?)
    }

    /// Replace the stored snapshot with the current state of the guild.
    pub async fn sync_template(
        &self,
        perms: Permissions,
        guild: GuildId,
        code: &str,
        current: TemplateSnapshot,
    ) -> SettingsResult<GuildTemplate> {
        let mut t = self.owned_template(perms, guild, code).await?;
        t.sync(current, self.now());
        self.repo.put_template(&t).await?;
        Ok(t)
    }

    pub async fn edit_template(
        &self,
        perms: Permissions,
        guild: GuildId,
        code: &str,
        name: Option<&str>,
        description: Option<Option<&str>>,
    ) -> SettingsResult<GuildTemplate> {
        let mut t = self.owned_template(perms, guild, code).await?;
        let new_name = name.unwrap_or(&t.name).to_owned();
        let new_desc = match description {
            Some(d) => d.map(str::to_owned),
            None => t.description.clone(),
        };
        templates::validate_template_text(&new_name, new_desc.as_deref())?;
        t.name = new_name.trim().to_owned();
        t.description = new_desc.filter(|d| !d.is_empty());
        t.updated_at_ms = self.now();
        self.repo.put_template(&t).await?;
        Ok(t)
    }

    pub async fn delete_template(
        &self,
        perms: Permissions,
        guild: GuildId,
        code: &str,
    ) -> SettingsResult<GuildTemplate> {
        let t = self.owned_template(perms, guild, code).await?;
        self.repo.delete_template(guild).await?;
        Ok(t)
    }

    async fn owned_template(
        &self,
        perms: Permissions,
        guild: GuildId,
        code: &str,
    ) -> SettingsResult<GuildTemplate> {
        require(perms, Permissions::MANAGE_GUILD)?;
        self.repo
            .get_template(code)
            .await?
            .filter(|t| t.guild_id == guild)
            .ok_or(SettingsError::UnknownTemplate)
    }
}
