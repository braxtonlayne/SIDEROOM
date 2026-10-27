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
    APPLY_BATCH, DEFAULT_MAX_OBJECTS, IndexStore, JOB_KIND, NameParser, Options, RunReport,
    Sidecar, SidecarMap, StoreError, VideoFacts, VideoRole, classify_all, is_sidecar_name,
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
    let run = store.begin(mount_id, now_ms()).await?;
    match run_inner(storage, store, mount_id, run, opts, max_objects, budget).await {
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

async fn run_inner(
    storage: &dyn ObjectStorage,
    store: &dyn IndexStore,
    mount_id: i64,
    run: i64,
    opts: Options,
    max_objects: u64,
    budget: Duration,
) -> Result<(RunReport, bool), IndexError> {
    let mut listing: Vec<(String, u64)> = Vec::new();
    let complete = storage
        .walk(max_objects, budget, &mut |k, s| {
            listing.push((k.to_owned(), s));
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
        .collect();
    // Takedowns: a delisted key gets no catalog row.
    let mut gone: std::collections::HashSet<String> = std::collections::HashSet::new();
    for chunk in storable.chunks(APPLY_BATCH) {
        let keys: Vec<String> = chunk.iter().map(|c| c.key.clone()).collect();
        gone.extend(store.delisted(mount_id, &keys).await?);
    }
    if !gone.is_empty() {
        storable.retain(|c| !gone.contains(&c.key));
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

/// Register the handler on `runner`; pair with [`schedule`].
pub fn register(
    runner: JobRunner,
    pool: &PgPool,
    store: Arc<dyn IndexStore>,
    mounts: MountStorages,
    repo: Arc<dyn MountRepo>,
) -> JobRunner {
    let pool = pool.clone();
    runner.register(JOB_KIND, 1, move |job: Job| {
        let (pool, store, mounts, repo) =
            (pool.clone(), store.clone(), mounts.clone(), repo.clone());
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

    #[derive(Default)]
    struct Memory {
        seen: Mutex<Vec<Classified>>,
        finished: Mutex<Vec<(bool, Option<String>)>>,
        delist: Mutex<Vec<String>>,
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
