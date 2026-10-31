//! Voice wiring: the embedded SFU, the gateway's voice configuration and the REST backend
//! that connects moderator actions to the gateway and the audit log.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use async_trait::async_trait;
use serde_json::json;
use sr_api::routes::guilds::backend::AuditSink;
use sr_api::routes::voice::{
    SelfStageRequest, SoundEffect, StageInstanceObject, StreamPreview, TurnCredentialsObject,
    VoiceBackend, VoiceError, VoicePatch,
};
use sr_chat::settings::audit::{AuditAction, AuditChange};
use sr_core::config::Config;
use sr_core::{ChannelId, GuildId, UserId};
use sr_gateway::{
    CallError, Gateway, ModAction, ModError, ModPatch, PreviewError, SelfStagePatch, SoundPlay,
    SoundPlayError, StageError, StageInstance, StatusError, VoiceConfig as GatewayVoice,
};
use sr_voice_proto::{TokenSigner, TurnConfig, derive_secret};

/// Everything the composition root needs once voice is switched on.
pub struct VoiceSetup {
    /// The gateway side (join tokens, relay to the SFU).
    pub gateway: GatewayVoice,
    /// The running SFU, for its routes and shutdown.
    pub sfu: sr_voice::SfuHandle,
}

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// TURN settings from the config (empty when voice is off).
pub fn turn_config(cfg: &Config) -> TurnConfig {
    TurnConfig {
        urls: cfg.voice.turn.urls.clone(),
        secret: cfg.voice.turn.secret.expose().to_owned(),
        ttl_secs: cfg.voice.turn.ttl_secs,
    }
}

/// `ws(s)://<host>/voice/ws` for the embedded SFU, from `server.public_url`.
pub fn default_endpoint(public_url: &str) -> String {
    let rest = public_url.trim_end_matches('/');
    let ws = if let Some(r) = rest.strip_prefix("https://") {
        format!("wss://{r}")
    } else if let Some(r) = rest.strip_prefix("http://") {
        format!("ws://{r}")
    } else {
        format!("ws://{rest}")
    };
    format!("{ws}/voice/ws")
}

/// Start the embedded SFU and build the gateway's voice configuration.
pub async fn start(cfg: &Config, master_key: &[u8; 32]) -> anyhow::Result<VoiceSetup> {
    let v = &cfg.voice;
    let secret = if v.token_secret.expose().is_empty() {
        derive_secret(master_key)
    } else {
        v.token_secret.expose().as_bytes().to_vec()
    };
    let sfu = sr_voice::start(sr_voice::SfuConfig {
        bind_ip: v.bind_ip.parse().context("voice.bind_ip")?,
        udp_port_min: v.udp_port_min,
        udp_port_max: v.udp_port_max,
        tcp_port: (v.tcp_port != 0).then_some(v.tcp_port),
        announce_ips: v
            .announce_ips
            .iter()
            .map(|ip| ip.parse())
            .collect::<Result<_, _>>()
            .context("voice.announce_ips")?,
        max_participants: v.max_participants,
        max_room_size: v.max_room_size,
        resume_window: std::time::Duration::from_secs(20),
        heartbeat_interval: std::time::Duration::from_millis(13_750),
        token_secret: secret.clone(),
        egress_budget_mbps: f64::from(v.egress_budget_mbps),
        video_bwe: v.video_bwe,
        datachannel_kbps: v.datachannel_kbps,
        shards: v.shards,
    })
    .await
    .context("starting the voice SFU")?;
    let endpoints = if v.endpoints.is_empty() {
        vec![default_endpoint(&cfg.server.public_url)]
    } else {
        v.endpoints.clone()
    };
    tracing::info!(
        udp = ?sfu.udp_ports().first().zip(sfu.udp_ports().last()),
        tcp = ?sfu.tcp_port(),
        ?endpoints,
        "voice SFU started"
    );
    Ok(VoiceSetup {
        gateway: GatewayVoice {
            signer: TokenSigner::new(secret),
            endpoints,
            turn: turn_config(cfg),
            relay: Some(Arc::new(sfu.clone())),
            session_grace: std::time::Duration::from_secs(v.session_grace_secs),
            calls: None,
        },
        sfu,
    })
}

/// REST backend: TURN credentials from the config, moderation through the gateway.
pub struct ServerVoice {
    gateway: Gateway,
    turn: TurnConfig,
    enabled: bool,
    audit: Arc<dyn AuditSink>,
}

impl ServerVoice {
    pub fn new(
        gateway: Gateway,
        turn: TurnConfig,
        enabled: bool,
        audit: Arc<dyn AuditSink>,
    ) -> Self {
        Self {
            gateway,
            turn,
            enabled,
            audit,
        }
    }

    async fn record(
        &self,
        guild: GuildId,
        actor: UserId,
        target: UserId,
        actions: &[ModAction],
        reason: Option<&str>,
    ) {
        for a in actions {
            let (action, changes) = match a {
                ModAction::Muted(on) => (
                    AuditAction::MemberUpdate,
                    vec![AuditChange::new("mute", Some(json!(!on)), Some(json!(on)))],
                ),
                ModAction::Deafened(on) => (
                    AuditAction::MemberUpdate,
                    vec![AuditChange::new("deaf", Some(json!(!on)), Some(json!(on)))],
                ),
                ModAction::Moved { from, to } => (
                    AuditAction::MemberMove,
                    vec![AuditChange::new(
                        "channel_id",
                        Some(json!(from.to_string())),
                        Some(json!(to.to_string())),
                    )],
                ),
                ModAction::Suppressed(on) => (
                    AuditAction::MemberUpdate,
                    vec![AuditChange::new(
                        "suppress",
                        Some(json!(!on)),
                        Some(json!(on)),
                    )],
                ),
                ModAction::Disconnected { from } => (
                    AuditAction::MemberDisconnect,
                    vec![AuditChange::new(
                        "channel_id",
                        Some(json!(from.to_string())),
                        None,
                    )],
                ),
            };
            self.audit
                .record(guild, actor, action, Some(target.get()), changes, reason)
                .await;
        }
    }
}

fn map_err(e: ModError) -> VoiceError {
    match e {
        ModError::NotInVoice => VoiceError::NotInVoice,
        ModError::MissingPermissions => VoiceError::MissingPermissions,
        ModError::UnknownChannel => VoiceError::UnknownChannel,
        ModError::NotVoiceChannel => VoiceError::NotVoiceChannel,
        ModError::NotStageChannel => VoiceError::NotStageChannel,
        ModError::Unavailable => VoiceError::Unavailable,
    }
}

fn stage_object(i: StageInstance) -> StageInstanceObject {
    StageInstanceObject {
        id: i.channel_id.to_string(),
        guild_id: i.guild_id.to_string(),
        channel_id: i.channel_id.to_string(),
        topic: i.topic,
        privacy_level: i.privacy_level,
        discoverable_disabled: true,
        guild_scheduled_event_id: None,
    }
}

fn stage_err(e: StageError) -> VoiceError {
    match e {
        StageError::Unavailable => VoiceError::Unavailable,
        StageError::UnknownChannel => VoiceError::UnknownChannel,
        StageError::NotStageChannel => VoiceError::NotStageChannel,
        StageError::MissingPermissions => VoiceError::MissingPermissions,
        StageError::AlreadyExists => VoiceError::StageExists,
        StageError::NoInstance => VoiceError::NoStageInstance,
        StageError::NotInVoice => VoiceError::NotInVoice,
        StageError::InvalidTopic => VoiceError::InvalidTopic,
        StageError::InvalidPrivacy => VoiceError::InvalidPrivacy,
    }
}

fn status_err(e: StatusError) -> VoiceError {
    match e {
        StatusError::Unavailable => VoiceError::Unavailable,
        StatusError::UnknownChannel => VoiceError::UnknownChannel,
        StatusError::NotVoiceChannel => VoiceError::NotVoiceChannel,
        StatusError::MissingPermissions => VoiceError::MissingPermissions,
        StatusError::NotInVoice => VoiceError::NotInVoice,
        StatusError::InvalidStatus => VoiceError::InvalidStatus,
    }
}

fn preview_err(e: PreviewError) -> VoiceError {
    match e {
        PreviewError::Unavailable => VoiceError::Unavailable,
        PreviewError::NotFound | PreviewError::NotOwner => VoiceError::NotFound,
        PreviewError::NotJpeg => VoiceError::NotJpeg,
        PreviewError::TooLarge => VoiceError::TooLarge,
        PreviewError::RateLimited => VoiceError::RateLimited,
    }
}

fn call_err(e: CallError) -> VoiceError {
    match e {
        CallError::Unavailable => VoiceError::Unavailable,
        CallError::NotFound => VoiceError::NotFound,
        CallError::NotInCall => VoiceError::NotInCall,
        CallError::NotAMember => VoiceError::NotAMember,
    }
}

#[async_trait]
impl VoiceBackend for ServerVoice {
    fn turn_credentials(&self, user: UserId) -> Option<TurnCredentialsObject> {
        if !self.enabled {
            return None;
        }
        self.turn
            .credentials(user, unix_secs())
            .map(|c| TurnCredentialsObject {
                username: c.username,
                password: c.password,
                ttl: c.ttl,
                uris: c.uris,
            })
    }

    async fn moderate(
        &self,
        actor: UserId,
        guild: GuildId,
        target: UserId,
        patch: VoicePatch,
        reason: Option<String>,
    ) -> Result<(), VoiceError> {
        let out = self
            .gateway
            .moderate_voice(
                actor,
                guild,
                target,
                ModPatch {
                    mute: patch.mute,
                    deaf: patch.deaf,
                    channel: patch.channel,
                    suppress: patch.suppress,
                },
            )
            .await
            .map_err(map_err)?;
        self.record(guild, actor, target, &out.actions, reason.as_deref())
            .await;
        Ok(())
    }

    async fn stage_create(
        &self,
        actor: UserId,
        channel: ChannelId,
        topic: String,
        privacy_level: Option<u8>,
    ) -> Result<StageInstanceObject, VoiceError> {
        self.gateway
            .stage_create(actor, channel, &topic, privacy_level)
            .await
            .map(stage_object)
            .map_err(stage_err)
    }

    async fn stage_get(
        &self,
        actor: UserId,
        channel: ChannelId,
    ) -> Result<StageInstanceObject, VoiceError> {
        self.gateway
            .stage_visible_to(actor, channel)
            .await
            .map(stage_object)
            .ok_or(VoiceError::NoStageInstance)
    }

    async fn stage_list(
        &self,
        actor: UserId,
        guild: GuildId,
    ) -> Result<Vec<StageInstanceObject>, VoiceError> {
        Ok(self
            .gateway
            .stage_instances_for(actor, guild)
            .await
            .into_iter()
            .map(stage_object)
            .collect())
    }

    async fn stage_update(
        &self,
        actor: UserId,
        channel: ChannelId,
        topic: Option<String>,
        privacy_level: Option<u8>,
    ) -> Result<StageInstanceObject, VoiceError> {
        self.gateway
            .stage_update(actor, channel, topic.as_deref(), privacy_level)
            .await
            .map(stage_object)
            .map_err(stage_err)
    }

    async fn stage_end(&self, actor: UserId, channel: ChannelId) -> Result<(), VoiceError> {
        self.gateway
            .stage_end(actor, channel)
            .await
            .map_err(stage_err)
    }

    async fn voice_status_set(
        &self,
        actor: UserId,
        channel: ChannelId,
        status: Option<String>,
    ) -> Result<(), VoiceError> {
        self.gateway
            .voice_status_set(actor, channel, status.as_deref())
            .await
            .map(|_| ())
            .map_err(status_err)
    }

    async fn voice_timer_set(
        &self,
        actor: UserId,
        channel: ChannelId,
        timer: Option<serde_json::Value>,
        label: String,
    ) -> Result<(), VoiceError> {
        self.gateway
            .voice_timer_set(actor, channel, timer.as_ref(), &label)
            .await
            .map_err(status_err)
    }

    async fn stage_self(
        &self,
        actor: UserId,
        guild: GuildId,
        channel: ChannelId,
        req: SelfStageRequest,
    ) -> Result<(), VoiceError> {
        self.gateway
            .stage_self_update(
                actor,
                guild,
                channel,
                SelfStagePatch {
                    suppress: req.suppress,
                    request_to_speak: req.request_to_speak,
                },
            )
            .await
            .map(|_| ())
            .map_err(stage_err)
    }

    async fn stream_preview_put(
        &self,
        actor: UserId,
        stream_key: &str,
        jpeg: Vec<u8>,
    ) -> Result<(), VoiceError> {
        self.gateway
            .stream_preview_put(actor, stream_key, jpeg)
            .map(|_| ())
            .map_err(preview_err)
    }

    async fn stream_preview_get(
        &self,
        actor: UserId,
        stream_key: &str,
    ) -> Result<StreamPreview, VoiceError> {
        let (jpeg, uploaded_ms) = self
            .gateway
            .stream_preview_get(actor, stream_key)
            .await
            .map_err(preview_err)?;
        Ok(StreamPreview {
            jpeg: jpeg.as_ref().clone(),
            uploaded_ms,
        })
    }

    async fn call_get(
        &self,
        actor: UserId,
        channel: ChannelId,
    ) -> Result<serde_json::Value, VoiceError> {
        self.gateway
            .call_state(actor, channel)
            .ok_or(VoiceError::NotFound)
    }

    async fn call_ring(
        &self,
        actor: UserId,
        channel: ChannelId,
        recipients: Option<Vec<UserId>>,
    ) -> Result<(), VoiceError> {
        self.gateway
            .call_ring(actor, channel, recipients)
            .await
            .map_err(call_err)
    }

    async fn call_stop_ringing(
        &self,
        actor: UserId,
        channel: ChannelId,
        recipients: Option<Vec<UserId>>,
    ) -> Result<(), VoiceError> {
        self.gateway
            .call_stop_ringing(actor, channel, recipients)
            .await
            .map_err(call_err)
    }

    async fn soundboard_play(
        &self,
        actor: UserId,
        guild: GuildId,
        channel: ChannelId,
        effect: SoundEffect,
    ) -> Result<(), VoiceError> {
        self.gateway
            .voice_sound_effect(
                actor,
                guild,
                channel,
                &SoundPlay {
                    sound_id: effect.sound_id,
                    volume: effect.volume,
                    emoji_id: effect.emoji_id,
                    emoji_name: effect.emoji_name,
                },
            )
            .await
            .map_err(|e| match e {
                SoundPlayError::Unavailable => VoiceError::Unavailable,
                SoundPlayError::NotInVoice => VoiceError::NotInVoice,
                SoundPlayError::Silenced => VoiceError::Silenced,
                SoundPlayError::Cooldown(_) => VoiceError::Cooldown,
            })
    }
}

/// Writes the `CALL` system message into a DM when its call ends (VOX-SIG-008). The messages
/// module is built after the gateway, so it is handed over later with [`CallRecorder::bind`].
#[derive(Default)]
pub struct CallRecorder {
    messages: std::sync::OnceLock<Arc<sr_api::routes::messages::MessagesDeps>>,
}

impl CallRecorder {
    pub fn bind(&self, messages: Arc<sr_api::routes::messages::MessagesDeps>) {
        let _ = self.messages.set(messages);
    }
}

#[async_trait]
impl sr_gateway::CallHooks for CallRecorder {
    async fn call_ended(&self, s: sr_gateway::CallSummary) {
        let Some(messages) = self.messages.get() else {
            return;
        };
        if let Err(e) = sr_api::routes::messages::call_record::post_call_record(
            messages,
            s.channel_id,
            s.initiator,
            &s.participants,
            s.started_ms,
            s.ended_ms,
        )
        .await
        {
            tracing::warn!(error = ?e, channel = %s.channel_id, "could not write the call record");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_endpoint_follows_the_public_url() {
        assert_eq!(
            default_endpoint("https://chat.example.org/"),
            "wss://chat.example.org/voice/ws"
        );
        assert_eq!(
            default_endpoint("http://localhost:8080"),
            "ws://localhost:8080/voice/ws"
        );
    }

    #[test]
    fn turn_config_comes_from_the_voice_section() {
        let mut cfg = Config::default();
        cfg.voice.turn.urls = vec!["turn:t:3478".into()];
        cfg.voice.turn.secret = sr_core::config::Secret::new("s");
        let t = turn_config(&cfg);
        assert_eq!(t.urls, vec!["turn:t:3478"]);
        assert_eq!(t.secret, "s");
        assert_eq!(t.ttl_secs, 3600);
    }
}
