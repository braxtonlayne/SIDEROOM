//! Activity bundles (J-12, D-247, D-654, D-655): validation and unpacking of the zip an owner uploads. This is a
//! security boundary: whatever passes here is served from the Activity's own origin after review, so every name,
//! type and size is checked before a byte is stored, and the same path rules guard the proxy's lookups.
//!
//! A bundle is a zip of static files, checked in two passes over any `Read + Seek` (a stored object through
//! [`crate::zipstore::ZipStoreReader`], or bytes in memory):
//!
//! 1. [`plan`], from the zip's index alone, before a byte is inflated: at most the application's
//!    [`PackageLimits::max_files`] entries (Zip64 allowed); entry names use `/`, letters, digits and `. _ - ~ @ +`
//!    only, are relative, have no `.` or `..` segment, no empty segment, no segment starting with `.`, and no
//!    percent-encoding or control characters; regular files and directories only (symlinks, devices and encrypted
//!    entries are refused); every file has an extension on the allow-list ([`content_type_for`]) and no two names
//!    differ only in case; `index.html` sits at the root; the declared sizes add up to at most
//!    [`PackageLimits::max_bytes`] and to at most [`crate::package::MAX_RATIO`] times the zip's own length (the
//!    zip-bomb rule).
//! 2. [`inflate`], one file at a time into an [`EntrySink`], counting the real bytes (the headers are never trusted):
//!    a file that yields more than its declared size, or a total over the limit, is refused at once.
//!
//! Every refusal names the offending entry. The size and count limits are not constants: they come from the
//! application's tier ([`crate::package`]). [`SINGLE_REQUEST_MAX_BYTES`] is only the body of the single-request
//! upload route.

use std::collections::HashSet;
use std::io::{Cursor, Read, Seek};

use sha2::{Digest, Sha256};
use zip::read::{ArchiveOffset, Config as ZipConfig};

use crate::package::PackageLimits;

/// Entries (files and directories) in a bundle at the default `activities.package.max_files`.
pub const DEFAULT_MAX_FILES: usize = crate::package::DEFAULT_MAX_FILES;
/// The body of the single-request upload route (`POST .../activity/bundles`), and the limit of the in-memory
/// [`unpack`]. Not a package limit: a larger package goes through the resumable upload.
pub const SINGLE_REQUEST_MAX_BYTES: u64 = 32 * 1024 * 1024;
/// Longest entry path.
pub const MAX_PATH_LEN: usize = 256;
/// Longest path segment.
pub const MAX_SEGMENT_LEN: usize = 128;
/// The optional manifest at the root of a bundle.
pub const MANIFEST: &str = "sideroom.json";
/// The data channel capability: the Activity shares state and inputs through the SFU (ACT-CH-001).
pub const DATACHANNEL: &str = "datachannel";
/// STU-ZK-001 (D-389, D-399): the Activity reads objects of a shelf it names through the short-lived per-object URLs
/// of `GET /shelves/{shelf_id}/objects/{object_hash}/url`; the sandbox CSP allow-lists exactly those hosts.
pub const BYOS_READ: &str = "byos_read";
/// Reserved (D-399): persistent saves to the developer's own bucket. The validator accepts the flag; the runner
/// refuses it (never handed to an instance) until it ships.
pub const BYOS_WRITE: &str = "byos_write";
/// The storage capability (ARC-SBX-001): the host keeps a small key-value store for the Activity and sends it
/// in `ready`, because the sandboxed frame has no `localStorage`.
pub const STORAGE: &str = "storage";
/// Playable by guests on a `/play/<token>` link (ARC-GST-001).
pub const GUEST_PLAY: &str = "guest_play";
/// A lobby room for the game session, with its own keys (ARC-GST-001).
pub const LOBBY_CHANNEL: &str = "lobby_channel";
/// D-388 (wave 46, ACT-LND-002): when the host leaves, the instance keeps running and the longest-connected member
/// becomes the host (`HOST_CHANGED`). Without it the instance ends with its host, as before (P36-ACT-1).
pub const HOST_HANDOFF: &str = "host_handoff";
/// Voice between the players of one game session; reserved (ARC-GST-001, docs/dev/design/ingame-voice.md).
pub const INGAME_VOICE: &str = "ingame_voice";
/// Largest manifest.
pub const MAX_MANIFEST_BYTES: usize = 8 * 1024;

/// The extension allow-list with each extension's content type.
pub const ALLOWED: &[(&str, &str)] = &[
    ("html", "text/html; charset=utf-8"),
    ("js", "text/javascript; charset=utf-8"),
    ("mjs", "text/javascript; charset=utf-8"),
    ("css", "text/css; charset=utf-8"),
    ("json", "application/json"),
    ("map", "application/json"),
    ("wasm", "application/wasm"),
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("webp", "image/webp"),
    ("gif", "image/gif"),
    ("svg", "image/svg+xml"),
    ("ico", "image/x-icon"),
    ("woff2", "font/woff2"),
    ("mp3", "audio/mpeg"),
    ("ogg", "audio/ogg"),
    ("webm", "video/webm"),
    ("txt", "text/plain; charset=utf-8"),
];

/// Why a bundle was refused, naming the entry when one is at fault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleError {
    pub entry: Option<String>,
    pub message: String,
}

impl BundleError {
    fn general(message: impl Into<String>) -> Self {
        Self {
            entry: None,
            message: message.into(),
        }
    }

    pub(crate) fn at(entry: &str, message: impl Into<String>) -> Self {
        Self {
            entry: Some(entry.to_owned()),
            message: message.into(),
        }
    }

    /// The text for the owner: `"<entry>: <message>"` or just the message.
    pub fn text(&self) -> String {
        match &self.entry {
            Some(e) => format!("{}: {}", display_name(e), self.message),
            None => self.message.clone(),
        }
    }
}

/// An entry name made safe to show (control characters and over-long names are cut).
fn display_name(n: &str) -> String {
    n.chars()
        .take(80)
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

/// One unpacked file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleFile {
    pub path: String,
    pub content_type: &'static str,
    pub data: Vec<u8>,
}

/// A validated bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unpacked {
    /// SHA-256 (lowercase hex) of the zip as uploaded.
    pub sha256: String,
    pub files: Vec<BundleFile>,
    /// Entries in the zip, files and directories.
    pub entries: usize,
    /// Total unpacked bytes.
    pub total: u64,
    /// Capabilities the bundle declares in `sideroom.json` (see [`crate::arcade::FLAGS`], the one catalogue), sorted, no duplicates.
    pub capabilities: Vec<String>,
    /// The tier the manifest declares (ARC-ROOM-001); `None` for a manifest written before tiers.
    pub tier: Option<crate::arcade::Tier>,
    /// Creator tip links the manifest declares, already checked against the host allow-list.
    pub tip_links: Vec<sr_core::tips::TipLink>,
}

/// The content type of a file name by its extension (case-insensitive), `None` when the extension is not
/// on the allow-list.
pub fn content_type_for(path: &str) -> Option<&'static str> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let (_, ext) = name.rsplit_once('.')?;
    let ext = ext.to_ascii_lowercase();
    ALLOWED.iter().find(|(e, _)| *e == ext).map(|(_, ct)| *ct)
}

/// Why `path` (relative, no trailing slash) is not an acceptable bundle path, or `None`. The proxy applies
/// the same rule to request paths, so a lookup can never leave the bundle's prefix.
pub fn path_problem(path: &str) -> Option<&'static str> {
    if path.is_empty() {
        return Some("is empty");
    }
    if path.len() > MAX_PATH_LEN {
        return Some("is longer than 256 characters");
    }
    if path.starts_with('/') {
        return Some("is an absolute path");
    }
    if path.contains('\\') {
        return Some("uses a backslash");
    }
    if path.contains('%') {
        return Some("contains an encoded character");
    }
    if path.chars().any(char::is_control) {
        return Some("contains a control character");
    }
    for seg in path.split('/') {
        if seg.is_empty() {
            return Some("has an empty path segment");
        }
        if seg == ".." || seg == "." {
            return Some("leaves the bundle with . or ..");
        }
        if seg.starts_with('.') {
            return Some("has a hidden file or directory");
        }
        if seg.len() > MAX_SEGMENT_LEN {
            return Some("has a segment longer than 128 characters");
        }
        // A drive letter ("C:") or any other character outside the allow-list.
        if !seg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '~' | '@' | '+'))
        {
            return Some("uses characters other than letters, digits and . _ - ~ @ +");
        }
    }
    None
}

/// Map a request path (without query) of the Activity origin to the bundle path it asks for: `/` and any
/// path ending in `/` are the `index.html` there. `None` when the path is not an acceptable bundle path.
pub fn request_to_path(request_path: &str) -> Option<String> {
    let rest = request_path.strip_prefix('/')?;
    let rel = if rest.is_empty() {
        "index.html".to_owned()
    } else if let Some(dir) = rest.strip_suffix('/') {
        format!("{dir}/index.html")
    } else {
        rest.to_owned()
    };
    path_problem(&rel).is_none().then_some(rel)
}

/// The refusal text for a limit: what was exceeded, then who can change it.
fn limit_text(what: &str, limits: &PackageLimits) -> String {
    format!(
        "{what} {} MiB. The operator of this server can raise the limit.",
        limits.max_mib()
    )
}

/// One file the first pass accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanFile {
    /// Position in the zip's index.
    pub index: usize,
    /// The path inside the bundle.
    pub path: String,
    pub content_type: &'static str,
    /// The size the zip says the file has; the second pass makes it exact.
    pub size: u64,
}

/// What the first pass (the zip's index, no data) accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The files, in the order of the zip.
    pub files: Vec<PlanFile>,
    /// Entries in the zip, files and directories.
    pub entries: usize,
    /// The sizes the zip declares, added up.
    pub declared_total: u64,
    /// The length of the zip itself.
    pub zip_len: u64,
}

impl Plan {
    /// The order the second pass inflates in: `sideroom.json` first (a bad manifest refuses the package before a
    /// file is stored), then every file by path (the order of the code hash, [`crate::sandbox::CodeHasher`]).
    pub fn inflate_order(&self) -> Vec<&PlanFile> {
        let mut v: Vec<&PlanFile> = self.files.iter().collect();
        v.sort_by(|a, b| {
            (a.path != MANIFEST)
                .cmp(&(b.path != MANIFEST))
                .then_with(|| a.path.cmp(&b.path))
        });
        v
    }
}

/// Why the second pass stopped: the package is refused, or something outside the package failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckError {
    /// The package breaks a rule; the error names the entry.
    Refused(BundleError),
    /// The store (or anything else that is not the package's fault) failed.
    Failed(String),
}

impl From<BundleError> for CheckError {
    fn from(e: BundleError) -> Self {
        Self::Refused(e)
    }
}

/// Where the second pass puts what it inflates.
pub trait EntrySink {
    /// A file starts.
    fn begin(&mut self, file: &PlanFile) -> Result<(), CheckError>;
    /// The next bytes of the file that started.
    fn write(&mut self, file: &PlanFile, data: &[u8]) -> Result<(), CheckError>;
    /// The file is complete, and its real size equals the declared one.
    fn finish(&mut self, file: &PlanFile) -> Result<(), CheckError>;
}

/// The length checks that need no read: an empty zip, and one longer than the application's limit.
pub fn check_zip_len(zip_len: u64, limits: &PackageLimits) -> Result<(), BundleError> {
    if zip_len == 0 {
        return Err(BundleError::general("The upload is empty."));
    }
    if zip_len > limits.max_bytes {
        return Err(BundleError::general(limit_text(
            "The zip is larger than",
            limits,
        )));
    }
    Ok(())
}

/// Open a zip for checking: refuse an empty or over-long zip and one whose index is larger than `limits` allow
/// before the index is read, then read it (the archive must start at byte 0: no prefixed archives) and run the
/// first pass ([`plan`]). `reader` is the whole zip, `zip_len` its length.
pub fn open_and_plan<R: Read + Seek>(
    mut reader: R,
    zip_len: u64,
    limits: &PackageLimits,
) -> Result<(zip::ZipArchive<R>, Plan), CheckError> {
    check_zip_len(zip_len, limits)?;
    match crate::zipstore::peek_end_record(&mut reader, zip_len) {
        Ok(Some(e)) => {
            if e.entries > limits.max_files as u64 {
                return Err(too_many(limits).into());
            }
            // An entry's name and fields are short; an index much larger than that is hostile.
            if e.index_bytes > (limits.max_files as u64).saturating_mul(1024) + 1024 * 1024 {
                return Err(BundleError::general("The zip index is damaged.").into());
            }
        }
        Ok(None) => {}
        Err(e) => return Err(read_failure(&e)),
    }
    let cfg = ZipConfig {
        archive_offset: ArchiveOffset::Known(0),
    };
    let mut zip = zip::ZipArchive::with_config(cfg, reader).map_err(|e| match &e {
        zip::result::ZipError::Io(io) => read_failure(io),
        _ => CheckError::Refused(BundleError::general("That is not a zip file.")),
    })?;
    if zip.len() > limits.max_files {
        return Err(too_many(limits).into());
    }
    let plan = plan(&mut zip, zip_len, limits)?;
    Ok((zip, plan))
}

fn too_many(limits: &PackageLimits) -> BundleError {
    BundleError::general(format!(
        "The zip has more than {} entries. The operator of this server can raise the limit.",
        limits.max_files
    ))
}

/// A read of the zip failed. A store that failed is not the package's fault ([`CheckError::Failed`]); a spent read
/// budget is refused with advice; anything else is a zip that cannot be read.
fn read_failure(e: &std::io::Error) -> CheckError {
    match crate::zipstore::store_failure(e) {
        Some(crate::zipstore::StoreFailure::Store(m)) => CheckError::Failed(m.clone()),
        Some(crate::zipstore::StoreFailure::Budget) => CheckError::Refused(BundleError::general(
            "The files of this zip are stored in an order that makes it too slow to check. \
             Pack the zip again with the files in path order.",
        )),
        None => CheckError::Refused(BundleError::general("The zip could not be read.")),
    }
}

/// The first pass: names, types and the declared sizes, from the zip's index only, before any data is inflated.
/// Refuses what [`path_problem`], the extension allow-list, encryption, link entries, case-insensitive duplicates, a
/// file below a file, a missing `index.html` and the limits of `limits` forbid, and a package whose declared total is
/// more than [`crate::package::MAX_RATIO`] times the zip. Zip64 is accepted. Every refusal names the entry.
pub fn plan<R: Read + Seek>(
    zip: &mut zip::ZipArchive<R>,
    zip_len: u64,
    limits: &PackageLimits,
) -> Result<Plan, BundleError> {
    let mut files: Vec<PlanFile> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut declared: u64 = 0;
    let ratio_cap = zip_len.saturating_mul(crate::package::MAX_RATIO);
    for i in 0..zip.len() {
        let f = zip
            .by_index_raw(i)
            .map_err(|_| BundleError::general("The zip index is damaged."))?;
        let raw = match f.name() {
            Ok(n) => n.into_owned(),
            Err(_) => {
                return Err(BundleError::at(
                    &String::from_utf8_lossy(f.name_raw()),
                    "has a name that is not valid",
                ));
            }
        };
        let is_dir = f.is_dir();
        let name = raw.strip_suffix('/').unwrap_or(&raw).to_owned();
        if let Some(p) = path_problem(&name) {
            return Err(BundleError::at(&raw, p));
        }
        if f.encrypted() {
            return Err(BundleError::at(&raw, "is encrypted"));
        }
        if let Some(mode) = f.unix_mode() {
            match mode & 0o170_000 {
                0 | 0o100_000 | 0o040_000 => {}
                0o120_000 => return Err(BundleError::at(&raw, "is a symbolic link")),
                _ => return Err(BundleError::at(&raw, "is not a regular file")),
            }
        }
        if !seen.insert(name.to_ascii_lowercase()) {
            return Err(BundleError::at(&raw, "is listed twice"));
        }
        if is_dir {
            continue;
        }
        let Some(ct) = content_type_for(&name) else {
            return Err(BundleError::at(&raw, "has a file type that is not allowed"));
        };
        declared = declared.saturating_add(f.size());
        if declared > limits.max_bytes {
            return Err(BundleError::at(
                &raw,
                limit_text("makes the bundle larger than", limits),
            ));
        }
        if declared > ratio_cap {
            return Err(BundleError::at(
                &raw,
                format!(
                    "makes the bundle unpack to more than {} times the size of the zip, which is not an honest zip",
                    crate::package::MAX_RATIO
                ),
            ));
        }
        files.push(PlanFile {
            index: i,
            path: name,
            content_type: ct,
            size: f.size(),
        });
    }
    // A file may not also be a directory (`a` and `a/b.js`).
    let paths: HashSet<&str> = files.iter().map(|f| f.path.as_str()).collect();
    for f in &files {
        let mut cut = f.path.as_str();
        while let Some((parent, _)) = cut.rsplit_once('/') {
            if paths.contains(parent) {
                return Err(BundleError::at(
                    &f.path,
                    "sits below a path that is also a file",
                ));
            }
            cut = parent;
        }
    }
    if !paths.contains("index.html") {
        return Err(BundleError::general(
            "index.html is missing at the root of the bundle.",
        ));
    }
    Ok(Plan {
        files,
        entries: seen.len(),
        declared_total: declared,
        zip_len,
    })
}

/// The check's error as the bundle error the in-memory forms return.
pub fn into_bundle_error(e: CheckError) -> BundleError {
    match e {
        CheckError::Refused(e) => e,
        CheckError::Failed(m) => BundleError::general(m),
    }
}

/// Size of the copy buffer of the second pass.
const COPY_BUF: usize = 64 * 1024;

fn classify(name: &str, e: &std::io::Error) -> CheckError {
    match crate::zipstore::store_failure(e) {
        Some(crate::zipstore::StoreFailure::Store(m)) => CheckError::Failed(m.clone()),
        Some(crate::zipstore::StoreFailure::Budget) => read_failure(e),
        None => CheckError::Refused(BundleError::at(name, "could not be read")),
    }
}

/// The second pass: inflate the planned files one at a time, in [`Plan::inflate_order`], into `sink`, counting the
/// real bytes. A file that yields more than its declared size, or a total over the limit, is refused at once (the
/// zip's own numbers are never trusted); one that yields less is refused too. `progress` is told the bytes done
/// after every chunk. Returns the total bytes. CPU-bound and, over a store, blocking: call from a blocking thread.
pub fn inflate<R: Read + Seek>(
    zip: &mut zip::ZipArchive<R>,
    plan: &Plan,
    limits: &PackageLimits,
    sink: &mut dyn EntrySink,
    progress: &mut dyn FnMut(u64),
) -> Result<u64, CheckError> {
    let mut total: u64 = 0;
    let mut buf = vec![0u8; COPY_BUF];
    for file in plan.inflate_order() {
        let mut entry = zip.by_index(file.index).map_err(|e| match &e {
            zip::result::ZipError::Io(io) => classify(&file.path, io),
            _ => CheckError::Refused(BundleError::at(&file.path, "could not be read")),
        })?;
        sink.begin(file)?;
        let mut count: u64 = 0;
        loop {
            let n = entry.read(&mut buf).map_err(|e| classify(&file.path, &e))?;
            if n == 0 {
                break;
            }
            count += n as u64;
            if count > file.size {
                return Err(
                    BundleError::at(&file.path, "has more data than the zip says it has").into(),
                );
            }
            total += n as u64;
            if total > limits.max_bytes {
                return Err(BundleError::at(
                    &file.path,
                    limit_text("makes the bundle larger than", limits),
                )
                .into());
            }
            sink.write(file, &buf[..n])?;
            progress(total);
        }
        if count != file.size {
            return Err(
                BundleError::at(&file.path, "has less data than the zip says it has").into(),
            );
        }
        sink.finish(file)?;
    }
    Ok(total)
}

/// Collects the files in memory ([`unpack`]).
struct MemSink {
    files: Vec<BundleFile>,
}

impl EntrySink for MemSink {
    fn begin(&mut self, f: &PlanFile) -> Result<(), CheckError> {
        self.files.push(BundleFile {
            path: f.path.clone(),
            content_type: f.content_type,
            data: Vec::with_capacity(usize::try_from(f.size).unwrap_or(0).min(1 << 20)),
        });
        Ok(())
    }
    fn write(&mut self, _f: &PlanFile, data: &[u8]) -> Result<(), CheckError> {
        if let Some(last) = self.files.last_mut() {
            last.data.extend_from_slice(data);
        }
        Ok(())
    }
    fn finish(&mut self, _f: &PlanFile) -> Result<(), CheckError> {
        Ok(())
    }
}

/// Validate and unpack a zip held in memory, at the limits of a single-request upload
/// ([`SINGLE_REQUEST_MAX_BYTES`]). CPU-bound: call from a blocking thread. A thin wrapper over [`unpack_with`].
pub fn unpack(zip_bytes: &[u8]) -> Result<Unpacked, BundleError> {
    unpack_with(
        zip_bytes,
        &PackageLimits {
            max_bytes: SINGLE_REQUEST_MAX_BYTES,
            max_files: DEFAULT_MAX_FILES,
            tier: crate::package::PackageTier::Standard,
        },
    )
}

/// [`unpack`] at the given limits: the same two passes the stored-package check runs ([`open_and_plan`] and
/// [`inflate`]), into memory.
pub fn unpack_with(zip_bytes: &[u8], limits: &PackageLimits) -> Result<Unpacked, BundleError> {
    let zip_len = zip_bytes.len() as u64;
    let sha256 = data_encoding::HEXLOWER.encode(Sha256::digest(zip_bytes).as_slice());
    let (mut zip, plan) =
        open_and_plan(Cursor::new(zip_bytes), zip_len, limits).map_err(into_bundle_error)?;
    let mut sink = MemSink { files: Vec::new() };
    let total = match inflate(&mut zip, &plan, limits, &mut sink, &mut |_| {}) {
        Ok(t) => t,
        Err(e) => return Err(into_bundle_error(e)),
    };
    let manifest = match sink.files.iter().find(|f| f.path == MANIFEST) {
        Some(f) => crate::arcade::parse_manifest(&f.data)?,
        None => crate::arcade::Manifest::default(),
    };
    Ok(Unpacked {
        sha256,
        entries: plan.entries,
        files: sink.files,
        total,
        capabilities: manifest.capabilities,
        tier: manifest.tier,
        tip_links: manifest.tip_links,
    })
}

/// The capabilities a `sideroom.json` declares: `{"capabilities": ["datachannel"]}`. Unknown capabilities,
/// a manifest that is not that shape and one over [`MAX_MANIFEST_BYTES`] are refused, so a bundle cannot
/// claim something the platform does not know. The whole manifest (tier, tip links, flag rules) is
/// [`crate::arcade::parse_manifest`].
pub fn parse_manifest(data: &[u8]) -> Result<Vec<String>, BundleError> {
    crate::arcade::parse_manifest(data).map(|m| m.capabilities)
}

/// The `sideroom.json` at the root of a zip held in memory ([`read_manifest_from`] over a cursor).
pub fn read_manifest(zip_bytes: &[u8]) -> Result<crate::arcade::Manifest, BundleError> {
    if zip_bytes.is_empty() {
        return Err(BundleError::general("The file is empty."));
    }
    read_manifest_from(Cursor::new(zip_bytes))
}

/// The `sideroom.json` at the root of a zip, read with the archive rules of [`unpack`] (archive at byte 0, no
/// encrypted manifest, the manifest's real size counted against [`MAX_MANIFEST_BYTES`]) and validated by
/// [`crate::arcade::parse_manifest`]. Only the manifest is inflated, and there is no limit on the zip's length or
/// entry count: a guild cartridge (D-568, D-656) is a game or a mod, whose other files need not be a browser bundle,
/// and it lives on the server's own shelf. The caller bounds what the reader may fetch. A zip without a manifest gives
/// the empty [`crate::arcade::Manifest`] (no tier). CPU-bound: call from a blocking thread.
pub fn read_manifest_from<R: Read + Seek>(r: R) -> Result<crate::arcade::Manifest, BundleError> {
    let cfg = ZipConfig {
        archive_offset: ArchiveOffset::Known(0),
    };
    let mut zip = zip::ZipArchive::with_config(cfg, r).map_err(|e| match &e {
        zip::result::ZipError::Io(io) => into_bundle_error(read_failure(io)),
        _ => BundleError::general("That is not a zip file."),
    })?;
    let Some(i) = zip.index_for_name(MANIFEST) else {
        return Ok(crate::arcade::Manifest::default());
    };
    let mut f = zip
        .by_index(i)
        .map_err(|_| BundleError::at(MANIFEST, "could not be read"))?;
    if f.encrypted() {
        return Err(BundleError::at(MANIFEST, "is encrypted"));
    }
    let mut data = Vec::new();
    f.by_ref()
        .take(MAX_MANIFEST_BYTES as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|_| BundleError::at(MANIFEST, "could not be read"))?;
    crate::arcade::parse_manifest(&data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn build(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (n, d) in entries {
            if n.ends_with('/') {
                z.add_directory(*n, SimpleFileOptions::default()).unwrap();
            } else {
                z.start_file(*n, SimpleFileOptions::default()).unwrap();
                z.write_all(d).unwrap();
            }
        }
        z.finish().unwrap().into_inner()
    }

    #[test]
    fn read_manifest_reads_only_the_manifest_of_a_cartridge() {
        // A mod need not be a browser bundle: other file types and no index.html are fine.
        let m = read_manifest(&build(&[
            ("data/level.pak", b"\x00\x01"),
            (
                "sideroom.json",
                br#"{"tier": "TIER_2_REIMPLEMENTATION", "capabilities": ["storage"]}"#,
            ),
        ]))
        .unwrap();
        assert_eq!(m.tier, Some(crate::arcade::Tier::Reimplementation));
        assert_eq!(m.capabilities, vec!["storage"]);
        // No manifest: no tier.
        assert_eq!(
            read_manifest(&build(&[("index.html", b"x")])).unwrap().tier,
            None
        );
        // The manifest rules hold: an unknown tier, a flag the tier forbids, an oversized manifest.
        for bad in [
            br#"{"tier": "TIER_3_FAN"}"#.to_vec(),
            br#"{"tier": "TIER_2_REIMPLEMENTATION", "capabilities": ["guest_play"]}"#.to_vec(),
            [b"{\"x\": \"".as_slice(), &[b'a'; 9000], b"\"}"].concat(),
        ] {
            let e = read_manifest(&build(&[("sideroom.json", &bad)])).unwrap_err();
            assert_eq!(e.entry.as_deref(), Some(MANIFEST), "{e:?}");
        }
        assert!(read_manifest(b"not a zip").is_err());
        assert!(read_manifest(b"").is_err());
    }

    fn err(entries: &[(&str, &[u8])]) -> BundleError {
        unpack(&build(entries)).unwrap_err()
    }

    #[test]
    fn a_plain_bundle_unpacks_with_content_types() {
        let z = build(&[
            ("index.html", b"<h1>hi</h1>"),
            ("assets/", b""),
            ("assets/app.JS", b"1"),
            ("assets/style.css", b"a{}"),
            ("img/logo.PNG", b"x"),
            ("main.wasm", b"\0asm"),
        ]);
        let u = unpack(&z).unwrap();
        assert_eq!(u.files.len(), 5);
        assert_eq!(u.entries, 6);
        assert_eq!(u.sha256.len(), 64);
        let ct = |p: &str| u.files.iter().find(|f| f.path == p).unwrap().content_type;
        assert_eq!(ct("index.html"), "text/html; charset=utf-8");
        assert_eq!(ct("assets/app.JS"), "text/javascript; charset=utf-8");
        assert_eq!(ct("img/logo.PNG"), "image/png");
        assert_eq!(ct("main.wasm"), "application/wasm");
        assert_eq!(u.total, 11 + 1 + 3 + 1 + 4);
        // The hash is of the zip as uploaded.
        assert_eq!(unpack(&z).unwrap().sha256, u.sha256);
    }

    #[test]
    fn traversal_absolute_and_encoded_names_are_refused_by_name() {
        for bad in [
            "../evil.js",
            "a/../../evil.js",
            "/etc/passwd.txt",
            "a//b.js",
            "a/./b.js",
            "a\\b.js",
            "a%2fb.js",
            "a%2Eb.js",
            "C:/x.js",
            ".hidden.js",
            "dir/.env.txt",
            "sp ace.js",
            "caf\u{e9}.js",
            "nul\0.js",
        ] {
            let e = err(&[("index.html", b"x"), (bad, b"x")]);
            assert!(e.entry.is_some(), "{bad:?} -> {e:?}");
            assert!(e.text().contains(':'), "{bad:?}");
        }
        let long = format!("{}.js", "a".repeat(130));
        assert!(err(&[("index.html", b"x"), (&long, b"x")]).entry.is_some());
        let deep = format!("{}/x.js", "d/".repeat(130).trim_end_matches('/'));
        assert!(err(&[("index.html", b"x"), (&deep, b"x")]).entry.is_some());
    }

    #[test]
    fn extensions_are_allow_listed() {
        for bad in [
            "run.exe",
            "a.php",
            "x.sh",
            "noext",
            "a.html.exe",
            "x.htm",
            "x.xml",
            "x.zip",
        ] {
            let e = err(&[("index.html", b"x"), (bad, b"x")]);
            assert_eq!(e.entry.as_deref(), Some(bad), "{bad}");
            assert!(e.message.contains("not allowed"));
        }
        for ok in [
            "a.mjs", "a.map", "a.jpeg", "a.woff2", "a.mp3", "a.ogg", "a.webm", "a.txt", "a.ico",
            "a.svg", "a.webp", "a.gif", "a.jpg",
        ] {
            unpack(&build(&[("index.html", b"x"), (ok, b"x")])).unwrap();
        }
    }

    #[test]
    fn index_html_must_be_at_the_root() {
        let e = err(&[("sub/index.html", b"x")]);
        assert!(e.entry.is_none() && e.message.contains("index.html"));
        assert!(err(&[]).message.contains("index.html"));
    }

    #[test]
    fn duplicates_ignoring_case_and_file_directory_clashes_are_refused() {
        let e = err(&[("index.html", b"x"), ("A.js", b"1"), ("a.js", b"2")]);
        assert_eq!(e.entry.as_deref(), Some("a.js"));
        let e = err(&[
            ("index.html", b"x"),
            ("lib.js", b"1"),
            ("lib.js/x.js", b"2"),
        ]);
        assert_eq!(e.entry.as_deref(), Some("lib.js/x.js"));
    }

    #[test]
    fn symlinks_and_special_files_are_refused() {
        let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
        z.start_file("index.html", SimpleFileOptions::default())
            .unwrap();
        z.write_all(b"x").unwrap();
        z.add_symlink("link.txt", "/etc/passwd", SimpleFileOptions::default())
            .unwrap();
        let bytes = z.finish().unwrap().into_inner();
        let e = unpack(&bytes).unwrap_err();
        assert_eq!(e.entry.as_deref(), Some("link.txt"));
        assert!(e.message.contains("symbolic link"));
    }

    /// `n` bytes that compress about two to one (a 16-letter alphabet), so a few MiB of them are a few MiB of zip.
    fn noisy(n: usize, seed: u64) -> Vec<u8> {
        let mut x = seed | 1;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x & 0x0f) as u8 + b'a'
            })
            .collect()
    }

    fn limits(max_mib: u64, max_files: usize) -> PackageLimits {
        PackageLimits {
            max_bytes: max_mib * 1024 * 1024,
            max_files,
            tier: crate::package::PackageTier::Standard,
        }
    }

    #[test]
    fn size_and_entry_limits_hold() {
        // Nine files of 1 MiB are more than 8 MiB, however the zip is packed; the refusal names the entry and the
        // limit, and says who can raise it.
        let parts: Vec<Vec<u8>> = (0..9).map(|i| noisy(1024 * 1024, i + 1)).collect();
        let mut entries: Vec<(String, &[u8])> = vec![("index.html".into(), b"x")];
        for (i, p) in parts.iter().enumerate() {
            entries.push((format!("f{i}.txt"), p));
        }
        let refs: Vec<(&str, &[u8])> = entries.iter().map(|(n, d)| (n.as_str(), *d)).collect();
        let e = unpack_with(&build(&refs), &limits(8, 100)).unwrap_err();
        assert!(e.entry.is_some(), "{e:?}");
        assert!(
            e.message.contains("8 MiB")
                && e.message
                    .ends_with("The operator of this server can raise the limit."),
            "{e:?}"
        );

        // 51 tiny entries against a limit of 50.
        let names: Vec<String> = (0..50).map(|i| format!("f{i}.txt")).collect();
        let mut v: Vec<(&str, &[u8])> = vec![("index.html", b"x")];
        v.extend(names.iter().map(|n| (n.as_str(), &b"x"[..])));
        let e = unpack_with(&build(&v), &limits(8, 50)).unwrap_err();
        assert!(
            e.message.contains("50") && e.message.contains("raise"),
            "{e:?}"
        );
        // Exactly 50 is fine.
        v.pop();
        unpack_with(&build(&v), &limits(8, 50)).unwrap();
    }

    #[test]
    fn a_zip_that_unpacks_to_more_than_100_times_its_size_is_refused_by_name() {
        // 5 MiB of zeros packs to a few KiB: far more than 100 times.
        let zeros = vec![0u8; 5 * 1024 * 1024];
        let z = build(&[("index.html", b"x"), ("bomb.txt", &zeros)]);
        assert!(z.len() < 50 * 1024, "{}", z.len());
        let e = unpack_with(&z, &limits(64, 100)).unwrap_err();
        assert_eq!(e.entry.as_deref(), Some("bomb.txt"), "{e:?}");
        assert!(e.message.contains("100 times"), "{e:?}");
        // Plenty of room under the limit does not matter: the ratio rule is independent of the size cap.
        assert!(unpack_with(&z, &limits(1_000_000, 100)).is_err());
    }

    /// A one-file stored zip whose index claims `claimed` bytes while the data has more.
    fn lying_zip(data: &[u8], claimed: u32) -> Vec<u8> {
        let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
        z.start_file(
            "index.html",
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
        z.write_all(data).unwrap();
        let mut b = z.finish().unwrap().into_inner();
        // Patch the uncompressed size in the index (central directory header: signature, then 20 bytes to the size).
        let at = b
            .windows(4)
            .rposition(|w| w == [0x50, 0x4b, 0x01, 0x02])
            .unwrap();
        b[at + 24..at + 28].copy_from_slice(&claimed.to_le_bytes());
        b
    }

    #[test]
    fn an_entry_that_inflates_past_its_declared_size_is_refused() {
        let z = lying_zip(&vec![b'a'; 4096], 10);
        let e = unpack(&z).unwrap_err();
        assert_eq!(e.entry.as_deref(), Some("index.html"), "{e:?}");
        // Honest sizes pass.
        unpack(&lying_zip(&vec![b'a'; 4096], 4096)).unwrap();
        // An entry that yields less than it says is refused too.
        let e = unpack(&lying_zip(&vec![b'a'; 10], 4096)).unwrap_err();
        assert_eq!(e.entry.as_deref(), Some("index.html"), "{e:?}");
    }

    #[test]
    fn the_second_pass_inflates_the_manifest_first_and_the_rest_by_path() {
        let z = build(&[
            ("index.html", b"x"),
            ("z.js", b"1"),
            ("sideroom.json", b"{}"),
            ("a/b.js", b"2"),
        ]);
        let (_, plan) =
            open_and_plan(Cursor::new(&z[..]), z.len() as u64, &limits(8, 100)).unwrap();
        let order: Vec<&str> = plan
            .inflate_order()
            .iter()
            .map(|f| f.path.as_str())
            .collect();
        assert_eq!(order, ["sideroom.json", "a/b.js", "index.html", "z.js"]);
        assert_eq!(plan.declared_total, 5);
    }

    #[test]
    fn garbage_and_oversize_uploads_are_refused() {
        assert!(unpack(b"").is_err());
        assert!(unpack(b"not a zip").unwrap_err().message.contains("zip"));
        // A zip prefixed by other bytes is not accepted.
        let mut p = vec![0x4d; 40];
        p.extend(build(&[("index.html", b"x")]));
        assert!(unpack(&p).is_err());
        let huge = vec![0u8; (SINGLE_REQUEST_MAX_BYTES + 1) as usize];
        let e = unpack(&huge).unwrap_err();
        assert!(
            e.message.contains("32 MiB") && e.message.contains("raise"),
            "{e:?}"
        );
    }
    #[test]
    fn request_paths_map_inside_the_bundle_only() {
        assert_eq!(request_to_path("/").as_deref(), Some("index.html"));
        assert_eq!(request_to_path("/a.js").as_deref(), Some("a.js"));
        assert_eq!(request_to_path("/sub/").as_deref(), Some("sub/index.html"));
        assert_eq!(request_to_path("/x/y.css").as_deref(), Some("x/y.css"));
        for bad in [
            "",
            "x",
            "/..",
            "/a/../b.js",
            "/a/%2e%2e/b.js",
            "//a.js",
            "/a//b.js",
            "/a\\b.js",
            "/.git/config.txt",
            "/a%2Fb.js",
            "/a b.js",
        ] {
            assert_eq!(request_to_path(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn content_types_by_extension() {
        assert_eq!(content_type_for("a/b.SVG"), Some("image/svg+xml"));
        assert_eq!(content_type_for("a/b"), None);
        assert_eq!(content_type_for("a.d/b"), None);
        assert_eq!(content_type_for("x.exe"), None);
    }

    #[test]
    fn a_manifest_declares_known_capabilities() {
        let ok = build(&[
            ("index.html", b"<p>x"),
            (
                "sideroom.json",
                br#"{"capabilities":["datachannel","datachannel"]}"#,
            ),
        ]);
        assert_eq!(unpack(&ok).unwrap().capabilities, vec!["datachannel"]);
        let none = build(&[("index.html", b"<p>x")]);
        assert!(unpack(&none).unwrap().capabilities.is_empty());
        let byos = build(&[
            ("index.html", b"<p>x"),
            (
                "sideroom.json",
                br#"{"capabilities":["byos_write","byos_read"]}"#,
            ),
        ]);
        assert_eq!(
            unpack(&byos).unwrap().capabilities,
            vec!["byos_read", "byos_write"]
        );
        let empty = build(&[("index.html", b"<p>x"), ("sideroom.json", b"{}")]);
        assert!(unpack(&empty).unwrap().capabilities.is_empty());
    }

    #[test]
    fn a_bad_manifest_names_itself() {
        for body in [
            &br#"{"capabilities":["root"]}"#[..],
            br#"{"capabilities":"datachannel"}"#,
            br#"{"capabilities":[7]}"#,
            b"[]",
            b"nope",
        ] {
            let z = build(&[("index.html", b"<p>x"), ("sideroom.json", body)]);
            let e = unpack(&z).unwrap_err();
            assert_eq!(e.entry.as_deref(), Some("sideroom.json"), "{body:?}");
        }
        let big = vec![b' '; MAX_MANIFEST_BYTES + 1];
        let z = build(&[("index.html", b"<p>x"), ("sideroom.json", &big)]);
        assert!(unpack(&z).is_err());
    }
}
