//! `sr-cli seed`: library samples (FND-DB-009). Every page is drawn in code (gradients, panels, a block
//! digit), so the media is original to this repository and released as CC0; nothing is downloaded. The
//! pages go through the real ingest pipeline (`sr_library::ingest::image::process_page`: WebP, sized
//! variants, blurhash) and are stored in the configured bucket, then the series are published.

use std::io::Cursor;
use std::sync::Arc;

use anyhow::{Context, Result};
use image::{ImageFormat, Rgb, RgbImage};
use sr_core::{PlatformPermissions, SystemClock};
use sr_db::{PgPool, auth_repo::IdSource};
use sr_library::ingest::image::{PageLimits, page_key, process_page, variant_key};
use sr_library::model::{
    ChapterFormat, ChapterInput, EditionInput, PageInput, SeriesInput, SeriesKind, SeriesStatus,
};
use sr_library::{Actor, LibraryService};
use sr_storage::ObjectStorage;

/// What one sample series looks like.
pub struct Sample {
    pub kind: SeriesKind,
    pub title: &'static str,
    pub synopsis: &'static str,
    /// Hue (degrees) the pages of the series are drawn around.
    pub hue: f32,
    pub chapters: u32,
    pub pages: u32,
    pub size: (u32, u32),
}

pub const SAMPLES: &[Sample] = &[
    Sample {
        kind: SeriesKind::Manga,
        title: "Sample Manga (CC0)",
        synopsis: "Generated placeholder pages for development. Released under CC0.",
        hue: 12.0,
        chapters: 2,
        pages: 4,
        size: (800, 1200),
    },
    Sample {
        kind: SeriesKind::Comic,
        title: "Sample Comic (CC0)",
        synopsis: "Generated placeholder pages for development. Released under CC0.",
        hue: 205.0,
        chapters: 1,
        pages: 3,
        size: (900, 1350),
    },
    Sample {
        kind: SeriesKind::Webtoon,
        title: "Sample Webtoon (CC0)",
        synopsis: "Generated tall strips for the vertical reader. Released under CC0.",
        hue: 130.0,
        chapters: 1,
        pages: 3,
        size: (800, 2400),
    },
];

fn hsv(h: f32, s: f32, v: f32) -> Rgb<u8> {
    let c = v * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    Rgb([
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    ])
}

/// Segments (top, top-left, top-right, middle, bottom-left, bottom-right, bottom) of the digits 0..=9.
const SEVEN: [u8; 10] = [
    0b1110111, 0b0010010, 0b1011101, 0b1011011, 0b0111010, 0b1101011, 0b1101111, 0b1010010,
    0b1111111, 0b1111011,
];

fn fill(img: &mut RgbImage, x0: u32, y0: u32, w: u32, h: u32, c: Rgb<u8>) {
    for y in y0..(y0 + h).min(img.height()) {
        for x in x0..(x0 + w).min(img.width()) {
            img.put_pixel(x, y, c);
        }
    }
}

fn digit(img: &mut RgbImage, d: u32, x: u32, y: u32, unit: u32, c: Rgb<u8>) {
    let mask = SEVEN[(d % 10) as usize];
    let seg = |i: u32| mask & (1 << (6 - i)) != 0;
    let (u, l) = (unit, unit * 5);
    if seg(0) {
        fill(img, x + u, y, l, u, c);
    }
    if seg(1) {
        fill(img, x, y + u, u, l, c);
    }
    if seg(2) {
        fill(img, x + u + l, y + u, u, l, c);
    }
    if seg(3) {
        fill(img, x + u, y + u + l, l, u, c);
    }
    if seg(4) {
        fill(img, x, y + 2 * u + l, u, l, c);
    }
    if seg(5) {
        fill(img, x + u + l, y + 2 * u + l, u, l, c);
    }
    if seg(6) {
        fill(img, x + u, y + 2 * u + 2 * l, l, u, c);
    }
}

/// One page as PNG bytes: a diagonal gradient, a panel grid and the page number in block digits.
/// Deterministic for the same arguments.
pub fn sample_page_png(hue: f32, chapter: u32, page: u32, (w, h): (u32, u32)) -> Result<Vec<u8>> {
    let tint = hue + chapter as f32 * 17.0 + page as f32 * 9.0;
    let mut img = RgbImage::from_fn(w, h, |x, y| {
        let t = (x as f32 / w as f32 + y as f32 / h as f32) / 2.0;
        hsv(tint + t * 40.0, 0.35, 0.95 - t * 0.35)
    });
    // Panels: a 2 column grid, the number of rows follows the page height.
    let margin = w / 20;
    let rows = (h / (w / 2)).clamp(2, 6);
    let (pw, ph) = ((w - 3 * margin) / 2, (h - (rows + 1) * margin) / rows);
    for r in 0..rows {
        for c in 0..2 {
            let shade = hsv(
                tint + 180.0,
                0.25,
                0.97 - 0.07 * ((r + c + page) % 4) as f32,
            );
            let (x, y) = (margin + c * (pw + margin), margin + r * (ph + margin));
            fill(&mut img, x, y, pw, ph, Rgb([20, 20, 28]));
            fill(&mut img, x + 6, y + 6, pw - 12, ph - 12, shade);
        }
    }
    // Page number, centred in the first panel.
    let unit = (pw / 24).max(4);
    let ink = Rgb([25, 25, 35]);
    let n = page + 1;
    let digits: Vec<u32> = if n >= 10 {
        vec![n / 10, n % 10]
    } else {
        vec![n]
    };
    let total = digits.len() as u32 * (7 * unit) + (digits.len() as u32 - 1) * unit;
    let x0 = margin + pw.saturating_sub(total) / 2;
    let y0 = margin + ph.saturating_sub(12 * unit) / 2;
    for (i, d) in digits.iter().enumerate() {
        digit(&mut img, *d, x0 + i as u32 * 8 * unit, y0, unit, ink);
    }
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .context("encoding a sample page")?;
    Ok(out)
}

/// Seed the library samples. Idempotent per title. Returns (series created, pages stored).
pub async fn seed_library(
    pool: &PgPool,
    storage: &dyn ObjectStorage,
    ids: IdSource,
    owner: i64,
    cdn_secret: Vec<u8>,
) -> Result<(usize, usize)> {
    let service = LibraryService::new(
        Arc::new(sr_db::library_repo::PgLibraryRepo::new(pool.clone())),
        sr_storage::cdn::CdnSigner::new(cdn_secret),
        Arc::new(SystemClock),
        ids,
    );
    let actor = Actor {
        user_id: owner,
        permissions: PlatformPermissions::PUBLISH_READING,
    };
    let (mut series_made, mut pages_made) = (0, 0);
    for s in SAMPLES {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM reading_series WHERE title = $1)")
                .bind(s.title)
                .fetch_one(pool)
                .await?;
        if exists {
            continue;
        }
        let series = service
            .create_series(
                actor,
                SeriesInput {
                    kind: s.kind,
                    title: s.title.into(),
                    alt_titles: vec![],
                    authors: vec!["Side Room".into()],
                    artists: vec!["Side Room".into()],
                    status: SeriesStatus::Completed,
                    year: Some(2025),
                    synopsis: s.synopsis.into(),
                    genres: vec!["Sample".into()],
                    tags: vec!["cc0".into()],
                    reading_direction: None,
                    age_restricted: false,
                    cover_key: None,
                },
            )
            .await
            .map_err(|e| anyhow::anyhow!("creating {}: {e}", s.title))?;
        let edition = service
            .create_edition(
                actor,
                series.id,
                EditionInput {
                    language: "en".into(),
                    label: "Generated".into(),
                    is_default: true,
                },
            )
            .await
            .map_err(|e| anyhow::anyhow!("creating the edition: {e}"))?;
        for c in 1..=s.chapters {
            let chapter = service
                .create_chapter(
                    actor,
                    edition.id,
                    ChapterInput {
                        number: f64::from(c),
                        title: Some(format!("Chapter {c}")),
                        volume_id: None,
                        format: ChapterFormat::Images,
                        source_key: None,
                        release_group: None,
                    },
                )
                .await
                .map_err(|e| anyhow::anyhow!("creating a chapter: {e}"))?;
            let mut inputs = Vec::new();
            for p in 0..s.pages {
                let png = sample_page_png(s.hue, c, p, s.size)?;
                let done = process_page(&png, &PageLimits::default())
                    .map_err(|e| anyhow::anyhow!("processing a sample page: {e:?}"))?;
                let key = page_key(series.id, chapter.id, None, p as usize);
                if c == 1 && p == 0 {
                    // The first page doubles as the cover.
                    let cover = format!("library/{}/cover.webp", series.id);
                    storage.put(&cover, done.webp.clone().into()).await?;
                    service
                        .set_series_cover(actor, series.id, &cover)
                        .await
                        .map_err(|e| anyhow::anyhow!("setting the cover: {e}"))?;
                }
                storage.put(&key, done.webp.into()).await?;
                let mut variants = std::collections::BTreeMap::new();
                for v in done.variants {
                    let vk = variant_key(series.id, chapter.id, None, p as usize, v.width);
                    storage.put(&vk, v.webp.into()).await?;
                    variants.insert(v.width.to_string(), vk);
                }
                inputs.push(PageInput {
                    key,
                    variants,
                    width: done.width,
                    height: done.height,
                    is_double_spread: done.is_double_spread,
                    blurhash: Some(done.blurhash),
                    crop: done.crop,
                });
                pages_made += 1;
            }
            service
                .replace_pages(actor, chapter.id, inputs)
                .await
                .map_err(|e| anyhow::anyhow!("storing pages: {e}"))?;
            service
                .set_chapter_published(actor, chapter.id, true)
                .await
                .map_err(|e| anyhow::anyhow!("publishing a chapter: {e}"))?;
        }
        service
            .set_series_published(actor, series.id, true)
            .await
            .map_err(|e| anyhow::anyhow!("publishing {}: {e}", s.title))?;
        series_made += 1;
    }
    Ok((series_made, pages_made))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_are_deterministic_valid_and_pass_the_ingest_pipeline() {
        let a = sample_page_png(12.0, 1, 0, (400, 600)).unwrap();
        assert_eq!(a, sample_page_png(12.0, 1, 0, (400, 600)).unwrap());
        assert_ne!(a, sample_page_png(12.0, 1, 1, (400, 600)).unwrap());
        let done = process_page(&a, &PageLimits::default()).unwrap();
        assert_eq!((done.width, done.height), (400, 600));
        assert!(!done.blurhash.is_empty());
        assert!(!done.webp.is_empty());
    }

    #[test]
    fn every_sample_renders_at_its_size_including_two_digit_pages() {
        for s in SAMPLES {
            let png = sample_page_png(s.hue, 1, 11, s.size).unwrap();
            let done = process_page(&png, &PageLimits::default()).unwrap();
            assert_eq!((done.width, done.height), s.size);
            assert!(!done.variants.is_empty(), "{} has sized variants", s.title);
        }
    }

    #[test]
    fn sample_titles_are_unique() {
        let mut t: Vec<_> = SAMPLES.iter().map(|s| s.title).collect();
        t.sort_unstable();
        t.dedup();
        assert_eq!(t.len(), SAMPLES.len());
    }
}

#[cfg(test)]
mod pg_tests {
    use super::*;
    use sr_storage::ObjectStoreStorage;

    /// FND-DB-009: the samples land published with pages, variants, a cover and objects in the bucket,
    /// and a second run changes nothing.
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn samples_are_seeded_published_and_idempotent() {
        let db = sr_db::testing::TestDb::new().await.expect("test db");
        let pool = db.pool().clone();
        let gen_ = Arc::new(
            sr_core::SnowflakeGenerator::new(SystemClock, 1, 7).expect("snowflake generator"),
        );
        let ids: IdSource = Arc::new(move || gen_.generate().get() as i64);
        let owner = ids();
        let person = ids();
        sqlx::query("INSERT INTO persons (id) VALUES ($1)")
            .bind(person)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $2, 'seedowner', 'seedowner@example.test', 'x')",
        )
        .bind(owner)
        .bind(person)
        .execute(&pool)
        .await
        .unwrap();
        let storage = ObjectStoreStorage::in_memory();
        let (series, pages) = seed_library(&pool, &storage, ids.clone(), owner, vec![7; 32])
            .await
            .unwrap();
        assert_eq!(series, SAMPLES.len());
        let expected: u32 = SAMPLES.iter().map(|s| s.chapters * s.pages).sum();
        assert_eq!(pages, expected as usize);
        let published: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM reading_series WHERE published_at IS NOT NULL AND cover_key IS NOT NULL AND title LIKE '%(CC0)'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(published as usize, SAMPLES.len());
        let stored: i64 = sqlx::query_scalar("SELECT count(*) FROM reading_pages")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(stored, i64::from(expected));
        // A stored page is a real WebP in the bucket.
        let key: String = sqlx::query_scalar("SELECT key FROM reading_pages ORDER BY key LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        let bytes = storage.get(&key).await.unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        // Second run: nothing new.
        let again = seed_library(&pool, &storage, ids, owner, vec![7; 32])
            .await
            .unwrap();
        assert_eq!(again, (0, 0));
    }
}
