//! Emoji and sticker edits (LNG-SSET-022/024).

use sr_core::permissions::Permissions;
use sr_core::{EmojiId, GuildId, RoleId, StickerId, UserId};

use super::super::audit::{AuditAction, AuditChange};
use super::super::error::{SettingsError, SettingsResult};
use super::super::expressions::{Emoji, Sticker, validate_sticker};
use super::super::repo::SettingsRepository;
use super::{SettingsService, require};

/// Fields a sticker update may change; `None` keeps the stored value.
#[derive(Debug, Clone, Default)]
pub struct StickerPatch {
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub tags: Option<String>,
}

impl<R: SettingsRepository> SettingsService<R> {
    /// Restrict an emoji to `roles` (empty = everyone).
    pub async fn set_emoji_roles(
        &self,
        perms: Permissions,
        actor: UserId,
        guild: GuildId,
        id: EmojiId,
        roles: Vec<RoleId>,
    ) -> SettingsResult<Emoji> {
        require(perms, Permissions::MANAGE_GUILD_EXPRESSIONS)?;
        if roles.len() > self.limits.max_roles_per_emoji {
            return Err(SettingsError::LimitReached("roles per emoji"));
        }
        let mut emoji = self
            .repo
            .get_emoji(guild, id)
            .await?
            .ok_or(SettingsError::UnknownEmoji)?;
        if emoji.role_ids == roles {
            return Ok(emoji);
        }
        let ids = |v: &[RoleId]| {
            serde_json::Value::from(v.iter().map(|r| r.get().to_string()).collect::<Vec<_>>())
        };
        let change = AuditChange::new("roles", Some(ids(&emoji.role_ids)), Some(ids(&roles)));
        emoji.role_ids = roles;
        self.repo.update_emoji(&emoji).await?;
        self.record_audit(
            guild,
            actor,
            AuditAction::EmojiUpdate,
            Some(id.get()),
            vec![change],
            None,
        )
        .await?;
        Ok(emoji)
    }

    pub async fn edit_sticker(
        &self,
        perms: Permissions,
        actor: UserId,
        guild: GuildId,
        id: StickerId,
        patch: StickerPatch,
    ) -> SettingsResult<Sticker> {
        require(perms, Permissions::MANAGE_GUILD_EXPRESSIONS)?;
        let mut s = self
            .repo
            .get_sticker(guild, id)
            .await?
            .ok_or(SettingsError::UnknownSticker)?;
        let name = patch.name.unwrap_or_else(|| s.name.clone());
        let description = match patch.description {
            Some(d) => d.filter(|d| !d.is_empty()),
            None => s.description.clone(),
        };
        let tags = patch.tags.unwrap_or_else(|| s.tags.clone());
        validate_sticker(&name, description.as_deref(), &tags)?;
        let mut changes = Vec::new();
        if name.trim() != s.name {
            changes.push(AuditChange::new(
                "name",
                Some(s.name.clone().into()),
                Some(name.trim().to_owned().into()),
            ));
        }
        if description != s.description {
            changes.push(AuditChange::new(
                "description",
                s.description.clone().map(Into::into),
                description.clone().map(Into::into),
            ));
        }
        if tags != s.tags {
            changes.push(AuditChange::new(
                "tags",
                Some(s.tags.clone().into()),
                Some(tags.clone().into()),
            ));
        }
        s.name = name.trim().to_owned();
        s.description = description;
        s.tags = tags;
        if changes.is_empty() {
            return Ok(s);
        }
        self.repo.update_sticker(&s).await?;
        self.record_audit(
            guild,
            actor,
            AuditAction::StickerUpdate,
            Some(id.get()),
            changes,
            None,
        )
        .await?;
        Ok(s)
    }
}
