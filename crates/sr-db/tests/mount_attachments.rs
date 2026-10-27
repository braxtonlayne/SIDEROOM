//! Postgres tests for attaching the platform bucket to a server (migration 1441, wave-27 follow-up).
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test mount_attachments -- --ignored --test-threads=1`

use sr_db::scopes_repo::PgScopeFacts;
use sr_db::testing::TestDb;
use sr_storage::mounts::{Mount, MountOwner, MountStatus, Provider, SealedSecret};
use sr_storage::scopes::{
    self, Grant, GrantSet, Refused, ResolveError, Scope, ScopeFacts as _, ScopeStore, Viewer,
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

fn platform_mount() -> Mount {
    Mount {
        id: 1,
        owner: MountOwner::Platform,
        label: "Instance".into(),
        provider: Provider::Garage,
        endpoint: "http://127.0.0.1:3900".into(),
        region: "garage".into(),
        bucket: "sideroom".into(),
        prefix: String::new(),
        path_style: true,
        access_key_id: "AK".into(),
        secret_key_enc: SealedSecret::from_bytes(vec![0]),
        key_hint: "AK".into(),
        status: MountStatus::Active,
        cors_ok_at: Some(1),
        last_test_at: None,
        last_test: None,
        created_at: 0,
        updated_at: 0,
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn attaching_the_platform_bucket_gives_a_server_member_the_shelf_and_detaching_takes_it_back()
{
    let db = TestDb::new().await.expect("db");
    let pool = db.pool().clone();
    let facts = PgScopeFacts::new(pool.clone());
    for id in [1, 2, 3] {
        account(&pool, id).await;
    }
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES (50, 'Room', 1)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES (51, 'Other', 1)")
        .execute(&pool)
        .await
        .unwrap();
    // @everyone (id = guild id) and a role that may view music; user 2 is a member with it, user 3 is not a member.
    for (rid, guild, name) in [
        (50i64, 50i64, "@everyone"),
        (61, 50, "dj"),
        (51, 51, "@everyone"),
    ] {
        sqlx::query("INSERT INTO roles (id, guild_id, name, permissions) VALUES ($1, $2, $3, 0)")
            .bind(rid)
            .bind(guild)
            .bind(name)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES (50, 2)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO member_roles (guild_id, user_id, role_id) VALUES (50, 2, 61)")
        .execute(&pool)
        .await
        .unwrap();
    // The mount row has to exist for the foreign key.
    sqlx::query(
        "INSERT INTO storage_mounts (id, owner_kind, owner_id, label, provider, endpoint, bucket, access_key_id, secret_key_enc) \
         VALUES (1, 'platform', 0, 'Instance', 'garage', 'http://127.0.0.1:3900', 'sideroom', 'AK', $1)",
    )
    .bind(vec![0u8])
    .execute(&pool)
    .await
    .unwrap();
    facts
        .set_role_grants(50, 61, &GrantSet::of([Grant::ViewMusic]))
        .await
        .unwrap();

    assert!(facts.guild_exists(50).await.unwrap());
    assert!(!facts.guild_exists(999).await.unwrap());
    let mount = platform_mount();
    let member = Viewer {
        id: 2,
        staff: false,
    };
    let outsider = Viewer {
        id: 3,
        staff: false,
    };

    // Before: the instance bucket is nobody's shelf.
    let before = scopes::resolve(&facts, member, &mount, "a.mp3").await;
    assert!(
        matches!(
            before,
            Err(ResolveError::Refused(Refused::NotMember | Refused::NoGrant))
        ),
        "{before:?}"
    );
    assert!(facts.attached_guilds(1).await.unwrap().is_empty());

    // Attach is once-only and recorded.
    assert!(facts.attach_mount(1, 50, 1).await.unwrap());
    assert!(!facts.attach_mount(1, 50, 1).await.unwrap());
    assert_eq!(facts.attached_guilds(1).await.unwrap(), vec![50]);

    // The member with the music grant reaches music, not video; a stranger reaches nothing.
    let scope = scopes::resolve(&facts, member, &mount, "a.mp3")
        .await
        .unwrap();
    assert_eq!(scope, Scope::Guild(50));
    assert!(
        scopes::resolve(&facts, member, &mount, "a.mkv")
            .await
            .is_err()
    );
    assert!(
        scopes::resolve(&facts, outsider, &mount, "a.mp3")
            .await
            .is_err()
    );
    // The shelf is read-only: nobody uploads into the platform bucket through it.
    assert!(!scopes::may_upload(&facts, member, &mount).await.unwrap());
    assert!(!scopes::may_manage(&facts, member, &mount).await.unwrap());

    // Detach takes it back; a second detach finds nothing.
    assert!(facts.detach_mount(1, 50).await.unwrap());
    assert!(!facts.detach_mount(1, 50).await.unwrap());
    assert!(
        scopes::resolve(&facts, member, &mount, "a.mp3")
            .await
            .is_err()
    );
    assert!(facts.attached_guilds(1).await.unwrap().is_empty());

    // Deleting the server drops its attachments.
    assert!(facts.attach_mount(1, 51, 1).await.unwrap());
    sqlx::query("DELETE FROM roles WHERE guild_id = 51")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM guilds WHERE id = 51")
        .execute(&pool)
        .await
        .unwrap();
    assert!(facts.attached_guilds(1).await.unwrap().is_empty());
}
