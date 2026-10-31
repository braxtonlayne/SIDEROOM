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
    repo::{SessionRecord, SessionRepository, TotpRepository},
    session::SessionToken,
    totp::TotpSecret,
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

#[path = "../../sr-api/tests/support/platform_cell.rs"]
mod platform_cell;
use platform_cell::{Platform, is_platform_name, parse_platform};

const MATRIX: &str = include_str!("../../sr-api/tests/route_matrix.tsv");
const KEY_HEX: &str = "0707070707070707070707070707070707070707070707070707070707070707";
/// An id nothing owns.
const BOGUS: &str = "900000000000000001";

/// Weak refusals allowed per group, in the order of [`GROUPS`]. A refusal is weak when it is a 404 next
/// to a 404 for the rightful actor (the seeded id names nothing). The numbers only go down: when a
/// seed improves, lower the number in the same change.
/// Admin is 28 since R-07 (D-366): the weak refusals there are one per appointed actor on
/// `POST /admin/review/submissions/{submission_id}/publish-now`, and the merge rows added the pair actor
/// `staff_publish_media_and_delete_content`; the seed did not get worse.
/// RDY-SEC-011 (P25): the control for a write route is now the write phase's own answer (a rightful actor
/// on a real resource), so a 404 next to it is a real refusal: the ceilings fell to 0, 0, 0, 0, 0 and 16.
const WEAK_CEILING: [usize; 6] = [0, 0, 0, 0, 0, 16];

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

/// The distinct `A+B` values of the PLATFORM column (D-366 grammar: `All` with two or more names).
fn pair_names(routes: &[Route]) -> Vec<String> {
    let set: BTreeSet<&String> = routes
        .iter()
        .filter(|r| r.platform != "-")
        .filter(|r| matches!(platform_of(r), Platform::All(_)))
        .map(|r| &r.platform)
        .collect();
    set.into_iter().cloned().collect()
}

/// The parsed PLATFORM cell of a staff row; the grammar is `route_matrix.rs`'s, so a bad cell
/// fails there first.
fn platform_of(r: &Route) -> Platform {
    parse_platform(&r.platform, is_platform_name)
        .unwrap_or_else(|e| panic!("{} {}: {e}", r.method, r.path))
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

/// Whether a holder of `held` passes a row that demands `platform`: `NAME`, `A+B` (both) or
/// `A|B` (any), never mixed (D-366, `support/platform_cell.rs`).
fn admitted(held: &[String], platform: &str) -> bool {
    parse_platform(platform, is_platform_name)
        .unwrap_or_else(|e| panic!("{e}"))
        .admits(held)
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
    /// The response header block, lowercased (PRF-PRO-001 checks `x-robots-tag`).
    head: String,
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
    raw_body(
        port,
        method,
        path,
        headers,
        body.map(|b| ("application/json", b.as_bytes())),
    )
    .await
}

/// The same call with any body and content type (RDY-SEC-012: the zip and binary upload routes).
async fn raw_body(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(String, String)],
    body: Option<(&str, &[u8])>,
) -> Resp {
    let fut = async {
        let mut s = TcpStream::connect(("127.0.0.1", port)).await.ok()?;
        let mut req = format!(
            "{method} {path} HTTP/1.1\r\nHost: localhost:{port}\r\nConnection: close\r\nAccept: application/json\r\n"
        )
        .into_bytes();
        let mut head = String::new();
        for (k, v) in headers {
            head.push_str(&format!("{k}: {v}\r\n"));
        }
        match body {
            Some((ctype, b)) => {
                head.push_str(&format!("Content-Type: {ctype}\r\n"));
                head.push_str(&format!("Content-Length: {}\r\n\r\n", b.len()));
                req.extend_from_slice(head.as_bytes());
                req.extend_from_slice(b);
            }
            None => {
                if method != "GET" && method != "HEAD" {
                    head.push_str("Content-Length: 0\r\n");
                }
                head.push_str("\r\n");
                req.extend_from_slice(head.as_bytes());
            }
        }
        s.write_all(&req).await.ok()?;
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
            head: head.to_lowercase(),
        })
    };
    match tokio::time::timeout(Duration::from_secs(20), fut).await {
        Ok(Some(r)) => r,
        _ => Resp {
            status: 0,
            body: "timeout or connection failure".into(),
            head: String::new(),
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

    /// A call that also sends the audit reason header the staff routes ask for.
    async fn call_reason(
        &self,
        cred: &Cred,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> Resp {
        let b = body.map(Value::to_string);
        let mut h = self.headers(cred, method);
        h.push(("X-Audit-Log-Reason".to_owned(), "authz matrix".to_owned()));
        raw(
            self.port,
            method,
            &format!("/api/v1{path}"),
            &h,
            b.as_deref(),
        )
        .await
    }

    /// A call with a body that is not JSON (a zip, the bytes of an image).
    async fn call_bytes(
        &self,
        cred: &Cred,
        method: &str,
        path: &str,
        ctype: &str,
        body: &[u8],
    ) -> Resp {
        raw_body(
            self.port,
            method,
            &format!("/api/v1{path}"),
            &self.headers(cred, method),
            Some((ctype, body)),
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
    /// The TOTP secret `staff_owner` is enrolled with (RDY-SEC-012), for the step-up of the write phase.
    totp: TotpSecret,
    /// The account ids of the cast, by seed name.
    users: BTreeMap<&'static str, i64>,
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

/// Open the staff step-up window of a session with a real TOTP code (the secret is the Owner's).
async fn step_up(srv: &Server, who: &Cred, totp: &TotpSecret) -> Resp {
    let now = SystemClock.now_unix_ms() as u64 / 1000;
    let code =
        sr_auth::totp::current_code(totp, "SIDE ROOM", "staffowner", now).expect("totp code");
    srv.call(
        who,
        "POST",
        "/admin/reauth",
        Some(&json!({"method": "totp", "code": code})),
    )
    .await
}

/// An RFC 3339 UTC time `days` from now (the days-from-civil conversion, no date crate).
fn rfc3339_in_days(days: u64) -> String {
    let secs = SystemClock.now_unix_ms() as u64 / 1000 + days * 86_400;
    let (d, rem) = ((secs / 86_400) as i64, secs % 86_400);
    let z = d + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// A real `w` x `h` RGBA PNG (stored deflate blocks), with its CRCs: the picture routes decode what they get.
fn make_png(w: u32, h: u32) -> Vec<u8> {
    fn crc(data: &[u8]) -> u32 {
        let mut c = 0xffff_ffffu32;
        for b in data {
            c ^= u32::from(*b);
            for _ in 0..8 {
                c = if c & 1 == 1 {
                    0xedb8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
        }
        !c
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc(&body).to_be_bytes());
    }
    let mut raw = Vec::new();
    for _ in 0..h {
        raw.push(0);
        raw.extend(std::iter::repeat_n([120u8, 60, 200, 255], w as usize).flatten());
    }
    let (mut a, mut b) = (1u32, 0u32);
    for x in &raw {
        a = (a + u32::from(*x)) % 65_521;
        b = (b + a) % 65_521;
    }
    let mut z = vec![0x78, 0x01, 0x01];
    z.extend_from_slice(&(raw.len() as u16).to_le_bytes());
    z.extend_from_slice(&(!(raw.len() as u16)).to_le_bytes());
    z.extend_from_slice(&raw);
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a].to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

/// The password of the accounts that present one in the write phase (a test value, in no real system).
const TEST_PASSWORD: &str = "authz-matrix-Correct-Horse-9";

/// Standard base64 with padding (the data URIs of stickers and sounds; no dependency for ten lines).
fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in data.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// A mono 8 kHz PCM WAV of `ms` milliseconds (the soundboard accepts MP3, Ogg and WAV; the container's own
/// length claim is what is measured, D-363).
fn wav_uri(ms: usize) -> String {
    let rate = 8_000usize;
    let n = rate * ms / 1000;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + n * 2) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(rate as u32).to_le_bytes());
    out.extend_from_slice(&((rate * 2) as u32).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&((n * 2) as u32).to_le_bytes());
    out.resize(out.len() + n * 2, 7);
    format!("data:audio/wav;base64,{}", b64(&out))
}

/// A zip of the given files.
fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in entries {
        z.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(data).unwrap();
    }
    z.finish().unwrap().into_inner()
}

/// A zip of one file, an Activity bundle that unpacks.
fn tiny_zip() -> Vec<u8> {
    use std::io::Write;
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    z.start_file("index.html", zip::write::SimpleFileOptions::default())
        .unwrap();
    z.write_all(b"<h1>authz</h1>").unwrap();
    z.finish().unwrap().into_inner()
}

/// The id a create answered with: `id`, or the one nested under the object it returned.
fn pick_id(v: &Value) -> Option<String> {
    [
        &v["id"],
        &v["upload"]["id"],
        &v["attachments"][0]["upload_id"],
        &v["token"]["id"],
        &v["bundle"]["id"],
        &v["submission"]["id"],
        &v["sound_id"],
    ]
    .into_iter()
    .find_map(|x| x.as_str().map(str::to_owned))
}

/// A create for the seed: the sampled body unless one is given; the new id.
async fn mk(
    srv: &Server,
    infos: &HashMap<(String, String), OpInfo>,
    who: &Cred,
    path: &str,
    body: Option<Value>,
) -> Option<String> {
    mk_m(srv, infos, who, "POST", path, body).await
}

async fn mk_m(
    srv: &Server,
    infos: &HashMap<(String, String), OpInfo>,
    who: &Cred,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> Option<String> {
    // The template for the sampled body is the path with its ids put back to the placeholders.
    let tpl = path
        .split('/')
        .map(|seg| {
            if seg.len() > 12 && seg.bytes().all(|b| b.is_ascii_digit()) {
                "{x}"
            } else {
                seg
            }
        })
        .collect::<Vec<_>>()
        .join("/");
    let key = infos
        .keys()
        .find(|(m, p)| m == method && same_template(p, &tpl))
        .map(|(_, p)| p.clone())
        .unwrap_or(tpl);
    create(srv, who, method, &key, path, infos, body)
        .await
        .as_ref()
        .and_then(pick_id)
}

/// Whether `template` (with `{x}` for every id) is the OpenAPI path `p`.
fn same_template(p: &str, template: &str) -> bool {
    let (a, b): (Vec<&str>, Vec<&str>) = (p.split('/').collect(), template.split('/').collect());
    a.len() == b.len()
        && a.iter()
            .zip(&b)
            .all(|(x, y)| x == y || (x.starts_with('{') && *y == "{x}"))
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
        "offeree",
        "dispdel",
        "disppw",
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

    // The accounts whose own password the write phase presents (reset-secret, unlink, change ...).
    {
        let hasher = sr_auth::password::PasswordHasher::new(sr_auth::password::PasswordParams {
            m_cost_kib: 1024,
            t_cost: 1,
            p_cost: 1,
        })
        .expect("hasher");
        let hash = hasher.hash(TEST_PASSWORD).expect("hash");
        for n in ["member", "appowner", "ownera", "dispdel", "disppw"] {
            sqlx::query("UPDATE accounts SET password_hash = $2 WHERE id = $1")
                .bind(u(n))
                .bind(&hash)
                .execute(&pool)
                .await
                .unwrap();
        }
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
        ("victim", "victim"),
        ("offeree", "offeree"),
        ("disp_del", "dispdel"),
        ("disp_pw", "disppw"),
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
                privacy_policy_url: Some(Some("https://client.test/privacy".into())),
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

    // RDY-SEC-012: the Owner is enrolled in TOTP (confirmed, with a step far in the past), so that the
    // write phase can open the staff step-up window once per group with a real code.
    let totp = TotpSecret::generate().expect("totp secret");
    let step_now = SystemClock.now_unix_ms() as u64 / 1000 / 30;
    TotpRepository::put_pending(&auth, u("staffowner"), totp.as_bytes())
        .await
        .unwrap();
    TotpRepository::confirm(&auth, u("staffowner"), step_now - 10)
        .await
        .unwrap();
    // The staff offeree is enrolled too (an offer is accepted only with a second factor).
    TotpRepository::put_pending(&auth, u("offeree"), totp.as_bytes())
        .await
        .unwrap();
    TotpRepository::confirm(&auth, u("offeree"), step_now - 10)
        .await
        .unwrap();

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
    // RDY-SEC-006: the abuse limits too. Twenty actors call each guarded route (a webhook token's
    // bucket is shared by every caller, interaction callbacks are per address, and every actor here
    // is 127.0.0.1), so the defaults would turn expected statuses into 429. Every abuse setting goes
    // to its catalog maximum, the data-export cooldown to its minimum; `abuse_limits.rs` proves the
    // limits themselves.
    for d in sr_admin::settings::CATALOG
        .iter()
        .filter(|d| d.group == "abuse")
    {
        let figure = if d.key == sr_admin::settings::ABUSE_DATA_EXPORT_COOLDOWN {
            d.min
        } else {
            d.max
        };
        sqlx::query("INSERT INTO platform_settings (key, value, updated_by) VALUES ($1, $2::text::jsonb, $3) ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value")
            .bind(d.key)
            .bind((figure as i64).to_string())
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
    // BRG-COR-001: the Discord bridge talks to a port nothing listens on, so a call that would reach Discord fails
    // at once (409) and real Discord is never contacted, not even by the inbound shard of a seeded bridge.
    let dead = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    cfg.bridge.discord_api = Some(format!("http://127.0.0.1:{dead}/api/v10"));
    cfg.bridge.discord_gateway = Some(format!("ws://127.0.0.1:{dead}/gateway"));
    // RDY-SEC-012: voice is on, so that the stage routes are reached past the module gate. The embedded SFU
    // binds loopback sockets in a small range of its own; nothing joins a call.
    cfg.voice.enabled = true;
    cfg.voice.bind_ip = "127.0.0.1".into();
    cfg.voice.announce_ips = vec!["127.0.0.1".into()];
    // A range of its own per world: a stopped node frees its sockets a moment after the process says so.
    static VOICE_N: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
    let voice_base = 52_000 + 10 * VOICE_N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    cfg.voice.udp_port_min = voice_base;
    cfg.voice.udp_port_max = voice_base + 9;
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
    // BYOS-SCP-002: a user mount of `member` and one of `ownerb` (the cross-tenant sweep over `{mount_id}`),
    // both past their CORS check so that the presign route reaches the signer. Nothing listens on the endpoint:
    // presigning is local signing.
    // A guild mount per tenant too (`guild_mount_id`), so that `{mount_id}` under `/guilds/{guild_id}/storage/mounts`
    // names the guild's own mount (OVERLAYS).
    for (owner, tenant_b, slot) in [
        (
            sr_storage::mounts::MountOwner::User(u("member")),
            false,
            "mount_id",
        ),
        (
            sr_storage::mounts::MountOwner::User(u("ownerb")),
            true,
            "mount_id",
        ),
        (
            sr_storage::mounts::MountOwner::Guild(ga),
            false,
            "guild_mount_id",
        ),
        (
            sr_storage::mounts::MountOwner::Guild(gb),
            true,
            "guild_mount_id",
        ),
    ] {
        let id = ids.generate().get() as i64;
        let sb = sr_db::sealed::SecretBox::new(key);
        let new = sr_storage::mounts::NewMount::seal(
            &sb,
            id,
            sr_storage::mounts::MountInput {
                owner,
                label: "Shelf".into(),
                provider: sr_storage::mounts::Provider::Minio,
                endpoint: "http://127.0.0.1:9".into(),
                region: "us-east-1".into(),
                bucket: "shelf".into(),
                prefix: String::new(),
                path_style: true,
                access_key_id: "AKIAAUTHZMATRIX".into(),
            },
            &sr_storage::mounts::Secret::new("authz-matrix-secret"),
        )
        .unwrap();
        use sr_storage::mounts::MountRepo as _;
        sr_db::mounts_repo::PgMountRepo::new(pool.clone())
            .create(new, None)
            .await
            .unwrap();
        sqlx::query("UPDATE storage_mounts SET cors_ok_at = now() WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        let ids_map = if tenant_b { &mut b } else { &mut a };
        ids_map.insert(slot, id.to_string());
    }
    // PRF-SCR-001 (own block): per tenant a scrapbook shelf (the server's own mount, set by its owner), an entry made
    // from the tenant's message with one copy (`scrapbook_entry_id`, `scrapbook_object_id`), and a bucket removal that
    // failed once (`scrapbook_removal_id`), so that the `{entry_id}`, `{object_id}` and `{removal_id}` routes name
    // the tenant's own rows (OVERLAYS).
    for (slot_map, guild, chan, author, granter) in [
        (&mut a, ga, chan_a, u("member"), u("ownera")),
        (&mut b, gb, chan_b, u("ownerb"), u("ownerb")),
    ] {
        let mount: i64 = slot_map["guild_mount_id"].parse().unwrap();
        let message: i64 = slot_map["message_id"].parse().unwrap();
        let (entry, object, removal) = (
            ids.generate().get() as i64,
            ids.generate().get() as i64,
            ids.generate().get() as i64,
        );
        sqlx::query(
            "INSERT INTO scrapbook_targets (guild_id, mount_id, granted_by) VALUES ($1, $2, $3)",
        )
        .bind(guild)
        .bind(mount)
        .bind(granter)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO scrapbook_entries (id, guild_id, channel_id, message_id, author_id, author_name, content, \n             sent_at, archived_by, via, mount_id) VALUES ($1, $2, $3, $4, $5, 'authz', 'authz entry', now(), $6, 'manual', $7)",
        )
        .bind(entry)
        .bind(guild)
        .bind(chan)
        .bind(message)
        .bind(author)
        .bind(granter)
        .bind(mount)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO scrapbook_objects (id, entry_id, position, mount_id, key, filename, content_type, size_bytes, kind) \n             VALUES ($1, $2, 0, $3, 'scrapbook/2026/01/authz/0-a.png', 'a.png', 'image/png', 3, 'image')",
        )
        .bind(object)
        .bind(entry)
        .bind(mount)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO scrapbook_removals (id, guild_id, mount_id, key, message_id, state, attempts, last_error) \n             VALUES ($1, $2, $3, 'scrapbook/2026/01/gone/0-b.png', $4, 'failed', 1, 'authz seed')",
        )
        .bind(removal)
        .bind(guild)
        .bind(mount)
        .bind(message)
        .execute(&pool)
        .await
        .unwrap();
        slot_map.insert("scrapbook_entry_id", entry.to_string());
        slot_map.insert("scrapbook_object_id", object.to_string());
        slot_map.insert("scrapbook_removal_id", removal.to_string());
    }
    // BRG-COR-001 (wave 35 integration, own block): per tenant a connected Discord bridge (its bot token sealed with
    // the world's key, as the route would) and one channel map that only receives from Discord (no webhook), so that
    // the `{map_id}` routes name the tenant's own map. The bridge's Discord is a closed local port (see the config
    // below): every call that would reach Discord fails at once, and real Discord is never contacted.
    {
        let sealer =
            sr_db::sealed::SecretBox::new(sr_db::auth_repo::parse_secret_key(KEY_HEX).unwrap());
        for (slot_map, guild, chan, owner) in [
            (&mut a, ga, chan_a, u("ownera")),
            (&mut b, gb, chan_b, u("ownerb")),
        ] {
            let token = sealer
                .seal(
                    format!("bridge:bot:{guild}").as_bytes(),
                    b"authz-fake-bot-token",
                )
                .unwrap();
            sqlx::query(
                "INSERT INTO bridges (guild_id, discord_guild_id, discord_guild_name, bot_user_id, bot_name, \
                 bot_token_enc, status, created_by) VALUES ($1, $2, 'Authz', $3, 'authz-bot', $4, 'connecting', $5)",
            )
            .bind(guild)
            .bind(ids.generate().get() as i64)
            .bind(ids.generate().get() as i64)
            .bind(token)
            .bind(owner)
            .execute(&pool)
            .await
            .unwrap();
            let map = ids.generate().get() as i64;
            sqlx::query(
                "INSERT INTO bridge_channel_maps (id, guild_id, channel_id, discord_channel_id, discord_channel_name, \
                 direction, created_by) VALUES ($1, $2, $3, $4, 'authz', 'from_discord', $5)",
            )
            .bind(map)
            .bind(guild)
            .bind(chan)
            .bind(ids.generate().get() as i64)
            .bind(owner)
            .execute(&pool)
            .await
            .unwrap();
            slot_map.insert("map_id", map.to_string());
        }
    }
    // PRF-RNG-001 (integration, own block): per tenant a live text ring shared with the tenant's server
    // (`ring_id`), so that the `{ring_id}` routes name a ring the tenant's actors may see: tenant A's is the plain
    // member's (author of the self routes), seen by owner_a through the shared server; tenant B's is owner_b's.
    for (slot_map, guild, author) in [(&mut a, ga, u("member")), (&mut b, gb, u("ownerb"))] {
        let ring = ids.generate().get() as i64;
        sqlx::query(
            "INSERT INTO status_rings (id, author_id, kind, body, audience, created_at, expires_at)              VALUES ($1, $2, 'text', 'authz ring', 'guilds', now(), now() + interval '23 hours')",
        )
        .bind(ring)
        .bind(author)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO status_ring_guilds (ring_id, guild_id) VALUES ($1, $2)")
            .bind(ring)
            .bind(guild)
            .execute(&pool)
            .await
            .unwrap();
        slot_map.insert("ring_id", ring.to_string());
    }
    // BYOS-LEG-001 (own block): a mount per tenant for `/admin/storage/mounts/{mount_id}/delist|relist` (OVERLAYS maps
    // `{mount_id}` there), so that a staff sweep that delists it cannot take the storage-scope routes' mount away.
    for (owner, tenant_b) in [(u("member"), false), (u("ownerb"), true)] {
        let id = ids.generate().get() as i64;
        sqlx::query(
            "INSERT INTO storage_mounts (id, owner_kind, owner_id, label, provider, endpoint, bucket, access_key_id, \
             secret_key_enc, key_hint) VALUES ($1, 'user', $2, 'Legal', 'minio', 'http://127.0.0.1:9', 'legal', \
             'AKIALEGAL', '\\xdeadbeef', 'EGAL')",
        )
        .bind(id)
        .bind(owner)
        .execute(&pool)
        .await
        .unwrap();
        let ids_map = if tenant_b { &mut b } else { &mut a };
        ids_map.insert("legal_mount_id", id.to_string());
    }
    // --- BYOS-BRW-001 seeds (byos-browser): one open upload per tenant, for the `{mount_upload_id}` routes ---
    for (who, ids_map) in [("member", &mut a), ("ownerb", &mut b)] {
        use sr_storage::uploads::UploadLedger as _;
        let upload_id = ids.generate().get() as i64;
        let mount_id: i64 = ids_map["mount_id"].parse().unwrap();
        sr_db::mount_uploads_repo::PgUploadLedger::new(pool.clone())
            .insert(sr_storage::uploads::MountUpload {
                id: upload_id,
                mount_id,
                user_id: u(who),
                key: "Music/seed.flac".into(),
                s3_upload_id: "authz-matrix-upload".into(),
                size: 40 * 1024 * 1024,
                part_size: sr_storage::uploads::PART_SIZE,
                part_count: 3,
                content_type: "audio/flac".into(),
                state: sr_storage::uploads::UploadState::Open,
                created_at: 1_900_000_000_000,
                updated_at: 1_900_000_000_000,
            })
            .await
            .unwrap();
        ids_map.insert("mount_upload_id", upload_id.to_string());
    }
    // The platform bucket, for the attach and detach routes (nothing else is attachable).
    let platform_id =
        insert_mount(&pool, &ids, sr_storage::mounts::MountOwner::Platform, true).await;
    a.insert("platform_mount_id", platform_id.to_string());
    // --- end BYOS-BRW-001 seeds ---
    // --- BYOS-CNV-003 seeds (conv-worker, own block): a worker token scoped to each tenant's mount, for
    // `DELETE /storage/mounts/{mount_id}/worker-tokens/{worker_token_id}` and its cross-tenant sweep (tenant B's
    // token id under tenant A's mount must not revoke anything) ---
    for ids_map in [&mut a, &mut b] {
        let mount: i64 = ids_map["mount_id"].parse().unwrap();
        let token = ids.generate().get() as i64;
        sqlx::query(
            "INSERT INTO worker_tokens (id, name, token_sha256, scope) VALUES ($1, 'Matrix worker', $2, $3)",
        )
        .bind(token)
        .bind(format!("{token:064x}"))
        .bind(format!("mount:{mount}"))
        .execute(&pool)
        .await
        .unwrap();
        ids_map.insert("worker_token_id", token.to_string());
    }
    // --- end BYOS-CNV-003 seeds ---
    // BYOS-CAT-001/002 (wave 28, byos-catalog): a series on each tenant's mount, indexed the way the indexer
    // writes it (unpublished, `source = 'mount'`), for the `/shelves/{section}/{item_id}[/parts/{part_id}]`
    // routes and their cross-tenant sweep.
    b.insert("section", "cinema".to_owned());
    a.insert("section", "cinema".to_owned());
    for ids_map in [&mut a, &mut b] {
        let mount: i64 = ids_map["mount_id"].parse().unwrap();
        let (obj, title, season, ep) = (
            ids.generate().get() as i64,
            ids.generate().get() as i64,
            ids.generate().get() as i64,
            ids.generate().get() as i64,
        );
        sqlx::query(
            "INSERT INTO storage_objects (id, mount_id, key, class, container, web_native, kind) \
             VALUES ($1, $2, 'Series/Authz/Authz S01E01.mp4', 'episode', 'mp4', TRUE, 'cinema')",
        )
        .bind(obj)
        .bind(mount)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO video_titles (id, kind, title, sort_title, published, mount_id, source, shelf_key) \
             VALUES ($1, 'series', 'Authz Show', 'authz show', FALSE, $2, 'mount', 'series/Authz')",
        )
        .bind(title)
        .bind(mount)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO video_seasons (id, title_id, number) VALUES ($1, $2, 1)")
            .bind(season)
            .bind(title)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO video_episodes (id, title_id, season_id, number, published, mount_id, source, mount_object_id) \
             VALUES ($1, $2, $3, 1, FALSE, $4, 'mount', $5)",
        )
        .bind(ep)
        .bind(title)
        .bind(season)
        .bind(mount)
        .bind(obj)
        .execute(&pool)
        .await
        .unwrap();
        ids_map.insert("shelf_item_id", title.to_string());
        ids_map.insert("shelf_part_id", ep.to_string());
    }
    // MUS-IDX-001/MUS-LIB-001 (wave 30): a listed track on each tenant's user mount (`{track_id}`, the member's
    // own music) and on its guild mount (`guild_track_id`, what the guild owner puts in a playlist), and a guild
    // playlist holding the guild track (`{playlist_id}`, `playlist_item_id`), for the `/music/*` routes and
    // their cross-tenant sweep.
    for (ids_map, guild) in [(&mut a, ga), (&mut b, gb)] {
        for (mount_slot, track_slot) in [
            ("mount_id", "track_id"),
            ("guild_mount_id", "guild_track_id"),
        ] {
            let mount: i64 = ids_map[mount_slot].parse().unwrap();
            let (obj, track) = (ids.generate().get() as i64, ids.generate().get() as i64);
            sqlx::query(
                "INSERT INTO storage_objects (id, mount_id, key, class, container, web_native, kind) \
                 VALUES ($1, $2, 'Music/Authz/01 Authz.mp3', 'track', 'mp3', TRUE, 'music')",
            )
            .bind(obj)
            .bind(mount)
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO music_tracks (id, mount_id, mount_object_id, title, artist, album, duration_ms) \
                 VALUES ($1, $2, $3, 'Authz', 'Matrix', 'Seeds', 1000)",
            )
            .bind(track)
            .bind(mount)
            .bind(obj)
            .execute(&pool)
            .await
            .unwrap();
            ids_map.insert(track_slot, track.to_string());
        }
        let (playlist, item) = (ids.generate().get() as i64, ids.generate().get() as i64);
        sqlx::query(
            "INSERT INTO music_playlists (id, owner_guild_id, name) VALUES ($1, $2, 'Authz playlist')",
        )
        .bind(playlist)
        .bind(guild)
        .execute(&pool)
        .await
        .unwrap();
        let track: i64 = ids_map["guild_track_id"].parse().unwrap();
        sqlx::query(
            "INSERT INTO music_playlist_items (id, playlist_id, track_id, position) VALUES ($1, $2, $3, 0)",
        )
        .bind(item)
        .bind(playlist)
        .bind(track)
        .execute(&pool)
        .await
        .unwrap();
        ids_map.insert("playlist_id", playlist.to_string());
        ids_map.insert("playlist_item_id", item.to_string());
    }
    // More objects, so that a refusal is not just "no such id": a moderation case, a takedown notice,
    // a hold, an announcement, a submission, a command, a grant and a running stage.
    let staff = Cred::Session(sess["staff_owner"].clone());
    let member_cred = Cred::Session(sess["member"].clone());
    step_up(&srv, &staff, &totp).await;
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
    // PRF-SCR-001: the scrapbook is hidden from servers until the operator offers it at platform scope; offer it,
    // and the loop below finds it in both servers' module lists.
    srv.call_reason(
        &staff,
        "PATCH",
        "/admin/features/lounge.scrapbook",
        Some(&json!({"enabled": true})),
    )
    .await;
    // PRF-RNG-001: status rings start off at platform scope; turn them on so the ring routes pass the gate.
    srv.call_reason(
        &staff,
        "PATCH",
        "/admin/features/lounge.rings",
        Some(&json!({"enabled": true})),
    )
    .await;
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
    let recipes: [(&'static str, &str, &Cred, Option<Value>); 9] = [
        (
            "sticker_id",
            "/guilds/{guild_id}/stickers",
            &oa,
            Some(json!({"name": "authzsticker", "tags": "authz", "file": png})),
        ),
        (
            "sound_id",
            "/guilds/{guild_id}/soundboard-sounds",
            &oa,
            Some(json!({"name": "authzsound", "sound": wav_uri(400)})),
        ),
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
            && let Some(id) = v["id"].as_str().or(v["sound_id"].as_str())
        {
            a.insert(param, id.to_owned());
        }
    }

    // RDY-SEC-012: two stage channels of server A, one with a running stage instance (the seeded
    // `stage_channel_id`) and one for the write phase to start a stage in (`stage_new_channel_id`).
    for (key, name) in [
        ("stage_channel_id", "stage-one"),
        ("stage_new_channel_id", "stage-two"),
    ] {
        if let Some(v) = create(
            &srv,
            &oa,
            "POST",
            "/guilds/{guild_id}/channels",
            &format!("/guilds/{ga}/channels"),
            infos,
            Some(json!({"name": name, "type": 13})),
        )
        .await
            && let Some(id) = v["id"].as_str()
        {
            a.insert(key, id.to_owned());
        }
    }
    if let Some(stage) = a.get("stage_channel_id").cloned() {
        create(
            &srv,
            &oa,
            "POST",
            "/stage-instances",
            "/stage-instances",
            infos,
            Some(json!({"channel_id": stage, "topic": "authz stage"})),
        )
        .await;
    }

    // UTL-COL-001, UTL-CRD-001, UTL-DEV-001 (wave 32): a tool room and an invite card in each tenant's channel
    // (made by its owner through the API, so the card is a real system message), a loot table of the server
    // (`guild_loot_table_id`) and one of the account (`table_id`: the member's in A, owner B's in B), and an
    // encrypted note of the same account (`note_id`). `{future_ms}` is a card start a week ahead for the
    // write phase's create body.
    let future_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
        + 7 * 24 * 3600 * 1000;
    for (ids_map, guild, owner, person) in [
        (&mut a, ga, &oa, u("member")),
        (&mut b, gb, &ob, u("ownerb")),
    ] {
        ids_map.insert("future_ms", future_ms.to_string());
        let chan = ids_map["channel_id"].clone();
        if let Some(v) = create(
            &srv,
            owner,
            "POST",
            "/channels/{channel_id}/tool-rooms",
            &format!("/channels/{chan}/tool-rooms"),
            infos,
            Some(json!({"kind": "tierlist", "title": "Authz tiers"})),
        )
        .await
            && let Some(id) = v["id"].as_str()
        {
            ids_map.insert("room_id", id.to_owned());
        }
        if let Some(v) = create(
            &srv,
            owner,
            "POST",
            "/channels/{channel_id}/cards",
            &format!("/channels/{chan}/cards"),
            infos,
            Some(json!({"kind": "game_night", "title": "Authz night", "starts_at": future_ms, "seats": 4})),
        )
        .await
            && let Some(id) = v["id"].as_str()
        {
            ids_map.insert("card_id", id.to_owned());
        }
        for (owner_col, owner_id, slot) in [
            ("owner_guild_id", guild, "guild_loot_table_id"),
            ("owner_user_id", person, "table_id"),
        ] {
            let table = ids.generate().get() as i64;
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "INSERT INTO loot_tables (id, {owner_col}, name, rows) \
                 VALUES ($1, $2, 'Authz loot', '[{{\"label\":\"Gold\",\"weight\":1}}]'::jsonb)"
            )))
            .bind(table)
            .bind(owner_id)
            .execute(&pool)
            .await
            .unwrap();
            ids_map.insert(slot, table.to_string());
        }
        let note = "00000000-0000-4000-8000-00000000a001";
        sqlx::query(
            "INSERT INTO scratch_notes (user_id, note_id, ciphertext) VALUES ($1, $2, '\\x01020304'::bytea)",
        )
        .bind(person)
        .bind(note)
        .execute(&pool)
        .await
        .unwrap();
        ids_map.insert("note_id", note.to_owned());
    }

    // MUS-JB-002/MUS-JB-003 (wave 31): the jukebox and listening-room routes need a voice channel
    // (`jukebox_channel_id`, mapped onto `{channel_id}` by `ctx_for`) with a session whose playing queue row
    // is the tenant's own track on its user mount. The write phase runs POST `ended` (a no-op this early),
    // `next` (plays the second row, `jukebox_item_id`, which `presign` then signs for the member), `skip`, and
    // later DELETE `queue/{item_id}` on the last row (`jukebox_queued_item_id`), still waiting by then.
    for (ids_map, guild, owner) in [(&mut a, ga, &oa), (&mut b, gb, &ob)] {
        let Some(v) = create(
            &srv,
            owner,
            "POST",
            "/guilds/{guild_id}/channels",
            &format!("/guilds/{guild}/channels"),
            infos,
            Some(json!({"name": "jukebox", "type": 2})),
        )
        .await
        else {
            continue;
        };
        let Some(vc) = v["id"].as_str().and_then(|s| s.parse::<i64>().ok()) else {
            continue;
        };
        let (track, mount): (i64, i64) = (
            ids_map["track_id"].parse().unwrap(),
            ids_map["mount_id"].parse().unwrap(),
        );
        let rows: Vec<i64> = (0..4).map(|_| ids.generate().get() as i64).collect();
        sqlx::query(
            "INSERT INTO jukebox_sessions (channel_id, guild_id, current_item_id, started_at) \
             VALUES ($1, $2, $3, now())",
        )
        .bind(vc)
        .bind(guild)
        .bind(rows[0])
        .execute(&pool)
        .await
        .unwrap();
        for (seq, row) in rows.iter().enumerate() {
            sqlx::query(
                "INSERT INTO jukebox_queue (id, channel_id, seq, track_id, mount_id, key, title, duration_ms, state) \
                 VALUES ($1, $2, $3, $4, $5, 'Music/Authz/01 Authz.mp3', 'Authz', 600000, $6)",
            )
            .bind(row)
            .bind(vc)
            .bind(seq as i64 + 1)
            .bind(track)
            .bind(mount)
            .bind(if seq == 0 { "playing" } else { "queued" })
            .execute(&pool)
            .await
            .unwrap();
        }
        // The same channel is a listening room on the tenant's playlist (GET reads its cursor; PUT and
        // DELETE in the write phase change and end it).
        let playlist: i64 = ids_map["playlist_id"].parse().unwrap();
        sqlx::query(
            "INSERT INTO listening_rooms (channel_id, guild_id, playlist_id) VALUES ($1, $2, $3)",
        )
        .bind(vc)
        .bind(guild)
        .bind(playlist)
        .execute(&pool)
        .await
        .unwrap();
        ids_map.insert("jukebox_channel_id", vc.to_string());
        ids_map.insert("jukebox_playing_item_id", rows[0].to_string());
        ids_map.insert("jukebox_item_id", rows[1].to_string());
        ids_map.insert("jukebox_queued_item_id", rows[3].to_string());
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
            && let Some(id) = pick_id(&v)
        {
            a.insert("upload_id", id);
        }
    }
    // RDY-SEC-011: the ids the write phase needs to reach real resources (published Cinema and Library
    // content, comments, lists, Live TV blocks, tokens), each under a key that `OVERLAYS` maps to a
    // placeholder. The staff step-up window is open (see above).
    {
        macro_rules! put {
            ($key:expr, $who:expr, $tpl:expr, $body:expr) => {{
                let tpl: String = $tpl;
                if let Some(id) = mk(&srv, infos, $who, &tpl, $body).await {
                    a.insert($key, id);
                }
            }};
        }
        let t = a.get("title_id").cloned().unwrap_or_default();
        let s = a.get("series_id").cloned().unwrap_or_default();
        let tv = a.get("tv_channel_id").cloned().unwrap_or_default();
        // A published film with a playable asset and a runtime: Live TV can place it, comments can be made.
        // The playable asset is a plain reference (no foreign key): the object store of this world cannot start an upload.
        let asset: Option<i64> = Some(ids.generate().get() as i64);
        sqlx::query("UPDATE video_titles SET published = true, published_at = now(), runtime_min = 90, asset_id = $2 WHERE id = $1")
            .bind(t.parse::<i64>().unwrap_or(0))
            .bind(asset)
            .execute(&pool)
            .await
            .unwrap();
        // A published series for the seasons and episodes (the film above is what Live TV plays).
        put!(
            "series_title_id",
            &staff,
            "/admin/cinema/titles".into(),
            Some(json!({"kind": "series", "title": "Authz show"}))
        );
        if let Some(st) = a.get("series_title_id").and_then(|x| x.parse::<i64>().ok()) {
            sqlx::query(
                "UPDATE video_titles SET published = true, published_at = now() WHERE id = $1",
            )
            .bind(st)
            .execute(&pool)
            .await
            .unwrap();
        }
        let st = a.get("series_title_id").cloned().unwrap_or_default();
        put!(
            "season_id",
            &staff,
            format!("/admin/cinema/titles/{st}/seasons"),
            None
        );
        if let Some(se) = a.get("season_id").cloned() {
            put!(
                "episode_id",
                &staff,
                format!("/admin/cinema/seasons/{se}/episodes"),
                None
            );
        }
        if let Some(e) = a.get("episode_id").and_then(|x| x.parse::<i64>().ok()) {
            sqlx::query(
                "UPDATE video_episodes SET published = true, published_at = now() WHERE id = $1",
            )
            .bind(e)
            .execute(&pool)
            .await
            .unwrap();
        }
        put!(
            "trailer_id",
            &staff,
            format!("/admin/cinema/titles/{t}/trailers"),
            None
        );
        put!(
            "variant_id",
            &staff,
            format!("/admin/cinema/titles/{t}/variants"),
            None
        );
        put!(
            "merge_title_id",
            &staff,
            "/admin/cinema/titles".into(),
            None
        );
        put!(
            "merge_series_id",
            &staff,
            "/admin/library/series".into(),
            None
        );
        put!(
            "collection_id",
            &staff,
            "/admin/cinema/collections".into(),
            None
        );
        mk(
            &srv,
            infos,
            &staff,
            &format!("/admin/library/series/{s}/publish"),
            None,
        )
        .await;
        if let Some(id) = mk(
            &srv,
            infos,
            &bot_cred,
            &format!("/applications/{aid}/guilds/{ga}/commands"),
            Some(json!({"name": "guildping", "description": "ping"})),
        )
        .await
        {
            a.insert("guild_command_id", id);
        }
        put!(
            "edition_id",
            &staff,
            format!("/admin/library/series/{s}/editions"),
            Some(json!({"language": "en", "label": "Official"}))
        );
        if let Some(e) = a.get("edition_id").cloned() {
            put!(
                "volume_id",
                &staff,
                format!("/admin/library/editions/{e}/volumes"),
                None
            );
            put!(
                "lib_chapter_id",
                &staff,
                format!("/admin/library/editions/{e}/chapters"),
                None
            );
        }
        if let Some(c) = a.get("lib_chapter_id").cloned() {
            sqlx::query(
                "UPDATE reading_chapters SET published = true, published_at = now() WHERE id = $1",
            )
            .bind(c.parse::<i64>().unwrap_or(0))
            .execute(&pool)
            .await
            .unwrap();
            put!(
                "lib_upload_id",
                &staff,
                format!("/admin/library/chapters/{c}/uploads"),
                Some(json!({"filename": "a.zip", "size_bytes": 10}))
            );
            put!(
                "lib_upload2_id",
                &staff,
                format!("/admin/library/chapters/{c}/uploads"),
                Some(json!({"filename": "b.zip", "size_bytes": 10}))
            );
            put!(
                "library_comment_id",
                &member_cred,
                format!("/library/chapters/{c}/comments"),
                None
            );
        }
        put!(
            "cinema_comment_id",
            &member_cred,
            format!("/cinema/titles/{t}/comments"),
            None
        );
        // Reviews (the member's), and a report of each comment by someone else.
        if let Some(id) = mk_m(
            &srv,
            infos,
            &member_cred,
            "PUT",
            &format!("/cinema/titles/{t}/review"),
            None,
        )
        .await
        {
            a.insert("cinema_review_id", id);
        }
        if let Some(id) = mk_m(
            &srv,
            infos,
            &member_cred,
            "PUT",
            &format!("/library/series/{s}/review"),
            None,
        )
        .await
        {
            a.insert("library_review_id", id);
        }
        if let Some(c) = a.get("cinema_comment_id").cloned() {
            put!(
                "cinema_report_id",
                &oa,
                format!("/cinema/comments/{c}/report"),
                None
            );
        }
        if let Some(c) = a.get("library_comment_id").cloned() {
            put!(
                "library_report_id",
                &oa,
                format!("/library/comments/{c}/report"),
                None
            );
        }
        put!("cinema_list_id", &member_cred, "/cinema/lists".into(), None);
        put!(
            "library_list_id",
            &member_cred,
            "/library/lists".into(),
            None
        );
        put!(
            "source_id",
            &staff,
            "/admin/ingest-sources".into(),
            Some(
                json!({"label": "authz", "kind": "storage_import", "backend": "storage", "location": "imports/authz", "owner_id": u("staffowner").to_string()})
            )
        );
        put!("token_id", &staff, "/admin/transcode/tokens".into(), None);
        if !tv.is_empty() {
            put!(
                "block_id",
                &staff,
                format!("/admin/tv/channels/{tv}/blocks"),
                Some(
                    json!({"title_id": t.clone(), "start_ms": clock.now_unix_ms() + 2 * 86_400_000})
                )
            );
            mk(
                &srv,
                infos,
                &staff,
                &format!("/admin/tv/channels/{tv}/publish"),
                None,
            )
            .await;
        }
    }
    // A watch party of the published film, and an Activity bundle (a one-file zip, sent as bytes).
    if let Some(title) = a.get("title_id").cloned()
        && let Some(v) = create(
            &srv,
            &member_cred,
            "POST",
            "/watch-parties",
            "/watch-parties",
            infos,
            Some(json!({"title_id": title, "channel_id": a["channel_id"]})),
        )
        .await
        && let Some(id) = v["id"].as_str()
    {
        a.insert("party_id", id.to_owned());
    }
    let zipped = tiny_zip();
    let r = srv
        .call_bytes(
            &appowner_cred,
            "POST",
            &format!("/applications/{aid}/activity/bundles"),
            "application/zip",
            &zipped,
        )
        .await;
    if let Some(id) = serde_json::from_str::<Value>(&r.body)
        .ok()
        .as_ref()
        .and_then(pick_id)
    {
        a.insert("bundle_id", id);
    } else {
        println!(
            "authz_matrix: seed activity bundle -> {} {}",
            r.status,
            r.body.chars().take(100).collect::<String>()
        );
    }
    // RDY-SEC-011: the per-account objects of the write phase's rightful actors (the plain member's own
    // reminders, bookmarks and lists, the owner's templates, the invitee's offers ...).
    {
        sqlx::query("UPDATE accounts SET created_at = now() - interval '400 days' WHERE id = $1")
            .bind(u("member"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO invite_credits_ledger (id, account_id, delta, reason) VALUES ($1, $2, 3, 'staff_grant')")
            .bind(ids.generate().get() as i64)
            .bind(u("member"))
            .execute(&pool)
            .await
            .unwrap();
        let insider_cred = Cred::Session(sess["insider"].clone());
        let outsider_cred = Cred::Session(sess["outsider"].clone());
        let party = a.get("party_id").cloned().unwrap_or_default();
        let msg = a.get("message_id").cloned().unwrap_or_default();
        let ch = a.get("channel_id").cloned().unwrap_or_default();
        let t = a.get("title_id").cloned().unwrap_or_default();
        let lib_ch = a.get("lib_chapter_id").cloned().unwrap_or_default();
        // The feature ids that exist, one list per family.
        for (key, who, path) in [
            (
                "channel_feature_id",
                &oa,
                format!("/channels/{ch}/sr/features"),
            ),
            (
                "user_feature_id",
                &member_cred,
                "/users/@me/sr/features".to_owned(),
            ),
            ("admin_feature_id", &staff, "/admin/features".to_owned()),
        ] {
            let r = srv.call(who, "GET", &path, None).await;
            let v: Value = serde_json::from_str(&r.body).unwrap_or(Value::Null);
            if let Some(id) = v
                .as_array()
                .and_then(|l| l.first())
                .and_then(|f| f["id"].as_str())
            {
                a.insert(key, id.to_owned());
            }
        }
        a.insert("guild_feature_id", "community.suggestions".to_owned());
        // The report intake is a platform module that is off until staff switch it on.
        srv.call(
            &staff,
            "PATCH",
            "/admin/features/moderation.reports",
            Some(&json!({"enabled": true})),
        )
        .await;
        // The member's own things.
        if let Some(id) = mk(
            &srv,
            infos,
            &member_cred,
            &format!("/channels/{ch}/scheduled-messages"),
            Some(json!({"content": "authz", "send_at": rfc3339_in_days(30)})),
        )
        .await
        {
            a.insert("sched_id", id);
        }
        if let Some(id) = mk(
            &srv,
            infos,
            &member_cred,
            "/users/@me/reminders",
            Some(json!({"note": "authz", "in_seconds": 86400})),
        )
        .await
        {
            a.insert("reminder_id", id);
        }
        let b1 = srv
            .call(
                &member_cred,
                "PUT",
                &format!("/channels/{ch}/messages/{msg}/bookmark"),
                None,
            )
            .await;
        let b2 = srv
            .call(
                &member_cred,
                "PUT",
                &format!("/library/chapters/{lib_ch}/bookmarks"),
                Some(&json!({"page_index": 0})),
            )
            .await;
        if let Some(id) = serde_json::from_str::<Value>(&b2.body)
            .ok()
            .as_ref()
            .and_then(pick_id)
        {
            a.insert("bookmark_id", id);
        } else {
            println!(
                "authz_matrix: seed library bookmark -> {} {}",
                b2.status,
                b2.body.chars().take(100).collect::<String>()
            );
        }
        let _ = b1;
        let png_bytes = make_png(8, 8);
        let r = srv
            .call_bytes(
                &member_cred,
                "POST",
                "/users/@me/profile-media?kind=gallery",
                "application/octet-stream",
                &png_bytes,
            )
            .await;
        if let Some(id) = serde_json::from_str::<Value>(&r.body)
            .ok()
            .as_ref()
            .and_then(pick_id)
        {
            a.insert("media_id", id);
        } else {
            println!(
                "authz_matrix: seed profile media -> {} {}",
                r.status,
                r.body.chars().take(100).collect::<String>()
            );
        }
        let r = srv
            .call(&member_cred, "POST", "/sr/invites", Some(&json!({})))
            .await;
        if let Some(code) = serde_json::from_str::<Value>(&r.body)
            .ok()
            .and_then(|v| v["code"].as_str().map(str::to_owned))
        {
            a.insert("sr_invite_code", code);
        } else {
            println!(
                "authz_matrix: seed member invite -> {} {}",
                r.status,
                r.body.chars().take(100).collect::<String>()
            );
        }
        // A reaction and a pin, so that removing them is a real removal.
        srv.call(
            &oa,
            "PUT",
            &format!("/channels/{ch}/messages/{msg}/reactions/%F0%9F%91%8D/@me"),
            None,
        )
        .await;
        srv.call(&oa, "PUT", &format!("/channels/{ch}/pins/{msg}"), None)
            .await;
        // A template, a directory listing, an application to join, a ticket, an upload slot.
        let r = srv
            .call(
                &oa,
                "POST",
                &format!("/guilds/{ga}/templates"),
                Some(&json!({"name": "authz template"})),
            )
            .await;
        if let Some(code) = serde_json::from_str::<Value>(&r.body)
            .ok()
            .and_then(|v| v["code"].as_str().map(str::to_owned))
        {
            a.insert("template_code", code);
        } else {
            println!(
                "authz_matrix: seed template -> {} {}",
                r.status,
                r.body.chars().take(100).collect::<String>()
            );
        }
        sqlx::query("INSERT INTO guild_community (guild_id, community_enabled) VALUES ($1, true) ON CONFLICT (guild_id) DO UPDATE SET community_enabled = true")
            .bind(ga)
            .execute(&pool)
            .await
            .unwrap();
        srv.call(
            &oa,
            "PUT",
            &format!("/guilds/{ga}/sr/applications/form"),
            Some(&json!({"enabled": true, "questions": []})),
        )
        .await;
        if let Some(id) = mk(
            &srv,
            infos,
            &outsider_cred,
            &format!("/guilds/{ga}/sr/applications"),
            Some(json!({"answers": []})),
        )
        .await
        {
            a.insert("guild_application_id", id);
        }
        if let Some(id) = mk(
            &srv,
            infos,
            &oa,
            &format!("/guilds/{ga}/tickets"),
            Some(json!({"subject": "authz ticket"})),
        )
        .await
        {
            a.insert("ticket_id", id);
        }
        if let Some(id) = mk(
            &srv,
            infos,
            &oa,
            &format!("/channels/{ch}/attachments"),
            Some(json!({"files": [{"filename": "a.txt", "file_size": 3, "id": "0"}]})),
        )
        .await
        {
            a.insert("attach_upload_id", id);
        }
        if let Some(id) = mk(
            &srv,
            infos,
            &appowner_cred,
            &format!("/applications/{aid}/emojis"),
            Some(json!({"name": "authzapp", "image": png})),
        )
        .await
        {
            a.insert("app_emoji_id", id);
        }
        if let Some(id) = mk(
            &srv,
            infos,
            &appowner_cred,
            &format!("/applications/{aid}/testers"),
            Some(json!({"username": "victim"})),
        )
        .await
        {
            a.insert("tester_invite_id", id);
        }
        if let Some(id) = mk(
            &srv,
            infos,
            &staff,
            "/admin/staff/offers",
            Some(json!({"account_id": u("offeree").to_string(), "role_ids": [staff_roles["Support"].to_string()], "reason": "authz"})),
        )
        .await
        {
            a.insert("staff_offer_id", id);
        }
        {
            let offeree_cred = Cred::Session(sess["offeree"].clone());
            let now = SystemClock.now_unix_ms() as u64 / 1000;
            let code =
                sr_auth::totp::current_code(&totp, "SIDE ROOM", "offeree", now).unwrap_or_default();
            let r = srv
                .call(
                    &offeree_cred,
                    "POST",
                    "/sr/auth/reauth",
                    Some(&json!({"method": "totp", "code": code})),
                )
                .await;
            if r.status != 200 && r.status != 204 {
                println!(
                    "authz_matrix: seed offeree reauth -> {} {}",
                    r.status,
                    r.body.chars().take(100).collect::<String>()
                );
            }
        }
        // Watch party: a queued item and a second participant.
        if !party.is_empty() {
            let j = srv
                .call(
                    &insider_cred,
                    "POST",
                    &format!("/watch-parties/{party}/join"),
                    Some(&json!({})),
                )
                .await;
            if j.status != 200 {
                println!(
                    "authz_matrix: seed party join -> {} {}",
                    j.status,
                    j.body.chars().take(140).collect::<String>()
                );
            }
            srv.call(
                &member_cred,
                "POST",
                &format!("/watch-parties/{party}/queue"),
                Some(&json!({"title_id": t})),
            )
            .await;
            // Two items: the write phase's `next` plays the first.
            let q = srv
                .call(
                    &member_cred,
                    "POST",
                    &format!("/watch-parties/{party}/queue"),
                    Some(&json!({"title_id": t})),
                )
                .await;
            let pv: Value = serde_json::from_str(&q.body).unwrap_or(Value::Null);
            if let Some(id) = pv["queue"][1]["id"].as_str() {
                a.insert("queue_item_id", id.to_owned());
            } else {
                println!(
                    "authz_matrix: seed queue item -> {} {}",
                    q.status,
                    q.body.chars().take(140).collect::<String>()
                );
            }
        }
        // A temporary role, an invoice.
        srv.call(
            &oa,
            "POST",
            &format!("/guilds/{ga}/sr/temp-roles"),
            Some(&json!({"user_id": victim, "role_id": a["role_id"], "duration_seconds": 86400})),
        )
        .await;
        if let Some(id) = mk(
            &srv,
            infos,
            &member_cred,
            "/donations/invoices",
            Some(json!({"purpose": "invite_credit", "credits": 1})),
        )
        .await
        {
            a.insert("invoice_id", id);
        }
    }
    // RDY-SEC-011: packages (submitted by the member, decided by the Owner), reports, evasion signals,
    // alerts, a data request, a guestbook entry, a second webhook and more reactions.
    {
        let t = a.get("title_id").cloned().unwrap_or_default();
        a.insert("insider_id", u("insider").to_string());
        a.insert("key", sr_admin::ENTRANCE_COPY_KEYS[0].to_owned());
        a.insert("member_id", u("member").to_string());
        a.insert("staff_target_id", u("friendtwo").to_string());
        a.insert("owner_b_id", u("ownerb").to_string());
        a.insert("support_role_id", staff_roles["Support"].to_string());
        if let Some(id) = mk(
            &srv,
            infos,
            &staff,
            "/submissions",
            Some(json!({"kind": "library.series", "payload": {"title": "Owner series", "kind": "manga"}, "title": "Owner series", "summary": "authz matrix"})),
        )
        .await
        {
            a.insert("own_submission_id", id);
        }
        let theme = |id: &str| {
            let m = json!({"id": id, "name": id, "version": "1.0.0", "author": "me", "targets": ["lounge"], "scheme": "dark"});
            zip_of(&[
                ("theme.json", m.to_string().as_bytes()),
                (
                    "tokens.json",
                    json!({"--ui-accent": "#5865f2"}).to_string().as_bytes(),
                ),
            ])
        };
        let pack = {
            let m = json!({"kind": "emoji_pack", "id": "faces", "name": "Faces", "version": "1.0.0", "author": "me",
                           "items": [{"name": "wave", "file": "emoji/wave.png"}]});
            zip_of(&[
                ("pack.json", m.to_string().as_bytes()),
                ("emoji/wave.png", &make_png(32, 32)),
            ])
        };
        let recipe = {
            let m = json!({"id": "starter", "name": "Starter", "version": "1.0.0", "author": "me",
                           "features": [{"id": "lounge.starboard", "enabled": true}]});
            zip_of(&[("recipe.json", m.to_string().as_bytes())])
        };
        // (key for the package, key for its version, zip, approved)
        let uploads: [(&'static str, &'static str, Vec<u8>, bool); 5] = [
            ("package_id", "approved_version_id", theme("ocean"), true),
            ("pack_id", "pack_version_id", pack, true),
            ("recipe_id", "recipe_version_id", recipe, true),
            (
                "pending_package_id",
                "pending_version_id",
                theme("pending-one"),
                false,
            ),
            (
                "withdraw_package_id",
                "withdraw_version_id",
                theme("pending-two"),
                false,
            ),
        ];
        for (pkey, vkey, zip, approve) in uploads {
            let r = srv
                .call_bytes(&member_cred, "POST", "/packages", "application/zip", &zip)
                .await;
            let v: Value = serde_json::from_str(&r.body).unwrap_or(Value::Null);
            let v = if v["version"].is_object() {
                v["version"].clone()
            } else {
                v
            };
            let (Some(pid), Some(vid)) = (v["package_id"].as_str(), v["id"].as_str()) else {
                println!(
                    "authz_matrix: seed package {pkey} -> {} {}",
                    r.status,
                    r.body.chars().take(140).collect::<String>()
                );
                continue;
            };
            a.insert(pkey, pid.to_owned());
            a.insert(vkey, vid.to_owned());
            if approve {
                let d = srv
                    .call(
                        &staff,
                        "POST",
                        &format!("/admin/developers/packages/versions/{vid}/decision"),
                        Some(&json!({"decision": "approve"})),
                    )
                    .await;
                if !(200..300).contains(&d.status) {
                    println!(
                        "authz_matrix: seed package decision -> {} {}",
                        d.status,
                        d.body.chars().take(100).collect::<String>()
                    );
                }
            }
        }
        // Reports of a comment (the member made the comments; the owner reports them).
        if let Some(c) = a.get("cinema_comment_id").cloned() {
            srv.call(
                &oa,
                "POST",
                &format!("/cinema/comments/{c}/report"),
                Some(&json!({"reason": "authz"})),
            )
            .await;
        }
        if let Some(c) = a.get("library_comment_id").cloned() {
            srv.call(
                &oa,
                "POST",
                &format!("/library/comments/{c}/report"),
                Some(&json!({"reason": "authz"})),
            )
            .await;
        }
        for (key, sql) in [
            (
                "cinema_report_id",
                "SELECT id FROM cinema_reports ORDER BY id LIMIT 1",
            ),
            (
                "library_report_id",
                "SELECT id FROM reading_comment_reports ORDER BY id LIMIT 1",
            ),
        ] {
            if let Ok(id) = sqlx::query_scalar::<_, i64>(sql).fetch_one(&pool).await {
                a.insert(key, id.to_string());
            }
        }
        // Two evasion signals between the victim's Person and the member's, and a staff alert.
        let member_person: i64 = sqlx::query_scalar("SELECT person_id FROM accounts WHERE id = $1")
            .bind(u("member"))
            .fetch_one(&pool)
            .await
            .unwrap();
        for (key, kind) in [
            ("signal_id", "shared_login_ip"),
            ("signal2_id", "shared_invite_lineage"),
        ] {
            let id = ids.generate().get() as i64;
            sqlx::query("INSERT INTO evasion_signals (id, person_id, related_person_id, kind) VALUES ($1, $2, $3, $4)")
                .bind(id)
                .bind(person)
                .bind(member_person)
                .bind(kind)
                .execute(&pool)
                .await
                .unwrap();
            a.insert(key, id.to_string());
        }
        let alert = ids.generate().get() as i64;
        sqlx::query(
            "INSERT INTO staff_alerts (id, kind, detail) VALUES ($1, 'authz', 'authz matrix')",
        )
        .bind(alert)
        .execute(&pool)
        .await
        .unwrap();
        a.insert("alert_id", alert.to_string());
        // A data request about the victim.
        if let Some(id) = mk(
            &srv,
            infos,
            &staff,
            "/admin/data-requests",
            Some(json!({"kind": "export", "account_id": victim})),
        )
        .await
        {
            a.insert("data_request_id", id);
        }
        if let Some(id) = mk(
            &srv,
            infos,
            &staff,
            "/admin/donations/entitlements",
            Some(
                json!({"kind": "patron", "days": 30, "user_id": victim, "reason": "authz matrix"}),
            ),
        )
        .await
        {
            a.insert("entitlement_grant_id", id);
        }
        if let Ok(id) =
            sqlx::query_scalar::<_, i64>("SELECT id FROM platform_invites ORDER BY id LIMIT 1")
                .fetch_one(&pool)
                .await
        {
            a.insert("platform_invite_id", id.to_string());
        }
        // PRF-PRO-001 (D-382): a profile is private until its owner chooses. The victim opens a public page with a
        // public guestbook that takes held notes, the operator allows public pages, and one note waits for
        // approval (the placeholder `pending_entry_id`); `handle` names the victim's public page.
        let victim_cred = Cred::Session(sess["victim"].clone());
        srv.call(
            &staff,
            "PATCH",
            "/admin/features/platform.public_profiles",
            Some(&json!({"enabled": true})),
        )
        .await;
        srv.call(
            &victim_cred,
            "PUT",
            "/users/@me/profile-doc",
            Some(&json!({"version": 1, "blocks": [
                {"id": "gb", "type": "guestbook", "x": 0, "y": 0, "w": 12, "h": 4, "config": {}}
            ]})),
        )
        .await;
        srv.call(
            &victim_cred,
            "PUT",
            "/users/@me/profile-privacy",
            Some(
                &json!({"page_tier": "public", "links_tier": "public", "bio_tier": "public",
                          "guestbook_tier": "public", "guestbook_accepts_public": true}),
            ),
        )
        .await;
        srv.call(
            &Cred::Anon,
            "POST",
            "/public/profiles/victim/guestbook",
            Some(&json!({"body": "authz held note"})),
        )
        .await;
        let held = srv
            .call(
                &victim_cred,
                "GET",
                &format!("/users/{victim}/guestbook"),
                None,
            )
            .await;
        if let Some(id) = serde_json::from_str::<Value>(&held.body)
            .ok()
            .and_then(|v| {
                v.as_array()?
                    .iter()
                    .find(|e| e["pending"] == true)
                    .and_then(|e| e["id"].as_str().map(str::to_owned))
            })
        {
            a.insert("pending_entry_id", id);
        }
        a.insert("handle", "victim".to_owned());
        // A guestbook entry the member wrote on the victim's board.
        if let Some(id) = mk(
            &srv,
            infos,
            &member_cred,
            &format!("/users/{victim}/guestbook"),
            Some(json!({"body": "authz"})),
        )
        .await
        {
            a.insert("entry_id", id);
        }
        // A second webhook (the token-addressed delete removes it; the id-addressed routes use the first),
        // and reactions to remove one way each.
        let h2 = srv
            .must(
                &oa,
                "POST",
                &format!("/channels/{chan_a}/webhooks"),
                json!({"name": "hook two"}),
            )
            .await;
        a.insert("webhook2_id", h2["id"].as_str().unwrap_or(BOGUS).to_owned());
        a.insert("token2", h2["token"].as_str().unwrap_or("x").to_owned());
        let ch = a.get("channel_id").cloned().unwrap_or_default();
        let msg = a.get("message_id").cloned().unwrap_or_default();
        for emoji in ["%F0%9F%98%80", "%F0%9F%8E%89"] {
            srv.call(
                &oa,
                "PUT",
                &format!("/channels/{ch}/messages/{msg}/reactions/{emoji}/@me"),
                None,
            )
            .await;
        }
        let _ = t;
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

    {
        let mut keys: Vec<&str> = a.keys().copied().collect();
        keys.sort_unstable();
        println!("authz_matrix: seeded {}", keys.join(" "));
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
            totp,
            users,
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
const TENANT: [&str; 39] = [
    "sticker_id",
    "sound_id",
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
    "mount_id",
    // BYOS-BRW-001 (byos-browser)
    "mount_upload_id",
    "shelf_item_id",
    "shelf_part_id",
    // BYOS-CNV-003 (conv-worker): a worker token scoped to the tenant's mount
    "worker_token_id",
    // MUS-IDX-001/MUS-LIB-001 (wave 30)
    "track_id",
    "playlist_id",
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
            // BYOS-BRW-001 (byos-browser): a part number that exists in the seeded upload.
            "part_number" => "1".to_owned(),
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
        // The client learns from `member: false` that it was dropped (a sleeping laptop, a removal) and says so.
        // A stranger gets the same answer with a neutral playback and nothing about the party; the service test
        // `a_heartbeat_from_a_stranger_reveals_no_playback` pins that.
        ("POST", "/watch-parties/{party_id}/heartbeat") => Some(
            "a non-member is told member:false with a neutral playback, the dropped participant's signal",
        ),
        _ => None,
    }
}

/// Hand-made bodies for the routes whose sampled body is refused as malformed before the
/// permission check could be seen, and for the escalation attempts that matter most (a role with
/// administrator rights, a role handed to someone else, a rename). `{name}` placeholders are
/// replaced with the seeded ids of tenant A.
fn body_override(method: &str, path: &str) -> Option<&'static str> {
    Some(match (method, path) {
        // BYOS-SCP-002: a presign for an audio key, a grants body naming the seeded role.
        ("POST", "/storage/mounts/{mount_id}/presign") => r#"{"key":"a.mp3","method":"GET"}"#,
        // BYOS-BRW-001 (byos-browser): the write side of the bucket browser.
        ("POST", "/storage/mounts/{mount_id}/uploads") => {
            r#"{"key":"Music/a.flac","size":1048576,"content_type":"audio/flac"}"#
        }
        ("POST", "/storage/mounts/{mount_id}/uploads/{mount_upload_id}/complete") => {
            r#"{"parts":[]}"#
        }
        ("POST", "/admin/storage/mounts/{platform_mount_id}/attach")
        | ("POST", "/admin/storage/mounts/{platform_mount_id}/detach") => {
            r#"{"guild_id":"{guild_id}"}"#
        }
        ("POST", "/storage/mounts/{mount_id}/folders") => r#"{"key":"Albums/"}"#,
        // BYOS-CNV-003 (conv-worker, own block): a conversion of a video key, a mount-scoped worker token.
        ("POST", "/storage/mounts/{mount_id}/transcode") => {
            r#"{"key":"Films/a.mkv","format":"mp4","preset":"low"}"#
        }
        ("POST", "/storage/mounts/{mount_id}/worker-tokens") => r#"{"name":"Matrix"}"#,
        ("POST", "/storage/mounts/{mount_id}/move") => r#"{"from":"a.mp3","to":"b.mp3"}"#,
        // PRF-SCR-001: choose the server's own shelf; retag the seeded entry.
        ("PUT", "/guilds/{guild_id}/scrapbook/target") => r#"{"mount_id":"{guild_mount_id}"}"#,
        // BRG-COR-001: a token-shaped body; the bridge's Discord is a closed local port, so the owner's call is
        // admitted and fails at Discord (409). The matrix never reaches a real Discord.
        ("PUT", "/guilds/{guild_id}/bridge/token") => r#"{"token":"authz.fake.token"}"#,
        ("POST", "/guilds/{guild_id}/bridge/maps") => {
            r#"{"channel_id":"{channel_id}","discord_channel_id":"123","direction":"both"}"#
        }
        // The seeded map only receives: keeping it so needs no webhook, so nothing reaches Discord.
        ("PATCH", "/guilds/{guild_id}/bridge/maps/{map_id}") => r#"{"direction":"from_discord"}"#,
        ("PATCH", "/guilds/{guild_id}/scrapbook/{entry_id}") => r#"{"tags":["authz"]}"#,
        // PRF-RNG-001: a text ring for the caller's friends.
        ("POST", "/users/@me/rings") => r#"{"kind":"text","text":"authz","audience":"friends"}"#,
        ("PUT", "/guilds/{guild_id}/storage/grants") => {
            r#"{"role_id":"{role_id}","grants":["view_music"]}"#
        }
        ("PUT", "/users/@me/storage/mounts/{mount_id}/shares/{user_id}") => {
            r#"{"cinema":false,"library":false,"music":true}"#
        }
        // BYOS-MNT-001: a second bucket (a new label: "Shelf" is taken). S3 with the default endpoint: the SSRF policy
        // refuses http and loopback (the seeded rows skip it), so this resolves the public AWS name; nothing connects.
        ("POST", "/users/@me/storage/mounts") | ("POST", "/guilds/{guild_id}/storage/mounts") => {
            r#"{"label":"Matrix","provider":"s3","region":"us-east-1","bucket":"matrix","path_style":true,"access_key_id":"AKIAAUTHZMATRIX","secret_access_key":"authz-matrix-secret"}"#
        }
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
    rightful_of(r)
}

/// The actor who is rightfully allowed on the route (RDY-SEC-011): the one the write phase calls it as.
fn rightful_of(r: &Route) -> &'static str {
    if let Some(a) = rightful_override(&r.method, &r.path) {
        a
    } else if r.access != "authenticated" {
        "anon"
    } else if r.path.contains("/commands") && r.path.starts_with("/applications/") {
        // Slash command management is the bot's (the owner's session is refused: "Only bots").
        "bot_in"
    } else if r.authz == "staff" || r.path.starts_with("/moderation") {
        "staff_owner"
    } else if r.authz == "owner" {
        "app_owner"
    } else if r.path.contains("/call/") || r.path.starts_with("/packages") {
        "member"
    } else if (r.path.starts_with("/cinema/") || r.path.starts_with("/library/"))
        || r.path.starts_with("/users/{user_id}/guestbook")
    {
        // Their lists, comments and posts are the plain member's.
        "member"
    } else if r.authz == "storage_scope" || r.authz == "mount_owner" {
        // The seeded mounts are the plain member's.
        "member"
    } else if r.authz == "self" || r.authz == "user" {
        // Per-account data: the plain member's, which the seed filled.
        "member"
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
                | "storage_scope"
                | "mount_owner"
                | "guild_owner"
                | "bridge_admin"
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
                // BRG-COR-001: a plain member is neither the owner nor a manager of the server.
                || matches!(r.authz.as_str(), "guild_owner" | "bridge_admin")
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
        if !is_write(&r.method) || r.authz == "guild_owner" {
            calls.push(std(
                "mod",
                if staff || r.authz == "guild_owner" {
                    Exp::Deny
                } else {
                    Exp::Observe
                },
            ));
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
const NOISY: [&str; 15] = [
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
    // UTL-CALC-001: the hourly ECB rates job records its attempt (and, online, the rates) at any moment.
    "fx_rates",
    "fx_fetch_state",
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

/// The columns of `listening_rooms` the cursor job writes (MUS-JB-003).
const LISTENING_ROOM_CURSOR: [&str; 6] = [
    "item_id",
    "position_ms",
    "cursor_at",
    "seq",
    "next_due_at",
    "last_published_at",
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
        let mut strip: String = STAMPS.iter().map(|c| format!(" - '{c}'")).collect();
        // MUS-JB-003: the `jukebox.rooms` job moves a listening room's cursor every tick, at times of its
        // choosing; the room itself (channel, playlist, mode, DJ) is still fingerprinted.
        if t == "listening_rooms" {
            for c in LISTENING_ROOM_CURSOR {
                strip.push_str(&format!(" - '{c}'"));
            }
        }
        // The system user's direct messages (a staff offer, a tester invite) are written and updated by the
        // server's own tasks at times of their choosing; no refused call is theirs.
        let own = if t == "messages" || t.starts_with("messages_") {
            format!(" WHERE x.author_id <> {SYSTEM_USER_ID}")
        } else {
            String::new()
        };
        let sql = format!(
            "SELECT md5(coalesce(string_agg((to_jsonb(x){strip})::text, '|' ORDER BY (to_jsonb(x){strip})::text), '')) FROM \"{t}\" x{own}"
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

// ---------------------------------------------------------------------------------------------
// RDY-SEC-011, RDY-SEC-012: the write side, the allow phase.
//
// After the refusal and read phases of a group, every write route of the group is called once as its
// rightful actor and must be *admitted*: any status but 401, 403 and 404 (and no 5xx, which is a handler
// bug). The calls go in dependency order, so that a rightful delete does not destroy the world for the
// rows after it: creates (POST), then edits (PUT, PATCH), then deletes from the leaf to the root (a message
// before its channel before its server). A 400 or 422 is admitted but reported with the body: the request
// shape was refused before the permission check could be seen, and a hand-made body is missing.

/// Write routes the allow phase cannot assert yet, each with the reason. The table can only shrink:
/// `write_observed_is_pinned` pins its length, and a route listed here that turns out to be admitted fails
/// the run until it is removed (and the pin lowered).
const WRITE_OBSERVED: &[(&str, &str, &str)] = &[
    (
        "POST",
        "/moderation/appeals",
        "an appeal is opened with the token mailed to a banned account",
    ),
    (
        "POST",
        "/moderation/appeals/{appeal_id}/decision",
        "an appeal is opened with the token mailed to a banned account",
    ),
    (
        "POST",
        "/admin/card-actions",
        "needs a posted alert card, a message carrying an sr: button",
    ),
    (
        "POST",
        "/admin/cinema/assets/{asset_id}/complete",
        "needs a Cinema asset held by the object store; the test stack's store cannot start an upload (RDY-SEC-011)",
    ),
    (
        "POST",
        "/admin/cinema/assets/{asset_id}/probe",
        "needs a Cinema asset held by the object store; the test stack's store cannot start an upload (RDY-SEC-011)",
    ),
    (
        "POST",
        "/admin/cinema/assets/{asset_id}/transcode",
        "needs a Cinema asset held by the object store; the test stack's store cannot start an upload (RDY-SEC-011)",
    ),
    (
        "POST",
        "/admin/jobs/{job_id}/retry",
        "needs a dead job, a running transcode job or a worker, which only a transcoder worker creates",
    ),
    (
        "POST",
        "/admin/persons/{person_id}/identities/{identity_id}/move",
        "needs a linked identity row (an external login) of the person",
    ),
    (
        "POST",
        "/admin/transcode/jobs/{job_id}/cancel",
        "needs a dead job, a running transcode job or a worker, which only a transcoder worker creates",
    ),
    (
        "PUT",
        "/admin/cinema/assets/{asset_id}/original-downloads",
        "needs a Cinema asset held by the object store; the test stack's store cannot start an upload (RDY-SEC-011)",
    ),
    (
        "PUT",
        "/admin/library/chapters/{chapter_id}/pages/{index}",
        "needs a stored page; pages are objects written by the ingest under library/ keys",
    ),
    (
        "PUT",
        "/admin/transcode/workers/{worker_id}/drain",
        "needs a dead job, a running transcode job or a worker, which only a transcoder worker creates",
    ),
    (
        "DELETE",
        "/admin/library/uploads/{upload_id}",
        "the second seeded admin upload answers Unknown upload (unresolved, see the handoff)",
    ),
    (
        "DELETE",
        "/admin/jobs/{job_id}",
        "needs a dead job, a running transcode job or a worker, which only a transcoder worker creates",
    ),
    (
        "PUT",
        "/applications/{application_id}/guilds/{guild_id}/commands/{command_id}/permissions",
        "bearer-only route: needs a user token with applications.commands.permissions.update, which no actor holds",
    ),
    (
        "POST",
        "/activity-instances/{instance_id}/join",
        "needs an approved Activity bundle and a running instance; no Activity host runs in this stack",
    ),
    (
        "POST",
        "/activity-instances/{instance_id}/leave",
        "needs an approved Activity bundle and a running instance; no Activity host runs in this stack",
    ),
    (
        "POST",
        "/channels/{channel_id}/activity",
        "needs an approved Activity bundle and a running instance; no Activity host runs in this stack",
    ),
    (
        "POST",
        "/channels/{channel_id}/call/ring",
        "the call state lives in the voice node (calls is unset in the embedded setup)",
    ),
    (
        "POST",
        "/channels/{channel_id}/call/stop-ringing",
        "the call state lives in the voice node (calls is unset in the embedded setup)",
    ),
    (
        "POST",
        "/cinema/assets/{asset_id}/refresh",
        "needs a Cinema asset held by the object store; the test stack's store cannot start an upload (RDY-SEC-011)",
    ),
    (
        "POST",
        "/cinema/titles/{title_id}/downloads",
        "needs a transcoded Cinema asset; the test store cannot start an upload (RDY-SEC-011)",
    ),
    (
        "POST",
        "/donations/invoices/{invoice_id}/cancel",
        "payments are off: no chain source in the test stack (D-257); ownership pinned by sr-api donations tests",
    ),
    (
        "POST",
        "/donations/invoices/{invoice_id}/check",
        "payments are off: no chain source in the test stack (D-257); ownership pinned by sr-api donations tests",
    ),
    (
        "POST",
        "/guilds/{guild_id}/alert-actions",
        "needs a posted alert card, a message carrying an sr: button",
    ),
    (
        "POST",
        "/interactions/{interaction_id}/{token}/callback",
        "needs an interaction id and token minted by a real interaction",
    ),
    (
        "POST",
        "/sr/auth/discord/link",
        "needs an external identity provider or a passkey ceremony",
    ),
    (
        "POST",
        "/sr/auth/passkeys/login/finish",
        "needs an external identity provider or a passkey ceremony",
    ),
    (
        "POST",
        "/users/@me/connections/{provider}/authorize",
        "needs an external identity provider or a passkey ceremony",
    ),
    (
        "POST",
        "/users/@me/connections/{provider}/{id}/refresh",
        "needs an external identity provider or a passkey ceremony",
    ),
    (
        "POST",
        "/users/@me/profile-doc/layout",
        "needs an approved profile_layout package",
    ),
    (
        "PATCH",
        "/sr/auth/passkeys/{id}",
        "needs an external identity provider or a passkey ceremony",
    ),
    (
        "PATCH",
        "/users/@me/connections/{provider}/{id}",
        "needs an external identity provider or a passkey ceremony",
    ),
    (
        "PATCH",
        "/webhooks/{application_id}/{token}/messages/{message_id}",
        "needs an interaction id and token minted by a real interaction",
    ),
    (
        "DELETE",
        "/users/@me/connections/{provider}/{id}",
        "needs an external identity provider or a passkey ceremony",
    ),
    (
        "DELETE",
        "/webhooks/{application_id}/{token}/messages/{message_id}",
        "needs an interaction id and token minted by a real interaction",
    ),
    (
        "DELETE",
        "/guilds/{guild_id}/integrations/{integration_id}",
        "no integration exists to remove (they are created by external sync)",
    ),
    (
        "DELETE",
        "/sr/auth/passkeys/{id}",
        "needs an external identity provider or a passkey ceremony",
    ),
    (
        "DELETE",
        "/activity-instances/{instance_id}",
        "needs an approved Activity bundle and a running instance; no Activity host runs in this stack",
    ),
    (
        "POST",
        "/watch-parties/{party_id}/leave",
        "the insider's leave follows its removal by the member delete (unresolved order, see the handoff)",
    ),
];

/// The length of [`WRITE_OBSERVED`]. Lower it when an entry goes; never raise it.
const WRITE_OBSERVED_LEN: usize = 40;

/// Admitted writes that answered 400 or 422 per group, in the order of [`GROUPS`]: the request shape was
/// refused before the permission check ran. Like [`WEAK_CEILING`], the numbers only go down.
///
/// Wave 34 integration raised `rest` from 45 to 46 for one new route, `POST /users/@me/rings/media` (PRF-RNG-001):
/// its body is a raw photo or clip that the matrix's JSON bodies cannot carry, so it is refused as malformed (400)
/// once the session and self checks passed. Its refusals for every other actor are asserted as usual.
const SHAPE_CEILING: [usize; 6] = [7, 28, 5, 4, 6, 46];

/// Rightful actors that differ from [`control_of`]'s rule, by route.
fn rightful_override(method: &str, path: &str) -> Option<&'static str> {
    match (method, path) {
        // Disposable accounts: the call ends the session or the account.
        ("POST", "/sr/auth/account/delete") => Some("disp_del"),
        ("POST", "/sr/auth/password") => Some("disp_pw"),
        // The only account enrolled in TOTP.
        ("POST", "/sr/auth/reauth") => Some("staff_owner"),
        ("POST", "/staff/offers/{offer_id}/respond") => Some("offeree"),
        // A participant leaves; the host leaving would end the party the deletes address.
        ("POST", "/watch-parties/{party_id}/leave") => Some("insider"),
        ("POST", "/developers/tester-invites/{invite_id}/respond") => Some("victim"),
        // PRF-PRO-001: the owner of the page releases a held note; the victim owns the seeded public page.
        ("POST", "/users/@me/guestbook/{entry_id}/approve") => Some("victim"),
        // Owner-only routes of a server (the AUTHZ cell `owner` also names application owners).
        ("POST", "/guilds/{guild_id}/mfa") => Some("owner_a"),
        // PRF-SCR-001: the copy is signed for the server's owner, who holds every storage grant on its shelf.
        ("POST", "/guilds/{guild_id}/scrapbook/{entry_id}/objects/{object_id}/presign") => {
            Some("owner_a")
        }
        // `author_or_staff`: the submitter of the published submission or a REVIEW_READING holder; the seeded
        // submission is not the member's, so the Owner fulfils.
        ("POST", "/requests/{request_id}/fulfil") => Some("staff_owner"),
        // Patrons only: the seed grants the victim a Patron entitlement (P25-SEC-1). The Cinema download
        // stays in WRITE_OBSERVED until a transcoded asset can be seeded.
        ("POST", "/library/chapters/{chapter_id}/download")
        | ("POST", "/cinema/titles/{title_id}/downloads") => Some("victim"),
        // A vote on a review is not the author's.
        ("PUT", "/library/reviews/{review_id}/helpful")
        | ("DELETE", "/library/reviews/{review_id}/helpful")
        | ("PUT", "/cinema/reviews/{review_id}/helpful")
        | ("DELETE", "/cinema/reviews/{review_id}/helpful") => Some("owner_a"),
        _ => None,
    }
}

/// Routes whose rightful call ends the actor's own session or account, or otherwise breaks what the
/// rest of the phase needs: they go last.
fn write_late(method: &str, path: &str) -> u8 {
    if matches!(
        (method, path),
        // A new token ends the old one, which the routes before it are called with; the delete ends the app
        // or the webhook.
        ("POST", "/webhooks/{webhook_id}/reset-token")
            | ("DELETE", "/webhooks/{webhook_id}")
            | ("POST", "/applications/{application_id}/bot/reset-token")
            | ("POST", "/applications/{application_id}/reset-secret")
            | ("DELETE", "/applications/{application_id}")
            // The channel and the server take their webhooks and members with them.
            | ("DELETE", "/channels/{channel_id}")
            | ("DELETE", "/guilds/{guild_id}")
    ) {
        return 2;
    }
    u8::from(matches!(
        (method, path),
        // Saving a profile page drops the pictures it does not use.
        ("PUT", "/users/@me/profile-doc")
            | ("DELETE", "/guilds/{guild_id}/never")
            // A bulk overwrite replaces the commands the routes before it address.
            | ("PUT", "/applications/{application_id}/commands")
            | ("PUT", "/applications/{application_id}/guilds/{guild_id}/commands")
            // Removing members ends what the member routes before them act on.
            | ("DELETE", "/guilds/{guild_id}/bans/{user_id}")
            // Leaving, then the party's removal.
            | ("POST", "/watch-parties/{party_id}/leave")
            | ("PUT", "/guilds/{guild_id}/bans/{user_id}")
            | ("POST", "/guilds/{guild_id}/prune")
            | ("POST", "/guilds/{guild_id}/bulk-ban")
            | ("DELETE", "/watch-parties/{party_id}")
    ))
}

/// Deletes of the entities others hang on without being nested in their path (a channel's messages, pins
/// and invites; a Live TV channel's blocks; an edition's chapters; a title's seasons): last among the deletes.
fn write_root(path: &str) -> bool {
    matches!(
        path,
        "/channels/{channel_id}"
            | "/channels/{channel_id}/messages/{message_id}"
            | "/admin/tv/channels/{channel_id}"
            | "/admin/library/editions/{edition_id}"
            | "/admin/library/series/{series_id}"
            | "/admin/cinema/titles/{title_id}"
            | "/admin/cinema/seasons/{season_id}"
            | "/watch-parties/{party_id}"
            | "/guilds/{guild_id}"
    )
}

/// Writes that settle or consume what the creates and edits before them act on (a hold is released or
/// retained once it has been reviewed): after the edits, before the deletes.
fn write_ending(method: &str, path: &str) -> bool {
    matches!(
        (method, path),
        ("POST", "/moderation/holds/{hold_id}/release")
            | ("POST", "/moderation/holds/{hold_id}/retain")
    )
}

/// Placeholder values that differ from the seeded ones for a route (the reaction the route removes is the
/// one `owner_a` made, not the victim's).
fn path_overrides(r: &Route, world: &World) -> Vec<(&'static str, String)> {
    if r.method == "DELETE" && r.path.ends_with("/reactions/{emoji}/{user_id}") {
        vec![
            ("user_id", world.users["ownera"].to_string()),
            ("emoji", "%F0%9F%98%80".to_owned()),
        ]
    } else if r.method == "DELETE" && r.path.ends_with("/reactions/{emoji}") {
        vec![("emoji", "%F0%9F%8E%89".to_owned())]
    } else if r.path.contains("/reactions/") {
        vec![("user_id", world.users["ownera"].to_string())]
    } else if r.path.contains("/bans/") {
        vec![("user_id", world.users["friendtwo"].to_string())]
    } else if r.method == "DELETE" && r.path == "/webhooks/{webhook_id}/{token}" {
        vec![
            ("webhook_id", world.a["webhook2_id"].clone()),
            ("token", world.a["token2"].clone()),
        ]
    } else if r.path.starts_with("/admin/staff/{account_id}") {
        vec![("account_id", world.a["staff_target_id"].clone())]
    } else if r.method == "DELETE" && r.path == "/admin/library/uploads/{upload_id}" {
        vec![("upload_id", world.a["lib_upload2_id"].clone())]
    } else if r.path == "/admin/cinema/titles/{title_id}/merge" {
        vec![("title_id", world.a["merge_title_id"].clone())]
    } else if r.path == "/admin/library/series/{series_id}/merge" {
        vec![("series_id", world.a["merge_series_id"].clone())]
    } else if r.path == "/moderation/evasion-signals/{signal_id}/link-case" {
        vec![("signal_id", world.a["signal2_id"].clone())]
    } else if r.path.starts_with("/watch-parties/") && r.path.contains("{user_id}") {
        vec![("user_id", world.users["insider"].to_string())]
    } else {
        Vec::new()
    }
}

/// A hand-made body for the write phase (placeholders in braces are replaced by the ids of tenant A),
/// consulted before [`body_override`].
fn write_body_override(method: &str, path: &str) -> Option<&'static str> {
    Some(match (method, path) {
        ("POST", "/admin/cinema/titles/{title_id}/merge") => r#"{"target_id":"{title_id}"}"#,
        ("POST", "/admin/library/series/{series_id}/merge") => r#"{"target_id":"{series_id}"}"#,
        ("POST", "/moderation/age/restrictions") => {
            r#"{"kind":"channel","id":"{channel_id}","restricted":true,"reason":"authz matrix"}"#
        }
        // UTL-COL-001, UTL-CRD-001, UTL-DEV-001 (wave 32): the seeded room, card, tables and note.
        ("POST", "/channels/{channel_id}/tool-rooms") => r#"{"kind":"poll","title":"Authz poll"}"#,
        ("PATCH", "/channels/{channel_id}/tool-rooms/{room_id}") => {
            r#"{"base_version":1,"set":{"tier.authz":"S"}}"#
        }
        ("POST", "/channels/{channel_id}/cards") => {
            r#"{"kind":"game_night","title":"Authz write","starts_at":{future_ms},"seats":2}"#
        }
        ("POST", "/channels/{channel_id}/cards/{card_id}/claim") => r#"{"remind":false}"#,
        ("POST", "/users/@me/tools/loot-tables")
        | ("POST", "/guilds/{guild_id}/tools/loot-tables") => {
            r#"{"name":"Authz loot write","rows":[{"label":"Gem","weight":2}]}"#
        }
        ("PATCH", "/users/@me/tools/loot-tables/{table_id}")
        | ("PATCH", "/guilds/{guild_id}/tools/loot-tables/{table_id}") => {
            r#"{"name":"Authz loot edit","rows":[{"label":"Gem","weight":3}]}"#
        }
        ("PUT", "/users/@me/tools/notes/{note_id}") => {
            r#"{"ciphertext":"AQIDBA==","base_revision":1}"#
        }
        ("PUT", "/channels/{channel_id}/voice-timer") => r#"{"timer":null}"#,
        ("POST", "/watch-parties") => r#"{"title_id":"{title_id}","channel_id":"{channel_id}"}"#,
        // MUS-IDX-001/MUS-LIB-001 (wave 30): the member's own mount and track; the guild's track for its playlist.
        ("POST", "/music/library/rescan") => r#"{"mount_id":"{mount_id}"}"#,
        // A sampled `source` is neither `file` nor `edit`: a plain title edit by the mount's owner.
        ("PATCH", "/music/tracks/{track_id}") => r#"{"title":"Authz edit"}"#,
        ("POST", "/users/@me/music/recent") => r#"{"track_id":"{track_id}"}"#,
        ("POST", "/music/playlists/{playlist_id}/items") => r#"{"track_ids":["{guild_track_id}"]}"#,
        // MUS-JB-002/003 (wave 31): the guild's shelf track, the seeded playing row, the guild's playlist.
        ("POST", "/channels/{channel_id}/jukebox/queue") => r#"{"track_ids":["{guild_track_id}"]}"#,
        ("POST", "/channels/{channel_id}/jukebox/ended") => {
            r#"{"item_id":"{jukebox_playing_item_id}"}"#
        }
        ("POST", "/channels/{channel_id}/jukebox/presign") => r#"{"item_id":"{jukebox_item_id}"}"#,
        ("PUT", "/channels/{channel_id}/jukebox") => r#"{"mode":"synced"}"#,
        ("PUT", "/channels/{channel_id}/jukebox/dj") => r#"{"user_id":null}"#,
        ("PUT", "/channels/{channel_id}/jukebox/playback") => r#"{"paused":false}"#,
        ("PUT", "/channels/{channel_id}/listening-room") => r#"{"playlist_id":"{playlist_id}"}"#,
        ("PATCH", "/music/playlists/{playlist_id}/items/{item_id}") => r#"{"position":0}"#,
        // The host hands the party to itself: allowed, and the member keeps the control the rest needs.
        ("POST", "/watch-parties/{party_id}/host") => r#"{"user_id":"{member_id}"}"#,
        ("POST", "/developers/tester-invites/{invite_id}/respond")
        | ("POST", "/staff/offers/{offer_id}/respond") => r#"{"accept":true}"#,
        ("POST", "/applications/{application_id}/activity/submit") => {
            r#"{"bundle_id":"{bundle_id}"}"#
        }
        ("POST", "/moderation/evasion-signals/{signal_id}/link-case") => {
            r#"{"case_id":"{case_id}"}"#
        }
        ("POST", "/admin/data-requests") => r#"{"kind":"export","account_id":"{user_id}"}"#,
        ("POST", "/channels/{channel_id}/attachments") => {
            r#"{"files":[{"filename":"a.txt","file_size":3}]}"#
        }
        ("POST", "/users/{user_id}/guestbook") => r#"{"body":"authz"}"#,
        ("POST", "/public/profiles/{handle}/guestbook") => r#"{"body":"authz public note"}"#,
        ("POST", "/packages/{package_id}/create-guild") => r#"{"name":"authz guild"}"#,
        ("POST", "/admin/staff") => {
            r#"{"account_id":"{staff_target_id}","role_ids":["{support_role_id}"],"reason":"authz matrix"}"#
        }
        ("POST", "/admin/staff/offers") => {
            r#"{"account_id":"{owner_b_id}","role_ids":["{support_role_id}"],"reason":"authz matrix"}"#
        }
        ("PUT", "/admin/staff/{account_id}/roles") => {
            r#"{"role_ids":["{support_role_id}"],"reason":"authz matrix"}"#
        }
        ("PUT", "/sr/auth/username") => r#"{"username":"authz.member","password":"{pw}"}"#,
        ("DELETE", "/sr/auth/passkeys/{id}") | ("PATCH", "/sr/auth/passkeys/{id}") => {
            r#"{"password":"{pw}","name":"authz"}"#
        }
        ("POST", "/sr/auth/password") => {
            r#"{"current_password":"{pw}","new_password":"authz-matrix-New-Horse-7"}"#
        }
        ("DELETE", "/users/@me/age-confirmation") => r#"{"current_password":"{pw}"}"#,
        ("POST", "/applications/{application_id}/reset-secret")
        | ("POST", "/applications/{application_id}/bot/reset-token")
        | ("DELETE", "/applications/{application_id}")
        | ("POST", "/sr/auth/discord/unlink")
        | ("POST", "/sr/auth/passkeys/register/begin") => {
            r#"{"password":"{pw}","current_password":"{pw}"}"#
        }
        ("POST", "/sr/auth/account/delete") => r#"{"password":"{pw}","username":"dispdel"}"#,
        ("POST", "/sr/auth/reauth") => r#"{"method":"totp","code":"{totp_next}"}"#,
        ("POST", "/guilds/{guild_id}/rss-feeds") => {
            r#"{"channel_id":"{channel_id}","url":"https://example.test/feed2.xml"}"#
        }
        ("POST", "/requests/{request_id}/fulfil") => r#"{"submission_id":"{submission_id}"}"#,
        ("PUT", "/watch-parties/{party_id}/media")
        | ("POST", "/watch-parties/{party_id}/queue") => r#"{"title_id":"{title_id}"}"#,
        ("POST", "/moderation/takedowns/{notice_id}/items") => {
            r#"{"kind":"message","id":"{message_id}"}"#
        }
        ("POST", "/stage-instances") => {
            r#"{"channel_id":"{stage_new_channel_id}","topic":"authz"}"#
        }
        _ => return None,
    })
}

fn write_observed(r: &Route) -> Option<&'static str> {
    WRITE_OBSERVED
        .iter()
        .find(|(m, p, _)| *m == r.method && *p == r.path)
        .map(|(_, _, why)| *why)
}

/// The order of the write phase: late routes last; then creates, edits, deletes; deletes deepest first.
fn write_rank(r: &Route, idx: usize) -> (u8, u8, usize, usize) {
    let kind = if write_ending(&r.method, &r.path) {
        3
    } else {
        match r.method.as_str() {
            "POST" => 0,
            "DELETE" if write_root(&r.path) => 5,
            "DELETE" => 4,
            _ => 1,
        }
    };
    let depth = r.path.matches('/').count();
    (
        write_late(&r.method, &r.path),
        kind,
        if kind >= 4 { usize::MAX - depth } else { 0 },
        idx,
    )
}

/// Make a sampled body fit what the handlers validate before they decide: a real picture where an image is
/// asked for, a real sound, a word where the sample put `x`. Only the write phase uses it.
fn fixup(v: &mut Value) {
    match v {
        Value::Object(m) => {
            for (k, x) in m.iter_mut() {
                match (k.as_str(), &*x) {
                    (
                        "image" | "file" | "thumbnail" | "cover" | "avatar" | "icon" | "banner",
                        Value::String(_),
                    ) => {
                        *x = json!(format!("data:image/png;base64,{}", b64(&make_png(8, 8))));
                    }
                    ("sound", Value::String(_)) => *x = json!(wav_uri(300)),
                    ("language", Value::String(_)) => *x = json!("en"),
                    ("reason" | "note", Value::String(s)) if s == "x" => *x = json!("authz matrix"),
                    _ => fixup(x),
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(fixup),
        Value::String(s) if s == "x" => *s = "authz_sample".to_owned(),
        _ => {}
    }
}

fn substitute(text: &str, ids: &HashMap<&'static str, String>, totp: &TotpSecret) -> String {
    let mut t = text.replace("{pw}", TEST_PASSWORD);
    if t.contains("{totp_next}") {
        // The step after the one the staff step-up used: a code is good once.
        let now = SystemClock.now_unix_ms() as u64 / 1000 + 30;
        let code =
            sr_auth::totp::current_code(totp, "SIDE ROOM", "staffowner", now).unwrap_or_default();
        t = t.replace("{totp_next}", &code);
    }
    for (k, v) in ids {
        t = t.replace(&format!("{{{k}}}"), v);
    }
    t
}

/// What the write phase found for a group.
#[derive(Default)]
struct WriteStats {
    calls: usize,
    admitted: usize,
    shape: usize,
    observed: usize,
}

async fn write_phase(
    world: &World,
    routes: &[Route],
    g: usize,
    infos: &HashMap<(String, String), OpInfo>,
    failures: &mut Vec<String>,
    obs: &mut Vec<Obs>,
) -> WriteStats {
    let gname = GROUPS[g];
    let mut stats = WriteStats::default();
    // The staff step-up, once per group: a real TOTP code for the Owner opens the 15-minute window of its
    // session, so that the gated staff writes are asserted (RDY-SEC-012).
    let owner = world.actor("staff_owner");
    let r = step_up(&world.srv, &owner.cred, &world.totp).await;
    if r.status != 200 {
        failures.push(format!(
            "[{gname}] the staff step-up (POST /admin/reauth with a TOTP code) answered {} {}",
            r.status,
            r.body.chars().take(160).collect::<String>()
        ));
    }
    // Listed in the directory only now: once approved, a stranger may join the server, which the refusal
    // phases before this one must not see.
    let ga = world.guild_a;
    let r = world
        .srv
        .call(
            &world.actor("owner_a").cred,
            "PUT",
            &format!("/guilds/{ga}/directory"),
            Some(&json!({"category": "gaming", "description": "authz"})),
        )
        .await;
    if !(200..300).contains(&r.status) {
        println!(
            "authz_matrix: directory listing -> {} {}",
            r.status,
            r.body.chars().take(100).collect::<String>()
        );
    }
    world
        .srv
        .call(
            &owner.cred,
            "PATCH",
            &format!("/admin/directory/{ga}"),
            Some(&json!({"decision": "approve"})),
        )
        .await;
    let mut order: Vec<usize> = routes
        .iter()
        .enumerate()
        .filter(|(_, r)| group_of(&r.path) == g && is_write(&r.method))
        .map(|(i, _)| i)
        .collect();
    order.sort_by_key(|i| write_rank(&routes[*i], *i));
    for i in order {
        let r = &routes[i];
        let actor_name = rightful_of(r);
        let actor = world.actor(actor_name);
        let info = infos.get(&(r.method.clone(), r.path.clone()));
        let (mut ctx, _) = ctx_for(r, &world.a);
        for (k, v) in path_overrides(r, world) {
            ctx.insert(k, v);
        }
        let mut path = fill(&r.path, &ctx);
        if let Some(i) = info
            && !i.query.is_empty()
        {
            path.push(if path.contains('?') { '&' } else { '?' });
            path.push_str(&i.query);
        }
        let text =
            write_body_override(&r.method, &r.path).or_else(|| body_override(&r.method, &r.path));
        let body = match text {
            Some(t) => serde_json::from_str(&substitute(t, &world.a, &world.totp)).ok(),
            None => Some(
                info.and_then(|i| i.body.clone())
                    .unwrap_or_else(|| json!({})),
            ),
        };
        let wants_reason = info
            .and_then(|i| i.full.as_ref())
            .is_some_and(|f| f.get("reason").is_some());
        let body = body.map(|mut b| {
            fixup(&mut b);
            if wants_reason && let Some(m) = b.as_object_mut() {
                m.entry("reason").or_insert(json!("authz matrix"));
            }
            b
        });
        let resp = world
            .srv
            .call_reason(&actor.cred, &r.method, &path, body.as_ref())
            .await;
        stats.calls += 1;
        let listed = write_observed(r);
        let refused = matches!(resp.status, 401 | 403 | 404);
        let broken = resp.status == 0 || resp.status >= 500;
        let short = |n: usize| resp.body.chars().take(n).collect::<String>();
        let (verdict, note): (&'static str, String) = if let Some(why) = listed {
            if refused || broken || matches!(resp.status, 400 | 422) {
                stats.observed += 1;
                ("observed", format!("{why}: {} {}", resp.status, short(80)))
            } else {
                failures.push(format!(
                    "[{gname}] {} {} is listed in WRITE_OBSERVED ({why}) but {actor_name} was admitted ({}): remove the entry and lower WRITE_OBSERVED_LEN",
                    r.method, r.path, resp.status
                ));
                (
                    "FAIL",
                    format!("stale WRITE_OBSERVED entry: {}", resp.status),
                )
            }
        } else if refused || broken {
            failures.push(format!(
                "[{gname}] {} {} as {actor_name} (write): expected admitted, got {} {}",
                r.method,
                r.path,
                resp.status,
                short(160)
            ));
            (
                "FAIL",
                format!("#{} {} {}", stats.calls, resp.status, short(160)),
            )
        } else if matches!(resp.status, 400 | 422) {
            stats.shape += 1;
            ("shape", format!("{} {}", resp.status, short(160)))
        } else {
            stats.admitted += 1;
            ("ok", short(80))
        };
        obs.push(Obs {
            group: g,
            method: r.method.clone(),
            path: r.path.clone(),
            actor: actor.name,
            pass: "write",
            status: resp.status,
            exp: Exp::Allow,
            verdict,
            note,
        });
        if actor_name == "member" {
            heal(world).await;
        }
    }
    if stats.shape > SHAPE_CEILING[g] {
        failures.push(format!(
            "[{gname}] {} admitted writes were refused as malformed (400 or 422), the ceiling is {}: a hand-made body stopped fitting",
            stats.shape, SHAPE_CEILING[g]
        ));
    }
    stats
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
    // OPS-IMM-002: under a flag state (immutable_rules.rs) only the refusal phase runs, after the state is applied.
    let refusals_only = harness::current().is_some();
    harness::after_seed(&world, g, &mut failures).await;

    let (denials, others): (Vec<&Call>, Vec<&Call>) =
        calls.iter().partition(|c| c.exp == Exp::Deny);
    let mut others = others;
    // RDY-SEC-011: SR_AUTHZ_WRITES=0 skips the write phase, SR_AUTHZ_WRITE_ONLY=1 runs nothing else (both
    // for iterating on the harness; the full run has neither).
    let write_on = std::env::var("SR_AUTHZ_WRITES").as_deref() != Ok("0");
    let write_only = std::env::var_os("SR_AUTHZ_WRITE_ONLY").is_some();
    let mut wstats = WriteStats::default();
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

    // Reads, then the write phase (the rightful actors, asserted), then the other actors' writes.
    let (reads, rest): (Vec<&Call>, Vec<&Call>) = others
        .into_iter()
        .partition(|c| class_of(&routes[c.route].method) == 0);
    // Let what the seed set going (a staff offer's direct message, a notification job) land first.
    let mut before = fingerprint(&world.pool).await;
    let mut steady = 0;
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(400)).await;
        let again = fingerprint(&world.pool).await;
        if again == before {
            steady += 1;
            if steady == 6 {
                break;
            }
        } else {
            steady = 0;
            before = again;
        }
    }
    let mut group_fail = 0usize;
    let mut n = 0usize;
    for (phase, list) in [(0, denials), (1, reads), (2, rest)] {
        if (write_only && phase != 2) || (refusals_only && phase != 0) {
            continue;
        }
        if phase == 2 && write_on {
            let before_fail = failures.len();
            wstats = write_phase(&world, routes, g, infos, &mut failures, &mut obs).await;
            group_fail += failures.len() - before_fail;
        }
        for c in list {
            let r = &routes[c.route];
            // The rightful actor's writes were made, in order and asserted, by the write phase.
            if write_on && is_write(&r.method) && c.actor == rightful_of(r) && c.pass == "std" {
                continue;
            }
            if write_only {
                continue;
            }
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
                let latest: Vec<(i64, i64, String)> = sqlx::query_as(
                    "SELECT id, channel_id, left(content, 80) FROM messages ORDER BY id DESC LIMIT 3",
                )
                .fetch_all(&world.pool)
                .await
                .unwrap_or_default();
                failures.push(format!(
                    "[{gname}] refused calls changed tables: {changed:?} (newest messages: {latest:?})"
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
        if (o.pass == "std" || o.pass == "write") && o.actor == control_of(r) {
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
    if !write_only && !refusals_only && weak > WEAK_CEILING[g] {
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
        "{}. {gname}: {status} ({n} calls, {group_fail} failures, {weak} weak refusals, {masked} masked; writes: {} asserted, {} admitted, {} malformed, {} observed; {:.0}s)",
        g + 1,
        wstats.calls,
        wstats.admitted,
        wstats.shape,
        wstats.observed,
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
    if harness::embedded() {
        return;
    }
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
    } else if r.path.starts_with("/stage-instances/") {
        "stage_channel_id"
    } else if r.path.contains("/call/") {
        "group_channel_id"
    } else if r.path.starts_with("/tv/favorites/") {
        "tv_channel_id"
    } else if r.path.starts_with("/tv/reminders/") {
        "block_id"
    } else if r.path.contains("/jukebox")
        || r.path.ends_with("/listening-room")
        || r.path.ends_with("/voice-timer")
    {
        // MUS-JB-002/003: a voice channel with a jukebox session. UTL-DEV-001: a timer goes to a voice channel too.
        "jukebox_channel_id"
    } else {
        return (overlay(r, base.clone()), false);
    };
    let mut c = base.clone();
    if let Some(v) = base.get(from) {
        c.insert("channel_id", v.clone());
    }
    (overlay(r, c), true)
}

/// Placeholders that mean different things in different route families: Cinema and Library each have
/// their own `{asset_id}`, `{comment_id}`, `{list_id}` ..., and the submission chapter is not the
/// published one. The write phase (and every other pass) fills them from the family's own seeded key.
const OVERLAYS: &[(&[&str], &str, &str)] = &[
    (&["/users/@me/guestbook/"], "entry_id", "pending_entry_id"),
    (
        &["/cinema/", "/admin/cinema/"],
        "asset_id",
        "cinema_asset_id",
    ),
    (
        &["/cinema/", "/admin/cinema/"],
        "comment_id",
        "cinema_comment_id",
    ),
    (&["/cinema/", "/admin/cinema/"], "list_id", "cinema_list_id"),
    (
        &["/cinema/", "/admin/cinema/"],
        "review_id",
        "cinema_review_id",
    ),
    (
        &["/cinema/", "/admin/cinema/"],
        "report_id",
        "cinema_report_id",
    ),
    (
        &["/library/", "/admin/library/"],
        "chapter_id",
        "lib_chapter_id",
    ),
    (
        &["/library/", "/admin/library/"],
        "upload_id",
        "lib_upload_id",
    ),
    (
        &["/library/", "/admin/library/"],
        "comment_id",
        "library_comment_id",
    ),
    (
        &["/library/", "/admin/library/"],
        "list_id",
        "library_list_id",
    ),
    (
        &["/library/", "/admin/library/"],
        "review_id",
        "library_review_id",
    ),
    (
        &["/library/", "/admin/library/"],
        "report_id",
        "library_report_id",
    ),
    (&["/guilds/"], "feature_id", "guild_feature_id"),
    (&["/channels/"], "feature_id", "channel_feature_id"),
    (&["/users/@me/"], "feature_id", "user_feature_id"),
    (&["/admin/"], "feature_id", "admin_feature_id"),
    (&["/users/@me/scheduled-messages/"], "id", "sched_id"),
    (&["/users/@me/reminders/"], "id", "reminder_id"),
    (&["/sr/invites/"], "code", "sr_invite_code"),
    (&["templates/{code}"], "code", "template_code"),
    (&["/channels/"], "upload_id", "attach_upload_id"),
    (
        &["sr/applications/{application_id}"],
        "application_id",
        "guild_application_id",
    ),
    (&["/applications/"], "emoji_id", "app_emoji_id"),
    (
        &["{application_id}/guilds/{guild_id}/commands/"],
        "command_id",
        "guild_command_id",
    ),
    (
        &["/developers/tester-invites/"],
        "invite_id",
        "tester_invite_id",
    ),
    (
        &["/staff/offers/", "/admin/staff/offers/"],
        "offer_id",
        "staff_offer_id",
    ),
    (&["/watch-parties/"], "item_id", "queue_item_id"),
    (&["/music/playlists/"], "item_id", "playlist_item_id"),
    // UTL-DEV-001: a server's loot table is not the account's.
    (
        &["/guilds/{guild_id}/tools/"],
        "table_id",
        "guild_loot_table_id",
    ),
    // Matched inside the path (no leading slash): the route is /channels/{channel_id}/jukebox/queue/{item_id}.
    (
        &["jukebox/queue/{item_id}"],
        "item_id",
        "jukebox_queued_item_id",
    ),
    (
        &["submissions/{submission_id}/publish-now"],
        "submission_id",
        "own_submission_id",
    ),
    (&["/admin/data-requests/"], "request_id", "data_request_id"),
    (
        &["/admin/donations/entitlements/"],
        "grant_id",
        "entitlement_grant_id",
    ),
    (&["/admin/invites/"], "invite_id", "platform_invite_id"),
    (
        &["/admin/developers/packages/versions/"],
        "version_id",
        "pending_version_id",
    ),
    (
        &["/packages/versions/"],
        "version_id",
        "withdraw_version_id",
    ),
    (&["{package_id}/import"], "package_id", "pack_id"),
    (
        &["{package_id}/apply", "/create-guild"],
        "package_id",
        "recipe_id",
    ),
    // BYOS-LEG-001: the staff delist acts on its own mount (see the seeding block).
    (&["/admin/storage/mounts/"], "mount_id", "legal_mount_id"),
    // A server's mount routes act on the server's own mount, not the plain member's.
    (
        &["/guilds/{guild_id}/storage/mounts"],
        "mount_id",
        "guild_mount_id",
    ),
    // PRF-SCR-001: the scrapbook routes act on the tenant's own entry, copy and failed removal.
    (
        &["/guilds/{guild_id}/scrapbook/"],
        "entry_id",
        "scrapbook_entry_id",
    ),
    (
        &["/guilds/{guild_id}/scrapbook/"],
        "object_id",
        "scrapbook_object_id",
    ),
    (
        &["/guilds/{guild_id}/scrapbook/"],
        "removal_id",
        "scrapbook_removal_id",
    ),
    // Episodes and seasons belong to the series title, not to the film.
    (
        &["{episode_id}", "{season_id}"],
        "title_id",
        "series_title_id",
    ),
];

fn overlay(r: &Route, mut c: HashMap<&'static str, String>) -> HashMap<&'static str, String> {
    for (prefixes, placeholder, key) in OVERLAYS {
        let by_placeholder = !prefixes[0].starts_with('/');
        if prefixes.iter().any(|p| {
            if by_placeholder {
                r.path.contains(p)
            } else {
                r.path.starts_with(p)
            }
        }) && let Some(v) = c.get(key).cloned()
        {
            c.insert(placeholder, v);
        }
    }
    c
}

/// D-366: how a PLATFORM cell admits an appointed actor (no database).
#[test]
fn admitted_grammar() {
    let held = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    let publisher = held(&["STAFF_BASIC", "PUBLISH_MEDIA"]);
    let deleter = held(&["STAFF_BASIC", "DELETE_CONTENT"]);
    let operator = held(&["STAFF_BASIC", "OPERATOR"]);
    let both = held(&["STAFF_BASIC", "PUBLISH_MEDIA", "DELETE_CONTENT"]);
    let basic = held(&["STAFF_BASIC"]);
    let owner = held(&["OWNER"]);
    let support = held(&["STAFF_BASIC", "MANAGE_ACCOUNTS", "MANAGE_INVITES"]);

    // `Any`: a holder of any listed bit, nobody else.
    assert!(admitted(&publisher, "PUBLISH_MEDIA|DELETE_CONTENT"));
    assert!(admitted(&deleter, "PUBLISH_MEDIA|DELETE_CONTENT"));
    assert!(!admitted(&operator, "PUBLISH_MEDIA|DELETE_CONTENT"));
    assert!(!admitted(&support, "PUBLISH_MEDIA|DELETE_CONTENT"));
    assert!(!admitted(&basic, "PUBLISH_MEDIA|DELETE_CONTENT"));
    // `All`: only a holder of every listed bit.
    assert!(admitted(&both, "PUBLISH_MEDIA+DELETE_CONTENT"));
    assert!(!admitted(&publisher, "PUBLISH_MEDIA+DELETE_CONTENT"));
    assert!(!admitted(&deleter, "PUBLISH_MEDIA+DELETE_CONTENT"));
    // `STAFF_BASIC` alone admits every appointment.
    for h in [&publisher, &deleter, &operator, &basic, &support] {
        assert!(admitted(h, "STAFF_BASIC"));
    }
    // `OWNER` passes every cell.
    for cell in [
        "OWNER",
        "OPERATOR",
        "PUBLISH_MEDIA+DELETE_CONTENT",
        "PUBLISH_MEDIA|DELETE_CONTENT",
        "STAFF_BASIC",
    ] {
        assert!(admitted(&owner, cell), "{cell}");
    }
    // A single bit.
    assert!(admitted(&operator, "OPERATOR"));
    assert!(!admitted(&publisher, "OPERATOR"));
    // The refused forms never reach `admitted` silently.
    for bad in [
        "PUBLISH_MEDIA+DELETE_CONTENT|OPERATOR",
        "OWNER|OPERATOR",
        "A|A",
    ] {
        assert!(
            std::panic::catch_unwind(|| admitted(&owner, bad)).is_err(),
            "{bad}"
        );
    }
}

/// RDY-SEC-011: the table of write routes the allow phase cannot assert only shrinks (no database).
#[test]
fn write_observed_is_pinned() {
    let routes = parse_matrix();
    assert_eq!(
        WRITE_OBSERVED.len(),
        WRITE_OBSERVED_LEN,
        "WRITE_OBSERVED changed: an entry may be removed (lower WRITE_OBSERVED_LEN in the same change), never added"
    );
    let mut seen = BTreeSet::new();
    for (m, p, why) in WRITE_OBSERVED {
        let r = routes
            .iter()
            .find(|r| r.method == *m && r.path == *p)
            .unwrap_or_else(|| panic!("WRITE_OBSERVED names {m} {p}, which is not in the matrix"));
        assert!(is_write(&r.method), "{m} {p} is not a write route");
        assert!(why.len() > 10, "{m} {p}: say why");
        assert!(seen.insert((*m, *p)), "{m} {p} is listed twice");
    }
}

// ---------------------------------------------------------------------------------------------
// BYOS-SCP-002: the storage scopes over HTTP, with the actors the generic matrix has no names for:
// a friend the mount is shared with (`friend_shared`), a friend it is not (`friend_unshared`), a guild
// member with and without each grant, and platform staff. Presign is the probe: it decides through
// `sr_storage::scopes` exactly like the listing and never calls the bucket.

async fn insert_mount(
    pool: &sqlx::PgPool,
    ids: &Ids,
    owner: sr_storage::mounts::MountOwner,
    cors: bool,
) -> i64 {
    use sr_storage::mounts::{MountInput, MountRepo as _, NewMount, Provider, Secret};
    let id = ids.generate().get() as i64;
    let key = sr_db::auth_repo::parse_secret_key(KEY_HEX).unwrap();
    let new = NewMount::seal(
        &sr_db::sealed::SecretBox::new(key),
        id,
        MountInput {
            owner,
            label: format!("Shelf {id}"),
            provider: Provider::Minio,
            endpoint: "http://127.0.0.1:9".into(),
            region: "us-east-1".into(),
            bucket: "shelf".into(),
            prefix: "lib".into(),
            path_style: true,
            access_key_id: "AKIAAUTHZMATRIX".into(),
        },
        &Secret::new("authz-matrix-secret"),
    )
    .unwrap();
    sr_db::mounts_repo::PgMountRepo::new(pool.clone())
        .create(new, None)
        .await
        .unwrap();
    if cors {
        sqlx::query("UPDATE storage_mounts SET cors_ok_at = now() WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
    }
    id
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn storage_scopes_hold_over_http() {
    if harness::embedded() {
        return;
    }
    let base = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL");
    let routes = parse_matrix();
    let infos = op_infos();
    let db = TestDb::new().await.expect("test db");
    let url = match base.rsplit_once('/') {
        Some((head, _)) => format!("{head}/{}", db.name()),
        None => panic!("bad SR_TEST_DATABASE_URL"),
    };
    let (world, stop, server) = seed(&db, &url, &infos, &pair_names(&routes)).await;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let ids: Ids = SnowflakeGenerator::new(clock.clone(), 9, 21).expect("ids");
    let id_src: sr_db::auth_repo::IdSource = {
        let g = Arc::new(SnowflakeGenerator::new(clock.clone(), 9, 22).expect("ids"));
        Arc::new(move || g.generate().get() as i64)
    };
    let auth = PgAuthRepo::new(
        world.pool.clone(),
        id_src,
        sr_db::auth_repo::parse_secret_key(KEY_HEX).unwrap(),
    );
    let friend_unshared = Cred::Session(session_for(&auth, &ids, world.users["friendtwo"]).await);
    let friend_shared = world.actor("victim").cred.clone();
    let member = world.actor("member").cred.clone();
    let pool = &world.pool;
    let srv = &world.srv;
    let m = world.a["mount_id"].clone();
    let gid = world.guild_a;
    let g_mount = insert_mount(pool, &ids, sr_storage::mounts::MountOwner::Guild(gid), true)
        .await
        .to_string();

    let mut failures: Vec<String> = Vec::new();
    let presign = async |who: &Cred, mount: &str, key: &str| -> Resp {
        srv.call(
            who,
            "POST",
            &format!("/storage/mounts/{mount}/presign"),
            Some(&json!({"key": key, "method": "GET"})),
        )
        .await
    };
    macro_rules! expect {
        ($what:expr, $resp:expr, $status:expr) => {{
            let r: Resp = $resp;
            if r.status != $status {
                failures.push(format!(
                    "{}: expected {}, got {} {}",
                    $what,
                    $status,
                    r.status,
                    r.body.chars().take(120).collect::<String>()
                ));
            }
            r
        }};
    }

    // The owner, with the TTL rule visible in the signed URL.
    let r = expect!(
        "owner presigns audio",
        presign(&member, &m, "a.mp3").await,
        200
    );
    if !r.body.contains("X-Amz-Expires=900") {
        failures.push(format!(
            "the default TTL is 900 s: {}",
            r.body.chars().take(200).collect::<String>()
        ));
    }
    expect!(
        "owner presigns video",
        presign(&member, &m, "a.mkv").await,
        200
    );

    // Friends: nothing without a share; per kind with one.
    expect!(
        "friend_shared before the share",
        presign(&friend_shared, &m, "a.mp3").await,
        404
    );
    expect!(
        "friend_unshared",
        presign(&friend_unshared, &m, "a.mp3").await,
        404
    );
    let victim = world.users["victim"];
    expect!(
        "the owner shares music",
        srv.call(
            &member,
            "PUT",
            &format!("/users/@me/storage/mounts/{m}/shares/{victim}"),
            Some(&json!({"cinema": false, "library": false, "music": true})),
        )
        .await,
        200
    );
    expect!(
        "friend_shared audio",
        presign(&friend_shared, &m, "a.mp3").await,
        200
    );
    expect!(
        "friend_shared cover",
        presign(&friend_shared, &m, "x/cover.jpg").await,
        200
    );
    expect!(
        "friend_shared video",
        presign(&friend_shared, &m, "a.mkv").await,
        404
    );
    expect!(
        "friend_unshared still",
        presign(&friend_unshared, &m, "a.mp3").await,
        404
    );
    expect!(
        "friend_unshared lists nothing",
        srv.call(
            &friend_unshared,
            "GET",
            &format!("/storage/mounts/{m}/objects"),
            None
        )
        .await,
        404
    );
    let incoming = expect!(
        "the friend sees the share",
        srv.call(&friend_shared, "GET", "/users/@me/storage/shares", None)
            .await,
        200
    );
    if !incoming.body.contains(&m) {
        failures.push("the incoming list names the mount".into());
    }
    let outgoing = expect!(
        "the owner lists the shares",
        srv.call(
            &member,
            "GET",
            &format!("/users/@me/storage/mounts/{m}/shares"),
            None
        )
        .await,
        200
    );
    if !outgoing.body.contains(&victim.to_string()) {
        failures.push("the outgoing list names the friend".into());
    }
    let sum = expect!(
        "the shelves summary",
        srv.call(&friend_shared, "GET", "/users/@me/shelves/summary", None)
            .await,
        200
    );
    if !sum.body.contains("\"music\":") {
        failures.push(format!("the summary has the music count: {}", sum.body));
    }
    let none = srv
        .call(&friend_unshared, "GET", "/users/@me/shelves/summary", None)
        .await;
    if !none.body.contains("\"music\":0") {
        failures.push(format!("no shelf, no count: {}", none.body));
    }
    if !none.body.contains("\"catalog\":{") {
        failures.push(format!("the summary carries catalog counts: {}", none.body));
    }
    // A share needs a friendship, and only the owner may give it.
    expect!(
        "no share to a stranger",
        srv.call(
            &member,
            "PUT",
            &format!(
                "/users/@me/storage/mounts/{m}/shares/{}",
                world.users["outsider"]
            ),
            Some(&json!({"music": true})),
        )
        .await,
        400
    );
    expect!(
        "a friend cannot share the owner's mount",
        srv.call(
            &friend_shared,
            "PUT",
            &format!(
                "/users/@me/storage/mounts/{m}/shares/{}",
                world.users["friendtwo"]
            ),
            Some(&json!({"music": true})),
        )
        .await,
        404
    );
    // Revoked by hand, then by unfriending.
    expect!(
        "the owner stops sharing",
        srv.call(
            &member,
            "DELETE",
            &format!("/users/@me/storage/mounts/{m}/shares/{victim}"),
            None
        )
        .await,
        204
    );
    expect!(
        "friend_shared after revoke",
        presign(&friend_shared, &m, "a.mp3").await,
        404
    );
    srv.call(
        &member,
        "PUT",
        &format!("/users/@me/storage/mounts/{m}/shares/{victim}"),
        Some(&json!({"music": true})),
    )
    .await;
    expect!(
        "shared again",
        presign(&friend_shared, &m, "a.mp3").await,
        200
    );
    expect!(
        "unfriend",
        srv.call(
            &member,
            "DELETE",
            &format!("/users/@me/relationships/{victim}"),
            None
        )
        .await,
        204
    );
    expect!(
        "friend_shared after unfriend",
        presign(&friend_shared, &m, "a.mp3").await,
        404
    );
    let left: i64 =
        sqlx::query_scalar("SELECT count(*) FROM storage_mount_shares WHERE mount_id = $1")
            .bind(m.parse::<i64>().unwrap())
            .fetch_one(pool)
            .await
            .unwrap();
    if left != 0 {
        failures.push(format!("unfriending left {left} share rows"));
    }

    // Strangers, bad input.
    let outsider = world.actor("outsider").cred.clone();
    expect!("outsider", presign(&outsider, &m, "a.mp3").await, 404);
    expect!("a key with ..", presign(&member, &m, "../a.mp3").await, 400);
    let pre = format!("/storage/mounts/{m}/presign");
    expect!(
        "only GET",
        srv.call(
            &member,
            "POST",
            &pre,
            Some(&json!({"key": "a.mp3", "method": "PUT"}))
        )
        .await,
        400
    );
    expect!(
        "a malformed range",
        srv.call(
            &member,
            "POST",
            &pre,
            Some(&json!({"key": "a.mp3", "method": "GET", "range": "bytes=5-2"}))
        )
        .await,
        400
    );
    expect!(
        "a good range",
        srv.call(
            &member,
            "POST",
            &pre,
            Some(&json!({"key": "a.mp3", "method": "GET", "range": "bytes=0-99"}))
        )
        .await,
        200
    );

    // Staff: a platform operator may look and it is logged; an unappointed holder of a role may not.
    expect!(
        "fake staff",
        presign(&world.actor("fake_staff").cred, &m, "a.mp3").await,
        404
    );
    expect!(
        "support has no OPERATOR",
        presign(&world.actor("support").cred, &m, "a.mp3").await,
        404
    );
    let mid: i64 = m.parse().unwrap();
    let logged = || async {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM staff_access_log WHERE action = 'storage.read' AND target_id = $1",
        )
        .bind(mid)
        .fetch_one(pool)
        .await
        .unwrap()
    };
    let before = logged().await;
    expect!(
        "staff owner",
        presign(&world.actor("staff_owner").cred, &m, "a.mp3").await,
        200
    );
    let after = logged().await;
    if after != before + 1 {
        failures.push(format!(
            "the staff read is logged once ({before} -> {after})"
        ));
    }
    expect!("the owner again", presign(&member, &m, "a.mp3").await, 200);
    if logged().await != after {
        failures.push("an owner's own read is not logged".into());
    }

    // Disabled, missing CORS, owner banned.
    sqlx::query("UPDATE storage_mounts SET status = 'disabled' WHERE id = $1")
        .bind(mid)
        .execute(pool)
        .await
        .unwrap();
    expect!("disabled, owner", presign(&member, &m, "a.mp3").await, 404);
    expect!(
        "disabled, staff",
        presign(&world.actor("staff_owner").cred, &m, "a.mp3").await,
        404
    );
    sqlx::query("UPDATE storage_mounts SET status = 'active', cors_ok_at = NULL WHERE id = $1")
        .bind(mid)
        .execute(pool)
        .await
        .unwrap();
    let r = expect!("no CORS yet", presign(&member, &m, "a.mp3").await, 409);
    if !r.body.contains("CORS_NOT_READY") {
        failures.push(format!("409 carries CORS_NOT_READY: {}", r.body));
    }
    sqlx::query("UPDATE storage_mounts SET cors_ok_at = now() WHERE id = $1")
        .bind(mid)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE accounts SET state = 'platform_banned' WHERE id = $1")
        .bind(world.users["member"])
        .execute(pool)
        .await
        .unwrap();
    expect!(
        "owner banned, staff",
        presign(&world.actor("staff_owner").cred, &m, "a.mp3").await,
        404
    );
    sqlx::query("UPDATE accounts SET state = 'active' WHERE id = $1")
        .bind(world.users["member"])
        .execute(pool)
        .await
        .unwrap();

    // A guild shelf: the grants decide.
    let owner_a = world.actor("owner_a").cred.clone();
    let modr = world.actor("mod").cred.clone();
    let insider = world.actor("insider").cred.clone();
    expect!(
        "guild owner",
        presign(&owner_a, &g_mount, "a.mkv").await,
        200
    );
    expect!(
        "MANAGE_GUILD implies manage",
        presign(&modr, &g_mount, "a.mkv").await,
        200
    );
    expect!(
        "member without a grant",
        presign(&member, &g_mount, "a.mp3").await,
        403
    );
    expect!(
        "insider without a grant",
        presign(&insider, &g_mount, "a.mp3").await,
        403
    );
    expect!(
        "non-member",
        presign(&outsider, &g_mount, "a.mp3").await,
        404
    );
    // P29-CNV-1 (3): a guild mount is transcoded only on the order of the operator (`MANAGE_TRANSCODE`); the
    // server's manager (MANAGE_GUILD, so the `manage` grant) gets 403, a non-member 404.
    let queue = async |who: &Cred| -> Resp {
        srv.call(
            who,
            "POST",
            &format!("/storage/mounts/{g_mount}/transcode"),
            Some(&json!({"key": "Films/a.mkv", "format": "mp4"})),
        )
        .await
    };
    expect!(
        "a guild manager queues a guild mount transcode",
        queue(&modr).await,
        403
    );
    expect!(
        "a non-member queues a guild mount transcode",
        queue(&outsider).await,
        404
    );
    let role = world.a["overwrite_id"].clone();
    let grants_path = format!("/guilds/{gid}/storage/grants");
    let put_grants = |grants: Value| json!({"role_id": role, "grants": grants});
    expect!(
        "grant view_music",
        srv.call(
            &owner_a,
            "PUT",
            &grants_path,
            Some(&put_grants(json!(["view_music"])))
        )
        .await,
        204
    );
    expect!(
        "grant view_games",
        srv.call(
            &owner_a,
            "PUT",
            &grants_path,
            Some(&put_grants(json!(["view_music", "view_games"])))
        )
        .await,
        204
    );
    expect!(
        "an unknown grant is refused",
        srv.call(
            &owner_a,
            "PUT",
            &grants_path,
            Some(&put_grants(json!(["view_arcade"])))
        )
        .await,
        400
    );
    expect!(
        "restore view_music only",
        srv.call(
            &owner_a,
            "PUT",
            &grants_path,
            Some(&put_grants(json!(["view_music"])))
        )
        .await,
        204
    );
    let roles = srv
        .call(&owner_a, "GET", &format!("/guilds/{gid}/roles"), None)
        .await;
    if !roles
        .body
        .contains("\"sr_storage_grants\":[\"view_music\"]")
    {
        failures.push(format!(
            "the role list carries the grants: {}",
            roles.body.chars().take(300).collect::<String>()
        ));
    }
    expect!(
        "insider audio",
        presign(&insider, &g_mount, "a.mp3").await,
        200
    );
    expect!(
        "insider video",
        presign(&insider, &g_mount, "a.mkv").await,
        403
    );
    // ACT-EMU-001: the sixth grant. A ROM indexed on the server's shelf is listed and presigned only for a role
    // holding `view_games`; saves are written to the caller's own mount only; a friend share never reaches `emu/`.
    let rom_key = "emu/roms/Authz Test.nes";
    sqlx::query(
        "INSERT INTO storage_objects (id, mount_id, key, size_bytes, kind, class, container, parsed, state)          VALUES ($1, $2, $3, 40976, 'games', 'rom', 'nes',                  '{\"title\":\"Authz Test\",\"platform\":\"nes\",\"core\":\"fceumm\"}'::jsonb, 'indexed')",
    )
    .bind(ids.generate().get() as i64)
    .bind(g_mount.parse::<i64>().unwrap())
    .bind(rom_key)
    .execute(pool)
    .await
    .unwrap();
    let roms =
        async |who: &Cred| -> Resp { srv.call(who, "GET", "/users/@me/games/roms", None).await };
    let listed = expect!("insider lists roms", roms(&insider).await, 200);
    if listed.body.contains("Authz Test") {
        failures.push("a role without view_games lists the server's ROM".into());
    }
    expect!(
        "insider presigns a rom without the grant",
        presign(&insider, &g_mount, rom_key).await,
        403
    );
    expect!(
        "grant view_games on top of view_music",
        srv.call(
            &owner_a,
            "PUT",
            &grants_path,
            Some(&put_grants(json!(["view_music", "view_games"])))
        )
        .await,
        204
    );
    let listed = expect!(
        "insider lists roms with the grant",
        roms(&insider).await,
        200
    );
    if !listed.body.contains("Authz Test") || !listed.body.contains("\"scope\":\"guild\"") {
        failures.push(format!("the grantee lists the ROM: {}", listed.body));
    }
    expect!(
        "insider presigns the rom",
        presign(&insider, &g_mount, rom_key).await,
        200
    );
    expect!(
        "view_games does not open video",
        presign(&insider, &g_mount, "a.mkv").await,
        403
    );
    let outs = expect!("an outsider lists roms", roms(&outsider).await, 200);
    if outs.body.contains("Authz Test") {
        failures.push("a non-member lists the server's ROM".into());
    }
    let own = expect!("the guild owner lists roms", roms(&owner_a).await, 200);
    if !own.body.contains("Authz Test") {
        failures.push("the server owner lists the ROM".into());
    }
    // Saves: PUT only under emu/saves/<core>/<hash>/<file>, only on the caller's own mount.
    let put_presign = async |who: &Cred, mount: &str, key: &str| -> Resp {
        srv.call(
            who,
            "POST",
            &format!("/storage/mounts/{mount}/presign"),
            Some(&json!({"key": key, "method": "PUT"})),
        )
        .await
    };
    let save = "emu/saves/fceumm/ab12cd/sram.srm";
    expect!(
        "the owner presigns a save PUT on their own mount",
        put_presign(&member, &m, save).await,
        200
    );
    expect!(
        "a save PUT outside emu/saves is refused",
        put_presign(&member, &m, "Movies/x.mp4").await,
        400
    );
    expect!(
        "a save PUT on a shelf the caller merely reaches",
        put_presign(&insider, &g_mount, save).await,
        403
    );
    // Whatever the friend's share is at this point of the run, a PUT is never signed for them.
    let friend_put = put_presign(&friend_shared, &m, save).await;
    if friend_put.status != 403 && friend_put.status != 404 {
        failures.push(format!("a friend PUT presign: got {}", friend_put.status));
    }
    expect!(
        "a friend with a share cannot read emu/",
        presign(&friend_shared, &m, "emu/bios/scph1001.bin").await,
        404
    );
    expect!(
        "restore view_music only",
        srv.call(
            &owner_a,
            "PUT",
            &grants_path,
            Some(&put_grants(json!(["view_music"])))
        )
        .await,
        204
    );
    expect!(
        "a member cannot edit grants",
        srv.call(
            &member,
            "PUT",
            &grants_path,
            Some(&put_grants(json!(["manage"])))
        )
        .await,
        403
    );
    expect!(
        "a stranger cannot edit grants",
        srv.call(
            &outsider,
            "PUT",
            &grants_path,
            Some(&put_grants(json!(["manage"])))
        )
        .await,
        404
    );
    expect!(
        "an unknown grant",
        srv.call(
            &owner_a,
            "PUT",
            &grants_path,
            Some(&put_grants(json!(["root"])))
        )
        .await,
        400
    );
    expect!(
        "a role of another server",
        srv.call(
            &owner_a,
            "PUT",
            &grants_path,
            Some(&json!({"role_id": world.b["role_id"], "grants": ["manage"]}))
        )
        .await,
        404
    );
    expect!(
        "an empty set clears",
        srv.call(&owner_a, "PUT", &grants_path, Some(&put_grants(json!([]))))
            .await,
        204
    );
    expect!(
        "insider after clearing",
        presign(&insider, &g_mount, "a.mp3").await,
        403
    );
    // The other tenant's mount is nobody else's.
    expect!(
        "cross tenant",
        presign(&member, &world.b["mount_id"], "a.mp3").await,
        404
    );

    let _ = stop.send(());
    let _ = tokio::time::timeout(Duration::from_secs(40), server).await;
    db.cleanup().await.ok();
    assert!(
        failures.is_empty(),
        "{} storage scope failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

// ---------------------------------------------------------------------------------------------
// OPS-IMM-002: the library entry of `immutable_rules.rs`. Everything below is that harness's own; the
// matrix above only calls `harness::current()` and `harness::after_seed()` from `run_group`.
// ---------------------------------------------------------------------------------------------

/// The IMMUTABLE RULE harness (docs/dev/roadmap-p27.md section 2): the refusal phase of a group run again
/// with every feature killed at platform scope and with every setting at an end of its range, plus the
/// checks that staff reads and admin actions are still logged and the log chain still verifies.
#[allow(dead_code)]
pub mod harness {
    use super::*;
    use std::sync::Mutex;

    /// A state of the platform flags the refusal phase runs under.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Flip {
        /// Every registered feature switched off at platform scope.
        KillFeatures,
        /// Every setting at its minimum (throttles stay at their maximum, see [`throttle`]).
        SettingsMin,
        /// Every setting at its maximum.
        SettingsMax,
    }

    static CURRENT: Mutex<Option<Flip>> = Mutex::new(None);

    /// True when this file is compiled into `immutable_rules.rs` (as its `matrix` module): the two big tests
    /// above then return at once, because their own binary runs them and the harness only needs the library.
    pub(super) fn embedded() -> bool {
        module_path!().starts_with("immutable_rules")
    }

    pub(super) fn current() -> Option<Flip> {
        *CURRENT.lock().unwrap()
    }

    /// What a run produced.
    #[derive(Default, Debug)]
    pub struct Report {
        /// One line per group: the matrix summary.
        pub lines: Vec<String>,
        /// Everything that went wrong, one line each.
        pub failures: Vec<String>,
    }

    /// Run the refusal phase of `groups` (indexes into [`GROUPS`]) under `flip`, each on a freshly seeded world.
    pub async fn run(flip: Flip, groups: &[usize]) -> Report {
        *CURRENT.lock().unwrap() = Some(flip);
        let base = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL");
        let routes = parse_matrix();
        let infos = op_infos();
        let mut rep = Report::default();
        for &g in groups {
            let res = run_group(g, &base, &routes, &infos).await;
            rep.lines.push(res.line);
            rep.failures.extend(res.failures);
        }
        *CURRENT.lock().unwrap() = None;
        rep
    }

    /// The group indexes whose refusals the IMMUTABLE RULE names: moderation, admin, and the rest (which holds
    /// the storage and voice routes).
    pub const RULE_GROUPS: [usize; 3] = [0, 1, 5];

    /// Settings that throttle the harness itself (HTTP rate limits and the abuse limiters). At their minimum
    /// every call would answer 429, which proves nothing about authorization, so they stay at their maximum
    /// in every state; the handoff names them.
    pub fn throttle(key: &str, group: &str) -> bool {
        key.starts_with("limits.rate.") || group == "abuse"
    }

    struct Logs {
        access: i64,
        audit: i64,
        private: i64,
    }

    async fn count(pool: &sqlx::PgPool, sql: &'static str) -> i64 {
        sqlx::query_scalar::<_, i64>(sql)
            .fetch_one(pool)
            .await
            .unwrap_or(-1)
    }

    async fn logs(pool: &sqlx::PgPool) -> Logs {
        Logs {
            access: count(pool, "SELECT count(*) FROM staff_access_log").await,
            audit: count(pool, "SELECT count(*) FROM platform_audit_log").await,
            private: count(
                pool,
                "SELECT count(*) FROM staff_access_log WHERE kind = 'private_read'",
            )
            .await,
        }
    }

    fn json_of(r: &Resp) -> Value {
        serde_json::from_str(&r.body).unwrap_or(Value::Null)
    }

    /// The value a setting takes at the end of its range `flip` names.
    pub fn setting_value(s: &Value, flip: Flip) -> Option<Value> {
        let key = s["key"].as_str()?;
        let group = s["group"].as_str().unwrap_or("");
        let to_max = flip == Flip::SettingsMax || throttle(key, group);
        match s["kind"].as_str()? {
            "integer" => {
                let v = if to_max { &s["max"] } else { &s["min"] };
                Some(json!(v.as_f64()? as i64))
            }
            "number" => Some(json!(if to_max {
                s["max"].as_f64()?
            } else {
                s["min"].as_f64()?
            })),
            "boolean" => Some(json!(to_max)),
            // A text has no range: empty at the minimum, the most restrictive known value at the maximum.
            "text" if flip == Flip::SettingsMax => Some(match key {
                "byos.who_may_mount" => json!("staff"),
                "byos.endpoint_allow" => json!("immutable.invalid"),
                _ => s["default"].clone(),
            }),
            "text" => Some(json!("")),
            _ => None,
        }
    }

    /// Put the platform into the state of `flip` through the Admin API as the Owner. Returns what it did.
    async fn apply(world: &World, flip: Flip) -> Result<String, String> {
        let owner = &world.actor("staff_owner").cred;
        let srv = &world.srv;
        match flip {
            Flip::KillFeatures => {
                let r = srv.call(owner, "GET", "/admin/features", None).await;
                let list = json_of(&r);
                let list = list
                    .as_array()
                    .ok_or(format!("GET /admin/features: {} {}", r.status, r.body))?;
                let (mut killed, mut skipped) = (0, Vec::new());
                for f in list {
                    let id = f["id"].as_str().unwrap_or_default();
                    if f["enabled"] != json!(true) {
                        continue;
                    }
                    let r = srv
                        .call_reason(
                            owner,
                            "PATCH",
                            &format!("/admin/features/{id}"),
                            Some(&json!({"enabled": false})),
                        )
                        .await;
                    if r.status == 200 {
                        killed += 1;
                    } else {
                        skipped.push(format!("{id} ({})", r.status));
                    }
                }
                if killed == 0 || skipped.len() * 10 > list.len() {
                    return Err(format!(
                        "only {killed} of {} features killed; skipped {skipped:?}",
                        list.len()
                    ));
                }
                Ok(format!(
                    "{killed} of {} features killed, skipped {skipped:?}",
                    list.len()
                ))
            }
            Flip::SettingsMin | Flip::SettingsMax => {
                let r = srv.call(owner, "GET", "/admin/settings", None).await;
                let list = json_of(&r);
                let list = list
                    .as_array()
                    .ok_or(format!("GET /admin/settings: {} {}", r.status, r.body))?;
                let mut values = serde_json::Map::new();
                for s in list {
                    if let (Some(k), Some(v)) = (s["key"].as_str(), setting_value(s, flip)) {
                        values.insert(k.to_owned(), v);
                    }
                }
                let batch = srv
                    .call_reason(
                        owner,
                        "PATCH",
                        "/admin/settings",
                        Some(&json!({"values": values})),
                    )
                    .await;
                if batch.status == 200 {
                    return Ok(format!("{} settings set in one patch", values.len()));
                }
                // Some settings are validated against each other (thresholds): one key at a time then.
                let (mut set, mut skipped) = (0, Vec::new());
                for (k, v) in &values {
                    let r = srv
                        .call_reason(
                            owner,
                            "PATCH",
                            "/admin/settings",
                            Some(&json!({"values": {k: v}})),
                        )
                        .await;
                    if r.status == 200 {
                        set += 1;
                    } else {
                        skipped.push(k.clone());
                    }
                }
                if set * 2 < values.len() {
                    return Err(format!(
                        "only {set} of {} settings applied; skipped {skipped:?}",
                        values.len()
                    ));
                }
                Ok(format!(
                    "{set} of {} settings set one by one; skipped {skipped:?}",
                    values.len()
                ))
            }
        }
    }

    /// Called by `run_group` right after the world is seeded, when a flip is current: apply it, then prove that
    /// admin actions and staff reads still write their logs and that the chain still verifies.
    pub(super) async fn after_seed(world: &World, g: usize, failures: &mut Vec<String>) {
        let Some(flip) = current() else { return };
        let tag = format!("[{} under {flip:?}]", GROUPS[g]);
        let owner = &world.actor("staff_owner").cred;
        let srv = &world.srv;
        let r = step_up(srv, owner, &world.totp).await;
        if r.status != 200 {
            failures.push(format!(
                "{tag} the staff step-up answered {} {}",
                r.status, r.body
            ));
            return;
        }
        let before = logs(&world.pool).await;
        match apply(world, flip).await {
            Ok(note) => println!("immutable_rules: {tag} {note}"),
            Err(e) => {
                failures.push(format!("{tag} could not apply the state: {e}"));
                return;
            }
        }
        // The state change itself was an admin action: the access log grew.
        let changed = logs(&world.pool).await;
        if changed.access <= before.access {
            failures.push(format!(
                "{tag} the admin actions that set the state wrote no staff_access_log row"
            ));
        }
        // And an audited admin action (a settings change; a feature switch is access-logged only) writes the
        // audit log and the access log: send a setting its own current value.
        let r = srv.call(owner, "GET", "/admin/settings", None).await;
        let list = json_of(&r);
        let probe = list
            .as_array()
            .and_then(|l| l.iter().find(|s| s["key"] == "retention.large_file_days"))
            .cloned();
        match probe {
            Some(p) => {
                let r = srv
                    .call_reason(
                        owner,
                        "PATCH",
                        "/admin/settings",
                        Some(&json!({"values": {"retention.large_file_days": p["value"]}})),
                    )
                    .await;
                if r.status != 200 {
                    failures.push(format!(
                        "{tag} the audited admin action answered {} {}",
                        r.status, r.body
                    ));
                }
            }
            None => failures.push(format!("{tag} the probe setting is not in the catalog")),
        }
        let after = logs(&world.pool).await;
        if after.audit <= changed.audit {
            failures.push(format!(
                "{tag} an audited admin action wrote no platform_audit_log row"
            ));
        }
        if after.access <= changed.access {
            failures.push(format!(
                "{tag} an audited admin action wrote no staff_access_log row"
            ));
        }
        // A staff read of a member archive is logged.
        let member = world.member_id;
        let r = srv
            .call_reason(
                owner,
                "GET",
                &format!("/admin/private/accounts/{member}/dms"),
                None,
            )
            .await;
        if r.status != 200 {
            failures.push(format!(
                "{tag} the staff read of a member archive answered {} {}",
                r.status, r.body
            ));
        }
        let read = logs(&world.pool).await;
        if read.private <= after.private {
            failures.push(format!(
                "{tag} the staff read of a member archive wrote no private_read access-log row"
            ));
        }
        // And the chain still verifies.
        let r = srv
            .call(owner, "GET", "/admin/access-log/verify", None)
            .await;
        let report = json_of(&r);
        if r.status != 200 || report["ok"] != json!(true) {
            failures.push(format!(
                "{tag} the access-log chain does not verify: {} {}",
                r.status, r.body
            ));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// BYOS-BRW-001 (byos-browser): the write side of the bucket browser. Nothing listens on the mounts'
// endpoint, so an admitted write answers 409 `BUCKET_UNREACHABLE` after the authorization decision (that is
// the proof it got past it); a refused one answers 403 or 404 before any bucket call. The part URL is local
// signing, so it answers 200 for the right caller.

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn storage_writes_hold_over_http() {
    let base = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL");
    let routes = parse_matrix();
    let infos = op_infos();
    let db = TestDb::new().await.expect("test db");
    let url = match base.rsplit_once('/') {
        Some((head, _)) => format!("{head}/{}", db.name()),
        None => panic!("bad SR_TEST_DATABASE_URL"),
    };
    let (world, stop, server) = seed(&db, &url, &infos, &pair_names(&routes)).await;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let ids: Ids = SnowflakeGenerator::new(clock.clone(), 9, 21).expect("ids");
    let id_src: sr_db::auth_repo::IdSource = {
        let g = Arc::new(SnowflakeGenerator::new(clock.clone(), 9, 22).expect("ids"));
        Arc::new(move || g.generate().get() as i64)
    };
    let auth = PgAuthRepo::new(
        world.pool.clone(),
        id_src,
        sr_db::auth_repo::parse_secret_key(KEY_HEX).unwrap(),
    );
    let friend_unshared = Cred::Session(session_for(&auth, &ids, world.users["friendtwo"]).await);
    let friend_shared = world.actor("victim").cred.clone();
    let member = world.actor("member").cred.clone();
    let owner_a = world.actor("owner_a").cred.clone();
    let insider = world.actor("insider").cred.clone();
    let outsider = world.actor("outsider").cred.clone();
    let owner_b = world.actor("owner_b").cred.clone();
    let pool = &world.pool;
    let srv = &world.srv;
    let m = world.a["mount_id"].clone();
    let up = world.a["mount_upload_id"].clone();
    let m_id: i64 = m.parse().unwrap();
    let g_mount = insert_mount(
        pool,
        &ids,
        sr_storage::mounts::MountOwner::Guild(world.guild_a),
        true,
    )
    .await
    .to_string();

    let mut failures: Vec<String> = Vec::new();
    macro_rules! expect {
        ($what:expr, $resp:expr, $status:expr) => {{
            let r: Resp = $resp;
            if r.status != $status {
                failures.push(format!(
                    "{}: expected {}, got {} {}",
                    $what,
                    $status,
                    r.status,
                    r.body.chars().take(160).collect::<String>()
                ));
            }
            r
        }};
    }
    let begin = |mount: &str| format!("/storage/mounts/{mount}/uploads");
    let body = json!({"key": "Music/a.flac", "size": 1_048_576});

    // The owner is admitted past the decision: the dead bucket is what stops the call.
    let r = expect!(
        "owner begins an upload",
        srv.call(&member, "POST", &begin(&m), Some(&body)).await,
        409
    );
    if !r.body.contains("BUCKET_UNREACHABLE") {
        failures.push(format!("owner begin should reach the bucket: {}", r.body));
    }
    // The owner's part URL is signed locally and carries no secret.
    let part = format!("{}/{up}/parts/1", begin(&m));
    let r = expect!(
        "owner asks for a part URL",
        srv.call(&member, "POST", &part, None).await,
        200
    );
    if !r.body.contains("partNumber=1") || r.body.contains("authz-matrix-secret") {
        failures.push(format!("part URL shape: {}", r.body));
    }
    expect!(
        "owner lists open uploads",
        srv.call(&member, "GET", &begin(&m), None).await,
        200
    );

    // A friend the mount is shared with reaches it and still cannot write; one it is not shared with cannot see it.
    let victim = world.users["victim"];
    expect!(
        "the owner shares everything",
        srv.call(
            &member,
            "PUT",
            &format!("/users/@me/storage/mounts/{m}/shares/{victim}"),
            Some(&json!({"cinema": true, "library": true, "music": true}))
        )
        .await,
        200
    );
    for (what, method, path, b) in [
        ("begin", "POST", begin(&m), Some(&body)),
        ("part", "POST", part.clone(), None),
        ("list", "GET", begin(&m), None),
    ] {
        expect!(
            &format!("shared friend: {what}"),
            srv.call(&friend_shared, method, &path, b).await,
            403
        );
    }
    let folder = json!({"key": "New/"});
    expect!(
        "shared friend: folder",
        srv.call(
            &friend_shared,
            "POST",
            &format!("/storage/mounts/{m}/folders"),
            Some(&folder)
        )
        .await,
        403
    );
    expect!(
        "shared friend: move",
        srv.call(
            &friend_shared,
            "POST",
            &format!("/storage/mounts/{m}/move"),
            Some(&json!({"from": "a.mp3", "to": "b.mp3"}))
        )
        .await,
        403
    );
    expect!(
        "shared friend: delete",
        srv.call(
            &friend_shared,
            "DELETE",
            &format!("/storage/mounts/{m}/objects/a.mp3"),
            None
        )
        .await,
        403
    );
    expect!(
        "unshared friend: begin",
        srv.call(&friend_unshared, "POST", &begin(&m), Some(&body))
            .await,
        404
    );
    expect!(
        "another tenant's owner: begin",
        srv.call(&owner_b, "POST", &begin(&m), Some(&body)).await,
        404
    );
    expect!(
        "another tenant's owner: the upload id",
        srv.call(&owner_b, "DELETE", &format!("{}/{up}", begin(&m)), None)
            .await,
        404
    );

    // A server shelf: the server owner may write; a member without the upload grant and a stranger may not.
    expect!(
        "server owner begins",
        srv.call(&owner_a, "POST", &begin(&g_mount), Some(&body))
            .await,
        409
    );
    expect!(
        "server owner makes a folder",
        srv.call(
            &owner_a,
            "POST",
            &format!("/storage/mounts/{g_mount}/folders"),
            Some(&folder)
        )
        .await,
        409
    );
    let r = srv
        .call(&insider, "POST", &begin(&g_mount), Some(&body))
        .await;
    if r.status != 403 && r.status != 404 {
        failures.push(format!("insider begin: {} {}", r.status, r.body));
    }
    expect!(
        "server stranger begins",
        srv.call(&outsider, "POST", &begin(&g_mount), Some(&body))
            .await,
        404
    );

    // The gate behind the decision: a mount that has not passed its CORS check starts nothing, and a disabled
    // mount is not there.
    sqlx::query("UPDATE storage_mounts SET cors_ok_at = NULL WHERE id = $1")
        .bind(m_id)
        .execute(pool)
        .await
        .unwrap();
    let r = expect!(
        "no CORS, no upload",
        srv.call(&member, "POST", &begin(&m), Some(&body)).await,
        409
    );
    if !r.body.contains("CORS_NOT_READY") {
        failures.push(format!("expected CORS_NOT_READY: {}", r.body));
    }
    sqlx::query("UPDATE storage_mounts SET status = 'disabled' WHERE id = $1")
        .bind(m_id)
        .execute(pool)
        .await
        .unwrap();
    expect!(
        "a disabled mount takes no upload",
        srv.call(&member, "POST", &begin(&m), Some(&body)).await,
        404
    );

    let _ = stop.send(());
    let _ = tokio::time::timeout(Duration::from_secs(40), server).await;
    db.cleanup().await.ok();
    assert!(
        failures.is_empty(),
        "{} storage write failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

// ---------------------------------------------------------------------------------------------
// BYOS-CAT-002 (byos-catalog): the shelves over HTTP. The seed put an unpublished series on each tenant's mount
// (`shelf_item_id`, `shelf_part_id`); who sees it, who may edit it and who may start a re-index follows
// `sr_storage::scopes`, and nothing of it is in the instance catalog.

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn shelves_hold_over_http() {
    let base = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL");
    let routes = parse_matrix();
    let infos = op_infos();
    let db = TestDb::new().await.expect("test db");
    let url = match base.rsplit_once('/') {
        Some((head, _)) => format!("{head}/{}", db.name()),
        None => panic!("bad SR_TEST_DATABASE_URL"),
    };
    let (world, stop, server) = seed(&db, &url, &infos, &pair_names(&routes)).await;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let ids: Ids = SnowflakeGenerator::new(clock.clone(), 9, 23).expect("ids");
    let id_src: sr_db::auth_repo::IdSource = {
        let g = Arc::new(SnowflakeGenerator::new(clock.clone(), 9, 24).expect("ids"));
        Arc::new(move || g.generate().get() as i64)
    };
    let auth = PgAuthRepo::new(
        world.pool.clone(),
        id_src,
        sr_db::auth_repo::parse_secret_key(KEY_HEX).unwrap(),
    );
    let stranger = Cred::Session(session_for(&auth, &ids, world.users["friendtwo"]).await);
    let friend = world.actor("victim").cred.clone();
    let owner = world.actor("member").cred.clone();
    let srv = &world.srv;
    let pool = &world.pool;
    let m = world.a["mount_id"].clone();
    let item = world.a["shelf_item_id"].clone();
    let part = world.a["shelf_part_id"].clone();
    let other_item = world.b["shelf_item_id"].clone();
    let other_part = world.b["shelf_part_id"].clone();
    let victim = world.users["victim"];

    let mut failures: Vec<String> = Vec::new();
    macro_rules! expect {
        ($what:expr, $resp:expr, $status:expr) => {{
            let r: Resp = $resp;
            if r.status != $status {
                failures.push(format!(
                    "{}: expected {}, got {} {}",
                    $what,
                    $status,
                    r.status,
                    r.body.chars().take(160).collect::<String>()
                ));
            }
            r
        }};
    }
    macro_rules! lists {
        ($what:expr, $who:expr, $yes:expr) => {{
            let r = expect!(
                $what,
                srv.call($who, "GET", "/shelves/cinema", None).await,
                200
            );
            if r.body.contains(&item) != $yes {
                failures.push(format!("{}: the item should be listed = {}", $what, $yes));
            }
            r
        }};
    }

    // The owner sees it, as "own", with its episode.
    let r = lists!("owner lists", &owner, true);
    if !r.body.contains("\"scope\":\"own\"") || !r.body.contains(&m) {
        failures.push("the owner's card names its source".into());
    }
    let r = expect!(
        "owner opens it",
        srv.call(&owner, "GET", &format!("/shelves/cinema/{item}"), None)
            .await,
        200
    );
    if !r.body.contains(&part) || !r.body.contains("\"can_edit\":true") {
        failures.push(format!(
            "owner detail: {}",
            r.body.chars().take(200).collect::<String>()
        ));
    }
    // It is not in the instance catalog.
    expect!(
        "the instance catalog does not know it",
        srv.call(&owner, "GET", &format!("/cinema/titles/{item}"), None)
            .await,
        404
    );

    // A friend without a share, and a stranger, see nothing.
    lists!("friend before the share", &friend, false);
    lists!("stranger", &stranger, false);
    for (who, name) in [(&friend, "friend"), (&stranger, "stranger")] {
        expect!(
            &format!("{name} opens it"),
            srv.call(who, "GET", &format!("/shelves/cinema/{item}"), None)
                .await,
            404
        );
        expect!(
            &format!("{name} edits it"),
            srv.call(
                who,
                "PATCH",
                &format!("/shelves/cinema/{item}"),
                Some(&json!({"title": "x"}))
            )
            .await,
            404
        );
        expect!(
            &format!("{name} starts an index"),
            srv.call(who, "POST", &format!("/storage/mounts/{m}/index"), None)
                .await,
            404
        );
    }

    // A library-only share shows no film; a cinema share shows it, read-only.
    expect!(
        "the owner shares the library",
        srv.call(
            &owner,
            "PUT",
            &format!("/users/@me/storage/mounts/{m}/shares/{victim}"),
            Some(&json!({"cinema": false, "library": true, "music": false}))
        )
        .await,
        200
    );
    lists!("friend with a library share", &friend, false);
    expect!(
        "the owner shares cinema",
        srv.call(
            &owner,
            "PUT",
            &format!("/users/@me/storage/mounts/{m}/shares/{victim}"),
            Some(&json!({"cinema": true, "library": false, "music": false}))
        )
        .await,
        200
    );
    let r = lists!("friend with a cinema share", &friend, true);
    if !r.body.contains("\"scope\":\"friend\"") {
        failures.push("the friend's card says friend".into());
    }
    let r = expect!(
        "friend opens it",
        srv.call(&friend, "GET", &format!("/shelves/cinema/{item}"), None)
            .await,
        200
    );
    if !r.body.contains("\"can_edit\":false") {
        failures.push("a friend cannot edit".into());
    }
    expect!(
        "friend edits it",
        srv.call(
            &friend,
            "PATCH",
            &format!("/shelves/cinema/{item}"),
            Some(&json!({"title": "x"}))
        )
        .await,
        403
    );
    expect!(
        "friend edits a part",
        srv.call(
            &friend,
            "PATCH",
            &format!("/shelves/cinema/{item}/parts/{part}"),
            Some(&json!({"title": "x"}))
        )
        .await,
        403
    );
    expect!(
        "friend starts an index",
        srv.call(&friend, "POST", &format!("/storage/mounts/{m}/index"), None)
            .await,
        403
    );
    expect!(
        "friend reads the index status",
        srv.call(&friend, "GET", &format!("/storage/mounts/{m}/index"), None)
            .await,
        403
    );
    // The shelf of music is not the shelf of cinema.
    expect!(
        "friend opens the item in the wrong section",
        srv.call(&friend, "GET", &format!("/shelves/music/{item}"), None)
            .await,
        404
    );

    // The owner edits; the edit is an override and shows.
    let r = expect!(
        "owner edits",
        srv.call(
            &owner,
            "PATCH",
            &format!("/shelves/cinema/{item}"),
            Some(&json!({"title": "Renamed", "year": 1999, "tags": ["Cult"],
                         "details": {"provider": "tmdb", "external_id": "603"}}))
        )
        .await,
        200
    );
    for needle in [
        "\"title\":\"Renamed\"",
        "\"overridden\"",
        "\"cult\"",
        "not endorsed",
    ] {
        if !r.body.contains(needle) {
            failures.push(format!(
                "the edit shows {needle}: {}",
                r.body.chars().take(300).collect::<String>()
            ));
        }
    }
    expect!(
        "owner edits a part",
        srv.call(
            &owner,
            "PATCH",
            &format!("/shelves/cinema/{item}/parts/{part}"),
            Some(&json!({"title": "Pilot", "number": 3}))
        )
        .await,
        200
    );
    expect!(
        "an unknown attribution source",
        srv.call(
            &owner,
            "PATCH",
            &format!("/shelves/cinema/{item}"),
            Some(&json!({"details": {"provider": "evil", "external_id": "1"}}))
        )
        .await,
        400
    );
    expect!(
        "a key that climbs out of the mount",
        srv.call(
            &owner,
            "PATCH",
            &format!("/shelves/cinema/{item}"),
            Some(&json!({"poster_key": "../secret"}))
        )
        .await,
        400
    );
    expect!(
        "the owner queues a re-index",
        srv.call(&owner, "POST", &format!("/storage/mounts/{m}/index"), None)
            .await,
        202
    );
    let r = expect!(
        "a second request while one is queued",
        srv.call(&owner, "POST", &format!("/storage/mounts/{m}/index"), None)
            .await,
        202
    );
    if !r.body.contains("\"queued\":false") {
        failures.push("one queued index job per mount".into());
    }
    expect!(
        "the owner reads the status",
        srv.call(&owner, "GET", &format!("/storage/mounts/{m}/index"), None)
            .await,
        200
    );
    expect!(
        "the instance bucket is not indexed",
        srv.call(&owner, "POST", "/storage/mounts/1/index", None)
            .await,
        404
    );

    // The other tenant's item is nobody else's, and a made-up section is nothing.
    expect!(
        "cross tenant read",
        srv.call(
            &owner,
            "GET",
            &format!("/shelves/cinema/{other_item}"),
            None
        )
        .await,
        404
    );
    expect!(
        "cross tenant edit",
        srv.call(
            &owner,
            "PATCH",
            &format!("/shelves/cinema/{other_item}"),
            Some(&json!({"title": "x"}))
        )
        .await,
        404
    );
    expect!(
        "cross tenant part",
        srv.call(
            &owner,
            "PATCH",
            &format!("/shelves/cinema/{other_item}/parts/{part}"),
            Some(&json!({"title": "x"}))
        )
        .await,
        404
    );
    expect!(
        "a part of another item",
        srv.call(
            &owner,
            "PATCH",
            &format!("/shelves/cinema/{item}/parts/{other_part}"),
            Some(&json!({"title": "x"}))
        )
        .await,
        404
    );
    expect!(
        "no such section",
        srv.call(&owner, "GET", "/shelves/podcasts", None).await,
        404
    );

    // A disabled mount takes the item off every shelf.
    sqlx::query("UPDATE storage_mounts SET status = 'disabled' WHERE id = $1")
        .bind(m.parse::<i64>().unwrap())
        .execute(pool)
        .await
        .unwrap();
    lists!("owner with the mount disabled", &owner, false);
    lists!("friend with the mount disabled", &friend, false);
    expect!(
        "owner opens it while disabled",
        srv.call(&owner, "GET", &format!("/shelves/cinema/{item}"), None)
            .await,
        404
    );

    let _ = stop.send(());
    let _ = tokio::time::timeout(Duration::from_secs(40), server).await;
    db.cleanup().await.ok();
    assert!(
        failures.is_empty(),
        "{} shelf failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

// ---------------------------------------------------------------------------------------------
// PRF-PRO-001 (D-382): profile privacy over HTTP, by actor.

const PP_BIO: &str = "PPBIO-visible-sentence";
const PP_LINK: &str = "pp-links-secret.example";
const PP_GUEST: &str = "PPGUEST-body";

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn profile_privacy_holds_over_http() {
    if harness::embedded() {
        return;
    }
    let base = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL");
    let routes = parse_matrix();
    let infos = op_infos();
    let db = TestDb::new().await.expect("test db");
    let url = match base.rsplit_once('/') {
        Some((head, _)) => format!("{head}/{}", db.name()),
        None => panic!("bad SR_TEST_DATABASE_URL"),
    };
    let (world, stop, server) = seed(&db, &url, &infos, &pair_names(&routes)).await;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let ids: Ids = SnowflakeGenerator::new(clock.clone(), 9, 25).expect("ids");
    let id_src: sr_db::auth_repo::IdSource = {
        let g = Arc::new(SnowflakeGenerator::new(clock.clone(), 9, 26).expect("ids"));
        Arc::new(move || g.generate().get() as i64)
    };
    let auth = PgAuthRepo::new(
        world.pool.clone(),
        id_src,
        sr_db::auth_repo::parse_secret_key(KEY_HEX).unwrap(),
    );
    let pool = &world.pool;
    let srv = &world.srv;

    // The cast: the owner, a member with no tie, one who shares a server, a friend, one the owner blocked and
    // one who blocked the owner.
    let owner_id = account(pool, &ids, "ppowner").await;
    let stranger_id = account(pool, &ids, "ppstranger").await;
    let mutual_id = account(pool, &ids, "ppmutual").await;
    let friend_id = account(pool, &ids, "ppfriend").await;
    let blocked_id = account(pool, &ids, "ppblocked").await;
    let blocker_id = account(pool, &ids, "ppblocker").await;
    let cred = |id: i64| {
        let auth = &auth;
        let ids = &ids;
        async move { Cred::Session(session_for(auth, ids, id).await) }
    };
    let owner = cred(owner_id).await;
    let stranger = cred(stranger_id).await;
    let mutual = cred(mutual_id).await;
    let friend = cred(friend_id).await;
    let blocked = cred(blocked_id).await;
    let blocker = cred(blocker_id).await;
    add_member(pool, world.guild_a, owner_id).await;
    add_member(pool, world.guild_a, mutual_id).await;
    for (a, b, t) in [
        (owner_id, friend_id, 1),
        (friend_id, owner_id, 1),
        (owner_id, blocked_id, 2),
        (blocker_id, owner_id, 2),
    ] {
        sqlx::query("INSERT INTO relationships (user_id, target_id, type) VALUES ($1, $2, $3)")
            .bind(a)
            .bind(b)
            .bind(t as i16)
            .execute(pool)
            .await
            .unwrap();
    }
    let staff = world.actor("staff_owner").cred.clone();
    let flag = async |on: bool| {
        srv.call(
            &staff,
            "PATCH",
            "/admin/features/platform.public_profiles",
            Some(&json!({"enabled": on})),
        )
        .await
    };

    let mut failures: Vec<String> = Vec::new();
    macro_rules! expect {
        ($what:expr, $resp:expr, $status:expr) => {{
            let r: Resp = $resp;
            if r.status != $status {
                failures.push(format!(
                    "{}: expected {}, got {} {}",
                    $what,
                    $status,
                    r.status,
                    r.body.chars().take(160).collect::<String>()
                ));
            }
            r
        }};
    }

    // The page: bio, links, a typed now playing line and the guestbook.
    expect!(
        "owner saves the page",
        srv.call(
            &owner,
            "PUT",
            "/users/@me/profile-doc",
            Some(&json!({"version": 1, "blocks": [
                {"id": "head", "type": "header", "x": 0, "y": 0, "w": 12, "h": 2, "config": {}},
                {"id": "bio", "type": "about", "x": 0, "y": 2, "w": 6, "h": 2, "config": {"text": PP_BIO}},
                {"id": "links", "type": "linktree", "x": 6, "y": 2, "w": 6, "h": 2,
                 "config": {"links": [{"label": "x", "url": format!("https://{PP_LINK}/a")}]}},
                {"id": "gb", "type": "guestbook", "x": 0, "y": 4, "w": 12, "h": 3, "config": {}}
            ]})),
        )
        .await,
        200
    );

    // Without the operator's switch (the default) the public route answers 404 for every handle, whatever the
    // member chose, and a members-only page is not public.
    let put = |preset: &str| {
        let body = json!({ "preset": preset });
        let owner = owner.clone();
        async move {
            srv.call(&owner, "PUT", "/users/@me/profile-privacy", Some(&body))
                .await
        }
    };
    let crawler = async |path: &str| -> Resp {
        raw(
            srv.port,
            "GET",
            &format!("/api/v1{path}"),
            &[
                ("Origin".to_owned(), srv.public.clone()),
                (
                    "User-Agent".to_owned(),
                    "Googlebot/2.1 (+http://www.google.com/bot.html)".to_owned(),
                ),
            ],
            None,
        )
        .await
    };
    expect!("owner picks public", put("public").await, 200);
    // The shared seed turned public pages on for its own sweep; this test starts from the default (off).
    expect!("operator switch off", flag(false).await, 200);
    for (who, cred) in [("anonymous", Cred::Anon)] {
        expect!(
            &format!("{who} with the switch off"),
            srv.call(&cred, "GET", "/public/profiles/ppowner", None)
                .await,
            404
        );
    }
    let r = expect!(
        "crawler with the switch off",
        crawler("/public/profiles/ppowner").await,
        404
    );
    if !r.head.contains("x-robots-tag: noindex, nofollow") {
        failures.push("a 404 for a crawler still says noindex".into());
    }
    expect!(
        "unknown handle with the switch off",
        srv.call(&Cred::Anon, "GET", "/public/profiles/nobodyhere", None)
            .await,
        404
    );
    expect!("operator allows public pages", flag(true).await, 200);
    expect!(
        "unknown handle with the switch on is the same 404",
        srv.call(&Cred::Anon, "GET", "/public/profiles/nobodyhere", None)
            .await,
        404
    );

    // The four presets against every actor on the signed-in route.
    let signed: [(&str, &Cred, u8); 4] = [
        ("member_stranger", &stranger, 1),
        ("member_mutual_guild", &mutual, 2),
        ("friend", &friend, 3),
        ("owner", &owner, 4),
    ];
    for (preset, page_rank) in [
        ("public", 0u8),
        ("members", 1),
        ("friends", 3),
        ("private", 4),
    ] {
        expect!(&format!("owner picks {preset}"), put(preset).await, 200);
        expect!(
            &format!("{preset}: anonymous on the signed-in route"),
            srv.call(&Cred::Anon, "GET", "/profile-pages/ppowner", None)
                .await,
            401
        );
        for (name, c, rank) in signed {
            let want = if rank >= page_rank { 200 } else { 404 };
            let r = expect!(
                &format!("{preset}: {name} reads the page"),
                srv.call(c, "GET", "/profile-pages/ppowner", None).await,
                want
            );
            if want == 200 && name != "owner" {
                // The public preset: bio and links for everyone, the guestbook block for members and up.
                let has_gb = r.body.contains("\"type\":\"guestbook\"");
                let want_gb = match preset {
                    "public" => rank >= 1,
                    _ => true,
                };
                if has_gb != want_gb {
                    failures.push(format!("{preset}: {name} guestbook block shown = {has_gb}"));
                }
            }
        }
        for (name, c) in [
            ("blocked_by_owner", &blocked),
            ("blocker_of_owner", &blocker),
        ] {
            expect!(
                &format!("{preset}: {name} reads the page"),
                srv.call(c, "GET", "/profile-pages/ppowner", None).await,
                404
            );
            expect!(
                &format!("{preset}: {name} reads the guestbook"),
                srv.call(c, "GET", &format!("/users/{owner_id}/guestbook"), None)
                    .await,
                404
            );
        }
        // The public route: anonymous and a crawler, identical, 200 only for the public preset.
        let want_pub = if preset == "public" { 200 } else { 404 };
        let r = expect!(
            &format!("{preset}: anonymous on the public route"),
            srv.call(&Cred::Anon, "GET", "/public/profiles/ppowner", None)
                .await,
            want_pub
        );
        let rc = expect!(
            &format!("{preset}: crawler on the public route"),
            crawler("/public/profiles/ppowner").await,
            want_pub
        );
        if r.body != rc.body {
            failures.push(format!(
                "{preset}: a crawler's user agent changed the answer"
            ));
        }
        for (what, resp) in [("anonymous", &r), ("crawler", &rc)] {
            if !resp.head.contains("x-robots-tag: noindex, nofollow") {
                failures.push(format!("{preset}: {what} answer lacks noindex"));
            }
        }
        if want_pub == 200 {
            if !r.body.contains(PP_BIO) || !r.body.contains(PP_LINK) {
                failures.push("the public preset shows the bio and the links".into());
            }
            if r.body.contains("\"type\":\"guestbook\"") {
                failures.push("the public preset hides the guestbook from the web".into());
            }
            if !r.head.contains("cache-control: no-store") {
                failures.push("a public page is never cached".into());
            }
        }
    }

    // Custom row 1: a public page with an only_me guestbook. The web sees the page, not the guestbook, and nobody
    // but the owner reads or signs it.
    expect!(
        "custom: public page, only_me guestbook",
        srv.call(
            &owner,
            "PUT",
            "/users/@me/profile-privacy",
            Some(
                &json!({"page_tier": "public", "links_tier": "public", "bio_tier": "public",
                          "guestbook_tier": "only_me"})
            )
        )
        .await,
        200
    );
    let r = expect!(
        "custom 1: the web reads the page",
        srv.call(&Cred::Anon, "GET", "/public/profiles/ppowner", None)
            .await,
        200
    );
    if r.body.contains("\"type\":\"guestbook\"") || r.body.contains(PP_GUEST) {
        failures.push("custom 1: an only_me guestbook shows to the web".into());
    }
    expect!(
        "custom 1: a friend cannot read the guestbook",
        srv.call(
            &friend,
            "GET",
            &format!("/users/{owner_id}/guestbook"),
            None
        )
        .await,
        404
    );
    expect!(
        "custom 1: a friend cannot sign the guestbook",
        srv.call(
            &friend,
            "POST",
            &format!("/users/{owner_id}/guestbook"),
            Some(&json!({"body": PP_GUEST}))
        )
        .await,
        403
    );
    expect!(
        "custom 1: anonymous cannot post a held note",
        srv.call(
            &Cred::Anon,
            "POST",
            "/public/profiles/ppowner/guestbook",
            Some(&json!({"body": "hello"}))
        )
        .await,
        404
    );

    // Custom row 2: a friends page with a public links block. The block clamps to the page.
    let r = expect!(
        "custom: friends page, public links",
        srv.call(
            &owner,
            "PUT",
            "/users/@me/profile-privacy",
            Some(&json!({"page_tier": "friends", "links_tier": "public", "guestbook_tier": "friends"}))
        )
        .await,
        200
    );
    if !r.body.contains("\"links_tier\":\"friends\"") {
        failures.push("custom 2: the public links block was not clamped to the page".into());
    }
    expect!(
        "custom 2: a mutual-server member sees nothing",
        srv.call(&mutual, "GET", "/profile-pages/ppowner", None)
            .await,
        404
    );
    expect!(
        "custom 2: the web sees nothing",
        srv.call(&Cred::Anon, "GET", "/public/profiles/ppowner", None)
            .await,
        404
    );
    let r = expect!(
        "custom 2: a friend sees the links",
        srv.call(&friend, "GET", "/profile-pages/ppowner", None)
            .await,
        200
    );
    if !r.body.contains(PP_LINK) {
        failures.push("custom 2: a friend sees the links block".into());
    }
    // A friend may sign a friends guestbook; the note is then read by friends only.
    expect!(
        "custom 2: a friend signs",
        srv.call(
            &friend,
            "POST",
            &format!("/users/{owner_id}/guestbook"),
            Some(&json!({"body": PP_GUEST}))
        )
        .await,
        201
    );
    expect!(
        "custom 2: a mutual-server member cannot read it",
        srv.call(
            &mutual,
            "GET",
            &format!("/users/{owner_id}/guestbook"),
            None
        )
        .await,
        404
    );

    // The public guestbook: only with the opt-in, held until the owner approves, five per address per hour.
    expect!(
        "owner opens a public guestbook",
        srv.call(
            &owner,
            "PUT",
            "/users/@me/profile-privacy",
            Some(
                &json!({"page_tier": "public", "links_tier": "public", "bio_tier": "public",
                          "guestbook_tier": "public", "guestbook_accepts_public": true})
            )
        )
        .await,
        200
    );
    expect!(
        "anonymous leaves a held note",
        srv.call(
            &Cred::Anon,
            "POST",
            "/public/profiles/ppowner/guestbook",
            Some(&json!({"body": "greetings from outside"}))
        )
        .await,
        204
    );
    expect!(
        "a link in a public note is refused",
        srv.call(
            &Cred::Anon,
            "POST",
            "/public/profiles/ppowner/guestbook",
            Some(&json!({"body": "see https://spam.example"}))
        )
        .await,
        400
    );
    let r = expect!(
        "a second anonymous fetch",
        srv.call(&Cred::Anon, "GET", "/public/profiles/ppowner", None)
            .await,
        200
    );
    if r.body.contains("greetings from outside") {
        failures.push("a held note is visible to the web".into());
    }
    let r = expect!(
        "the owner reads the held note",
        srv.call(&owner, "GET", &format!("/users/{owner_id}/guestbook"), None)
            .await,
        200
    );
    let list: Value = serde_json::from_str(&r.body).unwrap_or(Value::Null);
    let held = list
        .as_array()
        .and_then(|l| l.iter().find(|e| e["body"] == "greetings from outside"))
        .cloned();
    match held {
        Some(e) if e["pending"] == true => {
            let id = e["id"].as_str().unwrap_or("0").to_owned();
            expect!(
                "a friend cannot approve",
                srv.call(
                    &friend,
                    "POST",
                    &format!("/users/@me/guestbook/{id}/approve"),
                    None
                )
                .await,
                404
            );
            expect!(
                "the owner approves",
                srv.call(
                    &owner,
                    "POST",
                    &format!("/users/@me/guestbook/{id}/approve"),
                    None
                )
                .await,
                204
            );
            let r = expect!(
                "the web reads the approved note",
                srv.call(&Cred::Anon, "GET", "/public/profiles/ppowner", None)
                    .await,
                200
            );
            if !r.body.contains("greetings from outside") {
                failures.push("an approved note is visible to the web".into());
            }
        }
        _ => failures.push("the owner sees the held note flagged pending".into()),
    }
    // The seed's own held note shares this address; start the hourly count afresh.
    sqlx::query("UPDATE profile_guestbook SET ip_hash = NULL")
        .execute(pool)
        .await
        .unwrap();
    for i in 0..6 {
        let want = if i < 5 { 204 } else { 429 };
        let r = srv
            .call(
                &Cred::Anon,
                "POST",
                "/public/profiles/ppowner/guestbook",
                Some(&json!({"body": format!("note {i}")})),
            )
            .await;
        if r.status != want {
            failures.push(format!(
                "public note {i}: expected {want}, got {} {}",
                r.status, r.body
            ));
        }
    }

    // Custom row 3: a members page, with the operator's switch off again. It stays 404 publicly.
    expect!("operator turns public pages off", flag(false).await, 200);
    expect!(
        "custom: members page",
        srv.call(
            &owner,
            "PUT",
            "/users/@me/profile-privacy",
            Some(&json!({"page_tier": "members"}))
        )
        .await,
        200
    );
    expect!(
        "custom 3: the web gets 404 on a members page",
        srv.call(&Cred::Anon, "GET", "/public/profiles/ppowner", None)
            .await,
        404
    );
    // A stored `public` page is never effective while the switch is off: members still read it.
    expect!(
        "custom 3: public page while the switch is off",
        srv.call(
            &owner,
            "PUT",
            "/users/@me/profile-privacy",
            Some(&json!({"page_tier": "public"}))
        )
        .await,
        200
    );
    expect!(
        "custom 3: the web still gets 404",
        srv.call(&Cred::Anon, "GET", "/public/profiles/ppowner", None)
            .await,
        404
    );
    expect!(
        "custom 3: a member reads it",
        srv.call(&stranger, "GET", "/profile-pages/ppowner", None)
            .await,
        200
    );
    let r = expect!(
        "the privacy view says public is not possible",
        srv.call(&owner, "GET", "/users/@me/profile-privacy", None)
            .await,
        200
    );
    if !r.body.contains("\"public_possible\":false") {
        failures.push("GET profile-privacy tells whether public is possible".into());
    }

    let _ = stop.send(());
    let _ = tokio::time::timeout(Duration::from_secs(40), server).await;
    db.cleanup().await.ok();
    assert!(
        failures.is_empty(),
        "{} profile privacy failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
