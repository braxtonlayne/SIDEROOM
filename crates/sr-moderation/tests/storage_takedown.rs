//! Postgres-backed tests of BYOS-LEG-001: takedown and delisting for user-owned buckets (migration 1460). Run with a
//! scratch Postgres that may CREATE DATABASE:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-moderation --test storage_takedown -- --include-ignored --test-threads=1`

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicI64, Ordering},
};

use async_trait::async_trait;
use sqlx::PgPool;
use sr_admin::Caller;
use sr_core::PlatformPermissions as P;
use sr_db::{
    accounts::{self, NewAccount},
    storage_legal,
    testing::TestDb,
};
use sr_moderation::{
    ModError, Moderation, PolicySource, SubjectNotifier,
    byte_digest::DigestRecorded,
    model::{
        ItemKind, ItemState, NewItem, NewNotice, NoticeState, TakedownKind, TakedownPolicy, Who,
    },
};

static SEQ: AtomicI64 = AtomicI64::new(0);

fn uid() -> i64 {
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + SEQ.fetch_add(1, Ordering::Relaxed) % 1000
}

#[derive(Default)]
struct Dms(Mutex<Vec<(i64, String)>>);

#[async_trait]
impl SubjectNotifier for Dms {
    async fn system_dm(&self, to: i64, text: &str) -> bool {
        self.0.lock().unwrap().push((to, text.to_owned()));
        true
    }
}

struct Policy;

#[async_trait]
impl PolicySource for Policy {
    async fn takedown_policy(&self) -> TakedownPolicy {
        TakedownPolicy::default()
    }
}

async fn account(pool: &PgPool, name: &str) -> i64 {
    let id = uid();
    let ok = accounts::create_account(
        pool,
        &NewAccount {
            id,
            person_id: uid(),
            username: name,
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

async fn mount(pool: &PgPool, owner_kind: &str, owner_id: i64, label: &str) -> i64 {
    let id = uid();
    sqlx::query(
        "INSERT INTO storage_mounts (id, owner_kind, owner_id, label, provider, endpoint, bucket, access_key_id,
                                     secret_key_enc, key_hint)
         VALUES ($1, $2, $3, $4, 'garage', 'http://127.0.0.1:3900', 'shelf', 'GK0123456789abcd', '\\xdeadbeef', 'abcd')",
    )
    .bind(id)
    .bind(owner_kind)
    .bind(owner_id)
    .bind(label)
    .execute(pool)
    .await
    .unwrap();
    id
}

struct World {
    db: TestDb,
    m: Moderation,
    dms: Arc<Dms>,
    staff_id: i64,
    owner: i64,
}

async fn world() -> World {
    let db = TestDb::new().await.expect("test db");
    let pool = db.pool().clone();
    let dms = Arc::new(Dms::default());
    let m = Moderation::new(pool.clone(), Arc::new(uid))
        .with_notifier(dms.clone())
        .with_policy(Arc::new(Policy));
    let staff_id = account(&pool, "tkdstaff").await;
    let owner = account(&pool, "bucketowner").await;
    World {
        db,
        m,
        dms,
        staff_id,
        owner,
    }
}

impl World {
    fn pool(&self) -> &PgPool {
        self.db.pool()
    }

    fn caller(&self) -> Caller {
        Caller {
            user_id: self.staff_id,
            permissions: P::MANAGE_TAKEDOWNS,
            is_owner: false,
            ip: Some("127.0.0.1".into()),
        }
    }

    async fn notice_with(&self, mount: i64, key: &str) -> i64 {
        let c = self.caller();
        let who = Who::Staff(&c);
        let n = self
            .m
            .create_notice(
                &who,
                &NewNotice {
                    kind: TakedownKind::Copyright,
                    notifier_name: "Rights Holder Ltd".into(),
                    notifier_contact: "legal@rights.example".into(),
                    legal_basis: "Copyright in the film".into(),
                    locations: vec![],
                    original_text: "Please remove it.".into(),
                    received_at_ms: None,
                },
            )
            .await
            .unwrap();
        self.m
            .add_item(
                &who,
                n.summary.id,
                &NewItem {
                    kind: ItemKind::StorageObject,
                    id: mount,
                    key: Some(key.into()),
                },
            )
            .await
            .unwrap();
        n.summary.id
    }
}

fn keys(k: &[&str]) -> Vec<String> {
    k.iter().map(|s| (*s).to_owned()).collect()
}

#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn a_notice_against_an_object_delists_it_and_a_counter_notice_restores_it() {
    let w = world().await;
    let mnt = mount(w.pool(), "user", w.owner, "My shelf").await;
    let id = w.notice_with(mnt, "films/leak.mkv").await;
    let probe = keys(&["films/leak.mkv", "films/other.mkv"]);

    // Named but not upheld: still listed.
    assert!(
        storage_legal::delisted_keys(w.pool(), mnt, &probe)
            .await
            .unwrap()
            .is_empty()
    );
    let c = w.caller();
    let who = Who::Staff(&c);
    let out = w.m.uphold_notice(&who, id, Some("valid")).await.unwrap();
    assert_eq!(out.items_disabled, 1);
    assert_eq!(out.notified, 1);

    // Delisted: that key only, on that mount only; nothing was held. The one block is the mount's own CDN path,
    // so a /cdn/mount link minted before the takedown stops too (wave 53).
    assert_eq!(
        storage_legal::delisted_keys(w.pool(), mnt, &probe)
            .await
            .unwrap(),
        keys(&["films/leak.mkv"])
    );
    assert!(
        storage_legal::delisted_keys(w.pool(), mnt + 1, &probe)
            .await
            .unwrap()
            .is_empty()
    );
    let blocked: Vec<(String, String)> =
        sqlx::query_as("SELECT kind, value FROM blocked_objects WHERE notice_id = $1")
            .bind(id)
            .fetch_all(w.pool())
            .await
            .unwrap();
    assert_eq!(
        blocked,
        vec![("key".to_owned(), format!("mount/{mnt}/films/leak.mkv"))]
    );
    // The owner got the uploader's message (D-336 (c)) naming the object.
    {
        let dms = w.dms.0.lock().unwrap();
        assert_eq!(dms.len(), 1);
        assert_eq!(dms[0].0, w.owner);
        assert!(dms[0].1.contains("films/leak.mkv"));
    }
    // The bucket is the member's: the mount is untouched.
    let status: String = sqlx::query_scalar("SELECT status FROM storage_mounts WHERE id = $1")
        .bind(mnt)
        .fetch_one(w.pool())
        .await
        .unwrap();
    assert_eq!(status, "active");

    // Counter-notice, and the existing clock restores it.
    let d =
        w.m.record_counter_notice(&who, id, "It is mine")
            .await
            .unwrap();
    assert_eq!(d.summary.state, NoticeState::CounterNoticed);
    assert_eq!(
        storage_legal::delisted_keys(w.pool(), mnt, &probe)
            .await
            .unwrap()
            .len(),
        1,
        "stays delisted while the wait runs"
    );
    let due = d.summary.restore_after_ms.unwrap() + 1000;
    assert_eq!(w.m.run_due_restores(due).await.unwrap(), vec![id]);
    let after = w.m.get_notice(&who, id).await.unwrap();
    assert!(after.items.iter().all(|i| i.state == ItemState::Restored));
    assert_eq!(after.items[0].object_key.as_deref(), Some("films/leak.mkv"));
    let blocked: i64 =
        sqlx::query_scalar("SELECT count(*) FROM blocked_objects WHERE notice_id = $1")
            .bind(id)
            .fetch_one(w.pool())
            .await
            .unwrap();
    assert_eq!(
        blocked, 0,
        "the restore lifts the CDN block with the delisting"
    );
    assert!(
        storage_legal::delisted_keys(w.pool(), mnt, &probe)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn storage_items_are_checked_on_the_way_in() {
    let w = world().await;
    let mnt = mount(w.pool(), "user", w.owner, "My shelf").await;
    let c = w.caller();
    let who = Who::Staff(&c);
    let n = w.notice_with(mnt, "a/b.mkv").await;
    let add = |kind, id, key: Option<&str>| {
        let m = &w.m;
        let who = &who;
        let key = key.map(str::to_owned);
        async move { m.add_item(who, n, &NewItem { kind, id, key }).await }
    };
    // A key for a storage object and for nothing else.
    assert!(matches!(
        add(ItemKind::StorageObject, mnt, None).await,
        Err(ModError::Invalid(_))
    ));
    assert!(matches!(
        add(ItemKind::Message, mnt, Some("a")).await,
        Err(ModError::Invalid(_))
    ));
    for bad in ["", "/abs", "dir/", "a/../b", "a//b", "a\u{0}b"] {
        assert!(
            matches!(
                add(ItemKind::StorageObject, mnt, Some(bad)).await,
                Err(ModError::Invalid(_))
            ),
            "{bad:?}"
        );
    }
    // No such mount; the instance bucket is not a member's.
    assert!(matches!(
        add(ItemKind::StorageObject, mnt + 7, Some("a")).await,
        Err(ModError::NotFound(_))
    ));
    let platform = mount(w.pool(), "platform", 0, "Instance").await;
    assert!(matches!(
        add(ItemKind::StorageObject, platform, Some("a")).await,
        Err(ModError::Invalid(_))
    ));
    // The same key twice is refused; another key of the same mount is fine.
    assert!(matches!(
        add(ItemKind::StorageObject, mnt, Some("a/b.mkv")).await,
        Err(ModError::Conflict(_))
    ));
    add(ItemKind::StorageObject, mnt, Some("a/c.mkv"))
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn a_whole_mount_can_be_delisted_and_relisted_and_the_repo_cannot_undo_it() {
    let w = world().await;
    let mnt = mount(w.pool(), "user", w.owner, "My shelf").await;
    let c = w.caller();
    let who = Who::Staff(&c);

    // Needs the bit and a reason; the instance bucket is refused.
    let nobody = Caller {
        permissions: P::MANAGE_CASES,
        ..w.caller()
    };
    assert_eq!(
        w.m.delist_mount(&Who::Staff(&nobody), mnt, Some("x"))
            .await
            .unwrap_err(),
        ModError::MissingPermission("MANAGE_TAKEDOWNS")
    );
    assert_eq!(
        w.m.delist_mount(&who, mnt, None).await.unwrap_err(),
        ModError::ReasonRequired
    );
    let platform = mount(w.pool(), "platform", 0, "Instance").await;
    assert!(matches!(
        w.m.delist_mount(&who, platform, Some("x")).await,
        Err(ModError::NotFound(_))
    ));

    let m =
        w.m.delist_mount(&who, mnt, Some("Notice TK-1 names the whole bucket"))
            .await
            .unwrap();
    assert_eq!(m.status, "delisted");
    assert_eq!(w.dms.0.lock().unwrap().len(), 1);
    assert!(
        w.dms.0.lock().unwrap()[0]
            .1
            .contains("Notice TK-1 names the whole bucket")
    );
    let rows = storage_legal::admin_list(
        w.pool(),
        Some(("user", w.owner)),
        Some("delisted"),
        50,
        None,
    )
    .await
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].owner_name, "bucketowner");
    assert_eq!(rows[0].delist_reason, "Notice TK-1 names the whole bucket");
    let logged: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM staff_access_log WHERE action = 'storage.mount.delist'",
    )
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(logged, 1);

    // A disabled mount that was delisted comes back disabled, not active.
    sqlx::query("UPDATE storage_mounts SET status = 'disabled' WHERE id = $1")
        .bind(mnt)
        .execute(w.pool())
        .await
        .unwrap();
    w.m.delist_mount(&who, mnt, Some("again")).await.unwrap();
    let back =
        w.m.relist_mount(&who, mnt, Some("Counter-notice"))
            .await
            .unwrap();
    assert_eq!(back.status, "disabled");
    assert!(matches!(
        w.m.relist_mount(&who, mnt, Some("Counter-notice")).await,
        Err(ModError::Conflict(_))
    ));
    assert_eq!(w.dms.0.lock().unwrap().len(), 3);
}

#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn export_lists_mounts_without_keys_and_erasure_wipes_the_secret() {
    let w = world().await;
    let friend = account(w.pool(), "shelffriend").await;
    let mnt = mount(w.pool(), "user", w.owner, "My shelf").await;
    let theirs = mount(w.pool(), "user", friend, "Friend shelf").await;
    sqlx::query(
        "INSERT INTO storage_mount_shares (mount_id, friend_id, cinema) VALUES ($1, $2, TRUE)",
    )
    .bind(mnt)
    .bind(friend)
    .execute(w.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO storage_mount_shares (mount_id, friend_id, music) VALUES ($1, $2, TRUE)",
    )
    .bind(theirs)
    .bind(w.owner)
    .execute(w.pool())
    .await
    .unwrap();
    let id = w.notice_with(mnt, "secret/name.mkv").await;
    let c = w.caller();
    w.m.uphold_notice(&Who::Staff(&c), id, Some("valid"))
        .await
        .unwrap();

    let v = storage_legal::export_for(w.pool(), w.owner).await.unwrap();
    let text = v.to_string();
    assert_eq!(v["mounts"].as_array().unwrap().len(), 1);
    assert_eq!(v["mounts"][0]["label"], "My shelf");
    assert_eq!(v["mounts"][0]["bucket"], "shelf");
    assert_eq!(v["mounts"][0]["delisted_objects"], 1);
    assert_eq!(v["mounts"][0]["shares"].as_array().unwrap().len(), 1);
    assert_eq!(v["shared_with_you"].as_array().unwrap().len(), 1);
    // No key id, no secret, no hint, no object key.
    for needle in [
        "GK0123456789abcd",
        "deadbeef",
        "abcd\"",
        "secret/name.mkv",
        "secret_key",
    ] {
        assert!(!text.contains(needle), "{needle} leaked: {text}");
    }

    let mut tx = w.pool().begin().await.unwrap();
    let erased = storage_legal::erase_for(&mut tx, w.owner).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(erased.mounts, 1);
    assert_eq!(erased.shares, 2);
    let (enc_len, key_id, deleted): (i32, String, bool) = sqlx::query_as(
        "SELECT octet_length(secret_key_enc), access_key_id, deleted_at IS NOT NULL FROM storage_mounts WHERE id = $1",
    )
    .bind(mnt)
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!((enc_len, key_id.as_str(), deleted), (0, "erased", true));
    // The friend's own mount is untouched.
    let enc_len: i32 =
        sqlx::query_scalar("SELECT octet_length(secret_key_enc) FROM storage_mounts WHERE id = $1")
            .bind(theirs)
            .fetch_one(w.pool())
            .await
            .unwrap();
    assert_eq!(enc_len, 4);
    assert_eq!(
        storage_legal::export_for(w.pool(), w.owner).await.unwrap()["mounts"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

async fn person_of(pool: &PgPool, account: i64) -> i64 {
    sqlx::query_scalar("SELECT person_id FROM accounts WHERE id = $1")
        .bind(account)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// P28-LEG-1: on a server's bucket the strike goes to the uploader the ledger knows (the server owner is told,
/// without a strike), else to the server owner.
#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn a_server_bucket_strikes_the_uploader_when_the_ledger_knows_them_else_the_owner() {
    let w = world().await;
    let uploader = account(w.pool(), "shelfuploader").await;
    let guild = uid();
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'Club', $2)")
        .bind(guild)
        .bind(w.owner)
        .execute(w.pool())
        .await
        .unwrap();
    let mnt = mount(w.pool(), "guild", guild, "Server shelf").await;
    // An older aborted row by the owner and the latest completed one by the uploader: the completed one wins.
    for (who, state) in [(w.owner, "aborted"), (uploader, "completed")] {
        sqlx::query(
            "INSERT INTO storage_mount_uploads (id, mount_id, user_id, object_key, s3_upload_id, size_bytes, \
             part_size, part_count, state) VALUES ($1, $2, $3, 'films/up.mkv', 'u1', 10, 16777216, 1, $4)",
        )
        .bind(uid())
        .bind(mnt)
        .bind(who)
        .bind(state)
        .execute(w.pool())
        .await
        .unwrap();
    }
    let c = w.caller();
    let who = Who::Staff(&c);

    // With a ledger row: the uploader is the subject and is struck; the server owner is told too.
    let id = w.notice_with(mnt, "films/up.mkv").await;
    let out = w.m.uphold_notice(&who, id, Some("valid")).await.unwrap();
    assert_eq!(out.notified, 1);
    let (subject, struck, label): (Option<i64>, Option<i64>, String) = sqlx::query_as(
        "SELECT subject_account_id, subject_person_id, label FROM takedown_items WHERE notice_id = $1",
    )
    .bind(id)
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(subject, Some(uploader));
    assert_eq!(struck, Some(person_of(w.pool(), uploader).await));
    assert!(
        label.contains(&format!("(uploaded by {uploader})")),
        "{label}"
    );
    {
        let dms = w.dms.0.lock().unwrap();
        let to: Vec<i64> = dms.iter().map(|d| d.0).collect();
        assert_eq!(to.len(), 2, "{to:?}");
        assert!(to.contains(&uploader) && to.contains(&w.owner));
        assert!(dms.iter().all(|d| d.1.contains("films/up.mkv")));
        assert_eq!(dms[0].1, dms[1].1, "the owner gets the same text");
    }
    w.dms.0.lock().unwrap().clear();

    // Without a ledger row: the server owner is the subject and is struck, and told once.
    let id2 = w.notice_with(mnt, "films/elsewhere.mkv").await;
    w.m.uphold_notice(&who, id2, Some("valid")).await.unwrap();
    let (subject, struck, label): (Option<i64>, Option<i64>, String) = sqlx::query_as(
        "SELECT subject_account_id, subject_person_id, label FROM takedown_items WHERE notice_id = $1",
    )
    .bind(id2)
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(subject, Some(w.owner));
    assert_eq!(struck, Some(person_of(w.pool(), w.owner).await));
    assert!(label.ends_with("(server bucket, owner)"), "{label}");
    let dms = w.dms.0.lock().unwrap();
    assert_eq!(dms.len(), 1);
    assert_eq!(dms[0].0, w.owner);
}

/// FU-2 (FU-STU-6): a shelf delisting counts toward the repeat-infringer counter only through the takedown notice it
/// belongs to. A bare staff delisting (no notice) keeps its `shelf_delist_strikes` row and the owner is told, but it
/// is a policy act, not a copyright strike: nothing enters the counter, which reads upheld notices by person.
#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn a_bare_shelf_delisting_never_counts_toward_the_repeat_infringer_counter() {
    let w = world().await;
    let mnt = mount(w.pool(), "user", w.owner, "My shelf").await;
    let hash: String = sqlx::query_scalar("SELECT shelf_object_hash('films/bare.mkv')")
        .fetch_one(w.pool())
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO shelf_objects (shelf_id, object_hash, path, kind) VALUES ($1, $2, 'films/bare.mkv', 'cinema')",
    )
    .bind(mnt)
    .bind(&hash)
    .execute(w.pool())
    .await
    .unwrap();
    let c = w.caller();
    let who = Who::Staff(&c);

    // The bare delisting: no notice behind it.
    let d =
        w.m.delist_shelf_object(&who, mnt, &hash, None, Some("policy: not for this server"))
            .await
            .unwrap();
    assert_eq!(d.subject_account, Some(w.owner));
    let (shelf_strikes, linked): (i64, i64) = sqlx::query_as(
        "SELECT count(*), count(notice_id) FROM shelf_delist_strikes WHERE subject_account = $1",
    )
    .bind(w.owner)
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(
        (shelf_strikes, linked),
        (1, 0),
        "its own row, tied to no notice"
    );
    for table in ["takedown_notices", "takedown_items"] {
        let n: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
                .fetch_one(w.pool())
                .await
                .unwrap();
        assert_eq!(
            n, 0,
            "{table}: the delisting wrote nothing the counter reads"
        );
    }

    // One upheld notice against the same owner is exactly one strike: the delisting added none.
    let id = w.notice_with(mnt, "films/leak.mkv").await;
    w.m.uphold_notice(&who, id, Some("valid")).await.unwrap();
    let detail = w.m.get_notice(&who, id).await.unwrap();
    let owner_person = person_of(w.pool(), w.owner).await;
    let count = detail
        .strikes
        .iter()
        .find(|s| s.person_id == owner_person)
        .expect("the owner's person is counted");
    assert_eq!(count.strikes, 1, "only the upheld notice counts");
    assert_eq!(count.counted.len(), 1);
    assert_eq!(count.counted[0].id, id);
    assert_eq!(
        count.review_case_id, None,
        "one strike is below the review threshold"
    );
}

/// (mount, key, state, subject, derived_from) of a takedown item.
type DerivedRow = (i64, String, String, Option<i64>, Option<i64>);

const LEAK_TAG: &str = "9e107d9d372bb6826bd81d3542a419d6";
const LEAK_SIZE: i64 = 734_003_200;

/// A catalog row of the indexer with the fingerprint of its bytes (migration 2450).
async fn catalogued(pool: &PgPool, mount: i64, key: &str, tag: &str, size: i64) {
    sqlx::query(
        "INSERT INTO storage_objects (id, mount_id, key, size_bytes, kind, class, content_tag)
         VALUES ($1, $2, $3, $4, 'cinema', 'movie', $5)",
    )
    .bind(uid())
    .bind(mount)
    .bind(key)
    .bind(size)
    .bind(tag)
    .execute(pool)
    .await
    .unwrap();
}

fn listed(objects: &[(&str, &str, i64)]) -> Vec<(String, String, i64)> {
    objects
        .iter()
        .map(|(k, t, s)| ((*k).to_owned(), (*t).to_owned(), *s))
        .collect()
}

/// SEC-AUD-048 (D-571): a takedown follows the bytes. The upheld item records the fingerprint from the catalog;
/// the same bytes listed under another name, or in another member's bucket, are delisted under the same notice
/// (no strike, no message); the notice's restore lifts them with the rest; the path block stays.
#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn a_storage_takedown_follows_the_bytes_and_the_restore_lifts_the_matches() {
    let w = world().await;
    let other = account(w.pool(), "reuploader").await;
    let mnt = mount(w.pool(), "user", w.owner, "My shelf").await;
    let theirs = mount(w.pool(), "user", other, "Their shelf").await;
    catalogued(w.pool(), mnt, "films/leak.mkv", LEAK_TAG, LEAK_SIZE).await;
    let ids = uid;
    let probe = listed(&[
        ("x/renamed.mkv", LEAK_TAG, LEAK_SIZE),
        // The same digest with another size, and another digest with the same size, are other bytes.
        ("x/short.mkv", LEAK_TAG, LEAK_SIZE - 1),
        ("x/other.mkv", "0123456789abcdef0123456789abcdef", LEAK_SIZE),
    ]);

    // Named but not upheld: nothing matches yet.
    let id = w.notice_with(mnt, "films/leak.mkv").await;
    assert!(
        storage_legal::delist_byte_matches(w.pool(), &ids, theirs, &probe)
            .await
            .unwrap()
            .is_empty()
    );

    let c = w.caller();
    let who = Who::Staff(&c);
    w.m.uphold_notice(&who, id, Some("valid")).await.unwrap();
    let (hash, size): (Option<String>, Option<i64>) = sqlx::query_as(
        "SELECT object_hash, object_size FROM takedown_items WHERE notice_id = $1 AND object_key = 'films/leak.mkv'",
    )
    .bind(id)
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(
        hash.as_deref(),
        Some(LEAK_TAG),
        "the upheld item records the bytes"
    );
    assert_eq!(size, Some(LEAK_SIZE));
    let dms_before = w.dms.0.lock().unwrap().len();

    // Another member's bucket: only the same bytes match.
    let got = storage_legal::delist_byte_matches(w.pool(), &ids, theirs, &probe)
        .await
        .unwrap();
    assert_eq!(got, vec!["x/renamed.mkv".to_owned()]);
    let all = ["x/renamed.mkv", "x/short.mkv", "x/other.mkv"].map(str::to_owned);
    assert_eq!(
        storage_legal::delisted_keys(w.pool(), theirs, &all)
            .await
            .unwrap(),
        vec!["x/renamed.mkv".to_owned()],
        "every reader of the register now delists it"
    );
    // The owner's own bucket under a new name, too; asking again records nothing twice.
    let renamed = listed(&[("films/renamed.mkv", LEAK_TAG, LEAK_SIZE)]);
    for _ in 0..2 {
        assert_eq!(
            storage_legal::delist_byte_matches(w.pool(), &ids, mnt, &renamed)
                .await
                .unwrap(),
            vec!["films/renamed.mkv".to_owned()]
        );
    }
    let derived: Vec<DerivedRow> = sqlx::query_as(
        "SELECT target_id, object_key, state, subject_account_id, derived_from FROM takedown_items
         WHERE notice_id = $1 AND derived_from IS NOT NULL ORDER BY object_key",
    )
    .bind(id)
    .fetch_all(w.pool())
    .await
    .unwrap();
    assert_eq!(derived.len(), 2, "{derived:?}");
    let source: i64 = sqlx::query_scalar(
        "SELECT id FROM takedown_items WHERE notice_id = $1 AND derived_from IS NULL",
    )
    .bind(id)
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(
        derived[0],
        (
            mnt,
            "films/renamed.mkv".into(),
            "disabled".into(),
            Some(w.owner),
            Some(source)
        )
    );
    assert_eq!(
        derived[1],
        (
            theirs,
            "x/renamed.mkv".into(),
            "disabled".into(),
            Some(other),
            Some(source)
        )
    );
    // A match of a match still points at the item staff upheld.
    let again = listed(&[("y/third.mkv", LEAK_TAG, LEAK_SIZE)]);
    storage_legal::delist_byte_matches(w.pool(), &ids, theirs, &again)
        .await
        .unwrap();
    let from: Option<i64> = sqlx::query_scalar(
        "SELECT derived_from FROM takedown_items WHERE notice_id = $1 AND object_key = 'y/third.mkv'",
    )
    .bind(id)
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(from, Some(source));
    // The CDN paths are blocked under the notice, the timeline says so, and nobody was struck or messaged.
    let mut blocked: Vec<String> =
        sqlx::query_scalar("SELECT value FROM blocked_objects WHERE notice_id = $1 ORDER BY value")
            .bind(id)
            .fetch_all(w.pool())
            .await
            .unwrap();
    blocked.sort();
    let mut want = vec![
        format!("mount/{mnt}/films/leak.mkv"),
        format!("mount/{mnt}/films/renamed.mkv"),
        format!("mount/{theirs}/x/renamed.mkv"),
        format!("mount/{theirs}/y/third.mkv"),
    ];
    want.sort();
    assert_eq!(blocked, want);
    let events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM takedown_events WHERE notice_id = $1 AND kind = 'byte_match'",
    )
    .bind(id)
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(events, 3);
    let struck: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT subject_person_id) FROM takedown_items WHERE notice_id = $1",
    )
    .bind(id)
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(struck, 1, "only the owner staff named");
    assert_eq!(w.dms.0.lock().unwrap().len(), dms_before);

    // The restore lifts the matches with the rest, and the bytes no longer match.
    w.m.restore_notice(&who, id, Some("counter-notice accepted"))
        .await
        .unwrap();
    let standing: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM takedown_items WHERE notice_id = $1 AND state <> 'restored'",
    )
    .bind(id)
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(standing, 0);
    assert!(
        storage_legal::delisted_keys(w.pool(), theirs, &all)
            .await
            .unwrap()
            .is_empty()
    );
    let blocks: i64 =
        sqlx::query_scalar("SELECT count(*) FROM blocked_objects WHERE notice_id = $1")
            .bind(id)
            .fetch_one(w.pool())
            .await
            .unwrap();
    assert_eq!(blocks, 0);
    assert!(
        storage_legal::delist_byte_matches(w.pool(), &ids, theirs, &probe)
            .await
            .unwrap()
            .is_empty()
    );
}

/// An object the indexer never catalogued (or whose provider gives no content digest) is taken down by path only.
#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn an_uncatalogued_object_is_taken_down_by_path_only() {
    let w = world().await;
    let mnt = mount(w.pool(), "user", w.owner, "My shelf").await;
    let id = w.notice_with(mnt, "notes/scan.pdf").await;
    let c = w.caller();
    w.m.uphold_notice(&Who::Staff(&c), id, Some("valid"))
        .await
        .unwrap();
    let hash: Option<String> =
        sqlx::query_scalar("SELECT object_hash FROM takedown_items WHERE notice_id = $1")
            .bind(id)
            .fetch_one(w.pool())
            .await
            .unwrap();
    assert_eq!(hash, None);
    assert_eq!(
        storage_legal::delisted_keys(w.pool(), mnt, &["notes/scan.pdf".to_owned()])
            .await
            .unwrap()
            .len(),
        1
    );
}

// ---- SEC-AUD-053 / SEC-AUD-054 (D-583, D-584) ----------------------------------------------------------------------

const SHA: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

const WORKER: &str = "storage.takedown_digest";

async fn item_of(pool: &PgPool, notice: i64, key: &str) -> i64 {
    sqlx::query_scalar("SELECT id FROM takedown_items WHERE notice_id = $1 AND object_key = $2")
        .bind(notice)
        .bind(key)
        .fetch_one(pool)
        .await
        .unwrap()
}

type Fp = (Option<String>, Option<String>, Option<i64>);

async fn fp(pool: &PgPool, id: i64, key: &str) -> Fp {
    sqlx::query_as::<_, Fp>(
        "SELECT object_hash, object_sha256, object_size FROM takedown_items
         WHERE notice_id = $1 AND object_key = $2",
    )
    .bind(id)
    .bind(key)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// D-588: the uphold reads nothing. Each item records the catalog tag and size (an object without a tag has no
/// fingerprint yet) and one `storage.takedown_digest` job per item is queued; the job's result is written by
/// `record_digest`: the sha256 beside the tag and size when the size is the one read, nothing at all (D-591) when it
/// is not, the sha256 and size for an object the catalog gave no tag; a second result is
/// ignored.
#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn the_uphold_queues_one_digest_job_per_item_and_the_job_adds_the_sha256() {
    let w = world().await;
    let mnt = mount(w.pool(), "user", w.owner, "My shelf").await;
    catalogued(w.pool(), mnt, "films/read.mkv", LEAK_TAG, 8).await;
    catalogued(w.pool(), mnt, "films/stale.mkv", LEAK_TAG, LEAK_SIZE).await;
    // An object of the catalog without a content tag (a provider whose tag says nothing about the bytes).
    sqlx::query(
        "INSERT INTO storage_objects (id, mount_id, key, size_bytes, kind, class)
         VALUES ($1, $2, 'films/untagged.mkv', 8, 'cinema', 'movie')",
    )
    .bind(uid())
    .bind(mnt)
    .execute(w.pool())
    .await
    .unwrap();
    let c = w.caller();
    let who = Who::Staff(&c);
    let id = w.notice_with(mnt, "films/read.mkv").await;
    for key in ["films/stale.mkv", "films/untagged.mkv"] {
        w.m.add_item(
            &who,
            id,
            &NewItem {
                kind: ItemKind::StorageObject,
                id: mnt,
                key: Some(key.into()),
            },
        )
        .await
        .unwrap();
    }
    w.m.uphold_notice(&who, id, Some("valid")).await.unwrap();
    assert_eq!(
        fp(w.pool(), id, "films/read.mkv").await,
        (Some(LEAK_TAG.into()), None, Some(8)),
        "tag and size at once, no sha256 yet"
    );
    assert_eq!(
        fp(w.pool(), id, "films/untagged.mkv").await,
        (None, None, None),
        "no catalog tag: nothing to record until the job has read it"
    );
    let queued: Vec<(i64, i32)> = sqlx::query_as(
        "SELECT (payload ->> 'item_id')::bigint AS item, max_attempts FROM jobs WHERE kind = $1
           AND (payload ->> 'item_id')::bigint IN (SELECT id FROM takedown_items WHERE notice_id = $2)
         ORDER BY 1",
    )
    .bind(WORKER)
    .bind(id)
    .fetch_all(w.pool())
    .await
    .unwrap();
    let mut items = vec![
        item_of(w.pool(), id, "films/read.mkv").await,
        item_of(w.pool(), id, "films/stale.mkv").await,
        item_of(w.pool(), id, "films/untagged.mkv").await,
    ];
    items.sort_unstable();
    assert_eq!(
        queued.iter().map(|q| q.0).collect::<Vec<_>>(),
        items,
        "one job per item"
    );
    assert!(queued.iter().all(|q| q.1 == 14));

    let read = item_of(w.pool(), id, "films/read.mkv").await;
    assert_eq!(
        w.m.record_digest(read, SHA, 8).await.unwrap(),
        DigestRecorded::Written { copied: 0 }
    );
    assert_eq!(
        fp(w.pool(), id, "films/read.mkv").await,
        (Some(LEAK_TAG.into()), Some(SHA.into()), Some(8)),
        "tag, sha256 and size"
    );
    let stale = item_of(w.pool(), id, "films/stale.mkv").await;
    let before = fp(w.pool(), id, "films/stale.mkv").await;
    assert!(
        before.0.is_some() && before.1.is_none() && before.2 != Some(8),
        "{before:?}"
    );
    assert_eq!(
        w.m.record_digest(stale, SHA, 8).await.unwrap(),
        DigestRecorded::SizeChanged {
            recorded: before.2.unwrap(),
            read: 8
        }
    );
    assert_eq!(
        fp(w.pool(), id, "films/stale.mkv").await,
        before,
        "D-591: the file is not the size the catalog saw: nothing is written, the item keeps tag and size"
    );
    assert!(
        w.m.digest_target(stale).await.unwrap().is_some(),
        "still waiting for a sha256 of the bytes staff upheld"
    );
    let untagged = item_of(w.pool(), id, "films/untagged.mkv").await;
    assert_eq!(
        w.m.record_digest(untagged, SHA, 8).await.unwrap(),
        DigestRecorded::Written { copied: 0 }
    );
    assert_eq!(
        fp(w.pool(), id, "films/untagged.mkv").await,
        (None, Some(SHA.into()), Some(8)),
        "no catalog tag: the sha256 and the size read"
    );
    // The first answer stands.
    assert_eq!(
        w.m.record_digest(read, &"1".repeat(64), 9).await.unwrap(),
        DigestRecorded::Skipped
    );
    assert!(w.m.digest_target(read).await.unwrap().is_none());
}

/// D-588: an item restored before the job runs is skipped (nothing is written, the job has nothing to read), and the
/// sha256 is copied to the items derived from the upheld one whose size is the bytes read.
#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn a_restored_item_is_skipped_and_the_sha256_reaches_the_derived_items() {
    let w = world().await;
    let other = account(w.pool(), "reuploader").await;
    let mnt = mount(w.pool(), "user", w.owner, "My shelf").await;
    let theirs = mount(w.pool(), "user", other, "Their shelf").await;
    catalogued(w.pool(), mnt, "films/leak.mkv", LEAK_TAG, 8).await;
    catalogued(w.pool(), mnt, "films/restored.mkv", LEAK_TAG, 8).await;
    let c = w.caller();
    let who = Who::Staff(&c);
    let id = w.notice_with(mnt, "films/leak.mkv").await;
    w.m.uphold_notice(&who, id, Some("valid")).await.unwrap();
    let item = item_of(w.pool(), id, "films/leak.mkv").await;
    let target = w.m.digest_target(item).await.unwrap().expect("a target");
    assert_eq!(
        (target.mount_id, target.key.as_str(), target.notice_id),
        (mnt, "films/leak.mkv", id)
    );
    // A byte match by tag made before the sha256 exists, then the job's result.
    storage_legal::delist_byte_matches(
        w.pool(),
        &uid,
        theirs,
        &listed(&[("x/copy.mkv", LEAK_TAG, 8)]),
    )
    .await
    .unwrap();
    assert_eq!(fp(w.pool(), id, "x/copy.mkv").await.1, None);
    assert_eq!(
        w.m.record_digest(item, SHA, 8).await.unwrap(),
        DigestRecorded::Written { copied: 1 }
    );
    assert_eq!(
        fp(w.pool(), id, "x/copy.mkv").await,
        (Some(LEAK_TAG.into()), Some(SHA.into()), Some(8)),
        "the derived item carries the sha256 too"
    );

    // Restored before the job ran: skipped.
    let id2 = w.notice_with(mnt, "films/restored.mkv").await;
    w.m.uphold_notice(&who, id2, Some("valid")).await.unwrap();
    let item2 = item_of(w.pool(), id2, "films/restored.mkv").await;
    w.m.restore_notice(&who, id2, Some("counter-notice accepted"))
        .await
        .unwrap();
    assert!(w.m.digest_target(item2).await.unwrap().is_none());
    assert_eq!(
        w.m.record_digest(item2, SHA, 8).await.unwrap(),
        DigestRecorded::Skipped
    );
    assert_eq!(fp(w.pool(), id2, "films/restored.mkv").await.1, None);
}

/// D-583: the sieve and the match on the sha256. A different tag with equal bytes is delisted; equal size with other
/// bytes is not; the sieve names only sizes of a standing takedown that recorded a sha256, and says what is stored.
#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn a_different_tag_with_the_same_bytes_matches_and_the_same_size_alone_does_not() {
    let w = world().await;
    let other = account(w.pool(), "reuploader").await;
    let mnt = mount(w.pool(), "user", w.owner, "My shelf").await;
    let theirs = mount(w.pool(), "user", other, "Their shelf").await;
    catalogued(w.pool(), mnt, "films/leak.mkv", LEAK_TAG, 8).await;
    let id = w.notice_with(mnt, "films/leak.mkv").await;
    let c = w.caller();
    let who = Who::Staff(&c);
    let ids = uid;
    let multipart = "0123456789abcdef0123456789abcdef-12".to_owned();
    let cands = |objs: &[(&str, Option<&str>, i64)]| {
        objs.iter()
            .map(|(k, t, s)| ((*k).to_owned(), t.map(str::to_owned), *s))
            .collect::<Vec<_>>()
    };
    let probe_objs = [
        ("a/film.mkv", Some(multipart.as_str()), 8),
        ("a/notag.mkv", None, 8),
        ("a/long.mkv", Some(multipart.as_str()), 9),
    ];
    // Not upheld yet: no sieve.
    assert!(
        storage_legal::hash_candidates(w.pool(), theirs, &cands(&probe_objs))
            .await
            .unwrap()
            .is_empty()
    );
    w.m.uphold_notice(&who, id, Some("valid")).await.unwrap();
    // Until the job has read the object only the tag matches: no sieve yet (D-588).
    assert!(
        storage_legal::hash_candidates(w.pool(), theirs, &cands(&probe_objs))
            .await
            .unwrap()
            .is_empty()
    );
    let item = item_of(w.pool(), id, "films/leak.mkv").await;
    w.m.record_digest(item, SHA, 8).await.unwrap();
    let mut got = storage_legal::hash_candidates(w.pool(), theirs, &cands(&probe_objs))
        .await
        .unwrap();
    got.sort();
    assert_eq!(
        got,
        vec![("a/film.mkv".into(), None), ("a/notag.mkv".into(), None)],
        "only the sizes of a standing takedown with a sha256"
    );
    // What the indexer stored for an object at this tag and size is returned, and never at another tag.
    sqlx::query(
        "INSERT INTO storage_objects (id, mount_id, key, size_bytes, kind, class, content_tag, content_sha256)
         VALUES ($1, $2, 'a/hashed.mkv', 8, 'cinema', 'movie', $3, $4)",
    )
    .bind(uid())
    .bind(theirs)
    .bind(&multipart)
    .bind("0".repeat(64))
    .execute(w.pool())
    .await
    .unwrap();
    let got = storage_legal::hash_candidates(
        w.pool(),
        theirs,
        &cands(&[("a/hashed.mkv", Some(multipart.as_str()), 8)]),
    )
    .await
    .unwrap();
    assert_eq!(got, vec![("a/hashed.mkv".into(), Some("0".repeat(64)))]);
    let got = storage_legal::hash_candidates(
        w.pool(),
        theirs,
        &cands(&[(
            "a/hashed.mkv",
            Some("ffffffffffffffffffffffffffffffff-3"),
            8,
        )]),
    )
    .await
    .unwrap();
    assert_eq!(
        got,
        vec![("a/hashed.mkv".into(), None)],
        "another tag: read again"
    );

    let probe = |key: &str, tag: Option<&str>, sha: &str, size: i64| storage_legal::Probe {
        key: key.to_owned(),
        tag: tag.map(str::to_owned),
        sha256: Some(sha.to_owned()),
        size,
    };
    let mut got = storage_legal::delist_hash_matches(
        w.pool(),
        &ids,
        theirs,
        &[
            probe("a/film.mkv", Some(multipart.as_str()), SHA, 8),
            probe("a/notag.mkv", None, SHA, 8),
            // Same size, other bytes.
            probe(
                "a/lookalike.mkv",
                Some(multipart.as_str()),
                &"1".repeat(64),
                8,
            ),
            // Same bytes, other size (a different file).
            probe("a/longer.mkv", Some(multipart.as_str()), SHA, 9),
        ],
    )
    .await
    .unwrap();
    got.sort();
    assert_eq!(got, vec!["a/film.mkv".to_owned(), "a/notag.mkv".to_owned()]);
    type Row = (String, Option<String>, Option<String>, Option<i64>);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT object_key, object_hash, object_sha256, subject_account_id FROM takedown_items
         WHERE notice_id = $1 AND derived_from IS NOT NULL ORDER BY object_key",
    )
    .bind(id)
    .fetch_all(w.pool())
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![
            (
                "a/film.mkv".into(),
                Some(multipart.clone()),
                Some(SHA.into()),
                Some(other)
            ),
            ("a/notag.mkv".into(), None, Some(SHA.into()), Some(other)),
        ]
    );
    // A tag match (the fast path) still works and the match carries the upheld item's sha256.
    let tagged = listed(&[("b/same-tag.mkv", LEAK_TAG, 8)]);
    assert_eq!(
        storage_legal::delist_byte_matches(w.pool(), &ids, theirs, &tagged)
            .await
            .unwrap(),
        vec!["b/same-tag.mkv".to_owned()]
    );
    let sha: Option<String> = sqlx::query_scalar(
        "SELECT object_sha256 FROM takedown_items WHERE notice_id = $1 AND object_key = 'b/same-tag.mkv'",
    )
    .bind(id)
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(sha.as_deref(), Some(SHA));
    // The restore ends the sieve.
    w.m.restore_notice(&who, id, Some("counter-notice accepted"))
        .await
        .unwrap();
    assert!(
        storage_legal::hash_candidates(w.pool(), theirs, &cands(&probe_objs))
            .await
            .unwrap()
            .is_empty()
    );
}

/// D-584: a byte-matched item tells its owner the removal DM with the notice's reference (once, however often the
/// indexer runs), without the other member's key or bucket, and no strike; the restore DM reaches them; on a server's
/// bucket the owner is the uploader as P28-LEG-1 resolves it; a counter-notice is recorded against the notice.
#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn a_byte_matched_owner_is_told_on_removal_and_on_restore_and_is_never_struck() {
    let w = world().await;
    let reuploader = account(w.pool(), "reuploader").await;
    let guild_owner = account(w.pool(), "guildowner").await;
    let uploader = account(w.pool(), "guildmember").await;
    let mnt = mount(w.pool(), "user", w.owner, "My shelf").await;
    let theirs = mount(w.pool(), "user", reuploader, "Their shelf").await;
    let guild = uid();
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'The Server', $2)")
        .bind(guild)
        .bind(guild_owner)
        .execute(w.pool())
        .await
        .unwrap();
    let shared = mount(w.pool(), "guild", guild, "Server shelf").await;
    sqlx::query(
        "INSERT INTO storage_mount_uploads (id, mount_id, user_id, object_key, s3_upload_id, size_bytes, part_size, part_count, state)
         VALUES ($1, $2, $3, 'copy/leak.mkv', 'u1', 8, 16777216, 1, 'completed')",
    )
    .bind(uid())
    .bind(shared)
    .bind(uploader)
    .execute(w.pool())
    .await
    .unwrap();
    catalogued(w.pool(), mnt, "films/leak.mkv", LEAK_TAG, 8).await;
    let id = w.notice_with(mnt, "films/leak.mkv").await;
    let c = w.caller();
    let who = Who::Staff(&c);
    let ids = uid;

    // Nothing to tell before the uphold, and the indexer has found nothing yet.
    assert_eq!(w.m.notify_byte_matches().await, 0);
    w.m.uphold_notice(&who, id, Some("valid")).await.unwrap();
    let owner_dms = |to: i64| {
        w.dms
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|(t, _)| *t == to)
            .map(|(_, text)| text.clone())
            .collect::<Vec<_>>()
    };
    let before = w.dms.0.lock().unwrap().len();
    storage_legal::delist_byte_matches(
        w.pool(),
        &ids,
        theirs,
        &listed(&[("x/their-copy.mkv", LEAK_TAG, 8)]),
    )
    .await
    .unwrap();
    storage_legal::delist_byte_matches(
        w.pool(),
        &ids,
        shared,
        &listed(&[("copy/leak.mkv", LEAK_TAG, 8)]),
    )
    .await
    .unwrap();
    assert_eq!(
        w.dms.0.lock().unwrap().len(),
        before,
        "recording a match sends nothing by itself"
    );

    // The removal DM: the re-uploader, the uploader on the server's bucket, and the server owner as the contact.
    assert_eq!(w.m.notify_byte_matches().await, 2);
    let reference = format!("TK-{id}");
    let mine = owner_dms(reuploader);
    assert_eq!(mine.len(), 1, "{mine:?}");
    assert!(
        mine[0].contains("was removed after a copyright notice"),
        "{}",
        mine[0]
    );
    assert!(mine[0].contains(&reference), "{}", mine[0]);
    assert!(
        mine[0].contains("x/their-copy.mkv") && mine[0].contains("Their shelf"),
        "{}",
        mine[0]
    );
    assert!(mine[0].contains("counter-notice"), "{}", mine[0]);
    assert!(
        !mine[0].contains("films/leak.mkv") && !mine[0].contains("My shelf"),
        "another member's key and bucket stay off the DM: {}",
        mine[0]
    );
    let up = owner_dms(uploader);
    assert_eq!(up.len(), 1, "the uploader is told: {up:?}");
    assert!(up[0].contains("copy/leak.mkv"));
    assert_eq!(
        owner_dms(guild_owner).len(),
        1,
        "the server owner is told as the mount's contact"
    );
    // Once: another run tells nobody again.
    assert_eq!(w.m.notify_byte_matches().await, 0);
    assert_eq!(owner_dms(reuploader).len(), 1);
    let told: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM takedown_items WHERE notice_id = $1 AND derived_from IS NOT NULL AND notified_at IS NOT NULL",
    )
    .bind(id)
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(told, 2);
    // No strike: the derived items name no Person, so no one but the owner staff named has a strike count.
    let persons: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM takedown_items WHERE notice_id = $1 AND derived_from IS NOT NULL AND subject_person_id IS NOT NULL",
    )
    .bind(id)
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(persons, 0);
    let d = w.m.get_notice(&who, id).await.unwrap();
    let theirs_person = person_of(w.pool(), reuploader).await;
    let uploader_person = person_of(w.pool(), uploader).await;
    assert!(
        d.strikes
            .iter()
            .all(|s| s.person_id != theirs_person && s.person_id != uploader_person),
        "{:?}",
        d.strikes
    );

    // A counter-notice is recorded against the notice like any item's; the restore tells them again.
    w.m.record_counter_notice(&who, id, "I own this copy, here is my licence for it")
        .await
        .unwrap();
    let notice_state: String =
        sqlx::query_scalar("SELECT state FROM takedown_notices WHERE id = $1")
            .bind(id)
            .fetch_one(w.pool())
            .await
            .unwrap();
    assert_eq!(notice_state, NoticeState::CounterNoticed.as_str());
    w.m.restore_notice(&who, id, Some("counter-notice accepted"))
        .await
        .unwrap();
    let mine = owner_dms(reuploader);
    assert_eq!(mine.len(), 2, "{mine:?}");
    assert!(mine[1].contains("available again"), "{}", mine[1]);
    assert!(
        mine[1].contains("x/their-copy.mkv") && !mine[1].contains("films/leak.mkv"),
        "{}",
        mine[1]
    );
    assert_eq!(owner_dms(uploader).len(), 2);
    // A match made after the restore finds no standing takedown, and nobody is told.
    assert!(
        storage_legal::delist_byte_matches(
            w.pool(),
            &ids,
            theirs,
            &listed(&[("x/again.mkv", LEAK_TAG, 8)])
        )
        .await
        .unwrap()
        .is_empty()
    );
    assert_eq!(w.m.notify_byte_matches().await, 0);
}

// ---- SEC-AUD-059 (D-589): the one-off backfill of earlier takedown items --------------------------------------------

/// D-589: derived items from before D-584 get their owner (the mount owner, the uploader on a server's bucket; never a
/// Person) and the removal DM once; one whose bucket is gone is left and returned; every disabled item with a size and
/// no sha256 gets a digest job (none twice); a restored notice's items are not touched; a second run does nothing new.
#[tokio::test]
#[ignore = "needs Postgres (SR_TEST_DATABASE_URL)"]
async fn the_backfill_gives_earlier_derived_items_an_owner_and_every_unhashed_item_a_job() {
    let w = world().await;
    let reuploader = account(w.pool(), "reuploader").await;
    let uploader = account(w.pool(), "guildmember").await;
    let guild_owner = account(w.pool(), "guildowner").await;
    let gone_owner = account(w.pool(), "goneowner").await;
    let mnt = mount(w.pool(), "user", w.owner, "My shelf").await;
    let theirs = mount(w.pool(), "user", reuploader, "Their shelf").await;
    let gone = mount(w.pool(), "user", gone_owner, "Gone shelf").await;
    let guild = uid();
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'The Server', $2)")
        .bind(guild)
        .bind(guild_owner)
        .execute(w.pool())
        .await
        .unwrap();
    let shared = mount(w.pool(), "guild", guild, "Server shelf").await;
    sqlx::query(
        "INSERT INTO storage_mount_uploads (id, mount_id, user_id, object_key, s3_upload_id, size_bytes, part_size, part_count, state)
         VALUES ($1, $2, $3, 'copy/leak.mkv', 'u1', 8, 16777216, 1, 'completed')",
    )
    .bind(uid())
    .bind(shared)
    .bind(uploader)
    .execute(w.pool())
    .await
    .unwrap();
    catalogued(w.pool(), mnt, "films/leak.mkv", LEAK_TAG, 8).await;
    catalogued(
        w.pool(),
        mnt,
        "films/second.mkv",
        "ffffffffffffffffffffffffffffffff",
        8,
    )
    .await;
    let c = w.caller();
    let who = Who::Staff(&c);
    let id = w.notice_with(mnt, "films/leak.mkv").await;
    w.m.uphold_notice(&who, id, Some("valid")).await.unwrap();
    let id2 = w.notice_with(mnt, "films/second.mkv").await;
    w.m.uphold_notice(&who, id2, Some("valid")).await.unwrap();
    // Wave 55's derived items: matched by tag, no owner. One more under a notice that is then restored.
    for (m, key) in [
        (theirs, "x/their-copy.mkv"),
        (shared, "copy/leak.mkv"),
        (gone, "x/orphan.mkv"),
    ] {
        storage_legal::delist_byte_matches(w.pool(), &uid, m, &listed(&[(key, LEAK_TAG, 8)]))
            .await
            .unwrap();
    }
    storage_legal::delist_byte_matches(
        w.pool(),
        &uid,
        theirs,
        &listed(&[("x/restored-copy.mkv", "ffffffffffffffffffffffffffffffff", 8)]),
    )
    .await
    .unwrap();
    w.m.restore_notice(&who, id2, Some("counter-notice accepted"))
        .await
        .unwrap();
    sqlx::query("UPDATE takedown_items SET subject_account_id = NULL, notified_at = NULL WHERE derived_from IS NOT NULL")
        .execute(w.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE storage_mounts SET deleted_at = now() WHERE id = $1")
        .bind(gone)
        .execute(w.pool())
        .await
        .unwrap();
    // Drop the uphold's own jobs but keep one, to show an item with a job queued gets no second one.
    let kept = item_of(w.pool(), id, "films/leak.mkv").await;
    sqlx::query("DELETE FROM jobs WHERE kind = 'storage.takedown_digest' AND (payload ->> 'item_id')::bigint <> $1")
        .bind(kept)
        .execute(w.pool())
        .await
        .unwrap();
    let before = w.dms.0.lock().unwrap().len();

    let report = w.m.backfill_byte_takedowns().await.unwrap();
    assert_eq!(report.owners_assigned, 2, "{report:?}");
    assert_eq!(
        report.told, 2,
        "two owners (the server owner is a third DM as contact)"
    );
    let gone_item = item_of(w.pool(), id, "x/orphan.mkv").await;
    assert_eq!(report.unresolved, vec![(id, gone_item)]);
    let owner_of = |key: &'static str| {
        let pool = w.pool().clone();
        async move {
            sqlx::query_scalar::<_, Option<i64>>(
                "SELECT subject_account_id FROM takedown_items WHERE object_key = $1",
            )
            .bind(key)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    assert_eq!(owner_of("x/their-copy.mkv").await, Some(reuploader));
    assert_eq!(
        owner_of("copy/leak.mkv").await,
        Some(uploader),
        "the uploader on a server's bucket"
    );
    assert_eq!(owner_of("x/orphan.mkv").await, None, "left as it is");
    assert_eq!(
        owner_of("x/restored-copy.mkv").await,
        None,
        "a restored notice's item is not touched"
    );
    let persons: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM takedown_items WHERE derived_from IS NOT NULL AND subject_person_id IS NOT NULL",
    )
    .fetch_one(w.pool())
    .await
    .unwrap();
    assert_eq!(persons, 0, "no Person, so no strike");
    // The removal DM, once each, with the reference and without the other member's key and bucket.
    let sent: Vec<(i64, String)> = w.dms.0.lock().unwrap()[before..].to_vec();
    assert_eq!(sent.len(), 3, "{sent:?}");
    let to_reuploader: Vec<&String> = sent
        .iter()
        .filter(|(t, _)| *t == reuploader)
        .map(|(_, x)| x)
        .collect();
    assert_eq!(to_reuploader.len(), 1);
    assert!(
        to_reuploader[0].contains(&format!("TK-{id}")),
        "{}",
        to_reuploader[0]
    );
    assert!(
        !to_reuploader[0].contains("films/leak.mkv"),
        "{}",
        to_reuploader[0]
    );
    assert!(sent.iter().any(|(t, _)| *t == uploader));
    assert!(sent.iter().any(|(t, _)| *t == guild_owner));
    // Digest jobs: every disabled item with a size and no sha256, except the one that has a job queued already
    // (`films/leak.mkv`); not the restored notice's two items, not an item without a size.
    assert_eq!(report.digests_enqueued, 3, "{report:?}");
    let queued: Vec<String> = sqlx::query_scalar(
        "SELECT i.object_key FROM jobs j JOIN takedown_items i ON i.id = (j.payload ->> 'item_id')::bigint
         WHERE j.kind = 'storage.takedown_digest' ORDER BY 1",
    )
    .fetch_all(w.pool())
    .await
    .unwrap();
    assert_eq!(
        queued,
        vec![
            "copy/leak.mkv".to_owned(),
            "films/leak.mkv".to_owned(),
            "x/orphan.mkv".to_owned(),
            "x/their-copy.mkv".to_owned(),
        ]
    );

    // Again: nobody is told twice and no job is added.
    let again = w.m.backfill_byte_takedowns().await.unwrap();
    assert_eq!(
        (again.owners_assigned, again.told, again.digests_enqueued),
        (0, 0, 0),
        "{again:?}"
    );
    assert_eq!(again.unresolved.len(), 1, "still counted, still left");
    assert_eq!(w.dms.0.lock().unwrap().len(), before + 3);
}
