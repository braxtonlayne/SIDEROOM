//! The `storage.index` job against real Postgres and the in-memory object store (BYOS-CAT-001; run with
//! `SR_TEST_DATABASE_URL`, `--include-ignored`, `--test-threads=1`).

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant};

use bytes::Bytes;
use sr_db::{index_repo::PgIndexStore, testing::TestDb};
use sr_storage::index::{IndexControl, Options};
use sr_storage::{ObjectStorage, ObjectStoreStorage};

use crate::storage_jobs::{INDEX_WALK_BUDGET, index_mount};

fn store(db: &TestDb) -> PgIndexStore {
    let n = Arc::new(AtomicI64::new(5_000_000));
    PgIndexStore::new(
        db.pool().clone(),
        Arc::new(move || n.fetch_add(1, Ordering::SeqCst)),
    )
}

/// "Jobs at scale": 10,000 objects (films, episodes, chapters and tracks) index well inside the walk budget, and a
/// second run over the same bucket changes nothing.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn ten_thousand_objects_index_under_the_budget_and_a_rerun_is_quiet() {
    let db = TestDb::new().await.unwrap();
    let s = store(&db);
    let storage = ObjectStoreStorage::in_memory();
    let mut n = 0u32;
    for i in 0..2_500 {
        storage
            .put(
                &format!("Movies/Film {i} ({})/Film.{i}.mp4", 1950 + i % 70),
                Bytes::from_static(b"x"),
            )
            .await
            .unwrap();
        n += 1;
    }
    for show in 0..50 {
        for e in 1..=50 {
            storage
                .put(
                    &format!("Series/Show {show}/Season 01/Show.{show}.S01E{e:02}.mkv"),
                    Bytes::from_static(b"x"),
                )
                .await
                .unwrap();
            n += 1;
        }
    }
    for series in 0..50 {
        for v in 1..=50 {
            storage
                .put(
                    &format!("Manga/Series {series}/Series {series} v{v:02}.cbz"),
                    Bytes::from_static(b"x"),
                )
                .await
                .unwrap();
            n += 1;
        }
    }
    for album in 0..100 {
        for t in 1..=25 {
            storage
                .put(
                    &format!(
                        "Music/Artist {}/Album {album}/{t:02} - Track {t}.flac",
                        album % 10
                    ),
                    Bytes::from_static(b"x"),
                )
                .await
                .unwrap();
            n += 1;
        }
    }
    assert_eq!(n, 2_500 + 2_500 + 2_500 + 2_500);

    let started = Instant::now();
    let r = index_mount(
        &storage,
        &s,
        31,
        Options::default(),
        200_000,
        INDEX_WALK_BUDGET,
    )
    .await
    .unwrap();
    let took = started.elapsed();
    assert_eq!((r.objects, r.truncated), (10_000, false));
    assert_eq!(r.added, 10_000);
    assert!(took < Duration::from_secs(60), "indexing took {took:?}");
    eprintln!("indexed 10,000 objects in {took:?}");

    let count = |sql: &'static str| {
        let pool = db.pool().clone();
        async move {
            sqlx::query_scalar::<_, i64>(sql)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    assert_eq!(
        count("SELECT count(*) FROM storage_objects WHERE mount_id = 31").await,
        10_000
    );
    assert_eq!(
        count("SELECT count(*) FROM music_tracks WHERE mount_id = 31").await,
        2_500
    );
    assert_eq!(
        count("SELECT count(*) FROM video_episodes WHERE mount_id = 31").await,
        2_500
    );

    let again = index_mount(
        &storage,
        &s,
        31,
        Options::default(),
        200_000,
        INDEX_WALK_BUDGET,
    )
    .await
    .unwrap();
    assert_eq!((again.added, again.vanished), (0, 0));
    let st = s.status(31).await.unwrap().unwrap();
    assert_eq!(
        (st.state.as_str(), st.objects, st.truncated),
        ("done", 10_000, false)
    );

    // The member asks for a re-index twice: one job is queued, not two.
    assert!(s.request(31).await.unwrap());
    assert!(!s.request(31).await.unwrap());
}
