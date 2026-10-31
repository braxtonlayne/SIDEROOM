//! Postgres tests of the whole-guild sync storage (BRG-SYN-001): migrations 1750 to 1753, the ops queue, the
//! deletion hold. Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-bridge --test sync_db -- --include-ignored --test-threads=1`

#[path = "support/mod.rs"]
mod support;

use serde_json::json;
use sr_bridge::sync::{
    ObjectKind, OpDirection, OpPriority, OpState, SyncMode, SyncState, SyncedState, holds, ops,
    store,
};
use support::*;

fn new_op(g: i64, kind: ObjectKind, object: i64, priority: OpPriority) -> ops::NewOp {
    ops::NewOp {
        id: uid(),
        guild_id: g,
        kind,
        object_id: object,
        direction: OpDirection::ToDiscord,
        priority,
        seed_run: None,
        step: 0,
    }
}

async fn set_state(w: &World, g: i64, s: SyncState) {
    store::set_sync_state(w.db.pool(), g, s).await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_bridge_starts_selective_with_sync_off() {
    let w = world().await;
    let b = w.bridged().await;
    let s = store::get_sync(w.db.pool(), b.guild)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(s.mode, SyncMode::Selective);
    assert_eq!(s.state, SyncState::Off);
    let maps = store::list_structure_maps(w.db.pool(), b.guild)
        .await
        .unwrap();
    assert_eq!(maps.len(), 1);
    assert!(maps[0].selected && maps[0].origin == store::Origin::Matched);
    assert_eq!(maps[0].kind, "channel");
    assert!(
        store::live_whole_guild(w.db.pool())
            .await
            .unwrap()
            .is_empty()
    );
    let _ = w.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn one_whole_guild_bridge_per_discord_server() {
    let w = world().await;
    let a = w.bridged().await;
    sqlx::query("UPDATE bridges SET mode = 'whole_guild' WHERE guild_id = $1")
        .bind(a.guild)
        .execute(w.db.pool())
        .await
        .unwrap();
    // A second SIDE ROOM server on the same Discord server cannot also be whole_guild.
    let owner = account(w.db.pool()).await;
    let (g2, _c) = guild(w.db.pool(), owner).await;
    sqlx::query(
        "INSERT INTO bridges (guild_id, discord_guild_id, bot_user_id, bot_token_enc, created_by) \
         SELECT $1, discord_guild_id, bot_user_id, bot_token_enc, created_by FROM bridges WHERE guild_id = $2",
    )
    .bind(g2)
    .bind(a.guild)
    .execute(w.db.pool())
    .await
    .unwrap();
    let r = sqlx::query("UPDATE bridges SET mode = 'whole_guild' WHERE guild_id = $1")
        .bind(g2)
        .execute(w.db.pool())
        .await;
    assert!(
        r.is_err(),
        "the unique index refuses a second whole_guild bridge"
    );
    let _ = w.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn structure_and_role_maps_round_trip_with_their_synced_state() {
    let w = world().await;
    let b = w.bridged().await;
    let cat = channel_of(w.db.pool(), b.guild, "Text", 4).await;
    let mut synced = SyncedState::new();
    synced.record("name", json!("Text"), json!("Text"));
    let m = store::NewStructureMap {
        id: uid(),
        guild_id: b.guild,
        channel_id: cat,
        discord_channel_id: 9001,
        discord_channel_name: "Text".into(),
        kind: "category",
        selected: false,
        origin: store::Origin::Sr,
        parent_map_id: None,
        created_by: b.owner,
        synced: synced.clone(),
    };
    assert!(store::insert_structure_map(w.db.pool(), &m).await.unwrap());
    assert!(
        !store::insert_structure_map(w.db.pool(), &m).await.unwrap(),
        "the same pair is not mapped twice"
    );
    let got = store::structure_map_for_channel(w.db.pool(), cat)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got.kind, "category");
    assert_eq!(got.origin, store::Origin::Sr);
    assert_eq!(got.synced, synced);
    let other = store::structure_map_for_discord(w.db.pool(), b.guild, 9001)
        .await
        .unwrap();
    assert_eq!(other.unwrap().channel_id, cat);
    store::save_map_state(
        w.db.pool(),
        got.id,
        &SyncedState::new(),
        "degraded",
        Some("no Community"),
    )
    .await
    .unwrap();
    let again = store::structure_map_for_channel(w.db.pool(), cat)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(again.sync_status, "degraded");
    assert_eq!(again.degraded_reason.as_deref(), Some("no Community"));

    // A role map, with Discord-only bits kept raw.
    let role = uid();
    sqlx::query("INSERT INTO roles (id, guild_id, name) VALUES ($1, $2, 'Mods')")
        .bind(role)
        .bind(b.guild)
        .execute(w.db.pool())
        .await
        .unwrap();
    let rm = store::RoleMap {
        id: uid(),
        guild_id: b.guild,
        role_id: role,
        discord_role_id: 777,
        is_everyone: false,
        origin: store::Origin::Matched,
        discord_only_bits: (1u64 << 47) | (1u64 << 53),
        synced: SyncedState::new(),
        sync_status: "mapped".into(),
    };
    assert!(store::insert_role_map(w.db.pool(), &rm).await.unwrap());
    let back = store::role_map_for_role(w.db.pool(), role)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(back.discord_only_bits, (1u64 << 47) | (1u64 << 53));
    assert_eq!(
        store::role_map_for_discord(w.db.pool(), b.guild, 777)
            .await
            .unwrap()
            .unwrap()
            .role_id,
        role
    );
    assert_eq!(
        store::delete_role_maps(w.db.pool(), b.guild).await.unwrap(),
        1
    );
    let _ = w.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_burst_of_edits_is_one_pending_op_and_reverts_run_first() {
    let w = world().await;
    let b = w.bridged().await;
    set_state(&w, b.guild, SyncState::Live).await;
    let p = w.db.pool();
    assert!(
        ops::enqueue(
            p,
            new_op(b.guild, ObjectKind::Role, 5, OpPriority::Structure)
        )
        .await
        .unwrap()
    );
    for _ in 0..4 {
        assert!(
            !ops::enqueue(
                p,
                new_op(b.guild, ObjectKind::Role, 5, OpPriority::Structure)
            )
            .await
            .unwrap(),
            "coalesced into the pending row"
        );
    }
    // A revert for the same object raises the priority of the one row.
    assert!(
        !ops::enqueue(p, new_op(b.guild, ObjectKind::Role, 5, OpPriority::Revert))
            .await
            .unwrap()
    );
    assert!(
        ops::enqueue(
            p,
            new_op(b.guild, ObjectKind::Channel, 6, OpPriority::Structure)
        )
        .await
        .unwrap()
    );
    let c = ops::counts(p, b.guild).await.unwrap();
    assert_eq!((c.pending, c.dead), (2, 0));

    // Nothing is due inside the debounce.
    assert!(ops::claim_due(p, 10).await.unwrap().is_empty());
    sqlx::query("UPDATE bridge_sync_ops SET due_at = now() - interval '1 second'")
        .execute(p)
        .await
        .unwrap();
    let got = ops::claim_due(p, 10).await.unwrap();
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].kind, ObjectKind::Role, "the revert runs first");
    assert_eq!(got[0].priority, OpPriority::Revert);
    assert!(got.iter().all(|o| o.state == OpState::Running));
    assert!(
        ops::claim_due(p, 10).await.unwrap().is_empty(),
        "claimed once"
    );

    // A new edit while the op runs queues a new pending row.
    assert!(
        ops::enqueue(
            p,
            new_op(b.guild, ObjectKind::Role, 5, OpPriority::Structure)
        )
        .await
        .unwrap()
    );
    for o in &got {
        ops::complete(p, o.id).await.unwrap();
    }
    assert_eq!(ops::counts(p, b.guild).await.unwrap().pending, 1);
    let _ = w.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn paused_and_off_bridges_run_no_ops() {
    let w = world().await;
    let b = w.bridged().await;
    let p = w.db.pool();
    ops::enqueue(
        p,
        new_op(b.guild, ObjectKind::Role, 1, OpPriority::Background),
    )
    .await
    .unwrap();
    for s in [
        SyncState::Off,
        SyncState::Paused,
        SyncState::PausedDiscordAccess,
        SyncState::PausedInvalidRequests,
    ] {
        set_state(&w, b.guild, s).await;
        assert!(ops::claim_due(p, 10).await.unwrap().is_empty(), "{s:?}");
    }
    set_state(&w, b.guild, SyncState::Seeding).await;
    assert_eq!(ops::claim_due(p, 10).await.unwrap().len(), 1);
    let _ = w.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn failures_back_off_then_go_dead_and_retry_requeues() {
    let w = world().await;
    let b = w.bridged().await;
    set_state(&w, b.guild, SyncState::Live).await;
    let p = w.db.pool();
    let op = new_op(b.guild, ObjectKind::Channel, 3, OpPriority::Background);
    ops::enqueue(p, op).await.unwrap();
    let mut id = op.id;
    for attempt in 1..8 {
        sqlx::query("UPDATE bridge_sync_ops SET due_at = now() WHERE id = $1")
            .bind(id)
            .execute(p)
            .await
            .unwrap();
        let c = ops::claim_due(p, 5).await.unwrap();
        assert_eq!(c.len(), 1, "attempt {attempt}");
        id = c[0].id;
        assert_eq!(ops::fail(p, id, "429").await.unwrap(), OpState::Pending);
    }
    sqlx::query("UPDATE bridge_sync_ops SET due_at = now() WHERE id = $1")
        .bind(id)
        .execute(p)
        .await
        .unwrap();
    ops::claim_due(p, 5).await.unwrap();
    assert_eq!(
        ops::fail(p, id, "403 Missing Permissions").await.unwrap(),
        OpState::Dead
    );
    let dead = ops::list_dead(p, b.guild, 10).await.unwrap();
    assert_eq!(dead.len(), 1);
    assert_eq!(
        dead[0].last_error.as_deref(),
        Some("403 Missing Permissions")
    );
    assert_eq!(ops::counts(p, b.guild).await.unwrap().dead, 1);
    // Another guild cannot retry it.
    assert!(!ops::retry_dead(p, b.guild + 1, id).await.unwrap());
    assert!(ops::retry_dead(p, b.guild, id).await.unwrap());
    let c = ops::counts(p, b.guild).await.unwrap();
    assert_eq!((c.pending, c.dead), (1, 0));
    let _ = w.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn running_ops_come_back_after_a_crash() {
    let w = world().await;
    let b = w.bridged().await;
    set_state(&w, b.guild, SyncState::Live).await;
    let p = w.db.pool();
    ops::enqueue(
        p,
        new_op(b.guild, ObjectKind::Role, 9, OpPriority::Background),
    )
    .await
    .unwrap();
    assert_eq!(ops::claim_due(p, 5).await.unwrap().len(), 1);
    assert_eq!(ops::recover_running(p).await.unwrap(), 1);
    assert_eq!(ops::claim_due(p, 5).await.unwrap().len(), 1);
    let _ = w.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_held_channel_is_hidden_then_released_or_due() {
    let w = world().await;
    let b = w.bridged().await;
    let p = w.db.pool();
    let c2 = channel(p, b.guild, "second").await;
    assert!(!holds::is_held(p, c2).await.unwrap());
    assert!(
        holds::hold(p, b.guild, c2, 55, None, Some(42), 24)
            .await
            .unwrap()
    );
    assert!(
        !holds::hold(p, b.guild, c2, 55, None, Some(42), 1)
            .await
            .unwrap(),
        "a second delete event keeps the first deadline"
    );
    assert!(holds::is_held(p, c2).await.unwrap());
    let list = holds::list_held(p, b.guild).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].discord_actor_id, Some(42));
    assert!(list[0].delete_after_ms > list[0].held_at_ms + 23 * 3_600_000);
    assert!(holds::due(p, 10).await.unwrap().is_empty());

    // Zero hours is immediate.
    let c3 = channel(p, b.guild, "third").await;
    holds::hold(p, b.guild, c3, 56, None, None, 0)
        .await
        .unwrap();
    let due = holds::due(p, 10).await.unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].channel_id, c3);

    let h = holds::release(p, c2).await.unwrap().unwrap();
    assert_eq!(h.discord_channel_id, 55);
    assert!(!holds::is_held(p, c2).await.unwrap());
    assert!(holds::release(p, c2).await.unwrap().is_none());
    // The hold dies with the bridge (teardown cancels the hold: the object comes back).
    assert_eq!(holds::cancel_all(p, b.guild).await.unwrap(), 1);
    let _ = w.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_snapshot_keeps_the_first_state_and_goes_with_the_bridge() {
    let w = world().await;
    let b = w.bridged().await;
    let p = w.db.pool();
    holds::save_snapshot(p, b.guild, "role", 7, &json!({"permissions": "8"}))
        .await
        .unwrap();
    holds::save_snapshot(p, b.guild, "role", 7, &json!({"permissions": "0"}))
        .await
        .unwrap();
    let rows = holds::load_snapshot(p, b.guild, "role").await.unwrap();
    assert_eq!(
        rows,
        vec![(7, json!({"permissions": "8"}))],
        "first write wins: it is the pre-bridge state"
    );
    assert!(
        holds::load_snapshot(p, b.guild, "channel")
            .await
            .unwrap()
            .is_empty()
    );
    sqlx::query("DELETE FROM bridges WHERE guild_id = $1")
        .bind(b.guild)
        .execute(p)
        .await
        .unwrap();
    assert!(
        holds::load_snapshot(p, b.guild, "role")
            .await
            .unwrap()
            .is_empty()
    );
    let _ = w.db.cleanup().await;
}
