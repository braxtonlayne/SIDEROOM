//! Stage channels (VOX-STG-001): stage instances (topic, privacy), the speaker and audience
//! split, request to speak, invite to speak.
//!
//! # Model (Discord)
//!
//! * A **stage instance** is the live event in a stage channel. At most one per channel.
//!   Moderators (Manage Channels + Mute Members + Move Members in the channel) start, edit
//!   and end it. It ends by itself when the last member leaves.
//! * Everyone who joins a stage channel is **suppressed** (audience) except moderators, who
//!   speak at once. A suppressed member cannot send audio: the flag is part of the media
//!   flags the SFU enforces (`media_flags` in `crate::voice`).
//! * An audience member with Request to Speak can **raise a hand**
//!   (`request_to_speak_timestamp`). A moderator **invites** them (`suppress: false`) or
//!   moves a speaker back to the audience (`suppress: true`); a speaker can step down on
//!   their own.
//!
//! # Storage
//!
//! Instances live in memory on the node, like the voice states they describe. A stage with
//! nobody in it ends, so nothing needs to survive a restart. Changes are announced as
//! `STAGE_INSTANCE_CREATE/UPDATE/DELETE` to the members who can see the channel.

use serde_json::{Value, json};
use sr_core::{ChannelId, GuildId, Permissions, UserId};

use crate::hub::Gateway;
use crate::voice::{VoiceChannelKind, VoiceState, is_stage_moderator, unix_ms};

/// Longest stage topic, in characters.
pub const MAX_TOPIC_CHARS: usize = 120;
/// `GUILD_ONLY`, the only privacy level Discord still accepts.
pub const PRIVACY_GUILD_ONLY: u8 = 2;

/// A live stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageInstance {
    pub guild_id: GuildId,
    pub channel_id: ChannelId,
    pub topic: String,
    pub privacy_level: u8,
    pub created_ms: i64,
    /// Who started it.
    pub started_by: UserId,
    /// The scheduled event this stage belongs to, when it was opened by one (G-93).
    pub scheduled_event_id: Option<u64>,
}

impl StageInstance {
    /// Discord shape. The id is the channel id: a channel has one instance at a time.
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.channel_id,
            "guild_id": self.guild_id,
            "channel_id": self.channel_id,
            "topic": self.topic,
            "privacy_level": self.privacy_level,
            "discoverable_disabled": true,
            "guild_scheduled_event_id": self.scheduled_event_id.map(|id| id.to_string()),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageError {
    /// Voice is switched off on this installation.
    Unavailable,
    UnknownChannel,
    /// The channel exists but is not a stage channel.
    NotStageChannel,
    /// Needs Manage Channels, Mute Members and Move Members (or, for self changes, the
    /// permission named in the docs of the call).
    MissingPermissions,
    /// A stage is already running in the channel (Discord 150006).
    AlreadyExists,
    /// No stage is running in the channel (Discord 10067).
    NoInstance,
    /// The caller is not connected to the stage channel.
    NotInVoice,
    InvalidTopic,
    InvalidPrivacy,
}

/// Check a topic: 1 to [`MAX_TOPIC_CHARS`] characters once trimmed.
pub fn validate_topic(raw: &str) -> Result<String, StageError> {
    let t = raw.trim();
    if t.is_empty() || t.chars().count() > MAX_TOPIC_CHARS {
        return Err(StageError::InvalidTopic);
    }
    Ok(t.to_owned())
}

/// What a member asks for about their own state in a stage
/// (`PATCH /guilds/{id}/voice-states/@me`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelfStagePatch {
    /// `false` to become a speaker (needs Mute Members), `true` to step down.
    pub suppress: Option<bool>,
    /// `Some(true)` raises a hand (needs Request to Speak), `Some(false)` lowers it.
    pub request_to_speak: Option<bool>,
}

/// The pure part of [`SelfStagePatch`]: whether `perms` allow it for a member who is
/// currently `suppressed`.
pub fn authorize_self(
    patch: &SelfStagePatch,
    perms: Permissions,
    suppressed: bool,
) -> Result<(), StageError> {
    if patch.suppress == Some(false) && !perms.contains(Permissions::MUTE_MEMBERS) {
        return Err(StageError::MissingPermissions);
    }
    if patch.request_to_speak == Some(true) {
        // Speakers have nothing to ask for; the audience needs the permission.
        if !suppressed {
            return Err(StageError::MissingPermissions);
        }
        if !perms.contains(Permissions::REQUEST_TO_SPEAK) {
            return Err(StageError::MissingPermissions);
        }
    }
    Ok(())
}

impl Gateway {
    /// The stage running in `channel`, if any.
    pub fn stage_instance(&self, channel: ChannelId) -> Option<StageInstance> {
        self.st().vx.stages.get(&channel).cloned()
    }

    /// Stages running in `guild`, for the members who can see `viewer`'s channels.
    pub async fn stage_instances_for(&self, viewer: UserId, guild: GuildId) -> Vec<StageInstance> {
        let all: Vec<StageInstance> = self
            .st()
            .vx
            .stages
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
        out
    }

    /// The stage running in `channel` when `viewer` may see that channel.
    pub async fn stage_visible_to(
        &self,
        viewer: UserId,
        channel: ChannelId,
    ) -> Option<StageInstance> {
        let inst = self.stage_instance(channel)?;
        self.inner
            .dir
            .can_view_channel(viewer, inst.guild_id, channel)
            .await
            .then_some(inst)
    }

    /// Resolve `channel` to a stage channel and `actor`'s permissions in it.
    async fn stage_target(
        &self,
        actor: UserId,
        channel: ChannelId,
    ) -> Result<(GuildId, Permissions), StageError> {
        if self.voice_cfg().is_none() {
            return Err(StageError::Unavailable);
        }
        let dir = &self.inner.dir;
        let guild = dir
            .channel_guild(channel)
            .await
            .ok_or(StageError::UnknownChannel)?;
        let info = dir
            .voice_channel(guild, channel)
            .await
            .ok_or(StageError::UnknownChannel)?;
        if info.kind != VoiceChannelKind::Stage {
            return Err(StageError::NotStageChannel);
        }
        let perms = dir
            .channel_permissions(actor, guild, channel)
            .await
            .ok_or(StageError::MissingPermissions)?;
        Ok((guild, perms))
    }

    async fn stage_dispatch(&self, name: &str, st: &StageInstance) {
        self.dispatch_guild(st.guild_id, name, st.to_json(), Some(st.channel_id), None)
            .await;
    }

    /// `POST /stage-instances`: start a stage. Moderators only.
    pub async fn stage_create(
        &self,
        actor: UserId,
        channel: ChannelId,
        topic: &str,
        privacy_level: Option<u8>,
    ) -> Result<StageInstance, StageError> {
        let (guild, perms) = self.stage_target(actor, channel).await?;
        if !is_stage_moderator(perms) {
            return Err(StageError::MissingPermissions);
        }
        let topic = validate_topic(topic)?;
        let privacy = privacy_level.unwrap_or(PRIVACY_GUILD_ONLY);
        if privacy != PRIVACY_GUILD_ONLY {
            return Err(StageError::InvalidPrivacy);
        }
        let inst = StageInstance {
            guild_id: guild,
            channel_id: channel,
            topic,
            privacy_level: privacy,
            created_ms: unix_ms(),
            started_by: actor,
            scheduled_event_id: None,
        };
        {
            let mut st = self.st();
            if st.vx.stages.contains_key(&channel) {
                return Err(StageError::AlreadyExists);
            }
            st.vx.stages.insert(channel, inst.clone());
        }
        self.stage_dispatch("STAGE_INSTANCE_CREATE", &inst).await;
        Ok(inst)
    }

    /// A starting stage event opens the stage of its channel with the event name as topic
    /// (G-93, D-222). There is no moderator check: starting the event is the authority.
    pub async fn stage_open_for_event(
        &self,
        channel: ChannelId,
        topic: &str,
        started_by: UserId,
        event_id: u64,
    ) -> Result<StageInstance, StageError> {
        if self.voice_cfg().is_none() {
            return Err(StageError::Unavailable);
        }
        let dir = &self.inner.dir;
        let guild = dir
            .channel_guild(channel)
            .await
            .ok_or(StageError::UnknownChannel)?;
        let info = dir
            .voice_channel(guild, channel)
            .await
            .ok_or(StageError::UnknownChannel)?;
        if info.kind != VoiceChannelKind::Stage {
            return Err(StageError::NotStageChannel);
        }
        let inst = StageInstance {
            guild_id: guild,
            channel_id: channel,
            topic: validate_topic(topic)?,
            privacy_level: PRIVACY_GUILD_ONLY,
            created_ms: unix_ms(),
            started_by,
            scheduled_event_id: Some(event_id),
        };
        {
            let mut st = self.st();
            if st.vx.stages.contains_key(&channel) {
                return Err(StageError::AlreadyExists);
            }
            st.vx.stages.insert(channel, inst.clone());
        }
        self.stage_dispatch("STAGE_INSTANCE_CREATE", &inst).await;
        Ok(inst)
    }

    /// A stage event ended: delete the stage instance of its channel when one is live.
    pub async fn stage_end_for_event(&self, channel: ChannelId) {
        if self.stage_instance_exists(channel) {
            self.stage_finish(channel).await;
        }
    }

    /// `PATCH /stage-instances/{channel}`: change the topic or privacy. Moderators only.
    pub async fn stage_update(
        &self,
        actor: UserId,
        channel: ChannelId,
        topic: Option<&str>,
        privacy_level: Option<u8>,
    ) -> Result<StageInstance, StageError> {
        let (_, perms) = self.stage_target(actor, channel).await?;
        if !is_stage_moderator(perms) {
            return Err(StageError::MissingPermissions);
        }
        let topic = topic.map(validate_topic).transpose()?;
        if privacy_level.is_some_and(|p| p != PRIVACY_GUILD_ONLY) {
            return Err(StageError::InvalidPrivacy);
        }
        let inst = {
            let mut st = self.st();
            let inst = st
                .vx
                .stages
                .get_mut(&channel)
                .ok_or(StageError::NoInstance)?;
            if let Some(t) = topic {
                inst.topic = t;
            }
            inst.clone()
        };
        self.stage_dispatch("STAGE_INSTANCE_UPDATE", &inst).await;
        Ok(inst)
    }

    /// `DELETE /stage-instances/{channel}`: end the stage. Moderators only. Everyone who is
    /// not a moderator goes back to the audience.
    pub async fn stage_end(&self, actor: UserId, channel: ChannelId) -> Result<(), StageError> {
        let (_, perms) = self.stage_target(actor, channel).await?;
        if !is_stage_moderator(perms) {
            return Err(StageError::MissingPermissions);
        }
        if !self.stage_instance_exists(channel) {
            return Err(StageError::NoInstance);
        }
        self.stage_finish(channel).await;
        Ok(())
    }

    fn stage_instance_exists(&self, channel: ChannelId) -> bool {
        self.st().vx.stages.contains_key(&channel)
    }

    /// Remove the instance, announce it and send the audience back to listening.
    async fn stage_finish(&self, channel: ChannelId) {
        let Some(inst) = self.st().vx.stages.remove(&channel) else {
            return;
        };
        self.stage_dispatch("STAGE_INSTANCE_DELETE", &inst).await;
        let dir = &self.inner.dir;
        for cur in self.voice_occupants(channel) {
            let perms = dir
                .channel_permissions(cur.user_id, inst.guild_id, channel)
                .await
                .unwrap_or_else(Permissions::empty);
            let mut new = cur.clone();
            new.suppress = !is_stage_moderator(perms);
            new.request_to_speak_ms = None;
            if new != cur {
                self.commit_state_inplace(&cur, new, perms).await;
            }
        }
    }

    /// A member left `old.channel_id`: a stage nobody is in any more ends.
    pub(crate) async fn stage_member_left(&self, old: &VoiceState) {
        if !self.stage_instance_exists(old.channel_id) {
            return;
        }
        if self.voice_occupants(old.channel_id).is_empty() {
            self.stage_finish(old.channel_id).await;
        }
    }

    /// The channel was deleted: its stage goes with it.
    pub(crate) async fn stage_channel_deleted(&self, channel: ChannelId) {
        self.stage_finish(channel).await;
    }

    /// `PATCH /guilds/{id}/voice-states/@me`: raise or lower a hand, or step down.
    pub async fn stage_self_update(
        &self,
        user: UserId,
        guild: GuildId,
        channel: ChannelId,
        patch: SelfStagePatch,
    ) -> Result<VoiceState, StageError> {
        if self.voice_cfg().is_none() {
            return Err(StageError::Unavailable);
        }
        let dir = &self.inner.dir;
        let cur = self
            .voice_state(user)
            .filter(|s| s.guild_id == Some(guild) && s.channel_id == channel)
            .ok_or(StageError::NotInVoice)?;
        let info = dir
            .voice_channel(guild, channel)
            .await
            .ok_or(StageError::UnknownChannel)?;
        if info.kind != VoiceChannelKind::Stage {
            return Err(StageError::NotStageChannel);
        }
        let perms = dir
            .channel_permissions(user, guild, channel)
            .await
            .ok_or(StageError::MissingPermissions)?;
        authorize_self(&patch, perms, cur.suppress)?;
        if patch.request_to_speak == Some(true) && !self.stage_instance_exists(channel) {
            return Err(StageError::NoInstance);
        }
        let mut new = cur.clone();
        match patch.request_to_speak {
            Some(true) => new.request_to_speak_ms = Some(unix_ms()),
            Some(false) => new.request_to_speak_ms = None,
            None => {}
        }
        if let Some(sup) = patch.suppress {
            new.suppress = sup;
            // A speaker needs no hand up, and one who steps down starts with it lowered.
            new.request_to_speak_ms = None;
        }
        if new != cur {
            self.commit_state_inplace(&cur, new.clone(), perms).await;
        }
        Ok(new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Permissions as P;

    #[test]
    fn topics_are_trimmed_and_bounded() {
        assert_eq!(validate_topic("  Town hall  "), Ok("Town hall".into()));
        assert_eq!(validate_topic("   "), Err(StageError::InvalidTopic));
        assert_eq!(validate_topic(""), Err(StageError::InvalidTopic));
        assert!(validate_topic(&"x".repeat(MAX_TOPIC_CHARS)).is_ok());
        assert_eq!(
            validate_topic(&"x".repeat(MAX_TOPIC_CHARS + 1)),
            Err(StageError::InvalidTopic)
        );
        // Characters, not bytes.
        assert!(validate_topic(&"é".repeat(MAX_TOPIC_CHARS)).is_ok());
    }

    #[test]
    fn raising_a_hand_needs_the_permission_and_an_audience_seat() {
        let ask = SelfStagePatch {
            request_to_speak: Some(true),
            ..SelfStagePatch::default()
        };
        assert_eq!(authorize_self(&ask, P::REQUEST_TO_SPEAK, true), Ok(()));
        assert_eq!(
            authorize_self(&ask, P::empty(), true),
            Err(StageError::MissingPermissions)
        );
        // A speaker has nothing to ask for.
        assert_eq!(
            authorize_self(&ask, P::REQUEST_TO_SPEAK, false),
            Err(StageError::MissingPermissions)
        );
        // Lowering a hand is always fine.
        let lower = SelfStagePatch {
            request_to_speak: Some(false),
            ..SelfStagePatch::default()
        };
        assert_eq!(authorize_self(&lower, P::empty(), true), Ok(()));
    }

    #[test]
    fn only_moderators_unsuppress_themselves_but_anyone_steps_down() {
        let up = SelfStagePatch {
            suppress: Some(false),
            ..SelfStagePatch::default()
        };
        assert_eq!(authorize_self(&up, P::MUTE_MEMBERS, true), Ok(()));
        assert_eq!(
            authorize_self(&up, P::REQUEST_TO_SPEAK, true),
            Err(StageError::MissingPermissions)
        );
        let down = SelfStagePatch {
            suppress: Some(true),
            ..SelfStagePatch::default()
        };
        assert_eq!(authorize_self(&down, P::empty(), false), Ok(()));
    }

    #[test]
    fn instance_json_matches_discords_shape() {
        let i = StageInstance {
            guild_id: GuildId::new(1),
            channel_id: ChannelId::new(2),
            topic: "t".into(),
            privacy_level: PRIVACY_GUILD_ONLY,
            created_ms: 0,
            started_by: UserId::new(3),
            scheduled_event_id: None,
        };
        let v = i.to_json();
        for k in [
            "id",
            "guild_id",
            "channel_id",
            "topic",
            "privacy_level",
            "discoverable_disabled",
            "guild_scheduled_event_id",
        ] {
            assert!(v.get(k).is_some(), "missing {k}");
        }
        assert_eq!(v["privacy_level"], 2);
        assert_eq!(v["channel_id"], "2");
    }
}
