//! Postgres tests for the catalog indexer's store (BYOS-CAT-001, migration 1450).
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test index_repo -- --include-ignored --test-threads=1`

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use sqlx::Row;
use sr_db::index_repo::PgIndexStore;
use sr_db::testing::TestDb;
use sr_storage::index::{
    IndexStore, NameParser, Options, RunReport, SidecarMap, VideoFacts, classify_all,
};

/// `Show S01E02.mkv` style names only; the real parser is exercised in sr-server.
struct Fake;
impl NameParser for Fake {
    fn video(&self, path: &str) -> VideoFacts {
        let file = path.rsplit('/').next().unwrap_or(path);
        let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
        let mut f = VideoFacts {
            title: stem.to_owned(),
            ..VideoFacts::default()
        };
        if let Some(p) = stem.find(" S01E") {
            f.episode = true;
            f.title = stem[..p].to_owned();
            f.season = Some(1);
            f.episodes = vec![stem[p + 5..p + 7].parse().unwrap_or(1)];
        }
        f
    }
}

fn store(db: &TestDb) -> PgIndexStore {
    let n = Arc::new(AtomicI64::new(900_000));
    PgIndexStore::new(
        db.pool().clone(),
        Arc::new(move || n.fetch_add(1, Ordering::SeqCst)),
    )
}

fn listing(keys: &[&str]) -> Vec<(String, u64)> {
    keys.iter().map(|k| ((*k).to_owned(), 100)).collect()
}

async fn run(s: &PgIndexStore, mount: i64, keys: &[&str], complete: bool) -> (u64, u64) {
    let r = s.begin(mount, 0).await.unwrap();
    let all: Vec<_> = classify_all(
        &listing(keys),
        &Fake,
        &SidecarMap::new(),
        Options::default(),
    )
    .into_iter()
    .filter(|c| !c.ignored && c.class.kind().is_some())
    .collect();
    let added = s.apply(mount, r, &all).await.unwrap();
    let gone = s
        .finish(mount, r, &RunReport::default(), complete, None, 0)
        .await
        .unwrap();
    (added, gone)
}

async fn count(db: &TestDb, sql: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .fetch_one(db.pool())
        .await
        .unwrap()
}

const TREE: &[&str] = &[
    "Movies/Alien (1979)/Alien.mp4",
    "Movies/Alien (1979)/poster.jpg",
    "Series/Show/Show S01E01.mp4",
    "Series/Show/Show S01E02.mp4",
    "Anime/Cowboy/Cowboy S01E01.mkv",
    "Manga/Berserk/Vol 01.cbz",
    "Manga/Berserk/Vol 02.cbz",
    "Books/Dune.epub",
    "Music/Daft Punk/Discovery (2001)/01 - One More Time.flac",
    "Music/Daft Punk/Discovery (2001)/02 - Aerodynamic.flac",
    "Notes/readme.txt",
];

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn index_builds_catalog_rows_hidden_from_member_queries() {
    let db = TestDb::new().await.expect("db");
    let s = store(&db);
    let (added, gone) = run(&s, 7, TREE, true).await;
    assert_eq!((added, gone), (9, 0));

    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM storage_objects WHERE mount_id = 7"
        )
        .await,
        9
    );
    // Films, series and anime: one title each, episodes under them.
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM video_titles WHERE source = 'mount' AND mount_id = 7"
        )
        .await,
        3
    );
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM video_episodes WHERE source = 'mount' AND mount_id = 7"
        )
        .await,
        3
    );
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM video_titles WHERE kind = 'anime' AND 'anime' = ANY(tags)"
        )
        .await,
        1
    );
    let poster: Option<String> = sqlx::query_scalar(
        "SELECT poster_key FROM video_titles WHERE shelf_key = 'Movies/Alien (1979)/Alien.mp4'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(poster.as_deref(), Some("Movies/Alien (1979)/poster.jpg"));
    // Library: Berserk (2 chapters, manga) and Dune (a book).
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM reading_series WHERE source = 'mount'"
        )
        .await,
        2
    );
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM reading_chapters WHERE source = 'mount'"
        )
        .await,
        3
    );
    let fmt: String = sqlx::query_scalar(
        "SELECT format FROM reading_chapters WHERE source_key = 'Manga/Berserk/Vol 01.cbz'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(fmt, "images");
    assert_eq!(
        count(&db, "SELECT count(*) FROM music_tracks WHERE mount_id = 7").await,
        2
    );
    // No member query can see any of it: nothing is published.
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM video_titles WHERE published AND source = 'mount'"
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM reading_series WHERE published AND source = 'mount'"
        )
        .await,
        0
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn reindex_is_idempotent_and_keeps_overrides() {
    let db = TestDb::new().await.expect("db");
    let s = store(&db);
    run(&s, 7, TREE, true).await;
    // The member renames the show and retitles a track.
    sqlx::query(
        "UPDATE video_titles SET title = 'My Show', overridden = ARRAY['title'] WHERE shelf_key = 'series/Show'",
    )
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "UPDATE music_tracks SET title = 'Fixed', overridden = ARRAY['title'] WHERE track_no = 1",
    )
    .execute(db.pool())
    .await
    .unwrap();

    let (added, gone) = run(&s, 7, TREE, true).await;
    assert_eq!((added, gone), (0, 0), "nothing changed, nothing added");
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM video_titles WHERE source = 'mount'"
        )
        .await,
        3
    );
    let t: String =
        sqlx::query_scalar("SELECT title FROM video_titles WHERE shelf_key = 'series/Show'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(t, "My Show");
    let tr: String = sqlx::query_scalar("SELECT title FROM music_tracks WHERE track_no = 1")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(tr, "Fixed");
    // An unedited field is still rewritten from the name.
    let other: String = sqlx::query_scalar("SELECT artist FROM music_tracks WHERE track_no = 1")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(other, "Daft Punk");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_vanished_object_hides_its_rows_and_comes_back_with_its_edits() {
    let db = TestDb::new().await.expect("db");
    let s = store(&db);
    run(&s, 7, TREE, true).await;
    sqlx::query(
        "UPDATE music_tracks SET title = 'Fixed', overridden = ARRAY['title'] WHERE track_no = 2",
    )
    .execute(db.pool())
    .await
    .unwrap();

    // The second track, the whole anime and the book disappear from the bucket.
    let kept: Vec<&str> = TREE
        .iter()
        .copied()
        .filter(|k| {
            !k.contains("Aerodynamic") && !k.starts_with("Anime/") && !k.starts_with("Books/")
        })
        .collect();
    let (_, gone) = run(&s, 7, &kept, true).await;
    assert_eq!(gone, 3);
    let listed = |t: &str| format!("SELECT count(*) FROM {t} WHERE shelf_state = 'listed'");
    assert_eq!(count(&db, &listed("music_tracks")).await, 1);
    assert_eq!(count(&db, &listed("video_episodes")).await, 2);
    // The anime title lost its last episode and is hidden; the book series likewise.
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM video_titles WHERE kind = 'anime' AND shelf_state = 'listed'"
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM reading_series WHERE kind = 'book' AND shelf_state = 'listed'"
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM storage_objects WHERE state = 'vanished'"
        )
        .await,
        3
    );

    // A partial listing hides nothing.
    let (_, gone) = run(&s, 7, &kept[..2], false).await;
    assert_eq!(gone, 0);
    assert_eq!(count(&db, &listed("music_tracks")).await, 1);

    // The files come back: rows are listed again, the edit is still there, no duplicate rows.
    run(&s, 7, TREE, true).await;
    assert_eq!(count(&db, &listed("music_tracks")).await, 2);
    assert_eq!(count(&db, "SELECT count(*) FROM music_tracks").await, 2);
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM storage_objects WHERE state = 'vanished'"
        )
        .await,
        0
    );
    let tr: String = sqlx::query_scalar("SELECT title FROM music_tracks WHERE track_no = 2")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(tr, "Fixed");
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM video_titles WHERE kind = 'anime' AND shelf_state = 'listed'"
        )
        .await,
        1
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn one_run_at_a_time_and_mounts_do_not_mix() {
    let db = TestDb::new().await.expect("db");
    let s = store(&db);
    let r = s.begin(7, 0).await.unwrap();
    assert!(
        s.begin(7, 0).await.is_err(),
        "a second run on the same mount is refused"
    );
    assert!(s.begin(8, 0).await.is_ok(), "another mount is independent");
    s.finish(7, r, &RunReport::default(), true, None, 0)
        .await
        .unwrap();
    assert!(s.begin(7, 0).await.is_ok());

    // Same names on two mounts make separate rows.
    let s2 = store(&db);
    run(&s2, 20, &["Series/Show/Show S01E01.mp4"], true).await;
    run(&s2, 21, &["Series/Show/Show S01E01.mp4"], true).await;
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM video_titles WHERE source = 'mount'"
        )
        .await,
        2
    );
    // Duplicate episode numbers keep the first file.
    run(
        &s2,
        20,
        &[
            "Series/Show/Show S01E01.mp4",
            "Series/Show/Show S01E01 (alt).mp4",
        ],
        true,
    )
    .await;
    let n = sqlx::query("SELECT count(*) AS n FROM video_episodes WHERE mount_id = 20")
        .fetch_one(db.pool())
        .await
        .unwrap()
        .get::<i64, _>("n");
    assert_eq!(n, 1);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn deleting_and_moving_objects_updates_the_rows_at_once() {
    let db = TestDb::new().await.expect("db");
    let s = store(&db);
    run(&s, 7, TREE, true).await;
    // A delete hides the track; deleting the manga folder hides the series.
    assert_eq!(
        s.on_objects_deleted(7, "Music/Daft Punk/Discovery (2001)/02 - Aerodynamic.flac")
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM music_tracks WHERE shelf_state = 'listed'"
        )
        .await,
        1
    );
    assert_eq!(s.on_objects_deleted(7, "Manga/Berserk").await.unwrap(), 2);
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM reading_series WHERE title = 'Berserk' AND shelf_state = 'listed'"
        )
        .await,
        0
    );
    // Another mount's identical key is untouched.
    assert_eq!(s.on_objects_deleted(8, "Books/Dune.epub").await.unwrap(), 0);

    // A move keeps the row and its edit: the film is re-keyed with its object.
    sqlx::query(
        "UPDATE video_titles SET title = 'Mine', overridden = ARRAY['title'] \
         WHERE shelf_key = 'Movies/Alien (1979)/Alien.mp4'",
    )
    .execute(db.pool())
    .await
    .unwrap();
    assert_eq!(
        s.on_objects_moved(7, "Movies/Alien (1979)/Alien.mp4", "Films/Alien.mp4")
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM storage_objects WHERE mount_id = 7 AND key = 'Films/Alien.mp4'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM video_titles WHERE title = 'Mine' AND shelf_key = 'Films/Alien.mp4'"
        )
        .await,
        1
    );
    // A folder move re-keys everything below it.
    assert_eq!(
        s.on_objects_moved(7, "Books/", "Library/Books/")
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        count(
            &db,
            "SELECT count(*) FROM reading_chapters WHERE source_key = 'Library/Books/Dune.epub'"
        )
        .await,
        1
    );
}
