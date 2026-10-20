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
        .unwrap()
        .hidden;
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
        .unwrap()
        .hidden;
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

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn reactions_and_poll_votes_go_with_the_run_closed_polls_keep_an_anonymous_tally_and_holds_stay()
 {
    use sr_chat::rich_repo::RichStore;
    use sr_core::MessageId;
    use sr_db::rich_repo::PgRichStore;
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    let base = seed(pool).await;
    // Polls by another author: open (+10), closed (+11), under a hold (+12).
    let now_ms = 1_800_000_000_000_i64;
    for (off, poll, hold) in [
        (
            10,
            format!(r#"{{"finalized":false,"expiry_ms":{}}}"#, now_ms * 2),
            "NULL",
        ),
        (11, r#"{"finalized":true}"#.to_owned(), "NULL"),
        (12, r#"{"finalized":true}"#.to_owned(), "7"),
    ] {
        exec(
            pool,
            &format!(
                "INSERT INTO messages (id, channel_id, author_id, poll, hold_id) VALUES ({}, 10, 2, '{poll}'::jsonb, {hold})",
                base + off
            ),
        )
        .await;
    }
    // Reactions of account 1: two on a plain server message, one on a held message of theirs, one in the DM.
    for (msg, emoji) in [
        (base + 3, "a"),
        (base + 3, "b"),
        (base + 1, "a"),
        (base + 2, "a"),
    ] {
        exec(
            pool,
            &format!("INSERT INTO reactions (message_id, emoji_key, user_id) VALUES ({msg}, '{emoji}', 1)"),
        )
        .await;
    }
    // Votes: account 1 on the open, closed and held polls; account 2 on the closed poll too.
    for (msg, answer, user) in [(10, 1, 1), (11, 2, 1), (12, 1, 1), (11, 2, 2)] {
        exec(
            pool,
            &format!("INSERT INTO poll_votes (message_id, answer_id, user_id) VALUES ({}, {answer}, {user})", base + msg),
        )
        .await;
    }

    let servers = Scope { dms: false, ..ALL };
    let c = self_delete::counts(pool, 1, servers).await.unwrap();
    assert_eq!(
        (c.messages, c.reactions, c.poll_votes),
        (2, 2, 2),
        "held rows are not counted"
    );
    let all = self_delete::counts(pool, 1, ALL).await.unwrap();
    assert_eq!((all.reactions, all.poll_votes), (3, 2));

    let out = self_delete::hide(pool, 1, servers, now_ms).await.unwrap();
    assert_eq!(out.poll_votes, 2);
    assert_eq!(out.reactions.len(), 2);
    assert!(
        out.reactions
            .iter()
            .all(|r| r.channel_id == 10 && r.guild_id == Some(5) && r.message_id == base + 3)
    );
    let n = |sql: String| async move { scalar(pool, &sql).await };
    // Held message's reaction and held poll's vote stay; the DM reaction stays without dms.
    assert_eq!(
        n(format!(
            "SELECT count(*) FROM reactions WHERE user_id = 1 AND message_id IN ({}, {})",
            base + 1,
            base + 2
        ))
        .await,
        2
    );
    assert_eq!(
        n(format!(
            "SELECT count(*) FROM poll_votes WHERE user_id = 1 AND message_id = {}",
            base + 12
        ))
        .await,
        1
    );
    assert_eq!(
        n(format!(
            "SELECT count(*) FROM poll_votes WHERE user_id = 1 AND message_id IN ({}, {})",
            base + 10,
            base + 11
        ))
        .await,
        0
    );
    // The closed poll's result does not move; the voter is gone.
    let rich = PgRichStore::new(pool.clone());
    let t = rich
        .poll_tallies(&[MessageId::new(u64::try_from(base + 11).unwrap())], None)
        .await
        .unwrap();
    let tally = t.values().next().unwrap();
    assert_eq!(
        tally.counts.get(&2),
        Some(&2),
        "one named vote plus one anonymous"
    );
    let anon: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT poll->'anonymous_counts'->>'2' FROM messages WHERE id = {}",
        base + 11
    )))
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(anon.as_deref(), Some("1"));
    // The open poll just lost the vote.
    let open = rich
        .poll_tallies(&[MessageId::new(u64::try_from(base + 10).unwrap())], None)
        .await
        .unwrap();
    assert!(open.is_empty());

    // Direct messages only with dms; the cut-off applies to the reaction time.
    let out = self_delete::hide(pool, 1, ALL, now_ms).await.unwrap();
    assert_eq!(out.reactions.len(), 1);
    assert_eq!(out.reactions[0].guild_id, None);
    let old = Scope {
        before_ms: Some(1_000_000_000_000),
        ..ALL
    };
    assert_eq!(
        self_delete::counts(pool, 1, old).await.unwrap().reactions,
        0
    );
}
