//! Postgres tests for shares, grants and the scope facts (BYOS-SCP-001, migration 1410).
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test scopes_repo -- --ignored --test-threads=1`

use sr_chat::social::relationships::{RelOp, RelationshipKind};
use sr_chat::social::repo::RelationshipRepository;
use sr_core::UserId;
use sr_db::scopes_repo::PgScopeFacts;
use sr_db::social_repo::PgSocialRepo;
use sr_db::testing::TestDb;
use sr_storage::mounts::{Mount, MountOwner, MountStatus, Provider, SealedSecret};
use sr_storage::scopes::{
    self, Grant, GrantSet, Refused, ResolveError, Scope, ScopeFacts, Share, Viewer,
};

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

fn viewer(id: i64) -> Viewer {
    Viewer { id, staff: false }
}

async fn read(f: &PgScopeFacts, v: i64, m: &Mount, key: &str) -> Result<Scope, Refused> {
    match scopes::resolve(f, viewer(v), m, key).await {
        Ok(s) => Ok(s),
        Err(ResolveError::Refused(r)) => Err(r),
        Err(e) => panic!("{e}"),
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn share_lifecycle_and_unfriend_revokes() {
    let db = TestDb::new().await.expect("db");
    let pool = db.pool();
    for id in [1, 2, 3] {
        account(pool, id).await;
    }
    mount_row(pool, 900, "user", 1).await;
    let facts = PgScopeFacts::new(pool.clone());
    let social = PgSocialRepo::new(pool.clone());
    let m = mount(900, MountOwner::User(1));

    assert_eq!(read(&facts, 2, &m, "a.mp3").await, Err(Refused::NotFriend));
    social.apply(&friends(1, 2)).await.unwrap();
    assert_eq!(read(&facts, 2, &m, "a.mp3").await, Err(Refused::NoShare));

    let music = Share {
        music: true,
        ..Share::default()
    };
    facts.set_share(900, 2, music).await.unwrap();
    assert_eq!(
        read(&facts, 2, &m, "a.mp3").await,
        Ok(Scope::Friend { owner: 1 })
    );
    assert_eq!(read(&facts, 2, &m, "a.mkv").await, Err(Refused::NoShare));
    assert_eq!(read(&facts, 3, &m, "a.mp3").await, Err(Refused::NotFriend));
    assert_eq!(read(&facts, 1, &m, "a.mkv").await, Ok(Scope::Own));

    let incoming = facts.shares_to(2).await.unwrap();
    assert_eq!(incoming.len(), 1);
    assert_eq!((incoming[0].mount_id, incoming[0].owner_id), (900, 1));
    assert_eq!(facts.shares_of_mount(900).await.unwrap(), vec![(2, music)]);
    let sum = facts.shelves_summary(2).await.unwrap();
    assert_eq!((sum.cinema, sum.library, sum.music), (0, 0, 1));
    let own = facts.shelves_summary(1).await.unwrap();
    assert_eq!((own.cinema, own.library, own.music), (1, 1, 1));

    // Unfriend: the shares go with the friendship.
    social
        .apply(&[
            RelOp::Delete {
                user: UserId::new(1),
                target: UserId::new(2),
            },
            RelOp::Delete {
                user: UserId::new(2),
                target: UserId::new(1),
            },
        ])
        .await
        .unwrap();
    assert!(facts.shares_of_mount(900).await.unwrap().is_empty());
    assert!(facts.shares_to(2).await.unwrap().is_empty());
    assert_eq!(read(&facts, 2, &m, "a.mp3").await, Err(Refused::NotFriend));

    // Befriending again does not resurrect the share; a block also revokes.
    social.apply(&friends(1, 2)).await.unwrap();
    assert_eq!(read(&facts, 2, &m, "a.mp3").await, Err(Refused::NoShare));
    facts.set_share(900, 2, music).await.unwrap();
    social
        .apply(&[RelOp::Upsert {
            user: UserId::new(1),
            target: UserId::new(2),
            kind: RelationshipKind::Blocked,
            since_ms: 0,
        }])
        .await
        .unwrap();
    assert!(facts.shares_of_mount(900).await.unwrap().is_empty());

    // An all-off share deletes the row; a suspended owner closes the mount.
    social.apply(&friends(1, 3)).await.unwrap();
    facts.set_share(900, 3, music).await.unwrap();
    facts.set_share(900, 3, Share::default()).await.unwrap();
    assert!(facts.shares_of_mount(900).await.unwrap().is_empty());
    facts.set_share(900, 3, music).await.unwrap();
    assert!(read(&facts, 3, &m, "a.mp3").await.is_ok());
    sqlx::query("UPDATE accounts SET state = 'platform_banned' WHERE id = 1")
        .execute(pool)
        .await
        .unwrap();
    assert_eq!(
        read(&facts, 3, &m, "a.mp3").await,
        Err(Refused::OwnerInactive)
    );
    assert!(facts.shares_to(3).await.unwrap().is_empty());
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn guild_grants_resolve_with_owner_admin_and_manage_guild() {
    let db = TestDb::new().await.expect("db");
    let pool = db.pool();
    for id in [1, 2, 3, 4, 5, 6] {
        account(pool, id).await;
    }
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES (50, 'Room', 1)")
        .execute(pool)
        .await
        .unwrap();
    // @everyone has id = guild id; 61 grants music; 62 is ADMINISTRATOR; 63 is MANAGE_GUILD.
    for (rid, name, perms) in [
        (50i64, "@everyone", 0i64),
        (61, "dj", 0),
        (62, "admin", 8),
        (63, "mgr", 32),
    ] {
        sqlx::query("INSERT INTO roles (id, guild_id, name, permissions) VALUES ($1, 50, $2, $3)")
            .bind(rid)
            .bind(name)
            .bind(perms)
            .execute(pool)
            .await
            .unwrap();
    }
    for u in [1, 2, 3, 4, 5] {
        sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES (50, $1)")
            .bind(u)
            .execute(pool)
            .await
            .unwrap();
    }
    for (u, r) in [(2, 61), (3, 62), (4, 63)] {
        sqlx::query("INSERT INTO member_roles (guild_id, user_id, role_id) VALUES (50, $1, $2)")
            .bind(u)
            .bind(r)
            .execute(pool)
            .await
            .unwrap();
    }
    mount_row(pool, 901, "guild", 50).await;
    let facts = PgScopeFacts::new(pool.clone());
    let m = mount(901, MountOwner::Guild(50));
    facts
        .set_role_grants(50, 61, &GrantSet::of([Grant::ViewMusic]))
        .await
        .unwrap();

    assert_eq!(read(&facts, 6, &m, "a.mp3").await, Err(Refused::NotMember));
    assert_eq!(read(&facts, 5, &m, "a.mp3").await, Err(Refused::NoGrant));
    assert_eq!(read(&facts, 2, &m, "a.mp3").await, Ok(Scope::Guild(50)));
    assert_eq!(read(&facts, 2, &m, "a.mkv").await, Err(Refused::NoGrant));
    for u in [1, 3, 4] {
        assert_eq!(
            read(&facts, u, &m, "a.mkv").await,
            Ok(Scope::Guild(50)),
            "{u}"
        );
    }
    assert!(scopes::may_manage(&facts, viewer(4), &m).await.unwrap());
    assert!(!scopes::may_manage(&facts, viewer(2), &m).await.unwrap());
    assert!(!scopes::may_upload(&facts, viewer(2), &m).await.unwrap());

    // Grants on @everyone apply to every member; replacing a role's set replaces it.
    facts
        .set_role_grants(50, 50, &GrantSet::of([Grant::Upload]))
        .await
        .unwrap();
    assert!(scopes::may_upload(&facts, viewer(5), &m).await.unwrap());
    facts
        .set_role_grants(50, 50, &GrantSet::default())
        .await
        .unwrap();
    assert!(!scopes::may_upload(&facts, viewer(5), &m).await.unwrap());
    let all = facts.grants_of_guild(50).await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[&61].names(), vec!["view_music"]);

    let s = facts.shelves_summary(2).await.unwrap();
    assert_eq!((s.cinema, s.library, s.music), (0, 0, 1));
    let s5 = facts.shelves_summary(5).await.unwrap();
    assert_eq!((s5.cinema, s5.library, s5.music), (0, 0, 0));

    // A pending member is not a member; deleting a role drops its grants.
    sqlx::query("UPDATE guild_members SET pending = true WHERE user_id = 2")
        .execute(pool)
        .await
        .unwrap();
    assert!(facts.guild_standing(50, 2).await.unwrap().is_none());
    sqlx::query("DELETE FROM roles WHERE id = 61")
        .execute(pool)
        .await
        .unwrap();
    assert!(facts.grants_of_guild(50).await.unwrap().is_empty());
    db.cleanup().await.unwrap();
}
