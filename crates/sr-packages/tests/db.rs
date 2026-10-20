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

/// Records the review-queue card calls (D-258).
#[derive(Default)]
struct Cards(std::sync::Mutex<Vec<(i64, Option<String>)>>);

#[async_trait::async_trait]
impl sr_packages::service::PackageEvents for Cards {
    async fn card(&self, version_id: i64, outcome: Option<&str>) {
        self.0
            .lock()
            .unwrap()
            .push((version_id, outcome.map(str::to_owned)));
    }
}

async fn rig_with_cards() -> (Rig, Arc<Cards>) {
    let db = TestDb::new().await.unwrap();
    let storage: Arc<dyn ObjectStorage> = Arc::new(ObjectStoreStorage::in_memory());
    let pool = db.pool().clone();
    let cards = Arc::new(Cards::default());
    let svc = PackageService::new(pool.clone(), Arc::new(uid))
        .with_storage(storage)
        .with_events(cards.clone());
    (Rig { _db: db, pool, svc }, cards)
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn nobody_reviews_their_own_package_and_each_version_posts_one_card() {
    let (r, cards) = rig_with_cards().await;
    let author = account(&r.pool).await;
    let staff = account(&r.pool).await;
    let v = r
        .svc
        .submit(
            author,
            theme_zip("ocean-night", "1.0.0"),
            &Checks::lenient(),
        )
        .await
        .unwrap();
    let vid: i64 = v.id.parse().unwrap();
    assert_eq!(*cards.0.lock().unwrap(), vec![(vid, None)]);
    // The author is refused, whatever the decision, and nothing changes.
    for d in [Decision::Approve, Decision::Reject, Decision::Revoke] {
        assert_eq!(
            r.svc
                .decide(vid, author, d, Some("x".into()))
                .await
                .unwrap_err(),
            PackageError::SelfReview
        );
    }
    assert_eq!(cards.0.lock().unwrap().len(), 1);
    r.svc
        .decide(vid, staff, Decision::Approve, None)
        .await
        .unwrap();
    assert_eq!(
        cards.0.lock().unwrap().last().unwrap(),
        &(vid, Some("approved".to_owned()))
    );
    // A second decision on the same version is a conflict and posts no second outcome.
    assert!(
        r.svc
            .decide(vid, staff, Decision::Approve, None)
            .await
            .is_err()
    );
    assert_eq!(cards.0.lock().unwrap().len(), 2);
    // A rejection and a withdrawal close their cards too.
    let v2 = r
        .svc
        .submit(
            author,
            theme_zip("ocean-night", "1.0.1"),
            &Checks::lenient(),
        )
        .await
        .unwrap();
    let v2id: i64 = v2.id.parse().unwrap();
    r.svc
        .decide(v2id, staff, Decision::Reject, Some("no".into()))
        .await
        .unwrap();
    let v3 = r
        .svc
        .submit(
            author,
            theme_zip("ocean-night", "1.0.2"),
            &Checks::lenient(),
        )
        .await
        .unwrap();
    let v3id: i64 = v3.id.parse().unwrap();
    r.svc.withdraw(author, v3id).await.unwrap();
    let log = cards.0.lock().unwrap().clone();
    assert!(log.contains(&(v2id, Some("rejected".to_owned()))));
    assert_eq!(log.last().unwrap(), &(v3id, Some("withdrawn".to_owned())));
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn an_author_deletes_a_never_approved_package_and_frees_the_slug() {
    let (r, cards) = rig_with_cards().await;
    let author = account(&r.pool).await;
    let other = account(&r.pool).await;
    let staff = account(&r.pool).await;
    let v = r
        .svc
        .submit(
            author,
            theme_zip("ocean-night", "1.0.0"),
            &Checks::lenient(),
        )
        .await
        .unwrap();
    let pid: i64 = v.package_id.parse().unwrap();
    let vid: i64 = v.id.parse().unwrap();
    // Not somebody else's.
    assert_eq!(
        r.svc.delete_package(other, pid).await.unwrap_err(),
        PackageError::NotFound("package")
    );
    // The pending version is withdrawn by the same call; the slug is free for another author.
    r.svc.delete_package(author, pid).await.unwrap();
    assert!(
        cards
            .0
            .lock()
            .unwrap()
            .contains(&(vid, Some("withdrawn".to_owned())))
    );
    assert!(r.svc.mine(author).await.unwrap().is_empty());
    r.svc
        .submit(other, theme_zip("ocean-night", "1.0.0"), &Checks::lenient())
        .await
        .unwrap();

    // Rejected-only packages can go too; a package with an approved version cannot.
    let v = r
        .svc
        .submit(author, theme_zip("night-owl", "1.0.0"), &Checks::lenient())
        .await
        .unwrap();
    let (pid, vid): (i64, i64) = (v.package_id.parse().unwrap(), v.id.parse().unwrap());
    r.svc
        .decide(vid, staff, Decision::Approve, None)
        .await
        .unwrap();
    assert_eq!(
        r.svc.delete_package(author, pid).await.unwrap_err(),
        PackageError::Conflict("Approved packages can only be revoked by staff".to_owned())
    );
    assert_eq!(r.svc.mine(author).await.unwrap().len(), 1);
    // Once staff revoke it, nothing approved is left and the author may delete it.
    r.svc
        .decide(vid, staff, Decision::Revoke, Some("broke the rules".into()))
        .await
        .unwrap();
    r.svc.delete_package(author, pid).await.unwrap();
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn reserved_ids_are_refused_at_submission() {
    let r = rig().await;
    let author = account(&r.pool).await;
    for slug in ["sideroom", "lounge-dark", "default"] {
        let err = r
            .svc
            .submit(author, recipe_zip(slug, "1.0.0"), &Checks::lenient())
            .await
            .unwrap_err();
        assert!(
            matches!(&err, PackageError::Refused(p) if p.iter().any(|m| m.contains("reserved"))),
            "{slug}: {err:?}"
        );
    }
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn profile_pictures_are_capped_owned_and_dropped_when_a_saved_document_stops_using_them() {
    use sr_packages::service::media::NewMedia;
    let db = TestDb::new().await.unwrap();
    let storage = Arc::new(ObjectStoreStorage::in_memory());
    let pool = db.pool().clone();
    let svc = PackageService::new(pool.clone(), Arc::new(uid)).with_storage(storage.clone());
    let me = account(&pool).await;
    let other = account(&pool).await;

    let add = |user: i64, kind: &'static str| {
        let svc = svc.clone();
        let storage = storage.clone();
        async move {
            let id = svc.media_id();
            let key = format!("avatars/{user}/{id:032x}");
            storage
                .put(&key, bytes::Bytes::from_static(b"png"))
                .await
                .unwrap();
            let v = svc
                .media_add(
                    user,
                    NewMedia {
                        id,
                        kind: kind.to_owned(),
                        object_key: key.clone(),
                        content_type: "image/png".into(),
                        width: 640,
                        height: 480,
                        bytes: 3,
                    },
                )
                .await;
            (v, key)
        }
    };

    // Twelve gallery pictures fit, the thirteenth does not; the check can say so before processing.
    let mut gallery = Vec::new();
    for _ in 0..12 {
        let (v, key) = add(me, "gallery").await;
        gallery.push((v.unwrap(), key));
    }
    let (thirteenth, _) = add(me, "gallery").await;
    assert!(matches!(thirteenth, Err(PackageError::Conflict(m)) if m.contains("at most 12")));
    assert!(svc.media_check(me, "gallery").await.is_err());
    assert!(svc.media_check(me, "background").await.is_ok());
    assert!(svc.media_check(me, "video").await.is_err());
    assert!(
        gallery[0].0.url.starts_with(&format!("/cdn/avatars/{me}/"))
            && gallery[0].0.url.ends_with(".png")
    );

    // A new background replaces the old one and its file goes.
    let (first_bg, first_key) = add(me, "background").await;
    let first_bg = first_bg.unwrap();
    let (second_bg, _) = add(me, "background").await;
    let second_bg = second_bg.unwrap();
    assert!(
        storage.head(&first_key).await.is_err(),
        "replaced background file removed"
    );
    let all = svc.media_list(me).await.unwrap();
    assert_eq!(all.iter().filter(|m| m.kind == "background").count(), 1);
    assert_eq!(all.len(), 13);

    // A document may name only my own pictures.
    let (theirs, _) = add(other, "gallery").await;
    let theirs = theirs.unwrap();
    let doc = |bg: &str, imgs: &[&str]| -> Value {
        let items: Vec<Value> = imgs.iter().map(|i| json!({"image_id": i})).collect();
        json!({"version": 1,
               "style": {"background": {"kind": "image", "image_id": bg}},
               "blocks": [{"id": "g", "type": "gallery", "x": 0, "y": 0, "w": 6, "h": 3, "config": {"items": items}}]})
    };
    let err = svc
        .set_doc(me, &doc(&second_bg.id, &[&theirs.id]))
        .await
        .unwrap_err();
    assert!(
        matches!(&err, PackageError::Refused(p) if p.iter().any(|m| m.contains("not one of your pictures"))),
        "{err:?}"
    );
    let stale = svc
        .set_doc(me, &doc(&first_bg.id, &[&gallery[0].0.id]))
        .await
        .unwrap_err();
    assert!(
        matches!(stale, PackageError::Refused(_)),
        "the replaced background is gone"
    );

    // Saving keeps what the document names and drops (rows and files) every other picture.
    svc.set_doc(
        me,
        &doc(&second_bg.id, &[&gallery[0].0.id, &gallery[1].0.id]),
    )
    .await
    .unwrap();
    let kept: Vec<String> = svc
        .media_list(me)
        .await
        .unwrap()
        .into_iter()
        .map(|m| m.id)
        .collect();
    assert_eq!(kept.len(), 3);
    assert!(storage.head(&gallery[0].1).await.is_ok());
    assert!(
        storage.head(&gallery[5].1).await.is_err(),
        "orphan file deleted on save"
    );
    // The page lists exactly the pictures the document names, for anyone who may see it.
    let page = svc.page(other, None, Some(me)).await.unwrap();
    assert_eq!(page.media.len(), 3);
    assert!(page.guestbook_count.is_none());

    // Deleting a picture takes it out of the saved document too.
    svc.media_delete(me, gallery[1].0.id.parse().unwrap())
        .await
        .unwrap();
    assert!(storage.head(&gallery[1].1).await.is_err());
    let after = svc.own_doc(me).await.unwrap();
    assert_eq!(after.blocks[0].config["items"].as_array().unwrap().len(), 1);
    // Not somebody else's.
    assert_eq!(
        svc.media_delete(me, theirs.id.parse().unwrap())
            .await
            .unwrap_err(),
        PackageError::NotFound("picture")
    );
    assert_eq!(svc.media_keys(other).await.unwrap().len(), 1);
    db.cleanup().await.unwrap();
}

async fn guild(pool: &PgPool, owner: i64) -> i64 {
    let id = uid();
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'Book Club', $2)")
        .bind(id)
        .bind(owner)
        .execute(pool)
        .await
        .unwrap();
    id
}

async fn approved(r: &Rig, author: i64, staff: i64, zip: Vec<u8>) -> (i64, i64) {
    let v = r.svc.submit(author, zip, &Checks::lenient()).await.unwrap();
    let (pid, vid): (i64, i64) = (v.package_id.parse().unwrap(), v.id.parse().unwrap());
    r.svc
        .decide(vid, staff, Decision::Approve, None)
        .await
        .unwrap();
    (pid, vid)
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn a_server_gets_a_theme_an_accent_colour_and_pack_items_it_can_take_back() {
    let r = rig().await;
    let author = account(&r.pool).await;
    let staff = account(&r.pool).await;
    let owner = account(&r.pool).await;
    let g = guild(&r.pool, owner).await;

    // Nothing chosen: the built-in look.
    let a = r.svc.appearance(g).await.unwrap();
    assert!(a.theme.is_none() && a.accent_color.is_none());

    // A theme turned on for the server shows up for members, with its tokens.
    let (theme, _) = approved(&r, author, staff, theme_zip("ocean-night", "1.0.0")).await;
    r.svc
        .enable("guild", g, theme, "all", json!({}), owner)
        .await
        .unwrap();
    let a = r.svc.appearance(g).await.unwrap();
    let t = a.theme.expect("a server theme");
    assert_eq!(t.slug, "ocean-night");
    assert_eq!(t.scheme, "dark");
    assert_eq!(t.tokens["--ui-accent"], "#5865f2");
    // `all` is stored as `lounge` for a server (D-281); any other section is refused.
    let en = r.svc.enabled("guild", g).await.unwrap();
    assert_eq!(
        en.iter()
            .find(|e| e.package_id == theme.to_string())
            .unwrap()
            .section,
        "lounge"
    );
    assert!(
        r.svc
            .enable("guild", g, theme, "cinema", json!({}), owner)
            .await
            .is_err()
    );

    // The accent colour is set, replaced, cleared, and refused outside 0..=0xFFFFFF.
    r.svc.set_accent(g, Some(0x336699), owner).await.unwrap();
    assert_eq!(
        r.svc.appearance(g).await.unwrap().accent_color,
        Some(0x336699)
    );
    r.svc.set_accent(g, Some(0x112233), owner).await.unwrap();
    assert_eq!(
        r.svc.appearance(g).await.unwrap().accent_color,
        Some(0x112233)
    );
    r.svc.set_accent(g, None, owner).await.unwrap();
    assert_eq!(r.svc.appearance(g).await.unwrap().accent_color, None);
    assert!(r.svc.set_accent(g, Some(0x0100_0000), owner).await.is_err());
    assert!(r.svc.set_accent(g, Some(-1), owner).await.is_err());

    // Turning the theme off returns to the built-in look.
    r.svc.disable("guild", g, theme).await.unwrap();
    assert!(r.svc.appearance(g).await.unwrap().theme.is_none());

    // A pack's images are read out of its zip, and the items a server got from it are remembered.
    let (pack, vid) = approved(&r, author, staff, emoji_zip("faces")).await;
    let en = r
        .svc
        .enable("guild", g, pack, "all", json!({}), owner)
        .await
        .unwrap();
    assert_eq!(en.version_id, vid.to_string());
    let items = r.svc.pack_contents(vid).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name, "wave");
    assert!(items[0].bytes.starts_with(b"\x89PNG"));
    assert!(r.svc.pack_contents(i64::MAX).await.is_err());
    r.svc
        .record_pack_items(g, pack, "emoji", &[7, 8], owner)
        .await
        .unwrap();
    r.svc
        .record_pack_items(g, pack, "emoji", &[7], owner)
        .await
        .unwrap();
    assert_eq!(
        r.svc.pack_items(g, pack).await.unwrap(),
        vec![("emoji".to_owned(), 7), ("emoji".to_owned(), 8)]
    );
    r.svc.forget_pack_items(g, pack).await.unwrap();
    assert!(r.svc.pack_items(g, pack).await.unwrap().is_empty());

    // A theme is not a pack.
    assert!(matches!(
        r.svc
            .pack_contents(
                approved(&r, author, staff, theme_zip("other-theme", "1.0.0"))
                    .await
                    .1
            )
            .await
            .unwrap_err(),
        PackageError::Conflict(_)
    ));
}

#[tokio::test]
#[ignore = "needs Postgres"]
async fn the_sweep_removes_only_old_unreferenced_profile_pictures() {
    use sr_packages::service::media::NewMedia;
    let db = TestDb::new().await.unwrap();
    let storage = Arc::new(ObjectStoreStorage::in_memory());
    let pool = db.pool().clone();
    let svc = PackageService::new(pool.clone(), Arc::new(uid)).with_storage(storage.clone());
    let me = account(&pool).await;

    let mut pics = Vec::new();
    for _ in 0..3 {
        let id = svc.media_id();
        let key = format!("avatars/{me}/{id:032x}");
        storage
            .put(&key, bytes::Bytes::from_static(b"png"))
            .await
            .unwrap();
        svc.media_add(
            me,
            NewMedia {
                id,
                kind: "gallery".into(),
                object_key: key.clone(),
                content_type: "image/png".into(),
                width: 64,
                height: 64,
                bytes: 3,
            },
        )
        .await
        .unwrap();
        pics.push((id, key));
    }
    // The saved document names pics[0]; pics[0] and pics[1] are two days old, pics[2] is fresh.
    let doc = json!({"version": 1, "style": {}, "blocks": [
        {"id": "g", "type": "gallery", "x": 0, "y": 0, "w": 6, "h": 3,
         "config": {"items": [{"image_id": pics[0].0.to_string()}]}}]});
    sqlx::query("UPDATE accounts SET profile_doc = $2 WHERE id = $1")
        .bind(me)
        .bind(&doc)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE profile_media SET created_at = now() - interval '2 days' WHERE id = ANY($1)",
    )
    .bind([pics[0].0, pics[1].0].to_vec())
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(
        svc.sweep_unreferenced_media(24 * 3600, 100).await.unwrap(),
        1
    );
    assert!(storage.head(&pics[0].1).await.is_ok(), "referenced stays");
    assert!(storage.head(&pics[1].1).await.is_err(), "old orphan goes");
    assert!(storage.head(&pics[2].1).await.is_ok(), "fresh orphan stays");
    let left: Vec<String> = svc
        .media_list(me)
        .await
        .unwrap()
        .into_iter()
        .map(|m| m.id)
        .collect();
    assert_eq!(left.len(), 2);
    // A second run finds nothing to do.
    assert_eq!(
        svc.sweep_unreferenced_media(24 * 3600, 100).await.unwrap(),
        0
    );
    db.cleanup().await.unwrap();
}
