//! Postgres-backed tests of the feature modules: the SQL of every module, the limits, and the scheduler's dispatch
//! through a fake `Poster`. Run with a scratch Postgres that may CREATE DATABASE:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-userfeat --test db -- --ignored --test-threads=1`

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicI64, Ordering},
};

use async_trait::async_trait;
use serde_json::{Value, json};
use sr_db::{
    PgPool,
    accounts::{self, NewAccount},
    testing::TestDb,
};
use sr_features::{FeatureRegistry, FeatureService, InMemorySettingsStore, ScopeRef};
use sr_net::{HopRequest, HopResponse, HttpTransport, NetError, Policy, Resolver, SafeClient};
use sr_userfeat::announcers::PartyAnnouncement;
use sr_userfeat::birthdays::BirthdayInput;
use sr_userfeat::poll_templates::PollTemplateInput;
use sr_userfeat::reminders::Target;
use sr_userfeat::rss::{RssFeedInput, RssFeedPatch};
use sr_userfeat::suggestions::SuggestionResponse;
use sr_userfeat::tick::Ticker;
use sr_userfeat::{
    Card, Poster, SendFailure, UfError, UserFeatures, features::*, rss, scheduled::now_ms,
};

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

/// A guild with one text channel and the given members.
async fn guild(pool: &PgPool, owner: i64, members: &[i64]) -> (i64, i64) {
    let (g, c) = (uid(), uid());
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'Test guild', $2)")
        .bind(g)
        .bind(owner)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO channels (id, guild_id, type, name) VALUES ($1, $2, 0, 'general')")
        .bind(c)
        .bind(g)
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
    (g, c)
}

async fn channel(pool: &PgPool, guild: i64, name: &str) -> i64 {
    let c = uid();
    sqlx::query("INSERT INTO channels (id, guild_id, type, name) VALUES ($1, $2, 0, $3)")
        .bind(c)
        .bind(guild)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    c
}

#[derive(Default)]
struct Fake {
    next: AtomicI64,
    posts: Mutex<Vec<(i64, Card)>>,
    dms: Mutex<Vec<(i64, Card)>>,
    sent: Mutex<Vec<(i64, i64, String)>>,
    reacts: Mutex<Vec<(i64, i64, String)>>,
    edits: Mutex<Vec<(i64, i64, Card)>>,
    roles: Mutex<Vec<(&'static str, i64, i64, i64)>>,
    send_failure: Mutex<Option<SendFailure>>,
    fail_post: AtomicBool,
}

impl Fake {
    fn id(&self) -> i64 {
        uid() + self.next.fetch_add(1, Ordering::Relaxed)
    }
}

#[async_trait]
impl Poster for Fake {
    async fn post(&self, channel: i64, card: Card) -> Result<i64, String> {
        if self.fail_post.load(Ordering::SeqCst) {
            return Err("channel is gone".into());
        }
        self.posts.lock().unwrap().push((channel, card));
        Ok(self.id())
    }
    async fn dm(&self, user: i64, card: Card) -> Result<(i64, i64), String> {
        self.dms.lock().unwrap().push((user, card));
        Ok((self.id(), self.id()))
    }
    async fn send_as(&self, author: i64, channel: i64, content: &str) -> Result<i64, SendFailure> {
        if let Some(f) = self.send_failure.lock().unwrap().clone() {
            return Err(f);
        }
        self.sent
            .lock()
            .unwrap()
            .push((author, channel, content.to_owned()));
        Ok(self.id())
    }
    async fn react(&self, channel: i64, message: i64, emoji: &str) {
        self.reacts
            .lock()
            .unwrap()
            .push((channel, message, emoji.to_owned()));
    }
    async fn edit(&self, channel: i64, message: i64, card: Card) {
        self.edits.lock().unwrap().push((channel, message, card));
    }
    async fn add_role(&self, guild: i64, user: i64, role: i64) -> bool {
        self.roles.lock().unwrap().push(("add", guild, user, role));
        true
    }
    async fn remove_role(&self, guild: i64, user: i64, role: i64) -> bool {
        self.roles
            .lock()
            .unwrap()
            .push(("remove", guild, user, role));
        true
    }
}

/// Scripted network: one fixed body per URL, public addresses only.
struct Net {
    bodies: Mutex<HashMap<String, Result<Vec<u8>, u16>>>,
}

#[async_trait]
impl HttpTransport for Net {
    async fn get(&self, req: &HopRequest) -> Result<HopResponse, NetError> {
        match self.bodies.lock().unwrap().get(req.url.as_str()) {
            Some(Ok(b)) => Ok(HopResponse {
                status: 200,
                body: b.clone(),
                ..HopResponse::default()
            }),
            Some(Err(status)) => Ok(HopResponse {
                status: *status,
                ..HopResponse::default()
            }),
            None => Err(NetError::Http("no route".into())),
        }
    }
}

struct PublicDns;

#[async_trait]
impl Resolver for PublicDns {
    async fn resolve(&self, _h: &str, _p: u16) -> Result<Vec<IpAddr>, String> {
        Ok(vec!["93.184.216.34".parse().unwrap()])
    }
}

struct Env {
    db: TestDb,
    uf: UserFeatures,
    poster: Arc<Fake>,
    ticker: Ticker,
    net: Arc<Net>,
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
        let ids: sr_userfeat::IdSource = Arc::new(uid);
        let uf = UserFeatures::new(db.pool().clone(), features, ids);
        let poster = Arc::new(Fake::default());
        let net = Arc::new(Net {
            bodies: Mutex::default(),
        });
        let http = Arc::new(SafeClient::new(
            Policy::default(),
            Arc::new(PublicDns),
            net.clone(),
        ));
        let ticker = Ticker::new(uf.clone(), poster.clone(), Some(http));
        Self {
            db,
            uf,
            poster,
            ticker,
            net,
        }
    }

    fn pool(&self) -> &PgPool {
        self.db.pool()
    }

    async fn enable(&self, feature: &str, guild: i64, settings: Value) {
        let scope = ScopeRef::Guild {
            guild: sr_core::GuildId::new(guild as u64),
        };
        self.uf
            .features
            .set_enabled(feature, &scope, Some(true))
            .await
            .unwrap();
        self.uf
            .features
            .set_settings(feature, &scope, settings.as_object().cloned().unwrap())
            .await
            .unwrap();
        // The scheduler finds servers through the table the Postgres settings store writes; mirror it here.
        sqlx::query(
            "INSERT INTO feature_enablements (id, feature, scope, scope_id, enabled, config)              VALUES ($1, $2, 'guild'::enablement_scope, $3, TRUE, '{}')              ON CONFLICT (feature, scope, scope_id) DO UPDATE SET enabled = TRUE",
        )
        .bind(uid())
        .bind(feature)
        .bind(guild)
        .execute(self.pool())
        .await
        .unwrap();
    }

    async fn disable(&self, feature: &str, guild: i64) {
        let scope = ScopeRef::Guild {
            guild: sr_core::GuildId::new(guild as u64),
        };
        self.uf
            .features
            .set_enabled(feature, &scope, Some(false))
            .await
            .unwrap();
    }

    fn set_feed_body(&self, url: &str, body: &str) {
        self.net
            .bodies
            .lock()
            .unwrap()
            .insert(url.to_owned(), Ok(body.as_bytes().to_vec()));
    }
}

fn s(id: i64) -> String {
    id.to_string()
}

// ---------------------------------------------------------------- scheduled messages

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn scheduled_messages_lifecycle_and_dispatch() {
    let e = Env::new().await;
    let user = account(e.pool()).await;
    let (g, c) = guild(e.pool(), user, &[]).await;

    let soon = now_ms() + 120_000;
    let m =
        e.uf.schedule_message(user, c, Some(g), "  see you later  ", soon)
            .await
            .unwrap();
    assert_eq!(m.status, "pending");
    assert_eq!(m.content, "see you later");
    let id: i64 = m.id.parse().unwrap();

    // Too soon and too far are refused.
    assert!(matches!(
        e.uf.schedule_message(user, c, Some(g), "x", now_ms() + 1000)
            .await,
        Err(UfError::Invalid(_))
    ));
    assert!(matches!(
        e.uf.schedule_message(user, c, Some(g), "x", now_ms() + 400 * 86_400_000)
            .await,
        Err(UfError::Invalid(_))
    ));

    // Edit text and time; only the author sees it.
    let edited =
        e.uf.edit_scheduled(user, id, Some("changed"), Some(soon + 60_000))
            .await
            .unwrap();
    assert_eq!(edited.content, "changed");
    let stranger = account(e.pool()).await;
    assert!(matches!(
        e.uf.get_scheduled(stranger, id).await,
        Err(UfError::NotFound(_))
    ));
    assert!(
        e.uf.list_scheduled(stranger, true)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(e.uf.list_scheduled(user, false).await.unwrap().len(), 1);

    // Not yet due: the tick leaves it alone.
    let r = e.ticker.run_once(true).await;
    assert_eq!(r.scheduled_sent, 0);

    // Due: sent as the author.
    sqlx::query(
        "UPDATE scheduled_messages SET send_at = now() - interval '1 second' WHERE id = $1",
    )
    .bind(id)
    .execute(e.pool())
    .await
    .unwrap();
    let r = e.ticker.run_once(true).await;
    assert_eq!(r.scheduled_sent, 1);
    assert_eq!(
        e.poster.sent.lock().unwrap().as_slice(),
        &[(user, c, "changed".to_owned())]
    );
    let done = e.uf.get_scheduled(user, id).await.unwrap();
    assert_eq!(done.status, "sent");
    assert!(done.message_id.is_some());
    // A sent message cannot be edited or cancelled, and is not sent twice.
    assert!(matches!(
        e.uf.edit_scheduled(user, id, Some("again"), None).await,
        Err(UfError::Conflict(_))
    ));
    assert!(matches!(
        e.uf.cancel_scheduled(user, id).await,
        Err(UfError::Conflict(_))
    ));
    e.ticker.run_once(true).await;
    assert_eq!(e.poster.sent.lock().unwrap().len(), 1);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn scheduled_messages_cancel_limits_failures_and_switch() {
    let e = Env::new().await;
    let user = account(e.pool()).await;
    let (g, c) = guild(e.pool(), user, &[]).await;
    e.uf.features
        .set_settings(
            SCHEDULED_ID,
            &ScopeRef::Guild {
                guild: sr_core::GuildId::new(g as u64),
            },
            json!({"max_pending": 2}).as_object().cloned().unwrap(),
        )
        .await
        .unwrap();
    let at = now_ms() + 300_000;
    let a =
        e.uf.schedule_message(user, c, Some(g), "a", at)
            .await
            .unwrap();
    let b =
        e.uf.schedule_message(user, c, Some(g), "b", at)
            .await
            .unwrap();
    assert!(matches!(
        e.uf.schedule_message(user, c, Some(g), "c", at).await,
        Err(UfError::Conflict(_))
    ));
    // Cancelling frees a slot and is idempotent.
    let a_id: i64 = a.id.parse().unwrap();
    e.uf.cancel_scheduled(user, a_id).await.unwrap();
    e.uf.cancel_scheduled(user, a_id).await.unwrap();
    e.uf.schedule_message(user, c, Some(g), "c", at)
        .await
        .unwrap();

    // A permanent failure fails the row and tells the author.
    let b_id: i64 = b.id.parse().unwrap();
    sqlx::query(
        "UPDATE scheduled_messages SET send_at = now() - interval '1 second' WHERE id = $1",
    )
    .bind(b_id)
    .execute(e.pool())
    .await
    .unwrap();
    *e.poster.send_failure.lock().unwrap() = Some(SendFailure::Permanent(
        "You can no longer post here.".into(),
    ));
    let r = e.ticker.run_once(true).await;
    assert_eq!(r.scheduled_failed, 1);
    let failed = e.uf.get_scheduled(user, b_id).await.unwrap();
    assert_eq!(failed.status, "failed");
    assert_eq!(
        failed.failure.as_deref(),
        Some("You can no longer post here.")
    );
    let dms = e.poster.dms.lock().unwrap().clone();
    assert_eq!(dms.len(), 1);
    assert_eq!(dms[0].0, user);

    // A transient failure keeps it pending and pushes it back.
    let d =
        e.uf.schedule_message(user, c, Some(g), "d", now_ms() + 300_000)
            .await
            .unwrap();
    let d_id: i64 = d.id.parse().unwrap();
    sqlx::query(
        "UPDATE scheduled_messages SET send_at = now() - interval '1 second' WHERE id = $1",
    )
    .bind(d_id)
    .execute(e.pool())
    .await
    .unwrap();
    *e.poster.send_failure.lock().unwrap() = Some(SendFailure::Retry("busy".into()));
    e.ticker.run_once(true).await;
    let again = e.uf.get_scheduled(user, d_id).await.unwrap();
    assert_eq!(again.status, "pending");
    let later: bool =
        sqlx::query_scalar("SELECT send_at > now() FROM scheduled_messages WHERE id = $1")
            .bind(d_id)
            .fetch_one(e.pool())
            .await
            .unwrap();
    assert!(later);

    // Switching the module off refuses new ones and fails due ones.
    e.disable(SCHEDULED_ID, g).await;
    assert!(matches!(
        e.uf.schedule_message(user, c, Some(g), "e", now_ms() + 300_000)
            .await,
        Err(UfError::Disabled(_))
    ));
    sqlx::query(
        "UPDATE scheduled_messages SET send_at = now() - interval '1 second' WHERE id = $1",
    )
    .bind(d_id)
    .execute(e.pool())
    .await
    .unwrap();
    e.ticker.run_once(true).await;
    assert_eq!(
        e.uf.get_scheduled(user, d_id).await.unwrap().status,
        "failed"
    );
}

// ---------------------------------------------------------------- reminders

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn reminders_are_listed_cancelled_and_delivered_as_dms() {
    let e = Env::new().await;
    let user = account(e.pool()).await;
    let (g, c) = guild(e.pool(), user, &[]).await;

    assert!(matches!(
        e.uf.create_reminder(user, "x", now_ms() + 1000, Target::default())
            .await,
        Err(UfError::Invalid(_))
    ));
    let about_message =
        e.uf.create_reminder(
            user,
            "reply to this",
            now_ms() + 60_000,
            Target {
                channel: Some(c),
                guild: Some(g),
                message: Some(4242),
            },
        )
        .await
        .unwrap();
    let plain =
        e.uf.create_reminder(user, "", now_ms() + 90_000, Target::default())
            .await
            .unwrap();
    let cancelled =
        e.uf.create_reminder(user, "never", now_ms() + 90_000, Target::default())
            .await
            .unwrap();
    e.uf.cancel_reminder(user, cancelled.id.parse().unwrap())
        .await
        .unwrap();
    assert!(matches!(
        e.uf.cancel_reminder(user, 12345).await,
        Err(UfError::NotFound(_))
    ));
    let list = e.uf.list_reminders(user, false).await.unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].id, about_message.id, "soonest first");

    sqlx::query(
        "UPDATE reminders SET remind_at = now() - interval '1 second' WHERE status = 'pending'",
    )
    .execute(e.pool())
    .await
    .unwrap();
    let r = e.ticker.run_once(true).await;
    assert_eq!(r.reminders_sent, 2);
    let dms = e.poster.dms.lock().unwrap().clone();
    assert_eq!(dms.len(), 2);
    assert!(dms.iter().all(|(to, _)| *to == user));
    let linked = dms
        .iter()
        .find(|(_, card)| card.description.as_deref() == Some("reply to this"))
        .unwrap();
    assert_eq!(linked.1.url, Some(format!("/channels/{g}/{c}/4242")));
    let bare = dms.iter().find(|(_, card)| card.url.is_none()).unwrap();
    assert_eq!(
        bare.1.description.as_deref(),
        Some("You asked to be reminded.")
    );
    assert_eq!(e.uf.list_reminders(user, false).await.unwrap().len(), 0);
    let all = e.uf.list_reminders(user, true).await.unwrap();
    assert!(all.iter().any(|r| r.id == plain.id && r.status == "sent"));
    e.ticker.run_once(true).await;
    assert_eq!(e.poster.dms.lock().unwrap().len(), 2, "delivered once");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn reminders_respect_the_pending_limit() {
    let e = Env::new().await;
    let user = account(e.pool()).await;
    e.uf.features
        .set_settings(
            REMINDERS_ID,
            &ScopeRef::Platform,
            json!({"max_pending": 1}).as_object().cloned().unwrap(),
        )
        .await
        .unwrap();
    e.uf.create_reminder(user, "one", now_ms() + 60_000, Target::default())
        .await
        .unwrap();
    assert!(matches!(
        e.uf.create_reminder(user, "two", now_ms() + 60_000, Target::default())
            .await,
        Err(UfError::Conflict(_))
    ));
}

// ---------------------------------------------------------------- bookmarks

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn bookmarks_are_private_updatable_and_limited() {
    let e = Env::new().await;
    let user = account(e.pool()).await;
    let other = account(e.pool()).await;
    let (g, c) = guild(e.pool(), user, &[other]).await;

    let b =
        e.uf.save_bookmark(user, 100, c, Some(g), " later ")
            .await
            .unwrap();
    assert_eq!(b.note, "later");
    let b =
        e.uf.save_bookmark(user, 100, c, Some(g), "changed")
            .await
            .unwrap();
    assert_eq!(b.note, "changed");
    e.uf.save_bookmark(user, 200, c, Some(g), "").await.unwrap();
    let list = e.uf.list_bookmarks(user, None, 50).await.unwrap();
    assert_eq!(
        list.iter()
            .map(|b| b.message_id.as_str())
            .collect::<Vec<_>>(),
        ["200", "100"],
        "newest message first"
    );
    let page = e.uf.list_bookmarks(user, Some(200), 50).await.unwrap();
    assert_eq!(page.len(), 1);
    assert!(
        e.uf.list_bookmarks(other, None, 50)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        e.uf.get_bookmark(other, 100).await,
        Err(UfError::NotFound(_))
    ));

    e.uf.remove_bookmark(user, 100).await.unwrap();
    e.uf.remove_bookmark(user, 100).await.unwrap();
    assert_eq!(e.uf.list_bookmarks(user, None, 50).await.unwrap().len(), 1);

    // The limit counts new bookmarks only.
    e.uf.features
        .set_settings(
            BOOKMARKS_ID,
            &ScopeRef::Platform,
            json!({"max_per_user": 10}).as_object().cloned().unwrap(),
        )
        .await
        .unwrap();
    for m in 300..309 {
        e.uf.save_bookmark(user, m, c, Some(g), "").await.unwrap();
    }
    assert!(matches!(
        e.uf.save_bookmark(user, 999, c, Some(g), "").await,
        Err(UfError::Conflict(_))
    ));
    e.uf.save_bookmark(user, 300, c, Some(g), "still ok")
        .await
        .unwrap();
    assert!(matches!(
        e.uf.save_bookmark(user, 1, c, Some(g), &"x".repeat(501))
            .await,
        Err(UfError::Invalid(_))
    ));
}

// ---------------------------------------------------------------- birthdays

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn birthdays_are_announced_once_with_a_role_that_expires() {
    let e = Env::new().await;
    let owner = account(e.pool()).await;
    let a = account(e.pool()).await;
    let hidden = account(e.pool()).await;
    let outsider = account(e.pool()).await;
    let (g, _c) = guild(e.pool(), owner, &[a, hidden]).await;
    let announce = channel(e.pool(), g, "birthdays").await;
    let role = uid();
    sqlx::query("INSERT INTO roles (id, guild_id, name) VALUES ($1, $2, 'Birthday')")
        .bind(role)
        .bind(g)
        .execute(e.pool())
        .await
        .unwrap();
    e.enable(
        BIRTHDAYS_ID,
        g,
        json!({"channel": s(announce), "role": s(role), "message": "Happy birthday {user} ({name}), {age}!"}),
    )
    .await;

    let (month, day, year): (i32, i32, i32) = sqlx::query_as(
        "SELECT extract(month FROM now() AT TIME ZONE 'UTC')::int, \
                extract(day FROM now() AT TIME ZONE 'UTC')::int, \
                extract(year FROM now() AT TIME ZONE 'UTC')::int",
    )
    .fetch_one(e.pool())
    .await
    .unwrap();
    let today = |year: Option<i16>, visibility: &str| BirthdayInput {
        month: month as i16,
        day: day as i16,
        year,
        visibility: Some(visibility.into()),
    };
    // 29 February is only valid in a leap year; skip the year then.
    let born = (year - 30) as i16;
    e.uf.set_birthday(a, &today(Some(born), "servers"))
        .await
        .unwrap();
    e.uf.set_birthday(hidden, &today(None, "hidden"))
        .await
        .unwrap();
    e.uf.set_birthday(outsider, &today(None, "servers"))
        .await
        .unwrap();
    assert_eq!(e.uf.get_birthday(a).await.unwrap().year, Some(born));

    let upcoming = e.uf.upcoming_birthdays(g, 10).await.unwrap();
    assert_eq!(
        upcoming
            .iter()
            .map(|u| u.user_id.clone())
            .collect::<Vec<_>>(),
        vec![s(a)],
        "hidden birthdays and non-members are not listed"
    );

    let r = e.ticker.run_once(true).await;
    assert_eq!(r.birthdays, 1);
    {
        let posts = e.poster.posts.lock().unwrap().clone();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].0, announce);
        assert_eq!(
            posts[0].1.content,
            format!("Happy birthday <@{a}> (t{a}), 30!")
        );
        assert_eq!(posts[0].1.ping_users, vec![a]);
    }
    assert_eq!(
        e.poster.roles.lock().unwrap().as_slice(),
        &[("add", g, a, role)]
    );
    // Once a day only.
    e.ticker.run_once(true).await;
    assert_eq!(e.poster.posts.lock().unwrap().len(), 1);

    // The role comes off after 24 hours.
    sqlx::query("UPDATE birthday_announcements SET role_until = now() - interval '1 minute'")
        .execute(e.pool())
        .await
        .unwrap();
    let r = e.ticker.run_once(true).await;
    assert_eq!(r.roles_removed, 1);
    assert!(
        e.poster
            .roles
            .lock()
            .unwrap()
            .contains(&("remove", g, a, role))
    );
    e.ticker.run_once(true).await;
    assert_eq!(e.poster.roles.lock().unwrap().len(), 2);

    // Clearing removes the birthday.
    e.uf.clear_birthday(a).await.unwrap();
    assert!(matches!(
        e.uf.get_birthday(a).await,
        Err(UfError::NotFound(_))
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_failed_birthday_post_is_retried() {
    let e = Env::new().await;
    let owner = account(e.pool()).await;
    let (g, _c) = guild(e.pool(), owner, &[]).await;
    let announce = channel(e.pool(), g, "birthdays").await;
    e.enable(BIRTHDAYS_ID, g, json!({"channel": s(announce)}))
        .await;
    let (month, day): (i32, i32) = sqlx::query_as(
        "SELECT extract(month FROM now() AT TIME ZONE 'UTC')::int, extract(day FROM now() AT TIME ZONE 'UTC')::int",
    )
    .fetch_one(e.pool())
    .await
    .unwrap();
    e.uf.set_birthday(
        owner,
        &BirthdayInput {
            month: month as i16,
            day: day as i16,
            year: None,
            visibility: None,
        },
    )
    .await
    .unwrap();
    e.poster.fail_post.store(true, Ordering::SeqCst);
    assert_eq!(e.ticker.run_once(true).await.birthdays, 0);
    e.poster.fail_post.store(false, Ordering::SeqCst);
    assert_eq!(e.ticker.run_once(true).await.birthdays, 1);
    assert_eq!(
        e.poster.posts.lock().unwrap()[0].1.content,
        format!("Happy birthday, <@{owner}>!")
    );
}

// ---------------------------------------------------------------- leveling

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn leveling_awards_with_cooldown_levels_up_and_ranks() {
    let e = Env::new().await;
    let a = account(e.pool()).await;
    let b = account(e.pool()).await;
    let (g, _c) = guild(e.pool(), a, &[b]).await;
    e.enable(LEVELING_ID, g, json!({})).await;

    assert_eq!(e.uf.award_xp(g, a, 20, 60).await.unwrap(), None);
    // Cooldown: the second message in a minute earns nothing.
    assert_eq!(e.uf.award_xp(g, a, 20, 60).await.unwrap(), None);
    assert_eq!(e.uf.rank_card(g, a).await.unwrap().xp, 20);
    // No cooldown: 100 XP crosses level 1 (needs 100).
    let up = e.uf.award_xp(g, a, 100, 0).await.unwrap().unwrap();
    assert_eq!((up.old, up.new, up.xp), (0, 1, 120));
    assert_eq!(
        e.uf.award_xp(g, a, 10, 0).await.unwrap(),
        None,
        "same level"
    );
    e.uf.award_xp(g, b, 500, 0).await.unwrap();

    let card = e.uf.rank_card(g, a).await.unwrap();
    assert_eq!(
        (card.level, card.xp, card.level_xp, card.level_span),
        (1, 130, 30, 155)
    );
    assert_eq!(card.rank, Some(2));
    let top = e.uf.rank_card(g, b).await.unwrap();
    assert_eq!((top.rank, top.level), (Some(1), 3));

    let board = e.uf.leaderboard(g, 10, 0).await.unwrap();
    assert_eq!(
        board
            .iter()
            .map(|r| (r.rank, r.user_id.clone()))
            .collect::<Vec<_>>(),
        vec![(1, s(b)), (2, s(a))]
    );
    let second_page = e.uf.leaderboard(g, 1, 1).await.unwrap();
    assert_eq!(second_page.len(), 1);
    assert_eq!(second_page[0].rank, 2);

    // Opting out deletes the XP and earns nothing more; opting back in starts from zero.
    e.uf.set_xp_opt_out(g, a, true).await.unwrap();
    assert_eq!(e.uf.award_xp(g, a, 50, 0).await.unwrap(), None);
    let out = e.uf.rank_card(g, a).await.unwrap();
    assert!(out.opted_out);
    assert_eq!((out.xp, out.rank), (0, None));
    e.uf.set_xp_opt_out(g, a, false).await.unwrap();
    e.uf.award_xp(g, a, 50, 0).await.unwrap();
    assert_eq!(e.uf.rank_card(g, a).await.unwrap().xp, 50);

    // Off by default elsewhere.
    let (g2, _) = guild(e.pool(), a, &[]).await;
    assert!(matches!(
        e.uf.rank_card(g2, a).await,
        Err(UfError::Disabled(_))
    ));
    let stranger = account(e.pool()).await;
    assert!(matches!(
        e.uf.rank_card(g, stranger).await,
        Err(UfError::NotFound(_))
    ));
}

// ---------------------------------------------------------------- RSS

const FEED_URL: &str = "https://feeds.example.test/news.xml";

fn feed_xml(items: &[(&str, &str)]) -> String {
    let body: String = items
        .iter()
        .map(|(id, title)| {
            format!(
                "<item><title>{title}</title><link>https://example.test/{id}</link><guid>{id}</guid>\
                 <description>About {title}</description></item>"
            )
        })
        .collect();
    format!(
        "<?xml version=\"1.0\"?><rss version=\"2.0\"><channel><title>News</title>{body}</channel></rss>"
    )
}

async fn add_feed(
    e: &Env,
    g: i64,
    c: i64,
    by: i64,
    input: RssFeedInput,
) -> sr_userfeat::rss::RssFeed {
    let url = rss::check_feed_url(&Policy::default(), &input.url).unwrap();
    e.uf.add_feed(g, c, by, &url, &input).await.unwrap()
}

async fn due_now(e: &Env) {
    sqlx::query("UPDATE rss_feeds SET next_check_at = now() - interval '1 second'")
        .execute(e.pool())
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn rss_seeds_first_then_posts_only_new_entries_through_the_safe_client() {
    let e = Env::new().await;
    let owner = account(e.pool()).await;
    let (g, c) = guild(e.pool(), owner, &[]).await;
    e.enable(RSS_ID, g, json!({})).await;
    e.set_feed_body(FEED_URL, &feed_xml(&[("a", "Old one"), ("b", "Old two")]));
    let feed = add_feed(
        &e,
        g,
        c,
        owner,
        RssFeedInput {
            url: FEED_URL.into(),
            channel_id: s(c),
            include_keywords: vec![],
            exclude_keywords: vec!["spoiler".into()],
            poll_interval_secs: Some(600),
        },
    )
    .await;
    assert_eq!(feed.last_status, "pending");

    // First check: nothing is posted, everything is remembered.
    let r = e.ticker.run_once(true).await;
    assert_eq!(r.feeds_checked, 1);
    assert_eq!(r.feed_posts, 0);
    let listed = e.uf.list_feeds(g).await.unwrap();
    assert_eq!(listed[0].last_status, "ok");
    assert_eq!(listed[0].title, "News");

    // Not due again yet.
    assert_eq!(e.ticker.run_once(true).await.feeds_checked, 0);

    // Two new entries (one filtered out) appear.
    e.set_feed_body(
        FEED_URL,
        &feed_xml(&[
            ("d", "Spoiler alert"),
            ("c", "Fresh news"),
            ("a", "Old one"),
            ("b", "Old two"),
        ]),
    );
    due_now(&e).await;
    let r = e.ticker.run_once(true).await;
    assert_eq!(r.feed_posts, 1);
    {
        let posts = e.poster.posts.lock().unwrap().clone();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].0, c);
        assert_eq!(posts[0].1.title.as_deref(), Some("Fresh news"));
        assert_eq!(posts[0].1.url.as_deref(), Some("https://example.test/c"));
        assert_eq!(posts[0].1.footer.as_deref(), Some("News"));
    }
    // The filtered entry is remembered too, so it is not reconsidered.
    due_now(&e).await;
    assert_eq!(e.ticker.run_once(true).await.feed_posts, 0);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn rss_failures_back_off_and_unsafe_urls_are_refused() {
    let e = Env::new().await;
    let owner = account(e.pool()).await;
    let (g, c) = guild(e.pool(), owner, &[]).await;
    e.enable(RSS_ID, g, json!({"max_feeds": 1})).await;

    for bad in [
        "http://127.0.0.1/feed",
        "file:///etc/passwd",
        "http://user:pw@example.test/feed",
        "https://10.0.0.5/feed",
    ] {
        assert!(
            rss::check_feed_url(&Policy::default(), bad).is_err(),
            "{bad} must be refused"
        );
    }
    // A feed whose host resolves to a private address is stopped at fetch time by the client.
    struct PrivateDns;
    #[async_trait]
    impl Resolver for PrivateDns {
        async fn resolve(&self, _h: &str, _p: u16) -> Result<Vec<IpAddr>, String> {
            Ok(vec!["10.0.0.7".parse().unwrap()])
        }
    }
    let evil = SafeClient::new(Policy::default(), Arc::new(PrivateDns), e.net.clone());
    let url = url::Url::parse(FEED_URL).unwrap();
    assert!(matches!(
        rss::fetch_feed(&evil, &url).await,
        Err(rss::FeedError::Net(NetError::Blocked(_)))
    ));

    let feed = add_feed(
        &e,
        g,
        c,
        owner,
        RssFeedInput {
            url: FEED_URL.into(),
            channel_id: s(c),
            include_keywords: vec![],
            exclude_keywords: vec![],
            poll_interval_secs: None,
        },
    )
    .await;
    let id: i64 = feed.id.parse().unwrap();
    // Limit and duplicates.
    let url = rss::check_feed_url(&Policy::default(), "https://other.example.test/f").unwrap();
    assert!(matches!(
        e.uf.add_feed(
            g,
            c,
            owner,
            &url,
            &RssFeedInput {
                url: url.to_string(),
                channel_id: s(c),
                include_keywords: vec![],
                exclude_keywords: vec![],
                poll_interval_secs: None
            }
        )
        .await,
        Err(UfError::Conflict(_))
    ));

    // The server answers 500: the failure is recorded and the next check backs off.
    e.net
        .bodies
        .lock()
        .unwrap()
        .insert(FEED_URL.to_owned(), Err(500));
    e.ticker.run_once(true).await;
    let after = e.uf.list_feeds(g).await.unwrap().remove(0);
    assert_eq!(after.failures, 1);
    assert!(after.last_status.contains("500"), "{}", after.last_status);
    let delay: f64 = sqlx::query_scalar(
        "SELECT extract(epoch FROM next_check_at - now())::float8 FROM rss_feeds WHERE id = $1",
    )
    .bind(id)
    .fetch_one(e.pool())
    .await
    .unwrap();
    assert!(
        delay > 3000.0 && delay <= 3600.0,
        "doubled from 30 minutes: {delay}"
    );

    // Not a feed at all.
    e.set_feed_body(FEED_URL, "<html><body>hello</body></html>");
    due_now(&e).await;
    e.ticker.run_once(true).await;
    assert_eq!(e.uf.list_feeds(g).await.unwrap()[0].failures, 2);

    // Re-enabling resets the failures; disabling stops checks.
    let patched =
        e.uf.patch_feed(
            g,
            id,
            None,
            &RssFeedPatch {
                enabled: Some(true),
                ..RssFeedPatch::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(patched.failures, 0);
    e.uf.patch_feed(
        g,
        id,
        None,
        &RssFeedPatch {
            enabled: Some(false),
            ..RssFeedPatch::default()
        },
    )
    .await
    .unwrap();
    due_now(&e).await;
    assert_eq!(e.ticker.run_once(true).await.feeds_checked, 0);
    e.uf.delete_feed(g, id).await.unwrap();
    assert!(matches!(
        e.uf.delete_feed(g, id).await,
        Err(UfError::NotFound(_))
    ));
}

// ---------------------------------------------------------------- poll templates

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn poll_templates_crud_uniqueness_and_limit() {
    let e = Env::new().await;
    let owner = account(e.pool()).await;
    let (g, _c) = guild(e.pool(), owner, &[]).await;
    let input = |name: &str| PollTemplateInput {
        name: name.into(),
        question: "What do we watch?".into(),
        answers: vec!["Akira".into(), "Paprika".into()],
        allow_multiselect: true,
        duration_hours: Some(48),
    };
    let t =
        e.uf.create_poll_template(g, owner, &input("Movie night"))
            .await
            .unwrap();
    assert_eq!(t.duration_hours, 48);
    assert!(t.allow_multiselect);
    assert_eq!(t.answers, vec!["Akira", "Paprika"]);
    assert!(matches!(
        e.uf.create_poll_template(g, owner, &input("Movie night"))
            .await,
        Err(UfError::Conflict(_))
    ));
    let id: i64 = t.id.parse().unwrap();
    let mut changed = input("Movie night 2");
    changed.answers.push("Perfect Blue".into());
    let t2 = e.uf.update_poll_template(g, id, &changed).await.unwrap();
    assert_eq!(t2.name, "Movie night 2");
    assert_eq!(t2.answers.len(), 3);
    assert_eq!(e.uf.list_poll_templates(g).await.unwrap().len(), 1);
    assert_eq!(e.uf.get_poll_template(g, id).await.unwrap().id, t.id);
    // Another server cannot see or change it.
    let (g2, _) = guild(e.pool(), owner, &[]).await;
    assert!(e.uf.list_poll_templates(g2).await.unwrap().is_empty());
    assert!(matches!(
        e.uf.update_poll_template(g2, id, &changed).await,
        Err(UfError::NotFound(_))
    ));

    e.uf.features
        .set_settings(
            POLL_TEMPLATES_ID,
            &ScopeRef::Guild {
                guild: sr_core::GuildId::new(g as u64),
            },
            json!({"max_templates": 1}).as_object().cloned().unwrap(),
        )
        .await
        .unwrap();
    assert!(matches!(
        e.uf.create_poll_template(g, owner, &input("Another")).await,
        Err(UfError::Conflict(_))
    ));
    e.uf.delete_poll_template(g, id).await.unwrap();
    assert!(matches!(
        e.uf.delete_poll_template(g, id).await,
        Err(UfError::NotFound(_))
    ));
    e.uf.create_poll_template(g, owner, &input("Another"))
        .await
        .unwrap();
}

// ---------------------------------------------------------------- suggestions and tickets

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn suggestions_are_posted_with_votes_and_updated_by_staff() {
    let e = Env::new().await;
    let owner = account(e.pool()).await;
    let member = account(e.pool()).await;
    let (g, _c) = guild(e.pool(), owner, &[member]).await;
    // Off until switched on, and needs a channel.
    assert!(matches!(
        e.uf.submit_suggestion(&*e.poster, g, member, "More emoji")
            .await,
        Err(UfError::Disabled(_))
    ));
    e.enable(SUGGESTIONS_ID, g, json!({})).await;
    assert!(matches!(
        e.uf.submit_suggestion(&*e.poster, g, member, "More emoji")
            .await,
        Err(UfError::Conflict(_))
    ));
    let ch = channel(e.pool(), g, "suggestions").await;
    e.enable(SUGGESTIONS_ID, g, json!({"channel": s(ch)})).await;

    let sug =
        e.uf.submit_suggestion(&*e.poster, g, member, "  More emoji  ")
            .await
            .unwrap();
    assert_eq!(sug.status, "open");
    assert_eq!(sug.content, "More emoji");
    assert_ne!(sug.message_id, "0");
    {
        let posts = e.poster.posts.lock().unwrap().clone();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].0, ch);
        assert!(posts[0].1.content.contains(&format!("<@{member}>")));
        let reacts = e.poster.reacts.lock().unwrap();
        assert_eq!(reacts.len(), 2);
        assert_eq!(reacts[0].2, "\u{1F44D}");
        assert_eq!(reacts[1].2, "\u{1F44E}");
    }

    let id: i64 = sug.id.parse().unwrap();
    assert!(matches!(
        e.uf.respond_to_suggestion(
            &*e.poster,
            g,
            id,
            owner,
            &SuggestionResponse {
                status: "maybe".into(),
                response: None
            }
        )
        .await,
        Err(UfError::Invalid(_))
    ));
    let done =
        e.uf.respond_to_suggestion(
            &*e.poster,
            g,
            id,
            owner,
            &SuggestionResponse {
                status: "approved".into(),
                response: Some("Coming soon".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(done.status, "approved");
    assert_eq!(done.response, "Coming soon");
    assert_eq!(done.responder_id, Some(s(owner)));
    {
        let edits = e.poster.edits.lock().unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].0, ch);
        assert_eq!(edits[0].1.to_string(), done.message_id);
        assert_eq!(edits[0].2.color, 0x2ECC71);
    }
    assert_eq!(
        e.uf.list_suggestions(g, Some("approved"), None, 10)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        e.uf.list_suggestions(g, Some("open"), None, 10)
            .await
            .unwrap()
            .is_empty()
    );

    // A channel that cannot be posted to leaves no orphan row.
    e.poster.fail_post.store(true, Ordering::SeqCst);
    assert!(matches!(
        e.uf.submit_suggestion(&*e.poster, g, member, "Another")
            .await,
        Err(UfError::Conflict(_))
    ));
    assert_eq!(
        e.uf.list_suggestions(g, None, None, 10)
            .await
            .unwrap()
            .len(),
        1
    );
    e.uf.delete_suggestion(g, id).await.unwrap();
    assert!(
        e.uf.list_suggestions(g, None, None, 10)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn tickets_are_limited_per_member_and_closed_once() {
    let e = Env::new().await;
    let owner = account(e.pool()).await;
    let member = account(e.pool()).await;
    let (g, _c) = guild(e.pool(), owner, &[member]).await;
    assert!(matches!(
        e.uf.check_can_open_ticket(g, member, "help").await,
        Err(UfError::Disabled(_))
    ));
    e.enable(TICKETS_ID, g, json!({})).await;
    assert!(matches!(
        e.uf.check_can_open_ticket(g, member, "help").await,
        Err(UfError::Conflict(_))
    ));
    let ch = channel(e.pool(), g, "tickets").await;
    e.enable(
        TICKETS_ID,
        g,
        json!({"channel": s(ch), "max_open_per_user": 1}),
    )
    .await;

    let subject =
        e.uf.check_can_open_ticket(g, member, "  need   help ")
            .await
            .unwrap();
    assert_eq!(subject, "need help");
    let t =
        e.uf.record_ticket(g, member, ch + 1, &subject)
            .await
            .unwrap();
    assert_eq!(t.status, "open");
    assert!(matches!(
        e.uf.check_can_open_ticket(g, member, "again").await,
        Err(UfError::Conflict(_))
    ));
    assert!(
        e.uf.check_can_open_ticket(g, owner, "other person")
            .await
            .is_ok()
    );
    assert_eq!(
        e.uf.list_tickets(g, Some(member), Some("open"), 10)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        e.uf.list_tickets(g, Some(owner), None, 10)
            .await
            .unwrap()
            .is_empty()
    );

    let id: i64 = t.id.parse().unwrap();
    let closed = e.uf.close_ticket(g, id, owner).await.unwrap();
    assert_eq!(closed.status, "closed");
    assert_eq!(closed.closed_by, Some(s(owner)));
    // Closing again keeps the first closer.
    let again = e.uf.close_ticket(g, id, member).await.unwrap();
    assert_eq!(again.closed_by, Some(s(owner)));
    // The member can open another now.
    assert!(e.uf.check_can_open_ticket(g, member, "next").await.is_ok());
    assert!(matches!(
        e.uf.close_ticket(g, 1, owner).await,
        Err(UfError::NotFound(_))
    ));
}

// ---------------------------------------------------------------- announcers

async fn add_series(pool: &PgPool, kind: &str, genre: &str, restricted: bool) -> (i64, i64) {
    let (series, edition) = (uid(), uid());
    sqlx::query(
        "INSERT INTO reading_series (id, kind, title, genres, age_restricted, published, published_at) \
         VALUES ($1, $2, $3, ARRAY[$4], $5, TRUE, now())",
    )
    .bind(series)
    .bind(kind)
    .bind(format!("Series {series}"))
    .bind(genre)
    .bind(restricted)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO reading_editions (id, series_id, language, label, is_default) VALUES ($1, $2, 'en', 'Official', TRUE)")
        .bind(edition)
        .bind(series)
        .execute(pool)
        .await
        .unwrap();
    (series, edition)
}

async fn add_chapter(pool: &PgPool, series: i64, edition: i64, number: i32) -> i64 {
    let id = uid();
    sqlx::query(
        "INSERT INTO reading_chapters (id, series_id, edition_id, number, title, published, published_at) \
         VALUES ($1, $2, $3, $4, 'The one', TRUE, now())",
    )
    .bind(id)
    .bind(series)
    .bind(edition)
    .bind(number)
    .execute(pool)
    .await
    .unwrap();
    id
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn library_feed_posts_new_chapters_once_and_respects_filters() {
    let e = Env::new().await;
    let owner = account(e.pool()).await;
    let (g, _c) = guild(e.pool(), owner, &[]).await;
    let feed_channel = channel(e.pool(), g, "library").await;
    let role = uid();
    // Old chapter published before the feed is switched on: never replayed.
    let (old_series, old_edition) = add_series(e.pool(), "manga", "Action", false).await;
    add_chapter(e.pool(), old_series, old_edition, 1).await;

    e.enable(
        LIBRARY_FEED_ID,
        g,
        json!({"channel": s(feed_channel), "ping_role": s(role), "kinds": ["manga"], "genres": ["action"]}),
    )
    .await;
    // First pass only sets the cursor.
    assert_eq!(e.ticker.run_once(true).await.library_posts, 0);

    let (series, edition) = add_series(e.pool(), "manga", "Action", false).await;
    let chapter = add_chapter(e.pool(), series, edition, 12).await;
    // Filtered out: wrong kind, wrong genre, age-restricted.
    let (s2, e2) = add_series(e.pool(), "comic", "Action", false).await;
    add_chapter(e.pool(), s2, e2, 1).await;
    let (s3, e3) = add_series(e.pool(), "manga", "Romance", false).await;
    add_chapter(e.pool(), s3, e3, 1).await;
    let (s4, e4) = add_series(e.pool(), "manga", "Action", true).await;
    add_chapter(e.pool(), s4, e4, 1).await;

    let r = e.ticker.run_once(true).await;
    assert_eq!(r.library_posts, 1);
    {
        let posts = e.poster.posts.lock().unwrap().clone();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].0, feed_channel);
        assert!(posts[0].1.title.as_deref().unwrap().ends_with("chapter 12"));
        assert_eq!(posts[0].1.url, Some(format!("/library/r/{chapter}")));
        assert_eq!(posts[0].1.content, format!("<@&{role}>"));
        assert_eq!(posts[0].1.ping_roles, vec![role]);
    }
    assert_eq!(
        e.ticker.run_once(true).await.library_posts,
        0,
        "posted once"
    );

    // An unpublished chapter is invisible until it is published (published_at moves past the cursor).
    let (s5, e5) = add_series(e.pool(), "manga", "Action", false).await;
    let hidden = uid();
    sqlx::query("INSERT INTO reading_chapters (id, series_id, edition_id, number, published) VALUES ($1, $2, $3, 1, FALSE)")
        .bind(hidden)
        .bind(s5)
        .bind(e5)
        .execute(e.pool())
        .await
        .unwrap();
    assert_eq!(e.ticker.run_once(true).await.library_posts, 0);
    sqlx::query("UPDATE reading_chapters SET published = TRUE, published_at = now() WHERE id = $1")
        .bind(hidden)
        .execute(e.pool())
        .await
        .unwrap();
    assert_eq!(e.ticker.run_once(true).await.library_posts, 1);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn watch_party_announcer_posts_when_enabled_with_a_role_ping() {
    let e = Env::new().await;
    let host = account(e.pool()).await;
    let (g, c) = guild(e.pool(), host, &[]).await;
    let party = PartyAnnouncement {
        party_id: 77,
        channel_id: c,
        host_id: host,
        title: "Akira".into(),
        episode_label: None,
        viewers: 4,
        started: true,
    };
    // Off by default.
    assert!(!e.uf.announce_party(&*e.poster, &party).await.unwrap());
    let role = uid();
    e.enable(WATCH_PARTY_ANNOUNCER_ID, g, json!({"ping_role": s(role)}))
        .await;
    assert!(e.uf.announce_party(&*e.poster, &party).await.unwrap());
    {
        let posts = e.poster.posts.lock().unwrap().clone();
        assert_eq!(posts[0].0, c, "defaults to the party's channel");
        assert!(posts[0].1.content.starts_with(&format!("<@&{role}>")));
        assert_eq!(posts[0].1.url.as_deref(), Some("/cinema/party/77"));
    }
    // The end is silent unless asked for, and never pings.
    let ended = PartyAnnouncement {
        started: false,
        ..party.clone()
    };
    assert!(!e.uf.announce_party(&*e.poster, &ended).await.unwrap());
    let other = channel(e.pool(), g, "parties").await;
    e.enable(
        WATCH_PARTY_ANNOUNCER_ID,
        g,
        json!({"channel": s(other), "announce_end": true, "ping_role": s(role)}),
    )
    .await;
    assert!(e.uf.announce_party(&*e.poster, &ended).await.unwrap());
    let posts = e.poster.posts.lock().unwrap().clone();
    assert_eq!(posts[1].0, other);
    assert!(posts[1].1.ping_roles.is_empty());
    // A DM channel has no server and no announcement.
    let dm = uid();
    sqlx::query("INSERT INTO channels (id, type) VALUES ($1, 1)")
        .bind(dm)
        .execute(e.pool())
        .await
        .unwrap();
    assert!(
        !e.uf
            .announce_party(
                &*e.poster,
                &PartyAnnouncement {
                    channel_id: dm,
                    ..party
                }
            )
            .await
            .unwrap()
    );
}
