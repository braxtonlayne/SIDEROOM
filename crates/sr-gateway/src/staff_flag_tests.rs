//! D-673 (SEC-AUD-205): `user.sr_platform_staff` on the account's own READY, and nowhere else.

use std::sync::Arc;

use serde_json::{Value, json};
use sr_core::{ChannelId, GuildId, UserId};
use sr_events::{DomainEvent, EventBus, InProcessBus, Topic};

use crate::test_support::TestWorld;
use crate::{GatewayConfig, intents};

const G: GuildId = GuildId::new(1);

fn u(n: u64) -> UserId {
    UserId::new(n)
}

#[tokio::test]
async fn a_staff_accounts_ready_says_so() {
    let w = TestWorld::new(GatewayConfig::default());
    w.set_staff(u(7), true);
    let mut c = w.connect();
    let ready = c.identify("user:7:0:a", intents::GUILDS, 0).await;
    assert_eq!(
        ready["d"]["user"]["sr_platform_staff"],
        json!(true),
        "{ready}"
    );
    assert_eq!(ready["d"]["user"]["id"], "7");
}

/// The member's READY user object is exactly what the authenticator built, key for key: the field is absent, not
/// `false`. A bot with a staff appointment (it cannot hold one, but the lookup must not matter) gets nothing.
#[tokio::test]
async fn a_members_ready_user_is_unchanged() {
    let w = TestWorld::new(GatewayConfig::default());
    w.set_staff(u(9), true);
    let mut c = w.connect();
    let ready = c.identify("user:8:0:a", intents::GUILDS, 0).await;
    assert_eq!(ready["d"]["user"], json!({ "id": "8" }), "{ready}");
    assert!(ready["d"]["user"].get("sr_platform_staff").is_none());
    let mut bot = w.connect();
    let ready = bot.identify("bot:9:0:a", intents::GUILDS, 0).await;
    assert_eq!(ready["d"]["user"], json!({ "id": "9" }), "{ready}");
}

#[tokio::test]
async fn a_revoked_appointment_is_gone_at_the_next_session() {
    let w = TestWorld::new(GatewayConfig::default());
    w.set_staff(u(7), true);
    let mut first = w.connect();
    let ready = first.identify("user:7:0:a", intents::GUILDS, 0).await;
    assert_eq!(ready["d"]["user"]["sr_platform_staff"], json!(true));
    w.set_staff(u(7), false);
    let mut next = w.connect();
    let ready = next.identify("user:7:0:b", intents::GUILDS, 0).await;
    assert_eq!(ready["d"]["user"], json!({ "id": "7" }), "{ready}");
}

/// A resync (a lagged user topic) asks again: a revocation in between shows in the fresh READY.
#[tokio::test]
async fn a_resync_reads_the_flag_afresh() {
    let w = TestWorld::with_bus(
        GatewayConfig::default(),
        Arc::new(InProcessBus::with_capacity(4)),
    );
    w.set_staff(u(7), true);
    let mut c = w.connect();
    let first = c.identify("user:7:0:a", intents::GUILDS, 0).await;
    assert_eq!(first["d"]["user"]["sr_platform_staff"], json!(true));
    c.drain().await;
    w.set_staff(u(7), false);
    for n in 0..12 {
        w.bus
            .publish(
                Topic::User(u(7)),
                DomainEvent::GuildDeleted {
                    guild: GuildId::new(500 + n),
                },
            )
            .await
            .unwrap();
    }
    let ready = c.await_dispatch("READY").await;
    assert_eq!(ready["d"]["user"], json!({ "id": "7" }), "{ready}");
}

/// No other payload carries the flag: a staff account and a member in one guild, a message, and everything the
/// two receive after READY.
#[tokio::test]
async fn no_other_payload_carries_the_flag() {
    let w = TestWorld::new(GatewayConfig::default());
    w.add_member(u(7), G);
    w.add_member(u(8), G);
    w.set_staff(u(7), true);
    let mut clients = w
        .connect_users(&[7, 8], intents::GUILDS | intents::GUILD_MESSAGES)
        .await;
    for c in &mut clients {
        c.drain().await;
    }
    w.emit(DomainEvent::MessageSent {
        guild: Some(G),
        channel: ChannelId::new(10),
        message_id: 100,
        author: u(7),
        data: json!({
            "id": "100", "channel_id": "10", "guild_id": "1",
            "author": { "id": "7" }, "content": "hi", "mentions": [],
        }),
    })
    .await;
    for c in &mut clients {
        let seen: Value = c.await_dispatch("MESSAGE_CREATE").await;
        assert!(!seen.to_string().contains("sr_platform_staff"), "{seen}");
        c.assert_silent().await;
    }
}
