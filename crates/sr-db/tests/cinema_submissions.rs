//! DB-backed tests of the Cinema review kinds (`cinema.title`, `cinema.episodes`): drafts are
//! written through the real catalog, a submission is not ready without playable videos, nothing
//! is visible before approval, only approved episodes publish, and members cannot smuggle in
//! storage keys or somebody else's uploads.
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test cinema_submissions -- --ignored --test-threads=1`

use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

use serde_json::{Value, json};
use sr_cinema::model::*;
use sr_cinema::submissions::{CinemaKind, KIND_EPISODES, KIND_TITLE};
use sr_cinema::{Actor, CinemaService, PlayRef, TitleIndex, Viewer};
use sr_core::{Clock, PlatformPermissions as P, SystemClock};
use sr_db::{
    PgPool,
    accounts::{self, NewAccount},
    cinema_repo::PgCinemaRepo,
    media_repo::PgMediaRepo,
    review_repo::PgReviewRepo,
    testing::TestDb,
};
use sr_media::assets::{MediaRepository, NewAsset};
use sr_review::model as rm;
use sr_review::{Caller, KindRegistry, ReviewError, ReviewService};
use sr_storage::cdn::CdnSigner;

fn uid() -> i64 {
    static N: AtomicI64 = AtomicI64::new(0);
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + N.fetch_add(1, Ordering::Relaxed) % 1000
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

struct World {
    _db: TestDb,
    pool: PgPool,
    cinema: CinemaService,
    media: Arc<PgMediaRepo>,
    review: ReviewService,
    member: i64,
    other: i64,
    reviewer: Caller,
}

async fn world() -> World {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let cinema = CinemaService::new(
        Arc::new(PgCinemaRepo::new(pool.clone())),
        CdnSigner::new(vec![7u8; 32]),
        Arc::new(SystemClock),
        Arc::new(uid),
    )
    .with_index(Arc::new(TitleIndex::in_ram().unwrap()))
    .with_storage(Arc::new(sr_storage::ObjectStoreStorage::in_memory()));
    let media = Arc::new(PgMediaRepo::new(pool.clone(), Arc::new(uid)));
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let review = ReviewService::new(
        Arc::new(PgReviewRepo::new(pool.clone())),
        KindRegistry::new()
            .with(Arc::new(CinemaKind::title(cinema.clone(), media.clone())))
            .with(Arc::new(CinemaKind::episodes(
                cinema.clone(),
                media.clone(),
            ))),
        clock,
        Arc::new(uid),
    );
    let (member, other, r) = (
        account(&pool).await,
        account(&pool).await,
        account(&pool).await,
    );
    World {
        _db: db,
        pool,
        cinema,
        media,
        review,
        member,
        other,
        reviewer: Caller {
            user_id: r,
            permissions: P::STAFF_BASIC | P::REVIEW_MEDIA,
        },
    }
}

impl World {
    /// An asset of `user`, playable or still processing.
    async fn asset(&self, user: i64, playable: bool) -> i64 {
        let id = uid();
        self.media
            .create_asset(&NewAsset {
                id,
                title_id: None,
                episode_id: None,
                uploaded_by: Some(user),
                filename: "a.mkv".into(),
                original_key: format!("originals/{id}/source.mkv"),
                original_ext: "mkv".into(),
                original_size: 5,
                auto_ladder: true,
            })
            .await
            .unwrap();
        if playable {
            sqlx::query(
                "UPDATE video_assets SET state = 'published', hls_prefix = $2 WHERE id = $1",
            )
            .bind(id)
            .bind(format!("transcode/{id}/1"))
            .execute(&self.pool)
            .await
            .unwrap();
        }
        id
    }

    async fn create(
        &self,
        user: i64,
        kind: &str,
        payload: Value,
    ) -> Result<rm::Submission, ReviewError> {
        self.review
            .create(
                Caller::member(user),
                rm::CreateSubmission {
                    kind: kind.into(),
                    payload,
                    title: None,
                    summary: "about".into(),
                },
            )
            .await
            .map(|d| d.submission)
    }

    fn viewer(&self) -> Viewer {
        Viewer {
            user_id: self.other,
            adult: true,
            hide_restricted: false,
        }
    }

    fn staff(&self) -> Actor {
        Actor {
            user_id: self.reviewer.user_id,
            permissions: P::PUBLISH_MEDIA | P::DELETE_CONTENT,
        }
    }
}

fn series_payload(a1: i64, a2: i64) -> Value {
    json!({
        "kind": "series", "title": format!("Night Shift {}", uid()), "year": 2024,
        "poster_key": "cinema/evil/poster.webp",
        "seasons": [{ "number": 1, "episodes": [
            { "number": 1, "title": "Pilot", "asset_id": a1.to_string(), "still_key": "cinema/evil/still.webp" },
            { "number": 2, "title": "Second", "asset_id": a2.to_string() },
        ]}]
    })
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_member_series_waits_for_its_videos_then_publishes_after_review() {
    let w = world().await;
    let (ready, busy) = (
        w.asset(w.member, true).await,
        w.asset(w.member, false).await,
    );
    let s = w
        .create(w.member, KIND_TITLE, series_payload(ready, busy))
        .await
        .unwrap();
    let title = s.ref_id.unwrap();
    // The draft exists but readers cannot see it, and the member's storage keys were dropped.
    assert!(w.cinema.title_detail(w.viewer(), title).await.is_err());
    let detail = w.cinema.admin_title(w.staff(), title).await.unwrap();
    assert_eq!(detail.episodes.len(), 2);
    assert!(detail.title.poster_url.is_none());
    assert!(
        detail
            .episodes
            .iter()
            .all(|e| e.still_url.is_none() && !e.published)
    );
    assert!(detail.episodes.iter().all(|e| e.asset_id.is_some()));
    // One video is still processing: not ready.
    let err = w.review.submit(w.member, s.id).await.unwrap_err();
    match err {
        ReviewError::NotReady(p) => {
            assert_eq!(p.len(), 1, "{p:?}");
            assert!(
                p[0].contains("S01E02") && p[0].contains("still being processed"),
                "{p:?}"
            );
        }
        other => panic!("{other:?}"),
    }
    sqlx::query(
        "UPDATE video_assets SET state = 'published', hls_prefix = 'transcode/9/1' WHERE id = $1",
    )
    .bind(busy)
    .execute(&w.pool)
    .await
    .unwrap();
    let d = w.review.submit(w.member, s.id).await.unwrap();
    assert_eq!(d.items.len(), 2);
    assert_eq!(d.items[0].label, "S01E01 Pilot");
    let preview = w.review.preview_mine(w.member, s.id).await.unwrap();
    assert_eq!(preview["new_title"], true);
    assert_eq!(preview["episodes"].as_array().unwrap().len(), 2);
    // Still private while under review.
    w.review.claim(w.reviewer, s.id, false).await.unwrap();
    assert!(w.cinema.title_detail(w.viewer(), title).await.is_err());
    let d = w
        .review
        .approve(w.reviewer, s.id, rm::DecisionInput::default())
        .await
        .unwrap();
    assert_eq!(d.submission.state, rm::SubmissionState::Published);
    let t = w.cinema.title_detail(w.viewer(), title).await.unwrap();
    assert!(t.title.published);
    assert_eq!(t.title.episode_count, 2);
    let play = w
        .cinema
        .resolve_play(
            w.viewer(),
            PlayRef::Title {
                id: title,
                variant: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(play.asset_id, ready);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_film_needs_its_own_playable_video_and_assets_must_be_the_submitters() {
    let w = world().await;
    let film = |asset: Option<i64>| {
        let mut v = json!({ "kind": "movie", "title": format!("Film {}", uid()) });
        if let Some(a) = asset {
            v["asset_id"] = json!(a.to_string());
        }
        v
    };
    // Somebody else's upload cannot be claimed; nothing is left behind.
    let theirs = w.asset(w.other, true).await;
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM video_titles")
        .fetch_one(&w.pool)
        .await
        .unwrap();
    let err = w
        .create(w.member, KIND_TITLE, film(Some(theirs)))
        .await
        .unwrap_err();
    assert!(matches!(err, ReviewError::Invalid(_)), "{err:?}");
    let err = w
        .create(w.member, KIND_TITLE, film(Some(uid())))
        .await
        .unwrap_err();
    assert!(matches!(err, ReviewError::Invalid(_)));
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM video_titles")
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    // A film and seasons together make no sense.
    let mut both = film(Some(w.asset(w.member, true).await));
    both["seasons"] = json!([{ "number": 1, "episodes": [] }]);
    assert!(w.create(w.member, KIND_TITLE, both).await.is_err());
    // A film without a video blocks submitting; with one it goes through.
    let s = w.create(w.member, KIND_TITLE, film(None)).await.unwrap();
    let err = w.review.submit(w.member, s.id).await.unwrap_err();
    assert!(
        matches!(&err, ReviewError::NotReady(p) if p[0].contains("no video yet")),
        "{err:?}"
    );
    let own = w.asset(w.member, true).await;
    let s2 = w
        .create(w.member, KIND_TITLE, film(Some(own)))
        .await
        .unwrap();
    let d = w.review.submit(w.member, s2.id).await.unwrap();
    assert_eq!(d.items.len(), 1);
    w.review.claim(w.reviewer, s2.id, false).await.unwrap();
    w.review
        .approve(w.reviewer, s2.id, rm::DecisionInput::default())
        .await
        .unwrap();
    assert!(
        w.cinema
            .title_detail(w.viewer(), s2.ref_id.unwrap())
            .await
            .unwrap()
            .title
            .published
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn new_episodes_for_a_published_series_publish_only_what_was_approved() {
    let w = world().await;
    // A published series with one episode, made by staff.
    let a0 = w.asset(w.reviewer.user_id, true).await;
    let t = w
        .cinema
        .create_title(
            w.staff(),
            serde_json::from_value(json!({ "kind": "series", "title": format!("Base {}", uid()) }))
                .unwrap(),
        )
        .await
        .unwrap();
    let season = w
        .cinema
        .create_season(
            w.staff(),
            t.id,
            SeasonInput {
                number: 1,
                name: None,
                synopsis: None,
                poster_key: None,
            },
        )
        .await
        .unwrap();
    let e0 = w
        .cinema
        .create_episode(
            w.staff(),
            season.id,
            EpisodeInput {
                number: 1,
                absolute_number: None,
                title: "One".into(),
                synopsis: String::new(),
                air_date: None,
                runtime_min: None,
                still_key: None,
            },
        )
        .await
        .unwrap();
    w.cinema
        .link_episode_asset(w.staff(), e0.id, Some(a0))
        .await
        .unwrap();
    w.cinema
        .set_episode_published(w.staff(), e0.id, true)
        .await
        .unwrap();
    w.cinema
        .set_title_published(w.staff(), t.id, true)
        .await
        .unwrap();

    let (b1, b2) = (w.asset(w.member, true).await, w.asset(w.member, true).await);
    let payload = json!({ "title_id": t.id.to_string(), "season": 1, "episodes": [
        { "number": 1, "title": "dup", "asset_id": b1.to_string() },
    ]});
    // Episode 1 of season 1 exists already.
    assert!(matches!(
        w.create(w.member, KIND_EPISODES, payload).await,
        Err(ReviewError::Invalid(_))
    ));
    let payload = json!({ "title_id": t.id.to_string(), "season": 1, "episodes": [
        { "number": 2, "title": "Two", "asset_id": b1.to_string() },
        { "number": 3, "title": "Three", "asset_id": b2.to_string() },
    ]});
    let s = w.create(w.member, KIND_EPISODES, payload).await.unwrap();
    assert_eq!(s.kind, KIND_EPISODES);
    let d = w.review.submit(w.member, s.id).await.unwrap();
    assert_eq!(d.items.len(), 2, "only the new episodes are items");
    // Drafts stay hidden: readers still see one episode.
    assert_eq!(
        w.cinema
            .title_detail(w.viewer(), t.id)
            .await
            .unwrap()
            .title
            .episode_count,
        1
    );
    w.review.claim(w.reviewer, s.id, false).await.unwrap();
    for (item, state, note) in [
        (
            &d.items[1],
            rm::ItemState::Rejected,
            Some("audio out of sync"),
        ),
        (&d.items[0], rm::ItemState::Approved, None),
    ] {
        w.review
            .decide_item(
                w.reviewer,
                s.id,
                item.id,
                rm::ItemDecisionInput {
                    state,
                    note: note.map(Into::into),
                },
            )
            .await
            .unwrap();
    }
    w.review
        .approve(w.reviewer, s.id, rm::DecisionInput::default())
        .await
        .unwrap();
    let detail = w.cinema.admin_title(w.staff(), t.id).await.unwrap();
    let published: Vec<i32> = detail
        .episodes
        .iter()
        .filter(|e| e.published)
        .map(|e| e.number)
        .collect();
    assert_eq!(published, vec![1, 2]);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn housekeeping_purges_a_rejected_cinema_submission() {
    let w = world().await;
    let (ready, busy) = (
        w.asset(w.member, true).await,
        w.asset(w.member, false).await,
    );
    let s = w
        .create(w.member, KIND_TITLE, series_payload(ready, busy))
        .await
        .unwrap();
    let title = s.ref_id.unwrap();
    // Make both videos playable so the submission can be submitted, then drop one back to
    // "still processing" with a transcode prefix: it is the one the purge must collect.
    sqlx::query("UPDATE video_assets SET state = 'published', hls_prefix = $2 WHERE id = $1")
        .bind(busy)
        .bind("transcode/busy/1")
        .execute(&w.pool)
        .await
        .unwrap();
    w.review.submit(w.member, s.id).await.unwrap();
    w.review.claim(w.reviewer, s.id, false).await.unwrap();
    sqlx::query("UPDATE video_assets SET state = 'transcoding' WHERE id = $1")
        .bind(busy)
        .execute(&w.pool)
        .await
        .unwrap();
    w.review
        .reject(
            w.reviewer,
            s.id,
            rm::DecisionInput {
                reason: Some("not a fit".into()),
            },
        )
        .await
        .unwrap();
    // Not purged before the 14 day wait.
    assert_eq!(w.review.housekeeping().await.unwrap().purged, 0);
    assert!(w.cinema.admin_title(w.staff(), title).await.is_ok());
    sqlx::query("UPDATE submissions SET decided_at = now() - interval '15 days' WHERE id = $1")
        .bind(s.id)
        .execute(&w.pool)
        .await
        .unwrap();
    let report = w.review.housekeeping().await.unwrap();
    assert_eq!((report.purged, report.failed), (1, 0));
    // The draft title is gone; the published asset stays, the unpublished one is removed and its
    // transcode output is queued for collection.
    assert!(w.cinema.admin_title(w.staff(), title).await.is_err());
    let state = |id: i64| {
        let pool = w.pool.clone();
        async move {
            sqlx::query_scalar::<_, String>("SELECT state FROM video_assets WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    assert_eq!(state(ready).await, "published");
    assert_eq!(state(busy).await, "removed");
    let queued: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM cinema_media_gc WHERE prefix = 'transcode/busy/1'",
    )
    .fetch_one(&w.pool)
    .await
    .unwrap();
    assert_eq!(queued, 1);
    // Idempotent.
    assert_eq!(w.review.housekeeping().await.unwrap().purged, 0);
}

fn png(w: u32, h: u32) -> Vec<u8> {
    use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
    let img = RgbImage::from_fn(w, h, |x, y| Rgb([(x % 256) as u8, (y % 256) as u8, 7]));
    let mut out = Vec::new();
    DynamicImage::ImageRgb8(img)
        .write_to(&mut std::io::Cursor::new(&mut out), ImageFormat::Png)
        .unwrap();
    out
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_member_sets_the_poster_and_backdrop_of_a_draft_title() {
    let w = world().await;
    let a = w.asset(w.member, true).await;
    let s = w
        .create(
            w.member,
            KIND_TITLE,
            json!({ "kind": "movie", "title": format!("Film {}", uid()), "asset_id": a.to_string() }),
        )
        .await
        .unwrap();
    let member = Caller::member(w.member);
    let poster = w
        .review
        .set_artwork(member, s.id, "poster", &png(900, 1350))
        .await
        .unwrap();
    assert!(poster.is_some_and(|u| u.contains("/poster-")));
    let backdrop = w
        .review
        .set_artwork(member, s.id, "backdrop", &png(2400, 1350))
        .await
        .unwrap();
    assert!(backdrop.is_some_and(|u| u.contains("/backdrop-")));
    // The keys are stored with the draft title, under its own folder.
    let (p, b): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT poster_key, backdrop_key FROM video_titles WHERE id = $1")
            .bind(s.ref_id.unwrap())
            .fetch_one(&w.pool)
            .await
            .unwrap();
    let prefix = format!("cinema/{}/", s.ref_id.unwrap());
    assert!(p.is_some_and(|k| k.starts_with(&prefix)));
    assert!(b.is_some_and(|k| k.starts_with(&prefix)));
    // Other slots, bad images and strangers are refused.
    for slot in ["cover", "banner", "logo"] {
        assert!(matches!(
            w.review.set_artwork(member, s.id, slot, &png(10, 10)).await,
            Err(ReviewError::Invalid(_))
        ));
    }
    assert!(matches!(
        w.review.set_artwork(member, s.id, "poster", b"nope").await,
        Err(ReviewError::Invalid(_))
    ));
    assert!(matches!(
        w.review
            .set_artwork(Caller::member(w.other), s.id, "poster", &png(10, 10))
            .await,
        Err(ReviewError::NotFound(_))
    ));
}
