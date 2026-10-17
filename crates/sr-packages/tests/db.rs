//! Postgres-backed tests of the packages service: submission, the review that gates everything, enablements,
//! profile documents and the guestbook. Run with a scratch Postgres that may CREATE DATABASE:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-packages --test db -- --ignored --test-threads=1`

use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

use serde_json::{Value, json};
use sr_db::{
    PgPool,
    accounts::{self, NewAccount},
    testing::TestDb,
};
use sr_packages::service::{Decision, PackageService};
use sr_packages::zipfiles::build_zip;
use sr_packages::{Checks, PackageError, PackageKind};
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

async fn befriend(pool: &PgPool, a: i64, b: i64) {
    sqlx::query("INSERT INTO relationships (user_id, target_id, type) VALUES ($1,$2,1),($2,$1,1)")
        .bind(a)
        .bind(b)
        .execute(pool)
        .await
        .unwrap();
}

struct Rig {
    _db: TestDb,
    pool: PgPool,
    svc: PackageService,
}

async fn rig() -> Rig {
    let db = TestDb::new().await.unwrap();
    let storage: Arc<dyn ObjectStorage> = Arc::new(ObjectStoreStorage::in_memory());
    let pool = db.pool().clone();
    let svc = PackageService::new(pool.clone(), Arc::new(uid)).with_storage(storage);
    Rig { _db: db, pool, svc }
}

fn recipe_zip(slug: &str, version: &str) -> Vec<u8> {
    let m = json!({
        "id": slug, "name": "Starter", "version": version, "author": "me",
        "features": [{"id": "lounge.starboard", "enabled": true}]
    });
    build_zip(&[("recipe.json", m.to_string().as_bytes())])
}

fn theme_zip(slug: &str, version: &str) -> Vec<u8> {
    let m = json!({"id": slug, "name": "Ocean", "version": version, "author": "me",
                   "targets": ["lounge"], "scheme": "dark"});
    let t = json!({"--ui-accent": "#5865f2"});
    build_zip(&[
        ("theme.json", m.to_string().as_bytes()),
        ("tokens.json", t.to_string().as_bytes()),
    ])
}

fn emoji_zip(slug: &str) -> Vec<u8> {
    let m = json!({"kind": "emoji_pack", "id": slug, "name": "Faces", "version": "1.0.0", "author": "me",
                   "items": [{"name": "wave", "file": "emoji/wave.png"}]});
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend_from_slice(&64u32.to_be_bytes());
    png.extend_from_slice(&64u32.to_be_bytes());
    png.extend_from_slice(&[8, 6, 0, 0, 0]);
    build_zip(&[
        ("pack.json", m.to_string().as_bytes()),
        ("emoji/wave.png", &png),
    ])
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn a_package_is_reviewed_before_anyone_can_use_it() {
    let r = rig().await;
    let author = account(&r.pool).await;
    let staff = account(&r.pool).await;
    let user = account(&r.pool).await;
    let checks = Checks::lenient();

    let v = r
        .svc
        .submit(author, theme_zip("ocean-night", "1.0.0"), &checks)
        .await
        .unwrap();
    assert_eq!(v.review_state, "pending");
    assert_eq!(v.kind, PackageKind::Theme);
    // Not in the directory, not enableable.
    assert!(r.svc.directory(None, None, None).await.unwrap().is_empty());
    let pid: i64 = v.package_id.parse().unwrap();
    let vid: i64 = v.id.parse().unwrap();
    assert_eq!(
        r.svc
            .enable("user", user, pid, "all", json!({}), user)
            .await
            .unwrap_err(),
        PackageError::NotFound("package")
    );
    // The same slug from another author is refused, and so is a repeated version.
    let other = account(&r.pool).await;
    assert!(matches!(
        r.svc
            .submit(other, theme_zip("ocean-night", "1.0.1"), &checks)
            .await
            .unwrap_err(),
        PackageError::Invalid("id", _)
    ));
    assert!(matches!(
        r.svc
            .submit(author, theme_zip("ocean-night", "1.0.0"), &checks)
            .await
            .unwrap_err(),
        PackageError::Conflict(_)
    ));
    // A rejection needs a reason.
    assert!(
        r.svc
            .decide(vid, staff, Decision::Reject, None)
            .await
            .is_err()
    );
    assert_eq!(r.svc.pending(10).await.unwrap().len(), 1);

    r.svc
        .decide(vid, staff, Decision::Approve, None)
        .await
        .unwrap();
    assert!(matches!(
        r.svc
            .decide(vid, staff, Decision::Approve, None)
            .await
            .unwrap_err(),
        PackageError::Conflict(_)
    ));
    let dir = r
        .svc
        .directory(Some(PackageKind::Theme), None, None)
        .await
        .unwrap();
    assert_eq!(dir.len(), 1);
    assert_eq!(dir[0].author_id, author.to_string());
    assert!(
        dir[0].payload.as_ref().unwrap()["css"]
            .as_str()
            .unwrap()
            .contains("ocean-night")
    );
    assert!(
        r.svc
            .directory(Some(PackageKind::Recipe), None, None)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        r.svc
            .directory(None, Some("ocean"), None)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        r.svc
            .directory(None, Some("zzz%"), None)
            .await
            .unwrap()
            .is_empty()
    );

    // One theme per section: the second replaces the first.
    let e = r
        .svc
        .enable("user", user, pid, "all", json!({}), user)
        .await
        .unwrap();
    assert_eq!(e.section, "all");
    let v2 = r
        .svc
        .submit(author, theme_zip("sunset-glow", "1.0.0"), &checks)
        .await
        .unwrap();
    r.svc
        .decide(v2.id.parse().unwrap(), staff, Decision::Approve, None)
        .await
        .unwrap();
    r.svc
        .enable(
            "user",
            user,
            v2.package_id.parse().unwrap(),
            "all",
            json!({}),
            user,
        )
        .await
        .unwrap();
    let on = r.svc.enabled("user", user).await.unwrap();
    assert_eq!(on.len(), 1);
    assert_eq!(on[0].slug, "sunset-glow");
    // A section of its own keeps both.
    r.svc
        .enable("user", user, pid, "cinema", json!({}), user)
        .await
        .unwrap();
    assert_eq!(r.svc.enabled("user", user).await.unwrap().len(), 2);

    // Revoking takes the version out of the directory and out of every enablement.
    r.svc
        .decide(vid, staff, Decision::Revoke, Some("broke the rules".into()))
        .await
        .unwrap();
    assert_eq!(r.svc.directory(None, None, None).await.unwrap().len(), 1);
    assert_eq!(r.svc.enabled("user", user).await.unwrap().len(), 1);

    // Scope rules: a user cannot enable a recipe, nothing is enabled for an unknown scope.
    let rv = r
        .svc
        .submit(author, recipe_zip("club-starter", "1.0.0"), &checks)
        .await
        .unwrap();
    r.svc
        .decide(rv.id.parse().unwrap(), staff, Decision::Approve, None)
        .await
        .unwrap();
    let rid: i64 = rv.package_id.parse().unwrap();
    assert!(matches!(
        r.svc
            .enable("user", user, rid, "all", json!({}), user)
            .await
            .unwrap_err(),
        PackageError::Conflict(_)
    ));
    assert!(
        r.svc
            .enable("guild", 5, rid, "all", json!({"applied": []}), user)
            .await
            .is_ok()
    );
    assert_eq!(r.svc.enabled("guild", 5).await.unwrap().len(), 1);
    r.svc.disable("guild", 5, rid).await.unwrap();
    assert!(r.svc.disable("guild", 5, rid).await.is_err());
    let (kind, payload) = r
        .svc
        .approved_payload(rv.id.parse().unwrap())
        .await
        .unwrap();
    assert_eq!(kind, PackageKind::Recipe);
    assert_eq!(payload["features"][0]["id"], "lounge.starboard");
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn withdrawing_and_updates() {
    let r = rig().await;
    let author = account(&r.pool).await;
    let staff = account(&r.pool).await;
    let checks = Checks::lenient();
    let v1 = r
        .svc
        .submit(author, recipe_zip("club-starter", "1.0.0"), &checks)
        .await
        .unwrap();
    let w = r
        .svc
        .withdraw(author, v1.id.parse().unwrap())
        .await
        .unwrap();
    assert_eq!(w.review_state, "withdrawn");
    assert!(
        r.svc
            .withdraw(author, v1.id.parse().unwrap())
            .await
            .is_err()
    );
    let stranger = account(&r.pool).await;
    let v2 = r
        .svc
        .submit(author, recipe_zip("club-starter", "1.1.0"), &checks)
        .await
        .unwrap();
    assert!(
        r.svc
            .withdraw(stranger, v2.id.parse().unwrap())
            .await
            .is_err()
    );
    r.svc
        .decide(v2.id.parse().unwrap(), staff, Decision::Approve, None)
        .await
        .unwrap();
    let mine = r.svc.mine(author).await.unwrap();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].versions.len(), 2);

    // A newer approved version marks an enablement as outdated until the person updates.
    let guild = 77;
    let pid: i64 = v2.package_id.parse().unwrap();
    let e = r
        .svc
        .enable("guild", guild, pid, "all", json!({}), author)
        .await
        .unwrap();
    assert!(!e.update_available);
    let v3 = r
        .svc
        .submit(author, recipe_zip("club-starter", "2.0.0"), &checks)
        .await
        .unwrap();
    r.svc
        .decide(v3.id.parse().unwrap(), staff, Decision::Approve, None)
        .await
        .unwrap();
    assert!(r.svc.enabled("guild", guild).await.unwrap()[0].update_available);
    let e = r
        .svc
        .enable("guild", guild, pid, "all", json!({}), author)
        .await
        .unwrap();
    assert_eq!(e.version, "2.0.0");
    assert!(!e.update_available);
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn pack_files_are_served_only_from_approved_versions() {
    let r = rig().await;
    let author = account(&r.pool).await;
    let staff = account(&r.pool).await;
    let v = r
        .svc
        .submit(author, emoji_zip("faces-pack"), &Checks::lenient())
        .await
        .unwrap();
    let vid: i64 = v.id.parse().unwrap();
    assert!(r.svc.file(vid, "emoji/wave.png").await.is_err());
    r.svc
        .decide(vid, staff, Decision::Approve, None)
        .await
        .unwrap();
    let (data, ct) = r.svc.file(vid, "emoji/wave.png").await.unwrap();
    assert_eq!(ct, "image/png");
    assert!(data.starts_with(b"\x89PNG"));
    assert!(r.svc.file(vid, "pack.json").await.is_err());
    assert!(r.svc.file(vid, "../x.png").await.is_err());
    assert!(r.svc.file(vid, "emoji/other.png").await.is_err());
    // The reviewer can download what was uploaded.
    let (_, zip) = r.svc.zip(vid).await.unwrap();
    assert!(zip.starts_with(b"PK"));
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn profile_documents_pages_and_guestbook() {
    let r = rig().await;
    let me = account(&r.pool).await;
    let friend = account(&r.pool).await;
    let stranger = account(&r.pool).await;
    befriend(&r.pool, me, friend).await;

    let doc = |friends: Vec<String>| -> Value {
        json!({"version": 1, "blocks": [
            {"id": "head", "type": "header", "x": 0, "y": 0, "w": 12, "h": 3, "config": {"tagline": "hello"}},
            {"id": "fr", "type": "top_friends", "x": 0, "y": 3, "w": 6, "h": 4, "config": {"user_ids": friends}},
            {"id": "gb", "type": "guestbook", "x": 6, "y": 3, "w": 6, "h": 4}
        ]})
    };
    // Top friends must be friends.
    let err = r
        .svc
        .set_doc(me, &doc(vec![stranger.to_string()]))
        .await
        .unwrap_err();
    assert!(matches!(err, PackageError::Invalid("blocks", _)));
    // Invalid documents list every problem.
    let bad = json!({"blocks": [{"id": "a", "type": "about", "x": 9, "y": 0, "w": 6, "h": 2, "config": {"text": "x"}}]});
    assert!(
        matches!(r.svc.set_doc(me, &bad).await.unwrap_err(), PackageError::Refused(p) if !p.is_empty())
    );
    r.svc
        .set_doc(me, &doc(vec![friend.to_string()]))
        .await
        .unwrap();
    assert_eq!(r.svc.own_doc(me).await.unwrap().blocks.len(), 3);

    let me_name: String = sqlx::query_scalar("SELECT username::text FROM accounts WHERE id = $1")
        .bind(me)
        .fetch_one(&r.pool)
        .await
        .unwrap();
    let page = r
        .svc
        .page(stranger, Some(&me_name.to_uppercase()), None)
        .await
        .unwrap();
    assert!(page.customised && !page.is_owner && !page.can_sign);
    assert_eq!(page.friends.len(), 1);
    assert_eq!(page.friends[0].id, friend.to_string());
    // A friend who is removed no longer shows in Top friends.
    // Guestbook: friends and the owner only.
    assert!(matches!(
        r.svc.sign(stranger, me, "hi").await.unwrap_err(),
        PackageError::Forbidden
    ));
    let e = r.svc.sign(friend, me, "  nice page  ").await.unwrap();
    assert_eq!(e.body, "nice page");
    assert!(r.svc.sign(friend, me, "").await.is_err());
    assert!(r.svc.sign(friend, me, &"x".repeat(501)).await.is_err());
    assert_eq!(r.svc.guestbook(stranger, me, None).await.unwrap().len(), 1);
    // A stranger cannot remove it; the author and the owner can.
    let eid: i64 = e.id.parse().unwrap();
    assert!(r.svc.remove_entry(stranger, me, eid).await.is_err());
    r.svc.remove_entry(friend, me, eid).await.unwrap();
    let e2 = r.svc.sign(friend, me, "again").await.unwrap();
    r.svc
        .remove_entry(me, me, e2.id.parse().unwrap())
        .await
        .unwrap();
    assert!(r.svc.guestbook(me, me, None).await.unwrap().is_empty());
    // The rate limit.
    for i in 0..5 {
        r.svc.sign(friend, me, &format!("n{i}")).await.unwrap();
    }
    assert!(matches!(
        r.svc.sign(friend, me, "six").await.unwrap_err(),
        PackageError::Conflict(_)
    ));
    // Blocking hides the page both ways.
    sqlx::query("INSERT INTO relationships (user_id, target_id, type) VALUES ($1, $2, 2)")
        .bind(stranger)
        .bind(me)
        .execute(&r.pool)
        .await
        .unwrap();
    assert!(r.svc.page(stranger, None, Some(me)).await.is_err());
    assert!(r.svc.page(me, None, Some(stranger)).await.is_err());
    assert!(r.svc.guestbook(stranger, me, None).await.is_err());
    // No guestbook block, no signing.
    r.svc.set_doc(me, &json!({"blocks": []})).await.unwrap();
    assert!(matches!(
        r.svc.sign(friend, me, "late").await.unwrap_err(),
        PackageError::Conflict(_)
    ));
    // Unknown people.
    assert!(r.svc.page(me, Some("nobody.here"), None).await.is_err());
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn a_layout_package_starts_a_profile_and_keeps_text() {
    let r = rig().await;
    let author = account(&r.pool).await;
    let staff = account(&r.pool).await;
    let me = account(&r.pool).await;
    let layout = json!({
        "id": "retro-two-column", "name": "Retro", "version": "1.0.0", "author": "me",
        "doc": {"style": {"overlay": "stars", "border": "ridge"}, "blocks": [
            {"id": "a", "type": "about", "x": 0, "y": 0, "w": 6, "h": 4},
            {"id": "g", "type": "gallery", "x": 6, "y": 0, "w": 6, "h": 4}]}
    });
    // "ridge" is not an approved border.
    let zip = build_zip(&[("layout.json", layout.to_string().as_bytes())]);
    assert!(matches!(
        r.svc
            .submit(author, zip, &Checks::lenient())
            .await
            .unwrap_err(),
        PackageError::Refused(_)
    ));
    let mut layout = layout;
    layout["doc"]["style"]["border"] = json!("double");
    let zip = build_zip(&[("layout.json", layout.to_string().as_bytes())]);
    let v = r.svc.submit(author, zip, &Checks::lenient()).await.unwrap();
    let vid: i64 = v.id.parse().unwrap();
    // Not before approval.
    assert!(r.svc.apply_layout(me, vid).await.is_err());
    r.svc
        .decide(vid, staff, Decision::Approve, None)
        .await
        .unwrap();

    r.svc
        .set_doc(me, &json!({"blocks": [{"id": "x", "type": "about", "x": 0, "y": 0, "w": 12, "h": 3, "config": {"text": "mine"}}]}))
        .await
        .unwrap();
    let d = r.svc.apply_layout(me, vid).await.unwrap();
    assert_eq!(d.layout.as_deref(), Some("retro-two-column"));
    assert_eq!(d.style.overlay, "stars");
    assert_eq!(d.blocks.len(), 2);
    assert_eq!(d.blocks[0].config["text"], "mine");
    assert!(d.blocks[1].config.is_empty());
}
