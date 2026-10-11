//! Helpers for [`Topic::Firehose`] consumers.
//!
//! The firehose sees an event once per topic it was published to (a guild message
//! goes to its channel and guild topics; a DM goes to each recipient's user
//! topic). [`FirehoseDedup`] collapses those copies: an event equal to one seen
//! within the last [`FirehoseDedup::WINDOW`] is dropped. Two genuinely separate
//! occurrences of the same event differ in their payload (ids, timestamps) or are
//! idempotent for any consumer, so equality is a safe duplicate test.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use futures::StreamExt;
use futures::stream::BoxStream;

use crate::bus::EventBus;
use crate::event::{DomainEvent, Topic};

/// Remembers recently seen events to drop adjacent duplicates.
#[derive(Debug, Default)]
pub struct FirehoseDedup {
    recent: VecDeque<(Instant, DomainEvent)>,
}

impl FirehoseDedup {
    /// How long an event is remembered.
    pub const WINDOW: Duration = Duration::from_secs(2);
    /// Upper bound on remembered events.
    pub const MAX_ENTRIES: usize = 256;

    pub fn new() -> Self {
        Self::default()
    }

    /// True the first time `ev` is seen within the window, false for a repeat.
    pub fn is_new(&mut self, ev: &DomainEvent) -> bool {
        let now = Instant::now();
        while self
            .recent
            .front()
            .is_some_and(|(at, _)| now.duration_since(*at) > Self::WINDOW)
        {
            self.recent.pop_front();
        }
        if self.recent.iter().any(|(_, seen)| seen == ev) {
            return false;
        }
        if self.recent.len() >= Self::MAX_ENTRIES {
            self.recent.pop_front();
        }
        self.recent.push_back((now, ev.clone()));
        true
    }

    /// Subscribe to the firehose of `bus` with duplicates removed.
    pub fn subscribe(bus: &dyn EventBus) -> BoxStream<'static, DomainEvent> {
        let mut dedup = Self::new();
        bus.subscribe(Topic::Firehose)
            .filter(move |ev| futures::future::ready(dedup.is_new(ev)))
            .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::InProcessBus;
    use crate::ids::{ChannelId, GuildId, UserId};
    use std::time::Duration;
    use tokio::time::timeout;

    fn sent(id: u64) -> DomainEvent {
        DomainEvent::MessageSent {
            guild: Some(GuildId::new(1)),
            channel: ChannelId::new(2),
            message_id: id,
            author: UserId::new(3),
            data: serde_json::Value::Null,
        }
    }

    #[test]
    fn repeats_are_dropped_distinct_events_pass() {
        let mut d = FirehoseDedup::new();
        assert!(d.is_new(&sent(1)));
        assert!(!d.is_new(&sent(1)));
        assert!(d.is_new(&sent(2)));
    }

    #[tokio::test]
    async fn guild_message_reaches_the_firehose_once() {
        let bus = InProcessBus::new();
        let mut s = FirehoseDedup::subscribe(&bus);
        let ev = sent(7);
        for t in ev.topics() {
            bus.publish(t, ev.clone()).await.unwrap();
        }
        assert_eq!(
            timeout(Duration::from_secs(2), s.next()).await,
            Ok(Some(ev))
        );
        assert!(
            timeout(Duration::from_millis(100), s.next()).await.is_err(),
            "duplicate delivered"
        );
    }
}
