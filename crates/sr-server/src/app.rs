//! Process wiring and graceful shutdown.
//!
//! Startup order: pool, ids, storage, bus, backends, feature service,
//! gateway, router, listeners, job runner. Shutdown order (on SIGINT/SIGTERM):
//! tell gateway clients to reconnect, stop accepting HTTP and drain for at
//! most `shutdown_grace_secs`, stop the gateway tasks, let the job runner
//! finish in-flight jobs (same grace), close the pool.

use anyhow::Context;
use axum::{
    Extension, Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::{Next, from_fn_with_state},
    response::{IntoResponse, Response},
};
use sr_api::{
    ApiSettings, AppState, build_router, internal_router,
    router::with_request_id,
    routes::{
        self,
        auth::{
            AuthConfig, AuthService, DiscordSettings,
            mail::{LogMailer, Mailer, SmtpMailer},
        },
        guilds::GuildApi,
        search::SearchDeps,
        settings::{AutoModGateHandle, SettingsDeps, events::BusSettingsEvents},
    },
};
use sr_chat::settings::SettingsService;
use sr_chat::{ChatLimits, ChatService};
use sr_core::{Clock, SnowflakeGenerator, SystemClock, config::Config};
use sr_db::{
    DbConfig,
    auth_repo::{PgAuthRepo, parse_secret_key},
    auth_store::PgAuthStore,
    chat_repo::PgChatRepo,
    runner::JobRunner,
    settings_repo::PgSettingsRepo,
};
use sr_events::{EventBus, InProcessBus};
use sr_features::{FeatureRegistry, FeatureService};
use sr_gateway::{Gateway, GatewayConfig};
use sr_search::MessageIndex;
use sr_storage::{ObjectStorage, ObjectStoreStorage};
use std::{future::Future, net::SocketAddr, sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::watch, task::JoinHandle};

use crate::{
    features_store::PgSettingsStore,
    gateway_pg::{PgAuthenticator, PgGuildDirectory},
    ready::{DbReady, StorageReady},
};

/// Parse the 64-hex-char secret key; empty yields a random ephemeral key.
pub fn secret_key(hex: &str) -> anyhow::Result<[u8; 32]> {
    if hex.is_empty() {
        tracing::warn!(
            "server.secret_key is empty: using an ephemeral key, stored TOTP secrets will not survive a restart"
        );
        return sr_auth::random::bytes::<32>().map_err(|e| anyhow::anyhow!("{e}"));
    }
    anyhow::ensure!(
        hex.len() == 64 && hex.is_ascii(),
        "server.secret_key must be 64 hex characters"
    );
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16)
            .context("server.secret_key must be hex")?;
    }
    Ok(out)
}

/// The search index directory: `[search] dir`, except that the legacy
/// `SR_SEARCH_DIR` variable wins while `dir` is still the built-in default.
pub fn search_dir(configured: &str, legacy_env: Option<String>) -> String {
    match legacy_env {
        Some(v) if !v.is_empty() && configured == sr_core::config::SearchConfig::DEFAULT_DIR => v,
        _ => configured.to_owned(),
    }
}

/// Resolve on SIGINT or SIGTERM.
pub async fn os_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = term.recv() => {}
                }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
    tracing::info!("shutdown signal received");
}

/// Serve `app` on `listener` until `shutdown` resolves, then drain for at
/// most `grace`. Long-lived connections (WebSockets) never finish draining on
/// their own, so after the grace period the server is dropped regardless.
/// `on_shutdown` runs the moment shutdown begins (before draining).
pub async fn serve_until(
    listener: TcpListener,
    app: Router,
    shutdown: impl Future<Output = ()>,
    on_shutdown: impl FnOnce(),
    grace: Duration,
) -> anyhow::Result<()> {
    let (tx, mut rx) = watch::channel(false);
    let mut serve: JoinHandle<std::io::Result<()>> = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            let _ = rx.wait_for(|v| *v).await;
        })
        .await
    });
    tokio::select! {
        r = &mut serve => {
            return r.context("server task")?.context("serving");
        }
        () = shutdown => {}
    }
    on_shutdown();
    let _ = tx.send(true);
    match tokio::time::timeout(grace, &mut serve).await {
        Ok(r) => r.context("server task")?.context("serving"),
        Err(_) => {
            tracing::warn!(
                ?grace,
                "open connections did not drain in time; closing them"
            );
            serve.abort();
            Ok(())
        }
    }
}

/// State for [`connection_cap`].
#[derive(Clone)]
struct ConnectionCap {
    gateway: Gateway,
    max: usize,
}

/// Refuse new gateway upgrades with 503 once `gateway.max_connections`
/// sessions are open (open question 107).
async fn connection_cap(State(cap): State<ConnectionCap>, req: Request, next: Next) -> Response {
    if cap.gateway.session_count() >= cap.max {
        tracing::warn!(max = cap.max, "gateway connection cap reached");
        return (StatusCode::SERVICE_UNAVAILABLE, "gateway is at capacity").into_response();
    }
    next.run(req).await
}

/// Run the whole server until `shutdown` resolves.
pub async fn run(cfg: Config, shutdown: impl Future<Output = ()>) -> anyhow::Result<()> {
    let grace = Duration::from_secs(cfg.server.shutdown_grace_secs);
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);

    let mut db_cfg = DbConfig::new(cfg.database.url.expose());
    db_cfg.max_connections = cfg.database.max_connections;
    let pool = sr_db::connect(&db_cfg)
        .await
        .context("connecting to Postgres (is it up? has `sr-cli migrate` run?)")?;
    if cfg.database.auto_migrate {
        sr_db::migrate(&pool)
            .await
            .context("running migrations (database.auto_migrate)")?;
        tracing::info!("migrations applied");
    }
    // The Patron grace period shown by `sr_patron` is bound once at start (D-073, D-256).
    let patron_grace_days = sr_db::payments_repo::patron_grace_days(&pool).await;

    let ids = Arc::new(
        SnowflakeGenerator::new(SystemClock, cfg.server.worker_id, cfg.server.process_id)
            .context("snowflake generator")?,
    );
    let id_src: sr_db::auth_repo::IdSource = {
        let ids = ids.clone();
        Arc::new(move || ids.generate().get() as i64)
    };
    let previous_keys = cfg
        .server
        .secret_key_previous
        .iter()
        .map(|k| parse_secret_key(k.expose()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!("server.secret_key_previous: {e}"))?;
    let master_key = secret_key(cfg.server.secret_key.expose())?;
    let auth_repo = PgAuthRepo::new(pool.clone(), id_src.clone(), master_key)
        .with_previous_keys(previous_keys.clone());

    // The wrapper lets the search indexer notice when it lagged behind the bus.
    let lag_bus = Arc::new(crate::lag_bus::LagTapBus::new(
        Arc::new(InProcessBus::new()),
    ));
    let bus: Arc<dyn EventBus> = lag_bus.clone();

    // Entrance/auth service (open question 140): Postgres store, SMTP when
    // `[smtp]` has a host (the log mailer otherwise, for development), and the
    // shared snowflake source. It is registered on AppState below.
    let auth_cfg = AuthConfig {
        public_url: cfg.server.public_url.clone(),
        trust_forwarded_for: cfg.server.trust_forwarded_for,
        ..AuthConfig::default()
    };
    let mailer: Arc<dyn Mailer> = if cfg.smtp.host.is_empty() {
        Arc::new(LogMailer)
    } else {
        Arc::new(SmtpMailer::from_config(&cfg.smtp).context("configuring [smtp]")?)
    };
    let auth_service = AuthService::new(
        Arc::new(PgAuthStore::new(
            pool.clone(),
            auth_repo.clone(),
            id_src.clone(),
        )),
        mailer.clone(),
        id_src.clone(),
    )
    .with_config(auth_cfg.clone())
    // Banned-Person registration attempts raise a staff alert (open question 142).
    .with_bus(bus.clone());
    let auth_service = if cfg.discord.client_id.is_empty() {
        auth_service
    } else {
        // Discord OAuth; the callback URL must match the one registered with Discord.
        let redirect_uri = format!(
            "{}/api/v1/sr/auth/discord/callback",
            cfg.server.public_url.trim_end_matches('/')
        );
        auth_service
            .with_discord(&DiscordSettings {
                client_id: cfg.discord.client_id.clone(),
                client_secret: cfg.discord.client_secret.expose().to_owned(),
                redirect_uri,
            })
            .map_err(|e| anyhow::anyhow!("configuring [discord]: {e}"))?
    };
    let auth_service = Arc::new(auth_service);

    // The developer platform: applications, bot tokens, OAuth2, commands, interactions. Bot
    // and bearer credentials authenticate against its stores (open question 56).
    let apps_repo = Arc::new(sr_db::apps_repo::PgAppsRepo::new(
        pool.clone(),
        sr_db::sealed::SecretBox::new(master_key).with_previous_keys(previous_keys.clone()),
    ));
    // The plaintext signing-seed column is gone (migration 0803, G-85); seeds are sealed at rest.
    // Upgrading from before 0771: run the previous release once so its apps.seal_seeds job runs.
    let apps_service = Arc::new(sr_apps::AppsService::new(
        apps_repo,
        clock.clone(),
        id_src.clone(),
    ));
    let apps_auth = Arc::new(sr_apps::auth::AppsAuth::new(apps_service.clone()));
    let dev_deps = Arc::new(
        routes::developer::DevDeps::new(
            (*apps_service).clone(),
            Arc::new(PgChatRepo::new(pool.clone()).with_patron_grace_days(patron_grace_days)),
            bus.clone(),
            Arc::new(routes::developer::AuthPasswordConfirm::new(
                auth_service.clone(),
            )),
            clock.clone(),
            &cfg.server.public_url,
        )
        .with_endpoint_client(Arc::new(
            crate::endpoint_client::SafeEndpointClient::production(),
        )),
    );

    let s = &cfg.storage;
    // `storage.endpoint = "memory:"` keeps every object in this process (lost on restart, presigned
    // URLs do not work). Only for local browser tests on hosts without an S3 service.
    let storage: Arc<dyn ObjectStorage> = if s.endpoint == "memory:" {
        tracing::warn!(
            "object storage is in memory (storage.endpoint = \"memory:\"): test use only"
        );
        Arc::new(ObjectStoreStorage::in_memory())
    } else {
        Arc::new(
            ObjectStoreStorage::s3(
                &s.endpoint,
                &s.bucket,
                &s.region,
                s.access_key.expose(),
                s.secret_key.expose(),
            )
            .context("configuring object storage")?,
        )
    };

    // The instance bucket is mount 1 (BYOS-INS-001). Seed its row on first start and refuse to start when it
    // names another bucket than `storage.bucket`. Services keep using `storage` below: mount 1 always
    // resolves to this very handle (`MountStorages::for_mount(1)`), so the instance path is unchanged.
    // One repo, one sealer and one cached resolver for every mount user (instance storage, the mount
    // routes, the CORS recheck job), so a route's cache invalidation is seen by the media path too.
    let mount_sealer: Arc<dyn sr_storage::mounts::Sealer> = Arc::new(
        sr_db::sealed::SecretBox::new(master_key).with_previous_keys(previous_keys.clone()),
    );
    let mount_repo: Arc<dyn sr_storage::mounts::MountRepo> =
        Arc::new(sr_db::mounts_repo::PgMountRepo::new(pool.clone()));
    let mount_resolver = Arc::new(sr_storage::mount_cache::CachedMountResolver::new(
        mount_repo.clone(),
    ));
    // The shelves (BYOS-CAT-002) and the indexer (BYOS-CAT-001) share one store type over the same pool.
    let index_store = Arc::new(sr_db::index_repo::PgIndexStore::new(
        pool.clone(),
        id_src.clone(),
    ));
    let shelf_deps = Arc::new(routes::storage::ShelfDeps {
        store: Arc::new(sr_db::shelf_repo::PgShelfStore::new(
            pool.clone(),
            id_src.clone(),
        )),
        index: index_store.clone(),
    });
    if s.endpoint != "memory:" {
        crate::instance_mount::ensure_instance_mount(mount_repo.as_ref(), mount_sealer.as_ref(), s)
            .await
            .context("seeding the instance storage mount")?;
    }
    let mount_storages = sr_storage::mount_storage::MountStorages::new(
        storage,
        mount_resolver.clone(),
        mount_sealer.clone(),
    );
    let storage: Arc<dyn ObjectStorage> = mount_storages.instance();

    // Feature crates register into the registry before it is shared (none
    // exist yet). Their routes are mounted below from `Feature::routes()`.
    let registry = Arc::new(feature_registry()?);
    let features = FeatureService::new(
        registry.clone(),
        Arc::new(PgSettingsStore::new(pool.clone(), ids.clone())),
        Some(bus.clone()),
    );
    let features_listener = features.spawn_invalidation_listener();
    // The non-secret integration policy an administrator saved wins over the config file (D-254); the
    // config value is the bootstrap. It lives in a swappable handle that the consumers read per
    // request, and a subscriber to the settings event republishes it (D-300), so a change applies
    // without a restart.
    let boot_cfg = {
        let mut c = cfg.clone();
        crate::integration_policy::overlay(&mut c, &features).await;
        c
    };
    let live_policy = crate::integration_policy::LivePolicy::new(
        crate::integration_policy::IntegrationPolicy::from_config(&boot_cfg),
    );
    let (unfurl_cfg, gif_cfg) = (boot_cfg.unfurl, boot_cfg.gif);
    let _policy_task = crate::integration_policy::spawn_subscriber(
        live_policy.clone(),
        cfg.clone(),
        features.clone(),
        bus.clone(),
    );

    // Guild endpoints: the chat service over Postgres, with its own snowflake
    // worker (process id + 1, as the message deps use) so ids never collide.
    let chat = ChatService::new(
        PgChatRepo::new(pool.clone()).with_patron_grace_days(patron_grace_days),
        clock.clone(),
        SnowflakeGenerator::new(
            clock.clone(),
            cfg.server.worker_id,
            (cfg.server.process_id + 1) % 32,
        )
        .context("chat snowflake generator")?,
        ChatLimits::default(),
    );
    // Every guild mutation is also written to the guild audit log (open question
    // 133); the retention purge runs as a recurring job below.
    let audit_ids = SnowflakeGenerator::new(
        clock.clone(),
        cfg.server.worker_id,
        (cfg.server.process_id + 3) % 32,
    )
    .context("audit snowflake generator")?;
    let settings_service = SettingsService::new(
        PgSettingsRepo::new(pool.clone()),
        clock.clone(),
        audit_ids,
        Arc::new(|| {
            sr_auth::random::bytes::<8>()
                .map(u64::from_le_bytes)
                .unwrap_or_default()
        }),
    )
    // Every stored entry also reaches the gateway as GUILD_AUDIT_LOG_ENTRY_CREATE.
    .with_audit_observer(routes::settings::audit::bus_audit_observer(bus.clone()));
    let audit_sink: Arc<dyn routes::guilds::backend::AuditSink> = Arc::new(settings_service);
    let guilds = GuildApi(routes::guilds::backend::backend_with_audit(
        chat,
        bus.clone(),
        audit_sink.clone(),
    ));

    // MessagesDeps: the message endpoints' collaborators (open questions 97-158).
    // Chat and message store over Postgres; the social hooks reopen closed DMs and
    // record mention badges (Q114, Q117) on their own snowflake worker; attachment
    // URLs are signed with the server secret (Q153).
    let social_ids = SnowflakeGenerator::new(
        clock.clone(),
        cfg.server.worker_id,
        (cfg.server.process_id + 2) % 32,
    )
    .context("social snowflake generator")?;
    let social: Arc<dyn sr_chat::message_hooks::MessageHooks> =
        Arc::new(sr_chat::social::SocialService::new(
            sr_db::social_repo::PgSocialRepo::new(pool.clone()),
            clock.clone(),
            social_ids,
            sr_chat::social::SocialLimits::default(),
        ));
    // Threads: their own snowflake worker (process id + 5). The activity hook keeps
    // counters, reopens archived threads and auto-joins posters; it runs after the
    // social hooks on every stored message.
    let threads_deps = Arc::new(
        routes::threads::ThreadsDeps::new(
            sr_chat::threads::ThreadService::new(
                PgChatRepo::new(pool.clone()).with_patron_grace_days(patron_grace_days),
                clock.clone(),
                SnowflakeGenerator::new(
                    clock.clone(),
                    cfg.server.worker_id,
                    (cfg.server.process_id + 5) % 32,
                )
                .context("thread snowflake generator")?,
            ),
            bus.clone(),
        )
        .with_audit(audit_sink.clone()),
    );
    // The feature modules: leveling hears every stored message; the poster they speak through is filled in
    // once the message stack below exists.
    let user_features =
        sr_userfeat::UserFeatures::new(pool.clone(), features.clone(), id_src.clone());
    let feature_poster: crate::userfeat::PosterCell = Arc::default();
    let message_hooks: Arc<dyn sr_chat::message_hooks::MessageHooks> =
        Arc::new(sr_chat::threads::hooks::HookChain(vec![
            social,
            threads_deps.activity_hook(),
            Arc::new(crate::userfeat::LevelingHook {
                uf: user_features.clone(),
                poster: feature_poster.clone(),
            }),
        ]));
    let cdn_signer =
        sr_storage::cdn::CdnSigner::new(secret_key(cfg.server.secret_key.expose())?.to_vec());
    let messages = routes::messages::MessagesDeps::new(
        PgChatRepo::new(pool.clone()).with_patron_grace_days(patron_grace_days),
        Arc::new(sr_db::chat_repo::PgMessageStore::new(pool.clone())),
        storage.clone(),
        bus.clone(),
        Arc::new(routes::messages::FixedEntitlements(
            sr_storage::upload::SizePolicy::default().standard_limit,
        )),
        Arc::new(PgChatRepo::new(pool.clone()).with_patron_grace_days(patron_grace_days)),
        Some(message_hooks),
        clock.clone(),
        cfg.server.worker_id,
        cfg.server.process_id,
    )
    .context("message snowflake generator")?
    .with_cdn_signer(cdn_signer.clone())
    .with_audit(audit_sink.clone())
    // Age-restricted channels need the 18+ answer (D-032, LNG-CHN-006).
    .with_age_gate(Arc::new(crate::settings_pg::PgAgeGate::new(pool.clone())));
    // Rich messages: polls, stickers and link previews (LNG-MSG-010, LNG-MSG-014).
    if unfurl_cfg.allow_private_for_tests {
        tracing::warn!(
            "unfurl.allow_private_for_tests is ON: link previews may fetch loopback and private \n             addresses. This is for tests only and lets any user probe the internal network."
        );
    }
    let unfurl_policy = sr_unfurl::Policy {
        deny_hosts: unfurl_cfg.deny_hosts.clone(),
        allow_hosts: unfurl_cfg.allow_hosts.clone(),
        allow_private_for_tests: unfurl_cfg.allow_private_for_tests,
        live_hosts: Some(live_policy.hosts.clone()),
        ..sr_unfurl::Policy::default()
    };
    let unfurl_policy_for_feeds = unfurl_policy.clone();
    // Member buckets are reached only at addresses that were resolved and checked once, just before the call
    // (the DNS-rebind gap of wave 27).
    let mount_guard: Arc<dyn sr_storage::guard::EndpointGuard> = Arc::new(
        routes::storage::NetGuard::new(unfurl_policy_for_feeds.clone()),
    );
    // Bucket mounts, read side (BYOS-SCP-001/002): scoped listing and presign over the same repo and sealer.
    let scope_facts = Arc::new(sr_db::scopes_repo::PgScopeFacts::new(pool.clone()));
    let storage_scope_deps = Arc::new(routes::storage::ScopeDeps {
        // Uncached on purpose: a disabled mount or a cleared CORS flag (the recheck job, sr-cli, a direct
        // write) must stop presigning at once, not after the 60 s cache.
        mounts: mount_repo.clone(),
        store: scope_facts.clone(),
        sealer: mount_sealer.clone(),
        guard: Some(mount_guard.clone()),
    });
    // Browser uploads and object changes in a mount (BYOS-BRW-001): the ledger, the signed bucket calls, ids.
    let upload_io: Arc<dyn sr_storage::uploads::MountIo> = Arc::new(
        sr_storage::uploads::S3MountIo::new(mount_sealer.clone()).with_guard(mount_guard.clone()),
    );
    let upload_ledger: Arc<dyn sr_storage::uploads::UploadLedger> =
        Arc::new(sr_db::mount_uploads_repo::PgUploadLedger::new(pool.clone()));
    let upload_deps = Arc::new(routes::storage::UploadDeps {
        ledger: upload_ledger.clone(),
        io: upload_io.clone(),
        next_id: {
            let ids = ids.clone();
            Arc::new(move || ids.generate().get())
        },
    });
    let unfurler = Arc::new(sr_unfurl::production(
        unfurl_policy.clone(),
        unfurl_cfg.per_host_per_minute.max(1),
    ));
    live_policy.attach_unfurler(unfurler.clone());
    // The image proxy (D-176) fetches under the same SSRF policy and deny list.
    let external_images = Arc::new(routes::cdn::external::ExternalImageDeps::new(
        sr_unfurl::production_fetcher(unfurl_policy),
        storage.clone(),
        cdn_signer.clone(),
    ));
    let job_ids: crate::search_jobs::JobIds = {
        let ids = ids.clone();
        Arc::new(move || ids.generate().get() as i64)
    };
    let messages = messages.with_rich(routes::messages::RichDeps {
        store: Some(Arc::new(sr_db::rich_repo::PgRichStore::new(pool.clone()))),
        scheduler: Some(Arc::new(crate::rich_jobs::PgJobScheduler {
            pool: pool.clone(),
            ids: job_ids,
        })),
        unfurler: Some(unfurler),
        gate: Some(Arc::new(crate::rich_jobs::FeatureUnfurlGate(
            features.clone(),
        ))),
    });

    // Server settings and message search (open questions 131-138). The settings
    // service runs over Postgres on its own snowflake worker (process id + 3) and
    // gets invite/webhook entropy from the OS. Its events go to the bus, and the
    // audit entries it stores are announced on the gateway. The search index lives
    // in `[search] dir` (default `data/search`).
    let gateway_presence = Arc::new(crate::gateway_pg::GatewayPresence::default());
    // Joining by invite, vanity code or the directory; also drops temporary memberships
    // once their member has been offline for a minute (LNG-INV-005).
    let member_joiner = Arc::new(routes::settings::InviteMemberJoiner::new(
        Arc::new(sr_db::invite_join::PgInviteJoinRepo::new(pool.clone())),
        Arc::new(PgChatRepo::new(pool.clone()).with_patron_grace_days(patron_grace_days)),
        bus.clone(),
    ));
    let temporary_members = Arc::new(crate::gateway_pg::TemporaryMembers::new(
        member_joiner.clone(),
        std::time::Duration::from_secs(60),
    ));
    let settings_deps = Arc::new(
        SettingsDeps::new(
            Arc::new(PgSettingsRepo::new(pool.clone())),
            Arc::new(PgChatRepo::new(pool.clone()).with_patron_grace_days(patron_grace_days)),
            storage.clone(),
            clock.clone(),
            Arc::new(crate::settings_pg::secure_entropy),
            cfg.server.worker_id,
            (cfg.server.process_id + 3) % 32,
        )
        .context("settings snowflake generator")?
        .with_events(Arc::new(BusSettingsEvents::new(bus.clone())))
        .with_joiner(member_joiner.clone())
        .with_presence(gateway_presence.clone())
        .with_timeouter(Arc::new(routes::settings::AdminMemberTimeouter::new(
            Arc::new(PgChatRepo::new(pool.clone()).with_patron_grace_days(patron_grace_days)),
            bus.clone(),
        )))
        .with_audit_observer(routes::settings::audit::bus_audit_observer(bus.clone())),
    );
    let events_deps = settings_deps.clone();
    let search_dir = search_dir(&cfg.search.dir, std::env::var("SR_SEARCH_DIR").ok());
    let message_index = Arc::new(
        MessageIndex::open_in_dir(std::path::Path::new(&search_dir))
            .context("opening the search index ([search] dir)")?,
    );
    let search_deps = Arc::new(
        SearchDeps::new(message_index.clone())
            .with_age_gate(Arc::new(crate::settings_pg::PgAgeGate::new(pool.clone()))),
    );
    // The Library catalog (LIB-DATA-*): Postgres repository, signed page/cover URLs (the same
    // CDN signer as attachments), and a series search index in `<search dir>/library`,
    // rebuilt from the database at startup so it can never drift across restarts.
    let library_index = Arc::new(
        sr_library::SeriesIndex::open_in_dir(&std::path::Path::new(&search_dir).join("library"))
            .context("opening the library search index")?,
    );
    let library_service = sr_library::LibraryService::new(
        Arc::new(sr_db::library_repo::PgLibraryRepo::new(pool.clone())),
        cdn_signer.clone(),
        clock.clone(),
        id_src.clone(),
    )
    .with_index(library_index)
    .with_cover_hashes(Arc::new(sr_db::library_repo::PgLibraryRepo::new(
        pool.clone(),
    )))
    .with_events(Arc::new(routes::library::BusLibraryEvents::new(
        bus.clone(),
    )))
    .with_ingest(
        storage.clone(),
        Arc::new(crate::library_jobs::PgIngestSettings::new(pool.clone())),
        Arc::new(crate::library_jobs::PgIngestQueue::new(pool.clone())),
    )
    .with_audit(Arc::new(crate::library_jobs::AdminLibraryAudit::new(
        pool.clone(),
        id_src.clone(),
    )));
    // Resumable multipart uploads of chapter files (REV-UP-001) need part-level presigning, which
    // only the S3 API offers; with `SR_MEDIA_SINGLE_PUT=1` they are off and slots are one PUT.
    let library_service = if std::env::var("SR_MEDIA_SINGLE_PUT").is_ok_and(|v| v == "1") {
        library_service
    } else {
        let s3 = sr_storage::s3multipart::S3Multipart::new(
            &s.endpoint,
            &s.bucket,
            &s.region,
            s.access_key.expose(),
            s.secret_key.expose(),
        )
        .context("configuring library multipart uploads")?;
        library_service.with_multipart(sr_library::MultipartWiring::new(Arc::new(s3)))
    };
    let indexed = library_service
        .reindex_all()
        .await
        .map_err(|e| anyhow::anyhow!("indexing the library catalog: {e}"))?;
    tracing::info!(series = indexed, "library search index rebuilt");
    let library_deps = routes::library::LibraryDeps::new(library_service.clone())
        .with_age_gate(Arc::new(crate::settings_pg::PgAgeGate::new(pool.clone())))
        .with_prefs(Arc::new(crate::settings_pg::PgLibraryPrefs::new(
            pool.clone(),
        )))
        .with_social(
            sr_library_social::LibrarySocial::new(pool.clone(), id_src.clone())
                .with_trending_cache(
                    std::time::Duration::from_secs(3600),
                    std::time::Duration::from_secs(600),
                )
                .with_events(Arc::new(routes::library::BusLibraryEvents::new(
                    bus.clone(),
                ))),
        );

    // The Cinema catalog (CIN-DATA-*): Postgres repository, signed artwork URLs (the same CDN
    // signer), and a title search index in `<search dir>/cinema`, rebuilt from the database at
    // startup. Row providers for user-data rows (continue watching, trending, ...) are added to
    // this service by the social work (`CinemaService::with_provider`).
    let cinema_index = Arc::new(
        sr_cinema::TitleIndex::open_in_dir(&std::path::Path::new(&search_dir).join("cinema"))
            .context("opening the cinema search index")?,
    );
    let cinema_repo = Arc::new(sr_db::cinema_repo::PgCinemaRepo::new(pool.clone()));
    // The social service resolves titles and episodes through the same repository; it feeds the
    // user-data rows of the catalog (continue watching, trending, ...) and follows merges and deletions.
    let cinema_social_service = sr_cinema_social::CinemaService::new(
        pool.clone(),
        Arc::new(crate::cinema_catalog::RepoCatalog::new(
            cinema_repo.clone(),
            cdn_signer.clone(),
            clock.clone(),
        )),
        id_src.clone(),
    )
    .with_events(Arc::new(sr_cinema_social::BusEvents::new(bus.clone())));
    let mut cinema_service = sr_cinema::CinemaService::new(
        cinema_repo.clone(),
        cdn_signer.clone(),
        clock.clone(),
        id_src.clone(),
    )
    .with_index(cinema_index)
    .with_storage(storage.clone())
    .with_observer(Arc::new(sr_cinema_social::SocialObserver::new(
        pool.clone(),
    )));
    for provider in sr_cinema_social::row_providers(&cinema_social_service) {
        cinema_service = cinema_service.with_provider(provider);
    }
    let cinema_titles = cinema_service
        .reindex_all()
        .await
        .map_err(|e| anyhow::anyhow!("indexing the cinema catalog: {e}"))?;
    tracing::info!(titles = cinema_titles, "cinema search index rebuilt");
    // Playback (spec section 10): renditions come from a `PlaybackSource`. The Postgres source over the
    // ingest tables (`video_assets` / `renditions`) is plugged in here when those tables exist; until then the
    // source is empty, so the playback routes answer 404 for every asset.
    let media_repo = Arc::new(sr_db::media_repo::PgMediaRepo::new(
        pool.clone(),
        id_src.clone(),
    ));
    let playback_deps = Arc::new(routes::cinema::PlaybackDeps::new(
        Arc::new(crate::media_jobs::PgPlaybackSource::new(media_repo.clone())),
        Arc::new(cdn_signer.clone()),
        clock.clone(),
    ));
    let cinema_deps = Arc::new(
        routes::cinema::CinemaDeps::new(cinema_service.clone())
            .with_playback(playback_deps.clone())
            .with_age_gate(Arc::new(crate::settings_pg::PgAgeGate::new(pool.clone())))
            .with_prefs(Arc::new(crate::settings_pg::PgCinemaPrefs::new(
                pool.clone(),
            ))),
    );

    // Live TV (TV-ENG, TV-HLS): the schedule is stored in Postgres, pools come from the catalog repository,
    // and the live playlists are cut from the same renditions the VOD player uses (`/cdn/hls/<asset>/...`).
    // The slate is a transcoded loop (TV-HLS-003, `sr-cli tv slate`, D-269): the platform setting `tv.slate_asset`
    // first, `SR_TV_SLATE_ASSET` as the bootstrap fallback; a change applies without a restart.
    let tv_slate_env = std::env::var("SR_TV_SLATE_ASSET")
        .ok()
        .and_then(|v| crate::tv_catalog::parse_asset_id(&v));
    let tv_slate: Arc<dyn sr_tv::SlateSource> = Arc::new(crate::tv_catalog::PgSlateSetting::new(
        pool.clone(),
        tv_slate_env,
    ));
    let tv_service = sr_tv::TvService::new(
        Arc::new(sr_tv::pg::PgTvRepo::new(pool.clone())),
        Arc::new(
            crate::tv_catalog::RepoTvCatalog::new(cinema_repo)
                .with_asset_durations(Arc::new(crate::tv_catalog::PgAssetDurations::new(
                    pool.clone(),
                )))
                .with_shelves(crate::tv_catalog::ShelfMounts::new(
                    mount_repo.clone(),
                    scope_facts.clone(),
                )),
        ),
        clock.clone(),
        id_src.clone(),
    )
    .with_live(sr_tv::LiveDeps {
        source: Arc::new(crate::media_jobs::PgPlaybackSource::new(media_repo.clone())),
        signer: Arc::new(cdn_signer.clone()),
    })
    .with_slate_source(tv_slate.clone())
    .with_access(Arc::new(crate::tv_catalog::GuildChannelAccess::new(
        scope_facts.clone(),
    )));
    let tv_deps = Arc::new(
        routes::tv::TvDeps::new(tv_service)
            .with_age_gate(Arc::new(crate::settings_pg::PgAgeGate::new(pool.clone()))),
    );
    let tv_deps_for_reminders = tv_deps.clone();

    // Cinema media (CIN-ING-004/010, CIN-DL-001): the Postgres job store behind the Worker API
    // (`/api/v1/transcode/worker`, bearer worker tokens), the staff upload and transcoding admin,
    // Patron downloads, and the Postgres playback source behind `/cdn/hls`. Outputs go to the bucket with
    // presigned multipart uploads (`SR_MEDIA_SINGLE_PUT=1` falls back to one presigned PUT per
    // output for storage that cannot presign parts). `ffprobe` is read from `SR_FFPROBE`.
    let secs_clock: sr_media::service::Clock = Arc::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    });
    let single_put = std::env::var("SR_MEDIA_SINGLE_PUT").is_ok_and(|v| v == "1");
    let (media_outputs, worker_config): (Arc<dyn sr_media::worker_api::OutputStorage>, _) =
        if single_put {
            tracing::warn!(
                "SR_MEDIA_SINGLE_PUT is on: transcode outputs and originals upload as single presigned PUTs"
            );
            (
                Arc::new(sr_media::output_storage::SinglePutStorage::new(
                    storage.clone(),
                )),
                sr_media::worker_api::ServiceConfig::default().single_part(),
            )
        } else {
            let s3 = sr_storage::s3multipart::S3Multipart::new(
                &s.endpoint,
                &s.bucket,
                &s.region,
                s.access_key.expose(),
                s.secret_key.expose(),
            )
            .context("configuring multipart uploads")?;
            (
                Arc::new(sr_media::output_storage::S3OutputStorage::new(s3)),
                sr_media::worker_api::ServiceConfig::default(),
            )
        };
    let media_service = sr_media::service::MediaService::new(
        media_repo.clone(),
        storage.clone(),
        media_outputs.clone(),
        Arc::new(sr_media::runner::ProcessRunner),
        Arc::new(crate::media_jobs::PgMediaSettings::new(pool.clone())),
        Arc::new(crate::media_jobs::PgProbeQueue::new(
            pool.clone(),
            id_src.clone(),
        )),
        id_src.clone(),
        secs_clock.clone(),
        Arc::new(|| sr_auth::random::bytes::<32>().ok()),
    );
    let media_service = if single_put {
        media_service.with_part_size(u64::MAX / 4)
    } else {
        media_service
    };
    // Donations (DON-*): watch-only Bitcoin payments, entitlements, and the Patron download perk
    // on top of them (the legacy `cinema_download_grants` fallback is gone, migration 0871).
    let payments = crate::payments_jobs::build(&pool, id_src.clone(), bus.clone()).await;
    let download_entitlements: Arc<dyn sr_media::downloads::DownloadEntitlements> = Arc::new(
        sr_db::media_repo::EntitlementDownloads::new(payments.entitlements.clone()),
    );
    let download_service = Arc::new(sr_media::downloads::DownloadService::new(
        media_repo.clone(),
        media_repo.clone(),
        download_entitlements,
        Arc::new(cdn_signer.clone()),
        storage.clone(),
        id_src.clone(),
        secs_clock.clone(),
    ));
    let media_service_shared = Arc::new(media_service.clone());
    let media_deps = Arc::new(
        routes::cinema_media::MediaDeps::new(media_service, download_service.clone())
            .with_slate_source(tv_slate.clone()),
    );
    let worker_routes = with_request_id(sr_media::http::router(Arc::new(
        sr_media::worker_api::WorkerService {
            store: media_repo.clone(),
            outputs: media_outputs,
            downloads: Arc::new(cdn_signer.clone()),
            clock: secs_clock,
            config: worker_config,
        },
    )));

    // Cinema social and progress (CIN-PLY-007/009, CIN-MY-002, CIN-SOC-001/002): the service built above.
    let cinema_social_deps = Arc::new(
        routes::cinema_social::CinemaSocialDeps::new(cinema_social_service.clone())
            .with_age_gate(Arc::new(crate::settings_pg::PgAgeGate::new(pool.clone()))),
    );

    // Watch Parties (WP-BE-*): the catalog play gate, channel permissions from the chat repository,
    // gateway events on the bus. The timer worker settles buffering waits; the sweep job covers the rest.
    let watchparty_service =
        sr_watchparty::PartyService::new(pool.clone(), clock.clone(), id_src.clone())
            .with_play_gate(Arc::new(crate::watchparty::CatalogAndLiveGate::new(
                Arc::new(crate::watchparty::CinemaPlayGate::new(
                    cinema_service.clone(),
                )),
                tv_deps.service.clone(),
            )))
            .with_channels(Arc::new(crate::watchparty::ChatChannelAccess::new(
                PgChatRepo::new(pool.clone()).with_patron_grace_days(patron_grace_days),
                clock.clone(),
            )))
            .with_limits(Arc::new(crate::watchparty::EntitlementLimits::new(
                payments.entitlements.clone(),
            )))
            .with_events(Arc::new(sr_watchparty::BusEvents::new(bus.clone())))
            .with_chat(Arc::new(sr_watchparty::ChatHooks(vec![
                Arc::new(crate::userfeat::PartyAnnouncer {
                    uf: user_features.clone(),
                    poster: feature_poster.clone(),
                }),
                // The live card in the bound channel (WP-BE-008).
                Arc::new(
                    crate::party_cards::PartyCards::new(pool.clone(), payments.chat.clone())
                        .with_features(user_features.clone()),
                ),
                // CHANNEL_UPDATE when a room stops being one.
                Arc::new(crate::party_rooms::RoomFlagHook::new(
                    PgChatRepo::new(pool.clone()),
                    bus.clone(),
                )),
            ])));
    watchparty_service.spawn_timers();
    let watchparty_deps = Arc::new(routes::watchparty::WatchPartyDeps::new(
        watchparty_service.clone(),
    ));

    // Activities (DEV-ACT): settings and review, instances tied to voice channels, and the proxy for
    // `<app-id>.activities.<host>`. Everything but the owner's settings is off unless `[activities] enabled`.
    let activities_deps = Arc::new(routes::activities::ActivitiesDeps::new(
        sr_activities::ActivityService::new(pool.clone(), id_src.clone())
            .with_channels(Arc::new(crate::activities::ChatActivityChannels::new(
                PgChatRepo::new(pool.clone()).with_patron_grace_days(patron_grace_days),
                clock.clone(),
            )))
            .with_events(Arc::new(sr_activities::BusEvents::new(bus.clone())))
            .with_storage(storage.clone()),
        cfg.activities.enabled,
        &cfg.server.public_url,
        Some(Arc::new(
            crate::activities::SafeActivityFetcher::production(),
        )),
    ));
    // Declarative packages and customised profiles (EXT-PKG, EXT-PRF).
    let packages_deps = Arc::new(routes::packages::PackagesDeps {
        service: sr_packages::service::PackageService::new(pool.clone(), id_src.clone())
            .with_storage(storage.clone())
            .with_events(Arc::new(routes::packages::BusPackageEvents::new(
                bus.clone(),
            ))),
    });

    // J-16: participants follow the voice call (a leave from the channel removes them from the Activity).
    if cfg.activities.enabled {
        sr_activities::VoiceFollower::new(
            activities_deps.service.clone(),
            sr_activities::voice::DEFAULT_DEBOUNCE,
        )
        .spawn(bus.clone());
    }

    // The review pipeline (REV-*): submissions of the Library kinds, reviewed by staff with
    // REVIEW_READING and published through the same Library service. The submitter hears
    // about every step as SR_SUBMISSION_UPDATE.
    let review_dm = routes::review::DmSlot::default();
    let review_deps = Arc::new(
        routes::review::ReviewDeps::new(
            sr_review::ReviewService::new(
                Arc::new(sr_db::review_repo::PgReviewRepo::new(pool.clone())),
                sr_review::KindRegistry::new()
                    .with(Arc::new(sr_library::submissions::LibraryKind::series(
                        library_service.clone(),
                    )))
                    .with(Arc::new(sr_library::submissions::LibraryKind::items(
                        library_service.clone(),
                    )))
                    .with(Arc::new(sr_cinema::submissions::CinemaKind::title(
                        cinema_service.clone(),
                        media_repo.clone(),
                    )))
                    .with(Arc::new(sr_cinema::submissions::CinemaKind::episodes(
                        cinema_service.clone(),
                        media_repo.clone(),
                    ))),
                clock.clone(),
                id_src.clone(),
            )
            .with_events(Arc::new(
                routes::review::BusReviewEvents::new(bus.clone()).with_dm_slot(review_dm.clone()),
            ))
            .with_allowance(Arc::new(crate::review_jobs::PlatformAllowance::new(
                pool.clone(),
            ))),
        )
        .with_library(sr_library::submissions::LibraryDraftOps::new(
            library_service.clone(),
        ))
        .with_media(routes::review::ReviewMedia::new(
            media_repo.clone(),
            cdn_signer.clone(),
        )),
    );

    // Member video uploads of Cinema submissions (RDY-MED-001): the draft operations, the upload
    // sessions in Postgres and, with the S3 API, resumable parts (the same condition as the media
    // originals above).
    let cinema_submit_deps = {
        let d = routes::submissions::CinemaSubmitDeps::new(
            sr_cinema::drafts::DraftOps::new(cinema_service.clone(), media_repo.clone()),
            Arc::new(sr_db::cinema_uploads::PgUploadSessions::new(pool.clone())),
        );
        if single_put || s.endpoint == "memory:" {
            d
        } else {
            let s3 = sr_storage::s3multipart::S3Multipart::new(
                &s.endpoint,
                &s.bucket,
                &s.region,
                s.access_key.expose(),
                s.secret_key.expose(),
            )
            .context("configuring cinema submission uploads")?;
            d.with_parts(Arc::new(s3))
        }
    };

    // Watched folders and storage imports (REV-UP-005/006): draft submissions of staff-owned
    // sources. Local folders are limited to the roots in `[ingest] roots`.
    let source_scanner = sr_library::sources::SourceScanner::new(
        Arc::new(sr_db::library_repo::PgLibraryRepo::new(pool.clone())),
        Arc::new(sr_library::sources::FsAndStorageFiles::new(
            cfg.ingest
                .roots
                .iter()
                .map(std::path::PathBuf::from)
                .collect(),
            storage.clone(),
        )),
        library_service.clone(),
        review_deps.service.clone(),
        clock.clone(),
    );
    // Index jobs for every message event, DMs included, and search cache
    // invalidation, from one firehose subscription (see `search_jobs`).
    let search_consumer = {
        let ids = ids.clone();
        crate::search_jobs::spawn_firehose_consumer(
            lag_bus.tap(),
            pool.clone(),
            Arc::new(move || ids.generate().get() as i64),
            search_deps.clone(),
        )
    };
    let indexer = Arc::new(crate::search_jobs::Indexer {
        index: message_index,
        repo: Arc::new(PgChatRepo::new(pool.clone()).with_patron_grace_days(patron_grace_days)),
        store: Arc::new(sr_db::chat_repo::PgMessageStore::new(pool.clone())),
    });

    // Voice (off by default): the embedded SFU and the gateway's token/relay wiring.
    let voice_setup = if cfg.voice.enabled {
        Some(crate::voice::start(&cfg, &master_key).await?)
    } else {
        None
    };
    let call_recorder = Arc::new(crate::voice::CallRecorder::default());
    let gateway = Gateway::new(
        GatewayConfig {
            // Cookie-authenticated web sessions may IDENTIFY from the public origin only.
            allowed_origins: vec![cfg.server.public_url.trim_end_matches('/').to_owned()],
            // READY always names a resume URL, as Discord's does: typed libraries (serenity's `Ready`) refuse a
            // READY whose `resume_gateway_url` is null, and never get past it.
            resume_gateway_url: Some(gateway_ws_url(&cfg.server.public_url)),
            voice: voice_setup.as_ref().map(|v| {
                let mut g = v.gateway.clone();
                g.calls = Some(call_recorder.clone());
                g
            }),
            ..GatewayConfig::default()
        },
        Arc::new(
            PgAuthenticator::new(auth_repo.clone(), pool.clone(), clock.clone())
                .with_apps(apps_service.clone()),
        ),
        Arc::new(
            PgGuildDirectory::new(pool.clone(), clock.clone())
                .with_apps(apps_service.clone())
                .with_temporary_members(temporary_members.clone())
                .with_entitlements(payments.entitlements.clone()),
        ),
        bus.clone(),
    );
    gateway.start();
    // The event clock reads voice occupancy and stage instances from the gateway (G-91).
    settings_deps.set_event_world(Arc::new(crate::events_jobs::GatewayEventWorld(
        gateway.clone(),
        bus.is_single_node(),
    )));
    gateway_presence.attach(gateway.clone());
    temporary_members.attach(gateway.clone());
    temporary_members.schedule_startup_sweep(pool.clone());

    // Social endpoints (relationships, DMs, acks, notification settings, profiles):
    // their own service on its own snowflake worker (process id + 3).
    let social_api = routes::social::SocialDeps::new(
        routes::social::DynRepo::new(sr_db::social_repo::PgSocialRepo::new(pool.clone())),
        Arc::new(PgChatRepo::new(pool.clone()).with_patron_grace_days(patron_grace_days)),
        bus.clone(),
        clock.clone(),
        sr_chat::social::SocialLimits::default(),
        cfg.server.worker_id,
        (cfg.server.process_id + 3) % 32,
    )
    .context("social api snowflake generator")?;
    // The first message of a DM (or the first after Close DM) announces the channel to
    // each recipient before the message itself.
    let social_api = Arc::new(social_api);
    // Account connections (LNG-USET-024) and the data export (LNG-USET-020). Every request to
    // a provider goes through the SSRF-safe client; providers without an OAuth application
    // in `[connections]` are simply not offered.
    let provider_client = sr_net::SafeClient::production(
        sr_net::Policy::default(),
        &format!(
            "SIDEROOM/1.0 (+{})",
            cfg.server.public_url.trim_end_matches('/')
        ),
    );
    let connections_deps = Arc::new(routes::userset::connections::ConnectionsDeps::new(
        &cfg.server.public_url,
        &cfg.connections,
        (
            cfg.discord.client_id.as_str(),
            cfg.discord.client_secret.expose(),
        ),
        provider_client.clone(),
        clock.clone(),
    ));
    let export_deps = Arc::new(routes::userset::data_export::DataExportDeps {
        store: Arc::new(crate::userset_jobs::PgExportStore::new(
            pool.clone(),
            id_src.clone(),
        )),
        storage: storage.clone(),
        clock: clock.clone(),
    });
    // Large chat attachments go up as resumable multipart uploads (LNG-CMP-007); like the library's,
    // they need the S3 API, so a memory store or SR_MEDIA_SINGLE_PUT=1 keeps single PUTs.
    let messages = if s.endpoint == "memory:"
        || std::env::var("SR_MEDIA_SINGLE_PUT").is_ok_and(|v| v == "1")
    {
        messages
    } else {
        let s3 = sr_storage::s3multipart::S3Multipart::new(
            &s.endpoint,
            &s.bucket,
            &s.region,
            s.access_key.expose(),
            s.secret_key.expose(),
        )
        .context("configuring chat multipart uploads")?;
        messages.with_multipart(sr_library::MultipartWiring::new(Arc::new(s3)))
    };
    // D-238: the upload limit follows the donations runtime (Patron perk, server level).
    let messages = messages.with_upload_entitlements(Arc::new(
        crate::upload_limits::PaymentsUploadLimits::new(
            payments.entitlements.clone(),
            sr_storage::upload::SizePolicy::default().standard_limit,
        ),
    ));
    let messages = Arc::new(
        messages
            .with_dm_announcer(social_api.clone())
            .with_viewer_safety(social_api.clone()),
    );
    // Server-level payments post their "supported this server" notice through the chat module.
    let _ = payments.chat.set(messages.clone());

    // Web Push (LNG-NTF-003): the VAPID key lives in the database sealed with the server
    // secret; without a configured secret it is per-boot (nothing could open it later).
    let push_store: Arc<dyn sr_push::PushStore> =
        Arc::new(crate::push_pg::PgPushStore::new(pool.clone()));
    let vapid = if cfg.server.secret_key.expose().is_empty() {
        sr_push::VapidKey::generate().map_err(|e| anyhow::anyhow!("{e}"))?
    } else {
        sr_push::load_or_create_vapid(push_store.as_ref(), &master_key)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?
    };
    let push_deps = Arc::new(routes::push::PushDeps::new(
        push_store.clone(),
        vapid.public_key(),
        {
            let ids = ids.clone();
            Arc::new(move || ids.generate().get())
        },
    ));

    // BYOS storage mounts (BYOS-MNT-001): the member's and the server's own buckets, credentials sealed
    // with the server secret.
    // BYOS-NAV-001: `STORAGE_MOUNT_UPDATE` for the nav's Auto mode.
    let storage_events = Arc::new(routes::storage::events::StorageEvents::new(bus.clone()));
    let mounts_deps = Arc::new(
        routes::storage::mounts::MountsDeps::new(
            mount_repo.clone(),
            mount_sealer.clone(),
            {
                let ids = ids.clone();
                Arc::new(move || ids.generate().get())
            },
            unfurl_policy_for_feeds.clone(),
        )
        .with_public_origin(&cfg.server.public_url)
        .with_resolver(mount_resolver.clone())
        .with_facts(scope_facts.clone())
        .with_standing(scope_facts.clone())
        .with_events(storage_events.clone()),
    );

    // GIF picker (LNG-EXP-005): off unless `[gif]` names a provider and key.
    let gif_source: Option<Arc<dyn routes::gifs::GifSource>> = if gif_cfg.enabled() {
        Some(Arc::new(
            routes::gifs::TenorSource::new(
                gif_cfg.api_key.expose(),
                &gif_cfg.client_key,
                &gif_cfg.country,
                &gif_cfg.locale,
            )
            .with_live_region(live_policy.gif.clone()),
        ))
    } else {
        if !gif_cfg.provider.is_empty() {
            tracing::warn!(
                "gif.provider is set but unsupported or missing gif.api_key; the GIF picker stays off"
            );
        }
        None
    };
    let gif_deps = Arc::new(routes::gifs::GifDeps::new(
        gif_source,
        Some(cdn_signer.clone()),
    ));
    // Metadata lookup for the submission editor (REV-ED-005): off unless `[lookup]` enables a
    // provider and the platform setting `review.lookup.enabled` is on.
    let lookup_deps = Arc::new(
        routes::review::LookupDeps::new(
            crate::lookup_wiring::all_providers_from(&cfg.lookup),
            Arc::new(crate::lookup_wiring::PgLookupSwitch::new(pool.clone())),
            Some(cdn_signer.clone()),
        )
        .with_live_enabled(live_policy.lookup.clone()),
    );
    let push_subject = if cfg.server.public_url.starts_with("https://") {
        cfg.server.public_url.trim_end_matches('/').to_owned()
    } else {
        "mailto:admin@localhost.invalid".to_owned()
    };
    let push_sender: Arc<dyn sr_push::PushSender> = Arc::new(sr_push::WebPushClient::new(
        vapid,
        push_subject,
        sr_net::Policy::default(),
    ));
    // Interested members hear when a scheduled event starts (G-93).
    settings_deps.set_event_notifier(Arc::new(crate::events_jobs::EventStartPush {
        bus: bus.clone(),
        social: social_api.clone(),
        chat: Arc::new(PgChatRepo::new(pool.clone()).with_patron_grace_days(patron_grace_days)),
        store: push_store.clone(),
        sender: push_sender.clone(),
        activity: Some(Arc::new(gateway.clone())),
    }));
    let events_voice_listener =
        crate::events_jobs::spawn_voice_listener(lag_bus.tap(), settings_deps.clone());
    let push_consumer = crate::push_notify::spawn(
        lag_bus.tap(),
        Arc::new(crate::push_notify::PushNotifier {
            messages: messages.clone(),
            social: social_api.clone(),
            store: push_store,
            sender: push_sender,
            pool: pool.clone(),
            activity: Some(Arc::new(gateway.clone())),
        }),
    );
    call_recorder.bind(messages.clone());

    // The feature modules speak as the system user (and send scheduled messages as their authors) through the
    // message stack; the scheduler job dispatches what is due.
    let system_poster: Arc<dyn sr_userfeat::Poster> = Arc::new(crate::userfeat::SystemPoster::new(
        pool.clone(),
        messages.clone(),
        guilds.0.clone(),
        Some(settings_deps.clone()),
        &cfg.server.public_url,
    ));
    if feature_poster.set(system_poster.clone()).is_err() {
        tracing::warn!("the feature poster was already set");
    }
    let system_poster_for_tv = system_poster.clone();
    let feed_policy = unfurl_policy_for_feeds.clone();
    let ticker = Arc::new(sr_userfeat::tick::Ticker::new(
        user_features.clone(),
        system_poster.clone(),
        Some(crate::userfeat::feed_client(feed_policy.clone())),
    ));
    let user_feature_deps = Arc::new(routes::userfeat::UserFeatDeps {
        uf: user_features.clone(),
        poster: system_poster,
        net_policy: feed_policy,
    });

    // Platform admin API: permissions are resolved per request from the Staff Server; every
    // action is written to the hash-chained access log. Feature toggles go through the shared
    // feature service and the System page reports the live gateway session count.
    // The system user speaks through the message stack: appointment DMs, announcement DMs and
    // the cards of the Staff Server feeds (ADM-STF-007, ADM-ALR-001/002/003).
    let admin_repo = Arc::new(sr_db::admin_repo::PgAdminRepo::new(
        pool.clone(),
        id_src.clone(),
    ));
    let staff_voice = Arc::new(crate::staff_feed::StaffVoice::new(
        pool.clone(),
        messages.clone(),
        &cfg.server.public_url,
    ));
    // Tester invitations and endpoint warnings reach developers as system DMs.
    dev_deps.set_system_dm(staff_voice.clone());
    let self_delete_voice: Arc<dyn sr_admin::StaffNotifier> = staff_voice.clone();
    // Review decisions reach the submitter as a system DM too (REV-CORE-007).
    let _ = review_dm.set(staff_voice.clone());
    let admin_service = sr_admin::AdminService::new(admin_repo.clone())
        .with_extras(admin_repo.clone())
        .with_data_requests(admin_repo)
        .with_notifier(staff_voice.clone())
        .with_bus(bus.clone());
    // Reports, cases, holds and appeals (Phase 7, step 1); reporting is behind the `moderation.reports` flag.
    // Actions from a case run through the guild, message and account services (N-21).
    // The takedown register (step 2) reads the strike policy from the compliance module and blocks media.
    let media_blocklist = Arc::new(sr_moderation::blocklist::MediaBlocklist::new(pool.clone()));
    let moderation = sr_moderation::Moderation::new(pool.clone(), id_src.clone())
        .with_notifier(staff_voice.clone())
        .with_guild_audit(Arc::new(crate::moderation_audit::GuildAuditAdapter(
            audit_sink.clone(),
        )))
        .with_case_actions(Arc::new(crate::moderation_actions::ChatCaseActions {
            guilds: guilds.0.clone(),
            messages: messages.clone(),
            admin: admin_service.clone(),
        }))
        .with_policy(Arc::new(crate::takedown_jobs::TakedownPolicySource::new(
            features.clone(),
        )))
        .with_channel_sink(Arc::new(crate::moderation_age::BusChannelSink {
            guilds: guilds.0.clone(),
            bus: bus.clone(),
            audit: audit_sink.clone(),
            pool: pool.clone(),
        }))
        .with_item_index_sink(Arc::new(crate::moderation_index::CatalogIndexSink {
            library: library_service.clone(),
            cinema: cinema_service.clone(),
        }))
        .with_blocklist(media_blocklist.clone())
        .with_removals(Arc::new(crate::takedown_jobs::BusRemovalSink::new(
            bus.clone(),
        )));
    // Serious bans that push an inviter over a threshold open a case for senior staff (MOD-SUS-002).
    admin_service.set_suspicion_observer(Arc::new(moderation.clone()));
    let moderation_deps = Arc::new(
        routes::moderation::ModerationDeps::new(moderation.clone()).with_features(features.clone()),
    );
    // Incident "Resume invites" cards close once the pause is over, whoever ended it (D-253).
    let resume_voice = staff_voice.clone();
    let resume_cards_task = tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        loop {
            tick.tick().await;
            resume_voice.close_stale_resume_cards().await;
        }
    });
    crate::staff_feed::spawn(Arc::new(crate::staff_feed::StaffFeed {
        voice: staff_voice,
        admin: admin_service.clone(),
        mailer,
        bus: bus.clone(),
        throttle: crate::staff_feed::MailThrottle::default(),
    }));
    // Built-in feature modules (docs/inventory/09-feature-modules.md): they read the event
    // firehose and act through the system user and the guild backend.
    let feature_ctx = sr_features::FeatureCtx {
        bus: bus.clone(),
        settings: features.clone(),
        clock: clock.clone(),
        actions: Arc::new(
            crate::feature_actions::PgFeatureActions::new(
                pool.clone(),
                messages.clone(),
                threads_deps.clone(),
                guilds.0.clone(),
                &cfg.server.public_url,
            )
            .with_voice(Arc::new(crate::feature_actions::GatewayVoice(
                gateway.clone(),
                bus.is_single_node(),
            ))),
        ),
        state: Arc::new(crate::features_store::PgFeatureState::new(pool.clone())),
    };
    let feature_runtime = sr_features::FeatureRuntime::new(feature_ctx.clone());
    let feature_tasks = feature_runtime.spawn(bus.clone(), sr_features::runtime::TICK_INTERVAL);
    // Sensitive admin actions (private content, account email and password, staff changes,
    // announcements) need a second factor checked in the last 15 minutes (ADM-STF-009).
    // `SR_ADMIN_REQUIRE_REAUTH=0` switches that off, for development and end-to-end tests.
    // RDY-SEC-010: refused on any host that is not a development host.
    let require_reauth = sr_api::sensitive::require_reauth_from_env(
        std::env::var("SR_ADMIN_REQUIRE_REAUTH").ok().as_deref(),
        &cfg.server.public_url,
    )
    .map_err(anyhow::Error::msg)?;
    let integrations = integration_status(&cfg, voice_setup.is_some());
    // H-39: the live HTTP rate limits start from the stored settings and follow later edits.
    let (rate_tx, rate_rx) = {
        use sr_admin::settings as k;
        let d = sr_api::rate_limit::RateConfig::default();
        let get = |key: &str, default: u32| {
            let svc = admin_service.clone();
            let key = key.to_owned();
            async move {
                svc.setting_value_i64(&key)
                    .await
                    .ok()
                    .and_then(|n| u32::try_from(n).ok())
                    .unwrap_or(default)
            }
        };
        let mut start = sr_api::rate_limit::RateConfig {
            ip: get(k::RATE_IP, d.ip).await,
            session: get(k::RATE_SESSION, d.session).await,
            bot: get(k::RATE_BOT, d.bot).await,
            messages: get(k::RATE_MESSAGES, d.messages).await,
            uploads: get(k::RATE_UPLOADS, d.uploads).await,
            ..d
        };
        // RDY-SEC-006: the abuse limits start from their stored settings too.
        for a in sr_api::rate_limit::Abuse::ALL {
            start.abuse[a.index()] = get(a.key(), d.abuse[a.index()]).await;
        }
        tokio::sync::watch::channel(start)
    };
    let mut admin = routes::admin::AdminDeps::new(admin_service)
        .with_rate_config(rate_tx)
        .with_features(features.clone())
        .with_integrations(integrations)
        .with_system_probe(Arc::new(crate::admin_jobs::FsProbe::new(
            cfg.ingest.roots.clone(),
        )))
        .with_reauth(require_reauth)
        .with_storage(storage.clone())
        .with_preflight(Arc::new(crate::preflight::LivePreflight::new(
            pool.clone(),
            storage.clone(),
            &cfg,
        )?))
        .with_gateway_sessions({
            let gw = gateway.clone();
            Arc::new(move || gw.session_count())
        });
    if let Some(v) = &voice_setup {
        let live = v.sfu.live_stats();
        admin = admin.with_voice_stats(Arc::new(move || (live.rooms(), live.egress_bytes())));
    }

    let mut settings = ApiSettings {
        trust_forwarded_for: cfg.server.trust_forwarded_for,
        ..ApiSettings::for_public_url(&cfg.server.public_url)
    };
    settings.rate_limit.live = Some(rate_rx);
    let readiness_timeout = settings.readiness_timeout;
    let state = AppState::builder()
        .settings(settings)
        .clock(clock)
        .session_policy(auth_cfg.session_policy())
        .extension(auth_service)
        .extension(Arc::new(admin))
        .extension(moderation_deps)
        .sessions(Arc::new(auth_repo))
        .bots(apps_auth.clone())
        .bearers(apps_auth)
        .extension(dev_deps)
        .openapi(routes::openapi())
        .extension(Arc::new(routes::voice::VoiceApi(Arc::new(
            crate::voice::ServerVoice::new(
                gateway.clone(),
                crate::voice::turn_config(&cfg),
                cfg.voice.enabled,
                audit_sink.clone(),
            ),
        ))))
        .extension(Arc::new(guilds.clone()))
        .extension(Arc::new(routes::settings::features::GuildFeatures {
            ctx: feature_ctx,
            roles: Arc::new(routes::settings::features::BackendRoles(guilds.0.clone())),
        }))
        .extension(messages.clone())
        .extension(threads_deps.clone())
        .extension(social_api.clone())
        .extension(connections_deps)
        .extension(export_deps)
        .extension(Arc::new(routes::auth::SelfDeleteDeps {
            store: Arc::new(crate::userset_jobs::PgSelfDeleteStore::new(
                pool.clone(),
                id_src.clone(),
            )),
        }))
        .extension(push_deps)
        .extension(mounts_deps)
        .extension(storage_events)
        .extension(gif_deps)
        .extension(lookup_deps)
        .extension(Arc::new(routes::inbox::InboxDeps::new(Arc::new(
            crate::inbox_pg::PgMentionSource::new(pool.clone()),
        ))))
        .extension(settings_deps.clone())
        .extension(Arc::new(
            routes::directory::DirectoryDeps::new(Arc::new(
                sr_db::directory_repo::PgDirectoryRepo::new(pool.clone()),
            ))
            .with_features(features.clone()),
        ))
        .extension(Arc::new(AutoModGateHandle(settings_deps)))
        .extension(search_deps)
        .extension(Arc::new(library_deps.with_sources(source_scanner.clone())))
        .extension(cinema_deps)
        .extension(tv_deps)
        .extension(media_deps)
        .extension(cinema_social_deps)
        .extension(watchparty_deps)
        .extension(activities_deps.clone())
        .extension(packages_deps.clone())
        .extension(user_feature_deps)
        .extension(storage_scope_deps)
        .extension(upload_deps)
        .extension(shelf_deps)
        // The browser's delete and move update the index at once (wave-28 integration).
        .extension(Arc::new(routes::storage::IndexHooks {
            changes: index_store.clone(),
            index: index_store.clone(),
        }))
        .extension(Arc::new(
            routes::donations::DonationsDeps::new(payments.service.clone())
                .with_chat_slot(payments.chat.clone()),
        ))
        .extension(review_deps.clone())
        .extension(Arc::new(cinema_submit_deps))
        .extension(Arc::new(routes::media::MediaDeps::new(storage.clone())))
        .extension(Arc::new(
            routes::cdn::CdnDeps::new(storage.clone(), cdn_signer)
                .with_playlists(playback_deps)
                .with_blocklist(media_blocklist.clone())
                .with_mounts(Arc::new(mount_storages.clone())),
        ))
        .extension(external_images)
        .extension(Arc::new(routes::internal::CspReports::new(Arc::new(
            crate::csp_reports::PgCspStore::new(pool.clone()),
        ))))
        .readiness(Arc::new(DbReady::new(pool.clone(), readiness_timeout)))
        .readiness(Arc::new(StorageReady::new(storage.clone())))
        .build();

    let public = TcpListener::bind(&cfg.server.bind)
        .await
        .with_context(|| format!("binding {}", cfg.server.bind))?;
    let internal = TcpListener::bind(&cfg.server.metrics_bind)
        .await
        .with_context(|| format!("binding {}", cfg.server.metrics_bind))?;
    if !internal.local_addr()?.ip().is_loopback() {
        tracing::warn!(
            bind = %cfg.server.metrics_bind,
            "metrics listener is not loopback-only; firewall it"
        );
    }
    tracing::info!(bind = %cfg.server.bind, metrics = %cfg.server.metrics_bind, "listening");

    let internal_app = internal_router(state.clone());
    let metrics_task = tokio::spawn(async move {
        if let Err(e) = axum::serve(internal, internal_app).await {
            tracing::error!(error = %e, "internal listener failed");
        }
    });

    let gw_for_gauge = gateway.clone();
    let gauge_pool = pool.clone();
    let gauge_task = tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(15));
        loop {
            tick.tick().await;
            sr_api::observability::record_gateway_sessions(gw_for_gauge.session_count());
            sr_api::observability::record_db_pool(gauge_pool.size(), gauge_pool.num_idle());
        }
    });

    // Last-seen tracking for prune (D-166): the gateway collects users seen on
    // IDENTIFY/RESUME and the 10-minute refresh; one batched UPDATE per flush.
    let gw_for_seen = gateway.clone();
    let seen_pool = pool.clone();
    let seen_task = tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(30));
        loop {
            tick.tick().await;
            let ids: Vec<i64> = gw_for_seen
                .drain_seen()
                .into_iter()
                .filter_map(|u| i64::try_from(u.get()).ok())
                .collect();
            if let Err(e) = sr_db::accounts::touch_last_seen(&seen_pool, &ids).await {
                tracing::warn!(error = %e, "could not record last-seen times");
            }
        }
    });

    // Interactions and their message mappings outlive their token by a day, then go.
    let gc_apps = apps_service.clone();
    let apps_gc_task = tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(3600));
        loop {
            tick.tick().await;
            if let Err(e) = gc_apps.purge_interactions().await {
                tracing::warn!(error = %e, "could not purge expired interactions");
            }
        }
    });

    // Jobs a crashed node left `running` go back to the queue now, and hourly (H-15).
    if let Err(e) = crate::jobs::reap_stale_jobs(&pool).await {
        tracing::warn!(error = %e, "could not reap stale jobs at startup");
    }
    let (job_tx, job_rx) = watch::channel(false);
    let runner = crate::jobs::register(
        JobRunner::new(pool.clone(), format!("srv-{}", std::process::id())),
        &pool,
        storage.clone(),
        messages.clone(),
    );
    let runner = crate::jobs::register_attachment_gc(
        runner,
        &pool,
        storage.clone(),
        messages.multipart.as_ref().map(|m| m.store.clone()),
    );
    let runner = crate::jobs::register_audit_purge(runner, &pool);
    let runner = crate::jobs::register_reap(runner, &pool);
    let runner = crate::jobs::register_insights(runner, &pool);
    let runner = crate::jobs::register_thread_archive(runner, &pool, threads_deps.clone());
    let runner = crate::search_jobs::register(runner, &pool, indexer);
    let runner = crate::admin_jobs::register(runner, &pool, id_src.clone(), bus.clone());
    let runner =
        crate::admin_jobs::register_storage_measure(runner, &pool, id_src.clone(), storage.clone());
    let runner = crate::admin_jobs::register_cors_recheck(
        runner,
        &pool,
        mount_repo.clone(),
        mount_sealer.clone(),
        cfg.server.public_url.clone(),
        mount_guard.clone(),
    );
    let runner = crate::upload_jobs::register(
        runner,
        &pool,
        upload_ledger.clone(),
        mount_repo.clone(),
        upload_io.clone(),
    );
    // Catalog indexing of storage mounts (BYOS-CAT-001): the one-shot and the daily `storage.index` job.
    let runner = crate::storage_jobs::register(
        runner,
        &pool,
        index_store.clone(),
        mount_storages.clone(),
        mount_repo.clone(),
    );
    let runner = crate::rich_jobs::register(runner, &pool, messages.clone());
    let runner = crate::external_jobs::register(runner, &pool, storage.clone());
    let runner = crate::activities_jobs::register(runner, &pool, activities_deps.service.clone());
    let runner = crate::library_jobs::register(runner, &pool, library_service.clone());
    let runner = crate::library_jobs::register_sources(runner, &pool, source_scanner.clone());
    let runner = crate::cinema_jobs::register(runner, &pool, cinema_social_service.clone());
    let runner = crate::review_jobs::register(runner, &pool, review_deps.service.clone());
    let runner = crate::takedown_jobs::register(runner, &pool, moderation.clone());
    let runner = crate::suspicion_jobs::register(runner, &pool, moderation.clone());
    let runner = crate::apps_jobs::register(runner, &pool, (*apps_service).clone());
    let runner = crate::watchparty::register(runner, &pool, watchparty_service.clone());
    // The event clock is a leased singleton job: one node advances events (G-92).
    let runner = crate::events_jobs::register(runner, &pool, events_deps.clone());
    let tv_reminders = Arc::new(crate::userfeat::TvReminders {
        tv: tv_deps_for_reminders,
        poster: system_poster_for_tv,
        base_url: cfg.server.public_url.trim_end_matches('/').to_owned(),
    });
    let runner = crate::userfeat::register(runner, &pool, ticker, Some(tv_reminders));
    let runner = crate::media_jobs::register(
        runner,
        &pool,
        media_repo.clone(),
        media_service_shared,
        download_service,
    );
    let runner =
        crate::payments_jobs::register(runner, &pool, payments.service.clone(), features.clone());
    let runner = crate::userset_jobs::register(
        runner,
        &pool,
        storage.clone(),
        social_api.clone(),
        provider_client.clone(),
        packages_deps.service.clone(),
        crate::userset_jobs::SelfDeleteEnv {
            bus: bus.clone(),
            notifier: Some(self_delete_voice),
            ids: id_src.clone(),
            features: features.clone(),
        },
    );
    crate::jobs::schedule_recurring(&pool).await?;
    crate::jobs::schedule_audit_purge(&pool).await?;
    crate::jobs::schedule_reap(&pool).await?;
    crate::jobs::schedule_insights(&pool).await?;
    crate::jobs::schedule_thread_archive(&pool).await?;
    crate::jobs::schedule_attachment_gc(&pool).await?;
    crate::admin_jobs::schedule(&pool).await?;
    crate::storage_jobs::schedule(&pool).await?;
    crate::rich_jobs::schedule_poll_sweep(&pool).await?;
    crate::external_jobs::schedule(&pool).await?;
    crate::upload_jobs::schedule(&pool).await?;
    crate::activities_jobs::schedule(&pool).await?;
    crate::library_jobs::schedule_gc(&pool).await?;
    crate::library_jobs::schedule_sources(&pool).await?;
    crate::cinema_jobs::schedule(&pool).await?;
    crate::review_jobs::schedule(&pool).await?;
    crate::takedown_jobs::schedule(&pool).await?;
    crate::suspicion_jobs::schedule(&pool).await?;
    crate::apps_jobs::schedule(&pool).await?;
    crate::watchparty::schedule(&pool).await?;
    crate::events_jobs::schedule(&pool).await?;
    crate::userfeat::schedule(&pool).await?;
    crate::media_jobs::schedule(&pool).await?;
    crate::userset_jobs::schedule(&pool).await?;
    crate::payments_jobs::schedule(&pool).await?;
    let jobs_task = tokio::spawn(runner.run(job_rx));

    // REST: every feature module, feature-owned routes, and the guild backend.
    // The guild handlers still read it from the request, so it is also layered
    // onto the router until they take `Ext<GuildApi>` from `AppState`.
    let api = routes::all()
        .merge(routes::features(
            registry
                .iter()
                .filter_map(|f| f.routes().map(|r| (f.id(), r))),
        ))
        .layer(Extension(guilds));

    // The gateway is served at `/gateway` on the public listener, beside the
    // REST API (Discord clients expect it at the root). It gets request ids
    // and a connection cap, but not the API's rate limit or timeout layers.
    let gateway_routes = with_request_id(sr_gateway::routes().with_state(gateway.clone()))
        .route_layer(from_fn_with_state(
            ConnectionCap {
                gateway: gateway.clone(),
                max: cfg.gateway.max_connections,
            },
            connection_cap,
        ));
    // Signed media delivery at `/cdn/*`, outside `/api/v1`: no cookie auth or CSRF,
    // the URL signature is the credential.
    let cdn_routes = routes::cdn::layered(state.clone());
    // The SFU reports participants it lost (media or WebSocket gone) so the gateway can clear
    // their voice state instead of showing a ghost in the channel.
    let voice_events = voice_setup.as_ref().map(|v| v.sfu.subscribe());
    if let Some(mut rx) = voice_events {
        let gw = gateway.clone();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(e) => match e.kind {
                        sr_voice::SfuEventKind::Lost => {
                            gw.voice_media_lost(e.user, e.channel).await
                        }
                        sr_voice::SfuEventKind::Active => gw.voice_activity(e.user, e.channel),
                    },
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }
    let mut app = build_router(state, api)
        .merge(gateway_routes)
        .merge(cdn_routes)
        .merge(worker_routes);
    // The SFU's WebSocket (`/voice/ws`) is served beside the gateway; media uses its own sockets.
    let sfu = voice_setup.map(|v| v.sfu);
    if let Some(s) = &sfu {
        app = app.merge(with_request_id(sr_voice::router(s.clone())));
    }
    // Last, so every route (gateway, CDN, SFU included) answers an activity host the same way: not at all.
    let app = routes::activities::proxy::wrap_activity_host(app, activities_deps.clone());

    let gw = gateway.clone();
    let sfu_for_drain = sfu.clone();
    let result = serve_until(
        public,
        app,
        shutdown,
        move || {
            gw.request_reconnect();
            // Voice clients resume on another node (or after the restart).
            if let Some(s) = sfu_for_drain.clone() {
                tokio::spawn(async move { s.drain().await });
            }
        },
        grace,
    )
    .await;

    // Reached on signal or on a serve error: unwind in order either way.
    tracing::info!("shutting down");
    let _ = job_tx.send(true);
    gateway.shutdown();
    if let Some(s) = &sfu {
        s.shutdown().await;
    }
    gauge_task.abort();
    seen_task.abort();
    apps_gc_task.abort();
    resume_cards_task.abort();
    events_voice_listener.abort();
    metrics_task.abort();
    search_consumer.abort();
    push_consumer.abort();
    if let Some(h) = features_listener {
        h.abort();
    }
    for h in feature_tasks {
        h.abort();
    }
    if tokio::time::timeout(grace, jobs_task).await.is_err() {
        tracing::warn!("jobs still running after grace period; abandoning them");
    }
    pool.close().await;
    tracing::info!("shutdown complete");
    result
}

/// Every built-in feature module: the framework modules of `sr-features` (p11 featmod) and the
/// user/community modules of `sr-userfeat` (p11 featmod-b). Both sets are listed by
/// `GET /guilds/{id}/sr/features`.
pub fn feature_registry() -> anyhow::Result<FeatureRegistry> {
    let mut registry = FeatureRegistry::new();
    registry
        .register(crate::rich_jobs::UnfurlFeature)
        .context("registering the unfurl feature")?;
    registry
        .register(sr_features::ModerationReportsFeature)
        .context("registering the moderation.reports feature")?;
    registry
        .register(sr_features::DirectoryHiddenCategoriesFeature)
        .context("registering the directory.hidden_categories feature")?;
    registry
        .register(sr_features::AppearanceDefaultsFeature)
        .context("registering the platform.appearance feature")?;
    registry
        .register(sr_features::ComplianceFeature)
        .context("registering the platform.compliance feature")?;
    registry
        .register(sr_features::IntegrationsPolicyFeature)
        .context("registering the platform.integrations feature")?;
    registry
        .register(sr_features::ByosFeature)
        .context("registering the platform.byos feature")?;
    sr_features::modules::register_all(&mut registry)
        .context("registering the built-in feature modules")?;
    // User-facing and content modules: scheduled messages, reminders, bookmarks, birthdays, leveling, RSS,
    // library feed, watch party announcer, poll templates, suggestions, tickets (docs/inventory/09).
    sr_userfeat::register_all(&mut registry).context("registering the user feature modules")?;
    Ok(registry)
}

/// What the operator configured, for the Setup checklist (ADM-DASH-003). Notes carry no secrets.
fn integration_status(
    cfg: &sr_core::config::Config,
    voice: bool,
) -> Vec<routes::admin::IntegrationStatus> {
    let item =
        |id: &str, configured: bool, detail: Option<String>| routes::admin::IntegrationStatus {
            id: id.to_owned(),
            configured,
            detail,
        };
    vec![
        item(
            "smtp",
            !cfg.smtp.host.is_empty(),
            (!cfg.smtp.host.is_empty()).then(|| format!("{}:{}", cfg.smtp.host, cfg.smtp.port)),
        ),
        item("discord_oauth", !cfg.discord.client_id.is_empty(), None),
        // The URI to register on the same Discord application for account linking (M-05, D-292).
        item(
            "discord_connections_redirect",
            !cfg.discord.client_id.is_empty() || !cfg.connections.discord.client_id.is_empty(),
            Some(format!(
                "{}/api/v1/connections/discord/callback",
                cfg.server.public_url.trim_end_matches('/')
            )),
        ),
        item("voice", voice, None),
        item(
            "turn",
            !cfg.voice.turn.urls.is_empty(),
            Some(if cfg.voice.turn.secret.expose().is_empty() {
                "STUN only".to_owned()
            } else {
                "TURN credentials".to_owned()
            }),
        ),
        item(
            "gif",
            cfg.gif.enabled(),
            (!cfg.gif.provider.is_empty()).then(|| cfg.gif.provider.clone()),
        ),
    ]
}

/// `ws(s)://<host>/gateway` from `server.public_url`: the URL READY offers for resuming (the one
/// `GET /gateway` answers).
fn gateway_ws_url(public_url: &str) -> String {
    let base = public_url.trim_end_matches('/');
    let ws = if let Some(r) = base.strip_prefix("https://") {
        format!("wss://{r}")
    } else if let Some(r) = base.strip_prefix("http://") {
        format!("ws://{r}")
    } else {
        format!("ws://{base}")
    };
    format!("{ws}/gateway")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_resume_url_is_the_gateway_of_the_public_origin() {
        assert_eq!(
            gateway_ws_url("https://chat.example.org/"),
            "wss://chat.example.org/gateway"
        );
        assert_eq!(
            gateway_ws_url("http://127.0.0.1:8080"),
            "ws://127.0.0.1:8080/gateway"
        );
    }

    #[test]
    fn both_feature_module_sets_register_for_guilds() {
        let registry = feature_registry().unwrap();
        let guild_ids: Vec<&str> = registry
            .iter()
            .filter(|f| f.scopes().contains(&sr_features::Scope::Guild))
            .map(|f| f.id())
            .collect();
        for id in [
            sr_features::modules::starboard::STARBOARD_ID,
            sr_features::modules::welcome::WELCOME_ID,
            sr_userfeat::features::REMINDERS_ID,
            sr_userfeat::features::TICKETS_ID,
        ] {
            assert!(guild_ids.contains(&id), "{id} missing from {guild_ids:?}");
        }
    }
    use axum::routing::get;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn search_dir_prefers_config_over_the_legacy_variable() {
        assert_eq!(search_dir("data/search", None), "data/search");
        assert_eq!(
            search_dir("data/search", Some(String::new())),
            "data/search"
        );
        assert_eq!(search_dir("data/search", Some("/old".into())), "/old");
        assert_eq!(search_dir("/new", Some("/old".into())), "/new");
    }

    #[test]
    fn secret_key_parsing() {
        assert_eq!(secret_key(&"ab".repeat(32)).unwrap(), [0xab; 32]);
        assert!(secret_key("zz").is_err());
        assert!(secret_key(&"zz".repeat(32)).is_err());
        assert_eq!(secret_key("").unwrap().len(), 32);
    }

    async fn bound() -> (TcpListener, SocketAddr) {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let a = l.local_addr().unwrap();
        (l, a)
    }

    #[tokio::test]
    async fn stops_on_shutdown_and_runs_hook() {
        let (l, _) = bound().await;
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let hook = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let h2 = hook.clone();
        let app = Router::new().route("/", get(|| async { "ok" }));
        let t = tokio::spawn(serve_until(
            l,
            app,
            async move {
                let _ = rx.await;
            },
            move || h2.store(true, std::sync::atomic::Ordering::SeqCst),
            Duration::from_secs(5),
        ));
        tx.send(()).unwrap();
        t.await.unwrap().unwrap();
        assert!(hook.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn hung_connection_is_cut_after_grace() {
        let (l, addr) = bound().await;
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let app = Router::new().route(
            "/hang",
            get(|| async {
                std::future::pending::<()>().await;
                "never"
            }),
        );
        let t = tokio::spawn(serve_until(
            l,
            app,
            async move {
                let _ = rx.await;
            },
            || {},
            Duration::from_millis(200),
        ));
        let mut c = tokio::net::TcpStream::connect(addr).await.unwrap();
        c.write_all(b"GET /hang HTTP/1.1\r\nhost: x\r\n\r\n")
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        tx.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), t)
            .await
            .expect("shutdown must not hang")
            .unwrap()
            .unwrap();
        let mut buf = [0u8; 16];
        let _ = tokio::time::timeout(Duration::from_secs(2), c.read(&mut buf)).await;
    }
}
