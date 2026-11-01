//! Postgres tests for shelf access (STU-ZK-001, migration 2020): grants tied to the account, the plaintext catalog
//! with per-object delisting and its strike, and the hash-chained audit rows.
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test shelf_access -- --ignored --test-threads=1`

use sr_chat::social::relationships::{RelOp, RelationshipKind};
use sr_chat::social::repo::RelationshipRepository;
use sr_core::UserId;
use sr_db::scopes_repo::PgScopeFacts;
use sr_db::shelf_access_repo::{PgShelfAccess, delist_object, relist_object};
use sr_db::social_repo::PgSocialRepo;
use sr_db::testing::TestDb;
use sr_storage::mounts::{Mount, MountOwner, MountStatus, Provider, SealedSecret};
use sr_storage::scopes::{self, Refused, ResolveError, Scope, Viewer};
use sr_storage::shelf::{GrantError, GrantRow, ShelfAccessStore, path_hash};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

async fn account(pool: &sqlx::PgPool, id: i64) {
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(id + 100_000)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $2, $3, $4, 'x')",
    )
    .bind(id)
    .bind(id + 100_000)
    .bind(format!("u{id}"))
    .bind(format!("u{id}@example.test"))
    .execute(pool)
    .await
    .unwrap();
}

async fn mount_row(pool: &sqlx::PgPool, id: i64, kind: &str, owner: i64) {
    sqlx::query(
        "INSERT INTO storage_mounts (id, owner_kind, owner_id, label, provider, endpoint, bucket, access_key_id, secret_key_enc) \
         VALUES ($1, $2, $3, 'm', 'garage', 'http://127.0.0.1:3900', 'b', 'AK', $4)",
    )
    .bind(id)
    .bind(kind)
    .bind(owner)
    .bind(vec![0u8])
    .execute(pool)
    .await
    .unwrap();
}

async fn object(pool: &sqlx::PgPool, id: i64, mount: i64, key: &str, kind: &str) {
    sqlx::query(
        "INSERT INTO storage_objects (id, mount_id, key, class, container, web_native, kind, size_bytes) \
         VALUES ($1, $2, $3, 'track', 'mp3', TRUE, $4, 123)",
    )
    .bind(id)
    .bind(mount)
    .bind(key)
    .bind(kind)
    .execute(pool)
    .await
    .unwrap();
}

fn mount(id: i64, owner: MountOwner) -> Mount {
    Mount {
        id,
        owner,
        label: "m".into(),
        provider: Provider::Garage,
        endpoint: "http://127.0.0.1:3900".into(),
        region: "auto".into(),
        bucket: "b".into(),
        prefix: String::new(),
        path_style: true,
        access_key_id: "AK".into(),
        secret_key_enc: SealedSecret::from_bytes(vec![0]),
        key_hint: "AK".into(),
        status: MountStatus::Active,
        cors_ok_at: None,
        last_test_at: None,
        last_test: None,
        created_at: 0,
        updated_at: 0,
    }
}

fn friends(a: i64, b: i64) -> Vec<RelOp> {
    let u = |x: i64| UserId::new(x as u64);
    vec![
        RelOp::Upsert {
            user: u(a),
            target: u(b),
            kind: RelationshipKind::Friend,
            since_ms: 0,
        },
        RelOp::Upsert {
            user: u(b),
            target: u(a),
            kind: RelationshipKind::Friend,
            since_ms: 0,
        },
    ]
}

fn store(pool: &sqlx::PgPool) -> PgShelfAccess {
    let next = Arc::new(AtomicI64::new(7_000));
    PgShelfAccess::new(
        pool.clone(),
        Arc::new(move || next.fetch_add(1, Ordering::SeqCst)),
    )
}

async fn read(f: &PgScopeFacts, v: i64, m: &Mount, key: &str) -> Result<Scope, Refused> {
    match scopes::resolve(
        f,
        Viewer {
            id: v,
            staff: false,
        },
        m,
        key,
    )
    .await
    {
        Ok(s) => Ok(s),
        Err(ResolveError::Refused(r)) => Err(r),
        Err(e) => panic!("{e}"),
    }
}

fn friend_grant(shelf: i64, friend: i64, by: i64) -> GrantRow {
    GrantRow {
        id: 0,
        shelf_id: shelf,
        grantee_kind: "friend".into(),
        grantee_id: friend,
        guild_id: None,
        granted_by: by,
        created_ms: 0,
        revoked_ms: None,
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_grant_gives_access_on_any_device_and_a_revoked_grant_refuses() {
    let db = TestDb::new().await.expect("db");
    let pool = db.pool();
    for id in [1, 2, 3] {
        account(pool, id).await;
    }
    mount_row(pool, 900, "user", 1).await;
    let facts = PgScopeFacts::new(pool.clone());
    let shelf = store(pool);
    let m = mount(900, MountOwner::User(1));
    PgSocialRepo::new(pool.clone())
        .apply(&friends(1, 2))
        .await
        .unwrap();

    // Friends, but nothing granted: refused. A stranger is refused as not a friend.
    assert_eq!(read(&facts, 2, &m, "a.mp3").await, Err(Refused::NoShare));
    assert_eq!(read(&facts, 3, &m, "a.mp3").await, Err(Refused::NotFriend));
    assert!(shelf.candidate_shelves(2).await.unwrap().is_empty());

    let g = shelf.create_grant(&friend_grant(900, 2, 1)).await.unwrap();
    assert!(g.id > 0 && g.revoked_ms.is_none());
    // The decision reads account-bound rows only, so any session of account 2 gets the same answer; any kind
    // is readable (a whole-shelf grant), the stranger still gets nothing.
    for _device in 0..2 {
        assert_eq!(
            read(&facts, 2, &m, "a.mp3").await,
            Ok(Scope::Friend { owner: 1 })
        );
        assert_eq!(
            read(&facts, 2, &m, "film.mkv").await,
            Ok(Scope::Friend { owner: 1 })
        );
    }
    assert_eq!(read(&facts, 3, &m, "a.mp3").await, Err(Refused::NotFriend));
    assert_eq!(shelf.candidate_shelves(2).await.unwrap(), vec![900]);
    // The emulator's files stay the owner's.
    assert!(read(&facts, 2, &m, "emu/roms/x.sfc").await.is_err());

    // Granting twice is a conflict.
    assert_eq!(
        shelf
            .create_grant(&friend_grant(900, 2, 1))
            .await
            .unwrap_err(),
        GrantError::Duplicate
    );

    // Revoked: the next decision refuses; the row stays with its revocation.
    assert!(shelf.revoke_grant(900, g.id, 1).await.unwrap());
    assert!(!shelf.revoke_grant(900, g.id, 1).await.unwrap());
    assert_eq!(read(&facts, 2, &m, "a.mp3").await, Err(Refused::NoShare));
    let all = shelf.grants_of(900).await.unwrap();
    assert_eq!(all.len(), 1);
    assert!(all[0].revoked_ms.is_some());

    // A fresh grant, then the friendship ends: the grant ends with it.
    let g2 = shelf.create_grant(&friend_grant(900, 2, 1)).await.unwrap();
    assert_eq!(
        read(&facts, 2, &m, "a.mp3").await,
        Ok(Scope::Friend { owner: 1 })
    );
    let mut tx = pool.begin().await.unwrap();
    let n = sr_db::scopes_repo::revoke_shares_between(&mut tx, 1, 2)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(n, 1);
    sqlx::query("DELETE FROM relationships WHERE user_id IN (1, 2) AND target_id IN (1, 2)")
        .execute(pool)
        .await
        .unwrap();
    assert_eq!(read(&facts, 2, &m, "a.mp3").await, Err(Refused::NotFriend));
    let live: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM shelf_grants WHERE id = $1 AND revoked_at IS NULL",
    )
    .bind(g2.id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(live, 0);

    // Every create and revoke is in the hash-chained log, and the chain verifies.
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT action FROM staff_access_log WHERE action LIKE 'shelf.grant.%' ORDER BY seq",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![
            "shelf.grant.create",
            "shelf.grant.revoke",
            "shelf.grant.create"
        ]
    );
    let mut conn = pool.acquire().await.unwrap();
    let report = sr_db::admin_repo::verify_chain(&mut conn).await.unwrap();
    assert!(report.ok, "chain intact: {report:?}");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_guild_role_grant_reaches_the_roles_members_only() {
    let db = TestDb::new().await.expect("db");
    let pool = db.pool();
    for id in [1, 2, 3] {
        account(pool, id).await;
    }
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES (50, 'Room', 1)")
        .execute(pool)
        .await
        .unwrap();
    for (rid, name) in [(50i64, "@everyone"), (61, "readers")] {
        sqlx::query("INSERT INTO roles (id, guild_id, name, permissions) VALUES ($1, 50, $2, 0)")
            .bind(rid)
            .bind(name)
            .execute(pool)
            .await
            .unwrap();
    }
    for u in [1, 2, 3] {
        sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES (50, $1)")
            .bind(u)
            .execute(pool)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO member_roles (guild_id, user_id, role_id) VALUES (50, 2, 61)")
        .execute(pool)
        .await
        .unwrap();
    mount_row(pool, 900, "user", 1).await;
    let facts = PgScopeFacts::new(pool.clone());
    let shelf = store(pool);
    let m = mount(900, MountOwner::User(1));

    assert!(read(&facts, 2, &m, "a.mp3").await.is_err());
    let g = GrantRow {
        grantee_kind: "guild_role".into(),
        grantee_id: 61,
        guild_id: Some(50),
        ..friend_grant(900, 61, 1)
    };
    let made = shelf.create_grant(&g).await.unwrap();
    assert_eq!(read(&facts, 2, &m, "a.mp3").await, Ok(Scope::Guild(50)));
    // A member without the role, and a non-member, get nothing.
    assert!(read(&facts, 3, &m, "a.mp3").await.is_err());
    assert!(read(&facts, 99, &m, "a.mp3").await.is_err());
    assert_eq!(shelf.candidate_shelves(2).await.unwrap(), vec![900]);
    assert!(shelf.revoke_grant(900, made.id, 1).await.unwrap());
    assert!(read(&facts, 2, &m, "a.mp3").await.is_err());
    // The role must be a live grant to a role of the guild: the shape check refuses a mixed row.
    let bad = sqlx::query(
        "INSERT INTO shelf_grants (id, shelf_id, grantee_kind, grantee_id, granted_by) VALUES (1, 900, 'guild_role', 61, 1)",
    )
    .execute(pool)
    .await;
    assert!(bad.is_err(), "a role grant names its guild");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_catalog_follows_the_index_and_keeps_owner_edits() {
    let db = TestDb::new().await.expect("db");
    let pool = db.pool();
    account(pool, 1).await;
    mount_row(pool, 900, "user", 1).await;
    object(pool, 1, 900, "Music/a.mp3", "music").await;
    object(pool, 2, 900, "Films/b.mkv", "cinema").await;
    let shelf = store(pool);

    let page = shelf.catalog_page(900, None, 10).await.unwrap();
    assert_eq!(page.len(), 2);
    assert!(page.windows(2).all(|w| w[0].object_hash < w[1].object_hash));
    let h = path_hash("Music/a.mp3");
    let e = shelf.catalog_object(900, &h).await.unwrap().unwrap();
    assert_eq!(
        (e.path.as_str(), e.size, e.kind.as_deref()),
        ("Music/a.mp3", 123, Some("music"))
    );

    // The owner's edits survive a re-index (the indexer updates the row).
    assert!(
        shelf
            .set_owner_meta(
                900,
                &h,
                Some("My song"),
                Some("Music/cover.jpg"),
                Some("a note")
            )
            .await
            .unwrap()
    );
    sqlx::query("UPDATE storage_objects SET size_bytes = 999, seen_run = 2 WHERE id = 1")
        .execute(pool)
        .await
        .unwrap();
    let e = shelf.catalog_object(900, &h).await.unwrap().unwrap();
    assert_eq!(
        (e.size, e.owner_title.as_str(), e.owner_note.as_str()),
        (999, "My song", "a note")
    );

    // An object that vanishes from the bucket leaves the catalog; a moved one changes hash.
    sqlx::query("UPDATE storage_objects SET state = 'vanished' WHERE id = 2")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE storage_objects SET key = 'Music/renamed.mp3' WHERE id = 1")
        .execute(pool)
        .await
        .unwrap();
    let page = shelf.catalog_page(900, None, 10).await.unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].object_hash, path_hash("Music/renamed.mp3"));
    assert!(!shelf.catalog_object(900, &h).await.unwrap().unwrap().listed);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_delisted_object_refuses_for_everyone_and_logs_the_strike() {
    let db = TestDb::new().await.expect("db");
    let pool = db.pool();
    for id in [1, 2, 3] {
        account(pool, id).await;
    }
    mount_row(pool, 900, "user", 1).await;
    object(pool, 1, 900, "Music/a.mp3", "music").await;
    object(pool, 2, 900, "Music/b.mp3", "music").await;
    let facts = PgScopeFacts::new(pool.clone());
    let shelf = store(pool);
    let m = mount(900, MountOwner::User(1));
    PgSocialRepo::new(pool.clone())
        .apply(&friends(1, 2))
        .await
        .unwrap();
    shelf.create_grant(&friend_grant(900, 2, 1)).await.unwrap();
    assert!(read(&facts, 2, &m, "Music/a.mp3").await.is_ok());

    let h = path_hash("Music/a.mp3");
    let mut tx = pool.begin().await.unwrap();
    // A shelf the staff do not know, and a hash that is not there, are not delisted.
    assert!(
        delist_object(&mut tx, 8_001, 12345, &h, None, "x", 3)
            .await
            .unwrap()
            .is_none()
    );
    let d = delist_object(&mut tx, 8_002, 900, &h, None, "DMCA notice 17", 3)
        .await
        .unwrap()
        .expect("delisted");
    assert_eq!(
        (d.strike_id, d.subject_account, d.path.as_str()),
        (8_002, Some(1), "Music/a.mp3")
    );
    // Twice is nothing new.
    assert!(
        delist_object(&mut tx, 8_003, 900, &h, None, "again", 3)
            .await
            .unwrap()
            .is_none()
    );
    tx.commit().await.unwrap();

    // Refused for the owner, the grantee and the staff alike; absent from the catalog; the other object stays.
    for who in [1, 2] {
        assert_eq!(
            read(&facts, who, &m, "Music/a.mp3").await,
            Err(Refused::Delisted)
        );
        assert!(read(&facts, who, &m, "Music/b.mp3").await.is_ok());
    }
    let staff = scopes::resolve(&facts, Viewer { id: 3, staff: true }, &m, "Music/a.mp3").await;
    assert!(matches!(
        staff,
        Err(ResolveError::Refused(Refused::Delisted))
    ));
    let page = shelf.catalog_page(900, None, 10).await.unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].path, "Music/b.mp3");
    assert!(
        shelf
            .catalog_object(900, &h)
            .await
            .unwrap()
            .unwrap()
            .delisted
    );
    assert_eq!(shelf.labels(&[900]).await.unwrap()[0].object_count, 1);

    // The strike is kept (and cannot be deleted); a re-index does not bring the object back.
    let strikes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM shelf_delist_strikes WHERE shelf_id = 900 AND object_hash = $1 AND subject_account = 1",
    )
    .bind(&h)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(strikes, 1);
    assert!(
        sqlx::query("DELETE FROM shelf_delist_strikes")
            .execute(pool)
            .await
            .is_err()
    );
    sqlx::query("UPDATE storage_objects SET size_bytes = 5, seen_run = 3 WHERE id = 1")
        .execute(pool)
        .await
        .unwrap();
    assert_eq!(
        read(&facts, 2, &m, "Music/a.mp3").await,
        Err(Refused::Delisted)
    );

    // Relisting restores it and marks the strike restored.
    let mut tx = pool.begin().await.unwrap();
    assert!(relist_object(&mut tx, 900, &h, 3).await.unwrap().is_some());
    assert!(relist_object(&mut tx, 900, &h, 3).await.unwrap().is_none());
    tx.commit().await.unwrap();
    assert!(read(&facts, 2, &m, "Music/a.mp3").await.is_ok());
    let restored: bool = sqlx::query_scalar(
        "SELECT restored_at IS NOT NULL FROM shelf_delist_strikes WHERE id = 8002",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert!(restored);
}
