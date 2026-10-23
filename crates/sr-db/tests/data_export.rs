//! DB-backed tests for the data export (LNG-USET-020). Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test data_export -- --ignored`

use std::io::{Cursor, Read};

use sr_db::PgPool;
use sr_db::data_export::{self, RequestRefusal};
use sr_db::testing::TestDb;

async fn account(pool: &PgPool, id: i64, name: &str) {
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) \
         VALUES ($1, $1, $2, $3, 'secret-hash')",
    )
    .bind(id)
    .bind(name)
    .bind(format!("{name}@example.test"))
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn request_is_queued_once_and_cooled_down() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    account(pool, 1, "asker").await;

    let first = data_export::request(pool, 100, 101, 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.state, "pending");
    let job: (String, serde_json::Value) =
        sqlx::query_as("SELECT kind, payload FROM jobs WHERE id = 101")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(job.0, data_export::JOB_KIND);
    assert_eq!(job.1["export_id"], "100");

    // A second request while one is open is refused.
    assert!(matches!(
        data_export::request(pool, 102, 103, 1).await.unwrap(),
        Err(RequestRefusal::InProgress(r)) if r.id == 100
    ));
    // After it finished, the cooldown applies; a failed one does not count.
    assert_eq!(data_export::begin(pool, 100).await.unwrap(), Some(1));
    data_export::finish(pool, 100, "exports/1/100.zip", 10)
        .await
        .unwrap();
    assert!(matches!(
        data_export::request(pool, 102, 103, 1).await.unwrap(),
        Err(RequestRefusal::TooSoon { retry_after_secs }) if retry_after_secs > 23 * 3600
    ));
    let ready = data_export::latest(pool, 1).await.unwrap().unwrap();
    assert_eq!(ready.state, "ready");
    assert!(ready.expires_ms.unwrap() - ready.ready_ms.unwrap() >= 7 * 86_400_000 - 1000);
    assert!(
        data_export::get(pool, 2, 100).await.unwrap().is_none(),
        "not another user's"
    );

    sqlx::query("UPDATE data_exports SET state = 'failed' WHERE id = 100")
        .execute(pool)
        .await
        .unwrap();
    assert!(
        data_export::request(pool, 102, 103, 1)
            .await
            .unwrap()
            .is_ok()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_cooldown_follows_the_platform_setting() {
    // RDY-SEC-006: `abuse.data_export_cooldown_hours` (default 24) is read on every request.
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    account(pool, 1, "asker").await;
    data_export::request(pool, 100, 101, 1)
        .await
        .unwrap()
        .unwrap();
    data_export::begin(pool, 100).await.unwrap();
    data_export::finish(pool, 100, "exports/1/100.zip", 10)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO platform_settings (key, value) VALUES ('abuse.data_export_cooldown_hours', '72'::jsonb)",
    )
    .execute(pool)
    .await
    .unwrap();
    assert!(matches!(
        data_export::request(pool, 102, 103, 1).await.unwrap(),
        Err(RequestRefusal::TooSoon { retry_after_secs }) if retry_after_secs > 71 * 3600
    ));
    // A value outside the catalog range is ignored (the default applies).
    sqlx::query("UPDATE platform_settings SET value = '0'::jsonb WHERE key = 'abuse.data_export_cooldown_hours'")
        .execute(pool)
        .await
        .unwrap();
    assert!(matches!(
        data_export::request(pool, 102, 103, 1).await.unwrap(),
        Err(RequestRefusal::TooSoon { retry_after_secs }) if retry_after_secs > 23 * 3600 && retry_after_secs <= 24 * 3600
    ));
    // Lowered to one hour and the request made two hours ago: allowed.
    sqlx::query("UPDATE platform_settings SET value = '1'::jsonb WHERE key = 'abuse.data_export_cooldown_hours'")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE data_exports SET requested_at = now() - interval '2 hours' WHERE id = 100")
        .execute(pool)
        .await
        .unwrap();
    assert!(data_export::request(pool, 102, 103, 1).await.unwrap().is_ok());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn expired_links_are_found_and_marked() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    account(pool, 1, "asker").await;
    data_export::request(pool, 100, 101, 1)
        .await
        .unwrap()
        .unwrap();
    data_export::finish(pool, 100, "exports/1/100.zip", 10)
        .await
        .unwrap();
    assert!(data_export::lapsed(pool).await.unwrap().is_empty());
    sqlx::query("UPDATE data_exports SET expires_at = now() - interval '1 minute'")
        .execute(pool)
        .await
        .unwrap();
    assert_eq!(
        data_export::lapsed(pool).await.unwrap(),
        vec![(100, "exports/1/100.zip".to_owned())]
    );
    data_export::mark_expired(pool, 100).await.unwrap();
    assert_eq!(
        data_export::latest(pool, 1).await.unwrap().unwrap().state,
        "expired"
    );
    assert!(data_export::lapsed(pool).await.unwrap().is_empty());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_package_has_the_users_data_and_no_secrets() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
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
    sqlx::query("INSERT INTO guild_members (guild_id, user_id, nick) VALUES (10, 1, 'Nick')")
        .execute(pool)
        .await
        .unwrap();
    // Personal module settings: mine are in the package, another person's are not.
    sqlx::query(
        "INSERT INTO feature_enablements (id, feature, scope, scope_id, enabled, config) VALUES \n         (7001, 'community.leveling', 'user', 1, NULL, '{\"values\":{\"announce_mine\":false}}'), \n         (7002, 'community.leveling', 'user', 2, NULL, '{\"values\":{\"announce_mine\":true}}')",
    )
    .execute(pool)
    .await
    .unwrap();
    // Three messages by the user (one deleted) and one by someone else.
    for (id, author, content, deleted) in [
        (1_i64, 1_i64, "hello", false),
        (2, 1, "second", false),
        (3, 1, "gone", true),
        (4, 2, "not mine", false),
    ] {
        sqlx::query(
            "INSERT INTO messages (id, channel_id, guild_id, author_id, content, visibility, deleted_at) \
             VALUES ($1, 20, 10, $2, $3, $4::message_visibility, CASE WHEN $5 THEN now() END)",
        )
        .bind(id)
        .bind(author)
        .bind(content)
        .bind(if deleted { "user_deleted" } else { "visible" })
        .bind(deleted)
        .execute(pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO message_webhook_authors (message_id, webhook_id, username) VALUES (1, 88, 'Deploys')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO relationships (user_id, target_id, type) VALUES (1, 2, 1)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_notes (user_id, target_id, note) VALUES (1, 2, 'my note')")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO user_connections (user_id, provider, external_id, display_name) \
         VALUES (1, 'github', '9', 'octo')",
    )
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO profile_media (id, account_id, kind, object_key, content_type, width, height, bytes)          VALUES (6001, 1, 'background', 'avatars/1/abc', 'image/png', 640, 480, 1234)",
    )
    .execute(pool)
    .await
    .unwrap();

    // N-15: a reaction and a poll vote of the member, and one of someone else's.
    sqlx::query("UPDATE messages SET poll = '{\"finalized\":false}'::jsonb WHERE id = 4")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO reactions (message_id, emoji_key, user_id) VALUES (4, 'x', 1), (1, 'x', 2)",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO poll_votes (message_id, answer_id, user_id) VALUES (4, 2, 1), (4, 1, 2)",
    )
    .execute(pool)
    .await
    .unwrap();

    let mut buf = Cursor::new(Vec::new());
    let n = data_export::build_package(pool, 1, &mut buf, 1_700_000_000_000, 10)
        .await
        .unwrap();
    assert_eq!(n, 2, "only the user's own, undeleted messages");

    let mut zip = zip::ZipArchive::new(Cursor::new(buf.into_inner())).unwrap();
    let names: Vec<String> = zip.file_names().map(|n| n.unwrap().into_owned()).collect();
    for want in [
        "README.txt",
        "account.json",
        "settings.json",
        "relationships.json",
        "servers.json",
        "connections.json",
        "invites.json",
        "invoices.json",
        "activity.json",
        "security.json",
        "attachments.json",
        "reactions.json",
        "poll_votes.json",
        "messages/20.json",
        "package.json",
    ] {
        assert!(
            names.iter().any(|n| n == want),
            "{want} missing in {names:?}"
        );
    }
    let mut read = |name: &str| -> String {
        let mut s = String::new();
        zip.by_name(name).unwrap().read_to_string(&mut s).unwrap();
        s
    };
    let settings: serde_json::Value = serde_json::from_str(&read("settings.json")).unwrap();
    let modules = settings["module_settings"].as_array().unwrap();
    assert_eq!(modules.len(), 1, "only the person's own rows");
    assert_eq!(modules[0]["feature"], "community.leveling");
    assert_eq!(modules[0]["settings"]["announce_mine"], false);
    let reactions: serde_json::Value = serde_json::from_str(&read("reactions.json")).unwrap();
    assert_eq!(
        reactions.as_array().unwrap().len(),
        1,
        "only the member's own"
    );
    assert_eq!(reactions[0]["message_id"], "4");
    assert_eq!(reactions[0]["channel_id"], "20");
    assert_eq!(reactions[0]["emoji_key"], "x");
    let votes: serde_json::Value = serde_json::from_str(&read("poll_votes.json")).unwrap();
    assert_eq!(votes.as_array().unwrap().len(), 1);
    assert_eq!(
        (
            votes[0]["message_id"].as_str(),
            votes[0]["answer_id"].as_i64()
        ),
        (Some("4"), Some(2))
    );
    assert!(
        !reactions.to_string().contains("not mine"),
        "no message text"
    );
    let readme = read("README.txt");
    assert!(readme.contains("reactions.json") && readme.contains("poll_votes.json"));
    let activity: serde_json::Value = serde_json::from_str(&read("activity.json")).unwrap();
    assert_eq!(activity["profile_media"][0]["id"], "6001");
    assert_eq!(activity["profile_media"][0]["kind"], "background");
    assert_eq!(activity["profile_media"][0]["width"], 640);
    let account: serde_json::Value = serde_json::from_str(&read("account.json")).unwrap();
    assert_eq!(account["username"], "writer");
    assert_eq!(account["id"], "1", "ids are strings");
    let everything: String = names.iter().map(|n| read(n)).collect();
    assert!(
        !everything.contains("secret-hash"),
        "no password hash anywhere"
    );
    let messages: serde_json::Value = serde_json::from_str(&read("messages/20.json")).unwrap();
    let contents: Vec<&str> = messages
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["content"].as_str().unwrap())
        .collect();
    assert_eq!(contents, ["hello", "second"]);
    // D-235: a message sent through a webhook carries the webhook, the others do not.
    assert_eq!(
        messages[0]["webhook"],
        serde_json::json!({"id": "88", "name": "Deploys"})
    );
    assert!(messages[1]["webhook"].is_null());
    let rel: serde_json::Value = serde_json::from_str(&read("relationships.json")).unwrap();
    assert_eq!(rel["notes"][0]["note"], "my note");
    assert_eq!(rel["relationships"][0]["user_id"], "2");
    let servers: serde_json::Value = serde_json::from_str(&read("servers.json")).unwrap();
    assert_eq!(servers["memberships"][0]["name"], "Room");
    let conns: serde_json::Value = serde_json::from_str(&read("connections.json")).unwrap();
    assert_eq!(conns[0]["name"], "octo");
}
