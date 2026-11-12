//! Postgres-backed tests of the Activities service: settings and the review that follows every change, the
//! shelf, launching (permissions, one per channel), joining, leaving and ending. Channel permissions are a
//! fake. Run with a scratch Postgres that may CREATE DATABASE:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-activities --test db -- --ignored --test-threads=1`

use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicI64, Ordering},
};

use async_trait::async_trait;
use serde_json::Value;
use std::io::{Cursor, Write};

use sr_activities::model::{BundleState, ConfigInput, ReviewState, UrlMapping};
use sr_activities::{ActivityError, ActivityEvents, ActivityService, ChannelAccess, ChannelPerms};
use sr_activities::{Lookup, Serving, VoiceFollower};
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

/// Channel 1 is a voice channel everybody may use; 2 is a voice channel the `limited` user cannot
/// connect to; 3 is a text channel; user `manager` manages everything.
struct FakeChannels {
    perms: Mutex<HashMap<(i64, i64), ChannelPerms>>,
    default: Mutex<HashMap<i64, ChannelPerms>>,
}

impl FakeChannels {
    fn new() -> Self {
        let voice = ChannelPerms {
            guild_id: 9,
            voice: true,
            text: false,
            view: true,
            connect: true,
            use_activities: true,
            manage: false,
        };
        let mut d = HashMap::new();
        d.insert(1, voice);
        d.insert(5, voice);
        d.insert(
            3,
            ChannelPerms {
                voice: false,
                text: true,
                ..voice
            },
        );
        Self {
            perms: Mutex::new(HashMap::new()),
            default: Mutex::new(d),
        }
    }
    fn set(&self, user: i64, channel: i64, p: ChannelPerms) {
        self.perms.lock().unwrap().insert((user, channel), p);
    }
    fn base(&self, channel: i64) -> Option<ChannelPerms> {
        self.default.lock().unwrap().get(&channel).copied()
    }
}

#[async_trait]
impl ChannelAccess for FakeChannels {
    async fn perms(&self, user: i64, channel: i64) -> Result<Option<ChannelPerms>, String> {
        Ok(self
            .perms
            .lock()
            .unwrap()
            .get(&(user, channel))
            .copied()
            .or_else(|| self.base(channel)))
    }
    async fn viewers(&self, _channel: i64) -> Result<Vec<i64>, String> {
        Ok(vec![1, 2])
    }
}

#[derive(Default)]
struct Recorder(Mutex<Vec<(String, Value)>>);

#[async_trait]
impl ActivityEvents for Recorder {
    async fn emit(&self, _users: Vec<i64>, name: &'static str, payload: Value) {
        self.0.lock().unwrap().push((name.to_owned(), payload));
    }
}

struct Rig {
    db: TestDb,
    svc: ActivityService,
    chans: Arc<FakeChannels>,
    events: Arc<Recorder>,
    storage: Arc<dyn ObjectStorage>,
}

async fn rig() -> Rig {
    let db = TestDb::new().await.unwrap();
    let chans = Arc::new(FakeChannels::new());
    let events = Arc::new(Recorder::default());
    let storage: Arc<dyn ObjectStorage> = Arc::new(ObjectStoreStorage::in_memory());
    let svc = ActivityService::new(db.pool().clone(), Arc::new(uid))
        .with_storage(storage.clone())
        .with_channels(chans.clone())
        .with_events(events.clone());
    Rig {
        db,
        svc,
        chans,
        events,
        storage,
    }
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

fn site(version: &str) -> Vec<u8> {
    zip_of(&[
        ("index.html", format!("<h1>{version}</h1>").as_bytes()),
        ("app.js", b"1"),
        ("img/logo.png", b"png"),
    ])
}

fn input(dev: Option<&str>) -> ConfigInput {
    ConfigInput {
        dev_entry_url: dev.map(str::to_owned),
        url_mappings: vec![UrlMapping {
            prefix: "/api".into(),
            target: "https://api.example.com".into(),
        }],
    }
}

async fn set_state(r: &Rig, app: i64, state: &str) {
    sqlx::query("UPDATE applications SET release_state = $2::app_release_state WHERE id = $1")
        .bind(app)
        .bind(state)
        .execute(r.db.pool())
        .await
        .unwrap();
}

/// A released application with a reviewed bundle.
async fn approved_app(r: &Rig, name: &str) -> i64 {
    let owner = account(r.db.pool()).await;
    let app = application(r.db.pool(), owner, name).await;
    r.svc.save_config(app, input(None)).await.unwrap();
    let up = r.svc.upload_bundle(app, owner, site("one")).await.unwrap();
    let id: i64 = up.bundle.id.parse().unwrap();
    r.svc.submit(app, id).await.unwrap();
    r.svc.decide(app, 1, id, true, None).await.unwrap();
    set_state(r, app, "released").await;
    app
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_bundle_goes_draft_pending_approved_and_only_the_approved_one_is_served() {
    let r = rig().await;
    let owner = account(r.db.pool()).await;
    let app = application(r.db.pool(), owner, "Chess").await;

    let c = r.svc.config(app).await.unwrap();
    assert_eq!(c.review_state, ReviewState::Draft);
    assert!(c.bundles.is_empty() && c.reviewed_bundle.is_none());
    assert!(!r.svc.has_activity(app).await.unwrap());
    assert_eq!(r.svc.lookup(app).await.unwrap(), Lookup::Unknown);
    r.svc.save_config(app, input(None)).await.unwrap();

    // A refused zip names the entry and stores nothing.
    let e = r
        .svc
        .upload_bundle(
            app,
            owner,
            zip_of(&[("index.html", b"x"), ("../evil.js", b"x")]),
        )
        .await
        .unwrap_err();
    match e {
        ActivityError::Invalid("bundle", m) => assert!(m.contains("evil.js"), "{m}"),
        other => panic!("{other:?}"),
    }
    assert!(r.storage.list("activities/").await.unwrap().is_empty());
    assert!(r.svc.config(app).await.unwrap().bundles.is_empty());

    // Upload: a draft with its listing; objects are under the hash.
    let up = r.svc.upload_bundle(app, owner, site("one")).await.unwrap();
    let b1: i64 = up.bundle.id.parse().unwrap();
    assert_eq!(up.bundle.state, BundleState::Draft);
    assert_eq!(up.bundle.entry_count, 3);
    assert_eq!(up.entries.len(), 3);
    assert!(
        up.entries
            .iter()
            .any(|e| e.path == "img/logo.png" && e.content_type == "image/png")
    );
    assert_eq!(
        r.svc
            .bundle_file(app, &up.bundle.sha256, "index.html")
            .await
            .unwrap()
            .unwrap(),
        "<h1>one</h1>".as_bytes()
    );
    assert!(
        r.svc
            .bundle_file(app, &up.bundle.sha256, "nope.js")
            .await
            .unwrap()
            .is_none()
    );
    // The same zip again is the same bundle.
    let again = r.svc.upload_bundle(app, owner, site("one")).await.unwrap();
    assert_eq!(again.bundle.id, up.bundle.id);
    // A draft is not served; submitting a foreign bundle id fails.
    set_state(&r, app, "released").await;
    assert_eq!(r.svc.lookup(app).await.unwrap(), Lookup::Paused);
    assert!(matches!(
        r.svc.submit(app, 4040).await.unwrap_err(),
        ActivityError::NotFound("bundle")
    ));

    // Pending, in the staff queue, not yet served.
    let c = r.svc.submit(app, b1).await.unwrap();
    assert_eq!(c.review_state, ReviewState::Pending);
    assert!(matches!(
        r.svc.submit(app, b1).await.unwrap_err(),
        ActivityError::Conflict(_)
    ));
    assert_eq!(r.svc.lookup(app).await.unwrap(), Lookup::Paused);
    let q = r.svc.pending(10).await.unwrap();
    assert_eq!(q.len(), 1);
    assert_eq!(q[0].application_name, "Chess");
    assert_eq!(q[0].owner_id, owner.to_string());
    assert_eq!(q[0].bundle.bundle.sha256, up.bundle.sha256);
    assert_eq!(q[0].bundle.entries.len(), 3);
    let (sha, zip) = r.svc.bundle_zip(app, b1).await.unwrap();
    assert_eq!(sha, up.bundle.sha256);
    assert_eq!(zip.as_ref(), site("one").as_slice());

    // Nobody reviews their own Activity.
    assert!(matches!(
        r.svc.decide(app, owner, b1, true, None).await.unwrap_err(),
        ActivityError::Forbidden
    ));
    // Deciding names the bundle: a stale id is a conflict.
    assert!(matches!(
        r.svc.decide(app, 1, b1 + 1, true, None).await.unwrap_err(),
        ActivityError::Conflict(_)
    ));
    // Rejected with a note, then a new upload supersedes the rejected one.
    let c = r
        .svc
        .decide(app, 1, b1, false, Some("  too broad "))
        .await
        .unwrap();
    assert_eq!(c.review_state, ReviewState::Rejected);
    assert_eq!(c.review_note.as_deref(), Some("too broad"));
    assert!(c.reviewed_bundle.is_none());
    assert!(matches!(
        r.svc.decide(app, 1, b1, true, None).await.unwrap_err(),
        ActivityError::Conflict(_)
    ));
    let up2 = r.svc.upload_bundle(app, owner, site("two")).await.unwrap();
    let b2: i64 = up2.bundle.id.parse().unwrap();
    let c = r.svc.config(app).await.unwrap();
    assert_eq!(
        c.bundles
            .iter()
            .find(|b| b.id == up.bundle.id)
            .unwrap()
            .state,
        BundleState::Superseded
    );

    // Approved: the proxy now serves exactly this bundle, with the mappings.
    r.svc.submit(app, b2).await.unwrap();
    let c = r.svc.decide(app, 1, b2, true, None).await.unwrap();
    assert_eq!(c.review_state, ReviewState::Approved);
    assert_eq!(c.reviewed_bundle.as_ref().unwrap().id, up2.bundle.id);
    match r.svc.lookup(app).await.unwrap() {
        Lookup::Ready(s) => {
            assert_eq!(
                s.serving,
                Serving::Bundle {
                    sha256: up2.bundle.sha256.clone()
                }
            );
            assert_eq!(s.url_mappings.len(), 1);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(r.svc.shelf(owner).await.unwrap().len(), 1);

    // A new upload and submission leave the approved bundle serving until staff decide.
    let up3 = r
        .svc
        .upload_bundle(app, owner, site("three"))
        .await
        .unwrap();
    let b3: i64 = up3.bundle.id.parse().unwrap();
    let c = r.svc.submit(app, b3).await.unwrap();
    assert_eq!(c.review_state, ReviewState::Pending);
    assert_eq!(c.reviewed_bundle.as_ref().unwrap().id, up2.bundle.id);
    match r.svc.lookup(app).await.unwrap() {
        Lookup::Ready(s) => assert_eq!(
            s.serving,
            Serving::Bundle {
                sha256: up2.bundle.sha256.clone()
            }
        ),
        other => panic!("{other:?}"),
    }
    // A rejection of the new one leaves the old one.
    let c = r.svc.decide(app, 1, b3, false, None).await.unwrap();
    assert_eq!(c.review_state, ReviewState::Approved);
    assert_eq!(c.reviewed_bundle.as_ref().unwrap().id, up2.bundle.id);

    // Saving identical values keeps the approval; a change to the mappings withdraws it.
    let same = r.svc.save_config(app, input(None)).await.unwrap();
    assert_eq!(same.review_state, ReviewState::Approved);
    let mut changed = input(None);
    changed.url_mappings[0].target = "https://api2.example.com".into();
    let changed = r.svc.save_config(app, changed).await.unwrap();
    assert!(changed.reviewed_bundle.is_none());
    assert_ne!(changed.review_state, ReviewState::Approved);
    assert_eq!(r.svc.lookup(app).await.unwrap(), Lookup::Paused);
    assert!(r.svc.shelf(owner).await.unwrap().is_empty());

    // The collector deletes the objects of superseded bundles only: the first (superseded by the second
    // upload) and the second (withdrawn); the third was rejected and is still the latest, so it stays.
    // They have to be older than the grace period first (SEC-AUD-139).
    age_superseded(&r, 2).await;
    let removed = r.svc.gc().await.unwrap();
    assert!(removed > 0);
    let left = r.storage.list("activities/").await.unwrap();
    assert!(left.iter().all(|o| !o.key.contains(&up2.bundle.sha256)));
    assert!(left.iter().all(|o| !o.key.contains(&up.bundle.sha256)));
    assert!(left.iter().any(|o| o.key.contains(&up3.bundle.sha256)));
    assert_eq!(r.svc.gc().await.unwrap(), 0);
    r.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn an_unreleased_or_suspended_application_serves_no_bundle() {
    let r = rig().await;
    let app = approved_app(&r, "Chess").await;
    assert!(matches!(r.svc.lookup(app).await.unwrap(), Lookup::Ready(_)));
    for state in ["development", "in_review"] {
        set_state(&r, app, state).await;
        assert_eq!(r.svc.lookup(app).await.unwrap(), Lookup::Paused, "{state}");
    }
    set_state(&r, app, "released").await;
    assert_eq!(r.svc.shelf(1).await.unwrap().len(), 1);
    set_state(&r, app, "suspended").await;
    assert_eq!(r.svc.lookup(app).await.unwrap(), Lookup::Unknown);
    r.svc.save_config(app, input(None)).await.unwrap();
    assert!(r.svc.shelf(1).await.unwrap().is_empty());
    r.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn in_review_with_a_prior_release_keeps_serving_and_never_released_does_not() {
    let r = rig().await;
    let app = approved_app(&r, "Chess").await;
    let owner: i64 = sqlx::query_scalar("SELECT owner_id FROM applications WHERE id = $1")
        .bind(app)
        .fetch_one(r.db.pool())
        .await
        .unwrap();
    // Never released (no approved version row): in review serves nothing and cannot start.
    set_state(&r, app, "in_review").await;
    assert_eq!(r.svc.lookup(app).await.unwrap(), Lookup::Paused);
    assert!(r.svc.shelf(1).await.unwrap().is_empty());
    // A prior approved version: the reviewed bundle keeps serving while a new version is in review.
    sqlx::query(
        "INSERT INTO application_versions (id, application_id, version, manifest, state, submitted_by) \
         VALUES ($1, $2, 1, '{}'::jsonb, 'approved', $3)",
    )
    .bind(uid())
    .bind(app)
    .bind(owner)
    .execute(r.db.pool())
    .await
    .unwrap();
    assert!(matches!(r.svc.lookup(app).await.unwrap(), Lookup::Ready(_)));
    assert_eq!(r.svc.shelf(1).await.unwrap().len(), 1);
    // Suspension stops it at once.
    set_state(&r, app, "suspended").await;
    assert_eq!(r.svc.lookup(app).await.unwrap(), Lookup::Unknown);
    assert!(r.svc.shelf(1).await.unwrap().is_empty());
    r.db.cleanup().await.unwrap();
}
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_entry_url_survives_only_as_a_development_url() {
    let r = rig().await;
    let owner = account(r.db.pool()).await;
    let tester = account(r.db.pool()).await;
    let stranger = account(r.db.pool()).await;
    let app = application(r.db.pool(), owner, "Wip").await;

    // Bad URLs are named; the URL needs a development application.
    let e = r
        .svc
        .save_config(app, input(Some("http://insecure.example.com/")))
        .await
        .unwrap_err();
    assert!(matches!(e, ActivityError::Invalid("dev_entry_url", _)));
    let mut bad = input(None);
    bad.url_mappings[0].prefix = "/".into();
    assert!(matches!(
        r.svc.save_config(app, bad).await.unwrap_err(),
        ActivityError::Invalid("url_mappings", _)
    ));
    let c = r
        .svc
        .save_config(app, input(Some("https://dev.example.com/")))
        .await
        .unwrap();
    assert_eq!(c.dev_entry_url.as_deref(), Some("https://dev.example.com/"));
    assert!(r.svc.has_activity(app).await.unwrap());
    match r.svc.lookup(app).await.unwrap() {
        Lookup::Ready(s) => assert_eq!(
            s.serving,
            Serving::Dev {
                url: "https://dev.example.com/".into()
            }
        ),
        other => panic!("{other:?}"),
    }

    // Only the owner and accepted testers see and start it.
    sqlx::query(
        "INSERT INTO app_testers (id, application_id, account_id, state, invited_by) \
         VALUES ($1, $2, $3, 'accepted', $4)",
    )
    .bind(uid())
    .bind(app)
    .bind(tester)
    .bind(owner)
    .execute(r.db.pool())
    .await
    .unwrap();
    assert!(r.svc.shelf(owner).await.unwrap()[0].development);
    assert_eq!(r.svc.shelf(tester).await.unwrap().len(), 1);
    assert!(r.svc.shelf(stranger).await.unwrap().is_empty());
    assert!(matches!(
        r.svc.launch(stranger, app, 1).await.unwrap_err(),
        ActivityError::NotFound("activity")
    ));
    let inst = r.svc.launch(tester, app, 1).await.unwrap();
    assert!(inst.development);

    // Released, the live URL is no longer served; a development URL cannot be set any more.
    set_state(&r, app, "released").await;
    assert_eq!(r.svc.lookup(app).await.unwrap(), Lookup::Paused);
    let e = r
        .svc
        .save_config(app, input(Some("https://other.example.com/")))
        .await
        .unwrap_err();
    assert!(matches!(e, ActivityError::Invalid("dev_entry_url", _)));
    r.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn instances_follow_the_channel_rules() {
    let r = rig().await;
    let app = approved_app(&r, "Chess").await;
    let draft_owner = account(r.db.pool()).await;
    let draft_app = application(r.db.pool(), draft_owner, "Draft").await;
    let a = account(r.db.pool()).await;
    let b = account(r.db.pool()).await;
    let c = account(r.db.pool()).await;

    // Only released Activities with a reviewed bundle, only in guild channels, only with the permissions.
    assert!(matches!(
        r.svc.launch(a, draft_app, 1).await.unwrap_err(),
        ActivityError::NotFound("activity")
    ));
    // A channel the member cannot see (a DM, a missing channel) is unknown.
    assert!(matches!(
        r.svc.launch(a, app, 404).await.unwrap_err(),
        ActivityError::NotFound("channel")
    ));
    // A guild channel that is neither voice nor text is refused.
    r.chans.set(
        a,
        8,
        ChannelPerms {
            voice: false,
            text: false,
            ..r.chans.base(1).unwrap()
        },
    );
    assert!(matches!(
        r.svc.launch(a, app, 8).await.unwrap_err(),
        ActivityError::Invalid("channel_id", _)
    ));
    let voice = r.chans.base(1).unwrap();
    r.chans.set(
        c,
        1,
        ChannelPerms {
            use_activities: false,
            ..voice
        },
    );
    assert_eq!(
        r.svc.launch(c, app, 1).await.unwrap_err(),
        ActivityError::Forbidden
    );
    // Voice also needs CONNECT.
    r.chans.set(
        c,
        5,
        ChannelPerms {
            connect: false,
            ..voice
        },
    );
    assert_eq!(
        r.svc.launch(c, app, 5).await.unwrap_err(),
        ActivityError::Forbidden
    );

    let inst = r.svc.launch(a, app, 1).await.unwrap();
    assert_eq!(inst.application_name, "Chess");
    assert_eq!(inst.channel_id, "1");
    assert_eq!(inst.channel_type, "voice");
    assert!(!inst.development);
    assert_eq!(inst.guild_id, "9");
    assert_eq!(inst.participants.len(), 1);
    assert_eq!(inst.participants[0].user_id, a.to_string());
    let id: i64 = inst.id.parse().unwrap();
    assert_eq!(
        r.events.0.lock().unwrap().last().unwrap().0,
        "SR_ACTIVITY_INSTANCE_UPDATE"
    );

    // One per channel.
    assert!(matches!(
        r.svc.launch(b, app, 1).await.unwrap_err(),
        ActivityError::Conflict(_)
    ));
    let there = r.svc.in_channel(b, 1).await.unwrap().unwrap();
    assert_eq!(there.id, inst.id);

    // Join needs the permissions; leave and rejoin are idempotent.
    assert_eq!(
        r.svc.join(c, id).await.unwrap_err(),
        ActivityError::Forbidden
    );
    assert_eq!(r.svc.join(b, id).await.unwrap().participants.len(), 2);
    assert_eq!(r.svc.join(b, id).await.unwrap().participants.len(), 2);
    r.svc.leave(b, id).await.unwrap();
    assert_eq!(r.svc.get(a, id).await.unwrap().participants.len(), 1);
    assert_eq!(r.svc.join(b, id).await.unwrap().participants.len(), 2);

    // Only the starter or a channel manager ends it for everybody.
    assert_eq!(
        r.svc.end(b, id).await.unwrap_err(),
        ActivityError::Forbidden
    );
    r.chans.set(
        b,
        1,
        ChannelPerms {
            manage: true,
            ..voice
        },
    );
    r.svc.end(b, id).await.unwrap();
    assert!(r.svc.in_channel(a, 1).await.unwrap().is_none());
    assert!(matches!(
        r.svc.get(a, id).await.unwrap_err(),
        ActivityError::NotFound("activity")
    ));
    let last = r.events.0.lock().unwrap().last().cloned().unwrap();
    assert!(last.1["instance"].is_null());

    // The last participant leaving ends the instance and frees the channel.
    let again = r.svc.launch(a, app, 1).await.unwrap();
    let id2: i64 = again.id.parse().unwrap();
    r.svc.leave(a, id2).await.unwrap();
    assert!(r.svc.in_channel(a, 1).await.unwrap().is_none());
    assert!(r.svc.launch(b, app, 1).await.is_ok());
    r.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_text_channel_needs_view_and_use_activities_but_no_call() {
    let r = rig().await;
    let app = approved_app(&r, "Chess").await;
    let a = account(r.db.pool()).await;
    let b = account(r.db.pool()).await;
    let text = r.chans.base(3).unwrap();
    // CONNECT is a voice permission: a text channel does not need it.
    r.chans.set(
        a,
        3,
        ChannelPerms {
            connect: false,
            ..text
        },
    );
    r.chans.set(
        b,
        3,
        ChannelPerms {
            use_activities: false,
            ..text
        },
    );
    assert_eq!(
        r.svc.launch(b, app, 3).await.unwrap_err(),
        ActivityError::Forbidden
    );
    let inst = r.svc.launch(a, app, 3).await.unwrap();
    assert_eq!(inst.channel_type, "text");
    assert!(matches!(
        r.svc.launch(a, app, 3).await.unwrap_err(),
        ActivityError::Conflict(_)
    ));
    // A call ending does not touch a text-channel Activity.
    r.svc.voice_left(a, 3).await.unwrap();
    assert_eq!(
        r.svc
            .in_channel(a, 3)
            .await
            .unwrap()
            .unwrap()
            .participants
            .len(),
        1
    );
    r.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn participants_follow_the_voice_session() {
    use sr_core::{ChannelId, GuildId, UserId};
    use sr_events::{DomainEvent, EventBus, InProcessBus, Topic};
    use std::time::Duration;

    let r = rig().await;
    let app = approved_app(&r, "Chess").await;
    let a = account(r.db.pool()).await;
    let b = account(r.db.pool()).await;
    let inst = r.svc.launch(a, app, 1).await.unwrap();
    let id: i64 = inst.id.parse().unwrap();
    r.svc.join(b, id).await.unwrap();

    let bus = Arc::new(InProcessBus::new());
    let follower = VoiceFollower::new(r.svc.clone(), Duration::from_millis(150));
    let _task = follower.spawn(bus.clone());
    tokio::time::sleep(Duration::from_millis(50)).await;
    let left = |u: i64| DomainEvent::VoiceLeft {
        guild: GuildId::new(9),
        channel: ChannelId::new(1),
        user: UserId::new(u as u64),
    };
    let joined = |u: i64| DomainEvent::VoiceJoined {
        guild: GuildId::new(9),
        channel: ChannelId::new(1),
        user: UserId::new(u as u64),
    };
    let count = |r: &Rig| {
        let svc = r.svc.clone();
        async move { svc.get(a, id).await.map(|i| i.participants.len()) }
    };
    let guild = Topic::Guild(GuildId::new(9));

    // A leave followed by a rejoin inside the grace period is not a leave.
    bus.publish(guild, left(b)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(40)).await;
    bus.publish(guild, joined(b)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(count(&r).await.unwrap(), 2);

    // A leave that stays a leave removes the participant; somebody who was never in it changes nothing.
    bus.publish(guild, left(b)).await.unwrap();
    bus.publish(guild, left(a + 1)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(count(&r).await.unwrap(), 1);
    let ev = r.events.0.lock().unwrap().last().cloned().unwrap();
    assert_eq!(
        ev.1["instance"]["participants"].as_array().unwrap().len(),
        1
    );

    // The last one out ends the instance and dispatches a null instance.
    bus.publish(guild, left(a)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(r.svc.in_channel(a, 1).await.unwrap().is_none());
    let ev = r.events.0.lock().unwrap().last().cloned().unwrap();
    assert!(ev.1["instance"].is_null());
    // The channel is free again.
    assert!(r.svc.launch(a, app, 1).await.is_ok());
    r.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn an_instance_carries_the_capabilities_of_its_reviewed_bundle() {
    let r = rig().await;
    let owner = account(r.db.pool()).await;
    let app = application(r.db.pool(), owner, "Board").await;
    r.svc.save_config(app, input(None)).await.unwrap();
    let zip = zip_of(&[
        ("index.html", b"<h1>board</h1>"),
        ("sideroom.json", br#"{"capabilities":["datachannel"]}"#),
    ]);
    let up = r.svc.upload_bundle(app, owner, zip).await.unwrap();
    assert_eq!(up.bundle.capabilities, vec!["datachannel"]);
    let id: i64 = up.bundle.id.parse().unwrap();
    r.svc.submit(app, id).await.unwrap();
    r.svc.decide(app, 1, id, true, None).await.unwrap();
    set_state(&r, app, "released").await;
    let member = account(r.db.pool()).await;
    let inst = r.svc.launch(member, app, 1).await.unwrap();
    assert_eq!(inst.capabilities, vec!["datachannel"]);
    r.svc.end(member, inst.id.parse().unwrap()).await.unwrap();

    // P36-ACT-1: a data-channel instance is host-authoritative with no handover, so its starter leaving ends it
    // for everybody, by Leave or by leaving the call; a guest leaving does not.
    let guest = account(r.db.pool()).await;
    let inst = r.svc.launch(member, app, 1).await.unwrap();
    let id: i64 = inst.id.parse().unwrap();
    r.svc.join(guest, id).await.unwrap();
    let third = account(r.db.pool()).await;
    r.svc.join(third, id).await.unwrap();
    r.svc.leave(third, id).await.unwrap();
    assert_eq!(r.svc.get(guest, id).await.unwrap().participants.len(), 2);
    r.svc.leave(member, id).await.unwrap();
    assert!(r.svc.in_channel(guest, 1).await.unwrap().is_none());
    let last = r.events.0.lock().unwrap().last().cloned().unwrap();
    assert!(last.1["instance"].is_null());
    let inst = r.svc.launch(member, app, 1).await.unwrap();
    let id: i64 = inst.id.parse().unwrap();
    r.svc.join(guest, id).await.unwrap();
    r.svc.voice_left(member, 1).await.unwrap();
    assert!(r.svc.in_channel(guest, 1).await.unwrap().is_none());

    // A bundle without a manifest declares nothing, and its starter may leave while others play on.
    let plain = approved_app(&r, "Plain").await;
    let inst = r.svc.launch(member, plain, 1).await.unwrap();
    assert!(inst.capabilities.is_empty());
    let id: i64 = inst.id.parse().unwrap();
    r.svc.join(guest, id).await.unwrap();
    r.svc.leave(member, id).await.unwrap();
    assert_eq!(
        r.svc
            .in_channel(guest, 1)
            .await
            .unwrap()
            .unwrap()
            .participants
            .len(),
        1
    );
    r.db.cleanup().await.unwrap();
}

/// D-388 (wave 46, ACT-LND-002): a data-channel game that declares `host_handoff` keeps running when its host
/// leaves; the longest-connected member left becomes the host (earliest join, ties by user id), and the next host
/// leaving moves it again. The last one out still ends it. A rejoin counts from the rejoin.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_host_handoff_game_moves_its_host_to_the_longest_connected_member() {
    let r = rig().await;
    let owner = account(r.db.pool()).await;
    let app = application(r.db.pool(), owner, "Relay").await;
    r.svc.save_config(app, input(None)).await.unwrap();
    let zip = zip_of(&[
        ("index.html", b"<h1>relay</h1>"),
        (
            "sideroom.json",
            br#"{"capabilities":["datachannel","host_handoff"]}"#,
        ),
    ]);
    let up = r.svc.upload_bundle(app, owner, zip).await.unwrap();
    let bid: i64 = up.bundle.id.parse().unwrap();
    r.svc.submit(app, bid).await.unwrap();
    r.svc.decide(app, 1, bid, true, None).await.unwrap();
    set_state(&r, app, "released").await;

    let host = account(r.db.pool()).await;
    let first = account(r.db.pool()).await;
    let second = account(r.db.pool()).await;
    let inst = r.svc.launch(host, app, 1).await.unwrap();
    assert_eq!(inst.host_id, host.to_string());
    assert_eq!(inst.started_by, host.to_string());
    assert!(inst.capabilities.iter().any(|c| c == "host_handoff"));
    let id: i64 = inst.id.parse().unwrap();
    r.svc.join(first, id).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    r.svc.join(second, id).await.unwrap();

    // A non-host leaving changes nothing about the host.
    r.svc.leave(second, id).await.unwrap();
    assert_eq!(
        r.svc.get(first, id).await.unwrap().host_id,
        host.to_string()
    );
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    r.svc.join(second, id).await.unwrap();

    // The host leaves: `first` joined before `second` (who rejoined later), so `first` hosts now. The announced
    // instance carries the new host; the starter is still the starter.
    r.svc.leave(host, id).await.unwrap();
    let now = r.svc.get(first, id).await.unwrap();
    assert_eq!(now.host_id, first.to_string());
    assert_eq!(now.started_by, host.to_string());
    let ev = r.events.0.lock().unwrap().last().cloned().unwrap();
    assert_eq!(ev.1["instance"]["host_id"], first.to_string());

    // The old host comes back as a player; the new host leaves the call: `second` joined before the old host's
    // rejoin, so `second` is next.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    r.svc.join(host, id).await.unwrap();
    r.svc.voice_left(first, 1).await.unwrap();
    assert_eq!(
        r.svc.get(second, id).await.unwrap().host_id,
        second.to_string()
    );

    // Ties on the join time go to the smaller user id.
    r.svc.join(first, id).await.unwrap();
    sqlx::query("UPDATE activity_participants SET joined_at = '2026-01-01' WHERE instance_id = $1")
        .bind(id)
        .execute(r.db.pool())
        .await
        .unwrap();
    r.svc.leave(second, id).await.unwrap();
    let tie = host.min(first);
    assert_eq!(r.svc.get(host, id).await.unwrap().host_id, tie.to_string());

    // The last one out ends it.
    r.svc.leave(tie, id).await.unwrap();
    r.svc.leave(host.max(first), id).await.unwrap();
    assert!(r.svc.in_channel(first, 1).await.unwrap().is_none());
    r.db.cleanup().await.unwrap();
}
/// OPS-FLG-001 `platform.activities_datachannel`: while the switch is off an instance runs without the
/// `datachannel` capability (its host then never asks the call for a data channel); switched on, it has it again.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_datachannel_switch_strips_the_capability_from_new_and_running_instances() {
    let r = rig().await;
    let owner = account(r.db.pool()).await;
    let app = application(r.db.pool(), owner, "Board").await;
    r.svc.save_config(app, input(None)).await.unwrap();
    let zip = zip_of(&[
        ("index.html", b"<h1>board</h1>"),
        ("sideroom.json", br#"{"capabilities":["datachannel"]}"#),
    ]);
    let up = r.svc.upload_bundle(app, owner, zip).await.unwrap();
    let id: i64 = up.bundle.id.parse().unwrap();
    r.svc.submit(app, id).await.unwrap();
    r.svc.decide(app, 1, id, true, None).await.unwrap();
    set_state(&r, app, "released").await;
    let member = account(r.db.pool()).await;

    r.svc.set_datachannel_allowed(false);
    let inst = r.svc.launch(member, app, 1).await.unwrap();
    assert!(inst.capabilities.is_empty(), "{:?}", inst.capabilities);
    let running = r.svc.in_channel(member, 1).await.unwrap().unwrap();
    assert!(running.capabilities.is_empty());
    r.svc.set_datachannel_allowed(true);
    let running = r.svc.in_channel(member, 1).await.unwrap().unwrap();
    assert_eq!(running.capabilities, vec!["datachannel"]);
    r.db.cleanup().await.unwrap();
}

/// ARC-SBX-001 (D-399): the proxy token is the instance's own credential for URL-mapping writes from the
/// opaque-origin sandbox. Only a current participant gets one, it is valid only for the instance's own
/// application while the instance runs and its member stays, and minting again replaces it.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn proxy_tokens_belong_to_a_participant_of_a_running_instance() {
    let r = rig().await;
    let app = approved_app(&r, "Mapped").await;
    let other = approved_app(&r, "Other").await;
    let a = account(r.db.pool()).await;
    let b = account(r.db.pool()).await;
    let inst = r.svc.launch(a, app, 3).await.unwrap();
    let id: i64 = inst.id.parse().unwrap();

    // A member who has not joined gets nothing; neither does anybody for a missing instance.
    assert!(matches!(
        r.svc.mint_proxy_token(b, id).await.unwrap_err(),
        ActivityError::NotFound("activity")
    ));
    assert!(matches!(
        r.svc.mint_proxy_token(a, 404).await.unwrap_err(),
        ActivityError::NotFound("activity")
    ));

    let t = r.svc.mint_proxy_token(a, id).await.unwrap();
    assert_eq!(t.len(), 64);
    assert!(t.bytes().all(|c| c.is_ascii_hexdigit()));
    assert!(r.svc.proxy_token_valid(app, &t).await.unwrap());
    // Bound to the instance's own application; garbage is refused without a lookup.
    assert!(!r.svc.proxy_token_valid(other, &t).await.unwrap());
    assert!(!r.svc.proxy_token_valid(app, "").await.unwrap());
    assert!(!r.svc.proxy_token_valid(app, &"0".repeat(64)).await.unwrap());
    assert!(!r.svc.proxy_token_valid(app, &t[..63]).await.unwrap());
    // Only the digest is stored.
    let stored: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM activity_proxy_tokens WHERE encode(token_sha256, 'hex') = $1",
    )
    .bind(&t)
    .fetch_one(r.db.pool())
    .await
    .unwrap();
    assert_eq!(stored, 0);

    // Minting again replaces the token.
    let t2 = r.svc.mint_proxy_token(a, id).await.unwrap();
    assert_ne!(t, t2);
    assert!(!r.svc.proxy_token_valid(app, &t).await.unwrap());
    assert!(r.svc.proxy_token_valid(app, &t2).await.unwrap());

    // Leaving drops it, and a rejoin does not bring it back.
    r.svc.join(b, id).await.unwrap();
    let tb = r.svc.mint_proxy_token(b, id).await.unwrap();
    assert!(r.svc.proxy_token_valid(app, &tb).await.unwrap());
    r.svc.leave(b, id).await.unwrap();
    assert!(!r.svc.proxy_token_valid(app, &tb).await.unwrap());
    r.svc.join(b, id).await.unwrap();
    assert!(!r.svc.proxy_token_valid(app, &tb).await.unwrap());
    assert!(r.svc.proxy_token_valid(app, &t2).await.unwrap());

    // The end of the instance ends every token.
    r.svc.end(a, id).await.unwrap();
    assert!(!r.svc.proxy_token_valid(app, &t2).await.unwrap());
    assert!(matches!(
        r.svc.mint_proxy_token(a, id).await.unwrap_err(),
        ActivityError::NotFound("activity")
    ));
}

/// A released application whose reviewed bundle declares `capabilities` (and is TIER_1_ORIGINAL).
async fn app_with_caps(r: &Rig, name: &str, caps: &str) -> i64 {
    let owner = account(r.db.pool()).await;
    let app = application(r.db.pool(), owner, name).await;
    r.svc.save_config(app, input(None)).await.unwrap();
    let manifest = format!(r#"{{"tier":"TIER_1_ORIGINAL","capabilities":[{caps}]}}"#);
    let zip = zip_of(&[
        ("index.html", b"<h1>game</h1>"),
        ("sideroom.json", manifest.as_bytes()),
    ]);
    let up = r.svc.upload_bundle(app, owner, zip).await.unwrap();
    let id: i64 = up.bundle.id.parse().unwrap();
    r.svc.submit(app, id).await.unwrap();
    r.svc.decide(app, 1, id, true, None).await.unwrap();
    set_state(r, app, "released").await;
    app
}

/// ARC-GST-001 (D-385, D-397, D-399): a guest link is a lobby token for a running instance. It needs a member of the
/// instance and a game that declares `guest_play`; only its digest is stored; it stops working when revoked (by
/// the lobby host or the member who minted it), when it expires and when the instance ends; a guest session never
/// outlives its link; a dead link is one answer.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn guest_links_are_digest_only_expiring_revocable_and_end_with_the_lobby() {
    use sr_activities::guest;
    let r = rig().await;
    let plain = approved_app(&r, "NoGuests").await;
    let game = app_with_caps(
        &r,
        "Guests",
        r#""datachannel","guest_play","lobby_channel","ingame_voice""#,
    )
    .await;
    let host = account(r.db.pool()).await;
    let other = account(r.db.pool()).await;
    let stranger = account(r.db.pool()).await;

    // A game that does not declare guest_play gives no link (409), a stranger gets 404.
    let p = r.svc.launch(host, plain, 5).await.unwrap();
    assert!(matches!(
        r.svc
            .create_guest_link(host, p.id.parse().unwrap())
            .await
            .unwrap_err(),
        ActivityError::Conflict(_)
    ));
    r.svc.end(host, p.id.parse().unwrap()).await.unwrap();

    let inst = r.svc.launch(host, game, 1).await.unwrap();
    // guest_play and lobby_channel run; ingame_voice validates in the manifest but is never handed out.
    assert_eq!(
        inst.capabilities,
        vec!["datachannel", "guest_play", "lobby_channel"]
    );
    let id: i64 = inst.id.parse().unwrap();
    assert!(matches!(
        r.svc.create_guest_link(stranger, id).await.unwrap_err(),
        ActivityError::NotFound("activity")
    ));
    assert!(matches!(
        r.svc.create_guest_link(host, 404).await.unwrap_err(),
        ActivityError::NotFound("activity")
    ));

    let link = r.svc.create_guest_link(host, id).await.unwrap();
    assert!(guest::is_lobby_token(&link.lobby_token));
    assert!(link.expires_at.ends_with('Z') && link.expires_at.contains('T'));
    // Only the digest is stored.
    let raw: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM activity_guest_links WHERE encode(lobby_sha256, 'hex') = $1",
    )
    .bind(&link.lobby_token)
    .fetch_one(r.db.pool())
    .await
    .unwrap();
    assert_eq!(raw, 0);
    let ttl: f64 = sqlx::query_scalar(
        "SELECT EXTRACT(EPOCH FROM expires_at - created_at)::float8 FROM activity_guest_links",
    )
    .fetch_one(r.db.pool())
    .await
    .unwrap();
    assert!((ttl - guest::LINK_TTL_SECS as f64).abs() < 5.0, "{ttl}");

    // Joining: garbage and unknown tokens are the same 404; a good one gets a callsign and a topic.
    for bad in ["", "abc", &"0".repeat(64), &"Z".repeat(64)] {
        assert!(matches!(
            r.svc.guest_join(bad).await.unwrap_err(),
            ActivityError::NotFound("guest link")
        ));
    }
    let joined = r.svc.guest_join(&link.lobby_token).await.unwrap();
    assert!(guest::is_guest_token(&joined.guest_token));
    assert_eq!(joined.topic, format!("game:{}", link.lobby_token));
    assert_eq!(joined.socket_path, "/gateway/guest");
    assert_eq!(joined.application_name, "Guests");
    // The runner sees the manifest's flags, the reserved `ingame_voice` included, so it can refuse it.
    assert_eq!(
        joined.capabilities,
        vec!["datachannel", "guest_play", "ingame_voice", "lobby_channel"]
    );
    assert_eq!(joined.callsign.split(' ').count(), 3);
    assert!(joined.expires_at <= link.expires_at);
    let stored_raw: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM activity_guest_sessions WHERE encode(token_sha256, 'hex') = $1",
    )
    .bind(&joined.guest_token[4..])
    .fetch_one(r.db.pool())
    .await
    .unwrap();
    assert_eq!(stored_raw, 0);

    // The seats: the guest's, and the members' (Host for the starter, Player for another participant).
    let seat = r
        .svc
        .guest_seating(&joined.guest_token)
        .await
        .unwrap()
        .unwrap();
    assert!(!seat.member);
    assert_eq!(seat.callsign, joined.callsign);
    assert_eq!(seat.lobby, guest::lobby_id(&link.lobby_token));
    assert!(r.svc.guest_seating("srg_nope").await.unwrap().is_none());
    assert!(
        r.svc
            .guest_seating(&link.lobby_token)
            .await
            .unwrap()
            .is_none()
    );
    let m = r
        .svc
        .member_seating(host, &link.lobby_token)
        .await
        .unwrap()
        .unwrap();
    assert!(m.member && m.callsign == "Host" && m.lobby == seat.lobby);
    assert!(
        r.svc
            .member_seating(other, &link.lobby_token)
            .await
            .unwrap()
            .is_none()
    );
    r.svc.join(other, id).await.unwrap();
    let m2 = r
        .svc
        .member_seating(other, &link.lobby_token)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(m2.callsign, "Player");
    assert!(
        r.svc
            .member_seating(stranger, &link.lobby_token)
            .await
            .unwrap()
            .is_none()
    );

    // Revoking: a participant who is neither the host nor the minter is refused; a stranger sees nothing.
    let second = r.svc.create_guest_link(other, id).await.unwrap();
    assert!(matches!(
        r.svc
            .revoke_guest_link(other, id, &link.lobby_token)
            .await
            .unwrap_err(),
        ActivityError::Forbidden
    ));
    assert!(matches!(
        r.svc
            .revoke_guest_link(stranger, id, &link.lobby_token)
            .await
            .unwrap_err(),
        ActivityError::NotFound("activity")
    ));
    assert!(matches!(
        r.svc
            .revoke_guest_link(host, id, &"1".repeat(64))
            .await
            .unwrap_err(),
        ActivityError::NotFound("guest link")
    ));
    // The host revokes a link another member minted; the minter revokes their own.
    let guest2 = r.svc.guest_join(&second.lobby_token).await.unwrap();
    r.svc
        .revoke_guest_link(host, id, &second.lobby_token)
        .await
        .unwrap();
    assert!(
        r.svc
            .guest_seating(&guest2.guest_token)
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        r.svc.guest_join(&second.lobby_token).await.unwrap_err(),
        ActivityError::NotFound("guest link")
    ));
    assert!(
        r.svc
            .member_seating(other, &second.lobby_token)
            .await
            .unwrap()
            .is_none()
    );
    let third = r.svc.create_guest_link(other, id).await.unwrap();
    r.svc
        .revoke_guest_link(other, id, &third.lobby_token)
        .await
        .unwrap();
    // The first link is untouched by the others' revocations.
    assert!(
        r.svc
            .guest_seating(&joined.guest_token)
            .await
            .unwrap()
            .is_some()
    );

    // Expiry: an expired session is dead on its own; an expired link kills its sessions and joins.
    sqlx::query(
        "UPDATE activity_guest_sessions SET expires_at = now() - interval '1 second' WHERE token_sha256 = $1",
    )
    .bind(guest::digest(&joined.guest_token))
    .execute(r.db.pool())
    .await
    .unwrap();
    assert!(
        r.svc
            .guest_seating(&joined.guest_token)
            .await
            .unwrap()
            .is_none()
    );
    let again = r.svc.guest_join(&link.lobby_token).await.unwrap();
    sqlx::query(
        "UPDATE activity_guest_links SET expires_at = now() - interval '1 second' WHERE lobby_sha256 = $1",
    )
    .bind(guest::digest(&link.lobby_token))
    .execute(r.db.pool())
    .await
    .unwrap();
    assert!(
        r.svc
            .guest_seating(&again.guest_token)
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        r.svc.guest_join(&link.lobby_token).await.unwrap_err(),
        ActivityError::NotFound("guest link")
    ));
    assert!(
        r.svc
            .member_seating(host, &link.lobby_token)
            .await
            .unwrap()
            .is_none()
    );

    // The lobby's end ends the links: a fresh link dies with the instance and does not come back with a new one.
    let live = r.svc.create_guest_link(host, id).await.unwrap();
    let g = r.svc.guest_join(&live.lobby_token).await.unwrap();
    r.svc.end(host, id).await.unwrap();
    assert!(r.svc.guest_seating(&g.guest_token).await.unwrap().is_none());
    assert!(matches!(
        r.svc.guest_join(&live.lobby_token).await.unwrap_err(),
        ActivityError::NotFound("guest link")
    ));
    assert!(
        r.svc
            .member_seating(host, &live.lobby_token)
            .await
            .unwrap()
            .is_none()
    );
    let next = r.svc.launch(host, game, 1).await.unwrap();
    assert!(r.svc.guest_seating(&g.guest_token).await.unwrap().is_none());
    assert!(matches!(
        r.svc
            .revoke_guest_link(host, next.id.parse().unwrap(), &live.lobby_token)
            .await
            .unwrap_err(),
        ActivityError::NotFound("guest link")
    ));
    r.db.cleanup().await.unwrap();
}

/// The caps on a lobby: five working links per instance and sixteen guests per link.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_lobby_caps_its_links_and_its_guests() {
    use sr_activities::guest;
    let r = rig().await;
    let game = app_with_caps(&r, "Caps", r#""guest_play""#).await;
    let host = account(r.db.pool()).await;
    let inst = r.svc.launch(host, game, 1).await.unwrap();
    let id: i64 = inst.id.parse().unwrap();
    let mut links = Vec::new();
    for _ in 0..guest::MAX_LINKS_PER_INSTANCE {
        links.push(r.svc.create_guest_link(host, id).await.unwrap());
    }
    assert!(matches!(
        r.svc.create_guest_link(host, id).await.unwrap_err(),
        ActivityError::Conflict(_)
    ));
    // Revoking one frees a place.
    r.svc
        .revoke_guest_link(host, id, &links[0].lobby_token)
        .await
        .unwrap();
    assert!(r.svc.create_guest_link(host, id).await.is_ok());
    for _ in 0..guest::MAX_GUESTS_PER_LOBBY {
        r.svc.guest_join(&links[1].lobby_token).await.unwrap();
    }
    assert!(matches!(
        r.svc.guest_join(&links[1].lobby_token).await.unwrap_err(),
        ActivityError::Conflict(_)
    ));
    r.db.cleanup().await.unwrap();
}

// ------------------------------------------------------------------ lobbies (ARC-LOB-001, D-483 to D-487)

/// Guild `guild` named `name` with `channels` (`(id, type, name)`, Discord types: 0 text, 2 voice) and
/// `members` in it.
async fn server(r: &Rig, guild: i64, name: &str, channels: &[(i64, i16, &str)], members: &[i64]) {
    let owner = account(r.db.pool()).await;
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, $2, $3)")
        .bind(guild)
        .bind(name)
        .bind(owner)
        .execute(r.db.pool())
        .await
        .unwrap();
    for (id, kind, cname) in channels {
        sqlx::query("INSERT INTO channels (id, guild_id, type, name) VALUES ($1, $2, $3, $4)")
            .bind(id)
            .bind(guild)
            .bind(kind)
            .bind(cname)
            .execute(r.db.pool())
            .await
            .unwrap();
    }
    for m in members {
        sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
            .bind(guild)
            .bind(m)
            .execute(r.db.pool())
            .await
            .unwrap();
    }
}

/// Guild 9 "Den": 1 a voice channel "Lounge", 3 a text channel "general", 5 a voice channel "Back room".
async fn den(r: &Rig, members: &[i64]) {
    server(
        r,
        9,
        "Den",
        &[(1, 2, "Lounge"), (3, 0, "general"), (5, 2, "Back room")],
        members,
    )
    .await;
}

fn iid(v: &sr_activities::model::InstanceView) -> i64 {
    v.id.parse().unwrap()
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_lobby_in_a_channel_the_member_cannot_see_is_not_listed() {
    let r = rig().await;
    let app = approved_app(&r, "Chess").await;
    let a = account(r.db.pool()).await;
    let s = account(r.db.pool()).await;
    den(&r, &[a, s]).await;
    r.svc.launch(s, app, 1).await.unwrap();
    r.svc.launch(s, app, 3).await.unwrap();
    let hidden = ChannelPerms {
        view: false,
        ..r.chans.base(1).unwrap()
    };
    r.chans.set(a, 1, hidden);
    let list = r.svc.lobbies(a).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].channel_id, "3");
    assert_eq!(list[0].channel_name, "general");
    assert_eq!(list[0].guild_name, "Den");
    assert_eq!(list[0].channel_type, "text");
    assert_eq!(list[0].application_name, "Chess");
    assert_eq!(list[0].player_count, 1);
    assert!(!list[0].joined);
    assert!(!list[0].development);
    // The starter sees both.
    assert_eq!(r.svc.lobbies(s).await.unwrap().len(), 2);
    r.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_lobby_in_a_server_they_are_not_in_is_not_listed() {
    let r = rig().await;
    let app = approved_app(&r, "Chess").await;
    let a = account(r.db.pool()).await;
    let s = account(r.db.pool()).await;
    den(&r, &[a]).await;
    server(&r, 10, "Elsewhere", &[(6, 0, "general")], &[s]).await;
    // Even a channel port that (wrongly) said `a` could see the other server's channel is never asked: the
    // candidates come from `a`'s own memberships.
    let other = ChannelPerms {
        guild_id: 10,
        ..r.chans.base(3).unwrap()
    };
    r.chans.set(s, 6, other);
    r.chans.set(a, 6, other);
    r.svc.launch(s, app, 6).await.unwrap();
    assert!(r.svc.lobbies(a).await.unwrap().is_empty());
    assert_eq!(r.svc.lobbies(s).await.unwrap().len(), 1);
    r.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_voice_lobby_needs_connect_and_use_activities_a_text_lobby_use_activities() {
    let r = rig().await;
    let app = approved_app(&r, "Chess").await;
    let a = account(r.db.pool()).await;
    let b = account(r.db.pool()).await;
    let s = account(r.db.pool()).await;
    den(&r, &[a, b, s]).await;
    r.svc.launch(s, app, 1).await.unwrap();
    r.svc.launch(s, app, 3).await.unwrap();
    let voice = r.chans.base(1).unwrap();
    let text = r.chans.base(3).unwrap();
    // `a`: no CONNECT anywhere. The voice lobby goes, the text lobby stays (CONNECT is a voice permission).
    r.chans.set(
        a,
        1,
        ChannelPerms {
            connect: false,
            ..voice
        },
    );
    r.chans.set(
        a,
        3,
        ChannelPerms {
            connect: false,
            ..text
        },
    );
    let list = r.svc.lobbies(a).await.unwrap();
    assert_eq!(
        list.iter()
            .map(|l| l.channel_id.as_str())
            .collect::<Vec<_>>(),
        ["3"]
    );
    // `b`: no USE_EMBEDDED_ACTIVITIES anywhere. Nothing.
    for (ch, p) in [(1, voice), (3, text)] {
        r.chans.set(
            b,
            ch,
            ChannelPerms {
                use_activities: false,
                ..p
            },
        );
    }
    assert!(r.svc.lobbies(b).await.unwrap().is_empty());
    r.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_lobby_item_names_nobody() {
    let r = rig().await;
    let app = approved_app(&r, "Chess").await;
    let a = account(r.db.pool()).await;
    let s = account(r.db.pool()).await;
    den(&r, &[a, s]).await;
    r.svc.launch(s, app, 1).await.unwrap();
    let list = r.svc.lobbies(a).await.unwrap();
    let v = serde_json::to_value(&list[0]).unwrap();
    let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "application_id",
            "application_name",
            "channel_id",
            "channel_name",
            "channel_type",
            "created_at_ms",
            "development",
            "guild_id",
            "guild_name",
            "instance_id",
            "joined",
            "player_count",
        ]
    );
    let raw = v.to_string();
    for banned in [
        "user_id",
        "started_by",
        "host",
        "participants",
        "lobby_token",
    ] {
        assert!(!raw.contains(banned), "{banned} in {raw}");
    }
    assert!(
        !raw.contains(&s.to_string()),
        "the starter's id leaked: {raw}"
    );
    r.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_callers_own_lobby_comes_first_then_the_fullest() {
    let r = rig().await;
    let app = approved_app(&r, "Chess").await;
    let a = account(r.db.pool()).await;
    let b = account(r.db.pool()).await;
    let c = account(r.db.pool()).await;
    let d = account(r.db.pool()).await;
    den(&r, &[a, b, c, d]).await;
    r.svc.launch(a, app, 5).await.unwrap();
    let full = r.svc.launch(c, app, 3).await.unwrap();
    r.svc.join(d, iid(&full)).await.unwrap();
    r.svc.launch(b, app, 1).await.unwrap();
    let list = r.svc.lobbies(a).await.unwrap();
    assert_eq!(
        list.iter()
            .map(|l| (l.channel_id.as_str(), l.player_count, l.joined))
            .collect::<Vec<_>>(),
        [("5", 1, true), ("3", 2, false), ("1", 1, false)]
    );
    r.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn an_ended_instance_is_not_a_lobby() {
    let r = rig().await;
    let app = approved_app(&r, "Chess").await;
    let a = account(r.db.pool()).await;
    let s = account(r.db.pool()).await;
    den(&r, &[a, s]).await;
    let inst = r.svc.launch(s, app, 3).await.unwrap();
    assert_eq!(r.svc.lobbies(a).await.unwrap().len(), 1);
    r.svc.end(s, iid(&inst)).await.unwrap();
    assert!(r.svc.lobbies(a).await.unwrap().is_empty());
    r.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_list_stops_at_twenty() {
    let r = rig().await;
    let app = approved_app(&r, "Chess").await;
    let a = account(r.db.pool()).await;
    let s = account(r.db.pool()).await;
    let names: Vec<String> = (100..125).map(|i| format!("room-{i}")).collect();
    let chans: Vec<(i64, i16, &str)> = names
        .iter()
        .enumerate()
        .map(|(n, name)| (100 + n as i64, 0, name.as_str()))
        .collect();
    server(&r, 9, "Den", &chans, &[a, s]).await;
    let text = r.chans.base(3).unwrap();
    for (id, _, _) in &chans {
        r.chans.default.lock().unwrap().insert(*id, text);
        r.svc.launch(s, app, *id).await.unwrap();
    }
    assert_eq!(
        r.svc.lobbies(a).await.unwrap().len(),
        sr_activities::service::LOBBY_MAX
    );
    r.db.cleanup().await.unwrap();
}

/// SEC-AUD-097: after a host handoff the current host (what seating uses) may revoke guest links; the original
/// starter, no longer the host, may not unless they minted the link.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_current_host_revokes_guest_links_after_a_handoff() {
    let r = rig().await;
    let game = app_with_caps(
        &r,
        "GuestsHandoff",
        r#""datachannel","guest_play","lobby_channel""#,
    )
    .await;
    let starter = account(r.db.pool()).await;
    let new_host = account(r.db.pool()).await;
    let third = account(r.db.pool()).await;
    let inst = r.svc.launch(starter, game, 1).await.unwrap();
    let id: i64 = inst.id.parse().unwrap();
    r.svc.join(new_host, id).await.unwrap();
    r.svc.join(third, id).await.unwrap();
    let a = r.svc.create_guest_link(third, id).await.unwrap();
    let b = r.svc.create_guest_link(third, id).await.unwrap();
    sqlx::query("UPDATE activity_instances SET host_id = $2 WHERE id = $1")
        .bind(id)
        .bind(new_host)
        .execute(r.db.pool())
        .await
        .unwrap();
    assert!(matches!(
        r.svc
            .revoke_guest_link(starter, id, &a.lobby_token)
            .await
            .unwrap_err(),
        ActivityError::Forbidden
    ));
    r.svc
        .revoke_guest_link(new_host, id, &a.lobby_token)
        .await
        .unwrap();
    // The link's creator still may.
    r.svc
        .revoke_guest_link(third, id, &b.lobby_token)
        .await
        .unwrap();
}

/// Move every superseded bundle's `superseded_at` back by `days` days.
async fn age_superseded(r: &Rig, days: i32) {
    sqlx::query(
        "UPDATE application_activity_bundles SET superseded_at = now() - make_interval(days => $1) \
         WHERE state = 'superseded'",
    )
    .bind(days)
    .execute(r.db.pool())
    .await
    .unwrap();
}

/// An application with a first bundle that a second upload has superseded; returns (first sha, second sha).
async fn superseded_pair(r: &Rig) -> (i64, String, String) {
    let owner = account(r.db.pool()).await;
    let app = application(r.db.pool(), owner, "Gc").await;
    r.svc.save_config(app, input(None)).await.unwrap();
    let one = r.svc.upload_bundle(app, owner, site("one")).await.unwrap();
    let two = r.svc.upload_bundle(app, owner, site("two")).await.unwrap();
    (app, one.bundle.sha256, two.bundle.sha256)
}

async fn has_objects(r: &Rig, sha: &str) -> bool {
    r.storage
        .list("activities/")
        .await
        .unwrap()
        .iter()
        .any(|o| o.key.contains(sha))
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn superseding_a_bundle_stamps_the_time() {
    let r = rig().await;
    let (app, one, two) = superseded_pair(&r).await;
    let stamp = |sha: String| {
        let pool = r.db.pool().clone();
        async move {
            sqlx::query_scalar::<_, Option<f64>>(
                "SELECT EXTRACT(EPOCH FROM now() - superseded_at)::float8 FROM application_activity_bundles \
                 WHERE application_id = $1 AND sha256 = $2",
            )
            .bind(app)
            .bind(sha)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    let t = stamp(one).await.expect("the superseded bundle is stamped");
    assert!((0.0..60.0).contains(&t));
    assert!(stamp(two).await.is_none(), "a live bundle has no stamp");
    r.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn gc_keeps_a_bundle_superseded_within_the_grace_period() {
    let r = rig().await;
    let (_, one, two) = superseded_pair(&r).await;
    assert_eq!(r.svc.gc().await.unwrap(), 0);
    assert!(has_objects(&r, &one).await && has_objects(&r, &two).await);
    r.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn gc_removes_a_bundle_superseded_long_ago() {
    let r = rig().await;
    let (app, one, two) = superseded_pair(&r).await;
    age_superseded(&r, 2).await;
    assert!(r.svc.gc().await.unwrap() > 0);
    assert!(!has_objects(&r, &one).await);
    assert!(has_objects(&r, &two).await);

    // A row superseded before the column existed (NULL) counts as long ago.
    let owner = account(r.db.pool()).await;
    let three = r
        .svc
        .upload_bundle(app, owner, site("three"))
        .await
        .unwrap();
    assert!(has_objects(&r, &two).await && has_objects(&r, &three.bundle.sha256).await);
    sqlx::query(
        "UPDATE application_activity_bundles SET superseded_at = NULL WHERE state = 'superseded'",
    )
    .execute(r.db.pool())
    .await
    .unwrap();
    assert!(r.svc.gc().await.unwrap() > 0);
    assert!(!has_objects(&r, &two).await);
    assert!(has_objects(&r, &three.bundle.sha256).await);
    r.db.cleanup().await.unwrap();
}

/// Storage whose `list` runs a hook before reading the bucket, to model an upload that lands during the cleanup.
struct HookedList {
    inner: Arc<dyn ObjectStorage>,
    hook: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    resume: Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
}

#[async_trait]
impl ObjectStorage for HookedList {
    async fn put(&self, k: &str, d: bytes::Bytes) -> sr_storage::Result<()> {
        self.inner.put(k, d).await
    }
    async fn get(&self, k: &str) -> sr_storage::Result<bytes::Bytes> {
        self.inner.get(k).await
    }
    async fn get_range(&self, k: &str, s: u64, e: u64) -> sr_storage::Result<bytes::Bytes> {
        self.inner.get_range(k, s, e).await
    }
    async fn head(&self, k: &str) -> sr_storage::Result<sr_storage::ObjectInfo> {
        self.inner.head(k).await
    }
    async fn delete(&self, k: &str) -> sr_storage::Result<()> {
        self.inner.delete(k).await
    }
    async fn list(&self, p: &str) -> sr_storage::Result<Vec<sr_storage::ObjectInfo>> {
        let tx = self.hook.lock().unwrap().take();
        let rx = self.resume.lock().unwrap().take();
        if let (Some(tx), Some(rx)) = (tx, rx) {
            let _ = tx.send(());
            let _ = rx.await;
        }
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
    async fn multipart_create(
        &self,
        k: &str,
    ) -> sr_storage::Result<Box<dyn sr_storage::MultipartSession>> {
        self.inner.multipart_create(k).await
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn gc_keeps_a_bundle_uploaded_while_it_runs() {
    let db = TestDb::new().await.unwrap();
    let inner: Arc<dyn ObjectStorage> = Arc::new(ObjectStoreStorage::in_memory());
    let (hit_tx, hit_rx) = tokio::sync::oneshot::channel();
    let (go_tx, go_rx) = tokio::sync::oneshot::channel();
    let storage = Arc::new(HookedList {
        inner: inner.clone(),
        hook: Mutex::new(Some(hit_tx)),
        resume: Mutex::new(Some(go_rx)),
    });
    let svc = Arc::new(
        ActivityService::new(db.pool().clone(), Arc::new(uid))
            .with_storage(storage)
            .with_channels(Arc::new(FakeChannels::new()))
            .with_events(Arc::new(Recorder::default())),
    );
    let owner = account(db.pool()).await;
    let app = application(db.pool(), owner, "Race").await;
    svc.save_config(app, input(None)).await.unwrap();

    // The cleanup pauses on entering its listing, an upload lands, and the listing then sees the fresh objects.
    let gc = tokio::spawn({
        let svc = svc.clone();
        async move { svc.gc().await.unwrap() }
    });
    hit_rx.await.unwrap();
    let up = svc.upload_bundle(app, owner, site("fresh")).await.unwrap();
    go_tx.send(()).unwrap();
    gc.await.unwrap();
    let left = inner.list("activities/").await.unwrap();
    assert!(left.iter().any(|o| o.key.contains(&up.bundle.sha256)));
    // And one that was listed before its row existed is not deleted either way round.
    assert_eq!(svc.gc().await.unwrap(), 0);
    db.cleanup().await.unwrap();
}
