//! Single-page replacement (H-43, P13-LIB-7): swap one page of an image chapter for a new image without
//! re-uploading the archive. The new image goes through the same converter as an archive page (decode
//! limits, dimensions, WebP original, sized variants, blurhash, crop data). The page keeps its position and
//! its double-spread flag; the new objects live under a fresh revision folder so readers still loading the
//! old page are not disturbed, and the old objects are queued for collection. Because the export cache
//! and the CBZ fingerprint derive from the page keys, replacing a page changes both on its own.

use std::collections::BTreeMap;

use crate::error::{LibraryError, LibraryResult};
use crate::ingest::image::{PageLimits, page_key, process_page, variant_key};
use crate::model::*;
use crate::rules;
use crate::service::{Actor, LibraryService};
use crate::uploads::wiring;

/// The most bytes one replacement image may have (the archive page limit).
pub const MAX_REPLACEMENT_BYTES: u64 = 16 * 1024 * 1024;

fn input_of(p: &PageRecord) -> PageInput {
    PageInput {
        key: p.key.clone(),
        variants: p
            .variants
            .iter()
            .map(|(w, k)| (w.to_string(), k.clone()))
            .collect::<BTreeMap<_, _>>(),
        width: u32::try_from(p.width).unwrap_or(0),
        height: u32::try_from(p.height).unwrap_or(0),
        is_double_spread: p.is_double_spread,
        blurhash: p.blurhash.clone(),
        crop: p.crop,
    }
}

impl LibraryService {
    /// Replace page `index` (from 0) of an image chapter with `image` (PNG, JPEG, WebP, GIF or AVIF).
    /// Needs `PUBLISH_READING`. Returns the chapter with its page count unchanged.
    pub async fn replace_page(
        &self,
        a: Actor,
        chapter: i64,
        index: i32,
        image: Vec<u8>,
    ) -> LibraryResult<Chapter> {
        a.publish()?;
        let c = self
            .repo
            .get_chapter(chapter, None)
            .await?
            .ok_or(LibraryError::NotFound("chapter"))?;
        if c.format != ChapterFormat::Images {
            return Err(LibraryError::Conflict(
                "only image chapters have pages".into(),
            ));
        }
        let w = wiring(self)?;
        let pages = self.repo.list_pages(chapter).await?;
        let at = usize::try_from(index)
            .ok()
            .filter(|i| *i < pages.len())
            .ok_or(LibraryError::NotFound("page"))?;
        if image.is_empty() || image.len() as u64 > MAX_REPLACEMENT_BYTES {
            return Err(LibraryError::Invalid(format!(
                "the image must be between 1 byte and {} MB",
                MAX_REPLACEMENT_BYTES / (1024 * 1024)
            )));
        }
        let limits = PageLimits {
            max_bytes: MAX_REPLACEMENT_BYTES,
            ..PageLimits::default()
        };
        // Decoding and encoding are CPU work: off the request thread.
        let processed = tokio::task::spawn_blocking(move || process_page(&image, &limits))
            .await
            .map_err(|e| LibraryError::Storage(format!("image worker failed: {e}")))?
            .map_err(|e| LibraryError::Invalid(format!("this is not a usable image: {e}")))?;

        let revision = self.clock.now_unix_ms();
        let key = page_key(c.series_id, chapter, Some(revision), at);
        let mut stored = vec![key.clone()];
        let mut variants = BTreeMap::new();
        let put = |k: String, data: Vec<u8>| async move {
            w.storage
                .put(&k, data.into())
                .await
                .map_err(|e| LibraryError::Storage(e.to_string()))
        };
        let result: LibraryResult<()> = async {
            put(key.clone(), processed.webp.clone()).await?;
            for v in &processed.variants {
                let vk = variant_key(c.series_id, chapter, Some(revision), at, v.width);
                put(vk.clone(), v.webp.clone()).await?;
                stored.push(vk.clone());
                variants.insert(v.width.to_string(), vk);
            }
            Ok(())
        }
        .await;
        if let Err(e) = result {
            for k in &stored {
                let _ = w.storage.delete(k).await;
            }
            return Err(e);
        }

        let old = &pages[at];
        let old_keys: Vec<String> = std::iter::once(old.key.clone())
            .chain(old.variants.iter().map(|(_, k)| k.clone()))
            .collect();
        let mut inputs: Vec<PageInput> = pages.iter().map(input_of).collect();
        inputs[at] = PageInput {
            key,
            variants,
            width: processed.width,
            height: processed.height,
            // The page keeps its spread flag: the publisher set it (or the archive named it).
            is_double_spread: old.is_double_spread,
            blurhash: Some(processed.blurhash.clone()),
            crop: processed.crop,
        };
        let rows = rules::pages(c.series_id, chapter, &inputs)?;
        match self.repo.replace_pages(chapter, &rows).await? {
            Some(rec) => {
                self.repo.queue_gc(&old_keys, Some(chapter)).await?;
                Ok(Self::chapter(&rec))
            }
            None => {
                self.repo.queue_gc(&stored, None).await?;
                Err(LibraryError::NotFound("chapter"))
            }
        }
    }
}
