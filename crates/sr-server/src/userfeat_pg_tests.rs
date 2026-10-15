//! The feature modules wired to the real message stack (run with `SR_TEST_DATABASE_URL`, `--ignored`,
//! `--test-threads=1`): the system user posting cards and DMs, scheduled messages sent as their authors with the
//! author's permissions, suggestion cards with seeded votes and edited status, leveling from stored messages,
//! and roles handed out through the guild backend.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use sr_api::routes::guilds::backend::backend;
use sr_api::routes::messages::{FixedEntitlements, MessagesDeps};
use sr_chat::message_hooks::MessageHooks;
use sr_chat::{ChatLimits, ChatService, SendMessage};
use sr_core::{ChannelId, Clock, GuildId, SnowflakeGenerator, SystemClock, UserId};
use sr_db::{chat_repo::PgChatRepo, testing::TestDb};
use sr_events::InProcessBus;
use sr_features::{FeatureRegistry, FeatureService, ScopeRef};
use sr_storage::ObjectStoreStorage;
use sr_userfeat::reminders::Target;
use sr_userfeat::scheduled::now_ms;
use sr_userfeat::suggestions::SuggestionResponse;
use sr_userfeat::tick::Ticker;
use sr_userfeat::{Card, Poster, UserFeatures, features::*};

use crate::features_store::PgSettingsStore;
use crate::userfeat::{LevelingHook, PosterCell, SystemPoster};

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
    messages: Arc<MessagesDeps>,
    uf: UserFeatures,
    poster: Arc<SystemPoster>,
    ticker: Ticker,
    owner: i64,
    member: i64,
    outsider: i64,
    guild: i64,
    channel: i64,
}

async fn fx() -> Fx {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let ids: Ids = SnowflakeGenerator::new(clock.clone(), 24, 24).unwrap();
    let next = || ids.generate().get() as i64;

    // The system user, the server's owner, a member and someone who is not in the server.
    account(&pool, 1, "sideroom").await;
    let (owner, member, outsider) = (next(), next(), next());
    account(&pool, owner, "owner").await;
    account(&pool, member, "member").await;
    account(&pool, outsider, "outsider").await;

    let chat = ChatService::new(
        PgChatRepo::new(pool.clone()),
        clock.clone(),
        SnowflakeGenerator::new(clock.clone(), 25, 25).unwrap(),
        ChatLimits::default(),
    );
    let bp = chat
        .create_guild(UserId::new(owner as u64), "Feature server")
        .await
        .unwrap();
    let guild = bp.guild.id.get() as i64;
    sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
        .bind(guild)
        .bind(member)
        .execute(&pool)
        .await
        .unwrap();
    let channel: i64 = sqlx::query_scalar(
        "SELECT id FROM channels WHERE guild_id = $1 AND type = 0 ORDER BY id LIMIT 1",
    )
    .bind(guild)
    .fetch_one(&pool)
    .await
    .unwrap();

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
            26,
            26,
        )
        .unwrap(),
    );
    let mut registry = FeatureRegistry::new();
    register_all(&mut registry).unwrap();
    let features = FeatureService::new(
        Arc::new(registry),
        Arc::new(PgSettingsStore::new(
            pool.clone(),
            Arc::new(SnowflakeGenerator::new(SystemClock, 27, 27).unwrap()),
        )),
        None,
    );
    let uf_ids = Arc::new(SnowflakeGenerator::new(SystemClock, 28, 28).unwrap());
    let uf = UserFeatures::new(
        pool.clone(),
        features,
        Arc::new(move || uf_ids.generate().get() as i64),
    );
    let guild_backend = backend(
        ChatService::new(
            PgChatRepo::new(pool.clone()),
            clock.clone(),
            SnowflakeGenerator::new(clock.clone(), 29, 29).unwrap(),
            ChatLimits::default(),
        ),
        bus,
    );
    let poster = Arc::new(SystemPoster::new(
        pool.clone(),
        messages.clone(),
        guild_backend,
        None,
        "https://sr.test",
    ));
    let ticker = Ticker::new(uf.clone(), poster.clone(), None);
    Fx {
        db,
        pool,
        messages,
        uf,
        poster,
        ticker,
        owner,
        member,
        outsider,
        guild,
        channel,
    }
}

impl Fx {
    async fn enable(&self, feature: &str, settings: Value) {
        let scope = ScopeRef::Guild {
            guild: GuildId::new(self.guild as u64),
        };
        self.uf
            .features
            .set_settings(feature, &scope, settings.as_object().cloned().unwrap())
            .await
            .unwrap();
        self.uf
            .features
            .set_enabled(feature, &scope, Some(true))
            .await
            .unwrap();
    }

    async fn role(&self, name: &str) -> i64 {
        let id = self.uf.ids.as_ref()();
        sqlx::query("INSERT INTO roles (id, guild_id, name, position) VALUES ($1, $2, $3, 1)")
            .bind(id)
            .bind(self.guild)
            .bind(name)
            .execute(&self.pool)
            .await
            .unwrap();
        id
    }

    async fn messages_in(&self, channel: i64) -> Vec<sqlx::postgres::PgRow> {
        sqlx::query(
            "SELECT id, author_id, content, embeds, edited_at FROM messages WHERE channel_id = $1 ORDER BY id",
        )
        .bind(channel)
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }

    async fn has_role(&self, user: i64, role: i64) -> bool {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM member_roles WHERE guild_id = $1 AND user_id = $2 AND role_id = $3)",
        )
        .bind(self.guild)
        .bind(user)
        .bind(role)
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }
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
async fn scheduled_messages_go_out_as_their_author_with_their_permissions() {
    let f = fx().await;
    let ok =
        f.uf.schedule_message(
            f.member,
            f.channel,
            Some(f.guild),
            "from the past",
            now_ms() + 90_000,
        )
        .await
        .unwrap();
    // Someone outside the server (so without access) has one that is due too.
    let denied =
        f.uf.schedule_message(
            f.outsider,
            f.channel,
            Some(f.guild),
            "sneaky",
            now_ms() + 90_000,
        )
        .await
        .unwrap();
    sqlx::query("UPDATE scheduled_messages SET send_at = now() - interval '1 second'")
        .execute(&f.pool)
        .await
        .unwrap();
    let r = f.ticker.run_once(true).await;
    assert_eq!((r.scheduled_sent, r.scheduled_failed), (1, 1));

    let rows = f.messages_in(f.channel).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<i64, _>("author_id"), f.member);
    assert_eq!(rows[0].get::<String, _>("content"), "from the past");

    let sent =
        f.uf.get_scheduled(f.member, ok.id.parse().unwrap())
            .await
            .unwrap();
    assert_eq!(sent.status, "sent");
    assert_eq!(
        sent.message_id.unwrap(),
        rows[0].get::<i64, _>("id").to_string()
    );
    let failed =
        f.uf.get_scheduled(f.outsider, denied.id.parse().unwrap())
            .await
            .unwrap();
    assert_eq!(failed.status, "failed");
    // The author hears why, from the system user.
    let dm: Vec<(i64, String)> = sqlx::query_as(
        "SELECT m.author_id, m.embeds->0->>'title' FROM messages m \
         JOIN dm_channels d ON d.channel_id = m.channel_id \
         WHERE d.user_a = $1 OR d.user_b = $1",
    )
    .bind(f.outsider)
    .fetch_all(&f.pool)
    .await
    .unwrap();
    assert_eq!(dm.len(), 1);
    assert_eq!(dm[0].0, 1);
    assert_eq!(dm[0].1, "A scheduled message was not sent");
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_system_user_posts_cards_and_reminder_dms() {
    let f = fx().await;
    let id = f
        .poster
        .post(
            f.channel,
            Card {
                content: format!("<@{}> hello", f.member),
                title: Some("A card".into()),
                description: Some("With a link".into()),
                url: Some("/library/s/1".into()),
                color: 0x123456,
                footer: Some("Footer".into()),
                ping_users: vec![f.member],
                ..Card::default()
            },
        )
        .await
        .unwrap();
    let rows = f.messages_in(f.channel).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<i64, _>("id"), id);
    assert_eq!(rows[0].get::<i64, _>("author_id"), 1);
    let embeds: Value = rows[0].get("embeds");
    assert_eq!(embeds[0]["title"], "A card");
    assert_eq!(embeds[0]["url"], "https://sr.test/library/s/1");
    assert_eq!(embeds[0]["footer"]["text"], "Footer");
    // The system user joined the server to speak in it.
    let joined: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM guild_members WHERE guild_id = $1 AND user_id = 1)",
    )
    .bind(f.guild)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert!(joined);
    // A mention in the text pings only who the card allows.
    let mentions: Vec<i64> =
        sqlx::query_scalar("SELECT unnest(mention_user_ids) FROM messages WHERE id = $1")
            .bind(id)
            .fetch_all(&f.pool)
            .await
            .unwrap();
    assert_eq!(mentions, vec![f.member]);

    // A reminder arrives as a DM from the system user.
    let r =
        f.uf.create_reminder(
            f.member,
            "water the plants",
            now_ms() + 60_000,
            Target {
                channel: Some(f.channel),
                guild: Some(f.guild),
                message: Some(id),
            },
        )
        .await
        .unwrap();
    sqlx::query("UPDATE reminders SET remind_at = now() - interval '1 second' WHERE id = $1")
        .bind(r.id.parse::<i64>().unwrap())
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(f.ticker.run_once(true).await.reminders_sent, 1);
    let dm = sqlx::query(
        "SELECT m.author_id, m.embeds FROM messages m JOIN dm_channels d ON d.channel_id = m.channel_id \
         WHERE d.user_a = $1 OR d.user_b = $1",
    )
    .bind(f.member)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(dm.get::<i64, _>("author_id"), 1);
    let embeds: Value = dm.get("embeds");
    assert_eq!(embeds[0]["title"], "Reminder");
    assert_eq!(embeds[0]["description"], "water the plants");
    assert_eq!(
        embeds[0]["url"],
        format!("https://sr.test/channels/{}/{}/{}", f.guild, f.channel, id)
    );
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn suggestion_cards_get_votes_and_follow_the_status() {
    let f = fx().await;
    f.enable(SUGGESTIONS_ID, json!({"channel": f.channel.to_string()}))
        .await;
    let s =
        f.uf.submit_suggestion(&*f.poster, f.guild, f.member, "Add a movie night")
            .await
            .unwrap();
    let message: i64 = s.message_id.parse().unwrap();
    let reactions: Vec<(String, i64)> = sqlx::query_as(
        "SELECT emoji_key, user_id FROM reactions WHERE message_id = $1 ORDER BY emoji_key",
    )
    .bind(message)
    .fetch_all(&f.pool)
    .await
    .unwrap();
    assert_eq!(reactions.len(), 2);
    assert!(reactions.iter().all(|(_, user)| *user == 1));

    f.uf.respond_to_suggestion(
        &*f.poster,
        f.guild,
        s.id.parse().unwrap(),
        f.owner,
        &SuggestionResponse {
            status: "approved".into(),
            response: Some("Friday at 8".into()),
        },
    )
    .await
    .unwrap();
    let row =
        sqlx::query("SELECT embeds, edited_at IS NOT NULL AS edited FROM messages WHERE id = $1")
            .bind(message)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    let embeds: Value = row.get("embeds");
    assert_eq!(embeds[0]["description"], "Add a movie night");
    assert_eq!(embeds[0]["fields"][0]["value"], "Approved");
    assert_eq!(embeds[0]["fields"][1]["value"], "Friday at 8");
    assert_eq!(embeds[0]["color"], 0x2ECC71);
    assert!(row.get::<bool, _>("edited"));
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn leveling_awards_from_stored_messages_hands_out_roles_and_announces() {
    let f = fx().await;
    let role = f.role("Regular").await;
    f.enable(
        LEVELING_ID,
        json!({
            "xp_min": 100, "xp_max": 100, "cooldown": 0,
            "level_roles": [{"level": 1, "role": role.to_string()}]
        }),
    )
    .await;
    let cell: PosterCell = Arc::default();
    cell.set(f.poster.clone() as Arc<dyn Poster>).ok();
    let hook = LevelingHook {
        uf: f.uf.clone(),
        poster: cell,
    };
    let msg = f
        .messages
        .send(
            UserId::new(f.member as u64),
            ChannelId::new(f.channel as u64),
            SendMessage {
                content: "hello there".into(),
                ..SendMessage::default()
            },
        )
        .await
        .unwrap();
    hook.message_sent(&msg, &[]).await;

    assert!(
        wait_for(|| async { f.uf.rank_card(f.guild, f.member).await.map(|c| c.level) == Ok(1) })
            .await,
        "100 XP is level 1"
    );
    assert!(wait_for(|| async { f.has_role(f.member, role).await }).await);
    let announced = f.messages_in(f.channel).await;
    assert!(
        announced.iter().any(|r| r.get::<i64, _>("author_id") == 1
            && r.get::<String, _>("content").contains("reached level 1")),
        "the level-up is announced by the system user"
    );

    // The system user's own messages and servers without leveling earn nothing.
    let (g2, c2): (i64, i64) = {
        let bp = ChatService::new(
            PgChatRepo::new(f.pool.clone()),
            Arc::new(SystemClock),
            SnowflakeGenerator::new(Arc::new(SystemClock) as Arc<dyn Clock>, 30, 30).unwrap(),
            ChatLimits::default(),
        )
        .create_guild(UserId::new(f.owner as u64), "Quiet")
        .await
        .unwrap();
        let g = bp.guild.id.get() as i64;
        let c: i64 =
            sqlx::query_scalar("SELECT id FROM channels WHERE guild_id = $1 AND type = 0 LIMIT 1")
                .bind(g)
                .fetch_one(&f.pool)
                .await
                .unwrap();
        (g, c)
    };
    let quiet = f
        .messages
        .send(
            UserId::new(f.owner as u64),
            ChannelId::new(c2 as u64),
            SendMessage {
                content: "nothing".into(),
                ..SendMessage::default()
            },
        )
        .await
        .unwrap();
    LevelingHook {
        uf: f.uf.clone(),
        poster: Arc::default(),
    }
    .message_sent(&quiet, &[])
    .await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM member_xp WHERE guild_id = $1")
        .bind(g2)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
    f.db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn roles_are_handed_out_and_taken_back_through_the_guild_backend() {
    let f = fx().await;
    let role = f.role("Birthday").await;
    assert!(f.poster.add_role(f.guild, f.member, role).await);
    assert!(f.has_role(f.member, role).await);
    assert!(f.poster.remove_role(f.guild, f.member, role).await);
    assert!(!f.has_role(f.member, role).await);
    // An unknown role is reported, not panicked on.
    assert!(!f.poster.add_role(f.guild, f.member, role + 999_999).await);
    f.db.cleanup().await.unwrap();
}
