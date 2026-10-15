//! The built-in feature modules against real Postgres (run with `SR_TEST_DATABASE_URL`,
//! `--ignored`, `--test-threads=1`): the feature state store, the production
//! [`PgFeatureActions`] and a few modules driven through the runtime.

use std::sync::Arc;

use serde_json::{Value, json};
use sqlx::PgPool;
use sr_admin::SYSTEM_USER_ID;
use sr_api::routes::{
    guilds::backend::{GuildBackend, backend},
    messages::{FixedEntitlements, MessagesDeps},
    threads::ThreadsDeps,
};
use sr_chat::{ChatLimits, ChatService, repo::EmojiKey, role_rules::NewRole};
use sr_core::{ChannelId, Clock, GuildId, MessageId, SnowflakeGenerator, SystemClock, UserId};
use sr_db::{chat_repo::PgChatRepo, testing::TestDb};
use sr_events::{DomainEvent, EventBus, InProcessBus};
use sr_features::{
    FeatureCtx, FeatureRegistry, FeatureRuntime, FeatureService, ScopeRef,
    actions::{FeatureActions, OutMessage},
    modules::{self, starboard::STARBOARD_ID, welcome::WELCOME_ID},
    state::{FeatureState, StateEntry},
};
use sr_storage::ObjectStoreStorage;

use crate::feature_actions::PgFeatureActions;
use crate::features_store::{PgFeatureState, PgSettingsStore};

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

async fn join(pool: &PgPool, guild: GuildId, user: i64) {
    sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
        .bind(guild.get() as i64)
        .bind(user)
        .execute(pool)
        .await
        .unwrap();
}

struct Fx {
    _db: TestDb,
    pool: PgPool,
    bus: Arc<InProcessBus>,
    messages: Arc<MessagesDeps>,
    guilds: Arc<dyn GuildBackend>,
    actions: Arc<PgFeatureActions>,
    owner: UserId,
    guild: GuildId,
    general: ChannelId,
}

async fn fx() -> Fx {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let ids: Ids = SnowflakeGenerator::new(clock.clone(), 21, 21).unwrap();

    account(&pool, SYSTEM_USER_ID, "sideroom").await;
    let owner = ids.generate().get() as i64;
    account(&pool, owner, "owner").await;
    for (id, name) in [(5, "ann"), (6, "bob"), (7, "cat")] {
        account(&pool, id, name).await;
    }

    let svc = ChatService::new(
        PgChatRepo::new(pool.clone()),
        clock.clone(),
        SnowflakeGenerator::new(clock.clone(), 22, 22).unwrap(),
        ChatLimits::default(),
    );
    let bp = svc
        .create_guild(UserId::new(owner as u64), "Club")
        .await
        .unwrap();
    let guild = bp.guild.id;
    let general = bp
        .channels
        .iter()
        .find(|c| c.name.as_deref() == Some("general"))
        .unwrap()
        .id;
    for u in [5, 6, 7] {
        join(&pool, guild, u).await;
    }

    let bus = Arc::new(InProcessBus::new());
    let dynbus: Arc<dyn EventBus> = bus.clone();
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
    let threads = Arc::new(ThreadsDeps::new(
        sr_chat::threads::ThreadService::new(
            PgChatRepo::new(pool.clone()),
            clock.clone(),
            SnowflakeGenerator::new(clock.clone(), 24, 24).unwrap(),
        ),
        dynbus.clone(),
    ));
    let guilds = backend(
        ChatService::new(
            PgChatRepo::new(pool.clone()),
            clock.clone(),
            SnowflakeGenerator::new(clock.clone(), 25, 25).unwrap(),
            ChatLimits::default(),
        ),
        dynbus,
    );
    let actions = Arc::new(PgFeatureActions::new(
        pool.clone(),
        messages.clone(),
        threads,
        guilds.clone(),
        "https://sr.test",
    ));
    Fx {
        _db: db,
        pool,
        bus,
        messages,
        guilds,
        actions,
        owner: UserId::new(owner as u64),
        guild,
        general,
    }
}

async fn insert_user_message(f: &Fx, author: i64, content: &str) -> MessageId {
    let id = f.messages.next_id().get() as i64;
    sqlx::query(
        "INSERT INTO messages (id, channel_id, guild_id, author_id, content) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(f.general.get() as i64)
    .bind(f.guild.get() as i64)
    .bind(author)
    .bind(content)
    .execute(&f.pool)
    .await
    .unwrap();
    MessageId::new(id as u64)
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn feature_state_roundtrip() {
    let f = fx().await;
    let st = PgFeatureState::new(f.pool.clone());
    let g = f.guild;
    st.put(StateEntry::new("t.f", g, "a:1", json!({"n": 1})).due(1_000))
        .await
        .unwrap();
    st.put(StateEntry::new("t.f", g, "a:2", json!({"n": 2})).due(5_000))
        .await
        .unwrap();
    st.put(StateEntry::new("t.f", g, "b:1", json!({"n": 3})))
        .await
        .unwrap();

    let one = st.get("t.f", g, "a:1").await.unwrap().unwrap();
    assert_eq!(one.value, json!({"n": 1}));
    assert_eq!(one.due_ms, Some(1_000));
    assert_eq!(st.get("t.f", g, "b:1").await.unwrap().unwrap().due_ms, None);
    assert!(st.get("t.f", g, "zz").await.unwrap().is_none());

    let due = st.due("t.f", 2_000, 10).await.unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].key, "a:1");
    assert_eq!(st.due("t.f", 9_000, 10).await.unwrap().len(), 2);
    assert!(st.due("other.f", 9_000, 10).await.unwrap().is_empty());

    let listed = st.list("t.f", g, "a:").await.unwrap();
    assert_eq!(listed.len(), 2);
    // `%` and `_` in a prefix are not wildcards.
    assert!(st.list("t.f", g, "%").await.unwrap().is_empty());

    // Overwrite clears the due time.
    st.put(StateEntry::new("t.f", g, "a:1", json!({"n": 9})))
        .await
        .unwrap();
    assert_eq!(st.get("t.f", g, "a:1").await.unwrap().unwrap().due_ms, None);
    st.delete("t.f", g, "a:1").await.unwrap();
    assert!(st.get("t.f", g, "a:1").await.unwrap().is_none());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn system_user_posts_edits_and_deletes_messages() {
    let f = fx().await;
    let a = &f.actions;
    let id = a
        .post_message(
            f.general,
            OutMessage {
                content: "Hello <@5>".into(),
                embed: Some(json!({"type": "rich", "title": "T", "description": "D"})),
                mention_users: vec![UserId::new(5)],
            },
        )
        .await
        .unwrap();
    let got = a.get_message(f.general, id).await.unwrap().unwrap();
    assert_eq!(got.author.get(), SYSTEM_USER_ID as u64);
    assert_eq!(got.content, "Hello <@5>");
    assert_eq!(got.embeds[0]["title"], "T");

    a.edit_message(f.general, id, "Changed".into())
        .await
        .unwrap();
    assert_eq!(
        a.get_message(f.general, id).await.unwrap().unwrap().content,
        "Changed"
    );

    // Someone else's message cannot be edited through the port.
    let theirs = insert_user_message(&f, 5, "mine").await;
    assert!(
        a.edit_message(f.general, theirs.get(), "x".into())
            .await
            .is_err()
    );

    a.delete_message(f.general, id).await.unwrap();
    assert!(a.get_message(f.general, id).await.unwrap().is_none());
    assert!(matches!(
        a.delete_message(f.general, id).await,
        Err(sr_features::actions::ActionError::NotFound)
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn dm_goes_to_the_member() {
    let f = fx().await;
    f.actions
        .send_dm(UserId::new(6), OutMessage::text("welcome"))
        .await
        .unwrap();
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM messages m JOIN channels c ON c.id = m.channel_id \
         WHERE c.type = 1 AND m.content = 'welcome' AND m.author_id = 1",
    )
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn roles_and_reactions() {
    let f = fx().await;
    let role = f
        .guilds
        .create_role(
            f.owner,
            f.guild,
            NewRole {
                name: Some("Gamers".into()),
                ..NewRole::default()
            },
        )
        .await
        .unwrap();
    let a = &f.actions;
    let u = UserId::new(5);
    assert!(!a.has_role(f.guild, u, role.id.get()).await.unwrap());
    a.add_role(f.guild, u, role.id.get()).await.unwrap();
    assert!(a.has_role(f.guild, u, role.id.get()).await.unwrap());
    a.remove_role(f.guild, u, role.id.get()).await.unwrap();
    assert!(!a.has_role(f.guild, u, role.id.get()).await.unwrap());

    let m = insert_user_message(&f, 5, "star me").await;
    let star = EmojiKey::parse("\u{2b50}").unwrap();
    for user in [5, 6, 7] {
        f.messages
            .store
            .add_reaction(f.general, m, UserId::new(user), &star, 1)
            .await
            .unwrap();
    }
    assert_eq!(
        a.reaction_count(f.general, m.get(), "\u{2b50}", None)
            .await
            .unwrap(),
        3
    );
    assert_eq!(
        a.reaction_count(f.general, m.get(), "\u{2b50}", Some(UserId::new(5)))
            .await
            .unwrap(),
        2
    );
    a.remove_reaction(f.general, m.get(), "\u{2b50}", UserId::new(6))
        .await
        .unwrap();
    assert_eq!(
        a.reaction_count(f.general, m.get(), "\u{2b50}", None)
            .await
            .unwrap(),
        2
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn channel_edits_visibility_and_threads() {
    let f = fx().await;
    let a = &f.actions;

    a.set_slowmode(f.general, 30).await.unwrap();
    a.set_channel_name(f.general, "chat-2").await.unwrap();
    let info = a.get_channel(f.general).await.unwrap().unwrap();
    assert_eq!(info.rate_limit_per_user, 30);
    assert_eq!(info.name, "chat-2");
    assert!(info.everyone_can_view);

    // A channel hidden from @everyone is reported as not public.
    let private = f
        .guilds
        .create_channel(
            f.owner,
            f.guild,
            sr_chat::channel_rules::NewChannel::new(sr_chat::ChannelType::Text, "staff"),
            vec![sr_core::permissions::Overwrite {
                id: sr_core::Snowflake::new(f.guild.get()),
                kind: sr_core::permissions::OverwriteKind::Role,
                allow: sr_core::permissions::Permissions::empty(),
                deny: sr_core::permissions::Permissions::VIEW_CHANNEL,
            }],
        )
        .await
        .unwrap();
    let info = a.get_channel(private.channel.id).await.unwrap().unwrap();
    assert!(!info.everyone_can_view);

    let g = a.get_guild(f.guild).await.unwrap().unwrap();
    assert_eq!(g.name, "Club");
    assert_eq!(g.member_count, 4);
    let u = a.get_user(UserId::new(5)).await.unwrap().unwrap();
    assert_eq!(u.name, "ann");

    // A thread on a member's message; the member lacks no permission in a fresh guild.
    let m = insert_user_message(&f, 5, "let us talk").await;
    let thread = a
        .create_thread(f.general, m.get(), "ann: let us talk")
        .await
        .unwrap();
    let t = a.get_channel(thread).await.unwrap().unwrap();
    assert_eq!(t.name, "ann: let us talk");
    assert_eq!(t.parent, Some(f.general));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn starboard_and_welcome_run_through_the_runtime() {
    let f = fx().await;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let ids = Arc::new(SnowflakeGenerator::new(SystemClock, 26, 26).unwrap());
    let mut reg = FeatureRegistry::new();
    modules::register_all(&mut reg).unwrap();
    let dynbus: Arc<dyn EventBus> = f.bus.clone();
    let settings = FeatureService::new(
        Arc::new(reg),
        Arc::new(PgSettingsStore::new(f.pool.clone(), ids)),
        None,
    );
    let board = f
        .guilds
        .create_channel(
            f.owner,
            f.guild,
            sr_chat::channel_rules::NewChannel::new(sr_chat::ChannelType::Text, "starboard"),
            vec![],
        )
        .await
        .unwrap()
        .channel
        .id;
    let scope = ScopeRef::Guild { guild: f.guild };
    for id in [STARBOARD_ID, WELCOME_ID] {
        settings.set_enabled(id, &scope, Some(true)).await.unwrap();
    }
    let values = |v: Value| v.as_object().cloned().unwrap();
    settings
        .set_settings(
            STARBOARD_ID,
            &scope,
            values(json!({"channel": board.to_string(), "threshold": 2})),
        )
        .await
        .unwrap();
    settings
        .set_settings(
            WELCOME_ID,
            &scope,
            values(json!({"welcome": {
                "enabled": true, "channel": f.general.to_string(),
                "template": "Welcome {user.name} to {server}"}})),
        )
        .await
        .unwrap();
    let state = Arc::new(PgFeatureState::new(f.pool.clone()));
    let rt = FeatureRuntime::new(FeatureCtx {
        bus: dynbus,
        settings: settings.clone(),
        clock,
        actions: f.actions.clone(),
        state: state.clone(),
    });

    // Welcome.
    rt.dispatch(&DomainEvent::MemberJoined {
        guild: f.guild,
        user: UserId::new(7),
        payload: json!({"user": {"id": "7", "username": "cat"}}),
    })
    .await;
    let welcomed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM messages WHERE channel_id = $1 AND author_id = 1 \
         AND content = 'Welcome cat to Club'",
    )
    .bind(f.general.get() as i64)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(welcomed, 1);

    // Starboard: two stars from non-authors post to the board, a third updates it.
    let m = insert_user_message(&f, 5, "great news").await;
    let star = EmojiKey::parse("\u{2b50}").unwrap();
    let react = |user: u64| DomainEvent::ReactionAdded {
        guild: Some(f.guild),
        channel: f.general,
        message_id: m.get(),
        user: UserId::new(user),
        emoji: json!({"id": null, "name": "\u{2b50}"}),
    };
    f.messages
        .store
        .add_reaction(f.general, m, UserId::new(6), &star, 1)
        .await
        .unwrap();
    rt.dispatch(&react(6)).await;
    let on_board = || async {
        sqlx::query_scalar::<_, String>(
            "SELECT content FROM messages WHERE channel_id = $1 AND visibility = 'visible' ORDER BY id",
        )
        .bind(board.get() as i64)
        .fetch_all(&f.pool)
        .await
        .unwrap()
    };
    assert!(
        on_board().await.is_empty(),
        "one star is below the threshold"
    );
    f.messages
        .store
        .add_reaction(f.general, m, UserId::new(7), &star, 1)
        .await
        .unwrap();
    rt.dispatch(&react(7)).await;
    let posts = on_board().await;
    assert_eq!(posts.len(), 1);
    assert!(posts[0].contains("**2**"), "{posts:?}");

    // The author's own star does not count; a third real star updates the same post.
    f.messages
        .store
        .add_reaction(f.general, m, UserId::new(5), &star, 1)
        .await
        .unwrap();
    rt.dispatch(&react(5)).await;
    assert!(on_board().await[0].contains("**2**"));
    let state_row = state
        .get(STARBOARD_ID, f.guild, &format!("link:{}", m.get()))
        .await
        .unwrap();
    assert!(state_row.is_some());
}
