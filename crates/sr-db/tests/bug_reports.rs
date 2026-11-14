//! DB-backed tests of the beta bug report store (SEC-AUD-166 and 167, D-650). Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test bug_reports -- --ignored`

use serde_json::json;
use sqlx::PgPool;
use sr_db::account_deletion::delete_account;
use sr_db::bug_reports::{NewReport, PgBugReports, Purged, REPORT_DAYS, SCREENSHOT_DAYS};
use sr_db::testing::TestDb;

const JPEG: [u8; 6] = [0xFF, 0xD8, 0xFF, 0xE0, 0xFF, 0xD9];

async fn account(pool: &PgPool, id: i64, name: &str) {
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $1, $2, $3, 'phc')",
    )
    .bind(id)
    .bind(name)
    .bind(format!("{name}@example.test"))
    .execute(pool)
    .await
    .unwrap();
}

async fn add(store: &PgBugReports, who: i64, text: &str, picture: bool) -> i64 {
    let bundle = json!({ "commit": "abcdef1", "viewport": { "w": 800, "h": 600 } });
    store
        .insert(&NewReport {
            reporter_id: who,
            commit_sha: "abcdef1",
            route: "lounge",
            description: text,
            bundle: &bundle,
            screenshot: picture.then_some(JPEG.as_slice()),
        })
        .await
        .unwrap()
}

async fn age(pool: &PgPool, id: i64, days: i32) {
    sqlx::query(
        "UPDATE bug_reports SET created_at = now() - make_interval(days => $2) WHERE id = $1",
    )
    .bind(id)
    .bind(days)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_report_round_trips_and_the_list_carries_no_description_and_no_picture() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    account(pool, 1, "reporter").await;
    account(pool, 2, "staff").await;
    let store = PgBugReports::new(pool.clone());
    let with = add(&store, 1, "the send button does nothing", true).await;
    let without = add(&store, 1, "no picture here", false).await;

    let (rows, open) = store.list(None, None, 50).await.unwrap();
    assert_eq!(open, 2);
    assert_eq!(
        rows.iter().map(|r| r.id).collect::<Vec<_>>(),
        [without, with]
    );
    for r in &rows {
        assert!(
            r.description.is_none() && r.bundle.is_none(),
            "a list row is light"
        );
        assert_eq!(r.reporter.username, "reporter");
        assert_eq!(r.status, "open");
    }
    assert_eq!(
        rows.iter().map(|r| r.has_screenshot).collect::<Vec<_>>(),
        [false, true]
    );

    let one = store.get(with).await.unwrap().unwrap();
    assert_eq!(
        one.description.as_deref(),
        Some("the send button does nothing")
    );
    assert_eq!(one.bundle.as_ref().unwrap()["viewport"]["w"], 800);
    assert_eq!(
        store.screenshot(with).await.unwrap().as_deref(),
        Some(JPEG.as_slice())
    );
    assert_eq!(store.screenshot(without).await.unwrap(), None);
    assert_eq!(store.screenshot(999).await.unwrap(), None);
    assert!(store.get(999).await.unwrap().is_none());

    // The table refuses what the allow-list should never send.
    for (what, set) in [
        ("an empty description", "description = ''"),
        (
            "a description over 2000 characters",
            "description = repeat('x', 2001)",
        ),
        ("an unknown status", "status = 'wontfix'"),
        (
            "a picture over 2 MiB",
            "screenshot = decode(repeat('ff', 2097153), 'hex')",
        ),
    ] {
        let sql = format!("UPDATE bug_reports SET {set} WHERE id = {with}");
        let r = sqlx::query(sqlx::AssertSqlSafe(sql)).execute(pool).await;
        assert!(r.is_err(), "{what}");
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn closing_a_report_deletes_its_picture_and_is_idempotent() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    account(pool, 1, "reporter").await;
    account(pool, 2, "first").await;
    account(pool, 3, "second").await;
    let store = PgBugReports::new(pool.clone());
    let id = add(&store, 1, "broken", true).await;
    assert!(store.close(id, 2).await.unwrap());
    let r = store.get(id).await.unwrap().unwrap();
    assert_eq!(r.status, "closed");
    assert!(!r.has_screenshot);
    assert_eq!(store.screenshot(id).await.unwrap(), None);
    assert_eq!(
        r.closed_by.as_ref().map(|p| p.username.as_str()),
        Some("first")
    );
    let first_at = r.closed_ms.unwrap();
    // A second close, by somebody else, changes nothing.
    assert!(store.close(id, 3).await.unwrap());
    let r = store.get(id).await.unwrap().unwrap();
    assert_eq!(r.closed_by.unwrap().username, "first");
    assert_eq!(r.closed_ms, Some(first_at));
    assert!(!store.close(999, 2).await.unwrap(), "no such report");
    // The open count and the status filter follow.
    let (open, n) = store.list(Some("open"), None, 50).await.unwrap();
    assert!(open.is_empty());
    assert_eq!(n, 0);
    assert_eq!(
        store.list(Some("closed"), None, 50).await.unwrap().0.len(),
        1
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn retention_drops_pictures_after_30_days_and_reports_after_90() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    account(pool, 1, "reporter").await;
    let store = PgBugReports::new(pool.clone());
    let fresh = add(&store, 1, "today", true).await;
    let d29 = add(&store, 1, "29 days", true).await;
    let d31 = add(&store, 1, "31 days", true).await;
    let d89 = add(&store, 1, "89 days", true).await;
    let d91 = add(&store, 1, "91 days", true).await;
    for (id, days) in [(d29, 29), (d31, 31), (d89, 89), (d91, 91)] {
        age(pool, id, days).await;
    }
    assert_eq!((SCREENSHOT_DAYS, REPORT_DAYS), (30, 90));
    let p = store.purge(SCREENSHOT_DAYS, REPORT_DAYS).await.unwrap();
    // Pictures: the 31, 89 and 91 days old (the 91 days old row goes next, but loses its picture first).
    assert_eq!(
        p,
        Purged {
            pictures: 3,
            reports: 1
        }
    );
    for (id, picture) in [(fresh, true), (d29, true), (d31, false), (d89, false)] {
        let r = store
            .get(id)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("{id} is kept"));
        assert_eq!(r.has_screenshot, picture, "{id}");
        assert_eq!(
            store.screenshot(id).await.unwrap().is_some(),
            picture,
            "{id}"
        );
        // What the picture was attached to stays readable.
        assert!(r.description.is_some());
    }
    assert!(
        store.get(d91).await.unwrap().is_none(),
        "past 90 days the report goes"
    );
    // A second pass has nothing to do.
    assert_eq!(
        store.purge(SCREENSHOT_DAYS, REPORT_DAYS).await.unwrap(),
        Purged::default()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn erasing_an_account_deletes_its_bug_reports() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    account(pool, 1, "leaving").await;
    account(pool, 2, "staying").await;
    let store = PgBugReports::new(pool.clone());
    add(&store, 1, "mine", true).await;
    add(&store, 1, "also mine", false).await;
    let kept = add(&store, 2, "not mine", true).await;
    let report = delete_account(pool, 1, 9001).await.unwrap().unwrap();
    assert!(report.personal_rows_deleted >= 2);
    let left: Vec<i64> = sqlx::query_scalar("SELECT id FROM bug_reports ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap();
    assert_eq!(left, [kept], "only the other account's report is left");
    // An erased reporter cannot appear in the staff list.
    let (rows, _) = store.list(None, None, 50).await.unwrap();
    assert!(rows.iter().all(|r| r.reporter.id == 2));
}

/// SEC-AUD-267: before this, the list had a limit and no cursor, so with 60 reports the oldest ten could not be reached
/// at a page of 50 (and nothing past 200 at the largest limit). Walking with `before` reaches every report once.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_list_pages_with_before_and_reaches_every_report_once() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    account(pool, 1, "reporter").await;
    account(pool, 2, "staff").await;
    let store = PgBugReports::new(pool.clone());
    let mut ids = Vec::new();
    for i in 0..60 {
        ids.push(add(&store, 1, &format!("report {i}"), i % 7 == 0).await);
    }
    // Close every third one so the status filter has two sets to walk.
    for id in ids.iter().step_by(3) {
        store.close(*id, 2).await.unwrap();
    }
    // A page of 50 stops short of the oldest ten...
    let (first, _) = store.list(None, None, 50).await.unwrap();
    assert_eq!(first.len(), 50);
    assert_eq!(first.last().map(|r| r.id), Some(ids[10]));

    async fn walk(store: &PgBugReports, status: Option<&str>, page: i64) -> Vec<i64> {
        let mut seen = Vec::new();
        let mut before = None;
        for _ in 0..10 {
            let (rows, _) = store.list(status, before, page).await.unwrap();
            if rows.is_empty() {
                break;
            }
            before = rows.last().map(|r| r.id);
            seen.extend(rows.iter().map(|r| r.id));
        }
        seen
    }
    // ...and three pages of 25 (25, 25, 10) reach all 60, newest first, none twice.
    let all = walk(&store, None, 25).await;
    let mut expect: Vec<i64> = ids.clone();
    expect.reverse();
    assert_eq!(all, expect);
    // With a status filter too.
    let closed = walk(&store, Some("closed"), 7).await;
    let open = walk(&store, Some("open"), 7).await;
    let want_closed: Vec<i64> = expect
        .iter()
        .copied()
        .filter(|id| (ids.iter().position(|x| x == id).unwrap()) % 3 == 0)
        .collect();
    assert_eq!(closed, want_closed);
    assert_eq!(open.len() + closed.len(), 60);
    assert!(open.iter().all(|id| !closed.contains(id)));
    // The page after the oldest report is empty, and one more than shown can be asked for.
    assert!(
        store
            .list(None, Some(ids[0]), 50)
            .await
            .unwrap()
            .0
            .is_empty()
    );
    assert_eq!(store.list(None, None, 201).await.unwrap().0.len(), 60);
    assert!(store.exists(ids[0]).await.unwrap());
    assert!(!store.exists(ids[59] + 1000).await.unwrap());
}
