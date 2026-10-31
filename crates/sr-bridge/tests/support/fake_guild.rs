//! BRG-SYN-003: a stateful fake Discord guild behind `HttpTransport` (design 16.1). No Postgres needed.
//!
//! It keeps roles (positions, managed flag, Discord-only bits), channels of every type, overwrites, threads, forum
//! tags and webhooks, and enforces the Discord rules the mirror depends on:
//!
//! * a bot edits only roles below its top role and grants only bits it holds (unless it is an administrator);
//! * the 250 role and 500 channel caps;
//! * announcement (5), stage (13), forum (15) and media (16) channels need the COMMUNITY feature;
//! * two name or topic changes per channel per ten minutes (the third answers 429 with `Retry-After`;
//!   [`FakeGuild::reset_rename_window`] stands in for ten minutes passing);
//! * 429 with `Retry-After` (and the global flag) on demand ([`FakeGuild::rate_limit_next`]).
//!
//! Every write records the request (including the decoded `X-Audit-Log-Reason`), emits the matching gateway
//! dispatch into [`FakeGuild::drain_events`] and an audit-log entry with the reason echoed
//! ([`FakeGuild::audit`]), so tests can prove echo suppression. [`FakeGuild::discord_admin`] edits as a Discord
//! user (no reason), producing the same dispatches without going through the bot.

use std::collections::BTreeMap;
use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::{Map, Value, json};
use sr_sdk::http::{HttpRequest, HttpResponse, HttpTransport, Method};

pub const BASE: &str = "http://fake.discord/api/v10";
pub const ADMINISTRATOR: u64 = 1 << 3;
const CAP_ROLES: usize = 250;
const CAP_CHANNELS: usize = 500;

#[derive(Debug, Clone)]
pub struct Req {
    pub method: &'static str,
    pub path: String,
    pub body: Value,
    /// The decoded `X-Audit-Log-Reason`.
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AuditEntry {
    pub action: &'static str,
    pub target_id: i64,
    pub reason: Option<String>,
    pub user_id: i64,
}

#[derive(Debug, Default)]
pub struct Inner {
    pub guild_id: i64,
    pub bot_user: i64,
    pub bot_role: i64,
    pub token: String,
    pub premium_tier: i64,
    pub community: bool,
    pub roles: BTreeMap<i64, Value>,
    pub channels: BTreeMap<i64, Value>,
    pub webhooks: BTreeMap<i64, (String, i64)>,
    pub messages: Vec<Value>,
    pub next_id: i64,
    pub log: Vec<Req>,
    pub events: Vec<(String, Value)>,
    pub audit: Vec<AuditEntry>,
    pub rate_limit_next: u32,
    pub rate_limit_global: bool,
    renames: BTreeMap<i64, u32>,
}

pub struct FakeGuild {
    pub state: Mutex<Inner>,
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

fn refuse(status: u16, code: i64, msg: &str) -> HttpResponse {
    reply(status, vec![], json!({ "message": msg, "code": code }))
}

fn missing_perms() -> HttpResponse {
    refuse(403, 50013, "Missing Permissions")
}

fn word(v: &Value, k: &str) -> u64 {
    v.get(k)
        .and_then(Value::as_str)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

fn idv(v: &Value, k: &str) -> Option<i64> {
    v.get(k)
        .and_then(Value::as_str)
        .and_then(|s| s.parse().ok())
}

fn decode_reason(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(h) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(h);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl Inner {
    fn fresh(&mut self) -> i64 {
        self.next_id += 1;
        self.next_id
    }

    fn emit(&mut self, name: &str, data: Value) {
        self.events.push((name.to_owned(), data));
    }

    fn audit_entry(
        &mut self,
        action: &'static str,
        target: i64,
        reason: &Option<String>,
        user: i64,
    ) {
        self.audit.push(AuditEntry {
            action,
            target_id: target,
            reason: reason.clone(),
            user_id: user,
        });
        self.events.push((
            "GUILD_AUDIT_LOG_ENTRY_CREATE".into(),
            json!({
                "guild_id": self.guild_id.to_string(),
                "action_type": action,
                "target_id": target.to_string(),
                "user_id": user.to_string(),
                "reason": reason,
            }),
        ));
    }

    /// The union of the permission words of `@everyone` and the bot's role.
    fn bot_perms(&self) -> u64 {
        [self.guild_id, self.bot_role]
            .iter()
            .filter_map(|r| self.roles.get(r))
            .fold(0, |a, r| a | word(r, "permissions"))
    }

    fn bot_top(&self) -> i64 {
        self.roles
            .get(&self.bot_role)
            .and_then(|r| r.get("position").and_then(Value::as_i64))
            .unwrap_or(0)
    }

    fn may_grant(&self, bits: u64) -> bool {
        let have = self.bot_perms();
        have & ADMINISTRATOR != 0 || bits & !have == 0
    }

    fn pos_of(&self, role: i64) -> i64 {
        self.roles
            .get(&role)
            .and_then(|r| r.get("position").and_then(Value::as_i64))
            .unwrap_or(0)
    }

    fn overwrites_grant_ok(&self, list: &Value) -> bool {
        list.as_array().is_none_or(|a| {
            a.iter()
                .all(|o| self.may_grant(word(o, "allow") | word(o, "deny")))
        })
    }

    fn channel_json(&self, id: i64) -> Value {
        self.channels.get(&id).cloned().unwrap_or(Value::Null)
    }

    fn community_only(kind: i64) -> bool {
        matches!(kind, 5 | 13 | 15 | 16)
    }
}

impl FakeGuild {
    /// A guild with `@everyone` (id = guild id, position 0, default view/send/read), the bot's role pinned at the
    /// top with ADMINISTRATOR, and the bot user `bot_user`.
    pub fn new(guild_id: i64, bot_user: i64, token: &str) -> Self {
        let mut s = Inner {
            guild_id,
            bot_user,
            token: token.to_owned(),
            next_id: guild_id + 1000,
            ..Inner::default()
        };
        s.roles.insert(
            guild_id,
            json!({"id": guild_id.to_string(), "name": "@everyone", "color": 0, "hoist": false,
                   "position": 0, "permissions": ((1u64<<10)|(1<<11)|(1<<16)).to_string(),
                   "managed": false, "mentionable": false}),
        );
        let bot_role = s.fresh();
        s.bot_role = bot_role;
        s.roles.insert(
            bot_role,
            json!({"id": bot_role.to_string(), "name": "bridge-bot", "color": 0, "hoist": false,
                   "position": 1, "permissions": ADMINISTRATOR.to_string(), "managed": true,
                   "mentionable": false, "tags": {"bot_id": bot_user.to_string()}}),
        );
        Self {
            state: Mutex::new(s),
        }
    }

    pub fn with<R>(&self, f: impl FnOnce(&mut Inner) -> R) -> R {
        f(&mut self.state.lock().unwrap())
    }

    /// Take the gateway dispatches emitted so far: `(event name, data)`.
    pub fn drain_events(&self) -> Vec<(String, Value)> {
        self.with(|s| std::mem::take(&mut s.events))
    }

    pub fn audit(&self) -> Vec<AuditEntry> {
        self.with(|s| s.audit.clone())
    }

    pub fn log(&self) -> Vec<Req> {
        self.with(|s| s.log.clone())
    }

    /// The next `n` calls answer 429.
    pub fn rate_limit_next(&self, n: u32, global: bool) {
        self.with(|s| {
            s.rate_limit_next = n;
            s.rate_limit_global = global;
        });
    }

    /// Ten minutes have passed: the rename counters reset.
    pub fn reset_rename_window(&self) {
        self.with(|s| s.renames.clear());
    }

    pub fn set_community(&self, on: bool) {
        self.with(|s| s.community = on);
    }

    pub fn set_tier(&self, tier: i64) {
        self.with(|s| s.premium_tier = tier);
    }

    pub fn role_ids(&self) -> Vec<i64> {
        self.with(|s| s.roles.keys().copied().collect())
    }

    pub fn role(&self, id: i64) -> Option<Value> {
        self.with(|s| s.roles.get(&id).cloned())
    }

    pub fn channel(&self, id: i64) -> Option<Value> {
        self.with(|s| s.channels.get(&id).cloned())
    }

    pub fn channel_ids(&self) -> Vec<i64> {
        self.with(|s| s.channels.keys().copied().collect())
    }

    /// Seed a role as it existed on Discord before the bridge (no events, no audit).
    pub fn seed_role(&self, name: &str, permissions: u64, position: i64, managed: bool) -> i64 {
        self.with(|s| {
            let id = s.fresh();
            s.roles.insert(
                id,
                json!({"id": id.to_string(), "name": name, "color": 0, "hoist": false,
                       "position": position, "permissions": permissions.to_string(),
                       "managed": managed, "mentionable": false}),
            );
            id
        })
    }

    /// Seed a channel/category/forum/thread. `extra` fields are merged in (parent_id, permission_overwrites...).
    pub fn seed_channel(&self, kind: i64, name: &str, extra: Value) -> i64 {
        self.with(|s| {
            let id = s.fresh();
            let mut v = json!({"id": id.to_string(), "type": kind, "name": name, "position": 0,
                               "guild_id": s.guild_id.to_string(), "permission_overwrites": []});
            if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
                for (k, x) in e {
                    o.insert(k.clone(), x.clone());
                }
            }
            s.channels.insert(id, v);
            id
        })
    }

    /// A Discord user with Manage Roles/Channels edits `path` directly: same state change, the same dispatches,
    /// an audit entry by `user` with no bridge reason. Panics on a request the fake would refuse.
    pub fn discord_admin(&self, user: i64, method: Method, path: &str, body: Value) -> Value {
        let mut s = self.state.lock().unwrap();
        let r = route(&mut s, method_str(method), path, &body, &None, user, true);
        assert!(r.status < 300, "discord admin call refused: {}", r.status);
        serde_json::from_slice(&r.body).unwrap_or(Value::Null)
    }
}

fn method_str(m: Method) -> &'static str {
    match m {
        Method::Get => "GET",
        Method::Post => "POST",
        Method::Put => "PUT",
        Method::Patch => "PATCH",
        Method::Delete => "DELETE",
    }
}

#[async_trait]
impl HttpTransport for FakeGuild {
    async fn send(&self, req: HttpRequest) -> sr_sdk::Result<HttpResponse> {
        let rel = req.url.strip_prefix(BASE).unwrap_or(&req.url).to_owned();
        let method = method_str(req.method);
        let body: Value = req
            .body
            .as_deref()
            .and_then(|b| serde_json::from_slice(b).ok())
            .unwrap_or(Value::Null);
        let reason = req
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("x-audit-log-reason"))
            .map(|(_, v)| decode_reason(v));
        let auth = req
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("authorization"))
            .map(|(_, v)| v.clone());
        let mut s = self.state.lock().unwrap();
        s.log.push(Req {
            method,
            path: rel.clone(),
            body: body.clone(),
            reason: reason.clone(),
        });
        if s.rate_limit_next > 0 {
            s.rate_limit_next -= 1;
            let mut h = vec![("Retry-After", "1".to_owned())];
            if s.rate_limit_global {
                h.push(("X-RateLimit-Global", "true".to_owned()));
            }
            return Ok(reply(
                429,
                h,
                json!({"message": "You are being rate limited.", "retry_after": 1.0,
                       "global": s.rate_limit_global}),
            ));
        }
        let webhook = rel.starts_with("/webhooks/");
        if !webhook && auth.as_deref() != Some(&format!("Bot {}", s.token)) {
            return Ok(refuse(401, 0, "401: Unauthorized"));
        }
        let bot = s.bot_user;
        Ok(route(&mut s, method, &rel, &body, &reason, bot, false))
    }
}

fn route(
    s: &mut Inner,
    method: &'static str,
    rel: &str,
    body: &Value,
    reason: &Option<String>,
    actor: i64,
    as_admin: bool,
) -> HttpResponse {
    let (path, query) = rel.split_once('?').unwrap_or((rel, ""));
    let seg: Vec<&str> = path.trim_matches('/').split('/').collect();
    let g = s.guild_id.to_string();
    let bot_checks = !as_admin;
    match (method, seg.as_slice()) {
        ("GET", ["guilds", gid]) if *gid == g => reply(
            200,
            vec![],
            json!({"id": g, "name": "Fake guild", "owner_id": "1", "premium_tier": s.premium_tier,
                   "features": if s.community { json!(["COMMUNITY"]) } else { json!([]) }}),
        ),
        ("GET", ["guilds", gid, "members", uid]) if *gid == g => {
            if uid.parse::<i64>().ok() == Some(s.bot_user) {
                reply(
                    200,
                    vec![],
                    json!({"user": {"id": uid, "bot": true}, "roles": [s.bot_role.to_string()]}),
                )
            } else {
                refuse(404, 10007, "Unknown Member")
            }
        }
        ("GET", ["guilds", gid, "roles"]) if *gid == g => reply(
            200,
            vec![],
            Value::Array(s.roles.values().cloned().collect()),
        ),
        ("POST", ["guilds", gid, "roles"]) if *gid == g => {
            create_role(s, body, reason, actor, bot_checks)
        }
        ("PATCH", ["guilds", gid, "roles"]) if *gid == g => {
            let Some(list) = body.as_array() else {
                return refuse(400, 50035, "Invalid Form Body");
            };
            if bot_checks {
                let top = s.bot_top();
                if list.iter().any(|e| {
                    idv(e, "id").is_none_or(|i| {
                        s.pos_of(i) >= top || e["position"].as_i64().unwrap_or(0) >= top
                    })
                }) {
                    return missing_perms();
                }
            }
            for e in list {
                if let (Some(i), Some(p)) = (idv(e, "id"), e["position"].as_i64())
                    && let Some(r) = s.roles.get_mut(&i)
                {
                    r["position"] = json!(p);
                }
            }
            for e in list {
                if let Some(i) = idv(e, "id") {
                    let r = s.roles.get(&i).cloned().unwrap_or(Value::Null);
                    s.emit("GUILD_ROLE_UPDATE", json!({"guild_id": g, "role": r}));
                }
            }
            reply(
                200,
                vec![],
                Value::Array(s.roles.values().cloned().collect()),
            )
        }
        ("PATCH", ["guilds", gid, "roles", rid]) if *gid == g => {
            let Ok(id) = rid.parse::<i64>() else {
                return refuse(404, 10011, "Unknown Role");
            };
            edit_role(s, id, body, reason, actor, bot_checks)
        }
        ("DELETE", ["guilds", gid, "roles", rid]) if *gid == g => {
            let Ok(id) = rid.parse::<i64>() else {
                return refuse(404, 10011, "Unknown Role");
            };
            if !s.roles.contains_key(&id) {
                return refuse(404, 10011, "Unknown Role");
            }
            if id == s.guild_id || (bot_checks && s.pos_of(id) >= s.bot_top()) {
                return missing_perms();
            }
            s.roles.remove(&id);
            s.emit(
                "GUILD_ROLE_DELETE",
                json!({"guild_id": g, "role_id": id.to_string()}),
            );
            s.audit_entry("ROLE_DELETE", id, reason, actor);
            reply(204, vec![], Value::Null)
        }
        ("GET", ["guilds", gid, "channels"]) if *gid == g => reply(
            200,
            vec![],
            Value::Array(
                s.channels
                    .values()
                    .filter(|c| !matches!(c["type"].as_i64(), Some(10..=12)))
                    .cloned()
                    .collect(),
            ),
        ),
        ("POST", ["guilds", gid, "channels"]) if *gid == g => {
            create_channel(s, body, reason, actor, bot_checks)
        }
        ("PATCH", ["guilds", gid, "channels"]) if *gid == g => {
            let Some(list) = body.as_array() else {
                return refuse(400, 50035, "Invalid Form Body");
            };
            for e in list {
                let Some(i) = idv(e, "id") else { continue };
                if let Some(c) = s.channels.get_mut(&i) {
                    if let Some(p) = e["position"].as_i64() {
                        c["position"] = json!(p);
                    }
                    if e.get("parent_id").is_some() {
                        c["parent_id"] = e["parent_id"].clone();
                    }
                }
                let c = s.channel_json(i);
                s.emit("CHANNEL_UPDATE", c);
            }
            reply(204, vec![], Value::Null)
        }
        ("GET", ["guilds", gid, "threads", "active"]) if *gid == g => reply(
            200,
            vec![],
            json!({"threads": s.channels.values()
                .filter(|c| matches!(c["type"].as_i64(), Some(10..=12))
                    && !c["thread_metadata"]["archived"].as_bool().unwrap_or(false))
                .cloned().collect::<Vec<_>>(), "members": []}),
        ),
        ("GET", ["channels", cid]) => {
            match cid.parse::<i64>().ok().and_then(|i| s.channels.get(&i)) {
                Some(c) => reply(200, vec![], c.clone()),
                None => refuse(404, 10003, "Unknown Channel"),
            }
        }
        ("PATCH", ["channels", cid]) => {
            let Some(id) = cid
                .parse::<i64>()
                .ok()
                .filter(|i| s.channels.contains_key(i))
            else {
                return refuse(404, 10003, "Unknown Channel");
            };
            edit_channel(s, id, body, reason, actor, bot_checks)
        }
        ("DELETE", ["channels", cid]) => {
            let Some(id) = cid
                .parse::<i64>()
                .ok()
                .filter(|i| s.channels.contains_key(i))
            else {
                return refuse(404, 10003, "Unknown Channel");
            };
            let c = s.channels.remove(&id).unwrap_or(Value::Null);
            // Discord moves a category's children to the top level.
            if c["type"].as_i64() == Some(4) {
                let ids: Vec<i64> = s
                    .channels
                    .iter()
                    .filter(|(_, x)| idv(x, "parent_id") == Some(id))
                    .map(|(k, _)| *k)
                    .collect();
                for k in ids {
                    s.channels.get_mut(&k).map(|x| x["parent_id"] = Value::Null);
                    let cj = s.channel_json(k);
                    s.emit("CHANNEL_UPDATE", cj);
                }
            }
            let ev = if matches!(c["type"].as_i64(), Some(10..=12)) {
                "THREAD_DELETE"
            } else {
                "CHANNEL_DELETE"
            };
            s.emit(ev, c.clone());
            s.audit_entry("CHANNEL_DELETE", id, reason, actor);
            reply(200, vec![], c)
        }
        ("PUT", ["channels", cid, "permissions", target]) => {
            let (Some(id), Some(t)) = (cid.parse::<i64>().ok(), target.parse::<i64>().ok()) else {
                return refuse(404, 10003, "Unknown Channel");
            };
            if !s.channels.contains_key(&id) {
                return refuse(404, 10003, "Unknown Channel");
            }
            let ow = json!({"id": t.to_string(), "type": body["type"], "allow": body["allow"], "deny": body["deny"]});
            if bot_checks && !s.overwrites_grant_ok(&json!([ow.clone()])) {
                return missing_perms();
            }
            if let Some(list) = s
                .channels
                .get_mut(&id)
                .and_then(|c| c["permission_overwrites"].as_array_mut())
            {
                list.retain(|o| idv(o, "id") != Some(t));
                list.push(ow);
            }
            let cj = s.channel_json(id);
            s.emit("CHANNEL_UPDATE", cj);
            s.audit_entry("CHANNEL_OVERWRITE_UPDATE", id, reason, actor);
            reply(204, vec![], Value::Null)
        }
        ("DELETE", ["channels", cid, "permissions", target]) => {
            let (Some(id), Some(t)) = (cid.parse::<i64>().ok(), target.parse::<i64>().ok()) else {
                return refuse(404, 10003, "Unknown Channel");
            };
            if let Some(list) = s
                .channels
                .get_mut(&id)
                .and_then(|c| c["permission_overwrites"].as_array_mut())
            {
                list.retain(|o| idv(o, "id") != Some(t));
            }
            let cj = s.channel_json(id);
            s.emit("CHANNEL_UPDATE", cj);
            s.audit_entry("CHANNEL_OVERWRITE_DELETE", id, reason, actor);
            reply(204, vec![], Value::Null)
        }
        ("POST", ["channels", cid, "threads"]) => {
            let Some(parent) = cid
                .parse::<i64>()
                .ok()
                .filter(|i| s.channels.contains_key(i))
            else {
                return refuse(404, 10003, "Unknown Channel");
            };
            create_thread(s, parent, body, reason, actor)
        }
        ("POST", ["channels", cid, "webhooks"]) => {
            let Some(ch) = cid
                .parse::<i64>()
                .ok()
                .filter(|i| s.channels.contains_key(i))
            else {
                return refuse(404, 10003, "Unknown Channel");
            };
            let id = s.fresh();
            let tok = format!("whtok{id}");
            s.webhooks.insert(id, (tok.clone(), ch));
            reply(
                200,
                vec![],
                json!({"id": id.to_string(), "token": tok, "type": 1}),
            )
        }
        ("POST", ["webhooks", wid, wtok]) => {
            let Some((id, (tok, ch))) = wid
                .parse::<i64>()
                .ok()
                .and_then(|i| s.webhooks.get(&i).map(|w| (i, w.clone())))
            else {
                return refuse(404, 10015, "Unknown Webhook");
            };
            if tok != *wtok {
                return refuse(401, 50027, "Invalid Webhook Token");
            }
            let thread_q = query
                .split('&')
                .find_map(|p| p.strip_prefix("thread_id="))
                .and_then(|v| v.parse::<i64>().ok());
            let mut target = thread_q.unwrap_or(ch);
            if let Some(name) = body["thread_name"].as_str() {
                let mut params = json!({"name": name, "type": 11});
                if body.get("applied_tags").is_some() {
                    params["applied_tags"] = body["applied_tags"].clone();
                }
                let r = create_thread(s, ch, &params, &None, id);
                let v: Value = serde_json::from_slice(&r.body).unwrap_or(Value::Null);
                target = idv(&v, "id").unwrap_or(ch);
            }
            let mid = s.fresh();
            let m = json!({"id": mid.to_string(), "channel_id": target.to_string(), "content": body["content"], "webhook_id": id.to_string()});
            s.messages.push(m.clone());
            reply(200, vec![], m)
        }
        ("PATCH" | "DELETE", ["webhooks", _, _, "messages", _]) => reply(
            if method == "DELETE" { 204 } else { 200 },
            vec![],
            if method == "DELETE" {
                Value::Null
            } else {
                json!({})
            },
        ),
        _ => refuse(404, 0, "Unknown route"),
    }
}

fn create_role(
    s: &mut Inner,
    body: &Value,
    reason: &Option<String>,
    actor: i64,
    check: bool,
) -> HttpResponse {
    if s.roles.len() >= CAP_ROLES {
        return refuse(400, 30005, "Maximum number of server roles reached (250)");
    }
    let perms = word(body, "permissions");
    if check && !s.may_grant(perms) {
        return missing_perms();
    }
    let id = s.fresh();
    for r in s.roles.values_mut() {
        let p = r["position"].as_i64().unwrap_or(0);
        if p >= 1 {
            r["position"] = json!(p + 1);
        }
    }
    let role = json!({"id": id.to_string(), "name": body["name"].as_str().unwrap_or("new role"),
        "color": body["color"].as_i64().unwrap_or(0), "hoist": body["hoist"].as_bool().unwrap_or(false),
        "position": 1, "permissions": perms.to_string(), "managed": false,
        "mentionable": body["mentionable"].as_bool().unwrap_or(false),
        "unicode_emoji": body.get("unicode_emoji").cloned().unwrap_or(Value::Null)});
    s.roles.insert(id, role.clone());
    let g = s.guild_id.to_string();
    s.emit("GUILD_ROLE_CREATE", json!({"guild_id": g, "role": role}));
    s.audit_entry("ROLE_CREATE", id, reason, actor);
    reply(200, vec![], role)
}

fn edit_role(
    s: &mut Inner,
    id: i64,
    body: &Value,
    reason: &Option<String>,
    actor: i64,
    check: bool,
) -> HttpResponse {
    if !s.roles.contains_key(&id) {
        return refuse(404, 10011, "Unknown Role");
    }
    if check {
        if id != s.guild_id && s.pos_of(id) >= s.bot_top() {
            return missing_perms();
        }
        if body.get("permissions").is_some() && !s.may_grant(word(body, "permissions")) {
            return missing_perms();
        }
    }
    let Some(r) = s.roles.get_mut(&id) else {
        return refuse(404, 10011, "Unknown Role");
    };
    if let Some(o) = body.as_object() {
        for (k, v) in o {
            if matches!(
                k.as_str(),
                "name" | "permissions" | "color" | "hoist" | "mentionable" | "unicode_emoji"
            ) {
                r[k] = v.clone();
            }
        }
    }
    let role = r.clone();
    let g = s.guild_id.to_string();
    s.emit("GUILD_ROLE_UPDATE", json!({"guild_id": g, "role": role}));
    s.audit_entry("ROLE_UPDATE", id, reason, actor);
    reply(200, vec![], role)
}

fn create_channel(
    s: &mut Inner,
    body: &Value,
    reason: &Option<String>,
    actor: i64,
    check: bool,
) -> HttpResponse {
    let kind = body["type"].as_i64().unwrap_or(0);
    if s.channels.len() >= CAP_CHANNELS {
        return refuse(
            400,
            30005,
            "Maximum number of server channels reached (500)",
        );
    }
    if Inner::community_only(kind) && !s.community {
        return refuse(400, 50024, "Cannot execute action on this channel type");
    }
    if body["name"].as_str().is_none_or(str::is_empty) {
        return refuse(400, 50035, "Invalid Form Body");
    }
    if check && !s.overwrites_grant_ok(&body["permission_overwrites"]) {
        return missing_perms();
    }
    let id = s.fresh();
    let mut v = Map::new();
    v.insert("id".into(), json!(id.to_string()));
    v.insert("guild_id".into(), json!(s.guild_id.to_string()));
    v.insert("type".into(), json!(kind));
    v.insert(
        "position".into(),
        body.get("position")
            .cloned()
            .unwrap_or(json!(s.channels.len())),
    );
    v.insert(
        "permission_overwrites".into(),
        body.get("permission_overwrites")
            .cloned()
            .unwrap_or(json!([])),
    );
    for k in [
        "name",
        "topic",
        "parent_id",
        "nsfw",
        "rate_limit_per_user",
        "bitrate",
        "user_limit",
        "video_quality_mode",
        "available_tags",
        "default_reaction_emoji",
        "default_sort_order",
        "default_forum_layout",
        "default_auto_archive_duration",
        "default_thread_rate_limit_per_user",
    ] {
        if let Some(x) = body.get(k) {
            v.insert(k.into(), x.clone());
        }
    }
    if let Some(b) = v.get("bitrate").and_then(Value::as_i64) {
        let cap = match s.premium_tier {
            0 => 96_000,
            1 => 128_000,
            2 => 256_000,
            _ => 384_000,
        };
        v.insert("bitrate".into(), json!(b.min(cap)));
    }
    if let Some(tags) = v.get_mut("available_tags").and_then(Value::as_array_mut) {
        let mut next = Vec::new();
        for t in tags.iter() {
            let mut t = t.clone();
            if t.get("id").is_none_or(Value::is_null) {
                s.next_id += 1;
                t["id"] = json!(s.next_id.to_string());
            }
            next.push(t);
        }
        *tags = next;
    }
    let ch = Value::Object(v);
    s.channels.insert(id, ch.clone());
    s.emit("CHANNEL_CREATE", ch.clone());
    s.audit_entry("CHANNEL_CREATE", id, reason, actor);
    reply(200, vec![], ch)
}

fn edit_channel(
    s: &mut Inner,
    id: i64,
    body: &Value,
    reason: &Option<String>,
    actor: i64,
    check: bool,
) -> HttpResponse {
    let is_thread = matches!(s.channels[&id]["type"].as_i64(), Some(10..=12));
    if check && !s.overwrites_grant_ok(&body["permission_overwrites"]) {
        return missing_perms();
    }
    // Two name or topic changes per channel per ten minutes.
    let renames = ["name", "topic"]
        .iter()
        .any(|k| body.get(*k).is_some_and(|v| *v != s.channels[&id][*k]));
    if renames && !is_thread {
        let n = s.renames.entry(id).or_insert(0);
        if *n >= 2 {
            return reply(
                429,
                vec![("Retry-After", "300".into())],
                json!({"message": "You are being rate limited.", "retry_after": 300.0, "global": false}),
            );
        }
        *n += 1;
    }
    let Some(c) = s.channels.get_mut(&id) else {
        return refuse(404, 10003, "Unknown Channel");
    };
    if let Some(o) = body.as_object() {
        for (k, v) in o {
            match k.as_str() {
                "archived" | "locked" | "auto_archive_duration" | "invitable" => {
                    c["thread_metadata"][k] = v.clone();
                }
                "type" => {}
                _ => c[k] = v.clone(),
            }
        }
    }
    let mut cj = c.clone();
    if let Some(tags) = cj.get_mut("available_tags").and_then(Value::as_array_mut) {
        for t in tags.iter_mut() {
            if t.get("id").is_none_or(Value::is_null) {
                s.next_id += 1;
                t["id"] = json!(s.next_id.to_string());
            }
        }
    }
    s.channels.insert(id, cj.clone());
    s.emit(
        if is_thread {
            "THREAD_UPDATE"
        } else {
            "CHANNEL_UPDATE"
        },
        cj.clone(),
    );
    s.audit_entry("CHANNEL_UPDATE", id, reason, actor);
    reply(200, vec![], cj)
}

fn create_thread(
    s: &mut Inner,
    parent: i64,
    body: &Value,
    reason: &Option<String>,
    actor: i64,
) -> HttpResponse {
    let kind = body["type"].as_i64().unwrap_or(11);
    let id = s.fresh();
    let t = json!({"id": id.to_string(), "guild_id": s.guild_id.to_string(), "type": kind,
        "name": body["name"], "parent_id": parent.to_string(), "owner_id": actor.to_string(),
        "applied_tags": body.get("applied_tags").cloned().unwrap_or(json!([])),
        "thread_metadata": {"archived": false, "locked": false,
            "auto_archive_duration": body["auto_archive_duration"].as_i64().unwrap_or(1440),
            "invitable": body["invitable"].as_bool().unwrap_or(true)}});
    s.channels.insert(id, t.clone());
    s.emit("THREAD_CREATE", t.clone());
    s.audit_entry("THREAD_CREATE", id, reason, actor);
    reply(201, vec![], t)
}
