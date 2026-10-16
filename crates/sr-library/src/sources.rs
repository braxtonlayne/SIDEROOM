//! Ingest sources (REV-UP-005, REV-UP-006, ADM-CNT-005): folders the server watches and storage
//! locations staff import from.
//!
//! * **Watched folder**: a directory under an operator-approved root (`[ingest] roots`). The
//!   scanner polls it, waits until a file has stopped changing (size and modification time equal
//!   for `stable_secs`), groups the files into series by their names, and creates **draft**
//!   submissions owned by the source's staff account, one per series, with a draft chapter and an
//!   upload per file. Nothing is published; the owner finishes the drafts like any submitter.
//! * **Storage import**: a prefix below `imports/` in the object store, or a directory too. It is
//!   never polled. Staff run a **dry run** that reports the detected structure (series, chapters,
//!   what would not be read) and then import on demand.
//!
//! Only Library files are read (CBZ, ZIP, CB7, 7Z, CBR, RAR, EPUB, PDF). Names are read with
//! `sr_reader::naming`; a file without a chapter number is reported and skipped.
//! Every path is checked against the roots; links are never followed.

use std::collections::{BTreeMap, HashMap};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sr_core::{Clock, PlatformPermissions as P};
use sr_review::model::CreateSubmission;
use sr_review::{Caller, ReviewService};
use sr_storage::ObjectStorage;
use tokio::io::AsyncReadExt;
use utoipa::ToSchema;

use crate::error::{LibraryError, LibraryResult};
use crate::model::{ChapterFormat, ChapterInput, UploadRequest};
use crate::service::{Actor, LibraryService};
use crate::submissions::LibraryDraftOps;

/// Extensions the scanner reads.
pub const SOURCE_EXTS: [&str; 8] = ["cbz", "zip", "cb7", "7z", "cbr", "rar", "epub", "pdf"];
/// Most files listed from one source.
pub const MAX_FILES: usize = 20_000;
/// Deepest folder level scanned.
pub const MAX_DEPTH: usize = 8;
/// Files imported per scan; the rest wait for the next one.
pub const MAX_IMPORTS_PER_SCAN: usize = 200;
/// Prefix every storage import must live under.
pub const STORAGE_PREFIX: &str = "imports/";
const PART: usize = 8 * 1024 * 1024;

fn str_enum<T: Copy>(all: &[(T, &'static str)], s: &str) -> Option<T> {
    all.iter().find(|(_, n)| *n == s).map(|(v, _)| *v)
}

/// What a source is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Polled while enabled.
    WatchedFolder,
    /// Dry run, then imported on demand.
    StorageImport,
}

impl SourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WatchedFolder => "watched_folder",
            Self::StorageImport => "storage_import",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        str_enum(
            &[
                (Self::WatchedFolder, "watched_folder"),
                (Self::StorageImport, "storage_import"),
            ],
            s,
        )
    }
}

/// Where the files are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    /// A directory on the server or a mounted volume, under an approved root.
    Local,
    /// A prefix below `imports/` in the object store.
    Storage,
}

impl Backend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Storage => "storage",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        str_enum(&[(Self::Local, "local"), (Self::Storage, "storage")], s)
    }
}

/// A configured source.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct IngestSource {
    #[serde(with = "sr_review::model::id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub label: String,
    pub kind: SourceKind,
    pub backend: Backend,
    /// The directory (local) or key prefix (storage).
    pub location: String,
    /// The staff account the drafts belong to.
    #[serde(with = "sr_review::model::id")]
    #[schema(value_type = String)]
    pub owner_id: i64,
    pub enabled: bool,
    pub poll_secs: i32,
    /// A file must be unchanged this long before it is imported.
    pub stable_secs: i32,
    pub last_scan_at_ms: Option<i64>,
    /// What the last scan found and did.
    #[schema(value_type = Option<Object>)]
    pub last_scan: Option<Value>,
    pub last_error: Option<String>,
    pub created_at_ms: i64,
}

/// Body of `POST /admin/ingest-sources`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct SourceInput {
    pub label: String,
    pub kind: SourceKind,
    pub backend: Backend,
    pub location: String,
    #[serde(with = "sr_review::model::id")]
    #[schema(value_type = String)]
    pub owner_id: i64,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub poll_secs: Option<i32>,
    pub stable_secs: Option<i32>,
}

fn yes() -> bool {
    true
}

/// Body of `PATCH /admin/ingest-sources/{id}`.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
pub struct SourcePatch {
    pub label: Option<String>,
    pub enabled: Option<bool>,
    pub poll_secs: Option<i32>,
    pub stable_secs: Option<i32>,
    #[serde(default, with = "sr_review::model::opt_id")]
    #[schema(value_type = Option<String>)]
    pub owner_id: Option<i64>,
}

/// What the scanner remembers about a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FileState {
    Seen,
    Imported,
    Skipped,
    Failed,
}

impl FileState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Seen => "seen",
            Self::Imported => "imported",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        str_enum(
            &[
                (Self::Seen, "seen"),
                (Self::Imported, "imported"),
                (Self::Skipped, "skipped"),
                (Self::Failed, "failed"),
            ],
            s,
        )
    }
}

/// One row of the ledger.
#[derive(Debug, Clone, PartialEq)]
pub struct LedgerFile {
    pub rel_path: String,
    pub size: u64,
    pub mtime_ms: i64,
    pub etag: Option<String>,
    /// Since when `size`, `mtime_ms` and `etag` have not changed.
    pub stable_since_ms: i64,
    pub state: FileState,
    pub submission_id: Option<i64>,
    pub error: Option<String>,
}

/// Storage port of the sources. `sr-db` implements it.
#[async_trait]
pub trait SourceStore: Send + Sync {
    async fn list(&self) -> LibraryResult<Vec<IngestSource>>;
    async fn get(&self, id: i64) -> LibraryResult<Option<IngestSource>>;
    async fn create(&self, s: &IngestSource) -> LibraryResult<()>;
    async fn update(&self, s: &IngestSource) -> LibraryResult<bool>;
    async fn delete(&self, id: i64) -> LibraryResult<bool>;
    /// Enabled watched folders whose last scan is older than their poll interval.
    async fn due(&self, now_ms: i64) -> LibraryResult<Vec<IngestSource>>;
    /// Whether `user` holds a live staff appointment (drafts are owned by staff).
    async fn is_staff(&self, user: i64) -> LibraryResult<bool>;
    async fn ledger(&self, source: i64) -> LibraryResult<Vec<LedgerFile>>;
    async fn put_files(&self, source: i64, files: &[LedgerFile]) -> LibraryResult<()>;
    async fn finish_scan(
        &self,
        source: i64,
        now_ms: i64,
        summary: &Value,
        error: Option<&str>,
    ) -> LibraryResult<()>;
}

/// A file found at a source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    /// Path below the source, `/` separated.
    pub rel_path: String,
    pub size: u64,
    pub mtime_ms: i64,
    pub etag: Option<String>,
}

/// Listing and reading files of a source.
#[async_trait]
pub trait SourceFiles: Send + Sync {
    /// Check a new source's location (roots, prefix) before it is stored.
    fn check_location(&self, backend: Backend, location: &str) -> Result<(), String>;
    async fn list(&self, src: &IngestSource) -> Result<Vec<SourceFile>, String>;
    /// Copy one file into the object store under `dest_key`.
    async fn copy_to(
        &self,
        src: &IngestSource,
        rel_path: &str,
        size: u64,
        dest_key: &str,
    ) -> Result<(), String>;
}

/// Local directories under approved roots, and prefixes of the object store.
pub struct FsAndStorageFiles {
    roots: Vec<PathBuf>,
    storage: Arc<dyn ObjectStorage>,
}

impl FsAndStorageFiles {
    pub fn new(roots: Vec<PathBuf>, storage: Arc<dyn ObjectStorage>) -> Self {
        Self { roots, storage }
    }

    /// The canonical directory of a local source, when it lies under an approved root.
    fn local_dir(&self, location: &str) -> Result<PathBuf, String> {
        if self.roots.is_empty() {
            return Err(
                "local folders are off: the operator has not approved any root ([ingest] roots)"
                    .into(),
            );
        }
        let dir =
            std::fs::canonicalize(location).map_err(|_| "that folder does not exist".to_owned())?;
        if !dir.is_dir() {
            return Err("that path is not a folder".into());
        }
        for r in &self.roots {
            if let Ok(root) = std::fs::canonicalize(r)
                && dir.starts_with(&root)
            {
                return Ok(dir);
            }
        }
        Err("that folder is outside the approved roots".into())
    }
}

/// A relative path from a listing that stays inside its base.
fn safe_rel(rel: &str) -> bool {
    !rel.is_empty()
        && Path::new(rel)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
        && !rel.contains('\\')
}

#[async_trait]
impl SourceFiles for FsAndStorageFiles {
    fn check_location(&self, backend: Backend, location: &str) -> Result<(), String> {
        match backend {
            Backend::Local => self.local_dir(location).map(|_| ()),
            Backend::Storage => {
                let ok = location.starts_with(STORAGE_PREFIX)
                    && location.len() <= 400
                    && location
                        .split('/')
                        .all(|p| p != ".." && p != "." && !p.chars().any(char::is_control));
                if ok {
                    Ok(())
                } else {
                    Err(format!("a storage prefix must start with {STORAGE_PREFIX}"))
                }
            }
        }
    }

    async fn list(&self, src: &IngestSource) -> Result<Vec<SourceFile>, String> {
        match src.backend {
            Backend::Local => {
                let dir = self.local_dir(&src.location)?;
                tokio::task::spawn_blocking(move || walk(&dir))
                    .await
                    .map_err(|e| e.to_string())?
            }
            Backend::Storage => {
                self.check_location(Backend::Storage, &src.location)?;
                let prefix = if src.location.ends_with('/') {
                    src.location.clone()
                } else {
                    format!("{}/", src.location)
                };
                let objects = self
                    .storage
                    .list(&prefix)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(objects
                    .into_iter()
                    .filter_map(|o| {
                        let rel = o.key.strip_prefix(&prefix)?.to_owned();
                        safe_rel(&rel).then_some(SourceFile {
                            rel_path: rel,
                            size: o.size,
                            mtime_ms: 0,
                            etag: o.etag,
                        })
                    })
                    .take(MAX_FILES)
                    .collect())
            }
        }
    }

    async fn copy_to(
        &self,
        src: &IngestSource,
        rel_path: &str,
        size: u64,
        dest_key: &str,
    ) -> Result<(), String> {
        if !safe_rel(rel_path) {
            return Err("unsafe path".into());
        }
        match src.backend {
            Backend::Local => {
                let dir = self.local_dir(&src.location)?;
                let path = std::fs::canonicalize(dir.join(rel_path))
                    .map_err(|_| "the file is gone".to_owned())?;
                if !path.starts_with(&dir) {
                    return Err("the file is outside its folder".into());
                }
                let mut file = tokio::fs::File::open(&path)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut session =
                    (size > PART as u64).then(|| self.storage.multipart_create(dest_key));
                let mut session = match session.take() {
                    Some(fut) => Some(fut.await.map_err(|e| e.to_string())?),
                    None => None,
                };
                let mut whole = Vec::new();
                loop {
                    let mut buf = vec![0u8; PART];
                    let mut filled = 0;
                    while filled < PART {
                        let n = file
                            .read(&mut buf[filled..])
                            .await
                            .map_err(|e| e.to_string())?;
                        if n == 0 {
                            break;
                        }
                        filled += n;
                    }
                    if filled == 0 {
                        break;
                    }
                    buf.truncate(filled);
                    match session.as_mut() {
                        Some(s) => s
                            .put_part(Bytes::from(buf))
                            .await
                            .map_err(|e| e.to_string())?,
                        None => whole.extend_from_slice(&buf),
                    }
                    if filled < PART {
                        break;
                    }
                }
                match session {
                    Some(mut s) => s.complete().await.map_err(|e| e.to_string()),
                    None => self
                        .storage
                        .put(dest_key, Bytes::from(whole))
                        .await
                        .map_err(|e| e.to_string()),
                }
            }
            Backend::Storage => {
                let prefix = if src.location.ends_with('/') {
                    src.location.clone()
                } else {
                    format!("{}/", src.location)
                };
                let from = format!("{prefix}{rel_path}");
                self.check_location(Backend::Storage, &from)?;
                let mut body = self
                    .storage
                    .get_stream(&from, None)
                    .await
                    .map_err(|e| e.to_string())?
                    .body;
                let mut session = if size > PART as u64 {
                    Some(
                        self.storage
                            .multipart_create(dest_key)
                            .await
                            .map_err(|e| e.to_string())?,
                    )
                } else {
                    None
                };
                let mut pending: Vec<u8> = Vec::new();
                while let Some(chunk) = body.next().await {
                    pending.extend_from_slice(&chunk.map_err(|e| e.to_string())?);
                    if let Some(s) = session.as_mut()
                        && pending.len() >= PART
                    {
                        let part = std::mem::take(&mut pending);
                        s.put_part(Bytes::from(part))
                            .await
                            .map_err(|e| e.to_string())?;
                    }
                }
                match session {
                    Some(mut s) => {
                        if !pending.is_empty() {
                            s.put_part(Bytes::from(pending))
                                .await
                                .map_err(|e| e.to_string())?;
                        }
                        s.complete().await.map_err(|e| e.to_string())
                    }
                    None => self
                        .storage
                        .put(dest_key, Bytes::from(pending))
                        .await
                        .map_err(|e| e.to_string()),
                }
            }
        }
    }
}

/// Walk `dir` (blocking): regular files only, links skipped, depth and count capped.
fn walk(dir: &Path) -> Result<Vec<SourceFile>, String> {
    let mut out = Vec::new();
    let mut stack = vec![(dir.to_path_buf(), 0_usize)];
    while let Some((d, depth)) = stack.pop() {
        let entries = match std::fs::read_dir(&d) {
            Ok(e) => e,
            Err(e) if depth > 0 => {
                tracing::warn!(error = %e, "an ingest folder could not be read");
                continue;
            }
            Err(e) => return Err(e.to_string()),
        };
        for entry in entries.flatten() {
            let Ok(ty) = entry.file_type() else { continue };
            if ty.is_symlink() {
                continue;
            }
            let path = entry.path();
            if ty.is_dir() {
                if depth < MAX_DEPTH {
                    stack.push((path, depth + 1));
                }
                continue;
            }
            if !ty.is_file() {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            let Ok(rel) = path.strip_prefix(dir) else {
                continue;
            };
            let rel = rel
                .components()
                .filter_map(|c| match c {
                    Component::Normal(s) => s.to_str().map(str::to_owned),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("/");
            if !safe_rel(&rel) {
                continue;
            }
            let mtime_ms = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
            out.push(SourceFile {
                rel_path: rel,
                size: meta.len(),
                mtime_ms,
                etag: None,
            });
            if out.len() >= MAX_FILES {
                return Ok(out);
            }
        }
    }
    out.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(out)
}

// ---- planning ---------------------------------------------------------------------

/// One file as the importer reads it.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct PlannedChapter {
    pub rel_path: String,
    pub size_bytes: u64,
    pub chapter: Option<f64>,
    pub volume: Option<f64>,
    pub title: Option<String>,
    pub group: Option<String>,
    /// `new`, `waiting` (not stable yet), `imported`, `skipped`, `failed`.
    pub status: String,
    pub note: Option<String>,
}

/// The files that belong to one series.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct PlannedSeries {
    pub title: String,
    /// The published series these chapters would be added to, when its title matches exactly.
    #[serde(with = "sr_review::model::opt_id")]
    #[schema(value_type = Option<String>)]
    pub existing_series: Option<i64>,
    pub chapters: Vec<PlannedChapter>,
}

/// What a scan found and did.
#[derive(Debug, Clone, PartialEq, Default, Serialize, ToSchema)]
pub struct ScanReport {
    pub dry_run: bool,
    pub files_seen: u32,
    pub series: Vec<PlannedSeries>,
    /// Files that could not be placed (no series or chapter number in the name).
    pub unplaced: Vec<PlannedChapter>,
    pub imported: u32,
    pub waiting: u32,
    pub failed: u32,
    /// Drafts created or extended, by submission id.
    pub submissions: Vec<String>,
    pub problems: Vec<String>,
}

fn ext_of(rel: &str) -> String {
    rel.rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

fn format_of(rel: &str) -> ChapterFormat {
    match ext_of(rel).as_str() {
        "epub" => ChapterFormat::Epub,
        "pdf" => ChapterFormat::Pdf,
        _ => ChapterFormat::Images,
    }
}

/// Group files into series by their names. The series title comes from the name; a file straight
/// in the source root with no series in its name is unplaced.
pub fn plan(files: &[SourceFile]) -> (Vec<PlannedSeries>, Vec<PlannedChapter>) {
    use sr_reader::naming::parse_path;
    let mut by_series: BTreeMap<String, PlannedSeries> = BTreeMap::new();
    let mut unplaced = Vec::new();
    for f in files {
        if !SOURCE_EXTS.contains(&ext_of(&f.rel_path).as_str()) {
            continue;
        }
        let p = parse_path(&f.rel_path);
        let mut c = PlannedChapter {
            rel_path: f.rel_path.clone(),
            size_bytes: f.size,
            chapter: p.chapter,
            volume: p.volume,
            title: p.title.clone(),
            group: p.group.clone(),
            status: "new".into(),
            note: None,
        };
        let series = p
            .series
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let Some(series) = series else {
            c.status = "skipped".into();
            c.note = Some(
                "no series title in the name; put the file in a folder named after the series"
                    .into(),
            );
            unplaced.push(c);
            continue;
        };
        // A single-volume or one-shot file has no chapter number: number it by its volume, or 1.
        if c.chapter.is_none() {
            match (p.volume, format_of(&f.rel_path)) {
                (Some(v), _) => {
                    c.chapter = Some(v);
                    c.note = Some("numbered by its volume".into());
                }
                (None, ChapterFormat::Epub | ChapterFormat::Pdf) => {
                    c.chapter = Some(1.0);
                    c.note = Some("no number in the name; numbered 1".into());
                }
                (None, ChapterFormat::Images) => {
                    c.status = "skipped".into();
                    c.note = Some("no chapter or volume number in the name".into());
                    unplaced.push(c);
                    continue;
                }
            }
        }
        let key = sr_review::similar::normalize_title(&series);
        by_series
            .entry(key)
            .or_insert_with(|| PlannedSeries {
                title: series,
                existing_series: None,
                chapters: Vec::new(),
            })
            .chapters
            .push(c);
    }
    let mut out: Vec<PlannedSeries> = by_series.into_values().collect();
    for s in &mut out {
        s.chapters.sort_by(|a, b| {
            a.chapter
                .partial_cmp(&b.chapter)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.rel_path.cmp(&b.rel_path))
        });
        // Two files with the same number would collide; the later one waits for a person.
        let mut seen: Vec<f64> = Vec::new();
        for c in &mut s.chapters {
            if let Some(n) = c.chapter {
                if seen.contains(&n) {
                    c.status = "skipped".into();
                    c.note = Some(format!("another file already has chapter {n}"));
                } else {
                    seen.push(n);
                }
            }
        }
    }
    (out, unplaced)
}

// ---- the scanner ------------------------------------------------------------------

/// Runs scans: lists a source, remembers files, imports the stable ones as drafts.
#[derive(Clone)]
pub struct SourceScanner {
    store: Arc<dyn SourceStore>,
    files: Arc<dyn SourceFiles>,
    library: LibraryService,
    review: ReviewService,
    clock: Arc<dyn Clock>,
    running: Arc<Mutex<Vec<i64>>>,
}

struct RunGuard {
    running: Arc<Mutex<Vec<i64>>>,
    id: i64,
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        if let Ok(mut r) = self.running.lock() {
            r.retain(|i| *i != self.id);
        }
    }
}

impl SourceScanner {
    pub fn new(
        store: Arc<dyn SourceStore>,
        files: Arc<dyn SourceFiles>,
        library: LibraryService,
        review: ReviewService,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            store,
            files,
            library,
            review,
            clock,
            running: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn store(&self) -> &Arc<dyn SourceStore> {
        &self.store
    }

    /// Validate and store a new source.
    pub async fn create(&self, id: i64, input: SourceInput) -> LibraryResult<IngestSource> {
        let label = input.label.trim().to_owned();
        if label.is_empty() || label.chars().count() > 100 {
            return Err(LibraryError::Invalid(
                "the label must be 1 to 100 characters".into(),
            ));
        }
        self.files
            .check_location(input.backend, input.location.trim())
            .map_err(LibraryError::Invalid)?;
        if input.kind == SourceKind::WatchedFolder && input.backend == Backend::Storage {
            // A prefix can be polled as well; nothing more to check.
        }
        if !self.store.is_staff(input.owner_id).await? {
            return Err(LibraryError::Invalid(
                "the owner must be a staff account".into(),
            ));
        }
        let poll = input.poll_secs.unwrap_or(300);
        let stable = input.stable_secs.unwrap_or(60);
        if !(30..=86_400).contains(&poll) || !(5..=3600).contains(&stable) {
            return Err(LibraryError::Invalid(
                "poll_secs must be 30 to 86400 and stable_secs 5 to 3600".into(),
            ));
        }
        let src = IngestSource {
            id,
            label,
            kind: input.kind,
            backend: input.backend,
            location: input.location.trim().to_owned(),
            owner_id: input.owner_id,
            enabled: input.enabled,
            poll_secs: poll,
            stable_secs: stable,
            last_scan_at_ms: None,
            last_scan: None,
            last_error: None,
            created_at_ms: self.clock.now_unix_ms(),
        };
        self.store.create(&src).await?;
        Ok(src)
    }

    pub async fn patch(&self, id: i64, p: SourcePatch) -> LibraryResult<IngestSource> {
        let mut s = self
            .store
            .get(id)
            .await?
            .ok_or(LibraryError::NotFound("ingest source"))?;
        if let Some(l) = p.label {
            let l = l.trim().to_owned();
            if l.is_empty() || l.chars().count() > 100 {
                return Err(LibraryError::Invalid(
                    "the label must be 1 to 100 characters".into(),
                ));
            }
            s.label = l;
        }
        if let Some(e) = p.enabled {
            s.enabled = e;
        }
        if let Some(v) = p.poll_secs {
            if !(30..=86_400).contains(&v) {
                return Err(LibraryError::Invalid(
                    "poll_secs must be 30 to 86400".into(),
                ));
            }
            s.poll_secs = v;
        }
        if let Some(v) = p.stable_secs {
            if !(5..=3600).contains(&v) {
                return Err(LibraryError::Invalid(
                    "stable_secs must be 5 to 3600".into(),
                ));
            }
            s.stable_secs = v;
        }
        if let Some(o) = p.owner_id {
            if !self.store.is_staff(o).await? {
                return Err(LibraryError::Invalid(
                    "the owner must be a staff account".into(),
                ));
            }
            s.owner_id = o;
        }
        if !self.store.update(&s).await? {
            return Err(LibraryError::NotFound("ingest source"));
        }
        Ok(s)
    }

    fn guard(&self, id: i64) -> LibraryResult<RunGuard> {
        let mut r = self
            .running
            .lock()
            .map_err(|_| LibraryError::Conflict("the scanner is busy".into()))?;
        if r.contains(&id) {
            return Err(LibraryError::Conflict(
                "a scan of this source is already running".into(),
            ));
        }
        r.push(id);
        Ok(RunGuard {
            running: self.running.clone(),
            id,
        })
    }

    /// Scan every enabled watched folder that is due. Errors of one source never stop the rest.
    pub async fn scan_due(&self) -> u32 {
        let now = self.clock.now_unix_ms();
        let due = match self.store.due(now).await {
            Ok(d) => d,
            Err(e) => {
                tracing::error!(error = %e, "listing the due ingest sources failed");
                return 0;
            }
        };
        let mut n = 0;
        for s in due {
            match self.scan(s.id, false).await {
                Ok(_) => n += 1,
                Err(LibraryError::Conflict(_)) => {}
                Err(e) => tracing::warn!(source = s.id, error = %e, "an ingest scan failed"),
            }
        }
        n
    }

    /// Scan one source. A dry run lists, plans and reports; it writes nothing.
    pub async fn scan(&self, id: i64, dry_run: bool) -> LibraryResult<ScanReport> {
        let _guard = self.guard(id)?;
        let src = self
            .store
            .get(id)
            .await?
            .ok_or(LibraryError::NotFound("ingest source"))?;
        let now = self.clock.now_unix_ms();
        let listed = match self.files.list(&src).await {
            Ok(l) => l,
            Err(e) => {
                if !dry_run {
                    let _ = self.store.finish_scan(id, now, &json!({}), Some(&e)).await;
                }
                return Err(LibraryError::Storage(e));
            }
        };
        let candidates: Vec<SourceFile> = listed
            .into_iter()
            .filter(|f| SOURCE_EXTS.contains(&ext_of(&f.rel_path).as_str()))
            .collect();
        let mut report = ScanReport {
            dry_run,
            files_seen: u32::try_from(candidates.len()).unwrap_or(u32::MAX),
            ..ScanReport::default()
        };

        // The ledger decides what is new, unchanged, changed or done.
        let ledger: HashMap<String, LedgerFile> = self
            .store
            .ledger(id)
            .await?
            .into_iter()
            .map(|l| (l.rel_path.clone(), l))
            .collect();
        let stable_ms = i64::from(src.stable_secs) * 1000;
        let mut updated: Vec<LedgerFile> = Vec::new();
        let mut ready: Vec<SourceFile> = Vec::new();
        let mut states: HashMap<String, (String, Option<String>)> = HashMap::new();
        for f in &candidates {
            let prev = ledger.get(&f.rel_path);
            let unchanged = prev
                .is_some_and(|p| p.size == f.size && p.mtime_ms == f.mtime_ms && p.etag == f.etag);
            let stable_since = match prev {
                Some(p) if unchanged => p.stable_since_ms,
                _ => now,
            };
            let state = match prev {
                Some(p) if unchanged && p.state != FileState::Seen => p.state,
                _ => FileState::Seen,
            };
            match state {
                FileState::Imported => {
                    states.insert(f.rel_path.clone(), ("imported".into(), None));
                }
                FileState::Skipped | FileState::Failed => {
                    states.insert(
                        f.rel_path.clone(),
                        (state.as_str().into(), prev.and_then(|p| p.error.clone())),
                    );
                }
                FileState::Seen => {
                    // A storage import is an explicit action: no waiting for the file to settle.
                    let settled =
                        src.kind == SourceKind::StorageImport || now - stable_since >= stable_ms;
                    if settled {
                        ready.push(f.clone());
                    } else {
                        states.insert(f.rel_path.clone(), ("waiting".into(), None));
                        report.waiting += 1;
                    }
                }
            }
            updated.push(LedgerFile {
                rel_path: f.rel_path.clone(),
                size: f.size,
                mtime_ms: f.mtime_ms,
                etag: f.etag.clone(),
                stable_since_ms: stable_since,
                state,
                submission_id: prev.and_then(|p| p.submission_id),
                error: prev.and_then(|p| p.error.clone()),
            });
        }

        // Plan every file for the report, with the status the ledger gave it.
        let (mut series, mut unplaced) = plan(&candidates);
        for s in &mut series {
            s.existing_series = self.library.find_series_exact(&s.title).await?;
            for c in &mut s.chapters {
                if c.status == "new"
                    && let Some((st, note)) = states.get(&c.rel_path)
                {
                    c.status.clone_from(st);
                    if note.is_some() {
                        c.note.clone_from(note);
                    }
                }
            }
        }
        report.unplaced.append(&mut unplaced);

        if dry_run {
            report.series = series;
            return Ok(report);
        }

        // Import the ready files, series by series, within the per-scan budget.
        let ready_paths: Vec<&str> = ready.iter().map(|f| f.rel_path.as_str()).collect();
        let mut budget = MAX_IMPORTS_PER_SCAN;
        for s in &mut series {
            let todo: Vec<usize> = s
                .chapters
                .iter()
                .enumerate()
                .filter(|(_, c)| c.status == "new" && ready_paths.contains(&c.rel_path.as_str()))
                .map(|(i, _)| i)
                .take(budget)
                .collect();
            if todo.is_empty() {
                continue;
            }
            budget -= todo.len();
            match self
                .import_series(&src, s, &todo, &mut updated, &mut report)
                .await
            {
                Ok(()) => {}
                Err(e) => {
                    report.problems.push(format!("{}: {e}", s.title));
                    for i in &todo {
                        if s.chapters[*i].status == "new" {
                            s.chapters[*i].status = "failed".into();
                            s.chapters[*i].note = Some(e.to_string());
                            report.failed += 1;
                            if let Some(l) = updated
                                .iter_mut()
                                .find(|l| l.rel_path == s.chapters[*i].rel_path)
                            {
                                l.state = FileState::Failed;
                                l.error = Some(e.to_string());
                            }
                        }
                    }
                }
            }
            if budget == 0 {
                break;
            }
        }
        // Unplaced files are remembered as skipped so they are not reported as new every time.
        for u in &report.unplaced {
            if let Some(l) = updated.iter_mut().find(|l| l.rel_path == u.rel_path)
                && l.state == FileState::Seen
            {
                l.state = FileState::Skipped;
                l.error.clone_from(&u.note);
            }
        }
        self.store.put_files(id, &updated).await?;
        report.series = series;
        let summary = json!({
            "files_seen": report.files_seen,
            "imported": report.imported,
            "waiting": report.waiting,
            "failed": report.failed,
            "unplaced": report.unplaced.len(),
            "submissions": report.submissions,
        });
        self.store.finish_scan(id, now, &summary, None).await?;
        Ok(report)
    }

    /// Create (or extend) the draft submission of one series and import its files into it.
    async fn import_series(
        &self,
        src: &IngestSource,
        s: &mut PlannedSeries,
        todo: &[usize],
        updated: &mut [LedgerFile],
        report: &mut ScanReport,
    ) -> Result<(), LibraryError> {
        let owner = src.owner_id;
        let caller = Caller {
            user_id: owner,
            permissions: P::STAFF_BASIC | P::PUBLISH_READING,
        };
        let (kind, payload) = match s.existing_series {
            Some(id) => ("library.items", json!({ "series_id": id.to_string() })),
            None => (
                "library.series",
                json!({ "kind": "manga", "title": s.title, "language": "en" }),
            ),
        };
        let created = self
            .review
            .create(
                caller,
                CreateSubmission {
                    kind: kind.into(),
                    payload,
                    title: None,
                    summary: format!("Imported from {}", src.label),
                },
            )
            .await
            .map_err(|e| LibraryError::Conflict(e.to_string()))?;
        let sub_id = created.submission.id;
        report.submissions.push(sub_id.to_string());
        let submission = self
            .review
            .editable_mine(owner, sub_id)
            .await
            .map_err(|e| LibraryError::Conflict(e.to_string()))?;
        let ops = LibraryDraftOps::new(self.library.clone());
        let actor = Actor {
            user_id: owner,
            permissions: P::PUBLISH_READING,
        };
        for &i in todo {
            let c = s.chapters[i].clone();
            let outcome = async {
                let chapter = ops
                    .create_chapter(
                        &submission,
                        owner,
                        None,
                        ChapterInput {
                            number: c.chapter.unwrap_or(1.0),
                            title: c.title.clone(),
                            volume_id: None,
                            format: format_of(&c.rel_path),
                            source_key: None,
                            release_group: c.group.clone(),
                        },
                    )
                    .await
                    .map_err(|e| LibraryError::Conflict(e.to_string()))?;
                self.review
                    .register_item(
                        owner,
                        sub_id,
                        sr_review::repo::NewItem {
                            ref_id: chapter.id,
                            label: format!("Chapter {}", chapter.number),
                            position: (chapter.number.clamp(0.0, 1.0e6) * 100.0).round() as i32,
                        },
                    )
                    .await
                    .map_err(|e| LibraryError::Conflict(e.to_string()))?;
                let name = c
                    .rel_path
                    .rsplit('/')
                    .next()
                    .unwrap_or(&c.rel_path)
                    .to_owned();
                let slot = self
                    .library
                    .begin_upload(
                        actor,
                        chapter.id,
                        UploadRequest {
                            filename: name,
                            size_bytes: c.size_bytes.max(1),
                            multipart: false,
                        },
                    )
                    .await?;
                let key = self.library.upload_object_key(slot.upload.id).await?;
                let copied = self
                    .files
                    .copy_to(src, &c.rel_path, c.size_bytes, &key)
                    .await;
                match copied {
                    Ok(()) => {
                        self.library.complete_upload(actor, slot.upload.id).await?;
                        Ok(())
                    }
                    Err(e) => {
                        let _ = self.library.abort_upload(actor, slot.upload.id).await;
                        let _ = ops.delete_chapter(&submission, owner, chapter.id).await;
                        let _ = self.review.unregister_item(owner, sub_id, chapter.id).await;
                        Err(LibraryError::Storage(e))
                    }
                }
            }
            .await;
            let l = updated.iter_mut().find(|l| l.rel_path == c.rel_path);
            match outcome {
                Ok(()) => {
                    s.chapters[i].status = "imported".into();
                    report.imported += 1;
                    if let Some(l) = l {
                        l.state = FileState::Imported;
                        l.submission_id = Some(sub_id);
                        l.error = None;
                    }
                }
                Err(e) => {
                    s.chapters[i].status = "failed".into();
                    s.chapters[i].note = Some(e.to_string());
                    report.failed += 1;
                    if let Some(l) = l {
                        l.state = FileState::Failed;
                        l.error = Some(e.to_string());
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(rel: &str) -> SourceFile {
        SourceFile {
            rel_path: rel.into(),
            size: 10,
            mtime_ms: 0,
            etag: None,
        }
    }

    #[test]
    fn files_are_grouped_by_series_and_ordered_by_chapter() {
        let (series, unplaced) = plan(&[
            f("Blue Hour/Blue Hour c002.cbz"),
            f("Blue Hour/Blue Hour c001.cbz"),
            f("Blue Hour/notes.txt"),
            f("Other Series/Other Series v03.epub"),
            f("stray.cbz"),
        ]);
        let titles: Vec<&str> = series.iter().map(|s| s.title.as_str()).collect();
        assert!(titles.contains(&"Blue Hour"), "{titles:?}");
        let blue = series.iter().find(|s| s.title == "Blue Hour").unwrap();
        assert_eq!(
            blue.chapters.iter().map(|c| c.chapter).collect::<Vec<_>>(),
            vec![Some(1.0), Some(2.0)]
        );
        let other = series.iter().find(|s| s.title == "Other Series").unwrap();
        assert_eq!(
            other.chapters[0].chapter,
            Some(3.0),
            "numbered by its volume"
        );
        assert_eq!(unplaced.len(), 1);
        assert_eq!(unplaced[0].rel_path, "stray.cbz");
    }

    #[test]
    fn a_duplicate_chapter_number_waits_for_a_person() {
        let (series, _) = plan(&[
            f("Blue Hour/Blue Hour c001 [GroupA].cbz"),
            f("Blue Hour/Blue Hour c001 [GroupB].cbz"),
        ]);
        let ch = &series[0].chapters;
        assert_eq!(ch.iter().filter(|c| c.status == "new").count(), 1);
        assert_eq!(ch.iter().filter(|c| c.status == "skipped").count(), 1);
    }

    #[test]
    fn a_path_must_stay_inside_its_base() {
        assert!(safe_rel("a/b/c.cbz"));
        assert!(!safe_rel("../x.cbz"));
        assert!(!safe_rel("/x.cbz"));
        assert!(!safe_rel("a\\b.cbz"));
        assert!(!safe_rel(""));
    }
}
