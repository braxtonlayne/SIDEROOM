//! PRF-SCR-001: the scrapbook vault's core. A message is copied into the server's own bucket and described
//! by an entry; deleting the message removes both again.
//!
//! This file holds the parts that do not depend on a database or on the HTTP layer, so the rules are testable
//! with an in-memory store and in-memory buckets:
//!
//! * the data model ([`Entry`], [`Object`], [`Removal`]) and the [`ScrapbookStore`] port,
//! * the key layout `scrapbook/<yyyy>/<mm>/<message_id>/<n>-<file>` ([`object_key`]),
//! * [`copy_object`], **the one allowed server-side copy** of the BYOS zero-proxy rule (roadmap section 2): one
//!   object at a time, from the instance bucket into the server's own bucket, never more than the size cap,
//! * the [`Archiver`], which archives a message and runs the cascade when it goes away.
//!
//! # The service account
//!
//! The module writes as a service account whose only right is `upload` on one Guild Shelf. The shelf is the
//! row of `scrapbook_targets`; [`ScrapbookBuckets::writer`] decides, on every copy, whether the account still
//! holds that right (the mount is the server's, is switched on, and the member who delegated it still holds
//! `upload`). A refusal stops the archive with [`ArchiveError::TargetRefused`] and nothing is written.
//!
//! # The cascade is never silent
//!
//! Deleting an entry writes one [`Removal`] per object in the same transaction. The bucket is a third party's
//! (a member's S3, an operator's Garage) and may be down or have revoked the key. A removal that fails keeps
//! its row with the error, the attempt count and the next try, and the server's managers read the list
//! (`GET /guilds/{id}/scrapbook/removals`): the gallery shows "a copy could not be removed" until it is gone
//! or a manager dismisses it. [`Archiver::drain`] retries with a capped backoff; nothing gives up quietly.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;

use crate::error::StorageError;
use crate::path::safe_key;
use crate::storage::ObjectStorage;

/// Default for `scrapbook.max_object_mib`, and its ceiling.
pub const MAX_OBJECT_MIB: u64 = 200;
/// Attachments of one message that are copied; Discord allows ten per message.
pub const MAX_OBJECTS_PER_ENTRY: usize = 10;
/// Tags on one entry, and the length of each.
pub const MAX_TAGS: usize = 12;
/// Longest tag in characters.
pub const MAX_TAG_LEN: usize = 32;
/// Part size of a streamed copy (S3 wants at least 5 MiB for every part but the last).
const PART_BYTES: usize = 8 * 1024 * 1024;
/// Longest wait between two tries of a failed removal.
const MAX_BACKOFF_MS: i64 = 6 * 3_600_000;

/// How an entry came to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    /// A message collected enough of a trigger reaction.
    Reaction,
    /// A permitted member used "Send to Scrapbook".
    Manual,
}

impl Via {
    /// Column value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reaction => "reaction",
            Self::Manual => "manual",
        }
    }
    /// Parse the column value.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "reaction" => Some(Self::Reaction),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }
}

/// What a copied file is, for the gallery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    /// Shown inline.
    Image,
    /// Played inline.
    Video,
    /// Played inline.
    Audio,
    /// A link only.
    File,
}

impl ObjectKind {
    /// Column value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::File => "file",
        }
    }
    /// Parse the column value.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "image" => Some(Self::Image),
            "video" => Some(Self::Video),
            "audio" => Some(Self::Audio),
            "file" => Some(Self::File),
            _ => None,
        }
    }
    /// From the content type the upload was sniffed as, falling back to the extension.
    pub fn of(content_type: &str, filename: &str) -> Self {
        let ct = content_type.to_ascii_lowercase();
        let ext = filename
            .rsplit_once('.')
            .map(|(_, e)| e.to_ascii_lowercase())
            .unwrap_or_default();
        if ct.starts_with("image/")
            || matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp")
        {
            Self::Image
        } else if ct.starts_with("video/") || matches!(ext.as_str(), "mp4" | "webm" | "mov" | "m4v")
        {
            Self::Video
        } else if ct.starts_with("audio/") || matches!(ext.as_str(), "mp3" | "ogg" | "opus" | "wav")
        {
            Self::Audio
        } else {
            Self::File
        }
    }
}

/// The Guild Shelf the module writes to and who delegated the upload grant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// The server.
    pub guild_id: i64,
    /// The mount copies go to.
    pub mount_id: i64,
    /// The manager who delegated `upload` to the module's service account.
    pub granted_by: i64,
}

/// One archived message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Snowflake.
    pub id: i64,
    /// The server.
    pub guild_id: i64,
    /// Source channel (the entry is visible to whoever can view it).
    pub channel_id: i64,
    /// Source message.
    pub message_id: i64,
    /// Message author.
    pub author_id: i64,
    /// The author's display name when archived.
    pub author_name: String,
    /// The message text.
    pub content: String,
    /// When the message was sent, unix ms.
    pub sent_at_ms: i64,
    /// Who archived it by hand; `None` for a reaction.
    pub archived_by: Option<i64>,
    /// How it came to be.
    pub via: Via,
    /// Tags.
    pub tags: Vec<String>,
    /// The mount holding the copies.
    pub mount_id: i64,
    /// Attachments that were not copied (too large, or past the per-message limit).
    pub skipped: i32,
    /// When the entry was made, unix ms.
    pub created_at_ms: i64,
}

/// One copy in a bucket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object {
    /// Snowflake; also the id of its removal row.
    pub id: i64,
    /// The entry.
    pub entry_id: i64,
    /// Order in the message.
    pub position: i32,
    /// The mount.
    pub mount_id: i64,
    /// Mount-relative key.
    pub key: String,
    /// The file name the author gave it.
    pub filename: String,
    /// Content type.
    pub content_type: String,
    /// Size in bytes.
    pub size: i64,
    /// What it is.
    pub kind: ObjectKind,
}

/// An entry with its copies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryWithObjects {
    /// The entry.
    pub entry: Entry,
    /// Its copies, in message order.
    pub objects: Vec<Object>,
}

/// Where a bucket removal stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovalState {
    /// Not tried yet.
    Pending,
    /// Tried and failed at least once; shown to managers.
    Failed,
}

impl RemovalState {
    /// Column value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Failed => "failed",
        }
    }
    /// Parse the column value.
    pub fn parse(s: &str) -> Self {
        if s == "failed" {
            Self::Failed
        } else {
            Self::Pending
        }
    }
}

/// An object that has to leave a bucket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removal {
    /// Snowflake (the removed object's id).
    pub id: i64,
    /// The server the copy belonged to.
    pub guild_id: i64,
    /// The mount.
    pub mount_id: i64,
    /// Mount-relative key.
    pub key: String,
    /// The message the copy was made from.
    pub message_id: Option<i64>,
    /// Pending or failed.
    pub state: RemovalState,
    /// Tries so far.
    pub attempts: i32,
    /// Why the last try failed.
    pub last_error: String,
    /// When it was queued, unix ms.
    pub created_at_ms: i64,
    /// When the next try is due, unix ms.
    pub next_attempt_ms: i64,
}

/// Which entries a delete takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteScope {
    /// One entry of a server.
    Entry {
        /// The server.
        guild_id: i64,
        /// The entry.
        id: i64,
    },
    /// The entry made from a message (the message was deleted).
    Message(i64),
    /// Entries made from messages of a channel (the channel was deleted).
    Channel(i64),
    /// Entries about a member's messages (erasure of the member).
    Author(i64),
}

/// What a delete did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Deleted {
    /// Entries removed.
    pub entries: u64,
    /// Removal rows queued (one per copy).
    pub queued: Vec<i64>,
}

/// Which entries a gallery page lists.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    /// Channels the viewer may view. Entries of any other channel are never listed.
    pub viewable: Vec<i64>,
    /// Only this channel.
    pub channel_id: Option<i64>,
    /// Only messages by this member.
    pub author_id: Option<i64>,
    /// Only this tag.
    pub tag: Option<String>,
    /// Sent at or after, unix ms.
    pub from_ms: Option<i64>,
    /// Sent before, unix ms.
    pub to_ms: Option<i64>,
}

/// The filter choices a gallery offers, counted over what the viewer may see.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facets {
    /// `(tag, entries)`.
    pub tags: Vec<(String, i64)>,
    /// `(author id, name, entries)`.
    pub authors: Vec<(i64, String, i64)>,
    /// `(channel id, entries)`.
    pub channels: Vec<(i64, i64)>,
}

/// The store failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("scrapbook store: {0}")]
pub struct StoreError(pub String);

/// Result of a store call.
pub type StoreResult<T> = Result<T, StoreError>;

/// Persistence of entries, copies and pending removals. Postgres in production
/// (`sr_db::scrapbook_repo::PgScrapbook`), [`MemoryScrapbook`] in tests.
#[async_trait]
pub trait ScrapbookStore: Send + Sync {
    /// The server's target shelf.
    async fn target(&self, guild: i64) -> StoreResult<Option<Target>>;
    /// Set the target shelf.
    async fn set_target(&self, target: &Target) -> StoreResult<()>;
    /// Remove the target; `false` when there was none. Existing entries keep their copies.
    async fn clear_target(&self, guild: i64) -> StoreResult<bool>;
    /// Insert an entry with its copies. `false` when the message already has an entry (nothing written).
    async fn insert(&self, entry: &Entry, objects: &[Object]) -> StoreResult<bool>;
    /// The entry made from `message`.
    async fn by_message(&self, guild: i64, message: i64) -> StoreResult<Option<EntryWithObjects>>;
    /// One entry.
    async fn get(&self, guild: i64, id: i64) -> StoreResult<Option<EntryWithObjects>>;
    /// A gallery page, newest message first: entries with `id < before`, at most `limit`.
    async fn list(
        &self,
        guild: i64,
        filter: &Filter,
        before: Option<i64>,
        limit: i64,
    ) -> StoreResult<Vec<EntryWithObjects>>;
    /// Filter choices over `viewable` channels.
    async fn facets(&self, guild: i64, viewable: &[i64]) -> StoreResult<Facets>;
    /// Replace the tags; `false` when the entry does not exist.
    async fn set_tags(&self, guild: i64, id: i64, tags: &[String]) -> StoreResult<bool>;
    /// Delete entries and queue one removal per copy, in one transaction.
    async fn delete(&self, scope: DeleteScope) -> StoreResult<Deleted>;
    /// Removals whose next try is due.
    async fn due_removals(&self, now_ms: i64, limit: i64) -> StoreResult<Vec<Removal>>;
    /// One removal.
    async fn removal(&self, id: i64) -> StoreResult<Option<Removal>>;
    /// The copy is gone: forget the removal.
    async fn removal_done(&self, id: i64) -> StoreResult<()>;
    /// The try failed: keep the row with the error and the next try.
    async fn removal_failed(&self, id: i64, error: &str, next_ms: i64) -> StoreResult<()>;
    /// A server's removals that have failed at least once, oldest first.
    async fn failed_removals(&self, guild: i64) -> StoreResult<Vec<Removal>>;
    /// A manager asks for another try now; `false` when no such removal.
    async fn retry_now(&self, guild: i64, id: i64, now_ms: i64) -> StoreResult<bool>;
    /// A manager confirms the copy is gone some other way; `false` when no such removal.
    async fn dismiss(&self, guild: i64, id: i64) -> StoreResult<bool>;
    /// A copy that belongs to no entry (a failed archive that could not clean up after itself) goes into the
    /// removal list as already failed, so it is retried and shown like any other.
    async fn queue_orphan(
        &self,
        guild: i64,
        mount_id: i64,
        key: &str,
        message: i64,
        error: &str,
    ) -> StoreResult<()>;
}

/// Why the service account may not write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The mount no longer exists.
    MountGone,
    /// The mount is not this server's.
    NotTheServers,
    /// The mount is switched off, delisted or its owner is not active.
    Disabled,
    /// The member who delegated `upload` no longer holds it.
    GrantLost,
    /// The bucket could not be opened.
    Unavailable(String),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MountGone => f.write_str("the scrapbook shelf no longer exists"),
            Self::NotTheServers => f.write_str("the scrapbook shelf is not this server's"),
            Self::Disabled => f.write_str("the scrapbook shelf is switched off"),
            Self::GrantLost => f.write_str("the member who allowed uploads no longer can"),
            Self::Unavailable(e) => write!(f, "the scrapbook shelf cannot be opened: {e}"),
        }
    }
}

/// The buckets, as the module's service account reaches them.
#[async_trait]
pub trait ScrapbookBuckets: Send + Sync {
    /// The target's bucket, if the service account still holds `upload` on it.
    async fn writer(&self, target: &Target) -> Result<Arc<dyn ObjectStorage>, Refusal>;
    /// A bucket handle for deleting the module's own copies. Deleting needs no grant: a copy must be
    /// removable even after the shelf was switched off, as long as the credentials still work.
    async fn remover(&self, mount_id: i64) -> Result<Arc<dyn ObjectStorage>, String>;
}

/// A file name that is safe inside a key: ASCII letters, digits, `.`, `-` and `_`, at most 80 bytes.
pub fn safe_name(filename: &str) -> String {
    let base = filename.rsplit(['/', '\\']).next().unwrap_or(filename);
    let mut out: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    out = out.trim_start_matches('.').to_owned();
    if out.len() > 80 {
        // Keep the extension.
        let ext = out
            .rsplit_once('.')
            .map(|(_, e)| e.chars().take(10).collect::<String>())
            .unwrap_or_default();
        let stem: String = out.chars().take(60).collect();
        out = if ext.is_empty() {
            stem
        } else {
            format!("{stem}.{ext}")
        };
    }
    if out.is_empty() { "file".into() } else { out }
}

/// `(year, month)` of a unix-ms timestamp, UTC.
pub fn year_month(ms: i64) -> (i64, u32) {
    let days = ms.div_euclid(86_400_000);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32)
}

/// The key of a message's n-th copy: `scrapbook/<yyyy>/<mm>/<message_id>/<n>-<file>`.
pub fn object_key(sent_at_ms: i64, message_id: i64, position: usize, filename: &str) -> String {
    let (y, m) = year_month(sent_at_ms);
    format!(
        "scrapbook/{y:04}/{m:02}/{message_id}/{position}-{}",
        safe_name(filename)
    )
}

/// Clean a list of tags: trimmed, lower-case, no `#`, unique, at most [`MAX_TAGS`] of [`MAX_TAG_LEN`] characters.
pub fn clean_tags(raw: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in raw {
        let t: String = t
            .trim()
            .trim_start_matches('#')
            .chars()
            .filter(|c| !c.is_control())
            .collect::<String>()
            .to_lowercase()
            .chars()
            .take(MAX_TAG_LEN)
            .collect();
        if !t.is_empty() && !out.contains(&t) {
            out.push(t);
        }
        if out.len() == MAX_TAGS {
            break;
        }
    }
    out
}

/// Why a copy did not happen.
#[derive(Debug, thiserror::Error)]
pub enum CopyError {
    /// The source is larger than the cap.
    #[error("{size} bytes is more than the {max} byte limit")]
    TooLarge {
        /// Size of the source.
        size: u64,
        /// The cap.
        max: u64,
    },
    /// The source is gone.
    #[error("the original file is gone")]
    SourceGone,
    /// A bucket call failed.
    #[error("bucket error: {0}")]
    Bucket(String),
}

/// **The one allowed server-side copy** (roadmap section 2, "BYOS: zero proxy"). Copies one object from `src`
/// to `dst`, never more than `max_bytes`: the size is checked up front and again while streaming, so a source
/// that grows or lies cannot exceed the cap. Small objects go in one request, larger ones as a multipart
/// upload of 8 MiB parts that is aborted on any failure. Returns the bytes written.
pub async fn copy_object(
    src: &dyn ObjectStorage,
    src_key: &str,
    dst: &dyn ObjectStorage,
    dst_key: &str,
    max_bytes: u64,
) -> Result<u64, CopyError> {
    let info = src.head(src_key).await.map_err(|e| match e {
        StorageError::NotFound(_) => CopyError::SourceGone,
        other => CopyError::Bucket(other.to_string()),
    })?;
    if info.size > max_bytes {
        return Err(CopyError::TooLarge {
            size: info.size,
            max: max_bytes,
        });
    }
    if info.size as usize <= PART_BYTES {
        let data = src.get(src_key).await.map_err(|e| match e {
            StorageError::NotFound(_) => CopyError::SourceGone,
            other => CopyError::Bucket(other.to_string()),
        })?;
        if data.len() as u64 > max_bytes {
            return Err(CopyError::TooLarge {
                size: data.len() as u64,
                max: max_bytes,
            });
        }
        let n = data.len() as u64;
        dst.put(dst_key, data)
            .await
            .map_err(|e| CopyError::Bucket(e.to_string()))?;
        return Ok(n);
    }
    let mut stream = src
        .get_stream(src_key, None)
        .await
        .map_err(|e| CopyError::Bucket(e.to_string()))?;
    let mut upload = dst
        .multipart_create(dst_key)
        .await
        .map_err(|e| CopyError::Bucket(e.to_string()))?;
    let mut total = 0_u64;
    let mut part: Vec<u8> = Vec::with_capacity(PART_BYTES);
    let outcome: Result<(), CopyError> = async {
        while let Some(chunk) = stream.body.next().await {
            let chunk = chunk.map_err(|e| CopyError::Bucket(e.to_string()))?;
            total += chunk.len() as u64;
            if total > max_bytes {
                return Err(CopyError::TooLarge {
                    size: total,
                    max: max_bytes,
                });
            }
            part.extend_from_slice(&chunk);
            if part.len() >= PART_BYTES {
                let full = std::mem::replace(&mut part, Vec::with_capacity(PART_BYTES));
                upload
                    .put_part(Bytes::from(full))
                    .await
                    .map_err(|e| CopyError::Bucket(e.to_string()))?;
            }
        }
        if !part.is_empty() {
            upload
                .put_part(Bytes::from(std::mem::take(&mut part)))
                .await
                .map_err(|e| CopyError::Bucket(e.to_string()))?;
        }
        if total != info.size {
            return Err(CopyError::Bucket(format!(
                "the source gave {total} bytes, expected {}",
                info.size
            )));
        }
        upload
            .complete()
            .await
            .map_err(|e| CopyError::Bucket(e.to_string()))
    }
    .await;
    match outcome {
        Ok(()) => Ok(total),
        Err(e) => {
            let _ = upload.abort().await;
            Err(e)
        }
    }
}

/// A message to archive, as the caller read it.
#[derive(Debug, Clone)]
pub struct SourceMessage {
    /// The server.
    pub guild_id: i64,
    /// The channel.
    pub channel_id: i64,
    /// The message.
    pub message_id: i64,
    /// Its author.
    pub author_id: i64,
    /// The author's display name.
    pub author_name: String,
    /// The text.
    pub content: String,
    /// When it was sent, unix ms.
    pub sent_at_ms: i64,
    /// Its attachments in the instance bucket.
    pub attachments: Vec<SourceAttachment>,
}

/// One attachment of a [`SourceMessage`].
#[derive(Debug, Clone)]
pub struct SourceAttachment {
    /// Key in the instance bucket.
    pub key: String,
    /// File name.
    pub filename: String,
    /// Sniffed content type, or empty.
    pub content_type: String,
    /// Size in bytes.
    pub size: u64,
}

/// What [`Archiver::archive`] asks for.
#[derive(Debug, Clone)]
pub struct ArchiveRequest {
    /// The message.
    pub message: SourceMessage,
    /// Who sent it to the scrapbook; `None` for a reaction.
    pub by: Option<i64>,
    /// How.
    pub via: Via,
    /// Tags to start with.
    pub tags: Vec<String>,
    /// Largest object copied, bytes (`scrapbook.max_object_mib`).
    pub max_object_bytes: u64,
    /// Now, unix ms.
    pub now_ms: i64,
}

/// What an archive did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveOutcome {
    /// A new entry.
    Archived(EntryWithObjects),
    /// The message already had one; nothing changed.
    AlreadyArchived(EntryWithObjects),
}

/// Why a message was not archived.
#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    /// The server has not chosen a shelf.
    #[error("no scrapbook shelf is set for this server")]
    NoTarget,
    /// The service account may not write to the shelf.
    #[error("the scrapbook shelf refused the copy: {0}")]
    TargetRefused(Refusal),
    /// Nothing to keep: no text and no copyable attachment.
    #[error("the message has nothing to keep")]
    Empty,
    /// A copy failed; everything copied so far was rolled back.
    #[error("copying {filename} failed: {detail}")]
    Copy {
        /// The attachment.
        filename: String,
        /// Why.
        detail: String,
    },
    /// The store failed.
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// What a cascade did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CascadeReport {
    /// Entries deleted.
    pub entries: u64,
    /// Copies removed from their bucket.
    pub removed: u64,
    /// Copies that could not be removed and are now listed for the managers.
    pub failed: u64,
}

/// The archiver: store, buckets, the instance bucket the originals live in, and an id source.
pub struct Archiver {
    store: Arc<dyn ScrapbookStore>,
    buckets: Arc<dyn ScrapbookBuckets>,
    source: Arc<dyn ObjectStorage>,
    ids: Arc<dyn Fn() -> i64 + Send + Sync>,
}

impl Archiver {
    /// Build one. `source` is the bucket chat attachments live in.
    pub fn new(
        store: Arc<dyn ScrapbookStore>,
        buckets: Arc<dyn ScrapbookBuckets>,
        source: Arc<dyn ObjectStorage>,
        ids: Arc<dyn Fn() -> i64 + Send + Sync>,
    ) -> Self {
        Self {
            store,
            buckets,
            source,
            ids,
        }
    }

    /// The store, for the routes.
    pub fn store(&self) -> &Arc<dyn ScrapbookStore> {
        &self.store
    }

    /// The server's target and whether the service account could write to it right now (nothing is written).
    /// The refusal is `None` when the shelf is fine, or when there is no target.
    pub async fn target_health(
        &self,
        guild: i64,
    ) -> Result<(Option<Target>, Option<Refusal>), StoreError> {
        let Some(target) = self.store.target(guild).await? else {
            return Ok((None, None));
        };
        let refusal = self.buckets.writer(&target).await.err();
        Ok((Some(target), refusal))
    }

    /// Archive a message: copy its attachments into the server's shelf, then write the entry. Idempotent per
    /// message. If a copy fails, the copies already made are removed again (queued when the bucket refuses)
    /// and no entry is written.
    pub async fn archive(&self, req: ArchiveRequest) -> Result<ArchiveOutcome, ArchiveError> {
        let m = &req.message;
        if let Some(existing) = self.store.by_message(m.guild_id, m.message_id).await? {
            return Ok(ArchiveOutcome::AlreadyArchived(existing));
        }
        let target = self
            .store
            .target(m.guild_id)
            .await?
            .ok_or(ArchiveError::NoTarget)?;
        let writer = self
            .buckets
            .writer(&target)
            .await
            .map_err(ArchiveError::TargetRefused)?;

        let entry_id = (self.ids)();
        let mut objects: Vec<Object> = Vec::new();
        let mut skipped = 0_i32;
        for a in &m.attachments {
            if objects.len() >= MAX_OBJECTS_PER_ENTRY || a.size > req.max_object_bytes {
                skipped += 1;
                continue;
            }
            let position = objects.len();
            let key = object_key(m.sent_at_ms, m.message_id, position, &a.filename);
            // The key is built here from a sanitized name; check it like any key before a bucket sees it.
            if safe_key(&key, false).is_err() {
                skipped += 1;
                continue;
            }
            match copy_object(&*self.source, &a.key, &*writer, &key, req.max_object_bytes).await {
                Ok(size) => objects.push(Object {
                    id: (self.ids)(),
                    entry_id,
                    position: position as i32,
                    mount_id: target.mount_id,
                    key,
                    filename: a.filename.chars().take(255).collect(),
                    content_type: a.content_type.chars().take(255).collect(),
                    size: size as i64,
                    kind: ObjectKind::of(&a.content_type, &a.filename),
                }),
                // An original that is gone or grew past the cap is skipped, like one that was too large.
                Err(CopyError::SourceGone | CopyError::TooLarge { .. }) => skipped += 1,
                Err(CopyError::Bucket(detail)) => {
                    self.roll_back(m.guild_id, m.message_id, &target, &objects, &key)
                        .await;
                    return Err(ArchiveError::Copy {
                        filename: a.filename.clone(),
                        detail,
                    });
                }
            }
        }
        let content: String = m.content.chars().take(4000).collect();
        if objects.is_empty() && content.trim().is_empty() {
            return Err(ArchiveError::Empty);
        }
        let entry = Entry {
            id: entry_id,
            guild_id: m.guild_id,
            channel_id: m.channel_id,
            message_id: m.message_id,
            author_id: m.author_id,
            author_name: m.author_name.chars().take(100).collect(),
            content,
            sent_at_ms: m.sent_at_ms,
            archived_by: req.by,
            via: req.via,
            tags: clean_tags(&req.tags),
            mount_id: target.mount_id,
            skipped,
            created_at_ms: req.now_ms,
        };
        match self.store.insert(&entry, &objects).await {
            Ok(true) => Ok(ArchiveOutcome::Archived(EntryWithObjects {
                entry,
                objects,
            })),
            Ok(false) => {
                // Someone else archived the same message a moment ago. The keys are the same ones, so the
                // copies belong to their entry; leave them.
                let existing = self
                    .store
                    .by_message(m.guild_id, m.message_id)
                    .await?
                    .ok_or_else(|| StoreError("the entry vanished".into()))?;
                Ok(ArchiveOutcome::AlreadyArchived(existing))
            }
            Err(e) => {
                self.roll_back(m.guild_id, m.message_id, &target, &objects, "")
                    .await;
                Err(e.into())
            }
        }
    }

    /// Remove the copies of a failed archive. Whatever the bucket refuses to delete is queued as a removal
    /// (so it is retried and shown), never dropped.
    async fn roll_back(
        &self,
        guild: i64,
        message: i64,
        target: &Target,
        copied: &[Object],
        partial_key: &str,
    ) {
        let mut keys: Vec<String> = copied.iter().map(|o| o.key.clone()).collect();
        if !partial_key.is_empty() {
            keys.push(partial_key.to_owned());
        }
        let Ok(bucket) = self.buckets.remover(target.mount_id).await else {
            // Cannot even open it; the objects (if any) are listed by the next failed archive of this message,
            // and the module's tick has nothing more to go on. Say so in the log.
            tracing_warn(&format!(
                "scrapbook: could not open mount {} to roll back {} copies",
                target.mount_id,
                keys.len()
            ));
            return;
        };
        for key in keys {
            match bucket.delete(&key).await {
                Ok(()) | Err(StorageError::NotFound(_)) => {}
                Err(e) => {
                    // Queue it through the store so a manager can see it.
                    let _ = self
                        .store
                        .queue_orphan(guild, target.mount_id, &key, message, &e.to_string())
                        .await;
                }
            }
        }
    }

    /// The message (or its channel, or its author) is gone: delete the entries and take their copies out of
    /// the bucket. Failures stay listed for the managers.
    pub async fn cascade(
        &self,
        scope: DeleteScope,
        now_ms: i64,
    ) -> Result<CascadeReport, StoreError> {
        let deleted = self.store.delete(scope).await?;
        let mut report = CascadeReport {
            entries: deleted.entries,
            ..CascadeReport::default()
        };
        for id in deleted.queued {
            if self.try_removal(id, now_ms).await? {
                report.removed += 1;
            } else {
                report.failed += 1;
            }
        }
        Ok(report)
    }

    /// Try the removals that are due (the job `scrapbook.removals`). Returns `(removed, still failing)`.
    pub async fn drain(&self, now_ms: i64, limit: i64) -> Result<(u64, u64), StoreError> {
        let mut removed = 0;
        let mut failing = 0;
        for r in self.store.due_removals(now_ms, limit).await? {
            if self.try_removal(r.id, now_ms).await? {
                removed += 1;
            } else {
                failing += 1;
            }
        }
        Ok((removed, failing))
    }

    /// One try at one removal. `true` when the copy is gone (or was never there).
    pub async fn try_removal(&self, id: i64, now_ms: i64) -> Result<bool, StoreError> {
        let Some(r) = self.store.removal(id).await? else {
            return Ok(true);
        };
        let outcome = match self.buckets.remover(r.mount_id).await {
            Err(e) => Err(format!("the bucket could not be opened: {e}")),
            Ok(bucket) => match bucket.delete(&r.key).await {
                Ok(()) | Err(StorageError::NotFound(_)) => Ok(()),
                Err(e) => Err(e.to_string()),
            },
        };
        match outcome {
            Ok(()) => {
                self.store.removal_done(id).await?;
                Ok(true)
            }
            Err(e) => {
                let wait = backoff_ms(r.attempts);
                self.store.removal_failed(id, &e, now_ms + wait).await?;
                Ok(false)
            }
        }
    }
}

/// Wait before the next try after `attempts` failures: one minute, doubling, capped at six hours.
pub fn backoff_ms(attempts: i32) -> i64 {
    let shift = attempts.clamp(0, 20) as u32;
    (60_000_i64.saturating_mul(1_i64 << shift)).min(MAX_BACKOFF_MS)
}

fn tracing_warn(msg: &str) {
    // sr-storage has no logging dependency; stderr is what the server captures for the process.
    eprintln!("{msg}");
}

// ---------------------------------------------------------------------------------------------------------
// In-memory store, for tests of this crate and of the layers above.

use std::collections::BTreeMap;
use std::sync::Mutex;

/// An in-memory [`ScrapbookStore`].
#[derive(Default)]
pub struct MemoryScrapbook {
    inner: Mutex<MemState>,
}

#[derive(Default)]
struct MemState {
    targets: BTreeMap<i64, Target>,
    entries: BTreeMap<i64, EntryWithObjects>,
    removals: BTreeMap<i64, Removal>,
}

impl MemoryScrapbook {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }
    fn state(&self) -> std::sync::MutexGuard<'_, MemState> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }
    /// How many removals are queued (test helper).
    pub fn removal_count(&self) -> usize {
        self.state().removals.len()
    }
}

#[async_trait]
impl ScrapbookStore for MemoryScrapbook {
    async fn target(&self, guild: i64) -> StoreResult<Option<Target>> {
        Ok(self.state().targets.get(&guild).cloned())
    }
    async fn set_target(&self, t: &Target) -> StoreResult<()> {
        self.state().targets.insert(t.guild_id, t.clone());
        Ok(())
    }
    async fn clear_target(&self, guild: i64) -> StoreResult<bool> {
        Ok(self.state().targets.remove(&guild).is_some())
    }
    async fn insert(&self, e: &Entry, objects: &[Object]) -> StoreResult<bool> {
        let mut s = self.state();
        if s.entries
            .values()
            .any(|x| x.entry.guild_id == e.guild_id && x.entry.message_id == e.message_id)
        {
            return Ok(false);
        }
        s.entries.insert(
            e.id,
            EntryWithObjects {
                entry: e.clone(),
                objects: objects.to_vec(),
            },
        );
        Ok(true)
    }
    async fn by_message(&self, guild: i64, message: i64) -> StoreResult<Option<EntryWithObjects>> {
        Ok(self
            .state()
            .entries
            .values()
            .find(|x| x.entry.guild_id == guild && x.entry.message_id == message)
            .cloned())
    }
    async fn get(&self, guild: i64, id: i64) -> StoreResult<Option<EntryWithObjects>> {
        Ok(self
            .state()
            .entries
            .get(&id)
            .filter(|x| x.entry.guild_id == guild)
            .cloned())
    }
    async fn list(
        &self,
        guild: i64,
        f: &Filter,
        before: Option<i64>,
        limit: i64,
    ) -> StoreResult<Vec<EntryWithObjects>> {
        let s = self.state();
        let mut out: Vec<EntryWithObjects> = s
            .entries
            .values()
            .rev()
            .filter(|x| {
                let e = &x.entry;
                e.guild_id == guild
                    && f.viewable.contains(&e.channel_id)
                    && before.is_none_or(|b| e.id < b)
                    && f.channel_id.is_none_or(|c| c == e.channel_id)
                    && f.author_id.is_none_or(|a| a == e.author_id)
                    && f.tag.as_ref().is_none_or(|t| e.tags.contains(t))
                    && f.from_ms.is_none_or(|t| e.sent_at_ms >= t)
                    && f.to_ms.is_none_or(|t| e.sent_at_ms < t)
            })
            .cloned()
            .collect();
        out.truncate(limit.max(0) as usize);
        Ok(out)
    }
    async fn facets(&self, guild: i64, viewable: &[i64]) -> StoreResult<Facets> {
        let s = self.state();
        let mut tags: BTreeMap<String, i64> = BTreeMap::new();
        let mut authors: BTreeMap<i64, (String, i64)> = BTreeMap::new();
        let mut channels: BTreeMap<i64, i64> = BTreeMap::new();
        for x in s
            .entries
            .values()
            .filter(|x| x.entry.guild_id == guild && viewable.contains(&x.entry.channel_id))
        {
            for t in &x.entry.tags {
                *tags.entry(t.clone()).or_default() += 1;
            }
            authors
                .entry(x.entry.author_id)
                .or_insert((x.entry.author_name.clone(), 0))
                .1 += 1;
            *channels.entry(x.entry.channel_id).or_default() += 1;
        }
        Ok(Facets {
            tags: tags.into_iter().collect(),
            authors: authors.into_iter().map(|(i, (n, c))| (i, n, c)).collect(),
            channels: channels.into_iter().collect(),
        })
    }
    async fn set_tags(&self, guild: i64, id: i64, tags: &[String]) -> StoreResult<bool> {
        let mut s = self.state();
        match s.entries.get_mut(&id).filter(|x| x.entry.guild_id == guild) {
            Some(x) => {
                x.entry.tags = tags.to_vec();
                Ok(true)
            }
            None => Ok(false),
        }
    }
    async fn delete(&self, scope: DeleteScope) -> StoreResult<Deleted> {
        let mut s = self.state();
        let ids: Vec<i64> = s
            .entries
            .values()
            .filter(|x| match scope {
                DeleteScope::Entry { guild_id, id } => {
                    x.entry.guild_id == guild_id && x.entry.id == id
                }
                DeleteScope::Message(m) => x.entry.message_id == m,
                DeleteScope::Channel(c) => x.entry.channel_id == c,
                DeleteScope::Author(a) => x.entry.author_id == a,
            })
            .map(|x| x.entry.id)
            .collect();
        let mut out = Deleted::default();
        for id in ids {
            let Some(x) = s.entries.remove(&id) else {
                continue;
            };
            out.entries += 1;
            for o in x.objects {
                let dup = s
                    .removals
                    .values()
                    .any(|r| r.mount_id == o.mount_id && r.key == o.key);
                if dup {
                    continue;
                }
                s.removals.insert(
                    o.id,
                    Removal {
                        id: o.id,
                        guild_id: x.entry.guild_id,
                        mount_id: o.mount_id,
                        key: o.key,
                        message_id: Some(x.entry.message_id),
                        state: RemovalState::Pending,
                        attempts: 0,
                        last_error: String::new(),
                        created_at_ms: 0,
                        next_attempt_ms: 0,
                    },
                );
                out.queued.push(o.id);
            }
        }
        Ok(out)
    }
    async fn due_removals(&self, now_ms: i64, limit: i64) -> StoreResult<Vec<Removal>> {
        Ok(self
            .state()
            .removals
            .values()
            .filter(|r| r.next_attempt_ms <= now_ms)
            .take(limit.max(0) as usize)
            .cloned()
            .collect())
    }
    async fn removal(&self, id: i64) -> StoreResult<Option<Removal>> {
        Ok(self.state().removals.get(&id).cloned())
    }
    async fn removal_done(&self, id: i64) -> StoreResult<()> {
        self.state().removals.remove(&id);
        Ok(())
    }
    async fn removal_failed(&self, id: i64, error: &str, next_ms: i64) -> StoreResult<()> {
        if let Some(r) = self.state().removals.get_mut(&id) {
            r.state = RemovalState::Failed;
            r.attempts += 1;
            r.last_error = error.chars().take(500).collect();
            r.next_attempt_ms = next_ms;
        }
        Ok(())
    }
    async fn failed_removals(&self, guild: i64) -> StoreResult<Vec<Removal>> {
        Ok(self
            .state()
            .removals
            .values()
            .filter(|r| r.guild_id == guild && r.state == RemovalState::Failed)
            .cloned()
            .collect())
    }
    async fn retry_now(&self, guild: i64, id: i64, now_ms: i64) -> StoreResult<bool> {
        match self
            .state()
            .removals
            .get_mut(&id)
            .filter(|r| r.guild_id == guild)
        {
            Some(r) => {
                r.next_attempt_ms = now_ms;
                Ok(true)
            }
            None => Ok(false),
        }
    }
    async fn dismiss(&self, guild: i64, id: i64) -> StoreResult<bool> {
        let mut s = self.state();
        if s.removals.get(&id).is_some_and(|r| r.guild_id == guild) {
            s.removals.remove(&id);
            return Ok(true);
        }
        Ok(false)
    }
    async fn queue_orphan(
        &self,
        guild: i64,
        mount_id: i64,
        key: &str,
        message: i64,
        error: &str,
    ) -> StoreResult<()> {
        let mut s = self.state();
        if s.removals
            .values()
            .any(|r| r.mount_id == mount_id && r.key == key)
        {
            return Ok(());
        }
        let id = -(s.removals.len() as i64) - 1;
        s.removals.insert(
            id,
            Removal {
                id,
                guild_id: guild,
                mount_id,
                key: key.to_owned(),
                message_id: Some(message),
                state: RemovalState::Failed,
                attempts: 1,
                last_error: error.chars().take(500).collect(),
                created_at_ms: 0,
                next_attempt_ms: 0,
            },
        );
        Ok(())
    }
}

#[cfg(test)]
#[path = "scrapbook_tests.rs"]
mod tests;
