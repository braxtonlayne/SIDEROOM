//! RDY-PERF-004: what a gateway session and a guild subscription cost in memory, and a budget that fails when
//! the per-session figure doubles.
//!
//! The numbers are live heap bytes counted by [`sr_memcount`] (a test-only counting allocator, per thread), taken
//! on a current-thread Tokio runtime so every gateway task runs on the test thread. They include the in-process
//! [`TestClient`] side of each connection (its channels and the queued frames), which is the same shape of data
//! the server holds per socket; they do not include the kernel's socket buffers or the WebSocket framing, which
//! the release-build run measures as resident memory instead (docs/dev/load-testing.md).
//!
//! The budgets below are 1.5 times the figure measured when this test was written (Windows, debug build; heap
//! sizes do not depend on the profile). Doubling a per-session cost trips them; ordinary noise does not. When a
//! change legitimately costs more, re-measure (`cargo test -p sr-gateway memory_budget -- --nocapture`), write
//! the new figures into docs/dev/load-testing.md and raise the constants in the same commit.

use serde_json::Value;
use sr_core::{GuildId, UserId};
use sr_memcount::live_bytes;

use crate::test_support::{TestClient, TestWorld};
use crate::{GatewayConfig, intents};

#[global_allocator]
static ALLOC: sr_memcount::Counting = sr_memcount::Counting;

/// Budget for the heap one session holds with one guild (bytes).
const PER_SESSION_BUDGET: isize = 11_200; // measured 7,428
/// Budget for each further guild a session subscribes to (bytes).
const PER_GUILD_SUBSCRIPTION_BUDGET: isize = 21_500; // measured 14,318
/// Budget for the READY frame, and for one GUILD_CREATE of a guild with no channels in this directory (bytes).
const READY_BUDGET: usize = 5_400; // measured 3,549
const GUILD_CREATE_BUDGET: usize = 200; // measured 113

const SESSIONS: u64 = 100;

fn guild(i: u64) -> GuildId {
    GuildId::new(1000 + i)
}

/// A world with `SESSIONS` users, each a member of `guilds` guilds, all connected. Returns the live bytes the
/// connected world holds (measured against `before`, taken before it was built) with the clients kept alive.
async fn connected(guilds: u64) -> (isize, TestWorld, Vec<TestClient>) {
    let before = live_bytes();
    let world = TestWorld::new(GatewayConfig::default());
    for u in 1..=SESSIONS {
        for g in 0..guilds {
            world.add_member(UserId::new(u), guild(g));
        }
    }
    let ids: Vec<u64> = (1..=SESSIONS).collect();
    let clients = world.connect_users(&ids, intents::GUILDS).await;
    (live_bytes() - before, world, clients)
}

#[tokio::test]
async fn a_session_and_a_guild_subscription_stay_within_budget() {
    // Warm up: first-use allocations (statics, lazily built tables) must not be charged to a session.
    drop(connected(1).await);

    let (one, world1, clients1) = connected(1).await;
    let per_session = one / SESSIONS as isize;
    drop(clients1);
    drop(world1);

    let (twenty, world20, clients20) = connected(20).await;
    let per_subscription = (twenty - one) / (SESSIONS as isize * 19);
    drop(clients20);
    drop(world20);

    eprintln!(
        "RDY-PERF-004: {per_session} bytes per session (one guild), {per_subscription} bytes per further guild subscription"
    );
    assert!(
        per_session > 0 && per_subscription > 0,
        "{per_session} {per_subscription}"
    );
    assert!(
        per_session <= PER_SESSION_BUDGET,
        "a session now holds {per_session} bytes (budget {PER_SESSION_BUDGET}); if that is intended, re-measure and raise the budget"
    );
    assert!(
        per_subscription <= PER_GUILD_SUBSCRIPTION_BUDGET,
        "a guild subscription now holds {per_subscription} bytes (budget {PER_GUILD_SUBSCRIPTION_BUDGET})"
    );
}

/// The READY frame and the per-guild GUILD_CREATE of a member of 100 servers, and the lazy member list: a guild
/// with 5,000 members sends the same GUILD_CREATE as an empty one, and members arrive only in chunks on request.
#[tokio::test]
async fn ready_for_a_member_of_100_servers_is_small_and_member_lists_are_lazy() {
    let world = TestWorld::new(GatewayConfig::default());
    for g in 0..100 {
        world.add_member(UserId::new(1), guild(g));
    }
    // Guild 0 has a 5,000 member roster; guild 1 none.
    world.add_roster(guild(0), 10_000, 5_000);
    let mut c = world.connect();
    let hello = c.recv().await;
    assert_eq!(hello["op"], 10);
    c.send(serde_json::json!({"op": 2, "d": {"token": crate::test_support::token_for(1), "intents": intents::GUILDS, "properties": {}}}))
        .await;
    let ready = c.expect_event("READY").await;
    let ready_bytes = ready.to_string().len();
    let mut creates: Vec<usize> = Vec::new();
    for _ in 0..100 {
        creates.push(c.expect_event("GUILD_CREATE").await.to_string().len());
    }
    let total: usize = ready_bytes + creates.iter().sum::<usize>();
    eprintln!(
        "RDY-PERF-004: READY {ready_bytes} bytes, GUILD_CREATE {} bytes each (guild with a 5,000 roster: {}), 100 servers: {total} bytes in all",
        creates[1], creates[0]
    );
    assert!(
        ready_bytes <= READY_BUDGET,
        "READY is {ready_bytes} bytes (budget {READY_BUDGET})"
    );
    assert!(
        creates.iter().all(|&n| n <= GUILD_CREATE_BUDGET),
        "a GUILD_CREATE grew past {GUILD_CREATE_BUDGET} bytes: {creates:?}"
    );
    assert_eq!(
        creates[0], creates[1],
        "the roster size must not change GUILD_CREATE: member lists are lazy"
    );

    // Members come only when asked for, in chunks of at most CHUNK_SIZE.
    let mut bot = world.connect();
    world.add_member(UserId::new(7), guild(0));
    bot.identify(
        &format!("bot:7:{}", intents::GUILD_MEMBERS),
        intents::GUILDS | intents::GUILD_MEMBERS,
        1,
    )
    .await;
    bot.send(serde_json::json!({"op": 8, "d": {"guild_id": guild(0).get().to_string(), "query": "", "limit": 0}}))
        .await;
    let mut seen = 0usize;
    loop {
        let chunk: Value = bot.await_dispatch("GUILD_MEMBERS_CHUNK").await["d"].clone();
        let n = chunk["members"].as_array().map_or(0, Vec::len);
        assert!(n <= crate::members_request::CHUNK_SIZE);
        seen += n;
        if chunk["chunk_index"].as_u64().unwrap_or(0) + 1
            >= chunk["chunk_count"].as_u64().unwrap_or(1)
        {
            break;
        }
    }
    assert_eq!(seen, 5_000);
}
