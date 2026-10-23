//! RDY-SEC-008: upload hygiene over generated corpora for the image re-encoder, the surface allowlist, the
//! size policy and the metadata stripper. Nothing is downloaded: every input is built here from a seeded
//! generator, so a failure reproduces from the printed seed.
//!
//! The limits asserted are the ones in doc 12 section 3 "Upload hygiene"; `the_doc_table_matches_the_code`
//! fails when the table and the constants drift apart.

use std::io::Cursor;

use image::codecs::gif::{GifEncoder, Repeat};
use image::codecs::jpeg::JpegEncoder;
use image::{Delay, DynamicImage, ExtendedColorType, Frame, ImageFormat, Rgba, RgbaImage};
use sr_storage::img::{self, Fit, ImgError, ImgFormat, Limits};
use sr_storage::upload::{
    SizeError, SizePolicy, Surface, SurfaceError, is_active_content, sniff_mime, strip_metadata,
};

// ---- generator

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

const SEED: u64 = 0x5352_5032_3153_4543;

fn pixels(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| {
        Rgba([(x * 7 % 256) as u8, (y * 5 % 256) as u8, 90, 255])
    })
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    DynamicImage::ImageRgba8(pixels(w, h))
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .unwrap();
    out
}

fn jpeg(w: u32, h: u32) -> Vec<u8> {
    let rgb = DynamicImage::ImageRgba8(pixels(w, h)).to_rgb8();
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 85)
        .encode(rgb.as_raw(), w, h, ExtendedColorType::Rgb8)
        .unwrap();
    out
}

fn webp(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    DynamicImage::ImageRgba8(pixels(w, h))
        .write_to(&mut Cursor::new(&mut out), ImageFormat::WebP)
        .unwrap();
    out
}

fn gif(w: u32, h: u32, frames: usize) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = GifEncoder::new(&mut out);
        enc.set_repeat(Repeat::Infinite).unwrap();
        for i in 0..frames {
            let img = RgbaImage::from_fn(w, h, |x, _| {
                if (x as usize / 2 + i).is_multiple_of(2) {
                    Rgba([255, 0, 0, 255])
                } else {
                    Rgba([0, 0, 255, 255])
                }
            });
            enc.encode_frame(Frame::from_parts(
                img,
                0,
                0,
                Delay::from_numer_denom_ms(80, 1),
            ))
            .unwrap();
        }
    }
    out
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// CRC-32 as PNG chunks use it.
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

/// Put the right CRC on the IHDR chunk of a PNG whose width or height was rewritten.
fn fix_ihdr_crc(png: &mut [u8]) {
    let crc = crc32(&png[12..29]);
    png[29..33].copy_from_slice(&crc.to_be_bytes());
}

/// A PNG that only has a header: enough for the dimension check, nothing to decode.
fn png_header_only(w: u32, h: u32) -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
    v.extend_from_slice(&13u32.to_be_bytes());
    v.extend_from_slice(b"IHDR");
    v.extend_from_slice(&w.to_be_bytes());
    v.extend_from_slice(&h.to_be_bytes());
    v.extend_from_slice(&[8, 6, 0, 0, 0]);
    v.extend_from_slice(&[0, 0, 0, 0]);
    fix_ihdr_crc(&mut v);
    // The decoder reads up to the first IDAT before it reports the size, so give it a header and no pixels.
    v.extend_from_slice(&2u32.to_be_bytes());
    v.extend_from_slice(b"IDAT");
    v.extend_from_slice(&[0x78, 0x01]);
    let crc = crc32(&v[v.len() - 6..]);
    v.extend_from_slice(&crc.to_be_bytes());
    v
}

/// Small budget so fuzz runs stay fast: nothing above 300x300 or 256 KiB is ever decoded.
fn small() -> Limits {
    Limits {
        max_bytes: 256 * 1024,
        max_pixels: 90_000,
        max_animation_pixels: 200_000,
        max_frames: 12,
    }
}

// ---- size policy

#[test]
fn size_policy_edges_hold_at_the_limit_and_one_over() {
    let p = SizePolicy::default();
    const MIB: u64 = 1024 * 1024;
    assert_eq!(
        (p.standard_limit, p.system_max, p.multipart_threshold),
        (30 * MIB, 500 * MIB, 64 * MIB)
    );
    assert_eq!(p.check(0, 0), Err(SizeError::Empty));
    // Everyone: 30 MiB passes, one byte more does not.
    assert!(p.check(p.standard_limit, 0).is_ok());
    assert_eq!(
        p.check(p.standard_limit + 1, 0),
        Err(SizeError::ExceedsUserLimit { limit: 30 * MIB })
    );
    // An entitlement raises the limit but never past the system maximum, whatever it claims.
    assert!(p.check(p.system_max, u64::MAX).is_ok());
    assert_eq!(
        p.check(p.system_max + 1, u64::MAX),
        Err(SizeError::ExceedsUserLimit { limit: 500 * MIB })
    );
    assert_eq!(
        p.effective_limit(1),
        30 * MIB,
        "an entitlement below the standard limit is lifted to it"
    );
    // Multipart starts one byte above the threshold; large attachments expire above the standard limit.
    let at = p.check(p.multipart_threshold, u64::MAX).unwrap();
    let over = p.check(p.multipart_threshold + 1, u64::MAX).unwrap();
    assert!(!at.multipart && over.multipart);
    assert!(
        p.check(p.standard_limit, 0)
            .unwrap()
            .expires_after
            .is_none()
    );
    assert!(
        p.check(p.standard_limit + 1, u64::MAX)
            .unwrap()
            .expires_after
            .is_some()
    );
}

// ---- image budgets

#[test]
fn dimension_budget_holds_at_the_limit_and_one_over_without_decoding() {
    let l = Limits::default();
    assert_eq!(l.max_pixels, 24_000_000);
    // 6000 x 4000 is exactly the budget; one row more is refused from the header alone.
    assert!(img::probe(&png_header_only(6000, 4000), &l).is_ok());
    assert_eq!(
        img::probe(&png_header_only(6000, 4001), &l),
        Err(ImgError::TooLarge)
    );
    assert!(img::probe(&png_header_only(24_000_000, 1), &l).is_ok());
    assert_eq!(
        img::probe(&png_header_only(24_000_001, 1), &l),
        Err(ImgError::TooLarge)
    );
    // Products that overflow 32 bits are still refused, not wrapped.
    assert!(img::probe(&png_header_only(u32::MAX, u32::MAX), &l).is_err());
    assert!(img::probe(&png_header_only(65_536, 65_536), &l).is_err());
    assert_eq!(
        img::probe(&png_header_only(0, 10), &l),
        Err(ImgError::Malformed)
    );
    // The same bomb through the normaliser: refused before any pixel is allocated.
    for fit in [Fit::Square { max: 512 }, Fit::Bound { w: 960, h: 540 }] {
        assert_eq!(
            img::normalize(&png_header_only(60_000, 60_000), fit, &l).map(|n| n.bytes.len()),
            Err(ImgError::TooLarge)
        );
    }
}

#[test]
fn byte_budget_holds_at_the_limit_and_one_over() {
    let src = png(40, 40);
    let n = src.len() as u64;
    let mut l = small();
    l.max_bytes = n;
    assert!(img::probe(&src, &l).is_ok());
    l.max_bytes = n - 1;
    assert_eq!(img::probe(&src, &l), Err(ImgError::TooLarge));
    assert_eq!(Limits::default().max_bytes, 16 * 1024 * 1024);
}

#[test]
fn animation_budget_holds_at_the_limit_and_one_over() {
    let src = gif(8, 8, 6);
    let mut l = small();
    l.max_frames = 6;
    l.max_animation_pixels = 8 * 8 * 6;
    assert!(
        img::decode(&src, &l).is_ok(),
        "exactly 6 frames and 384 pixels"
    );
    l.max_frames = 5;
    assert_eq!(img::decode(&src, &l).map(|_| ()), Err(ImgError::TooLarge));
    l.max_frames = 6;
    l.max_animation_pixels = 8 * 8 * 6 - 1;
    assert_eq!(img::decode(&src, &l).map(|_| ()), Err(ImgError::TooLarge));
    let d = Limits::default();
    assert_eq!((d.max_frames, d.max_animation_pixels), (400, 32_000_000));
}

// ---- polyglots and trailing bytes

const ZIP_TRAILER: &[u8] = b"PK\x03\x04polyglot-payload\x00PK\x05\x06";
const HTML_TRAILER: &[u8] = b"<html><script>alert(1)</script></html>";

fn with_trailer(mut base: Vec<u8>, trailer: &[u8]) -> Vec<u8> {
    base.extend_from_slice(trailer);
    base
}

#[test]
fn polyglots_lose_their_second_face_in_the_re_encoder() {
    let l = small();
    let corpus: Vec<(&str, Vec<u8>, ImgFormat)> = vec![
        (
            "png+zip",
            with_trailer(png(24, 24), ZIP_TRAILER),
            ImgFormat::Png,
        ),
        (
            "png+html",
            with_trailer(png(24, 24), HTML_TRAILER),
            ImgFormat::Png,
        ),
        (
            "jpeg+zip",
            with_trailer(jpeg(24, 24), ZIP_TRAILER),
            ImgFormat::Jpeg,
        ),
        (
            "jpeg+html",
            with_trailer(jpeg(24, 24), HTML_TRAILER),
            ImgFormat::Jpeg,
        ),
        (
            "gif+html",
            with_trailer(gif(16, 16, 3), HTML_TRAILER),
            ImgFormat::Gif,
        ),
        (
            "gif+zip",
            with_trailer(gif(16, 16, 1), ZIP_TRAILER),
            ImgFormat::Png,
        ),
        (
            "webp+html",
            with_trailer(webp(24, 24), HTML_TRAILER),
            ImgFormat::Png,
        ),
    ];
    for (name, bytes, family) in corpus {
        for surface in [Surface::ProfileImage, Surface::Expression] {
            assert!(
                surface.check(&bytes).is_ok(),
                "{name}: the leading bytes decide the surface"
            );
        }
        let out = img::normalize(&bytes, Fit::Square { max: 64 }, &l).unwrap_or_else(|e| {
            panic!("{name}: a valid image with a trailer is re-encoded, got {e:?}")
        });
        assert!(
            !contains(&out.bytes, b"PK\x03\x04") && !contains(&out.bytes, b"PK\x05\x06"),
            "{name}: zip survived"
        );
        assert!(
            !contains(&out.bytes, b"<script") && !contains(&out.bytes, b"<html"),
            "{name}: html survived"
        );
        assert!(!contains(&out.bytes, b"polyglot-payload"), "{name}");
        // The output is exactly one image of the family the pipeline promises.
        assert_eq!(ImgFormat::sniff(&out.bytes), Some(out.format), "{name}");
        assert_eq!(out.format, family, "{name}");
        assert!(out.width <= 64 && out.height <= 64, "{name}");
    }
}

#[test]
fn markup_first_files_never_reach_an_inline_surface() {
    let png_after_markup = {
        let mut v = b"  \n<html>".to_vec();
        v.extend(png(8, 8));
        v
    };
    let corpus: Vec<Vec<u8>> = vec![
        b"<svg xmlns='http://www.w3.org/2000/svg' onload='alert(1)'/>".to_vec(),
        b"\xEF\xBB\xBF<?xml version='1.0'?><svg/>".to_vec(),
        b"<!DOCTYPE html><title>x</title>".to_vec(),
        b"   <SCRIPT>alert(1)</SCRIPT>".to_vec(),
        b"<!-- GIF89a --><svg/>".to_vec(),
        png_after_markup,
    ];
    for bytes in &corpus {
        assert!(is_active_content(bytes));
        for s in [Surface::ProfileImage, Surface::Expression] {
            assert_eq!(s.check(bytes), Err(SurfaceError::ActiveContent));
        }
        // Chat keeps the bytes but gives them no type, so the CDN serves a download.
        assert_eq!(Surface::ChatAttachment.check(bytes), Ok(None));
    }
}

#[test]
fn the_surface_table_is_what_doc_12_says() {
    let cases: Vec<(&str, Vec<u8>, Option<&str>)> = vec![
        ("png", png(8, 8), Some("image/png")),
        ("jpeg", jpeg(8, 8), Some("image/jpeg")),
        ("gif", gif(8, 8, 2), Some("image/gif")),
        ("webp", webp(8, 8), Some("image/webp")),
        ("avif", b"\0\0\0\x18ftypavif....".to_vec(), None),
        ("pdf", b"%PDF-1.7\n".to_vec(), None),
        ("mp4", b"\0\0\0\x18ftypmp42....".to_vec(), None),
        ("zip", b"PK\x03\x04....".to_vec(), None),
        (
            "random",
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13],
            None,
        ),
        ("empty", vec![], None),
    ];
    for (name, bytes, inline) in cases {
        for s in [Surface::ProfileImage, Surface::Expression] {
            match inline {
                Some(mime) => assert_eq!(s.check(&bytes), Ok(Some(mime)), "{name}"),
                None => assert_eq!(s.check(&bytes), Err(SurfaceError::NotAllowed), "{name}"),
            }
        }
        assert_eq!(
            Surface::ChatAttachment.check(&bytes),
            Ok(sniff_mime(&bytes)),
            "{name}"
        );
    }
}

#[test]
fn metadata_stripping_drops_png_trailers_and_keeps_every_other_byte_of_the_image() {
    let p = with_trailer(png(16, 16), ZIP_TRAILER);
    let out = strip_metadata(&p).unwrap();
    assert!(
        !contains(&out, b"PK\x03\x04"),
        "bytes after IEND are dropped"
    );
    assert!(out.starts_with(b"\x89PNG\r\n\x1a\n"));
    // A JPEG keeps what follows its end marker (multi-picture files carry their second image there); the
    // bytes are not interpreted or served as anything else, since attachments are sniffed from the head.
    let j = with_trailer(jpeg(16, 16), b"TRAILER");
    assert!(strip_metadata(&j).unwrap().ends_with(b"TRAILER"));
}

// ---- truncation and mutation

fn originals() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("png", png(40, 30)),
        ("jpeg", jpeg(40, 30)),
        ("gif", gif(20, 20, 3)),
        ("webp", webp(40, 30)),
    ]
}

/// What an accepted result must satisfy, whatever went in.
fn assert_sane(name: &str, what: &str, bytes: &[u8], l: &Limits) {
    let p = img::probe(bytes, l)
        .unwrap_or_else(|e| panic!("{name} {what}: output does not probe: {e:?}"));
    assert!(
        u64::from(p.width) * u64::from(p.height) <= l.max_pixels,
        "{name} {what}"
    );
}

#[test]
fn every_truncation_of_every_format_is_refused_or_clean() {
    let l = small();
    for (name, full) in originals() {
        for cut in 0..full.len() {
            let head = &full[..cut];
            // No panic is the first property; the second is that anything accepted is a sane image.
            if let Ok(n) = img::normalize(head, Fit::Bound { w: 64, h: 64 }, &l) {
                assert_sane(name, &format!("normalize cut at {cut}"), &n.bytes, &l);
            }
            let _ = strip_metadata(head);
            let _ = Surface::ProfileImage.check(head);
            let _ = img::probe(head, &l);
        }
    }
}

#[test]
fn random_corruption_never_panics_and_never_exceeds_the_budget() {
    let l = small();
    let mut rng = Rng(SEED);
    for (name, full) in originals() {
        for round in 0..400 {
            let mut b = full.clone();
            // 1 to 8 edits: flip a byte, overwrite a run, insert, delete, or truncate.
            for _ in 0..=rng.below(8) {
                let at = rng.below(b.len().max(1));
                match rng.below(5) {
                    0 => {
                        if let Some(x) = b.get_mut(at) {
                            *x ^= 1 << rng.below(8);
                        }
                    }
                    1 => {
                        for k in at..(at + 1 + rng.below(16)).min(b.len()) {
                            b[k] = rng.next() as u8;
                        }
                    }
                    2 => b.insert(at.min(b.len()), rng.next() as u8),
                    3 => {
                        if !b.is_empty() {
                            b.remove(at.min(b.len() - 1));
                        }
                    }
                    _ => b.truncate(at),
                }
            }
            if let Ok(n) = img::normalize(&b, Fit::Square { max: 64 }, &l) {
                assert_sane(name, &format!("round {round}"), &n.bytes, &l);
                assert!(
                    n.bytes.len() as u64 <= l.max_bytes * 4,
                    "{name} round {round}: output blew up"
                );
            }
            let _ = strip_metadata(&b);
        }
    }
}

#[test]
fn header_lies_about_dimensions_are_caught_by_the_budget_not_by_memory() {
    // Rewrite IHDR width and height of a real PNG: the probe reads the header, so the budget decides.
    let mut rng = Rng(SEED ^ 0xABCD);
    let l = small();
    for _ in 0..300 {
        let mut b = png(16, 16);
        let w = rng.next() as u32;
        let h = rng.next() as u32;
        b[16..20].copy_from_slice(&w.to_be_bytes());
        b[20..24].copy_from_slice(&h.to_be_bytes());
        fix_ihdr_crc(&mut b);
        let r = img::normalize(&b, Fit::Square { max: 64 }, &l);
        let pixels = u64::from(w) * u64::from(h);
        if pixels > l.max_pixels {
            // Refused either by the budget or, for sizes no decoder can represent, as malformed.
            assert!(r.is_err(), "{w}x{h}");
        }
    }
}

// ---- the doc

#[test]
fn the_doc_table_matches_the_code() {
    let doc = include_str!("../../../docs/12-deployment-and-operations.md");
    let l = Limits::default();
    let p = SizePolicy::default();
    const MIB: u64 = 1024 * 1024;
    for row in [
        format!(
            "| Chat attachment | standard size | {} MiB |",
            p.standard_limit / MIB
        ),
        format!(
            "| Chat attachment | donor and system maximum | {} MiB |",
            p.system_max / MIB
        ),
        format!(
            "| Chat attachment | multipart above | {} MiB |",
            p.multipart_threshold / MIB
        ),
        format!(
            "| Image re-encoder | source file | {} MiB |",
            l.max_bytes / MIB
        ),
        format!("| Image re-encoder | pixels per frame | {} |", l.max_pixels),
        format!(
            "| Image re-encoder | animation pixels and frames | {} and {} |",
            l.max_animation_pixels, l.max_frames
        ),
        format!(
            "| Image sizes served | range | {} to {} px, powers of two |",
            img::MIN_SIZE,
            img::MAX_SIZE
        ),
    ] {
        assert!(
            doc.contains(&row),
            "docs/12 section 3 lacks the row {row:?}"
        );
    }
}
