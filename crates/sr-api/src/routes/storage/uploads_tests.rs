//! Endpoint tests for the write side of the bucket browser (BYOS-BRW-001) over the in-memory ledger, bucket and
//! scope facts: begin, parts, status, complete, abort, folders, move, delete, and who may do which.

use super::router;
use super::{ScopeDeps, UploadDeps};
use crate::{
    ApiSettings, AppState,
    auth::{csrf, memory::MemorySessions},
    build_router,
};
use async_trait::async_trait;
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
    MemoryMountRepo, Mount, MountError, MountInput, MountOwner, MountRepo, NewMount, Provider,
    Sealer, Secret,
};
use sr_storage::scopes::{
    FactsError, Grant, GrantSet, GuildStanding, MemoryScopeFacts, ScopeFacts, ScopeStore, Share,
    SharedToMe, ShelvesSummary,
};
use sr_storage::uploads::{COPY_MAX_BYTES, MemoryLedger, MemoryMountIo, UploadLedger};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tower::ServiceExt;

const OWNER: i64 = 2; // owns the user mount
const FRIEND: i64 = 3; // a friend the mount is shared with
const STRANGER: i64 = 4;
const GUILD: i64 = 500;
const UPLOADER: i64 = 5; // server member with the upload grant
const VIEWER: i64 = 6; // server member with view grants only
const MANAGER: i64 = 7; // server member with the manage grant
const MIB: u64 = 1024 * 1024;

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

/// The in-memory facts plus the share and grant rows the trait wants; only the facts matter here.
struct Store(Arc<MemoryScopeFacts>);

#[async_trait]
impl ScopeFacts for Store {
    async fn owner_active(&self, o: MountOwner) -> Result<bool, FactsError> {
        self.0.owner_active(o).await
    }
    async fn are_friends(&self, a: i64, b: i64) -> Result<bool, FactsError> {
        self.0.are_friends(a, b).await
    }
    async fn share(&self, m: i64, f: i64) -> Result<Option<Share>, FactsError> {
        self.0.share(m, f).await
    }
    async fn guild_standing(&self, g: i64, u: i64) -> Result<Option<GuildStanding>, FactsError> {
        self.0.guild_standing(g, u).await
    }
    async fn role_grants(&self, g: i64, r: &[i64]) -> Result<GrantSet, FactsError> {
        self.0.role_grants(g, r).await
    }
    async fn attached_guilds(&self, m: i64) -> Result<Vec<i64>, FactsError> {
        self.0.attached_guilds(m).await
    }
    async fn delisted(&self, m: i64, k: &[String]) -> Result<Vec<String>, FactsError> {
        self.0.delisted(m, k).await
    }
}

#[async_trait]
impl ScopeStore for Store {
    async fn guild_exists(&self, _: i64) -> Result<bool, FactsError> {
        Ok(true)
    }
    async fn attach_mount(&self, m: i64, g: i64, _: i64) -> Result<bool, FactsError> {
        self.0.attach(m, g);
        Ok(true)
    }
    async fn detach_mount(&self, m: i64, g: i64) -> Result<bool, FactsError> {
        Ok(self.0.detach(m, g))
    }
    async fn role_in_guild(&self, _: i64, _: i64) -> Result<bool, FactsError> {
        Ok(true)
    }
    async fn set_share(&self, _: i64, _: i64, _: Share) -> Result<(), FactsError> {
        Ok(())
    }
    async fn delete_share(&self, _: i64, _: i64) -> Result<bool, FactsError> {
        Ok(false)
    }
    async fn shares_of_mount(&self, _: i64) -> Result<Vec<(i64, Share)>, FactsError> {
        Ok(vec![])
    }
    async fn shares_to(&self, _: i64) -> Result<Vec<SharedToMe>, FactsError> {
        Ok(vec![])
    }
    async fn set_role_grants(&self, _: i64, _: i64, _: &GrantSet) -> Result<(), FactsError> {
        Ok(())
    }
    async fn grants_of_guild(&self, _: i64) -> Result<BTreeMap<i64, GrantSet>, FactsError> {
        Ok(BTreeMap::new())
    }
    async fn catalog_counts(&self) -> Result<(i64, i64), FactsError> {
        Ok((0, 0))
    }
    async fn shelves_summary(&self, _: i64) -> Result<ShelvesSummary, FactsError> {
        Ok(ShelvesSummary {
            cinema: 0,
            library: 0,
            music: 0,
        })
    }
}

struct Env {
    app: Router,
    tokens: Vec<(i64, String)>,
    io: Arc<MemoryMountIo>,
    ledger: Arc<MemoryLedger>,
    repo: Arc<dyn MountRepo>,
    /// The user mount of OWNER, shared with FRIEND.
    user_mount: i64,
    /// The server mount of GUILD.
    guild_mount: i64,
}

async fn mk_mount(
    repo: &Arc<dyn MountRepo>,
    id: i64,
    owner: MountOwner,
    prefix: &str,
    cors: bool,
) -> Mount {
    let new = NewMount::seal(
        &XorSealer,
        id,
        MountInput {
            owner,
            label: format!("Shelf {id}"),
            provider: Provider::Garage,
            endpoint: "http://127.0.0.1:9".into(),
            region: "garage".into(),
            bucket: "shelf".into(),
            prefix: prefix.into(),
            path_style: true,
            access_key_id: "GK0123456789abcd".into(),
        },
        &Secret::new("wJalrXUtnFEMI/K7MDENG/SeedSecretValue"),
    )
    .unwrap();
    repo.create(new, None).await.unwrap();
    if cors {
        repo.set_cors_ok(id, Some(1)).await.unwrap();
    }
    repo.by_id(id).await.unwrap().unwrap()
}

fn standing(roles: &[i64]) -> GuildStanding {
    GuildStanding {
        is_owner: false,
        administrator: false,
        manage_guild: false,
        manage_roles: false,
        role_ids: roles.to_vec(),
    }
}

async fn env() -> Env {
    env_with(None).await
}

async fn env_with(guard: Option<Arc<dyn sr_storage::guard::EndpointGuard>>) -> Env {
    let repo: Arc<dyn MountRepo> = MemoryMountRepo::shared();
    let user_mount = mk_mount(&repo, 900, MountOwner::User(OWNER), "lib", true)
        .await
        .id;
    let guild_mount = mk_mount(&repo, 901, MountOwner::Guild(GUILD), "", true)
        .await
        .id;
    let facts = Arc::new(MemoryScopeFacts::new());
    facts.befriend(OWNER, FRIEND);
    facts.share_with(
        user_mount,
        FRIEND,
        Share {
            cinema: true,
            library: true,
            music: true,
        },
    );
    // Roles: 71 views everything, 72 may upload, 73 manages.
    facts.member(GUILD, UPLOADER, standing(&[72]));
    facts.member(GUILD, VIEWER, standing(&[71]));
    facts.member(GUILD, MANAGER, standing(&[73]));
    facts.grant(
        GUILD,
        71,
        GrantSet::of([Grant::ViewCinema, Grant::ViewLibrary, Grant::ViewMusic]),
    );
    facts.grant(GUILD, 72, GrantSet::of([Grant::ViewMusic, Grant::Upload]));
    facts.grant(GUILD, 73, GrantSet::of([Grant::Manage]));

    let sessions = Arc::new(MemorySessions::new());
    let mut tokens = Vec::new();
    for (n, uid) in [OWNER, FRIEND, STRANGER, UPLOADER, VIEWER, MANAGER]
        .into_iter()
        .enumerate()
    {
        let t = SessionToken::generate().unwrap();
        sessions
            .create(SessionRecord {
                id: 100 + n as i64,
                user_id: uid,
                token_hash: t.hash(),
                created_at: 0,
                last_seen_at: i64::MAX / 4,
                expires_at: i64::MAX / 4,
                device_name: None,
            })
            .await
            .unwrap();
        tokens.push((uid, t.expose().to_owned()));
    }
    let io = Arc::new(MemoryMountIo::new());
    let ledger = Arc::new(MemoryLedger::new());
    let next = Arc::new(AtomicU64::new(7000));
    let scope = ScopeDeps {
        mounts: repo.clone(),
        store: Arc::new(Store(facts)),
        sealer: Arc::new(XorSealer),
        guard,
    };
    let uploads = UploadDeps {
        ledger: ledger.clone(),
        io: io.clone(),
        next_id: Arc::new(move || next.fetch_add(1, Ordering::SeqCst)),
    };
    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let state = AppState::builder()
        .settings(settings)
        .sessions(sessions)
        .extension(Arc::new(scope))
        .extension(Arc::new(uploads))
        .build();
    Env {
        app: build_router(state, router()),
        tokens,
        io,
        ledger,
        repo,
        user_mount,
        guild_mount,
    }
}

impl Env {
    fn token(&self, uid: i64) -> &str {
        &self.tokens.iter().find(|(u, _)| *u == uid).unwrap().1
    }

    async fn call(
        &self,
        who: Option<i64>,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method.clone())
            .uri(format!("/api/v1{path}"))
            .header("origin", "http://localhost:8080");
        if let Some(uid) = who {
            let t = self.token(uid);
            req = req.header("cookie", format!("sr_session={t}"));
            if method != Method::GET {
                req = req.header("x-sr-csrf", csrf::token_for(t));
            }
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

    async fn post(&self, who: i64, path: &str, body: Value) -> (StatusCode, Value) {
        self.call(Some(who), Method::POST, path, Some(body)).await
    }
}

fn code(v: &Value) -> &str {
    v["errors"]["mount"]["_errors"][0]["code"]
        .as_str()
        .or_else(|| {
            v["errors"]
                .as_object()
                .and_then(|o| o.values().next())
                .and_then(|f| f["_errors"][0]["code"].as_str())
        })
        .unwrap_or("")
}

fn begin_body(key: &str, size: u64) -> Value {
    json!({ "key": key, "size": size, "content_type": "audio/flac" })
}

#[tokio::test]
async fn an_upload_runs_begin_parts_resume_complete_and_closes_the_ledger() {
    let e = env().await;
    let m = e.user_mount;
    let base = format!("/storage/mounts/{m}/uploads");
    let (st, up) = e
        .post(OWNER, &base, begin_body("Music/a.flac", 40 * MIB))
        .await;
    assert_eq!(st, StatusCode::CREATED, "{up}");
    assert_eq!(up["part_count"], 3);
    assert_eq!(up["part_size"], 16 * MIB);
    assert!(
        up.get("s3_upload_id").is_none(),
        "the bucket's id never leaves the server"
    );
    let id = up["id"].as_str().unwrap().to_owned();
    let one = format!("{base}/{id}");

    // The part URLs: sizes follow the file, a part past the end is refused.
    let (st, p1) = e.post(OWNER, &format!("{one}/parts/1"), json!({})).await;
    assert_eq!(st, StatusCode::OK, "{p1}");
    assert_eq!(p1["size"], 16 * MIB);
    assert!(p1["url"].as_str().unwrap().contains("partNumber=1"));
    let (_, p3) = e.post(OWNER, &format!("{one}/parts/3"), json!({})).await;
    assert_eq!(p3["size"], 8 * MIB);
    let (st, p4) = e.post(OWNER, &format!("{one}/parts/4"), json!({})).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{p4}");
    let (st, _) = e.post(OWNER, &format!("{one}/parts/0"), json!({})).await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    // A reload lists the open upload and the parts that arrived.
    let s3 = e.ledger.get(m, id.parse().unwrap()).await.unwrap().unwrap();
    e.io.send_part(&s3.s3_upload_id, 1, 16 * MIB).unwrap();
    let (_, listed) = e.call(Some(OWNER), Method::GET, &base, None).await;
    assert_eq!(listed.as_array().unwrap().len(), 1);
    let (st, status) = e.call(Some(OWNER), Method::GET, &one, None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(status["parts"].as_array().unwrap().len(), 1);
    assert_eq!(status["parts"][0]["number"], 1);

    // Missing parts: the object is not made.
    let (st, bad) = e.post(OWNER, &format!("{one}/complete"), json!({})).await;
    assert_eq!(st, StatusCode::CONFLICT, "{bad}");
    assert_eq!(code(&bad), "UPLOAD_INCOMPLETE");
    assert!(!e.io.has(m, "Music/a.flac"));

    // A part of the wrong size is refused too.
    e.io.send_part(&s3.s3_upload_id, 2, 10 * MIB).unwrap();
    e.io.send_part(&s3.s3_upload_id, 3, 8 * MIB).unwrap();
    let (st, bad) = e.post(OWNER, &format!("{one}/complete"), json!({})).await;
    assert_eq!(st, StatusCode::CONFLICT, "{bad}");
    e.io.send_part(&s3.s3_upload_id, 2, 16 * MIB).unwrap();

    // A client etag that is not the bucket's is refused; the right one completes.
    let (st, _) = e
        .post(
            OWNER,
            &format!("{one}/complete"),
            json!({"parts": [{"number": 1, "etag": "\"nope\""}]}),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);
    let etag = format!("\"etag-1-{}\"", 16 * MIB);
    let (st, done) = e
        .post(
            OWNER,
            &format!("{one}/complete"),
            json!({"parts": [{"number": 1, "etag": etag}]}),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{done}");
    assert_eq!(done["key"], "Music/a.flac");
    assert_eq!(done["size"], 40 * MIB);
    assert_eq!(e.io.size_of(m, "Music/a.flac"), Some(40 * MIB));

    // The ledger is closed: the upload is gone from the list and cannot be completed again.
    let (_, listed) = e.call(Some(OWNER), Method::GET, &base, None).await;
    assert!(listed.as_array().unwrap().is_empty());
    let (st, _) = e.post(OWNER, &format!("{one}/complete"), json!({})).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn abort_frees_the_bucket_and_closes_the_ledger() {
    let e = env().await;
    let m = e.user_mount;
    let base = format!("/storage/mounts/{m}/uploads");
    let (_, up) = e.post(OWNER, &base, begin_body("a.flac", 20 * MIB)).await;
    let id = up["id"].as_str().unwrap();
    let row = e.ledger.get(m, id.parse().unwrap()).await.unwrap().unwrap();
    assert!(e.io.is_open(&row.s3_upload_id));
    let (st, _) = e
        .call(Some(OWNER), Method::DELETE, &format!("{base}/{id}"), None)
        .await;
    assert_eq!(st, StatusCode::NO_CONTENT);
    assert!(!e.io.is_open(&row.s3_upload_id));
    let (st, _) = e
        .post(OWNER, &format!("{base}/{id}/parts/1"), json!({}))
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    assert_eq!(e.ledger.count_open(OWNER).await.unwrap(), 0);
}

#[tokio::test]
async fn who_may_write_owner_yes_friend_stranger_viewer_no_and_roles_decide() {
    let e = env().await;
    let m = e.user_mount;
    let begin = format!("/storage/mounts/{m}/uploads");
    // A friend with a full share reaches the shelf and still cannot write.
    let (st, _) = e.post(FRIEND, &begin, begin_body("x.mp3", MIB)).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, _) = e
        .post(
            FRIEND,
            &format!("/storage/mounts/{m}/folders"),
            json!({"key": "F/"}),
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    // A stranger cannot even see that the mount exists.
    let (st, _) = e.post(STRANGER, &begin, begin_body("x.mp3", MIB)).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    // No session: no write.
    let (st, _) = e
        .call(None, Method::POST, &begin, Some(begin_body("x.mp3", MIB)))
        .await;
    assert!(st == StatusCode::UNAUTHORIZED || st == StatusCode::FORBIDDEN);

    // Server shelf: the upload grant uploads and makes folders but may not move or delete; manage does all.
    let g = e.guild_mount;
    let gbegin = format!("/storage/mounts/{g}/uploads");
    let (st, _) = e.post(VIEWER, &gbegin, begin_body("x.mp3", MIB)).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, up) = e.post(UPLOADER, &gbegin, begin_body("x.mp3", MIB)).await;
    assert_eq!(st, StatusCode::CREATED, "{up}");
    let (st, _) = e
        .post(
            UPLOADER,
            &format!("/storage/mounts/{g}/folders"),
            json!({"key": "New/"}),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED);
    e.io.seed(g, "old.mp3", 5);
    let mv = json!({"from": "old.mp3", "to": "new.mp3"});
    let (st, _) = e
        .post(UPLOADER, &format!("/storage/mounts/{g}/move"), mv.clone())
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, _) = e
        .call(
            Some(UPLOADER),
            Method::DELETE,
            &format!("/storage/mounts/{g}/objects/old.mp3"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, body) = e
        .post(MANAGER, &format!("/storage/mounts/{g}/move"), mv)
        .await;
    assert_eq!(st, StatusCode::OK, "{body}");

    // Another member's upload id is not found, even for a manager.
    let uid = up["id"].as_str().unwrap();
    let (st, _) = e
        .post(MANAGER, &format!("{gbegin}/{uid}/parts/1"), json!({}))
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _) = e
        .call(
            Some(MANAGER),
            Method::DELETE,
            &format!("{gbegin}/{uid}"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn begin_validates_the_key_the_size_and_the_cors_gate() {
    let e = env().await;
    let m = e.user_mount;
    let base = format!("/storage/mounts/{m}/uploads");
    for key in ["../x", "/abs", "a//b", "dir/", "a\\b", ""] {
        let (st, _) = e.post(OWNER, &base, begin_body(key, MIB)).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "key {key:?}");
    }
    let (st, v) = e.post(OWNER, &base, begin_body("a.mp3", 0)).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{v}");
    let (st, _) = e
        .post(OWNER, &base, begin_body("a.mp3", 157 * 1024 * MIB))
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, _) = e
        .post(
            OWNER,
            &base,
            json!({"key": "a.mp3", "size": 5, "content_type": "a\nb"}),
        )
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    // CORS not passed: nothing starts.
    e.repo.set_cors_ok(m, None).await.unwrap();
    let (st, v) = e.post(OWNER, &base, begin_body("a.mp3", MIB)).await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert_eq!(code(&v), "CORS_NOT_READY");
    assert_eq!(e.ledger.count_open(OWNER).await.unwrap(), 0);
}

#[tokio::test]
async fn a_dead_bucket_and_the_open_upload_cap_and_the_hourly_part_quota() {
    let e = env().await;
    let m = e.user_mount;
    let base = format!("/storage/mounts/{m}/uploads");
    *e.io.broken.lock().unwrap() = true;
    let (st, v) = e.post(OWNER, &base, begin_body("a.mp3", MIB)).await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert_eq!(code(&v), "BUCKET_UNREACHABLE");
    assert_eq!(e.ledger.count_open(OWNER).await.unwrap(), 0);
    *e.io.broken.lock().unwrap() = false;

    let mut first = String::new();
    for n in 0..50 {
        let (st, v) = e
            .post(OWNER, &base, begin_body(&format!("f{n}.mp3"), MIB))
            .await;
        assert_eq!(st, StatusCode::CREATED, "{v}");
        if n == 0 {
            first = v["id"].as_str().unwrap().to_owned();
        }
    }
    let (st, v) = e.post(OWNER, &base, begin_body("one-more.mp3", MIB)).await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert_eq!(code(&v), "TOO_MANY_UPLOADS");

    // The quota counts part URLs issued in the last hour (2000 by default).
    let row = e
        .ledger
        .get(m, first.parse().unwrap())
        .await
        .unwrap()
        .unwrap();
    let now_ms = row.created_at;
    e.ledger
        .note_parts(OWNER, row.id, 2_000, now_ms)
        .await
        .unwrap();
    let (st, _) = e
        .post(OWNER, &format!("{base}/{first}/parts/1"), json!({}))
        .await;
    assert_eq!(st, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn folders_are_markers_and_move_and_delete_keep_the_tree_whole() {
    let e = env().await;
    let m = e.user_mount;
    let pre = format!("/storage/mounts/{m}");
    let (st, v) = e
        .post(
            OWNER,
            &format!("{pre}/folders"),
            json!({"key": "Albums/New/"}),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{v}");
    assert!(e.io.has(m, "Albums/New/"));
    for bad in ["Albums/New", "../x/", "/x/", "a//b/"] {
        let (st, _) = e
            .post(OWNER, &format!("{pre}/folders"), json!({ "key": bad }))
            .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{bad}");
    }

    // A folder with a nested folder and files moves whole, markers included, and the old tree is gone.
    e.io.seed(m, "Albums/", 0);
    e.io.seed(m, "Albums/a.flac", 9);
    e.io.seed(m, "Albums/Live/", 0);
    e.io.seed(m, "Albums/Live/b.flac", 7);
    e.io.seed(m, "Albums2/other.flac", 3);
    let (st, v) = e
        .post(
            OWNER,
            &format!("{pre}/move"),
            json!({"from": "Albums/", "to": "Records/"}),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(
        e.io.keys(m),
        vec![
            "Albums2/other.flac",
            "Records/",
            "Records/Live/",
            "Records/Live/b.flac",
            "Records/New/",
            "Records/a.flac",
        ]
    );
    assert_eq!(e.io.size_of(m, "Records/a.flac"), Some(9));

    // Refusals leave the tree alone.
    let mv = |from: &str, to: &str| json!({"from": from, "to": to});
    let cases = [
        (
            mv("Records/", "Records/Live/Deeper/"),
            StatusCode::BAD_REQUEST,
        ),
        (mv("Records/", "Records/"), StatusCode::BAD_REQUEST),
        (mv("Records/", "file.flac"), StatusCode::BAD_REQUEST),
        (mv("Records/a.flac", "Dir/"), StatusCode::BAD_REQUEST),
        (mv("Nothing/", "Else/"), StatusCode::NOT_FOUND),
        (mv("Records/a.flac", "../out.flac"), StatusCode::BAD_REQUEST),
        (
            mv("Records/a.flac", "Albums2/other.flac"),
            StatusCode::CONFLICT,
        ),
        (mv("Records/", "Albums2/"), StatusCode::CONFLICT),
    ];
    for (body, want) in cases {
        let (st, v) = e.post(OWNER, &format!("{pre}/move"), body.clone()).await;
        assert_eq!(st, want, "{body} -> {v}");
    }
    assert_eq!(e.io.keys(m).len(), 6);

    // A rename of one file; a name that merely starts the same is not touched.
    e.io.seed(m, "Records/a.flac.bak", 1);
    let (st, _) = e
        .post(
            OWNER,
            &format!("{pre}/move"),
            mv("Records/a.flac", "Records/z.flac"),
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    assert!(e.io.has(m, "Records/z.flac") && !e.io.has(m, "Records/a.flac"));
    assert!(e.io.has(m, "Records/a.flac.bak"));

    // A file over 5 GiB is refused with the reason, not half moved.
    e.io.seed(m, "huge.mkv", COPY_MAX_BYTES + 1);
    let (st, v) = e
        .post(OWNER, &format!("{pre}/move"), mv("huge.mkv", "film.mkv"))
        .await;
    assert_eq!(st, StatusCode::CONFLICT, "{v}");
    assert_eq!(code(&v), "OBJECT_TOO_LARGE");
    assert!(e.io.has(m, "huge.mkv") && !e.io.has(m, "film.mkv"));

    // Delete: one file, then a folder with all that is in it.
    let (st, v) = e
        .call(
            Some(OWNER),
            Method::DELETE,
            &format!("{pre}/objects/Records/z.flac"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["count"], 1);
    assert!(e.io.has(m, "Records/a.flac.bak"), "a prefix-sibling stays");
    let (st, v) = e
        .call(
            Some(OWNER),
            Method::DELETE,
            &format!("{pre}/objects/Records/"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["count"], 5);
    assert_eq!(e.io.keys(m), vec!["Albums2/other.flac", "huge.mkv"]);
    let (st, _) = e
        .call(
            Some(OWNER),
            Method::DELETE,
            &format!("{pre}/objects/Records/"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let (st, _) = e
        .call(
            Some(OWNER),
            Method::DELETE,
            &format!("{pre}/objects/missing.mp3"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_folder_over_the_cap_is_refused_whole() {
    let e = env().await;
    let m = e.user_mount;
    for i in 0..=500 {
        e.io.seed(m, &format!("Big/{i:04}.mp3"), 1);
    }
    let (st, v) = e
        .call(
            Some(OWNER),
            Method::DELETE,
            &format!("/storage/mounts/{m}/objects/Big/"),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT, "{v}");
    assert_eq!(code(&v), "FOLDER_TOO_LARGE");
    let (st, _) = e
        .post(
            OWNER,
            &format!("/storage/mounts/{m}/move"),
            json!({"from": "Big/", "to": "Huge/"}),
        )
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    assert_eq!(e.io.keys(m).len(), 501);
}

/// A guard that has seen the endpoint resolve to an internal address.
struct Rebound;

#[async_trait]
impl sr_storage::guard::EndpointGuard for Rebound {
    async fn validated_addrs(&self, _: &str) -> Result<Vec<std::net::SocketAddr>, String> {
        Err("blocked address 169.254.169.254".into())
    }
}

#[tokio::test]
async fn a_member_endpoint_that_rebinds_is_refused_before_the_bucket_is_called() {
    let e = env_with(Some(Arc::new(Rebound))).await;
    let m = e.user_mount;
    for path in [
        format!("/storage/mounts/{m}/objects"),
        format!("/storage/mounts/{m}/objects/a.mp3"),
    ] {
        let (st, v) = e.call(Some(OWNER), Method::GET, &path, None).await;
        assert_eq!(st, StatusCode::CONFLICT, "{path}: {v}");
        assert_eq!(code(&v), "MOUNT_ENDPOINT_BLOCKED");
        assert!(
            !v.to_string().contains("169.254"),
            "the reason is not shown"
        );
    }
}

#[tokio::test]
async fn the_net_guard_applies_the_ssrf_policy_to_every_answer() {
    use sr_storage::guard::EndpointGuard;
    let strict = super::NetGuard::new(sr_net::Policy::default());
    assert!(strict.validated_addrs("http://127.0.0.1:9").await.is_err());
    assert!(
        strict
            .validated_addrs("http://169.254.169.254")
            .await
            .is_err()
    );
    assert!(
        strict
            .validated_addrs("http://10.0.0.5:3900")
            .await
            .is_err()
    );
    assert!(strict.validated_addrs("not a url").await.is_err());
    let dev = super::NetGuard::new(sr_net::Policy {
        allow_private_for_tests: true,
        ..Default::default()
    });
    let addrs = dev.validated_addrs("http://127.0.0.1:9").await.unwrap();
    assert_eq!(addrs.len(), 1);
    assert_eq!(addrs[0].port(), 9);
}

#[tokio::test]
async fn the_listing_tells_the_ui_what_the_caller_may_do() {
    let e = env().await;
    let d = ScopeDeps {
        mounts: e.repo.clone(),
        store: Arc::new(Store({
            // The same facts as env(): rebuilt here because the router owns the originals.
            let f = Arc::new(MemoryScopeFacts::new());
            f.befriend(OWNER, FRIEND);
            f.share_with(
                e.user_mount,
                FRIEND,
                Share {
                    cinema: true,
                    library: true,
                    music: true,
                },
            );
            f.member(GUILD, UPLOADER, standing(&[72]));
            f.member(GUILD, MANAGER, standing(&[73]));
            f.grant(GUILD, 72, GrantSet::of([Grant::ViewMusic, Grant::Upload]));
            f.grant(GUILD, 73, GrantSet::of([Grant::Manage]));
            f
        })),
        sealer: Arc::new(XorSealer),
        guard: None,
    };
    let user_mount = e.repo.by_id(e.user_mount).await.unwrap().unwrap();
    let guild_mount = e.repo.by_id(e.guild_mount).await.unwrap().unwrap();
    // Own mount: everything. A friend with a share: nothing. Server shelf: the grants decide.
    let caps = |m: &Mount, user: i64| {
        let r = super::objects::Reach {
            access: sr_storage::scopes::Access {
                scope: sr_storage::scopes::Scope::Own,
                kinds: Share {
                    cinema: true,
                    library: true,
                    music: true,
                },
                unrestricted: true,
                mount_id: m.id,
            },
            mount: m.clone(),
            staff: None,
        };
        let d = &d;
        async move { super::objects::capabilities(d, &r, user).await }
    };
    assert_eq!(caps(&user_mount, OWNER).await, (true, true));
    assert_eq!(caps(&user_mount, FRIEND).await, (false, false));
    assert_eq!(caps(&guild_mount, UPLOADER).await, (true, false));
    assert_eq!(caps(&guild_mount, MANAGER).await, (true, true));
    assert_eq!(caps(&guild_mount, STRANGER).await, (false, false));
}
