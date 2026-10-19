//! Gateway operations a bot sends after IDENTIFY: `REQUEST_GUILD_MEMBERS` (op 8) and the
//! presence update (op 3), their payload builders, and the [`GatewayHandle`] that routes
//! them to the shard owning a guild and hands chunk events back as a typed stream.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tokio::sync::mpsc;

use crate::error::{Error, Result};
use crate::gateway::{OP_PRESENCE_UPDATE, OP_REQUEST_GUILD_MEMBERS};

/// Snowflake timestamp shift: the shard of a guild is `(guild_id >> 22) % shard_count`.
const SHARD_SHIFT: u32 = 22;

/// `REQUEST_GUILD_MEMBERS` (op 8). Needs the `GUILD_MEMBERS` intent (`GUILD_PRESENCES` too
/// when `presences` is set). Answered with one or more `GUILD_MEMBERS_CHUNK` dispatches.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RequestGuildMembers {
    pub guild_id: String,
    /// Names starting with this; the empty string with `limit` 0 asks for everyone.
    pub query: Option<String>,
    /// At most this many results (0 = no limit; only valid with an empty `query`).
    pub limit: u32,
    pub presences: bool,
    /// Ask for exactly these users instead of a prefix query.
    pub user_ids: Vec<String>,
    /// Echoed on every chunk; the SDK fills one in when this is empty.
    pub nonce: Option<String>,
}

impl RequestGuildMembers {
    /// Everyone in the guild.
    pub fn all(guild_id: impl Into<String>) -> Self {
        Self {
            guild_id: guild_id.into(),
            query: Some(String::new()),
            ..Self::default()
        }
    }

    /// Members whose name starts with `prefix`, at most `limit`.
    pub fn query(guild_id: impl Into<String>, prefix: impl Into<String>, limit: u32) -> Self {
        Self {
            guild_id: guild_id.into(),
            query: Some(prefix.into()),
            limit,
            ..Self::default()
        }
    }

    /// Exactly these users.
    pub fn users(guild_id: impl Into<String>, ids: Vec<String>) -> Self {
        Self {
            guild_id: guild_id.into(),
            user_ids: ids,
            ..Self::default()
        }
    }

    /// The op 8 frame.
    pub fn payload(&self) -> Value {
        let mut d = Map::new();
        d.insert("guild_id".into(), json!(self.guild_id));
        if self.user_ids.is_empty() {
            d.insert(
                "query".into(),
                json!(self.query.clone().unwrap_or_default()),
            );
            d.insert("limit".into(), json!(self.limit));
        } else {
            d.insert("user_ids".into(), json!(self.user_ids));
        }
        if self.presences {
            d.insert("presences".into(), json!(true));
        }
        if let Some(n) = &self.nonce {
            d.insert("nonce".into(), json!(n));
        }
        json!({"op": OP_REQUEST_GUILD_MEMBERS, "d": Value::Object(d)})
    }
}

/// One `GUILD_MEMBERS_CHUNK` dispatch.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MembersChunk {
    pub guild_id: String,
    #[serde(default)]
    pub members: Vec<Value>,
    #[serde(default)]
    pub chunk_index: u32,
    #[serde(default = "one")]
    pub chunk_count: u32,
    #[serde(default)]
    pub not_found: Vec<String>,
    #[serde(default)]
    pub presences: Vec<Value>,
    #[serde(default)]
    pub nonce: Option<String>,
}

fn one() -> u32 {
    1
}

impl MembersChunk {
    /// Whether this is the last chunk of its request.
    pub fn is_last(&self) -> bool {
        self.chunk_index + 1 >= self.chunk_count
    }
}

/// The status a bot can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
    #[default]
    Online,
    Idle,
    Dnd,
    Invisible,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Online => "online",
            Status::Idle => "idle",
            Status::Dnd => "dnd",
            Status::Invisible => "invisible",
        }
    }
}

/// Presence update (op 3).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PresenceUpdate {
    pub status: Status,
    /// Activity objects, for example `{"name": "chess", "type": 0}`.
    pub activities: Vec<Value>,
    pub afk: bool,
}

impl PresenceUpdate {
    pub fn new(status: Status) -> Self {
        Self {
            status,
            ..Self::default()
        }
    }

    /// Add a "Playing <name>" activity (type 0).
    pub fn playing(mut self, name: impl Into<String>) -> Self {
        self.activities
            .push(json!({"name": name.into(), "type": 0}));
        self
    }

    /// The op 3 frame.
    pub fn payload(&self) -> Value {
        json!({"op": OP_PRESENCE_UPDATE, "d": {
            "status": self.status.as_str(),
            "activities": self.activities,
            "since": Value::Null,
            "afk": self.afk,
        }})
    }
}

/// The shard that owns `guild_id` among `count` (`(id >> 22) % count`).
pub fn shard_for_guild(guild_id: &str, count: u32) -> Option<u32> {
    let id: u64 = guild_id.parse().ok()?;
    let count = u64::from(count.max(1));
    u32::try_from((id >> SHARD_SHIFT) % count).ok()
}

#[derive(Default)]
struct Shared {
    /// Outbound frames per running shard.
    shards: HashMap<u32, mpsc::Sender<String>>,
    count: u32,
    /// Open member requests by nonce.
    pending: HashMap<String, mpsc::UnboundedSender<MembersChunk>>,
}

/// Sends operations to the running shards. Get it from `Client::gateway` before `start`.
#[derive(Clone, Default)]
pub struct GatewayHandle {
    inner: Arc<Mutex<Shared>>,
    nonces: Arc<AtomicU64>,
}

impl GatewayHandle {
    pub(crate) fn register_shards(&self, count: u32, senders: HashMap<u32, mpsc::Sender<String>>) {
        let mut g = self.inner.lock().expect("gateway handle lock");
        g.count = count;
        g.shards = senders;
    }

    /// Hand a decoded chunk to the request waiting for it. Returns whether one was waiting.
    pub(crate) fn deliver_chunk(&self, chunk: &MembersChunk) -> bool {
        let Some(nonce) = chunk.nonce.as_deref() else {
            return false;
        };
        let mut g = self.inner.lock().expect("gateway handle lock");
        let Some(tx) = g.pending.get(nonce) else {
            return false;
        };
        let gone = tx.send(chunk.clone()).is_err();
        if gone || chunk.is_last() {
            g.pending.remove(nonce);
        }
        true
    }

    fn sender_for(&self, shard: u32) -> Result<mpsc::Sender<String>> {
        let g = self.inner.lock().expect("gateway handle lock");
        g.shards
            .get(&shard)
            .cloned()
            .ok_or_else(|| Error::Invalid(format!("shard {shard} is not running in this process")))
    }

    /// Ask for a guild's members. The stream yields each `GUILD_MEMBERS_CHUNK` for this request
    /// and ends after the last one.
    pub async fn request_guild_members(
        &self,
        mut req: RequestGuildMembers,
    ) -> Result<mpsc::UnboundedReceiver<MembersChunk>> {
        let count = self.inner.lock().expect("gateway handle lock").count;
        if count == 0 {
            return Err(Error::Invalid("the client has not started".into()));
        }
        let shard = shard_for_guild(&req.guild_id, count)
            .ok_or_else(|| Error::Invalid("guild_id is not a snowflake".into()))?;
        let tx = self.sender_for(shard)?;
        let nonce = if let Some(n) = req.nonce.clone() {
            n
        } else {
            let n = format!("sr-sdk-{}", self.nonces.fetch_add(1, Ordering::Relaxed));
            req.nonce = Some(n.clone());
            n
        };
        let (out, rx) = mpsc::unbounded_channel();
        self.inner
            .lock()
            .expect("gateway handle lock")
            .pending
            .insert(nonce.clone(), out);
        if tx.send(req.payload().to_string()).await.is_err() {
            self.inner
                .lock()
                .expect("gateway handle lock")
                .pending
                .remove(&nonce);
            return Err(Error::Transport("the shard has stopped".into()));
        }
        Ok(rx)
    }

    /// Send gateway op 4 (voice state update) on the shard that owns `guild_id`: join (or, with a null
    /// `channel_id` in `body`, leave) a voice channel. `body` is the op's `d`; see
    /// `sr_sdk::voice_transport::voice_state_body` and `sr_sdk::voice::E2eeCall::join_state` (DEV-BOT-007).
    /// The answer arrives as a `VOICE_SERVER_UPDATE` dispatch (`Event::Other`).
    pub async fn voice_state_update(&self, guild_id: &str, body: Value) -> Result<()> {
        let count = self.inner.lock().expect("gateway handle lock").count;
        if count == 0 {
            return Err(Error::Invalid("the client has not started".into()));
        }
        let shard = shard_for_guild(guild_id, count)
            .ok_or_else(|| Error::Invalid("guild_id is not a snowflake".into()))?;
        let tx = self.sender_for(shard)?;
        tx.send(json!({"op": 4, "d": body}).to_string())
            .await
            .map_err(|_| Error::Transport("the shard has stopped".into()))
    }

    /// Set the bot's presence on every shard running in this process.
    pub async fn update_presence(&self, p: &PresenceUpdate) -> Result<()> {
        let senders: Vec<_> = {
            let g = self.inner.lock().expect("gateway handle lock");
            g.shards.values().cloned().collect()
        };
        if senders.is_empty() {
            return Err(Error::Invalid("the client has not started".into()));
        }
        let frame = p.payload().to_string();
        for tx in senders {
            tx.send(frame.clone())
                .await
                .map_err(|_| Error::Transport("the shard has stopped".into()))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn op8_payloads_cover_all_query_and_users() {
        let mut all = RequestGuildMembers::all("42");
        all.presences = true;
        all.nonce = Some("n1".into());
        assert_eq!(
            all.payload(),
            json!({"op": 8, "d": {"guild_id": "42", "query": "", "limit": 0,
                "presences": true, "nonce": "n1"}})
        );
        assert_eq!(
            RequestGuildMembers::query("42", "al", 5).payload(),
            json!({"op": 8, "d": {"guild_id": "42", "query": "al", "limit": 5}})
        );
        assert_eq!(
            RequestGuildMembers::users("42", vec!["1".into(), "2".into()]).payload(),
            json!({"op": 8, "d": {"guild_id": "42", "user_ids": ["1", "2"]}})
        );
    }

    #[test]
    fn op3_payload_carries_status_and_activities() {
        let p = PresenceUpdate::new(Status::Dnd).playing("chess");
        assert_eq!(
            p.payload(),
            json!({"op": 3, "d": {"status": "dnd", "activities": [{"name": "chess", "type": 0}],
                "since": null, "afk": false}})
        );
    }

    #[test]
    fn a_guild_goes_to_the_shard_its_snowflake_selects() {
        let id = (7_u64 << SHARD_SHIFT) | 123;
        assert_eq!(shard_for_guild(&id.to_string(), 4), Some(3));
        assert_eq!(shard_for_guild("not-a-number", 4), None);
        assert_eq!(shard_for_guild("5", 0), Some(0));
    }

    #[test]
    fn chunk_decodes_and_knows_its_last() {
        let c: MembersChunk = serde_json::from_value(json!({"guild_id": "1", "members": [{}],
            "chunk_index": 1, "chunk_count": 2, "nonce": "n"}))
        .expect("chunk");
        assert!(c.is_last());
        assert_eq!(c.nonce.as_deref(), Some("n"));
    }

    #[tokio::test]
    async fn a_request_waits_for_its_nonce_and_closes_after_the_last_chunk() {
        let h = GatewayHandle::default();
        let (tx, mut wire) = mpsc::channel(4);
        h.register_shards(1, HashMap::from([(0, tx)]));
        let mut rx = h
            .request_guild_members(RequestGuildMembers::all("42"))
            .await
            .expect("sent");
        let frame: Value = serde_json::from_str(&wire.recv().await.expect("frame")).expect("json");
        let nonce = frame["d"]["nonce"].as_str().expect("nonce").to_owned();
        let chunk = |i, n| MembersChunk {
            guild_id: "42".into(),
            chunk_index: i,
            chunk_count: n,
            nonce: Some(nonce.clone()),
            ..MembersChunk::default()
        };
        assert!(h.deliver_chunk(&chunk(0, 2)));
        assert!(h.deliver_chunk(&chunk(1, 2)));
        assert!(!h.deliver_chunk(&chunk(0, 1)), "request already complete");
        assert_eq!(rx.recv().await.expect("first").chunk_index, 0);
        assert_eq!(rx.recv().await.expect("second").chunk_index, 1);
        assert!(
            rx.recv().await.is_none(),
            "stream ends after the last chunk"
        );
    }

    #[tokio::test]
    async fn op4_goes_to_the_shard_of_the_guild() {
        let h = GatewayHandle::default();
        assert!(matches!(
            h.voice_state_update("42", json!({})).await,
            Err(Error::Invalid(_))
        ));
        let (tx, mut wire) = mpsc::channel(4);
        h.register_shards(1, HashMap::from([(0, tx)]));
        h.voice_state_update("42", json!({"channel_id": "7", "sr_e2ee": true}))
            .await
            .expect("sent");
        let frame: Value = serde_json::from_str(&wire.recv().await.expect("frame")).expect("json");
        assert_eq!(
            frame,
            json!({"op": 4, "d": {"channel_id": "7", "sr_e2ee": true}})
        );
    }

    #[tokio::test]
    async fn ops_before_start_are_refused() {
        let h = GatewayHandle::default();
        assert!(matches!(
            h.update_presence(&PresenceUpdate::default()).await,
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            h.request_guild_members(RequestGuildMembers::all("1")).await,
            Err(Error::Invalid(_))
        ));
    }
}
