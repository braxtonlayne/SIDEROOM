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
        settings::{AutoModGateHandle, SettingsDeps, events::TodoBusAdapter},
    },
};
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
    backends::{NoBearers, NoBots},
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
    let auth_repo = PgAuthRepo::new(
        pool.clone(),
        id_src.clone(),
        secret_key(cfg.server.secret_key.expose())?,
    )
    .with_previous_keys(previous_keys);

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
        mailer,
        id_src.clone(),
    )
    .with_config(auth_cfg.clone());
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

    let s = &cfg.storage;
    let storage: Arc<dyn ObjectStorage> = Arc::new(
        ObjectStoreStorage::s3(
            &s.endpoint,
            &s.bucket,
            &s.region,
            s.access_key.expose(),
            s.secret_key.expose(),
        )
        .context("configuring object storage")?,
    );

    let bus: Arc<dyn EventBus> = Arc::new(InProcessBus::new());

    // Feature crates register into the registry before it is shared (none
    // exist yet). Their routes are mounted below from `Feature::routes()`.
    let registry = Arc::new(FeatureRegistry::new());
    let features = FeatureService::new(
        registry.clone(),
        Arc::new(PgSettingsStore::new(pool.clone(), ids.clone())),
        Some(bus.clone()),
    );
    let features_listener = features.spawn_invalidation_listener();

    // Guild endpoints: the chat service over Postgres, with its own snowflake
    // worker (process id + 1, as the message deps use) so ids never collide.
    let chat = ChatService::new(
        PgChatRepo::new(pool.clone()),
        clock.clone(),
        SnowflakeGenerator::new(
            clock.clone(),
            cfg.server.worker_id,
            (cfg.server.process_id + 1) % 32,
        )
        .context("chat snowflake generator")?,
        ChatLimits::default(),
    );
    let guilds = GuildApi(routes::guilds::backend::backend(chat, bus.clone()));

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
    let messages = routes::messages::MessagesDeps::new(
        PgChatRepo::new(pool.clone()),
        Arc::new(sr_db::chat_repo::PgMessageStore::new(pool.clone())),
        storage.clone(),
        bus.clone(),
        Arc::new(routes::messages::FixedEntitlements(
            sr_storage::upload::SizePolicy::default().standard_limit,
        )),
        Arc::new(PgChatRepo::new(pool.clone())),
        Some(social),
        clock.clone(),
        cfg.server.worker_id,
        cfg.server.process_id,
    )
    .context("message snowflake generator")?
    .with_cdn_signer(sr_storage::cdn::CdnSigner::new(
        secret_key(cfg.server.secret_key.expose())?.to_vec(),
    ));

    // Server settings and message search (open questions 131-138). The settings
    // service runs over Postgres on its own snowflake worker (process id + 3) and
    // gets invite/webhook entropy from the OS. Events go through the TODO bus
    // adapter until sr-events has the settings variants. The search index lives in
    // `SR_SEARCH_DIR` (default `data/search`); TODO(config): move to `[search]`.
    let settings_deps = Arc::new(
        SettingsDeps::new(
            Arc::new(PgSettingsRepo::new(pool.clone())),
            Arc::new(PgChatRepo::new(pool.clone())),
            storage.clone(),
            clock.clone(),
            Arc::new(crate::settings_pg::secure_entropy),
            cfg.server.worker_id,
            (cfg.server.process_id + 3) % 32,
        )
        .context("settings snowflake generator")?
        .with_events(Arc::new(TodoBusAdapter::new(bus.clone()))),
    );
    let search_dir = std::env::var("SR_SEARCH_DIR").unwrap_or_else(|_| "data/search".to_owned());
    let message_index = Arc::new(
        MessageIndex::open_in_dir(std::path::Path::new(&search_dir))
            .context("opening the search index (SR_SEARCH_DIR)")?,
    );
    let search_deps = Arc::new(
        SearchDeps::new(message_index.clone())
            .with_age_gate(Arc::new(crate::settings_pg::PgAgeGate::new(pool.clone()))),
    );
    let indexer = Arc::new(crate::search_jobs::Indexer {
        index: message_index,
        repo: Arc::new(PgChatRepo::new(pool.clone())),
        store: Arc::new(sr_db::chat_repo::PgMessageStore::new(pool.clone())),
    });

    let gateway = Gateway::new(
        GatewayConfig {
            // Cookie-authenticated web sessions may IDENTIFY from the public origin only.
            allowed_origins: vec![cfg.server.public_url.trim_end_matches('/').to_owned()],
            ..GatewayConfig::default()
        },
        Arc::new(PgAuthenticator::new(
            auth_repo.clone(),
            pool.clone(),
            clock.clone(),
        )),
        Arc::new(PgGuildDirectory::new(pool.clone(), clock.clone())),
        bus.clone(),
    );
    gateway.start();

    let settings = ApiSettings {
        trust_forwarded_for: cfg.server.trust_forwarded_for,
        ..ApiSettings::for_public_url(&cfg.server.public_url)
    };
    let readiness_timeout = settings.readiness_timeout;
    let state = AppState::builder()
        .settings(settings)
        .clock(clock)
        .session_policy(auth_cfg.session_policy())
        .extension(auth_service)
        .sessions(Arc::new(auth_repo))
        .bots(Arc::new(NoBots))
        .bearers(Arc::new(NoBearers))
        .openapi(routes::openapi())
        .extension(Arc::new(guilds.clone()))
        .extension(Arc::new(messages))
        .extension(settings_deps.clone())
        .extension(Arc::new(AutoModGateHandle(settings_deps)))
        .extension(search_deps)
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

    let (job_tx, job_rx) = watch::channel(false);
    let runner = crate::jobs::register(
        JobRunner::new(pool.clone(), format!("srv-{}", std::process::id())),
        &pool,
    );
    let runner = crate::jobs::register_attachment_gc(runner, &pool, storage.clone());
    let runner = crate::search_jobs::register(runner, &pool, indexer);
    crate::jobs::schedule_recurring(&pool).await?;
    crate::jobs::schedule_attachment_gc(&pool).await?;
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
    let app = build_router(state, api).merge(gateway_routes);

    let gw = gateway.clone();
    let result = serve_until(public, app, shutdown, move || gw.request_reconnect(), grace).await;

    // Reached on signal or on a serve error: unwind in order either way.
    tracing::info!("shutting down");
    let _ = job_tx.send(true);
    gateway.shutdown();
    gauge_task.abort();
    metrics_task.abort();
    if let Some(h) = features_listener {
        h.abort();
    }
    if tokio::time::timeout(grace, jobs_task).await.is_err() {
        tracing::warn!("jobs still running after grace period; abandoning them");
    }
    pool.close().await;
    tracing::info!("shutdown complete");
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
