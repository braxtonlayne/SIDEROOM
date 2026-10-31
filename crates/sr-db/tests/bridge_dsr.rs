//! BRG-UI-001: the Discord bridge's part of the data-subject export and erasure (lawyer brief Q21). Needs the tables of
//! migration 1720 (bridge-core). `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test bridge_dsr -- --ignored --test-threads=1`

use sr_db::PgPool;
use sr_db::bridge_dsr;
use sr_db::testing::TestDb;

async fn account(pool: &PgPool, id: i64, name: &str) {
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, display_name, email, password_hash) \
         VALUES ($1, $1, $2, $2, $3, 'phc')",
    )
    .bind(id)
    .bind(name)
    .bind(format!("{name}@example.test"))
    .execute(pool)
    .await
    .unwrap();
}

/// A server with a mirrored channel, three messages (two by `1`, one by `2`) and their map rows.
async fn world(pool: &PgPool) {
    account(pool, 1, "writer").await;
    account(pool, 2, "other").await;
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES (10, 'Room', 2)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO channels (id, guild_id, type, name) VALUES (20, 10, 0, 'general')")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO bridges (guild_id, discord_guild_id, bot_user_id, bot_token_enc, created_by) \
         VALUES (10, 500, 501, decode('00', 'hex'), 2)",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO bridge_channel_maps (id, guild_id, channel_id, discord_channel_id, discord_webhook_id, \
           webhook_token_enc, direction, created_by) VALUES (30, 10, 20, 600, 700, decode('00', 'hex'), 'both', 2)",
    )
    .execute(pool)
    .await
    .unwrap();
    for (id, author) in [(101_i64, 1_i64), (102, 1), (103, 2)] {
        sqlx::query(
            "INSERT INTO messages (id, channel_id, guild_id, author_id, content) VALUES ($1, 20, 10, $2, 'x')",
        )
        .bind(id)
        .bind(author)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO bridge_message_map (sr_message_id, discord_message_id, map_id, webhook_id, direction) \
             VALUES ($1, $1 + 9000, 30, 700, 'out')",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }
    sqlx::query("INSERT INTO bridge_optouts (user_id, channel_id, guild_id) VALUES (1, 20, 10), (2, 20, 10)")
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL and migration 1720"]
async fn the_export_lists_the_members_bridged_messages_as_ids_only() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    world(pool).await;
    let out = bridge_dsr::export_for(pool, 1).await.unwrap();
    let ids: Vec<&str> = out["bridged_messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["message_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["101", "102"], "only the member's own messages");
    let first = &out["bridged_messages"][0];
    assert_eq!(first["discord_message_id"], "9101");
    assert_eq!(first["channel_id"], "20");
    assert!(
        first.get("content").is_none(),
        "ids only, never text: {first}"
    );
    assert_eq!(out["opt_outs"].as_array().unwrap().len(), 1);
    assert_eq!(out["opt_outs"][0]["channel_id"], "20");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL and migration 1720"]
async fn erasure_queues_the_webhook_deletes_and_removes_map_rows_and_optouts() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    world(pool).await;
    let mut tx = pool.begin().await.unwrap();
    let done = bridge_dsr::erase_for(&mut tx, 1).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        (done.map_rows, done.deletions_queued, done.opt_outs),
        (2, 2, 1)
    );

    let left: Vec<i64> =
        sqlx::query_scalar("SELECT sr_message_id FROM bridge_message_map ORDER BY 1")
            .fetch_all(pool)
            .await
            .unwrap();
    assert_eq!(left, [103], "the other member's map row stays");
    let optouts: i64 = sqlx::query_scalar("SELECT count(*) FROM bridge_optouts WHERE user_id = 1")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(optouts, 0);
    let other: i64 = sqlx::query_scalar("SELECT count(*) FROM bridge_optouts WHERE user_id = 2")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(other, 1, "someone else's opt-out stays");

    // The deletes the worker needs carry everything, because the map rows are gone.
    let rows: Vec<(i64, String, serde_json::Value)> = sqlx::query_as(
        "SELECT sr_message_id, kind, payload FROM bridge_outbox ORDER BY sr_message_id",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, 101);
    assert_eq!(rows[0].1, "delete");
    assert_eq!(rows[0].2["discord_message_id"], "9101");
    assert_eq!(rows[0].2["webhook_id"], "700");
    assert_eq!(rows[0].2["reason"], "erasure");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL and migration 1720"]
async fn a_member_with_nothing_bridged_erases_cleanly() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    account(pool, 9, "nobody").await;
    let mut tx = pool.begin().await.unwrap();
    let done = bridge_dsr::erase_for(&mut tx, 9).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        (done.map_rows, done.deletions_queued, done.opt_outs),
        (0, 0, 0)
    );
    let out = bridge_dsr::export_for(pool, 9).await.unwrap();
    assert_eq!(out["bridged_messages"].as_array().unwrap().len(), 0);
}
