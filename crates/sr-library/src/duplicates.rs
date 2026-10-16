//! Duplicate detection for Library submissions (REV-ED-006): title and year matches through the
//! series search index, and perceptual (difference hash) matches between covers.
//!
//! A cover is reduced to a 64-bit hash of its brightness gradients (dHash). Two covers of the same
//! artwork, even re-encoded or resized, differ in only a few bits; different artwork differs in
//! about half of them. The hashes of published series live in `library_cover_hashes` and are filled
//! lazily (a bounded batch per check), so the first checks after deployment backfill the table.

use std::sync::Arc;

use async_trait::async_trait;
use sr_review::model::DuplicateMatch;
use sr_review::similar::title_score;

use crate::error::{LibraryError, LibraryResult};
use crate::model::{Scope, SeriesRecord};
use crate::service::LibraryService;

/// Hamming distance up to which two covers count as the same artwork.
pub const COVER_MAX_DISTANCE: u32 = 8;
/// Covers hashed per duplicate check while the table is being backfilled.
pub const BACKFILL_BATCH: u32 = 25;
/// Most title candidates read from the search index.
const TITLE_CANDIDATES: usize = 20;

/// Storage of the cover hashes. `sr-db` implements it over Postgres.
#[async_trait]
pub trait CoverHashes: Send + Sync {
    /// Remember the hash of `series`' cover `cover_key`.
    async fn put(&self, series: i64, cover_key: &str, hash: u64) -> LibraryResult<()>;
    /// Published series with a cover but no (current) hash, as `(series id, cover key)`.
    async fn unhashed(&self, limit: u32) -> LibraryResult<Vec<(i64, String)>>;
    /// Published series (never `exclude`) whose cover hash is within `max_distance` bits of
    /// `hash`, as `(series id, distance)`, nearest first.
    async fn nearest(
        &self,
        hash: u64,
        max_distance: u32,
        exclude: i64,
        limit: u32,
    ) -> LibraryResult<Vec<(i64, u32)>>;
}

/// The difference hash of an image: the picture is shrunk to 9 by 8 grey pixels and each bit says
/// whether a pixel is brighter than its right neighbour. `None` when the bytes are not an image.
pub fn dhash(bytes: &[u8]) -> Option<u64> {
    use image::imageops::FilterType;
    let img = image::load_from_memory(bytes).ok()?;
    let small = image::imageops::resize(&img.to_luma8(), 9, 8, FilterType::Triangle);
    let mut hash: u64 = 0;
    for y in 0..8 {
        for x in 0..8 {
            hash <<= 1;
            if small.get_pixel(x, y).0[0] > small.get_pixel(x + 1, y).0[0] {
                hash |= 1;
            }
        }
    }
    Some(hash)
}

/// Number of differing bits.
pub fn distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

fn year_of(r: &SeriesRecord) -> Option<i32> {
    r.year.map(i32::from)
}

impl LibraryService {
    /// Attach the cover hash store used by duplicate detection.
    pub fn with_cover_hashes(mut self, hashes: Arc<dyn CoverHashes>) -> Self {
        self.cover_hashes = Some(hashes);
        self
    }

    /// The published series whose title (or an alternative title) equals `title` once case,
    /// punctuation and bracketed tags are ignored. The most followed one wins a tie.
    pub async fn find_series_exact(&self, title: &str) -> LibraryResult<Option<i64>> {
        let Some(index) = &self.index else {
            return Ok(None);
        };
        let want = sr_review::similar::normalize_title(title);
        if want.is_empty() {
            return Ok(None);
        }
        let ids: Vec<i64> = index
            .search(title, true, TITLE_CANDIDATES)?
            .into_iter()
            .map(|h| h.id)
            .collect();
        let rows = self.repo.series_by_ids(&ids, Scope::Staff).await?;
        Ok(rows
            .iter()
            .filter(|r| r.published && r.merged_into.is_none())
            .filter(|r| {
                std::iter::once(&r.title)
                    .chain(r.alt_titles.iter())
                    .any(|t| sr_review::similar::normalize_title(t) == want)
            })
            .max_by_key(|r| r.follower_count)
            .map(|r| r.id))
    }

    async fn cover_hash(&self, key: &str) -> Option<u64> {
        let w = self.ingest.as_ref()?;
        let bytes = w.storage.get(key).await.ok()?;
        // Decoding is CPU work; keep it off the async threads.
        tokio::task::spawn_blocking(move || dhash(&bytes))
            .await
            .ok()
            .flatten()
    }

    /// Hash a bounded batch of covers that have no hash yet.
    async fn backfill_cover_hashes(&self, hashes: &dyn CoverHashes) {
        let Ok(todo) = hashes.unhashed(BACKFILL_BATCH).await else {
            return;
        };
        for (series, key) in todo {
            // A cover that cannot be read gets no hash (it would match everything).
            if let Some(h) = self.cover_hash(&key).await {
                let _ = hashes.put(series, &key, h).await;
            }
        }
    }

    /// Published series that look like the draft series `series`: same or similar title (also
    /// among alternative titles), and covers that are the same artwork.
    pub async fn find_duplicates(&self, series: i64) -> LibraryResult<Vec<DuplicateMatch>> {
        let mine = self
            .repo
            .get_series(series, Scope::Staff)
            .await?
            .ok_or(LibraryError::NotFound("series"))?;
        let mut found: std::collections::BTreeMap<i64, DuplicateMatch> =
            std::collections::BTreeMap::new();
        let mut note = |rec: &SeriesRecord, score: u8, reason: &str| {
            let e = found.entry(rec.id).or_insert_with(|| DuplicateMatch {
                section: "library".into(),
                ref_id: rec.id,
                title: rec.title.clone(),
                year: year_of(rec),
                score: 0,
                reasons: Vec::new(),
                href: Some(format!("/library/series/{}", rec.id)),
            });
            e.score = e.score.max(score);
            if !e.reasons.iter().any(|r| r == reason) {
                e.reasons.push(reason.to_owned());
            }
        };

        // Titles: the main title and the alternative ones, against the main and alternative
        // titles of the candidates.
        if let Some(index) = &self.index {
            let mut mine_titles = vec![mine.title.clone()];
            mine_titles.extend(mine.alt_titles.iter().cloned());
            let mut ids: Vec<i64> = Vec::new();
            for t in &mine_titles {
                for h in index.search(t, true, TITLE_CANDIDATES)? {
                    if h.id != series && !ids.contains(&h.id) {
                        ids.push(h.id);
                    }
                }
            }
            let candidates = self.repo.series_by_ids(&ids, Scope::Staff).await?;
            for c in candidates
                .iter()
                .filter(|c| c.id != series && c.published && c.merged_into.is_none())
            {
                let mut best: (u8, &str) = (0, "");
                for (i, mt) in mine_titles.iter().enumerate() {
                    let theirs = std::iter::once(&c.title).chain(c.alt_titles.iter());
                    for (j, ct) in theirs.enumerate() {
                        let s = title_score(mt, ct);
                        if s > best.0 {
                            best = (
                                s,
                                if i == 0 && j == 0 {
                                    "title"
                                } else {
                                    "alt_title"
                                },
                            );
                        }
                    }
                }
                if best.0 == 0 {
                    continue;
                }
                let mut score = best.0;
                note(c, score, best.1);
                if mine.year.is_some() && mine.year == c.year {
                    score = score.saturating_add(5).min(100);
                    note(c, score, "year");
                }
            }
        }

        // Covers: the same artwork under any title.
        if let (Some(hashes), Some(key)) = (&self.cover_hashes, &mine.cover_key) {
            self.backfill_cover_hashes(hashes.as_ref()).await;
            if let Some(h) = self.cover_hash(key).await {
                let near = hashes
                    .nearest(h, COVER_MAX_DISTANCE, series, TITLE_CANDIDATES as u32)
                    .await?;
                let ids: Vec<i64> = near.iter().map(|(id, _)| *id).collect();
                let rows = self.repo.series_by_ids(&ids, Scope::Staff).await?;
                for (id, d) in near {
                    let Some(rec) = rows.iter().find(|r| r.id == id) else {
                        continue;
                    };
                    let score = u8::try_from(100 - d * 5).unwrap_or(60);
                    note(rec, score, "cover");
                }
            }
        }
        Ok(found.into_values().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(f: impl Fn(u32, u32) -> u8, w: u32, h: u32) -> Vec<u8> {
        let img = image::GrayImage::from_fn(w, h, |x, y| image::Luma([f(x, y)]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn the_same_artwork_at_another_size_is_close_and_other_artwork_is_far() {
        // A diagonal ramp with a bright block, at two sizes.
        let art = |w: u32, h: u32| {
            png(
                move |x, y| {
                    let (u, v) = (x * 255 / w, y * 255 / h);
                    if (60..140).contains(&u) && (80..200).contains(&v) {
                        250
                    } else {
                        ((u + v) / 2) as u8
                    }
                },
                w,
                h,
            )
        };
        let big = dhash(&art(600, 900)).unwrap();
        let small = dhash(&art(200, 300)).unwrap();
        assert!(distance(big, small) <= 4, "{}", distance(big, small));
        // Different composition: vertical stripes.
        let other = dhash(&png(
            |x, _| if (x / 10) % 2 == 0 { 20 } else { 230 },
            600,
            900,
        ))
        .unwrap();
        assert!(
            distance(big, other) > COVER_MAX_DISTANCE,
            "{}",
            distance(big, other)
        );
    }

    #[test]
    fn not_an_image_has_no_hash() {
        assert!(dhash(b"definitely not an image").is_none());
    }
}
