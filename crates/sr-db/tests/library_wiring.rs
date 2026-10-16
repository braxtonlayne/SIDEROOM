//! DB-backed tests of the Library wiring: reading progress and sync events, read marks,
//! history, bookmarks, keyset chapter pages, the upload and ingest flow with real archives,
//! and the storage collector. Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test library_wiring -- --ignored --test-threads=1`

use std::io::{Cursor, Write};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicI64, Ordering},
};

use async_trait::async_trait;
use bytes::Bytes;
use serde_json::Value;
use sr_core::{Clock, PlatformPermissions as P};
use sr_db::{
    DbConfig, PgPool,
    accounts::{self, NewAccount},
    library_repo::PgLibraryRepo,
};
use sr_library::model::*;
use sr_library::reading::{BookmarkInput, ProgressInput, ReadMarkInput};
use sr_library::{
    Actor, IngestQueue, IngestSettingsSnapshot, LibraryError, LibraryEvents, LibraryService,
    StaticIngestSettings, Viewer,
};
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
struct Events(Mutex<Vec<(Vec<i64>, Value)>>);

#[async_trait]
impl LibraryEvents for Events {
    async fn emit(&self, users: Vec<i64>, payload: Value) {
        self.0.lock().unwrap().push((users, payload));
    }
}

impl Events {
    fn types(&self, user: i64) -> Vec<String> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(u, _)| u.contains(&user))
            .filter_map(|(_, p)| p["type"].as_str().map(str::to_owned))
            .collect()
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
    svc: LibraryService,
    pool: PgPool,
    storage: Arc<ObjectStoreStorage>,
    events: Arc<Events>,
    queue: Arc<Queue>,
    clock: Arc<TestClock>,
    publisher: Actor,
    member: Viewer,
    minor: Viewer,
}

async fn world_with(settings: IngestSettingsSnapshot) -> World {
    let pool = pool().await;
    let storage = Arc::new(ObjectStoreStorage::in_memory());
    let events = Arc::new(Events::default());
    let queue = Arc::new(Queue::default());
    let clock = Arc::new(TestClock(AtomicI64::new(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0),
    )));
    let svc = LibraryService::new(
        Arc::new(PgLibraryRepo::new(pool.clone())),
        CdnSigner::new(vec![7u8; 32]),
        clock.clone(),
        Arc::new(uid),
    )
    .with_events(events.clone())
    .with_ingest(
        storage.clone(),
        Arc::new(StaticIngestSettings(settings)),
        queue.clone(),
    );
    let (p, m, a) = (
        account(&pool).await,
        account(&pool).await,
        account(&pool).await,
    );
    World {
        svc,
        pool,
        storage,
        events,
        queue,
        clock,
        publisher: Actor {
            user_id: p,
            permissions: P::PUBLISH_READING | P::DELETE_CONTENT,
        },
        member: Viewer {
            user_id: m,
            adult: true,
            hide_restricted: false,
        },
        minor: Viewer {
            user_id: a,
            adult: false,
            hide_restricted: false,
        },
    }
}

async fn world() -> World {
    world_with(IngestSettingsSnapshot::default()).await
}

impl World {
    fn input(&self, title: &str, restricted: bool) -> SeriesInput {
        SeriesInput {
            kind: SeriesKind::Manga,
            title: format!("{title} {}", uid()),
            alt_titles: vec![],
            authors: vec![],
            artists: vec![],
            status: SeriesStatus::Ongoing,
            year: None,
            synopsis: String::new(),
            genres: vec![],
            tags: vec![],
            reading_direction: None,
            age_restricted: restricted,
            cover_key: None,
        }
    }

    /// A series with one edition; returns (series, edition).
    async fn series(&self, restricted: bool) -> (i64, i64) {
        let s = self
            .svc
            .create_series(self.publisher, self.input("Wiring", restricted))
            .await
            .unwrap();
        let e = self
            .svc
            .create_edition(
                self.publisher,
                s.id,
                EditionInput {
                    language: "en".into(),
                    label: "Official".into(),
                    is_default: false,
                },
            )
            .await
            .unwrap();
        (s.id, e.id)
    }

    async fn draft(&self, edition: i64, number: f64, format: ChapterFormat) -> i64 {
        self.svc
            .create_chapter(
                self.publisher,
                edition,
                ChapterInput {
                    number,
                    title: Some(format!("Ch {number}")),
                    volume_id: None,
                    format,
                    source_key: None,
                    release_group: None,
                },
            )
            .await
            .unwrap()
            .id
    }

    /// A published image chapter with `n` stored pages.
    async fn chapter(&self, series: i64, edition: i64, number: f64, n: usize) -> i64 {
        let c = self.draft(edition, number, ChapterFormat::Images).await;
        self.svc
            .replace_pages(self.publisher, c, pages(series, c, n))
            .await
            .unwrap();
        self.svc
            .set_chapter_published(self.publisher, c, true)
            .await
            .unwrap();
        c
    }

    async fn publish_series(&self, series: i64) {
        self.svc
            .set_series_published(self.publisher, series, true)
            .await
            .unwrap();
    }

    async fn gc_queue(&self, key: &str) -> bool {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM reading_gc_keys WHERE key = $1)",
        )
        .bind(key)
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }

    async fn exists(&self, key: &str) -> bool {
        self.storage.head(key).await.is_ok()
    }
}

fn pages(series: i64, chapter: i64, n: usize) -> Vec<PageInput> {
    (0..n)
        .map(|i| PageInput {
            key: format!("library/{series}/{chapter}/{i:04}.webp"),
            variants: [(
                "720".to_owned(),
                format!("library/{series}/{chapter}/{i:04}_720.webp"),
            )]
            .into_iter()
            .collect(),
            width: 1600,
            height: 2400,
            is_double_spread: false,
            blurhash: Some("LEHV6nWB2yk8pyo0adR*.7kCMdnj".into()),
            crop: (i == 1).then_some(sr_library::ingest::crop::PageCrop {
                left: 10,
                top: 20,
                right: 30,
                bottom: 40,
            }),
        })
        .collect()
}

fn progress(page: i32, percent: f32) -> ProgressInput {
    ProgressInput {
        page_index: page,
        percent,
        cfi: None,
        completed: None,
        base_updated_at_ms: None,
        force: None,
    }
}

fn png(w: u32, h: u32, seed: u8) -> Vec<u8> {
    use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
    let img = RgbImage::from_fn(w, h, |x, y| Rgb([seed, (x % 256) as u8, (y % 256) as u8]));
    let mut out = Vec::new();
    DynamicImage::ImageRgb8(img)
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .unwrap();
    out
}

fn cbz(n: usize) -> Vec<u8> {
    use zip::{ZipWriter, write::SimpleFileOptions};
    let mut w = ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..n {
        w.start_file(format!("{:03}.png", i + 1), SimpleFileOptions::default())
            .unwrap();
        w.write_all(&png(900, 1300, i as u8)).unwrap();
    }
    w.start_file("ComicInfo.xml", SimpleFileOptions::default())
        .unwrap();
    w.write_all(b"<ComicInfo><Title>Found Title</Title><Number>3</Number></ComicInfo>")
        .unwrap();
    w.finish().unwrap().into_inner()
}

const PDF: &[u8] =
    b"%PDF-1.7\n1 0 obj<</Type/Catalog>>endobj\ntrailer<</Root 1 0 R>>\nstartxref\n0\n%%EOF\n";

async fn upload(w: &World, chapter: i64, name: &str, bytes: Vec<u8>) -> Result<i64, LibraryError> {
    let slot = w
        .svc
        .begin_upload(
            w.publisher,
            chapter,
            UploadRequest {
                filename: name.into(),
                size_bytes: bytes.len() as u64,
                multipart: false,
            },
        )
        .await?;
    let key = format!(
        "library/{}/{}/src-{}.{}",
        slot.upload.series_id,
        chapter,
        slot.upload.id,
        name.rsplit('.').next().unwrap().to_ascii_lowercase()
    );
    assert!(slot.upload_url.contains(&key) || !slot.upload_url.is_empty());
    w.storage.put(&key, Bytes::from(bytes)).await.unwrap();
    w.svc.complete_upload(w.publisher, slot.upload.id).await?;
    Ok(slot.upload.id)
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn progress_syncs_conflicts_and_clears() {
    let w = world().await;
    let (s, e) = w.series(false).await;
    let c = w.chapter(s, e, 1.0, 5).await;
    w.publish_series(s).await;

    // Nothing saved yet.
    assert!(matches!(
        w.svc.progress(w.member, c).await.unwrap_err(),
        LibraryError::NotFound(_)
    ));
    let first = w
        .svc
        .save_progress(w.member, c, progress(1, 20.0))
        .await
        .unwrap();
    assert!(first.applied);
    assert_eq!(first.progress.page_index, 1);
    assert!(!first.progress.completed);
    let stored = w.svc.progress(w.member, c).await.unwrap();
    assert_eq!(stored, first.progress);

    // The chapter list carries the mark and the base for the next write.
    let list = w
        .svc
        .chapters(w.member, s, ChapterQuery::default())
        .await
        .unwrap();
    let mark = list.chapters[0].progress.unwrap();
    assert_eq!(mark.updated_at_ms, stored.updated_at_ms);

    // Another device reads on; the first device still writes against its old base.
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    let mut other = progress(3, 60.0);
    other.base_updated_at_ms = Some(stored.updated_at_ms);
    let second = w.svc.save_progress(w.member, c, other).await.unwrap();
    assert!(second.applied);
    assert!(second.progress.updated_at_ms >= stored.updated_at_ms);
    let mut stale = progress(0, 0.0);
    stale.base_updated_at_ms = Some(0);
    let rejected = w
        .svc
        .save_progress(w.member, c, stale.clone())
        .await
        .unwrap();
    assert!(
        !rejected.applied,
        "a newer stored position must win without force"
    );
    assert_eq!(rejected.progress.page_index, 3);
    stale.force = Some(true);
    let forced = w.svc.save_progress(w.member, c, stale).await.unwrap();
    assert!(forced.applied);
    assert_eq!(forced.progress.page_index, 0);

    // Validation.
    assert!(matches!(
        w.svc
            .save_progress(w.member, c, progress(5, 10.0))
            .await
            .unwrap_err(),
        LibraryError::Invalid(_)
    ));
    assert!(
        w.svc
            .save_progress(w.member, c, progress(0, 101.0))
            .await
            .is_err()
    );
    // Finishing marks it completed.
    let done = w
        .svc
        .save_progress(w.member, c, progress(4, 100.0))
        .await
        .unwrap();
    assert!(done.progress.completed);

    // Events went to the reader only.
    let types = w.events.types(w.member.user_id);
    assert_eq!(types.iter().filter(|t| *t == "progress").count(), 4);
    assert!(w.events.types(w.minor.user_id).is_empty());
    let payload = w.events.0.lock().unwrap()[0].1.clone();
    assert_eq!(payload["chapter_id"], c.to_string());

    // Unread.
    assert!(w.svc.clear_progress(w.member, c).await.unwrap());
    assert!(!w.svc.clear_progress(w.member, c).await.unwrap());
    assert!(
        w.events
            .types(w.member.user_id)
            .contains(&"progress_cleared".to_owned())
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn hidden_and_restricted_chapters_cannot_be_tracked() {
    let w = world().await;
    let (s, e) = w.series(true).await;
    let c = w.chapter(s, e, 1.0, 2).await;
    // Series not published yet: 404 for members.
    assert!(matches!(
        w.svc
            .save_progress(w.member, c, progress(0, 1.0))
            .await
            .unwrap_err(),
        LibraryError::NotFound(_)
    ));
    w.publish_series(s).await;
    // Restricted: needs the age confirmation, like opening the pages.
    assert_eq!(
        w.svc
            .save_progress(w.minor, c, progress(0, 1.0))
            .await
            .unwrap_err(),
        LibraryError::AgeConfirmationRequired
    );
    assert_eq!(
        w.svc
            .save_bookmark(
                w.minor,
                c,
                BookmarkInput {
                    page_index: 0,
                    note: None
                }
            )
            .await
            .unwrap_err(),
        LibraryError::AgeConfirmationRequired
    );
    assert!(
        w.svc
            .save_progress(w.member, c, progress(0, 1.0))
            .await
            .is_ok()
    );
    // Unpublished chapter.
    w.svc
        .set_chapter_published(w.publisher, c, false)
        .await
        .unwrap();
    assert!(matches!(
        w.svc.progress(w.member, c).await.unwrap_err(),
        LibraryError::NotFound(_)
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn read_marks_history_and_bookmarks() {
    let w = world().await;
    let (s, e) = w.series(false).await;
    let c1 = w.chapter(s, e, 1.0, 3).await;
    let c2 = w.chapter(s, e, 2.0, 3).await;
    let c3 = w.chapter(s, e, 3.0, 3).await;
    // A second series so history has two entries.
    let (s2, e2) = w.series(false).await;
    let d1 = w.chapter(s2, e2, 1.0, 2).await;
    w.publish_series(s).await;
    w.publish_series(s2).await;

    // "Mark read up to here" on chapter 2 marks chapters 1 and 2 only.
    let r = w
        .svc
        .mark_read(
            w.member,
            c2,
            ReadMarkInput {
                read: true,
                up_to: Some(true),
            },
        )
        .await
        .unwrap();
    assert_eq!(r.changed, 2);
    let again = w
        .svc
        .mark_read(
            w.member,
            c2,
            ReadMarkInput {
                read: true,
                up_to: Some(true),
            },
        )
        .await
        .unwrap();
    assert_eq!(again.changed, 0, "already read");
    let list = w
        .svc
        .chapters(w.member, s, ChapterQuery::default())
        .await
        .unwrap();
    let done: Vec<bool> = list
        .chapters
        .iter()
        .map(|c| c.progress.is_some_and(|p| p.completed))
        .collect();
    assert_eq!(done, [true, true, false]);
    // Someone else's marks are theirs.
    let other = w
        .svc
        .chapters(w.minor, s, ChapterQuery::default())
        .await
        .unwrap();
    assert!(other.chapters.iter().all(|c| c.progress.is_none()));

    // Mark chapter 1 unread.
    let u = w
        .svc
        .mark_read(
            w.member,
            c1,
            ReadMarkInput {
                read: false,
                up_to: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(u.changed, 1);

    // History: the latest chapter of each series, newest first.
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    w.svc
        .save_progress(w.member, d1, progress(1, 50.0))
        .await
        .unwrap();
    let hist = w.svc.history(w.member, true, None, None).await.unwrap();
    let ids: Vec<i64> = hist.items.iter().map(|h| h.chapter.id).collect();
    assert_eq!(ids, [d1, c2], "one row per series, newest first");
    assert!(hist.items[0].chapter.progress.is_some());
    // Every chapter, paged by keyset.
    let all = w.svc.history(w.member, false, None, Some(1)).await.unwrap();
    assert_eq!(all.items.len(), 1);
    let next = w
        .svc
        .history(w.member, false, all.next_before.as_deref(), Some(10))
        .await
        .unwrap();
    assert_eq!(next.items.len(), 1);
    assert!(next.next_before.is_none());
    assert!(
        w.svc
            .history(w.member, true, Some("x"), None)
            .await
            .is_err()
    );

    // Bookmarks.
    let b = w
        .svc
        .save_bookmark(
            w.member,
            c3,
            BookmarkInput {
                page_index: 1,
                note: Some(" great ".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(b.note, "great");
    assert_eq!((b.chapter_id, b.series_id), (c3, s));
    // The same page again updates the note, not the count.
    let b2 = w
        .svc
        .save_bookmark(
            w.member,
            c3,
            BookmarkInput {
                page_index: 1,
                note: Some("better".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(b2.note, "better");
    w.svc
        .save_bookmark(
            w.member,
            c3,
            BookmarkInput {
                page_index: 0,
                note: None,
            },
        )
        .await
        .unwrap();
    let mine = w.svc.chapter_bookmarks(w.member, c3).await.unwrap();
    assert_eq!(
        mine.bookmarks
            .iter()
            .map(|b| b.page_index)
            .collect::<Vec<_>>(),
        [0, 1]
    );
    assert!(
        w.svc
            .chapter_bookmarks(w.minor, c3)
            .await
            .unwrap()
            .bookmarks
            .is_empty()
    );
    assert!(matches!(
        w.svc
            .save_bookmark(
                w.member,
                c3,
                BookmarkInput {
                    page_index: 3,
                    note: None
                }
            )
            .await
            .unwrap_err(),
        LibraryError::Invalid(_)
    ));
    let page = w.svc.my_bookmarks(w.member, None, Some(1)).await.unwrap();
    assert_eq!(page.items.len(), 1);
    assert!(page.next_before.is_some());
    assert_eq!(page.items[0].chapter_number, 3.0);
    let rest = w
        .svc
        .my_bookmarks(w.member, page.next_before.as_deref(), Some(10))
        .await
        .unwrap();
    assert_eq!(rest.items.len(), 1);
    // Only the owner can delete.
    assert!(!w.svc.delete_bookmark(w.minor, b.id).await.unwrap());
    assert!(w.svc.delete_bookmark(w.member, b.id).await.unwrap());
    assert!(!w.svc.delete_bookmark(w.member, b.id).await.unwrap());
    let t = w.events.types(w.member.user_id);
    assert!(t.contains(&"bookmark_set".to_owned()) && t.contains(&"bookmark_removed".to_owned()));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn bookmarks_are_capped_per_chapter() {
    let w = world().await;
    let (s, e) = w.series(false).await;
    let c = w.draft(e, 1.0, ChapterFormat::Epub).await;
    // EPUB positions are not bounded by a page count; give it a source so it can publish.
    sqlx::query("UPDATE reading_chapters SET source_key = 'library/x/y/src.epub' WHERE id = $1")
        .bind(c)
        .execute(&w.pool)
        .await
        .unwrap();
    w.svc
        .set_chapter_published(w.publisher, c, true)
        .await
        .unwrap();
    w.publish_series(s).await;
    for i in 0..200 {
        w.svc
            .save_bookmark(
                w.member,
                c,
                BookmarkInput {
                    page_index: i,
                    note: None,
                },
            )
            .await
            .unwrap();
    }
    assert!(matches!(
        w.svc
            .save_bookmark(
                w.member,
                c,
                BookmarkInput {
                    page_index: 500,
                    note: None
                }
            )
            .await
            .unwrap_err(),
        LibraryError::Conflict(_)
    ));
    // Editing an existing one still works at the cap.
    assert!(
        w.svc
            .save_bookmark(
                w.member,
                c,
                BookmarkInput {
                    page_index: 7,
                    note: Some("x".into())
                }
            )
            .await
            .is_ok()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn chapter_list_pages_by_keyset_in_both_directions() {
    let w = world().await;
    let (s, e) = w.series(false).await;
    let e2 = w
        .svc
        .create_edition(
            w.publisher,
            s,
            EditionInput {
                language: "ja".into(),
                label: "Raw".into(),
                is_default: false,
            },
        )
        .await
        .unwrap()
        .id;
    let mut expected = Vec::new();
    for n in [1.0, 2.0, 2.5, 10.0, 11.0] {
        expected.push(w.chapter(s, e, n, 1).await);
    }
    for n in [1.0, 3.0] {
        expected.push(w.chapter(s, e2, n, 1).await);
    }
    w.publish_series(s).await;
    // An unpublished chapter is never listed to members.
    w.draft(e, 99.0, ChapterFormat::Images).await;

    let all = w
        .svc
        .chapters(w.member, s, ChapterQuery::default())
        .await
        .unwrap();
    assert_eq!(all.chapters.len(), 7);
    assert!(all.next_cursor.is_none());
    let order: Vec<i64> = all.chapters.iter().map(|c| c.id).collect();
    assert_eq!(order, expected, "edition, then number, then id");

    for descending in [false, true] {
        let mut got = Vec::new();
        let mut after = None;
        let mut pages = 0;
        loop {
            let page = w
                .svc
                .chapters(
                    w.member,
                    s,
                    ChapterQuery {
                        edition: None,
                        after: after.clone(),
                        limit: Some(3),
                        descending,
                    },
                )
                .await
                .unwrap();
            assert!(page.chapters.len() <= 3);
            got.extend(page.chapters.iter().map(|c| c.id));
            pages += 1;
            // Editions are repeated on every page.
            assert_eq!(page.editions.len(), 2);
            match page.next_cursor {
                Some(c) => after = Some(c),
                None => break,
            }
            assert!(pages < 10);
        }
        let mut want = expected.clone();
        if descending {
            want.reverse();
        }
        assert_eq!(got, want, "descending = {descending}");
        assert_eq!(pages, 3);
    }
    // One edition only, exact page boundary: no dangling cursor.
    let only = w
        .svc
        .chapters(
            w.member,
            s,
            ChapterQuery {
                edition: Some(e),
                after: None,
                limit: Some(5),
                descending: false,
            },
        )
        .await
        .unwrap();
    assert_eq!(only.chapters.len(), 5);
    assert!(only.next_cursor.is_none());
    assert!(
        w.svc
            .chapters(
                w.member,
                s,
                ChapterQuery {
                    after: Some("nope".into()),
                    ..Default::default()
                }
            )
            .await
            .is_err()
    );
    // Publishers see the draft too, with the same paging.
    let admin = w
        .svc
        .admin_chapters(
            w.publisher,
            s,
            ChapterQuery {
                limit: Some(100),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(admin.chapters.len(), 8);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn an_archive_upload_becomes_pages_and_a_replacement_is_collected() {
    let w = world().await;
    let (s, e) = w.series(false).await;
    let c = w.draft(e, 1.0, ChapterFormat::Images).await;

    let first = upload(&w, c, "Chapter 1.CBZ", cbz(3)).await.unwrap();
    assert_eq!(w.queue.0.lock().unwrap().as_slice(), [first]);
    let queued = w.svc.upload_status(w.publisher, first).await.unwrap();
    assert_eq!(queued.status, UploadStatus::Queued);
    // A second upload while one is queued is refused.
    assert!(matches!(
        w.svc
            .begin_upload(
                w.publisher,
                c,
                UploadRequest {
                    filename: "again.cbz".into(),
                    size_bytes: 10,
                    multipart: false,
                }
            )
            .await
            .unwrap_err(),
        LibraryError::Conflict(_)
    ));

    w.svc.run_ingest(first, true).await.unwrap();
    let done = w.svc.upload_status(w.publisher, first).await.unwrap();
    assert_eq!(done.status, UploadStatus::Done, "{:?}", done.error);
    assert_eq!(done.page_count, Some(3));
    assert_eq!(done.metadata["title"], "Found Title");
    assert_eq!(done.metadata["number"], "3");
    // The upload file is gone; the pages are stored under a revision folder.
    assert!(!w.exists(&format!("library/{s}/{c}/src-{first}.cbz")).await);
    let manifest = w.svc.admin_page_manifest(w.publisher, c).await.unwrap();
    assert_eq!(manifest.pages.len(), 3);
    let old_keys: Vec<String> =
        sqlx::query_scalar("SELECT key FROM reading_pages WHERE chapter_id = $1 ORDER BY idx")
            .bind(c)
            .fetch_all(&w.pool)
            .await
            .unwrap();
    assert!(old_keys[0].contains(&format!("/r{first}/")), "{old_keys:?}");
    for k in &old_keys {
        assert!(w.exists(k).await, "{k}");
    }
    // Running the job again (a retried delivery) changes nothing.
    w.svc.run_ingest(first, true).await.unwrap();

    // Replace with a 2-page archive: the old pages go to the collector.
    let second = upload(&w, c, "v2.cbz", cbz(2)).await.unwrap();
    w.svc.run_ingest(second, true).await.unwrap();
    assert_eq!(
        w.svc
            .upload_status(w.publisher, second)
            .await
            .unwrap()
            .status,
        UploadStatus::Done
    );
    assert_eq!(
        w.svc
            .admin_page_manifest(w.publisher, c)
            .await
            .unwrap()
            .pages
            .len(),
        2
    );
    assert!(w.gc_queue(&old_keys[0]).await);
    // Not before the grace period.
    let r = w.svc.run_gc().await.unwrap();
    assert_eq!(r.objects_deleted, 0);
    assert!(w.exists(&old_keys[0]).await);
    w.clock.0.fetch_add(11 * 60 * 1000, Ordering::SeqCst);
    // The grace is measured by the database clock, so wait for it to catch up too.
    sqlx::query(
        "UPDATE reading_gc_keys SET queued_at = now() - interval '1 hour' WHERE key = ANY($1)",
    )
    .bind(&old_keys)
    .execute(&w.pool)
    .await
    .unwrap();
    let r = w.svc.run_gc().await.unwrap();
    assert!(r.objects_deleted >= 3, "{r:?}");
    for k in &old_keys {
        assert!(!w.exists(k).await, "{k} should be collected");
    }
    // The live pages survive.
    let live: Vec<String> =
        sqlx::query_scalar("SELECT key FROM reading_pages WHERE chapter_id = $1")
            .bind(c)
            .fetch_all(&w.pool)
            .await
            .unwrap();
    assert_eq!(live.len(), 2);
    for k in &live {
        assert!(w.exists(k).await);
    }
    // The access log hook, events: the uploader heard about both.
    let t = w.events.types(w.publisher.user_id);
    assert_eq!(t.iter().filter(|t| *t == "upload").count(), 2);
    // History of uploads.
    let listed = w.svc.chapter_uploads(w.publisher, c).await.unwrap();
    assert_eq!(listed.uploads.len(), 2);
    assert_eq!(listed.uploads[0].id, second);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn upload_requests_and_bad_files_are_refused_with_reasons() {
    let w = world_with(IngestSettingsSnapshot {
        rar_enabled: false,
        max_upload_bytes: 4 * 1024 * 1024,
    })
    .await;
    let (_s, e) = w.series(false).await;
    let c = w.draft(e, 1.0, ChapterFormat::Images).await;
    let begin = |name: &str, size: u64| {
        w.svc.begin_upload(
            w.publisher,
            c,
            UploadRequest {
                filename: name.into(),
                size_bytes: size,
                multipart: false,
            },
        )
    };
    assert!(matches!(
        begin("a.epub", 10).await.unwrap_err(),
        LibraryError::Invalid(_)
    ));
    assert!(matches!(
        begin("a.cbz", 0).await.unwrap_err(),
        LibraryError::Invalid(_)
    ));
    assert!(matches!(
        begin("a.cbz", 5 * 1024 * 1024).await.unwrap_err(),
        LibraryError::Invalid(m) if m.contains("between")
    ));
    // Only publishers.
    let nobody = Actor {
        user_id: w.member.user_id,
        permissions: P::empty(),
    };
    assert!(matches!(
        w.svc
            .begin_upload(
                nobody,
                c,
                UploadRequest {
                    filename: "a.cbz".into(),
                    size_bytes: 1,
                    multipart: false,
                }
            )
            .await
            .unwrap_err(),
        LibraryError::MissingPermission(_)
    ));
    assert!(matches!(
        w.svc
            .begin_upload(
                w.publisher,
                1,
                UploadRequest {
                    filename: "a.cbz".into(),
                    size_bytes: 1,
                    multipart: false,
                }
            )
            .await
            .unwrap_err(),
        LibraryError::NotFound(_)
    ));

    // Completing before the file arrives.
    let slot = begin("a.cbz", 100).await.unwrap();
    assert_eq!(slot.max_bytes, 4 * 1024 * 1024);
    assert_eq!(slot.upload_method, "PUT");
    assert!(matches!(
        w.svc
            .complete_upload(w.publisher, slot.upload.id)
            .await
            .unwrap_err(),
        LibraryError::Invalid(_)
    ));
    // The file is bigger than allowed although the request promised less.
    let key = format!(
        "library/{}/{c}/src-{}.cbz",
        slot.upload.series_id, slot.upload.id
    );
    w.storage
        .put(&key, Bytes::from(vec![1u8; 5 * 1024 * 1024]))
        .await
        .unwrap();
    assert!(matches!(
        w.svc
            .complete_upload(w.publisher, slot.upload.id)
            .await
            .unwrap_err(),
        LibraryError::Invalid(_)
    ));
    let failed = w
        .svc
        .upload_status(w.publisher, slot.upload.id)
        .await
        .unwrap();
    assert_eq!(failed.status, UploadStatus::Failed);
    assert!(!w.exists(&key).await, "an oversized file is deleted");
    assert!(matches!(
        w.svc
            .complete_upload(w.publisher, slot.upload.id)
            .await
            .unwrap_err(),
        LibraryError::Conflict(_)
    ));

    // Garbage with an archive extension fails in the job, with the reason, and is removed.
    let bad = upload(&w, c, "junk.cbz", vec![9u8; 1000]).await.unwrap();
    w.svc.run_ingest(bad, true).await.unwrap();
    let st = w.svc.upload_status(w.publisher, bad).await.unwrap();
    assert_eq!(st.status, UploadStatus::Failed);
    assert!(st.error.unwrap().contains("archive"), "reason is shown");
    assert!(
        !w.exists(&format!(
            "library/{}/{c}/src-{bad}.cbz",
            slot.upload.series_id
        ))
        .await
    );
    // And the chapter can take a new upload right away.
    assert!(begin("ok.cbz", 100).await.is_ok());

    // RAR is switched off by the operator setting.
    let c2 = w.draft(e, 2.0, ChapterFormat::Images).await;
    let rar = upload(
        &w,
        c2,
        "x.rar",
        b"Rar!\x1a\x07\x01\x00junkjunkjunk".to_vec(),
    )
    .await
    .unwrap();
    w.svc.run_ingest(rar, true).await.unwrap();
    let st = w.svc.upload_status(w.publisher, rar).await.unwrap();
    assert_eq!(st.status, UploadStatus::Failed);
    assert!(st.error.unwrap().contains("disabled by the operator"));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_pdf_is_kept_as_the_chapters_source_and_replacing_it_collects_the_old_one() {
    let w = world().await;
    let (s, e) = w.series(false).await;
    let c = w.draft(e, 1.0, ChapterFormat::Pdf).await;
    // A chapter without a file cannot be published.
    assert!(matches!(
        w.svc
            .set_chapter_published(w.publisher, c, true)
            .await
            .unwrap_err(),
        LibraryError::Conflict(_)
    ));
    assert!(matches!(
        w.svc
            .begin_upload(
                w.publisher,
                c,
                UploadRequest {
                    filename: "a.cbz".into(),
                    size_bytes: 5,
                    multipart: false,
                }
            )
            .await
            .unwrap_err(),
        LibraryError::Invalid(_)
    ));
    let up = upload(&w, c, "book.pdf", PDF.to_vec()).await.unwrap();
    w.svc.run_ingest(up, true).await.unwrap();
    let st = w.svc.upload_status(w.publisher, up).await.unwrap();
    assert_eq!(st.status, UploadStatus::Done, "{:?}", st.error);
    assert_eq!(st.metadata["pdf_version"], "1.7");
    let key = format!("library/{s}/{c}/src-{up}.pdf");
    assert!(w.exists(&key).await, "the original is kept");
    w.svc
        .set_chapter_published(w.publisher, c, true)
        .await
        .unwrap();
    w.publish_series(s).await;
    let m = w.svc.page_manifest(w.member, c).await.unwrap();
    assert!(m.pages.is_empty());
    assert!(
        m.source_url
            .as_deref()
            .unwrap()
            .contains(&format!("/cdn/{key}"))
    );

    // A metadata edit through the catalog API does not wipe the file.
    w.svc
        .update_chapter(
            w.publisher,
            c,
            ChapterInput {
                number: 1.0,
                title: Some("Renamed".into()),
                volume_id: None,
                format: ChapterFormat::Pdf,
                source_key: None,
                release_group: None,
            },
        )
        .await
        .unwrap();
    assert!(!w.gc_queue(&key).await);

    let up2 = upload(&w, c, "book2.pdf", PDF.to_vec()).await.unwrap();
    w.svc.run_ingest(up2, true).await.unwrap();
    assert!(w.gc_queue(&key).await, "the replaced original is queued");
    // A key that became referenced again is dropped from the queue instead of deleted.
    let new_key = format!("library/{s}/{c}/src-{up2}.pdf");
    sqlx::query("INSERT INTO reading_gc_keys (key, queued_at) VALUES ($1, now() - interval '1 hour') ON CONFLICT (key) DO UPDATE SET queued_at = EXCLUDED.queued_at")
        .bind(&new_key)
        .execute(&w.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE reading_gc_keys SET queued_at = now() - interval '1 hour' WHERE key = $1")
        .bind(&key)
        .execute(&w.pool)
        .await
        .unwrap();
    w.clock.0.fetch_add(11 * 60 * 1000, Ordering::SeqCst);
    w.svc.run_gc().await.unwrap();
    assert!(!w.exists(&key).await);
    assert!(
        w.exists(&new_key).await,
        "the current source is never collected"
    );
    assert!(!w.gc_queue(&new_key).await);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn deleting_chapters_and_series_queues_their_objects() {
    let w = world().await;
    let (s, e) = w.series(false).await;
    let keep = w.chapter(s, e, 1.0, 2).await;
    let gone = w.chapter(s, e, 2.0, 2).await;
    for c in [keep, gone] {
        for p in pages(s, c, 2) {
            w.storage
                .put(&p.key, Bytes::from_static(b"x"))
                .await
                .unwrap();
            for v in p.variants.values() {
                w.storage.put(v, Bytes::from_static(b"x")).await.unwrap();
            }
        }
    }
    w.svc.delete_chapter(w.publisher, gone).await.unwrap();
    assert!(w.gc_queue(&format!("library/{s}/{gone}/0000.webp")).await);
    assert!(
        w.gc_queue(&format!("library/{s}/{gone}/0001_720.webp"))
            .await
    );
    assert!(!w.gc_queue(&format!("library/{s}/{keep}/0000.webp")).await);

    // Replacing pages queues what dropped out but not what stays.
    let mut shorter = pages(s, keep, 1);
    shorter[0].variants.clear();
    w.svc
        .replace_pages(w.publisher, keep, shorter)
        .await
        .unwrap();
    assert!(w.gc_queue(&format!("library/{s}/{keep}/0001.webp")).await);
    assert!(
        w.gc_queue(&format!("library/{s}/{keep}/0000_720.webp"))
            .await
    );
    assert!(!w.gc_queue(&format!("library/{s}/{keep}/0000.webp")).await);

    // A cover swap queues the old cover.
    let mut input = w.input("Cover", false);
    input.cover_key = Some(format!("library/{s}/cover-a.webp"));
    let s2 = w
        .svc
        .create_series(w.publisher, input.clone())
        .await
        .unwrap();
    input.cover_key = Some(format!("library/{}/cover-b.webp", s2.id));
    w.svc
        .update_series(w.publisher, s2.id, input)
        .await
        .unwrap();
    assert!(w.gc_queue(&format!("library/{s}/cover-a.webp")).await);

    // Deleting the series queues the rest, then the sweep removes the objects.
    w.svc.delete_series(w.publisher, s).await.unwrap();
    assert!(w.gc_queue(&format!("library/{s}/{keep}/0000.webp")).await);
    sqlx::query("UPDATE reading_gc_keys SET queued_at = now() - interval '1 hour'")
        .execute(&w.pool)
        .await
        .unwrap();
    w.clock.0.fetch_add(11 * 60 * 1000, Ordering::SeqCst);
    let before = w.exists(&format!("library/{s}/{gone}/0000.webp")).await;
    assert!(before);
    // Other tests' queued rows may be swept too; ours must be gone.
    for _ in 0..3 {
        w.svc.run_gc().await.unwrap();
    }
    for k in [
        format!("library/{s}/{gone}/0000.webp"),
        format!("library/{s}/{gone}/0001_720.webp"),
        format!("library/{s}/{keep}/0000.webp"),
    ] {
        assert!(!w.exists(&k).await, "{k}");
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn abandoned_upload_slots_expire_and_are_cleaned() {
    let w = world().await;
    let (_s, e) = w.series(false).await;
    let c = w.draft(e, 1.0, ChapterFormat::Images).await;
    let slot = w
        .svc
        .begin_upload(
            w.publisher,
            c,
            UploadRequest {
                filename: "a.cbz".into(),
                size_bytes: 10,
                multipart: false,
            },
        )
        .await
        .unwrap();
    let key = format!(
        "library/{}/{c}/src-{}.cbz",
        slot.upload.series_id, slot.upload.id
    );
    w.storage
        .put(&key, Bytes::from_static(b"half an upload"))
        .await
        .unwrap();
    // A fresh slot is left alone.
    w.svc.run_gc().await.unwrap();
    assert_eq!(
        w.svc
            .upload_status(w.publisher, slot.upload.id)
            .await
            .unwrap()
            .status,
        UploadStatus::AwaitingUpload
    );
    // A day later it expires; its object is queued, then (after the grace) deleted.
    w.clock.0.fetch_add(25 * 3600 * 1000, Ordering::SeqCst);
    sqlx::query(
        "UPDATE reading_uploads SET created_at = now() - interval '25 hours' WHERE id = $1",
    )
    .bind(slot.upload.id)
    .execute(&w.pool)
    .await
    .unwrap();
    let r = w.svc.run_gc().await.unwrap();
    assert!(r.uploads_expired >= 1);
    assert_eq!(
        w.svc
            .upload_status(w.publisher, slot.upload.id)
            .await
            .unwrap()
            .status,
        UploadStatus::Expired
    );
    assert!(matches!(
        w.svc
            .complete_upload(w.publisher, slot.upload.id)
            .await
            .unwrap_err(),
        LibraryError::Conflict(_)
    ));
    sqlx::query("UPDATE reading_gc_keys SET queued_at = now() - interval '1 hour' WHERE key = $1")
        .bind(&key)
        .execute(&w.pool)
        .await
        .unwrap();
    w.svc.run_gc().await.unwrap();
    assert!(!w.exists(&key).await);
    // A new slot supersedes an unused one without waiting for the expiry.
    let a = w
        .svc
        .begin_upload(
            w.publisher,
            c,
            UploadRequest {
                filename: "a.cbz".into(),
                size_bytes: 10,
                multipart: false,
            },
        )
        .await
        .unwrap();
    let b = w
        .svc
        .begin_upload(
            w.publisher,
            c,
            UploadRequest {
                filename: "b.cbz".into(),
                size_bytes: 10,
                multipart: false,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        w.svc
            .upload_status(w.publisher, a.upload.id)
            .await
            .unwrap()
            .status,
        UploadStatus::Expired
    );
    assert_eq!(b.upload.status, UploadStatus::AwaitingUpload);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn publishing_a_chapter_notifies_followers_with_the_bell_on() {
    let w = world().await;
    let (s, e) = w.series(false).await;
    w.chapter(s, e, 1.0, 1).await;
    w.publish_series(s).await;
    let bell = w.member;
    let quiet = w.minor;
    w.svc.follow(bell, s, Shelf::Reading, true).await.unwrap();
    w.svc.follow(quiet, s, Shelf::Reading, false).await.unwrap();
    let c2 = w.draft(e, 2.0, ChapterFormat::Images).await;
    w.svc
        .replace_pages(w.publisher, c2, pages(s, c2, 1))
        .await
        .unwrap();
    w.events.0.lock().unwrap().clear();
    w.svc
        .set_chapter_published(w.publisher, c2, true)
        .await
        .unwrap();
    // The fan-out runs off the request path.
    let mut got = false;
    for _ in 0..100 {
        if w.events
            .types(bell.user_id)
            .contains(&"chapter_published".to_owned())
        {
            got = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(got, "the follower with the bell on is told");
    assert!(w.events.types(quiet.user_id).is_empty(), "the bell is off");
    let p = w
        .events
        .0
        .lock()
        .unwrap()
        .iter()
        .find(|(_, p)| p["type"] == "chapter_published")
        .unwrap()
        .1
        .clone();
    assert_eq!(p["chapter_id"], c2.to_string());
    assert_eq!(p["chapter_number"], 2.0);
    // Unpublishing and publishing again is a new release.
    w.svc
        .set_chapter_published(w.publisher, c2, false)
        .await
        .unwrap();
    w.events.0.lock().unwrap().clear();
    w.svc
        .set_chapter_published(w.publisher, c2, true)
        .await
        .unwrap();
    let mut again = false;
    for _ in 0..100 {
        if w.events
            .types(bell.user_id)
            .contains(&"chapter_published".to_owned())
        {
            again = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(again, "unpublish then publish is a new release");
    // Follow changes sync across devices.
    w.svc.unfollow(bell, s).await.unwrap();
    assert!(
        w.events
            .types(bell.user_id)
            .contains(&"unfollow".to_owned())
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_chapter_export_is_a_cbz_of_the_pages_cached_by_page_list() {
    let w = world().await;
    let (s, e) = w.series(false).await;
    let c = w.chapter(s, e, 36.0, 3).await;
    w.publish_series(s).await;
    for i in 0..3 {
        w.storage
            .put(
                &format!("library/{s}/{c}/{i:04}.webp"),
                Bytes::from(vec![i as u8; 10 + i]),
            )
            .await
            .unwrap();
    }
    let first = w.svc.export_chapter(w.member, c).await.unwrap();
    assert_eq!(first.format, "cbz");
    assert!(first.filename.ends_with("-ch-36.cbz"), "{}", first.filename);
    assert!(first.url.contains("/cdn/library/exports/"));
    let key = first
        .url
        .split("/cdn/")
        .nth(1)
        .unwrap()
        .split('?')
        .next()
        .unwrap()
        .to_owned();
    let bytes = w.storage.get(&key).await.unwrap();
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).unwrap();
    assert_eq!(zip.len(), 3);
    assert_eq!(
        zip.by_index(0).unwrap().name().unwrap().to_string(),
        "001.webp"
    );
    assert_eq!(first.size_bytes, bytes.len() as u64);
    // The same page list reuses the stored file.
    let again = w.svc.export_chapter(w.member, c).await.unwrap();
    assert_eq!(again.size_bytes, first.size_bytes);
    // An unpublished chapter and a minor on a restricted series are refused.
    let (rs, re) = w.series(true).await;
    let rc = w.chapter(rs, re, 1.0, 1).await;
    w.publish_series(rs).await;
    assert!(matches!(
        w.svc.export_chapter(w.minor, rc).await.unwrap_err(),
        LibraryError::AgeConfirmationRequired
    ));
    w.svc
        .set_chapter_published(w.publisher, c, false)
        .await
        .unwrap();
    assert!(matches!(
        w.svc.export_chapter(w.member, c).await.unwrap_err(),
        LibraryError::NotFound(_)
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_collector_deletes_exports_nobody_read_for_a_week_and_adopts_strays() {
    let w = world().await;
    let (s, e) = w.series(false).await;
    let c = w.chapter(s, e, 36.0, 2).await;
    w.publish_series(s).await;
    for i in 0..2 {
        w.storage
            .put(
                &format!("library/{s}/{c}/{i:04}.webp"),
                Bytes::from(vec![i as u8; 10]),
            )
            .await
            .unwrap();
    }
    let export = w.svc.export_chapter(w.member, c).await.unwrap();
    let key = export
        .url
        .split("/cdn/")
        .nth(1)
        .unwrap()
        .split('?')
        .next()
        .unwrap()
        .to_owned();
    // Building it recorded a read; a fresh export survives the sweep.
    let rows = || async {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM library_export_cache WHERE export_key = $1")
            .bind(&key)
            .fetch_one(&w.pool)
            .await
            .unwrap()
    };
    assert_eq!(rows().await, 1);
    w.svc.run_gc().await.unwrap();
    assert!(w.exists(&key).await);

    // A stray from before the index existed is adopted as read now, not deleted.
    let stray = format!("library/exports/{c}/00000000deadbeef.cbz");
    w.storage.put(&stray, Bytes::from_static(b"zip")).await.unwrap();
    let r = w.svc.run_gc().await.unwrap();
    assert_eq!(r.exports_deleted, 0);
    assert!(w.exists(&stray).await);

    // Serving it again refreshes the read; 8 idle days make the sweep delete it.
    sqlx::query("UPDATE library_export_cache SET last_read_at = now() - interval '8 days' WHERE export_key = ANY($1)")
        .bind(vec![key.clone(), stray.clone()])
        .execute(&w.pool)
        .await
        .unwrap();
    w.svc.export_chapter(w.member, c).await.unwrap();
    let r = w.svc.run_gc().await.unwrap();
    assert_eq!(r.exports_deleted, 1, "only the stray was idle: {r:?}");
    assert!(w.exists(&key).await, "a read keeps the export");
    assert!(!w.exists(&stray).await);
    sqlx::query("UPDATE library_export_cache SET last_read_at = now() - interval '8 days' WHERE export_key = $1")
        .bind(&key)
        .execute(&w.pool)
        .await
        .unwrap();
    let r = w.svc.run_gc().await.unwrap();
    assert_eq!(r.exports_deleted, 1);
    assert!(!w.exists(&key).await);
    assert_eq!(rows().await, 0);
    // It is rebuilt on demand.
    assert!(w.svc.export_chapter(w.member, c).await.is_ok());
    assert!(w.exists(&key).await);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_multipart_upload_is_remembered_resumed_and_completed() {
    use sr_library::{MemoryPartStore, MultipartWiring};
    let w = world().await;
    let (s, e) = w.series(false).await;
    let c = w.chapter(s, e, 1.0, 1).await;
    let parts = Arc::new(MemoryPartStore::new(w.storage.clone()));
    let svc = w
        .svc
        .clone()
        .with_multipart(MultipartWiring::new(parts.clone()).with_part_size(4));
    let body = cbz(2);
    let slot = svc
        .begin_upload(
            w.publisher,
            c,
            UploadRequest {
                filename: "Ch 1.cbz".into(),
                size_bytes: body.len() as u64,
                multipart: true,
            },
        )
        .await
        .unwrap();
    let m = slot.multipart.clone().expect("a multipart slot");
    assert_eq!(m.part_size, 4);
    assert_eq!(u64::from(m.part_count), (body.len() as u64).div_ceil(4));
    let remembered: (String, i64) = sqlx::query_as(
        "SELECT multipart_id, part_size FROM reading_upload_multipart WHERE upload_id = $1",
    )
    .bind(slot.upload.id)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    assert_eq!(remembered.1, 4);

    // Send every part but the first, as a browser that refreshed halfway would have.
    for (i, chunk) in body.chunks(4).enumerate().skip(1) {
        parts.put_part(&remembered.0, i as u32 + 1, Bytes::copy_from_slice(chunk));
    }
    let resume = svc.upload_parts(w.publisher, slot.upload.id).await.unwrap();
    assert_eq!(resume.missing.len(), 1);
    assert_eq!(resume.missing[0].part_number, 1);
    assert!(
        svc.complete_upload(w.publisher, slot.upload.id)
            .await
            .is_err()
    );
    parts.put_part(&remembered.0, 1, Bytes::copy_from_slice(&body[..4]));
    let up = svc
        .complete_upload(w.publisher, slot.upload.id)
        .await
        .unwrap();
    assert_eq!(up.status.as_str(), "queued");
    assert_eq!(w.queue.0.lock().unwrap().as_slice(), [slot.upload.id]);
    // The slot is spent: no parts to resume, nothing to cancel.
    assert!(svc.upload_parts(w.publisher, slot.upload.id).await.is_err());
    assert!(svc.abort_upload(w.publisher, slot.upload.id).await.is_err());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn duplicates_match_by_title_and_by_cover_artwork() {
    use sr_library::{CoverHashes, SeriesIndex};
    let w = world().await;
    let svc = w
        .svc
        .clone()
        .with_index(Arc::new(SeriesIndex::in_ram().unwrap()))
        .with_cover_hashes(Arc::new(PgLibraryRepo::new(w.pool.clone())));
    let tag = uid();
    let title = format!("Zephyr Quill {tag}");
    let art = png(600, 900, 10);
    let other_art = {
        use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
        let img = RgbImage::from_fn(600, 900, |x, _| {
            if (x / 12) % 2 == 0 {
                Rgb([15, 15, 15])
            } else {
                Rgb([240, 240, 240])
            }
        });
        let mut out = Vec::new();
        DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .unwrap();
        out
    };
    let put_cover = |name: String, bytes: Vec<u8>| {
        let storage = w.storage.clone();
        async move {
            let key = format!("library/cover-{name}.png");
            storage.put(&key, Bytes::from(bytes)).await.unwrap();
            key
        }
    };
    let make = |title: String, key: String, publish: bool| {
        let svc = svc.clone();
        let mut input = w.input("x", false);
        input.title = title;
        input.cover_key = Some(key);
        let actor = w.publisher;
        async move {
            let s = svc.create_series(actor, input).await.unwrap();
            if publish {
                svc.set_series_published(actor, s.id, true).await.unwrap();
            }
            s.id
        }
    };
    let same_title = make(
        title.clone(),
        put_cover(format!("a{tag}"), art.clone()).await,
        true,
    )
    .await;
    let same_art = make(
        format!("Unrelated Words {tag}"),
        put_cover(format!("b{tag}"), art.clone()).await,
        true,
    )
    .await;
    let _different = make(
        format!("Nothing Alike {tag}"),
        put_cover(format!("c{tag}"), other_art).await,
        true,
    )
    .await;
    let draft = make(
        title.clone(),
        put_cover(format!("d{tag}"), art).await,
        false,
    )
    .await;

    let found = svc.find_duplicates(draft).await.unwrap();
    let by_id = |id: i64| found.iter().find(|m| m.ref_id == id);
    let t = by_id(same_title).expect("the series with the same title");
    assert_eq!(t.score, 100);
    assert!(t.reasons.contains(&"title".to_owned()), "{t:?}");
    assert!(t.reasons.contains(&"cover".to_owned()), "{t:?}");
    let a = by_id(same_art).expect("the series with the same cover");
    assert_eq!(a.reasons, vec!["cover".to_owned()]);
    assert!(a.score >= 90, "{a:?}");
    assert!(found.iter().all(|m| m.ref_id != draft), "never itself");
    assert!(
        found
            .iter()
            .all(|m| !m.title.starts_with("Nothing Alike") || !m.title.ends_with(&tag.to_string())),
        "different artwork and title are not matches: {found:?}"
    );
    // The hashes were filled in as a side effect.
    let left = CoverHashes::unhashed(&PgLibraryRepo::new(w.pool.clone()), 1000)
        .await
        .unwrap();
    assert!(
        left.iter()
            .all(|(id, _)| *id != same_title && *id != same_art)
    );
}
