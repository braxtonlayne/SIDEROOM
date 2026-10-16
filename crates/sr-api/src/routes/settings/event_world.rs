//! What scheduled events need from the rest of the platform (G-91, G-93): the voice side
//! (who is in a channel, stage instances) and the start notification.
//!
//! The composition root registers the real ones with [`super::SettingsDeps::with_event_world`]
//! and [`super::SettingsDeps::with_event_notifier`]; without them events still work, but no
//! stage instance is opened, nothing ends for want of listeners and nobody is notified.

use async_trait::async_trait;
use sr_chat::settings::scheduled_events::ScheduledEvent;
use sr_core::UserId;

pub use sr_chat::settings::scheduled_events::{EventWorld, NoWorld, VoiceView};

/// Tells the members interested in an event that it has started (gateway dispatch and Web
/// Push; the real one honours D-191 and each member's mute settings).
#[async_trait]
pub trait EventStartNotifier: Send + Sync {
    async fn event_started(&self, event: &ScheduledEvent, interested: &[UserId]);
}

/// Notifies nobody; the default until the composition root wires a real one.
pub struct NoNotifier;

#[async_trait]
impl EventStartNotifier for NoNotifier {
    async fn event_started(&self, _event: &ScheduledEvent, _interested: &[UserId]) {}
}
