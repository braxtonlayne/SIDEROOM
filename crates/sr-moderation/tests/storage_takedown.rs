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

    // Delisted: that key only, on that mount only; nothing was held or blocked on the instance's media path.
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
    let blocked: i64 =
        sqlx::query_scalar("SELECT count(*) FROM blocked_objects WHERE notice_id = $1")
            .bind(id)
            .fetch_one(w.pool())
            .await
            .unwrap();
    assert_eq!(blocked, 0);
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
