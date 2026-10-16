//! Archive pre-inspection (REV-UP-004): list what is inside a CBZ/ZIP, CB7/7z or CBR/RAR without
//! extracting a single page, so a submitter, a reviewer or the import wizard can see the structure
//! (chapter folders, page counts, ignored files, `ComicInfo.xml`) before anything is converted.
//!
//! The listing uses the same hardened reader as the real ingest ([`ArchiveReader`]), so it applies
//! the same limits and refuses the same archives (bombs, unsafe names, encryption, AVIF pages).
//! For an object in storage the file is read through [`RangeReader`]: ZIP and 7z keep their index
//! at the end of the file, so only that index is fetched, not the pages. RAR must be copied to a
//! temporary file to be listed, so it is only listed up to [`RAR_LISTING_MAX_BYTES`].
//!
//! The browser has its own CBZ pre-check (`sr-library-wasm`) for a file the member just picked;
//! this one serves files that are already on the server.

use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;

use bytes::Bytes;
use serde::Serialize;
use sr_storage::ObjectStorage;
use utoipa::ToSchema;

use super::archive::{ArchiveFormat, ArchiveLimits, ArchiveReader};
use super::{IngestError, comicinfo};

/// Largest RAR that is listed (the format cannot be listed without a local copy).
pub const RAR_LISTING_MAX_BYTES: u64 = 256 * 1024 * 1024;
/// Pages named in the report, from the start of reading order.
const FIRST_PAGES: usize = 12;
/// Ignored files named in the report.
const MAX_SKIPPED: usize = 50;
/// Bytes fetched per request when reading from storage (a ZIP reads one local header per entry).
const BLOCK: u64 = 16 * 1024;

/// Pages that share a folder inside the archive (a chapter folder, a volume folder, the root).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct FolderGroup {
    /// Folder path inside the archive; empty for the top level.
    pub path: String,
    pub pages: u32,
}

/// What `ComicInfo.xml` says, when the archive has one.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ComicInfoFacts {
    pub title: Option<String>,
    pub series: Option<String>,
    pub number: Option<f64>,
}

/// The pre-inspection of one archive.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ArchiveReport {
    /// `zip`, `7z` or `rar`.
    pub format: String,
    pub size_bytes: u64,
    pub pages: u32,
    /// Pages per folder, in reading order of their first page.
    pub folders: Vec<FolderGroup>,
    /// The first pages in reading order.
    pub first_pages: Vec<String>,
    /// Files the importer ignores (junk, symlinks, unsupported files, nested archives).
    pub skipped: Vec<String>,
    pub comic_info: Option<ComicInfoFacts>,
    pub has_series_json: bool,
    /// Why this archive would be refused or needs attention; empty when it looks fine.
    pub problems: Vec<String>,
}

fn format_name(f: ArchiveFormat) -> &'static str {
    match f {
        ArchiveFormat::Zip => "zip",
        ArchiveFormat::SevenZ => "7z",
        ArchiveFormat::Rar => "rar",
    }
}

fn folder_of(name: &str) -> &str {
    name.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// Inspect an archive read from `reader` (any `Read + Seek`: a local file, a range reader).
pub fn inspect<R: Read + Seek>(mut reader: R, limits: ArchiveLimits) -> ArchiveReport {
    let size = reader.seek(SeekFrom::End(0)).unwrap_or(0);
    let _ = reader.seek(SeekFrom::Start(0));
    let mut head = [0u8; 8];
    let n = reader.read(&mut head).unwrap_or(0);
    let _ = reader.seek(SeekFrom::Start(0));
    let sniffed = ArchiveFormat::sniff(&head[..n]);
    let format = sniffed.unwrap_or(ArchiveFormat::Zip);
    let mut report = ArchiveReport {
        format: format_name(format).to_owned(),
        size_bytes: size,
        pages: 0,
        folders: Vec::new(),
        first_pages: Vec::new(),
        skipped: Vec::new(),
        comic_info: None,
        has_series_json: false,
        problems: Vec::new(),
    };
    if sniffed.is_none() {
        report
            .problems
            .push("this file is not a ZIP, 7z or RAR archive".into());
        return report;
    }
    if format == ArchiveFormat::Rar && size > RAR_LISTING_MAX_BYTES {
        report.problems.push(
            "a RAR this large is listed when it is converted, not before; use CBZ to preview"
                .into(),
        );
        return report;
    }
    let mut archive = match ArchiveReader::open(reader, limits) {
        Ok(a) => a,
        Err(e) => {
            report.problems.push(describe(&e));
            return report;
        }
    };
    let pages = archive.pages();
    report.pages = u32::try_from(pages.len()).unwrap_or(u32::MAX);
    report.first_pages = pages
        .iter()
        .take(FIRST_PAGES)
        .map(|p| p.name.clone())
        .collect();
    for p in pages {
        let dir = folder_of(&p.name);
        match report.folders.last_mut() {
            Some(g) if g.path == dir => g.pages += 1,
            _ => match report.folders.iter_mut().find(|g| g.path == dir) {
                Some(g) => g.pages += 1,
                None => report.folders.push(FolderGroup {
                    path: dir.to_owned(),
                    pages: 1,
                }),
            },
        }
    }
    report.skipped = archive
        .skipped()
        .iter()
        .take(MAX_SKIPPED)
        .cloned()
        .collect();
    if let Ok(Some(xml)) = archive.comic_info()
        && let Ok(info) = comicinfo::parse(&xml)
    {
        report.comic_info = Some(ComicInfoFacts {
            title: info.title.clone(),
            series: info.series.clone(),
            number: info.chapter_number(),
        });
    }
    report.has_series_json = archive.series_json().ok().flatten().is_some();
    if report.pages == 0 {
        report.problems.push(describe(&IngestError::NoPages));
    }
    report
}

fn describe(e: &IngestError) -> String {
    e.to_string()
}

/// `Read + Seek` over an object in storage, fetching 16 KiB blocks on demand. Call it from a
/// blocking thread (`spawn_blocking`): every miss waits on the async runtime.
pub struct RangeReader {
    storage: Arc<dyn ObjectStorage>,
    key: String,
    size: u64,
    pos: u64,
    handle: tokio::runtime::Handle,
    block: Option<(u64, Bytes)>,
}

impl RangeReader {
    pub fn new(
        storage: Arc<dyn ObjectStorage>,
        key: String,
        size: u64,
        handle: tokio::runtime::Handle,
    ) -> Self {
        Self {
            storage,
            key,
            size,
            pos: 0,
            handle,
            block: None,
        }
    }
}

impl Read for RangeReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() || self.pos >= self.size {
            return Ok(0);
        }
        let cached = self
            .block
            .as_ref()
            .filter(|(start, b)| self.pos >= *start && self.pos < *start + b.len() as u64);
        if cached.is_none() {
            let start = self.pos / BLOCK * BLOCK;
            let end = (start + BLOCK).min(self.size);
            let data = self
                .handle
                .block_on(self.storage.get_range(&self.key, start, end))
                .map_err(std::io::Error::other)?;
            self.block = Some((start, data));
        }
        let (start, data) = self
            .block
            .as_ref()
            .ok_or_else(|| std::io::Error::other("no block"))?;
        let offset = usize::try_from(self.pos - *start).unwrap_or(usize::MAX);
        let avail = data.get(offset..).unwrap_or(&[]);
        let n = avail.len().min(buf.len());
        buf[..n].copy_from_slice(&avail[..n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for RangeReader {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        let next = match to {
            SeekFrom::Start(n) => i128::from(n),
            SeekFrom::End(d) => i128::from(self.size) + i128::from(d),
            SeekFrom::Current(d) => i128::from(self.pos) + i128::from(d),
        };
        if next < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "seek before the start",
            ));
        }
        self.pos = u64::try_from(next).unwrap_or(u64::MAX);
        Ok(self.pos)
    }
}

/// Inspect the archive stored under `key`.
pub async fn inspect_stored(
    storage: Arc<dyn ObjectStorage>,
    key: &str,
    limits: ArchiveLimits,
) -> Result<ArchiveReport, IngestError> {
    let info = storage
        .head(key)
        .await
        .map_err(|e| IngestError::Storage(e.to_string()))?;
    let handle = tokio::runtime::Handle::current();
    let reader = RangeReader::new(storage, key.to_owned(), info.size, handle);
    tokio::task::spawn_blocking(move || inspect(reader, limits))
        .await
        .map_err(|e| IngestError::Storage(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::{ZipWriter, write::SimpleFileOptions};

    fn png() -> Vec<u8> {
        let img = image::RgbImage::from_pixel(4, 4, image::Rgb([9, 9, 9]));
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    fn zip_of(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut w = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data) in files {
            w.start_file(*name, SimpleFileOptions::default()).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn lists_chapter_folders_without_converting() {
        let p = png();
        let bytes = zip_of(&[
            ("Series/Ch 1/001.png", p.clone()),
            ("Series/Ch 1/002.png", p.clone()),
            ("Series/Ch 2/001.png", p.clone()),
            ("Series/notes.txt", b"hi".to_vec()),
            (
                "ComicInfo.xml",
                b"<ComicInfo><Series>Blue Hour</Series><Title>Ch 1</Title><Number>1</Number></ComicInfo>".to_vec(),
            ),
        ]);
        let r = inspect(Cursor::new(bytes), ArchiveLimits::default());
        assert_eq!(r.format, "zip");
        assert_eq!(r.pages, 3);
        assert_eq!(
            r.folders,
            vec![
                FolderGroup {
                    path: "Series/Ch 1".into(),
                    pages: 2
                },
                FolderGroup {
                    path: "Series/Ch 2".into(),
                    pages: 1
                }
            ]
        );
        assert_eq!(r.first_pages[0], "Series/Ch 1/001.png");
        assert!(
            r.skipped.iter().any(|s| s.contains("notes.txt")),
            "{:?}",
            r.skipped
        );
        let ci = r.comic_info.unwrap();
        assert_eq!(ci.series.as_deref(), Some("Blue Hour"));
        assert_eq!(ci.number, Some(1.0));
        assert!(r.problems.is_empty(), "{:?}", r.problems);
    }

    #[test]
    fn says_why_an_archive_would_be_refused() {
        let r = inspect(
            Cursor::new(b"not an archive at all".to_vec()),
            ArchiveLimits::default(),
        );
        assert!(r.problems[0].contains("not a ZIP"), "{:?}", r.problems);
        let empty = zip_of(&[("readme.txt", b"x".to_vec())]);
        let r = inspect(Cursor::new(empty), ArchiveLimits::default());
        assert_eq!(r.pages, 0);
        assert!(
            r.problems[0].contains("no readable pages"),
            "{:?}",
            r.problems
        );
        let avif = zip_of(&[("001.avif", vec![0u8; 16]), ("002.png", png())]);
        let r = inspect(Cursor::new(avif), ArchiveLimits::default());
        assert!(r.problems[0].contains("unsupported"), "{:?}", r.problems);
    }

    #[test]
    fn a_hostile_name_is_reported_not_followed() {
        let evil = zip_of(&[("../../etc/passwd.png", png())]);
        let r = inspect(Cursor::new(evil), ArchiveLimits::default());
        assert!(
            r.problems[0].contains("not a safe relative path"),
            "{:?}",
            r.problems
        );
    }

    /// A storage that counts the bytes it hands out, to show the pages are never fetched.
    struct Counting {
        inner: sr_storage::ObjectStoreStorage,
        fetched: std::sync::atomic::AtomicU64,
    }

    #[async_trait::async_trait]
    impl ObjectStorage for Counting {
        async fn put(&self, key: &str, data: Bytes) -> sr_storage::Result<()> {
            self.inner.put(key, data).await
        }
        async fn get(&self, key: &str) -> sr_storage::Result<Bytes> {
            self.inner.get(key).await
        }
        async fn get_range(&self, key: &str, start: u64, end: u64) -> sr_storage::Result<Bytes> {
            self.fetched
                .fetch_add(end - start, std::sync::atomic::Ordering::SeqCst);
            self.inner.get_range(key, start, end).await
        }
        async fn head(&self, key: &str) -> sr_storage::Result<sr_storage::ObjectInfo> {
            self.inner.head(key).await
        }
        async fn delete(&self, key: &str) -> sr_storage::Result<()> {
            self.inner.delete(key).await
        }
        async fn list(&self, prefix: &str) -> sr_storage::Result<Vec<sr_storage::ObjectInfo>> {
            self.inner.list(prefix).await
        }
        async fn presign(
            &self,
            method: sr_storage::PresignMethod,
            key: &str,
            ttl: std::time::Duration,
        ) -> sr_storage::Result<String> {
            self.inner.presign(method, key, ttl).await
        }
        async fn multipart_create(
            &self,
            key: &str,
        ) -> sr_storage::Result<Box<dyn sr_storage::MultipartSession>> {
            self.inner.multipart_create(key).await
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_stored_archive_is_listed_from_its_index_not_downloaded() {
        // Fifty stored (uncompressed) 100 KB pages: 5 MB, but the listing needs a few blocks.
        let page = {
            let mut v = png();
            v.resize(100_000, 0);
            v
        };
        let files: Vec<(String, Vec<u8>)> = (1..=50)
            .map(|i| (format!("{i:03}.png"), page.clone()))
            .collect();
        let refs: Vec<(&str, Vec<u8>)> =
            files.iter().map(|(n, d)| (n.as_str(), d.clone())).collect();
        let mut w = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data) in &refs {
            w.start_file(
                *name,
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
            w.write_all(data).unwrap();
        }
        let bytes = w.finish().unwrap().into_inner();
        let total = bytes.len() as u64;
        let storage = Arc::new(Counting {
            inner: sr_storage::ObjectStoreStorage::in_memory(),
            fetched: Default::default(),
        });
        storage
            .put("library/1/ch.cbz", Bytes::from(bytes))
            .await
            .unwrap();
        let r = inspect_stored(
            storage.clone(),
            "library/1/ch.cbz",
            ArchiveLimits::default(),
        )
        .await
        .unwrap();
        assert_eq!(r.pages, 50);
        let fetched = storage.fetched.load(std::sync::atomic::Ordering::SeqCst);
        assert!(
            fetched < total / 4,
            "fetched {fetched} of {total} bytes: the pages were not downloaded"
        );
    }
}
