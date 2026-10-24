//! The member video submission routes (`/submissions/{id}/cinema/*`, RDY-MED-001) over real Postgres,
//! from a draft to a published title: the review service, the Cinema kinds, the media repository, the
//! upload sessions and an in-memory multipart store that stands in for the browser's part `PUT`s.
//!
//! Run with `SR_TEST_DATABASE_URL`, `--include-ignored`, `--test-threads=1`.

use super::{CinemaSubmitDeps, router};
use crate::{
    ApiSettings, AppState,
    auth::{
        csrf,
        memory::{MemoryBots, MemorySessions},
    },
    build_router,
    routes::{
        cinema_media::MediaDeps,
        review::{self, ReviewDeps},
    },
};
use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use bytes::Bytes;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sr_auth::{
    repo::{SessionRecord, SessionRepository},
    session::SessionToken,
};
use sr_cinema::drafts::{DraftOps, UploadSessions};
use sr_cinema::model::{TitleInput, TitleKind};
use sr_cinema::submissions::{CinemaKind, KIND_EPISODES, KIND_TITLE};
use sr_cinema::{Actor, CinemaService, PlayRef, TitleIndex, Viewer};
use sr_core::{PlatformPermissions as P, SystemClock};
use sr_db::{
    PgPool,
    accounts::{self, NewAccount},
    cinema_repo::PgCinemaRepo,
    cinema_uploads::PgUploadSessions,
    media_repo::PgMediaRepo,
    review_repo::PgReviewRepo,
    testing::TestDb,
};
use sr_library::multipart::{MemoryPartStore, PartStore, StoredPart};
use sr_media::downloads::{DownloadService, NoDownloads};
use sr_media::runner::FakeRunner;
use sr_media::service::{IdSource, MediaService, MediaSettings, ProbeQueue, StaticMediaSettings};
use sr_media::worker_api::{OutputStorage, PartDone};
use sr_review::model as rm;
use sr_review::service::AllowanceSource;
use sr_review::{Caller, KindRegistry, ReviewService};
use sr_storage::{ObjectStorage, ObjectStoreStorage, cdn::CdnSigner};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};
use tower::ServiceExt;

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

/// The multipart store behind the media service: the same in-memory store the routes list parts from.
struct PartOutputs(Arc<MemoryPartStore>);

#[async_trait]
impl OutputStorage for PartOutputs {
    async fn init_multipart(&self, key: &str, content_type: &str) -> Result<String, String> {
        self.0.create(key, content_type).await
    }

    async fn presign_part(
        &self,
        key: &str,
        upload_id: &str,
        part_number: u32,
        ttl_secs: u64,
    ) -> Result<String, String> {
        self.0
            .presign_part(key, upload_id, part_number, Duration::from_secs(ttl_secs))
    }

    async fn complete_multipart(
        &self,
        key: &str,
        upload_id: &str,
        parts: &[PartDone],
    ) -> Result<(), String> {
        let parts: Vec<StoredPart> = parts
            .iter()
            .map(|p| StoredPart {
                number: p.part_number,
                size: 0,
                etag: p.etag.clone(),
            })
            .collect();
        self.0.complete(key, upload_id, &parts).await
    }
}

struct NoQueue;

#[async_trait]
impl ProbeQueue for NoQueue {
    async fn enqueue_probe(&self, _: i64) -> Result<(), String> {
        Ok(())
    }
}

struct Daily(i64);

#[async_trait]
impl AllowanceSource for Daily {
    async fn daily_upload_bytes(&self) -> i64 {
        self.0
    }
}

/// Largest original the test media service accepts.
const MAX_BYTES: u64 = 10_000;

struct World {
    _db: TestDb,
    pool: PgPool,
    app: Router,
    cinema: CinemaService,
    review: ReviewService,
    parts: Arc<MemoryPartStore>,
    sessions: Arc<PgUploadSessions>,
    member: i64,
    other: i64,
    reviewer: Caller,
    tokens: BTreeMap<i64, String>,
}

async fn world(daily: i64) -> World {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let cinema = CinemaService::new(
        Arc::new(PgCinemaRepo::new(pool.clone())),
        CdnSigner::new(vec![7u8; 32]),
        Arc::new(SystemClock),
        Arc::new(uid),
    )
    .with_index(Arc::new(TitleIndex::in_ram().unwrap()))
    .with_storage(Arc::new(ObjectStoreStorage::in_memory()));
    let media_repo = Arc::new(PgMediaRepo::new(pool.clone(), Arc::new(uid)));
    let storage: Arc<dyn ObjectStorage> = Arc::new(ObjectStoreStorage::in_memory());
    let parts = Arc::new(MemoryPartStore::new(storage.clone()));
    let id_src: IdSource = Arc::new(uid);
    let media = MediaService::new(
        media_repo.clone(),
        storage.clone(),
        Arc::new(PartOutputs(parts.clone())),
        Arc::new(FakeRunner::fixed("")),
        Arc::new(StaticMediaSettings(MediaSettings {
            max_upload_bytes: MAX_BYTES,
            ..MediaSettings::default()
        })),
        Arc::new(NoQueue),
        id_src.clone(),
        Arc::new(|| 1_700_000_000),
        Arc::new(|| Some([9u8; 32])),
    )
    .with_part_size(4);
    let downloads = DownloadService::new(
        media_repo.clone(),
        media_repo.clone(),
        Arc::new(NoDownloads),
        Arc::new(CdnSigner::new(vec![3u8; 32])),
        storage.clone(),
        id_src,
        Arc::new(|| 1_700_000_000),
    );
    let review = ReviewService::new(
        Arc::new(PgReviewRepo::new(pool.clone())),
        KindRegistry::new()
            .with(Arc::new(CinemaKind::title(
                cinema.clone(),
                media_repo.clone(),
            )))
            .with(Arc::new(CinemaKind::episodes(
                cinema.clone(),
                media_repo.clone(),
            ))),
        Arc::new(SystemClock),
        Arc::new(uid),
    )
    .with_allowance(Arc::new(Daily(daily)));
    let sessions = Arc::new(PgUploadSessions::new(pool.clone()));
    let submit = CinemaSubmitDeps::new(
        DraftOps::new(cinema.clone(), media_repo.clone()),
        sessions.clone(),
    )
    .with_parts(parts.clone());
    let (member, other, r) = (
        account(&pool).await,
        account(&pool).await,
        account(&pool).await,
    );
    let session_store = Arc::new(MemorySessions::new());
    let mut tokens = BTreeMap::new();
    for (n, user) in [member, other, r].into_iter().enumerate() {
        let t = SessionToken::generate().unwrap();
        session_store
            .create(SessionRecord {
                id: 100 + n as i64,
                user_id: user,
                token_hash: t.hash(),
                created_at: 0,
                last_seen_at: i64::MAX / 4,
                expires_at: i64::MAX / 4,
                device_name: None,
            })
            .await
            .unwrap();
        tokens.insert(user, t.expose().to_owned());
    }
    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let state = AppState::builder()
        .settings(settings)
        .sessions(session_store)
        .bots(Arc::new(MemoryBots::new()))
        .extension(Arc::new(ReviewDeps::new(review.clone())))
        .extension(Arc::new(MediaDeps::new(media, Arc::new(downloads))))
        .extension(Arc::new(submit))
        .build();
    World {
        _db: db,
        pool,
        app: build_router(state, router().merge(review::router())),
        cinema,
        review,
        parts,
        sessions,
        member,
        other,
        reviewer: Caller {
            user_id: r,
            permissions: P::STAFF_BASIC | P::REVIEW_MEDIA | P::PUBLISH_MEDIA,
        },
        tokens,
    }
}

impl World {
    async fn call(
        &self,
        who: i64,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let raw = &self.tokens[&who];
        let mut req = Request::builder()
            .method(method.clone())
            .uri(format!("/api/v1{path}"))
            .header("origin", "http://localhost:8080")
            .header("cookie", format!("sr_session={raw}"));
        if method != Method::GET {
            req = req.header("x-sr-csrf", csrf::token_for(raw));
        }
        let body = match body {
            Some(b) => {
                req = req.header("content-type", "application/json");
                Body::from(b.to_string())
            }
            None => Body::empty(),
        };
        let resp = self
            .app
            .clone()
            .oneshot(req.body(body).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn get(&self, who: i64, path: &str) -> (StatusCode, Value) {
        self.call(who, Method::GET, path, None).await
    }

    async fn post(&self, who: i64, path: &str, body: Value) -> (StatusCode, Value) {
        self.call(who, Method::POST, path, Some(body)).await
    }

    /// A new `cinema.title` submission of a series (or a film) with no videos yet.
    async fn new_title(&self, who: i64, kind: &str, name: &str) -> String {
        let (st, b) = self
            .post(
                who,
                "/submissions",
                json!({ "kind": KIND_TITLE, "payload": { "kind": kind, "title": name, "year": 2024 } }),
            )
            .await;
        assert_eq!(st, StatusCode::CREATED, "{b}");
        b["submission"]["id"].as_str().unwrap().to_owned()
    }

    /// Reserve an upload of `size` bytes and return the slot.
    async fn begin(&self, who: i64, sub: &str, filename: &str, size: u64) -> (StatusCode, Value) {
        self.post(
            who,
            &format!("/submissions/{sub}/cinema/files"),
            json!({ "filename": filename, "size_bytes": size }),
        )
        .await
    }

    /// What the browser does for the parts in `which` (1-based) of a file of `data`.
    async fn put_parts(&self, asset: &str, which: &[u32], data: &[u8]) {
        let sess = self
            .sessions
            .get(asset.parse().unwrap())
            .await
            .unwrap()
            .expect("an open session");
        for n in which {
            let from = (*n as usize - 1) * sess.part_size as usize;
            let to = (from + sess.part_size as usize).min(data.len());
            self.parts.put_part(
                &sess.s3_upload_id,
                *n,
                Bytes::copy_from_slice(&data[from..to]),
            );
        }
    }

    async fn make_playable(&self, asset: &str) {
        sqlx::query("UPDATE video_assets SET state = 'published', hls_prefix = $2 WHERE id = $1")
            .bind(asset.parse::<i64>().unwrap())
            .bind(format!("transcode/{asset}/1"))
            .execute(&self.pool)
            .await
            .unwrap();
    }

    async fn asset_state(&self, asset: &str) -> String {
        sqlx::query_scalar("SELECT state FROM video_assets WHERE id = $1")
            .bind(asset.parse::<i64>().unwrap())
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    fn viewer(&self) -> Viewer {
        Viewer {
            user_id: self.other,
            adult: true,
            hide_restricted: false,
        }
    }

    /// Upload a whole file through the routes: begin, put every part, complete.
    async fn upload(&self, who: i64, sub: &str, filename: &str, data: &[u8]) -> String {
        let (st, slot) = self.begin(who, sub, filename, data.len() as u64).await;
        assert_eq!(st, StatusCode::CREATED, "{slot}");
        let asset = slot["asset_id"].as_str().unwrap().to_owned();
        let all: Vec<u32> = (1..=slot["part_count"].as_u64().unwrap() as u32).collect();
        self.put_parts(&asset, &all, data).await;
        let (st, done) = self
            .post(
                who,
                &format!("/submissions/{sub}/cinema/uploads/{asset}/complete"),
                json!({}),
            )
            .await;
        assert_eq!(st, StatusCode::ACCEPTED, "{done}");
        asset
    }
}

fn series_input(title: &str, synopsis: &str) -> TitleInput {
    TitleInput {
        kind: TitleKind::Series,
        title: title.into(),
        original_title: None,
        alt_titles: vec![],
        year: Some(2024),
        runtime_min: None,
        content_rating: None,
        synopsis: synopsis.into(),
        tagline: None,
        genres: vec![],
        tags: vec![],
        languages: vec![],
        subtitle_languages: vec![],
        cast: vec![],
        directors: vec![],
        writers: vec![],
        studios: vec![],
        crew: vec![],
        age_restricted: false,
        poster_key: Some("cinema/evil/poster.webp".into()),
        backdrop_key: None,
        logo_key: None,
        external_ids: BTreeMap::new(),
    }
}

fn png_data_uri() -> String {
    let img = image::RgbaImage::from_pixel(32, 48, image::Rgba([200, 30, 30, 255]));
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    format!(
        "data:image/png;base64,{}",
        data_encoding::BASE64.encode(&out)
    )
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_member_series_goes_from_draft_to_a_published_title() {
    let w = world(0).await;
    let name = format!("Night Shift {}", uid());
    let sub = w.new_title(w.member, "series", &name).await;
    let base = format!("/submissions/{sub}/cinema");

    // Empty draft.
    let (st, d) = w.get(w.member, &format!("{base}/draft")).await;
    assert_eq!(st, StatusCode::OK, "{d}");
    assert_eq!(d["draft"]["new_title"], true);
    assert_eq!(d["items"].as_array().unwrap().len(), 0);

    // The first file: season and episode come from the name, the episode is created.
    let (st, slot) = w
        .begin(w.member, &sub, "Night Shift - s01e01 - Pilot.mkv", 10)
        .await;
    assert_eq!(st, StatusCode::CREATED, "{slot}");
    assert_eq!(slot["created"], true);
    assert_eq!(slot["item"]["label"], "S01E01 Pilot");
    assert_eq!(slot["part_count"], 3);
    assert_eq!(slot["resumable"], true);
    assert!(
        slot["parts"][0]["url"]
            .as_str()
            .unwrap()
            .contains("partNumber=1")
    );
    let asset = slot["asset_id"].as_str().unwrap().to_owned();
    let data = b"0123456789";

    // Nothing is attached until the upload completes; it shows as pending.
    let (_, d) = w.get(w.member, &format!("{base}/draft")).await;
    assert_eq!(d["items"][0]["video"], Value::Null);
    assert_eq!(d["pending"][0]["video"]["state"], "awaiting_upload");

    // Resume: two of three parts arrived.
    w.put_parts(&asset, &[1, 3], data).await;
    let (_, p) = w
        .get(w.member, &format!("{base}/uploads/{asset}/parts"))
        .await;
    assert_eq!(p["received"].as_array().unwrap().len(), 2, "{p}");
    assert_eq!(p["missing"].as_array().unwrap().len(), 1);
    assert_eq!(p["missing"][0]["part_number"], 2);

    // Completing with a part missing is refused and the upload stays open.
    let (st, e) = w
        .post(
            w.member,
            &format!("{base}/uploads/{asset}/complete"),
            json!({}),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT, "{e}");
    assert!(e["message"].as_str().unwrap().contains("part 2"), "{e}");
    assert_eq!(w.asset_state(&asset).await, "awaiting_upload");

    w.put_parts(&asset, &[2], data).await;
    let (st, v) = w
        .post(
            w.member,
            &format!("{base}/uploads/{asset}/complete"),
            json!({}),
        )
        .await;
    assert_eq!(st, StatusCode::ACCEPTED, "{v}");
    assert_eq!(v["state"], "uploaded");
    // The video is attached; a repeat is harmless.
    let (st, again) = w
        .post(
            w.member,
            &format!("{base}/uploads/{asset}/complete"),
            json!({}),
        )
        .await;
    assert_eq!(st, StatusCode::ACCEPTED, "{again}");
    let (_, d) = w.get(w.member, &format!("{base}/draft")).await;
    assert_eq!(d["items"][0]["video"]["asset_id"], asset.as_str());
    assert_eq!(d["pending"].as_array().unwrap().len(), 0);

    // A second episode and an edit of its metadata.
    let a2 = w
        .upload(w.member, &sub, "Night Shift - s01e02.mp4", b"abcdefgh")
        .await;
    let (_, d) = w.get(w.member, &format!("{base}/draft")).await;
    assert_eq!(d["items"].as_array().unwrap().len(), 2, "{d}");
    let ep2 = d["draft"]["episodes"][1]["id"].as_str().unwrap().to_owned();
    let (st, e) = w
        .call(
            w.member,
            Method::PUT,
            &format!("{base}/episodes/{ep2}"),
            Some(json!({ "number": 2, "title": "Second Shift", "synopsis": "More." })),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{e}");
    assert_eq!(e["title"], "Second Shift");

    // The videos are still being processed: not ready, and the problem names the episode.
    let (st, e) = w
        .post(w.member, &format!("/submissions/{sub}/submit"), json!({}))
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");
    assert!(e.to_string().contains("still being processed"), "{e}");

    // Metadata and artwork: a poster set through the artwork route survives a metadata edit, and a
    // storage key in the edit is ignored.
    let (st, _) = w
        .post(
            w.member,
            &format!("/submissions/{sub}/artwork/poster"),
            json!({ "image": png_data_uri() }),
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    let (st, t) = w
        .call(
            w.member,
            Method::PUT,
            &format!("{base}/title"),
            Some(serde_json::to_value(sr_cinema_input(&name)).unwrap()),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{t}");
    assert_eq!(t["synopsis"], "A night job.");
    assert!(t["poster_url"].is_string(), "the stored poster stayed: {t}");
    assert!(!t["poster_url"].as_str().unwrap().contains("evil"), "{t}");

    // The ladders finish; the submission goes in.
    w.make_playable(&asset).await;
    w.make_playable(&a2).await;
    let (st, d) = w
        .post(w.member, &format!("/submissions/{sub}/submit"), json!({}))
        .await;
    assert_eq!(st, StatusCode::OK, "{d}");
    assert_eq!(d["items"].as_array().unwrap().len(), 2, "{d}");

    // Under review the member's draft is frozen.
    let (st, _) = w.begin(w.member, &sub, "Night Shift - s01e03.mkv", 4).await;
    assert_eq!(st, StatusCode::CONFLICT);
    let (st, _) = w
        .call(
            w.member,
            Method::PUT,
            &format!("{base}/title"),
            Some(serde_json::to_value(sr_cinema_input(&name)).unwrap()),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);

    // A reviewer reads the draft and the title is private until approval.
    let sid: i64 = sub.parse().unwrap();
    let title: i64 = w
        .review
        .get_for_review(w.reviewer, sid)
        .await
        .unwrap()
        .submission
        .ref_id
        .unwrap();
    // A reviewer of the kind may read the draft; a plain member may not (the harness has no staff
    // sessions, so this is checked on the service the routes call).
    w.review.readable_for(w.reviewer, sid).await.unwrap();
    assert!(
        w.review
            .readable_for(Caller::member(w.other), sid)
            .await
            .is_err()
    );
    assert!(w.cinema.title_detail(w.viewer(), title).await.is_err());
    w.review.claim(w.reviewer, sid, false).await.unwrap();
    let d = w
        .review
        .approve(w.reviewer, sid, rm::DecisionInput::default())
        .await
        .unwrap();
    assert_eq!(d.submission.state, rm::SubmissionState::Published);
    let t = w.cinema.title_detail(w.viewer(), title).await.unwrap();
    assert!(t.title.published);
    assert_eq!(t.title.episode_count, 2);
    let first = w.cinema.episodes(title, None, None, None).await.unwrap();
    assert_eq!(first.episodes[0].title, "Pilot");
    let play = w
        .cinema
        .resolve_play(w.viewer(), PlayRef::Episode(first.episodes[0].id))
        .await
        .unwrap();
    assert_eq!(play.asset_id.to_string(), asset);
}

fn sr_cinema_input(name: &str) -> TitleInput {
    series_input(name, "A night job.")
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn byte_limits_names_and_ownership_are_enforced() {
    // 15 bytes a day: one 10 byte file fits, a second does not.
    let w = world(15).await;
    let sub = w.new_title(w.member, "series", "Limits").await;
    let base = format!("/submissions/{sub}/cinema");

    // The operator's size limit, an empty file, a foreign container and non-video files.
    let (st, e) = w
        .begin(w.member, &sub, "Limits - s01e01.mkv", MAX_BYTES + 1)
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");
    let (st, _) = w.begin(w.member, &sub, "Limits - s01e01.mkv", 0).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = w.begin(w.member, &sub, "Limits - s01e01.exe", 5).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, e) = w.begin(w.member, &sub, "Limits - s01e01.en.srt", 5).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");
    // No season or episode in the name for a series: say so, create nothing.
    let (st, e) = w.begin(w.member, &sub, "holiday.mkv", 5).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{e}");
    assert!(
        e["message"]
            .as_str()
            .unwrap()
            .contains("season and episode"),
        "{e}"
    );
    let (_, d) = w.get(w.member, &format!("{base}/draft")).await;
    assert_eq!(d["items"].as_array().unwrap().len(), 0);
    let (st, _) = w.begin(w.member, &sub, "Limits - s01e08.exe", 5).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (_, d) = w.get(w.member, &format!("{base}/draft")).await;
    assert_eq!(
        d["items"].as_array().unwrap().len(),
        0,
        "a refused file leaves no empty episode: {d}"
    );

    // Explicit numbers work for such a file; a multi-episode name warns.
    let (st, slot) = w
        .post(
            w.member,
            &format!("{base}/files"),
            json!({ "filename": "holiday.mkv", "size_bytes": 10, "season": 2, "episode": 3 }),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{slot}");
    assert_eq!(slot["item"]["label"], "S02E03");
    let asset = slot["asset_id"].as_str().unwrap().to_owned();

    // The daily allowance counts the first 10 bytes; the next 10 are refused and given up.
    let (st, e) = w.begin(w.member, &sub, "Limits - s01e04.mkv", 10).await;
    assert_eq!(st, StatusCode::CONFLICT, "{e}");
    assert!(
        e["message"]
            .as_str()
            .unwrap()
            .contains("daily upload limit"),
        "{e}"
    );
    let removed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM video_assets WHERE state = 'removed' AND uploaded_by = $1",
    )
    .bind(w.member)
    .fetch_one(&w.pool)
    .await
    .unwrap();
    assert_eq!(removed, 1);
    let (_, d) = w.get(w.member, &format!("{base}/draft")).await;
    assert_eq!(
        d["items"].as_array().unwrap().len(),
        1,
        "the refused file left no episode: {d}"
    );

    // Somebody else's submission and upload are invisible.
    for path in [
        format!("{base}/draft"),
        format!("{base}/uploads/{asset}"),
        format!("{base}/uploads/{asset}/parts"),
    ] {
        let (st, _) = w.get(w.other, &path).await;
        assert_eq!(st, StatusCode::NOT_FOUND, "{path}");
    }
    let (st, _) = w
        .post(
            w.other,
            &format!("{base}/uploads/{asset}/complete"),
            json!({}),
        )
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _) = w.begin(w.other, &sub, "Limits - s01e09.mkv", 4).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    // A plain member is not a reviewer, so the draft stays closed to them as well.
    let (st, _) = w.get(w.other, &format!("{base}/draft")).await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    // Giving the upload up frees the parts and discards the asset.
    let data = b"0123456789";
    w.put_parts(&asset, &[1], data).await;
    let sess = w
        .sessions
        .get(asset.parse().unwrap())
        .await
        .unwrap()
        .unwrap();
    let (st, _) = w
        .call(
            w.member,
            Method::DELETE,
            &format!("{base}/uploads/{asset}"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    assert!(w.parts.aborted().contains(&sess.s3_upload_id));
    assert_eq!(w.asset_state(&asset).await, "removed");
    assert!(
        w.sessions
            .get(asset.parse().unwrap())
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_film_takes_one_video_and_a_replacement_gives_up_the_old_one() {
    let w = world(0).await;
    let sub = w.new_title(w.member, "movie", "Quiet Film").await;
    let base = format!("/submissions/{sub}/cinema");
    // A film has no episodes.
    let (st, e) = w
        .post(
            w.member,
            &format!("{base}/episodes"),
            json!({ "number": 1 }),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT, "{e}");

    let first = w
        .upload(w.member, &sub, "Quiet Film (2024).mkv", b"12345678")
        .await;
    let (_, d) = w.get(w.member, &format!("{base}/draft")).await;
    assert_eq!(d["items"][0]["target_kind"], "title");
    assert_eq!(d["items"][0]["video"]["asset_id"], first.as_str());
    // A better file replaces it; the first asset is given up.
    let second = w
        .upload(w.member, &sub, "Quiet Film (2024).mkv", b"abcdefgh")
        .await;
    assert_ne!(first, second);
    assert_eq!(w.asset_state(&first).await, "removed");
    let (_, d) = w.get(w.member, &format!("{base}/draft")).await;
    assert_eq!(d["items"][0]["video"]["asset_id"], second.as_str());
    assert_eq!(d["items"].as_array().unwrap().len(), 1);

    // A film cannot change into a series once it has a video.
    let mut input = series_input("Quiet Film", "x");
    let (st, _) = w
        .call(
            w.member,
            Method::PUT,
            &format!("{base}/title"),
            Some(serde_json::to_value(&input).unwrap()),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);
    input.kind = TitleKind::Movie;
    let (st, _) = w
        .call(
            w.member,
            Method::PUT,
            &format!("{base}/title"),
            Some(serde_json::to_value(&input).unwrap()),
        )
        .await;
    assert_eq!(st, StatusCode::OK);

    w.make_playable(&second).await;
    let (st, d) = w
        .post(w.member, &format!("/submissions/{sub}/submit"), json!({}))
        .await;
    assert_eq!(st, StatusCode::OK, "{d}");
    let sid: i64 = sub.parse().unwrap();
    w.review.claim(w.reviewer, sid, false).await.unwrap();
    w.review
        .approve(w.reviewer, sid, rm::DecisionInput::default())
        .await
        .unwrap();
    let title = w
        .review
        .get_for_review(w.reviewer, sid)
        .await
        .unwrap()
        .submission
        .ref_id
        .unwrap();
    assert!(
        w.cinema
            .title_detail(w.viewer(), title)
            .await
            .unwrap()
            .title
            .published
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn new_episodes_for_a_published_series_stay_inside_the_submission() {
    let w = world(0).await;
    // A published series with episode 1 (made by staff).
    let staff = Actor {
        user_id: w.reviewer.user_id,
        permissions: P::PUBLISH_MEDIA | P::DELETE_CONTENT,
    };
    let t = w
        .cinema
        .create_title(staff, series_input("Old Show", "Aired."))
        .await
        .unwrap();
    let season = w
        .cinema
        .create_season(
            staff,
            t.id,
            sr_cinema::model::SeasonInput {
                number: 1,
                name: None,
                synopsis: None,
                poster_key: None,
            },
        )
        .await
        .unwrap();
    let ep1 = w
        .cinema
        .create_episode(
            staff,
            season.id,
            sr_cinema::model::EpisodeInput {
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
        .link_episode_asset(staff, ep1.id, Some(777))
        .await
        .unwrap();
    w.cinema
        .set_episode_published(staff, ep1.id, true)
        .await
        .unwrap();
    w.cinema
        .set_title_published(staff, t.id, true)
        .await
        .unwrap();

    let (st, b) = w
        .post(
            w.member,
            "/submissions",
            json!({ "kind": KIND_EPISODES, "payload": { "title_id": t.id.to_string(), "season": 1,
                "episodes": [{ "number": 2, "title": "Two" }] } }),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{b}");
    let sub = b["submission"]["id"].as_str().unwrap().to_owned();
    let base = format!("/submissions/{sub}/cinema");

    // The published episode is not part of the submission: its number is taken, and it cannot be edited.
    let (st, e) = w.begin(w.member, &sub, "Old Show - s01e01.mkv", 4).await;
    assert_eq!(st, StatusCode::CONFLICT, "{e}");
    let (st, _) = w
        .call(
            w.member,
            Method::PUT,
            &format!("{base}/episodes/{}", ep1.id),
            Some(json!({ "number": 1, "title": "Hijacked" })),
        )
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _) = w
        .call(
            w.member,
            Method::DELETE,
            &format!("{base}/episodes/{}", ep1.id),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    // The draft metadata of a published series is not the member's to edit.
    let (st, _) = w
        .call(
            w.member,
            Method::PUT,
            &format!("{base}/title"),
            Some(serde_json::to_value(series_input("Old Show", "Mine now")).unwrap()),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);

    // Episode 2 (from the payload) and a new episode 3 (from a file name) take videos.
    let (_, d) = w.get(w.member, &format!("{base}/draft")).await;
    assert_eq!(d["draft"]["published_episodes"], 1);
    assert_eq!(d["items"].as_array().unwrap().len(), 1, "{d}");
    let ep2 = d["items"][0]["target_id"].as_str().unwrap().to_owned();
    let a2 = {
        let (st, slot) = w
            .post(
                w.member,
                &format!("{base}/files"),
                json!({ "filename": "anything.mkv", "size_bytes": 8, "episode_id": ep2 }),
            )
            .await;
        assert_eq!(st, StatusCode::CREATED, "{slot}");
        let a = slot["asset_id"].as_str().unwrap().to_owned();
        w.put_parts(&a, &[1, 2], b"abcdefgh").await;
        let (st, _) = w
            .post(w.member, &format!("{base}/uploads/{a}/complete"), json!({}))
            .await;
        assert_eq!(st, StatusCode::ACCEPTED);
        a
    };
    let a3 = w
        .upload(w.member, &sub, "Old Show - s01e03 - Three.mkv", b"12345678")
        .await;
    let (_, d) = w.get(w.member, &format!("{base}/draft")).await;
    assert_eq!(d["items"].as_array().unwrap().len(), 2, "{d}");
    w.make_playable(&a2).await;
    w.make_playable(&a3).await;

    let (st, d) = w
        .post(w.member, &format!("/submissions/{sub}/submit"), json!({}))
        .await;
    assert_eq!(st, StatusCode::OK, "{d}");
    assert_eq!(d["items"].as_array().unwrap().len(), 2, "{d}");
    let sid: i64 = sub.parse().unwrap();
    w.review.claim(w.reviewer, sid, false).await.unwrap();
    w.review
        .approve(w.reviewer, sid, rm::DecisionInput::default())
        .await
        .unwrap();
    let list = w.cinema.episodes(t.id, None, None, None).await.unwrap();
    let numbers: Vec<i32> = list.episodes.iter().map(|e| e.number).collect();
    assert_eq!(numbers, vec![1, 2, 3]);
}
