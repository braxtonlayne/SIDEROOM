//! Reusable `EventBus` contract tests. Call [`run_all`] from an implementation's
//! test module with a factory that creates a fresh bus.

use std::time::Duration;

use futures::StreamExt;
use tokio::time::timeout;

use crate::bus::EventBus;
use crate::event::{DomainEvent, Topic};
use crate::ids::{GuildId, UserId};

const WAIT: Duration = Duration::from_secs(2);
const QUIET: Duration = Duration::from_millis(100);

fn ev(n: u64) -> DomainEvent {
    DomainEvent::MemberJoined {
        guild: GuildId::new(n),
        user: UserId::new(n),
        payload: serde_json::Value::Null,
    }
}

/// Run every contract test against buses produced by `make`.
pub async fn run_all<B: EventBus>(make: impl Fn() -> B) {
    publish_without_subscribers_ok(&make()).await;
    subscriber_receives_in_order(&make()).await;
    fan_out_to_all_subscribers(&make()).await;
    topics_are_isolated(&make()).await;
    late_subscriber_misses_earlier_events(&make()).await;
    dropped_subscriber_does_not_break_others(&make()).await;
}

pub async fn publish_without_subscribers_ok<B: EventBus>(bus: &B) {
    assert!(bus.publish(Topic::Platform, ev(1)).await.is_ok());
}

pub async fn subscriber_receives_in_order<B: EventBus>(bus: &B) {
    let mut s = bus.subscribe(Topic::Platform);
    for i in 0..5 {
        bus.publish(Topic::Platform, ev(i)).await.unwrap();
    }
    for i in 0..5 {
        let got = timeout(WAIT, s.next()).await.expect("timed out");
        assert_eq!(got, Some(ev(i)));
    }
}

pub async fn fan_out_to_all_subscribers<B: EventBus>(bus: &B) {
    let mut a = bus.subscribe(Topic::Platform);
    let mut b = bus.subscribe(Topic::Platform);
    bus.publish(Topic::Platform, ev(7)).await.unwrap();
    assert_eq!(timeout(WAIT, a.next()).await.unwrap(), Some(ev(7)));
    assert_eq!(timeout(WAIT, b.next()).await.unwrap(), Some(ev(7)));
}

pub async fn topics_are_isolated<B: EventBus>(bus: &B) {
    let mut g1 = bus.subscribe(Topic::Guild(GuildId::new(1)));
    let mut g2 = bus.subscribe(Topic::Guild(GuildId::new(2)));
    bus.publish(Topic::Guild(GuildId::new(1)), ev(1))
        .await
        .unwrap();
    assert_eq!(timeout(WAIT, g1.next()).await.unwrap(), Some(ev(1)));
    assert!(
        timeout(QUIET, g2.next()).await.is_err(),
        "leaked across topics"
    );
}

pub async fn late_subscriber_misses_earlier_events<B: EventBus>(bus: &B) {
    let mut early = bus.subscribe(Topic::Platform);
    bus.publish(Topic::Platform, ev(1)).await.unwrap();
    let mut late = bus.subscribe(Topic::Platform);
    bus.publish(Topic::Platform, ev(2)).await.unwrap();
    assert_eq!(timeout(WAIT, early.next()).await.unwrap(), Some(ev(1)));
    assert_eq!(timeout(WAIT, late.next()).await.unwrap(), Some(ev(2)));
}

pub async fn dropped_subscriber_does_not_break_others<B: EventBus>(bus: &B) {
    let dropped = bus.subscribe(Topic::Platform);
    let mut kept = bus.subscribe(Topic::Platform);
    drop(dropped);
    bus.publish(Topic::Platform, ev(3)).await.unwrap();
    assert_eq!(timeout(WAIT, kept.next()).await.unwrap(), Some(ev(3)));
}
