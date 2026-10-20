//! A held library or cinema row cannot be deleted, and the refusal names the takedown notice (D-335 item 1).
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test hold_guard -- --include-ignored --test-threads=1`

use sr_cinema::{CinemaError, CinemaRepository};
use sr_db::{cinema_repo::PgCinemaRepo, library_repo::PgLibraryRepo, testing::TestDb};
use sr_library::{LibraryError, LibraryRepository};

async fn exec(pool: &sqlx::PgPool, sql: &'static str, binds: &[i64]) {
    let mut q = sqlx::query(sql);
    for b in binds {
        q = q.bind(*b);
    }
    q.execute(pool).await.unwrap();
}

/// A case, a takedown notice for it and a hold on that case; returns the hold id.
async fn hold_with_notice(pool: &sqlx::PgPool) -> i64 {
    exec(pool, "INSERT INTO persons (id) VALUES (1)", &[]).await;
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES (1, 1, 'staff', 's@example.test', 'x')",
    )
    .execute(pool)
    .await
    .unwrap();
    exec(
        pool,
        "INSERT INTO moderation_cases (id, category, routing, target_kind, opened_by_kind) \
         VALUES (50, 'illegal_content', 'platform', 'other', 'staff')",
        &[],
    )
    .await;
    sqlx::query(
        "INSERT INTO takedown_notices (id, kind, notifier_name, notifier_contact, legal_basis, original_text, \
         received_at, entered_by, triage_due_at, case_id) \
         VALUES (77, 'copyright', 'n', 'c', 'b', 't', now(), 1, now(), 50)",
    )
    .execute(pool)
    .await
    .unwrap();
    exec(
        pool,
        "INSERT INTO moderation_holds (id, reason, case_id) VALUES (60, 'evidence', 50)",
        &[],
    )
    .await;
    60
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn deleting_a_held_series_or_chapter_is_a_conflict_that_names_the_notice() {
    let db = TestDb::new().await.expect("test db");
    let pool = db.pool().clone();
    let hold = hold_with_notice(&pool).await;
    exec(
        &pool,
        "INSERT INTO reading_series (id, kind, title, hold_id) VALUES (1, 'manga', 'S', $1)",
        &[hold],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO reading_editions (id, series_id, language, label) VALUES (2, 1, 'en', 'E')",
        &[],
    )
    .await;
    exec(&pool, "INSERT INTO reading_chapters (id, series_id, edition_id, number, hold_id) VALUES (3, 1, 2, 1, $1)", &[hold]).await;
    let repo = PgLibraryRepo::new(pool.clone());
    let LibraryError::Conflict(m) = repo.delete_series(1).await.unwrap_err() else {
        panic!("a conflict")
    };
    assert!(
        m.contains("notice TK-77") && m.contains("Release the hold first"),
        "{m}"
    );
    let LibraryError::Conflict(m) = repo.delete_chapter(3).await.unwrap_err() else {
        panic!("a conflict")
    };
    assert!(m.contains("TK-77"), "{m}");
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM reading_series WHERE id = 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 1, "the row is still there");
    // An unheld series is still deletable.
    exec(
        &pool,
        "INSERT INTO reading_series (id, kind, title) VALUES (9, 'manga', 'Free')",
        &[],
    )
    .await;
    assert!(repo.delete_series(9).await.unwrap());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn deleting_a_held_title_or_episode_is_a_conflict_that_names_the_notice() {
    let db = TestDb::new().await.expect("test db");
    let pool = db.pool().clone();
    let hold = hold_with_notice(&pool).await;
    exec(&pool, "INSERT INTO video_titles (id, kind, title, sort_title, hold_id) VALUES (1, 'series', 'T', 't', $1)", &[hold]).await;
    exec(
        &pool,
        "INSERT INTO video_seasons (id, title_id, number) VALUES (2, 1, 1)",
        &[],
    )
    .await;
    exec(&pool, "INSERT INTO video_episodes (id, title_id, season_id, number, hold_id) VALUES (3, 1, 2, 1, $1)", &[hold]).await;
    let repo = PgCinemaRepo::new(pool.clone());
    let CinemaError::Conflict(m) = repo.delete_title(1).await.unwrap_err() else {
        panic!("a conflict")
    };
    assert!(m.contains("notice TK-77"), "{m}");
    let CinemaError::Conflict(m) = repo.delete_episode(3).await.unwrap_err() else {
        panic!("a conflict")
    };
    assert!(m.contains("TK-77"), "{m}");
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM video_titles WHERE id = 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 1);
}
