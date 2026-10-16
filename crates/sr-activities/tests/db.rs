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
use sr_activities::model::{ConfigInput, ReviewState, UrlMapping};
use sr_activities::{ActivityError, ActivityEvents, ActivityService, ChannelAccess, ChannelPerms};
use sr_db::{
    PgPool,
    accounts::{self, NewAccount},
    testing::TestDb,
};

static SEQ: AtomicI64 = AtomicI64::new(0);
const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

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
            view: true,
            connect: true,
            use_activities: true,
            manage: false,
        };
        let mut d = HashMap::new();
        d.insert(1, voice);
        d.insert(
            3,
            ChannelPerms {
                voice: false,
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
}

async fn rig() -> Rig {
    let db = TestDb::new().await.unwrap();
    let chans = Arc::new(FakeChannels::new());
    let events = Arc::new(Recorder::default());
    let svc = ActivityService::new(db.pool().clone(), Arc::new(uid))
        .with_channels(chans.clone())
        .with_events(events.clone());
    Rig {
        db,
        svc,
        chans,
        events,
    }
}

fn input(entry: &str) -> ConfigInput {
    ConfigInput {
        entry_url: Some(entry.to_owned()),
        url_mappings: vec![UrlMapping {
            prefix: "/api".into(),
            target: "https://api.example.com".into(),
        }],
    }
}

async fn approved_app(r: &Rig, name: &str) -> i64 {
    let owner = account(r.db.pool()).await;
    let app = application(r.db.pool(), owner, name).await;
    r.svc
        .save_config(app, input("https://a.example.com/"))
        .await
        .unwrap();
    r.svc.submit(app, SHA_A).await.unwrap();
    r.svc.decide(app, 1, true, None).await.unwrap();
    app
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn review_follows_every_change() {
    let r = rig().await;
    let owner = account(r.db.pool()).await;
    let app = application(r.db.pool(), owner, "Chess").await;

    let c = r.svc.config(app).await.unwrap();
    assert_eq!(c.review_state, ReviewState::Draft);
    assert!(c.entry_url.is_none());

    // Bad input is named by field.
    let e = r
        .svc
        .save_config(app, input("http://insecure.example.com/"))
        .await
        .unwrap_err();
    assert!(matches!(e, ActivityError::Invalid("entry_url", _)));
    let mut bad = input("https://a.example.com/");
    bad.url_mappings[0].prefix = "/".into();
    assert!(matches!(
        r.svc.save_config(app, bad).await.unwrap_err(),
        ActivityError::Invalid("url_mappings", _)
    ));

    // Cannot submit without an entry URL, or with a malformed digest.
    assert!(matches!(
        r.svc.submit(app, SHA_A).await.unwrap_err(),
        ActivityError::Conflict(_)
    ));
    r.svc
        .save_config(app, input("https://a.example.com/"))
        .await
        .unwrap();
    assert!(matches!(
        r.svc.submit(app, "nope").await.unwrap_err(),
        ActivityError::Invalid("sha256", _)
    ));

    // Pending, in the staff queue, not yet served.
    let c = r.svc.submit(app, SHA_A).await.unwrap();
    assert_eq!(c.review_state, ReviewState::Pending);
    assert_eq!(c.pinned_sha256.as_deref(), Some(SHA_A));
    assert!(r.svc.approved(app).await.unwrap().is_none());
    let q = r.svc.pending(10).await.unwrap();
    assert_eq!(q.len(), 1);
    assert_eq!(q[0].application_name, "Chess");
    assert_eq!(q[0].owner_id, owner.to_string());

    // Rejected, then resubmitted and approved: only the approved hash is served.
    let c = r
        .svc
        .decide(app, 1, false, Some("  too broad "))
        .await
        .unwrap();
    assert_eq!(c.review_state, ReviewState::Rejected);
    assert_eq!(c.review_note.as_deref(), Some("too broad"));
    assert!(c.reviewed_sha256.is_none());
    assert!(matches!(
        r.svc.decide(app, 1, true, None).await.unwrap_err(),
        ActivityError::Conflict(_)
    ));
    r.svc.submit(app, SHA_B).await.unwrap();
    let c = r.svc.decide(app, 1, true, None).await.unwrap();
    assert_eq!(c.review_state, ReviewState::Approved);
    assert_eq!(c.reviewed_sha256.as_deref(), Some(SHA_B));
    assert_eq!(
        r.svc
            .approved(app)
            .await
            .unwrap()
            .unwrap()
            .reviewed_sha256
            .as_deref(),
        Some(SHA_B)
    );
    assert_eq!(r.svc.shelf().await.unwrap().len(), 1);

    // Saving identical values keeps the approval; a change withdraws it.
    let same = r
        .svc
        .save_config(app, input("https://a.example.com/"))
        .await
        .unwrap();
    assert_eq!(same.review_state, ReviewState::Approved);
    let changed = r
        .svc
        .save_config(app, input("https://a.example.com/v2/"))
        .await
        .unwrap();
    assert_eq!(changed.review_state, ReviewState::Draft);
    assert!(changed.reviewed_sha256.is_none());
    assert!(r.svc.approved(app).await.unwrap().is_none());
    assert!(r.svc.shelf().await.unwrap().is_empty());

    // A suspended application is neither served nor listed.
    r.svc.submit(app, SHA_A).await.unwrap();
    r.svc.decide(app, 1, true, None).await.unwrap();
    sqlx::query("UPDATE applications SET release_state = 'suspended' WHERE id = $1")
        .bind(app)
        .execute(r.db.pool())
        .await
        .unwrap();
    assert!(r.svc.approved(app).await.unwrap().is_none());
    assert!(r.svc.shelf().await.unwrap().is_empty());
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

    // Only approved Activities, only in voice channels, only with the permissions.
    assert!(matches!(
        r.svc.launch(a, draft_app, 1).await.unwrap_err(),
        ActivityError::NotFound("activity")
    ));
    assert!(matches!(
        r.svc.launch(a, app, 3).await.unwrap_err(),
        ActivityError::Invalid("channel_id", _)
    ));
    assert!(matches!(
        r.svc.launch(a, app, 404).await.unwrap_err(),
        ActivityError::NotFound("channel")
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

    let inst = r.svc.launch(a, app, 1).await.unwrap();
    assert_eq!(inst.application_name, "Chess");
    assert_eq!(inst.channel_id, "1");
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
