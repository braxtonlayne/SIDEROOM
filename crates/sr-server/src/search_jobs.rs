//! Search indexing jobs (open question 132, LNG-SRC-002).
//!
//! * `search.index_message`: one job per message event. The payload says what
//!   happened (`upsert`, `delete`, `delete_channel`); the handler loads the
//!   message, converts it with `impl From<&Message> for MessageEvent`, adds the
//!   attachment `has:` flags and applies it to the index. Replays are
//!   idempotent (the index deletes before it adds).
//! * `search.reindex`: drops the index and rebuilds it from every visible
//!   message, channel by channel, in pages.
//!
//! TODO(events): nothing enqueues `search.index_message` yet. [`jobs_for_event`]
//! and [`enqueue_for_event`] are the adapter: they need a bus subscription that
//! sees every message event (a wildcard topic, or `MessageSent/Edited/Deleted`
//! also routed to `Topic::Platform`), which is `sr-events` territory. Until then
//! the message routes can call [`enqueue_for_event`] directly after their own
//! `publish`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::json;
use sr_chat::Message;
use sr_chat::repo::{ChatRepository, MessageStore, Page};
use sr_core::{ChannelId, MessageId};
use sr_db::jobs::{self, Job, NewJob};
use sr_db::runner::{HandlerError, JobRunner};
use sr_db::{DbResult, PgPool};
use sr_events::DomainEvent;
use sr_search::{MessageEvent, MessageIndex};

/// Job kind for a single message event.
pub const INDEX_MESSAGE_KIND: &str = "search.index_message";
/// Job kind for the full rebuild.
pub const REINDEX_KIND: &str = "search.reindex";

/// Messages read per page during a reindex.
const REINDEX_PAGE: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexOp {
    /// Created, edited or pin-changed: (re)index the stored message.
    Upsert,
    /// Deleted: remove from the index.
    Delete,
    /// The channel went away: remove every message in it.
    DeleteChannel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexPayload {
    pub op: IndexOp,
    pub channel_id: u64,
    /// Unused (0) for `delete_channel`.
    #[serde(default)]
    pub message_id: u64,
}

/// The index jobs a bus event implies.
pub fn jobs_for_event(ev: &DomainEvent) -> Vec<IndexPayload> {
    match ev {
        DomainEvent::MessageSent {
            channel,
            message_id,
            ..
        }
        | DomainEvent::MessageEdited {
            channel,
            message_id,
            ..
        } => vec![IndexPayload {
            op: IndexOp::Upsert,
            channel_id: channel.get(),
            message_id: *message_id,
        }],
        DomainEvent::MessageDeleted {
            channel,
            message_id,
            ..
        } => vec![IndexPayload {
            op: IndexOp::Delete,
            channel_id: channel.get(),
            message_id: *message_id,
        }],
        DomainEvent::MessagesBulkDeleted {
            channel,
            message_ids,
            ..
        } => message_ids
            .iter()
            .map(|id| IndexPayload {
                op: IndexOp::Delete,
                channel_id: channel.get(),
                message_id: *id,
            })
            .collect(),
        DomainEvent::ChannelDeleted { channel, .. } => vec![IndexPayload {
            op: IndexOp::DeleteChannel,
            channel_id: channel.get(),
            message_id: 0,
        }],
        _ => Vec::new(),
    }
}

/// Enqueue the jobs for `ev`; `next_id` mints job ids (snowflakes). Returns how
/// many were queued.
pub async fn enqueue_for_event(
    pool: &PgPool,
    mut next_id: impl FnMut() -> i64,
    ev: &DomainEvent,
) -> DbResult<usize> {
    let payloads = jobs_for_event(ev);
    for p in &payloads {
        let payload = serde_json::to_value(p).unwrap_or_else(|_| json!({}));
        jobs::enqueue(pool, &NewJob::new(next_id(), INDEX_MESSAGE_KIND, payload)).await?;
    }
    Ok(payloads.len())
}

/// Queue a full rebuild (the `reindex` command and the admin button).
pub async fn enqueue_reindex(pool: &PgPool, id: i64) -> DbResult<()> {
    jobs::enqueue(pool, &NewJob::new(id, REINDEX_KIND, json!({}))).await
}

/// What the index jobs read from: messages, attachments and the channel list.
pub struct Indexer {
    pub index: Arc<MessageIndex>,
    pub repo: Arc<dyn ChatRepository>,
    pub store: Arc<dyn MessageStore>,
}

#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error("bad payload: {0}")]
    Payload(String),
    #[error("repository: {0}")]
    Repo(String),
    #[error("search index: {0}")]
    Search(String),
}

impl Indexer {
    /// Build the event for one stored message, with attachment flags.
    async fn event_for(&self, msg: &Message) -> Result<MessageEvent, IndexError> {
        let event = MessageEvent::from(msg);
        if msg.attachment_ids.is_empty() {
            return Ok(event);
        }
        let found = self
            .store
            .attachments_of(&[msg.id])
            .await
            .map_err(|e| IndexError::Repo(e.to_string()))?;
        let types: Vec<String> = found
            .get(&msg.id)
            .into_iter()
            .flatten()
            .map(|a| {
                a.content_type
                    .clone()
                    .or_else(|| a.declared_content_type.clone())
                    .unwrap_or_default()
            })
            .collect();
        let kinds = types.iter().map(String::as_str);
        Ok(match event {
            MessageEvent::Created(i) => MessageEvent::Created(i.with_attachments(kinds)),
            MessageEvent::Updated(i) => MessageEvent::Updated(i.with_attachments(kinds)),
            other => other,
        })
    }

    /// Apply one job payload to the index.
    pub async fn handle(&self, p: IndexPayload) -> Result<(), IndexError> {
        let channel = ChannelId::new(p.channel_id);
        let event = match p.op {
            IndexOp::DeleteChannel => MessageEvent::ChannelDeleted(channel),
            IndexOp::Delete => MessageEvent::Deleted(MessageId::new(p.message_id)),
            IndexOp::Upsert => {
                let id = MessageId::new(p.message_id);
                match self
                    .repo
                    .get_message(channel, id)
                    .await
                    .map_err(|e| IndexError::Repo(e.to_string()))?
                {
                    Some(msg) => self.event_for(&msg).await?,
                    // Gone before we got here (purged, or never committed).
                    None => MessageEvent::Deleted(id),
                }
            }
        };
        self.apply(vec![event]).await
    }

    async fn apply(&self, events: Vec<MessageEvent>) -> Result<(), IndexError> {
        let index = self.index.clone();
        tokio::task::spawn_blocking(move || index.apply_batch(&events))
            .await
            .map_err(|e| IndexError::Search(e.to_string()))?
            .map_err(|e| IndexError::Search(e.to_string()))
    }

    /// Rebuild the index from `channels`: clear it, then index every visible
    /// message page by page. Returns the number of messages indexed.
    pub async fn reindex(&self, channels: &[ChannelId]) -> Result<u64, IndexError> {
        let index = self.index.clone();
        tokio::task::spawn_blocking(move || index.rebuild(std::iter::empty()))
            .await
            .map_err(|e| IndexError::Search(e.to_string()))?
            .map_err(|e| IndexError::Search(e.to_string()))?;
        let mut total = 0u64;
        for &channel in channels {
            let mut after: Option<MessageId> = None;
            loop {
                let page = self
                    .store
                    .fetch(
                        channel,
                        Page {
                            before: None,
                            after,
                            limit: REINDEX_PAGE,
                            ascending: true,
                        },
                    )
                    .await
                    .map_err(|e| IndexError::Repo(e.to_string()))?;
                let Some(last) = page.last().map(|m| m.id) else {
                    break;
                };
                let n = page.len();
                let mut events = Vec::with_capacity(n);
                for m in &page {
                    events.push(self.event_for(m).await?);
                }
                total += n as u64;
                self.apply(events).await?;
                if n < REINDEX_PAGE {
                    break;
                }
                after = Some(last);
            }
        }
        Ok(total)
    }
}

async fn index_handler(indexer: Arc<Indexer>, job: Job) -> Result<(), HandlerError> {
    let payload: IndexPayload = serde_json::from_value(job.payload.clone())
        .map_err(|e| IndexError::Payload(e.to_string()))?;
    indexer.handle(payload).await?;
    Ok(())
}

async fn reindex_handler(
    indexer: Arc<Indexer>,
    pool: PgPool,
    _job: Job,
) -> Result<(), HandlerError> {
    let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM channels ORDER BY id")
        .fetch_all(&pool)
        .await?;
    let channels: Vec<ChannelId> = ids
        .into_iter()
        .filter_map(|i| u64::try_from(i).ok())
        .map(ChannelId::new)
        .collect();
    let total = indexer.reindex(&channels).await?;
    tracing::info!(
        messages = total,
        channels = channels.len(),
        "search reindex finished"
    );
    Ok(())
}

/// Register both search job kinds on `runner`. Message jobs run one at a time:
/// the index has a single writer.
pub fn register(runner: JobRunner, pool: &PgPool, indexer: Arc<Indexer>) -> JobRunner {
    let for_messages = indexer.clone();
    let pool = pool.clone();
    runner
        .register(INDEX_MESSAGE_KIND, 1, move |job| {
            Box::pin(index_handler(for_messages.clone(), job))
        })
        .register(REINDEX_KIND, 1, move |job| {
            Box::pin(reindex_handler(indexer.clone(), pool.clone(), job))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_chat::memory::MemoryBackend;
    use sr_chat::repo::GuildRepository;
    use sr_chat::{Member, SendMessage};
    use sr_core::{Clock, GuildId, ManualClock, SnowflakeGenerator, UserId};
    use sr_search::{Access, IndexedMessage, SearchRequest};

    fn ev_sent(channel: u64, id: u64) -> DomainEvent {
        DomainEvent::MessageSent {
            guild: Some(GuildId::new(1)),
            channel: ChannelId::new(channel),
            message_id: id,
            author: UserId::new(2),
            data: json!({}),
        }
    }

    #[test]
    fn events_map_to_index_jobs() {
        assert_eq!(
            jobs_for_event(&ev_sent(5, 9)),
            vec![IndexPayload {
                op: IndexOp::Upsert,
                channel_id: 5,
                message_id: 9
            }]
        );
        let bulk = DomainEvent::MessagesBulkDeleted {
            guild: None,
            channel: ChannelId::new(5),
            message_ids: vec![1, 2],
        };
        assert_eq!(jobs_for_event(&bulk).len(), 2);
        assert!(
            jobs_for_event(&bulk)
                .iter()
                .all(|p| p.op == IndexOp::Delete)
        );
        let gone = DomainEvent::ChannelDeleted {
            guild: GuildId::new(1),
            channel: ChannelId::new(5),
            payload: json!({}),
            viewers: vec![],
        };
        assert_eq!(jobs_for_event(&gone)[0].op, IndexOp::DeleteChannel);
        assert!(
            jobs_for_event(&DomainEvent::MemberLeft {
                guild: GuildId::new(1),
                user: UserId::new(2)
            })
            .is_empty()
        );
    }

    #[test]
    fn payload_round_trips_as_json() {
        let p = IndexPayload {
            op: IndexOp::DeleteChannel,
            channel_id: 7,
            message_id: 0,
        };
        let v = serde_json::to_value(p).unwrap();
        assert_eq!(v["op"], "delete_channel");
        assert_eq!(serde_json::from_value::<IndexPayload>(v).unwrap(), p);
        // `message_id` may be omitted.
        let q: IndexPayload =
            serde_json::from_value(json!({"op": "upsert", "channel_id": 1})).unwrap();
        assert_eq!(q.message_id, 0);
    }

    struct World {
        indexer: Indexer,
        channel: ChannelId,
        clock: Arc<ManualClock>,
        svc: sr_chat::ChatService<MemoryBackend>,
    }

    async fn world() -> World {
        let clock = Arc::new(ManualClock::new(1_800_000_000_000));
        let dyn_clock: Arc<dyn Clock> = clock.clone();
        let backend = MemoryBackend::new();
        let ids = SnowflakeGenerator::new(dyn_clock.clone(), 9, 9).unwrap();
        let bp = sr_chat::guild_rules::plan_new_guild(UserId::new(1), "Club", 0, || ids.generate())
            .unwrap();
        let channel = bp.channels[2].id;
        backend.insert_guild(&bp).await.unwrap();
        backend.data().members.push(Member {
            guild_id: bp.guild.id,
            user_id: UserId::new(2),
            nick: None,
            role_ids: vec![],
            joined_at_ms: 0,
            communication_disabled_until_ms: None,
            pending: false,
        });
        let svc = sr_chat::ChatService::new(
            backend.clone(),
            dyn_clock,
            SnowflakeGenerator::new(clock.clone() as Arc<dyn Clock>, 1, 1).unwrap(),
            sr_chat::ChatLimits::default(),
        );
        let indexer = Indexer {
            index: Arc::new(MessageIndex::in_ram().unwrap()),
            repo: Arc::new(backend.clone()),
            store: Arc::new(backend.clone()),
        };
        World {
            indexer,
            channel,
            clock,
            svc,
        }
    }

    impl World {
        async fn say(&self, text: &str) -> MessageId {
            self.clock.advance_ms(5);
            self.svc
                .send_message(
                    UserId::new(2),
                    self.channel,
                    SendMessage {
                        content: text.to_owned(),
                        ..Default::default()
                    },
                )
                .await
                .unwrap()
                .id
        }

        fn hits(&self, q: &str) -> usize {
            let req = SearchRequest::new(q, Access::Channels(vec![self.channel]));
            self.indexer.index.search(&req).unwrap().total
        }
    }

    #[tokio::test]
    async fn upsert_and_delete_jobs_keep_the_index_current() {
        let w = world().await;
        let id = w.say("hello **world** https://example.com").await;
        let upsert = IndexPayload {
            op: IndexOp::Upsert,
            channel_id: w.channel.get(),
            message_id: id.get(),
        };
        w.indexer.handle(upsert).await.unwrap();
        w.indexer.handle(upsert).await.unwrap(); // replay is idempotent
        assert_eq!(w.hits("world"), 1);
        assert_eq!(w.hits("has:link"), 1);
        let page = w
            .indexer
            .index
            .search(&SearchRequest::new(
                "world",
                Access::Channels(vec![w.channel]),
            ))
            .unwrap();
        assert!(
            !page.hits[0].snippet.text.contains("**"),
            "markdown is stripped before indexing"
        );

        // Deleting the stored message and replaying the upsert removes it.
        w.svc
            .delete_message(UserId::new(2), w.channel, id)
            .await
            .unwrap();
        w.indexer.handle(upsert).await.unwrap();
        assert_eq!(w.hits("world"), 0);

        // Explicit delete and channel delete payloads.
        let id2 = w.say("second message").await;
        w.indexer
            .handle(IndexPayload {
                op: IndexOp::Upsert,
                channel_id: w.channel.get(),
                message_id: id2.get(),
            })
            .await
            .unwrap();
        assert_eq!(w.hits("second"), 1);
        w.indexer
            .handle(IndexPayload {
                op: IndexOp::DeleteChannel,
                channel_id: w.channel.get(),
                message_id: 0,
            })
            .await
            .unwrap();
        assert_eq!(w.hits("second"), 0);
        // A message that does not exist is just removed.
        w.indexer
            .handle(IndexPayload {
                op: IndexOp::Upsert,
                channel_id: w.channel.get(),
                message_id: 12345,
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn reindex_rebuilds_from_the_store_in_pages() {
        let w = world().await;
        for i in 0..7 {
            w.say(&format!("message number {i}")).await;
        }
        let deleted = w.say("message to delete").await;
        w.svc
            .delete_message(UserId::new(2), w.channel, deleted)
            .await
            .unwrap();
        // Stale content in the index is dropped by the rebuild.
        w.indexer
            .index
            .apply(&MessageEvent::Created(IndexedMessage {
                id: MessageId::new(99),
                channel_id: w.channel,
                guild_id: None,
                author_id: UserId::new(2),
                author_type: sr_search::AuthorType::User,
                content: "stale entry".into(),
                has: Default::default(),
                timestamp_ms: 1,
                pinned: false,
                mention_user_ids: vec![],
            }))
            .unwrap();
        assert_eq!(w.hits("stale"), 1);

        let n = w.indexer.reindex(&[w.channel]).await.unwrap();
        assert_eq!(n, 7, "deleted messages are not indexed");
        assert_eq!(w.hits("message"), 7);
        assert_eq!(w.hits("stale"), 0);
        assert_eq!(w.hits("delete"), 0);
    }
}
