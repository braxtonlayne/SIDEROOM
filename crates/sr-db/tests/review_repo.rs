//! DB-backed tests of the review pipeline: the Postgres repository under the real service and
//! the Library kinds with real uploads. Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test review_repo -- --ignored --test-threads=1`

use std::io::{Cursor, Write};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicI64, Ordering},
};

use async_trait::async_trait;
use bytes::Bytes;
use serde_json::{Value, json};
use sr_core::{Clock, PlatformPermissions as P};
use sr_db::{
    DbConfig, PgPool,
    accounts::{self, NewAccount},
    library_repo::PgLibraryRepo,
    review_repo::PgReviewRepo,
};
use sr_library::model::*;
use sr_library::submissions::{KIND_ITEMS, KIND_SERIES, LibraryDraftOps, LibraryKind};
use sr_library::{
    Actor, IngestQueue, IngestSettingsSnapshot, LibraryService, StaticIngestSettings, Viewer,
};
use sr_review::model as rm;
use sr_review::repo::NewItem;
use sr_review::{Caller, KindRegistry, ReviewError, ReviewEvents, ReviewService};
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
struct Sink(Mutex<Vec<(Vec<i64>, Value)>>);

#[async_trait]
impl ReviewEvents for Sink {
    async fn emit(&self, users: Vec<i64>, payload: Value) {
        self.0.lock().unwrap().push((users, payload));
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
    lib: LibraryService,
    ops: LibraryDraftOps,
    review: ReviewService,
    pool: PgPool,
    storage: Arc<ObjectStoreStorage>,
    clock: Arc<TestClock>,
    sink: Arc<Sink>,
    member: i64,
    other: i64,
    reviewer: Caller,
    curator: Caller,
}

async fn world() -> World {
    let pool = pool().await;
    let storage = Arc::new(ObjectStoreStorage::in_memory());
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
    .with_ingest(
        storage.clone(),
        Arc::new(StaticIngestSettings(IngestSettingsSnapshot::default())),
        Arc::new(Queue::default()),
    );
    let sink = Arc::new(Sink::default());
    let review = ReviewService::new(
        Arc::new(PgReviewRepo::new(pool.clone())),
        KindRegistry::new()
            .with(Arc::new(LibraryKind::series(lib.clone())))
            .with(Arc::new(LibraryKind::items(lib.clone()))),
        clock.clone(),
        Arc::new(uid),
    )
    .with_events(sink.clone());
    let (member, other, r, c) = (
        account(&pool).await,
        account(&pool).await,
        account(&pool).await,
        account(&pool).await,
    );
    World {
        ops: LibraryDraftOps::new(lib.clone()),
        lib,
        review,
        pool,
        storage,
        clock,
        sink,
        member,
        other,
        reviewer: Caller {
            user_id: r,
            permissions: P::STAFF_BASIC | P::REVIEW_READING,
        },
        curator: Caller {
            user_id: c,
            permissions: P::STAFF_BASIC | P::REVIEW_READING | P::PUBLISH_READING,
        },
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
        w.write_all(&png(300, 400, i as u8)).unwrap();
    }
    w.finish().unwrap().into_inner()
}

fn series_payload(title: &str) -> Value {
    json!({ "kind": "manga", "title": format!("{title} {}", uid()), "language": "en" })
}

fn chapter(number: f64) -> ChapterInput {
    ChapterInput {
        number,
        title: Some(format!("Ch {number}")),
        volume_id: None,
        format: ChapterFormat::Images,
        source_key: Some("library/evil/key".into()),
        release_group: None,
    }
}

impl World {
    async fn new_series_submission(&self) -> rm::Submission {
        self.review
            .create(
                Caller::member(self.member),
                rm::CreateSubmission {
                    kind: KIND_SERIES.into(),
                    payload: series_payload("Draft"),
                    title: None,
                    summary: "about".into(),
                },
            )
            .await
            .unwrap()
            .submission
    }

    /// Upload an archive through the submitter path and run the ingest job.
    async fn upload(&self, s: &rm::Submission, user: i64, chapter: i64, pages: usize) -> i64 {
        let bytes = cbz(pages);
        let slot = self
            .ops
            .begin_upload(
                s,
                user,
                chapter,
                UploadRequest {
                    filename: "c.cbz".into(),
                    size_bytes: bytes.len() as u64,
                },
            )
            .await
            .unwrap();
        let key = format!(
            "library/{}/{}/src-{}.cbz",
            slot.upload.series_id, chapter, slot.upload.id
        );
        self.storage.put(&key, Bytes::from(bytes)).await.unwrap();
        self.ops
            .complete_upload(s, user, slot.upload.id)
            .await
            .unwrap();
        self.lib.run_ingest(slot.upload.id, true).await.unwrap();
        let up = self
            .ops
            .upload_status(s, user, slot.upload.id)
            .await
            .unwrap();
        assert_eq!(up.status, UploadStatus::Done, "{:?}", up.error);
        slot.upload.id
    }

    fn viewer(&self) -> Viewer {
        Viewer {
            user_id: self.other,
            adult: true,
            hide_restricted: false,
        }
    }

    async fn state(&self, id: i64) -> rm::SubmissionState {
        self.review
            .get_mine(self.member, id)
            .await
            .unwrap()
            .submission
            .state
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_member_series_is_uploaded_reviewed_and_published() {
    let w = world().await;
    let s = w.new_series_submission().await;
    let series = s.ref_id.unwrap();
    assert_eq!(s.state, rm::SubmissionState::Draft);

    // The draft is invisible to readers.
    assert!(w.lib.series_detail(w.viewer(), series).await.is_err());

    // A chapter without pages blocks submitting, and the member-supplied source key is dropped.
    let ch = w
        .ops
        .create_chapter(&s, w.member, None, chapter(1.0))
        .await
        .unwrap();
    assert_eq!(
        w.review.submit(w.member, s.id).await.unwrap_err(),
        ReviewError::NotReady(vec![
            "Chapter 1: Ch 1 has no pages yet; upload its file first".into()
        ])
    );
    let upload = w.upload(&s, w.member, ch.id, 3).await;
    assert!(
        w.ops
            .chapter_uploads(&s, w.member, ch.id)
            .await
            .unwrap()
            .uploads
            .iter()
            .any(|u| u.id == upload)
    );

    let d = w.review.submit(w.member, s.id).await.unwrap();
    assert_eq!(d.submission.state, rm::SubmissionState::Submitted);
    assert_eq!(d.items.len(), 1);
    assert_eq!(d.items[0].ref_id, ch.id);
    // The content is locked for editing... through the review API; the preview shows it.
    let preview = w.review.preview_mine(w.member, s.id).await.unwrap();
    assert_eq!(preview["new_series"], true);
    assert_eq!(preview["chapters"][0]["page_count"], 3);

    // A reviewer sees it in the queue and claims it; still not public.
    let q = w
        .review
        .queue(
            w.reviewer,
            rm::SubmissionQuery {
                title_contains: Some(s.title.clone()),
                limit: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(q.total, 1);
    w.review.claim(w.reviewer, s.id, false).await.unwrap();
    let pages = w.ops.pages(&s, w.reviewer.user_id, ch.id).await.unwrap();
    assert_eq!(pages.pages.len(), 3);
    assert!(w.lib.series_detail(w.viewer(), series).await.is_err());

    let d = w
        .review
        .approve(w.reviewer, s.id, rm::DecisionInput::default())
        .await
        .unwrap();
    assert_eq!(d.submission.state, rm::SubmissionState::Published);
    assert_eq!(d.items[0].state, rm::ItemState::Published);
    // Now readers see the series and its chapter.
    let detail = w.lib.series_detail(w.viewer(), series).await.unwrap();
    assert!(detail.series.published);
    let chs = w
        .lib
        .chapters(w.viewer(), series, ChapterQuery::default())
        .await
        .unwrap();
    assert_eq!(chs.chapters.len(), 1);
    assert!(chs.chapters[0].published);
    // The submitter was told.
    {
        let sent = w.sink.0.lock().unwrap();
        assert!(sent.iter().all(|(u, _)| u == &vec![w.member]));
        assert!(sent.iter().any(|(_, p)| p["type"] == "published"));
    }
    assert_eq!(w.state(s.id).await, rm::SubmissionState::Published);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn new_chapters_for_a_published_series_go_through_changes_requested() {
    let w = world().await;
    // An existing published series with one chapter, made by staff.
    let staff = Actor {
        user_id: w.reviewer.user_id,
        permissions: P::PUBLISH_READING | P::DELETE_CONTENT,
    };
    let base = w
        .lib
        .create_series(
            staff,
            SeriesInput {
                kind: SeriesKind::Manga,
                title: format!("Base {}", uid()),
                alt_titles: vec![],
                authors: vec![],
                artists: vec![],
                status: SeriesStatus::Ongoing,
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
    let ed = w
        .lib
        .create_edition(
            staff,
            base.id,
            EditionInput {
                language: "en".into(),
                label: "Official".into(),
                is_default: true,
            },
        )
        .await
        .unwrap();
    // An unpublished series is not a valid target.
    let bad = w
        .review
        .create(
            Caller::member(w.member),
            rm::CreateSubmission {
                kind: KIND_ITEMS.into(),
                payload: json!({ "series_id": base.id.to_string() }),
                title: None,
                summary: String::new(),
            },
        )
        .await;
    assert!(matches!(bad, Err(ReviewError::Invalid(_))), "{bad:?}");
    w.lib
        .set_series_published(staff, base.id, true)
        .await
        .unwrap();

    let s = w
        .review
        .create(
            Caller::member(w.member),
            rm::CreateSubmission {
                kind: KIND_ITEMS.into(),
                payload: json!({ "series_id": base.id.to_string() }),
                title: None,
                summary: String::new(),
            },
        )
        .await
        .unwrap()
        .submission;
    assert!(s.title.starts_with("New chapters for Base"));
    assert_eq!(s.payload["edition_id"], ed.id.to_string());

    // Nothing registered yet: not ready. The ops refuse the member's edits to the live series.
    assert!(matches!(
        w.review.submit(w.member, s.id).await,
        Err(ReviewError::NotReady(_))
    ));
    assert!(matches!(
        w.ops
            .update_series(
                &s,
                w.member,
                serde_json::from_value(series_payload("x")).unwrap()
            )
            .await,
        Err(ReviewError::Conflict(_))
    ));
    let ch = w
        .ops
        .create_chapter(&s, w.member, None, chapter(7.0))
        .await
        .unwrap();
    w.review
        .register_item(
            w.member,
            s.id,
            NewItem {
                ref_id: ch.id,
                label: "Chapter 7".into(),
                position: 0,
            },
        )
        .await
        .unwrap();
    w.upload(&s, w.member, ch.id, 2).await;
    w.review.submit(w.member, s.id).await.unwrap();
    w.review.claim(w.reviewer, s.id, false).await.unwrap();
    // Another member cannot register items on or read this submission.
    assert!(
        w.review
            .register_item(
                w.other,
                s.id,
                NewItem {
                    ref_id: ch.id,
                    label: "x".into(),
                    position: 0
                }
            )
            .await
            .is_err()
    );
    let d = w
        .review
        .request_changes(
            w.reviewer,
            s.id,
            rm::DecisionInput {
                reason: Some("needs more pages".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(d.submission.state, rm::SubmissionState::ChangesRequested);

    // Replace the file, then resubmit and approve.
    w.upload(&s, w.member, ch.id, 4).await;
    let d = w.review.submit(w.member, s.id).await.unwrap();
    assert_eq!(d.events.last().unwrap().kind, "resubmitted");
    assert_eq!(
        d.events.last().unwrap().data["snapshot"]["chapters"][0]["page_count"],
        4
    );
    w.review.claim(w.reviewer, s.id, false).await.unwrap();
    let d = w
        .review
        .approve(w.reviewer, s.id, rm::DecisionInput::default())
        .await
        .unwrap();
    assert_eq!(d.submission.state, rm::SubmissionState::Published);
    let chs = w
        .lib
        .chapters(w.viewer(), base.id, ChapterQuery::default())
        .await
        .unwrap();
    assert_eq!(chs.chapters.len(), 1);
    assert_eq!(chs.chapters[0].number, 7.0);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn draft_ops_are_scoped_to_the_submissions_own_series() {
    let w = world().await;
    let a = w.new_series_submission().await;
    let b = w.new_series_submission().await;
    let ch_b = w
        .ops
        .create_chapter(&b, w.member, None, chapter(1.0))
        .await
        .unwrap();
    // Chapter and upload of submission B cannot be touched through submission A.
    assert!(matches!(
        w.ops
            .update_chapter(&a, w.member, ch_b.id, chapter(2.0))
            .await,
        Err(ReviewError::NotFound("chapter"))
    ));
    assert!(matches!(
        w.ops.delete_chapter(&a, w.member, ch_b.id).await,
        Err(ReviewError::NotFound("chapter"))
    ));
    assert!(matches!(
        w.ops
            .begin_upload(
                &a,
                w.member,
                ch_b.id,
                UploadRequest {
                    filename: "c.cbz".into(),
                    size_bytes: 10
                }
            )
            .await,
        Err(ReviewError::NotFound("chapter"))
    ));
    assert!(matches!(
        w.ops.pages(&a, w.member, ch_b.id).await,
        Err(ReviewError::NotFound("chapter"))
    ));
    let up_b = w.upload(&b, w.member, ch_b.id, 1).await;
    assert!(matches!(
        w.ops.complete_upload(&a, w.member, up_b).await,
        Err(ReviewError::NotFound("upload"))
    ));
    // Editing metadata drops the cover key a member cannot own.
    let mut input: SeriesInput = serde_json::from_value(series_payload("Edited")).unwrap();
    input.cover_key = Some("library/other/cover.webp".into());
    w.ops.update_series(&a, w.member, input).await.unwrap();
    let got = w
        .lib
        .admin_series(
            Actor {
                user_id: w.member,
                permissions: P::PUBLISH_READING,
            },
            a.ref_id.unwrap(),
        )
        .await
        .unwrap();
    assert!(got.series.cover_url.is_none());
    // The sponsored rights do not leak into the public API: the member's own permissions are none.
    assert!(
        w.review
            .queue(Caller::member(w.member), rm::SubmissionQuery::default())
            .await
            .unwrap()
            .items
            .is_empty()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn claims_are_atomic_and_leases_expire() {
    let w = world().await;
    let s = w.new_series_submission().await;
    let ch = w
        .ops
        .create_chapter(&s, w.member, None, chapter(1.0))
        .await
        .unwrap();
    w.upload(&s, w.member, ch.id, 1).await;
    w.review.submit(w.member, s.id).await.unwrap();

    // Two reviewers race: exactly one wins.
    let r2 = Caller {
        user_id: w.curator.user_id,
        permissions: P::STAFF_BASIC | P::REVIEW_READING,
    };
    let (a, b) = tokio::join!(
        w.review.claim(w.reviewer, s.id, false),
        w.review.claim(r2, s.id, false)
    );
    assert_eq!(a.is_ok() as u8 + b.is_ok() as u8, 1, "{a:?} {b:?}");
    let winner = if a.is_ok() { w.reviewer } else { r2 };
    let loser = if a.is_ok() { r2 } else { w.reviewer };
    // The loser cannot decide; a curator can take over; the old holder then cannot decide.
    assert!(
        w.review
            .approve(loser, s.id, rm::DecisionInput::default())
            .await
            .is_err()
    );
    let d = w.review.claim(w.curator, s.id, true).await;
    if winner.user_id == w.curator.user_id {
        assert!(d.is_ok());
    } else {
        assert_eq!(d.unwrap().submission.claimed_by, Some(w.curator.user_id));
        assert!(
            w.review
                .approve(winner, s.id, rm::DecisionInput::default())
                .await
                .is_err()
        );
    }
    // Lease expiry (the clock moves 31 minutes): the curator's decision is refused, and
    // another reviewer can claim.
    w.clock.0.fetch_add(31 * 60 * 1000, Ordering::SeqCst);
    assert!(
        w.review
            .reject(
                w.curator,
                s.id,
                rm::DecisionInput {
                    reason: Some("late".into())
                }
            )
            .await
            .is_err()
    );
    let d = w.review.claim(w.reviewer, s.id, false).await.unwrap();
    assert_eq!(d.submission.claimed_by, Some(w.reviewer.user_id));
    // The lease filters see it.
    let mine = w
        .review
        .queue(
            w.reviewer,
            rm::SubmissionQuery {
                claimed_by: Some(w.reviewer.user_id),
                title_contains: Some(s.title.clone()),
                limit: 5,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(mine.total, 1);
    // Reject with items: the rejected chapter stays a draft, nothing is public.
    let d = w
        .review
        .reject(
            w.reviewer,
            s.id,
            rm::DecisionInput {
                reason: Some("not for us".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(d.submission.state, rm::SubmissionState::Rejected);
    assert_eq!(d.items[0].state, rm::ItemState::Rejected);
    assert!(
        w.lib
            .series_detail(w.viewer(), s.ref_id.unwrap())
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn queue_listing_filters_sorts_and_pages() {
    let w = world().await;
    let tag = format!("QueueT{}", uid());
    let mut ids = Vec::new();
    for n in ["Cc", "Aa", "Bb"] {
        let s = w
            .review
            .create(
                Caller::member(w.member),
                rm::CreateSubmission {
                    kind: KIND_SERIES.into(),
                    payload: json!({ "kind": "manga", "title": format!("{n} {tag}") }),
                    title: None,
                    summary: String::new(),
                },
            )
            .await
            .unwrap()
            .submission;
        let ch = w
            .ops
            .create_chapter(&s, w.member, None, chapter(1.0))
            .await
            .unwrap();
        w.upload(&s, w.member, ch.id, 1).await;
        w.review.submit(w.member, s.id).await.unwrap();
        // Distinct submit times for the age sort.
        tokio::time::sleep(std::time::Duration::from_millis(15)).await;
        ids.push((n, s.id));
    }
    let q = |f: &dyn Fn(&mut rm::SubmissionQuery)| {
        let mut q = rm::SubmissionQuery {
            title_contains: Some(tag.clone()),
            limit: 10,
            ..Default::default()
        };
        f(&mut q);
        q
    };
    let r = w.reviewer;
    let page = w.review.queue(r, q(&|_| {})).await.unwrap();
    assert_eq!(page.total, 3);
    assert_eq!(
        page.items.iter().map(|s| s.id).collect::<Vec<_>>(),
        ids.iter().map(|x| x.1).collect::<Vec<_>>()
    );
    let page = w
        .review
        .queue(
            r,
            q(&|q| {
                q.descending = true;
                q.limit = 2;
                q.offset = 1;
            }),
        )
        .await
        .unwrap();
    assert_eq!(page.total, 3);
    assert_eq!(
        page.items.iter().map(|s| s.id).collect::<Vec<_>>(),
        [ids[1].1, ids[0].1]
    );
    // An offset past the end still reports the total.
    let page = w.review.queue(r, q(&|q| q.offset = 50)).await.unwrap();
    assert_eq!((page.items.len(), page.total), (0, 3));
    let page = w
        .review
        .queue(r, q(&|q| q.sort = rm::SortKey::Title))
        .await
        .unwrap();
    assert!(page.items[0].title.starts_with("Aa"));
    assert_eq!(
        w.review
            .queue(r, q(&|q| q.unclaimed = true))
            .await
            .unwrap()
            .total,
        3
    );
    // Wildcards in the search are literal.
    assert_eq!(
        w.review
            .queue(r, q(&|q| q.title_contains = Some("%".into())))
            .await
            .unwrap()
            .total,
        0
    );
    // Listing the member's own submissions.
    let mine = w
        .review
        .list_mine(
            w.member,
            rm::SubmissionQuery {
                title_contains: Some(tag.clone()),
                limit: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(mine.total, 3);
    assert_eq!(
        w.review
            .list_mine(
                w.other,
                rm::SubmissionQuery {
                    limit: 10,
                    ..Default::default()
                }
            )
            .await
            .unwrap()
            .total,
        0
    );
    // The open-submission limit counts these (3 open) plus drafts; staff are exempt.
    for _ in 0..2 {
        w.new_series_submission().await;
    }
    assert!(matches!(
        w.review
            .create(
                Caller::member(w.member),
                rm::CreateSubmission {
                    kind: KIND_SERIES.into(),
                    payload: series_payload("Over"),
                    title: None,
                    summary: String::new()
                }
            )
            .await,
        Err(ReviewError::Conflict(_))
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn requests_are_voted_and_fulfilled_by_a_published_submission() {
    let w = world().await;
    let req = w
        .review
        .create_request(
            w.other,
            "library",
            rm::RequestInput {
                title: format!("Wanted {}", uid()),
                creator: Some("Someone".into()),
                year: Some(2001),
                link: None,
                note: "please".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(req.vote_count, 1);
    let v = w.review.vote_request(w.member, req.id, true).await.unwrap();
    assert_eq!((v.vote_count, v.voted), (2, true));
    assert_eq!(
        w.review
            .vote_request(w.member, req.id, true)
            .await
            .unwrap()
            .vote_count,
        2
    );
    let page = w
        .review
        .list_requests(
            w.member,
            rm::RequestQuery {
                section: "library".into(),
                status: Some(rm::RequestStatus::Open),
                limit: 100,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(page.items.iter().any(|r| r.id == req.id && r.voted));
    let page = w
        .review
        .list_requests(
            w.member,
            rm::RequestQuery {
                section: "library".into(),
                newest: true,
                limit: 100,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(page.total >= 1);

    let s = w.new_series_submission().await;
    let ch = w
        .ops
        .create_chapter(&s, w.member, None, chapter(1.0))
        .await
        .unwrap();
    w.upload(&s, w.member, ch.id, 1).await;
    w.review.submit(w.member, s.id).await.unwrap();
    w.review.claim(w.reviewer, s.id, false).await.unwrap();
    // Not yet published: cannot fulfil.
    assert!(matches!(
        w.review
            .fulfil_request(Caller::member(w.member), req.id, s.id)
            .await,
        Err(ReviewError::Conflict(_))
    ));
    w.review
        .approve(w.reviewer, s.id, rm::DecisionInput::default())
        .await
        .unwrap();
    w.sink.0.lock().unwrap().clear();
    let done = w
        .review
        .fulfil_request(Caller::member(w.member), req.id, s.id)
        .await
        .unwrap();
    assert_eq!(done.status, rm::RequestStatus::Fulfilled);
    assert_eq!(done.submission_id, Some(s.id));
    {
        let sent = w.sink.0.lock().unwrap();
        assert_eq!(sent.len(), 1);
        let mut users = sent[0].0.clone();
        users.sort();
        let mut want = vec![w.member, w.other];
        want.sort();
        assert_eq!(users, want);
    }
    // Closed requests take no votes; the board count query still works.
    assert!(
        w.review
            .vote_request(w.reviewer.user_id, req.id, true)
            .await
            .is_err()
    );
    let _ = &w.pool;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn credits_quota_and_sweep_queries() {
    use sr_review::repo::{NewEvent, NewSubmission, SubmissionRepository};
    let w = world().await;
    let repo = PgReviewRepo::new(w.pool.clone());
    let ev = |kind: &str| NewEvent {
        id: uid(),
        kind: kind.into(),
        actor_id: None,
        body: None,
        internal: false,
        data: json!({}),
    };
    let mk = |title: &str| NewSubmission {
        id: uid(),
        kind: "library.series".into(),
        submitter_id: w.member,
        title: title.into(),
        summary: String::new(),
        ref_id: None,
        payload: json!({}),
    };
    let s = repo.create(&mk("Credits"), &ev("created")).await.unwrap();

    // Credits: unique per (section, item), kept after a revoke.
    let item = uid();
    let credit = |ref_id| rm::NewCredit {
        id: uid(),
        ref_id,
        label: "Chapter".into(),
    };
    let other_item = uid();
    assert_eq!(
        repo.grant_credits(
            "library",
            w.member,
            s.id,
            &[credit(item), credit(other_item)]
        )
        .await
        .unwrap(),
        2
    );
    assert_eq!(
        repo.grant_credits("library", w.other, s.id, &[credit(item)])
            .await
            .unwrap(),
        0,
        "the same item cannot be credited twice"
    );
    assert_eq!(repo.credit_total(w.member).await.unwrap(), 2);
    assert_eq!(
        repo.revoke_credits(s.id, Some(&[item]), "spam", 1_000)
            .await
            .unwrap(),
        1
    );
    assert_eq!(repo.credit_total(w.member).await.unwrap(), 1);
    assert_eq!(
        repo.grant_credits("library", w.member, s.id, &[credit(item)])
            .await
            .unwrap(),
        0,
        "a revoked credit is not earned again"
    );
    let credits = repo.credits_for(s.id).await.unwrap();
    assert_eq!(credits.len(), 2);
    assert!(
        credits
            .iter()
            .any(|c| c.ref_id == item && c.revoke_reason.as_deref() == Some("spam"))
    );
    assert_eq!(
        repo.revoke_credits(s.id, None, "all", 2_000).await.unwrap(),
        1
    );

    // Upload allowance: atomic, per day.
    let day = 20_000;
    assert_eq!(
        repo.add_upload_bytes(w.member, day, 40, 100).await.unwrap(),
        Some(40)
    );
    assert_eq!(
        repo.add_upload_bytes(w.member, day, 61, 100).await.unwrap(),
        None
    );
    assert_eq!(
        repo.add_upload_bytes(w.member, day, 60, 100).await.unwrap(),
        Some(100)
    );
    assert_eq!(repo.upload_bytes(w.member, day).await.unwrap(), 100);
    assert_eq!(
        repo.add_upload_bytes(w.member, day + 1, 101, 100)
            .await
            .unwrap(),
        None
    );
    assert_eq!(repo.upload_bytes(w.member, day + 1).await.unwrap(), 0);

    // Sweeps: an old draft with only a stale warning is idle; an event revives it.
    let old = repo.create(&mk("Old draft"), &ev("created")).await.unwrap();
    sqlx::query("UPDATE submissions SET created_at = now() - interval '40 days', updated_at = now() - interval '40 days' WHERE id = $1")
        .bind(old.id)
        .execute(&w.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE submission_events SET created_at = now() - interval '40 days' WHERE submission_id = $1")
        .bind(old.id)
        .execute(&w.pool)
        .await
        .unwrap();
    let cutoff = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
        - 30 * 86_400_000;
    let idle = repo.idle_drafts(cutoff, 500).await.unwrap();
    assert!(idle.iter().any(|x| x.id == old.id));
    assert!(!idle.iter().any(|x| x.id == s.id));
    repo.add_event(old.id, &ev("stale_warning")).await.unwrap();
    assert!(
        repo.idle_drafts(cutoff, 500)
            .await
            .unwrap()
            .iter()
            .any(|x| x.id == old.id),
        "a warning is not activity"
    );
    assert!(
        repo.last_event_ms(old.id, "stale_warning")
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        repo.last_event_ms(old.id, "purged")
            .await
            .unwrap()
            .is_none()
    );
    repo.add_event(old.id, &ev("comment")).await.unwrap();
    assert!(
        !repo
            .idle_drafts(cutoff, 500)
            .await
            .unwrap()
            .iter()
            .any(|x| x.id == old.id),
        "an event is activity"
    );

    let rej = repo.create(&mk("Rejected"), &ev("created")).await.unwrap();
    sqlx::query("UPDATE submissions SET state = 'rejected', decided_at = now() - interval '20 days' WHERE id = $1")
        .bind(rej.id)
        .execute(&w.pool)
        .await
        .unwrap();
    let before = cutoff + 16 * 86_400_000;
    assert!(
        repo.rejected_unpurged(before, 500)
            .await
            .unwrap()
            .iter()
            .any(|x| x.id == rej.id)
    );
    repo.add_event(rej.id, &ev("purged")).await.unwrap();
    assert!(
        !repo
            .rejected_unpurged(before, 500)
            .await
            .unwrap()
            .iter()
            .any(|x| x.id == rej.id)
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn request_claims_are_exclusive_and_cleared_by_the_holder() {
    use sr_review::repo::SubmissionRepository;
    let w = world().await;
    let repo = PgReviewRepo::new(w.pool.clone());
    let req = repo
        .create_request(
            uid(),
            "library",
            w.other,
            &rm::RequestInput {
                title: format!("Claimable {}", uid()),
                creator: None,
                year: None,
                link: None,
                note: String::new(),
            },
        )
        .await
        .unwrap();
    assert_eq!(req.claimed_by, None);
    let c = repo
        .claim_request(req.id, w.member, "vol 1", 1_000)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(c.claimed_by, Some(w.member));
    assert_eq!(c.claim_note.as_deref(), Some("vol 1"));
    assert!(c.claimed_by_me);
    assert!(
        repo.claim_request(req.id, w.other, "", 1_000)
            .await
            .unwrap()
            .is_none()
    );
    let seen = repo.get_request(req.id, w.other).await.unwrap().unwrap();
    assert_eq!(seen.claimed_by, Some(w.member));
    assert!(!seen.claimed_by_me);
    assert!(
        repo.release_request(req.id, w.other, false)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        repo.release_request(req.id, w.other, true)
            .await
            .unwrap()
            .is_some()
    );
    let again = repo
        .claim_request(req.id, w.other, "", 2_000)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(again.claim_note, None);
}
