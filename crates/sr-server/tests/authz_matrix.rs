//! RDY-SEC-001, RDY-SEC-002, RDY-SEC-003: the per-route authorization proof.
//!
//! This test boots the real server wiring (`sr_server::app::run`) in this process against a fresh
//! migrated database, seeds a small world and a cast of actors through the repositories and the
//! public API, and then calls **every route of `route_matrix.tsv`** over HTTP as the actors, with
//! placeholder ids taken from the seeded world. The status class each actor must see follows from
//! the AUTHZ, BOT and BEARER columns:
//!
//! * `anon` (no credentials): 401 on every authenticated route.
//! * `outsider` (a member of no server, no staff, no application): refused on every staff,
//!   portal, bot-only and resource-scoped (server, channel, message, application, ...) route.
//! * `fake_staff` (holds a Staff Server role but has no appointment): refused on every staff route.
//! * `bot_in` / `bot_out` (a bot installed in the server / a bot of another application): refused
//!   wherever BOT is `deny`; `bot_out` also on resource-scoped routes of the server.
//! * `bearer_identify` / `bearer_guilds` (OAuth2 tokens): refused wherever the policy accepts no
//!   bearer or wants the other scope.
//! * `owner_b` (owner of a second server and application): the **cross-tenant sweep**, calling every
//!   route that has a tenant id segment with the ids of the other tenant, then with its own
//!   `{guild_id}` and the other tenant's child ids (IDOR), and expecting a refusal.
//! * `member` (plain member) against a private channel the member's overwrites deny.
//!
//! Every denied call must answer 401, 403 or 404 (a 400/422 is reported as *masked*: the request
//! shape was refused before authorization could be seen), and the database must be byte-for-byte
//! unchanged after the denied calls of a group (no write). The groups are proven in the RDY-SEC-003
//! order: moderation, admin, developers and applications, oauth2, submissions, then the rest.
//!
//! Needs Postgres: `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-server --test authz_matrix
//! -- --ignored --test-threads=1 --nocapture`. Set `SR_AUTHZ_REPORT=<file>` for the full
//! observation table (one row per call).

use axum::http::Method;
use serde_json::{Value, json};
use sr_api::{auth::csrf, auth::policy, routes};
use sr_apps::service::{
    AppsService, AuthorizeParams, ClientCredentials, Created, InstallRequest, UpdateInput,
};
use sr_auth::{
    oauth::Scope,
    repo::{SessionRecord, SessionRepository},
    session::SessionToken,
};
use sr_chat::repo::GuildRepository;
use sr_core::{
    Clock, Permissions, PlatformPermissions, SnowflakeGenerator, SystemClock, UserId,
    config::{Config, Secret},
};
use sr_db::{
    accounts::{self, FLAG_SYSTEM, NewAccount, SYSTEM_USER_ID},
    apps_repo::PgAppsRepo,
    auth_repo::PgAuthRepo,
    chat_repo::PgChatRepo,
    testing::TestDb,
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

const MATRIX: &str = include_str!("../../sr-api/tests/route_matrix.tsv");
const KEY_HEX: &str = "0707070707070707070707070707070707070707070707070707070707070707";
/// An id nothing owns.
const BOGUS: &str = "900000000000000001";

/// Weak refusals allowed per group, in the order of [`GROUPS`]. A refusal is weak when it is a 404 next
/// to a 404 for the rightful actor (the seeded id names nothing). The numbers only go down: when a
/// seed improves, lower the number in the same change.
const WEAK_CEILING: [usize; 6] = [0, 27, 12, 0, 8, 79];

/// The groups in RDY-SEC-003 order.
const GROUPS: [&str; 6] = [
    "moderation",
    "admin",
    "developers+applications",
    "oauth2",
    "submissions",
    "rest",
];

/// The platform permission names usable in the PLATFORM column, as `PlatformPermissions` spells
/// them, without the two with special meaning (`STAFF_BASIC`: any appointment; `OWNER`).
fn platform_bits() -> Vec<(&'static str, PlatformPermissions)> {
    PlatformPermissions::all()
        .iter_names()
        .filter(|(n, _)| *n != "OWNER" && *n != "STAFF_BASIC")
        .collect()
}

fn bit_actor_name(bit: &str) -> &'static str {
    Box::leak(format!("staff_{}", bit.to_lowercase()).into_boxed_str())
}

fn pair_actor_name(pair: &str) -> &'static str {
    Box::leak(format!("staff_{}", pair.to_lowercase().replace('+', "_and_")).into_boxed_str())
}

/// The distinct `A+B` values of the PLATFORM column.
fn pair_names(routes: &[Route]) -> Vec<String> {
    let set: BTreeSet<&String> = routes
        .iter()
        .map(|r| &r.platform)
        .filter(|p| p.contains('+'))
        .collect();
    set.into_iter().cloned().collect()
}

/// Platform permissions an actor holds, by name; `None` for actors with no appointment.
fn holds(actor: &str, pairs: &[String]) -> Option<Vec<String>> {
    match actor {
        "staff_owner" => return Some(vec!["OWNER".into()]),
        "staff_basic" => return Some(vec!["STAFF_BASIC".into()]),
        // The Support template: accounts and invites.
        "support" => {
            return Some(vec![
                "STAFF_BASIC".into(),
                "MANAGE_ACCOUNTS".into(),
                "MANAGE_INVITES".into(),
            ]);
        }
        _ => {}
    }
    for (n, _) in platform_bits() {
        if bit_actor_name(n) == actor {
            return Some(vec!["STAFF_BASIC".into(), n.to_owned()]);
        }
    }
    for p in pairs {
        if pair_actor_name(p) == actor {
            let mut v = vec!["STAFF_BASIC".to_owned()];
            v.extend(p.split('+').map(str::to_owned));
            return Some(v);
        }
    }
    None
}

/// Whether a holder of `held` passes a row that demands `platform`: `NAME`, `A+B` (both) or `A|B` (any).
fn admitted(held: &[String], platform: &str) -> bool {
    let has = |p: &str| held.iter().any(|h| h == p);
    held.iter().any(|h| h == "OWNER")
        || if platform.contains('|') {
            platform.split('|').any(has)
        } else {
            platform.split('+').all(has)
        }
}

fn group_of(path: &str) -> usize {
    if path.starts_with("/moderation") {
        0
    } else if path.starts_with("/admin") {
        1
    } else if path.starts_with("/developers") || path.starts_with("/applications") {
        2
    } else if path.starts_with("/oauth2") {
        3
    } else if path.starts_with("/submissions") {
        4
    } else {
        5
    }
}

// ---------------------------------------------------------------------------------------------
// The matrix and the OpenAPI request shapes.

#[derive(Debug, Clone)]
struct Route {
    method: String,
    path: String,
    access: String,
    authz: String,
    bot_denied: bool,
    bearer: String,
    /// Platform permission(s) a `staff` row demands: `NAME`, `A+B` or `-` (D-360).
    platform: String,
}

fn parse_matrix() -> Vec<Route> {
    let mut out = Vec::new();
    for line in MATRIX.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        assert_eq!(f.len(), 7, "malformed matrix row {line:?}");
        out.push(Route {
            method: f[0].to_owned(),
            path: f[1].to_owned(),
            access: f[2].to_owned(),
            authz: f[3].to_owned(),
            bot_denied: f[4] == "deny",
            bearer: f[5].to_owned(),
            platform: f[6].to_owned(),
        });
    }
    out
}

struct OpInfo {
    body: Option<Value>,
    /// Every property filled in, so that the optional (change-making) paths run.
    full: Option<Value>,
    query: String,
}

fn resolve<'a>(schema: &'a Value, comps: &'a Value) -> &'a Value {
    if let Some(r) = schema.get("$ref").and_then(Value::as_str)
        && let Some(name) = r.rsplit('/').next()
        && let Some(s) = comps.get(name)
    {
        return s;
    }
    schema
}

fn sample(schema: &Value, comps: &Value, depth: usize, name: &str, all: bool) -> Value {
    let s = resolve(schema, comps);
    if depth > 5 {
        return Value::Null;
    }
    if let Some(e) = s.get("enum").and_then(Value::as_array)
        && let Some(v) = e.iter().find(|v| !v.is_null())
    {
        // utoipa lists Rust variant names; the wire format of this code base is snake_case.
        if let Some(t) = v.as_str()
            && t.chars().next().is_some_and(char::is_uppercase)
        {
            let mut out = String::new();
            for (i, ch) in t.chars().enumerate() {
                if ch.is_uppercase() && i > 0 {
                    out.push('_');
                }
                out.extend(ch.to_lowercase());
            }
            return json!(out);
        }
        return v.clone();
    }
    for k in ["oneOf", "anyOf"] {
        if let Some(arr) = s.get(k).and_then(Value::as_array) {
            for alt in arr {
                let r = resolve(alt, comps);
                if r.get("type").and_then(Value::as_str) != Some("null") {
                    return sample(alt, comps, depth + 1, name, all);
                }
            }
        }
    }
    if let Some(arr) = s.get("allOf").and_then(Value::as_array)
        && let Some(first) = arr.first()
    {
        return sample(first, comps, depth + 1, name, all);
    }
    if let Some(v) = s.get("example").or_else(|| s.get("default"))
        && !v.is_null()
    {
        return v.clone();
    }
    let ty = match s.get("type") {
        Some(Value::String(t)) => t.clone(),
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(Value::as_str)
            .find(|t| *t != "null")
            .unwrap_or("null")
            .to_owned(),
        _ => {
            if s.get("properties").is_some() {
                "object".to_owned()
            } else {
                "string".to_owned()
            }
        }
    };
    match ty.as_str() {
        "object" => {
            let mut m = serde_json::Map::new();
            let required: Vec<&str> = s
                .get("required")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            if let Some(props) = s.get("properties").and_then(Value::as_object) {
                for (k, v) in props {
                    if required.contains(&k.as_str()) || (all && depth < 2) {
                        m.insert(k.clone(), sample(v, comps, depth + 1, k, all));
                    }
                }
            }
            Value::Object(m)
        }
        "array" => Value::Array(Vec::new()),
        "integer" | "number" => json!(s.get("minimum").and_then(Value::as_i64).unwrap_or(1)),
        "boolean" => json!(false),
        "null" => Value::Null,
        _ => match s.get("format").and_then(Value::as_str) {
            Some("date-time") => json!("2030-01-01T00:00:00Z"),
            Some("date") => json!("2030-01-01"),
            Some("uuid") => json!("00000000-0000-4000-8000-000000000001"),
            Some("uri") => json!("https://example.test/x"),
            Some("email") => json!("x@example.test"),
            _ => {
                if name == "id" || name.ends_with("_id") {
                    json!("1")
                } else {
                    json!("x")
                }
            }
        },
    }
}

fn op_infos() -> HashMap<(String, String), OpInfo> {
    let doc = serde_json::to_value(routes::openapi()).unwrap();
    let comps = doc["components"]["schemas"].clone();
    let mut out = HashMap::new();
    for (path, item) in doc["paths"].as_object().unwrap() {
        for m in ["get", "post", "put", "patch", "delete"] {
            let Some(op) = item.get(m) else { continue };
            let mut body = None;
            let mut full = None;
            if let Some(content) = op
                .pointer("/requestBody/content")
                .and_then(Value::as_object)
                && let Some(js) = content.get("application/json")
                && let Some(schema) = js.get("schema")
            {
                body = Some(sample(schema, &comps, 0, "", false));
                full = Some(sample(schema, &comps, 0, "", true));
            }
            let mut q = Vec::new();
            let params = op
                .get("parameters")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for p in params.iter().chain(
                item.get("parameters")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten(),
            ) {
                if p["in"] == "query" && p["required"] == true {
                    let name = p["name"].as_str().unwrap_or("q");
                    let v = sample(&p["schema"], &comps, 0, name, false);
                    let s = match v {
                        Value::String(s) => s,
                        other => other.to_string(),
                    };
                    q.push(format!("{name}={s}"));
                }
            }
            out.insert(
                (m.to_uppercase(), path.clone()),
                OpInfo {
                    body,
                    full,
                    query: q.join("&"),
                },
            );
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// HTTP over a raw socket (no client dependency).

#[derive(Debug, Clone)]
struct Resp {
    status: u16,
    body: String,
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn dechunk(mut b: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(i) = find(b, b"\r\n") {
        let size = std::str::from_utf8(&b[..i])
            .ok()
            .and_then(|s| usize::from_str_radix(s.split(';').next().unwrap_or("0").trim(), 16).ok())
            .unwrap_or(0);
        b = &b[i + 2..];
        if size == 0 {
            break;
        }
        let take = size.min(b.len());
        out.extend_from_slice(&b[..take]);
        b = &b[take..];
        if b.starts_with(b"\r\n") {
            b = &b[2..];
        }
    }
    out
}

async fn raw(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(String, String)],
    body: Option<&str>,
) -> Resp {
    let fut = async {
        let mut s = TcpStream::connect(("127.0.0.1", port)).await.ok()?;
        let mut req = format!(
            "{method} {path} HTTP/1.1\r\nHost: localhost:{port}\r\nConnection: close\r\nAccept: application/json\r\n"
        );
        for (k, v) in headers {
            req.push_str(&format!("{k}: {v}\r\n"));
        }
        match body {
            Some(b) => {
                req.push_str("Content-Type: application/json\r\n");
                req.push_str(&format!("Content-Length: {}\r\n\r\n{b}", b.len()));
            }
            None => {
                if method != "GET" && method != "HEAD" {
                    req.push_str("Content-Length: 0\r\n");
                }
                req.push_str("\r\n");
            }
        }
        s.write_all(req.as_bytes()).await.ok()?;
        let mut buf = Vec::new();
        s.read_to_end(&mut buf).await.ok()?;
        let split = find(&buf, b"\r\n\r\n")?;
        let head = String::from_utf8_lossy(&buf[..split]).to_string();
        let status: u16 = head.split_whitespace().nth(1)?.parse().ok()?;
        let chunked = head.to_lowercase().contains("transfer-encoding: chunked");
        let body_bytes = &buf[split + 4..];
        let body = if chunked {
            dechunk(body_bytes)
        } else {
            body_bytes.to_vec()
        };
        Some(Resp {
            status,
            body: String::from_utf8_lossy(&body).to_string(),
        })
    };
    match tokio::time::timeout(Duration::from_secs(20), fut).await {
        Ok(Some(r)) => r,
        _ => Resp {
            status: 0,
            body: "timeout or connection failure".into(),
        },
    }
}

// ---------------------------------------------------------------------------------------------
// The cast.

#[derive(Debug, Clone)]
enum Cred {
    Anon,
    Session(String),
    Bot(String),
    Bearer(String),
}

#[derive(Debug, Clone)]
struct Actor {
    name: &'static str,
    cred: Cred,
}

struct Server {
    port: u16,
    public: String,
}

impl Server {
    fn headers(&self, cred: &Cred, method: &str) -> Vec<(String, String)> {
        let mut h = vec![("Origin".to_owned(), self.public.clone())];
        match cred {
            Cred::Anon => {}
            Cred::Session(t) => {
                h.push(("Cookie".into(), format!("sr_session={t}")));
                if !matches!(method, "GET" | "HEAD" | "OPTIONS") {
                    h.push(("X-SR-CSRF".into(), csrf::token_for(t)));
                }
            }
            Cred::Bot(t) => h.push(("Authorization".into(), format!("Bot {t}"))),
            Cred::Bearer(t) => h.push(("Authorization".into(), format!("Bearer {t}"))),
        }
        h
    }

    async fn call(&self, cred: &Cred, method: &str, path: &str, body: Option<&Value>) -> Resp {
        let b = body.map(Value::to_string);
        raw(
            self.port,
            method,
            &format!("/api/v1{path}"),
            &self.headers(cred, method),
            b.as_deref(),
        )
        .await
    }

    /// A seeding call that must succeed; returns the JSON body.
    async fn must(&self, cred: &Cred, method: &str, path: &str, body: Value) -> Value {
        let r = self.call(cred, method, path, Some(&body)).await;
        assert!(
            (200..300).contains(&r.status),
            "seed call {method} {path} -> {} {}",
            r.status,
            r.body
        );
        serde_json::from_str(&r.body).unwrap_or(Value::Null)
    }
}

type Ids = SnowflakeGenerator<Arc<dyn Clock>>;

async fn account(pool: &sqlx::PgPool, ids: &Ids, name: &str) -> i64 {
    let id = ids.generate().get() as i64;
    let ok = accounts::create_account(
        pool,
        &NewAccount {
            id,
            person_id: ids.generate().get() as i64,
            username: name,
            display_name: None,
            email: &format!("{name}@example.test"),
            password_hash: "x",
            state: "active",
            email_verified: true,
            initial_credits: 0,
            flags: 0,
        },
        ids.generate().get() as i64,
    )
    .await
    .unwrap();
    assert!(ok, "account {name}");
    id
}

async fn session_for(auth: &PgAuthRepo, ids: &Ids, user: i64) -> String {
    let t = SessionToken::generate().unwrap();
    let now = SystemClock.now_unix_ms() / 1000;
    auth.create(SessionRecord {
        id: ids.generate().get() as i64,
        user_id: user,
        token_hash: t.hash(),
        created_at: now,
        last_seen_at: now,
        expires_at: now + 6 * 3600,
        device_name: None,
    })
    .await
    .unwrap();
    t.expose().to_owned()
}

/// Everything the routes are called with.
struct World {
    srv: Server,
    pool: sqlx::PgPool,
    actors: Vec<Actor>,
    /// Placeholder values of tenant A (the server the cast is part of).
    a: HashMap<&'static str, String>,
    /// Placeholder values of tenant B (owned by `owner_b`).
    b: HashMap<&'static str, String>,
    /// `a`, with the channel and message replaced by those of the private channel.
    private: HashMap<&'static str, String>,
    member_id: i64,
    guild_a: i64,
}

impl World {
    fn actor(&self, name: &str) -> &Actor {
        self.actors.iter().find(|a| a.name == name).unwrap()
    }
}

fn permissions_all_but_admin() -> String {
    (Permissions::ALL - Permissions::ADMINISTRATOR)
        .bits()
        .to_string()
}

async fn add_member(pool: &sqlx::PgPool, guild: i64, user: i64) {
    sqlx::query(
        "INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(guild)
    .bind(user)
    .execute(pool)
    .await
    .unwrap();
}

async fn give_role(pool: &sqlx::PgPool, guild: i64, user: i64, role: i64) {
    sqlx::query(
        "INSERT INTO member_roles (guild_id, user_id, role_id) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(guild)
    .bind(user)
    .bind(role)
    .execute(pool)
    .await
    .unwrap();
}

fn id_of(v: &Value) -> String {
    v["id"].as_str().map(str::to_owned).unwrap_or_else(|| {
        panic!("no id in {v}");
    })
}

type ServerTask = tokio::task::JoinHandle<Result<(), String>>;

/// Create an object through the API with the OpenAPI-sampled body (or `body`), returning its JSON.
async fn create(
    srv: &Server,
    cred: &Cred,
    method: &str,
    template: &str,
    path: &str,
    infos: &HashMap<(String, String), OpInfo>,
    body: Option<Value>,
) -> Option<Value> {
    let body = body
        .or_else(|| {
            infos
                .get(&(method.to_owned(), template.to_owned()))
                .and_then(|i| i.body.clone())
        })
        .unwrap_or_else(|| json!({}));
    let r = srv.call(cred, method, path, Some(&body)).await;
    if (200..300).contains(&r.status) {
        let v: Option<Value> = serde_json::from_str(&r.body).ok();
        if v.as_ref().and_then(|v| v.get("id")).is_none() {
            println!(
                "authz_matrix: seed {method} {template} answered without an id: {}",
                r.body.chars().take(120).collect::<String>()
            );
        }
        v
    } else {
        println!(
            "authz_matrix: seed {method} {template} -> {} {}",
            r.status,
            r.body.chars().take(100).collect::<String>()
        );
        None
    }
}

async fn seed(
    db: &TestDb,
    url: &str,
    infos: &HashMap<(String, String), OpInfo>,
    pairs: &[String],
) -> (World, tokio::sync::oneshot::Sender<()>, ServerTask) {
    let pool = db.pool().clone();
    sr_db::migrate(&pool).await.expect("migrate");
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let ids: Ids = SnowflakeGenerator::new(clock.clone(), 9, 9).expect("ids");
    let id_src: sr_db::auth_repo::IdSource = {
        let g = Arc::new(SnowflakeGenerator::new(clock.clone(), 9, 10).expect("ids"));
        Arc::new(move || g.generate().get() as i64)
    };
    let key = sr_db::auth_repo::parse_secret_key(KEY_HEX).unwrap();
    let auth = PgAuthRepo::new(pool.clone(), id_src.clone(), key);

    // The system user, as `sr-cli bootstrap` creates it.
    accounts::create_account(
        &pool,
        &NewAccount {
            id: SYSTEM_USER_ID,
            person_id: SYSTEM_USER_ID,
            username: "sideroom",
            display_name: Some("SIDE ROOM"),
            email: "system@sideroom.invalid",
            password_hash: "!",
            state: "deactivated",
            email_verified: true,
            initial_credits: 0,
            flags: FLAG_SYSTEM,
        },
        ids.generate().get() as i64,
    )
    .await
    .unwrap();

    let mut users: BTreeMap<&'static str, i64> = BTreeMap::new();
    for n in [
        "ownera",
        "ownerb",
        "member",
        "insider",
        "mod",
        "outsider",
        "victim",
        "staffowner",
        "support",
        "fakestaff",
        "appowner",
        "staffbasic",
        "friendtwo",
    ] {
        users.insert(n, account(&pool, &ids, n).await);
    }
    let u = |n: &str| users[n];

    // Servers A and B and the Staff Server.
    let chat = PgChatRepo::new(pool.clone());
    let plan = |owner: i64, name: &str| {
        sr_chat::guild_rules::plan_new_guild(
            UserId::new(owner as u64),
            name,
            clock.now_unix_ms(),
            || ids.generate(),
        )
        .expect("plan guild")
    };
    let bp_a = plan(u("ownera"), "Alpha");
    let bp_b = plan(u("ownerb"), "Bravo");
    let mut bp_s = plan(u("staffowner"), "Staff");
    bp_s.guild.is_staff_guild = true;
    for bp in [&bp_a, &bp_b, &bp_s] {
        chat.insert_guild(bp).await.expect("insert guild");
    }
    let (ga, gb, gs) = (
        bp_a.guild.id.get() as i64,
        bp_b.guild.id.get() as i64,
        bp_s.guild.id.get() as i64,
    );
    let (chan_a, chan_b) = (
        bp_a.channels[2].id.get() as i64,
        bp_b.channels[2].id.get() as i64,
    );
    for n in ["member", "insider", "mod", "victim"] {
        add_member(&pool, ga, u(n)).await;
    }

    // Staff: roles from the templates; the Owner holds the Owner role and an appointment; Support
    // is appointed with the Support role; fakestaff has a role and no appointment.
    let templates: Vec<(String, i32, i32, i64)> = sqlx::query_as(
        "SELECT name, position, color, platform_permissions FROM staff_role_templates ORDER BY position",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let mut staff_roles = HashMap::new();
    for (name, position, color, perms) in templates {
        let id = ids.generate().get() as i64;
        sqlx::query(
            "INSERT INTO roles (id, guild_id, name, color, position, platform_permissions, hoist, mentionable) \
             VALUES ($1,$2,$3,$4,$5,$6,true,true)",
        )
        .bind(id)
        .bind(gs)
        .bind(&name)
        .bind(color)
        .bind(position)
        .bind(perms)
        .execute(&pool)
        .await
        .unwrap();
        staff_roles.insert(name, id);
    }
    // A staff role with the basic bit only: appointed staff who hold no section permission.
    let basic_role = ids.generate().get() as i64;
    sqlx::query(
        "INSERT INTO roles (id, guild_id, name, color, position, platform_permissions, hoist, mentionable) \n         VALUES ($1,$2,'Basic only',0,8,1,true,true)",
    )
    .bind(basic_role)
    .bind(gs)
    .execute(&pool)
    .await
    .unwrap();
    staff_roles.insert("Basic only".to_owned(), basic_role);
    for (user, role, appointed) in [
        ("staffbasic", "Basic only", true),
        ("staffowner", "Owner", true),
        ("support", "Support", true),
        ("fakestaff", "Owner", false),
    ] {
        add_member(&pool, gs, u(user)).await;
        give_role(&pool, gs, u(user), staff_roles[role]).await;
        if appointed {
            sqlx::query(
                "INSERT INTO staff_appointments (id, account_id, appointed_by, reason) VALUES ($1,$2,$2,'authz matrix') ON CONFLICT DO NOTHING",
            )
            .bind(ids.generate().get() as i64)
            .bind(u(user))
            .execute(&pool)
            .await
            .unwrap();
        }
    }

    // One staff actor per platform bit (appointment with STAFF_BASIC plus that bit), and one per
    // `A+B` pair named in the PLATFORM column (D-360).
    let mut extra: Vec<(&'static str, i64)> = Vec::new();
    let mut position = 20;
    let mut combos: Vec<(&'static str, Vec<String>)> = platform_bits()
        .into_iter()
        .map(|(n, _)| (bit_actor_name(n), vec![n.to_owned()]))
        .collect();
    for p in pairs {
        combos.push((
            pair_actor_name(p),
            p.split('+').map(str::to_owned).collect(),
        ));
    }
    for (name, bits) in combos {
        let mut perms = PlatformPermissions::STAFF_BASIC;
        for b in &bits {
            perms |= PlatformPermissions::from_name(b).expect("platform bit name");
        }
        position += 1;
        let uid = account(&pool, &ids, &format!("sb{position}")).await;
        let role = ids.generate().get() as i64;
        position += 1;
        sqlx::query(
            "INSERT INTO roles (id, guild_id, name, color, position, platform_permissions, hoist, mentionable) \
             VALUES ($1,$2,$3,0,$4,$5,true,true)",
        )
        .bind(role)
        .bind(gs)
        .bind(name)
        .bind(position)
        .bind(perms.bits() as i64)
        .execute(&pool)
        .await
        .unwrap();
        add_member(&pool, gs, uid).await;
        give_role(&pool, gs, uid, role).await;
        sqlx::query(
            "INSERT INTO staff_appointments (id, account_id, appointed_by, reason) VALUES ($1,$2,$2,'authz matrix')",
        )
        .bind(ids.generate().get() as i64)
        .bind(uid)
        .execute(&pool)
        .await
        .unwrap();
        extra.push((name, uid));
    }

    // Sessions.
    let mut actors = vec![Actor {
        name: "anon",
        cred: Cred::Anon,
    }];
    let mut sess: HashMap<&str, String> = HashMap::new();
    for (actor, user) in [
        ("outsider", "outsider"),
        ("member", "member"),
        ("insider", "insider"),
        ("mod", "mod"),
        ("owner_a", "ownera"),
        ("owner_b", "ownerb"),
        ("staff_owner", "staffowner"),
        ("staff_basic", "staffbasic"),
        ("support", "support"),
        ("fake_staff", "fakestaff"),
        ("app_owner", "appowner"),
    ] {
        let t = session_for(&auth, &ids, u(user)).await;
        sess.insert(actor, t.clone());
        actors.push(Actor {
            name: actor,
            cred: Cred::Session(t),
        });
    }
    for (name, uid) in extra {
        let t = session_for(&auth, &ids, uid).await;
        actors.push(Actor {
            name,
            cred: Cred::Session(t),
        });
    }

    // Applications: A (owned by app_owner, bot installed in server A), B (owned by owner_b).
    let apps = Arc::new(AppsService::new(
        Arc::new(PgAppsRepo::new(
            pool.clone(),
            sr_db::sealed::SecretBox::new(key),
        )),
        clock.clone(),
        id_src.clone(),
    ));
    let app_a: Created = apps
        .create_application(u("appowner"), "Alpha App", "authz matrix")
        .await
        .unwrap();
    let app_b: Created = apps
        .create_application(u("ownerb"), "Bravo App", "authz matrix")
        .await
        .unwrap();
    for app in [&app_a, &app_b] {
        apps.update_application(
            if std::ptr::eq(app, &app_a) {
                u("appowner")
            } else {
                u("ownerb")
            },
            app.application.id,
            UpdateInput {
                bot_public: Some(true),
                redirect_uris: Some(vec!["https://client.test/cb".into()]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }
    apps.install(InstallRequest {
        application_id: app_a.application.id,
        installer: u("appowner"),
        installer_perms: Permissions::ALL,
        guild_id: ga,
        scopes: vec![Scope::Bot, Scope::ApplicationsCommands],
        requested_permissions: (Permissions::VIEW_CHANNEL
            | Permissions::SEND_MESSAGES
            | Permissions::READ_MESSAGE_HISTORY)
            .bits(),
    })
    .await
    .unwrap();
    let bot_perms: Vec<(String, i64)> = sqlx::query_as(
        "SELECT r.name, r.permissions FROM roles r JOIN member_roles mr ON mr.role_id = r.id WHERE mr.user_id = $1",
    )
    .bind(app_a.application.bot_user_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    println!("authz_matrix: bot_in roles {bot_perms:?}");
    actors.push(Actor {
        name: "bot_in",
        cred: Cred::Bot(app_a.bot_token.clone()),
    });
    actors.push(Actor {
        name: "bot_out",
        cred: Cred::Bot(app_b.bot_token.clone()),
    });
    // Bearer tokens of `member` through application A.
    let creds = ClientCredentials {
        client_id: app_a.application.id.to_string(),
        secret: Some(app_a.client_secret.clone()),
    };
    for (actor, scope) in [("bearer_identify", "identify"), ("bearer_guilds", "guilds")] {
        let v = apps
            .validate_authorize(&AuthorizeParams {
                client_id: app_a.application.id.to_string(),
                scope: scope.into(),
                redirect_uri: Some("https://client.test/cb".into()),
                response_type: Some("code".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        let code = apps.issue_code(u("member"), &v).await.unwrap();
        let t = apps
            .exchange_code(&creds, &code, "https://client.test/cb", None)
            .await
            .unwrap();
        actors.push(Actor {
            name: actor,
            cred: Cred::Bearer(t.access_token),
        });
    }

    // Lift the HTTP rate limits so the sweep is not answered with 429 (the limiter is proven elsewhere).
    for key in [
        "limits.rate.ip",
        "limits.rate.session",
        "limits.rate.bot",
        "limits.rate.messages",
        "limits.rate.uploads",
    ] {
        sqlx::query("INSERT INTO platform_settings (key, value, updated_by) VALUES ($1, $2::text::jsonb, $3) ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value")
            .bind(key)
            .bind(if key.ends_with("messages") { "600" } else if key.ends_with("uploads") { "6000" } else { "60000" })
            .bind(SYSTEM_USER_ID)
            .execute(&pool)
            .await
            .unwrap();
    }
    // The server process.
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let mut cfg = Config::default();
    cfg.server.bind = format!("127.0.0.1:{port}");
    cfg.server.metrics_bind = "127.0.0.1:0".into();
    cfg.server.public_url = format!("http://localhost:{port}");
    cfg.server.secret_key = Secret::new(KEY_HEX);
    cfg.server.worker_id = 11;
    cfg.server.process_id = 11;
    cfg.database.url = Secret::new(url);
    cfg.database.max_connections = 12;
    cfg.storage.endpoint = "memory:".into();
    cfg.activities.enabled = true;
    cfg.server.shutdown_grace_secs = 1;
    static SEARCH_N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let search = std::env::temp_dir().join(format!(
        "sr_authz_search_{}_{}",
        std::process::id(),
        SEARCH_N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    cfg.search.dir = search.to_string_lossy().into_owned();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        sr_server::app::run(cfg, async {
            let _ = rx.await;
        })
        .await
        .map_err(|e| format!("{e:#}"))
    });
    let srv = Server {
        port,
        public: format!("http://localhost:{port}"),
    };
    let mut up = false;
    for _ in 0..240 {
        if handle.is_finished() {
            panic!("the server stopped while starting: {:?}", handle.await);
        }
        if raw(port, "GET", "/healthz", &[], None).await.status == 200 {
            up = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert!(up, "the server did not come up");

    // The world, through the API as the server owners.
    let oa = Cred::Session(sess["owner_a"].clone());
    let ob = Cred::Session(sess["owner_b"].clone());
    let mod_role = srv
        .must(
            &oa,
            "POST",
            &format!("/guilds/{ga}/roles"),
            json!({"name": "Mods", "permissions": permissions_all_but_admin()}),
        )
        .await;
    let mod_role = id_of(&mod_role);
    let insider_role = id_of(
        &srv.must(
            &oa,
            "POST",
            &format!("/guilds/{ga}/roles"),
            json!({"name": "Insiders", "permissions": "0"}),
        )
        .await,
    );
    give_role(&pool, ga, u("mod"), mod_role.parse().unwrap()).await;
    give_role(&pool, ga, u("insider"), insider_role.parse().unwrap()).await;

    // A private channel: @everyone may not view it, Insiders may.
    let priv_chan = id_of(
        &srv.must(
            &oa,
            "POST",
            &format!("/guilds/{ga}/channels"),
            json!({"name": "private", "type": 0}),
        )
        .await,
    );
    srv.must(
        &oa,
        "PUT",
        &format!("/channels/{priv_chan}/permissions/{ga}"),
        json!({"type": 0, "allow": "0", "deny": "1024"}),
    )
    .await;
    srv.must(
        &oa,
        "PUT",
        &format!("/channels/{priv_chan}/permissions/{insider_role}"),
        json!({"type": 0, "allow": "1024", "deny": "0"}),
    )
    .await;
    let msg_a = id_of(
        &srv.must(
            &oa,
            "POST",
            &format!("/channels/{chan_a}/messages"),
            json!({"content": "hello alpha"}),
        )
        .await,
    );
    let msg_priv = id_of(
        &srv.must(
            &oa,
            "POST",
            &format!("/channels/{priv_chan}/messages"),
            json!({"content": "secret alpha"}),
        )
        .await,
    );
    let msg_b = id_of(
        &srv.must(
            &ob,
            "POST",
            &format!("/channels/{chan_b}/messages"),
            json!({"content": "hello bravo"}),
        )
        .await,
    );
    let hook_a = srv
        .must(
            &oa,
            "POST",
            &format!("/channels/{chan_a}/webhooks"),
            json!({"name": "hook"}),
        )
        .await;
    let hook_b = srv
        .must(
            &ob,
            "POST",
            &format!("/channels/{chan_b}/webhooks"),
            json!({"name": "hook"}),
        )
        .await;
    let invite_a = srv
        .call(
            &oa,
            "POST",
            &format!("/channels/{chan_a}/invites"),
            Some(&json!({})),
        )
        .await;
    let invite_a: Value = serde_json::from_str(&invite_a.body).unwrap_or(Value::Null);
    let invite_b = srv
        .call(
            &ob,
            "POST",
            &format!("/channels/{chan_b}/invites"),
            Some(&json!({})),
        )
        .await;
    let invite_b: Value = serde_json::from_str(&invite_b.body).unwrap_or(Value::Null);
    let role_b = id_of(
        &srv.must(
            &ob,
            "POST",
            &format!("/guilds/{gb}/roles"),
            json!({"name": "Bravo mods", "permissions": "0"}),
        )
        .await,
    );
    let thread_a = srv
        .call(
            &oa,
            "POST",
            &format!("/channels/{chan_a}/messages/{msg_a}/threads"),
            Some(&json!({"name": "thread"})),
        )
        .await;
    let thread_a: Value = serde_json::from_str(&thread_a.body).unwrap_or(Value::Null);

    let victim = u("victim").to_string();
    let mut a: HashMap<&'static str, String> = HashMap::new();
    let mut b: HashMap<&'static str, String> = HashMap::new();
    a.insert("guild_id", ga.to_string());
    a.insert("channel_id", chan_a.to_string());
    a.insert("message_id", msg_a);
    a.insert("role_id", mod_role);
    a.insert("application_id", app_a.application.id.to_string());
    a.insert(
        "webhook_id",
        hook_a["id"].as_str().unwrap_or(BOGUS).to_owned(),
    );
    a.insert("token", hook_a["token"].as_str().unwrap_or("x").to_owned());
    a.insert("code", invite_a["code"].as_str().unwrap_or("x").to_owned());
    a.insert("user_id", victim.clone());
    a.insert("account_id", victim.clone());
    a.insert("person_id", victim.clone());
    a.insert(
        "thread_id",
        thread_a["id"].as_str().unwrap_or(BOGUS).to_owned(),
    );
    a.insert("overwrite_id", insider_role.clone());
    b.insert("guild_id", gb.to_string());
    b.insert("channel_id", chan_b.to_string());
    b.insert("message_id", msg_b);
    b.insert("role_id", role_b);
    b.insert("application_id", app_b.application.id.to_string());
    b.insert(
        "webhook_id",
        hook_b["id"].as_str().unwrap_or(BOGUS).to_owned(),
    );
    b.insert("token", hook_b["token"].as_str().unwrap_or("x").to_owned());
    b.insert("code", invite_b["code"].as_str().unwrap_or("x").to_owned());
    b.insert("user_id", u("ownerb").to_string());
    b.insert("account_id", u("ownerb").to_string());
    b.insert("person_id", u("ownerb").to_string());
    // More objects, so that a refusal is not just "no such id": a moderation case, a takedown notice,
    // a hold, an announcement, a submission, a command, a grant and a running stage.
    let staff = Cred::Session(sess["staff_owner"].clone());
    let member_cred = Cred::Session(sess["member"].clone());
    let bot_cred = Cred::Bot(app_a.bot_token.clone());
    let person: i64 = sqlx::query_scalar("SELECT person_id FROM accounts WHERE id = $1")
        .bind(u("victim"))
        .fetch_one(&pool)
        .await
        .unwrap();
    a.insert("person_id", person.to_string());
    if let Some(v) = create(
        &srv,
        &staff,
        "POST",
        "/moderation/cases",
        "/moderation/cases",
        infos,
        Some(json!({"category": "spam", "target_kind": "other", "guild_id": ga.to_string()})),
    )
    .await
        && let Some(case) = v["id"].as_str()
    {
        a.insert("case_id", case.to_owned());
        if let Some(h) = create(
            &srv,
            &staff,
            "POST",
            "/moderation/cases/{case_id}/holds",
            &format!("/moderation/cases/{case}/holds"),
            infos,
            Some(json!({"kind": "message", "id": a["message_id"], "reason": "authz matrix"})),
        )
        .await
            && let Some(hold) = h["hold_id"].as_str()
        {
            a.insert("hold_id", hold.to_owned());
        }
    }
    if let Some(v) = create(
        &srv,
        &staff,
        "POST",
        "/moderation/takedowns",
        "/moderation/takedowns",
        infos,
        None,
    )
    .await
        && let Some(n) = v["id"].as_str()
    {
        a.insert("notice_id", n.to_owned());
    }
    if let Some(v) = create(
        &srv,
        &staff,
        "POST",
        "/admin/announcements",
        "/admin/announcements",
        infos,
        None,
    )
    .await
        && let Some(n) = v["id"].as_str()
    {
        a.insert("announcement_id", n.to_owned());
    }
    if let Some(v) = create(
        &srv,
        &member_cred,
        "POST",
        "/submissions",
        "/submissions",
        infos,
        Some(json!({"kind": "library.series", "payload": {"title": "Authz series", "kind": "manga"}, "title": "Authz series", "summary": "authz matrix"})),
    )
    .await
        && let Some(n) = v["submission"]["id"].as_str()
    {
        a.insert("submission_id", n.to_owned());
    }
    if let Some(v) = create(
        &srv,
        &bot_cred,
        "POST",
        "/applications/{application_id}/commands",
        &format!("/applications/{}/commands", app_a.application.id),
        infos,
        Some(json!({"name": "ping", "description": "ping"})),
    )
    .await
        && let Some(n) = v["id"].as_str()
    {
        a.insert("command_id", n.to_owned());
    }
    if let Ok(g) = sqlx::query_scalar::<_, i64>("SELECT id FROM oauth2_grants ORDER BY id LIMIT 1")
        .fetch_one(&pool)
        .await
    {
        a.insert("grant_id", g.to_string());
    }
    // Every optional module on, in both servers, so that their routes are reached past the feature gate.
    for (who, g) in [(&oa, ga), (&ob, gb)] {
        let list = srv
            .call(who, "GET", &format!("/guilds/{g}/sr/features"), None)
            .await;
        let list: Value = serde_json::from_str(&list.body).unwrap_or(Value::Null);
        for f in list.as_array().into_iter().flatten() {
            if let Some(fid) = f["id"].as_str() {
                srv.call(
                    who,
                    "PATCH",
                    &format!("/guilds/{g}/sr/features/{fid}"),
                    Some(&json!({"enabled": true})),
                )
                .await;
            }
        }
    }
    // Children of the server, for the cross-tenant sweep.
    let png = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgYGD4DwABBAEAHDlN5QAAAABJRU5ErkJggg==";
    let emoji_body = json!({"name": "authzemoji", "image": png});
    // The modules that post into a channel need one configured.
    for f in [
        "community.suggestions",
        "community.tickets",
        "community.rss",
    ] {
        srv.call(
            &oa,
            "PATCH",
            &format!("/guilds/{ga}/sr/features/{f}"),
            Some(&json!({"enabled": true, "settings": {"channel": chan_a.to_string()}})),
        )
        .await;
    }
    let recipes: [(&'static str, &str, &Cred, Option<Value>); 7] = [
        (
            "event_id",
            "/guilds/{guild_id}/scheduled-events",
            &oa,
            Some(
                json!({"name": "authz", "entity_type": 3, "privacy_level": 2, "scheduled_start_time": "2031-01-01T00:00:00+00:00", "scheduled_end_time": "2031-01-01T02:00:00+00:00", "entity_metadata": {"location": "here"}}),
            ),
        ),
        (
            "rule_id",
            "/guilds/{guild_id}/auto-moderation/rules",
            &oa,
            Some(
                json!({"name": "authz", "event_type": 1, "trigger_type": 3, "enabled": false, "actions": [{"type": 1}]}),
            ),
        ),
        (
            "emoji_id",
            "/guilds/{guild_id}/emojis",
            &oa,
            Some(emoji_body),
        ),
        (
            "suggestion_id",
            "/guilds/{guild_id}/suggestions",
            &member_cred,
            Some(json!({"content": "authz suggestion"})),
        ),
        (
            "ticket_id",
            "/guilds/{guild_id}/tickets",
            &member_cred,
            Some(json!({"subject": "authz ticket"})),
        ),
        (
            "template_id",
            "/guilds/{guild_id}/poll-templates",
            &oa,
            Some(json!({"name": "authz", "question": "q", "answers": ["a", "b"]})),
        ),
        (
            "feed_id",
            "/guilds/{guild_id}/rss-feeds",
            &oa,
            Some(json!({"channel_id": chan_a.to_string(), "url": "https://example.test/feed.xml"})),
        ),
    ];
    for (param, tpl, who, body) in recipes {
        let path = fill(tpl, &a);
        if let Some(v) = create(&srv, who, "POST", tpl, &path, infos, body).await
            && let Some(id) = v["id"].as_str()
        {
            a.insert(param, id.to_owned());
        }
    }

    // R-03: more ids, from the world's own API. Each is stored under the placeholder the routes use.
    let appowner_cred = Cred::Session(sess["app_owner"].clone());
    let friend2 = u("friendtwo").to_string();
    for (x, y) in [(u("member"), u("victim")), (u("member"), u("friendtwo"))] {
        for (a_, b_) in [(x, y), (y, x)] {
            sqlx::query("INSERT INTO relationships (user_id, target_id, type) VALUES ($1, $2, 1)")
                .bind(a_)
                .bind(b_)
                .execute(&pool)
                .await
                .unwrap();
        }
    }
    let aid = app_a.application.id;
    #[allow(clippy::type_complexity)]
    let more: Vec<(&'static str, String, &Cred, Option<Value>)> = vec![
        (
            "dm_channel_id",
            "/users/@me/channels".into(),
            &member_cred,
            Some(json!({"recipient_id": victim})),
        ),
        (
            "group_channel_id",
            "/users/@me/channels".into(),
            &member_cred,
            Some(json!({"recipients": [victim, friend2]})),
        ),
        ("tv_channel_id", "/admin/tv/channels".into(), &staff, None),
        (
            "request_id",
            "/requests".into(),
            &member_cred,
            Some(json!({"section": "library", "title": "authz request"})),
        ),
        ("series_id", "/admin/library/series".into(), &staff, None),
        ("title_id", "/admin/cinema/titles".into(), &staff, None),
        (
            "chapter_id",
            format!("/submissions/{}/library/chapters", a["submission_id"]),
            &member_cred,
            Some(json!({"number": 1})),
        ),
        (
            "asset_id",
            format!("/applications/{aid}/assets"),
            &appowner_cred,
            Some(json!({"name": "authz", "image": png, "type": 1})),
        ),
        (
            "version_id",
            format!("/applications/{aid}/versions"),
            &appowner_cred,
            Some(json!({"note": "authz", "privacy_policy_url": "https://example.test/privacy"})),
        ),
        (
            "bundle_id",
            format!("/applications/{aid}/activity/bundles"),
            &appowner_cred,
            None,
        ),
        (
            "item_id",
            format!("/moderation/takedowns/{}/items", a["notice_id"]),
            &staff,
            Some(json!({"kind": "message", "id": a["message_id"]})),
        ),
    ];
    for (param, path, who, body) in more {
        if let Some(v) = create(&srv, who, "POST", &path, &path, infos, body).await
            && let Some(id) = v["id"].as_str().or(v["submission"]["id"].as_str())
        {
            a.insert(param, id.to_owned());
        }
    }
    if let Some(tv) = a.get("tv_channel_id").cloned()
        && let Some(v) = create(
            &srv,
            &member_cred,
            "POST",
            "/watch-parties",
            "/watch-parties",
            infos,
            Some(json!({"tv_channel_id": tv})),
        )
        .await
        && let Some(id) = v["id"].as_str()
    {
        a.insert("party_id", id.to_owned());
    }
    if let (Some(chapter), Some(sub)) = (a.get("chapter_id"), a.get("submission_id")) {
        let path = format!("/submissions/{sub}/library/chapters/{chapter}/uploads");
        if let Some(v) = create(
            &srv,
            &member_cred,
            "POST",
            &path,
            &path,
            infos,
            Some(json!({"filename": "a.zip", "size_bytes": 10})),
        )
        .await
            && let Some(id) = v["id"].as_str()
        {
            a.insert("upload_id", id.to_owned());
        }
    }
    // A running thread member: the seeded thread gets the victim.
    if let Some(t) = a.get("thread_id") {
        srv.call(
            &oa,
            "PUT",
            &format!("/channels/{t}/thread-members/{victim}"),
            None,
        )
        .await;
    }

    let mut private = a.clone();
    private.insert("channel_id", priv_chan);
    private.insert("message_id", msg_priv);

    (
        World {
            srv,
            pool,
            actors,
            a,
            b,
            private,
            member_id: u("member"),
            guild_a: ga,
        },
        tx,
        handle,
    )
}

// ---------------------------------------------------------------------------------------------
// Expectations.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Exp {
    /// 401, 403 or 404, and no write.
    Deny,
    /// Not 401/403 (a step-up prompt counts as allowed).
    Allow,
    /// Record only.
    Observe,
}

/// Placeholders that name a tenant resource: a request with the other tenant's value must be
/// refused. `user_id`, `account_id`, `token` and `code` are not tenant ids.
const TENANT: [&str; 30] = [
    "request_id",
    "chapter_id",
    "upload_id",
    "asset_id",
    "version_id",
    "bundle_id",
    "item_id",
    "notice_id",
    "series_id",
    "title_id",
    "command_id",
    "hold_id",
    "event_id",
    "rule_id",
    "emoji_id",
    "suggestion_id",
    "ticket_id",
    "template_id",
    "feed_id",
    "guild_id",
    "channel_id",
    "message_id",
    "thread_id",
    "application_id",
    "role_id",
    "webhook_id",
    "overwrite_id",
    "submission_id",
    "party_id",
    "case_id",
];

fn scoped(path: &str) -> bool {
    TENANT.iter().any(|p| path.contains(&format!("{{{p}}}")))
}

fn fill(path: &str, ids: &HashMap<&'static str, String>) -> String {
    let mut out = String::new();
    let mut rest = path;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let Some(j) = rest[i..].find('}') else { break };
        let name = &rest[i + 1..i + j];
        let v = ids.get(name).cloned().unwrap_or_else(|| match name {
            "provider" => "discord".to_owned(),
            "kind" => "poster".to_owned(),
            "emoji" => "%F0%9F%91%8D".to_owned(),
            "token" | "code" | "key" | "board" | "username" | "stream_key" | "hmac" | "url_b64"
            | "path" | "file" | "*path" => "x".to_owned(),
            "index" => "0".to_owned(),
            _ => BOGUS.to_owned(),
        });
        out.push_str(&v);
        rest = &rest[i + j + 1..];
    }
    out.push_str(rest);
    out
}

/// Routes where a resource-scoped request by someone with no relationship to the resource is
/// legitimately admitted, each with the reason. Everything else scoped must be refused.
fn outsider_exception(r: &Route) -> Option<&'static str> {
    let key = (r.method.as_str(), r.path.as_str());
    match key {
        ("GET", "/guilds/{guild_id}/preview") => Some("public preview of a discoverable server"),
        ("GET", "/guilds/{guild_id}/widget.json") => Some("public widget"),
        // Custom emoji are drawn in messages that anyone may read; this lookup gives the name and
        // whether it is animated, and names no server (`guild` is null for a non-member).
        ("GET", "/sr/emojis/{emoji_id}") => Some("public emoji metadata, no server named"),
        _ => None,
    }
}

/// Hand-made bodies for the routes whose sampled body is refused as malformed before the
/// permission check could be seen, and for the escalation attempts that matter most (a role with
/// administrator rights, a role handed to someone else, a rename). `{name}` placeholders are
/// replaced with the seeded ids of tenant A.
fn body_override(method: &str, path: &str) -> Option<&'static str> {
    Some(match (method, path) {
        ("POST", "/applications/{application_id}/testers") => r#"{"username":"victim"}"#,
        ("POST", "/submissions/{submission_id}/artwork/{kind}") => {
            r#"{"image":"data:image/png;base64,iVBORw0KGgo="}"#
        }
        ("PATCH", "/guilds/{guild_id}/vanity-url") => r#"{"code":"authzmatrix"}"#,
        ("PATCH", "/guilds/{guild_id}/voice-states/{user_id}") => r#"{"suppress":true}"#,
        ("PATCH", "/stage-instances/{channel_id}") => r#"{"topic":"authz"}"#,
        ("POST", "/channels/{channel_id}/scheduled-messages") => {
            r#"{"content":"authz","send_at":"2031-01-01T00:00:00Z"}"#
        }
        ("POST", "/guilds/{guild_id}/channels") => r#"{"name":"authz","type":0}"#,
        ("PUT", "/users/@me/channels/{channel_id}/request") => r#"{"state":"accepted"}"#,
        ("PUT", "/watch-parties/{party_id}/media") => r#"{"title_id":"1"}"#,
        ("PATCH", "/guilds/{guild_id}/members/@me") => r#"{"nick":"authz"}"#,
        ("PATCH", "/guilds/{guild_id}/members/{user_id}") => {
            r#"{"nick":"authz","roles":["{role_id}"]}"#
        }
        ("PATCH", "/guilds/{guild_id}/roles/{role_id}") => r#"{"name":"authz","permissions":"8"}"#,
        ("POST", "/guilds/{guild_id}/roles") => r#"{"name":"authz","permissions":"8"}"#,
        ("POST", "/channels/{channel_id}/threads") => r#"{"name":"authz","type":11}"#,
        ("PUT", "/guilds/{guild_id}/requests/@me") => r#"{"agreed":true}"#,
        ("PATCH", "/guilds/{guild_id}") => r#"{"name":"authz taken over"}"#,
        ("PATCH", "/channels/{channel_id}") => r#"{"name":"authz-renamed"}"#,
        ("PUT", "/channels/{channel_id}/permissions/{overwrite_id}") => {
            r#"{"type":0,"allow":"8","deny":"0"}"#
        }
        ("POST", "/channels/{channel_id}/messages") => r#"{"content":"authz matrix post"}"#,
        ("PATCH", "/guilds/{guild_id}/roles") => r#"[{"id":"{role_id}","position":1}]"#,
        ("PATCH", "/guilds/{guild_id}/emojis/{emoji_id}") => r#"{"name":"hijacked"}"#,
        ("POST", "/guilds/{guild_id}/poll-templates")
        | ("PATCH", "/guilds/{guild_id}/poll-templates/{template_id}") => {
            r#"{"name":"authz","question":"q","answers":["a","b"]}"#
        }
        ("PATCH", "/guilds/{guild_id}/suggestions/{suggestion_id}") => r#"{"status":"approved"}"#,
        ("PATCH", "/guilds/{guild_id}/scheduled-events/{event_id}") => r#"{"name":"hijacked"}"#,
        ("PUT", "/admin/review/submissions/{submission_id}/duplicate") => {
            r#"{"action":"not_duplicate"}"#
        }
        ("PUT", "/admin/library/chapters/{chapter_id}/pages/{index}") => {
            r#"{"image":"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgYGD4DwABBAEAHDlN5QAAAABJRU5ErkJggg=="}"#
        }
        ("POST", "/moderation/cases/{case_id}/holds") => {
            r#"{"kind":"message","id":"{message_id}","reason":"authz matrix"}"#
        }
        ("POST", "/moderation/evasion-signals/{signal_id}/review") => {
            r#"{"decision":"dismissed","reason":"authz matrix"}"#
        }
        ("PUT", "/admin/staff/{account_id}/delegation") => r#"{"max_permissions":"1"}"#,
        ("POST", "/admin/accounts/{account_id}/ban") => {
            r#"{"category":"standard","reason":"authz matrix"}"#
        }
        ("POST", "/admin/persons/{person_id}/ban") => {
            r#"{"category":"standard","reason":"authz matrix"}"#
        }
        _ => return None,
    })
}

fn is_write(method: &str) -> bool {
    !matches!(method, "GET" | "HEAD" | "OPTIONS")
}

#[derive(Debug, Clone)]
struct Obs {
    group: usize,
    method: String,
    path: String,
    actor: &'static str,
    pass: &'static str,
    status: u16,
    exp: Exp,
    verdict: &'static str,
    note: String,
}

#[derive(Clone)]
struct Call {
    route: usize,
    actor: &'static str,
    pass: &'static str,
    exp: Exp,
    path: String,
    /// Send the body with every property filled in.
    full: bool,
}

/// The actor who is rightfully allowed on the route: its status shows whether the seeded ids
/// reach a real resource (a refusal as 404 next to a 404 for the control proves little).
fn control_of(r: &Route) -> &'static str {
    if r.authz == "staff" || r.path.starts_with("/moderation") {
        "staff_owner"
    } else if r.authz == "owner" {
        "app_owner"
    } else if r.path.starts_with("/submissions")
        || r.path.starts_with("/users/@me/channels")
        || r.path.contains("/recipients")
        || r.path.starts_with("/watch-parties")
    {
        "member"
    } else {
        "owner_a"
    }
}

/// Answers that say nothing about a resource although the caller has no business with it: an empty
/// list or a "nothing running" body, produced by filtering, never by a lookup.
fn soft_empty(r: &Route) -> bool {
    matches!(
        (r.method.as_str(), r.path.as_str()),
        ("GET", "/guilds/{guild_id}/stage-instances") | ("GET", "/channels/{channel_id}/activity")
    )
}

fn plan_calls(
    world: &World,
    routes: &[Route],
    group: usize,
    infos: &HashMap<(String, String), OpInfo>,
) -> Vec<Call> {
    let pairs = pair_names(routes);
    let mut calls = Vec::new();
    for (i, r) in routes.iter().enumerate() {
        if group_of(&r.path) != group {
            continue;
        }
        let authed = r.access == "authenticated";
        let (ctx, swapped) = ctx_for(r, &world.a);
        let pa = fill(&r.path, &ctx);
        calls.push(Call {
            route: i,
            actor: "anon",
            pass: "std",
            exp: if authed { Exp::Deny } else { Exp::Observe },
            path: pa.clone(),
            full: false,
        });
        if !authed {
            continue;
        }
        let sc = scoped(&r.path);
        let staff = r.authz == "staff";
        let portal_owner = r.authz == "owner";
        let tenant_authz = matches!(
            r.authz.as_str(),
            "membership"
                | "author"
                | "owner"
                | "author_or_staff"
                | "party_access"
                | "party_member"
                | "party_control"
                | "party_manager"
                | "visibility"
        );
        let exempt = outsider_exception(r).is_some();
        let std = |actor: &'static str, exp: Exp| Call {
            route: i,
            actor,
            pass: "std",
            exp,
            path: pa.clone(),
            full: false,
        };
        calls.push(std(
            "outsider",
            if staff || r.authz == "bot" || (sc && tenant_authz && !exempt) {
                Exp::Deny
            } else {
                Exp::Observe
            },
        ));
        calls.push(std(
            "fake_staff",
            if staff { Exp::Deny } else { Exp::Observe },
        ));
        // Staff by platform bit (D-360): the holder of the named bit (or the Owner) is admitted, any
        // other appointed staff member is refused. `SR_AUTHZ_DERIVE=1` only records.
        let derive = std::env::var_os("SR_AUTHZ_DERIVE").is_some();
        for a in &world.actors {
            let Some(held) = holds(a.name, &pairs) else {
                continue;
            };
            // Only the staff rows are called by every bit holder; the three fixed staff actors
            // are called on every row.
            if !staff && !["staff_owner", "staff_basic", "support"].contains(&a.name) {
                continue;
            }
            let exp = if !staff || derive {
                Exp::Observe
            } else if admitted(&held, &r.platform) {
                if is_write(&r.method) {
                    Exp::Observe
                } else {
                    Exp::Allow
                }
            } else {
                Exp::Deny
            };
            let full_only = is_write(&r.method)
                && a.name != "staff_owner"
                && infos
                    .get(&(r.method.clone(), r.path.clone()))
                    .is_some_and(|i| i.full.is_some() && i.full != i.body);
            if full_only && body_override(&r.method, &r.path).is_none() {
                let mut c = std(a.name, exp);
                c.pass = "std+full";
                c.full = true;
                calls.push(c);
            } else {
                calls.push(std(a.name, exp));
            }
        }
        calls.push(std(
            "member",
            if staff
                || (portal_owner && r.path.contains("{application_id}"))
                || r.authz == "bot"
                || (r.authz == "membership" && r.path.starts_with("/moderation/cases"))
            {
                Exp::Deny
            } else {
                Exp::Observe
            },
        ));
        calls.push(std("owner_a", Exp::Observe));
        calls.push(std("app_owner", Exp::Observe));
        if !is_write(&r.method) {
            calls.push(std("mod", if staff { Exp::Deny } else { Exp::Observe }));
        }
        for bot in ["bot_in", "bot_out"] {
            let mut exp = if r.bot_denied {
                Exp::Deny
            } else {
                Exp::Observe
            };
            if bot == "bot_out"
                && !r.bot_denied
                && sc
                && tenant_authz
                && !exempt
                && r.path.contains("{guild_id}")
            {
                exp = Exp::Deny;
            }
            calls.push(std(bot, exp));
        }
        for (bearer, own) in [("bearer_identify", "identify"), ("bearer_guilds", "guilds")] {
            let exp = match r.bearer.as_str() {
                "-" => Exp::Deny,
                "any" => Exp::Allow,
                s if s == own => Exp::Allow,
                _ => Exp::Deny,
            };
            calls.push(std(bearer, exp));
        }
        // RDY-SEC-002: the owner of the other tenant with this tenant's ids, then with its own
        // {guild_id} and this tenant's children (IDOR), then a member against the private channel.
        if sc {
            calls.push(Call {
                route: i,
                actor: "owner_b",
                pass: "xtenant",
                exp: if tenant_authz && !exempt {
                    Exp::Deny
                } else {
                    Exp::Observe
                },
                path: pa.clone(),
                full: false,
            });
            let mut mixed = ctx.clone();
            mixed.insert("guild_id", world.b["guild_id"].clone());
            let has_child = TENANT
                .iter()
                .filter(|p| **p != "guild_id")
                .any(|p| r.path.contains(&format!("{{{p}}}")));
            if r.path.contains("{guild_id}") && has_child {
                calls.push(Call {
                    route: i,
                    actor: "owner_b",
                    pass: "idor",
                    exp: if tenant_authz && !exempt {
                        Exp::Deny
                    } else {
                        Exp::Observe
                    },
                    path: fill(&r.path, &mixed),
                    full: false,
                });
            }
            if r.path.contains("{channel_id}") && !swapped {
                let pp = fill(&r.path, &world.private);
                calls.push(Call {
                    route: i,
                    actor: "member",
                    pass: "private",
                    exp: if tenant_authz && !r.path.starts_with("/users") {
                        Exp::Deny
                    } else {
                        Exp::Observe
                    },
                    path: pp.clone(),
                    full: false,
                });
                calls.push(Call {
                    route: i,
                    actor: "insider",
                    pass: "private",
                    exp: Exp::Observe,
                    path: pp,
                    full: false,
                });
            }
        }
    }
    // The same calls with every optional property filled in: a handler that checks permission only
    // when a field actually changes (a no-op PATCH answers 200 to anyone) is exposed here.
    let extra: Vec<Call> = calls
        .iter()
        .filter(|c| {
            let r = &routes[c.route];
            is_write(&r.method)
                && ["member", "outsider", "fake_staff", "bot_out", "owner_b"].contains(&c.actor)
                && infos
                    .get(&(r.method.clone(), r.path.clone()))
                    .is_some_and(|i| i.full.is_some() && i.full != i.body)
                && body_override(&r.method, &r.path).is_none()
        })
        .map(|c| Call {
            pass: match c.pass {
                "std" => "std+full",
                "xtenant" => "xtenant+full",
                "idor" => "idor+full",
                _ => "private+full",
            },
            full: true,
            ..c.clone()
        })
        .collect();
    calls.extend(extra);
    calls
}

fn json_code(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get("code").map(|c| c.to_string()))
        .unwrap_or_default()
}

fn judge(exp: Exp, r: &Resp, soft: bool) -> (&'static str, String) {
    let s = r.status;
    let code = json_code(&r.body);
    let short = |n: usize| r.body.chars().take(n).collect::<String>();
    match exp {
        Exp::Deny => {
            if matches!(s, 401 | 403 | 404) {
                ("ok", String::new())
            } else if soft
                && s == 200
                && (r.body.trim() == "[]" || r.body.contains("\"instance\":null"))
            {
                ("ok", "empty answer".into())
            } else if matches!(s, 400 | 405 | 409 | 415 | 422) {
                ("masked", format!("{code} {}", short(100)))
            } else if s == 0 || s >= 500 {
                ("error", format!("{s} {}", short(120)))
            } else {
                ("FAIL", format!("{s} {}", short(160)))
            }
        }
        Exp::Allow => {
            // 900004 (re-confirm / set up two-factor) and 60003 are step-up prompts: the caller
            // passed the authorization check and is asked for a stronger proof.
            let stepup = code == "900004" || code == "60003";
            if (s == 401 || s == 403) && !stepup {
                ("FAIL", format!("{s} {}", short(160)))
            } else {
                ("ok", String::new())
            }
        }
        Exp::Observe => ("seen", String::new()),
    }
}

/// Tables a background job or any request moves (the job runner, sessions, the access log, the payment
/// watcher cursor), so that they say nothing about one refused call.
const NOISY: [&str; 13] = [
    "payment_watcher",
    "staff_access_log",
    "sessions",
    "jobs",
    "job_runs",
    "_sqlx_migrations",
    "audit_log",
    "rate_limits",
    "outbox",
    "gateway_sessions",
    "login_attempts",
    "presence",
    "user_activity",
];

/// Columns stamped by authentication itself (a bot token's last use, a session's last sight).
const STAMPS: [&str; 7] = [
    "last_used_at",
    "last_seen_at",
    // The RSS poller's bookkeeping.
    "next_check_at",
    "last_checked_at",
    "failures",
    "last_status",
    "seeded",
];

async fn fingerprint(pool: &sqlx::PgPool) -> BTreeMap<String, String> {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT tablename::text FROM pg_tables WHERE schemaname = 'public' ORDER BY 1",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let mut out = BTreeMap::new();
    for t in tables {
        if NOISY.contains(&t.as_str()) || t.starts_with("job") || t.contains("access_log") {
            continue;
        }
        let strip: String = STAMPS.iter().map(|c| format!(" - '{c}'")).collect();
        let sql = format!(
            "SELECT md5(coalesce(string_agg((to_jsonb(x){strip})::text, '|' ORDER BY (to_jsonb(x){strip})::text), '')) FROM \"{t}\" x"
        );
        let h: Result<String, _> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
            .fetch_one(pool)
            .await;
        if let Ok(h) = h {
            out.insert(t, h);
        }
    }
    out
}

fn diff(a: &BTreeMap<String, String>, b: &BTreeMap<String, String>) -> Vec<String> {
    a.iter()
        .filter(|(k, v)| b.get(*k) != Some(*v))
        .map(|(k, _)| k.clone())
        .collect()
}

async fn heal(world: &World) {
    // A plain member's probes (leave, ...) must not wear the cast away.
    add_member(&world.pool, world.guild_a, world.member_id).await;
}

fn class_of(method: &str) -> u8 {
    match method {
        "GET" | "HEAD" => 0,
        "DELETE" => 2,
        _ => 1,
    }
}

struct GroupResult {
    line: String,
    failures: Vec<String>,
    obs: Vec<Obs>,
}

async fn run_group(
    g: usize,
    base_url: &str,
    routes: &[Route],
    infos: &HashMap<(String, String), OpInfo>,
) -> GroupResult {
    let started = std::time::Instant::now();
    let gname = GROUPS[g];
    let db = TestDb::new().await.expect("test db");
    let url = match base_url.rsplit_once('/') {
        Some((head, _)) => format!("{head}/{}", db.name()),
        None => panic!("bad SR_TEST_DATABASE_URL"),
    };
    let (world, stop, server) = seed(&db, &url, infos, &pair_names(routes)).await;
    let calls = plan_calls(&world, routes, g, infos);
    let mut failures = Vec::new();
    let mut obs: Vec<Obs> = Vec::new();

    let (denials, others): (Vec<&Call>, Vec<&Call>) =
        calls.iter().partition(|c| c.exp == Exp::Deny);
    let mut others = others;
    // Reads, then writes, then deletes; deeper paths first among the deletes, so that removing a
    // server does not leave its children looking absent to the controls.
    others.sort_by_key(|c| {
        let r = &routes[c.route];
        let depth = r.path.matches('/').count();
        // The rightful actors go last in each class: a rename they made first would turn the
        // others' identical request into a no-op that no permission check ever sees.
        let rightful = matches!(c.actor, "owner_a" | "staff_owner" | "app_owner");
        (
            class_of(&r.method),
            rightful,
            if r.method == "DELETE" {
                usize::MAX - depth
            } else {
                0
            },
        )
    });

    let before = fingerprint(&world.pool).await;
    let mut group_fail = 0usize;
    let mut n = 0usize;
    for (phase, list) in [(0, denials), (1, others)] {
        for c in list {
            let r = &routes[c.route];
            let actor = world.actor(c.actor);
            let info = infos.get(&(r.method.clone(), r.path.clone()));
            let mut path = c.path.clone();
            if let Some(i) = info
                && !i.query.is_empty()
            {
                path.push(if path.contains('?') { '&' } else { '?' });
                path.push_str(&i.query);
            }
            let body = if is_write(&r.method) {
                if let Some(text) = body_override(&r.method, &r.path) {
                    {
                        let mut t = text.to_owned();
                        for (k, v) in &world.a {
                            t = t.replace(&format!("{{{k}}}"), v);
                        }
                        serde_json::from_str(&t).ok()
                    }
                } else {
                    Some(
                        info.and_then(|i| {
                            if c.full {
                                i.full.clone()
                            } else {
                                i.body.clone()
                            }
                        })
                        .unwrap_or_else(|| json!({})),
                    )
                }
            } else {
                None
            };
            let resp = world
                .srv
                .call(&actor.cred, &r.method, &path, body.as_ref())
                .await;
            n += 1;
            let (verdict, note) = judge(c.exp, &resp, soft_empty(r));
            if verdict == "FAIL" || verdict == "error" {
                group_fail += 1;
                failures.push(format!(
                    "[{gname}] {} {} as {} ({}): expected {:?}, got {}",
                    r.method, r.path, c.actor, c.pass, c.exp, note
                ));
            }
            obs.push(Obs {
                group: g,
                method: r.method.clone(),
                path: r.path.clone(),
                actor: c.actor,
                pass: c.pass,
                status: resp.status,
                exp: c.exp,
                verdict,
                note: if verdict == "seen" || verdict == "ok" {
                    resp.body.chars().take(80).collect()
                } else {
                    note
                },
            });
            if c.actor == "member" {
                heal(&world).await;
            }
        }
        if phase == 0 {
            // RDY-SEC-002: refused calls write nothing.
            let after = fingerprint(&world.pool).await;
            let changed = diff(&before, &after);
            if !changed.is_empty() {
                group_fail += 1;
                failures.push(format!(
                    "[{gname}] refused calls changed tables: {changed:?}"
                ));
            }
        }
    }

    // Weak refusals: a 404 where the rightful actor also found nothing.
    let mut control: HashMap<(String, String), u16> = HashMap::new();
    for o in &obs {
        let r = routes
            .iter()
            .find(|r| r.method == o.method && r.path == o.path)
            .unwrap();
        if o.pass == "std" && o.actor == control_of(r) {
            control.insert((o.method.clone(), o.path.clone()), o.status);
        }
    }
    let (mut weak, mut masked) = (0, 0);
    for o in obs.iter_mut() {
        if o.verdict == "masked" {
            masked += 1;
        }
        if o.exp == Exp::Deny && o.status == 404 && o.pass != "idor" {
            let c = control
                .get(&(o.method.clone(), o.path.clone()))
                .copied()
                .unwrap_or(0);
            // The refusal is only as good as its difference from the rightful actor's answer.
            if c == o.status || c == 0 || c >= 500 {
                o.verdict = "weak";
                weak += 1;
            }
        }
    }

    // R-03: the weak refusals of a group may only go down (lower WEAK_CEILING when they do).
    if weak > WEAK_CEILING[g] {
        group_fail += 1;
        failures.push(format!(
            "[{gname}] {weak} weak refusals, the ceiling is {}: a seeded id stopped reaching its resource",
            WEAK_CEILING[g]
        ));
    }
    let _ = stop.send(());
    let _ = tokio::time::timeout(Duration::from_secs(40), server).await;
    db.cleanup().await.ok();
    let status = if group_fail == 0 { "proven" } else { "FAILED" };
    let line = format!(
        "{}. {gname}: {status} ({n} calls, {group_fail} failures, {weak} weak refusals, {masked} masked, {:.0}s)",
        g + 1,
        started.elapsed().as_secs_f64()
    );
    GroupResult {
        line,
        failures,
        obs,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn every_route_enforces_its_authz_row() {
    let base = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL");
    let routes = parse_matrix();
    let infos = op_infos();
    // Optional: SR_AUTHZ_GROUPS=0,1 runs only those groups (default: all, in proof order).
    let only: Option<Vec<usize>> = std::env::var("SR_AUTHZ_GROUPS")
        .ok()
        .map(|s| s.split(',').filter_map(|x| x.trim().parse().ok()).collect());
    let mut failures: Vec<String> = Vec::new();
    let mut all_obs: Vec<Obs> = Vec::new();
    let mut proven: Vec<String> = Vec::new();
    for g in 0..GROUPS.len() {
        if only.as_ref().is_some_and(|o| !o.contains(&g)) {
            continue;
        }
        let res = run_group(g, &base, &routes, &infos).await;
        println!("authz_matrix: {}", res.line);
        proven.push(res.line);
        failures.extend(res.failures);
        all_obs.extend(res.obs);
    }
    if let Ok(path) = std::env::var("SR_AUTHZ_REPORT") {
        let mut s =
            String::from("group\tmethod\tpath\tactor\tpass\tstatus\texpected\tverdict\tnote\n");
        for o in &all_obs {
            s.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{:?}\t{}\t{}\n",
                GROUPS[o.group],
                o.method,
                o.path,
                o.actor,
                o.pass,
                o.status,
                o.exp,
                o.verdict,
                o.note.replace(['\n', '\t'], " ")
            ));
        }
        let _ = std::fs::write(path, s);
    }
    // The per-group search index directories are scratch.
    let prefix = format!("sr_authz_search_{}_", std::process::id());
    if let Ok(rd) = std::fs::read_dir(std::env::temp_dir()) {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().starts_with(&prefix) {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    println!("authz_matrix: groups in the order proven:");
    for l in &proven {
        println!("authz_matrix:   {l}");
    }
    let unique: BTreeSet<&String> = failures.iter().collect();
    assert!(
        unique.is_empty(),
        "{} authorization failures:\n{}",
        unique.len(),
        unique
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    let _ = (Method::GET, policy::normalise("/"));
}

/// Routes whose `{channel_id}` is not a text channel of server A: thread members take the thread,
/// direct-message routes the direct message, recipient routes the group, Live TV the TV channel.
fn ctx_for(
    r: &Route,
    base: &HashMap<&'static str, String>,
) -> (HashMap<&'static str, String>, bool) {
    let from = if r.path.contains("/thread-members") {
        "thread_id"
    } else if r.path.starts_with("/users/@me/channels") {
        "dm_channel_id"
    } else if r.path.contains("/recipients") {
        "group_channel_id"
    } else if r.path.contains("tv/channels") {
        "tv_channel_id"
    } else {
        return (base.clone(), false);
    };
    let mut c = base.clone();
    if let Some(v) = base.get(from) {
        c.insert("channel_id", v.clone());
    }
    (c, true)
}
