//! The client: owns REST, the cache and the shard manager, and feeds events to a handler and
//! the command router.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio::time::sleep;

use crate::cache::Cache;
use crate::command::{CommandRouter, Context};
use crate::error::{Error, Result};
use crate::gateway::{
    Dispatch, Event, GatewayConnector, ShardConfig, WsConnector, host_allowed, run_shard_with,
    url_host,
};
use crate::http::{HttpTransport, MessageBody, Rest};
use crate::model::{CallbackType, intents};
use crate::ops::GatewayHandle;

/// Receives every decoded event. Handlers run on their own task, so a slow one never delays
/// the heartbeat.
#[async_trait]
pub trait EventHandler: Send + Sync + 'static {
    async fn on_event(&self, _ctx: Context, _shard: u32, _event: Event) {}

    /// A command handler or the command registration failed.
    async fn on_error(&self, _ctx: Context, _error: Error) {}
}

/// How many shards to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shards {
    /// Use the count `GET /gateway/bot` recommends.
    Auto,
    /// Exactly this many, all in this process.
    Count(u32),
    /// Only shard `id` of `count` (to spread shards over processes).
    One { id: u32, count: u32 },
}

pub struct ClientBuilder {
    token: String,
    base_url: String,
    intents: u64,
    shards: Shards,
    gateway_url: Option<String>,
    gateway_hosts: Vec<String>,
    handler: Option<Arc<dyn EventHandler>>,
    router: Option<CommandRouter>,
    register_commands: bool,
    transport: Option<Arc<dyn HttpTransport>>,
    connector: Option<Arc<dyn GatewayConnector>>,
    identify_gap: Duration,
}

impl ClientBuilder {
    /// `base_url` is the API root, for example `https://chat.example/api/v1`.
    pub fn new(token: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            base_url: base_url.into(),
            intents: intents::DEFAULT,
            shards: Shards::Auto,
            gateway_url: None,
            gateway_hosts: Vec::new(),
            handler: None,
            router: None,
            register_commands: true,
            transport: None,
            connector: None,
            // The server allows one IDENTIFY at a time (max_concurrency 1).
            identify_gap: Duration::from_secs(5),
        }
    }

    pub fn intents(mut self, bits: u64) -> Self {
        self.intents = bits;
        self
    }
    pub fn shards(mut self, s: Shards) -> Self {
        self.shards = s;
        self
    }
    /// Also follow gateway and resume URLs on this host (D-234). By default only the host of
    /// the base URL (and of an explicit `gateway_url`) is followed.
    pub fn allow_gateway_host(mut self, host: impl Into<String>) -> Self {
        self.gateway_hosts.push(host.into().to_ascii_lowercase());
        self
    }
    /// Skip `GET /gateway/bot` for the URL.
    pub fn gateway_url(mut self, url: impl Into<String>) -> Self {
        self.gateway_url = Some(url.into());
        self
    }
    pub fn handler(mut self, h: impl EventHandler) -> Self {
        self.handler = Some(Arc::new(h));
        self
    }
    pub fn commands(mut self, r: CommandRouter) -> Self {
        self.router = Some(r);
        self
    }
    /// Register the router's commands globally at the first READY (default on).
    pub fn register_commands(mut self, on: bool) -> Self {
        self.register_commands = on;
        self
    }
    pub fn http_transport(mut self, t: Arc<dyn HttpTransport>) -> Self {
        self.transport = Some(t);
        self
    }
    pub fn gateway_connector(mut self, c: Arc<dyn GatewayConnector>) -> Self {
        self.connector = Some(c);
        self
    }
    pub fn identify_gap(mut self, d: Duration) -> Self {
        self.identify_gap = d;
        self
    }

    pub fn build(self) -> Result<Client> {
        let rest = match self.transport {
            Some(t) => Rest::with_transport(self.base_url.clone(), self.token.clone(), t),
            None => Rest::new(self.base_url.clone(), self.token.clone())?,
        };
        Ok(Client {
            rest,
            cache: Cache::new(),
            intents: self.intents,
            shards: self.shards,
            gateway_url: self.gateway_url,
            gateway_hosts: self.gateway_hosts,
            base_url: self.base_url,
            gateway: GatewayHandle::default(),
            handler: self.handler,
            router: self.router.map(Arc::new),
            register_commands: self.register_commands,
            connector: self.connector.unwrap_or_else(|| Arc::new(WsConnector)),
            identify_gap: self.identify_gap,
        })
    }
}

/// `wss://host/gateway` for an API root such as `https://host/api/v1` (the server's default).
fn default_gateway_url(base_url: &str) -> String {
    let (scheme, rest) = base_url.split_once("://").unwrap_or(("https", base_url));
    let ws = if scheme.eq_ignore_ascii_case("http") {
        "ws"
    } else {
        "wss"
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    format!("{ws}://{authority}/gateway")
}

pub struct Client {
    rest: Rest,
    cache: Cache,
    intents: u64,
    shards: Shards,
    gateway_url: Option<String>,
    gateway_hosts: Vec<String>,
    base_url: String,
    gateway: GatewayHandle,
    handler: Option<Arc<dyn EventHandler>>,
    router: Option<Arc<CommandRouter>>,
    register_commands: bool,
    connector: Arc<dyn GatewayConnector>,
    identify_gap: Duration,
}

impl Client {
    pub fn builder(token: impl Into<String>, base_url: impl Into<String>) -> ClientBuilder {
        ClientBuilder::new(token, base_url)
    }

    pub fn rest(&self) -> &Rest {
        &self.rest
    }

    /// Send gateway operations (member requests, presence) to the running shards. Clone it
    /// before `start` consumes the client.
    pub fn gateway(&self) -> GatewayHandle {
        self.gateway.clone()
    }

    pub fn cache(&self) -> &Cache {
        &self.cache
    }

    fn context(&self) -> Context {
        Context {
            rest: self.rest.clone(),
            cache: self.cache.clone(),
        }
    }

    /// Connect every shard and run until one fails fatally (bad token or intents).
    pub async fn start(self) -> Result<()> {
        // D-234: the base host and the operator's allow-list are the only hosts followed.
        let mut allowed: Vec<String> = url_host(&self.base_url).into_iter().collect();
        allowed.extend(self.gateway_hosts.iter().cloned());
        let (url, recommended) = match &self.gateway_url {
            Some(u) => (u.clone(), 1),
            None => {
                let (u, n) = self.rest.gateway_bot().await?;
                if host_allowed(&u, &allowed) {
                    (u, n)
                } else {
                    let fallback = default_gateway_url(&self.base_url);
                    tracing::warn!(
                        gateway_url = u.as_str(),
                        fallback = fallback.as_str(),
                        "ignoring a gateway URL on a host that is neither the base host nor allow-listed"
                    );
                    (fallback, n)
                }
            }
        };
        if let Some(h) = url_host(&url)
            && !allowed.contains(&h)
        {
            allowed.push(h);
        }
        let plan: Vec<(u32, u32)> = match self.shards {
            Shards::Auto => (0..recommended).map(|i| (i, recommended)).collect(),
            Shards::Count(n) => (0..n.max(1)).map(|i| (i, n.max(1))).collect(),
            Shards::One { id, count } => vec![(id, count)],
        };

        let (tx, mut rx) = mpsc::channel::<Dispatch>(256);
        let mut shard_tasks: JoinSet<Result<()>> = JoinSet::new();
        let shard_count = plan.first().map_or(1, |s| s.1);
        let mut senders = std::collections::HashMap::new();
        for (n, shard) in plan.into_iter().enumerate() {
            let mut cfg = ShardConfig::new(self.rest.token(), self.intents, shard, url.clone());
            cfg.allowed_hosts.clone_from(&allowed);
            let (out_tx, out_rx) = mpsc::channel::<String>(32);
            senders.insert(shard.0, out_tx);
            let connector = self.connector.clone();
            let tx = tx.clone();
            let delay = self.identify_gap * u32::try_from(n).unwrap_or(u32::MAX);
            shard_tasks.spawn(async move {
                if !delay.is_zero() {
                    sleep(delay).await;
                }
                run_shard_with(cfg, connector.as_ref(), tx, Some(out_rx)).await
            });
        }
        drop(tx);
        self.gateway.register_shards(shard_count, senders);

        let ctx = self.context();
        let mut registered = false;
        loop {
            tokio::select! {
                d = rx.recv() => {
                    let Some(d) = d else { break };
                    self.cache.apply(&d.name, &d.data);
                    let event = Event::from_dispatch(&d.name, d.data);
                    if let Event::GuildMembersChunk(c) = &event {
                        self.gateway.deliver_chunk(c);
                    }
                    if matches!(event, Event::Ready(_)) && !registered {
                        registered = true;
                        self.register(&ctx).await;
                    }
                    self.route(&ctx, d.shard, event);
                }
                done = shard_tasks.join_next() => {
                    match done {
                        Some(Ok(Err(e))) => {
                            shard_tasks.abort_all();
                            return Err(e);
                        }
                        Some(_) => {}
                        None => break,
                    }
                }
            }
        }
        // Every dispatch sender is gone, so each shard task has ended or is about to: the first
        // fatal error among them is the result (the channel can close before we see it).
        while let Some(done) = shard_tasks.join_next().await {
            if let Ok(Err(e)) = done {
                shard_tasks.abort_all();
                return Err(e);
            }
        }
        Ok(())
    }

    async fn register(&self, ctx: &Context) {
        let (Some(router), true) = (&self.router, self.register_commands) else {
            return;
        };
        if router.is_empty() {
            return;
        }
        let Some(app) = ctx.cache.application_id() else {
            return;
        };
        if let Err(e) = ctx
            .rest
            .set_global_commands(&app, &router.definitions())
            .await
            && let Some(h) = &self.handler
        {
            h.on_error(ctx.clone(), e).await;
        }
    }

    fn route(&self, ctx: &Context, shard: u32, event: Event) {
        if let Event::InteractionCreate(i) = &event
            && let Some(router) = &self.router
        {
            let router = router.clone();
            let ctx = ctx.clone();
            let handler = self.handler.clone();
            let i = (**i).clone();
            tokio::spawn(async move {
                let for_error = i.clone();
                if let Err(e) = router.dispatch(&ctx, i).await {
                    // Tell the invoking user, then let the bot's own hook see it. A reply
                    // that fails (already answered, window passed) is not worth more noise.
                    let _ = ctx
                        .rest
                        .interaction_callback(
                            &for_error,
                            CallbackType::ChannelMessage,
                            Some(
                                MessageBody::content("Something went wrong with that command.")
                                    .ephemeral()
                                    .into_json(),
                            ),
                        )
                        .await;
                    if let Some(h) = handler {
                        h.on_error(ctx, e).await;
                    }
                }
            });
        }
        if let Some(h) = &self.handler {
            let h = h.clone();
            let ctx = ctx.clone();
            tokio::spawn(async move { h.on_event(ctx, shard, event).await });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use serde_json::{Value, json};

    use super::*;
    use crate::command::CommandDef;
    use crate::gateway::Incoming;
    use crate::gateway::fake::{FakeConnector, text};
    use crate::http::fake::FakeTransport;

    struct Collect(Arc<Mutex<Vec<(u32, String)>>>);

    #[async_trait]
    impl EventHandler for Collect {
        async fn on_event(&self, _ctx: Context, shard: u32, event: Event) {
            let name = match event {
                Event::Ready(_) => "ready".to_owned(),
                Event::MessageCreate(m) => format!("msg:{}", m.content),
                _ => "other".to_owned(),
            };
            self.0.lock().expect("lock").push((shard, name));
        }
    }

    fn hello() -> Incoming {
        text(json!({"op": 10, "d": {"heartbeat_interval": 40000}}))
    }

    fn ready() -> Incoming {
        text(json!({"op": 0, "s": 1, "t": "READY", "d": {
            "session_id": "s", "user": {"id": "9", "bot": true}, "application": {"id": "5", "flags": 0}, "guilds": []}}))
    }

    #[tokio::test(start_paused = true)]
    async fn start_runs_shards_registers_commands_routes_events_and_stops_on_fatal_close() {
        let transport = FakeTransport::new(vec![
            FakeTransport::json(200, json!({"url": "wss://x.test/gateway", "shards": 1})),
            FakeTransport::json(200, json!([])),
            FakeTransport::no_content(),
        ]);
        let (conn, links) = FakeConnector::new(1);
        let events = Arc::new(Mutex::new(Vec::new()));
        let router = CommandRouter::new()
            .command(CommandDef::chat_input("hi", "Say hi"), |cx| async move {
                cx.reply("hello").await
            });
        let client = Client::builder("tok", "https://x.test/api/v1")
            .intents(intents::DEFAULT | intents::MESSAGE_CONTENT)
            .handler(Collect(events.clone()))
            .commands(router)
            .http_transport(transport.clone())
            .gateway_connector(Arc::new(conn))
            .identify_gap(Duration::ZERO)
            .build()
            .expect("client");
        let cache = client.cache().clone();
        let run = tokio::spawn(client.start());

        let l = &links[0];
        l.to_client.send(hello()).expect("hello");
        l.to_client.send(ready()).expect("ready");
        l.to_client
            .send(text(json!({"op": 0, "s": 2, "t": "MESSAGE_CREATE", "d":
                {"id": "1", "channel_id": "2", "content": "yo", "author": {"id": "3"}}})))
            .expect("msg");
        l.to_client
            .send(text(json!({"op": 0, "s": 3, "t": "INTERACTION_CREATE", "d":
                {"id": "11", "application_id": "5", "type": 2, "token": "itok", "data": {"name": "hi"}}})))
            .expect("interaction");
        sleep(Duration::from_millis(100)).await;

        assert_eq!(cache.application_id().as_deref(), Some("5"));
        {
            let ev = events.lock().expect("lock");
            assert!(
                ev.contains(&(0, "ready".into())) && ev.contains(&(0, "msg:yo".into())),
                "{ev:?}"
            );
        }
        {
            let reqs = transport.seen.lock().expect("lock");
            assert_eq!(reqs[0].url, "https://x.test/api/v1/gateway/bot");
            assert_eq!(reqs[1].url, "https://x.test/api/v1/applications/5/commands");
            let defs: Value =
                serde_json::from_slice(reqs[1].body.as_ref().expect("body")).expect("json");
            assert_eq!(defs[0]["name"], "hi");
            assert_eq!(
                reqs[2].url,
                "https://x.test/api/v1/interactions/11/itok/callback"
            );
        }
        l.to_client
            .send(Incoming::Close(Some(4004)))
            .expect("close");
        let err = run.await.expect("join").expect_err("bad token is fatal");
        assert!(matches!(err, Error::GatewayFatal { code: 4004, .. }));
    }

    #[tokio::test(start_paused = true)]
    async fn auto_sharding_uses_the_recommended_count_and_identifies_each_shard() {
        let transport = FakeTransport::new(vec![FakeTransport::json(
            200,
            json!({"url": "wss://x.test/gateway", "shards": 2}),
        )]);
        let (conn, links) = FakeConnector::new(2);
        let client = Client::builder("tok", "https://x.test/api/v1")
            .http_transport(transport)
            .gateway_connector(Arc::new(conn))
            .identify_gap(Duration::from_secs(5))
            .build()
            .expect("client");
        let run = tokio::spawn(client.start());
        sleep(Duration::from_millis(10)).await;
        links[0].to_client.send(hello()).expect("hello 0");
        sleep(Duration::from_secs(6)).await;
        links[1].to_client.send(hello()).expect("hello 1");
        sleep(Duration::from_millis(100)).await;
        let a = links[0].from_client.lock().expect("lock")[0].clone();
        let b = links[1].from_client.lock().expect("lock")[0].clone();
        assert_eq!(a["d"]["shard"], json!([0, 2]));
        assert_eq!(b["d"]["shard"], json!([1, 2]));
        run.abort();
    }
}

#[cfg(test)]
mod gateway_ops_tests {
    //! D-234 gateway address rule and the op 8 / op 3 senders.
    use super::*;
    use crate::gateway::Incoming;
    use crate::gateway::fake::{FakeConnector, text};
    use crate::http::fake::FakeTransport;
    use crate::ops::{PresenceUpdate, RequestGuildMembers, Status};
    use serde_json::json;

    fn hello() -> Incoming {
        text(json!({"op": 10, "d": {"heartbeat_interval": 40000}}))
    }

    async fn gateway_url_used(url: &str, allow: Option<&str>) -> String {
        let transport = FakeTransport::new(vec![FakeTransport::json(
            200,
            json!({"url": url, "shards": 1}),
        )]);
        let (conn, _links) = FakeConnector::new(1);
        let conn = Arc::new(conn);
        let mut b = Client::builder("tok", "https://x.test/api/v1")
            .http_transport(transport)
            .gateway_connector(conn.clone())
            .identify_gap(Duration::ZERO);
        if let Some(h) = allow {
            b = b.allow_gateway_host(h);
        }
        let run = tokio::spawn(b.build().expect("client").start());
        sleep(Duration::from_millis(100)).await;
        run.abort();
        conn.urls.lock().expect("lock")[0].clone()
    }

    #[tokio::test(start_paused = true)]
    async fn a_gateway_url_on_another_host_falls_back_to_the_configured_one() {
        assert_eq!(
            gateway_url_used("wss://evil.test/gateway", None).await,
            "wss://x.test/gateway"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn an_allow_listed_gateway_host_is_followed() {
        assert_eq!(
            gateway_url_used("wss://gw.test/gateway", Some("GW.test")).await,
            "wss://gw.test/gateway"
        );
        assert_eq!(
            gateway_url_used("wss://x.test:8443/gateway", None).await,
            "wss://x.test:8443/gateway",
            "the base host is always allowed"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn member_requests_and_presence_reach_the_shard_and_chunks_come_back() {
        let transport = FakeTransport::new(vec![FakeTransport::json(
            200,
            json!({"url": "wss://x.test/gateway", "shards": 1}),
        )]);
        let (conn, links) = FakeConnector::new(1);
        let client = Client::builder("tok", "https://x.test/api/v1")
            .http_transport(transport)
            .gateway_connector(Arc::new(conn))
            .identify_gap(Duration::ZERO)
            .build()
            .expect("client");
        let gw = client.gateway();
        let run = tokio::spawn(client.start());
        links[0].to_client.send(hello()).expect("hello");
        sleep(Duration::from_millis(50)).await;

        let mut chunks = gw
            .request_guild_members(RequestGuildMembers::all("42"))
            .await
            .expect("op 8");
        gw.update_presence(&PresenceUpdate::new(Status::Idle))
            .await
            .expect("op 3");
        sleep(Duration::from_millis(50)).await;
        let (nonce, ops): (String, Vec<u64>) = {
            let sent = links[0].from_client.lock().expect("lock");
            let nonce = sent[1]["d"]["nonce"].as_str().expect("nonce").to_owned();
            (
                nonce,
                sent.iter().filter_map(|f| f["op"].as_u64()).collect(),
            )
        };
        assert_eq!(ops, vec![2, 8, 3]);

        links[0]
            .to_client
            .send(text(
                json!({"op": 0, "s": 1, "t": "GUILD_MEMBERS_CHUNK", "d": {
                "guild_id": "42", "members": [{"user": {"id": "1"}}],
                "chunk_index": 0, "chunk_count": 1, "nonce": nonce}}),
            ))
            .expect("chunk");
        let c = chunks.recv().await.expect("chunk");
        assert_eq!(c.members.len(), 1);
        assert!(chunks.recv().await.is_none());
        run.abort();
    }
}
