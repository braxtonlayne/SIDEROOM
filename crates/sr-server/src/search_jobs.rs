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
//! Producers publish each event to its own topics (channel, guild, or each DM
//! recipient's user topic), so no single ordinary topic sees every message
//! event. [`spawn_firehose_consumer`] consumes a tap of everything published
//! (see [`crate::lag_bus::LagTapBus`]): it enqueues the index jobs ([`jobs_for_event`]) and
//! keeps the search access caches fresh ([`SearchDeps::apply_event`]). When the consumer lags and events are
//! dropped, a rate-limited `search.reindex` repairs the index ([`LagRecovery`]).

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::lag_bus::TapItem;
use futures::stream::BoxStream;
use futures::{FutureExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sr_api::routes::search::SearchDeps;
use sr_chat::Message;
use sr_chat::repo::{ChatRepository, MessageStore, Page};
use sr_core::{ChannelId, GuildId, MessageId};
use sr_db::jobs::{self, Job, NewJob};
use sr_db::runner::{HandlerError, JobRunner};
use sr_db::{DbResult, PgPool};
use sr_events::{DomainEvent, FirehoseDedup};
use sr_search::{MessageEvent, MessageIndex};
use tokio::task::JoinHandle;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

/// Job kind for a single message event.
pub const INDEX_MESSAGE_KIND: &str = "search.index_message";
/// Job kind for the full rebuild.
pub const REINDEX_KIND: &str = "search.reindex";

/// Messages read per page during a reindex.
const REINDEX_PAGE: usize = 1000;
/// Documents added between two commits of a reindex (RDY-PERF-003): each commit writes a segment and reloads the
/// reader, so committing per page made the job's time grow with the page count.
const REINDEX_COMMIT_EVERY: u64 = 50_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexOp {
    /// Created, edited or pin-changed: (re)index the stored message.
    Upsert,
    /// Deleted: remove from the index.
    Delete,
    /// The channel went away: remove every message in it.
    DeleteChannel,
    /// The guild was deleted: remove every message indexed under it.
    DeleteGuild,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexPayload {
    pub op: IndexOp,
    pub channel_id: u64,
    /// Only used by `delete_guild`.
    #[serde(default)]
    pub guild_id: u64,
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
        }
        | DomainEvent::MessageRestored {
            channel,
            message_id,
            ..
        } => vec![IndexPayload {
            op: IndexOp::Upsert,
            channel_id: channel.get(),
            message_id: *message_id,
            guild_id: 0,
        }],
        DomainEvent::MessageDeleted {
            channel,
            message_id,
            ..
        } => vec![IndexPayload {
            op: IndexOp::Delete,
            channel_id: channel.get(),
            message_id: *message_id,
            guild_id: 0,
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
                guild_id: 0,
            })
            .collect(),
        DomainEvent::ChannelDeleted { channel, .. } => vec![IndexPayload {
            op: IndexOp::DeleteChannel,
            channel_id: channel.get(),
            message_id: 0,
            guild_id: 0,
        }],
        DomainEvent::GuildDeleted { guild } => vec![IndexPayload {
            op: IndexOp::DeleteGuild,
            channel_id: 0,
            message_id: 0,
            guild_id: guild.get(),
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

/// Mints job ids (snowflakes).
pub type JobIds = Arc<dyn Fn() -> i64 + Send + Sync>;

/// Sees one event on the firehose: refreshes the search access caches and adds its index jobs to `out`.
/// `dedup` collapses the copies a multi-topic publish produces; the cache refresh is idempotent so it runs
/// on every copy.
fn consume_event(
    next_id: &JobIds,
    search: &SearchDeps,
    dedup: &mut FirehoseDedup,
    ev: &DomainEvent,
    out: &mut Vec<NewJob>,
) {
    search.apply_event(ev);
    let payloads = jobs_for_event(ev);
    if payloads.is_empty() || !dedup.is_new(ev) {
        return;
    }
    for p in &payloads {
        let payload = serde_json::to_value(p).unwrap_or_else(|_| json!({}));
        out.push(NewJob::new(next_id(), INDEX_MESSAGE_KIND, payload));
    }
}

/// Events the firehose consumer takes in one go when they are already waiting (RDY-PERF-002).
const FIREHOSE_BATCH: usize = 512;

/// Insert the index jobs gathered from a batch of events, in one statement.
async fn flush_jobs(pool: &PgPool, pending: &mut Vec<NewJob>) {
    if pending.is_empty() {
        return;
    }
    if let Err(e) = jobs::enqueue_many(pool, pending).await {
        tracing::error!(error = %e, jobs = pending.len(), "enqueueing search index jobs failed");
    }
    pending.clear();
}

/// Minimum time between two lag-triggered `search.reindex` jobs.
pub const LAG_REINDEX_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// Decides whether a lagged firehose warrants a full reindex: at most one per
/// [`LAG_REINDEX_INTERVAL`] (a reindex drops and rebuilds the whole index, so a
/// burst of lag reports must not queue one each).
#[derive(Debug, Default)]
pub struct LagRecovery {
    last: Option<Instant>,
}

impl LagRecovery {
    /// Record that `now` saw lag; true when a reindex should be enqueued.
    pub fn should_reindex(&mut self, now: Instant) -> bool {
        if self
            .last
            .is_some_and(|t| now.saturating_duration_since(t) < LAG_REINDEX_INTERVAL)
        {
            return false;
        }
        self.last = Some(now);
        true
    }
}

/// React to `missed` lost events: log, and enqueue `search.reindex` unless one
/// was queued within the last five minutes.
async fn recover_from_lag(pool: &PgPool, next_id: &JobIds, limiter: &mut LagRecovery, missed: u64) {
    if !limiter.should_reindex(Instant::now()) {
        tracing::warn!(
            missed,
            "search firehose lagged; a reindex was queued recently, not queueing another"
        );
        return;
    }
    match enqueue_reindex(pool, next_id()).await {
        Ok(()) => tracing::warn!(missed, "search firehose lagged; queued search.reindex"),
        Err(e) => tracing::error!(
            missed,
            error = %e,
            "search firehose lagged and queueing search.reindex failed"
        ),
    }
}

/// Consume the lag-aware event tap (see [`crate::lag_bus::LagTapBus::tap`]) and
/// feed the search subsystem: index jobs for message events (and guild deletes),
/// cache invalidation for permission-relevant ones, and a rate-limited
/// `search.reindex` when events were dropped for this consumer.
/// Subscribe to the tap before calling, so events published afterwards are seen.
pub fn spawn_firehose_consumer(
    mut events: BoxStream<'static, TapItem>,
    pool: PgPool,
    next_id: JobIds,
    search: Arc<SearchDeps>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut dedup = FirehoseDedup::new();
        let mut limiter = LagRecovery::default();
        let mut pending = Vec::new();
        while let Some(first) = events.next().await {
            // Take what is already waiting too, so a slow database costs one insert per batch instead of one
            // per message; one insert per message is what let the tap overflow under load (RDY-PERF-002).
            let mut batch = vec![first];
            while batch.len() < FIREHOSE_BATCH {
                match events.next().now_or_never() {
                    Some(Some(item)) => batch.push(item),
                    _ => break,
                }
            }
            for item in batch {
                match item {
                    Ok(ev) => consume_event(&next_id, &search, &mut dedup, &ev, &mut pending),
                    Err(BroadcastStreamRecvError::Lagged(missed)) => {
                        flush_jobs(&pool, &mut pending).await;
                        recover_from_lag(&pool, &next_id, &mut limiter, missed).await;
                    }
                }
            }
            flush_jobs(&pool, &mut pending).await;
        }
    })
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
        if msg.attachment_ids.is_empty() {
            return Ok(MessageEvent::from(msg));
        }
        let found = self
            .store
            .attachments_of(&[msg.id])
            .await
            .map_err(|e| IndexError::Repo(e.to_string()))?;
        Ok(Self::event_with(msg, &found))
    }

    /// The events of a page of messages, with one attachment lookup for the whole page instead of one per
    /// message (RDY-PERF-003, `search.reindex`).
    async fn events_for_page(&self, page: &[Message]) -> Result<Vec<MessageEvent>, IndexError> {
        let with: Vec<MessageId> = page
            .iter()
            .filter(|m| !m.attachment_ids.is_empty())
            .map(|m| m.id)
            .collect();
        let found = if with.is_empty() {
            Default::default()
        } else {
            self.store
                .attachments_of(&with)
                .await
                .map_err(|e| IndexError::Repo(e.to_string()))?
        };
        Ok(page.iter().map(|m| Self::event_with(m, &found)).collect())
    }

    fn event_with(
        msg: &Message,
        found: &std::collections::HashMap<MessageId, Vec<sr_chat::repo::Attachment>>,
    ) -> MessageEvent {
        let event = MessageEvent::from(msg);
        if msg.attachment_ids.is_empty() {
            return event;
        }
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
        match event {
            MessageEvent::Created(i) => MessageEvent::Created(i.with_attachments(kinds)),
            MessageEvent::Updated(i) => MessageEvent::Updated(i.with_attachments(kinds)),
            other => other,
        }
    }

    /// Apply one job payload to the index.
    pub async fn handle(&self, p: IndexPayload) -> Result<(), IndexError> {
        self.handle_many(&[p]).await
    }

    /// Apply several job payloads, in order, with one index commit (RDY-PERF-002: a commit and reader
    /// reload per message capped indexing at a few dozen messages per second).
    pub async fn handle_many(&self, payloads: &[IndexPayload]) -> Result<(), IndexError> {
        let mut events = Vec::with_capacity(payloads.len());
        for &p in payloads {
            events.push(self.event_of(p).await?);
        }
        self.apply(events).await
    }

    /// What one job payload does to the index, read from the stored message.
    async fn event_of(&self, p: IndexPayload) -> Result<MessageEvent, IndexError> {
        let channel = ChannelId::new(p.channel_id);
        Ok(match p.op {
            IndexOp::DeleteChannel => MessageEvent::ChannelDeleted(channel),
            IndexOp::DeleteGuild => MessageEvent::GuildDeleted(GuildId::new(p.guild_id)),
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
        })
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
        let mut committed = 0u64;
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
                let events = self.events_for_page(&page).await?;
                total += n as u64;
                self.add_bulk(events).await?;
                if total - committed >= REINDEX_COMMIT_EVERY {
                    self.commit_bulk().await?;
                    committed = total;
                }
                if n < REINDEX_PAGE {
                    break;
                }
                after = Some(last);
            }
        }
        self.commit_bulk().await?;
        Ok(total)
    }

    async fn add_bulk(&self, events: Vec<MessageEvent>) -> Result<(), IndexError> {
        let index = self.index.clone();
        tokio::task::spawn_blocking(move || index.add_bulk(&events))
            .await
            .map_err(|e| IndexError::Search(e.to_string()))?
            .map_err(|e| IndexError::Search(e.to_string()))
    }

    async fn commit_bulk(&self) -> Result<(), IndexError> {
        let index = self.index.clone();
        tokio::task::spawn_blocking(move || index.commit_bulk())
            .await
            .map_err(|e| IndexError::Search(e.to_string()))?
            .map_err(|e| IndexError::Search(e.to_string()))
    }
}

/// Index jobs applied together: the runner's job plus up to this many minus one more that are already due.
pub const INDEX_BATCH: i64 = 256;
/// `locked_by` of the jobs an index handler claims on top of the runner's.
const INDEX_BATCH_WORKER: &str = "search-index-batch";

fn index_payload(job: &Job) -> Result<IndexPayload, IndexError> {
    serde_json::from_value(job.payload.clone()).map_err(|e| IndexError::Payload(e.to_string()))
}

/// Runs one `search.index_message` job and, in the same index commit, the other index jobs already queued
/// behind it (RDY-PERF-002). The runner completes or fails its own job from the result; this handler does the
/// same for the ones it claimed. Jobs stay in queue order, so an edit after a delete still lands last.
async fn index_handler(indexer: Arc<Indexer>, pool: PgPool, job: Job) -> Result<(), HandlerError> {
    let first = index_payload(&job)?;
    let more = jobs::claim(
        &pool,
        &[INDEX_MESSAGE_KIND.to_owned()],
        INDEX_BATCH_WORKER,
        INDEX_BATCH - 1,
    )
    .await
    .unwrap_or_else(|e| {
        tracing::warn!(error = %e, "claiming more search index jobs failed; indexing one");
        Vec::new()
    });
    let mut payloads = vec![first];
    let mut claimed = Vec::with_capacity(more.len());
    for j in &more {
        match index_payload(j) {
            Ok(p) => {
                payloads.push(p);
                claimed.push(j.id);
            }
            Err(e) => {
                let _ = jobs::fail(&pool, j.id, &e.to_string()).await;
            }
        }
    }
    let outcome = indexer.handle_many(&payloads).await;
    match &outcome {
        Ok(()) => {
            if let Err(e) = jobs::complete_many(&pool, &claimed).await {
                tracing::error!(error = %e, "recording batched search index jobs failed");
            }
        }
        Err(err) => {
            for &id in &claimed {
                if let Err(e) = jobs::fail(&pool, id, &err.to_string()).await {
                    tracing::error!(job_id = id, error = %e, "recording a search index job failed");
                }
            }
        }
    }
    outcome?;
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

/// Register both search job kinds on `runner`. Message jobs run one handler at a time (the index has a
/// single writer); each takes the queued backlog with it, see [`INDEX_BATCH`].
pub fn register(runner: JobRunner, pool: &PgPool, indexer: Arc<Indexer>) -> JobRunner {
    let for_messages = indexer.clone();
    let pool = pool.clone();
    let messages_pool = pool.clone();
    runner
        .register(INDEX_MESSAGE_KIND, 1, move |job| {
            Box::pin(index_handler(
                for_messages.clone(),
                messages_pool.clone(),
                job,
            ))
        })
        .register(REINDEX_KIND, 1, move |job| {
            Box::pin(reindex_handler(indexer.clone(), pool.clone(), job))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `sr-cli reindex` queues this job by name (the CLI does not depend on this crate).
    #[test]
    fn the_cli_queues_the_job_kind_registered_here() {
        let ops = include_str!("../../sr-cli/src/ops.rs");
        assert!(
            ops.contains(&format!("REINDEX_JOB: &str = \"{REINDEX_KIND}\";")),
            "sr-cli ops.rs REINDEX_JOB must equal {REINDEX_KIND}"
        );
    }
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
                message_id: 9,
                guild_id: 0,
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
    fn guild_delete_maps_to_a_delete_guild_job_and_purges_the_index() {
        let jobs = jobs_for_event(&DomainEvent::GuildDeleted {
            guild: GuildId::new(7),
        });
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].op, IndexOp::DeleteGuild);
        assert_eq!(jobs[0].guild_id, 7);
        // `guild_id` defaults for old queued payloads.
        let q: IndexPayload =
            serde_json::from_value(json!({"op": "delete", "channel_id": 1, "message_id": 2}))
                .unwrap();
        assert_eq!(q.guild_id, 0);
    }

    #[test]
    fn lag_reindex_is_rate_limited_to_one_per_five_minutes() {
        let mut l = LagRecovery::default();
        let t0 = Instant::now();
        assert!(l.should_reindex(t0));
        assert!(!l.should_reindex(t0 + Duration::from_secs(1)));
        assert!(!l.should_reindex(t0 + LAG_REINDEX_INTERVAL - Duration::from_secs(1)));
        assert!(l.should_reindex(t0 + LAG_REINDEX_INTERVAL));
        assert!(!l.should_reindex(t0 + LAG_REINDEX_INTERVAL + Duration::from_secs(1)));
    }

    #[test]
    fn payload_round_trips_as_json() {
        let p = IndexPayload {
            op: IndexOp::DeleteChannel,
            channel_id: 7,
            message_id: 0,
            guild_id: 0,
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
            avatar_key: None,
            banner_key: None,
            bio: None,
            extras: Default::default(),
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
            guild_id: 0,
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
                guild_id: 0,
            })
            .await
            .unwrap();
        assert_eq!(w.hits("second"), 1);
        w.indexer
            .handle(IndexPayload {
                op: IndexOp::DeleteChannel,
                channel_id: w.channel.get(),
                message_id: 0,
                guild_id: 0,
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
                guild_id: 0,
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn delete_guild_job_purges_the_guilds_messages() {
        let w = world().await;
        let id = w.say("purge me please").await;
        w.indexer
            .handle(IndexPayload {
                op: IndexOp::Upsert,
                channel_id: w.channel.get(),
                message_id: id.get(),
                guild_id: 0,
            })
            .await
            .unwrap();
        let page = w
            .indexer
            .index
            .search(&SearchRequest::new(
                "purge",
                Access::Channels(vec![w.channel]),
            ))
            .unwrap();
        let guild = page.hits[0].guild_id.expect("a guild message");
        let delete = |g: u64| IndexPayload {
            op: IndexOp::DeleteGuild,
            channel_id: 0,
            message_id: 0,
            guild_id: g,
        };
        // Another guild's purge leaves it alone.
        w.indexer.handle(delete(guild.get() + 1)).await.unwrap();
        assert_eq!(w.hits("purge"), 1);
        w.indexer.handle(delete(guild.get())).await.unwrap();
        assert_eq!(w.hits("purge"), 0);
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

    /// RDY-PERF-002: several payloads go in with one commit, in order (a delete after an upsert wins).
    #[tokio::test]
    async fn a_batch_of_payloads_is_applied_in_order() {
        let w = world().await;
        let a = w.say("alpha batch").await;
        let b = w.say("bravo batch").await;
        let up = |id: MessageId| IndexPayload {
            op: IndexOp::Upsert,
            channel_id: w.channel.get(),
            message_id: id.get(),
            guild_id: 0,
        };
        let del_b = IndexPayload {
            op: IndexOp::Delete,
            ..up(b)
        };
        w.indexer.handle_many(&[up(a), up(b), del_b]).await.unwrap();
        assert_eq!(w.hits("alpha"), 1);
        assert_eq!(w.hits("bravo"), 0, "the later delete wins");
        w.indexer.handle_many(&[]).await.unwrap();
    }

    /// RDY-PERF-002: one handler run takes the queued index jobs with it and marks them done.
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn the_index_handler_drains_the_queue_in_one_commit() {
        let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
        let pool = sr_db::connect(&sr_db::DbConfig::new(url))
            .await
            .expect("connect");
        sr_db::migrate(&pool).await.expect("migrate");
        let w = world().await;
        let indexer = Arc::new(Indexer {
            index: w.indexer.index.clone(),
            repo: w.indexer.repo.clone(),
            store: w.indexer.store.clone(),
        });
        let ids: Vec<MessageId> = vec![
            w.say("one queued").await,
            w.say("two queued").await,
            w.say("three queued").await,
        ];
        let base = i64::try_from(std::process::id()).unwrap_or(1) * 1_000_000_000 + 7_000_000;
        let mut job_ids = Vec::new();
        for (n, id) in ids.iter().enumerate() {
            let job_id = base + i64::try_from(n).unwrap_or(0);
            let mut job = NewJob::new(
                job_id,
                INDEX_MESSAGE_KIND,
                serde_json::to_value(IndexPayload {
                    op: IndexOp::Upsert,
                    channel_id: w.channel.get(),
                    message_id: id.get(),
                    guild_id: 0,
                })
                .unwrap(),
            );
            job.priority = i16::MAX; // ahead of anything another suite left queued
            jobs::enqueue(&pool, &job).await.unwrap();
            job_ids.push(job_id);
        }
        let first = jobs::claim(&pool, &[INDEX_MESSAGE_KIND.to_owned()], "runner", 1)
            .await
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(first.id, job_ids[0]);
        index_handler(indexer, pool.clone(), first).await.unwrap();
        assert_eq!(w.hits("queued"), 3, "all three in the index");
        let states: Vec<(i64, String)> =
            sqlx::query_as("SELECT id, state::text FROM jobs WHERE id = ANY($1) ORDER BY id")
                .bind(&job_ids)
                .fetch_all(&pool)
                .await
                .unwrap();
        // The runner records its own job; the handler recorded the two it claimed.
        assert_eq!(
            states,
            vec![
                (job_ids[0], "running".to_owned()),
                (job_ids[1], "succeeded".to_owned()),
                (job_ids[2], "succeeded".to_owned()),
            ]
        );
        sqlx::query("DELETE FROM jobs WHERE id = ANY($1)")
            .bind(&job_ids)
            .execute(&pool)
            .await
            .unwrap();
    }

    /// RDY-PERF-002: a burst on the firehose becomes index jobs through batched inserts, one job per message
    /// (the copies a multi-topic publish makes are collapsed).
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn a_firehose_burst_is_queued_in_batches() {
        use crate::lag_bus::LagTapBus;
        use sr_events::{EventBus, InProcessBus};
        let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
        let pool = sr_db::connect(&sr_db::DbConfig::new(url))
            .await
            .expect("connect");
        sr_db::migrate(&pool).await.expect("migrate");
        let bus = LagTapBus::new(Arc::new(InProcessBus::new()));
        let next = Arc::new(std::sync::atomic::AtomicI64::new(
            i64::try_from(std::process::id()).unwrap_or(1) * 1_000_000_000 + 8_000_000,
        ));
        let first_job = next.load(std::sync::atomic::Ordering::Relaxed);
        let ids: JobIds = {
            let next = next.clone();
            Arc::new(move || next.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
        };
        let task = spawn_firehose_consumer(
            bus.tap(),
            pool.clone(),
            ids,
            Arc::new(SearchDeps::new(Arc::new(MessageIndex::in_ram().unwrap()))),
        );
        let base = 9_100_000_000u64 + u64::from(std::process::id()) * 1000;
        for n in 0..300u64 {
            let ev = ev_sent(77, base + n);
            for t in ev.topics() {
                bus.publish(t, ev.clone()).await.unwrap();
            }
        }
        let mut queued = 0i64;
        for _ in 0..100 {
            queued = sqlx::query_scalar(
                "SELECT count(*) FROM jobs WHERE kind = $1 AND id >= $2 AND id < $3",
            )
            .bind(INDEX_MESSAGE_KIND)
            .bind(first_job)
            .bind(first_job + 10_000)
            .fetch_one(&pool)
            .await
            .unwrap();
            if queued >= 300 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        task.abort();
        sqlx::query("DELETE FROM jobs WHERE id >= $1 AND id < $2")
            .bind(first_job)
            .bind(first_job + 10_000)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(queued, 300, "one index job per message, none lost");
    }
}
