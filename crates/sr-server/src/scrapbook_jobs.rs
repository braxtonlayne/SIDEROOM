//! The scrapbook vault's server side (PRF-SCR-001): the service account over real buckets, the job that
//! performs reaction-triggered archives, the sweep that retries failed bucket removals, and the consumer that
//! starts the cascade when a message or channel is deleted.
//!
//! * [`PgScrapbookBuckets`] is the module's **service account**. It may write to exactly one place: the
//!   server's own shelf named in `scrapbook_targets`, and only while the member who delegated `upload`
//!   still holds it (`scopes::may_upload`). Anything else is a [`Refusal`] and nothing is written.
//! * `scrapbook.requests` (singleton 1690, every 30 s): the module's reaction trigger leaves `req:<message>`
//!   rows in its runtime state; this job performs them. A request that cannot be served for a lasting
//!   reason (module off, message gone, nothing to keep, no shelf) is dropped; one that fails for a passing
//!   reason (bucket down) is retried with a backoff, five times.
//! * `scrapbook.removals` (singleton 1691, every 5 min): retries the bucket removals that are due and
//!   reconciles entries whose message is gone (lost events, retention purges, staff removals).
//! * [`spawn_cascade_consumer`]: the delete hook. A deleted message, a bulk delete or a deleted channel
//!   removes the entries made from it and takes their copies out of the bucket at once. It runs whether
//!   or not the module is still switched on: data must leave when the message does.
//!
//! The cascade never fails silently: the entry rows and the removal rows change in one transaction
//! (`sr_db::scrapbook_repo::delete_in`), and a bucket that refuses leaves a `failed` removal that the server's
//! managers read at `GET /guilds/{id}/scrapbook/removals`.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::BoxStream;
use serde_json::{Value, json};
use sqlx::PgPool;
use sr_api::routes::guilds::scrapbook::load_source;
use sr_chat::repo::{ChatRepository, MessageStore};
use sr_core::{ChannelId, Clock, GuildId, MessageId};
use sr_db::DbResult;
use sr_db::jobs::{self, Job};
use sr_db::runner::{HandlerError, JobRunner};
use sr_events::DomainEvent;
use sr_features::modules::scrapbook::{SCRAPBOOK_ID, ScrapbookConfig, parse_message_key};
use sr_features::state::StateEntry;
use sr_features::{FeatureCtx, ScopeRef};
use sr_storage::mount_storage::MountStorages;
use sr_storage::mounts::{MountOwner, MountResolver, MountStatus};
use sr_storage::scopes::{ScopeFacts, Viewer};
use sr_storage::scrapbook::{
    ArchiveError, ArchiveRequest, Archiver, DeleteScope, Refusal, ScrapbookBuckets, Target, Via,
};
use sr_storage::storage::{MultipartSession, ObjectInfo, ObjectStream, PresignMethod};
use sr_storage::{ObjectStorage, scrapbook::StoreError};
use tokio::task::JoinHandle;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

use crate::lag_bus::TapItem;

/// Kind of the request job.
pub const REQUESTS_KIND: &str = "scrapbook.requests";
/// Reserved singleton id (migration range 1690 to 1699; unique, checked by `job_ids_are_unique` below).
pub const REQUESTS_JOB_ID: i64 = 1690;
/// How often reaction-triggered requests are performed.
pub const REQUESTS_EVERY: Duration = Duration::from_secs(30);
/// Kind of the removal sweep.
pub const REMOVALS_KIND: &str = "scrapbook.removals";
/// Reserved singleton id.
pub const REMOVALS_JOB_ID: i64 = 1691;
/// How often failed removals are retried and orphaned entries reconciled.
pub const REMOVALS_EVERY: Duration = Duration::from_secs(300);
/// Requests handled per run.
const REQUESTS_PER_RUN: usize = 25;
/// Removals retried per run.
const REMOVALS_PER_RUN: i64 = 100;
/// Entries reconciled per run.
const RECONCILE_PER_RUN: i64 = 100;
/// Tries of a reaction-triggered request before it is dropped.
const MAX_REQUEST_ATTEMPTS: u64 = 5;

/// The module's service account over real mounts.
pub struct PgScrapbookBuckets {
    mounts: Arc<dyn MountResolver>,
    facts: Arc<dyn ScopeFacts>,
    storages: MountStorages,
}

impl PgScrapbookBuckets {
    /// Build one.
    pub fn new(
        mounts: Arc<dyn MountResolver>,
        facts: Arc<dyn ScopeFacts>,
        storages: MountStorages,
    ) -> Self {
        Self {
            mounts,
            facts,
            storages,
        }
    }
}

#[async_trait]
impl ScrapbookBuckets for PgScrapbookBuckets {
    async fn writer(&self, target: &Target) -> Result<Arc<dyn ObjectStorage>, Refusal> {
        let mount = self
            .mounts
            .by_id(target.mount_id)
            .await
            .map_err(|e| Refusal::Unavailable(e.to_string()))?
            .ok_or(Refusal::MountGone)?;
        // The service account never writes outside the server's own shelf: not to a member's bucket, not to
        // another server's, not to the instance bucket.
        if mount.owner != MountOwner::Guild(target.guild_id) {
            return Err(Refusal::NotTheServers);
        }
        if mount.status != MountStatus::Active {
            return Err(Refusal::Disabled);
        }
        // `upload` was delegated by a member; it ends when they lose it.
        let ok = sr_storage::scopes::may_upload(
            &*self.facts,
            Viewer {
                id: target.granted_by,
                staff: false,
            },
            &mount,
        )
        .await
        .map_err(|e| Refusal::Unavailable(e.to_string()))?;
        if !ok {
            return Err(Refusal::GrantLost);
        }
        self.storages
            .for_mount(mount.id)
            .await
            .map(Deadlines::wrap)
            .map_err(|e| Refusal::Unavailable(e.to_string()))
    }

    async fn remover(&self, mount_id: i64) -> Result<Arc<dyn ObjectStorage>, String> {
        self.storages
            .for_mount(mount_id)
            .await
            .map(Deadlines::wrap)
            .map_err(|e| e.to_string())
    }
}

/// Longest wait for a bucket call that moves no bulk data (open, head, delete, list, sign).
const QUICK: Duration = Duration::from_secs(10);
/// Longest wait for a call that moves data (a whole object or one part of a multipart upload).
const BULK: Duration = Duration::from_secs(120);

/// A member's or operator's bucket is a third party: `object_store` retries a dead endpoint for minutes, and a
/// scrapbook request (an archive, a delete, a retry button) must not hang that long. Every call gets a deadline;
/// a call that runs out is an ordinary bucket failure, which the archive reports and the cascade lists for the
/// managers like any other.
struct Deadlines {
    inner: Arc<dyn ObjectStorage>,
}

impl Deadlines {
    fn wrap(inner: Arc<dyn ObjectStorage>) -> Arc<dyn ObjectStorage> {
        Arc::new(Self { inner })
    }
}

async fn within<T>(
    wait: Duration,
    what: &str,
    f: impl std::future::Future<Output = sr_storage::Result<T>>,
) -> sr_storage::Result<T> {
    match tokio::time::timeout(wait, f).await {
        Ok(r) => r,
        Err(_) => Err(sr_storage::StorageError::Backend(format!(
            "{what} timed out after {} s",
            wait.as_secs()
        ))),
    }
}

struct DeadlineSession(Box<dyn MultipartSession>);

#[async_trait]
impl MultipartSession for DeadlineSession {
    async fn put_part(&mut self, data: bytes::Bytes) -> sr_storage::Result<()> {
        within(BULK, "uploading a part", self.0.put_part(data)).await
    }
    async fn complete(&mut self) -> sr_storage::Result<()> {
        within(BULK, "completing the upload", self.0.complete()).await
    }
    async fn abort(&mut self) -> sr_storage::Result<()> {
        within(QUICK, "aborting the upload", self.0.abort()).await
    }
}

#[async_trait]
impl ObjectStorage for Deadlines {
    async fn put(&self, key: &str, data: bytes::Bytes) -> sr_storage::Result<()> {
        within(BULK, "writing the object", self.inner.put(key, data)).await
    }
    async fn get(&self, key: &str) -> sr_storage::Result<bytes::Bytes> {
        within(BULK, "reading the object", self.inner.get(key)).await
    }
    async fn get_range(&self, key: &str, s: u64, e: u64) -> sr_storage::Result<bytes::Bytes> {
        within(BULK, "reading the object", self.inner.get_range(key, s, e)).await
    }
    async fn get_stream(
        &self,
        key: &str,
        range: Option<std::ops::Range<u64>>,
    ) -> sr_storage::Result<ObjectStream> {
        within(
            QUICK,
            "opening the object",
            self.inner.get_stream(key, range),
        )
        .await
    }
    async fn head(&self, key: &str) -> sr_storage::Result<ObjectInfo> {
        within(QUICK, "asking for the object", self.inner.head(key)).await
    }
    async fn delete(&self, key: &str) -> sr_storage::Result<()> {
        within(QUICK, "deleting the object", self.inner.delete(key)).await
    }
    async fn list(&self, prefix: &str) -> sr_storage::Result<Vec<ObjectInfo>> {
        within(QUICK, "listing the bucket", self.inner.list(prefix)).await
    }
    async fn presign(
        &self,
        method: PresignMethod,
        key: &str,
        ttl: Duration,
    ) -> sr_storage::Result<String> {
        within(QUICK, "signing", self.inner.presign(method, key, ttl)).await
    }
    async fn multipart_create(&self, key: &str) -> sr_storage::Result<Box<dyn MultipartSession>> {
        let s = within(
            QUICK,
            "starting the upload",
            self.inner.multipart_create(key),
        )
        .await?;
        Ok(Box::new(DeadlineSession(s)))
    }
}

/// Everything the jobs need.
pub struct Worker {
    /// Archives and cascades.
    pub archiver: Arc<Archiver>,
    /// Messages.
    pub chat: Arc<dyn ChatRepository>,
    /// Attachments.
    pub messages: Arc<dyn MessageStore>,
    /// The feature runtime's handles: settings and the module's request rows.
    pub ctx: FeatureCtx,
    /// The database.
    pub pool: PgPool,
    /// Time.
    pub clock: Arc<dyn Clock>,
}

/// Delay before the next try of a request that has failed `attempts` times.
pub fn request_backoff(attempts: u64) -> Duration {
    Duration::from_secs(60 * (1_u64 << attempts.min(8)))
}

impl Worker {
    fn now(&self) -> i64 {
        self.clock.now_unix_ms()
    }

    /// Perform the due reaction requests. Returns how many were archived.
    pub async fn run_requests(&self) -> Result<usize, String> {
        let now = self.now();
        let due = self
            .ctx
            .state
            .due(SCRAPBOOK_ID, now, REQUESTS_PER_RUN)
            .await
            .map_err(|e| e.to_string())?;
        let mut archived = 0;
        for e in due {
            if self.perform(e, now).await? {
                archived += 1;
            }
        }
        Ok(archived)
    }

    async fn drop_request(&self, guild: GuildId, key: &str) -> Result<(), String> {
        self.ctx
            .state
            .delete(SCRAPBOOK_ID, guild, key)
            .await
            .map_err(|e| e.to_string())
    }

    /// One request. `true` when a new entry was made.
    async fn perform(&self, e: StateEntry, now: i64) -> Result<bool, String> {
        let guild = e.guild;
        let Some(message) = parse_message_key(&e.key) else {
            self.drop_request(guild, &e.key).await?;
            return Ok(false);
        };
        let channel = e.value.get("channel").and_then(Value::as_u64).unwrap_or(0);
        // The module may have been switched off, or the operator may have withdrawn it, since the reaction.
        let on = self
            .ctx
            .settings
            .is_enabled(SCRAPBOOK_ID, &ScopeRef::Guild { guild })
            .await
            .map_err(|e| e.to_string())?;
        if !on || channel == 0 {
            self.drop_request(guild, &e.key).await?;
            return Ok(false);
        }
        let Some(src) = load_source(
            &*self.chat,
            &*self.messages,
            guild,
            ChannelId::new(channel),
            MessageId::new(message),
        )
        .await?
        else {
            self.drop_request(guild, &e.key).await?;
            return Ok(false);
        };
        let platform = ScrapbookConfig::from_values(
            &self
                .ctx
                .settings
                .effective_settings(SCRAPBOOK_ID, &ScopeRef::Platform)
                .await
                .map_err(|e| e.to_string())?,
        );
        let outcome = self
            .archiver
            .archive(ArchiveRequest {
                message: src,
                by: None,
                via: Via::Reaction,
                tags: Vec::new(),
                max_object_bytes: platform.max_object_bytes(),
                now_ms: now,
            })
            .await;
        match outcome {
            Ok(sr_storage::scrapbook::ArchiveOutcome::Archived(_)) => {
                self.drop_request(guild, &e.key).await?;
                Ok(true)
            }
            // Already there, nothing to keep, or no shelf chosen: not something a retry fixes. The managers
            // see a missing or refused shelf in the module's status.
            Ok(_) | Err(ArchiveError::Empty | ArchiveError::NoTarget) => {
                self.drop_request(guild, &e.key).await?;
                Ok(false)
            }
            Err(err) => {
                let attempts = e.value.get("attempts").and_then(Value::as_u64).unwrap_or(0) + 1;
                if attempts >= MAX_REQUEST_ATTEMPTS {
                    tracing::warn!(guild = guild.get(), message, error = %err, "scrapbook archive given up");
                    self.drop_request(guild, &e.key).await?;
                } else {
                    tracing::info!(guild = guild.get(), message, error = %err, attempts, "scrapbook archive will be retried");
                    let due = now + request_backoff(attempts).as_millis() as i64;
                    self.ctx
                        .state
                        .put(
                            StateEntry::new(
                                SCRAPBOOK_ID,
                                guild,
                                e.key.clone(),
                                json!({"channel": channel, "via": "reaction", "attempts": attempts}),
                            )
                            .due(due),
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                }
                Ok(false)
            }
        }
    }

    /// Retry due removals and reconcile entries whose message is gone. Returns `(removed, still failing,
    /// reconciled entries)`.
    pub async fn sweep(&self) -> Result<(u64, u64, u64), String> {
        let now = self.now();
        let (removed, failing) = self
            .archiver
            .drain(now, REMOVALS_PER_RUN)
            .await
            .map_err(|e| e.to_string())?;
        let reconciled = self.reconcile(now).await?;
        Ok((removed, failing, reconciled))
    }

    /// Entries whose message is no longer visible go (the cascade, run from the table side).
    pub async fn reconcile(&self, now: i64) -> Result<u64, String> {
        let orphans = orphaned_messages(&self.pool, RECONCILE_PER_RUN)
            .await
            .map_err(|e| e.to_string())?;
        let mut entries = 0;
        for m in orphans {
            entries += self
                .archiver
                .cascade(DeleteScope::Message(m), now)
                .await
                .map_err(|e: StoreError| e.to_string())?
                .entries;
        }
        Ok(entries)
    }
}

/// Messages that have an entry but are gone, deleted or hidden.
pub async fn orphaned_messages(pool: &PgPool, limit: i64) -> Result<Vec<i64>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT e.message_id FROM scrapbook_entries e \
         WHERE NOT EXISTS (SELECT 1 FROM messages m WHERE m.id = e.message_id AND m.channel_id = e.channel_id \
                             AND m.visibility = 'visible' AND m.deleted_at IS NULL) \
         ORDER BY e.id LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// The delete hook: start the cascade for every deleted message and channel seen on the event tap.
pub fn spawn_cascade_consumer(
    mut events: BoxStream<'static, TapItem>,
    archiver: Arc<Archiver>,
    clock: Arc<dyn Clock>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(item) = events.next().await {
            let scopes: Vec<DeleteScope> = match item {
                Ok(DomainEvent::MessageDeleted { message_id, .. }) => {
                    vec![DeleteScope::Message(message_id as i64)]
                }
                Ok(DomainEvent::MessagesBulkDeleted { message_ids, .. }) => message_ids
                    .into_iter()
                    .map(|m| DeleteScope::Message(m as i64))
                    .collect(),
                Ok(DomainEvent::ChannelDeleted { channel, .. }) => {
                    vec![DeleteScope::Channel(channel.get() as i64)]
                }
                Ok(_) => continue,
                Err(BroadcastStreamRecvError::Lagged(missed)) => {
                    // The sweep (`scrapbook.removals`) reconciles whatever was missed.
                    tracing::warn!(
                        missed,
                        "scrapbook cascade consumer lagged; the sweep will catch up"
                    );
                    continue;
                }
            };
            for scope in scopes {
                match archiver.cascade(scope, clock.now_unix_ms()).await {
                    Ok(r) if r.entries > 0 => {
                        tracing::info!(
                            entries = r.entries,
                            removed = r.removed,
                            failed = r.failed,
                            "scrapbook cascade"
                        );
                    }
                    Ok(_) => {}
                    // The sweep finds the entry again; say so loudly meanwhile.
                    Err(e) => {
                        tracing::error!(error = %e, "scrapbook cascade failed; the sweep will retry")
                    }
                }
            }
        }
    })
}

async fn requests_handler(w: Arc<Worker>, job: Job) -> Result<(), HandlerError> {
    match w.run_requests().await {
        Ok(n) if n > 0 => tracing::info!(archived = n, "scrapbook requests"),
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "scrapbook requests failed"),
    }
    jobs::reschedule_running(&w.pool, job.id, REQUESTS_EVERY).await?;
    Ok(())
}

async fn removals_handler(w: Arc<Worker>, job: Job) -> Result<(), HandlerError> {
    match w.sweep().await {
        Ok((_, failing, reconciled)) => {
            if failing > 0 {
                tracing::warn!(
                    failing,
                    "scrapbook copies still cannot be removed from their bucket"
                );
            }
            if reconciled > 0 {
                tracing::info!(reconciled, "scrapbook entries of gone messages removed");
            }
        }
        Err(e) => tracing::warn!(error = %e, "scrapbook removal sweep failed"),
    }
    jobs::reschedule_running(&w.pool, job.id, REMOVALS_EVERY).await?;
    Ok(())
}

/// Register `scrapbook.requests` and `scrapbook.removals`. Pair with [`schedule`].
pub fn register(runner: JobRunner, worker: Arc<Worker>) -> JobRunner {
    let a = worker.clone();
    let b = worker;
    runner
        .register(REQUESTS_KIND, 1, move |job| {
            Box::pin(requests_handler(a.clone(), job))
        })
        .register(REMOVALS_KIND, 1, move |job| {
            Box::pin(removals_handler(b.clone(), job))
        })
}

/// Create or re-arm the singletons. Call only when [`register`] is on the runner.
pub async fn schedule(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(
        pool,
        REQUESTS_JOB_ID,
        REQUESTS_KIND,
        &json!({}),
        REQUESTS_EVERY,
    )
    .await?;
    jobs::schedule_singleton(
        pool,
        REMOVALS_JOB_ID,
        REMOVALS_KIND,
        &json!({}),
        REMOVALS_EVERY,
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_backoff_doubles_from_a_minute_and_caps() {
        assert_eq!(request_backoff(0), Duration::from_secs(60));
        assert_eq!(request_backoff(1), Duration::from_secs(120));
        assert_eq!(request_backoff(3), Duration::from_secs(480));
        assert_eq!(request_backoff(50), Duration::from_secs(60 * 256));
    }

    #[test]
    fn job_ids_are_in_the_waves_range_and_distinct() {
        assert_ne!(REQUESTS_JOB_ID, REMOVALS_JOB_ID);
        for id in [REQUESTS_JOB_ID, REMOVALS_JOB_ID] {
            assert!((1690..=1699).contains(&id));
        }
        assert_ne!(REQUESTS_KIND, REMOVALS_KIND);
    }
}
