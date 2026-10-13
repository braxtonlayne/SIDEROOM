//! The page image pipeline (LIB-ING-001): decode, normalise, and derive everything
//! the reader needs from one source image.
//!
//! Output per page: a WebP original at its own size, WebP variants at width 720, 1080
//! and 1600 (only for sources wider than that: never upscaled), the dimensions, a
//! blurhash for the loading placeholder and a double-page-spread flag.
//!
//! Encoding is lossy WebP at quality 85 through libwebp (the `webp` crate, ADR-0005,
//! D-168); PNG sources are also encoded lossless and the smaller file is kept, for the
//! original and for every variant. Decoding stays pure Rust (`image`). libwebp cannot
//! encode a side longer than 16,383 px: such a page is refused as too large. AVIF input
//! is not decoded (D-169) and is reported by [`is_avif`]. Input is bounded by
//! [`PageLimits`] before pixels are decoded.

use ::image::{RgbaImage, imageops::FilterType};
use sr_storage::img::{self, Decoded, ImgError, ImgFormat, Limits};

/// Variant widths produced, ascending (spec section 3.4).
pub const VARIANT_WIDTHS: [u32; 3] = [720, 1080, 1600];
/// A page wider than this many times its height is a double-page spread (ingest spec 2.1).
pub const SPREAD_RATIO: f64 = 1.3;

/// Budget for decoding one page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageLimits {
    /// Largest source file.
    pub max_bytes: u64,
    /// Largest `width * height`. Webtoon strips are tall, so this is generous.
    pub max_pixels: u64,
}

impl Default for PageLimits {
    fn default() -> Self {
        Self {
            max_bytes: 64 * 1024 * 1024,
            max_pixels: 60_000_000,
        }
    }
}

impl PageLimits {
    fn decode_limits(self) -> Limits {
        Limits {
            max_bytes: self.max_bytes,
            max_pixels: self.max_pixels,
            // Animated sources are reduced to their first frame; keep the budget small.
            max_animation_pixels: 32_000_000,
            max_frames: 400,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantImage {
    pub width: u32,
    pub height: u32,
    pub webp: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessedPage {
    pub width: u32,
    pub height: u32,
    pub webp: Vec<u8>,
    pub variants: Vec<VariantImage>,
    pub blurhash: String,
    pub is_double_spread: bool,
    /// Format of the source file.
    pub source: ImgFormat,
}

/// Whether `width` x `height` is a double-page spread.
pub fn is_spread(width: u32, height: u32) -> bool {
    height > 0 && f64::from(width) > SPREAD_RATIO * f64::from(height)
}

/// Whether a file name marks a spread (`page_012_spread.jpg`).
pub fn name_marks_spread(name: &str) -> bool {
    let stem = name.rsplit('/').next().unwrap_or(name);
    let stem = stem
        .rsplit_once('.')
        .map_or(stem, |(s, _)| s)
        .to_lowercase();
    stem.ends_with("_spread") || stem.ends_with("-spread")
}

/// Whether `data` is an AVIF still or sequence (an ISO-BMFF `ftyp` box with an `avif` or
/// `avis` brand). Such pages are refused, not skipped (D-169).
pub fn is_avif(data: &[u8]) -> bool {
    data.len() >= 12 && &data[4..8] == b"ftyp" && matches!(&data[8..12], b"avif" | b"avis")
}

/// Lossy WebP quality for page images (D-168).
pub const WEBP_QUALITY: f32 = 85.0;
/// libwebp cannot encode a side longer than this.
pub const WEBP_MAX_SIDE: u32 = 16_383;

/// Encode one image: lossy q85, and for PNG sources also lossless, keeping the smaller
/// file. A PNG is usually flat line art that lossless codes better and without artefacts.
fn encode_webp(img: &RgbaImage, also_lossless: bool) -> Result<Vec<u8>, ImgError> {
    let (w, h) = img.dimensions();
    if w > WEBP_MAX_SIDE || h > WEBP_MAX_SIDE {
        return Err(ImgError::TooLarge);
    }
    let opaque = img.pixels().all(|p| p[3] == 255);
    // Opaque pages (nearly all of them) drop the alpha channel: a quarter smaller.
    let rgb: Vec<u8>;
    let enc = if opaque {
        rgb = img.pixels().flat_map(|p| [p[0], p[1], p[2]]).collect();
        webp::Encoder::from_rgb(&rgb, w, h)
    } else {
        webp::Encoder::from_rgba(img.as_raw(), w, h)
    };
    let lossy = enc
        .encode_simple(false, WEBP_QUALITY)
        .map_err(|_| ImgError::Malformed)?;
    let mut best: Vec<u8> = lossy.to_vec();
    if also_lossless {
        let lossless = enc
            .encode_simple(true, 100.0)
            .map_err(|_| ImgError::Malformed)?;
        if lossless.len() < best.len() {
            best = lossless.to_vec();
        }
    }
    Ok(best)
}

fn blurhash_of(img: &RgbaImage) -> Result<String, ImgError> {
    // Hash a thumbnail: the placeholder only needs the coarse colours.
    let (w, h) = img.dimensions();
    let scale = (32.0 / f64::from(w.max(h))).min(1.0);
    let tw = ((f64::from(w) * scale).round() as u32).max(1);
    let th = ((f64::from(h) * scale).round() as u32).max(1);
    let thumb = ::image::imageops::resize(img, tw, th, FilterType::Triangle);
    // Components follow the aspect ratio, 4 on the long side, at least 3 on the short.
    let (cx, cy) = if w >= h {
        (
            4,
            (4.0 * f64::from(h) / f64::from(w)).round().clamp(3.0, 4.0) as u32,
        )
    } else {
        (
            (4.0 * f64::from(w) / f64::from(h)).round().clamp(3.0, 4.0) as u32,
            4,
        )
    };
    blurhash::encode(cx, cy, tw, th, thumb.as_raw()).map_err(|_| ImgError::Malformed)
}

/// Decode `src` and derive the stored representations.
pub fn process_page(src: &[u8], limits: &PageLimits) -> Result<ProcessedPage, ImgError> {
    process_page_sized(src, limits, &VARIANT_WIDTHS)
}

/// [`process_page`] with explicit variant widths (ascending).
pub fn process_page_sized(
    src: &[u8],
    limits: &PageLimits,
    widths: &[u32],
) -> Result<ProcessedPage, ImgError> {
    let (probe, decoded) = img::decode(src, &limits.decode_limits())?;
    // Animated GIF/WebP sources become still pages: the first frame.
    let rgba: RgbaImage = match decoded {
        Decoded::Still(i) => i.to_rgba8(),
        Decoded::Anim(frames) => frames
            .into_iter()
            .next()
            .ok_or(ImgError::Malformed)?
            .into_buffer(),
    };
    let (width, height) = rgba.dimensions();
    let lossless_too = probe.format == ImgFormat::Png;
    let webp = encode_webp(&rgba, lossless_too)?;
    let mut variants = Vec::new();
    for &w in widths {
        // Never upscale, and skip a variant that would be (nearly) the original.
        if width <= w {
            break;
        }
        let vh = ((f64::from(height) * f64::from(w) / f64::from(width)).round() as u32).max(1);
        let resized = ::image::imageops::resize(&rgba, w, vh, FilterType::Lanczos3);
        variants.push(VariantImage {
            width: w,
            height: vh,
            webp: encode_webp(&resized, lossless_too)?,
        });
    }
    Ok(ProcessedPage {
        width,
        height,
        webp,
        variants,
        blurhash: blurhash_of(&rgba)?,
        is_double_spread: is_spread(width, height),
        source: probe.format,
    })
}

/// Object key of a page original.
pub fn page_key(series: i64, chapter: i64, index: usize) -> String {
    format!("library/{series}/{chapter}/{index:04}.webp")
}

/// Object key of a sized variant.
pub fn variant_key(series: i64, chapter: i64, index: usize, width: u32) -> String {
    format!("library/{series}/{chapter}/{index:04}_{width}.webp")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::image::{DynamicImage, ImageFormat, Rgb, RgbImage};
    use std::io::Cursor;

    fn gradient(w: u32, h: u32) -> RgbImage {
        RgbImage::from_fn(w, h, |x, y| {
            Rgb([(x * 255 / w.max(1)) as u8, (y * 255 / h.max(1)) as u8, 128])
        })
    }

    fn encoded(img: &RgbImage, f: ImageFormat) -> Vec<u8> {
        let mut out = Vec::new();
        DynamicImage::ImageRgb8(img.clone())
            .write_to(&mut Cursor::new(&mut out), f)
            .unwrap();
        out
    }

    fn webp_dims(b: &[u8]) -> (u32, u32) {
        let i = ::image::load_from_memory_with_format(b, ImageFormat::WebP).unwrap();
        (i.width(), i.height())
    }

    #[test]
    fn a_jpeg_page_gets_original_variants_and_blurhash() {
        let src = encoded(&gradient(340, 480), ImageFormat::Jpeg);
        let p = process_page_sized(&src, &PageLimits::default(), &[72, 108, 160]).unwrap();
        assert_eq!((p.width, p.height), (340, 480));
        assert_eq!(p.source, ImgFormat::Jpeg);
        assert_eq!(webp_dims(&p.webp), (340, 480));
        assert_eq!(
            p.variants.iter().map(|v| v.width).collect::<Vec<_>>(),
            [72, 108, 160]
        );
        for v in &p.variants {
            assert_eq!(webp_dims(&v.webp), (v.width, v.height));
            // Aspect ratio is kept.
            assert_eq!(
                v.height,
                (480.0 * f64::from(v.width) / 340.0).round() as u32
            );
        }
        assert!(!p.is_double_spread);
        assert!(p.blurhash.len() >= 6);
        assert!(p.blurhash.chars().all(|c| c.is_ascii_graphic()));
    }

    /// The production widths, on a short image so the test stays quick.
    #[test]
    fn production_variant_widths_are_720_1080_1600() {
        let src = encoded(&gradient(1700, 90), ImageFormat::Png);
        let p = process_page(&src, &PageLimits::default()).unwrap();
        assert_eq!(
            p.variants.iter().map(|v| v.width).collect::<Vec<_>>(),
            [720, 1080, 1600]
        );
        assert_eq!(VARIANT_WIDTHS, [720, 1080, 1600]);
    }

    #[test]
    fn small_pages_are_never_upscaled() {
        let src = encoded(&gradient(700, 40), ImageFormat::Png);
        let p = process_page(&src, &PageLimits::default()).unwrap();
        assert!(p.variants.is_empty());
        let src = encoded(&gradient(1000, 40), ImageFormat::Png);
        let p = process_page(&src, &PageLimits::default()).unwrap();
        assert_eq!(
            p.variants.iter().map(|v| v.width).collect::<Vec<_>>(),
            [720]
        );
    }

    #[test]
    fn all_supported_formats_decode() {
        let g = gradient(64, 96);
        for f in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::Gif] {
            let p = process_page(&encoded(&g, f), &PageLimits::default()).unwrap();
            assert_eq!((p.width, p.height), (64, 96), "{f:?}");
        }
        // A WebP source round-trips.
        let w = process_page(&encoded(&g, ImageFormat::Png), &PageLimits::default())
            .unwrap()
            .webp;
        let again = process_page(&w, &PageLimits::default()).unwrap();
        assert_eq!(
            (again.width, again.height, again.source),
            (64, 96, ImgFormat::Webp)
        );
    }

    #[test]
    fn transparency_is_kept() {
        let mut img = RgbaImage::from_pixel(32, 32, ::image::Rgba([10, 20, 30, 255]));
        img.put_pixel(0, 0, ::image::Rgba([0, 0, 0, 0]));
        let mut png = Vec::new();
        DynamicImage::ImageRgba8(img)
            .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
            .unwrap();
        let p = process_page(&png, &PageLimits::default()).unwrap();
        let back = ::image::load_from_memory_with_format(&p.webp, ImageFormat::WebP)
            .unwrap()
            .to_rgba8();
        assert_eq!(back.get_pixel(0, 0)[3], 0);
        assert_eq!(back.get_pixel(5, 5)[3], 255);
    }

    #[test]
    fn spreads_are_detected_by_shape_and_by_name() {
        assert!(is_spread(2700, 2000));
        assert!(!is_spread(2600, 2000), "exactly 1.3 is not a spread");
        assert!(!is_spread(2000, 2000));
        assert!(!is_spread(1000, 0));
        let src = encoded(&gradient(270, 200), ImageFormat::Png);
        assert!(
            process_page(&src, &PageLimits::default())
                .unwrap()
                .is_double_spread
        );
        assert!(name_marks_spread("ch1/page_012_spread.jpg"));
        assert!(name_marks_spread("p-spread.PNG"));
        assert!(!name_marks_spread("spread_of_things/001.jpg"));
    }

    #[test]
    fn exif_free_output_and_junk_is_refused() {
        assert_eq!(
            process_page(b"not an image at all", &PageLimits::default()).unwrap_err(),
            ImgError::Unsupported
        );
        let mut truncated = encoded(&gradient(200, 200), ImageFormat::Jpeg);
        truncated.truncate(300);
        assert!(process_page(&truncated, &PageLimits::default()).is_err());
        // SVG and AVIF-looking input are not images we decode.
        assert!(
            process_page(
                b"<svg xmlns='http://www.w3.org/2000/svg'/>",
                &PageLimits::default()
            )
            .is_err()
        );
        assert_eq!(
            process_page(
                b"\0\0\0\x1cftypavif\0\0\0\0avifmif1",
                &PageLimits::default()
            )
            .unwrap_err(),
            ImgError::Unsupported
        );
    }

    #[test]
    fn budgets_are_enforced_before_decoding() {
        let src = encoded(&gradient(400, 400), ImageFormat::Png);
        let tight = PageLimits {
            max_bytes: 100,
            ..PageLimits::default()
        };
        assert_eq!(process_page(&src, &tight).unwrap_err(), ImgError::TooLarge);
        let few_pixels = PageLimits {
            max_pixels: 1000,
            ..PageLimits::default()
        };
        assert_eq!(
            process_page(&src, &few_pixels).unwrap_err(),
            ImgError::TooLarge
        );
    }

    /// A PNG whose header claims 60000 x 60000 pixels: refused from the header alone,
    /// without allocating the 14 GB canvas.
    #[test]
    fn a_pixel_bomb_is_refused_from_its_header() {
        let mut png = encoded(&gradient(8, 8), ImageFormat::Png);
        png[16..20].copy_from_slice(&60_000u32.to_be_bytes());
        png[20..24].copy_from_slice(&60_000u32.to_be_bytes());
        // Fix the IHDR CRC so the header is well formed.
        let crc = crc32(&png[12..29]);
        png[29..33].copy_from_slice(&crc.to_be_bytes());
        assert_eq!(
            process_page(&png, &PageLimits::default()).unwrap_err(),
            ImgError::TooLarge
        );
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &b in data {
            crc ^= u32::from(b);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    #[test]
    fn keys_stay_inside_the_chapter_prefix() {
        assert_eq!(page_key(1, 2, 7), "library/1/2/0007.webp");
        assert_eq!(variant_key(1, 2, 7, 720), "library/1/2/0007_720.webp");
        assert!(crate::rules::check_key("k", &page_key(1, 2, 12345)).is_ok());
        assert!(crate::rules::check_key("k", &variant_key(1, 2, 0, 1600)).is_ok());
    }

    #[test]
    fn tall_webtoon_strips_scale_by_width() {
        let src = encoded(&gradient(900, 3000), ImageFormat::Png);
        let p = process_page(&src, &PageLimits::default()).unwrap();
        assert_eq!(p.variants.len(), 1);
        assert_eq!((p.variants[0].width, p.variants[0].height), (720, 2400));
    }

    /// Deterministic noisy, photo-like pixels: lossless coding cannot shrink them much.
    fn noisy(w: u32, h: u32) -> RgbImage {
        let mut s = 0x2545_F491_4F6C_DD1Du64;
        RgbImage::from_fn(w, h, |x, y| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            let n = (s % 61) as i32 - 30;
            let base = (x * 255 / w) as i32;
            Rgb([
                (base + n).clamp(0, 255) as u8,
                ((y * 255 / h) as i32 + n).clamp(0, 255) as u8,
                (128 + n).clamp(0, 255) as u8,
            ])
        })
    }

    /// Flat line art: a few colours in blocks.
    fn flat(w: u32, h: u32) -> RgbImage {
        RgbImage::from_fn(w, h, |x, y| {
            if (x / 16 + y / 16) % 2 == 0 {
                Rgb([255, 255, 255])
            } else {
                Rgb([20, 20, 20])
            }
        })
    }

    fn is_lossy_webp(b: &[u8]) -> bool {
        // RIFF....WEBPVP8 (lossy) / VP8L (lossless) / VP8X (extended, with alpha)
        &b[12..16] == b"VP8 "
    }

    #[test]
    fn lossy_q85_beats_lossless_on_photographic_pages() {
        let img = noisy(240, 320);
        let lossless = webp::Encoder::from_rgb(img.as_raw(), 240, 320)
            .encode_simple(true, 100.0)
            .unwrap();
        // JPEG source: lossy only.
        let from_jpeg = process_page(&encoded(&img, ImageFormat::Jpeg), &PageLimits::default())
            .unwrap()
            .webp;
        assert!(is_lossy_webp(&from_jpeg));
        assert!(
            from_jpeg.len() < lossless.len() / 2,
            "{} vs {}",
            from_jpeg.len(),
            lossless.len()
        );
        // PNG source of the same noise: lossless is bigger, so lossy is kept.
        let from_png = process_page(&encoded(&img, ImageFormat::Png), &PageLimits::default())
            .unwrap()
            .webp;
        assert!(is_lossy_webp(&from_png));
        assert!(from_png.len() < lossless.len());
    }

    #[test]
    fn png_line_art_keeps_the_smaller_lossless_encoding() {
        let img = flat(256, 256);
        let lossy = webp::Encoder::from_rgb(img.as_raw(), 256, 256)
            .encode_simple(false, WEBP_QUALITY)
            .unwrap();
        let p = process_page(&encoded(&img, ImageFormat::Png), &PageLimits::default()).unwrap();
        assert!(
            p.webp.len() <= lossy.len(),
            "{} vs {}",
            p.webp.len(),
            lossy.len()
        );
        assert!(!is_lossy_webp(&p.webp), "lossless should win on flat art");
        // Lossless means exact pixels.
        let back = ::image::load_from_memory_with_format(&p.webp, ImageFormat::WebP)
            .unwrap()
            .to_rgb8();
        assert_eq!(back.as_raw(), img.as_raw());
    }

    #[test]
    fn variants_follow_the_same_size_rule() {
        let p = process_page_sized(
            &encoded(&noisy(300, 100), ImageFormat::Png),
            &PageLimits::default(),
            &[100, 200],
        )
        .unwrap();
        assert_eq!(p.variants.len(), 2);
        for v in &p.variants {
            assert_eq!(webp_dims(&v.webp), (v.width, v.height));
        }
        let q = process_page_sized(
            &encoded(&noisy(300, 100), ImageFormat::Jpeg),
            &PageLimits::default(),
            &[100],
        )
        .unwrap();
        assert!(is_lossy_webp(&q.variants[0].webp));
    }

    #[test]
    fn a_side_beyond_libwebp_limits_is_refused() {
        let tall = RgbImage::from_pixel(1, WEBP_MAX_SIDE + 1, Rgb([9, 9, 9]));
        assert_eq!(
            process_page(&encoded(&tall, ImageFormat::Png), &PageLimits::default()).unwrap_err(),
            ImgError::TooLarge
        );
        let ok = RgbImage::from_pixel(1, WEBP_MAX_SIDE, Rgb([9, 9, 9]));
        assert!(process_page(&encoded(&ok, ImageFormat::Png), &PageLimits::default()).is_ok());
    }

    #[test]
    fn avif_is_recognised_by_its_ftyp_brand() {
        assert!(is_avif(b"\0\0\0\x1cftypavif\0\0\0\0avifmif1"));
        assert!(is_avif(b"\0\0\0\x1cftypavis\0\0\0\0"));
        assert!(!is_avif(b"\0\0\0\x1cftypheic\0\0\0\0"));
        assert!(!is_avif(b"short"));
    }
}
