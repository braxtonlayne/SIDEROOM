//! BRG-COR-001: the Discord bridge wired into the server process.
//!
//! * [`PgSealer`] seals the bot token and the webhook tokens with `server.secret_key` (the layout of TOTP
//!   secrets, D-185; `sr_db::sealed::SecretBox`).
//! * [`CdnMedia`] signs the avatar and attachment links that leave the instance (24 hour life).
//! * [`PosterNotices`] lets the system user speak in a mapped channel (a map was created or removed, the bridge
//!   was switched off), and sends a `CHANNEL_UPDATE` when a channel's bridge state changes (the gateway adds
//!   `sr_bridge` to it, see `gateway_pg.rs`), so banners appear and go at once.
//! * [`spawn_event_consumer`] feeds the platform's events to the bridge (mirroring, and the backstop that tears a
//!   bridge down when a switch changes by any path); [`spawn_outbox_worker`] drains the outbound queue.
//!
//! The Discord API root is `SR_BRIDGE_DISCORD_API` (default `https://discord.com/api/v10`); the end-to-end
//! suite points it at the fake Discord (`e2e/support/fake-discord.mjs`). Real Discord is never contacted by tests.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::BoxStream;
use sr_bridge::{BridgeError, BridgeNotices, BridgeResult, BridgeService, MediaUrls, TokenSealer};
use sr_core::Clock;
use sr_db::sealed::SecretBox;
use sr_events::DomainEvent;
use sr_storage::cdn::CdnSigner;
use sr_userfeat::{Card, Poster};
use tokio::task::JoinHandle;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

use crate::lag_bus::TapItem;

/// How often the outbound queue is looked at.
const WORKER_TICK: Duration = Duration::from_millis(400);

/// `server.secret_key` sealing for the bridge's tokens.
pub struct PgSealer(pub SecretBox);

impl TokenSealer for PgSealer {
    fn seal(&self, aad: &[u8], plain: &[u8]) -> BridgeResult<Vec<u8>> {
        self.0.seal(aad, plain).map_err(|_| BridgeError::Seal)
    }
    fn open(&self, aad: &[u8], blob: &[u8]) -> BridgeResult<Vec<u8>> {
        self.0.open(aad, blob).map_err(|_| BridgeError::Seal)
    }
}

/// Signed `/cdn` links on the instance's public URL.
pub struct CdnMedia {
    pub signer: CdnSigner,
    pub public_url: String,
    pub clock: Arc<dyn Clock>,
}

impl MediaUrls for CdnMedia {
    fn signed_url(&self, path: &str, ttl_secs: u64) -> String {
        let now = u64::try_from(self.clock.now_unix_ms().div_euclid(1000)).unwrap_or(0);
        format!(
            "{}{}",
            self.public_url.trim_end_matches('/'),
            self.signer.signed_url(path, now + ttl_secs)
        )
    }
}

/// System messages through the feature modules' poster, and channel updates through the message stack's bus.
pub struct PosterNotices {
    pub poster: Arc<dyn Poster>,
    pub messages: Arc<sr_api::routes::messages::MessagesDeps>,
}

#[async_trait]
impl BridgeNotices for PosterNotices {
    async fn system_message(&self, channel: i64, text: &str) {
        let card = Card {
            content: text.to_owned(),
            ..Card::default()
        };
        if let Err(e) = self.poster.post(channel, card).await {
            tracing::warn!(channel, error = %e, "could not post a bridge notice");
        }
    }

    async fn channel_changed(&self, channel: i64) {
        let id = sr_core::ChannelId::new(u64::try_from(channel).unwrap_or(0));
        let repo = &self.messages.repo;
        let Ok(Some(ch)) = repo.get_channel(id).await else {
            return; // A deleted channel has nothing to update.
        };
        let Some(guild) = ch.guild_id else { return };
        let overwrites = repo
            .get_overwrites_of(&[id])
            .await
            .ok()
            .and_then(|mut m| m.remove(&id))
            .unwrap_or_default();
        let payload = serde_json::to_value(sr_api::routes::guilds::dto::ChannelObject::new(
            &ch,
            &overwrites,
        ))
        .unwrap_or(serde_json::Value::Null);
        self.messages
            .publish(DomainEvent::ChannelUpdated {
                guild,
                channel: id,
                payload,
            })
            .await;
    }
}

/// The Discord API root: `SR_BRIDGE_DISCORD_API` or Discord's own.
pub fn discord_api_base() -> String {
    std::env::var("SR_BRIDGE_DISCORD_API")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| sr_bridge::discord::DEFAULT_API_BASE.to_owned())
}

/// Feed the platform's events to the bridge. Switch changes run on their own task (a teardown talks to Discord
/// and may take seconds); messages are handled in order.
pub fn spawn_event_consumer(
    mut events: BoxStream<'static, TapItem>,
    svc: Arc<BridgeService>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(item) = events.next().await {
            match item {
                Ok(ev @ DomainEvent::FeatureSettingsChanged { .. }) => {
                    let svc = svc.clone();
                    tokio::spawn(async move { svc.handle_event(&ev).await });
                }
                Ok(ev) => svc.handle_event(&ev).await,
                Err(BroadcastStreamRecvError::Lagged(missed)) => {
                    tracing::warn!(
                        missed,
                        "discord bridge event consumer lagged; some messages were not mirrored"
                    );
                }
            }
        }
    })
}

/// Drain the outbound queue.
pub fn spawn_outbox_worker(svc: Arc<BridgeService>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(WORKER_TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            if let Err(e) = svc.drain_outbox().await {
                tracing::warn!(error = %e, "discord bridge queue pass failed");
            }
        }
    })
}
