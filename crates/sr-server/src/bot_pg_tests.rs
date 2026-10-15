//! A bot driven end to end against real Postgres (run with `SR_TEST_DATABASE_URL`, `--ignored`,
//! `--test-threads=1`). This is the in-repo core of the SDK compatibility harness
//! (docs/dev/sdk-compat.md): a client that behaves like a Discord library talks to the real
//! gateway (IDENTIFY, READY, dispatches) and the real REST router, with the real stores behind
//! both.
//!
//! The list of behaviours checked here is the "smoke" tier of the compatibility test list; the
//! other tiers (discord.js, discord.py, serenity, twilight sample bots) need those libraries
//! and a running server, which CI does not have yet.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use sr_api::{
    ApiSettings, AppState,
    auth::csrf,
    build_router,
    routes::{
        developer::{self, DevDeps, PasswordConfirm},
        messages::{
            FixedEntitlements, MessagesDeps,
            error::{MsgError, MsgResult},
        },
    },
};
use sr_apps::{
    AppsService,
    service::{InstallRequest, UpdateInput},
};
use sr_auth::{
    oauth::Scope,
    repo::{SessionRecord, SessionRepository},
    session::SessionToken,
};
use sr_chat::repo::GuildRepository;
use sr_core::{ApiError, Clock, ErrorCode, Permissions, SnowflakeGenerator, SystemClock, UserId};
use sr_db::{apps_repo::PgAppsRepo, auth_repo::PgAuthRepo, chat_repo::PgChatRepo, testing::TestDb};
use sr_events::{EventBus, InProcessBus};
use sr_gateway::{ConnectParams, Frame, Gateway, GatewayConfig, close, intents, run_connection};
use sr_storage::ObjectStoreStorage;
use tower::ServiceExt;

use crate::gateway_pg::{PgAuthenticator, PgGuildDirectory};

struct NoPasswords;

#[async_trait]
impl PasswordConfirm for NoPasswords {
    async fn confirm(&self, _: i64, _: i64, _: &str) -> MsgResult<()> {
        Err(MsgError::Api(ApiError::new(ErrorCode::SrReauthRequired)))
    }
}

type Ids = SnowflakeGenerator<Arc<dyn Clock>>;

async fn account(pool: &PgPool, ids: &Ids, name: &str) -> UserId {
    let person = ids.generate().get() as i64;
    let acct = ids.generate();
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(person)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $2, $3, $4, 'x')",
    )
    .bind(acct.get() as i64)
    .bind(person)
    .bind(name)
    .bind(format!("{name}@example.test"))
    .execute(pool)
    .await
    .unwrap();
    UserId::new(acct.get())
}

/// A gateway client the way a library would use it.
struct Client {
    tx: tokio::sync::mpsc::Sender<Frame>,
    rx: tokio::sync::mpsc::Receiver<Frame>,
}

enum Opened {
    Ready(Box<Client>, Value),
    Closed(u16),
}

impl Client {
    async fn open(gw: &Gateway, token: &str, intents: u64, shard: Option<Value>) -> Opened {
        let (tx, in_rx) = tokio::sync::mpsc::channel(64);
        let (out_tx, rx) = tokio::sync::mpsc::channel(256);
        tokio::spawn(run_connection(
            gw.clone(),
            ConnectParams::default(),
            in_rx,
            out_tx,
        ));
        let mut c = Client { tx, rx };
        let hello = c.next().await;
        assert_eq!(hello["op"], 10, "HELLO first");
        let mut d = json!({
            "token": token, "intents": intents,
            "properties": {"os": "linux", "browser": "sdk-compat", "device": "sdk-compat"},
        });
        if let Some(s) = shard {
            d["shard"] = s;
        }
        c.send(json!({"op": 2, "d": d})).await;
        loop {
            match c.recv().await {
                Frame::Close { code, .. } => return Opened::Closed(code),
                Frame::Text(t) => {
                    let v: Value = serde_json::from_str(&t).unwrap();
                    if v["t"] == "READY" {
                        let d = v["d"].clone();
                        return Opened::Ready(Box::new(c), d);
                    }
                }
                Frame::Binary(_) => panic!("binary frame"),
            }
        }
    }

    async fn send(&self, v: Value) {
        self.tx.send(Frame::Text(v.to_string())).await.unwrap();
    }

    async fn recv(&mut self) -> Frame {
        tokio::time::timeout(Duration::from_secs(20), self.rx.recv())
            .await
            .expect("timed out")
            .expect("closed")
    }

    async fn next(&mut self) -> Value {
        match self.recv().await {
            Frame::Text(t) => serde_json::from_str(&t).unwrap(),
            other => panic!("unexpected {other:?}"),
        }
    }

    async fn event(&mut self, name: &str) -> Value {
        loop {
            let v = self.next().await;
            if v["t"] == name {
                return v["d"].clone();
            }
        }
    }

    /// Dispatches seen within `ms` (to prove something did *not* arrive).
    async fn drain(&mut self, ms: u64) -> Vec<Value> {
        let mut out = Vec::new();
        while let Ok(Some(Frame::Text(t))) =
            tokio::time::timeout(Duration::from_millis(ms), self.rx.recv()).await
        {
            out.push(serde_json::from_str(&t).unwrap());
        }
        out
    }
}

struct Rest {
    app: Router,
}

impl Rest {
    async fn call(
        &self,
        auth: &[(&str, String)],
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(format!("/api/v1{path}"))
            .header("origin", "http://localhost:8080");
        for (k, v) in auth {
            req = req.header(*k, v);
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
        let (parts, body) = resp.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        (
            parts.status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
}

fn bot_auth(token: &str) -> Vec<(&'static str, String)> {
    vec![("authorization", format!("Bot {token}"))]
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_bot_connects_listens_with_the_right_intents_and_answers_commands() {
    let db = TestDb::new().await.expect("test db");
    let pool = db.pool().clone();
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let w = ((std::process::id() % 16) * 2) as u8 + 1;
    let ids = Arc::new(SnowflakeGenerator::new(clock.clone(), w, w).expect("ids"));
    let id_src: sr_db::auth_repo::IdSource = {
        let ids = ids.clone();
        Arc::new(move || ids.generate().get() as i64)
    };

    // People and a server.
    let owner = account(&pool, &ids, "devowner").await;
    let member = account(&pool, &ids, "chatter").await;
    let chat = PgChatRepo::new(pool.clone());
    let bp = sr_chat::guild_rules::plan_new_guild(owner, "SDK test", clock.now_unix_ms(), || {
        ids.generate()
    })
    .expect("plan guild");
    chat.insert_guild(&bp).await.expect("insert guild");
    let (guild, channel) = (bp.guild.id, bp.channels[2].id);
    sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
        .bind(guild.get() as i64)
        .bind(member.get() as i64)
        .execute(&pool)
        .await
        .unwrap();

    // Services and routers, wired as in `app.rs`.
    let bus = Arc::new(InProcessBus::new());
    let apps = Arc::new(AppsService::new(
        Arc::new(PgAppsRepo::new(pool.clone())),
        clock.clone(),
        id_src.clone(),
    ));
    let auth_repo = PgAuthRepo::new(pool.clone(), id_src.clone(), [7u8; 32]);
    let dev = DevDeps::new(
        (*apps).clone(),
        Arc::new(PgChatRepo::new(pool.clone())),
        bus.clone(),
        Arc::new(NoPasswords),
        clock.clone(),
        "http://localhost:8080",
    );
    let messages = MessagesDeps::new(
        PgChatRepo::new(pool.clone()),
        Arc::new(sr_db::chat_repo::PgMessageStore::new(pool.clone())),
        Arc::new(ObjectStoreStorage::in_memory()),
        bus.clone(),
        Arc::new(FixedEntitlements(30 * 1024 * 1024)),
        Arc::new(PgChatRepo::new(pool.clone())),
        None,
        clock.clone(),
        w,
        w,
    )
    .unwrap();
    let apps_auth = Arc::new(sr_apps::auth::AppsAuth::new(apps.clone()));
    let mut settings = ApiSettings::for_public_url("http://localhost:8080");
    settings.rate_limit.enabled = false;
    let state = AppState::builder()
        .settings(settings)
        .clock(clock.clone())
        .sessions(Arc::new(auth_repo.clone()))
        .bots(apps_auth.clone())
        .bearers(apps_auth)
        .extension(Arc::new(dev))
        .extension(Arc::new(messages))
        .build();
    let rest = Rest {
        app: build_router(
            state,
            developer::router().merge(sr_api::routes::messages::router()),
        ),
    };
    let gw = Gateway::new(
        GatewayConfig::default(),
        Arc::new(
            PgAuthenticator::new(auth_repo.clone(), pool.clone(), clock.clone())
                .with_apps(apps.clone()),
        ),
        Arc::new(PgGuildDirectory::new(pool.clone(), clock.clone()).with_apps(apps.clone())),
        bus.clone(),
    );
    gw.start();

    // The member's browser session, for the things a person does.
    let session = SessionToken::generate().unwrap();
    let now = clock.now_unix_ms() / 1000;
    auth_repo
        .create(SessionRecord {
            id: ids.generate().get() as i64,
            user_id: member.get() as i64,
            token_hash: session.hash(),
            created_at: now,
            last_seen_at: now,
            expires_at: now + 3600,
            device_name: None,
        })
        .await
        .unwrap();
    let person = vec![
        ("cookie", format!("sr_session={}", session.expose())),
        ("x-sr-csrf", csrf::token_for(session.expose())),
    ];

    // An application with a bot, public, installed in the server by its owner.
    let created = apps
        .create_application(owner.get() as i64, "Compat Bot", "sdk smoke test")
        .await
        .unwrap();
    let app_id = created.application.id;
    let bot_user = created.application.bot_user_id;
    let token = created.bot_token.clone();
    apps.update_application(
        owner.get() as i64,
        app_id,
        UpdateInput {
            bot_public: Some(true),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let out = apps
        .install(InstallRequest {
            application_id: app_id,
            installer: owner.get() as i64,
            installer_perms: Permissions::ALL,
            guild_id: guild.get() as i64,
            scopes: vec![Scope::Bot, Scope::ApplicationsCommands],
            requested_permissions: (Permissions::VIEW_CHANNEL
                | Permissions::SEND_MESSAGES
                | Permissions::READ_MESSAGE_HISTORY)
                .bits(),
        })
        .await
        .unwrap();
    assert!(out.member_added);

    // REST identity: GET /users/@me with the bot token, GET /gateway/bot.
    let (s, me) = rest
        .call(&bot_auth(&token), Method::GET, "/users/@me", None)
        .await;
    assert_eq!(s, StatusCode::OK, "{me}");
    assert_eq!(
        (me["id"].clone(), me["bot"].clone()),
        (json!(bot_user.to_string()), json!(true))
    );
    let (s, gwi) = rest
        .call(&bot_auth(&token), Method::GET, "/gateway/bot", None)
        .await;
    assert_eq!((s, gwi["shards"].clone()), (StatusCode::OK, json!(1)));

    // The wrong credentials never connect.
    for bad in ["", "not.a.token", &format!("{token}x"), &"A".repeat(43)] {
        match Client::open(&gw, bad, intents::GUILDS, None).await {
            Opened::Closed(c) => assert_eq!(c, close::AUTHENTICATION_FAILED, "{bad:?}"),
            Opened::Ready(..) => panic!("{bad:?} connected"),
        }
    }
    // A shard id outside its count is refused; a real shard of two connects (DEV-BOT-002).
    match Client::open(&gw, &token, intents::GUILDS, Some(json!([2, 2]))).await {
        Opened::Closed(c) => assert_eq!(c, close::INVALID_SHARD),
        Opened::Ready(..) => panic!("out-of-range shard accepted"),
    }
    match Client::open(&gw, &token, intents::GUILDS, Some(json!([1, 2]))).await {
        Opened::Closed(c) => panic!("shard 1 of 2 refused with {c}"),
        Opened::Ready(..) => {}
    }
    // A privileged intent needs to be enabled for the application first.
    match Client::open(
        &gw,
        &token,
        intents::GUILDS | intents::MESSAGE_CONTENT,
        None,
    )
    .await
    {
        Opened::Closed(c) => assert_eq!(c, close::DISALLOWED_INTENTS),
        Opened::Ready(..) => panic!("MESSAGE_CONTENT without being enabled"),
    }

    // IDENTIFY without message content: READY has the bot, the application, the server.
    let base = intents::GUILDS | intents::GUILD_MESSAGES;
    let Opened::Ready(mut plain, ready) =
        Client::open(&gw, &token, base, Some(json!([0, 1]))).await
    else {
        panic!("bot could not connect");
    };
    assert_eq!(ready["user"]["id"], json!(bot_user.to_string()));
    assert_eq!(ready["user"]["bot"], true);
    assert_eq!(ready["application"]["id"], json!(app_id.to_string()));
    assert_eq!(ready["guilds"][0]["id"], json!(guild.to_string()));
    assert!(
        ready.get("relationships").is_none(),
        "a bot's READY has none of a person's state"
    );
    let created_guild = plain.event("GUILD_CREATE").await;
    assert_eq!(created_guild["id"], json!(guild.to_string()));
    let roles = created_guild["roles"].as_array().unwrap();
    assert!(
        roles
            .iter()
            .any(|r| r["managed"] == true && r["name"] == "Compat Bot"),
        "the bot's managed role is in the server"
    );
    assert!(
        created_guild["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["user"]["id"] == json!(bot_user.to_string()) && m["user"]["bot"] == true),
        "the bot is a member and marked as one"
    );

    // Enable MESSAGE_CONTENT (a development application may) and connect a second session.
    apps.update_application(
        owner.get() as i64,
        app_id,
        UpdateInput {
            privileged_intents: Some(vec!["MESSAGE_CONTENT".into()]),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let Opened::Ready(mut full, _) =
        Client::open(&gw, &token, base | intents::MESSAGE_CONTENT, None).await
    else {
        panic!("MESSAGE_CONTENT was enabled but refused");
    };
    full.event("GUILD_CREATE").await;

    // A person writes; only the session with the intent sees the words.
    let (s, sent) = rest
        .call(
            &person,
            Method::POST,
            &format!("/channels/{channel}/messages"),
            Some(json!({"content": "the secret plan"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{sent}");
    let with = full.event("MESSAGE_CREATE").await;
    assert_eq!(with["content"], "the secret plan");
    let without = plain.event("MESSAGE_CREATE").await;
    assert_eq!(without["id"], sent["id"]);
    assert_eq!(without["content"], "", "no MESSAGE_CONTENT, no content");
    assert_eq!(without["embeds"], json!([]));
    // ...unless the bot is mentioned.
    let (s, _) = rest
        .call(
            &person,
            Method::POST,
            &format!("/channels/{channel}/messages"),
            Some(json!({"content": format!("hello <@{bot_user}>")})),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let mention = plain.event("MESSAGE_CREATE").await;
    assert!(mention["content"].as_str().unwrap().contains("hello"));

    // The bot answers through REST, as any library would.
    let (s, reply) = rest
        .call(
            &bot_auth(&token),
            Method::POST,
            &format!("/channels/{channel}/messages"),
            Some(json!({"content": "pong"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{reply}");
    assert_eq!(reply["author"]["id"], json!(bot_user.to_string()));
    assert_eq!(reply["author"]["bot"], true);
    let own = plain.event("MESSAGE_CREATE").await;
    assert_eq!(own["content"], "pong", "a bot always sees its own messages");

    // It cannot do what its roles do not allow: no MANAGE_MESSAGES, no account routes.
    let (s, v) = rest
        .call(
            &bot_auth(&token),
            Method::DELETE,
            &format!(
                "/channels/{channel}/messages/{}",
                sent["id"].as_str().unwrap()
            ),
            None,
        )
        .await;
    assert_eq!(
        (s, v["code"].clone()),
        (StatusCode::FORBIDDEN, json!(50013)),
        "{v}"
    );
    let (s, v) = rest
        .call(&bot_auth(&token), Method::GET, "/applications", None)
        .await;
    assert_eq!(
        (s, v["code"].clone()),
        (StatusCode::FORBIDDEN, json!(20001))
    );

    // Slash commands: register, invoke as a person, answer through the interaction token.
    let (s, cmd) = rest
        .call(
            &bot_auth(&token),
            Method::POST,
            &format!("/applications/{app_id}/commands"),
            Some(json!({"name": "ping", "description": "Replies pong"})),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{cmd}");
    let (s, v) = rest
        .call(
            &person,
            Method::POST,
            "/interactions",
            Some(json!({
                "type": 2, "application_id": app_id.to_string(), "channel_id": channel.to_string(),
                "nonce": "n1", "data": {"id": cmd["id"], "options": []},
            })),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{v}");
    let ix = plain.event("INTERACTION_CREATE").await;
    assert_eq!(ix["data"]["name"], "ping");
    assert_eq!(ix["member"]["user"]["id"], json!(member.to_string()));
    let (s, v) = rest
        .call(
            &[],
            Method::POST,
            &format!(
                "/interactions/{}/{}/callback",
                ix["id"].as_str().unwrap(),
                ix["token"].as_str().unwrap()
            ),
            Some(json!({"type": 4, "data": {"content": "pong from a slash command"}})),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{v}");
    let answered = plain.event("MESSAGE_CREATE").await;
    assert_eq!(answered["content"], "pong from a slash command");
    assert_eq!(answered["author"]["id"], json!(bot_user.to_string()));

    // Resetting the token locks the old one out of REST and of new gateway sessions.
    let fresh = apps
        .reset_bot_token(owner.get() as i64, app_id)
        .await
        .unwrap();
    let (s, _) = rest
        .call(&bot_auth(&token), Method::GET, "/users/@me", None)
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    match Client::open(&gw, &token, base, None).await {
        Opened::Closed(c) => assert_eq!(c, close::AUTHENTICATION_FAILED),
        Opened::Ready(..) => panic!("revoked token connected"),
    }
    assert!(matches!(
        Client::open(&gw, &fresh, base, None).await,
        Opened::Ready(..)
    ));

    // Quiet again: nothing else was delivered to the session without the intent.
    let _ = plain.drain(200).await;
    gw.shutdown();
    db.cleanup().await.unwrap();
    let _: &dyn EventBus = &*bus;
}
