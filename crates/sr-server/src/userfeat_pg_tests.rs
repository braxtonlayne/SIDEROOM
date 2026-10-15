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
    for _ in 0..400 {
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

    // G-72: votes count the members' reactions and not the system user's seed.
    for (key, user) in [("👍", f.member), ("👍", f.owner), ("👎", f.outsider)] {
        sqlx::query("INSERT INTO reactions (message_id, emoji_key, user_id) VALUES ($1, $2, $3)")
            .bind(message)
            .bind(key)
            .bind(user)
            .execute(&f.pool)
            .await
            .unwrap();
    }
    let as_member =
        f.uf.list_suggestions(f.guild, None, None, 10, Some(f.member))
            .await
            .unwrap();
    assert_eq!((as_member[0].votes.up, as_member[0].votes.down), (2, 1));
    assert_eq!(
        as_member[0].my_vote,
        Some(sr_userfeat::suggestions::Vote::Up)
    );
    let as_outsider =
        f.uf.list_suggestions(f.guild, None, None, 10, Some(f.outsider))
            .await
            .unwrap();
    assert_eq!(
        as_outsider[0].my_vote,
        Some(sr_userfeat::suggestions::Vote::Down)
    );
    let anon =
        f.uf.list_suggestions(f.guild, None, None, 10, None)
            .await
            .unwrap();
    assert_eq!(anon[0].my_vote, None);

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
    // G-70: the rank card lists the level roles, and drops one whose role was deleted.
    let card = f.uf.rank_card(f.guild, f.member).await.unwrap();
    assert_eq!(card.level_roles.len(), 1);
    assert_eq!(card.level_roles[0].level, 1);
    assert_eq!(card.level_roles[0].role_id, role.to_string());
    // The announcement is posted after the role grant, so wait for it as well.
    assert!(
        wait_for(|| async {
            f.messages_in(f.channel).await.iter().any(|r| {
                r.get::<i64, _>("author_id") == 1
                    && r.get::<String, _>("content").contains("reached level 1")
            })
        })
        .await,
        "the level-up is announced by the system user"
    );
    sqlx::query("DELETE FROM roles WHERE id = $1")
        .bind(role)
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(
        f.uf.rank_card(f.guild, f.member)
            .await
            .unwrap()
            .level_roles
            .is_empty()
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

// ---- HTTP: modules capabilities (G-71) and the ticket flow end to end (G-73) ------------------------

mod http {
    use super::*;
    use axum::{
        Router,
        body::Body,
        http::{Method, Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use sr_api::{
        ApiSettings, AppState,
        auth::{csrf, memory::MemorySessions},
        build_router,
        routes::{
            settings::deps::SettingsDeps,
            threads::ThreadsDeps,
            userfeat::{UserFeatDeps, router},
        },
    };
    use sr_auth::{
        repo::{SessionRecord, SessionRepository},
        session::SessionToken,
    };
    use sr_db::settings_repo::PgSettingsRepo;
    use tower::ServiceExt;

    struct Http {
        app: Router,
        sessions: Arc<MemorySessions>,
        next_session: std::sync::atomic::AtomicI64,
    }

    impl Http {
        async fn login(&self, user: i64) -> String {
            let t = SessionToken::generate().unwrap();
            self.sessions
                .create(SessionRecord {
                    id: self
                        .next_session
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst),
                    user_id: user,
                    token_hash: t.hash(),
                    created_at: 0,
                    last_seen_at: i64::MAX / 4,
                    expires_at: i64::MAX / 4,
                    device_name: None,
                })
                .await
                .unwrap();
            t.expose().to_owned()
        }

        async fn call(
            &self,
            token: &str,
            method: Method,
            path: &str,
            body: Option<Value>,
        ) -> (StatusCode, Value) {
            let mut req = Request::builder()
                .method(method.clone())
                .uri(format!("/api/v1{path}"))
                .header("origin", "http://localhost:8080")
                .header("cookie", format!("sr_session={token}"));
            if method != Method::GET {
                req = req.header("x-sr-csrf", csrf::token_for(token));
            }
            let body = match body {
                Some(b) => {
                    req = req.header("content-type", "application/json");
                    Body::from(b.to_string())
                }
                None => Body::empty(),
            };
            let resp = self
                .app
                .clone()
                .oneshot(req.body(body).unwrap())
                .await
                .unwrap();
            let status = resp.status();
            let bytes = resp.into_body().collect().await.unwrap().to_bytes();
            (
                status,
                serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            )
        }
    }

    fn http(f: &Fx) -> Http {
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let bus: Arc<dyn sr_events::EventBus> = Arc::new(InProcessBus::new());
        let sessions = Arc::new(MemorySessions::new());
        let threads = ThreadsDeps::new(
            sr_chat::threads::ThreadService::new(
                PgChatRepo::new(f.pool.clone()),
                clock.clone(),
                SnowflakeGenerator::new(clock.clone(), 30, 30).unwrap(),
            ),
            bus,
        );
        let settings_deps = SettingsDeps::new(
            Arc::new(PgSettingsRepo::new(f.pool.clone())),
            Arc::new(PgChatRepo::new(f.pool.clone())),
            Arc::new(ObjectStoreStorage::in_memory()),
            clock,
            Arc::new(|| 1),
            31,
            31,
        )
        .unwrap();
        let deps = UserFeatDeps {
            uf: f.uf.clone(),
            poster: f.poster.clone(),
            net_policy: sr_net::Policy::default(),
        };
        let mut settings = ApiSettings::default();
        settings.rate_limit.enabled = false;
        let state = AppState::builder()
            .settings(settings)
            .sessions(sessions.clone())
            .extension(Arc::new(deps))
            .extension(Arc::new(settings_deps))
            .extension(Arc::new(threads))
            .extension(f.messages.clone())
            .build();
        Http {
            app: build_router(state, router()),
            sessions,
            next_session: std::sync::atomic::AtomicI64::new(1000),
        }
    }

    async fn join_as_staff(f: &Fx, name: &str, role: i64) -> i64 {
        let id = f.uf.ids.as_ref()();
        account(&f.pool, id, name).await;
        sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
            .bind(f.guild)
            .bind(id)
            .execute(&f.pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO member_roles (guild_id, user_id, role_id) VALUES ($1, $2, $3)")
            .bind(f.guild)
            .bind(id)
            .bind(role)
            .execute(&f.pool)
            .await
            .unwrap();
        id
    }

    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn modules_report_what_the_caller_may_do_for_suggestions_and_tickets() {
        let f = fx().await;
        let staff_role = f.role("Helpers").await;
        let helper = join_as_staff(&f, "helper", staff_role).await;
        f.enable(
            SUGGESTIONS_ID,
            json!({"channel": f.channel.to_string(), "staff_role": staff_role.to_string()}),
        )
        .await;
        f.enable(
            TICKETS_ID,
            json!({"channel": f.channel.to_string(), "staff_role": staff_role.to_string()}),
        )
        .await;
        let h = http(&f);
        let path = format!("/guilds/{}/modules", f.guild);
        for (who, expect) in [(f.member, false), (helper, true), (f.owner, true)] {
            let token = h.login(who).await;
            let (status, body) = h.call(&token, Method::GET, &path, None).await;
            assert_eq!(status, StatusCode::OK, "{body}");
            for module in [SUGGESTIONS_ID, TICKETS_ID] {
                assert_eq!(
                    body["member_capabilities"][module]["can_respond"], expect,
                    "{module} for {who}: {body}"
                );
                assert_eq!(body["modules"][module], true);
            }
            // Modules that gate nothing carry no capabilities.
            assert!(body["member_capabilities"].get(LEVELING_ID).is_none());
        }
        f.db.cleanup().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn a_ticket_opens_a_private_thread_with_staff_and_closes_to_an_archive() {
        let f = fx().await;
        let staff_role = f.role("Helpers").await;
        let helper = join_as_staff(&f, "helper", staff_role).await;
        f.enable(
            TICKETS_ID,
            json!({
                "channel": f.channel.to_string(),
                "staff_role": staff_role.to_string(),
                "max_open_per_user": 1,
            }),
        )
        .await;
        // Members need to be allowed to create private threads in the ticket channel (bit 36).
        sqlx::query("UPDATE roles SET permissions = permissions | (1::bigint << 36) WHERE id = $1")
            .bind(f.guild)
            .execute(&f.pool)
            .await
            .unwrap();
        let h = http(&f);
        let opener = h.login(f.member).await;
        let staff = h.login(helper).await;
        let stranger = h.login(f.outsider).await;
        sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
            .bind(f.guild)
            .bind(f.outsider)
            .execute(&f.pool)
            .await
            .unwrap();
        let tickets = format!("/guilds/{}/tickets", f.guild);

        // Open: a private thread with the opener and the members of the staff role in it.
        let (status, ticket) = h
            .call(
                &opener,
                Method::POST,
                &tickets,
                Some(json!({"subject": "Cannot upload"})),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{ticket}");
        assert_eq!(ticket["status"], "open");
        let thread: i64 = ticket["thread_id"].as_str().unwrap().parse().unwrap();
        let kind: i16 = sqlx::query_scalar("SELECT type FROM channels WHERE id = $1")
            .bind(thread)
            .fetch_one(&f.pool)
            .await
            .unwrap();
        assert_eq!(kind, 12, "a private thread");
        let mut members: Vec<i64> =
            sqlx::query_scalar("SELECT user_id FROM thread_members WHERE channel_id = $1")
                .bind(thread)
                .fetch_all(&f.pool)
                .await
                .unwrap();
        // The system user joins to greet them.
        members.retain(|u| *u != 1);
        members.sort_unstable();
        let mut expected = vec![f.member, helper];
        expected.sort_unstable();
        assert_eq!(members, expected, "the opener and the staff, nobody else");
        // The system user greets them and pings the staff role.
        let greeting = f.messages_in(thread).await;
        assert!(greeting.iter().any(|m| {
            m.get::<i64, _>("author_id") == 1
                && m.get::<String, _>("content")
                    .contains(&format!("<@&{staff_role}>"))
        }));

        // A second open is refused while one is open (max_open_per_user = 1).
        let (status, again) = h
            .call(
                &opener,
                Method::POST,
                &tickets,
                Some(json!({"subject": "Another"})),
            )
            .await;
        assert_eq!(status, StatusCode::CONFLICT, "{again}");

        // A staff reply is visible in the thread.
        f.messages
            .send(
                UserId::new(helper as u64),
                ChannelId::new(thread as u64),
                SendMessage {
                    content: "Try a smaller file".into(),
                    ..SendMessage::default()
                },
            )
            .await
            .unwrap();
        assert!(f.messages_in(thread).await.iter().any(|m| {
            m.get::<i64, _>("author_id") == helper
                && m.get::<String, _>("content") == "Try a smaller file"
        }));

        // Someone who is neither the opener nor staff cannot close it.
        let id = ticket["id"].as_str().unwrap();
        let close = format!("{tickets}/{id}/close");
        let (status, _) = h.call(&stranger, Method::POST, &close, None).await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        // Staff close it: the ticket is closed and the thread archived.
        let (status, closed) = h.call(&staff, Method::POST, &close, None).await;
        assert_eq!(status, StatusCode::OK, "{closed}");
        assert_eq!(closed["status"], "closed");
        let (archived, locked): (bool, bool) =
            sqlx::query_as("SELECT archived, locked FROM thread_metadata WHERE channel_id = $1")
                .bind(thread)
                .fetch_one(&f.pool)
                .await
                .unwrap();
        // The thread is locked too where the closer may lock it (staff here hold no thread powers).
        assert!(archived, "locked: {locked}");

        // With nothing open any more the member may open a new one.
        let (status, second) = h
            .call(
                &opener,
                Method::POST,
                &tickets,
                Some(json!({"subject": "Next problem"})),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{second}");
        // The opener sees only their own tickets, staff see all.
        let (_, mine) = h.call(&opener, Method::GET, &tickets, None).await;
        assert_eq!(mine.as_array().unwrap().len(), 2);
        let (_, theirs) = h.call(&stranger, Method::GET, &tickets, None).await;
        assert!(theirs.as_array().unwrap().is_empty());
        let (_, all) = h.call(&staff, Method::GET, &tickets, None).await;
        assert_eq!(all.as_array().unwrap().len(), 2);
        f.db.cleanup().await.unwrap();
    }
}
