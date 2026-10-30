//! Blocking HTTP implementations of [`WorkerApi`] and [`Transfer`].
//!
//! The bearer token is sent only to the SIDE ROOM server (never to storage URLs), ids are
//! checked before they go into a URL path, and downloads stop at the size the server
//! announced.

use crate::worker::{ApiClientError, Transfer, TransferError, WorkerApi};
use reqwest::blocking::Client;
use reqwest::redirect::Policy;
use serde::Serialize;
use serde::de::DeserializeOwned;
use sr_media::worker_api::{
    API_PREFIX, Ack, ClaimRequest, ClaimResponse, CompleteRequest, FailRequest,
    FinishUploadRequest, HeartbeatRequest, HeartbeatResponse, InitUploadRequest,
    InitUploadResponse, PartDone, ProgressRequest, ProgressResponse, RegisterRequest,
    RegisterResponse, SourceRef,
};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Duration;

fn transport(e: impl std::fmt::Display) -> ApiClientError {
    ApiClientError::Transport(e.to_string())
}

fn safe_id(id: &str) -> Result<&str, ApiClientError> {
    let ok = !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok {
        Ok(id)
    } else {
        Err(transport("server sent an unsafe id"))
    }
}

pub struct HttpWorkerApi {
    base: String,
    token: String,
    client: Client,
}

impl HttpWorkerApi {
    pub fn new(base: &str, token: &str) -> Result<Self, ApiClientError> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .redirect(Policy::none())
            .build()
            .map_err(transport)?;
        Ok(Self {
            base: base.trim_end_matches('/').to_string(),
            token: token.to_string(),
            client,
        })
    }

    fn post<B: Serialize, R: DeserializeOwned>(
        &self,
        tail: &str,
        body: &B,
    ) -> Result<R, ApiClientError> {
        let url = format!("{}{API_PREFIX}{tail}", self.base);
        let resp = self
            .client
            .post(url)
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .map_err(transport)?;
        let status = resp.status();
        if status.is_success() {
            return resp.json::<R>().map_err(transport);
        }
        #[derive(serde::Deserialize, Default)]
        struct Body {
            #[serde(default)]
            code: String,
            #[serde(default)]
            message: String,
        }
        let parsed: Body = resp.json().unwrap_or_default();
        Err(ApiClientError::Status {
            status: status.as_u16(),
            code: parsed.code.chars().take(40).collect(),
            message: parsed.message.chars().take(200).collect(),
        })
    }
}

impl WorkerApi for HttpWorkerApi {
    fn register(&self, req: &RegisterRequest) -> Result<RegisterResponse, ApiClientError> {
        self.post("/register", req)
    }
    fn heartbeat(
        &self,
        worker_id: &str,
        req: &HeartbeatRequest,
    ) -> Result<HeartbeatResponse, ApiClientError> {
        self.post(&format!("/workers/{}/heartbeat", safe_id(worker_id)?), req)
    }
    fn claim(&self, worker_id: &str, req: &ClaimRequest) -> Result<ClaimResponse, ApiClientError> {
        self.post(&format!("/workers/{}/claim", safe_id(worker_id)?), req)
    }
    fn progress(
        &self,
        job_id: &str,
        req: &ProgressRequest,
    ) -> Result<ProgressResponse, ApiClientError> {
        self.post(&format!("/jobs/{}/progress", safe_id(job_id)?), req)
    }
    fn init_upload(
        &self,
        job_id: &str,
        req: &InitUploadRequest,
    ) -> Result<InitUploadResponse, ApiClientError> {
        self.post(&format!("/jobs/{}/uploads", safe_id(job_id)?), req)
    }
    fn finish_upload(
        &self,
        job_id: &str,
        upload_id: &str,
        req: &FinishUploadRequest,
    ) -> Result<Ack, ApiClientError> {
        self.post(
            &format!(
                "/jobs/{}/uploads/{}/complete",
                safe_id(job_id)?,
                safe_id(upload_id)?
            ),
            req,
        )
    }
    fn complete(&self, job_id: &str, req: &CompleteRequest) -> Result<Ack, ApiClientError> {
        self.post(&format!("/jobs/{}/complete", safe_id(job_id)?), req)
    }
    fn fail(&self, job_id: &str, req: &FailRequest) -> Result<Ack, ApiClientError> {
        self.post(&format!("/jobs/{}/fail", safe_id(job_id)?), req)
    }
}

/// Presigned-URL transfers. Relative URLs (the server's signed `/cdn/...` paths) are resolved
/// against `base`.
pub struct HttpTransfer {
    base: String,
    client: Client,
}

impl HttpTransfer {
    pub fn new(base: &str) -> Result<Self, TransferError> {
        // Originals are huge, so the whole-transfer cap is generous (6 h).
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(6 * 3600))
            .redirect(Policy::limited(3))
            .build()
            .map_err(|e| TransferError::Download(e.to_string()))?;
        Ok(Self {
            base: base.trim_end_matches('/').to_string(),
            client,
        })
    }

    fn resolve(&self, url: &str) -> String {
        if url.starts_with('/') {
            format!("{}{url}", self.base)
        } else {
            url.to_string()
        }
    }
}

impl Transfer for HttpTransfer {
    fn download(
        &self,
        source: &SourceRef,
        dest: &Path,
        max_bytes: u64,
    ) -> Result<u64, TransferError> {
        let err = |e: &dyn std::fmt::Display| TransferError::Download(e.to_string());
        let resp = self
            .client
            .get(self.resolve(&source.download_url))
            .send()
            .map_err(|e| err(&e))?;
        if !resp.status().is_success() {
            return Err(TransferError::Download(format!("HTTP {}", resp.status())));
        }
        if resp.content_length().is_some_and(|l| l > max_bytes) {
            return Err(TransferError::Download(
                "original is larger than announced".into(),
            ));
        }
        let mut file = File::create(dest).map_err(|e| err(&e))?;
        let mut limited = resp.take(max_bytes + 1);
        let n = std::io::copy(&mut limited, &mut file).map_err(|e| err(&e))?;
        if n > max_bytes {
            return Err(TransferError::Download(
                "original is larger than announced".into(),
            ));
        }
        file.flush().map_err(|e| err(&e))?;
        Ok(n)
    }

    fn upload(
        &self,
        init: &InitUploadResponse,
        file: &Path,
    ) -> Result<Vec<PartDone>, TransferError> {
        let err = |e: &dyn std::fmt::Display| TransferError::Upload(e.to_string());
        let mut f = File::open(file).map_err(|e| err(&e))?;
        let size = f.metadata().map_err(|e| err(&e))?.len();
        let mut done = Vec::with_capacity(init.parts.len());
        for part in &init.parts {
            let offset = u64::from(part.part_number - 1) * init.part_size;
            let len = init.part_size.min(size.saturating_sub(offset));
            f.seek(SeekFrom::Start(offset)).map_err(|e| err(&e))?;
            let mut buf = vec![0u8; usize::try_from(len).map_err(|e| err(&e))?];
            f.read_exact(&mut buf).map_err(|e| err(&e))?;
            let mut last_error = String::new();
            let mut etag = None;
            for attempt in 0..3u64 {
                if attempt > 0 {
                    std::thread::sleep(Duration::from_millis(500 * attempt));
                }
                match self
                    .client
                    .put(self.resolve(&part.url))
                    .header("content-type", init.content_type.as_str())
                    .body(buf.clone())
                    .send()
                {
                    Ok(r) if r.status().is_success() => {
                        etag = Some(
                            r.headers()
                                .get("etag")
                                .and_then(|v| v.to_str().ok())
                                .unwrap_or("")
                                .trim_matches('"')
                                .to_string(),
                        );
                        break;
                    }
                    Ok(r) => last_error = format!("HTTP {}", r.status()),
                    Err(e) => last_error = e.to_string(),
                }
            }
            let etag = etag.ok_or(TransferError::Upload(last_error))?;
            done.push(PartDone {
                part_number: part.part_number,
                // Some S3-compatible stores omit the ETag; the server only requires non-empty.
                etag: if etag.is_empty() {
                    "-".to_string()
                } else {
                    etag
                },
            });
        }
        Ok(done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker::tests::{
        TempRoot, caps, cfg, ladder_job, never_fail, proxy_job, runner, sheets_of,
    };
    use crate::worker::{JobOutcome, Worker};
    use async_trait::async_trait;
    use axum::Router;
    use axum::body::Bytes;
    use axum::extract::{Path as AxPath, State};
    use axum::http::{HeaderMap, HeaderValue, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::{get, put};
    use sr_media::playlist::UrlSigner;
    use sr_media::worker_api::{
        JobState, JobStore, MemoryJobStore, NewJob, OutputStorage, ServiceConfig, TokenInfo,
        WorkerService,
    };
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Blobs {
        original: Arc<Vec<u8>>,
        puts: Arc<Mutex<BTreeMap<String, usize>>>,
    }

    async fn get_original(
        State(b): State<Blobs>,
        AxPath(_key): AxPath<String>,
    ) -> impl IntoResponse {
        b.original.as_ref().clone()
    }

    async fn put_part(
        State(b): State<Blobs>,
        AxPath(key): AxPath<String>,
        headers: HeaderMap,
        body: Bytes,
    ) -> impl IntoResponse {
        assert!(
            headers.get("authorization").is_none(),
            "token leaked to storage"
        );
        b.puts.lock().unwrap().insert(key, body.len());
        let mut h = HeaderMap::new();
        h.insert("etag", HeaderValue::from_static("\"abc123\""));
        (StatusCode::OK, h)
    }

    struct LocalStorage(String);
    #[async_trait]
    impl OutputStorage for LocalStorage {
        async fn init_multipart(&self, _k: &str, _c: &str) -> Result<String, String> {
            static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            Ok(format!(
                "mp{}",
                N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            ))
        }
        async fn presign_part(
            &self,
            key: &str,
            _u: &str,
            n: u32,
            _t: u64,
        ) -> Result<String, String> {
            Ok(format!("{}/put/{key}?part={n}", self.0))
        }
        async fn complete_multipart(
            &self,
            _k: &str,
            _u: &str,
            parts: &[PartDone],
        ) -> Result<(), String> {
            assert!(parts.iter().all(|p| p.etag == "abc123"));
            Ok(())
        }
    }

    struct PathSigner;
    impl UrlSigner for PathSigner {
        fn signed_url(&self, path: &str, exp: u64) -> String {
            format!("{path}?ex={exp}&sig=T")
        }
    }

    const TOKEN: &str = "test-worker-token";

    /// Start the real Worker API router plus fake storage on a random local port.
    fn start_server(
        original_len: usize,
    ) -> (
        String,
        Arc<MemoryJobStore>,
        Blobs,
        std::thread::JoinHandle<()>,
        tokio::sync::oneshot::Sender<()>,
    ) {
        let store = Arc::new(MemoryJobStore::new());
        store.add_token(
            TOKEN,
            TokenInfo {
                token_id: 1,
                name: "t".into(),
                scope: sr_media::worker_api::TokenScope::Instance,
            },
        );
        let blobs = Blobs {
            original: Arc::new(vec![9u8; original_len]),
            puts: Arc::default(),
        };
        let (addr_tx, addr_rx) = std::sync::mpsc::channel::<String>();
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
        let store2 = Arc::clone(&store);
        let blobs2 = blobs.clone();
        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async move {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let base = format!("http://{}", listener.local_addr().unwrap());
                let svc = Arc::new(WorkerService {
                    store: store2,
                    outputs: Arc::new(LocalStorage(base.clone())),
                    downloads: Arc::new(PathSigner),
                    clock: Arc::new(|| {
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_or(0, |d| d.as_secs())
                    }),
                    mounts: None,
                    config: ServiceConfig {
                        part_size: 64,
                        ..ServiceConfig::default()
                    },
                });
                let app = sr_media::http::router(svc).merge(
                    Router::new()
                        .route("/cdn/originals/{*key}", get(get_original))
                        .route("/put/{*key}", put(put_part))
                        .with_state(blobs2),
                );
                addr_tx.send(base).unwrap();
                axum::serve(listener, app)
                    .with_graceful_shutdown(async {
                        let _ = stop_rx.await;
                    })
                    .await
                    .unwrap();
            });
        });
        let base = addr_rx.recv().unwrap();
        (base, store, blobs, handle, stop_tx)
    }

    fn enqueue(
        store: &MemoryJobStore,
        job: &sr_media::worker_api::ClaimedJob,
        key: &str,
    ) -> String {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(store.enqueue(NewJob {
            asset_id: 1,
            spec: job.spec.clone(),
            source_key: key.into(),
            source_size: job.source.size_bytes,
            source_extension: "mkv".into(),
            priority: 5,
        }))
        .unwrap()
    }

    #[test]
    fn ladder_job_end_to_end_over_real_http() {
        let root = TempRoot::new();
        let template = ladder_job("x");
        let (base, store, blobs, handle, stop) = start_server(template.source.size_bytes as usize);
        let job_id = enqueue(&store, &template, "orig/source.mkv");

        let api = HttpWorkerApi::new(&base, TOKEN).unwrap();
        let transfer = HttpTransfer::new(&base).unwrap();
        let run = runner(sheets_of(&template), never_fail());
        let w = Worker::new(&api, &transfer, &run, cfg(&root.0), caps(false));
        let reg = api
            .register(&RegisterRequest {
                name: "e2e".into(),
                version: "0".into(),
                capabilities: w.capabilities(),
            })
            .unwrap();

        let outcome = w
            .poll_once(&reg.worker_id)
            .unwrap()
            .expect("a job was queued");
        if outcome != JobOutcome::Completed {
            let rt0 = tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap();
            let rec = rt0.block_on(store.get_job(&job_id)).unwrap().unwrap();
            panic!("{outcome:?} {:?} {:?}", rec.error, rec.log_tail);
        }

        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let rec = rt.block_on(store.get_job(&job_id)).unwrap().unwrap();
        assert_eq!(rec.state, JobState::Succeeded);
        assert_eq!(rec.encoder.as_deref(), Some("libx264"));
        assert!(!rec.segment_index.is_empty());
        // Files really crossed the wire, in 64-byte parts for the bigger ones.
        let puts = blobs.puts.lock().unwrap();
        assert!(puts.keys().any(|k| k.ends_with("v/1080p/index.m3u8")));
        assert!(
            puts.keys()
                .all(|k| k.starts_with(&format!("transcode/{job_id}/1/")))
        );
        drop(puts);
        // Nothing left to claim.
        assert!(w.poll_once(&reg.worker_id).unwrap().is_none());

        let _ = stop.send(());
        handle.join().unwrap();
    }

    #[test]
    fn proxy_job_and_bad_token_over_http() {
        let root = TempRoot::new();
        let template = proxy_job("x");
        let (base, store, _blobs, handle, stop) = start_server(template.source.size_bytes as usize);
        enqueue(&store, &template, "orig/source.mkv");

        // A wrong token is refused with 401.
        let bad = HttpWorkerApi::new(&base, "nope").unwrap();
        let err = bad
            .register(&RegisterRequest {
                name: "e2e".into(),
                version: "0".into(),
                capabilities: Default::default(),
            })
            .unwrap_err();
        assert!(
            matches!(err, ApiClientError::Status { status: 401, .. }),
            "{err:?}"
        );

        let api = HttpWorkerApi::new(&base, TOKEN).unwrap();
        let transfer = HttpTransfer::new(&base).unwrap();
        let run = runner(0, never_fail());
        let w = Worker::new(&api, &transfer, &run, cfg(&root.0), caps(true));
        let reg = api
            .register(&RegisterRequest {
                name: "e2e".into(),
                version: "0".into(),
                capabilities: w.capabilities(),
            })
            .unwrap();
        assert_eq!(
            w.poll_once(&reg.worker_id).unwrap(),
            Some(JobOutcome::Completed)
        );

        let _ = stop.send(());
        handle.join().unwrap();
    }

    #[test]
    fn unsafe_ids_never_reach_the_url() {
        let api = HttpWorkerApi::new("http://127.0.0.1:9", "t").unwrap();
        let e = api
            .claim("../../admin", &ClaimRequest::default())
            .unwrap_err();
        assert!(matches!(e, ApiClientError::Transport(m) if m.contains("unsafe id")));
    }

    #[test]
    fn oversized_download_is_cut_off() {
        let root = TempRoot::new();
        let (base, _store, _blobs, handle, stop) = start_server(1000);
        let transfer = HttpTransfer::new(&base).unwrap();
        let source = SourceRef {
            download_url: "/cdn/originals/x.mkv".into(),
            size_bytes: 500,
            expires_at: 0,
            extension: "mkv".into(),
        };
        let r = transfer.download(&source, &root.0.join("x"), 500);
        assert!(matches!(r, Err(TransferError::Download(_))), "{r:?}");
        let ok = transfer.download(&source, &root.0.join("y"), 1000).unwrap();
        assert_eq!(ok, 1000);
        let _ = stop.send(());
        handle.join().unwrap();
    }
}
