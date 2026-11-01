//! Postgres-backed tests of build trust (ARC-SBX-001): code hashes, the operator's signatures and hash-pinned
//! vouches, including the badge flip when the developer ships different code. Run with a scratch Postgres that
//! may CREATE DATABASE:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-activities --test trust_db -- --ignored --test-threads=1`

use std::io::{Cursor, Write};
use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

use sr_activities::model::{ConfigInput, UrlMapping};
use sr_activities::trust::{self, BadgeKind};
use sr_activities::{ActivityError, ActivityService, Lookup, sandbox};
use sr_db::{
    PgPool,
    accounts::{self, NewAccount},
    testing::TestDb,
};
use sr_storage::{ObjectStorage, ObjectStoreStorage};

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

/// A member in good standing: an active account that is ten days old.
async fn veteran(pool: &PgPool) -> i64 {
    let id = account(pool).await;
    sqlx::query("UPDATE accounts SET created_at = now() - interval '10 days' WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
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

struct Rig {
    db: TestDb,
    svc: ActivityService,
    storage: Arc<dyn ObjectStorage>,
}

async fn rig() -> Rig {
    let db = TestDb::new().await.unwrap();
    let storage: Arc<dyn ObjectStorage> = Arc::new(ObjectStoreStorage::in_memory());
    let svc = ActivityService::new(db.pool().clone(), Arc::new(uid)).with_storage(storage.clone());
    Rig { db, svc, storage }
}

fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (n, d) in files {
        z.start_file(*n, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(d).unwrap();
    }
    z.finish().unwrap().into_inner()
}

/// A bundle whose code is `code` and whose art is `art`.
fn game(code: &str, art: &str) -> Vec<u8> {
    zip_of(&[
        ("index.html", b"<script src=\"app.js\"></script>"),
        ("app.js", code.as_bytes()),
        ("art/logo.png", art.as_bytes()),
    ])
}

async fn set_state(r: &Rig, app: i64, state: &str) {
    sqlx::query("UPDATE applications SET release_state = $2::app_release_state WHERE id = $1")
        .bind(app)
        .bind(state)
        .execute(r.db.pool())
        .await
        .unwrap();
}

/// Upload, submit and approve `zip` for `app`; returns the bundle id.
async fn ship(r: &Rig, app: i64, owner: i64, zip: Vec<u8>) -> i64 {
    let up = r.svc.upload_bundle(app, owner, zip).await.unwrap();
    let id: i64 = up.bundle.id.parse().unwrap();
    r.svc.submit(app, id).await.unwrap();
    r.svc.decide(app, 1, id, true, None).await.unwrap();
    set_state(r, app, "released").await;
    id
}

async fn released_game(r: &Rig, code: &str) -> (i64, i64) {
    let owner = account(r.db.pool()).await;
    let app = application(r.db.pool(), owner, "Arcade").await;
    r.svc
        .save_config(
            app,
            ConfigInput {
                dev_entry_url: None,
                url_mappings: vec![],
            },
        )
        .await
        .unwrap();
    ship(r, app, owner, game(code, "a")).await;
    (app, owner)
}

async fn hash_of(r: &Rig, app: i64) -> String {
    r.svc.served_code_hash(app).await.unwrap().unwrap()
}

/// Make and activate an operator key, returning its id and seed.
async fn operator_key(r: &Rig) -> (i64, [u8; 32]) {
    let seed = [9u8; 32];
    let id = uid();
    r.svc
        .add_signing_key(id, &trust::verify_key_hex(&seed), b"sealed-in-real-life")
        .await
        .unwrap();
    (id, seed)
}

async fn sign(r: &Rig, app: i64, key: (i64, [u8; 32]), hash: &str) {
    let files = [("app.js", &b"x"[..])];
    let audit = sandbox::audit(&files, &[]);
    r.svc
        .record_signature(
            app,
            hash,
            key.0,
            &trust::sign_hex(&key.1, hash),
            &audit,
            "tester",
        )
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_code_hash_covers_code_and_not_art() {
    let r = rig().await;
    let (app, owner) = released_game(&r, "console.log(1)").await;
    let h1 = hash_of(&r, app).await;
    assert_eq!(h1.len(), 64);
    let builds = r.svc.builds_with_hash(&h1).await.unwrap();
    assert_eq!(builds.len(), 1);
    assert_eq!(builds[0].application_id, app);
    assert_eq!(builds[0].state, "approved");

    // Different art, same code: the same code hash, a different bundle.
    ship(&r, app, owner, game("console.log(1)", "other art")).await;
    assert_eq!(hash_of(&r, app).await, h1);
    assert_eq!(r.svc.builds_with_hash(&h1).await.unwrap().len(), 2);

    // Different code: a different hash.
    ship(&r, app, owner, game("console.log(2)", "other art")).await;
    assert_ne!(hash_of(&r, app).await, h1);

    // A malformed hash is refused, not searched.
    assert!(matches!(
        r.svc.builds_with_hash("ABC").await,
        Err(ActivityError::Invalid("hash", _))
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_bundle_from_before_hashes_gets_one_from_its_stored_files() {
    let r = rig().await;
    let (app, _) = released_game(&r, "console.log(1)").await;
    let h = hash_of(&r, app).await;
    sqlx::query("UPDATE application_activity_bundles SET code_sha256 = NULL")
        .execute(r.db.pool())
        .await
        .unwrap();
    // The first trust read computes it again, from storage, and stores it.
    assert_eq!(hash_of(&r, app).await, h);
    sqlx::query("UPDATE application_activity_bundles SET code_sha256 = NULL")
        .execute(r.db.pool())
        .await
        .unwrap();
    assert_eq!(r.svc.backfill_code_hashes().await.unwrap(), 1);
    assert_eq!(r.svc.builds_with_hash(&h).await.unwrap().len(), 1);
    // Only code files are read: with the art gone the hash still comes out.
    for k in r.storage.list("activities/").await.unwrap() {
        if k.key.contains("/art/") {
            r.storage.delete(&k.key).await.unwrap();
        }
    }
    sqlx::query("UPDATE application_activity_bundles SET code_sha256 = NULL")
        .execute(r.db.pool())
        .await
        .unwrap();
    assert_eq!(hash_of(&r, app).await, h);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn connect_hosts_come_from_the_manifest_and_bad_ones_refuse_the_upload() {
    let r = rig().await;
    let owner = account(r.db.pool()).await;
    let app = application(r.db.pool(), owner, "Net").await;
    r.svc
        .save_config(
            app,
            ConfigInput {
                dev_entry_url: None,
                url_mappings: vec![UrlMapping {
                    prefix: "/api".into(),
                    target: "https://api.example.com".into(),
                }],
            },
        )
        .await
        .unwrap();
    let with = |manifest: &str| {
        zip_of(&[
            ("index.html", b"<p>"),
            ("app.js", b"1"),
            ("sideroom.json", manifest.as_bytes()),
        ])
    };
    for bad in [
        r#"{"connect":["http://assets.example.com"]}"#,
        r#"{"connect":["https://10.0.0.1"]}"#,
        r#"{"connect":["https://assets.example.com/path"]}"#,
    ] {
        let e = r
            .svc
            .upload_bundle(app, owner, with(bad))
            .await
            .unwrap_err();
        assert!(
            matches!(e, ActivityError::Invalid("bundle", _)),
            "{bad}: {e:?}"
        );
    }
    ship(
        &r,
        app,
        owner,
        with(r#"{"connect":["https://assets.example.com","self"],"capabilities":["storage"]}"#),
    )
    .await;
    match r.svc.lookup(app).await.unwrap() {
        Lookup::Ready(spec) => {
            assert_eq!(spec.connect, vec!["https://assets.example.com", "self"]);
        }
        other => panic!("{other:?}"),
    }
    let (_, connect) = r
        .svc
        .bundle_manifest(
            r.svc
                .builds_with_hash(&hash_of(&r, app).await)
                .await
                .unwrap()[0]
                .bundle_id,
        )
        .await
        .unwrap();
    assert_eq!(connect.len(), 2);

    // No manifest, no hosts.
    let (plain, _) = released_game(&r, "1").await;
    match r.svc.lookup(plain).await.unwrap() {
        Lookup::Ready(spec) => assert!(spec.connect.is_empty()),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_signature_counts_only_while_it_verifies_and_is_not_revoked() {
    let r = rig().await;
    let (app, _) = released_game(&r, "console.log(1)").await;
    let h = hash_of(&r, app).await;
    let t = r.svc.trust(app, None).await.unwrap();
    assert_eq!(t.headline, BadgeKind::Unsigned);
    assert!(t.signature.is_none() && !t.can_vouch);

    let key = operator_key(&r).await;
    // Only one key signs at a time.
    assert!(matches!(
        r.svc
            .add_signing_key(uid(), &trust::verify_key_hex(&[1; 32]), b"x")
            .await,
        Err(ActivityError::Conflict(_))
    ));
    sign(&r, app, key, &h).await;
    let t = r.svc.trust(app, None).await.unwrap();
    assert_eq!(t.headline, BadgeKind::SwarmSafetySigned);
    assert_eq!(
        t.signature.as_ref().unwrap().verify_key,
        trust::verify_key_hex(&key.1)
    );
    assert_eq!(t.signature.as_ref().unwrap().audit_version, 1);
    assert_eq!(t.code_sha256.as_deref(), Some(h.as_str()));

    // A signature made for another hash, stored under this one, does not verify: no badge.
    let other = "e".repeat(64);
    sqlx::query("UPDATE arcade_code_signatures SET signature = $1 WHERE application_id = $2")
        .bind(trust::sign_hex(&key.1, &other))
        .bind(app)
        .execute(r.db.pool())
        .await
        .unwrap();
    assert_eq!(
        r.svc.trust(app, None).await.unwrap().headline,
        BadgeKind::Unsigned
    );

    // Signing again repairs it; revoking removes it at once; a retired key stops counting.
    sign(&r, app, key, &h).await;
    assert_eq!(
        r.svc.trust(app, None).await.unwrap().headline,
        BadgeKind::SwarmSafetySigned
    );
    assert!(
        r.svc
            .revoke_signature(app, &h, "found a problem")
            .await
            .unwrap()
    );
    assert_eq!(
        r.svc.trust(app, None).await.unwrap().headline,
        BadgeKind::Unsigned
    );
    sign(&r, app, key, &h).await;
    assert_eq!(
        r.svc.trust(app, None).await.unwrap().headline,
        BadgeKind::SwarmSafetySigned
    );
    assert!(r.svc.retire_signing_key(key.0).await.unwrap());
    assert_eq!(
        r.svc.trust(app, None).await.unwrap().headline,
        BadgeKind::Unsigned
    );
    // A new key can be made once the old one is retired.
    r.svc
        .add_signing_key(uid(), &trust::verify_key_hex(&[2; 32]), b"x")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn vouches_are_pinned_to_the_hash_and_the_badge_flips_on_an_update() {
    let r = rig().await;
    let (app, owner) = released_game(&r, "console.log('v1')").await;
    let h1 = hash_of(&r, app).await;
    let key = operator_key(&r).await;
    sign(&r, app, key, &h1).await;

    let people = [
        veteran(r.db.pool()).await,
        veteran(r.db.pool()).await,
        veteran(r.db.pool()).await,
    ];
    for p in people {
        let t = r.svc.vouch(app, p, &h1).await.unwrap();
        assert!(t.vouched_by_me);
    }
    // Vouching twice changes nothing.
    r.svc.vouch(app, people[0], &h1).await.unwrap();
    let t = r.svc.trust(app, Some(people[0])).await.unwrap();
    assert_eq!((t.vouches, t.headline), (3, BadgeKind::SwarmSafetySigned));
    assert!(t.can_vouch && t.previous.is_none());
    // Someone who is not signed in sees the count and no button.
    let anon = r.svc.trust(app, None).await.unwrap();
    assert_eq!(
        (anon.vouches, anon.can_vouch, anon.vouched_by_me),
        (3, false, false)
    );

    // The developer ships different code. The hash changes; the old signature and vouches stay with the old
    // build; the new build has neither: the badge flips and remembers what people vouched for.
    ship(&r, app, owner, game("console.log('v2')", "a")).await;
    let h2 = hash_of(&r, app).await;
    assert_ne!(h1, h2);
    let t = r.svc.trust(app, Some(people[0])).await.unwrap();
    assert_eq!(t.headline, BadgeKind::UnverifiedUpdate);
    assert_eq!(t.vouches, 0);
    assert!(
        !t.vouched_by_me,
        "the old vouch does not follow the member to new code"
    );
    let prev = t.previous.unwrap();
    assert_eq!(
        (prev.code_sha256.as_str(), prev.vouches, prev.signed),
        (h1.as_str(), 3, true)
    );
    assert_eq!(prev.short, h1[..8]);

    // A vouch for the replaced build is refused: nobody vouches for code they did not see.
    assert!(matches!(
        r.svc.vouch(app, people[1], &h1).await,
        Err(ActivityError::Conflict(_))
    ));
    // One member vouches for the new build: the warning goes (it is unsigned, with a vouch).
    let t = r.svc.vouch(app, people[1], &h2).await.unwrap();
    assert_eq!((t.vouches, t.headline), (1, BadgeKind::Unsigned));
    // The operator signs the new build.
    sign(&r, app, key, &h2).await;
    assert_eq!(
        r.svc.trust(app, None).await.unwrap().headline,
        BadgeKind::SwarmSafetySigned
    );
    // Withdrawing removes the member's vouches.
    let t = r.svc.unvouch(app, people[1]).await.unwrap();
    assert_eq!((t.vouches, t.vouched_by_me), (0, false));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn only_members_in_good_standing_vouch_and_count() {
    let r = rig().await;
    let (app, _) = released_game(&r, "console.log(1)").await;
    let h = hash_of(&r, app).await;

    // A day-old account, a bot and a deactivated account cannot vouch.
    let fresh = account(r.db.pool()).await;
    assert!(matches!(
        r.svc.vouch(app, fresh, &h).await,
        Err(ActivityError::Forbidden)
    ));
    assert!(!r.svc.trust(app, Some(fresh)).await.unwrap().can_vouch);
    let bot = veteran(r.db.pool()).await;
    sqlx::query("UPDATE accounts SET is_bot = true WHERE id = $1")
        .bind(bot)
        .execute(r.db.pool())
        .await
        .unwrap();
    assert!(matches!(
        r.svc.vouch(app, bot, &h).await,
        Err(ActivityError::Forbidden)
    ));

    // A vouch stops counting when the member is no longer in good standing.
    let a = veteran(r.db.pool()).await;
    let b = veteran(r.db.pool()).await;
    r.svc.vouch(app, a, &h).await.unwrap();
    r.svc.vouch(app, b, &h).await.unwrap();
    assert_eq!(r.svc.trust(app, None).await.unwrap().vouches, 2);
    sqlx::query("UPDATE accounts SET state = 'platform_banned' WHERE id = $1")
        .bind(a)
        .execute(r.db.pool())
        .await
        .unwrap();
    assert_eq!(r.svc.trust(app, None).await.unwrap().vouches, 1);
    // A malformed hash is refused.
    assert!(matches!(
        r.svc.vouch(app, b, "nothex").await,
        Err(ActivityError::Invalid("code_sha256", _))
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_member_cannot_vouch_in_bulk() {
    let r = rig().await;
    let (app, _) = released_game(&r, "console.log(1)").await;
    let h = hash_of(&r, app).await;
    let farmer = veteran(r.db.pool()).await;
    // Thirty vouches already today (for other builds of the same game).
    for i in 0..30 {
        sqlx::query(
            "INSERT INTO arcade_build_vouches (application_id, code_sha256, account_id) \
             VALUES ($1, $2, $3)",
        )
        .bind(app)
        .bind(format!("{i:064x}"))
        .bind(farmer)
        .execute(r.db.pool())
        .await
        .unwrap();
    }
    assert!(matches!(
        r.svc.vouch(app, farmer, &h).await,
        Err(ActivityError::Conflict(_))
    ));
    // Someone else is not affected.
    let other = veteran(r.db.pool()).await;
    r.svc.vouch(app, other, &h).await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_badge_of_a_game_that_is_not_served_is_not_found() {
    let r = rig().await;
    let owner = account(r.db.pool()).await;
    let app = application(r.db.pool(), owner, "Draft").await;
    assert!(matches!(
        r.svc.trust(app, None).await,
        Err(ActivityError::NotFound(_))
    ));
    let (game, _) = released_game(&r, "1").await;
    set_state(&r, game, "suspended").await;
    assert!(matches!(
        r.svc.trust(game, None).await,
        Err(ActivityError::NotFound(_))
    ));
    assert!(matches!(
        r.svc.unvouch(game, owner).await,
        Err(ActivityError::NotFound(_))
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn featuring_shows_only_while_the_current_code_is_signed() {
    let r = rig().await;
    let (app, owner) = released_game(&r, "console.log('v1')").await;
    // Without the Arcade room's table nobody is featured; with it, the flag follows the row.
    assert!(!r.svc.trust(app, None).await.unwrap().featured);
    // The table (migration 2040 on the arcade-room branch) is not part of this branch: a stand-in.
    sqlx::query("CREATE TABLE IF NOT EXISTS arcade_featured (application_id BIGINT PRIMARY KEY)")
        .execute(r.db.pool())
        .await
        .unwrap();
    let key = operator_key(&r).await;
    let h1 = hash_of(&r, app).await;
    sign(&r, app, key, &h1).await;
    sqlx::query("INSERT INTO arcade_featured (application_id) VALUES ($1)")
        .bind(app)
        .execute(r.db.pool())
        .await
        .unwrap();
    let t = r.svc.trust(app, None).await.unwrap();
    assert!(t.featured);
    assert_eq!(t.headline, BadgeKind::OperatorFeatured);
    assert_eq!(
        t.badges,
        vec![BadgeKind::OperatorFeatured, BadgeKind::SwarmSafetySigned]
    );
    // The developer updates: still featured by the operator, but the new code is unsigned.
    ship(&r, app, owner, game("console.log('v2')", "a")).await;
    let t = r.svc.trust(app, None).await.unwrap();
    assert!(t.featured);
    assert_eq!(t.headline, BadgeKind::UnverifiedUpdate);
    assert!(!t.badges.contains(&BadgeKind::OperatorFeatured));
}
