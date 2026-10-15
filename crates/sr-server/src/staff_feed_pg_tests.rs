//! The Staff Server's system user against real Postgres (run with `SR_TEST_DATABASE_URL`,
//! `--ignored`, `--test-threads=1`): cards in the staff channels, the appointment DM, the
//! alert store and the events that feed them.

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use serde_json::Value;
use sqlx::{PgPool, Row};
use sr_admin::{AdminRepository, AdminService, OfferState, SYSTEM_USER_ID, StaffExtrasRepository};
use sr_api::routes::{
    auth::mail::{Mailer, MemoryMailer},
    messages::{FixedEntitlements, MessagesDeps},
};
use sr_chat::{ChatLimits, ChatService};
use sr_core::{ChannelId, Clock, GuildId, SnowflakeGenerator, SystemClock, UserId};
use sr_db::{admin_repo::PgAdminRepo, chat_repo::PgChatRepo, testing::TestDb};
use sr_events::{DomainEvent, EventBus, InProcessBus, StaffAlertKind, StaffFeedKind, Topic};
use sr_storage::ObjectStoreStorage;

use crate::staff_feed::{
    MailThrottle, StaffFeed, StaffVoice, alert_card, feed_card, offer_card, spawn,
};

type Ids = SnowflakeGenerator<Arc<dyn Clock>>;

async fn account(pool: &PgPool, id: i64, name: &str) {
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $1, $2, $3, 'x')",
    )
    .bind(id)
    .bind(name)
    .bind(format!("{name}@example.test"))
    .execute(pool)
    .await
    .unwrap();
}

struct Fx {
    db: TestDb,
    pool: PgPool,
    bus: Arc<InProcessBus>,
    voice: Arc<StaffVoice>,
    admin: AdminService,
    repo: Arc<PgAdminRepo>,
    owner: i64,
    guild: GuildId,
    channels: std::collections::HashMap<String, ChannelId>,
}

async fn fx() -> Fx {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let ids: Ids = SnowflakeGenerator::new(clock.clone(), 21, 21).unwrap();

    // The system user and the Owner.
    account(&pool, SYSTEM_USER_ID, "sideroom").await;
    let owner = ids.generate().get() as i64;
    account(&pool, owner, "owner").await;

    // A real guild from the blueprint (so @everyone has the usual permissions), then promoted
    // to the Staff Server with the channels the feeds post in.
    let svc = ChatService::new(
        PgChatRepo::new(pool.clone()),
        clock.clone(),
        SnowflakeGenerator::new(clock.clone(), 22, 22).unwrap(),
        ChatLimits::default(),
    );
    let bp = svc
        .create_guild(UserId::new(owner as u64), "SIDE ROOM Staff")
        .await
        .unwrap();
    let guild = bp.guild.id;
    sqlx::query("UPDATE guilds SET is_staff_guild = true WHERE id = $1")
        .bind(guild.get() as i64)
        .execute(&pool)
        .await
        .unwrap();
    let mut channels = std::collections::HashMap::new();
    for name in ["alerts", "new-members", "donations", "staff-announcements"] {
        let id = ids.generate().get() as i64;
        sqlx::query(
            "INSERT INTO channels (id, guild_id, type, name, position) VALUES ($1, $2, 0, $3, 9)",
        )
        .bind(id)
        .bind(guild.get() as i64)
        .bind(name)
        .execute(&pool)
        .await
        .unwrap();
        channels.insert(name.to_owned(), ChannelId::new(id as u64));
    }

    let bus = Arc::new(InProcessBus::new());
    let messages = Arc::new(
        MessagesDeps::new(
            PgChatRepo::new(pool.clone()),
            Arc::new(sr_db::chat_repo::PgMessageStore::new(pool.clone())),
            Arc::new(ObjectStoreStorage::in_memory()),
            bus.clone(),
            Arc::new(FixedEntitlements(30 * 1024 * 1024)),
            Arc::new(PgChatRepo::new(pool.clone())),
            None,
            clock.clone(),
            23,
            23,
        )
        .unwrap(),
    );
    let voice = Arc::new(StaffVoice::new(pool.clone(), messages, "https://sr.test"));
    let next = Arc::new(std::sync::atomic::AtomicI64::new(
        ids.generate().get() as i64
    ));
    let repo = Arc::new(PgAdminRepo::new(
        pool.clone(),
        Arc::new(move || next.fetch_add(1, std::sync::atomic::Ordering::SeqCst)),
    ));
    let admin = AdminService::new(repo.clone())
        .with_extras(repo.clone())
        .with_notifier(voice.clone())
        .with_bus(bus.clone());
    Fx {
        db,
        pool,
        bus,
        voice,
        admin,
        repo,
        owner,
        guild,
        channels,
    }
}

async fn messages_in(pool: &PgPool, channel: ChannelId) -> Vec<sqlx::postgres::PgRow> {
    sqlx::query(
        "SELECT id, author_id, content, embeds, components FROM messages WHERE channel_id = $1 ORDER BY id",
    )
    .bind(channel.get() as i64)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn wait_for<F, Fut>(mut f: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for _ in 0..200 {
        if f().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    false
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_system_user_posts_cards_in_the_staff_channels() {
    let f = fx().await;
    let alerts = f.channels["alerts"];
    let mut live = f.bus.subscribe(Topic::Channel(alerts));
    let card = alert_card(
        StaffAlertKind::DiskLow,
        "Disk 91% full",
        42,
        "https://sr.test",
    );
    assert!(f.voice.post("alerts", card).await);

    let rows = messages_in(&f.pool, alerts).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<i64, _>("author_id"), SYSTEM_USER_ID);
    let embeds: Value = rows[0].get("embeds");
    assert_eq!(embeds[0]["title"], "Disk is filling up");
    assert_eq!(embeds[0]["description"], "Disk 91% full");
    let components: Value = rows[0].get("components");
    let buttons = components[0]["components"].as_array().unwrap();
    assert_eq!(buttons[0]["custom_id"], "sr:ack:42");
    assert_eq!(buttons[1]["url"], "https://sr.test/admin/system");
    // Clients hear about it like any other message.
    let ev = tokio::time::timeout(Duration::from_secs(2), live.next())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(ev, DomainEvent::MessageSent { author, .. } if author.get() == 1));
    // The system user became a member of the Staff Server to be able to post.
    let member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM guild_members WHERE guild_id = $1 AND user_id = 1)",
    )
    .bind(f.guild.get() as i64)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert!(member);
    // A channel that does not exist drops the card quietly.
    sqlx::query("DELETE FROM channels WHERE id = $1")
        .bind(f.channels["donations"].get() as i64)
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(
        !f.voice
            .post(
                "donations",
                feed_card(StaffFeedKind::Donation, "x", "y", None, "https://sr.test")
            )
            .await
    );
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_appointment_offer_arrives_as_a_system_dm_with_buttons() {
    let f = fx().await;
    let target = f.owner + 1000;
    account(&f.pool, target, "newbie").await;
    sqlx::query("INSERT INTO roles (id, guild_id, name, platform_permissions, position) VALUES ($1, $2, 'Support', 1, 1)")
        .bind(target + 1)
        .bind(f.guild.get() as i64)
        .execute(&f.pool)
        .await
        .unwrap();
    let caller = f.admin.resolve(f.owner, None).await.unwrap();
    let offer = f
        .admin
        .offer_appointment(&caller, target, &[target + 1], Some("good helper"))
        .await
        .unwrap();
    let (dm, msg) = (offer.dm_channel_id.unwrap(), offer.dm_message_id.unwrap());
    let row = sqlx::query("SELECT author_id, content, components FROM messages WHERE id = $1")
        .bind(msg)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(row.get::<i64, _>("author_id"), SYSTEM_USER_ID);
    assert_eq!(
        row.get::<String, _>("content"),
        "You've been invited to join SIDE ROOM Staff."
    );
    let comps: Value = row.get("components");
    assert_eq!(
        comps[0]["components"][0]["custom_id"],
        format!("sr:offer-accept:{}", offer.id)
    );
    assert_eq!(
        comps[0]["components"][1]["custom_id"],
        format!("sr:offer-decline:{}", offer.id)
    );
    // It is a real DM between the system user and the appointee.
    let recipients: Vec<i64> = sqlx::query_scalar(
        "SELECT user_id FROM channel_recipients WHERE channel_id = $1 ORDER BY user_id",
    )
    .bind(dm)
    .fetch_all(&f.pool)
    .await
    .unwrap();
    assert_eq!(recipients, vec![SYSTEM_USER_ID, target]);

    // The appointee accepts: staff, with a notice in #staff-announcements.
    let closed = f
        .admin
        .respond_to_offer(target, offer.id, true, None)
        .await
        .unwrap();
    assert_eq!(closed.state, OfferState::Accepted);
    assert!(f.admin.resolve(target, None).await.is_ok());
    let notices = messages_in(&f.pool, f.channels["staff-announcements"]).await;
    assert_eq!(notices.len(), 1);
    assert!(
        notices[0]
            .get::<String, _>("content")
            .contains("newbie joined the staff as Support")
    );

    // A second DM to the same person reuses the channel.
    let again = f.voice.dm(target, offer_card(&offer)).await.unwrap();
    assert_eq!(again.0, dm);
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn platform_events_become_cards_mail_and_revocation_pushes() {
    let f = fx().await;
    let mailer = Arc::new(MemoryMailer::new());
    let feed = Arc::new(StaffFeed {
        voice: f.voice.clone(),
        admin: f.admin.clone(),
        mailer: mailer.clone() as Arc<dyn Mailer>,
        bus: f.bus.clone(),
        throttle: MailThrottle::default(),
    });
    let task = spawn(feed);
    tokio::time::sleep(Duration::from_millis(50)).await;

    // An alert: stored for the dashboard and posted to #alerts. No mail yet (off by default).
    f.bus
        .publish(
            Topic::Platform,
            DomainEvent::StaffAlert {
                kind: StaffAlertKind::InviterSuspicion,
                detail: "bob reached suspicion level 2".into(),
            },
        )
        .await
        .unwrap();
    let alerts = f.channels["alerts"];
    assert!(
        wait_for(|| async { messages_in(&f.pool, alerts).await.len() == 1 }).await,
        "the card was posted"
    );
    let stored = f.repo.recent_alerts(10).await.unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].kind, "inviter_suspicion");
    let card = &messages_in(&f.pool, alerts).await[0];
    let comps: Value = card.get("components");
    assert_eq!(
        comps[0]["components"][0]["custom_id"],
        format!("sr:ack:{}", stored[0].id)
    );
    assert!(mailer.sent().is_empty());

    // Turn on the Owner email: the next alert is mailed, a repeat of the same kind is not.
    let c = f.admin.resolve(f.owner, None).await.unwrap();
    f.admin
        .patch_settings(
            &c,
            &[("alerts.email_to_owner".to_owned(), serde_json::json!(true))],
            Some("test"),
        )
        .await
        .unwrap();
    for _ in 0..2 {
        f.bus
            .publish(
                Topic::Platform,
                DomainEvent::StaffAlert {
                    kind: StaffAlertKind::DiskLow,
                    detail: "Disk 91% full".into(),
                },
            )
            .await
            .unwrap();
    }
    assert!(wait_for(|| async { messages_in(&f.pool, alerts).await.len() == 3 }).await);
    let sent = mailer.sent();
    assert_eq!(sent.len(), 1, "throttled to one mail per kind");
    assert_eq!(sent[0].to, "owner@example.test");
    assert!(sent[0].subject.contains("Disk is filling up"));
    assert!(sent[0].text.contains("Disk 91% full"));
    assert!(sent[0].text.contains("https://sr.test/admin/system"));

    // The feeds.
    f.bus
        .publish(
            Topic::Platform,
            DomainEvent::StaffFeed {
                feed: StaffFeedKind::NewMember,
                title: "New member".into(),
                detail: "@new joined, invited by @issuer.".into(),
                link: Some("/admin/invite-graph?account=5".into()),
            },
        )
        .await
        .unwrap();
    let members = f.channels["new-members"];
    assert!(wait_for(|| async { messages_in(&f.pool, members).await.len() == 1 }).await);
    let e: Value = messages_in(&f.pool, members).await[0].get("embeds");
    assert_eq!(e[0]["description"], "@new joined, invited by @issuer.");

    // Leaving the Staff Server: the appointment closes in the database, the member's own
    // sessions are told, and #alerts says so.
    let member = f.owner + 5000;
    account(&f.pool, member, "leaver").await;
    sqlx::query("INSERT INTO roles (id, guild_id, name, platform_permissions, position) VALUES ($1, $2, 'Basic', 1, 1)")
        .bind(member + 1)
        .bind(f.guild.get() as i64)
        .execute(&f.pool)
        .await
        .unwrap();
    f.repo
        .appoint(
            member,
            &[member + 1],
            f.owner,
            "test",
            &sr_admin::AuditEntry {
                actor_id: f.owner,
                kind: sr_admin::AccessKind::AdminAction,
                action: "staff.appoint".into(),
                target_kind: Some("account".into()),
                target_id: Some(member),
                reason: "test".into(),
                ip: None,
                detail: "{}".into(),
            },
        )
        .await
        .unwrap();
    assert!(f.admin.resolve(member, None).await.is_ok());
    let mut own = f.bus.subscribe(Topic::User(UserId::new(member as u64)));
    sqlx::query("DELETE FROM guild_members WHERE guild_id = $1 AND user_id = $2")
        .bind(f.guild.get() as i64)
        .bind(member)
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(
        f.admin.resolve(member, None).await.is_err(),
        "access ends at once"
    );
    f.bus
        .publish(
            Topic::Platform,
            DomainEvent::MemberLeft {
                guild: f.guild,
                user: UserId::new(member as u64),
            },
        )
        .await
        .unwrap();
    let pushed = tokio::time::timeout(Duration::from_secs(3), own.next())
        .await
        .expect("the leaver is told")
        .unwrap();
    match pushed {
        DomainEvent::UserScoped { name, payload, .. } => {
            assert_eq!(name, "SR_STAFF_ALERT");
            assert_eq!(payload["revoked"], true);
        }
        other => panic!("unexpected {other:?}"),
    }
    assert!(
        wait_for(|| async {
            f.repo
                .recent_alerts(20)
                .await
                .unwrap()
                .iter()
                .any(|a| a.kind == "staff_revoked" && a.detail.contains("leaver"))
        })
        .await
    );
    // Someone leaving an ordinary server is not a staff event.
    let before = f.repo.recent_alerts(50).await.unwrap().len();
    f.bus
        .publish(
            Topic::Platform,
            DomainEvent::MemberLeft {
                guild: GuildId::new(7),
                user: UserId::new(member as u64),
            },
        )
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(f.repo.recent_alerts(50).await.unwrap().len(), before);

    task.abort();
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn announcements_can_be_sent_as_dms_to_everyone_active() {
    use sr_admin::{AnnouncementInput, AnnouncementSeverity};
    let f = fx().await;
    let (a, b) = (f.owner + 11, f.owner + 12);
    account(&f.pool, a, "anna").await;
    account(&f.pool, b, "ben").await;
    sqlx::query("UPDATE accounts SET state = 'deactivated' WHERE id = $1")
        .bind(b)
        .execute(&f.pool)
        .await
        .unwrap();
    let c = f.admin.resolve(f.owner, None).await.unwrap();
    f.admin
        .announce(
            &c,
            AnnouncementInput {
                title: "Downtime".into(),
                body: "Tonight at 22:00 UTC.".into(),
                severity: Some(AnnouncementSeverity::Warning),
                critical: false,
                send_dm: true,
                starts_at_ms: None,
                ends_at_ms: None,
            },
            None,
        )
        .await
        .unwrap();
    // Anna (active) and the Owner get a DM; Ben (deactivated) and the system user do not.
    let dms = |who: i64| {
        let pool = f.pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM messages m JOIN channel_recipients cr ON cr.channel_id = m.channel_id \
                 WHERE cr.user_id = $1 AND m.author_id = 1 AND m.embeds->0->>'title' = 'Downtime'",
            )
            .bind(who)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    assert!(wait_for(|| async { dms(a).await == 1 && dms(f.owner).await == 1 }).await);
    assert_eq!(dms(b).await, 0);
    assert_eq!(f.admin.active_announcements().await.unwrap().len(), 1);

    // D-192: Anna turns the platform DMs off. An ordinary announcement skips her, a critical one
    // (which is always sent) still reaches her.
    sqlx::query(
        "INSERT INTO user_settings (user_id, settings, version)          VALUES ($1, '{\"notifications\": {\"platform_dm\": false}}', 1)",
    )
    .bind(a)
    .execute(&f.pool)
    .await
    .unwrap();
    for (title, critical) in [("Quiet news", false), ("Outage", true)] {
        f.admin
            .announce(
                &c,
                AnnouncementInput {
                    title: title.into(),
                    body: "Body.".into(),
                    severity: None,
                    critical,
                    send_dm: !critical,
                    starts_at_ms: None,
                    ends_at_ms: None,
                },
                None,
            )
            .await
            .unwrap();
    }
    let titled = |who: i64, title: &'static str| {
        let pool = f.pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM messages m JOIN channel_recipients cr ON cr.channel_id = m.channel_id                  WHERE cr.user_id = $1 AND m.author_id = 1 AND m.embeds->0->>'title' = $2",
            )
            .bind(who)
            .bind(title)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    assert!(
        wait_for(|| async {
            titled(f.owner, "Outage").await == 1 && titled(f.owner, "Quiet news").await == 1
        })
        .await
    );
    assert!(wait_for(|| async { titled(a, "Outage").await == 1 }).await);
    assert_eq!(titled(a, "Quiet news").await, 0);
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_health_check_alerts_about_a_missing_backup_once_a_day() {
    let f = fx().await;
    let dir = std::env::temp_dir().join(format!("sr-health-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let c = f.admin.resolve(f.owner, None).await.unwrap();
    f.admin
        .patch_settings(
            &c,
            &[
                (
                    "ops.backup_dir".to_owned(),
                    serde_json::json!(dir.to_string_lossy()),
                ),
                // The machine running the tests may be fuller than 80%.
                ("alerts.disk_percent".to_owned(), serde_json::json!(99)),
            ],
            Some("test"),
        )
        .await
        .unwrap();
    let bus: Arc<dyn EventBus> = f.bus.clone();
    let mut events = bus.subscribe(Topic::Platform);

    crate::admin_jobs::run_health(&f.pool, &f.admin, &bus).await;
    let ev = tokio::time::timeout(Duration::from_secs(2), events.next())
        .await
        .unwrap()
        .unwrap();
    let DomainEvent::StaffAlert { kind, detail } = ev else {
        panic!("expected an alert");
    };
    assert_eq!(kind, StaffAlertKind::BackupFailed);
    assert!(detail.contains("No backup found"));
    // Once the alert is stored (the feed does that), the same problem is not raised again.
    f.admin.record_alert(kind.as_str(), &detail).await.unwrap();
    crate::admin_jobs::run_health(&f.pool, &f.admin, &bus).await;
    assert!(
        tokio::time::timeout(Duration::from_millis(300), events.next())
            .await
            .is_err()
    );
    // A fresh backup file clears the condition (a new day would raise nothing).
    std::fs::write(dir.join("sideroom-1.dump"), b"x").unwrap();
    sqlx::query("DELETE FROM staff_alerts")
        .execute(&f.pool)
        .await
        .unwrap();
    crate::admin_jobs::run_health(&f.pool, &f.admin, &bus).await;
    assert!(
        tokio::time::timeout(Duration::from_millis(300), events.next())
            .await
            .is_err()
    );
    std::fs::remove_dir_all(&dir).unwrap();
    f.db.cleanup().await.unwrap();
}
