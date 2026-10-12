//! An [`EventBus`] wrapper that lets one consumer see when it fell behind.
//!
//! [`InProcessBus`](sr_events::InProcessBus) silently skips events a slow
//! subscriber missed. The search indexer must know: a missed message event means
//! a stale index until the next `search.reindex`. [`LagTapBus`] forwards every
//! call to the inner bus and additionally copies each published event into a
//! private tokio broadcast channel; [`LagTapBus::tap`] hands out a receiver
//! stream that yields `Err(Lagged(n))` when that channel overflowed. The tap
//! carries the same copies as `Topic::Firehose` (one per topic published to).

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::BoxStream;
use sr_events::{DomainEvent, EventBus, Topic};
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_stream::wrappers::{BroadcastStream, errors::BroadcastStreamRecvError};

/// One item of the tap: an event, or the number of events that were missed.
pub type TapItem = Result<DomainEvent, BroadcastStreamRecvError>;

pub struct LagTapBus {
    inner: Arc<dyn EventBus>,
    tap: broadcast::Sender<DomainEvent>,
}

impl LagTapBus {
    /// Buffer of the tap, as large as the in-process firehose buffer.
    pub const TAP_CAPACITY: usize = 4096;

    pub fn new(inner: Arc<dyn EventBus>) -> Self {
        Self::with_capacity(inner, Self::TAP_CAPACITY)
    }

    pub fn with_capacity(inner: Arc<dyn EventBus>, capacity: usize) -> Self {
        Self {
            inner,
            tap: broadcast::channel(capacity.max(1)).0,
        }
    }

    /// Every event published from now on, with lag reported as `Err`.
    pub fn tap(&self) -> BoxStream<'static, TapItem> {
        BroadcastStream::new(self.tap.subscribe()).boxed()
    }
}

#[async_trait]
impl EventBus for LagTapBus {
    async fn publish(&self, topic: Topic, event: DomainEvent) -> Result<(), sr_events::EventError> {
        if self.tap.receiver_count() > 0 {
            // Err only means no receivers.
            let _ = self.tap.send(event.clone());
        }
        self.inner.publish(topic, event).await
    }

    fn subscribe(&self, topic: Topic) -> BoxStream<'static, DomainEvent> {
        self.inner.subscribe(topic)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_core::GuildId;
    use sr_events::InProcessBus;

    fn ev(n: u64) -> DomainEvent {
        DomainEvent::GuildDeleted {
            guild: GuildId::new(n),
        }
    }

    #[tokio::test]
    async fn tap_reports_lag_and_inner_bus_still_works() {
        let bus = LagTapBus::with_capacity(Arc::new(InProcessBus::new()), 2);
        let mut inner_sub = bus.subscribe(Topic::Firehose);
        let mut tap = bus.tap();
        for n in 1..=5 {
            bus.publish(Topic::Platform, ev(n)).await.unwrap();
        }
        // The tap overflowed: it first reports the gap, then the newest events.
        assert!(matches!(
            tap.next().await,
            Some(Err(BroadcastStreamRecvError::Lagged(3)))
        ));
        assert_eq!(tap.next().await.unwrap().unwrap(), ev(4));
        assert_eq!(tap.next().await.unwrap().unwrap(), ev(5));
        // The wrapped bus is untouched.
        assert_eq!(inner_sub.next().await.unwrap(), ev(1));
    }
}
