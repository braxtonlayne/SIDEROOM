//! Postgres tests of the conflict log (BRG-SYN-002, migration 1754):
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-bridge --test conflicts -- --ignored --test-threads=1`

#[path = "support/mod.rs"]
mod support;

use serde_json::json;
use sr_bridge::BridgeError;
use sr_bridge::sync::conflicts::{
    Conflict, ConflictFilter, ConflictKind, NewConflict, ObjectKind, Resolution,
};
use support::*;

fn reverted(g: i64, role: i64) -> NewConflict {
    let mut c = NewConflict::new(
        g,
        ConflictKind::PermissionReverted,
        ObjectKind::Role,
        "permissions",
    );
    c.sr_object_id = Some(role);
    c.discord_object_id = Some(role + 1);
    c.discord_value = Some(json!("8"));
    c.sr_value = Some(json!("2048"));
    c.discord_actor_id = Some(77);
    c.discord_actor_name = Some("Mod Mara".into());
    c
}

async fn audit_rows(w: &World, g: i64, action: i16) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM audit_log_entries WHERE guild_id = $1 AND action = $2")
        .bind(g)
        .bind(action)
        .fetch_one(w.db.pool())
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_repeat_inside_ten_minutes_bumps_one_row_and_writes_one_audit_row() {
    let w = world().await;
    let b = w.bridged().await;
    let first = w.svc.record_conflict(reverted(b.guild, 500)).await.unwrap();
    assert!(first.created);
    assert_eq!(first.conflict.count, 1);
    let again = w.svc.record_conflict(reverted(b.guild, 500)).await.unwrap();
    assert!(!again.created);
    assert_eq!(again.conflict.id, first.conflict.id);
    assert_eq!(again.conflict.count, 2);
    // Another role is another row.
    let other = w.svc.record_conflict(reverted(b.guild, 600)).await.unwrap();
    assert!(other.created);
    assert_eq!(audit_rows(&w, b.guild, 222).await, 2);
    let row: (Option<String>, Option<serde_json::Value>) = sqlx::query_as(
        "SELECT reason, options FROM audit_log_entries WHERE guild_id = $1 AND action = 222 ORDER BY id LIMIT 1",
    )
    .bind(b.guild)
    .fetch_one(w.db.pool())
    .await
    .unwrap();
    assert_eq!(row.0.as_deref(), Some("Discord bridge: Mod Mara"));
    assert_eq!(row.1.unwrap()["sr_bridge"]["origin"], "discord");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn an_old_open_row_is_superseded_and_a_new_one_opens() {
    let w = world().await;
    let b = w.bridged().await;
    let first = w.svc.record_conflict(reverted(b.guild, 500)).await.unwrap();
    sqlx::query(
        "UPDATE bridge_conflicts SET last_at = now() - interval '11 minutes' WHERE id = $1",
    )
    .bind(first.conflict.id)
    .execute(w.db.pool())
    .await
    .unwrap();
    let second = w.svc.record_conflict(reverted(b.guild, 500)).await.unwrap();
    assert!(second.created);
    assert_ne!(second.conflict.id, first.conflict.id);
    let old = w
        .svc
        .get_conflict(b.guild, first.conflict.id)
        .await
        .unwrap();
    assert_eq!(old.status, "superseded");
    let all = ConflictFilter {
        limit: 10,
        ..Default::default()
    };
    assert_eq!(w.svc.list_conflicts(b.guild, &all).await.unwrap().len(), 2);
    let open = ConflictFilter {
        status: Some("open".into()),
        limit: 10,
        ..Default::default()
    };
    assert_eq!(w.svc.list_conflicts(b.guild, &open).await.unwrap().len(), 1);
    assert_eq!(w.svc.open_conflict_count(b.guild).await.unwrap(), 1);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn kinds_pick_the_audit_action_and_filters_work() {
    let w = world().await;
    let b = w.bridged().await;
    w.svc.record_conflict(reverted(b.guild, 1)).await.unwrap();
    let mut frozen = NewConflict::new(b.guild, ConflictKind::Frozen, ObjectKind::Channel, "");
    frozen.sr_object_id = Some(9);
    w.svc.record_conflict(frozen).await.unwrap();
    assert_eq!(audit_rows(&w, b.guild, 222).await, 1);
    assert_eq!(audit_rows(&w, b.guild, 223).await, 1);
    let f = ConflictFilter {
        kind: Some(ConflictKind::Frozen),
        limit: 10,
        ..Default::default()
    };
    let rows: Vec<Conflict> = w.svc.list_conflicts(b.guild, &f).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].object_kind, "channel");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn resolving_is_once_only_and_per_server() {
    let w = world().await;
    let b = w.bridged().await;
    let other_guild = b.guild + 1;
    let c = w
        .svc
        .record_conflict(reverted(b.guild, 5))
        .await
        .unwrap()
        .conflict;
    // Another server's manager cannot see or close it.
    assert!(matches!(
        w.svc.get_conflict(other_guild, c.id).await,
        Err(BridgeError::NotFound(_))
    ));
    assert!(matches!(
        w.svc
            .resolve_conflict(other_guild, c.id, b.owner, Resolution::Dismissed)
            .await,
        Err(BridgeError::NotFound(_))
    ));
    let done = w
        .svc
        .resolve_conflict(b.guild, c.id, b.owner, Resolution::AppliedOnSideRoom)
        .await
        .unwrap();
    assert_eq!(done.status, "resolved");
    assert_eq!(done.resolved_by, Some(b.owner));
    assert!(matches!(
        w.svc
            .resolve_conflict(b.guild, c.id, b.owner, Resolution::Dismissed)
            .await,
        Err(BridgeError::Conflict(_))
    ));
    // A closed key opens a fresh row next time.
    assert!(
        w.svc
            .record_conflict(reverted(b.guild, 5))
            .await
            .unwrap()
            .created
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn closed_rows_are_pruned_after_ninety_days_and_the_teardown_removes_the_rest() {
    let w = world().await;
    let b = w.bridged().await;
    let c = w
        .svc
        .record_conflict(reverted(b.guild, 5))
        .await
        .unwrap()
        .conflict;
    w.svc
        .resolve_conflict(b.guild, c.id, b.owner, Resolution::Dismissed)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE bridge_conflicts SET resolved_at = now() - interval '91 days' WHERE id = $1",
    )
    .bind(c.id)
    .execute(w.db.pool())
    .await
    .unwrap();
    let live = w
        .svc
        .record_conflict(reverted(b.guild, 6))
        .await
        .unwrap()
        .conflict;
    assert_eq!(w.svc.prune_closed_conflicts().await.unwrap(), 1);
    assert!(w.svc.get_conflict(b.guild, live.id).await.is_ok());
    w.svc
        .teardown(b.guild, b.owner, sr_bridge::TeardownReason::Owner)
        .await
        .unwrap();
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM bridge_conflicts WHERE guild_id = $1")
        .bind(b.guild)
        .fetch_one(w.db.pool())
        .await
        .unwrap();
    assert_eq!(n, 0);
}
