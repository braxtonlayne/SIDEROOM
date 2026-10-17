//! The watch party card against real Postgres (run with `SR_TEST_DATABASE_URL`, `--ignored`,
//! `--test-threads=1`): the system user posts one type-200 message in the party's channel, rewrites it when
//! the headcount changes (spaced out, the newest change wins) and greys it when the party ends.

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use sqlx::{PgPool, Row};
use sr_api::routes::{
    donations::ChatSlot,
    messages::{FixedEntitlements, MessagesDeps},
};
use sr_chat::{ChatLimits, ChatService};
use sr_core::{Clock, SnowflakeGenerator, SystemClock, UserId};
use sr_db::{chat_repo::PgChatRepo, testing::TestDb};
use sr_events::InProcessBus;
use sr_storage::ObjectStoreStorage;
use sr_watchparty::{Announcement, AnnouncementKind, ChatHook};

use crate::party_cards::PartyCards;

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
    cards: PartyCards,
    channel: i64,
}

async fn fx(gap: Duration) -> Fx {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let ids: Ids = SnowflakeGenerator::new(clock.clone(), 14, 14).unwrap();
    account(&pool, sr_chat::SYSTEM_USER_ID as i64, "sideroom").await;
    let owner = ids.generate().get() as i64;
    account(&pool, owner, "owner").await;
    let svc = ChatService::new(
        PgChatRepo::new(pool.clone()),
        clock.clone(),
        SnowflakeGenerator::new(clock.clone(), 15, 15).unwrap(),
        ChatLimits::default(),
    );
    let bp = svc
        .create_guild(UserId::new(owner as u64), "Movie night")
        .await
        .unwrap();
    let channel = ids.generate().get() as i64;
    sqlx::query("INSERT INTO channels (id, guild_id, type, name, position) VALUES ($1, $2, 0, 'theatre', 9)")
        .bind(channel)
        .bind(bp.guild.id.get() as i64)
        .execute(&pool)
        .await
        .unwrap();
    let messages = Arc::new(
        MessagesDeps::new(
            PgChatRepo::new(pool.clone()),
            Arc::new(sr_db::chat_repo::PgMessageStore::new(pool.clone())),
            Arc::new(ObjectStoreStorage::in_memory()),
            Arc::new(InProcessBus::new()),
            Arc::new(FixedEntitlements(30 * 1024 * 1024)),
            Arc::new(PgChatRepo::new(pool.clone())),
            None,
            clock.clone(),
            16,
            16,
        )
        .unwrap()
        .with_cdn_signer(sr_storage::cdn::CdnSigner::new(
            b"party-card-test-secret-0123456789".to_vec(),
        )),
    );
    let slot = ChatSlot::default();
    let _ = slot.set(messages);
    let cards = PartyCards::new(pool.clone(), slot).with_gap(gap);
    Fx {
        db,
        pool,
        cards,
        channel,
    }
}

fn ann(f: &Fx, kind: AnnouncementKind, viewers: usize) -> Announcement {
    Announcement {
        party_id: 4242,
        channel_id: f.channel,
        host_id: 7,
        title: "Akira".into(),
        episode_label: None,
        viewers,
        age_restricted: false,
        kind,
    }
}

async fn card_rows(f: &Fx) -> Vec<sqlx::postgres::PgRow> {
    sqlx::query(
        "SELECT id, author_id, type, content, embeds, components, edited_at IS NOT NULL AS edited \
         FROM messages WHERE channel_id = $1 ORDER BY id",
    )
    .bind(f.channel)
    .fetch_all(&f.pool)
    .await
    .unwrap()
}

async fn description(f: &Fx) -> String {
    let rows = card_rows(f).await;
    let embeds: Value = rows[0].get("embeds");
    embeds[0]["description"].as_str().unwrap().to_owned()
}

async fn wait_for_description(f: &Fx, want: &str) -> bool {
    for _ in 0..200 {
        if description(f).await == want {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    false
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_card_is_posted_once_followed_and_greyed_at_the_end() {
    let f = fx(Duration::from_millis(150)).await;
    f.cards
        .announce(ann(&f, AnnouncementKind::Started, 1))
        .await;

    let rows = card_rows(&f).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get::<i64, _>("author_id"),
        sr_chat::SYSTEM_USER_ID as i64
    );
    assert_eq!(rows[0].get::<i16, _>("type"), 200);
    assert_eq!(rows[0].get::<String, _>("content"), "");
    assert!(!rows[0].get::<bool, _>("edited"));
    let embeds: Value = rows[0].get("embeds");
    assert_eq!(embeds[0]["title"], "Akira");
    assert_eq!(embeds[0]["description"], "1 watching");
    assert_eq!(embeds[0]["url"], "/cinema/party/4242");
    let components: Value = rows[0].get("components");
    assert!(components.as_array().is_some_and(Vec::is_empty));
    let stored: i64 =
        sqlx::query_scalar("SELECT count(*) FROM watch_party_cards WHERE party_id = 4242")
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(stored, 1);

    // The same party announced again posts nothing new.
    f.cards
        .announce(ann(&f, AnnouncementKind::Started, 1))
        .await;
    assert_eq!(card_rows(&f).await.len(), 1);

    // Updates inside the gap are queued: only the newest is written, once the gap is over.
    f.cards
        .announce(ann(&f, AnnouncementKind::Updated, 2))
        .await;
    f.cards
        .announce(ann(&f, AnnouncementKind::Updated, 3))
        .await;
    f.cards
        .announce(ann(&f, AnnouncementKind::Updated, 5))
        .await;
    assert_eq!(description(&f).await, "1 watching");
    assert!(wait_for_description(&f, "5 watching").await);
    assert_eq!(
        card_rows(&f).await.len(),
        1,
        "edited in place, not reposted"
    );
    assert!(card_rows(&f).await[0].get::<bool, _>("edited"));

    // The end is written at once, whatever the gap, and the card is greyed.
    f.cards
        .announce(ann(&f, AnnouncementKind::Updated, 4))
        .await;
    f.cards.announce(ann(&f, AnnouncementKind::Ended, 4)).await;
    assert_eq!(description(&f).await, "Ended after 4 viewers");
    let embeds: Value = card_rows(&f).await[0].get("embeds");
    assert_eq!(embeds[0]["color"], 0x0074_7F8D);
    // The queued update does not come back afterwards.
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(description(&f).await, "Ended after 4 viewers");
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_restricted_title_is_not_named_and_a_vanished_channel_is_forgotten() {
    let f = fx(Duration::from_millis(10)).await;
    let mut a = ann(&f, AnnouncementKind::Started, 2);
    a.age_restricted = true;
    f.cards.announce(a).await;
    let embeds: Value = card_rows(&f).await[0].get("embeds");
    assert_eq!(embeds[0]["title"], "Age-restricted title");

    // The message is deleted: the next edit finds nothing and forgets the card.
    sqlx::query("DELETE FROM messages WHERE channel_id = $1")
        .bind(f.channel)
        .execute(&f.pool)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(30)).await;
    f.cards
        .announce(ann(&f, AnnouncementKind::Updated, 3))
        .await;
    let left: i64 =
        sqlx::query_scalar("SELECT count(*) FROM watch_party_cards WHERE party_id = 4242")
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(left, 0);

    // A party in a channel that does not exist leaves no row behind.
    let mut gone = ann(&f, AnnouncementKind::Started, 1);
    gone.party_id = 99;
    gone.channel_id = 123_456_789;
    f.cards.announce(gone).await;
    let rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM watch_party_cards WHERE party_id = 99")
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(rows, 0);
    f.db.cleanup().await.unwrap();
}
