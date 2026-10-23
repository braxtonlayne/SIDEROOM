//! A channel that platform staff marked (or unmarked) age-restricted (MOD-AGE-001): live clients get a
//! `CHANNEL_UPDATE` with the full channel object, and the server's own audit log gets a channel-update row naming
//! the `nsfw` change, so its owners can see who did it. [`sr_moderation::ChannelSink`] over the guild backend, the
//! event bus and the guild audit writer.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use sqlx::PgPool;
use sr_api::routes::guilds::GuildBackend;
use sr_api::routes::guilds::backend::AuditSink;
use sr_api::routes::guilds::dto::ChannelObject;
use sr_chat::settings::audit::{AuditAction, AuditChange};
use sr_core::{ChannelId, GuildId, UserId};
use sr_events::{DomainEvent, EventBus};
use sr_moderation::ChannelSink;

pub struct BusChannelSink {
    pub guilds: Arc<dyn GuildBackend>,
    pub bus: Arc<dyn EventBus>,
    pub audit: Arc<dyn AuditSink>,
    pub pool: PgPool,
}

#[async_trait]
impl ChannelSink for BusChannelSink {
    async fn channel_changed(&self, guild: i64, channel: i64, actor: i64, nsfw: bool) {
        let (Ok(g), Ok(c), Ok(a)) = (
            u64::try_from(guild),
            u64::try_from(channel),
            u64::try_from(actor),
        ) else {
            return;
        };
        let (guild_id, channel_id, actor_id) = (GuildId::new(g), ChannelId::new(c), UserId::new(a));
        self.audit
            .record(
                guild_id,
                actor_id,
                AuditAction::ChannelUpdate,
                Some(c),
                vec![AuditChange::new(
                    "nsfw",
                    Some(json!(!nsfw)),
                    Some(json!(nsfw)),
                )],
                Some("Platform staff: age restriction"),
            )
            .await;
        // The owner sees every channel, so the full object can be built through the guild backend.
        let owner: Option<i64> = sqlx::query_scalar("SELECT owner_id FROM guilds WHERE id = $1")
            .bind(guild)
            .fetch_optional(&self.pool)
            .await
            .ok()
            .flatten();
        let Some(owner) = owner.and_then(|o| u64::try_from(o).ok()) else {
            return;
        };
        let view = match self
            .guilds
            .get_channel(UserId::new(owner), channel_id)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "could not load a channel to announce its age restriction");
                return;
            }
        };
        let ev = DomainEvent::ChannelUpdated {
            guild: guild_id,
            channel: channel_id,
            payload: serde_json::to_value(ChannelObject::from(&view)).unwrap_or_default(),
        };
        for topic in ev.topics() {
            if let Err(e) = self.bus.publish(topic, ev.clone()).await {
                tracing::warn!(error = %e, "publishing a channel update failed");
            }
        }
    }
}
