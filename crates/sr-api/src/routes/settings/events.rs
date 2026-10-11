//! Gateway-facing events of the settings endpoints.
//!
//! The handlers publish through the small [`SettingsEvents`] trait so they need
//! not know about the bus; [`BusSettingsEvents`] maps each [`SettingsEvent`] onto
//! its `DomainEvent` and publishes it to the event's topics (payloads are
//! Discord-shaped JSON).
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
//! | `CommunityChanged` | `GUILD_UPDATE` (full guild object) |

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
    /// Community, onboarding, welcome-screen or safety flags changed; `payload` is
    /// the full guild object (the `GUILD_UPDATE` body).
    CommunityChanged {
        guild: GuildId,
        payload: Value,
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
            | Self::CommunityChanged { guild, .. } => *guild,
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

impl SettingsEvent {
    /// The bus event this settings event becomes.
    pub fn into_domain(self) -> DomainEvent {
        match self {
            Self::InviteCreated {
                guild,
                channel,
                invite,
            } => DomainEvent::InviteCreated {
                guild,
                channel,
                payload: invite,
            },
            Self::InviteDeleted {
                guild,
                channel,
                code,
            } => DomainEvent::InviteDeleted {
                guild,
                channel,
                payload: serde_json::json!({
                    "channel_id": channel.to_string(),
                    "guild_id": guild.to_string(),
                    "code": code,
                }),
            },
            Self::EmojisUpdated { guild, payload } => DomainEvent::EmojisUpdated { guild, payload },
            Self::StickersUpdated { guild, payload } => {
                DomainEvent::StickersUpdated { guild, payload }
            }
            Self::WebhooksUpdated { guild, channel } => {
                DomainEvent::WebhooksUpdated { guild, channel }
            }
            Self::AutoModRuleCreated { guild, rule } => DomainEvent::AutoModRuleCreated {
                guild,
                payload: rule,
            },
            Self::AutoModRuleUpdated { guild, rule } => DomainEvent::AutoModRuleUpdated {
                guild,
                payload: rule,
            },
            Self::AutoModRuleDeleted { guild, rule } => DomainEvent::AutoModRuleDeleted {
                guild,
                payload: rule,
            },
            Self::AutoModActionExecuted { guild, payload } => {
                DomainEvent::AutoModActionExecuted { guild, payload }
            }
            Self::AuditLogEntryCreated { guild, entry } => DomainEvent::AuditLogEntryCreated {
                guild,
                payload: entry,
            },
            Self::CommunityChanged { guild, payload } => {
                DomainEvent::GuildUpdated { guild, payload }
            }
        }
    }
}

/// Publishes settings events on the bus. Best effort: the mutation is committed,
/// so a bus failure is logged, never surfaced.
pub struct BusSettingsEvents {
    bus: Arc<dyn EventBus>,
}

impl BusSettingsEvents {
    pub fn new(bus: Arc<dyn EventBus>) -> Self {
        Self { bus }
    }
}

#[async_trait]
impl SettingsEvents for BusSettingsEvents {
    async fn publish(&self, ev: SettingsEvent) {
        let name = ev.name();
        let domain = ev.into_domain();
        for topic in domain.topics() {
            if let Err(e) = self.bus.publish(topic, domain.clone()).await {
                tracing::warn!(error = %e, event = name, "failed to publish settings event");
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use serde_json::json;
    use sr_events::{InProcessBus, Topic};

    const G: GuildId = GuildId::new(1);
    const C: ChannelId = ChannelId::new(2);

    #[test]
    fn every_settings_event_maps_to_its_domain_event() {
        let p = json!({ "x": 1 });
        let cases: Vec<(SettingsEvent, DomainEvent)> = vec![
            (
                SettingsEvent::InviteCreated {
                    guild: G,
                    channel: C,
                    invite: p.clone(),
                },
                DomainEvent::InviteCreated {
                    guild: G,
                    channel: C,
                    payload: p.clone(),
                },
            ),
            (
                SettingsEvent::InviteDeleted {
                    guild: G,
                    channel: C,
                    code: "abc".into(),
                },
                DomainEvent::InviteDeleted {
                    guild: G,
                    channel: C,
                    payload: json!({ "channel_id": "2", "guild_id": "1", "code": "abc" }),
                },
            ),
            (
                SettingsEvent::EmojisUpdated {
                    guild: G,
                    payload: p.clone(),
                },
                DomainEvent::EmojisUpdated {
                    guild: G,
                    payload: p.clone(),
                },
            ),
            (
                SettingsEvent::StickersUpdated {
                    guild: G,
                    payload: p.clone(),
                },
                DomainEvent::StickersUpdated {
                    guild: G,
                    payload: p.clone(),
                },
            ),
            (
                SettingsEvent::WebhooksUpdated {
                    guild: G,
                    channel: C,
                },
                DomainEvent::WebhooksUpdated {
                    guild: G,
                    channel: C,
                },
            ),
            (
                SettingsEvent::AutoModRuleCreated {
                    guild: G,
                    rule: p.clone(),
                },
                DomainEvent::AutoModRuleCreated {
                    guild: G,
                    payload: p.clone(),
                },
            ),
            (
                SettingsEvent::AutoModRuleUpdated {
                    guild: G,
                    rule: p.clone(),
                },
                DomainEvent::AutoModRuleUpdated {
                    guild: G,
                    payload: p.clone(),
                },
            ),
            (
                SettingsEvent::AutoModRuleDeleted {
                    guild: G,
                    rule: p.clone(),
                },
                DomainEvent::AutoModRuleDeleted {
                    guild: G,
                    payload: p.clone(),
                },
            ),
            (
                SettingsEvent::AutoModActionExecuted {
                    guild: G,
                    payload: p.clone(),
                },
                DomainEvent::AutoModActionExecuted {
                    guild: G,
                    payload: p.clone(),
                },
            ),
            (
                SettingsEvent::AuditLogEntryCreated {
                    guild: G,
                    entry: p.clone(),
                },
                DomainEvent::AuditLogEntryCreated {
                    guild: G,
                    payload: p.clone(),
                },
            ),
            (
                SettingsEvent::CommunityChanged {
                    guild: G,
                    payload: p.clone(),
                },
                DomainEvent::GuildUpdated {
                    guild: G,
                    payload: p,
                },
            ),
        ];
        for (settings, domain) in cases {
            assert_eq!(settings.into_domain(), domain);
        }
    }

    #[tokio::test]
    async fn the_bus_adapter_publishes_to_the_guild_topic() {
        let bus = Arc::new(InProcessBus::new());
        let mut guild = bus.subscribe(Topic::Guild(G));
        BusSettingsEvents::new(bus)
            .publish(SettingsEvent::WebhooksUpdated {
                guild: G,
                channel: C,
            })
            .await;
        let got = tokio::time::timeout(std::time::Duration::from_secs(1), guild.next())
            .await
            .unwrap();
        assert_eq!(
            got,
            Some(DomainEvent::WebhooksUpdated {
                guild: G,
                channel: C
            })
        );
    }
}
