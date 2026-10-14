//! DB-backed tests of the Cinema media pipeline: the Postgres job store (claim, lease, retry,
//! requeue), worker tokens, the upload/probe/plan flow with fakes for storage and ffprobe, the
//! effects of a finished ladder (renditions, playback data) and the Patron download rules.
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test media_repo -- --ignored --test-threads=1`

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicI64, Ordering},
};

use async_trait::async_trait;
use bytes::Bytes;
use sr_db::{
    PgPool,
    accounts::{self, NewAccount},
    media_repo::{PgDownloadEntitlements, PgMediaRepo},
    testing::TestDb,
};
use sr_media::assets::*;
use sr_media::downloads::*;
use sr_media::output_storage::SinglePutStorage;
use sr_media::plan::{TranscodeSettings, plan_ladder_job};
use sr_media::probe::parse_ffprobe_json;
use sr_media::runner::{FakeResult, FakeRunner};
use sr_media::service::*;
use sr_media::worker_api::*;
use sr_storage::cdn::CdnSigner;
use sr_storage::{ObjectStorage, ObjectStoreStorage};

const PROBE_JSON: &str = r#"{"streams":[
  {"index":0,"codec_name":"h264","codec_type":"video","width":1920,"height":1080,
   "pix_fmt":"yuv420p","field_order":"progressive","r_frame_rate":"24000/1001",
   "avg_frame_rate":"24000/1001","disposition":{"default":1}},
  {"index":1,"codec_name":"ac3","codec_type":"audio","channels":6,"sample_rate":"48000",
   "disposition":{"default":1},"tags":{"language":"eng"}},
  {"index":2,"codec_name":"subrip","codec_type":"subtitle","tags":{"language":"spa"}}
],"format":{"format_name":"matroska,webm","duration":"1200.0"}}"#;

fn next_id() -> i64 {
    static N: AtomicI64 = AtomicI64::new(0);
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + N.fetch_add(1, Ordering::Relaxed) % 1000
}

fn ids() -> IdSource {
    Arc::new(next_id)
}

async fn account(pool: &PgPool) -> i64 {
    let id = next_id();
    let name = format!("t{id}");
    let ok = accounts::create_account(
        pool,
        &NewAccount {
            id,
            person_id: next_id(),
            username: &name,
            display_name: None,
            email: &format!("{name}@example.test"),
            password_hash: "x",
            state: "active",
            email_verified: true,
            initial_credits: 0,
            flags: 0,
        },
        next_id(),
    )
    .await
    .unwrap();
    assert!(ok);
    id
}

#[derive(Default)]
struct RecordingQueue(Mutex<Vec<i64>>);

#[async_trait]
impl ProbeQueue for RecordingQueue {
    async fn enqueue_probe(&self, asset_id: i64) -> Result<(), String> {
        self.0.lock().unwrap().push(asset_id);
        Ok(())
    }
}

struct World {
    _db: TestDb,
    pool: PgPool,
    repo: Arc<PgMediaRepo>,
    storage: Arc<ObjectStoreStorage>,
    svc: MediaService,
    queue: Arc<RecordingQueue>,
    spool: std::path::PathBuf,
}

async fn world(ffprobe: FakeRunner) -> World {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let repo = Arc::new(PgMediaRepo::new(pool.clone(), ids()));
    let storage = Arc::new(ObjectStoreStorage::in_memory());
    let queue = Arc::new(RecordingQueue::default());
    let spool = std::env::temp_dir().join(format!("sr-media-test-{}", next_id()));
    let settings = MediaSettings {
        spool_dir: spool.clone(),
        ..MediaSettings::default()
    };
    let svc = MediaService::new(
        repo.clone(),
        storage.clone(),
        Arc::new(SinglePutStorage::new(storage.clone())),
        Arc::new(ffprobe),
        Arc::new(StaticMediaSettings(settings)),
        queue.clone(),
        ids(),
        Arc::new(|| 1_700_000_000),
        Arc::new(|| Some([7u8; 32])),
    )
    .with_part_size(u64::MAX / 4);
    World {
        _db: db,
        pool,
        repo,
        storage,
        svc,
        queue,
        spool,
    }
}

fn probe_ok() -> FakeRunner {
    FakeRunner::new(|_| FakeResult::ok(PROBE_JSON.to_owned()))
}

fn worker_service(repo: Arc<PgMediaRepo>, now: Arc<std::sync::atomic::AtomicU64>) -> WorkerService {
    WorkerService {
        store: repo,
        outputs: Arc::new(MemoryOutputStorage::default()),
        downloads: Arc::new(CdnSigner::new(b"secret".to_vec())),
        clock: Arc::new(move || now.load(Ordering::SeqCst)),
        config: ServiceConfig::default(),
    }
}

/// Upload a file through the service, as the browser would, and return the asset.
async fn upload(w: &World, user: i64, auto_ladder: bool) -> Asset {
    let slot = w
        .svc
        .begin_upload(
            user,
            UploadRequest {
                filename: "Movie.mkv".into(),
                size_bytes: 5,
                title_id: None,
                episode_id: None,
                auto_ladder: Some(auto_ladder),
            },
        )
        .await
        .unwrap();
    assert_eq!(slot.asset.state, AssetState::AwaitingUpload);
    assert_eq!(slot.parts.len(), 1);
    // The PUT to the presigned URL, simulated.
    w.storage
        .put(&slot.asset.original_key, Bytes::from_static(b"12345"))
        .await
        .unwrap();
    let done = w
        .svc
        .complete_upload(
            slot.asset.id,
            CompleteUploadRequest {
                upload_id: slot.upload_id.clone(),
                parts: vec![PartDone {
                    part_number: 1,
                    etag: "e".into(),
                }],
            },
        )
        .await
        .unwrap();
    assert_eq!(done.state, AssetState::Uploaded);
    done
}

/// Run a claimed job's whole upload/complete protocol through the real service.
async fn finish_ladder(svc: &WorkerService, token: &str, now: u64) -> (String, String) {
    let c = svc.authenticate(Some(token)).await.unwrap();
    let wid = svc
        .register(
            &c,
            RegisterRequest {
                name: "gpu".into(),
                version: "1".into(),
                capabilities: WorkerCapabilities::default(),
            },
        )
        .await
        .unwrap()
        .worker_id;
    let job = svc
        .claim(
            &c,
            &wid,
            ClaimRequest {
                kinds: vec![JobKind::Ladder],
            },
        )
        .await
        .unwrap()
        .job
        .expect("a ladder job is queued");
    let _ = now;
    let JobSpec::Ladder { plan } = &job.spec else {
        panic!("not a ladder")
    };
    let mut paths = plan.required_outputs();
    for r in &plan.rungs {
        paths.push(format!("{}/seg_00000.m4s", r.dir));
    }
    for a in &plan.audio {
        paths.push(format!("{}/seg_00000.m4s", a.dir));
    }
    let mut outputs = Vec::new();
    for p in &paths {
        let init = svc
            .init_upload(
                &c,
                &job.job_id,
                InitUploadRequest {
                    path: p.clone(),
                    size: 1000,
                },
            )
            .await
            .unwrap();
        svc.finish_upload(
            &c,
            &job.job_id,
            &init.upload_id,
            FinishUploadRequest {
                parts: vec![PartDone {
                    part_number: 1,
                    etag: "e".into(),
                }],
            },
        )
        .await
        .unwrap();
        outputs.push(OutputRef {
            path: p.clone(),
            size: 1000,
        });
    }
    let index: Vec<SegmentIndex> = plan
        .rungs
        .iter()
        .map(|r| r.playlist.clone())
        .chain(plan.audio.iter().map(|a| a.playlist.clone()))
        .map(|playlist| SegmentIndex {
            playlist,
            init: "init.mp4".into(),
            segments: vec![SegmentEntry {
                name: "seg_00000.m4s".into(),
                duration_secs: 6.0,
                size_bytes: 1000,
            }],
        })
        .collect();
    svc.complete(
        &c,
        &job.job_id,
        CompleteRequest {
            outputs,
            segment_index: index,
            encoder: "libx264".into(),
            wall_secs: 5.0,
            log_tail: None,
        },
    )
    .await
    .unwrap();
    (job.job_id, wid)
}

fn new_job(asset: i64, spec: JobSpec, priority: u8) -> NewJob {
    NewJob {
        asset_id: asset as u64,
        spec,
        source_key: format!("originals/{asset}/source.mkv"),
        source_size: 5,
        source_extension: "mkv".into(),
        priority,
    }
}

fn ladder_spec() -> JobSpec {
    let probe = parse_ffprobe_json(PROBE_JSON.as_bytes()).unwrap();
    JobSpec::Ladder {
        plan: Box::new(plan_ladder_job(&probe, &TranscodeSettings::default()).unwrap()),
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn claim_orders_by_priority_is_exclusive_and_keeps_leases() {
    let w = world(probe_ok()).await;
    let a = upload(&w, account(&w.pool).await, false).await;
    let low = w
        .repo
        .enqueue(new_job(a.id, ladder_spec(), 1))
        .await
        .unwrap();
    let high = w
        .repo
        .enqueue(new_job(a.id, ladder_spec(), 9))
        .await
        .unwrap();
    let low2 = w
        .repo
        .enqueue(new_job(a.id, ladder_spec(), 1))
        .await
        .unwrap();
    let token = w
        .svc
        .create_token(account(&w.pool).await, "gpu")
        .await
        .unwrap();
    let info = w
        .repo
        .authenticate(&token_hash(&token.secret))
        .await
        .unwrap()
        .unwrap();
    let worker = w
        .repo
        .upsert_worker(
            &info,
            &RegisterRequest {
                name: "w".into(),
                version: "1".into(),
                capabilities: WorkerCapabilities::default(),
            },
            1000,
        )
        .await
        .unwrap();
    // Three claimers race for three jobs: each gets a different one, best first.
    let kinds = [JobKind::Ladder];
    let (r1, r2, r3, r4) = tokio::join!(
        w.repo.claim_next(&worker.worker_id, &kinds, 1000, 120),
        w.repo.claim_next(&worker.worker_id, &kinds, 1000, 120),
        w.repo.claim_next(&worker.worker_id, &kinds, 1000, 120),
        w.repo.claim_next(&worker.worker_id, &kinds, 1000, 120),
    );
    let mut got: Vec<String> = [r1, r2, r3, r4]
        .into_iter()
        .filter_map(|r| r.unwrap())
        .map(|j| j.job_id)
        .collect();
    assert_eq!(got.len(), 3, "the fourth claim finds nothing");
    got.sort();
    let mut want = vec![low.clone(), high.clone(), low2.clone()];
    want.sort();
    assert_eq!(got, want);
    // Sequential order: priority first, then age.
    let w2 = world(probe_ok()).await;
    let a2 = upload(&w2, account(&w2.pool).await, false).await;
    let l1 = w2
        .repo
        .enqueue(new_job(a2.id, ladder_spec(), 1))
        .await
        .unwrap();
    let h1 = w2
        .repo
        .enqueue(new_job(a2.id, ladder_spec(), 9))
        .await
        .unwrap();
    let l2 = w2
        .repo
        .enqueue(new_job(a2.id, ladder_spec(), 1))
        .await
        .unwrap();
    let t2 = w2
        .svc
        .create_token(account(&w2.pool).await, "g")
        .await
        .unwrap();
    let i2 = w2
        .repo
        .authenticate(&token_hash(&t2.secret))
        .await
        .unwrap()
        .unwrap();
    let wk = w2
        .repo
        .upsert_worker(
            &i2,
            &RegisterRequest {
                name: "w".into(),
                version: "1".into(),
                capabilities: WorkerCapabilities::default(),
            },
            1000,
        )
        .await
        .unwrap();
    let mut order = Vec::new();
    while let Some(j) = w2
        .repo
        .claim_next(&wk.worker_id, &kinds, 1000, 120)
        .await
        .unwrap()
    {
        assert_eq!(j.attempt, 1);
        assert_eq!(j.lease_expires_at, Some(1120));
        order.push(j.job_id);
    }
    assert_eq!(order, vec![h1.clone(), l1, l2]);

    // Progress extends the lease; another worker cannot report.
    let p = ProgressRequest {
        stage: Stage::Video,
        percent: 40.0,
        fps: None,
        speed: None,
        out_time_secs: None,
        message: None,
    };
    let rec = w2
        .repo
        .report_progress(&h1, &wk.worker_id, &p, 1100, 120)
        .await
        .unwrap();
    assert_eq!(rec.lease_expires_at, Some(1220));
    assert_eq!(rec.progress.unwrap().percent, 40.0);
    assert!(
        w2.repo
            .report_progress(&h1, "nobody", &p, 1100, 120)
            .await
            .is_err()
    );

    // Expired leases: requeued with the attempt kept, failed after three attempts.
    assert!(
        w2.repo.reap_expired(1219, 3).await.unwrap().len() == 2,
        "the two other jobs lapsed at 1120"
    );
    let again = w2.repo.reap_expired(1221, 3).await.unwrap();
    assert_eq!(again, vec![h1.clone()]);
    let rec = w2.repo.get_job(&h1).await.unwrap().unwrap();
    assert_eq!((rec.state, rec.attempt), (JobState::Queued, 1));
    for round in 2..=3u32 {
        let j = w2
            .repo
            .claim_next(&wk.worker_id, &kinds, 2000, 120)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(j.attempt, if j.job_id == h1 { round } else { j.attempt });
        let _ = w2.repo.reap_expired(5000, 3).await.unwrap();
    }
    let rec = w2.repo.get_job(&h1).await.unwrap().unwrap();
    assert_eq!(rec.state, JobState::Failed, "three attempts, then failed");
    assert_eq!(rec.error.as_deref(), Some("worker lease expired"));
    let asset = w2.repo.get_asset(a2.id).await.unwrap().unwrap();
    assert_eq!(
        asset.state,
        AssetState::Uploaded,
        "only a transcoding asset is marked failed"
    );

    // Retryable fail requeues, non-retryable fails, cancel of a queued job is immediate.
    let a3 = upload(&w2, account(&w2.pool).await, true).await;
    let j = w2
        .repo
        .enqueue(new_job(a3.id, ladder_spec(), 5))
        .await
        .unwrap();
    w2.repo
        .transition_asset(
            a3.id,
            &[AssetState::Uploaded],
            AssetState::Transcoding,
            None,
        )
        .await
        .unwrap();
    let c = w2
        .repo
        .claim_next(&wk.worker_id, &kinds, 3000, 120)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(c.job_id, j);
    let fail = |retryable| FailRequest {
        stage: Stage::Video,
        message: "boom".into(),
        log_tail: Some("log".into()),
        retryable,
        cancelled: false,
    };
    assert_eq!(
        w2.repo
            .fail_job(&j, &wk.worker_id, &fail(true), 3)
            .await
            .unwrap(),
        FailOutcome::Requeued
    );
    w2.repo
        .claim_next(&wk.worker_id, &kinds, 3000, 120)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        w2.repo
            .fail_job(&j, &wk.worker_id, &fail(false), 3)
            .await
            .unwrap(),
        FailOutcome::Failed
    );
    let asset = w2.repo.get_asset(a3.id).await.unwrap().unwrap();
    assert_eq!(
        (asset.state, asset.error.as_deref()),
        (AssetState::Failed, Some("boom"))
    );
    assert!(
        w2.repo
            .fail_job(&j, &wk.worker_id, &fail(false), 3)
            .await
            .is_err()
    );
    // Retrying the failed job puts the asset back to transcoding.
    assert!(w2.svc.retry_job(j.parse().unwrap()).await.is_ok());
    assert_eq!(
        w2.repo.get_asset(a3.id).await.unwrap().unwrap().state,
        AssetState::Transcoding
    );
    w2.repo.request_cancel(&j).await.unwrap();
    assert_eq!(
        w2.repo.get_job(&j).await.unwrap().unwrap().state,
        JobState::Cancelled
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn tokens_are_hashed_revocable_and_drain_their_workers() {
    let w = world(probe_ok()).await;
    let admin = account(&w.pool).await;
    let created = w.svc.create_token(admin, "gpu-box").await.unwrap();
    assert!(created.secret.starts_with("srw_"));
    let stored: String = sqlx::query_scalar("SELECT token_sha256 FROM worker_tokens WHERE id = $1")
        .bind(created.token.id)
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(stored, token_hash(&created.secret));
    assert!(!stored.contains(&created.secret));
    assert!(w.svc.create_token(admin, "  ").await.is_err());

    let now = Arc::new(std::sync::atomic::AtomicU64::new(1000));
    let svc = worker_service(w.repo.clone(), now);
    let c = svc.authenticate(Some(&created.secret)).await.unwrap();
    assert_eq!(c.0.name, "gpu-box");
    assert!(svc.authenticate(Some("srw_wrong")).await.is_err());
    let reg = svc
        .register(
            &c,
            RegisterRequest {
                name: "gpu".into(),
                version: "1".into(),
                capabilities: WorkerCapabilities::default(),
            },
        )
        .await
        .unwrap();
    // Registering again keeps the worker id.
    let again = svc
        .register(
            &c,
            RegisterRequest {
                name: "gpu".into(),
                version: "2".into(),
                capabilities: WorkerCapabilities::default(),
            },
        )
        .await
        .unwrap();
    assert_eq!(reg.worker_id, again.worker_id);
    let list = w.svc.workers().await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].version, "2");
    // Admin drain reaches the worker on its heartbeat.
    w.svc.set_drain(&reg.worker_id, true).await.unwrap();
    let hb = svc
        .heartbeat(&c, &reg.worker_id, HeartbeatRequest::default())
        .await
        .unwrap();
    assert!(hb.drain);
    w.svc.set_drain(&reg.worker_id, false).await.unwrap();
    // Revoking stops authentication at once and drains what is registered.
    w.svc.revoke_token(created.token.id).await.unwrap();
    assert!(svc.authenticate(Some(&created.secret)).await.is_err());
    assert!(
        w.svc.revoke_token(created.token.id).await.is_err(),
        "already revoked"
    );
    let list = w.svc.workers().await.unwrap();
    assert!(list[0].drain && list[0].token_revoked && !list[0].online);
    assert!(w.svc.tokens().await.unwrap()[0].revoked_at_ms.is_some());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn upload_probe_plan_transcode_and_playback_data() {
    let w = world(probe_ok()).await;
    let user = account(&w.pool).await;
    // Limits: wrong type, empty, too large.
    for (name, size) in [("a.exe", 5u64), ("a.mkv", 0), ("a.mkv", u64::MAX / 2)] {
        let r = w
            .svc
            .begin_upload(
                user,
                UploadRequest {
                    filename: name.into(),
                    size_bytes: size,
                    title_id: None,
                    episode_id: None,
                    auto_ladder: None,
                },
            )
            .await;
        assert!(matches!(r, Err(MediaError::Invalid(_))), "{name} {size}");
    }
    let asset = upload(&w, user, true).await;
    assert_eq!(w.queue.0.lock().unwrap().as_slice(), &[asset.id]);
    // Completing twice is harmless; a size mismatch is refused.
    let wrong = w
        .svc
        .begin_upload(
            user,
            UploadRequest {
                filename: "b.mp4".into(),
                size_bytes: 10,
                title_id: None,
                episode_id: None,
                auto_ladder: None,
            },
        )
        .await
        .unwrap();
    w.storage
        .put(&wrong.asset.original_key, Bytes::from_static(b"123"))
        .await
        .unwrap();
    let r = w
        .svc
        .complete_upload(
            wrong.asset.id,
            CompleteUploadRequest {
                upload_id: wrong.upload_id,
                parts: vec![PartDone {
                    part_number: 1,
                    etag: "e".into(),
                }],
            },
        )
        .await;
    assert!(matches!(r, Err(MediaError::Invalid(_))));
    assert_eq!(
        w.svc.get(wrong.asset.id).await.unwrap().state,
        AssetState::Failed
    );

    w.svc.run_probe(asset.id, false).await.unwrap();
    let st = w.svc.status(asset.id).await.unwrap();
    assert_eq!(st.asset.state, AssetState::Transcoding);
    assert_eq!(st.asset.duration_ms, Some(1_200_000));
    assert_eq!(st.probe.as_ref().unwrap().height, Some(1080));
    let kinds: Vec<JobKind> = st.jobs.iter().map(|j| j.kind).collect();
    assert!(kinds.contains(&JobKind::Proxy) && kinds.contains(&JobKind::Ladder));
    let proxy = st.jobs.iter().find(|j| j.kind == JobKind::Proxy).unwrap();
    let ladder = st.jobs.iter().find(|j| j.kind == JobKind::Ladder).unwrap();
    assert!(
        proxy.priority > ladder.priority,
        "the review proxy goes first"
    );
    assert!(
        std::fs::read_dir(&w.spool).map(|d| d.count()).unwrap_or(0) == 0,
        "spool file removed"
    );

    // A worker finishes the ladder: renditions, tracks and the playable prefix appear.
    let admin = account(&w.pool).await;
    let token = w.svc.create_token(admin, "gpu").await.unwrap();
    let svc = worker_service(
        w.repo.clone(),
        Arc::new(std::sync::atomic::AtomicU64::new(1000)),
    );
    let (job_id, _) = finish_ladder(&svc, &token.secret, 1000).await;
    let asset = w.svc.get(asset.id).await.unwrap();
    assert_eq!(asset.state, AssetState::Published);
    assert!(asset.playable && asset.published_at_ms.is_some());
    let pb = w.repo.playback(asset.id).await.unwrap().unwrap();
    assert_eq!(pb.base_path, format!("/cdn/hls/{job_id}/1"));
    assert!(!pb.media.videos.is_empty());
    assert!(
        pb.media
            .videos
            .windows(2)
            .all(|p| p[0].bandwidth_bps <= p[1].bandwidth_bps)
    );
    assert_eq!(pb.media.audios.len(), 1);
    assert_eq!(pb.media.subtitles.len(), 1);
    assert_eq!(pb.duration_secs, 1200.0);
    let signer = CdnSigner::new(b"s".to_vec());
    let master = sr_media::playback::render_file(&pb, "master.m3u8", &signer, 1000).unwrap();
    assert!(master.body.contains(&format!("/cdn/hls/{job_id}/1/v/")));
    let grant = sr_media::playback::playback_grant(&pb, &signer, 1000);
    assert!(
        grant
            .master_url
            .starts_with(&format!("/cdn/hls/{job_id}/1/master.m3u8?ex="))
    );
    assert!(grant.sprites_url.is_some());

    w.svc
        .cancel_job(proxy.job_id.parse().unwrap())
        .await
        .unwrap();
    // A re-transcode replaces the files and queues the old prefix for collection.
    let out = w
        .svc
        .retranscode(sr_media::retranscode::RetranscodeRequest {
            scope: sr_media::retranscode::RetranscodeScope::Asset(asset.id as u64),
            only_older_than_profile: None,
            reason: "test".into(),
        })
        .await
        .unwrap();
    assert_eq!(out.enqueued.len(), 1, "{out:?}");
    assert_eq!(
        w.svc.get(asset.id).await.unwrap().state,
        AssetState::Transcoding
    );
    let (job2, _) = finish_ladder(&svc, &token.secret, 1000).await;
    assert_ne!(job2, job_id);
    let queued: Vec<String> = sqlx::query_scalar("SELECT prefix FROM cinema_media_gc")
        .fetch_all(&w.pool)
        .await
        .unwrap();
    assert_eq!(queued, vec![format!("transcode/{job_id}/1")]);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_server_without_ffprobe_fails_the_asset_with_a_clear_message() {
    let runner = FakeRunner::new(|_| FakeResult::fail(127, "ffprobe: not found"));
    let w = world(runner).await;
    let user = account(&w.pool).await;
    let a = upload(&w, user, true).await;
    w.svc.run_probe(a.id, false).await.unwrap();
    let a = w.svc.get(a.id).await.unwrap();
    assert_eq!(a.state, AssetState::Failed);
    let msg = a.error.unwrap();
    assert!(msg.contains("ffprobe"), "{msg}");
    // Re-probing queues it again once the tool exists.
    let again = w.svc.reprobe(a.id).await.unwrap();
    assert_eq!(again.state, AssetState::Uploaded);
    assert_eq!(w.queue.0.lock().unwrap().len(), 2);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn sweep_applies_retention_and_expires_slots() {
    let w = world(probe_ok()).await;
    let user = account(&w.pool).await;
    // A removed asset past the grace period loses its original; a stale slot expires.
    let a = upload(&w, user, true).await;
    w.repo
        .transition_asset(a.id, &[AssetState::Uploaded], AssetState::Removed, None)
        .await
        .unwrap();
    sqlx::query("UPDATE video_assets SET ended_at = now() - interval '60 days' WHERE id = $1")
        .bind(a.id)
        .execute(&w.pool)
        .await
        .unwrap();
    let slot = w
        .svc
        .begin_upload(
            user,
            UploadRequest {
                filename: "c.mkv".into(),
                size_bytes: 5,
                title_id: None,
                episode_id: None,
                auto_ladder: None,
            },
        )
        .await
        .unwrap();
    sqlx::query("UPDATE video_assets SET created_at = now() - interval '3 days' WHERE id = $1")
        .bind(slot.asset.id)
        .execute(&w.pool)
        .await
        .unwrap();
    let svc = w.svc.clone();
    // The service clock is fixed in 2023, so move the asset times to match it.
    sqlx::query(
        "UPDATE video_assets SET ended_at = to_timestamp(1700000000 - 5184000) WHERE id = $1",
    )
    .bind(a.id)
    .execute(&w.pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE video_assets SET created_at = to_timestamp(1700000000 - 259200) WHERE id = $1",
    )
    .bind(slot.asset.id)
    .execute(&w.pool)
    .await
    .unwrap();
    let report = svc.sweep().await.unwrap();
    assert_eq!(report.originals_deleted, 1, "{report:?}");
    assert_eq!(report.slots_expired, 1);
    assert!(w.storage.head(&a.original_key).await.is_err());
    assert!(svc.get(a.id).await.unwrap().original_deleted);
    assert_eq!(
        svc.get(slot.asset.id).await.unwrap().state,
        AssetState::Expired
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn link_sets_title_and_episode_on_the_asset() {
    let w = world(probe_ok()).await;
    let user = account(&w.pool).await;
    let a = upload(&w, user, true).await;
    let linked = w.svc.link(a.id, Some(77), None).await.unwrap();
    assert_eq!((linked.title_id, linked.episode_id), (Some(77), None));
    let found = w
        .svc
        .list_assets(AssetQuery {
            title_id: Some(77),
            limit: 10,
            ..AssetQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    assert!(w.svc.link(9, Some(1), None).await.is_err());
}

struct Downloads {
    w: World,
    dl: DownloadService,
    asset: Asset,
    token: String,
    jobs: Arc<std::sync::atomic::AtomicU64>,
}

async fn downloads(per_day: i64) -> (Downloads, i64, i64) {
    let w = world(probe_ok()).await;
    let admin = account(&w.pool).await;
    let a = upload(&w, admin, true).await;
    w.svc.run_probe(a.id, false).await.unwrap();
    let token = w.svc.create_token(admin, "gpu").await.unwrap().secret;
    let svc = worker_service(
        w.repo.clone(),
        Arc::new(std::sync::atomic::AtomicU64::new(1000)),
    );
    finish_ladder(&svc, &token, 1000).await;
    let dl = DownloadService::new(
        w.repo.clone(),
        w.repo.clone(),
        Arc::new(PgDownloadEntitlements::new(w.pool.clone())),
        Arc::new(CdnSigner::new(b"s".to_vec())),
        w.storage.clone(),
        ids(),
        Arc::new(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs())
        }),
    );
    let patron = account(&w.pool).await;
    let other = account(&w.pool).await;
    sqlx::query("INSERT INTO cinema_download_grants (user_id, per_day) VALUES ($1, $2), ($3, $2)")
        .bind(patron)
        .bind(per_day as i32)
        .bind(other)
        .execute(&w.pool)
        .await
        .unwrap();
    let asset = w.svc.get(a.id).await.unwrap();
    (
        Downloads {
            w,
            dl,
            asset,
            token,
            jobs: Arc::new(std::sync::atomic::AtomicU64::new(1000)),
        },
        patron,
        other,
    )
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn downloads_are_for_patrons_limited_per_day_and_cached_across_members() {
    let (d, patron, other) = downloads(2).await;
    let nobody = account(&d.w.pool).await;
    let a = d.asset.id;
    // Not a Patron: forbidden everywhere.
    assert!(matches!(
        d.dl.options(nobody, a).await,
        Err(MediaError::Forbidden(_))
    ));
    assert!(matches!(
        d.dl.start(nobody, 1, None, a, "720p").await,
        Err(MediaError::Forbidden(_))
    ));

    let opts = d.dl.options(patron, a).await.unwrap();
    assert_eq!(opts.daily_remaining, Some(2));
    let names: Vec<&str> = opts.renditions.iter().map(|r| r.id.as_str()).collect();
    assert!(
        names.contains(&"1080p") || names.contains(&"720p"),
        "{names:?}"
    );
    assert!(
        !names.contains(&"original"),
        "originals are off until staff enable them"
    );
    let rung = opts.renditions[0].id.clone();
    assert!(!opts.renditions[0].cached);

    // First request queues one remux job.
    let j1 = d.dl.start(patron, 1, None, a, &rung).await.unwrap();
    assert_eq!(j1.state, DownloadState::Queued);
    assert!(j1.url.is_none());
    assert!(matches!(
        d.dl.file_url(patron, j1.id.parse().unwrap()).await,
        Err(MediaError::Conflict(_))
    ));
    // Another member asking for the same rendition shares the preparation.
    let j2 = d.dl.start(other, 1, None, a, &rung).await.unwrap();
    let jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM transcode_jobs WHERE kind = 'remux'")
        .fetch_one(&d.w.pool)
        .await
        .unwrap();
    assert_eq!(jobs, 1);
    // Unknown renditions are refused; the original needs the staff switch.
    assert!(d.dl.start(patron, 1, None, a, "4320p").await.is_err());
    assert!(matches!(
        d.dl.start(patron, 1, None, a, ORIGINAL).await,
        Err(MediaError::Forbidden(_))
    ));
    // The daily limit counts requests (2 allowed, 2 used).
    assert!(matches!(
        d.dl.start(patron, 1, None, a, &rung).await.map(|_| ()),
        Ok(())
    ));
    assert!(matches!(
        d.dl.start(patron, 1, None, a, &rung).await,
        Err(MediaError::RateLimited(_))
    ));
    assert_eq!(
        d.dl.options(patron, a).await.unwrap().daily_remaining,
        Some(0)
    );

    // A worker runs the remux job; the download becomes ready and the link works for its owner only.
    let svc = worker_service(d.w.repo.clone(), d.jobs.clone());
    let c = svc.authenticate(Some(&d.token)).await.unwrap();
    let wid = svc
        .register(
            &c,
            RegisterRequest {
                name: "cpu".into(),
                version: "1".into(),
                capabilities: WorkerCapabilities::default(),
            },
        )
        .await
        .unwrap()
        .worker_id;
    let job = svc
        .claim(
            &c,
            &wid,
            ClaimRequest {
                kinds: vec![JobKind::Remux],
            },
        )
        .await
        .unwrap()
        .job
        .unwrap();
    assert!(!job.inputs.is_empty());
    assert!(
        job.inputs
            .iter()
            .all(|i| i.url.starts_with("/cdn/hls/") && i.url.contains("sig="))
    );
    let mid = d.dl.job(patron, j1.id.parse().unwrap()).await.unwrap();
    assert_eq!(mid.state, DownloadState::Preparing);
    let init = svc
        .init_upload(
            &c,
            &job.job_id,
            InitUploadRequest {
                path: "remux/video.mp4".into(),
                size: 4096,
            },
        )
        .await
        .unwrap();
    svc.finish_upload(
        &c,
        &job.job_id,
        &init.upload_id,
        FinishUploadRequest {
            parts: vec![PartDone {
                part_number: 1,
                etag: "e".into(),
            }],
        },
    )
    .await
    .unwrap();
    svc.complete(
        &c,
        &job.job_id,
        CompleteRequest {
            outputs: vec![OutputRef {
                path: "remux/video.mp4".into(),
                size: 4096,
            }],
            segment_index: vec![],
            encoder: "copy".into(),
            wall_secs: 3.0,
            log_tail: None,
        },
    )
    .await
    .unwrap();
    let ready = d.dl.job(patron, j1.id.parse().unwrap()).await.unwrap();
    assert_eq!(ready.state, DownloadState::Ready);
    assert_eq!(
        ready.url.as_deref(),
        Some(&*format!("/cinema/downloads/{}/file", j1.id))
    );
    assert!(ready.expires_at_ms.is_some());
    let url = d.dl.file_url(patron, j1.id.parse().unwrap()).await.unwrap();
    assert!(
        url.starts_with(&format!("/cdn/hls/{}/1/remux/video.mp4?ex=", job.job_id)),
        "{url}"
    );
    // Someone else's download id is a 404 for everyone but its owner.
    assert!(matches!(
        d.dl.job(other, j1.id.parse().unwrap()).await,
        Err(MediaError::NotFound(_))
    ));
    assert!(d.dl.job(other, j2.id.parse().unwrap()).await.is_ok());
    // The picker now shows the rendition as cached.
    assert!(
        d.dl.options(other, a)
            .await
            .unwrap()
            .renditions
            .iter()
            .any(|r| r.id == rung && r.cached)
    );

    // After seven days the entry expires and its object goes.
    d.w.storage
        .put(
            &format!("transcode/{}/1/remux/video.mp4", job.job_id),
            Bytes::from_static(b"mp4"),
        )
        .await
        .unwrap();
    sqlx::query("UPDATE cinema_download_cache SET expires_at = now() - interval '1 minute'")
        .execute(&d.w.pool)
        .await
        .unwrap();
    assert_eq!(d.dl.sweep().await.unwrap(), 1);
    assert!(
        d.w.storage
            .head(&format!("transcode/{}/1/remux/video.mp4", job.job_id))
            .await
            .is_err()
    );
    assert!(matches!(
        d.dl.file_url(patron, j1.id.parse().unwrap()).await,
        Err(MediaError::Conflict(_))
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn original_downloads_need_the_staff_switch() {
    let (d, patron, _) = downloads(5).await;
    let a = d.asset.id;
    d.w.svc.set_original_downloads(a, true).await.unwrap();
    let opts = d.dl.options(patron, a).await.unwrap();
    let orig = opts.renditions.iter().find(|r| r.id == ORIGINAL).unwrap();
    assert_eq!(orig.kind, DownloadKind::Original);
    let job = d.dl.start(patron, 1, None, a, ORIGINAL).await.unwrap();
    assert_eq!(job.state, DownloadState::Ready);
    let url =
        d.dl.file_url(patron, job.id.parse().unwrap())
            .await
            .unwrap();
    assert!(
        url.starts_with(&format!("/cdn/originals/{a}/source.mkv?ex=")),
        "{url}"
    );
    d.w.svc.set_original_downloads(a, false).await.unwrap();
    assert!(
        d.dl.file_url(patron, job.id.parse().unwrap())
            .await
            .is_err()
    );
}
