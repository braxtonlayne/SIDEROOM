//! Gateway events raised by the social endpoints.
//!
//! `sr-events` has no per-user event variants yet (it is owned by another change
//! this wave), so handlers describe what happened as a [`SocialEvent`] and
//! [`publish_social`] is the single adapter to the bus.
//!
//! Every social dispatch is a [`DomainEvent::UserScoped`] published to each addressed
//! user's topic.

use serde_json::{Value, json};
use sr_chat::social::presence::PresencePrefs;
use sr_core::{ChannelId, UserId};
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

/// Map a social event onto the bus: every social dispatch is a self-scoped
/// [`DomainEvent::UserScoped`] (DM channel events included, since they have no guild).
pub fn to_domain(ev: &SocialEvent) -> Option<DomainEvent> {
    Some(DomainEvent::UserScoped {
        users: ev.users.clone(),
        name: ev.name.to_owned(),
        payload: ev.payload.clone(),
    })
}

/// The chosen presence as a [`DomainEvent::PresenceChanged`]; the custom status
/// travels as a Discord custom-status activity (`type` 4).
pub fn presence_changed(prefs: &PresencePrefs) -> DomainEvent {
    let custom = &prefs.custom;
    let custom_status = (!custom.is_empty()).then(|| {
        let emoji = custom
            .emoji_name
            .as_ref()
            .map(|name| json!({ "name": name, "id": custom.emoji_id.map(|i| i.to_string()) }));
        json!({
            "type": 4,
            "name": "Custom Status",
            "state": custom.text,
            "emoji": emoji,
        })
    });
    DomainEvent::PresenceChanged {
        user: prefs.user,
        status: prefs.status.as_str().to_owned(),
        custom_status,
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
