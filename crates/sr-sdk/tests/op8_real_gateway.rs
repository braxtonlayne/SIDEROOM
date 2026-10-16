//! The sr-sdk gateway client against the real gateway state machine (H-52, D-242): op 8
//! `REQUEST_GUILD_MEMBERS` goes out through the SDK, the server's `GUILD_MEMBERS_CHUNK`
//! dispatches come back as the typed stream. The transport is an in-memory pipe into
//! `sr_gateway::run_connection`; HTTP only answers `GET /gateway/bot`.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;
use sr_core::{GuildId, UserId};
use sr_gateway::test_support::TestWorld;
use sr_gateway::{ConnectParams, Frame, GatewayConfig, run_connection};
use sr_sdk::Client;
use sr_sdk::error::{Error, Result};
use sr_sdk::gateway::{GatewayConnection, GatewayConnector, Incoming};
use sr_sdk::http::{HttpRequest, HttpResponse, HttpTransport};
use sr_sdk::ops::RequestGuildMembers;
use tokio::sync::mpsc;

/// `GUILDS | GUILD_MEMBERS`.
const INTENTS: u64 = 1 | (1 << 1);
/// The privileged bit the test application may use (`GUILD_MEMBERS`).
const ALLOWED: u64 = 1 << 1;

struct Pipe {
    to_gateway: mpsc::Sender<Frame>,
    from_gateway: mpsc::Receiver<Frame>,
}

#[async_trait]
impl GatewayConnection for Pipe {
    async fn recv(&mut self) -> Result<Incoming> {
        match self.from_gateway.recv().await {
            Some(Frame::Text(t)) => Ok(Incoming::Text(t)),
            Some(Frame::Close { code, .. }) => Ok(Incoming::Close(Some(code))),
            Some(Frame::Binary(_)) => Err(Error::Transport("binary frame".into())),
            None => Ok(Incoming::Close(None)),
        }
    }

    async fn send(&mut self, text: String) -> Result<()> {
        self.to_gateway
            .send(Frame::Text(text))
            .await
            .map_err(|_| Error::Transport("gateway closed".into()))
    }
}

struct InProcess(sr_gateway::Gateway);

#[async_trait]
impl GatewayConnector for InProcess {
    async fn connect(&self, _url: &str) -> Result<Box<dyn GatewayConnection>> {
        let (to_gateway, in_rx) = mpsc::channel(64);
        let (out_tx, from_gateway) = mpsc::channel(256);
        tokio::spawn(run_connection(
            self.0.clone(),
            ConnectParams::default(),
            in_rx,
            out_tx,
        ));
        Ok(Box::new(Pipe {
            to_gateway,
            from_gateway,
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_sdk_gets_its_member_chunks_from_the_real_gateway() {
    let world = TestWorld::new(GatewayConfig::default());
    let guild = GuildId::new(1);
    world.add_member(UserId::new(7), guild);
    world.add_roster(guild, 1000, 1001);

    let client = Client::builder(format!("bot:7:{ALLOWED}"), "https://x.test/api/v1")
        .intents(INTENTS)
        .http_transport(Arc::new(GatewayBot))
        .gateway_connector(Arc::new(InProcess(world.gw.clone())))
        .identify_gap(Duration::ZERO)
        .build()
        .expect("client");
    let gateway = client.gateway();
    let run = tokio::spawn(client.start());

    // The shard registers itself right after start; ask until it is there.
    let mut asked = None;
    for _ in 0..200 {
        match gateway
            .request_guild_members(RequestGuildMembers::all("1"))
            .await
        {
            Ok(rx) => {
                asked = Some(rx);
                break;
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(25)).await,
        }
    }
    let mut chunks = asked.expect("the shard started");

    // The request may precede READY on the wire; the server answers once IDENTIFY went through.
    let first = tokio::time::timeout(Duration::from_secs(10), chunks.recv())
        .await
        .expect("a chunk in time")
        .expect("a chunk");
    assert_eq!((first.chunk_index, first.chunk_count), (0, 2));
    assert_eq!(first.members.len(), 1000);
    assert_eq!(first.guild_id, "1");
    assert!(!first.is_last());
    let second = chunks.recv().await.expect("the second chunk");
    assert_eq!((second.chunk_index, second.chunk_count), (1, 2));
    assert_eq!(second.members.len(), 1);
    assert!(second.is_last());
    assert!(
        chunks.recv().await.is_none(),
        "the stream ends after the last chunk"
    );

    // By id: one member, one id that is not a member.
    let mut by_id = gateway
        .request_guild_members(RequestGuildMembers::users(
            "1",
            vec!["1003".into(), "42".into()],
        ))
        .await
        .expect("op 8");
    let c = by_id.recv().await.expect("a chunk");
    assert_eq!(c.members.len(), 1);
    assert_eq!(c.not_found, vec!["42".to_owned()]);
    assert!(c.is_last());

    run.abort();
}
