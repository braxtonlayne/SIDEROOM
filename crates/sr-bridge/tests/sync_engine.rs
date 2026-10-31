//! The sync engine against a stateful in-memory Discord guild and a SQL-backed SIDE ROOM port. Run with a
//! scratch Postgres: `SR_TEST_DATABASE_URL=... cargo test -p sr-bridge --test sync_engine -- --include-ignored --test-threads=1`
//!
//! The fake here is sync-core's minimal one (roles, channels, threads, positions); sync-gateway's richer fake
//! replaces it when the branches meet.

#[path = "support/mod.rs"]
mod support;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{Value, json};
use sr_bridge::sync::{
    Action, BridgeOrigin, ItemKind, PlanEdit, SrNewChannel, SrNewRole, SrStructuralPatch,
    SrStructure, SyncEngine, SyncMode, SyncState, holds, store,
};
use sr_bridge::{BridgeError, BridgeResult, DiscordApi};
use sr_core::permissions::Permissions;
use sr_sdk::http::{HttpRequest, HttpResponse, HttpTransport, Method};
use support::*;

const GUILD_BASE: &str = "http://fake.discord/api/v10";

// ---- the Discord guild ----

struct GuildState {
    guild_id: i64,
    bot_user: i64,
    next: i64,
    roles: BTreeMap<i64, Value>,
    channels: BTreeMap<i64, Value>,
    threads: BTreeMap<i64, Value>,
    log: Vec<(String, String, Value)>,
    features: Vec<String>,
}

struct GuildFake(Mutex<GuildState>);

fn reply(status: u16, body: Value) -> HttpResponse {
    HttpResponse {
        status,
        headers: vec![],
        body: if body.is_null() {
            vec![]
        } else {
            body.to_string().into_bytes()
        },
    }
}

impl GuildFake {
    fn new(guild: i64, bot_user: i64, admin: bool) -> Arc<Self> {
        let mut roles = BTreeMap::new();
        roles.insert(
            guild,
            json!({"id": guild.to_string(), "name": "@everyone", "color": 0, "hoist": false, "position": 0,
                   "permissions": "1024", "managed": false, "mentionable": false}),
        );
        // The bot's own managed role, on top.
        roles.insert(
            5001,
            json!({"id": "5001", "name": "bridgebot", "color": 0, "hoist": false, "position": 100,
                   "permissions": if admin { "8" } else { "0" }, "managed": true, "mentionable": false,
                   "tags": {"bot_id": bot_user.to_string()}}),
        );
        Arc::new(Self(Mutex::new(GuildState {
            guild_id: guild,
            bot_user,
            next: 800_000,
            roles,
            channels: BTreeMap::new(),
            threads: BTreeMap::new(),
            log: vec![],
            features: vec!["COMMUNITY".into()],
        })))
    }

    fn add_role(&self, id: i64, name: &str, position: i64, perms: u64) {
        self.0.lock().unwrap().roles.insert(
            id,
            json!({"id": id.to_string(), "name": name, "color": 0, "hoist": false, "position": position,
                   "permissions": perms.to_string(), "managed": false, "mentionable": false}),
        );
    }

    fn add_channel(&self, id: i64, kind: i64, name: &str, parent: Option<i64>, position: i64) {
        self.0.lock().unwrap().channels.insert(
            id,
            json!({"id": id.to_string(), "type": kind, "name": name, "position": position,
                   "parent_id": parent.map(|p| p.to_string()), "permission_overwrites": []}),
        );
    }

    fn writes(&self) -> Vec<(String, String, Value)> {
        self.0
            .lock()
            .unwrap()
            .log
            .iter()
            .filter(|(m, _, _)| m != "GET")
            .cloned()
            .collect()
    }

    fn names(&self, which: &str) -> Vec<String> {
        let s = self.0.lock().unwrap();
        let map = if which == "roles" {
            &s.roles
        } else {
            &s.channels
        };
        map.values()
            .filter_map(|v| v["name"].as_str().map(str::to_owned))
            .collect()
    }

    fn role_perms(&self, name: &str) -> Option<String> {
        let s = self.0.lock().unwrap();
        s.roles
            .values()
            .find(|v| v["name"] == name)
            .map(|v| v["permissions"].as_str().unwrap().to_owned())
    }

    fn has_channel(&self, id: i64) -> bool {
        self.0.lock().unwrap().channels.contains_key(&id)
    }
}

#[async_trait]
impl HttpTransport for GuildFake {
    async fn send(&self, req: HttpRequest) -> sr_sdk::Result<HttpResponse> {
        let rel = req
            .url
            .strip_prefix(GUILD_BASE)
            .unwrap_or(&req.url)
            .to_owned();
        let path = rel.split('?').next().unwrap_or("").to_owned();
        let m = match req.method {
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
        let mut s = self.0.lock().unwrap();
        s.log.push((m.to_owned(), path.clone(), body.clone()));
        let g = s.guild_id.to_string();
        let segs: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        let resp = match (m, segs.as_slice()) {
            ("GET", ["guilds", id]) if *id == g => reply(
                200,
                json!({"id": g, "name": "Fake", "premium_tier": 1, "features": s.features}),
            ),
            ("GET", ["guilds", id, "members", _]) if *id == g => reply(
                200,
                json!({"user": {"id": s.bot_user.to_string()}, "roles": ["5001"]}),
            ),
            ("GET", ["guilds", id, "roles"]) if *id == g => {
                reply(200, Value::Array(s.roles.values().cloned().collect()))
            }
            ("POST", ["guilds", id, "roles"]) if *id == g => {
                s.next += 1;
                let rid = s.next;
                let mut v = json!({"id": rid.to_string(), "name": "new role", "color": 0, "hoist": false,
                                   "position": 1, "permissions": "0", "managed": false, "mentionable": false});
                for k in [
                    "name",
                    "color",
                    "hoist",
                    "mentionable",
                    "permissions",
                    "unicode_emoji",
                ] {
                    if let Some(x) = body.get(k) {
                        v[k] = x.clone();
                    }
                }
                s.roles.insert(rid, v.clone());
                reply(200, v)
            }
            ("PATCH", ["guilds", id, "roles"]) if *id == g => {
                for it in body.as_array().cloned().unwrap_or_default() {
                    let rid: i64 = it["id"].as_str().unwrap().parse().unwrap();
                    if let Some(r) = s.roles.get_mut(&rid) {
                        r["position"] = it["position"].clone();
                    }
                }
                reply(200, Value::Array(s.roles.values().cloned().collect()))
            }
            ("PATCH", ["guilds", id, "roles", rid]) if *id == g => {
                let rid: i64 = rid.parse().unwrap();
                match s.roles.get_mut(&rid) {
                    Some(r) => {
                        for (k, x) in body.as_object().cloned().unwrap_or_default() {
                            r[k.as_str()] = x;
                        }
                        reply(200, r.clone())
                    }
                    None => reply(404, json!({"message": "Unknown Role"})),
                }
            }
            ("DELETE", ["guilds", id, "roles", rid]) if *id == g => {
                let rid: i64 = rid.parse().unwrap();
                s.roles.remove(&rid);
                reply(204, Value::Null)
            }
            ("GET", ["guilds", id, "channels"]) if *id == g => {
                reply(200, Value::Array(s.channels.values().cloned().collect()))
            }
            ("POST", ["guilds", id, "channels"]) if *id == g => {
                s.next += 1;
                let cid = s.next;
                let kind = body["type"].as_i64().unwrap_or(0);
                if matches!(kind, 5 | 13 | 15 | 16) && !s.features.iter().any(|f| f == "COMMUNITY")
                {
                    return Ok(reply(400, json!({"message": "Community required"})));
                }
                let mut v = json!({"id": cid.to_string(), "type": kind, "name": body["name"], "position": 0,
                                   "parent_id": body.get("parent_id").cloned().unwrap_or(Value::Null),
                                   "permission_overwrites": []});
                for k in [
                    "topic",
                    "nsfw",
                    "rate_limit_per_user",
                    "bitrate",
                    "user_limit",
                ] {
                    if let Some(x) = body.get(k) {
                        v[k] = x.clone();
                    }
                }
                s.channels.insert(cid, v.clone());
                reply(200, v)
            }
            ("PATCH", ["guilds", id, "channels"]) if *id == g => {
                for it in body.as_array().cloned().unwrap_or_default() {
                    let cid: i64 = it["id"].as_str().unwrap().parse().unwrap();
                    if let Some(c) = s.channels.get_mut(&cid) {
                        c["position"] = it["position"].clone();
                        if let Some(p) = it.get("parent_id") {
                            c["parent_id"] = p.clone();
                        }
                    }
                }
                reply(204, Value::Null)
            }
            ("GET", ["guilds", id, "threads", "active"]) if *id == g => reply(
                200,
                json!({"threads": s.threads.values().cloned().collect::<Vec<_>>()}),
            ),
            ("POST", ["channels", pid, "threads"]) => {
                s.next += 1;
                let tid = s.next;
                let v = json!({"id": tid.to_string(), "type": body.get("type").cloned().unwrap_or(json!(11)),
                               "name": body["name"], "parent_id": pid, "position": 0,
                               "thread_metadata": {"archived": false, "locked": false, "auto_archive_duration": 1440}});
                s.threads.insert(tid, v.clone());
                reply(200, v)
            }
            ("PATCH", ["channels", cid]) => {
                let cid: i64 = cid.parse().unwrap();
                match s.channels.get_mut(&cid) {
                    Some(c) => {
                        for (k, x) in body.as_object().cloned().unwrap_or_default() {
                            c[k.as_str()] = x;
                        }
                        reply(200, c.clone())
                    }
                    None => reply(404, json!({"message": "Unknown Channel"})),
                }
            }
            ("DELETE", ["channels", cid]) => {
                let cid: i64 = cid.parse().unwrap();
                s.channels.remove(&cid);
                s.threads.remove(&cid);
                reply(200, json!({}))
            }
            _ => reply(404, json!({"message": "Not Found"})),
        };
        Ok(resp)
    }
}

// ---- the SIDE ROOM port, over SQL ----

#[derive(Default)]
struct SrLog {
    created_roles: Vec<String>,
    created_channels: Vec<String>,
    imported: Vec<(i64, u64)>,
    applied: Vec<SrStructuralPatch>,
    deleted: Vec<i64>,
    origins: Vec<BridgeOrigin>,
}

struct SqlSr {
    pool: sr_db::PgPool,
    log: Mutex<SrLog>,
}

#[async_trait]
impl SrStructure for SqlSr {
    async fn create_role(
        &self,
        guild: i64,
        new: SrNewRole,
        origin: &BridgeOrigin,
    ) -> BridgeResult<i64> {
        let id = uid();
        sqlx::query("INSERT INTO roles (id, guild_id, name, position, permissions, hoist) VALUES ($1, $2, $3, 1, 0, $4)")
            .bind(id).bind(guild).bind(&new.name).bind(new.hoist).execute(&self.pool).await?;
        let mut l = self.log.lock().unwrap();
        l.created_roles.push(new.name);
        l.origins.push(origin.clone());
        Ok(id)
    }

    async fn create_channel(
        &self,
        guild: i64,
        new: SrNewChannel,
        origin: &BridgeOrigin,
    ) -> BridgeResult<i64> {
        let id = uid();
        sqlx::query("INSERT INTO channels (id, guild_id, type, name, position, parent_id) VALUES ($1, $2, $3, $4, $5, $6)")
            .bind(id).bind(guild).bind(new.kind).bind(&new.name).bind(new.position).bind(new.parent_id)
            .execute(&self.pool).await?;
        let mut l = self.log.lock().unwrap();
        l.created_channels.push(new.name);
        l.origins.push(origin.clone());
        Ok(id)
    }

    async fn apply(
        &self,
        _guild: i64,
        patch: &SrStructuralPatch,
        origin: &BridgeOrigin,
    ) -> BridgeResult<()> {
        if let SrStructuralPatch::Channel {
            channel_id,
            patch: p,
        } = patch
        {
            if let Some(n) = &p.name {
                sqlx::query("UPDATE channels SET name = $2 WHERE id = $1")
                    .bind(channel_id)
                    .bind(n)
                    .execute(&self.pool)
                    .await?;
            }
            if let Some(pos) = p.position {
                sqlx::query("UPDATE channels SET position = $2 WHERE id = $1")
                    .bind(channel_id)
                    .bind(pos)
                    .execute(&self.pool)
                    .await?;
            }
        }
        let mut l = self.log.lock().unwrap();
        l.applied.push(patch.clone());
        l.origins.push(origin.clone());
        Ok(())
    }

    async fn delete_role(&self, _g: i64, role: i64, _o: &BridgeOrigin) -> BridgeResult<()> {
        sqlx::query("DELETE FROM roles WHERE id = $1")
            .bind(role)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn delete_channel(&self, _g: i64, channel: i64, _o: &BridgeOrigin) -> BridgeResult<()> {
        sqlx::query("DELETE FROM channels WHERE id = $1")
            .bind(channel)
            .execute(&self.pool)
            .await?;
        self.log.lock().unwrap().deleted.push(channel);
        Ok(())
    }

    async fn import_role_permissions(
        &self,
        _g: i64,
        role: i64,
        shared: Permissions,
        origin: &BridgeOrigin,
    ) -> BridgeResult<()> {
        assert!(
            origin.sr_actor.is_some(),
            "an import is always the owner's act"
        );
        sqlx::query("UPDATE roles SET permissions = permissions | $2 WHERE id = $1")
            .bind(role)
            .bind(shared.bits() as i64)
            .execute(&self.pool)
            .await?;
        self.log
            .lock()
            .unwrap()
            .imported
            .push((role, shared.bits()));
        Ok(())
    }

    async fn role_has_denies(&self, _g: i64, _r: i64) -> BridgeResult<bool> {
        Ok(false)
    }
}

// ---- the scene ----

struct Scene {
    w: World,
    eng: SyncEngine,
    fake: Arc<GuildFake>,
    sr: Arc<SqlSr>,
    guild: i64,
    owner: i64,
    dg: i64,
    /// SIDE ROOM: role Mods, role Admins (with a SIDE ROOM-only bit), category, two channels.
    ids: BTreeMap<&'static str, i64>,
}

async fn scene(admin: bool) -> Scene {
    let w = world().await;
    let b = w.bridged().await;
    let pool = w.db.pool().clone();
    let fake = GuildFake::new(b.discord_guild, 500, admin);
    let discord = DiscordApi::with_transport(BASE, fake.clone());
    discord.set_struct_writes_per_sec(1000.0).await;
    let mut svc = w.svc.clone();
    svc.discord = discord;
    let sr = Arc::new(SqlSr {
        pool: pool.clone(),
        log: Mutex::new(SrLog::default()),
    });
    let eng = SyncEngine::new(svc, sr.clone());
    // Align the stored bot user with the fake's, and grant the intents the preflight wants.
    sqlx::query("UPDATE bridges SET bot_user_id = 500, intents_granted = $2 WHERE guild_id = $1")
        .bind(b.guild)
        .bind((1i64 << 1) | (1 << 15))
        .execute(&pool)
        .await
        .unwrap();
    // The wave-35 map that `bridged()` made points at a channel the new fake does not have; drop it.
    sqlx::query("DELETE FROM bridge_channel_maps WHERE guild_id = $1")
        .bind(b.guild)
        .execute(&pool)
        .await
        .unwrap();

    let mut ids = BTreeMap::new();
    for (key, name, pos, perms) in [
        ("mods", "Mods", 2i32, 0x2i64 | (1 << 56)),
        ("admins", "Admins", 3, 0x8),
    ] {
        let id = uid();
        sqlx::query("INSERT INTO roles (id, guild_id, name, position, permissions) VALUES ($1, $2, $3, $4, $5)")
            .bind(id).bind(b.guild).bind(name).bind(pos).bind(perms).execute(&pool).await.unwrap();
        ids.insert(key, id);
    }
    // @everyone has id = guild id.
    sqlx::query("INSERT INTO roles (id, guild_id, name, position, permissions) VALUES ($1, $1, '@everyone', 0, 1024) ON CONFLICT DO NOTHING")
        .bind(b.guild).execute(&pool).await.unwrap();
    // `guild()` made a "general" channel; make it a child of a category.
    let cat = channel_of(&pool, b.guild, "Text Rooms", 4).await;
    ids.insert("cat", cat);
    sqlx::query("UPDATE channels SET parent_id = $2, position = 0 WHERE id = $1")
        .bind(b.channel)
        .bind(cat)
        .execute(&pool)
        .await
        .unwrap();
    ids.insert("general", b.channel);
    let lonely = channel(&pool, b.guild, "Sr Only").await;
    ids.insert("sr_only", lonely);

    // Discord already has a category with the same name, a matching channel, and a role Mods.
    fake.add_role(9001, "mods", 2, 0x2 | 0x4);
    fake.add_role(9002, "Only There", 1, 0x8 | (1 << 47));
    fake.add_channel(9100, 4, "text rooms", None, 0);
    fake.add_channel(9101, 0, "general", Some(9100), 0);
    fake.add_channel(9102, 0, "there-only", Some(9100), 1);
    Scene {
        w,
        eng,
        fake,
        sr,
        guild: b.guild,
        owner: b.owner,
        dg: b.discord_guild,
        ids,
    }
}

async fn ready_plan(s: &Scene) -> (i64, String) {
    let id = s.eng.create_plan(s.guild, s.owner, "sr").await.unwrap();
    let v = s.eng.get_plan(s.guild, id).await.unwrap();
    (id, v.plan_hash)
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_plan_is_a_dry_run_that_matches_and_writes_nothing() {
    let s = scene(true).await;
    let (id, _hash) = ready_plan(&s).await;
    let v = s.eng.get_plan(s.guild, id).await.unwrap();
    assert_eq!(v.status, "ready");
    assert!(v.blockers.is_empty(), "{:?}", v.blockers);
    let item = |name: &str| {
        v.plan
            .items
            .iter()
            .find(|i| i.name == name)
            .unwrap_or_else(|| panic!("{name}"))
    };
    assert_eq!(item("Mods").action, Action::Match);
    assert_eq!(
        item("Mods").sr_permissions,
        0x2,
        "the SIDE ROOM-only bit never shows as something Discord gets"
    );
    assert_eq!(item("Admins").action, Action::CreateOnDiscord);
    assert_eq!(item("Only There").action, Action::CreateHere);
    assert_eq!(item("Text Rooms").action, Action::Match);
    assert_eq!(item("general").action, Action::Match);
    assert_eq!(item("Sr Only").action, Action::CreateOnDiscord);
    assert!(s.fake.writes().is_empty(), "planning only reads");
    let sync = store::get_sync(s.w.db.pool(), s.guild)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(sync.mode, SyncMode::Selective);
    // A newer plan supersedes the open one.
    let id2 = s.eng.create_plan(s.guild, s.owner, "sr").await.unwrap();
    assert_eq!(
        s.eng.get_plan(s.guild, id).await.unwrap().status,
        "superseded"
    );
    assert_eq!(s.eng.get_plan(s.guild, id2).await.unwrap().status, "ready");
    // Another server cannot read it.
    assert!(matches!(
        s.eng.get_plan(s.guild + 1, id2).await,
        Err(BridgeError::NotFound(_))
    ));
    let _ = s.w.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_bot_without_administrator_blocks_confirmation() {
    let s = scene(false).await;
    let (id, hash) = ready_plan(&s).await;
    let v = s.eng.get_plan(s.guild, id).await.unwrap();
    assert!(v.blockers.iter().any(|b| b.code == "bot_not_admin"));
    assert!(matches!(
        s.eng.confirm_mode(s.guild, s.owner, id, &hash).await,
        Err(BridgeError::Conflict(_))
    ));
    let _ = s.w.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn confirming_needs_the_current_hash_and_a_fresh_state() {
    let s = scene(true).await;
    let (id, hash) = ready_plan(&s).await;
    assert!(matches!(
        s.eng.confirm_mode(s.guild, s.owner, id, "nope").await,
        Err(BridgeError::PlanStale)
    ));
    // Discord changes after the plan was built.
    s.fake.add_channel(9103, 0, "surprise", Some(9100), 2);
    assert!(matches!(
        s.eng.confirm_mode(s.guild, s.owner, id, &hash).await,
        Err(BridgeError::PlanStale)
    ));
    assert_eq!(s.eng.get_plan(s.guild, id).await.unwrap().status, "expired");
    let sync = store::get_sync(s.w.db.pool(), s.guild)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (sync.mode, sync.state),
        (SyncMode::Selective, SyncState::Off)
    );
    let _ = s.w.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn plan_edits_change_the_hash_that_must_be_confirmed() {
    let s = scene(true).await;
    let (id, hash) = ready_plan(&s).await;
    let v = s.eng.get_plan(s.guild, id).await.unwrap();
    let up = v
        .plan
        .items
        .iter()
        .position(|i| i.name == "Only There")
        .unwrap();
    let v2 = s
        .eng
        .edit_plan(
            s.guild,
            id,
            PlanEdit::Import {
                index: up,
                import: false,
            },
        )
        .await
        .unwrap();
    assert_ne!(v2.plan_hash, hash);
    assert!(matches!(
        s.eng.confirm_mode(s.guild, s.owner, id, &hash).await,
        Err(BridgeError::PlanStale)
    ));
    let _ = s.w.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_seed_builds_both_sides_and_never_sends_a_sideroom_only_bit() {
    let s = scene(true).await;
    let (id, hash) = ready_plan(&s).await;
    s.eng
        .confirm_mode(s.guild, s.owner, id, &hash)
        .await
        .unwrap();
    let sync = store::get_sync(s.w.db.pool(), s.guild)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (sync.mode, sync.state),
        (SyncMode::WholeGuild, SyncState::Seeding)
    );
    assert!(
        s.fake.writes().is_empty(),
        "confirming alone writes nothing to Discord"
    );

    let report = s.eng.run_seed(s.guild).await.unwrap();
    assert_eq!(report.roles_created_on_discord, 1, "Admins");
    assert_eq!(report.roles_created_here, 1, "Only There");
    assert_eq!(report.channels_created_on_discord, 1, "Sr Only");
    assert_eq!(report.channels_created_here, 1, "there-only");
    assert!(report.roles_matched >= 2, "@everyone and Mods");

    // Discord has SIDE ROOM's objects.
    assert!(s.fake.names("roles").contains(&"Admins".to_owned()));
    assert!(s.fake.names("channels").contains(&"sr-only".to_owned()));
    // SIDE ROOM has Discord's.
    let here: Vec<String> = sqlx::query_scalar("SELECT name FROM roles WHERE guild_id = $1")
        .bind(s.guild)
        .fetch_all(s.w.db.pool())
        .await
        .unwrap();
    assert!(here.contains(&"Only There".to_owned()));
    let chans: Vec<Option<String>> =
        sqlx::query_scalar("SELECT name FROM channels WHERE guild_id = $1")
            .bind(s.guild)
            .fetch_all(s.w.db.pool())
            .await
            .unwrap();
    assert!(chans.contains(&Some("there-only".to_owned())));

    // SIDE ROOM's permissions are the master copy: Mods on Discord is now exactly SIDE ROOM's shared word.
    assert_eq!(s.fake.role_perms("mods").as_deref(), Some("2"));
    // The imported role got Discord's shared bits (the owner confirmed), not its Discord-only bit.
    let (_, bits) = report.imported[0];
    assert_eq!(bits, 0x8);
    let perms: i64 = sqlx::query_scalar(
        "SELECT permissions FROM roles WHERE name = 'Only There' AND guild_id = $1",
    )
    .bind(s.guild)
    .fetch_one(s.w.db.pool())
    .await
    .unwrap();
    assert_eq!(perms, 0x8);
    // ... and on Discord its Discord-only bit 47 is preserved by the push.
    assert!(s.fake.role_perms("Only There").is_some());

    // No request body ever carried a SIDE ROOM-only bit (56 to 62).
    for (_, path, body) in s.fake.writes() {
        if let Some(p) = body.get("permissions").and_then(Value::as_str) {
            let word: u64 = p.parse().unwrap();
            assert_eq!(
                word & sr_core::permissions::SR_RESERVED_RAW,
                0,
                "{path} sent {word:#x}"
            );
        }
        if let Some(arr) = body.as_array() {
            assert!(arr.iter().all(|x| x.get("permissions").is_none()));
        }
    }

    // Maps and state.
    let roles = store::list_role_maps(s.w.db.pool(), s.guild).await.unwrap();
    assert!(roles.iter().any(|m| m.origin == store::Origin::Sr));
    assert!(
        roles
            .iter()
            .any(|m| m.origin == store::Origin::Discord && m.discord_only_bits == 1 << 47)
    );
    assert!(roles.iter().any(|m| m.is_everyone));
    let maps = store::list_structure_maps(s.w.db.pool(), s.guild)
        .await
        .unwrap();
    assert!(
        maps.iter()
            .any(|m| m.kind == "category" && m.origin == store::Origin::Matched)
    );
    assert!(maps.iter().all(|m| !m.selected));
    let sync = store::get_sync(s.w.db.pool(), s.guild)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(sync.state, SyncState::Live);
    // The first Discord snapshot was taken before any write.
    assert!(
        !holds::load_snapshot(s.w.db.pool(), s.guild, "role")
            .await
            .unwrap()
            .is_empty()
    );
    // Audit rows: the mode update and the seed run.
    let actions: Vec<i16> = sqlx::query_scalar("SELECT action FROM audit_log_entries WHERE guild_id = $1 AND action IN (220, 221) ORDER BY id")
        .bind(s.guild).fetch_all(s.w.db.pool()).await.unwrap();
    assert_eq!(actions, vec![220, 221]);
    // Every SIDE ROOM change the seed made carries the origin of the run.
    assert!(
        s.sr.log
            .lock()
            .unwrap()
            .origins
            .iter()
            .all(|o| o.op_id != 0)
    );
    let _ = s.w.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_second_seed_run_creates_nothing_twice() {
    let s = scene(true).await;
    let (id, hash) = ready_plan(&s).await;
    s.eng
        .confirm_mode(s.guild, s.owner, id, &hash)
        .await
        .unwrap();
    s.eng.run_seed(s.guild).await.unwrap();
    let discord_roles = s.fake.names("roles").len();
    let discord_channels = s.fake.names("channels").len();
    let here_roles = s.sr.log.lock().unwrap().created_roles.len();
    // A crash before `live` leaves the state at seeding; the restart runs the seed again.
    store::set_sync_state(s.w.db.pool(), s.guild, SyncState::Seeding)
        .await
        .unwrap();
    let again = s.eng.run_seed(s.guild).await.unwrap();
    assert_eq!(
        again.roles_created_on_discord
            + again.roles_created_here
            + again.channels_created_on_discord
            + again.channels_created_here,
        0
    );
    assert_eq!(s.fake.names("roles").len(), discord_roles);
    assert_eq!(s.fake.names("channels").len(), discord_channels);
    assert_eq!(s.sr.log.lock().unwrap().created_roles.len(), here_roles);
    // A seed with nothing to run is refused once the bridge is live.
    assert!(matches!(
        s.eng.run_seed(s.guild).await,
        Err(BridgeError::Conflict(_))
    ));
    let _ = s.w.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn without_community_the_types_degrade_and_say_why() {
    let s = scene(true).await;
    s.fake.0.lock().unwrap().features.clear();
    let ann = channel_of(s.w.db.pool(), s.guild, "News", 5).await;
    let (id, hash) = ready_plan(&s).await;
    s.eng
        .confirm_mode(s.guild, s.owner, id, &hash)
        .await
        .unwrap();
    let r = s.eng.run_seed(s.guild).await.unwrap();
    assert_eq!(r.degraded, 1);
    let m = store::structure_map_for_channel(s.w.db.pool(), ann)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(m.sync_status, "degraded");
    assert!(m.degraded_reason.unwrap().contains("Community"));
    let _ = s.w.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn back_to_selective_deletes_nothing_anywhere() {
    let s = scene(true).await;
    let (id, hash) = ready_plan(&s).await;
    s.eng
        .confirm_mode(s.guild, s.owner, id, &hash)
        .await
        .unwrap();
    s.eng.run_seed(s.guild).await.unwrap();
    let (dr, dc) = (s.fake.names("roles").len(), s.fake.names("channels").len());
    let sr_channels: i64 = sqlx::query_scalar("SELECT count(*) FROM channels WHERE guild_id = $1")
        .bind(s.guild)
        .fetch_one(s.w.db.pool())
        .await
        .unwrap();
    let writes_before = s.fake.writes().len();
    s.eng.switch_to_selective(s.guild, s.owner).await.unwrap();
    assert_eq!(s.fake.writes().len(), writes_before, "no Discord call");
    assert_eq!(
        (s.fake.names("roles").len(), s.fake.names("channels").len()),
        (dr, dc)
    );
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM channels WHERE guild_id = $1")
        .bind(s.guild)
        .fetch_one(s.w.db.pool())
        .await
        .unwrap();
    assert_eq!(after, sr_channels);
    assert!(
        store::list_role_maps(s.w.db.pool(), s.guild)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        holds::load_snapshot(s.w.db.pool(), s.guild, "role")
            .await
            .unwrap()
            .is_empty()
    );
    let maps = store::list_structure_maps(s.w.db.pool(), s.guild)
        .await
        .unwrap();
    assert!(!maps.is_empty());
    assert!(maps.iter().any(|m| m.selected && m.parent_map_id.is_none()));
    assert!(
        maps.iter()
            .filter(|m| m.parent_map_id.is_some())
            .all(|m| !m.selected)
    );
    let sync = store::get_sync(s.w.db.pool(), s.guild)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (sync.mode, sync.state),
        (SyncMode::Selective, SyncState::Off)
    );
    assert!(matches!(
        s.eng.switch_to_selective(s.guild, s.owner).await,
        Err(BridgeError::Conflict(_))
    ));
    let _ = s.w.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_channel_deleted_on_discord_is_held_then_deleted_or_restored() {
    let s = scene(true).await;
    let (id, hash) = ready_plan(&s).await;
    s.eng
        .confirm_mode(s.guild, s.owner, id, &hash)
        .await
        .unwrap();
    s.eng.run_seed(s.guild).await.unwrap();
    let pool = s.w.db.pool();

    // Delete on Discord: held 24 hours, hidden at once, nothing deleted here.
    let general = s.ids["general"];
    let m = store::structure_map_for_channel(pool, general)
        .await
        .unwrap()
        .unwrap();
    assert!(
        s.eng
            .hold_deleted_on_discord(s.guild, m.discord_channel_id, Some(77), 24)
            .await
            .unwrap()
    );
    assert!(holds::is_held(pool, general).await.unwrap());
    assert_eq!(s.eng.run_due_holds().await.unwrap(), 0);
    assert!(s.sr.log.lock().unwrap().deleted.is_empty());
    let snap = s.eng.read_sr(s.guild).await.unwrap();
    assert!(
        snap.channels.iter().all(|c| c.id != general),
        "a held channel is out of the SIDE ROOM snapshot"
    );

    // Restore: the Discord side is recreated from SIDE ROOM's state and the map follows it.
    let new_id = s.eng.restore_held(s.guild, general).await.unwrap();
    assert!(s.fake.has_channel(new_id));
    assert!(!holds::is_held(pool, general).await.unwrap());
    let m2 = store::structure_map_for_channel(pool, general)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (m2.discord_channel_id, m2.sync_status.as_str()),
        (new_id, "mapped")
    );

    // Another channel deleted on Discord with a zero hold is deleted by the job, through the port.
    let lonely = s.ids["sr_only"];
    let lm = store::structure_map_for_channel(pool, lonely)
        .await
        .unwrap()
        .unwrap();
    s.eng
        .hold_deleted_on_discord(s.guild, lm.discord_channel_id, None, 0)
        .await
        .unwrap();
    assert_eq!(s.eng.run_due_holds().await.unwrap(), 1);
    assert_eq!(s.sr.log.lock().unwrap().deleted, vec![lonely]);
    // Unmapped Discord channels are ignored.
    assert!(
        !s.eng
            .hold_deleted_on_discord(s.guild, 123, None, 0)
            .await
            .unwrap()
    );
    let _ = (s.dg, ItemKind::Role);
    let _ = s.w.db.cleanup().await;
}

async fn seeded() -> Scene {
    let s = scene(true).await;
    let (id, hash) = ready_plan(&s).await;
    s.eng
        .confirm_mode(s.guild, s.owner, id, &hash)
        .await
        .unwrap();
    s.eng.run_seed(s.guild).await.unwrap();
    s
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_default_teardown_leaves_discord_as_it_is() {
    let s = seeded().await;
    let (roles, chans) = (s.fake.names("roles"), s.fake.names("channels"));
    let pre = s.eng.svc.teardown_preview(s.guild).await.unwrap();
    assert!(
        pre.whole_guild && pre.created_roles == 1 && pre.created_channels == 1,
        "{pre:?}"
    );
    let rep = s
        .eng
        .svc
        .teardown(s.guild, s.owner, sr_bridge::TeardownReason::Owner)
        .await
        .unwrap();
    assert_eq!(rep.removal.removed(), 0);
    assert_eq!(s.fake.names("roles"), roles);
    assert_eq!(s.fake.names("channels"), chans);
    assert!(
        store::get_sync(s.w.db.pool(), s.guild)
            .await
            .unwrap()
            .is_none(),
        "the bridge row is gone"
    );
    assert!(
        store::list_role_maps(s.w.db.pool(), s.guild)
            .await
            .unwrap()
            .is_empty()
    );
    let _ = s.w.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn also_remove_deletes_only_what_the_bridge_created_there() {
    let s = seeded().await;
    let opts = sr_bridge::TeardownOptions {
        remove_created: true,
        restore_permissions: false,
    };
    let rep = s
        .eng
        .svc
        .teardown_with(s.guild, s.owner, sr_bridge::TeardownReason::Owner, opts)
        .await
        .unwrap();
    assert_eq!(
        (rep.removal.roles, rep.removal.channels),
        (1, 1),
        "{:?}",
        rep.removal
    );
    assert!(rep.removal.failed.is_empty());
    let roles = s.fake.names("roles");
    assert!(
        !roles.contains(&"Admins".to_owned()),
        "the bridge-created role is gone"
    );
    assert!(
        roles.contains(&"mods".to_owned()) && roles.contains(&"Only There".to_owned()),
        "Discord-native roles stay"
    );
    let chans = s.fake.names("channels");
    assert!(!chans.contains(&"sr-only".to_owned()));
    for native in ["text rooms", "general", "there-only"] {
        assert!(chans.contains(&native.to_owned()), "{native} stays");
    }
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log_entries WHERE guild_id = $1 AND action = 224",
    )
    .bind(s.guild)
    .fetch_one(s.w.db.pool())
    .await
    .unwrap();
    assert_eq!(n, 1);
    // Nothing is deleted on the SIDE ROOM side by a teardown.
    let sr_chans: i64 = sqlx::query_scalar("SELECT count(*) FROM channels WHERE guild_id = $1")
        .bind(s.guild)
        .fetch_one(s.w.db.pool())
        .await
        .unwrap();
    assert_eq!(sr_chans, 4, "cat, general, sr only and there-only");
    let _ = s.w.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn restore_puts_discords_permissions_back_to_before_the_bridge() {
    let s = seeded().await;
    assert_eq!(
        s.fake.role_perms("mods").as_deref(),
        Some("2"),
        "the seed pushed SIDE ROOM's word"
    );
    let opts = sr_bridge::TeardownOptions {
        remove_created: false,
        restore_permissions: true,
    };
    let rep = s
        .eng
        .svc
        .teardown_with(s.guild, s.owner, sr_bridge::TeardownReason::Owner, opts)
        .await
        .unwrap();
    assert!(rep.removal.restored_roles >= 1, "{:?}", rep.removal);
    assert_eq!(
        s.fake.role_perms("mods").as_deref(),
        Some("6"),
        "Discord's pre-bridge word is back"
    );
    assert!(
        s.fake.names("roles").contains(&"Admins".to_owned()),
        "restore removes nothing"
    );
    let _ = s.w.db.cleanup().await;
}
