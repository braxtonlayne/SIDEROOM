//! A `Read + Seek` over one stored object, for the `zip` crate (SEC-AUD-173, D-654).
//!
//! The zip of an Activity package never travels through memory whole. [`ZipStoreReader`] answers the reads of the zip
//! reader from the object store with ranged `get`s, in blocks of [`CHECK_BLOCK`] (4 MiB) that are aligned, so a
//! sequential pass fetches every byte once, and it keeps the last [`CACHE_BLOCKS`] blocks, so a small step backwards
//! costs nothing. It must be used from a blocking thread (`spawn_blocking`): every miss waits for the runtime
//! through a [`tokio::runtime::Handle`].
//!
//! **Memory bound of a check**, whatever the package size: the reader's cache (2 blocks, 8 MiB), the inflate window
//! and buffers of the `zip`/`flate2` readers (well under 1 MiB), the copy buffer (64 KiB), one output part of the
//! file being stored (at most 8 MiB, [`crate::service::STORE_PART_BYTES`]), and the zip's index, which the `zip` crate parses
//! whole: its entry count and size are read from the end record and refused above the application's file limit
//! before any of it is read ([`peek_end_record`]). Nothing else is kept except the manifest (8 KiB).
//!
//! Work is bounded too: the reader counts what it fetched ([`ReadStats`]) and, when given a budget, fails the next
//! fetch once the budget is spent, so a zip whose entries are ordered to defeat the read-ahead cannot make the
//! server read the store without end.

use std::collections::VecDeque;
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use async_trait::async_trait;
use bytes::Bytes;
use sr_storage::ObjectStorage;

/// Size of one aligned fetch of the package check.
pub const CHECK_BLOCK: u64 = 4 * 1024 * 1024;
/// Blocks kept after use.
pub const CACHE_BLOCKS: usize = 2;

/// Where the bytes come from: a range of one object.
#[async_trait]
pub trait RangeSource: Send + Sync {
    /// The bytes `[start, end)`; a short or empty answer is an error.
    async fn fetch(&self, start: u64, end: u64) -> Result<Bytes, String>;
}

/// One object of an [`ObjectStorage`].
pub struct StoredObject {
    pub storage: Arc<dyn ObjectStorage>,
    pub key: String,
}

#[async_trait]
impl RangeSource for StoredObject {
    async fn fetch(&self, start: u64, end: u64) -> Result<Bytes, String> {
        self.storage
            .get_range(&self.key, start, end)
            .await
            .map_err(|e| e.to_string())
    }
}

/// A request for a range, sent by a [`ChannelSource`] to whoever owns the real object.
pub struct RangeRequest {
    pub start: u64,
    pub end: u64,
    pub reply: tokio::sync::oneshot::Sender<Result<Bytes, String>>,
}

/// A [`RangeSource`] whose answers come from another task over a channel, for an object that cannot be handed to the
/// blocking thread because it is only borrowed (a cartridge on a shelf, D-656): the async side serves
/// [`RangeRequest`]s while the blocking thread reads the zip.
pub struct ChannelSource {
    pub tx: tokio::sync::mpsc::UnboundedSender<RangeRequest>,
}

#[async_trait]
impl RangeSource for ChannelSource {
    async fn fetch(&self, start: u64, end: u64) -> Result<Bytes, String> {
        let (reply, answer) = tokio::sync::oneshot::channel();
        self.tx
            .send(RangeRequest { start, end, reply })
            .map_err(|_| "the reader is gone".to_owned())?;
        answer.await.map_err(|_| "no answer".to_owned())?
    }
}

/// Why a read failed, carried inside the `io::Error` so the checker can tell a broken store from a broken zip.
#[derive(Debug)]
pub enum StoreFailure {
    /// The store could not answer.
    Store(String),
    /// The read budget is spent.
    Budget,
}

impl std::fmt::Display for StoreFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(m) => write!(f, "the object store failed: {m}"),
            Self::Budget => f.write_str("the read budget of this check is spent"),
        }
    }
}

impl std::error::Error for StoreFailure {}

/// The [`StoreFailure`] inside an `io::Error`, if that is what it is.
pub fn store_failure(e: &io::Error) -> Option<&StoreFailure> {
    e.get_ref()?.downcast_ref::<StoreFailure>()
}

/// What a reader fetched (shared, so a test or a progress line can read it while the reader is in use).
#[derive(Debug, Default)]
pub struct ReadStats {
    fetched: AtomicU64,
    fetches: AtomicU64,
    largest: AtomicU64,
    over_budget: AtomicBool,
}

impl ReadStats {
    /// Bytes fetched in all.
    pub fn fetched(&self) -> u64 {
        self.fetched.load(Ordering::Relaxed)
    }
    /// Number of fetches.
    pub fn fetches(&self) -> u64 {
        self.fetches.load(Ordering::Relaxed)
    }
    /// Bytes of the largest single fetch.
    pub fn largest_fetch(&self) -> u64 {
        self.largest.load(Ordering::Relaxed)
    }
    /// Whether a fetch was refused because the budget was spent.
    pub fn over_budget(&self) -> bool {
        self.over_budget.load(Ordering::Relaxed)
    }
}

/// `Read + Seek` over a [`RangeSource`] of known length.
pub struct ZipStoreReader {
    source: Arc<dyn RangeSource>,
    size: u64,
    pos: u64,
    handle: tokio::runtime::Handle,
    block: u64,
    cache: VecDeque<(u64, Bytes)>,
    budget: Option<u64>,
    stats: Arc<ReadStats>,
}

impl ZipStoreReader {
    /// `block` is the aligned fetch size; `budget` the most bytes this reader may fetch in all.
    pub fn new(
        source: Arc<dyn RangeSource>,
        size: u64,
        handle: tokio::runtime::Handle,
        block: u64,
        budget: Option<u64>,
    ) -> Self {
        Self {
            source,
            size,
            pos: 0,
            handle,
            block: block.max(1),
            cache: VecDeque::with_capacity(CACHE_BLOCKS + 1),
            budget,
            stats: Arc::new(ReadStats::default()),
        }
    }

    /// The package check's reader over one object: 4 MiB blocks and a budget of four times the object plus 16 MiB
    /// (a zip written in path order is read once; one written in a hostile order is stopped).
    pub fn for_object(
        storage: Arc<dyn ObjectStorage>,
        key: &str,
        size: u64,
        handle: tokio::runtime::Handle,
    ) -> Self {
        let source = Arc::new(StoredObject {
            storage,
            key: key.to_owned(),
        });
        Self::new(
            source,
            size,
            handle,
            CHECK_BLOCK,
            Some(size.saturating_mul(4).saturating_add(16 * 1024 * 1024)),
        )
    }

    /// The counters of this reader.
    pub fn stats(&self) -> Arc<ReadStats> {
        self.stats.clone()
    }

    fn fill(&mut self, start: u64) -> io::Result<()> {
        let end = (start + self.block).min(self.size);
        let len = end - start;
        if let Some(b) = self.budget
            && self.stats.fetched() + len > b
        {
            self.stats.over_budget.store(true, Ordering::Relaxed);
            return Err(io::Error::other(StoreFailure::Budget));
        }
        let data = self
            .handle
            .block_on(self.source.fetch(start, end))
            .map_err(|m| io::Error::other(StoreFailure::Store(m)))?;
        if data.len() as u64 != len {
            return Err(io::Error::other(StoreFailure::Store(format!(
                "asked for {len} bytes at {start} and got {}",
                data.len()
            ))));
        }
        self.stats.fetched.fetch_add(len, Ordering::Relaxed);
        self.stats.fetches.fetch_add(1, Ordering::Relaxed);
        self.stats.largest.fetch_max(len, Ordering::Relaxed);
        if self.cache.len() >= CACHE_BLOCKS {
            self.cache.pop_front();
        }
        self.cache.push_back((start, data));
        Ok(())
    }
}

impl Read for ZipStoreReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() || self.pos >= self.size {
            return Ok(0);
        }
        let start = self.pos / self.block * self.block;
        if !self.cache.iter().any(|(s, _)| *s == start) {
            self.fill(start)?;
        }
        let Some((_, data)) = self.cache.iter().find(|(s, _)| *s == start) else {
            return Err(io::Error::other("block missing after fetch"));
        };
        let offset = usize::try_from(self.pos - start).unwrap_or(usize::MAX);
        let avail = data.get(offset..).unwrap_or(&[]);
        let n = avail.len().min(buf.len());
        buf[..n].copy_from_slice(&avail[..n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for ZipStoreReader {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let next = match to {
            SeekFrom::Start(n) => i128::from(n),
            SeekFrom::End(d) => i128::from(self.size) + i128::from(d),
            SeekFrom::Current(d) => i128::from(self.pos) + i128::from(d),
        };
        if next < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek before the start",
            ));
        }
        self.pos = u64::try_from(next).unwrap_or(u64::MAX);
        Ok(self.pos)
    }
}

/// What the end record of a zip says about its index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndRecord {
    /// Entries in the index.
    pub entries: u64,
    /// Bytes of the index.
    pub index_bytes: u64,
}

const EOCD_SIG: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
const ZIP64_LOCATOR_SIG: [u8; 4] = [0x50, 0x4b, 0x06, 0x07];
const ZIP64_EOCD_SIG: [u8; 4] = [0x50, 0x4b, 0x06, 0x06];

fn le16(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from(u16::from_le_bytes(
        b.get(at..at + 2)?.try_into().ok()?,
    )))
}
fn le32(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from(u32::from_le_bytes(
        b.get(at..at + 4)?.try_into().ok()?,
    )))
}
fn le64(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// The entry count and index size from the end record of a zip (Zip64 included), read from the last 64 KiB and,
/// for Zip64, one more small read. `None` when no end record is found (the zip reader will say "not a zip"). The
/// reader's position afterwards is unspecified.
pub fn peek_end_record<R: Read + Seek>(r: &mut R, size: u64) -> io::Result<Option<EndRecord>> {
    let tail_len = size.min(64 * 1024 + 22);
    r.seek(SeekFrom::Start(size - tail_len))?;
    let mut tail = vec![0u8; usize::try_from(tail_len).unwrap_or(0)];
    r.read_exact(&mut tail)?;
    let Some(at) = tail.windows(4).rposition(|w| w == EOCD_SIG) else {
        return Ok(None);
    };
    let (Some(entries16), Some(size32)) = (le16(&tail, at + 10), le32(&tail, at + 12)) else {
        return Ok(None);
    };
    if entries16 != 0xFFFF && size32 != 0xFFFF_FFFF {
        return Ok(Some(EndRecord {
            entries: entries16,
            index_bytes: size32,
        }));
    }
    // Zip64: the locator sits right before the end record and names the Zip64 end record.
    let Some(loc) = at.checked_sub(20) else {
        return Ok(None);
    };
    if tail.get(loc..loc + 4) != Some(&ZIP64_LOCATOR_SIG[..]) {
        return Ok(None);
    }
    let Some(off) = le64(&tail, loc + 8) else {
        return Ok(None);
    };
    if off.saturating_add(56) > size {
        return Ok(None);
    }
    r.seek(SeekFrom::Start(off))?;
    let mut rec = [0u8; 56];
    r.read_exact(&mut rec)?;
    if rec[..4] != ZIP64_EOCD_SIG {
        return Ok(None);
    }
    match (le64(&rec, 32), le64(&rec, 40)) {
        (Some(entries), Some(index_bytes)) => Ok(Some(EndRecord {
            entries,
            index_bytes,
        })),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_storage::ObjectStoreStorage;
    use std::io::Cursor;

    #[tokio::test(flavor = "multi_thread")]
    async fn reads_and_seeks_like_the_object_and_never_fetches_more_than_a_block() {
        let data: Vec<u8> = (0..1_000_000u32).map(|i| (i % 251) as u8).collect();
        let storage: Arc<dyn ObjectStorage> = Arc::new(ObjectStoreStorage::in_memory());
        storage.put("k", Bytes::from(data.clone())).await.unwrap();
        let h = tokio::runtime::Handle::current();
        let src = Arc::new(StoredObject {
            storage,
            key: "k".into(),
        });
        let out = tokio::task::spawn_blocking(move || {
            let mut r = ZipStoreReader::new(src, 1_000_000, h, 64 * 1024, None);
            let stats = r.stats();
            let mut all = Vec::new();
            r.read_to_end(&mut all).unwrap();
            r.seek(SeekFrom::Start(500_000)).unwrap();
            let mut mid = [0u8; 10];
            r.read_exact(&mut mid).unwrap();
            r.seek(SeekFrom::End(-5)).unwrap();
            let mut end = Vec::new();
            r.read_to_end(&mut end).unwrap();
            (all, mid, end, stats.largest_fetch(), stats.fetched())
        })
        .await
        .unwrap();
        assert_eq!(out.0, data);
        assert_eq!(out.1, data[500_000..500_010]);
        assert_eq!(out.2, data[999_995..]);
        assert!(out.3 <= 64 * 1024);
        assert!(out.4 >= 1_000_000);
    }

    #[test]
    fn the_end_record_gives_the_entry_count_and_index_size() {
        use std::io::Write;
        let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for i in 0..7 {
            z.start_file(
                format!("f{i}.txt"),
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            z.write_all(b"x").unwrap();
        }
        let bytes = z.finish().unwrap().into_inner();
        let len = bytes.len() as u64;
        let e = peek_end_record(&mut Cursor::new(bytes), len)
            .unwrap()
            .unwrap();
        assert_eq!(e.entries, 7);
        assert!(e.index_bytes >= 7 * 46);
        assert_eq!(
            peek_end_record(&mut Cursor::new(b"not a zip at all".to_vec()), 16).unwrap(),
            None
        );
    }
}
