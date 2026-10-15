//! Soundboard sound edits (VOX-SB-001).
//!
//! Permissions follow Discord: Manage Expressions can change any sound, a member with
//! Create Expressions only the ones they uploaded themselves.

use sr_core::permissions::Permissions;
use sr_core::{EmojiId, GuildId, SoundId, UserId};

use super::super::audit::{AuditAction, AuditChange};
use super::super::error::{SettingsError, SettingsResult};
use super::super::repo::SettingsRepository;
use super::super::soundboard::{
    Sound, SoundFormat, check_new_sound, validate_sound_name, validate_sound_volume,
};
use super::{SettingsService, require};

/// Fields of a new sound, already measured and uploaded to storage.
#[derive(Debug, Clone)]
pub struct NewSound {
    pub name: String,
    pub volume: f64,
    pub emoji_id: Option<EmojiId>,
    pub emoji_name: Option<String>,
    pub format: SoundFormat,
    pub size_bytes: u64,
    pub duration_ms: u32,
    pub media_key: String,
}

/// Fields a sound update may change; `None` keeps the stored value.
#[derive(Debug, Clone, Default)]
pub struct SoundPatch {
    pub name: Option<String>,
    pub volume: Option<f64>,
    /// `Some(None)` clears the emoji.
    pub emoji: Option<Option<(Option<EmojiId>, Option<String>)>>,
}

fn may_manage(perms: Permissions, actor: UserId, owner: UserId) -> SettingsResult<()> {
    if perms.has(Permissions::MANAGE_GUILD_EXPRESSIONS) {
        return Ok(());
    }
    if perms.has(Permissions::CREATE_GUILD_EXPRESSIONS) && actor == owner {
        return Ok(());
    }
    Err(SettingsError::MissingPermissions)
}

impl<R: SettingsRepository> SettingsService<R> {
    pub async fn create_sound(
        &self,
        perms: Permissions,
        actor: UserId,
        guild: GuildId,
        new: NewSound,
    ) -> SettingsResult<Sound> {
        require(
            perms,
            Permissions::MANAGE_GUILD_EXPRESSIONS | Permissions::CREATE_GUILD_EXPRESSIONS,
        )
        .or_else(|_| require(perms, Permissions::MANAGE_GUILD_EXPRESSIONS))
        .or_else(|_| require(perms, Permissions::CREATE_GUILD_EXPRESSIONS))?;
        validate_sound_name(&new.name)?;
        validate_sound_volume(new.volume)?;
        let existing = self.repo.list_sounds(guild).await?;
        check_new_sound(&existing, new.size_bytes, new.duration_ms, &self.limits)?;
        let sound = Sound {
            id: SoundId::new(self.ids.generate().get()),
            guild_id: guild,
            name: new.name.trim().to_owned(),
            volume: new.volume,
            emoji_id: new.emoji_id,
            emoji_name: new.emoji_name,
            available: true,
            creator_id: actor,
            media_key: new.media_key,
            format: new.format,
            duration_ms: new.duration_ms,
            created_at_ms: self.now(),
        };
        self.repo.insert_sound(&sound).await?;
        self.record_audit(
            guild,
            actor,
            AuditAction::SoundboardSoundCreate,
            Some(sound.id.get()),
            vec![AuditChange::new(
                "name",
                None,
                Some(sound.name.clone().into()),
            )],
            None,
        )
        .await?;
        Ok(sound)
    }

    pub async fn edit_sound(
        &self,
        perms: Permissions,
        actor: UserId,
        guild: GuildId,
        id: SoundId,
        patch: SoundPatch,
    ) -> SettingsResult<Sound> {
        let mut s = self
            .repo
            .get_sound(guild, id)
            .await?
            .ok_or(SettingsError::UnknownSound)?;
        may_manage(perms, actor, s.creator_id)?;
        let mut changes = Vec::new();
        if let Some(name) = patch.name {
            validate_sound_name(&name)?;
            let name = name.trim().to_owned();
            if name != s.name {
                changes.push(AuditChange::new(
                    "name",
                    Some(s.name.clone().into()),
                    Some(name.clone().into()),
                ));
                s.name = name;
            }
        }
        if let Some(v) = patch.volume {
            validate_sound_volume(v)?;
            if (v - s.volume).abs() > f64::EPSILON {
                changes.push(AuditChange::new(
                    "volume",
                    Some(s.volume.into()),
                    Some(v.into()),
                ));
                s.volume = v;
            }
        }
        if let Some(e) = patch.emoji {
            let (emoji_id, emoji_name) = e.unwrap_or((None, None));
            if emoji_id != s.emoji_id || emoji_name != s.emoji_name {
                changes.push(AuditChange::new(
                    "emoji_name",
                    s.emoji_name.clone().map(Into::into),
                    emoji_name.clone().map(Into::into),
                ));
                s.emoji_id = emoji_id;
                s.emoji_name = emoji_name;
            }
        }
        if changes.is_empty() {
            return Ok(s);
        }
        self.repo.update_sound(&s).await?;
        self.record_audit(
            guild,
            actor,
            AuditAction::SoundboardSoundUpdate,
            Some(id.get()),
            changes,
            None,
        )
        .await?;
        Ok(s)
    }

    pub async fn delete_sound(
        &self,
        perms: Permissions,
        actor: UserId,
        guild: GuildId,
        id: SoundId,
    ) -> SettingsResult<Sound> {
        let s = self
            .repo
            .get_sound(guild, id)
            .await?
            .ok_or(SettingsError::UnknownSound)?;
        may_manage(perms, actor, s.creator_id)?;
        if !self.repo.delete_sound(guild, id).await? {
            return Err(SettingsError::UnknownSound);
        }
        self.record_audit(
            guild,
            actor,
            AuditAction::SoundboardSoundDelete,
            Some(id.get()),
            vec![],
            None,
        )
        .await?;
        Ok(s)
    }
}
