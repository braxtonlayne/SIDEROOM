//! The `storage.index` job (BYOS-CAT-001): lists a storage mount, classifies what it finds from folders, names and
//! sidecars, and writes the catalog (`sr_db::index_repo`).
//!
//! One kind, two shapes: a one-shot job with `{"mount_id": n}` (the Re-index button, an upload that completed)
//! indexes that mount; the daily singleton (id 1450, 06:10 UTC, no payload) indexes every live user and guild mount
//! that has passed its connection test. The instance bucket (mount 1) is never indexed: it holds the server's own
//! prefixes, not a shelf. A run lists at most `ops.storage_measure_max_objects` objects within the
//! same 60 s budget as `storage.measure`; a run that stops early hides nothing.

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use sr_db::{
    PgPool,
    jobs::{self, Job},
    runner::JobRunner,
};
use sr_storage::ObjectStorage;
use sr_storage::index::{
    APPLY_BATCH, DEFAULT_MAX_OBJECTS, HashedObject, IndexStore, JOB_KIND, NameParser, Options,
    RunReport, Sidecar, SidecarMap, SizedObject, StoreError, TaggedObject, VideoFacts, VideoRole,
    classify_all, content_tag, is_sidecar_name,
};
use sr_storage::mount_storage::MountStorages;
use sr_storage::mounts::{INSTANCE_MOUNT_ID, MountOwner, MountRepo, MountStatus};

/// The daily singleton. Unique among `*_JOB_ID` constants (the admin-jobs test greps for duplicates).
pub const STORAGE_INDEX_JOB_ID: i64 = 1450;
pub const STORAGE_INDEX_KIND: &str = JOB_KIND;
const HOUR_UTC: u32 = 6;
const MINUTE_UTC: u32 = 10;
/// Listing budget of one run.
pub const INDEX_WALK_BUDGET: Duration = Duration::from_secs(60);
/// A sidecar bigger than this is ignored.
const MAX_SIDECAR_BYTES: u64 = 64 * 1024;
/// Most sidecars read in one run.
const MAX_SIDECARS: usize = 2_000;

/// What one run may read to fingerprint size candidates of a byte takedown (D-583). Sizes rarely collide, so a run
/// reads few objects; what is over the cap waits for the next run (its object stays catalogued meanwhile).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HashLimits {
    /// Most objects read.
    pub max_reads: usize,
    /// Most bytes read.
    pub max_bytes: u64,
    /// Wall-clock budget of the reads.
    pub budget: Duration,
}

impl Default for HashLimits {
    fn default() -> Self {
        Self {
            max_reads: 20,
            max_bytes: 64 << 30,
            budget: Duration::from_secs(30 * 60),
        }
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(0))
        .unwrap_or(0)
}

/// `sr_media::naming` behind the classifier's [`NameParser`].
pub struct MediaNames;

impl NameParser for MediaNames {
    fn video(&self, path: &str) -> VideoFacts {
        use sr_media::naming::{Media, Role, parse_path};
        let p = parse_path(path);
        VideoFacts {
            role: match p.role {
                Role::Video => VideoRole::Video,
                Role::Subtitle => VideoRole::Subtitle,
                _ => VideoRole::Other,
            },
            episode: p.media == Media::Episode,
            title: p.title,
            year: p.year,
            season: p.season,
            episodes: p.episodes,
            episode_title: p.episode_title,
            absolute: p.absolute,
            special: p.special,
            variant: p.variant,
            language: p.language,
            tags: p.tags,
        }
    }
}

/// Read a sidecar's JSON. Unknown fields are ignored; a bad file is `None`.
pub fn parse_sidecar(text: &str) -> Option<Sidecar> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let o = v.as_object()?;
    let s = |k: &str| {
        o.get(k)
            .and_then(|x| x.as_str())
            .map(str::trim)
            .filter(|x| !x.is_empty() && x.chars().count() <= 300)
            .map(str::to_owned)
    };
    Some(Sidecar {
        title: s("title"),
        year: o
            .get("year")
            .and_then(|y| y.as_u64())
            .and_then(|y| u16::try_from(y).ok()),
        kind: s("kind").map(|k| k.to_ascii_lowercase()),
        tags: o
            .get("tags")
            .and_then(|t| t.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .take(20)
                    .collect()
            })
            .unwrap_or_default(),
        artist: s("artist"),
        album: s("album"),
        ignore: o.get("ignore").and_then(|b| b.as_bool()).unwrap_or(false),
    })
}

/// The failure of a run.
#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    /// The store failed.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// The bucket could not be listed.
    #[error("listing the mount failed: {0}")]
    List(String),
}

/// One run over one mount's storage.
pub async fn index_mount(
    storage: &dyn ObjectStorage,
    store: &dyn IndexStore,
    mount_id: i64,
    opts: Options,
    max_objects: u64,
    budget: Duration,
) -> Result<RunReport, IndexError> {
    index_mount_with(
        storage,
        store,
        mount_id,
        opts,
        max_objects,
        budget,
        HashLimits::default(),
    )
    .await
}

/// [`index_mount`] with the caps on the reads a byte takedown may ask for (D-583).
pub async fn index_mount_with(
    storage: &dyn ObjectStorage,
    store: &dyn IndexStore,
    mount_id: i64,
    opts: Options,
    max_objects: u64,
    budget: Duration,
    hash: HashLimits,
) -> Result<RunReport, IndexError> {
    let run = store.begin(mount_id, now_ms()).await?;
    match run_inner(
        storage,
        store,
        mount_id,
        run,
        opts,
        max_objects,
        budget,
        hash,
    )
    .await
    {
        Ok((report, complete)) => {
            let gone = store
                .finish(mount_id, run, &report, complete, None, now_ms())
                .await?;
            Ok(RunReport {
                vanished: gone,
                ..report
            })
        }
        Err(e) => {
            // Record the failure; hide nothing.
            let _ = store
                .finish(
                    mount_id,
                    run,
                    &RunReport::default(),
                    false,
                    Some(&e.to_string()),
                    now_ms(),
                )
                .await;
            Err(e)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_inner(
    storage: &dyn ObjectStorage,
    store: &dyn IndexStore,
    mount_id: i64,
    run: i64,
    opts: Options,
    max_objects: u64,
    budget: Duration,
    hash: HashLimits,
) -> Result<(RunReport, bool), IndexError> {
    let mut listing: Vec<(String, u64)> = Vec::new();
    // D-571: the fingerprint of each object's bytes, when the provider's entity tag is a content digest.
    let mut tags: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let complete = storage
        .walk_tagged(max_objects, budget, &mut |k, s, etag| {
            listing.push((k.to_owned(), s));
            if let Some(t) = etag.and_then(content_tag) {
                tags.insert(k.to_owned(), t);
            }
        })
        .await
        .map_err(|e| IndexError::List(e.to_string()))?;

    // Sidecars: small, and read at most MAX_SIDECARS of them.
    let mut sidecars = SidecarMap::new();
    for (key, size) in listing
        .iter()
        .filter(|(k, _)| is_sidecar_name(k.rsplit('/').next().unwrap_or(k)))
        .take(MAX_SIDECARS)
    {
        if *size > MAX_SIDECAR_BYTES {
            continue;
        }
        let Ok(bytes) = storage.get(key).await else {
            continue;
        };
        let Some(sc) = std::str::from_utf8(&bytes).ok().and_then(parse_sidecar) else {
            continue;
        };
        let folder = key
            .rsplit_once('/')
            .map_or(String::new(), |(d, _)| format!("{d}/"));
        sidecars.insert(folder, sc);
    }

    let classified = classify_all(&listing, &MediaNames, &sidecars, opts);
    let mut storable: Vec<_> = classified
        .into_iter()
        .filter(|c| !c.ignored && c.class.kind().is_some())
        .map(|mut c| {
            c.content_tag = tags.remove(&c.key);
            c
        })
        .collect();
    // Takedowns: a delisted key gets no catalog row, and neither do the same bytes under another name (D-571).
    let mut gone: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut shas: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let (mut reads, mut read_bytes, started) = (0_usize, 0_u64, std::time::Instant::now());
    for chunk in storable.chunks(APPLY_BATCH) {
        let keys: Vec<String> = chunk.iter().map(|c| c.key.clone()).collect();
        let delisted = store.delisted(mount_id, &keys).await?;
        let tagged: Vec<TaggedObject> = chunk
            .iter()
            .filter(|c| !delisted.contains(&c.key))
            .filter_map(|c| {
                c.content_tag.as_ref().map(|t| TaggedObject {
                    key: c.key.clone(),
                    tag: t.clone(),
                    size: c.size,
                })
            })
            .collect();
        gone.extend(delisted);
        if !tagged.is_empty() {
            gone.extend(store.byte_matches(mount_id, &tagged).await?);
        }
        // D-583: an object left over whose size equals a standing byte takedown's is read once and compared on its
        // sha256 (a multipart upload cut into other parts has another tag for the same bytes). Only size
        // candidates are read, an object already hashed at the same tag and size is not read again, and a run reads
        // at most `hash` worth; the rest wait for the next run. A failed read changes nothing.
        let rest: Vec<SizedObject> = chunk
            .iter()
            .filter(|c| !gone.contains(&c.key))
            .map(|c| SizedObject {
                key: c.key.clone(),
                tag: c.content_tag.clone(),
                size: c.size,
            })
            .collect();
        if !rest.is_empty() {
            let candidates = store.hash_candidates(mount_id, &rest).await?;
            let mut hashed: Vec<HashedObject> = Vec::new();
            for cand in candidates {
                let Some(obj) = rest.iter().find(|o| o.key == cand.key) else {
                    continue;
                };
                let (sha256, size) = match cand.known_sha256 {
                    Some(s) => (s, obj.size),
                    None => {
                        if reads >= hash.max_reads
                            || read_bytes >= hash.max_bytes
                            || started.elapsed() >= hash.budget
                        {
                            tracing::info!(mount = mount_id, key = %obj.key, "storage.index: hash read deferred to the next run");
                            continue;
                        }
                        match sr_storage::digest::sha256_object(storage, &obj.key).await {
                            Ok((s, n)) => {
                                reads += 1;
                                read_bytes += n;
                                (s, n)
                            }
                            Err(e) => {
                                tracing::warn!(mount = mount_id, key = %obj.key, error = %e, "storage.index: could not read a size candidate");
                                continue;
                            }
                        }
                    }
                };
                if size == obj.size {
                    shas.insert(obj.key.clone(), sha256.clone());
                }
                hashed.push(HashedObject {
                    key: obj.key.clone(),
                    tag: obj.tag.clone(),
                    size,
                    sha256,
                });
            }
            if !hashed.is_empty() {
                gone.extend(store.hash_matches(mount_id, &hashed).await?);
            }
        }
    }
    if !gone.is_empty() {
        storable.retain(|c| !gone.contains(&c.key));
    }
    for c in &mut storable {
        c.content_sha256 = shas.remove(&c.key);
    }
    let mut added = 0u64;
    for chunk in storable.chunks(APPLY_BATCH) {
        added += store.apply(mount_id, run, chunk).await?;
    }
    Ok((
        RunReport {
            objects: listing.len() as u64,
            added,
            vanished: 0,
            truncated: !complete,
        },
        complete,
    ))
}

/// A platform setting as a number, whatever JSON type it was saved as.
async fn setting_u64(pool: &PgPool, key: &str) -> Option<u64> {
    let v: serde_json::Value =
        sqlx::query_scalar("SELECT value FROM platform_settings WHERE key = $1")
            .bind(key)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()?;
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

async fn setting_bool(pool: &PgPool, key: &str, default: bool) -> bool {
    let v: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT value FROM platform_settings WHERE key = $1")
            .bind(key)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
    match v {
        Some(serde_json::Value::Bool(b)) => b,
        Some(serde_json::Value::Number(n)) => n.as_i64().is_some_and(|n| n != 0),
        Some(serde_json::Value::String(s)) => matches!(s.as_str(), "true" | "1"),
        _ => default,
    }
}

/// The longest `disable_item`'s read of an upheld object may take (D-583): staff wait for the uphold, so a very
/// large object that cannot be read in this time is recorded by tag and size only.
const DIGEST_READ_CAP: Duration = Duration::from_secs(180);

/// Reads a mount's object for the sha256 an upheld byte takedown records (D-583).
pub struct MountObjectDigests {
    pub mounts: MountStorages,
}

#[async_trait::async_trait]
impl sr_moderation::ObjectDigests for MountObjectDigests {
    async fn digest(&self, mount_id: i64, key: &str) -> Option<(String, u64)> {
        let storage = self.mounts.for_mount(mount_id).await.ok()?;
        tokio::time::timeout(
            DIGEST_READ_CAP,
            sr_storage::digest::sha256_object(storage.as_ref(), key),
        )
        .await
        .ok()?
        .ok()
    }
}

/// Register the handler on `runner`; pair with [`schedule`].
pub fn register(
    runner: JobRunner,
    pool: &PgPool,
    store: Arc<dyn IndexStore>,
    mounts: MountStorages,
    repo: Arc<dyn MountRepo>,
    moderation: sr_moderation::Moderation,
) -> JobRunner {
    let pool = pool.clone();
    runner.register(JOB_KIND, 1, move |job: Job| {
        let (pool, store, mounts, repo, moderation) = (
            pool.clone(),
            store.clone(),
            mounts.clone(),
            repo.clone(),
            moderation.clone(),
        );
        Box::pin(async move {
            let one = job.payload.get("mount_id").and_then(|v| v.as_i64());
            let opts = Options {
                rar_enabled: setting_bool(&pool, "library.ingest.rar_enabled", true).await,
            };
            let max = setting_u64(&pool, "ops.storage_measure_max_objects")
                .await
                .unwrap_or(DEFAULT_MAX_OBJECTS);
            let targets: Vec<i64> = match one {
                Some(id) => vec![id],
                None => repo
                    .list_enabled()
                    .await
                    .map(|ms| {
                        ms.into_iter()
                            .filter(|m| {
                                // Only mounts in good standing: a disabled, delisted or otherwise unusable mount
                                // is left alone (an allow-list, so a new status is skipped until named here).
                                matches!(m.status, MountStatus::Active | MountStatus::Failing)
                                    && m.id != INSTANCE_MOUNT_ID
                                    && m.owner != MountOwner::Platform
                                    && m.cors_ok_at.is_some()
                            })
                            .map(|m| m.id)
                            .collect()
                    })
                    .unwrap_or_default(),
            };
            for id in targets {
                if id == INSTANCE_MOUNT_ID {
                    tracing::warn!("storage.index: the instance bucket is not indexed");
                    continue;
                }
                let storage = match mounts.for_mount(id).await {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!(mount = id, error = %e, "storage.index: mount unavailable");
                        continue;
                    }
                };
                match index_mount(
                    storage.as_ref(),
                    store.as_ref(),
                    id,
                    opts,
                    max,
                    INDEX_WALK_BUDGET,
                )
                .await
                {
                    Ok(r) => tracing::info!(mount = id, report = ?r, "storage indexed"),
                    Err(e) => tracing::warn!(mount = id, error = %e, "storage index failed"),
                }
                // D-584: whoever owns an object the run delisted as a byte match is told, once.
                moderation.notify_byte_matches().await;
            }
            if one.is_none() {
                jobs::reschedule_running(
                    &pool,
                    job.id,
                    jobs::delay_until_daily_utc(now_ms(), HOUR_UTC, MINUTE_UTC),
                )
                .await?;
            }
            Ok(())
        })
    })
}

/// Arm the daily singleton.
pub async fn schedule(pool: &PgPool) -> sr_db::DbResult<()> {
    jobs::schedule_singleton(
        pool,
        STORAGE_INDEX_JOB_ID,
        STORAGE_INDEX_KIND,
        &json!({}),
        jobs::delay_until_daily_utc(now_ms(), HOUR_UTC, MINUTE_UTC),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use bytes::Bytes;
    use sr_storage::ObjectStoreStorage;
    use sr_storage::index::{Class, Classified};

    /// (tag, size, sha256) the catalog holds for a key.
    type KnownHash = (Option<String>, u64, String);

    #[derive(Default)]
    struct Memory {
        seen: Mutex<Vec<Classified>>,
        finished: Mutex<Vec<(bool, Option<String>)>>,
        delist: Mutex<Vec<String>>,
        /// (content tag, size) of the bytes a standing takedown delisted.
        taken_bytes: Mutex<Vec<(String, u64)>>,
        /// What `byte_matches` was asked.
        asked: Mutex<Vec<TaggedObject>>,
        /// (sha256, size) of the bytes a standing takedown recorded (D-583).
        taken_shas: Mutex<Vec<(String, u64)>>,
        /// What the catalog already holds: key -> (tag, size, sha256).
        known: Mutex<std::collections::HashMap<String, KnownHash>>,
    }

    #[async_trait]
    impl IndexStore for Memory {
        async fn begin(&self, _m: i64, _now: i64) -> Result<i64, StoreError> {
            Ok(1)
        }
        async fn delisted(&self, _m: i64, keys: &[String]) -> Result<Vec<String>, StoreError> {
            let d = self.delist.lock().unwrap();
            Ok(keys.iter().filter(|k| d.contains(k)).cloned().collect())
        }
        async fn byte_matches(
            &self,
            _m: i64,
            objects: &[TaggedObject],
        ) -> Result<Vec<String>, StoreError> {
            self.asked.lock().unwrap().extend_from_slice(objects);
            let taken = self.taken_bytes.lock().unwrap();
            Ok(objects
                .iter()
                .filter(|o| taken.iter().any(|(t, s)| *t == o.tag && *s == o.size))
                .map(|o| o.key.clone())
                .collect())
        }
        async fn hash_candidates(
            &self,
            _m: i64,
            objects: &[SizedObject],
        ) -> Result<Vec<sr_storage::index::HashCandidate>, StoreError> {
            let taken = self.taken_shas.lock().unwrap();
            let known = self.known.lock().unwrap();
            Ok(objects
                .iter()
                .filter(|o| taken.iter().any(|(_, s)| *s == o.size))
                .map(|o| sr_storage::index::HashCandidate {
                    key: o.key.clone(),
                    known_sha256: known
                        .get(&o.key)
                        .filter(|(t, s, _)| t.is_some() && *t == o.tag && *s == o.size)
                        .map(|(_, _, sha)| sha.clone()),
                })
                .collect())
        }
        async fn hash_matches(
            &self,
            _m: i64,
            objects: &[HashedObject],
        ) -> Result<Vec<String>, StoreError> {
            let taken = self.taken_shas.lock().unwrap();
            Ok(objects
                .iter()
                .filter(|o| taken.iter().any(|(h, s)| *h == o.sha256 && *s == o.size))
                .map(|o| o.key.clone())
                .collect())
        }
        async fn apply(&self, _m: i64, _r: i64, b: &[Classified]) -> Result<u64, StoreError> {
            self.seen.lock().unwrap().extend_from_slice(b);
            Ok(b.len() as u64)
        }
        async fn finish(
            &self,
            _m: i64,
            _r: i64,
            _rep: &RunReport,
            complete: bool,
            error: Option<&str>,
            _now: i64,
        ) -> Result<u64, StoreError> {
            self.finished
                .lock()
                .unwrap()
                .push((complete, error.map(str::to_owned)));
            Ok(0)
        }
    }

    /// 200 names across the sections, in the shapes real libraries use.
    fn tree() -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for i in 1..=60 {
            v.push(format!(
                "Movies/Film {i} ({}) [1080p]/Film.{i}.{}.1080p.BluRay.x264-GRP.mkv",
                1980 + i,
                1980 + i
            ));
        }
        for s in 1..=2 {
            for e in 1..=15 {
                v.push(format!(
                    "Series/Some Show (2010)/Season {s:02}/Some.Show.S{s:02}E{e:02}.720p.mkv"
                ));
            }
        }
        for e in 1..=20 {
            v.push(format!(
                "Anime/Great Anime/[Group] Great Anime - {e:02} [1080p].mkv"
            ));
        }
        for n in 1..=20 {
            v.push(format!("Manga/Some Manga/Some Manga v{n:02}.cbz"));
        }
        for n in 1..=15 {
            v.push(format!("Comics/Some Comic/Some Comic #{n:03}.cbz"));
        }
        for n in 1..=10 {
            v.push(format!("Books/Author - Book {n}.epub"));
        }
        for a in 1..=3 {
            for t in 1..=15 {
                v.push(format!(
                    "Music/Artist {a}/Album {a} (200{a})/{t:02} - Track {t}.flac"
                ));
            }
        }
        v.push("Movies/Film 1 (1981) [1080p]/poster.jpg".into());
        v.push("Series/Some Show (2010)/poster.jpg".into());
        v.push("Music/Artist 1/Album 1 (2001)/cover.jpg".into());
        v.push("Notes/todo.txt".into());
        v
    }

    #[tokio::test]
    async fn a_generated_tree_is_classified_through_the_real_naming_rules() {
        let names = tree();
        assert!(names.len() >= 200, "{}", names.len());
        let storage = ObjectStoreStorage::in_memory();
        for n in &names {
            storage.put(n, Bytes::from_static(b"x")).await.unwrap();
        }
        let store = Memory::default();
        let r = index_mount(
            &storage,
            &store,
            9,
            Options::default(),
            10_000,
            Duration::from_secs(30),
        )
        .await
        .unwrap();
        assert_eq!(r.objects, names.len() as u64);
        assert!(!r.truncated);
        let seen = store.seen.lock().unwrap();
        let count = |c: Class| seen.iter().filter(|x| x.class == c).count();
        assert_eq!(count(Class::Movie), 60, "films");
        assert_eq!(count(Class::Episode), 30 + 20, "series and anime episodes");
        assert_eq!(count(Class::Manga), 20);
        assert_eq!(count(Class::Comic), 15);
        assert_eq!(count(Class::Book), 10);
        assert_eq!(count(Class::Track), 45);
        // Misfile rate on the fixture tree: every episode lands under its show, every film has its year.
        let shows: std::collections::BTreeSet<_> = seen
            .iter()
            .filter(|c| c.class == Class::Episode && !c.anime)
            .map(|c| c.series.clone().unwrap())
            .collect();
        assert_eq!(shows.len(), 1, "{shows:?}");
        assert!(
            seen.iter()
                .filter(|c| c.class == Class::Movie)
                .all(|c| c.year.is_some())
        );
        let anime: Vec<_> = seen
            .iter()
            .filter(|c| c.anime && c.class == Class::Episode)
            .collect();
        assert_eq!(anime.len(), 20);
        assert!(
            anime.iter().all(|c| c.title == "Great Anime"),
            "{:?}",
            anime[0].title
        );
        // Covers bound by folder.
        let t1 = seen
            .iter()
            .find(|c| c.key.ends_with("01 - Track 1.flac") && c.key.contains("Artist 1"))
            .unwrap();
        assert_eq!(
            t1.cover_key.as_deref(),
            Some("Music/Artist 1/Album 1 (2001)/cover.jpg")
        );
        assert_eq!(store.finished.lock().unwrap().as_slice(), &[(true, None)]);
    }

    #[tokio::test]
    async fn delisted_keys_get_no_catalog_row() {
        let storage = ObjectStoreStorage::in_memory();
        for k in ["Movies/A.mp4", "Movies/B.mp4"] {
            storage.put(k, Bytes::from_static(b"x")).await.unwrap();
        }
        let store = Memory::default();
        store.delist.lock().unwrap().push("Movies/A.mp4".into());
        index_mount(
            &storage,
            &store,
            9,
            Options::default(),
            100,
            Duration::from_secs(30),
        )
        .await
        .unwrap();
        let seen = store.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].key, "Movies/B.mp4");
    }

    /// A bucket whose listing tags every object the way S3 does: the MD5-shaped digest of its bytes.
    struct S3Tags(
        ObjectStoreStorage,
        &'static str,
        std::sync::atomic::AtomicUsize,
    );

    fn s3_tag(bytes: &[u8]) -> String {
        // Not MD5, but a digest of the bytes in MD5's shape, which is all the indexer looks at.
        let mut h: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
        for b in bytes {
            h = (h ^ u128::from(*b)).wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b);
        }
        format!("\"{h:032x}\"")
    }

    #[async_trait]
    impl ObjectStorage for S3Tags {
        async fn put(&self, key: &str, data: Bytes) -> sr_storage::Result<()> {
            self.0.put(key, data).await
        }
        async fn get(&self, key: &str) -> sr_storage::Result<Bytes> {
            self.0.get(key).await
        }
        async fn get_range(&self, key: &str, s: u64, e: u64) -> sr_storage::Result<Bytes> {
            // The indexer's reads of a size candidate (D-583) come through here.
            self.2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if key.contains("broken") {
                return Err(sr_storage::StorageError::Backend("connection reset".into()));
            }
            self.0.get_range(key, s, e).await
        }
        async fn head(&self, key: &str) -> sr_storage::Result<sr_storage::ObjectInfo> {
            self.0.head(key).await
        }
        async fn delete(&self, key: &str) -> sr_storage::Result<()> {
            self.0.delete(key).await
        }
        async fn list(&self, prefix: &str) -> sr_storage::Result<Vec<sr_storage::ObjectInfo>> {
            let mut out = self.0.list(prefix).await?;
            for o in &mut out {
                // `self.1` is a multipart-style suffix (`-12`): the same bytes, tagged by another part size.
                let t = s3_tag(&self.0.get(&o.key).await?);
                o.etag = Some(format!("\"{}{}\"", t.trim_matches('"'), self.1));
            }
            Ok(out)
        }
        async fn presign(
            &self,
            m: sr_storage::PresignMethod,
            key: &str,
            ttl: Duration,
        ) -> sr_storage::Result<String> {
            self.0.presign(m, key, ttl).await
        }
        async fn multipart_create(
            &self,
            key: &str,
        ) -> sr_storage::Result<Box<dyn sr_storage::MultipartSession>> {
            self.0.multipart_create(key).await
        }
    }

    #[tokio::test]
    async fn taken_down_bytes_under_a_new_name_get_no_catalog_row() {
        // D-571: the leak was taken down as Movies/Leak.mp4; the same bytes come back as Movies/Renamed.mp4.
        let storage = S3Tags(ObjectStoreStorage::in_memory(), "", Default::default());
        let leak = Bytes::from_static(b"the very same film bytes");
        storage
            .put("Movies/Renamed.mp4", leak.clone())
            .await
            .unwrap();
        storage
            .put("Movies/Other.mp4", Bytes::from_static(b"another film"))
            .await
            .unwrap();
        // Same size as the leak, other bytes: the size alone never matches.
        storage
            .put(
                "Movies/Lookalike.mp4",
                Bytes::from_static(b"the very same film BYTES"),
            )
            .await
            .unwrap();
        let store = Memory::default();
        let tag = sr_storage::index::content_tag(&s3_tag(&leak)).unwrap();
        store
            .taken_bytes
            .lock()
            .unwrap()
            .push((tag.clone(), leak.len() as u64));
        index_mount(
            &storage,
            &store,
            9,
            Options::default(),
            100,
            Duration::from_secs(30),
        )
        .await
        .unwrap();
        let seen = store.seen.lock().unwrap();
        let mut keys: Vec<&str> = seen.iter().map(|c| c.key.as_str()).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["Movies/Lookalike.mp4", "Movies/Other.mp4"]);
        // The catalog rows carry their fingerprint, so a later takedown of them can record it.
        assert!(seen.iter().all(|c| c.content_tag.is_some()));
        let asked = store.asked.lock().unwrap();
        assert_eq!(asked.len(), 3);
        assert!(
            asked
                .iter()
                .any(|o| o.key == "Movies/Renamed.mp4" && o.tag == tag)
        );
    }

    #[tokio::test]
    async fn a_listing_without_content_digests_asks_for_no_byte_match() {
        // The in-memory store tags objects with a counter, which says nothing about the bytes.
        let storage = ObjectStoreStorage::in_memory();
        storage
            .put("Movies/A.mp4", Bytes::from_static(b"x"))
            .await
            .unwrap();
        let store = Memory::default();
        index_mount(
            &storage,
            &store,
            9,
            Options::default(),
            100,
            Duration::from_secs(30),
        )
        .await
        .unwrap();
        assert!(store.asked.lock().unwrap().is_empty());
        assert_eq!(store.seen.lock().unwrap()[0].content_tag, None);
    }

    #[tokio::test]
    async fn a_partial_listing_is_flagged_and_hides_nothing() {
        let storage = ObjectStoreStorage::in_memory();
        for i in 0..20 {
            storage
                .put(&format!("Movies/F{i}.mp4"), Bytes::from_static(b"x"))
                .await
                .unwrap();
        }
        let store = Memory::default();
        let r = index_mount(
            &storage,
            &store,
            9,
            Options::default(),
            5,
            Duration::from_secs(30),
        )
        .await
        .unwrap();
        assert!(r.truncated);
        assert_eq!(store.finished.lock().unwrap().as_slice(), &[(false, None)]);
    }

    const FILM: &[u8] = b"the very same film bytes";

    async fn sha_of(bytes: &'static [u8]) -> String {
        let s = ObjectStoreStorage::in_memory();
        s.put("x", Bytes::from_static(bytes)).await.unwrap();
        sr_storage::digest::sha256_object(&s, "x").await.unwrap().0
    }

    fn reads(s: &S3Tags) -> usize {
        s.2.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// A bucket of a re-upload made by another tool: the film again under another name, tagged by another part size
    /// (so the tag the takedown recorded never matches), a same-size look-alike, and a file of another size.
    async fn reupload() -> (S3Tags, Memory) {
        let storage = S3Tags(ObjectStoreStorage::in_memory(), "-12", Default::default());
        for (k, b) in [
            ("Movies/Renamed.mp4", FILM),
            // Same size as the film, other bytes.
            ("Movies/Lookalike.mp4", b"the very same film BYTES"),
            ("Movies/Other.mp4", b"another film"),
        ] {
            storage.put(k, Bytes::from_static(b)).await.unwrap();
        }
        let store = Memory::default();
        // The takedown recorded the single-part tag and the sha256 of the film (D-583).
        let single = content_tag(&s3_tag(FILM)).unwrap();
        store
            .taken_bytes
            .lock()
            .unwrap()
            .push((single, FILM.len() as u64));
        let film_sha = sha_of(FILM).await;
        store
            .taken_shas
            .lock()
            .unwrap()
            .push((film_sha, FILM.len() as u64));
        (storage, store)
    }

    #[tokio::test]
    async fn the_same_bytes_under_another_part_size_are_delisted_and_look_alikes_are_not() {
        // D-583: the tag of this upload differs from the taken-down object's, the bytes do not.
        let (storage, store) = reupload().await;
        index_mount(
            &storage,
            &store,
            9,
            Options::default(),
            100,
            Duration::from_secs(30),
        )
        .await
        .unwrap();
        let look_sha = sha_of(b"the very same film BYTES").await;
        let seen = store.seen.lock().unwrap();
        let mut keys: Vec<&str> = seen.iter().map(|c| c.key.as_str()).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["Movies/Lookalike.mp4", "Movies/Other.mp4"],
            "the re-upload under a multipart tag is delisted; same size alone and other sizes are kept"
        );
        // Only the size candidates were read: the film and its look-alike, never the file of another size.
        assert_eq!(reads(&storage), 2);
        // The look-alike's sha256 is kept on its catalog row, so it is never read for this again.
        let look = seen
            .iter()
            .find(|c| c.key.ends_with("Lookalike.mp4"))
            .unwrap();
        assert_eq!(look.content_sha256.as_deref(), Some(look_sha.as_str()));
        assert!(
            seen.iter()
                .find(|c| c.key.ends_with("Other.mp4"))
                .unwrap()
                .content_sha256
                .is_none()
        );
    }

    #[tokio::test]
    async fn an_object_already_hashed_at_the_same_tag_is_not_read_again() {
        let (storage, store) = reupload().await;
        index_mount(
            &storage,
            &store,
            9,
            Options::default(),
            100,
            Duration::from_secs(30),
        )
        .await
        .unwrap();
        assert_eq!(reads(&storage), 2);
        // The next run: the catalog holds the look-alike's sha256 at its tag and size, and the film's re-upload is
        // delisted by key. Nothing is read.
        let (tag, size, sha) = {
            let seen = store.seen.lock().unwrap();
            let c = seen
                .iter()
                .find(|c| c.key.ends_with("Lookalike.mp4"))
                .unwrap();
            (
                c.content_tag.clone(),
                c.size,
                c.content_sha256.clone().unwrap(),
            )
        };
        store
            .known
            .lock()
            .unwrap()
            .insert("Movies/Lookalike.mp4".into(), (tag.clone(), size, sha));
        store
            .delist
            .lock()
            .unwrap()
            .push("Movies/Renamed.mp4".into());
        index_mount(
            &storage,
            &store,
            9,
            Options::default(),
            100,
            Duration::from_secs(30),
        )
        .await
        .unwrap();
        assert_eq!(reads(&storage), 2, "no object was read twice");
        // The same file under another tag is another upload, and is read again.
        store.known.lock().unwrap().insert(
            "Movies/Lookalike.mp4".into(),
            (Some("0".repeat(32)), size, "0".repeat(64)),
        );
        index_mount(
            &storage,
            &store,
            9,
            Options::default(),
            100,
            Duration::from_secs(30),
        )
        .await
        .unwrap();
        assert_eq!(reads(&storage), 3);
    }

    #[tokio::test]
    async fn the_reads_of_one_run_are_capped_and_the_rest_wait() {
        let (storage, store) = reupload().await;
        let limits = HashLimits {
            max_reads: 1,
            ..HashLimits::default()
        };
        index_mount_with(
            &storage,
            &store,
            9,
            Options::default(),
            100,
            Duration::from_secs(30),
            limits,
        )
        .await
        .unwrap();
        assert_eq!(reads(&storage), 1, "one read, the other candidate waits");
        // Whatever was not read stays catalogued (it is served until the next run reads it).
        assert!(store.seen.lock().unwrap().len() >= 2);
    }

    #[tokio::test]
    async fn a_failed_read_changes_nothing() {
        // The film's re-upload cannot be read right now: it is neither delisted nor given a sha256, and the run
        // still completes.
        let storage = S3Tags(ObjectStoreStorage::in_memory(), "-12", Default::default());
        storage
            .put("Movies/broken.mp4", Bytes::from_static(FILM))
            .await
            .unwrap();
        let store = Memory::default();
        let film_sha = sha_of(FILM).await;
        store
            .taken_shas
            .lock()
            .unwrap()
            .push((film_sha, FILM.len() as u64));
        index_mount(
            &storage,
            &store,
            9,
            Options::default(),
            100,
            Duration::from_secs(30),
        )
        .await
        .unwrap();
        let seen = store.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].content_sha256, None);
        assert_eq!(store.finished.lock().unwrap().as_slice(), &[(true, None)]);
    }

    #[tokio::test]
    async fn a_listing_without_a_standing_byte_takedown_reads_nothing() {
        let storage = S3Tags(ObjectStoreStorage::in_memory(), "-12", Default::default());
        storage
            .put("Movies/A.mp4", Bytes::from_static(FILM))
            .await
            .unwrap();
        let store = Memory::default();
        index_mount(
            &storage,
            &store,
            9,
            Options::default(),
            100,
            Duration::from_secs(30),
        )
        .await
        .unwrap();
        assert_eq!(reads(&storage), 0);
    }

    #[tokio::test]
    async fn sidecars_are_read_and_a_bad_one_is_skipped() {
        let storage = ObjectStoreStorage::in_memory();
        storage
            .put(
                "Series/odd/.sideroom.json",
                Bytes::from_static(
                    br#"{"title":"Fixed","year":1999,"kind":"anime","tags":["Cult"]}"#,
                ),
            )
            .await
            .unwrap();
        storage
            .put("Series/odd/Odd S01E01.mkv", Bytes::from_static(b"x"))
            .await
            .unwrap();
        storage
            .put("Movies/bad/.sideroom.json", Bytes::from_static(b"{nope"))
            .await
            .unwrap();
        storage
            .put("Movies/bad/Film (2000).mp4", Bytes::from_static(b"x"))
            .await
            .unwrap();
        let store = Memory::default();
        index_mount(
            &storage,
            &store,
            9,
            Options::default(),
            100,
            Duration::from_secs(30),
        )
        .await
        .unwrap();
        let seen = store.seen.lock().unwrap();
        let ep = seen.iter().find(|c| c.class == Class::Episode).unwrap();
        assert_eq!(
            (ep.title.as_str(), ep.year, ep.anime),
            ("Fixed", Some(1999), true)
        );
        assert!(ep.tags.contains(&"cult".to_owned()));
        assert!(
            seen.iter().any(|c| c.class == Class::Movie),
            "a bad sidecar changes nothing"
        );
    }

    #[test]
    fn the_sidecar_reader_is_forgiving() {
        let s = parse_sidecar(r#"{"title":" X ","ignore":true,"extra":1,"tags":["a",1]}"#).unwrap();
        assert_eq!(s.title.as_deref(), Some("X"));
        assert!(s.ignore);
        assert_eq!(s.tags, vec!["a".to_owned()]);
        assert!(parse_sidecar("[1]").is_none());
        assert!(parse_sidecar("").is_none());
    }

    #[test]
    fn the_index_job_id_and_kind_are_used_once() {
        // The admin-jobs test greps every `*_JOB_ID` for duplicates; this pins our own pair.
        assert_eq!(STORAGE_INDEX_JOB_ID, 1450);
        assert_eq!(STORAGE_INDEX_KIND, "storage.index");
    }
}
