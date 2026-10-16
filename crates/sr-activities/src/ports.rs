//! Collaborators of the Activities service: channel permissions and user-scoped gateway events.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use sr_core::UserId;

/// An activity instance started, changed (participants) or ended. Sent to everybody who can see the channel.
pub const EVENT_INSTANCE_UPDATE: &str = "SR_ACTIVITY_INSTANCE_UPDATE";

/// What an account may do in a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelPerms {
    pub guild_id: i64,
    pub voice: bool,
    /// `VIEW_CHANNEL`.
    pub view: bool,
    /// `CONNECT`.
    pub connect: bool,
    /// `USE_EMBEDDED_ACTIVITIES`.
    pub use_activities: bool,
    /// `MANAGE_CHANNELS` or administrator.
    pub manage: bool,
}

/// Channel permissions, computed from guild roles and overwrites by the chat system.
#[async_trait]
pub trait ChannelAccess: Send + Sync {
    /// `None` when the channel does not exist, is not a guild channel, or the account cannot see it.
    async fn perms(&self, user: i64, channel: i64) -> Result<Option<ChannelPerms>, String>;
    /// Everybody who can see `channel` (capped by the implementation).
    async fn viewers(&self, channel: i64) -> Result<Vec<i64>, String>;
}

/// No channels: everything is refused (tests, installs without activities).
pub struct NoChannels;

#[async_trait]
impl ChannelAccess for NoChannels {
    async fn perms(&self, _user: i64, _channel: i64) -> Result<Option<ChannelPerms>, String> {
        Ok(None)
    }
    async fn viewers(&self, _channel: i64) -> Result<Vec<i64>, String> {
        Ok(Vec::new())
    }
}

/// Delivers user-scoped gateway events.
#[async_trait]
pub trait ActivityEvents: Send + Sync {
    async fn emit(&self, users: Vec<i64>, name: &'static str, payload: Value);
}

/// Drops every event.
pub struct NoEvents;

#[async_trait]
impl ActivityEvents for NoEvents {
    async fn emit(&self, _users: Vec<i64>, _name: &'static str, _payload: Value) {}
}

/// Publishes as `DomainEvent::UserScoped` on each addressed user's topic.
pub struct BusEvents {
    bus: Arc<dyn sr_events::EventBus>,
}

impl BusEvents {
    pub fn new(bus: Arc<dyn sr_events::EventBus>) -> Self {
        Self { bus }
    }
}

#[async_trait]
impl ActivityEvents for BusEvents {
    async fn emit(&self, users: Vec<i64>, name: &'static str, payload: Value) {
        let users: Vec<UserId> = users
            .into_iter()
            .filter_map(|u| u64::try_from(u).ok().map(UserId::new))
            .collect();
        let ev = sr_events::DomainEvent::UserScoped {
            users: users.clone(),
            name: name.to_owned(),
            payload,
        };
        for u in users {
            if let Err(e) = self
                .bus
                .publish(sr_events::Topic::User(u), ev.clone())
                .await
            {
                tracing::warn!(error = %e, event = name, "publishing an activity event failed");
            }
        }
    }
}
