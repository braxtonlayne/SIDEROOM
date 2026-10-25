//! RDY-DEV-002: the sr-sdk shard resumes against the real gateway state machine. The transport is an
//! in-memory pipe into `sr_gateway::run_connection` that the test can cut, as a network loss would.
//! Events published while the shard is away come back after RESUME, in order, with no second READY; a
//! session the gateway can no longer replay (the buffer overflowed) is replaced by a fresh IDENTIFY.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;
use sr_core::{ChannelId, GuildId, UserId};
use sr_events::DomainEvent;
use sr_gateway::test_support::TestWorld;
use sr_gateway::{ConnectParams, Frame, GatewayConfig, run_connection};
use sr_sdk::error::{Error, Result};
use sr_sdk::gateway::{GatewayConnection, GatewayConnector, Incoming};
use sr_sdk::http::{HttpRequest, HttpResponse, HttpTransport};
use sr_sdk::{Client, Context, Event, EventHandler};
use tokio::sync::{mpsc, watch};

/// `GUILDS | GUILD_MESSAGES`.
const INTENTS: u64 = 1 | (1 << 9);
const GUILD: GuildId = GuildId::new(1);
const CHANNEL: ChannelId = ChannelId::new(10);

struct Pipe {
    to_gateway: mpsc::Sender<Frame>,
    from_gateway: mpsc::Receiver<Frame>,
    cut: watch::Receiver<bool>,
}

#[async_trait]
impl GatewayConnection for Pipe {
    async fn recv(&mut self) -> Result<Incoming> {
        tokio::select! {
            f = self.from_gateway.recv() => match f {
                Some(Frame::Text(t)) => Ok(Incoming::Text(t)),
                Some(Frame::Close { code, .. }) => Ok(Incoming::Close(Some(code))),
                Some(Frame::Binary(_)) => Err(Error::Transport("binary frame".into())),
                None => Ok(Incoming::Close(None)),
            },
            _ = self.cut.wait_for(|c| *c) => Err(Error::Transport("network lost".into())),
        }
    }

    async fn send(&mut self, text: String) -> Result<()> {
        if *self.cut.borrow() {
            return Err(Error::Transport("network lost".into()));
        }
        self.to_gateway
            .send(Frame::Text(text))
            .await
            .map_err(|_| Error::Transport("gateway closed".into()))
    }
}

/// Connects to the in-process gateway; `cut_now` severs every connection made so far and refuses new ones
/// until `heal`.
struct Cuttable {
    gw: sr_gateway::Gateway,
    cut: watch::Sender<bool>,
    connects: Mutex<u32>,
}

impl Cuttable {
    fn new(gw: sr_gateway::Gateway) -> Arc<Self> {
        Arc::new(Self {
            gw,
            cut: watch::channel(false).0,
            connects: Mutex::new(0),
        })
    }

    fn cut_now(&self) {
        self.cut.send_replace(true);
    }

    /// The network is back: later connections work.
    fn heal(&self) {
        self.cut.send_replace(false);
    }

    fn connects(&self) -> u32 {
        *self.connects.lock().unwrap()
    }
}

struct Connector(Arc<Cuttable>);

#[async_trait]
impl GatewayConnector for Connector {
    async fn connect(&self, _url: &str) -> Result<Box<dyn GatewayConnection>> {
        let me = &self.0;
        if *me.cut.borrow() {
            return Err(Error::Transport("network down".into()));
        }
        *me.connects.lock().unwrap() += 1;
        let (to_gateway, in_rx) = mpsc::channel(64);
        let (out_tx, from_gateway) = mpsc::channel(256);
        tokio::spawn(run_connection(
            me.gw.clone(),
            ConnectParams::default(),
            in_rx,
            out_tx,
        ));
        Ok(Box::new(Pipe {
            to_gateway,
            from_gateway,
            cut: me.cut.subscribe(),
        }))
    }
}

struct GatewayBot;

#[async_trait]
impl HttpTransport for GatewayBot {
    async fn send(&self, _req: HttpRequest) -> Result<HttpResponse> {
        Ok(HttpResponse {
            status: 200,
            headers: vec![],
            body: json!({"url": "wss://x.test/gateway", "shards": 1})
                .to_string()
                .into_bytes(),
        })
    }
}

/// What the bot saw, as short labels.
struct Log(mpsc::UnboundedSender<String>);

#[async_trait]
impl EventHandler for Log {
    async fn on_event(&self, _ctx: Context, _shard: u32, event: Event) {
        let label = match event {
            Event::Ready(_) => "ready".to_owned(),
            Event::Resumed => "resumed".to_owned(),
            Event::MessageCreate(m) => format!("message {}", m.id),
            _ => return,
        };
        let _ = self.0.send(label);
    }
}

fn message(id: u64) -> DomainEvent {
    DomainEvent::MessageSent {
        guild: Some(GUILD),
        channel: CHANNEL,
        message_id: id,
        author: UserId::new(1),
        data: json!({
            "id": id.to_string(), "channel_id": CHANNEL.to_string(),
            "guild_id": GUILD.to_string(), "author": {"id": "1"},
            "content": "hello", "mentions": [],
        }),
    }
}

async fn next(rx: &mut mpsc::UnboundedReceiver<String>) -> String {
    tokio::time::timeout(Duration::from_secs(20), rx.recv())
        .await
        .expect("an event in time")
        .expect("the log is open")
}

/// The next `n` labels, sorted.
async fn next_n(rx: &mut mpsc::UnboundedReceiver<String>, n: usize) -> Vec<String> {
    let mut out = Vec::new();
    for _ in 0..n {
        out.push(next(rx).await);
    }
    out.sort();
    out
}

struct Rig {
    world: TestWorld,
    link: Arc<Cuttable>,
    seen: mpsc::UnboundedReceiver<String>,
    run: tokio::task::JoinHandle<Result<()>>,
}

async fn rig(cfg: GatewayConfig) -> Rig {
    let world = TestWorld::new(cfg);
    world.add_member(UserId::new(7), GUILD);
    let link = Cuttable::new(world.gw.clone());
    let (tx, mut seen) = mpsc::unbounded_channel();
    let client = Client::builder("bot:7", "https://x.test/api/v1")
        .intents(INTENTS)
        .http_transport(Arc::new(GatewayBot))
        .gateway_connector(Arc::new(Connector(link.clone())))
        .identify_gap(Duration::ZERO)
        .handler(Log(tx))
        .build()
        .expect("client");
    let run = tokio::spawn(client.start());
    assert_eq!(next(&mut seen).await, "ready");
    Rig {
        world,
        link,
        seen,
        run,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lost_connection_resumes_and_replays_what_was_missed() {
    let mut r = rig(GatewayConfig::default()).await;
    r.world.emit(message(101)).await;
    assert_eq!(next(&mut r.seen).await, "message 101");

    // The network drops. Two messages are sent while the bot is away.
    r.link.cut_now();
    tokio::time::sleep(Duration::from_millis(200)).await;
    r.world.emit(message(102)).await;
    r.world.emit(message(103)).await;
    r.link.heal();

    // RESUME: the gateway replays both, then marks RESUMED. No second READY. (The client runs each handler
    // call in its own task, so the order the handler sees them in is not guaranteed; compare as a set.)
    assert_eq!(
        next_n(&mut r.seen, 3).await,
        ["message 102", "message 103", "resumed"]
    );
    assert_eq!(r.link.connects(), 2);

    // Live again, and the sequence kept counting: a second loss resumes from the new position.
    r.world.emit(message(104)).await;
    assert_eq!(next(&mut r.seen).await, "message 104");
    r.link.cut_now();
    tokio::time::sleep(Duration::from_millis(200)).await;
    r.world.emit(message(105)).await;
    r.link.heal();
    assert_eq!(next_n(&mut r.seen, 2).await, ["message 105", "resumed"]);
    assert_eq!(r.link.connects(), 3);
    r.run.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_session_that_cannot_be_replayed_is_replaced_by_a_fresh_identify() {
    // A buffer of 2 events cannot hold what the bot misses.
    let mut r = rig(GatewayConfig {
        resume_buffer: 2,
        ..GatewayConfig::default()
    })
    .await;
    r.link.cut_now();
    tokio::time::sleep(Duration::from_millis(200)).await;
    for id in 201..=206 {
        r.world.emit(message(id)).await;
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    r.link.heal();
    // INVALID_SESSION (false) on the RESUME, then IDENTIFY on a new connection: a second READY, and the
    // bot is live again.
    assert_eq!(next(&mut r.seen).await, "ready");
    r.world.emit(message(207)).await;
    assert_eq!(next(&mut r.seen).await, "message 207");
    assert!(r.link.connects() >= 3, "resume attempt, then identify");
    r.run.abort();
}
