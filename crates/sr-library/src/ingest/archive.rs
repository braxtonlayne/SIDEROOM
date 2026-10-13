//! A hardened ZIP / CBZ reader (LIB-ING-002).
//!
//! Archives are untrusted. This reader:
//!
//! * checks the file size, the entry count and every entry's declared size and
//!   compression ratio from the central directory, before inflating anything;
//! * inflates one entry at a time through a hard byte cap (`take`), so a header that
//!   lies about its size still cannot allocate more than the per-entry limit, and
//!   keeps a running budget across the whole archive;
//! * rejects (rather than repairs) names with `..`, absolute paths, drive letters or
//!   NUL, rejects encrypted entries, and skips symlinks. Names are only ever used as
//!   labels and sort keys, never as file system paths;
//! * never opens nested archives;
//! * lists pages in natural order and ignores junk: `__MACOSX/`, `.DS_Store`,
//!   `Thumbs.db`, `desktop.ini`, AppleDouble `._*` files, dot files and everything that
//!   is not a supported image, besides `ComicInfo.xml` and `series.json`.
//!
//! RAR/CBR and 7z/CB7 need non-pure-Rust code and are not read; see `docs/open-questions.md`.

use std::io::{Read, Seek, SeekFrom};

use zip::ZipArchive;

use super::{IngestError, natural_cmp};

/// Budgets for one archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArchiveLimits {
    /// Largest archive file.
    pub max_archive_bytes: u64,
    /// Most entries in the central directory.
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
        }
    }
}

/// Metadata files inside an archive are small; anything bigger is not metadata.
pub const MAX_METADATA_BYTES: u64 = 1024 * 1024;

const IMAGE_EXTS: [&str; 5] = ["jpg", "jpeg", "png", "webp", "gif"];

/// A page (or metadata file) inside the archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    index: usize,
    /// Normalised name: `/` separators, no leading `./`.
    pub name: String,
    /// Declared uncompressed size.
    pub size: u64,
}

pub struct ArchiveReader<R: Read + Seek> {
    zip: ZipArchive<R>,
    limits: ArchiveLimits,
    pages: Vec<Entry>,
    comic_info: Option<Entry>,
    series_json: Option<Entry>,
    skipped: Vec<String>,
    read_total: u64,
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

impl<R: Read + Seek> ArchiveReader<R> {
    /// Open an archive and index its pages. Nothing is inflated yet.
    pub fn open(mut reader: R, limits: ArchiveLimits) -> Result<Self, IngestError> {
        let len = reader
            .seek(SeekFrom::End(0))
            .map_err(|e| IngestError::Archive(e.to_string()))?;
        if len > limits.max_archive_bytes {
            return Err(IngestError::TooLarge("archive file"));
        }
        reader
            .seek(SeekFrom::Start(0))
            .map_err(|e| IngestError::Archive(e.to_string()))?;
        let mut zip = ZipArchive::new(reader).map_err(|e| IngestError::Archive(e.to_string()))?;
        if zip.len() > limits.max_entries {
            return Err(IngestError::TooManyEntries);
        }

        let (mut pages, mut comic_info, mut series_json) = (Vec::new(), None, None);
        let (mut skipped, mut declared_total) = (Vec::new(), 0u64);
        for index in 0..zip.len() {
            // `by_index_raw` reads the header only; no decompression happens here.
            let f = zip
                .by_index_raw(index)
                .map_err(|e| IngestError::Archive(e.to_string()))?;
            let raw_name = f
                .name()
                .map_err(|e| IngestError::Archive(e.to_string()))?
                .into_owned();
            let name = safe_name(&raw_name)?;
            if f.encrypted() {
                return Err(IngestError::Encrypted(name));
            }
            if f.is_dir() || raw_name.ends_with('/') {
                continue;
            }
            if f.is_symlink() {
                skipped.push(name);
                continue;
            }
            let (size, packed) = (f.size(), f.compressed_size());
            if size > limits.max_entry_bytes {
                return Err(IngestError::EntryTooLarge(name));
            }
            if size >= limits.ratio_floor && size / packed.max(1) > limits.max_ratio {
                return Err(IngestError::SuspiciousRatio(name));
            }
            if is_junk(&name) {
                skipped.push(name);
                continue;
            }
            let entry = Entry { index, name, size };
            let file = base(&entry.name).to_lowercase();
            if file == "comicinfo.xml" && size <= MAX_METADATA_BYTES {
                prefer(&mut comic_info, entry);
            } else if file == "series.json" && size <= MAX_METADATA_BYTES {
                prefer(&mut series_json, entry);
            } else if IMAGE_EXTS.contains(&ext(&entry.name).as_str()) {
                declared_total = declared_total.saturating_add(size);
                if declared_total > limits.max_total_bytes {
                    return Err(IngestError::TooLarge("total uncompressed size"));
                }
                pages.push(entry);
            } else {
                skipped.push(entry.name);
            }
        }
        pages.sort_by(|a, b| natural_cmp(&a.name, &b.name));
        Ok(Self {
            zip,
            limits,
            pages,
            comic_info,
            series_json,
            skipped,
            read_total: 0,
        })
    }

    /// Pages in reading order.
    pub fn pages(&self) -> &[Entry] {
        &self.pages
    }

    /// Names that were ignored (junk, symlinks, unsupported files).
    pub fn skipped(&self) -> &[String] {
        &self.skipped
    }

    /// `ComicInfo.xml`, shallowest one, when present.
    pub fn comic_info(&mut self) -> Result<Option<Vec<u8>>, IngestError> {
        match self.comic_info.clone() {
            Some(e) => self.read(&e).map(Some),
            None => Ok(None),
        }
    }

    /// `series.json`, shallowest one, when present.
    pub fn series_json(&mut self) -> Result<Option<Vec<u8>>, IngestError> {
        match self.series_json.clone() {
            Some(e) => self.read(&e).map(Some),
            None => Ok(None),
        }
    }

    /// Inflate one entry. The read is capped at the per-entry limit whatever the header
    /// claims, counted against the archive's total budget, and must match the declared size.
    pub fn read(&mut self, e: &Entry) -> Result<Vec<u8>, IngestError> {
        // Never inflate more than the header promised, nor more than the per-entry limit.
        let cap = e.size.min(self.limits.max_entry_bytes);
        let file = self
            .zip
            .by_index(e.index)
            .map_err(|err| IngestError::Archive(err.to_string()))?;
        let mut out = Vec::with_capacity(usize::try_from(cap).unwrap_or(0));
        file.take(cap + 1)
            .read_to_end(&mut out)
            .map_err(|err| IngestError::Archive(format!("{}: {err}", e.name)))?;
        let n = out.len() as u64;
        if n > cap {
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
