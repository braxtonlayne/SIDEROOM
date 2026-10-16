//! DB-backed tests of ingest sources: watched folders and storage imports create draft
//! submissions from files, wait for files to settle, and never leave the approved roots.
//! Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test ingest_sources -- --include-ignored --test-threads=1`

use std::io::{Cursor, Write};
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicI64, Ordering},
};

use async_trait::async_trait;
use bytes::Bytes;
use sr_core::Clock;
use sr_db::{
    DbConfig, PgPool,
    accounts::{self, NewAccount},
    library_repo::PgLibraryRepo,
    review_repo::PgReviewRepo,
};
use sr_library::sources::{
    Backend, FsAndStorageFiles, SourceInput, SourceKind, SourcePatch, SourceScanner,
};
use sr_library::submissions::LibraryKind;
use sr_library::{IngestQueue, IngestSettingsSnapshot, LibraryService, StaticIngestSettings};
use sr_review::{KindRegistry, ReviewService};
use sr_storage::cdn::CdnSigner;
use sr_storage::{ObjectStorage, ObjectStoreStorage};

fn uid() -> i64 {
    static N: AtomicI64 = AtomicI64::new(0);
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + N.fetch_add(1, Ordering::Relaxed) % 1000
}

async fn pool() -> PgPool {
    let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
    let pool = sr_db::connect(&DbConfig::new(url)).await.expect("connect");
    sr_db::migrate(&pool).await.expect("migrate");
    pool
}

async fn account(pool: &PgPool) -> i64 {
    let id = uid();
    let name = format!("t{id}");
    let ok = accounts::create_account(
        pool,
        &NewAccount {
            id,
            person_id: uid(),
            username: &name,
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
    .unwrap();
    assert!(ok);
    id
}

struct TestClock(AtomicI64);

impl Clock for TestClock {
    fn now_unix_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Default)]
struct Queue(Mutex<Vec<i64>>);

#[async_trait]
impl IngestQueue for Queue {
    async fn enqueue(&self, upload_id: i64) -> Result<(), String> {
        self.0.lock().unwrap().push(upload_id);
        Ok(())
    }
}

struct World {
    scanner: SourceScanner,
    review: ReviewService,
    lib: LibraryService,
    pool: PgPool,
    storage: Arc<ObjectStoreStorage>,
    queue: Arc<Queue>,
    clock: Arc<TestClock>,
    root: PathBuf,
    owner: i64,
}

async fn world(approve_root: bool) -> World {
    let pool = pool().await;
    let storage = Arc::new(ObjectStoreStorage::in_memory());
    let queue = Arc::new(Queue::default());
    let clock = Arc::new(TestClock(AtomicI64::new(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0),
    )));
    let lib = LibraryService::new(
        Arc::new(PgLibraryRepo::new(pool.clone())),
        CdnSigner::new(vec![7u8; 32]),
        clock.clone(),
        Arc::new(uid),
    )
    .with_index(Arc::new(sr_library::SeriesIndex::in_ram().unwrap()))
    .with_ingest(
        storage.clone(),
        Arc::new(StaticIngestSettings(IngestSettingsSnapshot::default())),
        queue.clone(),
    );
    let review = ReviewService::new(
        Arc::new(PgReviewRepo::new(pool.clone())),
        KindRegistry::new()
            .with(Arc::new(LibraryKind::series(lib.clone())))
            .with(Arc::new(LibraryKind::items(lib.clone()))),
        clock.clone(),
        Arc::new(uid),
    );
    let owner = account(&pool).await;
    let admin = account(&pool).await;
    sqlx::query(
        "INSERT INTO staff_appointments (id, account_id, appointed_by, reason) VALUES ($1, $2, $3, 'test')",
    )
    .bind(uid())
    .bind(owner)
    .bind(admin)
    .execute(&pool)
    .await
    .unwrap();
    let root = std::env::temp_dir().join(format!("sr-ingest-test-{}", uid()));
    std::fs::create_dir_all(&root).unwrap();
    let roots = if approve_root {
        vec![root.clone()]
    } else {
        vec![]
    };
    let scanner = SourceScanner::new(
        Arc::new(PgLibraryRepo::new(pool.clone())),
        Arc::new(FsAndStorageFiles::new(roots, storage.clone())),
        lib.clone(),
        review.clone(),
        clock.clone(),
    );
    World {
        scanner,
        review,
        lib,
        pool,
        storage,
        queue,
        clock,
        root,
        owner,
    }
}

fn cbz(n: usize) -> Vec<u8> {
    use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
    use zip::{ZipWriter, write::SimpleFileOptions};
    let mut w = ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..n {
        let img = RgbImage::from_fn(30, 40, |x, y| Rgb([i as u8, x as u8, y as u8]));
        let mut png = Vec::new();
        DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
            .unwrap();
        w.start_file(format!("{:03}.png", i + 1), SimpleFileOptions::default())
            .unwrap();
        w.write_all(&png).unwrap();
    }
    w.finish().unwrap().into_inner()
}

fn input(w: &World, kind: SourceKind, backend: Backend, location: &str) -> SourceInput {
    SourceInput {
        label: "Storage Box".into(),
        kind,
        backend,
        location: location.into(),
        owner_id: w.owner,
        enabled: true,
        poll_secs: Some(60),
        stable_secs: Some(10),
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_watched_folder_waits_for_files_to_settle_then_creates_drafts() {
    let w = world(true).await;
    let series = format!("Blue Hour {}", uid());
    let dir = w.root.join(&series);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{series} c001.cbz")), cbz(2)).unwrap();
    std::fs::write(dir.join(format!("{series} c002.cbz")), cbz(3)).unwrap();
    std::fs::write(dir.join("notes.txt"), b"ignored").unwrap();
    std::fs::write(w.root.join("lonely.cbz"), cbz(1)).unwrap();

    let src = w
        .scanner
        .create(
            uid(),
            input(
                &w,
                SourceKind::WatchedFolder,
                Backend::Local,
                w.root.to_str().unwrap(),
            ),
        )
        .await
        .unwrap();

    // A dry run reports the structure and writes nothing.
    let dry = w.scanner.scan(src.id, true).await.unwrap();
    assert!(dry.dry_run);
    assert_eq!(dry.files_seen, 3, "the text file is not a candidate");
    assert_eq!(dry.series.len(), 1);
    assert_eq!(dry.series[0].title, series);
    assert_eq!(dry.series[0].chapters.len(), 2);
    assert_eq!(dry.unplaced.len(), 1);
    assert_eq!(dry.unplaced[0].rel_path, "lonely.cbz");
    assert!(w.scanner.store().ledger(src.id).await.unwrap().is_empty());
    assert_eq!(dry.imported, 0);

    // The first real scan only notes the files: they might still be copying.
    let first = w.scanner.scan(src.id, false).await.unwrap();
    assert_eq!(first.imported, 0);
    assert_eq!(
        first.waiting, 3,
        "every candidate waits, the unplaced one too"
    );
    assert!(first.submissions.is_empty());
    assert_eq!(w.scanner.store().ledger(src.id).await.unwrap().len(), 3);

    // A file that is still growing starts waiting again.
    std::fs::write(dir.join(format!("{series} c002.cbz")), cbz(4)).unwrap();
    w.clock.0.fetch_add(11_000, Ordering::SeqCst);
    let second = w.scanner.scan(src.id, false).await.unwrap();
    assert_eq!(
        second.imported, 1,
        "only the unchanged file is ready: {second:?}"
    );
    assert_eq!(second.waiting, 1);

    w.clock.0.fetch_add(11_000, Ordering::SeqCst);
    let third = w.scanner.scan(src.id, false).await.unwrap();
    assert_eq!(third.imported, 1);
    assert_eq!(
        w.queue.0.lock().unwrap().len(),
        2,
        "both uploads were queued for conversion"
    );

    // The drafts belong to the source's owner and are not submitted.
    let ledger = w.scanner.store().ledger(src.id).await.unwrap();
    let imported: Vec<_> = ledger
        .iter()
        .filter(|l| l.state == sr_library::sources::FileState::Imported)
        .collect();
    assert_eq!(imported.len(), 2);
    let sub = imported[0].submission_id.unwrap();
    let d = w.review.get_mine(w.owner, sub).await.unwrap();
    assert_eq!(d.submission.kind, "library.series");
    assert_eq!(d.submission.state.as_str(), "draft");
    assert!(!d.items.is_empty());

    // Nothing is imported twice.
    w.clock.0.fetch_add(11_000, Ordering::SeqCst);
    let fourth = w.scanner.scan(src.id, false).await.unwrap();
    assert_eq!(fourth.imported, 0);
    assert!(fourth.submissions.is_empty());
    let src = w.scanner.store().get(src.id).await.unwrap().unwrap();
    assert!(src.last_scan_at_ms.is_some());
    assert!(src.last_error.is_none());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn files_are_added_to_a_published_series_with_the_same_title() {
    let w = world(true).await;
    let title = format!("Existing Saga {}", uid());
    let svc = w.lib.clone();
    let actor = sr_library::Actor {
        user_id: w.owner,
        permissions: sr_core::PlatformPermissions::PUBLISH_READING,
    };
    let s = svc
        .create_series(
            actor,
            sr_library::model::SeriesInput {
                kind: sr_library::model::SeriesKind::Manga,
                title: title.clone(),
                alt_titles: vec![],
                authors: vec![],
                artists: vec![],
                status: sr_library::model::SeriesStatus::Ongoing,
                year: None,
                synopsis: String::new(),
                genres: vec![],
                tags: vec![],
                reading_direction: None,
                age_restricted: false,
                cover_key: None,
            },
        )
        .await
        .unwrap();
    svc.create_edition(
        actor,
        s.id,
        sr_library::model::EditionInput {
            language: "en".into(),
            label: "Official".into(),
            is_default: true,
        },
    )
    .await
    .unwrap();
    svc.set_series_published(actor, s.id, true).await.unwrap();

    let dir = w.root.join(&title);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{title} c005.cbz")), cbz(2)).unwrap();
    let src = w
        .scanner
        .create(
            uid(),
            input(
                &w,
                SourceKind::StorageImport,
                Backend::Local,
                w.root.to_str().unwrap(),
            ),
        )
        .await
        .unwrap();
    let dry = w.scanner.scan(src.id, true).await.unwrap();
    assert_eq!(dry.series[0].existing_series, Some(s.id), "{dry:?}");
    // A storage import does not wait for files to settle.
    let done = w.scanner.scan(src.id, false).await.unwrap();
    assert_eq!(done.imported, 1, "{done:?}");
    let sub = w
        .review
        .get_mine(w.owner, done.submissions[0].parse().unwrap())
        .await
        .unwrap();
    assert_eq!(sub.submission.kind, "library.items");
    assert_eq!(sub.submission.ref_id, Some(s.id));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_storage_prefix_is_imported_and_sources_stay_inside_their_limits() {
    let w = world(true).await;
    let series = format!("Storage Tale {}", uid());
    let prefix = format!("imports/{}", uid());
    for n in 1..=2 {
        w.storage
            .put(
                &format!("{prefix}/{series}/{series} c00{n}.cbz"),
                Bytes::from(cbz(2)),
            )
            .await
            .unwrap();
    }
    let src = w
        .scanner
        .create(
            uid(),
            input(&w, SourceKind::StorageImport, Backend::Storage, &prefix),
        )
        .await
        .unwrap();
    let dry = w.scanner.scan(src.id, true).await.unwrap();
    assert_eq!(dry.series[0].chapters.len(), 2, "{dry:?}");
    let done = w.scanner.scan(src.id, false).await.unwrap();
    assert_eq!(done.imported, 2, "{done:?}");

    // A prefix outside imports/ and a folder outside the approved roots are refused.
    let err = w
        .scanner
        .create(
            uid(),
            input(
                &w,
                SourceKind::StorageImport,
                Backend::Storage,
                "attachments/1",
            ),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("imports/"), "{err}");
    let outside = std::env::temp_dir().to_str().unwrap().to_owned();
    let err = w
        .scanner
        .create(
            uid(),
            input(&w, SourceKind::WatchedFolder, Backend::Local, &outside),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("approved roots"), "{err}");
    // The owner must be staff.
    let member = account(&w.pool).await;
    let mut bad = input(&w, SourceKind::StorageImport, Backend::Storage, &prefix);
    bad.owner_id = member;
    let err = w.scanner.create(uid(), bad).await.unwrap_err();
    assert!(err.to_string().contains("staff"), "{err}");

    // Edits are validated; a second scan at the same time is refused while one runs.
    let patched = w
        .scanner
        .patch(
            src.id,
            SourcePatch {
                enabled: Some(false),
                poll_secs: Some(120),
                ..SourcePatch::default()
            },
        )
        .await
        .unwrap();
    assert!(!patched.enabled);
    assert!(
        w.scanner
            .patch(
                src.id,
                SourcePatch {
                    poll_secs: Some(5),
                    ..SourcePatch::default()
                }
            )
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn local_folders_are_off_without_an_approved_root() {
    let w = world(false).await;
    let err = w
        .scanner
        .create(
            uid(),
            input(
                &w,
                SourceKind::WatchedFolder,
                Backend::Local,
                w.root.to_str().unwrap(),
            ),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("off"), "{err}");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn due_watched_folders_follow_their_poll_interval() {
    let w = world(true).await;
    let src = w
        .scanner
        .create(
            uid(),
            input(
                &w,
                SourceKind::WatchedFolder,
                Backend::Local,
                w.root.to_str().unwrap(),
            ),
        )
        .await
        .unwrap();
    let now = w.clock.now_unix_ms();
    let due = |ids: Vec<i64>| ids.contains(&src.id);
    let ids =
        |v: Vec<sr_library::sources::IngestSource>| v.into_iter().map(|s| s.id).collect::<Vec<_>>();
    assert!(
        due(ids(w.scanner.store().due(now).await.unwrap())),
        "never scanned"
    );
    w.scanner.scan(src.id, false).await.unwrap();
    assert!(!due(ids(w.scanner.store().due(now + 1_000).await.unwrap())));
    assert!(due(ids(w.scanner.store().due(now + 61_000).await.unwrap())));
    w.scanner
        .patch(
            src.id,
            SourcePatch {
                enabled: Some(false),
                ..SourcePatch::default()
            },
        )
        .await
        .unwrap();
    assert!(
        !due(ids(w.scanner.store().due(now + 61_000).await.unwrap())),
        "off"
    );
    assert!(w.scanner.store().delete(src.id).await.unwrap());
}
