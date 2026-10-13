//! Webhook operations (LNG-SSET-044/045, open question 137).

use sr_core::permissions::Permissions;
use sr_core::{ChannelId, GuildId, UserId, WebhookId};

use super::super::audit::{AuditAction, AuditChange};
use super::super::error::{SettingsError, SettingsResult};
use super::super::repo::SettingsRepository;
use super::super::webhooks::{self, Webhook, WebhookKind};
use super::{SettingsService, require};

/// Fields a webhook update may change.
#[derive(Debug, Clone, Default)]
pub struct WebhookPatch {
    pub name: Option<String>,
    /// Must be a channel of the same guild; the caller checks that.
    pub channel_id: Option<ChannelId>,
}

impl<R: SettingsRepository> SettingsService<R> {
    /// Create an incoming webhook. Returns the webhook and its execution token,
    /// which is shown once and never stored (only its SHA-256 is).
    pub async fn create_webhook(
        &self,
        perms: Permissions,
        actor: UserId,
        guild: GuildId,
        channel: ChannelId,
        name: &str,
    ) -> SettingsResult<(Webhook, String)> {
        require(perms, Permissions::MANAGE_WEBHOOKS)?;
        Webhook::check_channel_quota(self.repo.count_channel_webhooks(channel).await?)?;
        let token = webhooks::generate_token(|| (self.entropy)());
        let webhook = Webhook::new_incoming(
            WebhookId::new(self.ids.generate().get()),
            guild,
            channel,
            actor,
            name,
            webhooks::hash_token(&token),
            self.now(),
        )?;
        self.repo.insert_webhook(&webhook).await?;
        self.record_audit(
            guild,
            actor,
            AuditAction::WebhookCreate,
            Some(webhook.id.get()),
            vec![
                AuditChange::new("name", None, Some(webhook.name.clone().into())),
                AuditChange::new("channel_id", None, Some(channel.get().to_string().into())),
            ],
            None,
        )
        .await?;
        Ok((webhook, token))
    }

    pub async fn list_guild_webhooks(
        &self,
        perms: Permissions,
        guild: GuildId,
    ) -> SettingsResult<Vec<Webhook>> {
        require(perms, Permissions::MANAGE_WEBHOOKS)?;
        Ok(self.repo.list_guild_webhooks(guild).await?)
    }

    /// A webhook of `guild`; webhooks of other guilds are reported as unknown.
    pub async fn guild_webhook(
        &self,
        perms: Permissions,
        guild: GuildId,
        id: WebhookId,
    ) -> SettingsResult<Webhook> {
        require(perms, Permissions::MANAGE_WEBHOOKS)?;
        self.repo
            .get_webhook(id)
            .await?
            .filter(|w| w.guild_id == guild)
            .ok_or(SettingsError::UnknownWebhook)
    }

    pub async fn modify_webhook(
        &self,
        perms: Permissions,
        actor: UserId,
        guild: GuildId,
        id: WebhookId,
        patch: WebhookPatch,
    ) -> SettingsResult<Webhook> {
        let mut w = self.guild_webhook(perms, guild, id).await?;
        let mut changes = Vec::new();
        if let Some(name) = patch.name {
            let name = webhooks::validate_name(&name)?;
            if name != w.name {
                changes.push(AuditChange::new(
                    "name",
                    Some(w.name.clone().into()),
                    Some(name.clone().into()),
                ));
                w.name = name;
            }
        }
        if let Some(channel) = patch.channel_id
            && channel != w.channel_id
        {
            if w.kind == WebhookKind::Incoming {
                Webhook::check_channel_quota(self.repo.count_channel_webhooks(channel).await?)?;
            }
            changes.push(AuditChange::new(
                "channel_id",
                Some(w.channel_id.get().to_string().into()),
                Some(channel.get().to_string().into()),
            ));
            w.move_to(channel);
        }
        if changes.is_empty() {
            return Ok(w);
        }
        self.repo.update_webhook(&w).await?;
        self.record_audit(
            guild,
            actor,
            AuditAction::WebhookUpdate,
            Some(id.get()),
            changes,
            None,
        )
        .await?;
        Ok(w)
    }

    /// Replace the execution token of an incoming webhook (`MANAGE_WEBHOOKS`). The old token
    /// stops working at once; the new one is returned once and, like the first, only its
    /// SHA-256 is stored. This is how a manager gets a copyable URL again: stored tokens are
    /// hashes (open question 137) and cannot be shown later.
    pub async fn reset_webhook_token(
        &self,
        perms: Permissions,
        actor: UserId,
        guild: GuildId,
        id: WebhookId,
    ) -> SettingsResult<(Webhook, String)> {
        let mut w = self.guild_webhook(perms, guild, id).await?;
        if w.kind != WebhookKind::Incoming {
            return Err(SettingsError::UnknownWebhook);
        }
        let token = webhooks::generate_token(|| (self.entropy)());
        w.token_hash = Some(webhooks::hash_token(&token));
        self.repo.update_webhook(&w).await?;
        self.record_audit(
            guild,
            actor,
            AuditAction::WebhookUpdate,
            Some(id.get()),
            // The value is a secret; the entry only says that it changed.
            vec![AuditChange::new("token", None, None)],
            None,
        )
        .await?;
        Ok((w, token))
    }

    pub async fn delete_webhook(
        &self,
        perms: Permissions,
        actor: UserId,
        guild: GuildId,
        id: WebhookId,
    ) -> SettingsResult<()> {
        let w = self.guild_webhook(perms, guild, id).await?;
        if !self.repo.delete_webhook(id).await? {
            return Err(SettingsError::UnknownWebhook);
        }
        self.record_audit(
            guild,
            actor,
            AuditAction::WebhookDelete,
            Some(id.get()),
            vec![AuditChange::new("name", Some(w.name.into()), None)],
            None,
        )
        .await?;
        Ok(())
    }

    /// Resolve a webhook by id and execution token. A wrong token and an unknown
    /// id are indistinguishable to the caller.
    pub async fn authenticate_webhook(
        &self,
        id: WebhookId,
        token: &str,
    ) -> SettingsResult<Webhook> {
        let w = self
            .repo
            .get_webhook(id)
            .await?
            .ok_or(SettingsError::UnknownWebhook)?;
        if w.kind == WebhookKind::Incoming && w.token_matches(&webhooks::hash_token(token)) {
            Ok(w)
        } else {
            Err(SettingsError::UnknownWebhook)
        }
    }
}
