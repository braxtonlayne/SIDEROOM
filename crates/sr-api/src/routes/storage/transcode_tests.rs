//! Route tests of the conversions into a member's own bucket (BYOS-CNV-003, decision P29-CNV-1 (2)): who may
//! queue a `mount_transcode` while `byos.worker_for_members` is off. The bucket is a tiny local HTTP answer to
//! `HEAD` (the route reads the object's size and nothing else); the media service runs over the cinema-media stub.

use super::{ScopeDeps, TranscodeDeps, router};
use crate::routes::cinema_media::tests::{Stub, stub_media_service};
use crate::{
    ApiSettings, AppState,
    auth::{csrf, memory::MemorySessions},
    build_router,
};
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sr_auth::{
    repo::{SessionRecord, SessionRepository},
    session::SessionToken,
};
use sr_storage::mounts::{
    MemoryMountRepo, MountError, MountInput, MountOwner, MountRepo, NewMount, Provider, Sealer,
    Secret,
};
use sr_storage::scopes::MemoryScopeFacts;
use std::io::{Read, Write};
use std::sync::Arc;
use tower::ServiceExt;

const OWNER: i64 = 2;
const MOUNT: i64 = 900;
const BARE: i64 = 901; // a second mount of OWNER, with no worker token

struct XorSealer;
impl Sealer for XorSealer {
    fn seal(&self, aad: &[u8], plain: &[u8]) -> Result<Vec<u8>, MountError> {
        let mut v = aad.to_vec();
        v.push(b'|');
        v.extend(plain.iter().map(|b| b ^ 0x5a));
        Ok(v)
    }
    fn open(&self, aad: &[u8], blob: &[u8]) -> Result<Vec<u8>, MountError> {
        let rest = blob.strip_prefix(aad).and_then(|r| r.strip_prefix(b"|"));
        rest.map(|r| r.iter().map(|b| b ^ 0x5a).collect())
            .ok_or(MountError::Unseal)
    }
}

/// A bucket that answers every request like a `HEAD` of a 4096-byte object. Returns `http://127.0.0.1:<port>`.
fn fake_bucket() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let mut buf = Vec::new();
            let mut chunk = [0u8; 1024];
            while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                match s.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => buf.extend_from_slice(&chunk[..n]),
                }
            }
            let _ = s.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 4096\r\nContent-Type: video/x-matroska\r\n\
                  Last-Modified: Wed, 21 Oct 2026 07:28:00 GMT\r\nETag: \"abc\"\r\nConnection: close\r\n\r\n",
            );
        }
    });
    format!("http://{addr}")
}

struct Env {
    app: Router,
    token: String,
}

async fn env(worker_for_members: bool) -> Env {
    let endpoint = fake_bucket();
    let repo: Arc<dyn MountRepo> = MemoryMountRepo::shared();
    for id in [MOUNT, BARE] {
        let new = NewMount::seal(
            &XorSealer,
            id,
            MountInput {
                owner: MountOwner::User(OWNER),
                label: format!("Shelf {id}"),
                provider: Provider::Garage,
                endpoint: endpoint.clone(),
                region: "garage".into(),
                bucket: "shelf".into(),
                prefix: String::new(),
                path_style: true,
                access_key_id: "GK0123456789abcd".into(),
            },
            &Secret::new("wJalrXUtnFEMI/K7MDENG/SeedSecretValue"),
        )
        .unwrap();
        repo.create(new, None).await.unwrap();
    }

    let sessions = Arc::new(MemorySessions::new());
    let t = SessionToken::generate().unwrap();
    sessions
        .create(SessionRecord {
            id: 100,
            user_id: OWNER,
            token_hash: t.hash(),
            created_at: 0,
            last_seen_at: i64::MAX / 4,
            expires_at: i64::MAX / 4,
            device_name: None,
        })
        .await
        .unwrap();

    let scope = ScopeDeps {
        mounts: repo,
        store: Arc::new(super::uploads_tests::Store(Arc::new(
            MemoryScopeFacts::new(),
        ))),
        sealer: Arc::new(XorSealer),
        guard: None,
    };
    let transcode = TranscodeDeps {
        media: Arc::new(stub_media_service(Arc::new(Stub::default()))),
    };
    let admin_repo = Arc::new(sr_admin::memory::MemoryAdminRepo::default());
    if worker_for_members {
        let audit = sr_admin::AuditEntry {
            actor_id: 1,
            kind: sr_admin::AccessKind::AdminAction,
            action: "settings.test".into(),
            target_kind: None,
            target_id: None,
            reason: "test".into(),
            ip: None,
            detail: "{}".into(),
        };
        sr_admin::AdminRepository::put_settings(
            admin_repo.as_ref(),
            &[(
                sr_admin::settings::BYOS_WORKER_FOR_MEMBERS.to_owned(),
                json!(true),
            )],
            1,
            &audit,
        )
        .await
        .unwrap();
    }
    let admin = crate::routes::admin::AdminDeps::new(sr_admin::AdminService::new(admin_repo));

    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let state = AppState::builder()
        .settings(settings)
        .sessions(sessions)
        .extension(Arc::new(scope))
        .extension(Arc::new(transcode))
        .extension(Arc::new(admin))
        .build();
    Env {
        app: build_router(state, router()),
        token: t.expose().to_owned(),
    }
}

impl Env {
    async fn post(&self, path: &str, body: Value) -> (StatusCode, Value) {
        let req = Request::builder()
            .method(Method::POST)
            .uri(format!("/api/v1{path}"))
            .header("origin", "http://localhost:8080")
            .header("cookie", format!("sr_session={}", self.token))
            .header("x-sr-csrf", csrf::token_for(&self.token))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
}

fn queue_body() -> Value {
    json!({ "key": "films/Film.mkv", "format": "mp4" })
}

#[tokio::test]
async fn an_owner_queues_with_their_own_worker_token_when_the_operator_does_not_serve_members_and_gets_409_without_one()
 {
    // byos.worker_for_members off (the default).
    let e = env(false).await;
    // No token on the mount: nobody would ever take the job.
    let (st, body) = e
        .post(&format!("/storage/mounts/{MOUNT}/transcode"), queue_body())
        .await;
    assert_eq!(st, StatusCode::CONFLICT, "{body}");
    assert!(body.to_string().contains("WORKER_NOT_ENABLED"), "{body}");

    // With a worker token of their own (BYOC), the owner queues; the operator's workers never take it.
    let (st, body) = e
        .post(
            &format!("/storage/mounts/{MOUNT}/worker-tokens"),
            json!({ "name": "home pc" }),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{body}");
    let (st, body) = e
        .post(&format!("/storage/mounts/{MOUNT}/transcode"), queue_body())
        .await;
    assert_eq!(st, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body["operator_worker"], json!(false), "{body}");

    // The token is scoped to its mount: the owner's other mount still has no worker.
    let (st, body) = e
        .post(&format!("/storage/mounts/{BARE}/transcode"), queue_body())
        .await;
    assert_eq!(st, StatusCode::CONFLICT, "{body}");

    // byos.worker_for_members on: the operator's workers serve the member, no token needed.
    let e = env(true).await;
    let (st, body) = e
        .post(&format!("/storage/mounts/{BARE}/transcode"), queue_body())
        .await;
    assert_eq!(st, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body["operator_worker"], json!(true), "{body}");
}
