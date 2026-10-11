//! Gateway events raised by the social endpoints.
//!
//! `sr-events` has no per-user event variants yet (it is owned by another change
//! this wave), so handlers describe what happened as a [`SocialEvent`] and
//! [`publish_social`] is the single adapter to the bus.
//!
//! TODO(social-events): once `DomainEvent` gains a user-scoped variant (see the
//! list in the hand-off report: `UserScoped { users, name, payload }`, routed to
//! `Topic::User`), replace the body of [`to_domain`] with that variant and delete
//! the lossy mapping. Until then only the DM channel events ride on the existing
//! `Channel*` variants (published to each recipient's `Topic::User`); every other
//! event is logged and dropped.

use serde_json::Value;
use sr_core::{ChannelId, GuildId, UserId};
use sr_events::{DomainEvent, EventBus, Topic};

/// A Discord gateway dispatch addressed to specific users.
#[derive(Debug, Clone, PartialEq)]
pub struct SocialEvent {
    /// Dispatch name, e.g. `RELATIONSHIP_ADD`.
    pub name: &'static str,
    /// Users whose sessions receive it.
    pub users: Vec<UserId>,
    /// The channel the event concerns, for the DM channel family.
    pub channel: Option<ChannelId>,
    pub payload: Value,
}

impl SocialEvent {
    pub fn new(name: &'static str, users: Vec<UserId>, payload: Value) -> Self {
        Self {
            name,
            users,
            channel: None,
            payload,
        }
    }

    pub fn for_channel(mut self, channel: ChannelId) -> Self {
        self.channel = Some(channel);
        self
    }
}

/// DM events have no guild; `0` is the "no guild" marker on the legacy variants.
const NO_GUILD: GuildId = GuildId::new(0);

/// TODO(social-events): the lossy adapter described in the module docs.
pub fn to_domain(ev: &SocialEvent) -> Option<DomainEvent> {
    let channel = ev.channel?;
    let payload = ev.payload.clone();
    match ev.name {
        "CHANNEL_CREATE" => Some(DomainEvent::ChannelCreatedFull {
            guild: NO_GUILD,
            channel,
            payload,
        }),
        "CHANNEL_UPDATE" => Some(DomainEvent::ChannelUpdated {
            guild: NO_GUILD,
            channel,
            payload,
        }),
        "CHANNEL_DELETE" => Some(DomainEvent::ChannelDeleted {
            guild: NO_GUILD,
            channel,
            payload,
        }),
        _ => None,
    }
}

/// Publish to every addressed user's topic. Best effort: the mutation already
/// happened, so failures are logged.
pub async fn publish_social(bus: &dyn EventBus, ev: SocialEvent) {
    let Some(domain) = to_domain(&ev) else {
        tracing::debug!(
            event = ev.name,
            "social event has no DomainEvent variant yet"
        );
        return;
    };
    for user in &ev.users {
        if let Err(e) = bus.publish(Topic::User(*user), domain.clone()).await {
            tracing::warn!(error = %e, event = ev.name, "failed to publish social event");
        }
    }
}
