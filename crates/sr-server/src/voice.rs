//! Voice wiring: the embedded SFU, the gateway's voice configuration and the REST backend
//! that connects moderator actions to the gateway and the audit log.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use async_trait::async_trait;
use serde_json::json;
use sr_api::routes::guilds::backend::AuditSink;
use sr_api::routes::voice::{TurnCredentialsObject, VoiceBackend, VoiceError, VoicePatch};
use sr_chat::settings::audit::{AuditAction, AuditChange};
use sr_core::config::Config;
use sr_core::{GuildId, UserId};
use sr_gateway::{Gateway, ModAction, ModError, ModPatch, VoiceConfig as GatewayVoice};
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
        ModError::StageUnsupported => VoiceError::StageUnsupported,
        ModError::Unavailable => VoiceError::Unavailable,
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
                },
            )
            .await
            .map_err(map_err)?;
        self.record(guild, actor, target, &out.actions, reason.as_deref())
            .await;
        Ok(())
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
