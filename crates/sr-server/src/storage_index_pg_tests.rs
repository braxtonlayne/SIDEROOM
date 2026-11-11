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

// ---- SEC-AUD-053 / SEC-AUD-054 (D-583, D-584): a takedown follows the bytes through the indexer -------------------

mod bytes_follow {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use sr_admin::Caller;
    use sr_core::PlatformPermissions as P;
    use sr_db::accounts::{self, NewAccount};
    use sr_moderation::{
        Moderation, ObjectDigests, SubjectNotifier,
        model::{ItemKind, NewItem, NewNotice, TakedownKind, Who},
    };

    const FILM: &[u8] = b"the very same film bytes";

    fn uid() -> i64 {
        static N: AtomicI64 = AtomicI64::new(0);
        let base = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_micros() as i64)
            .unwrap_or(0);
        base * 1000 + N.fetch_add(1, Ordering::Relaxed) % 1000
    }

    #[derive(Default)]
    struct Dms(Mutex<Vec<(i64, String)>>);

    #[async_trait::async_trait]
    impl SubjectNotifier for Dms {
        async fn system_dm(&self, to: i64, text: &str) -> bool {
            self.0.lock().unwrap().push((to, text.to_owned()));
            true
        }
    }

    /// The mounts' buckets, read by the uphold.
    struct Buckets(HashMap<i64, Arc<ObjectStoreStorage>>);

    #[async_trait::async_trait]
    impl ObjectDigests for Buckets {
        async fn digest(&self, mount_id: i64, key: &str) -> Option<(String, u64)> {
            let s = self.0.get(&mount_id)?;
            sr_storage::digest::sha256_object(s.as_ref(), key)
                .await
                .ok()
        }
    }

    async fn account(db: &TestDb, name: &str) -> i64 {
        let id = uid();
        assert!(
            accounts::create_account(
                db.pool(),
                &NewAccount {
                    id,
                    person_id: uid(),
                    username: name,
                    display_name: None,
                    email: &format!("{name}@example.test"),
                    password_hash: "x",
                    state: "active",
                    email_verified: true,
                    initial_credits: 0,
                    flags: 0,
                },
                uid(),
            )
            .await
            .unwrap()
        );
        id
    }

    async fn mount(db: &TestDb, owner: i64, label: &str) -> i64 {
        let id = uid();
        sqlx::query(
            "INSERT INTO storage_mounts (id, owner_kind, owner_id, label, provider, endpoint, bucket,
                                         access_key_id, secret_key_enc, key_hint)
             VALUES ($1, 'user', $2, $3, 'garage', 'http://127.0.0.1:3900', 'shelf', 'GK0123456789abcd',
                     '\\xdeadbeef', 'abcd')",
        )
        .bind(id)
        .bind(owner)
        .bind(label)
        .execute(db.pool())
        .await
        .unwrap();
        id
    }

    async fn put(s: &ObjectStoreStorage, key: &str, bytes: &'static [u8]) {
        s.put(key, Bytes::from_static(bytes)).await.unwrap();
    }

    async fn run(s: &PgIndexStore, storage: &ObjectStoreStorage, mount: i64) {
        index_mount(
            storage,
            s,
            mount,
            Options::default(),
            1_000,
            INDEX_WALK_BUDGET,
        )
        .await
        .unwrap();
    }

    /// The film is taken down in one member's bucket; the same bytes uploaded to another member's bucket under
    /// another name (the listing gives no usable tag, as with another tool's part size) are read once, delisted
    /// and their owner is told; a same-size file with other bytes stays; the restore lifts it and tells them.
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn a_reupload_is_found_by_its_bytes_and_its_owner_is_told() {
        let db = TestDb::new().await.unwrap();
        let s = store(&db);
        let staff = account(&db, "p56staff").await;
        let owner = account(&db, "p56owner").await;
        let other = account(&db, "p56other").await;
        let ma = mount(&db, owner, "Owner shelf").await;
        let mb = mount(&db, other, "Other shelf").await;
        let sa = Arc::new(ObjectStoreStorage::in_memory());
        let sb = Arc::new(ObjectStoreStorage::in_memory());
        put(&sa, "Movies/Leak (2001)/Leak (2001).mp4", FILM).await;
        run(&s, &sa, ma).await;
        let dms = Arc::new(Dms::default());
        let m = Moderation::new(db.pool().clone(), Arc::new(uid))
            .with_notifier(dms.clone())
            .with_object_digests(Arc::new(Buckets(HashMap::from([
                (ma, sa.clone()),
                (mb, sb.clone()),
            ]))));
        let caller = Caller {
            user_id: staff,
            permissions: P::MANAGE_TAKEDOWNS,
            is_owner: false,
            ip: Some("127.0.0.1".into()),
        };
        let who = Who::Staff(&caller);
        let notice = m
            .create_notice(
                &who,
                &NewNotice {
                    kind: TakedownKind::Copyright,
                    notifier_name: "Rights Holder Ltd".into(),
                    notifier_contact: "legal@rights.example".into(),
                    legal_basis: "Copyright in the film".into(),
                    locations: vec![],
                    original_text: "Please remove it.".into(),
                    received_at_ms: None,
                },
            )
            .await
            .unwrap()
            .summary
            .id;
        m.add_item(
            &who,
            notice,
            &NewItem {
                kind: ItemKind::StorageObject,
                id: ma,
                key: Some("Movies/Leak (2001)/Leak (2001).mp4".into()),
            },
        )
        .await
        .unwrap();
        m.uphold_notice(&who, notice, Some("valid")).await.unwrap();
        let (sha, size): (Option<String>, Option<i64>) = sqlx::query_as(
            "SELECT object_sha256, object_size FROM takedown_items WHERE notice_id = $1 AND derived_from IS NULL",
        )
        .bind(notice)
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(
            sha.map(|s| s.len()),
            Some(64),
            "the uphold read the object once"
        );
        assert_eq!(size, Some(FILM.len() as i64));

        // The other member's bucket: the film again, a look-alike of the same size, and an unrelated file.
        put(&sb, "Movies/Copy (2001)/Copy (2001).mp4", FILM).await;
        put(
            &sb,
            "Movies/Look (2002)/Look (2002).mp4",
            b"the very same film BYTES",
        )
        .await;
        put(&sb, "Movies/Other (2003)/Other (2003).mp4", b"another film").await;
        run(&s, &sb, mb).await;
        let cataloged: Vec<String> = sqlx::query_scalar(
            "SELECT key FROM storage_objects WHERE mount_id = $1 AND state = 'indexed' ORDER BY key",
        )
        .bind(mb)
        .fetch_all(db.pool())
        .await
        .unwrap();
        assert_eq!(
            cataloged,
            [
                "Movies/Look (2002)/Look (2002).mp4",
                "Movies/Other (2003)/Other (2003).mp4"
            ],
            "the re-upload is delisted, the look-alike and the other file are kept"
        );
        let look: Option<String> = sqlx::query_scalar(
            "SELECT content_sha256 FROM storage_objects WHERE mount_id = $1 AND key LIKE '%Look%'",
        )
        .bind(mb)
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(
            look.map(|s| s.len()),
            Some(64),
            "its sha256 is kept: never read twice"
        );
        let other_hash: Option<String> = sqlx::query_scalar(
            "SELECT content_sha256 FROM storage_objects WHERE mount_id = $1 AND key LIKE '%Other%'",
        )
        .bind(mb)
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(other_hash, None, "only size candidates are read");
        let derived: Vec<(String, Option<i64>, Option<i64>)> = sqlx::query_as(
            "SELECT object_key, subject_account_id, subject_person_id FROM takedown_items
             WHERE notice_id = $1 AND derived_from IS NOT NULL",
        )
        .bind(notice)
        .fetch_all(db.pool())
        .await
        .unwrap();
        assert_eq!(
            derived,
            vec![(
                "Movies/Copy (2001)/Copy (2001).mp4".to_owned(),
                Some(other),
                None
            )],
            "owned by the other member, no Person: no strike"
        );

        // A second run changes nothing and tells nobody twice.
        run(&s, &sb, mb).await;
        assert_eq!(m.notify_byte_matches().await, 1);
        assert_eq!(m.notify_byte_matches().await, 0);
        let to_other: Vec<String> = dms
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|(to, _)| *to == other)
            .map(|(_, t)| t.clone())
            .collect();
        assert_eq!(to_other.len(), 1, "{to_other:?}");
        assert!(
            to_other[0].contains(&format!("TK-{notice}")),
            "{}",
            to_other[0]
        );
        assert!(!to_other[0].contains("Leak (2001)"), "{}", to_other[0]);

        // The restore lifts the delisting and tells them.
        m.restore_notice(&who, notice, Some("counter-notice accepted"))
            .await
            .unwrap();
        let told = dms
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|(to, t)| *to == other && t.contains("available again"))
            .count();
        assert_eq!(told, 1);
        assert!(
            sr_db::storage_legal::delisted_keys(
                db.pool(),
                mb,
                &["Movies/Copy (2001)/Copy (2001).mp4".to_owned()]
            )
            .await
            .unwrap()
            .is_empty()
        );
    }
}
