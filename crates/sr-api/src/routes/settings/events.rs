//! Gateway-facing events of the settings endpoints.
//!
//! `sr-events` and `sr-gateway` belong to the backend-gaps wave, so the handlers
//! publish through the small [`SettingsEvents`] trait instead of naming
//! `DomainEvent` variants that do not exist yet. [`TodoBusAdapter`] is the
//! placeholder wiring: it forwards nothing today and logs each event it drops.
//!
//! `DomainEvent` variants this module needs (payloads are Discord-shaped JSON):
//!
//! | `SettingsEvent` | gateway dispatch |
//! |---|---|
//! | `InviteCreated` / `InviteDeleted` | `INVITE_CREATE` / `INVITE_DELETE` |
//! | `EmojisUpdated` | `GUILD_EMOJIS_UPDATE` |
//! | `StickersUpdated` | `GUILD_STICKERS_UPDATE` |
//! | `WebhooksUpdated` | `WEBHOOKS_UPDATE` |
//! | `AutoModRuleCreated/Updated/Deleted` | `AUTO_MODERATION_RULE_CREATE/UPDATE/DELETE` |
//! | `AutoModActionExecuted` | `AUTO_MODERATION_ACTION_EXECUTION` |
//! | `AuditLogEntryCreated` | `GUILD_AUDIT_LOG_ENTRY_CREATE` |
//! | `CommunityChanged` | `GUILD_UPDATE` (existing `GuildUpdated` fits) |

use async_trait::async_trait;
use serde_json::Value;
use sr_core::{ChannelId, GuildId};
use sr_events::{DomainEvent, EventBus};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub enum SettingsEvent {
    InviteCreated {
        guild: GuildId,
        channel: ChannelId,
        invite: Value,
    },
    InviteDeleted {
        guild: GuildId,
        channel: ChannelId,
        code: String,
    },
    /// `payload` is `{guild_id, emojis: [...]}`.
    EmojisUpdated {
        guild: GuildId,
        payload: Value,
    },
    /// `payload` is `{guild_id, stickers: [...]}`.
    StickersUpdated {
        guild: GuildId,
        payload: Value,
    },
    WebhooksUpdated {
        guild: GuildId,
        channel: ChannelId,
    },
    AutoModRuleCreated {
        guild: GuildId,
        rule: Value,
    },
    AutoModRuleUpdated {
        guild: GuildId,
        rule: Value,
    },
    AutoModRuleDeleted {
        guild: GuildId,
        rule: Value,
    },
    AutoModActionExecuted {
        guild: GuildId,
        payload: Value,
    },
    AuditLogEntryCreated {
        guild: GuildId,
        entry: Value,
    },
    /// Community, onboarding, welcome-screen or safety flags changed.
    CommunityChanged {
        guild: GuildId,
    },
}

impl SettingsEvent {
    pub fn guild(&self) -> GuildId {
        match self {
            Self::InviteCreated { guild, .. }
            | Self::InviteDeleted { guild, .. }
            | Self::EmojisUpdated { guild, .. }
            | Self::StickersUpdated { guild, .. }
            | Self::WebhooksUpdated { guild, .. }
            | Self::AutoModRuleCreated { guild, .. }
            | Self::AutoModRuleUpdated { guild, .. }
            | Self::AutoModRuleDeleted { guild, .. }
            | Self::AutoModActionExecuted { guild, .. }
            | Self::AuditLogEntryCreated { guild, .. }
            | Self::CommunityChanged { guild } => *guild,
        }
    }

    pub const fn name(&self) -> &'static str {
        match self {
            Self::InviteCreated { .. } => "INVITE_CREATE",
            Self::InviteDeleted { .. } => "INVITE_DELETE",
            Self::EmojisUpdated { .. } => "GUILD_EMOJIS_UPDATE",
            Self::StickersUpdated { .. } => "GUILD_STICKERS_UPDATE",
            Self::WebhooksUpdated { .. } => "WEBHOOKS_UPDATE",
            Self::AutoModRuleCreated { .. } => "AUTO_MODERATION_RULE_CREATE",
            Self::AutoModRuleUpdated { .. } => "AUTO_MODERATION_RULE_UPDATE",
            Self::AutoModRuleDeleted { .. } => "AUTO_MODERATION_RULE_DELETE",
            Self::AutoModActionExecuted { .. } => "AUTO_MODERATION_ACTION_EXECUTION",
            Self::AuditLogEntryCreated { .. } => "GUILD_AUDIT_LOG_ENTRY_CREATE",
            Self::CommunityChanged { .. } => "GUILD_UPDATE",
        }
    }
}

/// Best-effort sink: the mutation already happened, so a failure is logged,
/// never surfaced.
#[async_trait]
pub trait SettingsEvents: Send + Sync {
    async fn publish(&self, ev: SettingsEvent);
}

/// Drops every event.
pub struct NoopSettingsEvents;

#[async_trait]
impl SettingsEvents for NoopSettingsEvents {
    async fn publish(&self, _ev: SettingsEvent) {}
}

/// TODO(events): swap for a real mapping once `sr-events` has the variants listed
/// in the module docs. Until then only `CommunityChanged` is expressible, and it
/// needs the full guild object for `GuildUpdated`, which this layer does not
/// have, so nothing is forwarded.
pub struct TodoBusAdapter {
    #[allow(dead_code)]
    bus: Arc<dyn EventBus>,
}

impl TodoBusAdapter {
    pub fn new(bus: Arc<dyn EventBus>) -> Self {
        Self { bus }
    }
}

#[async_trait]
impl SettingsEvents for TodoBusAdapter {
    async fn publish(&self, ev: SettingsEvent) {
        // Keeps `DomainEvent` referenced so the adapter is updated with the enum.
        let _: Option<DomainEvent> = None;
        tracing::debug!(
            event = ev.name(),
            guild = ev.guild().get(),
            "settings event has no DomainEvent variant yet; dropped"
        );
    }
}

/// Collects events for assertions in tests.
#[derive(Default)]
pub struct RecordingEvents(pub std::sync::Mutex<Vec<SettingsEvent>>);

#[async_trait]
impl SettingsEvents for RecordingEvents {
    async fn publish(&self, ev: SettingsEvent) {
        if let Ok(mut v) = self.0.lock() {
            v.push(ev);
        }
    }
}

impl RecordingEvents {
    pub fn names(&self) -> Vec<&'static str> {
        self.0
            .lock()
            .map(|v| v.iter().map(SettingsEvent::name).collect())
            .unwrap_or_default()
    }
}
