//! The scrapbook routes over the in-memory repositories (PRF-SCR-001): who sees an entry (`VIEW_CHANNEL` of its
//! source channel), who may send, edit and remove, who reads the removal list and chooses the shelf, how a
//! member's storage scopes decide whether a file is signed for them, and that the module is hidden until the
//! operator offers it.

use super::scrapbook::{ScrapbookDeps, router};
use crate::auth::{
    BotIdentity, csrf,
    memory::{MemoryBots, MemorySessions},
};
use crate::routes::messages::{FixedEntitlements, MessagesDeps};
use crate::routes::settings::features::{GuildFeatures, RoleAssigner};
use crate::routes::settings::SettingsDeps;
use crate::routes::storage::ScopeDeps;
use crate::{ApiSettings, AppState, build_router};
use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use bytes::Bytes;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sr_auth::{
    repo::{SessionRecord, SessionRepository},
    session::SessionToken,
};
use sr_chat::guild_rules::plan_new_guild;
use sr_chat::memory::MemoryBackend;
use sr_chat::repo::{
    Attachment, GuildRepository, MessageRepository, MessageStore, UploadState,
};
use sr_chat::settings::memory::MemorySettingsRepo;
use sr_chat::{
    Channel, ChannelType, ChatError, ChatResult, Member, Message, MessageType, Role, Visibility,
};
use sr_core::permissions::{Overwrite, OverwriteKind, Permissions};
use sr_core::{
    AttachmentId, ChannelId, Clock, GuildId, ManualClock, MessageId, RoleId, SnowflakeGenerator,
    UserId,
};
use sr_events::InProcessBus;
use sr_features::testing::FakeActions;
use sr_features::{FeatureCtx, FeatureRegistry, FeatureService, InMemorySettingsStore, ScopeRef};
use sr_storage::mounts::{
    MemoryMountRepo, MountError, MountInput, MountOwner, MountRepo, NewMount, Provider, Sealer,
    Secret,
};
use sr_storage::scopes::{
    FactsError, Grant, GrantSet, GuildStanding, MemoryScopeFacts, ScopeFacts, ScopeStore, Share,
    SharedToMe, ShelvesSummary,
};
use sr_storage::scrapbook::{
    Archiver, MemoryScrapbook, Refusal, ScrapbookBuckets, ScrapbookStore, Target,
};
use sr_storage::storage::{MultipartSession, ObjectInfo, ObjectStream, PresignMethod};
use sr_storage::{ObjectStorage, ObjectStoreStorage};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tower::ServiceExt;

const T0: i64 = 1_800_000_000_000;
const OWNER: u64 = 1;
const AUTHOR: u64 = 2; // a member who wrote the messages
const PLAIN: u64 = 3; // a member with no extra rights
const MOD: u64 = 4; // MANAGE_MESSAGES
const ADMIN: u64 = 5; // MANAGE_GUILD
const VIEWER: u64 = 6; // a member whose role has every storage view grant
const OUT: u64 = 7; // not a member

struct Xor;
impl Sealer for Xor {
    fn seal(&self, aad: &[u8], plain: &[u8]) -> Result<Vec<u8>, MountError> {
        let mut v = aad.to_vec();
        v.push(b'|');
        v.extend(plain.iter().map(|b| b ^ 0x5a));
        Ok(v)
    }
    fn open(&self, aad: &[u8], blob: &[u8]) -> Result<Vec<u8>, MountError> {
        let rest = blob.strip_prefix(aad).and_then(|r| r.strip_prefix(b"|"));
        rest.map(|r| r.iter().map(|b| b ^ 0x5a).collect())
            .ok_or(MountError::Unseal)
    }
}

struct FactsStore(Arc<MemoryScopeFacts>);

#[async_trait]
impl ScopeFacts for FactsStore {
    async fn owner_active(&self, o: MountOwner) -> Result<bool, FactsError> {
        self.0.owner_active(o).await
    }
    async fn are_friends(&self, a: i64, b: i64) -> Result<bool, FactsError> {
        self.0.are_friends(a, b).await
    }
    async fn share(&self, m: i64, f: i64) -> Result<Option<Share>, FactsError> {
        self.0.share(m, f).await
    }
    async fn guild_standing(&self, g: i64, u: i64) -> Result<Option<GuildStanding>, FactsError> {
        self.0.guild_standing(g, u).await
    }
    async fn role_grants(&self, g: i64, r: &[i64]) -> Result<GrantSet, FactsError> {
        self.0.role_grants(g, r).await
    }
    async fn attached_guilds(&self, m: i64) -> Result<Vec<i64>, FactsError> {
        self.0.attached_guilds(m).await
    }
    async fn delisted(&self, m: i64, k: &[String]) -> Result<Vec<String>, FactsError> {
        self.0.delisted(m, k).await
    }
}

#[async_trait]
impl ScopeStore for FactsStore {
    async fn guild_exists(&self, _: i64) -> Result<bool, FactsError> {
        Ok(true)
    }
    async fn attach_mount(&self, _: i64, _: i64, _: i64) -> Result<bool, FactsError> {
        Ok(true)
    }
    async fn detach_mount(&self, _: i64, _: i64) -> Result<bool, FactsError> {
        Ok(true)
    }
    async fn role_in_guild(&self, _: i64, _: i64) -> Result<bool, FactsError> {
        Ok(true)
    }
    async fn set_share(&self, _: i64, _: i64, _: Share) -> Result<(), FactsError> {
        Ok(())
    }
    async fn delete_share(&self, _: i64, _: i64) -> Result<bool, FactsError> {
        Ok(false)
    }
    async fn shares_of_mount(&self, _: i64) -> Result<Vec<(i64, Share)>, FactsError> {
        Ok(vec![])
    }
    async fn shares_to(&self, _: i64) -> Result<Vec<SharedToMe>, FactsError> {
        Ok(vec![])
    }
    async fn set_role_grants(&self, _: i64, _: i64, _: &GrantSet) -> Result<(), FactsError> {
        Ok(())
    }
    async fn grants_of_guild(&self, _: i64) -> Result<BTreeMap<i64, GrantSet>, FactsError> {
        Ok(BTreeMap::new())
    }
    async fn catalog_counts(&self) -> Result<(i64, i64), FactsError> {
        Ok((0, 0))
    }
    async fn shelves_summary(&self, _: i64) -> Result<ShelvesSummary, FactsError> {
        Ok(ShelvesSummary {
            cinema: 0,
            library: 0,
            music: 0,
        })
    }
}

struct NoRoles;
#[async_trait]
impl RoleAssigner for NoRoles {
    async fn assign(
        &self,
        _: UserId,
        _: GuildId,
        _: UserId,
        _: RoleId,
        _: bool,
    ) -> ChatResult<()> {
        Err(ChatError::MissingPermissions)
    }
}

/// A shelf bucket whose deletes can be made to fail (a third-party bucket that revoked the key).
struct Shelf {
    inner: ObjectStoreStorage,
    refuse_delete: AtomicBool,
}

#[async_trait]
impl ObjectStorage for Shelf {
    async fn put(&self, key: &str, data: Bytes) -> sr_storage::Result<()> {
        self.inner.put(key, data).await
    }
    async fn get(&self, key: &str) -> sr_storage::Result<Bytes> {
        self.inner.get(key).await
    }
    async fn get_range(&self, key: &str, s: u64, e: u64) -> sr_storage::Result<Bytes> {
        self.inner.get_range(key, s, e).await
    }
    async fn get_stream(
        &self,
        key: &str,
        range: Option<std::ops::Range<u64>>,
    ) -> sr_storage::Result<ObjectStream> {
        self.inner.get_stream(key, range).await
    }
    async fn head(&self, key: &str) -> sr_storage::Result<ObjectInfo> {
        self.inner.head(key).await
    }
    async fn delete(&self, key: &str) -> sr_storage::Result<()> {
        if self.refuse_delete.load(Ordering::SeqCst) {
            return Err(sr_storage::StorageError::Backend("403 AccessDenied".into()));
        }
        self.inner.delete(key).await
    }
    async fn list(&self, prefix: &str) -> sr_storage::Result<Vec<ObjectInfo>> {
        self.inner.list(prefix).await
    }
    async fn presign(
        &self,
        m: PresignMethod,
        key: &str,
        ttl: Duration,
    ) -> sr_storage::Result<String> {
        self.inner.presign(m, key, ttl).await
    }
    async fn multipart_create(&self, key: &str) -> sr_storage::Result<Box<dyn MultipartSession>> {
        self.inner.multipart_create(key).await
    }
}

struct Buckets(Arc<Shelf>);
#[async_trait]
impl ScrapbookBuckets for Buckets {
    async fn writer(&self, _: &Target) -> Result<Arc<dyn ObjectStorage>, Refusal> {
        Ok(self.0.clone())
    }
    async fn remover(&self, _: i64) -> Result<Arc<dyn ObjectStorage>, String> {
        Ok(self.0.clone())
    }
}

struct Env {
    app: Router,
    tokens: Vec<(u64, String)>,
    backend: MemoryBackend,
    features: FeatureService,
    store: Arc<MemoryScrapbook>,
    shelf: Arc<Shelf>,
    source: Arc<ObjectStoreStorage>,
    guild: GuildId,
    general: ChannelId,
    secret: ChannelId,
    mount: i64,
    other_mount: i64,
    next: std::sync::atomic::AtomicU64,
}

fn standing(roles: &[i64], manage_guild: bool) -> GuildStanding {
    GuildStanding {
        is_owner: false,
        administrator: false,
        manage_guild,
        manage_roles: false,
        role_ids: roles.to_vec(),
    }
}

async fn env() -> Env {
    let clock = Arc::new(ManualClock::new(T0));
    let dyn_clock: Arc<dyn Clock> = clock.clone();
    let backend = MemoryBackend::new();
    let bus = Arc::new(InProcessBus::new());
    let source = Arc::new(ObjectStoreStorage::in_memory());
    let ids = SnowflakeGenerator::new(dyn_clock.clone(), 9, 9).unwrap();
    let bp = plan_new_guild(UserId::new(OWNER), "Club", T0, || ids.generate()).unwrap();
    let guild = bp.guild.id;
    let general = bp.channels[2].id;
    backend.insert_guild(&bp).await.unwrap();

    // Roles: mods (MANAGE_MESSAGES), admins (MANAGE_GUILD), viewers (no chat rights; storage grants below).
    let role = |name: &str, perms: Permissions, pos: i32| Role {
        id: RoleId::new(ids.generate().get()),
        guild_id: guild,
        name: name.into(),
        color: 0,
        secondary_color: None,
        tertiary_color: None,
        hoist: false,
        position: pos,
        permissions: perms,
        mentionable: false,
        managed: false,
        unicode_emoji: None,
        icon_key: None,
    };
    let mods = role("mods", Permissions::MANAGE_MESSAGES, 1);
    let admins = role("admins", Permissions::MANAGE_GUILD, 2);
    let viewers = role("viewers", Permissions::empty(), 3);
    let (mods_id, admins_id, viewers_id) = (mods.id, admins.id, viewers.id);
    let member = |user: u64, roles: Vec<RoleId>| Member {
        guild_id: guild,
        user_id: UserId::new(user),
        nick: None,
        role_ids: roles,
        joined_at_ms: T0,
        communication_disabled_until_ms: None,
        pending: false,
        avatar_key: None,
        banner_key: None,
        bio: None,
        extras: Default::default(),
    };
    {
        let mut d = backend.data();
        d.roles.extend([mods, admins, viewers]);
        d.members.extend([
            member(AUTHOR, vec![]),
            member(PLAIN, vec![]),
            member(MOD, vec![mods_id]),
            member(ADMIN, vec![admins_id]),
            member(VIEWER, vec![viewers_id]),
        ]);
        for (u, name) in [
            (OWNER, "owner"),
            (AUTHOR, "ada"),
            (PLAIN, "pat"),
            (MOD, "mo"),
            (ADMIN, "adam"),
            (VIEWER, "vic"),
        ] {
            d.users.insert(
                UserId::new(u),
                sr_chat::repo::UserSummary {
                    id: UserId::new(u),
                    username: name.into(),
                    display_name: None,
                    avatar_key: None,
                    banner_key: None,
                    bot: false,
                    primary_guild: None,
                    patron: false,
                },
            );
        }
    }
    // A second channel that @everyone cannot see.
    let secret = ChannelId::new(ids.generate().get());
    backend.data().channels.insert(
        secret,
        Channel::new(secret, Some(guild), ChannelType::Text),
    );
    backend.data().overwrites.insert(
        secret,
        vec![Overwrite {
            id: guild.snowflake(),
            kind: OverwriteKind::Role,
            allow: Permissions::empty(),
            deny: Permissions::VIEW_CHANNEL,
        }],
    );

    // Storage: the server's shelf, the facts behind the viewers' scopes, another server's shelf.
    let g = guild.get() as i64;
    let repo: Arc<dyn MountRepo> = MemoryMountRepo::shared();
    let mk = |id: i64, owner: MountOwner| {
        NewMount::seal(
            &Xor,
            id,
            MountInput {
                owner,
                label: format!("Shelf {id}"),
                provider: Provider::Garage,
                endpoint: "http://127.0.0.1:9".into(),
                region: "garage".into(),
                bucket: "shelf".into(),
                prefix: String::new(),
                path_style: true,
                access_key_id: "GK0123456789abcd".into(),
            },
            &Secret::new("wJalrXUtnFEMI/K7MDENG/SeedSecretValue"),
        )
        .unwrap()
    };
    let mount = 901;
    let other_mount = 902;
    repo.create(mk(mount, MountOwner::Guild(g)), None)
        .await
        .unwrap();
    repo.set_cors_ok(mount, Some(1)).await.unwrap();
    repo.create(mk(other_mount, MountOwner::Guild(g + 1)), None)
        .await
        .unwrap();
    let facts = Arc::new(MemoryScopeFacts::new());
    let (r_viewers, r_admins) = (viewers_id.get() as i64, admins_id.get() as i64);
    facts.member(g, VIEWER as i64, standing(&[r_viewers], false));
    facts.member(g, ADMIN as i64, standing(&[r_admins], true));
    facts.member(g, AUTHOR as i64, standing(&[], false));
    facts.member(g, PLAIN as i64, standing(&[], false));
    facts.member(g, MOD as i64, standing(&[], false));
    facts.grant(
        g,
        r_viewers,
        GrantSet::of([Grant::ViewCinema, Grant::ViewLibrary, Grant::ViewMusic]),
    );

    // The module: offered by the operator, switched on in this server, a shelf chosen.
    let mut registry = FeatureRegistry::new();
    sr_features::modules::register_all(&mut registry).unwrap();
    let features = FeatureService::new(
        Arc::new(registry),
        Arc::new(InMemorySettingsStore::new()),
        None,
    );
    features
        .set_enabled(
            sr_features::modules::scrapbook::SCRAPBOOK_ID,
            &ScopeRef::Platform,
            Some(true),
        )
        .await
        .unwrap();
    features
        .set_enabled(
            sr_features::modules::scrapbook::SCRAPBOOK_ID,
            &ScopeRef::Guild { guild },
            Some(true),
        )
        .await
        .unwrap();
    let store = Arc::new(MemoryScrapbook::new());
    store
        .set_target(&Target {
            guild_id: g,
            mount_id: mount,
            granted_by: OWNER as i64,
        })
        .await
        .unwrap();
    let shelf = Arc::new(Shelf {
        inner: ObjectStoreStorage::in_memory(),
        refuse_delete: AtomicBool::new(false),
    });
    let next = Arc::new(std::sync::atomic::AtomicI64::new(5_000_000));
    let archiver = Arc::new(Archiver::new(
        store.clone(),
        Arc::new(Buckets(shelf.clone())),
        source.clone(),
        Arc::new(move || next.fetch_add(1, Ordering::SeqCst)),
    ));
    let msgs = MessagesDeps::new(
        backend.clone(),
        Arc::new(backend.clone()),
        source.clone(),
        bus.clone(),
        Arc::new(FixedEntitlements(1 << 20)),
        Arc::new(backend.clone()),
        None,
        dyn_clock.clone(),
        1,
        1,
    )
    .unwrap();

    let sessions = Arc::new(MemorySessions::new());
    let mut tokens = Vec::new();
    for (n, uid) in [OWNER, AUTHOR, PLAIN, MOD, ADMIN, VIEWER, OUT]
        .into_iter()
        .enumerate()
    {
        let t = SessionToken::generate().unwrap();
        sessions
            .create(SessionRecord {
                id: 100 + n as i64,
                user_id: uid as i64,
                token_hash: t.hash(),
                created_at: 0,
                last_seen_at: i64::MAX / 4,
                expires_at: i64::MAX / 4,
                device_name: None,
            })
            .await
            .unwrap();
        tokens.push((uid, t.expose().to_owned()));
    }
    let bots = Arc::new(MemoryBots::new());
    bots.insert(
        "bot",
        BotIdentity {
            user_id: OWNER as i64,
            application_id: 1,
        },
    );
    let settings_repo = Arc::new(MemorySettingsRepo::default());
    let entropy: crate::routes::settings::Entropy = Arc::new(|| 7);
    let deps = SettingsDeps::new(
        settings_repo,
        Arc::new(backend.clone()),
        source.clone(),
        dyn_clock.clone(),
        entropy,
        1,
        5,
    )
    .unwrap();
    let gf = GuildFeatures {
        ctx: FeatureCtx {
            bus: bus.clone(),
            settings: features.clone(),
            clock: dyn_clock.clone(),
            actions: Arc::new(FakeActions::new()),
            state: Arc::new(sr_features::state::InMemoryState::new()),
        },
        roles: Arc::new(NoRoles),
    };
    let scope = ScopeDeps {
        mounts: repo,
        store: Arc::new(FactsStore(facts)),
        sealer: Arc::new(Xor),
        guard: None,
    };
    let mut api = ApiSettings::default();
    api.rate_limit.enabled = false;
    let state = AppState::builder()
        .settings(api)
        .sessions(sessions)
        .bots(bots)
        .clock(dyn_clock)
        .extension(Arc::new(deps))
        .extension(Arc::new(gf))
        .extension(Arc::new(scope))
        .extension(Arc::new(msgs))
        .extension(Arc::new(ScrapbookDeps {
            archiver,
            messages: Arc::new(backend.clone()),
        }))
        .build();
    Env {
        app: build_router(state, router()),
        tokens,
        backend,
        features,
        store,
        shelf,
        source,
        guild,
        general,
        secret,
        mount,
        other_mount,
        next: std::sync::atomic::AtomicU64::new(700_000),
    }
}

impl Env {
    fn g(&self, rest: &str) -> String {
        format!("/guilds/{}/scrapbook{rest}", self.guild)
    }

    async fn call(
        &self,
        who: Option<u64>,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method.clone())
            .uri(format!("/api/v1{path}"));
        if let Some(uid) = who {
            let t = &self.tokens.iter().find(|(u, _)| *u == uid).unwrap().1;
            req = req.header("cookie", format!("sr_session={t}"));
            if method != Method::GET {
                req = req.header("x-sr-csrf", csrf::token_for(t));
            }
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

    async fn get(&self, who: u64, path: &str) -> (StatusCode, Value) {
        self.call(Some(who), Method::GET, path, None).await
    }

    /// A message by `AUTHOR` in `channel`, with one image attachment held in the instance bucket.
    async fn message(&self, channel: ChannelId, text: &str, file: Option<(&str, &str)>) -> MessageId {
        let n = self.next.fetch_add(2, Ordering::SeqCst);
        let id = MessageId::new(n);
        let m = Message {
            id,
            channel_id: channel,
            guild_id: Some(self.guild),
            author_id: UserId::new(AUTHOR),
            kind: MessageType::Default,
            content: text.into(),
            flags: 0,
            edited_at_ms: None,
            reference: None,
            attachment_ids: vec![],
            embeds: vec![],
            components: vec![],
            sticker_ids: vec![],
            poll: None,
            mention_everyone: false,
            mention_user_ids: vec![],
            mention_role_ids: vec![],
            mention_channel_ids: vec![],
            pinned: false,
            visibility: Visibility::Visible,
            deleted_at_ms: None,
            purge_after_ms: None,
            hold_id: None,
        };
        let mut m = m;
        if let Some((name, content_type)) = file {
            let key = format!("att/{n}/{name}");
            self.source
                .put(&key, Bytes::from_static(b"png-bytes"))
                .await
                .unwrap();
            let att = Attachment {
                id: AttachmentId::new(n + 1),
                channel_id: channel,
                uploader: UserId::new(AUTHOR),
                filename: name.into(),
                size: 9,
                key,
                declared_content_type: Some(content_type.into()),
                content_type: Some(content_type.into()),
                description: None,
                spoiler: false,
                state: UploadState::Attached,
                message_id: Some(id),
                expires_at_ms: None,
                created_at_ms: T0,
                duration_ms: None,
                waveform: None,
                width: None,
                height: None,
                blurhash: None,
            };
            m.attachment_ids = vec![att.id];
            self.backend.insert_attachment(&att).await.unwrap();
        }
        self.backend.insert_message(&m).await.unwrap();
        id
    }

    async fn send(&self, who: u64, channel: ChannelId, message: MessageId) -> (StatusCode, Value) {
        self.call(
            Some(who),
            Method::PUT,
            &format!("/channels/{channel}/messages/{message}/scrapbook"),
            Some(json!({"tags": ["Funny"]})),
        )
        .await
    }
}

#[tokio::test]
async fn the_module_is_hidden_until_the_operator_offers_it() {
    let e = env().await;
    let id = sr_features::modules::scrapbook::SCRAPBOOK_ID;
    // Offered and on: a member sees the status.
    let (s, v) = e.get(PLAIN, &e.g("/status")).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!((v["offered"].as_bool(), v["enabled"].as_bool()), (Some(true), Some(true)));
    // The operator withdraws it (clears the platform flag): hidden, the gallery is a 404.
    e.features
        .set_enabled(id, &ScopeRef::Platform, None)
        .await
        .unwrap();
    let (s, v) = e.get(PLAIN, &e.g("/status")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["offered"], false);
    assert_eq!(e.get(PLAIN, &e.g("")).await.0, StatusCode::NOT_FOUND);
    // Offered again but the server has it off: 403 SR_FEATURE_DISABLED.
    e.features
        .set_enabled(id, &ScopeRef::Platform, Some(true))
        .await
        .unwrap();
    e.features
        .set_enabled(id, &ScopeRef::Guild { guild: e.guild }, Some(false))
        .await
        .unwrap();
    let (s, v) = e.get(PLAIN, &e.g("")).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(v["code"], 900001);
    let (_, v) = e.get(PLAIN, &e.g("/status")).await;
    assert_eq!((v["offered"].as_bool(), v["enabled"].as_bool()), (Some(true), Some(false)));
}

#[tokio::test]
async fn only_members_use_it_and_bots_are_refused() {
    let e = env().await;
    assert_eq!(e.get(OUT, &e.g("")).await.0, StatusCode::FORBIDDEN);
    assert_eq!(e.get(OUT, &e.g("/status")).await.0, StatusCode::FORBIDDEN);
    // No session: 401.
    assert_eq!(
        e.call(None, Method::GET, &e.g(""), None).await.0,
        StatusCode::UNAUTHORIZED
    );
    // A bot token is not a session.
    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/v1{}", e.g("")))
        .header("authorization", "Bot bot")
        .body(Body::empty())
        .unwrap();
    let resp = e.app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status() == StatusCode::FORBIDDEN || resp.status() == StatusCode::UNAUTHORIZED,
        "{}",
        resp.status()
    );
}

#[tokio::test]
async fn who_may_send_by_hand_is_a_role_or_manage_messages() {
    let e = env().await;
    let m = e.message(e.general, "frame me", Some(("cat.png", "image/png"))).await;
    // The status tells each viewer whether the menu entry is theirs.
    for (who, can) in [(PLAIN, false), (AUTHOR, false), (MOD, true), (OWNER, true)] {
        let (_, v) = e.get(who, &e.g("/status")).await;
        assert_eq!(v["can_send"], can, "user {who}");
    }
    assert_eq!(e.send(PLAIN, e.general, m).await.0, StatusCode::FORBIDDEN);
    let (s, v) = e.send(MOD, e.general, m).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["via"], "manual");
    assert_eq!(v["tags"], json!(["funny"]));
    assert_eq!(v["objects"].as_array().unwrap().len(), 1);
    // The copy is in the server's shelf under the dated prefix.
    let keys = e.shelf.list("").await.unwrap();
    assert_eq!(keys.len(), 1);
    assert!(keys[0].key.starts_with("scrapbook/"));
    assert!(keys[0].key.contains(&format!("/{m}/0-cat.png")));
    // Sending again is the same entry, not a second copy.
    assert_eq!(e.send(MOD, e.general, m).await.0, StatusCode::OK);
    assert_eq!(e.shelf.list("").await.unwrap().len(), 1);
    // Someone who cannot see the channel cannot send its messages either (the channel is unknown to them).
    let hidden = e.message(e.secret, "secret thing", None).await;
    assert_eq!(e.send(PLAIN, e.secret, hidden).await.0, StatusCode::FORBIDDEN);
    // A role named in the module settings lets a member send.
    let viewers = e
        .backend
        .data()
        .roles
        .iter()
        .find(|r| r.name == "viewers")
        .unwrap()
        .id;
    e.features
        .set_settings(
            sr_features::modules::scrapbook::SCRAPBOOK_ID,
            &ScopeRef::Guild { guild: e.guild },
            json!({"roles": [viewers.to_string()]})
                .as_object()
                .unwrap()
                .clone(),
        )
        .await
        .unwrap();
    assert_eq!(e.get(VIEWER, &e.g("/status")).await.1["can_send"], true);
    let m2 = e.message(e.general, "another", None).await;
    assert_eq!(e.send(VIEWER, e.general, m2).await.0, StatusCode::OK);
}

#[tokio::test]
async fn sending_without_a_shelf_says_so() {
    let e = env().await;
    e.store.clear_target(e.guild.get() as i64).await.unwrap();
    let m = e.message(e.general, "x", None).await;
    let (s, v) = e.send(MOD, e.general, m).await;
    assert_eq!(s, StatusCode::CONFLICT, "{v}");
    assert!(v.to_string().contains("SCRAPBOOK_NO_SHELF"), "{v}");
}

#[tokio::test]
async fn an_entry_is_listed_only_to_members_who_can_view_its_source_channel() {
    let e = env().await;
    let open = e.message(e.general, "in the open", None).await;
    let hidden = e.message(e.secret, "behind a closed door", None).await;
    for (m, ch) in [(open, e.general), (hidden, e.secret)] {
        assert_eq!(e.send(OWNER, ch, m).await.0, StatusCode::OK);
    }
    let texts = |v: &Value| -> Vec<String> {
        v["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["content"].as_str().unwrap().to_owned())
            .collect()
    };
    // The owner sees both; an ordinary member only the open one.
    let (s, v) = e.get(OWNER, &e.g("")).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(texts(&v).len(), 2);
    let (_, v) = e.get(PLAIN, &e.g("")).await;
    assert_eq!(texts(&v), vec!["in the open".to_string()]);
    // The filter choices count only what the viewer may see.
    let (_, v) = e.get(PLAIN, &e.g("?facets=true")).await;
    assert_eq!(v["facets"]["channels"].as_array().unwrap().len(), 1);
    // Filtering by the hidden channel does not reveal it.
    let (_, v) = e
        .get(PLAIN, &format!("{}?channel_id={}", e.g(""), e.secret))
        .await;
    assert!(texts(&v).is_empty());
    // Opening the channel to everyone shows the entry at once.
    e.backend.data().overwrites.remove(&e.secret);
    let (_, v) = e.get(PLAIN, &e.g("")).await;
    assert_eq!(texts(&v).len(), 2);
}

#[tokio::test]
async fn the_gallery_filters_by_member_tag_and_date() {
    let e = env().await;
    let a = e.message(e.general, "one", None).await;
    let b = e.message(e.general, "two", None).await;
    for m in [a, b] {
        assert_eq!(e.send(MOD, e.general, m).await.0, StatusCode::OK);
    }
    let count = |v: &Value| v["entries"].as_array().unwrap().len();
    let (_, v) = e.get(PLAIN, &format!("{}?author_id={AUTHOR}", e.g(""))).await;
    assert_eq!(count(&v), 2);
    let (_, v) = e.get(PLAIN, &format!("{}?author_id={PLAIN}", e.g(""))).await;
    assert_eq!(count(&v), 0);
    let (_, v) = e.get(PLAIN, &format!("{}?tag=funny", e.g(""))).await;
    assert_eq!(count(&v), 2);
    let (_, v) = e.get(PLAIN, &format!("{}?tag=other", e.g(""))).await;
    assert_eq!(count(&v), 0);
    // The test message ids are the first instants after the platform epoch (2026-01-01): a later window is empty.
    let (s, v) = e.get(PLAIN, &format!("{}?from=2026-02-01", e.g(""))).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(count(&v), 0);
    let (_, v) = e.get(PLAIN, &format!("{}?from=2025-12-31&to=2026-01-01", e.g(""))).await;
    assert_eq!(count(&v), 2);
    assert_eq!(
        e.get(PLAIN, &format!("{}?from=yesterday", e.g(""))).await.0,
        StatusCode::BAD_REQUEST
    );
    // Paging: one at a time, the cursor leads to the other.
    let (_, v) = e.get(PLAIN, &format!("{}?limit=1", e.g(""))).await;
    assert_eq!(count(&v), 1);
    let next = v["next_before"].as_str().unwrap().to_owned();
    let (_, v2) = e.get(PLAIN, &format!("{}?limit=1&before={next}", e.g(""))).await;
    assert_eq!(count(&v2), 1);
    assert!(v2.get("next_before").is_none() || v2["next_before"].is_null());
}

#[tokio::test]
async fn a_file_is_signed_only_for_a_viewer_whose_storage_scopes_reach_the_shelf() {
    let e = env().await;
    let m = e.message(e.general, "pic", Some(("cat.png", "image/png"))).await;
    let (s, v) = e.send(MOD, e.general, m).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (entry, object) = (
        v["id"].as_str().unwrap().to_owned(),
        v["objects"][0]["id"].as_str().unwrap().to_owned(),
    );
    let path = e.g(&format!("/{entry}/objects/{object}/presign"));
    // A member without any storage view grant sees the text and "not shared" for the file ...
    let (_, v) = e.get(PLAIN, &e.g("")).await;
    assert_eq!(v["entries"][0]["content"], "pic");
    assert_eq!(v["entries"][0]["objects"][0]["playable"], false);
    assert_eq!(
        e.call(Some(PLAIN), Method::POST, &path, Some(json!({}))).await.0,
        StatusCode::FORBIDDEN
    );
    // ... and one whose role holds the view grants gets a signature.
    let (_, v) = e.get(VIEWER, &e.g("")).await;
    assert_eq!(v["entries"][0]["objects"][0]["playable"], true);
    let (s, v) = e.call(Some(VIEWER), Method::POST, &path, Some(json!({}))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert!(v["url"].as_str().unwrap().contains("X-Amz-Signature"), "{v}");
    // A signature for an entry in a channel the viewer cannot see is refused as unknown.
    let hidden = e.message(e.secret, "secret", Some(("s.png", "image/png"))).await;
    let (_, v) = e.send(OWNER, e.secret, hidden).await;
    let hpath = e.g(&format!(
        "/{}/objects/{}/presign",
        v["id"].as_str().unwrap(),
        v["objects"][0]["id"].as_str().unwrap()
    ));
    assert_eq!(
        e.call(Some(VIEWER), Method::POST, &hpath, Some(json!({}))).await.0,
        StatusCode::NOT_FOUND
    );
    // An object id that is not the entry's is unknown.
    let wrong = e.g(&format!("/{entry}/objects/12345/presign"));
    assert_eq!(
        e.call(Some(VIEWER), Method::POST, &wrong, Some(json!({}))).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn tags_and_removal_belong_to_the_author_the_sender_and_message_managers() {
    let e = env().await;
    let m = e.message(e.general, "mine", Some(("a.png", "image/png"))).await;
    let (_, v) = e.send(MOD, e.general, m).await;
    let entry = v["id"].as_str().unwrap().to_owned();
    let path = e.g(&format!("/{entry}"));
    // A plain member neither retags nor removes.
    assert_eq!(
        e.call(Some(PLAIN), Method::PATCH, &path, Some(json!({"tags": ["x"]}))).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        e.call(Some(PLAIN), Method::DELETE, &path, None).await.0,
        StatusCode::FORBIDDEN
    );
    // The author does.
    let (s, v) = e
        .call(Some(AUTHOR), Method::PATCH, &path, Some(json!({"tags": ["#Cats", "cats", " "]})))
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["tags"], json!(["cats"]));
    let (_, v) = e.get(PLAIN, &e.g("")).await;
    assert_eq!(v["entries"][0]["can_edit"], false);
    let (_, v) = e.get(AUTHOR, &e.g("")).await;
    assert_eq!(v["entries"][0]["can_edit"], true);
    // Removal also reaches the bucket.
    assert_eq!(e.shelf.list("").await.unwrap().len(), 1);
    let (s, v) = e.call(Some(AUTHOR), Method::DELETE, &path, None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!((v["copies_removed"].as_u64(), v["copies_failed"].as_u64()), (Some(1), Some(0)));
    assert!(e.shelf.list("").await.unwrap().is_empty());
    assert_eq!(
        e.call(Some(AUTHOR), Method::DELETE, &path, None).await.0,
        StatusCode::NOT_FOUND
    );
    // A member who manages messages may remove anyone's entry, even while the module is off for the server.
    let m2 = e.message(e.general, "two", None).await;
    let (_, v) = e.send(MOD, e.general, m2).await;
    let p2 = e.g(&format!("/{}", v["id"].as_str().unwrap()));
    e.features
        .set_enabled(
            sr_features::modules::scrapbook::SCRAPBOOK_ID,
            &ScopeRef::Guild { guild: e.guild },
            Some(false),
        )
        .await
        .unwrap();
    assert_eq!(
        e.call(Some(MOD), Method::DELETE, &p2, None).await.0,
        StatusCode::OK
    );
}

/// The roadmap's risk, through the routes: a bucket that refuses the delete is visible to the managers, and
/// to nobody else.
#[tokio::test]
async fn a_failed_bucket_removal_is_visible_to_managers_only_and_can_be_retried_or_dismissed() {
    let e = env().await;
    let m = e.message(e.general, "doomed", Some(("d.png", "image/png"))).await;
    let (_, v) = e.send(MOD, e.general, m).await;
    let entry = v["id"].as_str().unwrap().to_owned();
    e.shelf.refuse_delete.store(true, Ordering::SeqCst);
    let (s, v) = e
        .call(Some(AUTHOR), Method::DELETE, &e.g(&format!("/{entry}")), None)
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    // The member who removed it is told the copy is still there.
    assert_eq!((v["copies_removed"].as_u64(), v["copies_failed"].as_u64()), (Some(0), Some(1)));
    assert_eq!(e.shelf.list("").await.unwrap().len(), 1);

    // Managers read the list; members (and message managers who do not manage the server) cannot.
    for who in [PLAIN, AUTHOR, MOD] {
        assert_eq!(
            e.get(who, &e.g("/removals")).await.0,
            StatusCode::FORBIDDEN,
            "user {who}"
        );
    }
    let (s, v) = e.get(ADMIN, &e.g("/removals")).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let list = v["removals"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert!(list[0]["last_error"].as_str().unwrap().contains("AccessDenied"));
    assert_eq!(list[0]["attempts"], 1);
    assert!(list[0]["key"].as_str().unwrap().contains("d.png"));
    // The status counts them for managers.
    assert_eq!(e.get(ADMIN, &e.g("/status")).await.1["failed_removals"], 1);
    assert_eq!(e.get(PLAIN, &e.g("/status")).await.1["failed_removals"], 0);

    let id = list[0]["id"].as_str().unwrap().to_owned();
    let retry = e.g(&format!("/removals/{id}/retry"));
    let dismiss = e.g(&format!("/removals/{id}"));
    assert_eq!(
        e.call(Some(PLAIN), Method::POST, &retry, Some(json!({}))).await.0,
        StatusCode::FORBIDDEN
    );
    // Still down: the retry reports it and the row stays with one more attempt.
    let (s, v) = e.call(Some(ADMIN), Method::POST, &retry, Some(json!({}))).await;
    assert_eq!((s, v["removed"].as_bool()), (StatusCode::OK, Some(false)));
    assert_eq!(e.get(ADMIN, &e.g("/removals")).await.1["removals"][0]["attempts"], 2);
    // The key works again: the retry removes the copy and the row.
    e.shelf.refuse_delete.store(false, Ordering::SeqCst);
    let (_, v) = e.call(Some(ADMIN), Method::POST, &retry, Some(json!({}))).await;
    assert_eq!(v["removed"], true);
    assert!(e.shelf.list("").await.unwrap().is_empty());
    assert!(e.get(ADMIN, &e.g("/removals")).await.1["removals"].as_array().unwrap().is_empty());
    // An unknown removal is unknown, and dismissing needs the manager too.
    assert_eq!(
        e.call(Some(ADMIN), Method::DELETE, &dismiss, None).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.call(Some(PLAIN), Method::DELETE, &dismiss, None).await.0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn only_managers_choose_the_shelf_and_it_must_be_the_servers_own() {
    let e = env().await;
    let put = |who: u64, mount: i64| {
        let e = &e;
        async move {
            e.call(
                Some(who),
                Method::PUT,
                &e.g("/target"),
                Some(json!({"mount_id": mount.to_string()})),
            )
            .await
        }
    };
    assert_eq!(put(PLAIN, e.mount).await.0, StatusCode::FORBIDDEN);
    assert_eq!(put(MOD, e.mount).await.0, StatusCode::FORBIDDEN);
    // Another server's shelf is unknown.
    assert_eq!(put(ADMIN, e.other_mount).await.0, StatusCode::NOT_FOUND);
    assert_eq!(put(ADMIN, 424_242).await.0, StatusCode::NOT_FOUND);
    // The server's own shelf, by a manager who holds the grant being delegated.
    let (s, v) = put(ADMIN, e.mount).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["mount_id"], e.mount.to_string());
    assert_eq!(v["granted_by"], ADMIN.to_string());
    let t = e.store.target(e.guild.get() as i64).await.unwrap().unwrap();
    assert_eq!((t.mount_id, t.granted_by), (e.mount, ADMIN as i64));
    // Dropping it needs a manager as well.
    assert_eq!(
        e.call(Some(PLAIN), Method::DELETE, &e.g("/target"), None).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        e.call(Some(ADMIN), Method::DELETE, &e.g("/target"), None).await.0,
        StatusCode::NO_CONTENT
    );
    assert!(e.store.target(e.guild.get() as i64).await.unwrap().is_none());
    // The status shows managers the shelf and the problem.
    let (_, v) = e.get(ADMIN, &e.g("/status")).await;
    assert!(v.get("target").is_none() || v["target"].is_null());
    assert_eq!(v["can_manage"], true);
}
