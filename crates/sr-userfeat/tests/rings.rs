//! Postgres tests of the status rings (PRF-RNG-001): who sees a ring (friend, non-friend, guild member of a chosen
//! and of an unchosen server, stranger, blocked), the author-only viewer list, the hard expiry, the moderation-hold
//! exception, the daily limit, the data export and the erasure. Run with a scratch Postgres that may CREATE DATABASE:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-userfeat --test rings -- --ignored --test-threads=1`

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicI64, Ordering},
};

use async_trait::async_trait;
use sr_db::{
    PgPool,
    accounts::{self, NewAccount},
    testing::TestDb,
};
use sr_features::{FeatureRegistry, FeatureService, InMemorySettingsStore, ScopeRef};
use sr_userfeat::rings::{
    NewRing, ObjectFailure, RingAudienceKind, RingKind, RingObject, RingObjects, RingTrack,
    ring_audience, ring_visible_to, viewer_may_see_a_ring_of,
};
use sr_userfeat::{UfError, UserFeatures, features::*};

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

async fn guild(pool: &PgPool, owner: i64, members: &[i64]) -> i64 {
    let g = uid();
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'Test guild', $2)")
        .bind(g)
        .bind(owner)
        .execute(pool)
        .await
        .unwrap();
    for m in std::iter::once(&owner).chain(members) {
        sqlx::query(
            "INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(g)
        .bind(*m)
        .execute(pool)
        .await
        .unwrap();
    }
    g
}

/// Two `friend` rows: a mutual friendship.
async fn befriend(pool: &PgPool, a: i64, b: i64) {
    for (x, y) in [(a, b), (b, a)] {
        sqlx::query("INSERT INTO relationships (user_id, target_id, type) VALUES ($1, $2, 1)")
            .bind(x)
            .bind(y)
            .execute(pool)
            .await
            .unwrap();
    }
}

/// The blocker's row becomes a `blocked` row (a block replaces their side of a friendship).
async fn block(pool: &PgPool, blocker: i64, target: i64) {
    sqlx::query(
        "INSERT INTO relationships (user_id, target_id, type) VALUES ($1, $2, 2)          ON CONFLICT (user_id, target_id) DO UPDATE SET type = 2",
    )
    .bind(blocker)
    .bind(target)
    .execute(pool)
    .await
    .unwrap();
}

async fn mount(pool: &PgPool, id: i64, owner: Option<i64>) {
    let (kind, owner_id) = match owner {
        Some(o) => ("user", o),
        None => ("platform", 0),
    };
    sqlx::query(
        "INSERT INTO storage_mounts (id, owner_kind, owner_id, label, provider, endpoint, bucket, access_key_id, secret_key_enc) \
         VALUES ($1, $2, $3, $4, 's3', 'https://s3.example.test', 'b', 'k', '\\x00') ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .bind(kind)
    .bind(owner_id)
    .bind(format!("m{id}"))
    .execute(pool)
    .await
    .unwrap();
}

#[derive(Default)]
struct Bucket {
    deleted: Mutex<Vec<(i64, String)>>,
    copied: Mutex<Vec<(i64, String, String)>>,
    fail_deletes: Mutex<bool>,
}

#[async_trait]
impl RingObjects for Bucket {
    async fn delete(&self, mount_id: i64, key: &str) -> Result<(), ObjectFailure> {
        if *self.fail_deletes.lock().unwrap() {
            return Err(ObjectFailure::Retry("bucket is down".into()));
        }
        self.deleted
            .lock()
            .unwrap()
            .push((mount_id, key.to_owned()));
        Ok(())
    }
    async fn copy_to_instance(
        &self,
        mount_id: i64,
        key: &str,
        to_key: &str,
    ) -> Result<(), ObjectFailure> {
        self.copied
            .lock()
            .unwrap()
            .push((mount_id, key.to_owned(), to_key.to_owned()));
        Ok(())
    }
}

struct Env {
    db: TestDb,
    uf: UserFeatures,
}

impl Env {
    async fn new() -> Self {
        let db = TestDb::new().await.expect("test db");
        let mut registry = FeatureRegistry::new();
        register_all(&mut registry).unwrap();
        let features = FeatureService::new(
            Arc::new(registry),
            Arc::new(InMemorySettingsStore::new()),
            None,
        );
        // Rings start off (the operator turns them on); these tests are about rings, so turn them on.
        features
            .set_enabled(RINGS_ID, &ScopeRef::Platform, Some(true))
            .await
            .unwrap();
        let ids: sr_userfeat::IdSource = Arc::new(uid);
        let uf = UserFeatures::new(db.pool().clone(), features, ids);
        mount(db.pool(), 1, None).await;
        Self { db, uf }
    }

    fn pool(&self) -> &PgPool {
        self.db.pool()
    }

    async fn friends_text(&self, author: i64, text: &str) -> i64 {
        let r = self
            .uf
            .create_ring(
                author,
                NewRing {
                    kind: RingKind::Text,
                    body: text.into(),
                    track: None,
                    audience: RingAudienceKind::Friends,
                    guild_ids: vec![],
                    object: None,
                },
            )
            .await
            .unwrap();
        r.id.parse().unwrap()
    }

    async fn guilds_text(&self, author: i64, guilds: &[i64]) -> i64 {
        let r = self
            .uf
            .create_ring(
                author,
                NewRing {
                    kind: RingKind::Text,
                    body: "for the servers".into(),
                    track: None,
                    audience: RingAudienceKind::Guilds,
                    guild_ids: guilds.to_vec(),
                    object: None,
                },
            )
            .await
            .unwrap();
        r.id.parse().unwrap()
    }

    async fn photo(&self, author: i64, mount_id: i64) -> (i64, String) {
        let key = format!("rings/{}", uid());
        let r = self
            .uf
            .create_ring(
                author,
                NewRing {
                    kind: RingKind::Photo,
                    body: String::new(),
                    track: None,
                    audience: RingAudienceKind::Friends,
                    guild_ids: vec![],
                    object: Some(RingObject {
                        mount_id,
                        object_key: key.clone(),
                        content_type: "image/jpeg".into(),
                        bytes: 1234,
                        duration_ms: None,
                    }),
                },
            )
            .await
            .unwrap();
        (r.id.parse().unwrap(), key)
    }

    /// Age a ring so that it is past its day.
    async fn age(&self, ring: i64) {
        sqlx::query(
            "UPDATE status_rings SET created_at = now() - interval '25 hours', expires_at = now() - interval '1 hour' WHERE id = $1",
        )
        .bind(ring)
        .execute(self.pool())
        .await
        .unwrap();
    }

    async fn hold(&self, ring: i64) -> i64 {
        let h = uid();
        sqlx::query("INSERT INTO moderation_holds (id, reason, scope_kind, scope_id) VALUES ($1, 'reported', 'status_ring', $2)")
            .bind(h)
            .bind(ring)
            .execute(self.pool())
            .await
            .unwrap();
        sqlx::query("UPDATE status_rings SET hold_id = $2 WHERE id = $1")
            .bind(ring)
            .bind(h)
            .execute(self.pool())
            .await
            .unwrap();
        h
    }

    async fn rows(&self, ring: i64) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM status_rings WHERE id = $1")
            .bind(ring)
            .fetch_one(self.pool())
            .await
            .unwrap()
    }
}

fn unknown() -> UfError {
    UfError::NotFound("ring")
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_friends_ring_is_for_mutual_friends_only() {
    let e = Env::new().await;
    let p = e.pool();
    let (author, friend, stranger, one_way, member_only) = (
        account(p).await,
        account(p).await,
        account(p).await,
        account(p).await,
        account(p).await,
    );
    befriend(p, author, friend).await;
    // A request the author sent and the other has not accepted: one row only.
    sqlx::query("INSERT INTO relationships (user_id, target_id, type) VALUES ($1, $2, 1)")
        .bind(author)
        .bind(one_way)
        .execute(p)
        .await
        .unwrap();
    // Somebody who shares a server with the author but is not their friend.
    guild(p, author, &[member_only]).await;

    let ring = e.friends_text(author, "good morning").await;
    assert_eq!(
        e.uf.ring_for(author, ring).await.unwrap().body,
        "good morning"
    );
    assert_eq!(
        e.uf.ring_for(friend, ring).await.unwrap().audience,
        "friends"
    );
    for outsider in [stranger, one_way, member_only] {
        assert_eq!(e.uf.ring_for(outsider, ring).await, Err(unknown()));
        assert_eq!(e.uf.ring_media(outsider, ring).await, Err(unknown()));
        assert!(!ring_visible_to(p, ring, outsider).await.unwrap());
        assert!(e.uf.ring_feed(outsider).await.unwrap().is_empty());
        assert!(!viewer_may_see_a_ring_of(p, outsider, author).await.unwrap());
    }
    // The 404 of a hidden ring is the answer for a ring that is not there.
    assert_eq!(
        e.uf.ring_for(stranger, ring).await,
        e.uf.ring_for(stranger, 42).await
    );

    let feed = e.uf.ring_feed(friend).await.unwrap();
    assert_eq!(feed.len(), 1);
    assert_eq!(feed[0].author_id, author.to_string());
    assert!(viewer_may_see_a_ring_of(p, friend, author).await.unwrap());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_server_ring_is_for_the_members_of_the_chosen_servers_only() {
    let e = Env::new().await;
    let p = e.pool();
    let (author, chosen_member, other_member, friend, stranger, pending) = (
        account(p).await,
        account(p).await,
        account(p).await,
        account(p).await,
        account(p).await,
        account(p).await,
    );
    befriend(p, author, friend).await;
    let chosen = guild(p, author, &[chosen_member, pending]).await;
    guild(p, author, &[other_member]).await; // a server the author did not choose
    sqlx::query("UPDATE guild_members SET pending = TRUE WHERE guild_id = $1 AND user_id = $2")
        .bind(chosen)
        .bind(pending)
        .execute(p)
        .await
        .unwrap();

    let ring = e.guilds_text(author, &[chosen]).await;
    assert!(e.uf.ring_for(chosen_member, ring).await.is_ok());
    for outsider in [other_member, friend, stranger, pending] {
        assert_eq!(
            e.uf.ring_for(outsider, ring).await,
            Err(unknown()),
            "{outsider} must get the same answer as for a ring that does not exist"
        );
        assert!(!ring_visible_to(p, ring, outsider).await.unwrap());
    }

    // Leaving the chosen server takes the ring away from the author's side too: no common server, no audience.
    sqlx::query("DELETE FROM guild_members WHERE guild_id = $1 AND user_id = $2")
        .bind(chosen)
        .bind(author)
        .execute(p)
        .await
        .unwrap();
    assert_eq!(e.uf.ring_for(chosen_member, ring).await, Err(unknown()));
    // A viewer never learns which servers the author chose.
    let mine = e.uf.ring_for(author, ring).await.unwrap();
    assert_eq!(mine.guild_ids, vec![chosen.to_string()]);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn only_servers_the_author_belongs_to_can_be_chosen_and_viewers_do_not_see_the_list() {
    let e = Env::new().await;
    let p = e.pool();
    let (author, other, member) = (account(p).await, account(p).await, account(p).await);
    let theirs = guild(p, other, &[]).await;
    let mine = guild(p, author, &[member]).await;
    let err =
        e.uf.create_ring(
            author,
            NewRing {
                kind: RingKind::Text,
                body: "hi".into(),
                track: None,
                audience: RingAudienceKind::Guilds,
                guild_ids: vec![theirs],
                object: None,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(err, UfError::NotFound("server"));
    let ring = e.guilds_text(author, &[mine]).await;
    let seen = e.uf.ring_for(member, ring).await.unwrap();
    assert!(
        seen.guild_ids.is_empty(),
        "the list of chosen servers is the author's"
    );
    assert!(!seen.mine);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_blocked_person_sees_nothing_in_either_direction() {
    let e = Env::new().await;
    let p = e.pool();
    let (author, a, b) = (account(p).await, account(p).await, account(p).await);
    befriend(p, author, a).await;
    befriend(p, author, b).await;
    let g = guild(p, author, &[a, b]).await;
    let friends_ring = e.friends_text(author, "x").await;
    let server_ring = e.guilds_text(author, &[g]).await;
    assert!(e.uf.ring_for(a, friends_ring).await.is_ok());
    assert!(e.uf.ring_for(b, server_ring).await.is_ok());

    block(p, author, a).await; // the author blocks a
    block(p, b, author).await; // b blocks the author
    for (viewer, ring) in [
        (a, friends_ring),
        (a, server_ring),
        (b, friends_ring),
        (b, server_ring),
    ] {
        assert_eq!(e.uf.ring_for(viewer, ring).await, Err(unknown()));
        assert!(e.uf.ring_feed(viewer).await.unwrap().is_empty());
        assert_eq!(e.uf.record_ring_view(viewer, ring).await, Err(unknown()));
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_viewer_list_belongs_to_the_author_alone() {
    let e = Env::new().await;
    let p = e.pool();
    let (author, f1, f2, stranger) = (
        account(p).await,
        account(p).await,
        account(p).await,
        account(p).await,
    );
    befriend(p, author, f1).await;
    befriend(p, author, f2).await;
    let ring = e.friends_text(author, "seen by?").await;

    e.uf.record_ring_view(f1, ring).await.unwrap();
    e.uf.record_ring_view(f1, ring).await.unwrap(); // opening twice lists once
    e.uf.record_ring_view(author, ring).await.unwrap(); // the author's own opens are not recorded
    assert_eq!(e.uf.record_ring_view(stranger, ring).await, Err(unknown()));

    let list = e.uf.ring_viewers(author, ring).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].user_id, f1.to_string());

    // Nobody else can read it: a viewer, another friend, a stranger, and an id that is not there all get one answer.
    for other in [f1, f2, stranger] {
        assert_eq!(e.uf.ring_viewers(other, ring).await, Err(unknown()));
    }
    assert_eq!(e.uf.ring_viewers(author, 7).await, Err(unknown()));
    // No other reader carries it: the ring a friend gets has no viewer data and `seen` is only theirs.
    let as_f2 = e.uf.ring_for(f2, ring).await.unwrap();
    assert!(!as_f2.seen);
    let as_f1 = e.uf.ring_for(f1, ring).await.unwrap();
    assert!(as_f1.seen);
    let json = serde_json::to_string(&as_f2).unwrap();
    assert!(!json.contains("viewer"), "{json}");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn expiry_is_hard_rows_and_objects_on_the_instance_bucket_and_on_the_authors_mount() {
    let e = Env::new().await;
    let p = e.pool();
    let (author, friend) = (account(p).await, account(p).await);
    befriend(p, author, friend).await;
    let own_mount = uid();
    mount(p, own_mount, Some(author)).await;
    let (instance_ring, instance_key) = e.photo(author, 1).await;
    let (mount_ring, mount_key) = e.photo(author, own_mount).await;
    let fresh = e.friends_text(author, "still today").await;
    e.uf.record_ring_view(friend, instance_ring).await.unwrap();

    // Past its day nobody reads it, even before the job runs.
    e.age(instance_ring).await;
    e.age(mount_ring).await;
    assert_eq!(e.uf.ring_for(friend, instance_ring).await, Err(unknown()));
    assert_eq!(e.uf.ring_media(author, mount_ring).await, Err(unknown()));
    assert_eq!(e.uf.my_rings(author).await.unwrap().len(), 1);

    let r = e.uf.expire_rings(100).await.unwrap();
    assert_eq!(
        (r.rows_deleted, r.objects_queued, r.held_skipped),
        (2, 2, 0)
    );
    assert_eq!(e.rows(instance_ring).await + e.rows(mount_ring).await, 0);
    assert_eq!(e.rows(fresh).await, 1, "a ring still in its day stays");
    let views: i64 =
        sqlx::query_scalar("SELECT count(*) FROM status_ring_views WHERE ring_id = $1")
            .bind(instance_ring)
            .fetch_one(p)
            .await
            .unwrap();
    assert_eq!(views, 0, "the viewer list goes with the ring");

    let bucket = Bucket::default();
    // A bucket outage keeps the entries for the next pass.
    *bucket.fail_deletes.lock().unwrap() = true;
    let down = e.uf.drain_ring_purge(&bucket, 100).await.unwrap();
    assert_eq!((down.deleted, down.retried), (0, 2));
    *bucket.fail_deletes.lock().unwrap() = false;
    let up = e.uf.drain_ring_purge(&bucket, 100).await.unwrap();
    assert_eq!(up.deleted, 2);
    let mut gone = bucket.deleted.lock().unwrap().clone();
    gone.sort();
    let mut want = vec![(1, instance_key), (own_mount, mount_key)];
    want.sort();
    assert_eq!(
        gone, want,
        "the instance object and the one on the author's own mount"
    );
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM status_ring_purge")
        .fetch_one(p)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_author_can_delete_a_ring_at_any_time() {
    let e = Env::new().await;
    let p = e.pool();
    let (author, friend) = (account(p).await, account(p).await);
    befriend(p, author, friend).await;
    let (ring, _key) = e.photo(author, 1).await;
    // Only the author deletes: a friend gets "unknown ring" and the ring stays.
    assert_eq!(e.uf.delete_ring(friend, ring).await, Err(unknown()));
    assert!(e.uf.ring_for(friend, ring).await.is_ok());
    let removed = e.uf.delete_ring(author, ring).await.unwrap();
    assert!(removed.object_queued && !removed.kept_under_hold);
    assert_eq!(e.uf.ring_for(friend, ring).await, Err(unknown()));
    assert_eq!(e.rows(ring).await, 0);
    assert_eq!(e.uf.delete_ring(author, ring).await, Err(unknown()));
    let bucket = Bucket::default();
    assert_eq!(e.uf.drain_ring_purge(&bucket, 10).await.unwrap().deleted, 1);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_ring_under_a_moderation_hold_keeps_its_object_but_the_reader_never_sees_it() {
    let e = Env::new().await;
    let p = e.pool();
    let (author, friend) = (account(p).await, account(p).await);
    befriend(p, author, friend).await;
    let own_mount = uid();
    mount(p, own_mount, Some(author)).await;
    let (ring, key) = e.photo(author, own_mount).await;
    let hold = e.hold(ring).await;

    // The evidence of a file on a member's own mount is copied to the instance bucket.
    let bucket = Bucket::default();
    assert_eq!(e.uf.copy_held_ring_evidence(&bucket, 10).await.unwrap(), 1);
    assert_eq!(
        bucket.copied.lock().unwrap().clone(),
        vec![(own_mount, key.clone(), format!("rings/evidence/{ring}"))]
    );
    assert_eq!(
        e.uf.copy_held_ring_evidence(&bucket, 10).await.unwrap(),
        0,
        "copied once"
    );
    let media = e.uf.held_ring_media(ring).await.unwrap().unwrap();
    assert_eq!(
        (media.mount_id, media.object_key.as_str()),
        (1, &*format!("rings/evidence/{ring}"))
    );

    // Past its day: nobody reads it, the job leaves row and object alone.
    e.age(ring).await;
    assert_eq!(e.uf.ring_for(friend, ring).await, Err(unknown()));
    assert_eq!(e.uf.ring_media(friend, ring).await, Err(unknown()));
    assert_eq!(e.uf.ring_media(author, ring).await, Err(unknown()));
    let r = e.uf.expire_rings(100).await.unwrap();
    assert_eq!((r.rows_deleted, r.held_skipped), (0, 1));
    assert_eq!(e.rows(ring).await, 1);
    assert_eq!(e.uf.drain_ring_purge(&bucket, 10).await.unwrap().deleted, 0);
    assert!(bucket.deleted.lock().unwrap().is_empty());
    // The database refuses to delete a held row, whoever asks.
    let refused = sqlx::query("DELETE FROM status_rings WHERE id = $1")
        .bind(ring)
        .execute(p)
        .await;
    assert!(refused.is_err());

    // The author's own delete hides it and keeps it.
    let fresh_held = {
        let (r2, _) = e.photo(author, 1).await;
        e.hold(r2).await;
        r2
    };
    let removed = e.uf.delete_ring(author, fresh_held).await.unwrap();
    assert!(removed.kept_under_hold && !removed.object_queued);
    assert_eq!(e.uf.ring_for(friend, fresh_held).await, Err(unknown()));
    assert_eq!(e.rows(fresh_held).await, 1);

    // Staff still read the held copy (the logged read), and the hold's scope is the ring.
    assert!(e.uf.held_ring_media(fresh_held).await.unwrap().is_some());
    let scope: String = sqlx::query_scalar("SELECT scope_kind FROM moderation_holds WHERE id = $1")
        .bind(hold)
        .fetch_one(p)
        .await
        .unwrap();
    assert_eq!(scope, "status_ring");

    // When the hold ends the next pass deletes the row, the object and the evidence copy.
    sqlx::query("UPDATE status_rings SET hold_id = NULL WHERE id = $1")
        .bind(ring)
        .execute(p)
        .await
        .unwrap();
    let r = e.uf.expire_rings(100).await.unwrap();
    assert_eq!(r.rows_deleted, 1);
    assert_eq!(e.rows(ring).await, 0);
    assert_eq!(e.uf.drain_ring_purge(&bucket, 10).await.unwrap().deleted, 2);
    let mut gone = bucket.deleted.lock().unwrap().clone();
    gone.sort();
    let mut want = vec![(own_mount, key), (1, format!("rings/evidence/{ring}"))];
    want.sort();
    assert_eq!(gone, want);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn limits_and_validation() {
    let e = Env::new().await;
    let p = e.pool();
    let author = account(p).await;
    e.uf.features
        .set_settings(
            RINGS_ID,
            &ScopeRef::Platform,
            serde_json::json!({"max_per_day": 3})
                .as_object()
                .cloned()
                .unwrap(),
        )
        .await
        .unwrap();
    let first = e.friends_text(author, "one").await;
    e.friends_text(author, "two").await;
    // Deleting a ring does not give the allowance back.
    e.uf.delete_ring(author, first).await.unwrap();
    e.friends_text(author, "three").await;
    let err =
        e.uf.create_ring(
            author,
            NewRing {
                kind: RingKind::Text,
                body: "four".into(),
                track: None,
                audience: RingAudienceKind::Friends,
                guild_ids: vec![],
                object: None,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, UfError::Conflict(_)), "{err:?}");

    let mk = |kind, body: &str, track, object| NewRing {
        kind,
        body: body.into(),
        track,
        audience: RingAudienceKind::Friends,
        guild_ids: vec![],
        object,
    };
    e.uf.features
        .set_settings(
            RINGS_ID,
            &ScopeRef::Platform,
            serde_json::json!({"max_per_day": 50})
                .as_object()
                .cloned()
                .unwrap(),
        )
        .await
        .unwrap();
    let clip = |ms: Option<i64>, bytes: i64| RingObject {
        mount_id: 1,
        object_key: "rings/x".into(),
        content_type: "video/mp4".into(),
        bytes,
        duration_ms: ms,
    };
    for bad in [
        mk(RingKind::Text, "   ", None, None),
        mk(RingKind::Text, &"x".repeat(281), None, None),
        mk(RingKind::Track, "", None, None),
        mk(
            RingKind::Track,
            "",
            Some(RingTrack {
                title: " ".into(),
                artist: "a".into(),
            }),
            None,
        ),
        mk(RingKind::Photo, "", None, None),
        mk(RingKind::Text, "hi", None, Some(clip(None, 10))),
        mk(RingKind::Clip, "", None, Some(clip(Some(60_001), 10))),
        mk(RingKind::Clip, "", None, Some(clip(None, 10))),
        mk(
            RingKind::Clip,
            "",
            None,
            Some(clip(Some(1000), 50 * 1024 * 1024 + 1)),
        ),
    ] {
        assert!(matches!(
            e.uf.create_ring(author, bad).await,
            Err(UfError::Invalid(_))
        ));
    }
    // A clip of exactly 60 seconds and 50 MiB is fine; so is a track.
    e.uf.create_ring(
        author,
        mk(
            RingKind::Clip,
            "",
            None,
            Some(clip(Some(60_000), 50 * 1024 * 1024)),
        ),
    )
    .await
    .unwrap();
    let track =
        e.uf.create_ring(
            author,
            mk(
                RingKind::Track,
                "",
                Some(RingTrack {
                    title: "Song".into(),
                    artist: "Band".into(),
                }),
                None,
            ),
        )
        .await
        .unwrap();
    assert_eq!(track.track.unwrap().artist, "Band");
    // Friends rings name no servers; server rings name at least one.
    let mut with_guild = mk(RingKind::Text, "hi", None, None);
    with_guild.guild_ids = vec![1];
    assert!(matches!(
        e.uf.create_ring(author, with_guild).await,
        Err(UfError::Invalid(_))
    ));
    let mut no_guild = mk(RingKind::Text, "hi", None, None);
    no_guild.audience = RingAudienceKind::Guilds;
    assert!(matches!(
        e.uf.create_ring(author, no_guild).await,
        Err(UfError::Invalid(_))
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_profile_cap_reads_the_audience_through_one_function() {
    let e = Env::new().await;
    let p = e.pool();
    let (author, friend) = (account(p).await, account(p).await);
    befriend(p, author, friend).await;
    assert!(ring_audience(p, author).await.unwrap().is_none(), "no ring");
    e.friends_text(author, "hello").await;
    let a = ring_audience(p, author).await.unwrap().unwrap();
    assert_eq!((a.kind, a.guild_ids.len()), (RingAudienceKind::Friends, 0));
    let g = guild(p, author, &[]).await;
    e.guilds_text(author, &[g]).await;
    let a = ring_audience(p, author).await.unwrap().unwrap();
    assert_eq!((a.kind, a.guild_ids), (RingAudienceKind::Guilds, vec![g]));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn export_lists_active_rings_and_erasure_deletes_them_all_except_held_ones() {
    let e = Env::new().await;
    let p = e.pool();
    let (author, friend) = (account(p).await, account(p).await);
    befriend(p, author, friend).await;
    let (photo, key) = e.photo(author, 1).await;
    let text = e.friends_text(author, "export me").await;
    let (held, _) = e.photo(author, 1).await;
    e.hold(held).await;
    let expired = e.friends_text(author, "too old").await;
    e.age(expired).await;
    e.uf.record_ring_view(friend, text).await.unwrap();
    // The friend opens a ring too: the erasure of the friend forgets that they did.
    let friends_own = e.friends_text(friend, "mine").await;
    e.uf.record_ring_view(author, friends_own).await.unwrap();

    let export = sr_db::rings_dsr::export_for(p, author).await.unwrap();
    let active = export["active_rings"].as_array().unwrap();
    assert_eq!(active.len(), 3, "the expired one is not listed: {export}");
    let ids: Vec<&str> = active.iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert!(
        ids.contains(&text.to_string().as_str()) && !ids.contains(&expired.to_string().as_str())
    );
    let listed = active
        .iter()
        .find(|r| r["id"].as_str() == Some(text.to_string().as_str()))
        .unwrap();
    assert_eq!(
        listed["viewers"].as_array().unwrap().len(),
        1,
        "the viewer list is the author's own data"
    );
    assert_eq!(export["rings_you_opened"].as_array().unwrap().len(), 1);

    let mut conn = p.acquire().await.unwrap();
    assert_eq!(
        sr_db::rings_dsr::held_rows(&mut conn, author)
            .await
            .unwrap(),
        1
    );
    drop(conn);
    let mut tx = p.begin().await.unwrap();
    let erased = sr_db::rings_dsr::erase_for(&mut tx, author).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        (erased.rings, erased.held),
        (3, 1),
        "the expired row and two live ones go; the held one is hidden"
    );
    assert_eq!(
        e.rows(photo).await + e.rows(text).await + e.rows(expired).await,
        0
    );
    assert_eq!(e.rows(held).await, 1);
    assert_eq!(e.uf.ring_for(friend, held).await, Err(unknown()));
    let none = sr_db::rings_dsr::export_for(p, author).await.unwrap();
    assert!(none["active_rings"].as_array().unwrap().is_empty());
    assert!(none["rings_you_opened"].as_array().unwrap().is_empty());
    let bucket = Bucket::default();
    e.uf.drain_ring_purge(&bucket, 10).await.unwrap();
    assert_eq!(bucket.deleted.lock().unwrap().clone(), vec![(1, key)]);
    let opened: i64 =
        sqlx::query_scalar("SELECT count(*) FROM status_ring_views WHERE viewer_id = $1")
            .bind(author)
            .fetch_one(p)
            .await
            .unwrap();
    assert_eq!(opened, 0);
}
