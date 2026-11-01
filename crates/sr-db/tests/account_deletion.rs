//! DB-backed tests for account deletion (LNG-USET-007). Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test account_deletion -- --ignored`

use sr_db::PgPool;
use sr_db::account_deletion::{Blocked, delete_account, username_reserved};
use sr_db::testing::TestDb;

async fn account(pool: &PgPool, id: i64, name: &str) {
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, display_name, email, password_hash, \
           bio, pronouns, accent_color, avatar_key, banner_key) \
         VALUES ($1, $1, $2, 'Real Name', $3, 'phc', 'about me', 'they/them', 255, 'av', 'bn')",
    )
    .bind(id)
    .bind(name)
    .bind(format!("{name}@example.test"))
    .execute(pool)
    .await
    .unwrap();
}

async fn count(pool: &PgPool, sql: &str, id: i64) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn deletion_scrubs_the_account_and_removes_ties() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    let (me, friend, other) = (1001_i64, 1002_i64, 1003_i64);
    account(pool, me, "leaving").await;
    account(pool, friend, "pal").await;
    account(pool, other, "owner").await;

    // A guild owned by someone else, with me as a member.
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES (2001, 'Room', $1)")
        .bind(other)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES (2001, $1), (2001, $2)")
        .bind(me)
        .bind(other)
        .execute(pool)
        .await
        .unwrap();
    // Friendship (both directions), a note, a session and an open invite plus credits.
    sqlx::query("INSERT INTO relationships (user_id, target_id, type) VALUES ($1,$2,1),($2,$1,1)")
        .bind(me)
        .bind(friend)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_notes (user_id, target_id, note) VALUES ($1,$2,'hi')")
        .bind(friend)
        .bind(me)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO sessions (id, account_id, token_hash, expires_at) \
         VALUES (3001, $1, decode(repeat('ab', 32), 'hex'), now() + interval '1 day')",
    )
    .bind(me)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO platform_invites (id, code, issuer_account_id) VALUES (4001, 'ABCDEFGHJK', $1)",
    )
    .bind(me)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO invite_credits_ledger (id, account_id, delta, reason) \
         VALUES (5001, $1, 5, 'initial_grant')",
    )
    .bind(me)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO user_connections (user_id, provider, external_id, display_name) \
         VALUES ($1, 'github', '1', 'me')",
    )
    .bind(me)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO profile_media (id, account_id, kind, object_key, content_type, width, height, bytes)          VALUES (6001, $1, 'gallery', 'avatars/9/abc', 'image/png', 10, 10, 100)",
    )
    .bind(me)
    .execute(pool)
    .await
    .unwrap();

    // PRF-PRO-001: my privacy policy and a held note a visitor left on my page go with me.
    sqlx::query("INSERT INTO profile_privacy (user_id, preset) VALUES ($1, 'members')")
        .bind(me)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO profile_guestbook (id, profile_id, author_id, body, status, anonymous)          VALUES (6101, $1, NULL, 'held note', 'pending', true)",
    )
    .bind(me)
    .execute(pool)
    .await
    .unwrap();

    // Personal module preferences (user-scope feature settings): mine go, a friend's stay.
    sqlx::query(
        "INSERT INTO feature_enablements (id, feature, scope, scope_id, enabled, config) \n         VALUES (7001, 'community.leveling', 'user', $1, NULL, '{\"values\":{\"announce_mine\":false}}'), \n                (7002, 'community.leveling', 'user', $2, NULL, '{}')",
    )
    .bind(me)
    .bind(friend)
    .execute(pool)
    .await
    .unwrap();

    let report = delete_account(pool, me, 5002).await.unwrap().unwrap();
    assert_eq!(
        count(
            pool,
            "SELECT count(*) FROM feature_enablements WHERE scope = 'user' AND scope_id = $1",
            me
        )
        .await,
        0
    );
    assert_eq!(
        count(
            pool,
            "SELECT count(*) FROM feature_enablements WHERE scope = 'user' AND scope_id = $1",
            friend
        )
        .await,
        1
    );
    assert_eq!(report.profile_media_keys, vec!["avatars/9/abc".to_owned()]);
    assert_eq!(
        count(
            pool,
            "SELECT count(*) FROM profile_media WHERE account_id = $1",
            me
        )
        .await,
        0
    );
    assert_eq!(
        count(
            pool,
            "SELECT count(*) FROM profile_privacy WHERE user_id = $1",
            me
        )
        .await,
        0
    );
    assert_eq!(
        count(
            pool,
            "SELECT count(*) FROM profile_guestbook WHERE profile_id = $1",
            me
        )
        .await,
        0,
        "pending notes on the page are erased too"
    );
    assert_eq!(report.guilds_left, vec![2001]);
    assert_eq!(report.friends, vec![friend]);
    assert_eq!(report.credits_forfeited, 5);
    assert_eq!(report.invites_revoked, 1);
    assert_eq!(report.avatar_key.as_deref(), Some("av"));

    let row: (
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<i32>,
    ) = sqlx::query_as(
        "SELECT state::text, username::text, display_name, email::text, bio, accent_color \
         FROM accounts WHERE id = $1",
    )
    .bind(me)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(row.0, "deleted");
    assert_eq!(row.1, format!("deleted.{me}"));
    assert_eq!(row.2.as_deref(), Some("Deleted User"));
    assert!(row.3.ends_with("@deleted.invalid"));
    assert!(row.4.is_none() && row.5.is_none());

    assert_eq!(
        count(
            pool,
            "SELECT count(*) FROM relationships WHERE user_id = $1 OR target_id = $1",
            me
        )
        .await,
        0
    );
    assert_eq!(
        count(
            pool,
            "SELECT count(*) FROM user_notes WHERE target_id = $1",
            me
        )
        .await,
        0
    );
    assert_eq!(
        count(
            pool,
            "SELECT count(*) FROM guild_members WHERE user_id = $1",
            me
        )
        .await,
        0
    );
    assert_eq!(
        count(
            pool,
            "SELECT count(*) FROM user_connections WHERE user_id = $1",
            me
        )
        .await,
        0
    );
    assert_eq!(
        count(
            pool,
            "SELECT count(*) FROM sessions WHERE account_id = $1 AND revoked_at IS NULL",
            me
        )
        .await,
        0
    );
    assert_eq!(
        count(
            pool,
            "SELECT count(*) FROM platform_invites WHERE issuer_account_id = $1 AND state = 'open'",
            me
        )
        .await,
        0
    );
    assert_eq!(
        count(pool, "SELECT COALESCE(SUM(delta), 0)::BIGINT FROM invite_credits_ledger WHERE account_id = $1", me).await,
        0,
        "credits are forfeited through the ledger"
    );
    // The friend and the guild owner are untouched.
    assert_eq!(
        count(
            pool,
            "SELECT count(*) FROM guild_members WHERE user_id = $1",
            other
        )
        .await,
        1
    );

    // The original username is archived and reserved for 30 days.
    assert!(username_reserved(pool, "leaving").await.unwrap());
    assert!(!username_reserved(pool, "someone-else").await.unwrap());
    let archived: String =
        sqlx::query_scalar("SELECT email::text FROM deleted_accounts WHERE account_id = $1")
            .bind(me)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(archived, "leaving@example.test");

    // Once the hold lapses the name is free again.
    sqlx::query("UPDATE deleted_accounts SET username_release_at = now() - interval '1 second'")
        .execute(pool)
        .await
        .unwrap();
    assert!(!username_reserved(pool, "leaving").await.unwrap());

    // A second deletion is refused.
    assert_eq!(
        delete_account(pool, me, 5003).await.unwrap(),
        Err(Blocked::NotActive)
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_guild_owner_must_hand_over_first() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    account(pool, 1, "boss").await;
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES (10, 'Mine', 1)")
        .execute(pool)
        .await
        .unwrap();
    assert_eq!(
        delete_account(pool, 1, 9).await.unwrap(),
        Err(Blocked::OwnsGuilds(vec![10]))
    );
    let state: String = sqlx::query_scalar("SELECT state::text FROM accounts WHERE id = 1")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(state, "active", "a refused deletion changes nothing");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn group_dm_ownership_passes_on() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    account(pool, 1, "gone").await;
    account(pool, 2, "stays").await;
    sqlx::query("INSERT INTO channels (id, type, owner_id) VALUES (50, 3, 1)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO channel_recipients (channel_id, user_id) VALUES (50, 1), (50, 2)")
        .execute(pool)
        .await
        .unwrap();
    delete_account(pool, 1, 9).await.unwrap().unwrap();
    let owner: i64 = sqlx::query_scalar("SELECT owner_id FROM channels WHERE id = 50")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(owner, 2);
    assert_eq!(
        count(
            pool,
            "SELECT count(*) FROM channel_recipients WHERE user_id = $1",
            1
        )
        .await,
        0
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn staff_and_the_owner_cannot_delete_themselves() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    account(pool, 1, "boss").await;
    account(pool, 2, "helper").await;
    sqlx::query(
        "INSERT INTO guilds (id, name, owner_id, is_staff_guild) VALUES (10, 'Staff', 1, TRUE)",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO staff_appointments (id, account_id, appointed_by) VALUES (1, 2, 1)")
        .execute(pool)
        .await
        .unwrap();
    assert_eq!(
        delete_account(pool, 1, 9).await.unwrap(),
        Err(Blocked::Owner)
    );
    assert_eq!(
        delete_account(pool, 2, 9).await.unwrap(),
        Err(Blocked::Staff)
    );
    // Once the appointment ends, the deletion goes through.
    sqlx::query(
        "UPDATE staff_appointments SET revoked_at = now(), revoked_by = 1, \
         revoked_reason = 'revoked' WHERE account_id = 2",
    )
    .execute(pool)
    .await
    .unwrap();
    assert!(delete_account(pool, 2, 9).await.unwrap().is_ok());
}

/// Integrator (wave 44): erasure and the graduated-moderation rows. Vouches given and received go (MOD-VCH-001);
/// the member's restrictions and subtree-case nodes stay as staff-side moderation history (D-124, G-66), and the
/// case is named nowhere on the scrubbed account: its reference lives in the staff tables and the audit log only.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn erasure_removes_vouches_and_keeps_the_moderation_record() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    let (me, voucher, vouchee, staff) = (3001_i64, 3002_i64, 3003_i64, 3004_i64);
    account(pool, me, "frozen").await;
    account(pool, voucher, "fan").await;
    account(pool, vouchee, "pal").await;
    account(pool, staff, "mod").await;
    sqlx::query(
        "INSERT INTO profile_vouches (voucher_id, vouchee_id) VALUES ($1, $2), ($2, $3), ($1, $3)",
    )
    .bind(voucher)
    .bind(me)
    .bind(vouchee)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO moderation_subtree_cases (id, root_account_id, category, opened_by) VALUES (77, $1, 'fraud', $2)",
    )
    .bind(vouchee)
    .bind(staff)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO moderation_subtree_nodes (case_id, account_id, role, depth, registered_at, unactivated, status) \
         VALUES (77, $1, 'descendant', 1, now() - interval '40 days', true, 'auto_frozen')",
    )
    .bind(me)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO member_restrictions (id, account_id, kind, reason, case_id, imposed_by, ends_at, removes_account, \
         appeal_message, appealed_at, appeal_state) \
         VALUES (88, $1, 'case_freeze', 'under review', 77, $2, now() + interval '30 days', true, 'I was away', now(), 'open')",
    )
    .bind(me)
    .bind(staff)
    .execute(pool)
    .await
    .unwrap();

    delete_account(pool, me, 9).await.unwrap().unwrap();

    // Vouches given and received by the erased account are gone; the others stay.
    let left: Vec<(i64, i64)> =
        sqlx::query_as("SELECT voucher_id, vouchee_id FROM profile_vouches ORDER BY 1, 2")
            .fetch_all(pool)
            .await
            .unwrap();
    assert_eq!(left, vec![(voucher, vouchee)]);
    // The moderation record stays for staff.
    assert_eq!(
        count(
            pool,
            "SELECT count(*) FROM member_restrictions WHERE account_id = $1",
            me
        )
        .await,
        1
    );
    assert_eq!(
        count(
            pool,
            "SELECT count(*) FROM moderation_subtree_nodes WHERE account_id = $1",
            me
        )
        .await,
        1
    );
    // The scrubbed account row carries no case reference.
    let row: String =
        sqlx::query_scalar("SELECT row_to_json(a)::text FROM accounts a WHERE id = $1")
            .bind(me)
            .fetch_one(pool)
            .await
            .unwrap();
    assert!(
        !row.contains("fraud") && !row.contains("\"77\"") && !row.contains("under review"),
        "{row}"
    );
}
