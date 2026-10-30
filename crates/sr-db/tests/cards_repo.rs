//! Postgres tests for the invite cards (UTL-CRD-001, migration 1670): the seat cap, a double claim, unclaim,
//! the last seat under concurrency, the reminder bookkeeping, the start and close passes and the channel cap.
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test cards_repo -- --include-ignored --test-threads=1`

use sqlx::PgPool;
use sr_chat::cards::reminders::Window;
use sr_chat::cards::{
    CLOSE_AFTER_START_MS, Card, CardError, CardStore, ClaimOutcome, MAX_OPEN_PER_CHANNEL,
};
use sr_db::cards_repo::PgCards;
use sr_db::testing::TestDb;

const T0: i64 = 1_800_000_000_000;
const HOUR: i64 = 3_600_000;

async fn account(pool: &PgPool, id: i64) {
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $1, $2, $3, 'x')",
    )
    .bind(id)
    .bind(format!("user{id}"))
    .bind(format!("user{id}@example.test"))
    .execute(pool)
    .await
    .unwrap();
}

/// Two channels and `users` accounts with ids 1000.. .
async fn world(users: i64) -> (TestDb, PgCards) {
    let db = TestDb::new().await.expect("db");
    for ch in [11, 12] {
        sqlx::query("INSERT INTO channels (id, type) VALUES ($1, 1)")
            .bind(ch)
            .execute(db.pool())
            .await
            .unwrap();
    }
    for u in 0..users {
        account(db.pool(), 1000 + u).await;
    }
    let store = PgCards::new(db.pool().clone());
    (db, store)
}

fn card(id: u64, channel: u64, seats: i32, starts_ms: i64) -> Card {
    Card {
        id,
        channel_id: channel,
        guild_id: None,
        kind: "game_night".into(),
        title: "Catan".into(),
        note: String::new(),
        starts_ms,
        duration_min: 120,
        timezone: "Europe/Berlin".into(),
        seats,
        title_id: None,
        link_path: None,
        created_by: Some(1000),
        created_ms: T0,
        canceled_ms: None,
        started_ms: None,
        closed_ms: None,
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_card_round_trips() {
    let (_db, s) = world(1).await;
    let mut c = card(1, 11, 4, T0 + 2 * HOUR);
    c.title_id = Some(77);
    c.link_path = Some("/cinema/title/77".into());
    c.kind = "cinema".into();
    s.create(&c).await.unwrap();
    assert_eq!(s.get(11, 1).await.unwrap(), Some(c.clone()));
    // Only through its own channel.
    assert_eq!(s.get(12, 1).await.unwrap(), None);
    assert_eq!(s.get_any(1).await.unwrap(), Some(c));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_seat_cap_a_double_claim_and_unclaim() {
    let (_db, s) = world(4).await;
    s.create(&card(1, 11, 2, T0 + 2 * HOUR)).await.unwrap();
    let (a, b, c) = (1000, 1001, 1002);
    assert_eq!(
        s.claim(1, a, true, T0).await.unwrap(),
        ClaimOutcome::Claimed
    );
    // A double claim keeps one seat and only updates the reminder choice.
    assert_eq!(
        s.claim(1, a, false, T0 + 1).await.unwrap(),
        ClaimOutcome::AlreadyClaimed
    );
    let claims = s.claims(1).await.unwrap();
    assert_eq!(claims.len(), 1);
    assert!(!claims[0].remind);
    assert_eq!(
        s.claim(1, b, true, T0 + 2).await.unwrap(),
        ClaimOutcome::Claimed
    );
    // The cap: the third member is turned away, the first two keep their seats.
    assert_eq!(
        s.claim(1, c, true, T0 + 3).await.unwrap(),
        ClaimOutcome::Full
    );
    assert_eq!(s.claims(1).await.unwrap().len(), 2);
    // A full card still lets a seat holder re-claim (idempotent), not lose it.
    assert_eq!(
        s.claim(1, a, true, T0 + 4).await.unwrap(),
        ClaimOutcome::AlreadyClaimed
    );
    // Unclaim frees the seat for the next member; a second unclaim is a no-op.
    assert!(s.unclaim(1, a).await.unwrap());
    assert!(!s.unclaim(1, a).await.unwrap());
    assert_eq!(
        s.claim(1, c, true, T0 + 5).await.unwrap(),
        ClaimOutcome::Claimed
    );
    let order: Vec<u64> = s
        .claims(1)
        .await
        .unwrap()
        .iter()
        .map(|k| k.user_id)
        .collect();
    assert_eq!(order, [b, c]);
    // An unknown card is "closed", never a panic.
    assert_eq!(
        s.claim(99, a, true, T0).await.unwrap(),
        ClaimOutcome::Closed
    );
}

/// The last seat goes to exactly one of many concurrent callers, and the table never holds more rows than seats.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn concurrent_claims_for_the_last_seat_have_exactly_one_winner() {
    let (db, s) = world(40).await;
    s.create(&card(1, 11, 3, T0 + 2 * HOUR)).await.unwrap();
    // Two seats are already gone; 38 callers race for the last one.
    assert_eq!(
        s.claim(1, 1000, true, T0).await.unwrap(),
        ClaimOutcome::Claimed
    );
    assert_eq!(
        s.claim(1, 1001, true, T0).await.unwrap(),
        ClaimOutcome::Claimed
    );
    let mut tasks = Vec::new();
    for u in 2..40 {
        let s = s.clone();
        tasks.push(tokio::spawn(async move {
            s.claim(1, 1000 + u, true, T0 + 1).await.unwrap()
        }));
    }
    let mut won = 0;
    let mut full = 0;
    for t in tasks {
        match t.await.unwrap() {
            ClaimOutcome::Claimed => won += 1,
            ClaimOutcome::Full => full += 1,
            other => panic!("{other:?}"),
        }
    }
    assert_eq!((won, full), (1, 37));
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM invite_card_claims WHERE card_id = 1")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(rows, 3);
}

/// The same race from the very first seat, so a card never oversells however it is approached.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_crowd_never_oversells_a_card() {
    let (db, s) = world(30).await;
    s.create(&card(1, 11, 7, T0 + 2 * HOUR)).await.unwrap();
    let mut tasks = Vec::new();
    for u in 0..30 {
        let s = s.clone();
        tasks.push(tokio::spawn(async move {
            s.claim(1, 1000 + u, true, T0).await.unwrap()
        }));
    }
    let mut won = 0;
    for t in tasks {
        if t.await.unwrap() == ClaimOutcome::Claimed {
            won += 1;
        }
    }
    assert_eq!(won, 7);
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM invite_card_claims")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(rows, 7);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_canceled_card_takes_no_more_seats() {
    let (_db, s) = world(2).await;
    s.create(&card(1, 11, 5, T0 + 2 * HOUR)).await.unwrap();
    assert_eq!(
        s.claim(1, 1000, true, T0).await.unwrap(),
        ClaimOutcome::Claimed
    );
    // Wrong channel: nothing happens.
    assert!(!s.cancel(12, 1, T0 + 1).await.unwrap());
    assert!(s.cancel(11, 1, T0 + 1).await.unwrap());
    assert!(!s.cancel(11, 1, T0 + 2).await.unwrap());
    assert_eq!(
        s.claim(1, 1001, true, T0 + 3).await.unwrap(),
        ClaimOutcome::Closed
    );
    let c = s.get(11, 1).await.unwrap().unwrap();
    assert_eq!(c.canceled_ms, Some(T0 + 1));
    assert!(!c.is_open());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn deleting_the_channel_removes_cards_and_claims() {
    let (db, s) = world(1).await;
    s.create(&card(1, 11, 5, T0 + 2 * HOUR)).await.unwrap();
    s.claim(1, 1000, true, T0).await.unwrap();
    sqlx::query("DELETE FROM channels WHERE id = 11")
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(s.get_any(1).await.unwrap(), None);
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM invite_card_claims")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_channel_holds_a_bounded_number_of_open_cards() {
    let (_db, s) = world(1).await;
    for i in 0..MAX_OPEN_PER_CHANNEL as u64 {
        s.create(&card(100 + i, 11, 2, T0 + 2 * HOUR))
            .await
            .unwrap();
    }
    assert_eq!(
        s.create(&card(999, 11, 2, T0 + 2 * HOUR)).await,
        Err(CardError::TooMany)
    );
    // Another channel is unaffected, and canceling one makes room.
    s.create(&card(998, 12, 2, T0 + 2 * HOUR)).await.unwrap();
    assert!(s.cancel(11, 100, T0).await.unwrap());
    s.create(&card(999, 11, 2, T0 + 2 * HOUR)).await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn reminder_candidates_marks_and_unmarks() {
    let (_db, s) = world(3).await;
    let start = T0 + 3 * HOUR;
    s.create(&card(1, 11, 5, start)).await.unwrap();
    s.claim(1, 1000, true, T0).await.unwrap();
    s.claim(1, 1001, false, T0).await.unwrap(); // does not want reminders
    // Too early: the start is more than an hour away.
    assert!(
        s.reminder_candidates(T0 + HOUR, 50)
            .await
            .unwrap()
            .is_empty()
    );
    // Inside the hour: only the member who wants one.
    let now = start - HOUR + 1000;
    let due = s.reminder_candidates(now, 50).await.unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].1.user_id, 1000);
    assert!(!due[0].1.reminded_hour && !due[0].1.reminded_ten);
    // The mark is taken by one caller only.
    assert!(s.mark_reminded(1, 1000, Window::Hour, now).await.unwrap());
    assert!(!s.mark_reminded(1, 1000, Window::Hour, now).await.unwrap());
    let due = s.reminder_candidates(now, 50).await.unwrap();
    assert!(due[0].1.reminded_hour && !due[0].1.reminded_ten);
    // A failed delivery gives the window back.
    s.unmark_reminded(1, 1000, Window::Hour).await.unwrap();
    assert!(s.mark_reminded(1, 1000, Window::Hour, now).await.unwrap());
    // The ten-minute mark also covers the hour window.
    s.unmark_reminded(1, 1000, Window::Hour).await.unwrap();
    assert!(
        s.mark_reminded(1, 1000, Window::TenMinutes, now)
            .await
            .unwrap()
    );
    // Both windows are covered now, so the claim is no longer a candidate.
    // After the start nothing is owed either.
    assert!(s.reminder_candidates(now, 50).await.unwrap().is_empty());
    assert!(
        s.reminder_candidates(start + 1, 50)
            .await
            .unwrap()
            .is_empty()
    );
    // A canceled card owes nothing.
    s.create(&card(2, 11, 5, start)).await.unwrap();
    s.claim(2, 1002, true, T0).await.unwrap();
    assert_eq!(s.reminder_candidates(now, 50).await.unwrap().len(), 1);
    s.cancel(11, 2, now).await.unwrap();
    assert!(s.reminder_candidates(now, 50).await.unwrap().is_empty());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn cards_start_and_close_once() {
    let (_db, s) = world(1).await;
    s.create(&card(1, 11, 5, T0 + HOUR)).await.unwrap();
    s.create(&card(2, 11, 5, T0 + 5 * HOUR)).await.unwrap();
    // Before the start nothing is due.
    assert!(s.take_started(T0, 10).await.unwrap().is_empty());
    let started = s.take_started(T0 + HOUR + 1, 10).await.unwrap();
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].id, 1);
    assert_eq!(started[0].started_ms, Some(T0 + HOUR + 1));
    // The second pass finds nothing: a card starts once.
    assert!(s.take_started(T0 + HOUR + 2, 10).await.unwrap().is_empty());
    // Closing waits for the grace period after the start.
    assert!(
        s.take_closed(T0 + HOUR + CLOSE_AFTER_START_MS - 1, 10)
            .await
            .unwrap()
            .is_empty()
    );
    let closed = s
        .take_closed(T0 + HOUR + CLOSE_AFTER_START_MS, 10)
        .await
        .unwrap();
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].id, 1);
    assert!(closed[0].closed_ms.is_some());
    assert!(!closed[0].is_open());
    assert!(
        s.take_closed(T0 + HOUR + CLOSE_AFTER_START_MS + 1, 10)
            .await
            .unwrap()
            .is_empty()
    );
    // A closed card takes no seats.
    assert_eq!(
        s.claim(1, 1000, true, T0).await.unwrap(),
        ClaimOutcome::Closed
    );
    // The second card was never touched.
    let c2 = s.get(11, 2).await.unwrap().unwrap();
    assert_eq!((c2.started_ms, c2.closed_ms), (None, None));
}
