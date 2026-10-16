//! Voice channel status (VOX-UI-015): a short line under a voice or stage channel's name that
//! says what the people in it are doing ("Playing Mario Kart"). Discord's `PUT
//! /channels/{id}/voice-status`.
//!
//! * Set by a member who is connected to the channel and has `SET_VOICE_CHANNEL_STATUS`, or
//!   by anyone with `MANAGE_CHANNELS` (who may also clear it from outside).
//! * At most [`MAX_STATUS_CHARS`] characters; an empty status clears it.
//! * It lives in memory next to the voice states it describes and ends with the last member
//!   leaving, like Discord's. Changes are announced as `VOICE_CHANNEL_STATUS_UPDATE` to the
//!   members who can see the channel, and `GUILD_CREATE` carries the current ones as the
//!   `status` field of the channel objects.

use serde_json::{Value, json};
use sr_core::{ChannelId, GuildId, Permissions, UserId};

use crate::hub::Gateway;
use crate::voice::{VoiceChannelKind, VoiceState, unix_ms};

/// Longest status, in characters.
pub const MAX_STATUS_CHARS: usize = 500;

/// A status that is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelStatus {
    pub guild_id: GuildId,
    pub channel_id: ChannelId,
    pub text: String,
    pub set_by: UserId,
    pub set_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusError {
    /// Voice is switched off on this installation.
    Unavailable,
    UnknownChannel,
    /// The channel is neither a voice nor a stage channel.
    NotVoiceChannel,
    MissingPermissions,
    /// Setting a status needs someone in the channel (it would end at once otherwise), and a
    /// member without Manage Channels must be one of them.
    NotInVoice,
    InvalidStatus,
}

/// Trim a status: `None` clears it, more than [`MAX_STATUS_CHARS`] characters is refused.
pub fn validate_status(raw: &str) -> Result<Option<String>, StatusError> {
    let t = raw.trim();
    if t.chars().count() > MAX_STATUS_CHARS {
        return Err(StatusError::InvalidStatus);
    }
    Ok((!t.is_empty()).then(|| t.to_owned()))
}

/// Whether `perms` let a member change the status, given whether they are in the channel.
/// Managers change it from anywhere; everyone else needs the permission and a seat in it.
pub fn authorize(perms: Permissions, in_channel: bool) -> Result<(), StatusError> {
    if perms.contains(Permissions::MANAGE_CHANNELS) {
        return Ok(());
    }
    if !perms.contains(Permissions::SET_VOICE_CHANNEL_STATUS) {
        return Err(StatusError::MissingPermissions);
    }
    if in_channel {
        Ok(())
    } else {
        Err(StatusError::NotInVoice)
    }
}

impl ChannelStatus {
    /// `VOICE_CHANNEL_STATUS_UPDATE` body.
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.channel_id,
            "guild_id": self.guild_id,
            "status": self.text,
        })
    }
}

impl Gateway {
    /// The status of `channel`, if one is set.
    pub fn voice_channel_status(&self, channel: ChannelId) -> Option<ChannelStatus> {
        self.st().vx.statuses.get(&channel).cloned()
    }

    /// The statuses of `guild`'s channels that `viewer` can see (for `GUILD_CREATE`).
    pub async fn voice_statuses_for(&self, viewer: UserId, guild: GuildId) -> Vec<ChannelStatus> {
        let all: Vec<ChannelStatus> = self
            .st()
            .vx
            .statuses
            .values()
            .filter(|s| s.guild_id == guild)
            .cloned()
            .collect();
        let mut out = Vec::new();
        for s in all {
            if self
                .inner
                .dir
                .can_view_channel(viewer, guild, s.channel_id)
                .await
            {
                out.push(s);
            }
        }
        out.sort_by_key(|s| s.channel_id);
        out
    }

    /// `PUT /channels/{id}/voice-status`: set (`Some`) or clear (`None` or empty) the status.
    /// Returns the status now in force.
    pub async fn voice_status_set(
        &self,
        actor: UserId,
        channel: ChannelId,
        raw: Option<&str>,
    ) -> Result<Option<ChannelStatus>, StatusError> {
        if self.voice_cfg().is_none() {
            return Err(StatusError::Unavailable);
        }
        let dir = &self.inner.dir;
        let guild = dir
            .channel_guild(channel)
            .await
            .ok_or(StatusError::UnknownChannel)?;
        let info = dir
            .voice_channel(guild, channel)
            .await
            .ok_or(StatusError::UnknownChannel)?;
        if info.kind == VoiceChannelKind::Other {
            return Err(StatusError::NotVoiceChannel);
        }
        let perms = dir
            .channel_permissions(actor, guild, channel)
            .await
            .ok_or(StatusError::MissingPermissions)?;
        let text = validate_status(raw.unwrap_or(""))?;
        let in_channel = self
            .voice_state(actor)
            .is_some_and(|s| s.guild_id == Some(guild) && s.channel_id == channel);
        authorize(perms, in_channel)?;
        if text.is_some() && self.voice_occupants(channel).is_empty() {
            return Err(StatusError::NotInVoice);
        }
        let new = text.map(|text| ChannelStatus {
            guild_id: guild,
            channel_id: channel,
            text,
            set_by: actor,
            set_ms: unix_ms(),
        });
        let changed = {
            let mut st = self.st();
            let old = st.vx.statuses.get(&channel).map(|s| s.text.clone());
            match &new {
                Some(s) => {
                    st.vx.statuses.insert(channel, s.clone());
                }
                None => {
                    st.vx.statuses.remove(&channel);
                }
            }
            old != new.as_ref().map(|s| s.text.clone())
        };
        if changed {
            self.status_dispatch(guild, channel, new.as_ref()).await;
        }
        Ok(new)
    }

    async fn status_dispatch(&self, guild: GuildId, channel: ChannelId, s: Option<&ChannelStatus>) {
        let body = json!({
            "id": channel,
            "guild_id": guild,
            "status": s.map(|s| s.text.as_str()),
        });
        self.dispatch_guild(
            guild,
            "VOICE_CHANNEL_STATUS_UPDATE",
            body,
            Some(channel),
            None,
        )
        .await;
    }

    /// A member left `old.channel_id`: a status nobody is around for ends.
    pub(crate) async fn voice_status_member_left(&self, old: &VoiceState) {
        let Some(guild) = old.guild_id else { return };
        if !self.st().vx.statuses.contains_key(&old.channel_id) {
            return;
        }
        if self.voice_occupants(old.channel_id).is_empty() {
            self.st().vx.statuses.remove(&old.channel_id);
            self.status_dispatch(guild, old.channel_id, None).await;
        }
    }

    /// The channel was deleted: its status goes with it (the channel's own delete event tells
    /// clients).
    pub(crate) fn voice_status_channel_deleted(&self, channel: ChannelId) {
        self.st().vx.statuses.remove(&channel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Permissions as P;

    #[test]
    fn statuses_are_trimmed_bounded_and_an_empty_one_clears() {
        assert_eq!(
            validate_status("  Mario Kart  "),
            Ok(Some("Mario Kart".into()))
        );
        assert_eq!(validate_status("   "), Ok(None));
        assert_eq!(validate_status(""), Ok(None));
        assert!(validate_status(&"x".repeat(MAX_STATUS_CHARS)).is_ok());
        assert_eq!(
            validate_status(&"x".repeat(MAX_STATUS_CHARS + 1)),
            Err(StatusError::InvalidStatus)
        );
        // Characters, not bytes.
        assert!(validate_status(&"é".repeat(MAX_STATUS_CHARS)).is_ok());
    }

    #[test]
    fn a_member_in_the_channel_needs_the_permission() {
        assert_eq!(authorize(P::SET_VOICE_CHANNEL_STATUS, true), Ok(()));
        assert_eq!(
            authorize(P::empty(), true),
            Err(StatusError::MissingPermissions)
        );
        // Not in the channel: refused without Manage Channels.
        assert_eq!(
            authorize(P::SET_VOICE_CHANNEL_STATUS, false),
            Err(StatusError::NotInVoice)
        );
    }

    #[test]
    fn managers_may_change_it_from_anywhere() {
        assert_eq!(authorize(P::MANAGE_CHANNELS, false), Ok(()));
        assert_eq!(authorize(P::MANAGE_CHANNELS, true), Ok(()));
    }
}
