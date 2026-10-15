//! Soundboard on the gateway (VOX-SB-001): op 31 `REQUEST_SOUNDBOARD_SOUNDS` and the
//! `VOICE_CHANNEL_EFFECT_SEND` dispatch that tells a voice channel who played which sound.
//!
//! The sound itself is played by the sender's client (it mixes the decoded file into its
//! microphone stream); this module only checks that the sender may be heard right now
//! and announces the play.

use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Value, json};
use sr_core::{ChannelId, GuildId, UserId};

use crate::hub::Gateway;
use crate::session::Session;

/// Most guilds one op 31 may name.
pub const MAX_REQUEST_GUILDS: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundPlayError {
    Unavailable,
    /// The sender is not in that voice channel.
    NotInVoice,
    /// Muted, deafened, in the audience or in the AFK channel: nobody would hear it.
    Silenced,
    /// Another sound was played a moment ago; wait this many milliseconds.
    Cooldown(i64),
}

/// What `VOICE_CHANNEL_EFFECT_SEND` carries for a soundboard play.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundPlay {
    pub sound_id: String,
    pub volume: f64,
    pub emoji_id: Option<String>,
    pub emoji_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RequestIn {
    guild_ids: Vec<GuildId>,
}

impl Gateway {
    /// Op 31: send `SOUNDBOARD_SOUNDS` for each requested guild the caller is in.
    pub(crate) async fn soundboard_request(&self, s: &Arc<Session>, d: Value) {
        let Ok(req) = serde_json::from_value::<RequestIn>(d) else {
            return;
        };
        let mine = self.inner.dir.guilds_for_user(s.user).await;
        for g in req
            .guild_ids
            .into_iter()
            .take(MAX_REQUEST_GUILDS)
            .filter(|g| mine.contains(g))
        {
            let sounds = self.inner.dir.soundboard_sounds(g).await;
            s.deliver(
                "SOUNDBOARD_SOUNDS",
                json!({ "guild_id": g, "soundboard_sounds": sounds }),
            );
        }
    }

    /// Announce that `user` played a sound in `channel`. The caller has already checked
    /// permissions and the sound; this checks the member is in the channel and audible.
    pub async fn voice_sound_effect(
        &self,
        user: UserId,
        guild: GuildId,
        channel: ChannelId,
        play: &SoundPlay,
    ) -> Result<(), SoundPlayError> {
        if self.voice_cfg().is_none() {
            return Err(SoundPlayError::Unavailable);
        }
        let st = self
            .voice_state(user)
            .filter(|s| s.guild_id == Some(guild) && s.channel_id == channel)
            .ok_or(SoundPlayError::NotInVoice)?;
        if st.mute || st.self_mute || st.deaf || st.self_deaf || st.suppress || st.afk {
            return Err(SoundPlayError::Silenced);
        }
        let played = self
            .st()
            .vx
            .cooldown
            .try_play(user, crate::voice::unix_ms());
        if let Err(wait) = played {
            return Err(SoundPlayError::Cooldown(wait));
        }
        self.voice_touch(user);
        let emoji = if play.emoji_id.is_some() || play.emoji_name.is_some() {
            json!({ "id": play.emoji_id, "name": play.emoji_name })
        } else {
            Value::Null
        };
        self.dispatch_guild(
            guild,
            "VOICE_CHANNEL_EFFECT_SEND",
            json!({
                "channel_id": channel,
                "guild_id": guild,
                "user_id": user,
                "emoji": emoji,
                "animation_type": null,
                "animation_id": null,
                "sound_id": play.sound_id,
                "sound_volume": play.volume,
            }),
            Some(channel),
            None,
        )
        .await;
        Ok(())
    }
}
