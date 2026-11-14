//! Postgres-backed tests of the stored-package check (SEC-AUD-173, D-654, D-655): `ingest_stored_zip`, the tiered
//! limits and `upload_bundle` on top of them. Storage is in memory behind a spy that records how it was read.
//! Run with a scratch Postgres that may CREATE DATABASE:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-activities --test package_db -- --ignored --test-threads=1`

use std::io::{Cursor, Write};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicI64, AtomicUsize, Ordering},
};

use async_trait::async_trait;
use bytes::Bytes;
use sr_activities::model::BundleState;
use sr_activities::package::{
    PackageSettingsSnapshot, PackageTier, PackageTrust, StaticPackageSettings,
};
use sr_activities::zipstore::CHECK_BLOCK;
use sr_activities::{ActivityError, ActivityService, sandbox};
use sr_db::{
    PgPool,
    accounts::{self, NewAccount},
    testing::TestDb,
};
use sr_storage::{MultipartSession, ObjectInfo, ObjectStorage, ObjectStoreStorage, ObjectStream};
use zip::write::SimpleFileOptions;

static SEQ: AtomicI64 = AtomicI64::new(0);

fn uid() -> i64 {
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + SEQ.fetch_add(1, Ordering::Relaxed) % 1000
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

async fn application(pool: &PgPool, owner: i64, name: &str) -> i64 {
    let bot = account(pool).await;
    let id = uid();
    sqlx::query(
        "INSERT INTO applications (id, owner_id, bot_user_id, name, client_secret_hash) \
         VALUES ($1, $2, $3, $4, decode(repeat('ab', 32), 'hex'))",
    )
    .bind(id)
    .bind(owner)
    .bind(bot)
    .bind(name)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// Storage that records how it was read and written.
struct Spy {
    inner: Arc<dyn ObjectStorage>,
    gets: Mutex<Vec<String>>,
    ranges: Mutex<Vec<(String, u64)>>,
    multiparts: Mutex<Vec<String>>,
    parts: Arc<AtomicUsize>,
    puts: Mutex<Vec<(String, usize)>>,
}

impl Spy {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::new(ObjectStoreStorage::in_memory()),
            gets: Mutex::new(Vec::new()),
            ranges: Mutex::new(Vec::new()),
            multiparts: Mutex::new(Vec::new()),
            parts: Arc::new(AtomicUsize::new(0)),
            puts: Mutex::new(Vec::new()),
        })
    }
    async fn keys(&self) -> Vec<String> {
        self.inner
            .list("")
            .await
            .unwrap()
            .into_iter()
            .map(|o| o.key)
            .collect()
    }
}

struct CountParts {
    inner: Box<dyn MultipartSession>,
    parts: Arc<AtomicUsize>,
}

#[async_trait]
impl MultipartSession for CountParts {
    async fn put_part(&mut self, data: Bytes) -> sr_storage::Result<()> {
        self.parts.fetch_add(1, Ordering::Relaxed);
        self.inner.put_part(data).await
    }
    async fn complete(&mut self) -> sr_storage::Result<()> {
        self.inner.complete().await
    }
    async fn abort(&mut self) -> sr_storage::Result<()> {
        self.inner.abort().await
    }
}

#[async_trait]
impl ObjectStorage for Spy {
    async fn put(&self, k: &str, d: Bytes) -> sr_storage::Result<()> {
        self.puts.lock().unwrap().push((k.to_owned(), d.len()));
        self.inner.put(k, d).await
    }
    async fn get(&self, k: &str) -> sr_storage::Result<Bytes> {
        self.gets.lock().unwrap().push(k.to_owned());
        self.inner.get(k).await
    }
    async fn get_range(&self, k: &str, s: u64, e: u64) -> sr_storage::Result<Bytes> {
        self.ranges.lock().unwrap().push((k.to_owned(), e - s));
        self.inner.get_range(k, s, e).await
    }
    async fn get_stream(
        &self,
        k: &str,
        r: Option<std::ops::Range<u64>>,
    ) -> sr_storage::Result<ObjectStream> {
        self.inner.get_stream(k, r).await
    }
    async fn head(&self, k: &str) -> sr_storage::Result<ObjectInfo> {
        self.inner.head(k).await
    }
    async fn delete(&self, k: &str) -> sr_storage::Result<()> {
        self.inner.delete(k).await
    }
    async fn list(&self, p: &str) -> sr_storage::Result<Vec<ObjectInfo>> {
        self.inner.list(p).await
    }
    async fn presign(
        &self,
        m: sr_storage::PresignMethod,
        k: &str,
        t: std::time::Duration,
    ) -> sr_storage::Result<String> {
        self.inner.presign(m, k, t).await
    }
    async fn multipart_create(&self, k: &str) -> sr_storage::Result<Box<dyn MultipartSession>> {
        self.multiparts.lock().unwrap().push(k.to_owned());
        let inner = self.inner.multipart_create(k).await?;
        Ok(Box::new(CountParts {
            inner,
            parts: self.parts.clone(),
        }))
    }
}

struct Platform(i64);

#[async_trait]
impl PackageTrust for Platform {
    async fn platform_owned(&self, owner: i64) -> bool {
        owner == self.0
    }
}

struct Rig {
    db: TestDb,
    svc: ActivityService,
    spy: Arc<Spy>,
}

fn settings(standard: u64, reviewed: u64, platform: u64, files: usize) -> PackageSettingsSnapshot {
    PackageSettingsSnapshot {
        standard_mb: standard,
        reviewed_mb: reviewed,
        platform_mb: platform,
        max_files: files,
    }
}

async fn rig_with(
    snapshot: PackageSettingsSnapshot,
    platform_owner: Option<i64>,
    part: Option<usize>,
) -> Rig {
    let db = TestDb::new().await.unwrap();
    let spy = Spy::new();
    let mut svc = ActivityService::new(db.pool().clone(), Arc::new(uid))
        .with_storage(spy.clone())
        .with_package_settings(Arc::new(StaticPackageSettings(snapshot)));
    if let Some(o) = platform_owner {
        svc = svc.with_package_trust(Arc::new(Platform(o)));
    }
    if let Some(p) = part {
        svc = svc.with_part_bytes(p);
    }
    Rig { db, svc, spy }
}

async fn rig() -> Rig {
    rig_with(settings(512, 4096, 32768, 20_000), None, None).await
}

fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (n, d) in files {
        z.start_file(*n, SimpleFileOptions::default()).unwrap();
        z.write_all(d).unwrap();
    }
    z.finish().unwrap().into_inner()
}

/// `n` bytes that compress about two to one, so a few MiB of them are a few MiB of zip.
fn noisy(n: usize, seed: u64) -> Vec<u8> {
    let mut x = seed | 1;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x & 0x0f) as u8 + b'a'
        })
        .collect()
}

/// A one-file stored zip (`z.js`, after `a.js`) whose index claims `claimed` bytes for it while the data has more.
fn lying_zip() -> Vec<u8> {
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    z.start_file("index.html", stored).unwrap();
    z.write_all(b"<p>").unwrap();
    z.start_file("a.js", stored).unwrap();
    z.write_all(b"let a = 1;").unwrap();
    z.start_file("z.js", stored).unwrap();
    z.write_all(&[b'x'; 4096]).unwrap();
    let mut b = z.finish().unwrap().into_inner();
    // The index header of z.js: signature, then 20 bytes to the uncompressed size.
    let at = b
        .windows(4)
        .rposition(|w| w == [0x50, 0x4b, 0x01, 0x02])
        .unwrap();
    b[at + 24..at + 28].copy_from_slice(&10u32.to_le_bytes());
    b
}

async fn stage(r: &Rig, app: i64, name: &str, zip: Vec<u8>) -> String {
    let key = format!("activities/{app}/uploads/{name}.zip");
    r.spy.inner.put(&key, Bytes::from(zip)).await.unwrap();
    key
}

async fn bundle_rows(r: &Rig, app: i64) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM application_activity_bundles WHERE application_id = $1",
    )
    .bind(app)
    .fetch_one(r.db.pool())
    .await
    .unwrap()
}

async fn fresh_app(r: &Rig, name: &str) -> (i64, i64) {
    let owner = account(r.db.pool()).await;
    (application(r.db.pool(), owner, name).await, owner)
}

fn invalid(e: ActivityError) -> String {
    match e {
        ActivityError::Invalid("bundle", m) => m,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn every_existing_refusal_still_refuses_through_the_stored_path() {
    let r = rig().await;
    let (app, owner) = fresh_app(&r, "Refusals").await;
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    z.start_file("index.html", SimpleFileOptions::default())
        .unwrap();
    z.write_all(b"x").unwrap();
    z.add_symlink("link.txt", "/etc/passwd", SimpleFileOptions::default())
        .unwrap();
    let symlink = z.finish().unwrap().into_inner();
    let cases: Vec<(&str, Vec<u8>, &str)> = vec![
        (
            "traversal",
            zip_of(&[("index.html", b"x"), ("../evil.js", b"x")]),
            "evil.js",
        ),
        (
            "extension",
            zip_of(&[("index.html", b"x"), ("run.exe", b"x")]),
            "run.exe",
        ),
        (
            "duplicate",
            zip_of(&[("index.html", b"x"), ("A.js", b"1"), ("a.js", b"2")]),
            "a.js",
        ),
        (
            "file below file",
            zip_of(&[
                ("index.html", b"x"),
                ("lib.js", b"1"),
                ("lib.js/x.js", b"2"),
            ]),
            "lib.js/x.js",
        ),
        ("symlink", symlink, "link.txt"),
        (
            "bad manifest",
            zip_of(&[
                ("index.html", b"x"),
                ("sideroom.json", br#"{"capabilities":["root"]}"#),
            ]),
            "sideroom.json",
        ),
        ("no index", zip_of(&[("a.js", b"x")]), "index.html"),
        ("not a zip", b"definitely not a zip".to_vec(), "zip"),
        ("empty", Vec::new(), "empty"),
    ];
    for (i, (what, bytes, needle)) in cases.into_iter().enumerate() {
        let key = stage(&r, app, &format!("c{i}"), bytes).await;
        let e = r
            .svc
            .ingest_stored_zip(app, owner, &key, None)
            .await
            .unwrap_err();
        let m = invalid(e);
        assert!(m.contains(needle), "{what}: {m}");
    }
    assert!(r.spy.keys().await.is_empty(), "{:?}", r.spy.keys().await);
    assert_eq!(bundle_rows(&r, app).await, 0);
    r.db.cleanup().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_package_over_the_application_limit_is_refused_and_names_the_limit() {
    // One MiB for everybody.
    let r = rig_with(settings(1, 1, 1, 20_000), None, None).await;
    let (app, owner) = fresh_app(&r, "Big").await;
    // Declares 1.5 MiB unpacked, the zip itself is under 1 MiB.
    let body = noisy(1_500_000, 7);
    let z = zip_of(&[("index.html", b"x"), ("data.txt", &body)]);
    assert!(z.len() < 1024 * 1024, "{}", z.len());
    let key = stage(&r, app, "unpacked", z).await;
    let m = invalid(
        r.svc
            .ingest_stored_zip(app, owner, &key, None)
            .await
            .unwrap_err(),
    );
    assert!(m.contains("data.txt") && m.contains("1 MiB"), "{m}");
    assert!(
        m.ends_with("The operator of this server can raise the limit."),
        "{m}"
    );
    // The zip itself over the limit: refused before it is read.
    let big = zip_of(&[("index.html", b"x"), ("data.txt", &noisy(3_000_000, 9))]);
    let key = stage(&r, app, "zip", big).await;
    let before = r.spy.ranges.lock().unwrap().len();
    let m = invalid(
        r.svc
            .ingest_stored_zip(app, owner, &key, None)
            .await
            .unwrap_err(),
    );
    assert!(m.contains("1 MiB") && m.contains("operator"), "{m}");
    assert_eq!(
        r.spy.ranges.lock().unwrap().len(),
        before,
        "nothing was read"
    );
    assert!(r.spy.keys().await.is_empty());
    assert_eq!(bundle_rows(&r, app).await, 0);
    r.db.cleanup().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_reviewed_application_gets_the_reviewed_limit() {
    let r = rig_with(settings(1, 4, 8, 20_000), None, None).await;
    let (app, owner) = fresh_app(&r, "Reviewed").await;
    let l = r.svc.package_limits(app).await.unwrap();
    assert_eq!((l.tier, l.max_bytes), (PackageTier::Standard, 1024 * 1024));
    // Give it a reviewed bundle.
    let up = r
        .svc
        .upload_bundle(app, owner, zip_of(&[("index.html", b"<p>one")]))
        .await
        .unwrap();
    let id: i64 = up.bundle.id.parse().unwrap();
    r.svc
        .save_config(
            app,
            sr_activities::model::ConfigInput {
                dev_entry_url: None,
                url_mappings: Vec::new(),
            },
        )
        .await
        .unwrap();
    r.svc.submit(app, id).await.unwrap();
    r.svc.decide(app, 1, id, true, None).await.unwrap();
    let l = r.svc.package_limits(app).await.unwrap();
    assert_eq!(
        (l.tier, l.max_bytes),
        (PackageTier::Reviewed, 4 * 1024 * 1024)
    );
    // 2.5 MiB unpacked passes now.
    let body = noisy(2_500_000, 3);
    r.svc
        .upload_bundle(
            app,
            owner,
            zip_of(&[("index.html", b"<p>two"), ("d.txt", &body)]),
        )
        .await
        .unwrap();
    r.db.cleanup().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_platform_owned_application_gets_the_platform_limit() {
    let db = TestDb::new().await.unwrap();
    let staff = account(db.pool()).await;
    let dev = account(db.pool()).await;
    let spy = Spy::new();
    let svc = ActivityService::new(db.pool().clone(), Arc::new(uid))
        .with_storage(spy)
        .with_package_settings(Arc::new(StaticPackageSettings(settings(1, 4, 8, 20_000))))
        .with_package_trust(Arc::new(Platform(staff)));
    let own = application(db.pool(), staff, "Flagship").await;
    let other = application(db.pool(), dev, "Indie").await;
    let l = svc.package_limits(own).await.unwrap();
    assert_eq!(
        (l.tier, l.max_bytes),
        (PackageTier::Platform, 8 * 1024 * 1024)
    );
    let l = svc.package_limits(other).await.unwrap();
    assert_eq!((l.tier, l.max_bytes), (PackageTier::Standard, 1024 * 1024));
    db.cleanup().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn raising_the_standard_setting_above_the_others_applies_to_everyone() {
    let db = TestDb::new().await.unwrap();
    let staff = account(db.pool()).await;
    let dev = account(db.pool()).await;
    let svc = ActivityService::new(db.pool().clone(), Arc::new(uid))
        .with_storage(Spy::new())
        .with_package_settings(Arc::new(StaticPackageSettings(settings(100, 4, 8, 20_000))))
        .with_package_trust(Arc::new(Platform(staff)));
    let own = application(db.pool(), staff, "Flagship").await;
    let other = application(db.pool(), dev, "Indie").await;
    for app in [own, other] {
        assert_eq!(
            svc.package_limits(app).await.unwrap().max_bytes,
            100 * 1024 * 1024
        );
    }
    // The tier is still what the application earned.
    assert_eq!(
        svc.package_limits(own).await.unwrap().tier,
        PackageTier::Platform
    );
    assert_eq!(
        svc.package_limits(other).await.unwrap().tier,
        PackageTier::Standard
    );
    db.cleanup().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_zip_that_unpacks_to_more_than_100_times_its_size_is_refused_before_anything_is_stored() {
    let r = rig().await;
    let (app, owner) = fresh_app(&r, "Bomb").await;
    let zeros = vec![0u8; 8 * 1024 * 1024];
    let z = zip_of(&[
        ("index.html", b"x"),
        ("a.txt", b"fine"),
        ("bomb.txt", &zeros),
    ]);
    assert!(z.len() < 100 * 1024);
    let key = stage(&r, app, "bomb", z).await;
    let m = invalid(
        r.svc
            .ingest_stored_zip(app, owner, &key, None)
            .await
            .unwrap_err(),
    );
    assert!(m.contains("bomb.txt") && m.contains("100 times"), "{m}");
    assert!(r.spy.keys().await.is_empty(), "{:?}", r.spy.keys().await);
    assert_eq!(bundle_rows(&r, app).await, 0);
    r.db.cleanup().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn an_entry_that_inflates_past_its_declared_size_is_refused() {
    let r = rig().await;
    let (app, owner) = fresh_app(&r, "Liar").await;
    let key = stage(&r, app, "liar", lying_zip()).await;
    let m = invalid(
        r.svc
            .ingest_stored_zip(app, owner, &key, None)
            .await
            .unwrap_err(),
    );
    assert!(m.contains("z.js"), "{m}");
    r.db.cleanup().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_refused_package_leaves_no_objects_and_no_row() {
    // The liar passes the index, so a.js and index.html are stored before z.js is refused: all of it goes again.
    let r = rig().await;
    let (app, owner) = fresh_app(&r, "Half").await;
    let key = stage(&r, app, "half", lying_zip()).await;
    let before = r.spy.puts.lock().unwrap().len();
    let _ = invalid(
        r.svc
            .ingest_stored_zip(app, owner, &key, None)
            .await
            .unwrap_err(),
    );
    assert!(
        r.spy.puts.lock().unwrap().len() > before,
        "files were stored before the refusal"
    );
    assert!(r.spy.keys().await.is_empty(), "{:?}", r.spy.keys().await);
    assert_eq!(bundle_rows(&r, app).await, 0);
    assert!(r.svc.config(app).await.unwrap().bundles.is_empty());
    // Also with a file in parts when the refusal comes (the multipart upload is aborted, nothing is left).
    let r = rig_with(settings(512, 4096, 32768, 20_000), None, Some(64 * 1024)).await;
    let (app, owner) = fresh_app(&r, "HalfParts").await;
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    z.start_file("index.html", stored).unwrap();
    z.write_all(b"<p>").unwrap();
    z.start_file("big.txt", stored).unwrap();
    z.write_all(&noisy(300_000, 5)).unwrap();
    let mut b = z.finish().unwrap().into_inner();
    let at = b
        .windows(4)
        .rposition(|w| w == [0x50, 0x4b, 0x01, 0x02])
        .unwrap();
    b[at + 24..at + 28].copy_from_slice(&200_000u32.to_le_bytes());
    let key = stage(&r, app, "halfparts", b).await;
    let _ = invalid(
        r.svc
            .ingest_stored_zip(app, owner, &key, None)
            .await
            .unwrap_err(),
    );
    assert!(r.spy.keys().await.is_empty(), "{:?}", r.spy.keys().await);
    assert_eq!(bundle_rows(&r, app).await, 0);
    r.db.cleanup().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_file_over_8_mib_is_stored_in_parts() {
    // The threshold is lowered to 64 KiB for the test.
    let r = rig_with(settings(512, 4096, 32768, 20_000), None, Some(64 * 1024)).await;
    let (app, owner) = fresh_app(&r, "Parts").await;
    let big = noisy(300_000, 11);
    let z = zip_of(&[
        ("index.html", b"<p>hi"),
        ("big.txt", &big),
        ("tiny.txt", b"t"),
    ]);
    let key = stage(&r, app, "parts", z).await;
    let up = r
        .svc
        .ingest_stored_zip(app, owner, &key, None)
        .await
        .unwrap();
    let sha = up.bundle.sha256.clone();
    let big_key = format!("activities/{app}/{sha}/big.txt");
    assert_eq!(
        r.spy
            .multiparts
            .lock()
            .unwrap()
            .iter()
            .filter(|k| **k == big_key)
            .count(),
        1
    );
    assert!(
        r.spy.parts.load(Ordering::Relaxed) >= 5,
        "{}",
        r.spy.parts.load(Ordering::Relaxed)
    );
    // The small files used `put`; the big one's bytes are intact.
    assert!(
        r.spy
            .puts
            .lock()
            .unwrap()
            .iter()
            .any(|(k, n)| k.ends_with("/tiny.txt") && *n == 1)
    );
    assert!(
        !r.spy
            .puts
            .lock()
            .unwrap()
            .iter()
            .any(|(k, _)| *k == big_key)
    );
    assert_eq!(
        r.spy.inner.get(&big_key).await.unwrap().as_ref(),
        big.as_slice()
    );
    // The zip itself was copied in parts too.
    assert!(
        r.spy
            .multiparts
            .lock()
            .unwrap()
            .iter()
            .any(|k| k.ends_with(".zip") && !k.contains("uploads"))
    );
    assert!(
        r.spy
            .inner
            .get(&format!("activities/{app}/{sha}.zip"))
            .await
            .is_ok()
    );
    r.db.cleanup().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_check_reads_the_zip_in_bounded_pieces() {
    let r = rig().await;
    let (app, owner) = fresh_app(&r, "Bounded").await;
    // About 6 MiB of zip: more than one 4 MiB block.
    let files: Vec<(String, Vec<u8>)> = (0..4)
        .map(|i| (format!("d{i}.txt"), noisy(3_000_000, 20 + i as u64)))
        .collect();
    let mut entries: Vec<(&str, &[u8])> = vec![("index.html", b"<p>")];
    entries.extend(files.iter().map(|(n, d)| (n.as_str(), d.as_slice())));
    let z = zip_of(&entries);
    assert!(z.len() as u64 > CHECK_BLOCK, "{}", z.len());
    let key = stage(&r, app, "bounded", z).await;
    r.svc
        .ingest_stored_zip(app, owner, &key, None)
        .await
        .unwrap();
    let ranges = r.spy.ranges.lock().unwrap().clone();
    let staged: Vec<u64> = ranges
        .iter()
        .filter(|(k, _)| *k == key)
        .map(|(_, n)| *n)
        .collect();
    assert!(staged.len() >= 2, "{staged:?}");
    assert!(staged.iter().all(|n| *n <= CHECK_BLOCK), "{staged:?}");
    assert!(
        !r.spy.gets.lock().unwrap().contains(&key),
        "the staged zip was fetched whole"
    );
    assert!(
        r.spy.keys().await.iter().all(|k| !k.contains("/uploads/")),
        "staged object kept"
    );
    r.db.cleanup().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_code_hash_of_a_stored_package_equals_the_in_memory_one() {
    let r = rig().await;
    let (app, owner) = fresh_app(&r, "Hash").await;
    let files: Vec<(&str, Vec<u8>)> = vec![
        (
            "index.html",
            b"<script type=module src=main.js></script>".to_vec(),
        ),
        ("main.js", noisy(70_000, 1)),
        ("lib/util.mjs", noisy(5_000, 2)),
        ("engine.wasm", b"\0asm\x01\0\0\0".to_vec()),
        ("img/a.png", noisy(9_000, 3)),
        (
            "sideroom.json",
            br#"{"capabilities":["datachannel"]}"#.to_vec(),
        ),
    ];
    let refs: Vec<(&str, &[u8])> = files.iter().map(|(n, d)| (*n, d.as_slice())).collect();
    let z = zip_of(&refs);
    let expect = sandbox::code_sha256(refs.iter().copied()).unwrap();
    let key = stage(&r, app, "hash", z.clone()).await;
    let up = r
        .svc
        .ingest_stored_zip(app, owner, &key, None)
        .await
        .unwrap();
    let id: i64 = up.bundle.id.parse().unwrap();
    let stored: Option<String> =
        sqlx::query_scalar("SELECT code_sha256 FROM application_activity_bundles WHERE id = $1")
            .bind(id)
            .fetch_one(r.db.pool())
            .await
            .unwrap();
    assert_eq!(stored.as_deref(), Some(expect.as_str()));
    assert_eq!(up.bundle.capabilities, vec!["datachannel"]);
    // The in-memory unpack gives the same files and the same zip hash.
    let u = sr_activities::bundle::unpack(&z).unwrap();
    assert_eq!(u.sha256, up.bundle.sha256);
    assert_eq!(up.bundle.state, BundleState::Draft);
    assert_eq!(up.entries.len(), refs.len());
    r.db.cleanup().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn more_than_max_files_is_refused() {
    let r = rig_with(settings(512, 4096, 32768, 50), None, None).await;
    let (app, owner) = fresh_app(&r, "Many").await;
    let names: Vec<String> = (0..60).map(|i| format!("f{i}.txt")).collect();
    let mut v: Vec<(&str, &[u8])> = vec![("index.html", b"x")];
    v.extend(names.iter().map(|n| (n.as_str(), &b"x"[..])));
    let key = stage(&r, app, "many", zip_of(&v)).await;
    let m = invalid(
        r.svc
            .ingest_stored_zip(app, owner, &key, None)
            .await
            .unwrap_err(),
    );
    assert!(m.contains("50 entries") && m.contains("operator"), "{m}");
    // Exactly the limit passes.
    v.truncate(50);
    let key = stage(&r, app, "ok", zip_of(&v)).await;
    r.svc
        .ingest_stored_zip(app, owner, &key, None)
        .await
        .unwrap();
    r.db.cleanup().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_repeated_upload_returns_the_bundle_and_repairs_a_half_written_one() {
    let r = rig().await;
    let (app, owner) = fresh_app(&r, "Repair").await;
    let z = zip_of(&[("index.html", b"<p>"), ("app.js", b"1")]);
    let one = r.svc.upload_bundle(app, owner, z.clone()).await.unwrap();
    let key = format!("activities/{app}/{}/app.js", one.bundle.sha256);
    r.spy.inner.delete(&key).await.unwrap();
    let again = r.svc.upload_bundle(app, owner, z).await.unwrap();
    assert_eq!(again.bundle.id, one.bundle.id);
    assert!(
        r.spy.inner.get(&key).await.is_ok(),
        "the missing file was written again"
    );
    assert_eq!(bundle_rows(&r, app).await, 1);
    r.db.cleanup().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_collector_leaves_the_staging_area_and_a_check_in_flight_alone() {
    let r = rig().await;
    let (app, _) = fresh_app(&r, "Gc").await;
    let staged = stage(&r, app, "waiting", zip_of(&[("index.html", b"<p>")])).await;
    assert_eq!(r.svc.gc().await.unwrap(), 0);
    assert!(r.spy.inner.get(&staged).await.is_ok());
    r.db.cleanup().await.unwrap();
}
