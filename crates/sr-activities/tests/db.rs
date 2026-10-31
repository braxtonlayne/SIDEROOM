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

    // A bundle without a manifest declares nothing.
    let plain = approved_app(&r, "Plain").await;
    let inst = r.svc.launch(member, plain, 1).await.unwrap();
    assert!(inst.capabilities.is_empty());
}
