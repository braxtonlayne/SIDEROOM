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
    media_repo::PgMediaRepo,
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
        // Fresh bytes per call (the first call is still all sevens): two worker tokens made in one test must
        // not share a secret, or the second insert hits the unique token hash.
        {
            let calls = std::sync::atomic::AtomicU8::new(0);
            Arc::new(move || {
                let mut b = [7u8; 32];
                b[0] = 7u8.wrapping_add(calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst));
                Some(b)
            })
        },
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
        mounts: None,
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
        w.repo
            .claim_next(&worker.worker_id, &kinds, TokenScope::Instance, 1000, 120),
        w.repo
            .claim_next(&worker.worker_id, &kinds, TokenScope::Instance, 1000, 120),
        w.repo
            .claim_next(&worker.worker_id, &kinds, TokenScope::Instance, 1000, 120),
        w.repo
            .claim_next(&worker.worker_id, &kinds, TokenScope::Instance, 1000, 120),
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
        .claim_next(&wk.worker_id, &kinds, TokenScope::Instance, 1000, 120)
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
            .claim_next(&wk.worker_id, &kinds, TokenScope::Instance, 2000, 120)
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
        .claim_next(&wk.worker_id, &kinds, TokenScope::Instance, 3000, 120)
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
        .claim_next(&wk.worker_id, &kinds, TokenScope::Instance, 3000, 120)
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
    assert_eq!(pb.base_path, format!("/cdn/transcode/{job_id}/1"));
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
    let master = sr_media::grant::render_file(&pb, "master.m3u8", &signer, 1000).unwrap();
    assert!(
        master
            .body
            .contains(&format!("/cdn/transcode/{job_id}/1/v/"))
    );
    let grant = sr_media::grant::playback_grant(&pb, &signer, 1000);
    assert!(
        grant
            .master_url
            .starts_with(&format!("/cdn/transcode/{job_id}/1/master.m3u8?ex="))
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

/// The Patron perk as the entitlements would report it: a fixed allowance for the listed members.
struct Patrons {
    members: Mutex<Vec<i64>>,
    per_day: u32,
}

#[async_trait]
impl DownloadEntitlements for Patrons {
    async fn allowance(&self, user: i64) -> MediaResult<Option<Allowance>> {
        Ok(self
            .members
            .lock()
            .unwrap()
            .contains(&user)
            .then_some(Allowance {
                per_day: self.per_day,
            }))
    }
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
    let patrons = Arc::new(Patrons {
        members: Mutex::new(Vec::new()),
        per_day: per_day as u32,
    });
    let dl = DownloadService::new(
        w.repo.clone(),
        w.repo.clone(),
        patrons.clone(),
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
    patrons.members.lock().unwrap().extend([patron, other]);
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
            .all(|i| i.url.starts_with("/cdn/transcode/") && i.url.contains("sig="))
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
        url.starts_with(&format!(
            "/cdn/transcode/{}/1/remux/video.mp4?ex=",
            job.job_id
        )),
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

// ---- BYOS-CNV-003 (own block): mount_transcode jobs and mount-scoped worker tokens ----------------------

async fn register_worker(w: &World, info: &TokenInfo, name: &str) -> String {
    w.repo
        .upsert_worker(
            info,
            &RegisterRequest {
                name: name.into(),
                version: "1".into(),
                capabilities: WorkerCapabilities::default(),
            },
            1000,
        )
        .await
        .unwrap()
        .worker_id
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_members_worker_token_claims_only_its_own_mounts_jobs() {
    use sr_media::mount_job::{MountFormat, MountPreset};
    let w = world(probe_ok()).await;
    let admin = account(&w.pool).await;
    let t7 = w
        .svc
        .create_mount_token(admin, "member-7", 7)
        .await
        .unwrap();
    let t8 = w
        .svc
        .create_mount_token(admin, "member-8", 8)
        .await
        .unwrap();
    let op = w.svc.create_token(admin, "operator").await.unwrap();
    assert_eq!(t7.token.scope, "mount:7");
    assert_eq!(op.token.scope, "instance");
    let auth = |secret: String| {
        let repo = w.repo.clone();
        async move {
            repo.authenticate(&token_hash(&secret))
                .await
                .unwrap()
                .unwrap()
        }
    };
    let (i7, i8, io) = (
        auth(t7.secret.clone()).await,
        auth(t8.secret.clone()).await,
        auth(op.secret.clone()).await,
    );
    assert_eq!(i7.scope, TokenScope::Mount(7));
    assert_eq!(io.scope, TokenScope::Instance);
    let (w7, w8, wo) = (
        register_worker(&w, &i7, "box7").await,
        register_worker(&w, &i8, "box8").await,
        register_worker(&w, &io, "ops").await,
    );

    // A private job of mount 7 (the operator did not offer its workers), a shared one of mount 9, a ladder.
    let private = w
        .svc
        .queue_mount_transcode(
            7,
            "Films/a.mkv",
            10,
            MountFormat::Mp4,
            MountPreset::Low,
            false,
        )
        .await
        .unwrap();
    let shared = w
        .svc
        .queue_mount_transcode(
            9,
            "Films/b.mkv",
            10,
            MountFormat::Hls,
            MountPreset::High,
            true,
        )
        .await
        .unwrap();
    let asset = upload(&w, admin, true).await;
    let ladder = w
        .repo
        .enqueue(new_job(asset.id, ladder_spec(), 5))
        .await
        .unwrap();
    let all = JobKind::ALL;

    // Mount 8's token: nothing, whatever kinds it asks for.
    assert!(
        w.repo
            .claim_next(&w8, &all, i8.scope, 1000, 120)
            .await
            .unwrap()
            .is_none()
    );
    // The operator's token: the shared mount job and the ladder, never the private job.
    let mut got = Vec::new();
    while let Some(j) = w
        .repo
        .claim_next(&wo, &all, io.scope, 1000, 120)
        .await
        .unwrap()
    {
        got.push(j.job_id);
    }
    got.sort();
    let mut want = vec![shared.clone(), ladder.clone()];
    want.sort();
    assert_eq!(got, want);
    // Mount 7's token: its private job only (not mount 9's, not the ladder).
    let j = w
        .repo
        .claim_next(&w7, &all, i7.scope, 1000, 120)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(j.job_id, private);
    assert_eq!(j.spec.mount_id(), Some(7));
    assert!(
        w.repo
            .claim_next(&w7, &all, i7.scope, 1000, 120)
            .await
            .unwrap()
            .is_none()
    );

    // Finishing records what the member's player opens.
    w.repo
        .complete_job(
            &private,
            &w7,
            &CompleteRequest {
                outputs: vec![OutputRef {
                    path: "proxy/480p.mp4".into(),
                    size: 5,
                }],
                segment_index: Vec::new(),
                encoder: "libx264".into(),
                wall_secs: 1.0,
                log_tail: None,
            },
        )
        .await
        .unwrap();
    let (entry, prefix): (String, String) = sqlx::query_as(
        "SELECT entry, prefix FROM storage_object_transcodes WHERE mount_id = 7 AND key = 'Films/a.mkv'",
    )
    .fetch_one(&w.pool)
    .await
    .unwrap();
    assert_eq!(entry, "proxy/480p.mp4");
    assert_eq!(prefix, "transcoded/Films/a.mkv/");

    // The jobs of one mount list by mount; a token revokes only through its own mount.
    let listed = w.svc.mount_jobs(7, 10).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].mount_id, Some(7));
    let id7: i64 = t7.token.id;
    assert!(w.svc.revoke_mount_token(8, id7).await.is_err());
    assert_eq!(w.svc.mount_tokens(7).await.unwrap().len(), 1);
    w.svc.revoke_mount_token(7, id7).await.unwrap();
    assert!(
        w.repo
            .authenticate(&token_hash(&t7.secret))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_database_refuses_a_malformed_token_scope_and_a_mount_job_without_a_mount() {
    let w = world(probe_ok()).await;
    let bad = sqlx::query(
        "INSERT INTO worker_tokens (id, name, token_sha256, scope) VALUES ($1, 'x', $2, 'guild:3')",
    )
    .bind(next_id())
    .bind("0".repeat(64))
    .execute(&w.pool)
    .await;
    assert!(bad.is_err(), "scope must be instance or mount:<id>");
    let bad = sqlx::query(
        "INSERT INTO transcode_jobs (id, asset_id, kind, spec, source_key, source_size, source_ext) \
         VALUES ($1, 0, 'mount_transcode', '{}'::jsonb, 'a.mkv', 1, 'mkv')",
    )
    .bind(next_id())
    .execute(&w.pool)
    .await;
    assert!(bad.is_err(), "a mount job names its mount");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn parallel_download_starts_cannot_pass_the_daily_allowance() {
    // SEC-AUD-035: the count and the insert are one step, so a burst of starts records at most `per_day`.
    let (d, patron, _) = downloads(2).await;
    let a = d.asset.id;
    d.w.svc.set_original_downloads(a, true).await.unwrap();
    let s = || d.dl.start(patron, 1, None, a, ORIGINAL);
    let (r1, r2, r3, r4, r5, r6) = tokio::join!(s(), s(), s(), s(), s(), s());
    let results = [r1, r2, r3, r4, r5, r6];
    let ok = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(ok, 2, "{results:?}");
    assert!(
        results
            .iter()
            .filter_map(|r| r.as_ref().err())
            .all(|e| matches!(e, MediaError::RateLimited(_))),
        "{results:?}"
    );
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM cinema_downloads WHERE user_id = $1")
        .bind(patron)
        .fetch_one(&d.w.pool)
        .await
        .unwrap();
    assert_eq!(rows, 2);
}

async fn remux_job_row(pool: &PgPool, asset: i64, state: &str) -> i64 {
    let id = next_id();
    sqlx::query(
        "INSERT INTO transcode_jobs (id, asset_id, kind, spec, source_key, source_size, source_ext, state) \
         VALUES ($1, $2, 'remux', '{}'::jsonb, 'a.mkv', 1, 'mkv', $3)",
    )
    .bind(id)
    .bind(asset)
    .bind(state)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn cache_row(pool: &PgPool, asset: i64, job: i64, state: &str) -> i64 {
    let id = next_id();
    sqlx::query(
        "INSERT INTO cinema_download_cache (id, asset_id, rendition, state, job_id) VALUES ($1, $2, 'r1', $3, $4)",
    )
    .bind(id)
    .bind(asset)
    .bind(state)
    .bind(job)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// SEC-AUD-092: a retry that collides with a newer live cache row still queues the job (the old cache entry stays
/// failed); any other error is reported instead of `Ok(true)`.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn retry_job_does_not_report_success_for_a_failed_cache_update() {
    let w = world(probe_ok()).await;
    let a = upload(&w, account(&w.pool).await, false).await;
    let asset = a.id;
    let old = remux_job_row(&w.pool, asset, "failed").await;
    let newer = remux_job_row(&w.pool, asset, "running").await;
    let old_cache = cache_row(&w.pool, asset, old, "failed").await;
    let _live = cache_row(&w.pool, asset, newer, "preparing").await;
    assert!(w.repo.retry_job(old).await.unwrap());
    let state: String = sqlx::query_scalar("SELECT state FROM transcode_jobs WHERE id = $1")
        .bind(old)
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(state, "queued");
    let cache: String = sqlx::query_scalar("SELECT state FROM cinema_download_cache WHERE id = $1")
        .bind(old_cache)
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(cache, "failed");

    // A forced error: the retry fails and the job stays failed.
    let failed = remux_job_row(&w.pool, asset, "failed").await;
    sqlx::query("DELETE FROM cinema_download_cache WHERE id <> $1 AND asset_id = $2")
        .bind(0_i64)
        .bind(asset)
        .execute(&w.pool)
        .await
        .unwrap();
    let _c = cache_row(&w.pool, asset, failed, "failed").await;
    sqlx::query(
        "CREATE OR REPLACE FUNCTION p59_refuse_cache_update() RETURNS trigger AS \
         $$ BEGIN RAISE EXCEPTION 'forced'; END $$ LANGUAGE plpgsql",
    )
    .execute(&w.pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER p59_refuse_cache_update BEFORE UPDATE ON cinema_download_cache \
         FOR EACH ROW EXECUTE FUNCTION p59_refuse_cache_update()",
    )
    .execute(&w.pool)
    .await
    .unwrap();
    let r = w.repo.retry_job(failed).await;
    sqlx::query("DROP TRIGGER p59_refuse_cache_update ON cinema_download_cache")
        .execute(&w.pool)
        .await
        .unwrap();
    assert!(r.is_err(), "{r:?}");
    let state: String = sqlx::query_scalar("SELECT state FROM transcode_jobs WHERE id = $1")
        .bind(failed)
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(state, "failed");
}

// ---- subtitles (D-685, SEC-AUD-200) --------------------------------------------------------

const PROBE_TWO_SUBS: &str = r#"{"streams":[
  {"index":0,"codec_name":"h264","codec_type":"video","width":1920,"height":1080,
   "pix_fmt":"yuv420p","field_order":"progressive","r_frame_rate":"24000/1001",
   "avg_frame_rate":"24000/1001","disposition":{"default":1}},
  {"index":1,"codec_name":"ac3","codec_type":"audio","channels":6,"sample_rate":"48000",
   "disposition":{"default":1},"tags":{"language":"eng"}},
  {"index":2,"codec_name":"subrip","codec_type":"subtitle","tags":{"language":"spa"}},
  {"index":3,"codec_name":"subrip","codec_type":"subtitle","tags":{"language":"fra"}}
],"format":{"format_name":"matroska,webm","duration":"1200.0"}}"#;

const SRT_FILE: &str = "1\n00:00:01,000 --> 00:00:02,000\nHello <i>there</i>\n";
const VTT_FILE: &str = "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nHola\n";
const ASS_FILE: &str = "[Script Info]\nPlayResX: 1920\nPlayResY: 1080\n\n[V4+ Styles]\n\
    Format: Name, Fontname, Fontsize, PrimaryColour, Bold, Italic, Underline, Alignment\n\
    Style: Default,Arial,48,&H00FFFFFF,0,0,0,2\n\n[Events]\n\
    Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n\
    Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,Styled\n";

struct Published {
    w: World,
    asset: i64,
    worker: WorkerService,
    secret: String,
}

/// An asset through upload, probe and a finished ladder: the embedded tracks of the probe exist.
async fn published(probe_json: &str) -> Published {
    let json = probe_json.to_owned();
    let w = world(FakeRunner::new(move |_| FakeResult::ok(json.clone()))).await;
    let user = account(&w.pool).await;
    let asset = upload(&w, user, true).await;
    w.svc.run_probe(asset.id, false).await.unwrap();
    let admin = account(&w.pool).await;
    let token = w.svc.create_token(admin, "gpu").await.unwrap();
    let worker = worker_service(
        w.repo.clone(),
        Arc::new(std::sync::atomic::AtomicU64::new(1000)),
    );
    finish_ladder(&worker, &token.secret, 1000).await;
    Published {
        asset: asset.id,
        secret: token.secret,
        w,
        worker,
    }
}

impl Published {
    /// Cancel the review proxy, queue a re-transcode and let the worker finish it.
    async fn retranscode(&self) {
        let st = self.w.svc.status(self.asset).await.unwrap();
        for j in st.jobs.iter().filter(|j| j.kind == JobKind::Proxy) {
            let _ = self.w.svc.cancel_job(j.job_id.parse().unwrap()).await;
        }
        let out = self
            .w
            .svc
            .retranscode(sr_media::retranscode::RetranscodeRequest {
                scope: sr_media::retranscode::RetranscodeScope::Asset(self.asset as u64),
                only_older_than_profile: None,
                reason: "test".into(),
            })
            .await
            .unwrap();
        assert_eq!(out.enqueued.len(), 1, "{out:?}");
        finish_ladder(&self.worker, &self.secret, 1000).await;
    }

    async fn upload_sub(&self, label: &str, language: &str, file: &str) -> SubtitleTrack {
        self.w
            .svc
            .upload_subtitle(
                self.asset,
                SubtitleUploadMeta {
                    label: Some(label.into()),
                    language: Some(language.into()),
                    ..SubtitleUploadMeta::default()
                },
                file.as_bytes(),
            )
            .await
            .unwrap()
    }

    async fn tracks(&self) -> Vec<StoredSubtitle> {
        self.w.repo.subtitle_tracks(self.asset).await.unwrap()
    }

    async fn playback(&self) -> AssetPlayback {
        self.w.repo.playback(self.asset).await.unwrap().unwrap()
    }
}

fn upload_key(asset: i64, track: i64, ext: &str) -> String {
    format!("subtitles/{asset}/{track}.{ext}")
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn an_uploaded_subtitle_survives_a_retranscode_and_still_plays() {
    let p = published(PROBE_JSON).await;
    let up = p.upload_sub("Deutsch", "de", SRT_FILE).await;
    assert_eq!(up.source, SubtitleSource::Uploaded);
    let before = p.playback().await;
    assert_eq!(
        before.media.subtitles.len(),
        2,
        "one embedded, one uploaded"
    );
    let old_prefix = before.base_path.clone();
    let uploaded = before
        .media
        .subtitles
        .iter()
        .find(|s| s.name == "Deutsch")
        .expect("the uploaded track plays");
    assert_eq!(uploaded.vtt_path, format!("u/{}.vtt", up.id));
    assert_eq!(uploaded.language_tag.as_deref(), Some("de"));
    assert!(
        before
            .subtitle_ids
            .contains(&(uploaded.vtt_path.clone(), up.id))
    );
    // It is stored as WebVTT, outside the transcode output prefix.
    let key = upload_key(p.asset, up.id, "vtt");
    let body = p.w.storage.get(&key).await.unwrap();
    assert!(String::from_utf8_lossy(&body).starts_with("WEBVTT\n\n"));

    p.retranscode().await;

    let after = p.playback().await;
    assert_ne!(
        after.base_path, old_prefix,
        "the ladder moved to a new prefix"
    );
    assert_eq!(
        after.media.subtitles.len(),
        2,
        "the uploaded track is still there"
    );
    let again = after
        .media
        .subtitles
        .iter()
        .find(|s| s.name == "Deutsch")
        .expect("still plays after the re-transcode");
    assert_eq!(again.vtt_path, uploaded.vtt_path, "same path");
    assert!(
        after
            .subtitle_ids
            .contains(&(again.vtt_path.clone(), up.id)),
        "the same track id"
    );
    assert!(
        p.w.storage.get(&key).await.is_ok(),
        "the object was not touched"
    );
    // Ordinals stay unique: the embedded track first, the uploaded one after it.
    let ordinals: Vec<i32> = sqlx::query_scalar(
        "SELECT ordinal FROM video_subtitle_tracks WHERE asset_id = $1 ORDER BY ordinal",
    )
    .bind(p.asset)
    .fetch_all(&p.w.pool)
    .await
    .unwrap();
    assert_eq!(ordinals, vec![0, 1]);
    let order: Vec<SubtitleSource> = p.tracks().await.iter().map(|t| t.track.source).collect();
    assert_eq!(
        order,
        vec![SubtitleSource::Embedded, SubtitleSource::Uploaded]
    );
    // The collector only ever has the replaced ladder prefix, never the subtitle prefix.
    let queued: Vec<String> = sqlx::query_scalar("SELECT prefix FROM cinema_media_gc")
        .fetch_all(&p.w.pool)
        .await
        .unwrap();
    assert!(
        !queued.is_empty() && queued.iter().all(|q| q.starts_with("transcode/")),
        "{queued:?}"
    );
    // The playlist of the uploaded track still renders.
    let signer = CdnSigner::new(b"s".to_vec());
    assert!(
        sr_media::grant::render_file(&after, &format!("u/{}.m3u8", up.id), &signer, 1000).is_some()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_hidden_track_is_in_neither_the_playback_answer_nor_the_playlist() {
    let p = published(PROBE_JSON).await;
    let up = p.upload_sub("Deutsch", "de", VTT_FILE).await;
    let embedded = p
        .tracks()
        .await
        .into_iter()
        .find(|t| t.track.source == SubtitleSource::Embedded)
        .unwrap();
    let signer = CdnSigner::new(b"s".to_vec());
    let master = |pb: &AssetPlayback| {
        sr_media::grant::render_file(pb, "master.m3u8", &signer, 1000)
            .unwrap()
            .body
    };
    let pb = p.playback().await;
    assert_eq!(pb.media.subtitles.len(), 2);
    assert!(master(&pb).contains(&embedded.vtt_path.replace(".vtt", ".m3u8")));
    // Hide the embedded track, then the uploaded one.
    let hidden =
        p.w.svc
            .update_subtitle(
                p.asset,
                embedded.track.id,
                SubtitlePatch {
                    hidden: Some(true),
                    ..SubtitlePatch::default()
                },
            )
            .await
            .unwrap();
    assert!(hidden.hidden);
    let pb = p.playback().await;
    assert_eq!(pb.media.subtitles.len(), 1);
    assert!(!master(&pb).contains(&embedded.vtt_path.replace(".vtt", ".m3u8")));
    assert!(
        sr_media::grant::render_file(
            &pb,
            &embedded.vtt_path.replace(".vtt", ".m3u8"),
            &signer,
            1000
        )
        .is_none()
    );
    assert!(
        sr_media::grant::playback_grant(&pb, &signer, 1000)
            .subtitles
            .iter()
            .all(|s| !s.vtt_url.contains(&embedded.vtt_path))
    );
    p.w.svc
        .update_subtitle(
            p.asset,
            up.id,
            SubtitlePatch {
                hidden: Some(true),
                ..SubtitlePatch::default()
            },
        )
        .await
        .unwrap();
    let pb = p.playback().await;
    assert!(pb.media.subtitles.is_empty() && pb.subtitle_ids.is_empty());
    assert!(!master(&pb).contains("SUBTITLES"));
    // Hidden tracks stay in the staff list, and showing them again brings them back.
    let all = p.tracks().await;
    assert_eq!(all.len(), 2);
    assert!(all.iter().all(|t| t.track.hidden));
    for t in &all {
        p.w.svc
            .update_subtitle(
                p.asset,
                t.track.id,
                SubtitlePatch {
                    hidden: Some(false),
                    ..SubtitlePatch::default()
                },
            )
            .await
            .unwrap();
    }
    assert_eq!(p.playback().await.media.subtitles.len(), 2);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn edits_of_an_embedded_track_survive_a_retranscode() {
    let p = published(PROBE_TWO_SUBS).await;
    let tracks = p.tracks().await;
    assert_eq!(tracks.len(), 2);
    let (spa, fra) = (&tracks[0], &tracks[1]);
    let fra_label = fra.track.label.clone();
    // Staff edit the first track (label, language, flags, default) and hide it; the second is left alone.
    p.w.svc
        .update_subtitle(
            p.asset,
            spa.track.id,
            SubtitlePatch {
                label: Some("Castellano".into()),
                language: Some("es-ES".into()),
                forced: Some(true),
                hearing_impaired: Some(true),
                is_default: Some(true),
                hidden: Some(true),
            },
        )
        .await
        .unwrap();
    // Hiding alone (no other edit) is kept too.
    p.w.svc
        .update_subtitle(
            p.asset,
            fra.track.id,
            SubtitlePatch {
                hidden: Some(true),
                ..SubtitlePatch::default()
            },
        )
        .await
        .unwrap();

    p.retranscode().await;

    let after = p.tracks().await;
    assert_eq!(after.len(), 2);
    let spa2 = after.iter().find(|t| t.vtt_path == spa.vtt_path).unwrap();
    let fra2 = after.iter().find(|t| t.vtt_path == fra.vtt_path).unwrap();
    assert_ne!(
        spa2.track.id, spa.track.id,
        "the row is new, the stream is the same"
    );
    assert_eq!(spa2.track.label, "Castellano");
    assert_eq!(spa2.track.language.as_deref(), Some("es-ES"));
    assert!(
        spa2.track.forced
            && spa2.track.hearing_impaired
            && spa2.track.is_default
            && spa2.track.hidden
    );
    // The track that was only hidden keeps the plan's own label and flags, and stays hidden.
    assert_eq!(fra2.track.label, fra_label);
    assert!(fra2.track.hidden);
    assert!(p.playback().await.media.subtitles.is_empty());
    // A third round keeps them (the edit flag is carried with the row).
    p.retranscode().await;
    let third = p.tracks().await;
    assert_eq!(
        third
            .iter()
            .find(|t| t.vtt_path == spa.vtt_path)
            .unwrap()
            .track
            .label,
        "Castellano"
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn setting_a_default_clears_the_other_defaults() {
    let p = published(PROBE_TWO_SUBS).await;
    let a = p.upload_sub("A", "de", SRT_FILE).await;
    let b = p.upload_sub("B", "it", VTT_FILE).await;
    let defaults = |tracks: &[StoredSubtitle]| -> Vec<i64> {
        tracks
            .iter()
            .filter(|t| t.track.is_default)
            .map(|t| t.track.id)
            .collect()
    };
    let set = |id: i64| {
        p.w.svc.update_subtitle(
            p.asset,
            id,
            SubtitlePatch {
                is_default: Some(true),
                ..SubtitlePatch::default()
            },
        )
    };
    set(a.id).await.unwrap();
    assert_eq!(defaults(&p.tracks().await), vec![a.id]);
    set(b.id).await.unwrap();
    assert_eq!(defaults(&p.tracks().await), vec![b.id]);
    // An embedded track too, and the uploaded one that was default is cleared.
    let embedded = p.tracks().await[0].track.id;
    set(embedded).await.unwrap();
    assert_eq!(defaults(&p.tracks().await), vec![embedded]);
    // After a re-transcode there is still exactly one default (the staff's choice wins over the plan's).
    set(a.id).await.unwrap();
    p.retranscode().await;
    assert_eq!(defaults(&p.tracks().await), vec![a.id]);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn an_embedded_track_cannot_be_deleted_only_hidden() {
    let p = published(PROBE_JSON).await;
    let embedded = p.tracks().await.remove(0);
    let r = p.w.svc.delete_subtitle(p.asset, embedded.track.id).await;
    match r {
        Err(MediaError::Invalid(m)) => assert!(m.contains("hide it instead"), "{m}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(p.tracks().await.len(), 1, "the track is still there");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn deleting_an_uploaded_track_removes_its_objects() {
    let p = published(PROBE_JSON).await;
    let up = p.upload_sub("Styled", "en", ASS_FILE).await;
    assert!(up.styled);
    let (vtt, ass) = (
        upload_key(p.asset, up.id, "vtt"),
        upload_key(p.asset, up.id, "ass"),
    );
    assert!(p.w.storage.get(&vtt).await.is_ok() && p.w.storage.get(&ass).await.is_ok());
    p.w.svc.delete_subtitle(p.asset, up.id).await.unwrap();
    assert!(p.w.storage.get(&vtt).await.is_err(), "the WebVTT is gone");
    assert!(p.w.storage.get(&ass).await.is_err(), "the ASS is gone");
    assert_eq!(p.tracks().await.len(), 1);
    assert!(matches!(
        p.w.svc.delete_subtitle(p.asset, up.id).await,
        Err(MediaError::NotFound(_))
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn deleting_the_asset_removes_uploaded_subtitles() {
    let w = world(probe_ok()).await;
    let user = account(&w.pool).await;
    let a = upload(&w, user, true).await;
    let up = w
        .svc
        .upload_subtitle(
            a.id,
            SubtitleUploadMeta {
                language: Some("de".into()),
                ..SubtitleUploadMeta::default()
            },
            SRT_FILE.as_bytes(),
        )
        .await
        .unwrap();
    assert_eq!(up.label, "de", "the language names an unlabelled track");
    let key = upload_key(a.id, up.id, "vtt");
    assert!(w.storage.get(&key).await.is_ok());
    // The asset is removed and its grace period is over: the sweep deletes the original and the uploads.
    w.repo
        .transition_asset(a.id, &[AssetState::Uploaded], AssetState::Removed, None)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE video_assets SET ended_at = to_timestamp(1700000000 - 5184000) WHERE id = $1",
    )
    .bind(a.id)
    .execute(&w.pool)
    .await
    .unwrap();
    // A removed asset takes no new subtitles.
    let r = w
        .svc
        .upload_subtitle(a.id, SubtitleUploadMeta::default(), SRT_FILE.as_bytes())
        .await;
    assert!(matches!(r, Err(MediaError::Conflict(_))), "{r:?}");
    let report = w.svc.sweep().await.unwrap();
    assert_eq!(report.originals_deleted, 1, "{report:?}");
    assert!(
        w.storage.get(&key).await.is_err(),
        "the uploaded file is gone"
    );
    let rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM video_subtitle_tracks WHERE asset_id = $1")
            .bind(a.id)
            .fetch_one(&w.pool)
            .await
            .unwrap();
    assert_eq!(rows, 0);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_file_with_no_cue_is_refused() {
    let p = published(PROBE_JSON).await;
    for bad in [
        &b"nothing but words"[..],
        &b"WEBVTT\n\nNOTE only a note\n"[..],
        &b"1\n00:00:03,000 --> 00:00:03,000\nzero length\n"[..],
        &b""[..],
    ] {
        let r =
            p.w.svc
                .upload_subtitle(p.asset, SubtitleUploadMeta::default(), bad)
                .await;
        match r {
            Err(MediaError::Invalid(m)) => assert!(m.contains("no readable cues"), "{m}"),
            other => panic!("{bad:?}: {other:?}"),
        }
    }
    assert!(
        p.w.storage
            .list(&format!("subtitles/{}/", p.asset))
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(p.tracks().await.len(), 1, "only the embedded track");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_file_over_the_limit_is_refused() {
    let p = published(PROBE_JSON).await;
    let big = vec![b'a'; sr_media::subtitles::MAX_SUBTITLE_BYTES + 1];
    let r =
        p.w.svc
            .upload_subtitle(p.asset, SubtitleUploadMeta::default(), &big)
            .await;
    match r {
        Err(MediaError::Invalid(m)) => assert!(m.contains("larger than"), "{m}"),
        other => panic!("{other:?}"),
    }
    assert!(
        p.w.storage
            .list(&format!("subtitles/{}/", p.asset))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_format_is_read_from_the_content_not_the_name() {
    // The service takes bytes only (a name is never an input), so the same bytes give the same track
    // whatever the browser called the file: SubRip, WebVTT and ASS each come out as WebVTT.
    let p = published(PROBE_JSON).await;
    for (file, styled, needle) in [
        (SRT_FILE, false, "Hello <i>there</i>"),
        (VTT_FILE, false, "Hola"),
        (ASS_FILE, true, "Styled"),
    ] {
        let t = p.upload_sub("X", "en", file).await;
        assert_eq!(t.styled, styled, "{file}");
        let vtt =
            p.w.storage
                .get(&upload_key(p.asset, t.id, "vtt"))
                .await
                .unwrap();
        let vtt = String::from_utf8(vtt.to_vec()).unwrap();
        assert!(
            vtt.starts_with("WEBVTT\n\n") && vtt.contains(needle),
            "{vtt}"
        );
        assert_eq!(
            p.w.storage
                .get(&upload_key(p.asset, t.id, "ass"))
                .await
                .is_ok(),
            styled
        );
    }
    // A file that only claims to be WebVTT is parsed, not stored as sent.
    let hostile = "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\n<script>alert(1)</script>\n\n\
                   STYLE\n::cue { background: url(javascript:x) }\n";
    let t = p.upload_sub("Y", "en", hostile).await;
    let vtt =
        p.w.storage
            .get(&upload_key(p.asset, t.id, "vtt"))
            .await
            .unwrap();
    let vtt = String::from_utf8(vtt.to_vec()).unwrap();
    assert!(
        !vtt.contains("<script") && !vtt.contains("STYLE") && !vtt.contains("javascript"),
        "{vtt}"
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_track_of_another_asset_is_404() {
    let p = published(PROBE_JSON).await;
    let up = p.upload_sub("Mine", "de", SRT_FILE).await;
    let user = account(&p.w.pool).await;
    let other = upload(&p.w, user, false).await;
    assert!(matches!(
        p.w.svc
            .update_subtitle(
                other.id,
                up.id,
                SubtitlePatch {
                    label: Some("x".into()),
                    ..SubtitlePatch::default()
                }
            )
            .await,
        Err(MediaError::NotFound(_))
    ));
    assert!(matches!(
        p.w.svc.delete_subtitle(other.id, up.id).await,
        Err(MediaError::NotFound(_))
    ));
    // The track is untouched, and a missing asset is 404 for a list and an upload.
    let mine = p.tracks().await;
    assert!(
        mine.iter()
            .any(|t| t.track.id == up.id && t.track.label == "Mine")
    );
    assert!(matches!(
        p.w.svc.subtitle_tracks(1).await,
        Err(MediaError::NotFound("asset"))
    ));
    assert!(matches!(
        p.w.svc
            .upload_subtitle(1, SubtitleUploadMeta::default(), SRT_FILE.as_bytes())
            .await,
        Err(MediaError::NotFound("asset"))
    ));
}

// ---- subtitle languages join the title (D-694, SEC-AUD-214) ---------------------------------

/// An asset through upload and probe with a worker, the ladder not finished yet.
async fn staged(probe_json: &str) -> Published {
    let json = probe_json.to_owned();
    let w = world(FakeRunner::new(move |_| FakeResult::ok(json.clone()))).await;
    let user = account(&w.pool).await;
    let asset = upload(&w, user, true).await;
    w.svc.run_probe(asset.id, false).await.unwrap();
    let admin = account(&w.pool).await;
    let token = w.svc.create_token(admin, "gpu").await.unwrap();
    let worker = worker_service(
        w.repo.clone(),
        Arc::new(std::sync::atomic::AtomicU64::new(1000)),
    );
    Published {
        asset: asset.id,
        secret: token.secret,
        w,
        worker,
    }
}

impl Published {
    /// A published title that plays this asset.
    async fn film(&self, languages: &[&str]) -> i64 {
        let t = next_id();
        sqlx::query(
            "INSERT INTO video_titles (id, kind, title, sort_title, published, published_at, asset_id, subtitle_languages) \
             VALUES ($1, 'movie', 'Film', 'film', TRUE, now(), $2, $3)",
        )
        .bind(t)
        .bind(self.asset)
        .bind(languages.iter().map(|l| (*l).to_owned()).collect::<Vec<_>>())
        .execute(&self.w.pool)
        .await
        .unwrap();
        t
    }

    /// A show whose one episode plays this asset; returns the show.
    async fn show(&self) -> i64 {
        let (t, season, ep) = (next_id(), next_id(), next_id());
        sqlx::query(
            "INSERT INTO video_titles (id, kind, title, sort_title, published, published_at) \
             VALUES ($1, 'series', 'Show', 'show', TRUE, now())",
        )
        .bind(t)
        .execute(&self.w.pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO video_seasons (id, title_id, number) VALUES ($1, $2, 1)")
            .bind(season)
            .bind(t)
            .execute(&self.w.pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO video_episodes (id, title_id, season_id, number, asset_id) VALUES ($1, $2, $3, 1, $4)",
        )
        .bind(ep)
        .bind(t)
        .bind(season)
        .bind(self.asset)
        .execute(&self.w.pool)
        .await
        .unwrap();
        t
    }

    async fn languages_of(&self, title: i64) -> Vec<String> {
        let mut v: Vec<String> =
            sqlx::query_scalar("SELECT subtitle_languages FROM video_titles WHERE id = $1")
                .bind(title)
                .fetch_one(&self.w.pool)
                .await
                .unwrap();
        v.sort();
        v
    }

    /// What the route does after a change: offer the track's language to its title.
    async fn offer(&self, track: &SubtitleTrack) -> Option<String> {
        self.w
            .svc
            .add_track_language_to_title(self.asset, track)
            .await
    }

    async fn patch(&self, track: i64, patch: SubtitlePatch) -> SubtitleTrack {
        self.w
            .svc
            .update_subtitle(self.asset, track, patch)
            .await
            .unwrap()
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn an_uploaded_track_adds_its_language_to_the_title() {
    let p = published(PROBE_JSON).await;
    let title = p.film(&["en"]).await;
    let up = p.upload_sub("Portugues", "pt-br", SRT_FILE).await;
    assert_eq!(p.offer(&up).await.as_deref(), Some("pt-BR"));
    assert_eq!(p.languages_of(title).await, ["en", "pt-BR"]);
    // Offered again: already there, nothing changes, nothing to report.
    assert_eq!(p.offer(&up).await, None);
    assert_eq!(p.languages_of(title).await, ["en", "pt-BR"]);
    // An asset no title plays adds nothing and is no error.
    let lone = published(PROBE_JSON).await;
    let t = lone.upload_sub("Deutsch", "de", SRT_FILE).await;
    assert_eq!(lone.offer(&t).await, None);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn an_episodes_track_adds_to_the_show() {
    let p = published(PROBE_JSON).await;
    let show = p.show().await;
    let up = p.upload_sub("Deutsch", "de", SRT_FILE).await;
    assert_eq!(p.offer(&up).await.as_deref(), Some("de"));
    assert_eq!(p.languages_of(show).await, ["de"]);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn showing_a_hidden_track_adds_its_language() {
    let p = published(PROBE_JSON).await;
    let title = p.film(&[]).await;
    let up = p.upload_sub("Deutsch", "de", SRT_FILE).await;
    let hidden = p
        .patch(
            up.id,
            SubtitlePatch {
                hidden: Some(true),
                ..SubtitlePatch::default()
            },
        )
        .await;
    assert_eq!(p.offer(&hidden).await, None, "a hidden track adds nothing");
    assert!(p.languages_of(title).await.is_empty());
    let shown = p
        .patch(
            up.id,
            SubtitlePatch {
                hidden: Some(false),
                ..SubtitlePatch::default()
            },
        )
        .await;
    assert_eq!(p.offer(&shown).await.as_deref(), Some("de"));
    assert_eq!(p.languages_of(title).await, ["de"]);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_language_edit_adds_the_new_one_and_keeps_the_old() {
    let p = published(PROBE_JSON).await;
    let title = p.film(&[]).await;
    let up = p.upload_sub("Track", "de", SRT_FILE).await;
    assert_eq!(p.offer(&up).await.as_deref(), Some("de"));
    let edited = p
        .patch(
            up.id,
            SubtitlePatch {
                language: Some("it".into()),
                ..SubtitlePatch::default()
            },
        )
        .await;
    assert_eq!(p.offer(&edited).await.as_deref(), Some("it"));
    assert_eq!(
        p.languages_of(title).await,
        ["de", "it"],
        "the old one stays"
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_finished_transcode_adds_the_embedded_languages() {
    let p = staged(PROBE_TWO_SUBS).await;
    let title = p.film(&["en"]).await;
    finish_ladder(&p.worker, &p.secret, 1000).await;
    assert_eq!(p.languages_of(title).await, ["en", "es", "fr"]);

    // A track staff hid stays out on the next transcode: empty the list, hide fr, encode again.
    sqlx::query("UPDATE video_titles SET subtitle_languages = '{}' WHERE id = $1")
        .bind(title)
        .execute(&p.w.pool)
        .await
        .unwrap();
    let fr = p
        .tracks()
        .await
        .into_iter()
        .find(|t| t.track.language.as_deref() == Some("fr"))
        .expect("the French track")
        .track;
    p.patch(
        fr.id,
        SubtitlePatch {
            hidden: Some(true),
            ..SubtitlePatch::default()
        },
    )
    .await;
    p.retranscode().await;
    assert_eq!(
        p.languages_of(title).await,
        ["es"],
        "the hidden track adds nothing"
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn hiding_or_deleting_a_track_removes_nothing() {
    let p = published(PROBE_JSON).await;
    let title = p.film(&[]).await;
    let up = p.upload_sub("Deutsch", "de", SRT_FILE).await;
    assert_eq!(p.offer(&up).await.as_deref(), Some("de"));
    p.patch(
        up.id,
        SubtitlePatch {
            hidden: Some(true),
            ..SubtitlePatch::default()
        },
    )
    .await;
    assert_eq!(p.languages_of(title).await, ["de"]);
    p.w.svc.delete_subtitle(p.asset, up.id).await.unwrap();
    assert_eq!(p.languages_of(title).await, ["de"]);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn und_adds_nothing() {
    let p = published(PROBE_JSON).await;
    let title = p.film(&[]).await;
    let und = p.upload_sub("Unknown", "und", SRT_FILE).await;
    assert_eq!(p.offer(&und).await, None);
    let none =
        p.w.svc
            .upload_subtitle(p.asset, SubtitleUploadMeta::default(), SRT_FILE.as_bytes())
            .await
            .unwrap();
    assert_eq!(none.language, None);
    assert_eq!(p.offer(&none).await, None);
    // A tag the title editor would refuse adds nothing.
    let odd = SubtitleTrack {
        language: Some("x-klingon".into()),
        ..none
    };
    assert_eq!(p.offer(&odd).await, None);
    assert!(p.languages_of(title).await.is_empty());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_catalog_filter_finds_the_title_afterwards() {
    use sr_cinema::model::{BrowseQuery, Scope};
    use sr_cinema::repo::CinemaRepository;
    let p = published(PROBE_JSON).await;
    let title = p.film(&["en"]).await;
    let cinema = sr_db::cinema_repo::PgCinemaRepo::new(p.w.pool.clone());
    let q = BrowseQuery {
        subtitle_language: Some("de".into()),
        limit: 10,
        ..BrowseQuery::default()
    };
    assert_eq!(cinema.browse(&q, Scope::Public).await.unwrap().total, 0);
    let up = p.upload_sub("Deutsch", "de", SRT_FILE).await;
    assert_eq!(p.offer(&up).await.as_deref(), Some("de"));
    let found = cinema.browse(&q, Scope::Public).await.unwrap();
    assert_eq!(
        found.items.iter().map(|t| t.id).collect::<Vec<_>>(),
        [title]
    );
    let facets = cinema.facets(None, true).await.unwrap();
    assert!(facets.subtitle_languages.iter().any(|f| f.name == "de"));
}

// ---- workers show their throughput (D-705, SEC-AUD-216) -------------------------------------

struct Fleet {
    w: World,
    asset: i64,
    ids: Vec<String>,
}

/// `n` registered workers (named `w0`, `w1`, ...) on one token, and an asset to hang jobs on.
async fn fleet(n: usize) -> Fleet {
    let w = world(probe_ok()).await;
    let user = account(&w.pool).await;
    let asset = upload(&w, user, false).await.id;
    let admin = account(&w.pool).await;
    let token = w.svc.create_token(admin, "fleet").await.unwrap();
    let svc = worker_service(
        w.repo.clone(),
        Arc::new(std::sync::atomic::AtomicU64::new(1000)),
    );
    let c = svc.authenticate(Some(&token.secret)).await.unwrap();
    let mut ids = Vec::new();
    for i in 0..n {
        let caps = WorkerCapabilities {
            gpu: Some(format!("GPU {i}")),
            ..WorkerCapabilities::default()
        };
        ids.push(
            svc.register(
                &c,
                RegisterRequest {
                    name: format!("w{i}"),
                    version: "1".into(),
                    capabilities: caps,
                },
            )
            .await
            .unwrap()
            .worker_id,
        );
    }
    Fleet { w, asset, ids }
}

impl Fleet {
    /// A job of `worker`. `lease_secs` is the lease's end relative to now, `finished_hours_ago` the finish time.
    async fn job(
        &self,
        worker: usize,
        state: &str,
        speed: Option<f64>,
        lease_secs: i64,
        finished_hours_ago: Option<i64>,
    ) {
        sqlx::query(
            "INSERT INTO transcode_jobs (id, asset_id, kind, spec, source_key, source_size, source_ext, state, \
               worker_id, lease_expires_at, progress, finished_at) \
             VALUES ($1, $2, 'remux', '{}'::jsonb, 'k', 1, 'mkv', $3, $4, \
               now() + make_interval(secs => $5), $6, now() - make_interval(hours => $7))",
        )
        .bind(next_id())
        .bind(self.asset)
        .bind(state)
        .bind(&self.ids[worker])
        .bind(lease_secs as f64)
        .bind(speed.map(|s| serde_json::json!({ "stage": "encode", "percent": 10, "speed": s })))
        .bind(finished_hours_ago.map(|h| h as i32))
        .execute(&self.w.pool)
        .await
        .unwrap();
    }

    async fn view(&self, worker: usize) -> WorkerView {
        self.w
            .repo
            .list_workers()
            .await
            .unwrap()
            .into_iter()
            .find(|v| v.id == self.ids[worker])
            .expect("the worker is listed")
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn speed_now_sums_running_jobs_with_a_live_lease() {
    let f = fleet(2).await;
    assert_eq!(f.view(0).await.speed_now, None, "it runs nothing");
    f.job(0, "running", Some(2.5), 60, None).await;
    f.job(0, "running", Some(1.0), 60, None).await;
    // A running job that has not reported a speed yet adds nothing; a queued or finished one is not running.
    f.job(0, "running", None, 60, None).await;
    f.job(0, "succeeded", Some(9.0), 60, Some(1)).await;
    let a = f.view(0).await;
    assert_eq!(a.speed_now, Some(3.5));
    assert_eq!(a.active_jobs, 3);
    // The other worker's jobs are not summed into this one, and a running job without a speed is no speed.
    f.job(1, "running", None, 60, None).await;
    assert_eq!(f.view(1).await.speed_now, None);
    // The GPU name rides on the capabilities.
    assert_eq!(a.capabilities.gpu.as_deref(), Some("GPU 0"));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn an_expired_lease_does_not_count() {
    let f = fleet(1).await;
    f.job(0, "running", Some(4.0), -30, None).await;
    assert_eq!(f.view(0).await.speed_now, None, "a dead lease is no speed");
    f.job(0, "running", Some(1.5), 30, None).await;
    assert_eq!(f.view(0).await.speed_now, Some(1.5));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_day_counts_are_per_worker() {
    let f = fleet(2).await;
    for h in [1, 5, 23] {
        f.job(0, "succeeded", None, 0, Some(h)).await;
    }
    f.job(0, "failed", None, 0, Some(2)).await;
    // Older than a day, cancelled and queued jobs are not counted.
    f.job(0, "succeeded", None, 0, Some(25)).await;
    f.job(0, "failed", None, 0, Some(48)).await;
    f.job(0, "cancelled", None, 0, Some(1)).await;
    f.job(1, "succeeded", None, 0, Some(3)).await;
    f.job(1, "failed", None, 0, Some(4)).await;
    f.job(1, "failed", None, 0, Some(6)).await;
    let a = f.view(0).await;
    assert_eq!((a.jobs_done_24h, a.jobs_failed_24h), (3, 1));
    let b = f.view(1).await;
    assert_eq!((b.jobs_done_24h, b.jobs_failed_24h), (1, 2));
}
