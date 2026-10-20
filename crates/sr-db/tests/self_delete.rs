//! DB-backed tests of the member bulk self-delete (N-05, D-324). Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test self_delete -- --ignored --test-threads=1`

use sr_db::PgPool;
use sr_db::self_delete::{self, Refusal, Scope, Via};
use sr_db::testing::TestDb;

async fn exec(pool: &PgPool, sql: &str) {
    sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()))
        .execute(pool)
        .await
        .unwrap();
}

async fn scalar(pool: &PgPool, sql: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .fetch_one(pool)
        .await
        .unwrap()
}

const ALL: Scope = Scope {
    servers: true,
    dms: true,
    before_ms: None,
};

/// Account 1 wrote messages `base..base+5` in a server channel (10), a DM (11); account 2 wrote one.
async fn seed(pool: &PgPool) -> i64 {
    for id in [1, 2] {
        exec(pool, &format!("INSERT INTO persons (id) VALUES ({id})")).await;
        exec(
            pool,
            &format!(
                "INSERT INTO accounts (id, person_id, username, email, password_hash) \
                 VALUES ({id}, {id}, 'u{id}', 'u{id}@example.test', 'x')"
            ),
        )
        .await;
    }
    exec(
        pool,
        "INSERT INTO guilds (id, name, owner_id) VALUES (5, 'Room', 2)",
    )
    .await;
    exec(
        pool,
        "INSERT INTO channels (id, type, guild_id) VALUES (10, 0, 5)",
    )
    .await;
    exec(pool, "INSERT INTO channels (id, type) VALUES (11, 1)").await;
    exec(
        pool,
        "INSERT INTO moderation_holds (id, reason) VALUES (7, 'case')",
    )
    .await;
    let _: String = sqlx::query_scalar("SELECT sr_ensure_message_partition(CURRENT_DATE)")
        .fetch_one(pool)
        .await
        .unwrap();
    let base = scalar(
        pool,
        "SELECT sr_snowflake_floor(date_trunc('month', now())) + 1000",
    )
    .await;
    // base: server, base+1: server under a hold, base+2: DM, base+3: another author, base+4: already deleted.
    for (off, ch, author, extra) in [
        (0, 10, 1, ""),
        (1, 10, 1, ""),
        (2, 11, 1, ""),
        (3, 10, 2, ""),
        (4, 10, 1, ""),
    ] {
        exec(
            pool,
            &format!(
                "INSERT INTO messages (id, channel_id, author_id) VALUES ({}, {ch}, {author}) {extra}",
                base + off
            ),
        )
        .await;
    }
    exec(
        pool,
        &format!("UPDATE messages SET hold_id = 7 WHERE id = {}", base + 1),
    )
    .await;
    exec(
        pool,
        &format!(
            "UPDATE messages SET visibility = 'user_deleted', deleted_at = now() WHERE id = {}",
            base + 4
        ),
    )
    .await;
    base
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn counts_and_hides_only_the_members_visible_messages_and_never_purges_held_rows() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    let base = seed(pool).await;

    assert_eq!(self_delete::count_visible(pool, 1, ALL).await.unwrap(), 3);
    assert_eq!(
        self_delete::count_visible(pool, 1, Scope { dms: false, ..ALL })
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        self_delete::count_visible(
            pool,
            1,
            Scope {
                servers: false,
                ..ALL
            }
        )
        .await
        .unwrap(),
        1
    );

    // Servers only: the DM message stays visible.
    let hidden = self_delete::hide(pool, 1, Scope { dms: false, ..ALL }, 1_800_000_000_000)
        .await
        .unwrap();
    assert_eq!(hidden.len(), 1);
    assert_eq!((hidden[0].channel_id, hidden[0].guild_id), (10, Some(5)));
    assert_eq!(hidden[0].message_ids.len(), 2);
    let vis = |id: i64| {
        format!("SELECT count(*) FROM messages WHERE id = {id} AND visibility = 'visible'")
    };
    assert_eq!(
        scalar(pool, &vis(base + 2)).await,
        1,
        "the DM is untouched without dms"
    );
    assert_eq!(
        scalar(pool, &vis(base + 3)).await,
        1,
        "another author's message is untouched"
    );

    // The plain row is purge-dated, the held row is hidden but never purge-dated (D-122).
    assert_eq!(
        scalar(
            pool,
            &format!("SELECT count(*) FROM messages WHERE id = {base} AND purge_after IS NOT NULL")
        )
        .await,
        1
    );
    assert_eq!(
        scalar(
            pool,
            &format!(
                "SELECT count(*) FROM messages WHERE id = {} AND visibility = 'user_deleted' AND purge_after IS NULL AND hold_id = 7",
                base + 1
            )
        )
        .await,
        1
    );

    // Now the DMs.
    let hidden = self_delete::hide(pool, 1, ALL, 1_800_000_000_000)
        .await
        .unwrap();
    assert_eq!(hidden.len(), 1);
    assert_eq!((hidden[0].channel_id, hidden[0].guild_id), (11, None));
    assert_eq!(self_delete::count_visible(pool, 1, ALL).await.unwrap(), 0);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_cutoff_keeps_newer_messages() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    seed(pool).await;
    // A cutoff in the past hides nothing: every seeded message is from this month.
    let past = Scope {
        before_ms: Some(1_000_000_000_000),
        ..ALL
    };
    assert_eq!(self_delete::count_visible(pool, 1, past).await.unwrap(), 0);
    let future = Scope {
        before_ms: Some(
            i64::try_from(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis(),
            )
            .unwrap()
                + 86_400_000,
        ),
        ..ALL
    };
    assert_eq!(
        self_delete::count_visible(pool, 1, future).await.unwrap(),
        3
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn one_run_at_a_time_and_once_per_24_hours_for_members() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    seed(pool).await;

    let first = self_delete::request(pool, 100, 101, 1, ALL, Via::Member)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.state, "queued");
    let jobs = scalar(
        pool,
        "SELECT count(*) FROM jobs WHERE kind = 'user.messages_self_delete'",
    )
    .await;
    assert_eq!(jobs, 1);
    // A run is open: refused, even for staff.
    for via in [Via::Member, Via::Staff] {
        assert!(matches!(
            self_delete::request(pool, 102, 103, 1, ALL, via)
                .await
                .unwrap(),
            Err(Refusal::InProgress(_))
        ));
    }
    // Done: a member run within 24 hours is refused with a retry time, a staff run is not.
    assert!(self_delete::begin(pool, 100).await.unwrap().is_some());
    self_delete::finish(pool, 100, 3).await.unwrap();
    match self_delete::request(pool, 104, 105, 1, ALL, Via::Member)
        .await
        .unwrap()
    {
        Err(Refusal::TooSoon { retry_after_secs }) => {
            assert!((1..=86_400).contains(&retry_after_secs));
        }
        other => panic!("{other:?}"),
    }
    let staff = self_delete::request(pool, 106, 107, 1, ALL, Via::Staff)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(staff.via, "staff");
    self_delete::fail(pool, 106, "boom").await.unwrap();
    // A failed run does not count; the 24 hours still run from the finished member run.
    assert!(matches!(
        self_delete::request(pool, 108, 109, 1, ALL, Via::Member)
            .await
            .unwrap(),
        Err(Refusal::TooSoon { .. })
    ));
    // After the window the member may run again.
    exec(
        pool,
        "UPDATE message_self_deletes SET requested_at = now() - interval '25 hours' WHERE id = 100",
    )
    .await;
    assert!(
        self_delete::request(pool, 110, 111, 1, ALL, Via::Member)
            .await
            .unwrap()
            .is_ok()
    );
    // Another account is not affected.
    assert!(
        self_delete::request(pool, 112, 113, 2, ALL, Via::Member)
            .await
            .unwrap()
            .is_ok()
    );
    let latest = self_delete::latest(pool, 1).await.unwrap().unwrap();
    assert_eq!(latest.id, 110);
    // A run must cover servers or direct messages.
    assert!(
        sqlx::query("INSERT INTO message_self_deletes (id, account_id, servers, dms) VALUES (120, 2, false, false)")
            .execute(pool)
            .await
            .is_err()
    );
}
