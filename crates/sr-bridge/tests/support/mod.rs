//! Shared helpers of the Postgres tests: accounts, servers, a sealer, an in-memory Discord and a service.
#![allow(dead_code)]

pub mod fake_guild;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use sr_bridge::{
    BridgeError, BridgeNotices, BridgeResult, BridgeService, DiscordApi, MediaUrls, TokenSealer,
    Tuning,
};
use sr_db::accounts::{self, NewAccount};
use sr_db::sealed::SecretBox;
pub use sr_db::{PgPool, testing::TestDb};
use sr_features::{
    BridgeModule, FeatureRegistry, FeatureService, InMemorySettingsStore, PlatformBridge, ScopeRef,
};
use sr_sdk::http::{HttpRequest, HttpResponse, HttpTransport, Method};

static SEQ: AtomicI64 = AtomicI64::new(0);

pub fn uid() -> i64 {
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + SEQ.fetch_add(1, Ordering::Relaxed) % 1000
}

pub async fn account(pool: &PgPool) -> i64 {
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

/// A server owned by `owner` with one text channel.
pub async fn guild(pool: &PgPool, owner: i64) -> (i64, i64) {
    let g = uid();
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'Test guild', $2)")
        .bind(g)
        .bind(owner)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
        .bind(g)
        .bind(owner)
        .execute(pool)
        .await
        .unwrap();
    (g, channel(pool, g, "general").await)
}

pub async fn channel(pool: &PgPool, guild: i64, name: &str) -> i64 {
    channel_of(pool, guild, name, 0).await
}

pub async fn channel_of(pool: &PgPool, guild: i64, name: &str, kind: i16) -> i64 {
    let c = uid();
    sqlx::query("INSERT INTO channels (id, guild_id, type, name) VALUES ($1, $2, $3, $4)")
        .bind(c)
        .bind(guild)
        .bind(kind)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    c
}

pub async fn dm_channel(pool: &PgPool) -> i64 {
    let c = uid();
    sqlx::query("INSERT INTO channels (id, type) VALUES ($1, 1)")
        .bind(c)
        .execute(pool)
        .await
        .unwrap();
    c
}

pub async fn member(pool: &PgPool, guild: i64, user: i64) {
    sqlx::query(
        "INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(guild)
    .bind(user)
    .execute(pool)
    .await
    .unwrap();
}

// ---- the sealer ----

pub struct TestSealer(pub SecretBox);

impl TestSealer {
    pub fn new() -> Arc<Self> {
        Arc::new(Self(SecretBox::new([7u8; 32])))
    }
}

impl TokenSealer for TestSealer {
    fn seal(&self, aad: &[u8], plain: &[u8]) -> BridgeResult<Vec<u8>> {
        self.0.seal(aad, plain).map_err(|_| BridgeError::Seal)
    }
    fn open(&self, aad: &[u8], blob: &[u8]) -> BridgeResult<Vec<u8>> {
        self.0.open(aad, blob).map_err(|_| BridgeError::Seal)
    }
}

// ---- notices and media ----

#[derive(Default)]
pub struct Notices(pub Mutex<Vec<(i64, String)>>);

#[async_trait]
impl BridgeNotices for Notices {
    async fn system_message(&self, channel: i64, text: &str) {
        self.0.lock().unwrap().push((channel, text.to_owned()));
    }
}

pub struct Media;

impl MediaUrls for Media {
    fn signed_url(&self, path: &str, ttl_secs: u64) -> String {
        format!("https://sr.test{path}?ex={ttl_secs}&sig=test")
    }
}

// ---- an in-memory Discord ----

/// One request the fake saw.
#[derive(Debug, Clone)]
pub struct Seen {
    pub method: &'static str,
    pub path: String,
    pub auth: Option<String>,
    pub body: Value,
}

#[derive(Default)]
pub struct State {
    next_id: i64,
    /// token -> (bot user id, is a bot)
    pub tokens: HashMap<String, (i64, bool)>,
    /// (bot token, guild id, name)
    pub guilds: Vec<(String, i64, String)>,
    /// (guild, channel id, name, type)
    pub channels: Vec<(i64, i64, String, i64)>,
    /// webhook id -> (token, channel id)
    pub webhooks: HashMap<i64, (String, i64)>,
    /// (webhook, message id) -> content
    pub messages: HashMap<(i64, i64), Value>,
    pub seen: Vec<Seen>,
    /// Webhook ids whose DELETE always fails with 500.
    pub undeletable: HashSet<i64>,
    /// Next N webhook executions answer 429.
    pub rate_limit_next: u32,
    /// Next N webhook executions answer 500.
    pub fail_next: u32,
    /// After each webhook execution, answer `remaining: 0, reset-after: N` seconds.
    pub exhaust_bucket_secs: Option<f64>,
}

impl State {
    fn fresh(&mut self) -> i64 {
        self.next_id += 1;
        self.next_id
    }
}

/// A fake Discord behind the SDK's transport trait.
pub struct FakeDiscord {
    pub state: Mutex<State>,
}

pub const BASE: &str = "http://fake.discord/api/v10";

impl FakeDiscord {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                next_id: 9_000_000_000_000_000,
                ..State::default()
            }),
        })
    }

    /// A bot token that belongs to a bot, in one Discord server with two text channels and a voice channel.
    /// Returns `(discord guild id, [text channel ids])`.
    pub fn with_bot(self: &Arc<Self>, token: &str) -> (i64, Vec<i64>) {
        let mut s = self.state.lock().unwrap();
        let bot = s.fresh();
        let guild = s.fresh();
        let (c1, c2, v) = (s.fresh(), s.fresh(), s.fresh());
        s.tokens.insert(token.to_owned(), (bot, true));
        s.guilds
            .push((token.to_owned(), guild, "Their server".into()));
        s.channels.push((guild, c1, "general".into(), 0));
        s.channels.push((guild, c2, "random".into(), 0));
        s.channels.push((guild, v, "voice".into(), 2));
        (guild, vec![c1, c2])
    }

    pub fn user_token(self: &Arc<Self>, token: &str) {
        let mut s = self.state.lock().unwrap();
        let id = s.fresh();
        s.tokens.insert(token.to_owned(), (id, false));
    }

    pub fn log(&self) -> Vec<Seen> {
        self.state.lock().unwrap().seen.clone()
    }

    pub fn count(&self, method: &str, path_prefix: &str) -> usize {
        self.log()
            .iter()
            .filter(|s| s.method == method && s.path.starts_with(path_prefix))
            .count()
    }

    pub fn posted(&self) -> Vec<Value> {
        self.log()
            .into_iter()
            .filter(|s| s.method == "POST" && s.path.starts_with("/webhooks/"))
            .map(|s| s.body)
            .collect()
    }

    pub fn webhook_ids(&self) -> Vec<i64> {
        self.state
            .lock()
            .unwrap()
            .webhooks
            .keys()
            .copied()
            .collect()
    }
}

fn reply(status: u16, headers: Vec<(&str, String)>, body: Value) -> HttpResponse {
    HttpResponse {
        status,
        headers: headers
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
        body: if body.is_null() {
            Vec::new()
        } else {
            body.to_string().into_bytes()
        },
    }
}

fn unauthorized() -> HttpResponse {
    reply(
        401,
        vec![],
        json!({"message": "401: Unauthorized", "code": 0}),
    )
}

#[async_trait]
impl HttpTransport for FakeDiscord {
    async fn send(&self, req: HttpRequest) -> sr_sdk::Result<HttpResponse> {
        let rel = req.url.strip_prefix(BASE).unwrap_or(&req.url).to_owned();
        let path = rel.split('?').next().unwrap_or("").to_owned();
        let method = match req.method {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
        };
        let body: Value = req
            .body
            .as_deref()
            .and_then(|b| serde_json::from_slice(b).ok())
            .unwrap_or(Value::Null);
        let auth = req
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("authorization"))
            .map(|(_, v)| v.clone());
        let mut s = self.state.lock().unwrap();
        s.seen.push(Seen {
            method,
            path: path.clone(),
            auth: auth.clone(),
            body: body.clone(),
        });
        let segs: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        let bot = auth
            .as_deref()
            .and_then(|a| a.strip_prefix("Bot "))
            .and_then(|t| s.tokens.get(t).copied());
        let resp = match (method, segs.as_slice()) {
            ("GET", ["users", "@me"]) => match bot {
                Some((id, is_bot)) => reply(
                    200,
                    vec![],
                    json!({"id": id.to_string(), "username": "bridgebot", "bot": is_bot}),
                ),
                None => unauthorized(),
            },
            ("GET", ["users", "@me", "guilds"]) => match bot {
                Some(_) => {
                    let tok = auth
                        .as_deref()
                        .and_then(|a| a.strip_prefix("Bot "))
                        .unwrap_or("");
                    reply(
                        200,
                        vec![],
                        Value::Array(
                            s.guilds
                                .iter()
                                .filter(|(t, _, _)| t == tok)
                                .map(|(_, id, n)| json!({"id": id.to_string(), "name": n}))
                                .collect(),
                        ),
                    )
                }
                None => unauthorized(),
            },
            ("GET", ["guilds", g, "channels"]) => match bot {
                Some(_) => {
                    let g: i64 = g.parse().unwrap_or(0);
                    reply(
                        200,
                        vec![],
                        Value::Array(
                            s.channels
                                .iter()
                                .filter(|c| c.0 == g)
                                .map(|c| json!({"id": c.1.to_string(), "name": c.2, "type": c.3}))
                                .collect(),
                        ),
                    )
                }
                None => unauthorized(),
            },
            ("POST", ["channels", c, "webhooks"]) => match bot {
                Some(_) => {
                    let c: i64 = c.parse().unwrap_or(0);
                    if !s.channels.iter().any(|x| x.1 == c) {
                        reply(404, vec![], json!({"message": "Unknown Channel"}))
                    } else {
                        let id = s.fresh();
                        let token = format!("whtok{id}secret");
                        s.webhooks.insert(id, (token.clone(), c));
                        reply(
                            200,
                            vec![],
                            json!({"id": id.to_string(), "token": token, "channel_id": c.to_string(), "name": body["name"]}),
                        )
                    }
                }
                None => unauthorized(),
            },
            ("POST", ["webhooks", id, token]) => {
                let id: i64 = id.parse().unwrap_or(0);
                match s.webhooks.get(&id) {
                    Some((t, _)) if t == token => {
                        if s.rate_limit_next > 0 {
                            s.rate_limit_next -= 1;
                            reply(
                                429,
                                vec![("Retry-After", "1".into())],
                                json!({"message": "You are being rate limited.", "retry_after": 0.2, "global": false}),
                            )
                        } else if s.fail_next > 0 {
                            s.fail_next -= 1;
                            reply(500, vec![], json!({"message": "boom"}))
                        } else {
                            let mid = s.fresh();
                            s.messages.insert((id, mid), body.clone());
                            let mut h = vec![("X-RateLimit-Remaining", "4".to_owned())];
                            if let Some(secs) = s.exhaust_bucket_secs {
                                h = vec![
                                    ("X-RateLimit-Remaining", "0".to_owned()),
                                    ("X-RateLimit-Reset-After", secs.to_string()),
                                ];
                            }
                            reply(
                                200,
                                h,
                                json!({"id": mid.to_string(), "content": body["content"]}),
                            )
                        }
                    }
                    _ => reply(
                        404,
                        vec![],
                        json!({"message": "Unknown Webhook", "code": 10015}),
                    ),
                }
            }
            ("PATCH", ["webhooks", id, token, "messages", mid]) => {
                let (id, mid): (i64, i64) = (id.parse().unwrap_or(0), mid.parse().unwrap_or(0));
                let ok = s.webhooks.get(&id).is_some_and(|(t, _)| t == token);
                match s.messages.get_mut(&(id, mid)) {
                    Some(m) if ok => {
                        m["content"] = body["content"].clone();
                        reply(200, vec![], json!({"id": mid.to_string()}))
                    }
                    _ => reply(404, vec![], json!({"message": "Unknown Message"})),
                }
            }
            ("DELETE", ["webhooks", id, token, "messages", mid]) => {
                let (id, mid): (i64, i64) = (id.parse().unwrap_or(0), mid.parse().unwrap_or(0));
                let ok = s.webhooks.get(&id).is_some_and(|(t, _)| t == token);
                if ok && s.messages.remove(&(id, mid)).is_some() {
                    reply(204, vec![], Value::Null)
                } else {
                    reply(404, vec![], json!({"message": "Unknown Message"}))
                }
            }
            ("DELETE", ["webhooks", id, token]) => {
                let id: i64 = id.parse().unwrap_or(0);
                if s.undeletable.contains(&id) {
                    reply(500, vec![], json!({"message": "internal error"}))
                } else if s.webhooks.get(&id).is_some_and(|(t, _)| t == token) {
                    s.webhooks.remove(&id);
                    reply(204, vec![], Value::Null)
                } else {
                    reply(404, vec![], json!({"message": "Unknown Webhook"}))
                }
            }
            _ => reply(404, vec![], json!({"message": "Not Found"})),
        };
        Ok(resp)
    }
}

// ---- the service ----

pub struct World {
    pub db: TestDb,
    pub discord: Arc<FakeDiscord>,
    pub notices: Arc<Notices>,
    pub svc: BridgeService,
    pub features: FeatureService,
}

pub const TOKEN: &str = "MTIzNDU2.bot.token-value-123";

/// A service over a fresh database and an in-memory Discord.
pub async fn world() -> World {
    let db = TestDb::new().await.unwrap();
    let discord = FakeDiscord::new();
    let mut reg = FeatureRegistry::new();
    reg.register(BridgeModule).unwrap();
    reg.register(PlatformBridge).unwrap();
    let features = FeatureService::new(Arc::new(reg), Arc::new(InMemorySettingsStore::new()), None);
    let notices = Arc::new(Notices::default());
    let svc = BridgeService::new(
        db.pool().clone(),
        features.clone(),
        Arc::new(uid),
        TestSealer::new(),
        DiscordApi::with_transport(BASE, discord.clone()),
    )
    .with_notices(notices.clone())
    .with_media(Arc::new(Media))
    .with_tuning(Tuning {
        delete_backoff: vec![Duration::from_millis(1), Duration::from_millis(1)],
        max_attempts: 3,
        retry_base: Duration::from_millis(1),
        retry_cap: Duration::from_millis(5),
        batch: 50,
    });
    World {
        db,
        discord,
        notices,
        svc,
        features,
    }
}

pub fn guild_scope(g: i64) -> ScopeRef {
    ScopeRef::Guild {
        guild: sr_core::GuildId::new(g as u64),
    }
}

impl World {
    /// A server with an owner, one text channel, the module on, a connected bot and a bridged channel
    /// (direction `both`). Returns `(guild, owner, sr channel, discord guild, discord channels, map id)`.
    pub async fn bridged(&self) -> Bridged {
        self.bridged_with(TOKEN).await
    }

    pub async fn bridged_with(&self, token: &str) -> Bridged {
        let owner = account(self.db.pool()).await;
        let (g, ch) = guild(self.db.pool(), owner).await;
        let (dg, dch) = self.discord.with_bot(token);
        self.features
            .set_enabled(sr_features::BRIDGE_MODULE_ID, &guild_scope(g), Some(true))
            .await
            .unwrap();
        self.svc.connect(g, owner, token, None).await.unwrap();
        let map = self
            .svc
            .create_map(g, owner, ch, dch[0], sr_bridge::Direction::Both)
            .await
            .unwrap();
        Bridged {
            guild: g,
            owner,
            channel: ch,
            discord_guild: dg,
            discord_channels: dch,
            map: map.id,
        }
    }
}

pub struct Bridged {
    pub guild: i64,
    pub owner: i64,
    pub channel: i64,
    pub discord_guild: i64,
    pub discord_channels: Vec<i64>,
    pub map: i64,
}
