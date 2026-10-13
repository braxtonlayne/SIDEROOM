//! A hardened archive reader for ZIP/CBZ, 7z/CB7 and RAR/CBR (LIB-ING-002).
//!
//! Archives are untrusted. [`ArchiveReader`] is an enum over the three formats and every
//! variant is held to the same [`ArchiveLimits`]:
//!
//! * the file size, the entry count and every entry's declared size are checked from the
//!   headers before anything is inflated. ZIP also checks each entry's compression ratio;
//!   7z checks each solid block's ratio and its unpacked size, and RAR (whose headers do
//!   not carry packed sizes) checks the ratio of the whole archive;
//! * one entry is inflated at a time through a hard byte cap, and a running budget is kept
//!   across the whole archive. 7z and RAR decode solid blocks sequentially, so for them the
//!   budget counts every entry of the archive, junk included;
//! * names with `..`, absolute paths, drive letters or NUL are rejected (not repaired),
//!   encrypted entries and encrypted headers are rejected, and symlinks are skipped. Names
//!   are only ever used as labels and sort keys, never as file system paths;
//! * nested archives are never opened: they are just unsupported files and are skipped;
//! * pages are listed in natural order and junk is ignored: `__MACOSX/`, `.DS_Store`,
//!   `Thumbs.db`, `desktop.ini`, AppleDouble `._*` files, dot files and everything that
//!   is not a supported image, besides `ComicInfo.xml` and `series.json`;
//! * AVIF pages are refused with [`IngestError::UnsupportedFormat`] listing their names.
//!
//! RAR uses the UnRAR C library (`unrar` crate, ADR-0005). It can only open a file path, so
//! a RAR upload is spooled to a private temporary file that is deleted when the reader is
//! dropped. RAR is behind [`ArchiveLimits::rar_enabled`], which the ingest job sets from
//! the operator setting `library.ingest.rar_enabled`.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use sevenz_rust2::{Archive as SevenArchive, BlockDecoder, Password};
use unrar::{
    Archive as RarArchive, CursorBeforeHeader, OpenArchive, Process, error::Code as RarCode,
};
use zip::ZipArchive;

use super::{IngestError, natural_cmp};

/// Budgets for one archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArchiveLimits {
    /// Largest archive file.
    pub max_archive_bytes: u64,
    /// Most entries in the archive.
    pub max_entries: usize,
    /// Largest uncompressed entry.
    pub max_entry_bytes: u64,
    /// Largest sum of all inflated entries.
    pub max_total_bytes: u64,
    /// Entries at least this big are checked against `max_ratio`.
    pub ratio_floor: u64,
    /// Largest uncompressed : compressed ratio. Page images compress poorly (JPEG ~1,
    /// flat scans well under 100), so 150 only trips on bombs.
    pub max_ratio: u64,
    /// Largest unpacked size of one 7z solid block (a block must be decoded from its
    /// start, so this bounds the CPU spent on one page).
    pub max_solid_block_bytes: u64,
    /// Whether RAR/CBR archives are read at all (operator setting
    /// `library.ingest.rar_enabled`, default on).
    pub rar_enabled: bool,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self {
            max_archive_bytes: 2 * 1024 * 1024 * 1024,
            max_entries: 4000,
            max_entry_bytes: 64 * 1024 * 1024,
            max_total_bytes: 2 * 1024 * 1024 * 1024,
            ratio_floor: 1024 * 1024,
            max_ratio: 150,
            max_solid_block_bytes: 1024 * 1024 * 1024,
            rar_enabled: true,
        }
    }
}

/// Metadata files inside an archive are small; anything bigger is not metadata.
pub const MAX_METADATA_BYTES: u64 = 1024 * 1024;

/// Largest LZMA/LZMA2 dictionary or PPMd model a 7z coder may ask for. The decoder
/// allocates it up front, so a header can otherwise demand gigabytes.
pub const MAX_7Z_CODER_MEMORY: u64 = 512 * 1024 * 1024;

/// Solid 7z blocks up to this unpacked size are decoded once and their pages cached, so
/// reading pages in any order stays linear. Larger blocks are re-decoded per page.
const SOLID_CACHE_BYTES: u64 = 128 * 1024 * 1024;

const IMAGE_EXTS: [&str; 5] = ["jpg", "jpeg", "png", "webp", "gif"];
const AVIF_EXTS: [&str; 2] = ["avif", "avifs"];

/// The archive container formats this reader understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    SevenZ,
    Rar,
}

impl ArchiveFormat {
    /// Identify a container from its first bytes. `None` for anything unknown.
    pub fn sniff(head: &[u8]) -> Option<Self> {
        if head.starts_with(b"PK") {
            Some(Self::Zip)
        } else if head.starts_with(&[b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C]) {
            Some(Self::SevenZ)
        } else if head.starts_with(b"Rar!\x1A\x07") {
            Some(Self::Rar)
        } else {
            None
        }
    }
}

/// A page (or metadata file) inside the archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    index: usize,
    /// Normalised name: `/` separators, no leading `./`.
    pub name: String,
    /// Declared uncompressed size.
    pub size: u64,
}

/// What every format reports about one entry before anything is inflated.
struct Raw {
    index: usize,
    name: String,
    is_dir: bool,
    encrypted: bool,
    symlink: bool,
    size: u64,
    /// Compressed size when the format records it per entry.
    packed: Option<u64>,
}

/// The classified listing, identical for all formats.
struct Index {
    pages: Vec<Entry>,
    comic_info: Option<Entry>,
    series_json: Option<Entry>,
    skipped: Vec<String>,
}

/// Per-archive state shared by the variants.
struct Core {
    limits: ArchiveLimits,
    index: Index,
    read_total: u64,
}

pub enum ArchiveReader<R: Read + Seek> {
    Zip(ZipReader<R>),
    SevenZ(Box<SevenReader<R>>),
    Rar(Box<RarReader>),
}

pub struct ZipReader<R: Read + Seek> {
    zip: ZipArchive<R>,
    core: Core,
}

pub struct SevenReader<R: Read + Seek> {
    archive: SevenArchive,
    source: R,
    password: Password,
    /// File indexes that are pages or metadata.
    wanted: HashSet<usize>,
    /// Decoded entries of solid blocks that were decoded in one go.
    cache: HashMap<usize, Vec<u8>>,
    core: Core,
}

pub struct RarReader {
    /// The spooled upload; removed from disk on drop.
    file: tempfile::TempPath,
    /// An open cursor and the index of the next header it will return.
    cursor: Option<(usize, OpenArchive<Process, CursorBeforeHeader>)>,
    core: Core,
}

fn is_junk(name: &str) -> bool {
    let lower = name.to_lowercase();
    let base = lower.rsplit('/').next().unwrap_or(&lower);
    lower
        .split('/')
        .any(|c| c == "__macosx" || c == ".git" || c == ".svn")
        || base.starts_with("._")
        || base.starts_with('.')
        || matches!(base, "thumbs.db" | "desktop.ini" | "ehthumbs.db")
}

/// Normalise an entry name and refuse anything that could escape its directory.
fn safe_name(raw: &str) -> Result<String, IngestError> {
    let unsafe_name = || IngestError::UnsafePath(raw.chars().take(200).collect());
    if raw.is_empty() || raw.contains('\0') {
        return Err(unsafe_name());
    }
    let name = raw.replace('\\', "/");
    let b = name.as_bytes();
    if name.starts_with('/') || (b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic()) {
        return Err(unsafe_name());
    }
    let mut parts = Vec::new();
    for c in name.split('/') {
        match c {
            ".." => return Err(unsafe_name()),
            "" | "." => {}
            c => parts.push(c),
        }
    }
    if parts.is_empty() || name.len() > 1024 {
        return Err(unsafe_name());
    }
    Ok(parts.join("/"))
}

fn ext(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

fn base(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

/// Keep the shallowest candidate; ties by natural order.
fn prefer(slot: &mut Option<Entry>, e: Entry) {
    let depth = |n: &str| n.matches('/').count();
    let better = match slot {
        None => true,
        Some(cur) => (depth(&e.name), &e.name) < (depth(&cur.name), &cur.name),
    };
    if better {
        *slot = Some(e);
    }
}

fn archive_err(e: impl std::fmt::Display) -> IngestError {
    IngestError::Archive(e.to_string())
}

/// Apply the shared rules to a format's raw listing.
///
/// `solid` formats (7z, RAR) must decode every entry in order to reach a page, so their
/// budget counts all entries; ZIP counts the pages it will actually inflate.
fn build_index(raws: Vec<Raw>, limits: &ArchiveLimits, solid: bool) -> Result<Index, IngestError> {
    let (mut pages, mut comic_info, mut series_json) = (Vec::new(), None, None);
    let (mut skipped, mut avif) = (Vec::new(), Vec::new());
    let mut declared_total = 0u64;
    for r in raws {
        let name = safe_name(&r.name)?;
        if r.encrypted {
            return Err(IngestError::Encrypted(name));
        }
        if r.is_dir || r.name.ends_with('/') {
            continue;
        }
        if r.symlink {
            skipped.push(name);
            continue;
        }
        let size = r.size;
        if size > limits.max_entry_bytes {
            return Err(IngestError::EntryTooLarge(name));
        }
        if let Some(packed) = r.packed
            && size >= limits.ratio_floor
            && size / packed.max(1) > limits.max_ratio
        {
            return Err(IngestError::SuspiciousRatio(name));
        }
        if solid {
            declared_total = declared_total.saturating_add(size);
            if declared_total > limits.max_total_bytes {
                return Err(IngestError::TooLarge("total uncompressed size"));
            }
        }
        if is_junk(&name) {
            skipped.push(name);
            continue;
        }
        let entry = Entry {
            index: r.index,
            name,
            size,
        };
        let file = base(&entry.name).to_lowercase();
        let e = ext(&entry.name);
        if file == "comicinfo.xml" && size <= MAX_METADATA_BYTES {
            prefer(&mut comic_info, entry);
        } else if file == "series.json" && size <= MAX_METADATA_BYTES {
            prefer(&mut series_json, entry);
        } else if AVIF_EXTS.contains(&e.as_str()) {
            avif.push(entry.name);
        } else if IMAGE_EXTS.contains(&e.as_str()) {
            if !solid {
                declared_total = declared_total.saturating_add(size);
                if declared_total > limits.max_total_bytes {
                    return Err(IngestError::TooLarge("total uncompressed size"));
                }
            }
            pages.push(entry);
        } else {
            // Includes nested archives: they are never opened.
            skipped.push(entry.name);
        }
    }
    if !avif.is_empty() {
        avif.sort_by(|a, b| natural_cmp(a, b));
        return Err(IngestError::UnsupportedFormat {
            what: "AVIF pages",
            files: avif,
        });
    }
    pages.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    Ok(Index {
        pages,
        comic_info,
        series_json,
        skipped,
    })
}

/// The whole-archive ratio, for formats without a per-entry packed size.
fn check_archive_ratio(
    declared: u64,
    archive_len: u64,
    limits: &ArchiveLimits,
) -> Result<(), IngestError> {
    if declared >= limits.ratio_floor && declared / archive_len.max(1) > limits.max_ratio {
        return Err(IngestError::SuspiciousRatio("(whole archive)".into()));
    }
    Ok(())
}

impl Core {
    /// Enforce the per-entry cap, the header's size and the running budget on bytes that
    /// were read with `cap + 1` as the hard stop.
    fn account(&mut self, e: &Entry, out: Vec<u8>) -> Result<Vec<u8>, IngestError> {
        let n = out.len() as u64;
        if n > self.cap(e) {
            return Err(IngestError::EntryTooLarge(e.name.clone()));
        }
        if n != e.size {
            return Err(IngestError::Archive(format!(
                "{}: size does not match the header",
                e.name
            )));
        }
        self.read_total = self.read_total.saturating_add(n);
        if self.read_total > self.limits.max_total_bytes {
            return Err(IngestError::TooLarge("total uncompressed size"));
        }
        Ok(out)
    }

    /// Never read more than the header promised, nor more than the per-entry limit.
    fn cap(&self, e: &Entry) -> u64 {
        e.size.min(self.limits.max_entry_bytes)
    }
}

fn read_capped(r: impl Read, cap: u64, name: &str) -> Result<Vec<u8>, IngestError> {
    let mut out = Vec::with_capacity(usize::try_from(cap.min(4 << 20)).unwrap_or(0));
    r.take(cap + 1)
        .read_to_end(&mut out)
        .map_err(|err| IngestError::Archive(format!("{name}: {err}")))?;
    Ok(out)
}

// ---------------------------------------------------------------- ZIP

fn open_zip<R: Read + Seek>(reader: R, limits: ArchiveLimits) -> Result<ZipReader<R>, IngestError> {
    let mut zip = ZipArchive::new(reader).map_err(archive_err)?;
    if zip.len() > limits.max_entries {
        return Err(IngestError::TooManyEntries);
    }
    let mut raws = Vec::with_capacity(zip.len());
    for index in 0..zip.len() {
        // `by_index_raw` reads the header only; no decompression happens here.
        let f = zip.by_index_raw(index).map_err(archive_err)?;
        let name = f.name().map_err(archive_err)?.into_owned();
        raws.push(Raw {
            index,
            is_dir: f.is_dir(),
            encrypted: f.encrypted(),
            symlink: f.is_symlink(),
            size: f.size(),
            packed: Some(f.compressed_size()),
            name,
        });
    }
    let index = build_index(raws, &limits, false)?;
    Ok(ZipReader {
        zip,
        core: Core {
            limits,
            index,
            read_total: 0,
        },
    })
}

impl<R: Read + Seek> ZipReader<R> {
    fn read(&mut self, e: &Entry) -> Result<Vec<u8>, IngestError> {
        let cap = self.core.cap(e);
        let file = self.zip.by_index(e.index).map_err(archive_err)?;
        let out = read_capped(file, cap, &e.name)?;
        self.core.account(e, out)
    }
}

// ---------------------------------------------------------------- 7z

const SEVENZ_AES: [u8; 4] = [0x06, 0xF1, 0x07, 0x01];
const SEVENZ_LZMA: [u8; 3] = [0x03, 0x01, 0x01];
const SEVENZ_LZMA2: [u8; 1] = [0x21];
const SEVENZ_PPMD: [u8; 3] = [0x03, 0x04, 0x01];

/// Memory a coder will allocate according to its properties, when it says so.
fn coder_memory(id: &[u8], props: &[u8]) -> u64 {
    if (id == SEVENZ_LZMA || id == SEVENZ_PPMD) && props.len() >= 5 {
        // LZMA: dictionary size; PPMd: model memory. Both a little-endian u32 at offset 1.
        u64::from(u32::from_le_bytes([props[1], props[2], props[3], props[4]]))
    } else if id == SEVENZ_LZMA2 && props.len() == 1 {
        let p = u64::from(props[0]);
        if p >= 40 {
            u64::from(u32::MAX)
        } else {
            (2 | (p & 1)) << (p / 2 + 11)
        }
    } else {
        0
    }
}

fn sevenz_err(e: sevenz_rust2::Error) -> IngestError {
    match e {
        sevenz_rust2::Error::PasswordRequired | sevenz_rust2::Error::MaybeBadPassword(_) => {
            IngestError::Encrypted("(archive)".into())
        }
        e => archive_err(e),
    }
}

fn open_sevenz<R: Read + Seek>(
    mut source: R,
    limits: ArchiveLimits,
    archive_len: u64,
) -> Result<SevenReader<R>, IngestError> {
    let password = Password::empty();
    let archive = SevenArchive::read(&mut source, &password).map_err(sevenz_err)?;
    if archive.files.len() > limits.max_entries {
        return Err(IngestError::TooManyEntries);
    }
    // Per-block checks: encryption, coder memory, unpacked size and ratio.
    let first_pack = archive.stream_map.block_first_pack_stream_index();
    for (b, block) in archive.blocks.iter().enumerate() {
        for coder in &block.coders {
            let id = coder.encoder_method_id();
            if id == SEVENZ_AES {
                return Err(IngestError::Encrypted("(archive)".into()));
            }
            if coder_memory(id, coder.properties()) > MAX_7Z_CODER_MEMORY {
                return Err(IngestError::TooLarge("7z coder memory"));
            }
        }
        let unpacked = block.get_unpack_size();
        if unpacked > limits.max_solid_block_bytes {
            return Err(IngestError::TooLarge("7z solid block"));
        }
        let from = first_pack.get(b).copied().unwrap_or(0);
        let to = first_pack
            .get(b + 1)
            .copied()
            .unwrap_or(archive.pack_sizes().len());
        let packed: u64 = archive
            .pack_sizes()
            .get(from..to)
            .map_or(0, |s| s.iter().sum());
        if unpacked >= limits.ratio_floor && unpacked / packed.max(1) > limits.max_ratio {
            return Err(IngestError::SuspiciousRatio(format!("(7z block {b})")));
        }
    }
    let mut raws = Vec::with_capacity(archive.files.len());
    let mut declared = 0u64;
    for (index, f) in archive.files.iter().enumerate() {
        let attr = f.windows_attributes();
        // 0x8000 marks a Unix mode in the high 16 bits; 0x400 is a Windows reparse point.
        let unix_symlink = attr & 0x8000 != 0 && (attr >> 16) & 0o170000 == 0o120000;
        let reparse = attr & 0x400 != 0;
        if !f.is_directory() {
            declared = declared.saturating_add(f.size());
        }
        raws.push(Raw {
            index,
            name: f.name().to_string(),
            is_dir: f.is_directory(),
            encrypted: false,
            symlink: unix_symlink || reparse,
            size: f.size(),
            packed: None,
        });
    }
    check_archive_ratio(declared, archive_len, &limits)?;
    let index = build_index(raws, &limits, true)?;
    let wanted = index
        .pages
        .iter()
        .chain(index.comic_info.iter())
        .chain(index.series_json.iter())
        .map(|e| e.index)
        .collect();
    Ok(SevenReader {
        archive,
        source,
        password,
        wanted,
        cache: HashMap::new(),
        core: Core {
            limits,
            index,
            read_total: 0,
        },
    })
}

impl<R: Read + Seek> SevenReader<R> {
    fn read(&mut self, e: &Entry) -> Result<Vec<u8>, IngestError> {
        if let Some(out) = self.cache.remove(&e.index) {
            return self.core.account(e, out);
        }
        let Some(block) = self.archive.stream_map.file_block_index[e.index] else {
            // No stream: an empty file.
            return self.core.account(e, Vec::new());
        };
        let start = self.archive.stream_map.block_first_file_index[block];
        let whole_block = self.archive.blocks[block].get_unpack_size() <= SOLID_CACHE_BYTES;
        let cap = self.core.cap(e);
        let max_entry = self.core.limits.max_entry_bytes;
        let (target, wanted) = (e.index, &self.wanted);
        let mut found: Option<Vec<u8>> = None;
        let mut extra: Vec<(usize, Vec<u8>)> = Vec::new();
        let mut pos = start;
        let mut dec = BlockDecoder::new(1, block, &self.archive, &self.password, &mut self.source);
        dec.set_thread_count(1);
        let other = |err: IngestError| sevenz_rust2::Error::Other(err.to_string().into());
        dec.for_each_entries(&mut |entry, reader| {
            let i = pos;
            pos += 1;
            if i == target {
                found = Some(read_capped(reader, cap, entry.name()).map_err(other)?);
                // Stop here unless the rest of this block is wanted too.
                Ok(whole_block)
            } else if whole_block && wanted.contains(&i) {
                let c = entry.size().min(max_entry);
                extra.push((i, read_capped(reader, c, entry.name()).map_err(other)?));
                Ok(true)
            } else {
                // Skipped entries are still decoded (solid), but not kept.
                std::io::copy(&mut reader.take(entry.size()), &mut std::io::sink())?;
                Ok(true)
            }
        })
        .map_err(sevenz_err)?;
        self.cache.extend(extra);
        let out = found.ok_or_else(|| IngestError::Archive(format!("{}: not found", e.name)))?;
        self.core.account(e, out)
    }
}

// ---------------------------------------------------------------- RAR

fn rar_err(e: unrar::error::UnrarError) -> IngestError {
    match e.code {
        RarCode::MissingPassword | RarCode::BadPassword => {
            IngestError::Encrypted("(archive)".into())
        }
        _ => archive_err(format!("rar: {e}")),
    }
}

fn open_rar<R: Read + Seek>(
    mut reader: R,
    limits: ArchiveLimits,
    archive_len: u64,
) -> Result<RarReader, IngestError> {
    if !limits.rar_enabled {
        return Err(IngestError::UnsupportedFormat {
            what: "RAR archives (disabled by the operator)",
            files: Vec::new(),
        });
    }
    // UnRAR opens a path, so spool the upload to a private temporary file.
    reader.seek(SeekFrom::Start(0)).map_err(archive_err)?;
    let mut tmp = tempfile::Builder::new()
        .prefix("sr-ingest-")
        .suffix(".rar")
        .tempfile()
        .map_err(archive_err)?;
    let copied = std::io::copy(&mut reader.take(limits.max_archive_bytes + 1), &mut tmp)
        .map_err(archive_err)?;
    if copied > limits.max_archive_bytes {
        return Err(IngestError::TooLarge("archive file"));
    }
    tmp.flush().map_err(archive_err)?;
    let file = tmp.into_temp_path();

    let listing = RarArchive::new(&*file)
        .open_for_listing()
        .map_err(rar_err)?;
    let mut raws = Vec::new();
    let mut declared = 0u64;
    for (index, h) in listing.enumerate() {
        if index >= limits.max_entries {
            return Err(IngestError::TooManyEntries);
        }
        let h = h.map_err(rar_err)?;
        if h.is_split() {
            return Err(IngestError::Archive(
                "multi-volume RAR archives are not supported".into(),
            ));
        }
        // A Unix symlink carries S_IFLNK in the attributes.
        let symlink = h.file_attr & 0xF000 == 0xA000;
        if h.is_file() {
            declared = declared.saturating_add(h.unpacked_size);
        }
        raws.push(Raw {
            index,
            name: h.filename.to_string_lossy().into_owned(),
            is_dir: h.is_directory(),
            encrypted: h.is_encrypted(),
            symlink,
            size: h.unpacked_size,
            packed: None,
        });
    }
    check_archive_ratio(declared, archive_len, &limits)?;
    let index = build_index(raws, &limits, true)?;
    Ok(RarReader {
        file,
        cursor: None,
        core: Core {
            limits,
            index,
            read_total: 0,
        },
    })
}

impl RarReader {
    fn open_cursor(path: &Path) -> Result<OpenArchive<Process, CursorBeforeHeader>, IngestError> {
        RarArchive::new(path).open_for_processing().map_err(rar_err)
    }

    fn read(&mut self, e: &Entry) -> Result<Vec<u8>, IngestError> {
        // Archives are solid, so entries must be passed in order: keep the cursor and only
        // reopen when asked for something before it.
        let (mut at, mut cur) = match self.cursor.take() {
            Some((at, cur)) if at <= e.index => (at, cur),
            _ => (0, Self::open_cursor(&self.file)?),
        };
        loop {
            let Some(header) = cur.read_header().map_err(rar_err)? else {
                return Err(IngestError::Archive(format!("{}: not found", e.name)));
            };
            if at == e.index {
                // The listing vouched for this size; UnRAR stops at the declared size.
                if header.entry().unpacked_size != e.size {
                    return Err(IngestError::Archive(format!(
                        "{}: size does not match the header",
                        e.name
                    )));
                }
                let (data, next) = header.read().map_err(rar_err)?;
                self.cursor = Some((at + 1, next));
                return self.core.account(e, data);
            }
            cur = header.skip().map_err(rar_err)?;
            at += 1;
        }
    }
}

// ---------------------------------------------------------------- the enum

fn read_up_to(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

impl<R: Read + Seek> ArchiveReader<R> {
    /// Open an archive of any supported format and index its pages. Nothing is inflated
    /// yet.
    pub fn open(mut reader: R, limits: ArchiveLimits) -> Result<Self, IngestError> {
        let len = reader.seek(SeekFrom::End(0)).map_err(archive_err)?;
        if len > limits.max_archive_bytes {
            return Err(IngestError::TooLarge("archive file"));
        }
        reader.seek(SeekFrom::Start(0)).map_err(archive_err)?;
        let mut head = [0u8; 8];
        let n = read_up_to(&mut reader, &mut head).map_err(archive_err)?;
        reader.seek(SeekFrom::Start(0)).map_err(archive_err)?;
        // Anything unrecognised goes to the ZIP reader, which reports it as unreadable.
        match ArchiveFormat::sniff(&head[..n]).unwrap_or(ArchiveFormat::Zip) {
            ArchiveFormat::Zip => open_zip(reader, limits).map(Self::Zip),
            ArchiveFormat::SevenZ => {
                open_sevenz(reader, limits, len).map(|r| Self::SevenZ(Box::new(r)))
            }
            ArchiveFormat::Rar => open_rar(reader, limits, len).map(|r| Self::Rar(Box::new(r))),
        }
    }

    /// Which container this is.
    pub fn format(&self) -> ArchiveFormat {
        match self {
            Self::Zip(_) => ArchiveFormat::Zip,
            Self::SevenZ(_) => ArchiveFormat::SevenZ,
            Self::Rar(_) => ArchiveFormat::Rar,
        }
    }

    fn core(&self) -> &Core {
        match self {
            Self::Zip(r) => &r.core,
            Self::SevenZ(r) => &r.core,
            Self::Rar(r) => &r.core,
        }
    }

    /// Pages in reading order.
    pub fn pages(&self) -> &[Entry] {
        &self.core().index.pages
    }

    /// Names that were ignored (junk, symlinks, unsupported files, nested archives).
    pub fn skipped(&self) -> &[String] {
        &self.core().index.skipped
    }

    /// `ComicInfo.xml`, shallowest one, when present.
    pub fn comic_info(&mut self) -> Result<Option<Vec<u8>>, IngestError> {
        match self.core().index.comic_info.clone() {
            Some(e) => self.read(&e).map(Some),
            None => Ok(None),
        }
    }

    /// `series.json`, shallowest one, when present.
    pub fn series_json(&mut self) -> Result<Option<Vec<u8>>, IngestError> {
        match self.core().index.series_json.clone() {
            Some(e) => self.read(&e).map(Some),
            None => Ok(None),
        }
    }

    /// Inflate one entry. The read is capped at the per-entry limit whatever the header
    /// claims, counted against the archive's total budget, and must match the declared size.
    pub fn read(&mut self, e: &Entry) -> Result<Vec<u8>, IngestError> {
        match self {
            Self::Zip(r) => r.read(e),
            Self::SevenZ(r) => r.read(e),
            Self::Rar(r) => r.read(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    fn build(entries: &[(&str, &[u8])]) -> Vec<u8> {
        build_with(entries, CompressionMethod::Deflated)
    }

    fn build_with(entries: &[(&str, &[u8])], method: CompressionMethod) -> Vec<u8> {
        let mut w = ZipWriter::new(Cursor::new(Vec::new()));
        let o = SimpleFileOptions::default().compression_method(method);
        for (name, data) in entries {
            w.start_file(*name, o).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    fn open(
        bytes: Vec<u8>,
        l: ArchiveLimits,
    ) -> Result<ArchiveReader<Cursor<Vec<u8>>>, IngestError> {
        ArchiveReader::open(Cursor::new(bytes), l)
    }

    fn names(r: &ArchiveReader<Cursor<Vec<u8>>>) -> Vec<&str> {
        r.pages().iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn pages_come_out_in_natural_order() {
        let z = build(&[
            ("10.jpg", b"j"),
            ("2.jpg", b"j"),
            ("1.png", b"p"),
            ("Ch 2/002.webp", b"w"),
            ("Ch 2/001.gif", b"g"),
        ]);
        let r = open(z, ArchiveLimits::default()).unwrap();
        assert_eq!(
            names(&r),
            ["1.png", "2.jpg", "10.jpg", "Ch 2/001.gif", "Ch 2/002.webp"]
        );
    }

    #[test]
    fn junk_and_non_images_are_skipped() {
        let z = build(&[
            ("001.jpg", b"a"),
            ("__MACOSX/._001.jpg", b"x"),
            ("__MACOSX/002.jpg", b"x"),
            (".DS_Store", b"x"),
            ("sub/Thumbs.db", b"x"),
            ("desktop.ini", b"x"),
            ("._002.jpg", b"x"),
            (".hidden.jpg", b"x"),
            ("readme.txt", b"x"),
            ("scans.nfo", b"x"),
            ("nested.zip", b"x"),
            ("vector.svg", b"<svg/>"),
            ("002.jpg", b"b"),
            ("dir/", b""),
        ]);
        let r = open(z, ArchiveLimits::default()).unwrap();
        assert_eq!(names(&r), ["001.jpg", "002.jpg"]);
        assert!(r.skipped().iter().any(|s| s == "__MACOSX/._001.jpg"));
        assert!(r.skipped().iter().any(|s| s == "nested.zip"));
        assert!(r.skipped().iter().any(|s| s == "vector.svg"));
    }

    #[test]
    fn metadata_files_are_found_at_the_shallowest_level() {
        let z = build(&[
            ("a/ComicInfo.xml", b"<deep/>"),
            ("comicinfo.XML", b"<top/>"),
            ("series.json", b"{}"),
            ("1.jpg", b"j"),
        ]);
        let mut r = open(z, ArchiveLimits::default()).unwrap();
        assert_eq!(r.comic_info().unwrap().unwrap(), b"<top/>");
        assert_eq!(r.series_json().unwrap().unwrap(), b"{}");
        assert_eq!(names(&r), ["1.jpg"]);
        let mut none = open(build(&[("1.jpg", b"j")]), ArchiveLimits::default()).unwrap();
        assert!(none.comic_info().unwrap().is_none());
        assert!(none.series_json().unwrap().is_none());
    }

    #[test]
    fn page_bytes_round_trip() {
        let data: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
        let z = build(&[("1.jpg", &data), ("2.jpg", b"second")]);
        let mut r = open(z, ArchiveLimits::default()).unwrap();
        let pages = r.pages().to_vec();
        assert_eq!(r.read(&pages[0]).unwrap(), data);
        assert_eq!(r.read(&pages[1]).unwrap(), b"second");
    }

    #[test]
    fn path_traversal_names_are_rejected() {
        for bad in [
            "../evil.jpg",
            "a/../../evil.jpg",
            "a/../b/../../x.png",
            "..\\evil.jpg",
            "/abs/evil.jpg",
            "\\abs\\evil.jpg",
            "C:\\evil.jpg",
            "c:/evil.jpg",
        ] {
            let z = build(&[("ok.jpg", b"j"), (bad, b"x")]);
            assert!(
                matches!(
                    open(z, ArchiveLimits::default()),
                    Err(IngestError::UnsafePath(_))
                ),
                "{bad}"
            );
        }
        // Dots inside names and `./` prefixes are fine.
        let z = build(&[("./a/./1.jpg", b"j"), ("v1..2/2.jpg", b"j")]);
        let r = open(z, ArchiveLimits::default()).unwrap();
        assert_eq!(names(&r), ["a/1.jpg", "v1..2/2.jpg"]);
    }

    #[test]
    fn a_compression_bomb_is_rejected_before_inflating() {
        // 12 MiB of zeros deflates to ~12 KiB: ratio ~1000.
        let zeros = vec![0u8; 12 * 1024 * 1024];
        let z = build(&[("bomb.jpg", &zeros)]);
        assert!(z.len() < 64 * 1024);
        let l = ArchiveLimits {
            ratio_floor: 1024 * 1024,
            ..ArchiveLimits::default()
        };
        assert!(matches!(open(z, l), Err(IngestError::SuspiciousRatio(_))));
    }

    #[test]
    fn oversized_entries_totals_and_counts_are_rejected() {
        let big = vec![7u8; 100_000];
        let z = build_with(&[("a.jpg", &big)], CompressionMethod::Stored);
        let small = ArchiveLimits {
            max_entry_bytes: 50_000,
            ..ArchiveLimits::default()
        };
        assert!(matches!(
            open(z.clone(), small),
            Err(IngestError::EntryTooLarge(_))
        ));
        let total = ArchiveLimits {
            max_total_bytes: 150_000,
            ..ArchiveLimits::default()
        };
        let two = build_with(
            &[("a.jpg", &big), ("b.jpg", &big)],
            CompressionMethod::Stored,
        );
        assert!(matches!(open(two, total), Err(IngestError::TooLarge(_))));
        let archive_cap = ArchiveLimits {
            max_archive_bytes: 1000,
            ..ArchiveLimits::default()
        };
        assert!(matches!(
            open(z, archive_cap),
            Err(IngestError::TooLarge(_))
        ));
        let many: Vec<(String, &[u8])> = (0..30).map(|i| (format!("{i}.jpg"), &b"x"[..])).collect();
        let refs: Vec<(&str, &[u8])> = many.iter().map(|(n, d)| (n.as_str(), *d)).collect();
        let few = ArchiveLimits {
            max_entries: 10,
            ..ArchiveLimits::default()
        };
        assert!(matches!(
            open(build(&refs), few),
            Err(IngestError::TooManyEntries)
        ));
    }

    /// The running budget stops a series of individually fine entries.
    #[test]
    fn the_inflate_budget_is_enforced_while_reading() {
        let chunk = vec![1u8; 40_000];
        let z = build_with(
            &[("1.jpg", &chunk), ("2.jpg", &chunk), ("3.jpg", &chunk)],
            CompressionMethod::Stored,
        );
        let l = ArchiveLimits {
            max_total_bytes: 100_000,
            ..ArchiveLimits::default()
        };
        // Declared total (120 kB) already exceeds the budget.
        assert!(matches!(open(z, l), Err(IngestError::TooLarge(_))));
        let z = build_with(
            &[("1.jpg", &chunk), ("2.jpg", &chunk)],
            CompressionMethod::Stored,
        );
        let l = ArchiveLimits {
            max_total_bytes: 100_000,
            ..ArchiveLimits::default()
        };
        let mut r = open(z, l).unwrap();
        let pages = r.pages().to_vec();
        r.read(&pages[0]).unwrap();
        r.read(&pages[1]).unwrap();
        // Reading the same entries again goes over the running total.
        assert!(matches!(r.read(&pages[0]), Err(IngestError::TooLarge(_))));
    }

    /// A header that understates the size must not let more than the cap through.
    #[test]
    fn a_lying_size_header_cannot_exceed_the_cap() {
        let data = vec![9u8; 4096];
        let mut z = build_with(&[("1.jpg", &data)], CompressionMethod::Stored);
        // Patch the uncompressed size in the central directory (offset 24 of the header)
        // and in the local header (offset 22) down to 10 bytes.
        let patch = |z: &mut Vec<u8>, sig: [u8; 4], off: usize| {
            let at = z.windows(4).position(|w| w == sig).unwrap();
            z[at + off..at + off + 4].copy_from_slice(&10u32.to_le_bytes());
        };
        patch(&mut z, [0x50, 0x4b, 0x01, 0x02], 24);
        patch(&mut z, [0x50, 0x4b, 0x03, 0x04], 22);
        match open(z, ArchiveLimits::default()) {
            // Either the open or the read notices; neither may return the full 4096 bytes
            // as if they matched the header.
            Err(_) => {}
            Ok(mut r) => {
                let e = r.pages()[0].clone();
                assert!(r.read(&e).is_err());
            }
        }
    }

    #[test]
    fn garbage_is_not_an_archive() {
        assert!(matches!(
            open(b"definitely not a zip".to_vec(), ArchiveLimits::default()),
            Err(IngestError::Archive(_))
        ));
        assert!(matches!(
            open(Vec::new(), ArchiveLimits::default()),
            Err(IngestError::Archive(_))
        ));
    }

    #[test]
    fn symlink_entries_are_skipped() {
        let mut w = ZipWriter::new(Cursor::new(Vec::new()));
        w.start_file("1.jpg", SimpleFileOptions::default()).unwrap();
        w.write_all(b"j").unwrap();
        w.add_symlink("link.jpg", "/etc/passwd", SimpleFileOptions::default())
            .unwrap();
        let z = w.finish().unwrap().into_inner();
        let r = open(z, ArchiveLimits::default()).unwrap();
        assert_eq!(names(&r), ["1.jpg"]);
        assert!(r.skipped().iter().any(|s| s == "link.jpg"));
    }
}

#[cfg(test)]
#[path = "archive_formats_tests.rs"]
mod formats;
