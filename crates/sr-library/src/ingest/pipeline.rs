//! Archive in, stored pages out: reads a CBZ/ZIP, converts every page, uploads the
//! originals and variants to object storage, and returns the page list that
//! `LibraryService::replace_pages` accepts.
//!
//! Decoding and encoding run on a blocking thread and stream pages to the uploader
//! through a small bounded channel, so a long chapter never sits in memory at once. Any
//! failure deletes what this run uploaded and reports the offending page by name.

use std::collections::{BTreeMap, HashSet};
use std::io::{Read, Seek};
use std::sync::Arc;

use bytes::Bytes;
use sr_storage::ObjectStorage;
use tokio::sync::mpsc;

use super::IngestError;
use super::archive::{ArchiveLimits, ArchiveReader};
use super::comicinfo::{self, ComicInfo};
use super::image::{
    PageLimits, ProcessedPage, is_avif, name_marks_spread, page_key, process_page, variant_key,
};
use super::series_json::{self, SeriesFile};
use crate::model::PageInput;

#[derive(Debug, Clone, Copy, Default)]
pub struct IngestLimits {
    pub archive: ArchiveLimits,
    pub page: PageLimits,
}

/// What an archive yielded.
#[derive(Debug, Clone, PartialEq)]
pub struct IngestedChapter {
    pub pages: Vec<PageInput>,
    pub comic_info: Option<ComicInfo>,
    pub series_file: Option<SeriesFile>,
    /// Ignored files (junk, unsupported) and unreadable metadata files.
    pub warnings: Vec<String>,
}

struct Produced {
    comic_info: Option<ComicInfo>,
    series_file: Option<SeriesFile>,
    warnings: Vec<String>,
}

type PageMsg = (usize, String, ProcessedPage);

fn produce<R: Read + Seek>(
    reader: R,
    limits: IngestLimits,
    tx: &mpsc::Sender<PageMsg>,
    double_pages: &mut Option<HashSet<u32>>,
) -> Result<Produced, IngestError> {
    let mut ar = ArchiveReader::open(reader, limits.archive)?;
    if ar.pages().is_empty() {
        return Err(IngestError::NoPages);
    }
    let mut warnings: Vec<String> = ar
        .skipped()
        .iter()
        .map(|s| format!("skipped {s}"))
        .collect();
    // Metadata is advisory: a broken file is reported, not fatal.
    let comic_info = match ar.comic_info()? {
        Some(b) => match comicinfo::parse(&b) {
            Ok(c) => Some(c),
            Err(e) => {
                warnings.push(e.to_string());
                None
            }
        },
        None => None,
    };
    let series_file = match ar.series_json()? {
        Some(b) => match series_json::parse(&b) {
            Ok(s) => Some(s),
            Err(e) => {
                warnings.push(e.to_string());
                None
            }
        },
        None => None,
    };
    *double_pages = comic_info
        .as_ref()
        .map(|c| c.double_page_indexes().into_iter().collect());
    let pages = ar.pages().to_vec();
    for (i, entry) in pages.iter().enumerate() {
        let bytes = ar.read(entry)?;
        // An AVIF hiding behind another extension is refused like an `.avif` page.
        if is_avif(&bytes) {
            return Err(IngestError::UnsupportedFormat {
                what: "AVIF pages",
                files: vec![entry.name.clone()],
            });
        }
        let processed =
            process_page(&bytes, &limits.page).map_err(|source| IngestError::Image {
                page: entry.name.clone(),
                source,
            })?;
        drop(bytes);
        // The receiver is gone when the uploader failed: stop quietly, it reports the error.
        if tx
            .blocking_send((i, entry.name.clone(), processed))
            .is_err()
        {
            break;
        }
    }
    Ok(Produced {
        comic_info,
        series_file,
        warnings,
    })
}

async fn put(storage: &dyn ObjectStorage, key: &str, data: Vec<u8>) -> Result<(), IngestError> {
    storage
        .put(key, Bytes::from(data))
        .await
        .map_err(|e| IngestError::Storage(e.to_string()))
}

/// Convert and store every page of a ZIP/CBZ for chapter `chapter` of `series`.
pub async fn ingest_archive<R>(
    reader: R,
    storage: Arc<dyn ObjectStorage>,
    series: i64,
    chapter: i64,
    limits: IngestLimits,
) -> Result<IngestedChapter, IngestError>
where
    R: Read + Seek + Send + 'static,
{
    let (tx, mut rx) = mpsc::channel::<PageMsg>(2);
    // The double-page hints come from ComicInfo, which the producer reads first; it hands
    // them over through the join value, so the uploader works with what it knows per page.
    let producer = tokio::task::spawn_blocking(move || {
        let mut hints = None;
        let r = produce(reader, limits, &tx, &mut hints);
        (r, hints)
    });

    let mut stored: Vec<String> = Vec::new();
    let mut pages: Vec<PageInput> = Vec::new();
    let mut failure: Option<IngestError> = None;
    while let Some((i, name, p)) = rx.recv().await {
        let res: Result<PageInput, IngestError> = async {
            let key = page_key(series, chapter, i);
            put(storage.as_ref(), &key, p.webp).await?;
            stored.push(key.clone());
            let mut variants = BTreeMap::new();
            for v in p.variants {
                let vk = variant_key(series, chapter, i, v.width);
                put(storage.as_ref(), &vk, v.webp).await?;
                stored.push(vk.clone());
                variants.insert(v.width.to_string(), vk);
            }
            Ok(PageInput {
                key,
                variants,
                width: p.width,
                height: p.height,
                is_double_spread: p.is_double_spread || name_marks_spread(&name),
                blurhash: Some(p.blurhash),
            })
        }
        .await;
        match res {
            Ok(page) => pages.push(page),
            Err(e) => {
                failure = Some(e);
                break;
            }
        }
    }
    // Dropping the receiver unblocks a producer that is waiting to send.
    drop(rx);
    let (produced, hints) = producer
        .await
        .map_err(|e| IngestError::Archive(format!("ingest worker failed: {e}")))?;

    let outcome = match (failure, produced) {
        (Some(e), _) | (None, Err(e)) => Err(e),
        (None, Ok(p)) => {
            if let Some(h) = hints {
                for (i, page) in pages.iter_mut().enumerate() {
                    if u32::try_from(i).is_ok_and(|i| h.contains(&i)) {
                        page.is_double_spread = true;
                    }
                }
            }
            Ok(p)
        }
    };
    match outcome {
        Ok(p) => Ok(IngestedChapter {
            pages,
            comic_info: p.comic_info,
            series_file: p.series_file,
            warnings: p.warnings,
        }),
        Err(e) => {
            for key in stored {
                let _ = storage.delete(&key).await;
            }
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::image::{DynamicImage, ImageFormat, Rgb, RgbImage};
    use sr_storage::ObjectStoreStorage;
    use std::io::{Cursor, Write};
    use zip::{ZipWriter, write::SimpleFileOptions};

    fn png(w: u32, h: u32, seed: u8) -> Vec<u8> {
        let img = RgbImage::from_fn(w, h, |x, y| Rgb([seed, (x % 256) as u8, (y % 256) as u8]));
        let mut out = Vec::new();
        DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .unwrap();
        out
    }

    fn cbz(entries: &[(&str, Vec<u8>)]) -> Cursor<Vec<u8>> {
        let mut w = ZipWriter::new(Cursor::new(Vec::new()));
        for (n, d) in entries {
            w.start_file(*n, SimpleFileOptions::default()).unwrap();
            w.write_all(d).unwrap();
        }
        Cursor::new(w.finish().unwrap().into_inner())
    }

    fn mem() -> Arc<ObjectStoreStorage> {
        Arc::new(ObjectStoreStorage::in_memory())
    }

    const INFO: &str = r#"<ComicInfo><Title>Ch 1</Title><Number>1</Number><Manga>YesAndRightToLeft</Manga>
        <Pages><Page Image="1" DoublePage="true"/></Pages></ComicInfo>"#;

    #[tokio::test]
    async fn a_cbz_becomes_stored_pages_in_order() {
        let storage = mem();
        let z = cbz(&[
            ("ch/10.png", png(900, 1300, 1)),
            ("ch/2.png", png(900, 1300, 2)),
            ("ch/1.png", png(300, 200, 3)),
            ("__MACOSX/._1.png", b"junk".to_vec()),
            ("ComicInfo.xml", INFO.as_bytes().to_vec()),
            ("Thumbs.db", b"x".to_vec()),
            ("series.json", br#"{"title":"Example"}"#.to_vec()),
        ]);
        let out = ingest_archive(z, storage.clone(), 7, 9, IngestLimits::default())
            .await
            .unwrap();
        assert_eq!(out.pages.len(), 3);
        // Natural order: 1, 2, 10.
        assert_eq!((out.pages[0].width, out.pages[1].width), (300, 900));
        assert_eq!(out.pages[0].key, "library/7/9/0000.webp");
        assert_eq!(out.pages[2].key, "library/7/9/0002.webp");
        // Variants exist where the source is wider than 720.
        assert!(out.pages[0].variants.is_empty());
        assert_eq!(out.pages[1].variants.len(), 1);
        assert_eq!(out.pages[1].variants["720"], "library/7/9/0001_720.webp");
        // Shape-detected spread (300x200) and ComicInfo-marked spread (index 1).
        assert!(out.pages[0].is_double_spread);
        assert!(out.pages[1].is_double_spread);
        assert!(!out.pages[2].is_double_spread);
        assert!(out.pages.iter().all(|p| p.blurhash.is_some()));
        assert_eq!(
            out.comic_info.as_ref().unwrap().title.as_deref(),
            Some("Ch 1")
        );
        assert_eq!(out.series_file.as_ref().unwrap().series.title, "Example");
        assert!(out.warnings.iter().any(|w| w.contains("Thumbs.db")));
        // Everything referenced is in storage, and the manifest passes the service's rules.
        for p in &out.pages {
            assert!(storage.head(&p.key).await.is_ok());
            for k in p.variants.values() {
                assert!(storage.head(k).await.is_ok());
            }
        }
        let rows = crate::rules::pages(7, 9, &out.pages).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(storage.list("library/7/9/").await.unwrap().len(), 3 + 1 + 1);
    }

    #[tokio::test]
    async fn a_bad_page_fails_the_chapter_and_cleans_up() {
        let storage = mem();
        let z = cbz(&[
            ("1.png", png(100, 100, 1)),
            ("2.png", png(100, 100, 2)),
            ("3.png", b"this is not a png".to_vec()),
        ]);
        let e = ingest_archive(z, storage.clone(), 1, 2, IngestLimits::default())
            .await
            .unwrap_err();
        assert!(
            matches!(e, IngestError::Image { ref page, .. } if page == "3.png"),
            "{e}"
        );
        assert!(storage.list("library/").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn empty_and_hostile_archives_are_refused() {
        let storage = mem();
        let e = ingest_archive(
            cbz(&[("readme.txt", b"hi".to_vec())]),
            storage.clone(),
            1,
            2,
            IngestLimits::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(e, IngestError::NoPages));
        let e = ingest_archive(
            cbz(&[("../x.png", png(10, 10, 1))]),
            storage.clone(),
            1,
            2,
            IngestLimits::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(e, IngestError::UnsafePath(_)));
        let e = ingest_archive(
            Cursor::new(b"PK nope".to_vec()),
            storage.clone(),
            1,
            2,
            IngestLimits::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(e, IngestError::Archive(_)));
        assert!(storage.list("library/").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn broken_metadata_is_a_warning_not_a_failure() {
        let z = cbz(&[
            ("1.png", png(50, 50, 1)),
            (
                "ComicInfo.xml",
                b"<ComicInfo><Title>x</Titel></ComicInfo>".to_vec(),
            ),
            ("series.json", b"{nope".to_vec()),
        ]);
        let out = ingest_archive(z, mem(), 1, 2, IngestLimits::default())
            .await
            .unwrap();
        assert_eq!(out.pages.len(), 1);
        assert!(out.comic_info.is_none() && out.series_file.is_none());
        assert_eq!(
            out.warnings
                .iter()
                .filter(|w| w.contains("ComicInfo") || w.contains("series.json"))
                .count(),
            2
        );
    }

    #[tokio::test]
    async fn page_budgets_apply_per_page() {
        let l = IngestLimits {
            page: PageLimits {
                max_bytes: 10 * 1024 * 1024,
                max_pixels: 10_000,
            },
            ..IngestLimits::default()
        };
        let e = ingest_archive(cbz(&[("1.png", png(200, 200, 1))]), mem(), 1, 2, l)
            .await
            .unwrap_err();
        assert!(matches!(e, IngestError::Image { .. }));
    }

    fn seven(entries: &[(&str, Vec<u8>)]) -> Cursor<Vec<u8>> {
        use sevenz_rust2::{ArchiveEntry, ArchiveWriter, SourceReader};
        let mut w = ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
        let es = entries
            .iter()
            .map(|(n, _)| ArchiveEntry::new_file(n))
            .collect();
        let rs = entries
            .iter()
            .map(|(_, d)| SourceReader::new(Cursor::new(d.clone())))
            .collect();
        w.push_archive_entries(es, rs).unwrap();
        Cursor::new(w.finish().unwrap().into_inner())
    }

    #[tokio::test]
    async fn a_cb7_becomes_stored_pages_too() {
        let storage = mem();
        let z = seven(&[
            ("10.png", png(900, 300, 1)),
            ("2.png", png(300, 400, 2)),
            ("ComicInfo.xml", INFO.as_bytes().to_vec()),
        ]);
        let out = ingest_archive(z, storage.clone(), 3, 4, IngestLimits::default())
            .await
            .unwrap();
        assert_eq!(out.pages.len(), 2);
        assert_eq!((out.pages[0].width, out.pages[1].width), (300, 900));
        assert_eq!(out.comic_info.unwrap().title.as_deref(), Some("Ch 1"));
        assert_eq!(storage.list("library/3/4/").await.unwrap().len(), 2 + 1);
    }

    #[tokio::test]
    async fn avif_pages_reject_the_archive_and_store_nothing() {
        let storage = mem();
        let avif = b"\0\0\0\x1cftypavif\0\0\0\0avifmif1".to_vec();
        // By extension: nothing is even decoded.
        let e = ingest_archive(
            cbz(&[("1.png", png(50, 50, 1)), ("2.avif", avif.clone())]),
            storage.clone(),
            1,
            2,
            IngestLimits::default(),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(e, IngestError::UnsupportedFormat { ref files, .. } if files == &["2.avif"]),
            "{e}"
        );
        // By content, under a misleading name; pages already uploaded are removed.
        let e = ingest_archive(
            cbz(&[("1.png", png(50, 50, 1)), ("2.jpg", avif)]),
            storage.clone(),
            1,
            2,
            IngestLimits::default(),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(e, IngestError::UnsupportedFormat { ref files, .. } if files == &["2.jpg"]),
            "{e}"
        );
        assert!(storage.list("library/").await.unwrap().is_empty());
    }
}
